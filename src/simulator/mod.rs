use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use http::header::SEC_WEBSOCKET_PROTOCOL;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

use crate::ocpp::{
  BootReason, ChargingRateUnit, ConfigurationKey, IdTokenType,
  IncomingAction_V1_6, IncomingAction_V2_X, Measurand, MeterUnit,
  OcppErrorCode, OcppFrame, OcppVersion, OutgoingAction, ReadingContext,
  ResponseStatus, StatusNotificationErrorCode, StopReason,
  TransactionTriggerReason, TriggerMessage_V1_6, TriggerMessage_V2_X,
  VariableAttributeType, build_call, build_call_error, build_call_result,
  parse_frame,
};

mod support;
mod types;

pub(in crate::simulator) use support::{
  authorize_status, default_configuration_entries, map_stop_reason_v1_6,
  map_stop_reason_v2_x, now_timestamp, optional_u16_field, required_i64_field,
  required_string_field, required_u16_field, required_u64_field,
  validate_negotiated_subprotocol,
};
pub(in crate::simulator) use types::{
  ConfigurationEntry, ConnectorState, ConnectorStatus, HeartbeatTask,
  PendingCall, PendingContext, QueuedCall, Simulator, TransactionEventRequest,
  TransactionState, TxEventType, normalize_identifier,
};
pub use types::{
  ConnectorSnapshot, SimulatorCommand, SimulatorConfig, SimulatorSnapshot,
  UiEvent, UiLogLevel,
};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsWrite = SplitSink<WsStream, Message>;
type WsRead = SplitStream<WsStream>;
const QUEUE_DEPTH_WARN_THRESHOLD: usize = 1_000;

/// Runs the simulator event loop and bridges UI commands, WS I/O, and state.
pub async fn run_simulator(
  config: SimulatorConfig,
  mut cmd_rx: UnboundedReceiver<SimulatorCommand>,
  ui_tx: UnboundedSender<UiEvent>,
  self_cmd_tx: UnboundedSender<SimulatorCommand>,
) {
  let mut simulator = Simulator::new(config, ui_tx, self_cmd_tx);
  initialize_simulator_runtime(&mut simulator);

  let mut connection: Option<Connection> = None;
  let mut timeout_tick = tokio::time::interval(Duration::from_millis(200));
  timeout_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

  'outer: loop {
    if let Some(io) = connection.as_mut() {
      if let Err(error) = simulator.try_send_next(&mut io.write).await {
        simulator.log(UiLogLevel::Error, format!("Send failed: {error}"));
        simulator.handle_disconnect("Connection lost while sending.");
        connection = None;
        continue;
      }

      let mut outcome = CommandOutcome::Continue;
      let mut disconnected = false;

      tokio::select! {
        _ = timeout_tick.tick() => {
          simulator.check_pending_timeout();
        }
        maybe_command = cmd_rx.recv() => {
          match maybe_command {
            Some(command) => {
              match simulator
                .handle_connected_command(command, &mut io.write)
                .await
              {
                Ok(next) => {
                  outcome = next;
                }
                Err(error) => {
                  simulator.log(
                    UiLogLevel::Error,
                    format!("Command failed: {error}"),
                  );
                }
              }
            }
            None => {
              outcome = CommandOutcome::Exit;
            }
          }
        }
        message = io.read.next() => {
          match message {
            Some(Ok(frame)) => {
              if let Err(error) =
                simulator.handle_ws_message(frame, &mut io.write).await
              {
                simulator.log(
                  UiLogLevel::Error,
                  format!("Connection error: {error}"),
                );
                disconnected = true;
              }
            }
            Some(Err(error)) => {
              simulator.log(
                UiLogLevel::Error,
                format!("WebSocket read error: {error}"),
              );
              disconnected = true;
            }
            None => {
              simulator.log(
                UiLogLevel::Warn,
                "CSMS closed the WebSocket connection.",
              );
              disconnected = true;
            }
          }
        }
      }

      if disconnected {
        simulator.handle_disconnect("Disconnected.");
        connection = None;
      }

      match outcome {
        CommandOutcome::Continue => {}
        CommandOutcome::Disconnect => {
          simulator.handle_disconnect("Disconnected.");
          connection = None;
        }
        CommandOutcome::Exit => {
          break 'outer;
        }
      }
    } else if !handle_offline_loop_step(
      &mut simulator,
      &mut cmd_rx,
      &mut connection,
    )
    .await
    {
      break 'outer;
    }
  }

  simulator.stop_heartbeat();
}

