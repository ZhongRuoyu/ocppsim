/// Dispatches a fallible handler that returns `Result<ResponseStatus>`.
///
/// On success, sends a CALLRESULT with a `{ "status": "..." }` payload.
/// On error, sends the protocol-specific format-violation CALLERROR.
macro_rules! dispatch_status {
  ($self:ident, $write:ident, $mid:ident, $handler:expr) => {
    match $handler {
      Ok(status) => {
        $self
          .send_call_result(
            $write,
            $mid,
            crate::simulator::payloads::to_value(
              &crate::simulator::payloads::StatusPayload {
                status: status.as_str(),
              },
            ),
          )
          .await?;
      }
      Err(error) => {
        $self
          .send_format_violation($write, $mid, &error.to_string())
          .await?;
      }
    }
  };
}

/// Dispatches a fallible handler that returns `Result<Value>`.
///
/// On success, sends the `Value` as a CALLRESULT payload.
/// On error, sends the protocol-specific format-violation CALLERROR.
macro_rules! dispatch_response {
  ($self:ident, $write:ident, $mid:ident, $handler:expr) => {
    match $handler {
      Ok(response) => {
        $self.send_call_result($write, $mid, response).await?;
      }
      Err(error) => {
        $self
          .send_format_violation($write, $mid, &error.to_string())
          .await?;
      }
    }
  };
}

mod dispatch;
mod dispatch_v1_6;
mod dispatch_v2_x;
mod pending;
mod request;
mod shared;
mod v1_6;
mod v2_x;
mod ws;
