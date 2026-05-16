use super::super::*;

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
        let reason = close
          .as_ref()
          .map(|item| item.reason.to_string())
          .unwrap_or_else(|| "No reason".to_string());
        return Err(anyhow!("Connection closed by CSMS: {reason}"));
      }
      Message::Ping(payload) => {
        write.send(Message::Pong(payload)).await?;
      }
      Message::Pong(_) => {}
      Message::Binary(_) => {}
      Message::Frame(_) => {}
    }
    Ok(())
  }

  /// Parses inbound OCPP text frames and dispatches protocol-level handlers.
  pub(in crate::simulator) async fn handle_ws_text(
    &mut self,
    text: String,
    write: &mut WsWrite,
  ) -> Result<()> {
    if self.config.trace_frames {
      self.log(UiLogLevel::Rx, text.clone());
    }

    match parse_frame(&text) {
      Ok(OcppFrame::Call {
        message_id,
        action,
        payload,
      }) => {
        self.log(UiLogLevel::Rx, format!("CALL {} {}", message_id, action));
        self
          .handle_incoming_call(write, &message_id, &action, payload)
          .await?;
      }
      Ok(OcppFrame::CallResult {
        message_id,
        payload,
      }) => {
        self.log(UiLogLevel::Rx, format!("CALLRESULT {}", message_id));
        self.handle_call_result(&message_id, payload)?;
      }
      Ok(OcppFrame::CallError {
        message_id,
        code,
        description,
        details,
      }) => {
        self.log(
          UiLogLevel::Rx,
          format!("CALLERROR {} {} {}", message_id, code, description),
        );
        if self.config.trace_frames {
          self.log(UiLogLevel::Rx, format!("CALLERROR details={details}"));
        }
        self.handle_call_error(&message_id, &code, &description)?;
      }
      Ok(OcppFrame::CallResultError {
        message_id,
        code,
        description,
        details,
      }) => {
        self.log(
          UiLogLevel::Warn,
          format!(
            "Received CALLRESULTERROR {} {} {}",
            message_id, code, description
          ),
        );
        if self.config.trace_frames {
          self.log(
            UiLogLevel::Warn,
            format!("CALLRESULTERROR details={details}"),
          );
        }
      }
      Ok(OcppFrame::Send {
        message_id,
        action,
        payload,
      }) => {
        self.log(
          UiLogLevel::Warn,
          format!(
            "Received SEND {} {} (no response expected)",
            message_id, action
          ),
        );
        if self.config.trace_frames {
          self.log(UiLogLevel::Rx, format!("SEND payload={payload}"));
        }
      }
      Ok(OcppFrame::Unsupported {
        message_type,
        message_id,
      }) => {
        let id = message_id.unwrap_or_else(|| "-1".to_string());
        self.log(
          UiLogLevel::Warn,
          format!("Unsupported message type {}.", message_type),
        );
        let error = build_call_error(
          &id,
          "MessageTypeNotSupported",
          "Unsupported OCPP message type",
          json!({}),
        );
        self
          .send_text(write, error, UiLogLevel::Tx, "CALLERROR".to_string())
          .await?;
      }
      Err(error) => {
        self.log(UiLogLevel::Warn, format!("Malformed OCPP frame: {error}"));
        let call_error = build_call_error(
          "-1",
          "ProtocolError",
          "Malformed OCPP frame",
          json!({ "reason": error }),
        );
        self
          .send_text(write, call_error, UiLogLevel::Tx, "CALLERROR".to_string())
          .await?;
      }
    }

    Ok(())
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