fn initialize_simulator_runtime(simulator: &mut Simulator) {
  simulator.log(
    UiLogLevel::Info,
    "OCPP framing follows CALL/CALLRESULT/CALLERROR arrays from OCPP-J.",
  );
  simulator.log(
    UiLogLevel::Info,
    format!(
      "Configured WebSocket subprotocol: {}",
      simulator.config.protocol.subprotocol()
    ),
  );
  if simulator.config.strict {
    simulator.log(
      UiLogLevel::Info,
      "Strict inbound schema validation is enabled.",
    );
  }
  simulator.emit_snapshot();

  if let Some(seconds) = simulator.config.heartbeat_seconds {
    simulator.start_heartbeat(seconds);
  }
}

async fn handle_offline_loop_step(
  simulator: &mut Simulator,
  cmd_rx: &mut UnboundedReceiver<SimulatorCommand>,
  connection: &mut Option<Connection>,
) -> bool {
  match cmd_rx.recv().await {
    Some(command) => match simulator.handle_offline_command(command).await {
      Ok(OfflineOutcome::Continue) => true,
      Ok(OfflineOutcome::Connect(new_connection)) => {
        *connection = Some(new_connection);
        true
      }
      Ok(OfflineOutcome::Exit) => false,
      Err(error) => {
        simulator.log(UiLogLevel::Error, format!("Command failed: {error}"));
        true
      }
    },
    None => false,
  }
}

#[derive(Debug)]
struct Connection {
  write: WsWrite,
  read: WsRead,
}

#[derive(Debug)]
enum OfflineOutcome {
  Continue,
  Connect(Connection),
  Exit,
}

#[derive(Debug)]
enum CommandOutcome {
  Continue,
  Disconnect,
  Exit,
}

impl Simulator {
  /// Builds an initialized simulator state with default connector entries.
  fn new(
    config: SimulatorConfig,
    ui_tx: UnboundedSender<UiEvent>,
    self_cmd_tx: UnboundedSender<SimulatorCommand>,
  ) -> Self {
    let configuration = default_configuration_entries(&config);
    let mut connectors = BTreeMap::new();
    for connector in 1..=config.connectors {
      connectors.insert(
        connector,
        ConnectorState {
          status: ConnectorStatus::Available,
          meter_wh: 0,
          offered_limit: None,
          scheduled_availability: None,
          transaction: None,
        },
      );
    }

    Self {
      config,
      ui_tx,
      self_cmd_tx,
      connectors,
      configuration,
      reservations: BTreeMap::new(),
      charging_profiles: BTreeMap::new(),
      local_auth_list_version: 0,
      queue: VecDeque::new(),
      pending: None,
      next_message_id: 1,
      next_tx_id: 1,
      heartbeat: None,
      connected: false,
    }
  }

  /// Handles a command while disconnected from the CSMS.
  async fn handle_offline_command(
    &mut self,
    command: SimulatorCommand,
  ) -> Result<OfflineOutcome> {
    match command {
      SimulatorCommand::Connect => {
        let connection = self.connect().await?;
        Ok(OfflineOutcome::Connect(connection))
      }
      SimulatorCommand::Disconnect => {
        self.log(UiLogLevel::Warn, "Already disconnected.");
        Ok(OfflineOutcome::Continue)
      }
      SimulatorCommand::Shutdown => Ok(OfflineOutcome::Exit),
      other => {
        self.handle_common_command(other, false)?;
        Ok(OfflineOutcome::Continue)
      }
    }
  }

  /// Handles a command while connected, including connection-only commands.
  async fn handle_connected_command(
    &mut self,
    command: SimulatorCommand,
    write: &mut WsWrite,
  ) -> Result<CommandOutcome> {
    match command {
      SimulatorCommand::Connect => {
        self.log(UiLogLevel::Warn, "Already connected.");
        Ok(CommandOutcome::Continue)
      }
      SimulatorCommand::Disconnect => {
        self.close_connection(write).await;
        Ok(CommandOutcome::Disconnect)
      }
      SimulatorCommand::Shutdown => {
        self.close_connection(write).await;
        Ok(CommandOutcome::Exit)
      }
      other => {
        self.handle_common_command(other, true)?;
        Ok(CommandOutcome::Continue)
      }
    }
  }

