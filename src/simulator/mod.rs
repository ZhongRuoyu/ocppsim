use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use http::header::{AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream, connect_async, connect_async_tls_with_config,
};
use url::Url;

use crate::ocpp::{
  BootReason, CertificateType, ChargingRateUnit, ConfigurationKey,
  ExtendedTriggerMessage_V1_6, IdTokenType, IncomingAction_V1_6,
  IncomingAction_V2_X, Measurand, MeterUnit, OcppErrorCode, OcppFrame,
  OcppVersion, OutgoingAction, ReadingContext, ResponseStatus,
  StatusNotificationErrorCode, StopReason, TransactionTriggerReason,
  TriggerMessage_V1_6, TriggerMessage_V2_X, VariableAttributeType, build_call,
  build_call_error, build_call_result, parse_frame,
};

mod security;
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
  PendingCall, PendingContext, QueuedCall, SecurityProfileFallback,
  SecurityState, Simulator, TransactionEventRequest, TransactionState,
  TxEventType, normalize_identifier,
};
pub use types::{
  ConnectorSnapshot, SimulatorCommand, SimulatorConfig,
  SimulatorConnectionConfig, SimulatorSnapshot, UiEvent, UiLogLevel,
};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsWrite = SplitSink<WsStream, Message>;
type WsRead = SplitStream<WsStream>;

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
    if connection.is_some() {
      if !handle_connected_loop_step(
        &mut simulator,
        &mut cmd_rx,
        &mut timeout_tick,
        &mut connection,
      )
      .await
      {
        break 'outer;
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

async fn handle_connected_loop_step(
  simulator: &mut Simulator,
  cmd_rx: &mut UnboundedReceiver<SimulatorCommand>,
  timeout_tick: &mut tokio::time::Interval,
  connection: &mut Option<Connection>,
) -> bool {
  let Some(mut io) = connection.take() else {
    return true;
  };
  if let Err(error) = simulator.try_send_next(&mut io.write).await {
    simulator.log(UiLogLevel::Error, format!("Send failed: {error}"));
    simulator.handle_disconnect("Connection lost while sending.");
    return true;
  }

  let mut outcome = CommandOutcome::Continue;
  let mut disconnected = false;

  tokio::select! {
    _ = timeout_tick.tick() => {
      simulator.check_pending_timeout();
    }
    maybe_command = cmd_rx.recv() => {
      handle_connected_command_result(
        simulator,
        maybe_command,
        &mut io.write,
        &mut outcome,
      ).await;
    }
    message = io.read.next() => {
      disconnected = handle_connected_ws_message(
        simulator,
        message,
        &mut io.write,
        &mut outcome,
      ).await;
    }
  }

  if disconnected {
    simulator.handle_disconnect("Disconnected.");
  }

  match outcome {
    CommandOutcome::Continue => {
      if !disconnected {
        *connection = Some(io);
      }
      true
    }
    CommandOutcome::Disconnect => {
      simulator.handle_disconnect("Disconnected.");
      true
    }
    CommandOutcome::Reconnect => {
      if !disconnected {
        *connection = reconnect_after_security_change(simulator, &mut io).await;
      }
      true
    }
    CommandOutcome::Exit => false,
  }
}

async fn handle_connected_command_result(
  simulator: &mut Simulator,
  maybe_command: Option<SimulatorCommand>,
  write: &mut WsWrite,
  outcome: &mut CommandOutcome,
) {
  match maybe_command {
    Some(command) => {
      match simulator.handle_connected_command(command, write).await {
        Ok(next) => {
          *outcome = next;
        }
        Err(error) => {
          simulator.log(UiLogLevel::Error, format!("Command failed: {error}"));
        }
      }
    }
    None => {
      *outcome = CommandOutcome::Exit;
    }
  }
}

async fn handle_connected_ws_message(
  simulator: &mut Simulator,
  message: Option<
    std::result::Result<Message, tokio_tungstenite::tungstenite::Error>,
  >,
  write: &mut WsWrite,
  outcome: &mut CommandOutcome,
) -> bool {
  match message {
    Some(Ok(frame)) => {
      if let Err(error) = simulator.handle_ws_message(frame, write).await {
        simulator.log(UiLogLevel::Error, format!("Connection error: {error}"));
        true
      } else if simulator.security.pending_reconnect.is_some() {
        *outcome = CommandOutcome::Reconnect;
        false
      } else {
        false
      }
    }
    Some(Err(error)) => {
      simulator
        .log(UiLogLevel::Error, format!("WebSocket read error: {error}"));
      true
    }
    None => {
      simulator.log(UiLogLevel::Warn, "CSMS closed the WebSocket connection.");
      true
    }
  }
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
  Reconnect,
  Exit,
}

async fn reconnect_after_security_change(
  simulator: &mut Simulator,
  connection: &mut Connection,
) -> Option<Connection> {
  let plan = simulator.security.pending_reconnect.take()?;
  simulator.close_connection(&mut connection.write).await;
  simulator.handle_disconnect("Reconnecting after security parameter change.");

  match simulator.connect().await {
    Ok(new_connection) => Some(new_connection),
    Err(error) => {
      simulator.log(
        UiLogLevel::Error,
        format!("Reconnect after security parameter change failed: {error}"),
      );
      simulator.record_secure_connection_failure(&error);
      if let SecurityProfileFallback::Restore(fallback) =
        plan.fallback_security_profile
      {
        simulator.security.security_profile = fallback;
        simulator.config.security_profile = fallback;
        if let Some(entry) = simulator
          .configuration
          .get_mut(&ConfigurationKey::SecurityProfile)
        {
          entry.value = fallback.unwrap_or(0).to_string();
        }
        simulator.log(
          UiLogLevel::Warn,
          "Falling back to the previous security profile.",
        );
        match simulator.connect().await {
          Ok(new_connection) => Some(new_connection),
          Err(error) => {
            simulator.log(
              UiLogLevel::Error,
              format!("Fallback reconnect failed: {error}"),
            );
            None
          }
        }
      } else {
        None
      }
    }
  }
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
    let security = SecurityState::new(&config);

    Self {
      config,
      ui_tx,
      self_cmd_tx,
      connectors,
      configuration,
      reservations: BTreeMap::new(),
      charging_profiles: BTreeMap::new(),
      security,
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
      SimulatorCommand::Connect { config } => {
        if let Some(config) = config {
          self.apply_connection_config(*config);
        }
        if !self.has_connection_target() {
          self.log(
            UiLogLevel::Warn,
            "No connection target configured. Use `connect <profile>` or \
            `connect <ws-url> <cp-id>`.",
          );
          return Ok(OfflineOutcome::Continue);
        }
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
      SimulatorCommand::Connect { .. } => {
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
      SimulatorCommand::Connect { .. }
      | SimulatorCommand::Disconnect
      | SimulatorCommand::Shutdown => {}
    }
    Ok(())
  }

  /// Applies an interactive connection target before opening the WebSocket.
  fn apply_connection_config(&mut self, config: SimulatorConnectionConfig) {
    self.config.profile = config.profile;
    self.config.ws_url = Some(config.ws_url);
    self.config.cp_id = Some(config.cp_id);
    self.config.append_cp_id = config.append_cp_id;
    self.config.protocol = config.protocol;
    self.config.vendor = config.vendor;
    self.config.model = config.model;
    self.config.firmware = config.firmware;
    self.config.trace_frames = config.trace_frames;
    self.config.strict = config.strict;
    self.config.request_timeout = config.request_timeout;
    self.config.security_profile = config.security_profile;
    self.config.basic_auth_password = config.basic_auth_password;
    self.config.ca_cert_path = config.ca_cert_path;
    self.config.client_cert_path = config.client_cert_path;
    self.config.client_key_path = config.client_key_path;
    self.security.security_profile = self.config.security_profile;
    self.security.basic_auth_password = self.config.basic_auth_password.clone();
    self.resize_connectors(config.connectors);
    self.apply_heartbeat_config(config.heartbeat_seconds);
    self.refresh_runtime_configuration_entries();
    self.emit_snapshot();
  }

  fn has_connection_target(&self) -> bool {
    self.config.ws_url.is_some() && self.config.cp_id.is_some()
  }

  fn resize_connectors(&mut self, count: u16) {
    let current = self.config.connectors;
    self.config.connectors = count;

    if count > current {
      for connector in current.saturating_add(1)..=count {
        self.connectors.insert(
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
    } else if count < current {
      self.connectors.retain(|connector, _| *connector <= count);
      self.reservations.retain(|_, connector| *connector <= count);
      self
        .charging_profiles
        .retain(|connector, _| *connector <= count);
    }
  }

  fn apply_heartbeat_config(&mut self, heartbeat_seconds: Option<u64>) {
    if self.config.heartbeat_seconds == heartbeat_seconds {
      return;
    }
    self.config.heartbeat_seconds = heartbeat_seconds;
    if let Some(seconds) = heartbeat_seconds {
      self.start_heartbeat(seconds);
    } else {
      self.stop_heartbeat();
    }
  }

  fn refresh_runtime_configuration_entries(&mut self) {
    if let Some(entry) = self
      .configuration
      .get_mut(&ConfigurationKey::NumberOfConnectors)
    {
      entry.value = self.config.connectors.to_string();
    }
    if let Some(entry) = self
      .configuration
      .get_mut(&ConfigurationKey::HeartbeatInterval)
    {
      entry.value = self.config.heartbeat_seconds.unwrap_or(30).to_string();
    }
    if let Some(entry) = self
      .configuration
      .get_mut(&ConfigurationKey::SecurityProfile)
    {
      entry.value = self.config.security_profile.unwrap_or(0).to_string();
    }
  }

  /// Opens the WebSocket connection and performs initial boot/status enqueue.
  async fn connect(&mut self) -> Result<Connection> {
    let url = self.connection_url()?;
    self.validate_connection_security(&url)?;
    let mut request = url.as_str().into_client_request()?;
    request.headers_mut().insert(
      SEC_WEBSOCKET_PROTOCOL,
      HeaderValue::from_str(self.config.protocol.subprotocol())?,
    );
    if let Some(header) = self.basic_auth_header()? {
      request.headers_mut().insert(AUTHORIZATION, header);
    }

    self.log(
      UiLogLevel::Info,
      format!(
        "Connecting to {} with subprotocol {} ...",
        url,
        self.config.protocol.subprotocol()
      ),
    );
    let connector = match self.tls_connector() {
      Ok(connector) => connector,
      Err(error) => {
        self.record_secure_connection_failure(&error);
        return Err(error);
      }
    };
    let connection_result = if connector.is_some() {
      connect_async_tls_with_config(request, None, false, connector).await
    } else {
      connect_async(request).await
    };
    let (stream, response) = match connection_result {
      Ok(connection) => connection,
      Err(error) => {
        self.record_secure_connection_failure(&error);
        return Err(error.into());
      }
    };
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
    self.emit_snapshot();

    let (write, read) = stream.split();
    Ok(Connection { write, read })
  }

  /// Builds the final WebSocket URL, appending charge point id when enabled.
  fn connection_url(&self) -> Result<Url> {
    let ws_url = self
      .config
      .ws_url
      .as_deref()
      .ok_or_else(|| anyhow!("No WebSocket URL configured."))?;
    let cp_id = self
      .config
      .cp_id
      .as_deref()
      .ok_or_else(|| anyhow!("No charge point id configured."))?;
    let mut url = Url::parse(ws_url)?;
    if self.config.append_cp_id {
      let mut segments = url
        .path_segments_mut()
        .map_err(|()| anyhow!("WebSocket URL cannot be a base URL."))?;
      segments.pop_if_empty().push(cp_id);
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
    self.reset_inflight_security_event_notifications();
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
      | PendingContext::SignCertificate
      | PendingContext::SignedFirmwareStatusNotification
      | PendingContext::Authorize { .. }
      | PendingContext::RemoteStartAuthorizeV1_6 { .. }
      | PendingContext::StatusNotification { .. }
      | PendingContext::MeterValues { .. } => Ok(()),
      PendingContext::SecurityEventNotification { event_id } => {
        self.retry_security_event_notification(*event_id);
        Ok(())
      }
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
