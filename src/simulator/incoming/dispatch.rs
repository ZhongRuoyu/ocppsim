use super::super::payloads::{StatusPayload, to_value};
use super::super::{
  ResponseStatus, Result, Simulator, UiLogLevel, WsMessageSink,
};

impl Simulator {
  /// Sends a CALLRESULT with a standard `{ "status": "..." }` payload.
  pub(in crate::simulator) async fn send_status_response(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    status: ResponseStatus,
  ) -> Result<()> {
    self
      .send_call_result(
        write,
        message_id,
        to_value(&StatusPayload {
          status: status.as_str(),
        }),
      )
      .await
  }

  /// Resolves a requested connector or falls back to the first startable one.
  pub(in crate::simulator) fn requested_or_first_startable_connector(
    &self,
    requested_connector: Option<u16>,
  ) -> Option<u16> {
    match requested_connector {
      Some(connector) => Some(connector),
      None => self.first_startable_connector(),
    }
  }

  /// Resolves a start connector or replies with `Rejected` when none exist.
  pub(in crate::simulator) async fn resolve_start_connector_or_reject(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    requested_connector: Option<u16>,
  ) -> Result<Option<u16>> {
    if let Some(connector) =
      self.requested_or_first_startable_connector(requested_connector)
    {
      Ok(Some(connector))
    } else {
      self
        .send_status_response(write, message_id, ResponseStatus::Rejected)
        .await?;
      Ok(None)
    }
  }

  /// Resolves a connector and rejects requests for non-startable connectors.
  pub(in crate::simulator) async fn resolve_startable_connector_or_reject(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
    requested_connector: Option<u16>,
  ) -> Result<Option<u16>> {
    let Some(connector) = self
      .resolve_start_connector_or_reject(write, message_id, requested_connector)
      .await?
    else {
      return Ok(None);
    };
    if self.validate_start_connector(connector).is_ok() {
      Ok(Some(connector))
    } else {
      self
        .send_status_response(write, message_id, ResponseStatus::Rejected)
        .await?;
      Ok(None)
    }
  }

  /// Logs and acknowledges a reset request without mutating simulator state.
  pub(in crate::simulator) async fn handle_reset_only_call(
    &mut self,
    write: &mut impl WsMessageSink,
    message_id: &str,
  ) -> Result<()> {
    self.log(
      UiLogLevel::Info,
      "Received Reset request. Simulator will acknowledge only.",
    );
    self
      .send_status_response(write, message_id, ResponseStatus::Accepted)
      .await
  }
}