  /// Handles commands that are valid in both online and offline states.
  fn handle_common_command(
    &mut self,
    command: SimulatorCommand,
    is_connected: bool,
  ) -> Result<()> {
    match command {
      SimulatorCommand::Status => {
        self.emit_snapshot();
      }
      SimulatorCommand::Boot => {
        if !is_connected {
          self.log(
            UiLogLevel::Warn,
            "Not connected. Connect first to send BootNotification.",
          );
          return Ok(());
        }
        self.enqueue_boot_notification();
      }
      SimulatorCommand::Authorize { id_token } => {
        if !is_connected {
          self.log(
            UiLogLevel::Warn,
            "Not connected. Connect first to send Authorize.",
          );
          return Ok(());
        }
        self.enqueue_authorize(id_token);
      }
      SimulatorCommand::DataTransfer {
        vendor_id,
        message_id,
        data,
      } => {
        if !is_connected {
          self.log(
            UiLogLevel::Warn,
            "Not connected. Connect first to send DataTransfer.",
          );
          return Ok(());
        }
        self.enqueue_data_transfer(
          vendor_id.as_str(),
          message_id.as_deref(),
          data.as_deref(),
        );
      }
      SimulatorCommand::StartTransaction {
        connector,
        id_token,
      } => {
        self.start_transaction(
          connector,
          id_token,
          false,
          None,
          is_connected,
        )?;
      }
      SimulatorCommand::StopTransaction { connector, reason } => {
        self.stop_transaction(
          connector,
          reason.as_deref(),
          false,
          is_connected,
        )?;
      }
      SimulatorCommand::SetMeter {
        connector,
        value_wh,
      } => {
        self.set_meter(connector, value_wh)?;
      }
      SimulatorCommand::SendMeter { connector } => {
        self.send_meter(connector, is_connected)?;
      }
      SimulatorCommand::Heartbeat => {
        if !is_connected {
          self.log(
            UiLogLevel::Warn,
            "Not connected. Connect first to send Heartbeat.",
          );
          return Ok(());
        }
        self.enqueue_heartbeat();
      }
      SimulatorCommand::StartHeartbeat { seconds } => {
        self.start_heartbeat(seconds);
      }
      SimulatorCommand::StopHeartbeat => {
        self.stop_heartbeat();
      }
      SimulatorCommand::SetConnectorStatus { connector, status } => {
        self.set_connector_status(connector, &status, is_connected)?;
      }
      SimulatorCommand::HeartbeatTick => {
        if is_connected && self.pending.is_none() && self.queue.is_empty() {
          self.enqueue_heartbeat();
        }
      }
      SimulatorCommand::Connect
      | SimulatorCommand::Disconnect
      | SimulatorCommand::Shutdown => {}
    }
    Ok(())
  }

  /// Opens the WebSocket connection and performs initial boot/status enqueue.
  async fn connect(&mut self) -> Result<Connection> {
    let url = self.connection_url()?;
    let mut request = url.as_str().into_client_request()?;
    request.headers_mut().insert(
      SEC_WEBSOCKET_PROTOCOL,
      HeaderValue::from_str(self.config.protocol.subprotocol())?,
    );

    self.log(
      UiLogLevel::Info,
      format!(
        "Connecting to {} with subprotocol {} ...",
        url,
        self.config.protocol.subprotocol()
      ),
    );
    let (stream, response) = connect_async(request).await?;
    let expected_subprotocol = self.config.protocol.subprotocol();
    let negotiated = response
      .headers()
      .get(SEC_WEBSOCKET_PROTOCOL)
      .and_then(|value| value.to_str().ok());
    let negotiated =
      validate_negotiated_subprotocol(expected_subprotocol, negotiated)?;

    self.connected = true;
    self.pending = None;
    self.queue.clear();
    self.log(
      UiLogLevel::Info,
      format!("Connected. Negotiated WebSocket subprotocol: {negotiated}"),
    );

    self.enqueue_boot_notification();
    let connectors: Vec<u16> = self.connectors.keys().copied().collect();
    for connector in connectors {
      self.enqueue_status_notification(connector)?;
    }
    self.emit_snapshot();

    let (write, read) = stream.split();
    Ok(Connection { write, read })
  }

