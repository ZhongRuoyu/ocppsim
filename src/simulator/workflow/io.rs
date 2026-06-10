use super::super::{
  ConnectorSnapshot, Duration, HeartbeatTask, Message, MissedTickBehavior,
  OcppErrorCode, OcppVersion, Result, Simulator, SimulatorCommand,
  SimulatorRuntimeState, SimulatorSnapshot, SinkExt, UiEvent, UiLogLevel,
  Value, WsMessageSink, build_call_error, build_call_result, json,
  sanitized_trace_frame_text,
};
use crate::sensitive::{redact_text_secrets, redact_url_secrets};

impl Simulator {
  /// Starts or restarts periodic heartbeat scheduling.
  pub(in crate::simulator) fn start_heartbeat(&mut self, seconds: u64) {
    if seconds == 0 {
      self.log(UiLogLevel::Warn, "Heartbeat interval must be positive.");
      return;
    }

    self.stop_heartbeat();

    let tx = self.self_cmd_tx.clone();
    let handle = tokio::spawn(async move {
      let mut ticker = tokio::time::interval(Duration::from_secs(seconds));
      ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
      loop {
        ticker.tick().await;
        if tx.send(SimulatorCommand::HeartbeatTick).is_err() {
          break;
        }
      }
    });
    self.heartbeat = Some(HeartbeatTask { seconds, handle });
    self.log(
      UiLogLevel::Info,
      format!("Periodic heartbeat started: every {seconds}s"),
    );
    self.emit_snapshot();
  }

  /// Stops the periodic heartbeat task if one is active.
  pub(in crate::simulator) fn stop_heartbeat(&mut self) {
    if let Some(task) = self.heartbeat.take() {
      task.handle.abort();
      self.log(UiLogLevel::Info, "Periodic heartbeat stopped.");
      self.emit_snapshot();
    }
  }

  /// Sends a CALLRESULT frame for an inbound CALL.
  pub(in crate::simulator) async fn send_call_result(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    payload: Value,
  ) -> Result<()> {
    let text = build_call_result(message_id, &payload);
    self
      .send_text(
        write,
        text,
        UiLogLevel::Tx,
        format!("CALLRESULT {message_id}"),
      )
      .await
  }

  /// Sends a CALLERROR frame for an inbound CALL.
  pub(in crate::simulator) async fn send_call_error(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    code: &str,
    description: &str,
    details: Value,
  ) -> Result<()> {
    let text = build_call_error(message_id, code, description, &details);
    self
      .send_text(
        write,
        text,
        UiLogLevel::Tx,
        format!("CALLERROR {message_id} {code}"),
      )
      .await
  }

  /// Sends a `FormationViolation` CALLERROR for invalid inbound payloads.
  pub(in crate::simulator) async fn send_formation_violation(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    description: &str,
  ) -> Result<()> {
    self
      .send_call_error(
        write,
        message_id,
        OcppErrorCode::FormationViolation.as_str(),
        description,
        json!({}),
      )
      .await
  }

  /// Sends raw text over WebSocket and logs either summary or full frame.
  pub(in crate::simulator) async fn send_text(
    &mut self,
    write: &mut impl WsMessageSink,
    text: String,
    level: UiLogLevel,
    summary: String,
  ) -> Result<()> {
    write.send(Message::Text(text.clone().into())).await?;
    if self.config.trace_frames {
      self.log(level, sanitized_trace_frame_text(&text));
    } else {
      self.log(level, summary);
    }
    Ok(())
  }

  /// Allocates the next monotonic local OCPP message id.
  pub(in crate::simulator) fn next_message_id(&mut self) -> String {
    let message_id = format!("m{}", self.next_message_id);
    self.next_message_id = self.next_message_id.saturating_add(1);
    message_id
  }

  /// Emits a full simulator snapshot event to the UI channel.
  pub(in crate::simulator) fn emit_snapshot(&self) {
    let heartbeat_seconds = self.heartbeat.as_ref().map(|task| task.seconds);
    let connectors = self
      .connectors
      .iter()
      .map(|(id, state)| ConnectorSnapshot {
        id: *id,
        status: state.status.display().to_string(),
        meter_wh: state.meter_wh,
        transaction: state.transaction.as_ref().map(|tx| {
          match self.config.protocol {
            OcppVersion::V1_6 => format!(
              "local={} remote={}",
              tx.local_id,
              tx.v1_6_transaction_id
                .map_or_else(|| "-".to_string(), |value| value.to_string())
            ),
            OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
              format!("id={}", tx.transaction_uid)
            }
          }
        }),
      })
      .collect();

    let pending_action = self.pending_action_text();

    let connection_url = self.connection_url().map_or_else(
      |_| {
        self
          .config
          .ws_url
          .as_deref()
          .map(redact_url_secrets)
          .unwrap_or_default()
      },
      |u| redact_url_secrets(u.as_str()),
    );

    let snapshot = SimulatorSnapshot {
      profile: self.config.profile.clone(),
      cp_id: self.config.cp_id.clone(),
      protocol: self.config.protocol,
      connection_url,
      connected: self.connected,
      heartbeat_seconds,
      queue_depth: self.queue.len(),
      pending_action,
      connectors,
    };
    let _ = self.ui_tx.send(UiEvent::Snapshot(snapshot));
    self.emit_runtime_state();
  }

  /// Emits current connection and in-flight work state without snapshot logs.
  pub(in crate::simulator) fn emit_runtime_state(&self) {
    let _ = self.ui_tx.send(UiEvent::RuntimeState(self.runtime_state()));
  }

  fn runtime_state(&self) -> SimulatorRuntimeState {
    SimulatorRuntimeState {
      connected: self.connected,
      queue_depth: self.queue.len(),
      pending_action: self.pending_action_text(),
      active_transactions: self.active_transaction_count(),
      pending_reconnect: self.security.pending_reconnect.is_some(),
    }
  }

  fn active_transaction_count(&self) -> usize {
    self
      .connectors
      .values()
      .filter(|state| state.transaction.is_some())
      .count()
  }

  fn pending_action_text(&self) -> Option<String> {
    self.pending.as_ref().map(|pending| {
      format!("{} ({})", pending.call.action, pending.message_id)
    })
  }

  /// Emits one log event to the UI channel.
  pub(in crate::simulator) fn log<S: Into<String>>(
    &self,
    level: UiLogLevel,
    message: S,
  ) {
    let message = message.into();
    let _ = self.ui_tx.send(UiEvent::Log {
      level,
      message: redact_text_secrets(&message),
    });
  }
}
