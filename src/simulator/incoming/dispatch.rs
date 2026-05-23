use super::super::payloads::{StatusPayload, to_value};
use super::super::{ResponseStatus, Result, Simulator, WsWrite};

impl Simulator {
  /// Sends a CALLRESULT with a standard `{ "status": "..." }` payload.
  pub(in crate::simulator) async fn send_status_response(
    &mut self,
    write: &mut WsWrite,
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
}