  /// Builds the final WebSocket URL, appending charge point id when enabled.
  fn connection_url(&self) -> Result<Url> {
    let mut url = Url::parse(&self.config.ws_url)?;
    if self.config.append_cp_id {
      let mut segments = url
        .path_segments_mut()
        .map_err(|()| anyhow!("WebSocket URL cannot be a base URL."))?;
      segments.pop_if_empty().push(&self.config.cp_id);
    }
    Ok(url)
  }

  /// Sends a WebSocket close frame.
  async fn close_connection(&mut self, write: &mut WsWrite) {
    let _ = write.send(Message::Close(None)).await;
  }

  /// Marks simulator as disconnected, clears pending queue, and logs reason.
  fn handle_disconnect(&mut self, message: &str) {
    self.connected = false;
    self.pending = None;
    self.queue.clear();
    self.log(UiLogLevel::Warn, message);
    self.emit_snapshot();
  }

  /// Sends the next queued CALL frame when no pending request exists.
  async fn try_send_next(&mut self, write: &mut WsWrite) -> Result<()> {
    if self.pending.is_some() {
      return Ok(());
    }
    let Some(call) = self.queue.pop_front() else {
      return Ok(());
    };

    let message_id = self.next_message_id();
    let payload = build_call(&message_id, &call.action, &call.payload);
    self
      .send_text(
        write,
        payload,
        UiLogLevel::Tx,
        format!("CALL {} {}", message_id, call.action),
      )
      .await?;

    self.pending = Some(PendingCall {
      message_id,
      sent_at: Instant::now(),
      call,
    });
    Ok(())
  }

  /// Checks the pending request timeout and clears stale pending state.
  fn check_pending_timeout(&mut self) {
    let Some(pending) = self.pending.as_ref() else {
      return;
    };
    if pending.sent_at.elapsed() < self.config.request_timeout {
      return;
    }

    let action = pending.call.action.clone();
    let message_id = pending.message_id.clone();
    let context = pending.call.context.clone();

    self.log(
      UiLogLevel::Warn,
      format!(
        "Timed out waiting for response to {action} (messageId={message_id})."
      ),
    );
    self.handle_pending_timeout_context(&context);
    self.pending = None;
  }

  /// Restores local state for pending calls that timed out before a response.
  fn handle_pending_timeout_context(&mut self, context: &PendingContext) {
    let result = match context {
      PendingContext::StartTxV1_6 {
        connector,
        local_tx_id,
      } => self
        .cancel_transaction_start(*connector, *local_tx_id)
        .and_then(|()| self.enqueue_status_notification(*connector)),
      PendingContext::StopTxV1_6 {
        connector,
        local_tx_id,
      } => self
        .restore_active_transaction_status(*connector, *local_tx_id)
        .and_then(|()| self.enqueue_status_notification(*connector)),
      PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type: TxEventType::Started,
      } => self
        .cancel_transaction_start(*connector, *local_tx_id)
        .and_then(|()| self.enqueue_status_notification(*connector)),
      PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type: TxEventType::Ended,
      } => self
        .restore_active_transaction_status(*connector, *local_tx_id)
        .and_then(|()| self.enqueue_status_notification(*connector)),
      PendingContext::TxEvent { .. }
      | PendingContext::Boot
      | PendingContext::Heartbeat
      | PendingContext::DataTransfer
      | PendingContext::DiagnosticsStatusNotification
      | PendingContext::FirmwareStatusNotification
      | PendingContext::LogStatusNotification
      | PendingContext::Authorize { .. }
      | PendingContext::RemoteStartAuthorizeV1_6 { .. }
      | PendingContext::StatusNotification { .. }
      | PendingContext::MeterValues { .. } => Ok(()),
    };
    if let Err(error) = result {
      self.log(
        UiLogLevel::Error,
        format!("Failed to apply timeout rollback: {error}"),
      );
    }
  }
}

mod incoming;
mod payloads;
mod workflow;

#[cfg(test)]
mod tests;
