use super::super::{Result, Simulator, UiLogLevel, Value};

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
    self.emit_runtime_state();
    Ok(())
  }

  /// Rejects a schema-invalid CALLRESULT without applying response side effects.
  pub(in crate::simulator) fn reject_call_result(&mut self, message_id: &str) {
    let Some(pending) = &self.pending else {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Unexpected invalid CALLRESULT {message_id} (no pending request)."
        ),
      );
      return;
    };
    if pending.message_id != message_id {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Invalid CALLRESULT {} does not match pending {}.",
          message_id, pending.message_id
        ),
      );
      return;
    }

    let pending = self.pending.take().expect("pending exists");
    let _ = self.handle_pending_failure_context(&pending.call.context);
    self.emit_runtime_state();
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

    self.handle_pending_failure_context(&pending.call.context)?;
    self.emit_runtime_state();
    Ok(())
  }
}
