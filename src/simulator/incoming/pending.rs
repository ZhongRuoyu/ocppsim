use super::super::{
  PendingContext, Result, Simulator, TxEventType, UiLogLevel, Value,
};

impl Simulator {
  /// Applies a CALLRESULT to the current pending request context.
  pub(in crate::simulator) fn handle_call_result(
    &mut self,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let Some(pending) = &self.pending else {
      self.log(
        UiLogLevel::Warn,
        format!("Unexpected CALLRESULT {message_id} (no pending request)."),
      );
      return Ok(());
    };
    if pending.message_id != message_id {
      self.log(
        UiLogLevel::Warn,
        format!(
          "CALLRESULT {} does not match pending {}.",
          message_id, pending.message_id
        ),
      );
      return Ok(());
    }

    let pending = self.pending.take().expect("pending exists");
    self.apply_call_result_context(&pending.call.context, payload)?;
    Ok(())
  }

  /// Handles CALLERROR for the current pending request and performs rollback.
  pub(in crate::simulator) fn handle_call_error(
    &mut self,
    message_id: &str,
    code: &str,
    description: &str,
  ) -> Result<()> {
    let Some(pending) = &self.pending else {
      self.log(
        UiLogLevel::Warn,
        format!("Unexpected CALLERROR {message_id}: {code} {description}"),
      );
      return Ok(());
    };
    if pending.message_id != message_id {
      self.log(
        UiLogLevel::Warn,
        format!(
          "CALLERROR {} does not match pending {}.",
          message_id, pending.message_id
        ),
      );
      return Ok(());
    }

    let pending = self.pending.take().expect("pending exists");
    self.log(
      UiLogLevel::Error,
      format!("{} failed: {} {}", pending.call.action, code, description),
    );

    match pending.call.context {
      PendingContext::StartTxV1_6 {
        connector,
        local_tx_id,
      }
      | PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type: TxEventType::Started,
      } => {
        self.cancel_transaction_start(connector, local_tx_id)?;
        self.enqueue_status_notification(connector)?;
      }
      PendingContext::StopTxV1_6 {
        connector,
        local_tx_id,
      }
      | PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type: TxEventType::Ended,
      } => {
        self.restore_active_transaction_status(connector, local_tx_id)?;
        self.enqueue_status_notification(connector)?;
      }
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
      | PendingContext::MeterValues { .. } => {}
    }
    Ok(())
  }
}
