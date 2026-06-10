use crate::ocpp::OcppMessageTypeId;

use super::super::{
  Message, OcppErrorCode, OcppFrame, OcppVersion, Result, Simulator, SinkExt,
  UiLogLevel, Value, WsWrite, anyhow, build_call_error, json, parse_frame,
  sanitized_trace_details, sanitized_trace_frame, sanitized_trace_payload,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncomingRequestSchemaValidation {
  Valid,
  MissingSchema,
}

impl Simulator {
  /// Handles one inbound WebSocket frame and routes it by WS frame type.
  pub(in crate::simulator) async fn handle_ws_message(
    &mut self,
    frame: Message,
    write: &mut WsWrite,
  ) -> Result<()> {
    match frame {
      Message::Text(text) => {
        self.handle_ws_text(text.to_string(), write).await?;
      }
      Message::Close(close) => {
        let reason = close.as_ref().map_or_else(
          || "No reason".to_string(),
          |item| item.reason.to_string(),
        );
        return Err(anyhow!("Connection closed by CSMS: {reason}"));
      }
      Message::Ping(payload) => {
        write.send(Message::Pong(payload)).await?;
      }
      Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
    }
    Ok(())
  }

  /// Parses inbound OCPP text frames and dispatches protocol-level handlers.
  pub(in crate::simulator) async fn handle_ws_text(
    &mut self,
    text: String,
    write: &mut WsWrite,
  ) -> Result<()> {
    match parse_frame(&text) {
      Ok(frame) => {
        let frame = frame_supported_for_protocol(self.config.protocol, frame);
        if self.config.trace_frames {
          self.log(UiLogLevel::Rx, sanitized_trace_frame(&frame));
        }
        self.handle_parsed_ws_frame(frame, write).await?;
        self.emit_runtime_state();
      }
      Err(error) => {
        if self.config.trace_frames {
          self.log(UiLogLevel::Rx, "Malformed OCPP frame omitted from trace.");
        }
        self.handle_malformed_ws_frame(write, error).await?;
      }
    }

    Ok(())
  }

  async fn handle_parsed_ws_frame(
    &mut self,
    frame: OcppFrame,
    write: &mut WsWrite,
  ) -> Result<()> {
    match frame {
      OcppFrame::Call {
        message_id,
        action,
        payload,
      } => {
        self.log(UiLogLevel::Rx, format!("CALL {message_id} {action}"));
        self
          .handle_incoming_call(write, &message_id, &action, payload)
          .await?;
      }
      OcppFrame::CallResult {
        message_id,
        payload,
      } => {
        self.log(UiLogLevel::Rx, format!("CALLRESULT {message_id}"));
        self.handle_call_result(&message_id, &payload)?;
      }
      OcppFrame::CallError {
        message_id,
        code,
        description,
        details,
      } => {
        self.handle_call_error_frame(
          &message_id,
          &code,
          &description,
          &details,
        )?;
      }
      OcppFrame::CallResultError {
        message_id,
        code,
        description,
        details,
      } => {
        self.handle_call_result_error_frame(
          &message_id,
          &code,
          &description,
          &details,
        );
      }
      OcppFrame::Send {
        message_id,
        action,
        payload,
      } => {
        self.handle_send_frame(&message_id, &action, &payload);
      }
      OcppFrame::Unsupported {
        message_type,
        message_id,
      } => {
        let id = message_id.unwrap_or_else(|| "-1".to_string());
        self.log(
          UiLogLevel::Warn,
          format!("Unsupported message type {message_type}."),
        );
        let error = build_call_error(
          &id,
          OcppErrorCode::MessageTypeNotSupported.as_str(),
          "Unsupported OCPP message type",
          &json!({}),
        );
        self
          .send_text(write, error, UiLogLevel::Tx, "CALLERROR".to_string())
          .await?;
      }
    }
    Ok(())
  }

  fn handle_call_error_frame(
    &mut self,
    message_id: &str,
    code: &str,
    description: &str,
    details: &Value,
  ) -> Result<()> {
    self.log(
      UiLogLevel::Rx,
      format!("CALLERROR {message_id} {code} {description}"),
    );
    if self.config.trace_frames {
      self.log(
        UiLogLevel::Rx,
        format!("CALLERROR details={}", sanitized_trace_details(details)),
      );
    }
    self.handle_call_error(message_id, code, description)
  }

  fn handle_call_result_error_frame(
    &mut self,
    message_id: &str,
    code: &str,
    description: &str,
    details: &Value,
  ) {
    self.log(
      UiLogLevel::Warn,
      format!("Received CALLRESULTERROR {message_id} {code} {description}"),
    );
    if self.config.trace_frames {
      self.log(
        UiLogLevel::Warn,
        format!(
          "CALLRESULTERROR details={}",
          sanitized_trace_details(details)
        ),
      );
    }
  }

  fn handle_send_frame(
    &mut self,
    message_id: &str,
    action: &str,
    payload: &Value,
  ) {
    self.log(
      UiLogLevel::Warn,
      format!("Received SEND {message_id} {action} (no response expected)"),
    );
    if self.config.trace_frames {
      self.log(
        UiLogLevel::Rx,
        format!("SEND payload={}", sanitized_trace_payload(action, payload)),
      );
    }
  }

  async fn handle_malformed_ws_frame(
    &mut self,
    write: &mut WsWrite,
    error: impl std::fmt::Display,
  ) -> Result<()> {
    self.log(UiLogLevel::Warn, format!("Malformed OCPP frame: {error}"));
    let call_error = build_call_error(
      "-1",
      OcppErrorCode::ProtocolError.as_str(),
      "Malformed OCPP frame",
      &json!({ "reason": error.to_string() }),
    );
    self
      .send_text(write, call_error, UiLogLevel::Tx, "CALLERROR".to_string())
      .await
  }

  /// Dispatches an inbound CALL to the active protocol-version handler.
  pub(in crate::simulator) async fn handle_incoming_call(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: &str,
    payload: Value,
  ) -> Result<()> {
    if self.config.strict {
      match self.validate_incoming_request_schema(action, &payload) {
        Ok(IncomingRequestSchemaValidation::Valid) => {}
        Ok(IncomingRequestSchemaValidation::MissingSchema) => {
          self.log(
            UiLogLevel::Warn,
            format!(
              "Strict schema coverage is missing for {} request {action}; \
              payload validation skipped.",
              self.config.protocol.label()
            ),
          );
        }
        Err(error) => {
          self.log(
            UiLogLevel::Warn,
            format!("Strict schema validation failed for {action}: {error}"),
          );
          self
            .send_formation_violation(write, message_id, &error.to_string())
            .await?;
          return Ok(());
        }
      }
    }

    match self.config.protocol {
      OcppVersion::V1_6 => {
        self
          .handle_incoming_call_v1_6(write, message_id, action, payload)
          .await
      }
      OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
        self
          .handle_incoming_call_v2_x(write, message_id, action, payload)
          .await
      }
    }
  }

  /// Validates an inbound CALL payload against the checked-in request schema.
  fn validate_incoming_request_schema(
    &self,
    action: &str,
    payload: &Value,
  ) -> Result<IncomingRequestSchemaValidation> {
    let Some(schema_text) =
      crate::embedded_schemas::incoming_request_schema_text(
        self.config.protocol,
        action,
      )
    else {
      return Ok(IncomingRequestSchemaValidation::MissingSchema);
    };

    let schema: Value = serde_json::from_str(schema_text)?;
    let validator = jsonschema::validator_for(&schema)?;
    let errors = validator
      .iter_errors(payload)
      .take(5)
      .map(|error| error.to_string())
      .collect::<Vec<_>>();
    if errors.is_empty() {
      return Ok(IncomingRequestSchemaValidation::Valid);
    }

    Err(anyhow!(errors.join("; ")))
  }
}

fn frame_supported_for_protocol(
  protocol: OcppVersion,
  frame: OcppFrame,
) -> OcppFrame {
  if protocol == OcppVersion::V2_1 {
    return frame;
  }

  match frame {
    OcppFrame::CallResultError { message_id, .. } => OcppFrame::Unsupported {
      message_type: OcppMessageTypeId::CallResultError.value(),
      message_id: Some(message_id),
    },
    OcppFrame::Send { message_id, .. } => OcppFrame::Unsupported {
      message_type: OcppMessageTypeId::Send.value(),
      message_id: Some(message_id),
    },
    _ => frame,
  }
}
