use super::super::payloads::{
  ListVersion_V2_X_Response, RequestStartTransactionResponse, to_value,
};
use super::super::{
  IncomingAction_V2_X, OcppErrorCode, ResponseStatus, Result, Simulator,
  TriggerMessage_V2_X, UiLogLevel, Value, WsWrite, json,
};
use super::request::{
  RequestStartTransactionRequest_V2_X, RequestStopTransactionRequest_V2_X,
  TriggerMessageRequest_V2_X,
};

impl Simulator {
  /// Dispatches an inbound OCPP 2.x CALL action and sends its response.
  pub(in crate::simulator) async fn handle_incoming_call_v2_x(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: &str,
    payload: Value,
  ) -> Result<()> {
    let Some(parsed_action) = IncomingAction_V2_X::parse(action) else {
      return self
        .handle_unknown_incoming_action_v2_x(write, message_id, action)
        .await;
    };

    self
      .handle_parsed_incoming_call_v2_x_primary(
        write,
        message_id,
        parsed_action,
        &payload,
      )
      .await
  }

  async fn handle_parsed_incoming_call_v2_x_primary(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: IncomingAction_V2_X,
    payload: &Value,
  ) -> Result<()> {
    match action {
      IncomingAction_V2_X::ChangeAvailability => dispatch_status!(
        self,
        write,
        message_id,
        self.change_availability_v2_x(payload)
      ),
      IncomingAction_V2_X::ClearCache => {
        self
          .send_status_response(write, message_id, ResponseStatus::Accepted)
          .await?;
      }
      IncomingAction_V2_X::DataTransfer => {
        self
          .send_call_result(
            write,
            message_id,
            Self::data_transfer_v2_x(payload),
          )
          .await?;
      }
      IncomingAction_V2_X::GetLocalListVersion => {
        self
          .send_call_result(
            write,
            message_id,
            to_value(&ListVersion_V2_X_Response {
              version_number: self.local_auth_list_version,
            }),
          )
          .await?;
      }
      IncomingAction_V2_X::GetLog => {
        dispatch_response!(self, write, message_id, self.get_log_v2_x(payload));
      }
      IncomingAction_V2_X::GetVariables => dispatch_response!(
        self,
        write,
        message_id,
        self.get_variables_v2_x(payload)
      ),
      IncomingAction_V2_X::RequestStartTransaction => {
        self
          .handle_request_start_transaction_call_v2_x(
            write, message_id, payload,
          )
          .await?;
      }
      IncomingAction_V2_X::RequestStopTransaction => {
        self
          .handle_request_stop_transaction_call_v2_x(write, message_id, payload)
          .await?;
      }
      other => {
        self
          .handle_parsed_incoming_call_v2_x_secondary(
            write, message_id, other, payload,
          )
          .await?;
      }
    }
    Ok(())
  }

  async fn handle_parsed_incoming_call_v2_x_secondary(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: IncomingAction_V2_X,
    payload: &Value,
  ) -> Result<()> {
    match action {
      IncomingAction_V2_X::ReserveNow => dispatch_status!(
        self,
        write,
        message_id,
        self.reserve_now_v2_x(payload)
      ),
      IncomingAction_V2_X::CancelReservation => dispatch_status!(
        self,
        write,
        message_id,
        self.cancel_reservation_v2_x(payload)
      ),
      IncomingAction_V2_X::SendLocalList => dispatch_status!(
        self,
        write,
        message_id,
        self.send_local_list_v2_x(payload)
      ),
      IncomingAction_V2_X::SetChargingProfile => dispatch_status!(
        self,
        write,
        message_id,
        self.set_charging_profile_v2_x(payload)
      ),
      IncomingAction_V2_X::SetVariables => dispatch_response!(
        self,
        write,
        message_id,
        self.set_variables_v2_x(payload)
      ),
      IncomingAction_V2_X::ClearChargingProfile => {
        let status = self.clear_charging_profile_v2_x(payload);
        self.send_status_response(write, message_id, status).await?;
      }
      IncomingAction_V2_X::GetCompositeSchedule => dispatch_response!(
        self,
        write,
        message_id,
        self.get_composite_schedule_v2_x(payload)
      ),
      IncomingAction_V2_X::TriggerMessage => {
        self
          .handle_trigger_message_call_v2_x(write, message_id, payload)
          .await?;
      }
      IncomingAction_V2_X::UnlockConnector => dispatch_status!(
        self,
        write,
        message_id,
        self.unlock_connector_v2_x(payload)
      ),
      IncomingAction_V2_X::UpdateFirmware => dispatch_status!(
        self,
        write,
        message_id,
        self.update_firmware_v2_x(payload)
      ),
      IncomingAction_V2_X::Reset => {
        self.log(
          UiLogLevel::Info,
          "Received Reset request. Simulator will acknowledge only.",
        );
        self
          .send_status_response(write, message_id, ResponseStatus::Accepted)
          .await?;
      }
      IncomingAction_V2_X::ChangeAvailability
      | IncomingAction_V2_X::ClearCache
      | IncomingAction_V2_X::DataTransfer
      | IncomingAction_V2_X::GetLocalListVersion
      | IncomingAction_V2_X::GetLog
      | IncomingAction_V2_X::GetVariables
      | IncomingAction_V2_X::RequestStartTransaction
      | IncomingAction_V2_X::RequestStopTransaction => unreachable!(),
    }
    Ok(())
  }

  async fn handle_request_start_transaction_call_v2_x(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match RequestStartTransactionRequest_V2_X::parse(payload) {
      Ok(value) => value,
      Err(error) => {
        return self
          .send_formation_violation(write, message_id, &error.to_string())
          .await;
      }
    };
    let connector = match request.connector {
      Some(value) => value,
      None => match self.first_startable_connector() {
        Some(value) => value,
        None => {
          return self
            .send_status_response(write, message_id, ResponseStatus::Rejected)
            .await;
        }
      },
    };

    if let Some(existing) = self.active_transaction_uid(connector) {
      return self
        .send_call_result(
          write,
          message_id,
          to_value(&RequestStartTransactionResponse {
            status: ResponseStatus::Accepted.as_str(),
            transaction_id: Some(&existing),
          }),
        )
        .await;
    }

    let status = if self
      .start_transaction(
        connector,
        request.id_token,
        true,
        Some(request.remote_start_id),
        true,
      )
      .is_ok()
    {
      ResponseStatus::Accepted
    } else {
      ResponseStatus::Rejected
    };
    self.send_status_response(write, message_id, status).await
  }

  async fn handle_request_stop_transaction_call_v2_x(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match RequestStopTransactionRequest_V2_X::parse(payload) {
      Ok(value) => value,
      Err(error) => {
        return self
          .send_formation_violation(write, message_id, &error.to_string())
          .await;
      }
    };
    let status = if let Some(connector) =
      self.find_transaction_by_uid(&request.transaction_id)
    {
      if self.stop_transaction(connector, None, true, true).is_ok() {
        ResponseStatus::Accepted
      } else {
        ResponseStatus::Rejected
      }
    } else {
      ResponseStatus::Rejected
    };
    self.send_status_response(write, message_id, status).await
  }

  async fn handle_trigger_message_call_v2_x(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match TriggerMessageRequest_V2_X::parse(payload) {
      Ok(value) => value,
      Err(error) => {
        return self
          .send_formation_violation(write, message_id, &error.to_string())
          .await;
      }
    };
    let Some(message) = TriggerMessage_V2_X::parse(&request.requested_message)
    else {
      return self
        .send_status_response(write, message_id, ResponseStatus::NotImplemented)
        .await;
    };
    match self.trigger_message_v2_x(message, request.connector) {
      Ok(()) => {
        self
          .send_status_response(write, message_id, ResponseStatus::Accepted)
          .await
      }
      Err(error) => {
        self
          .send_formation_violation(write, message_id, &error.to_string())
          .await
      }
    }
  }

  async fn handle_unknown_incoming_action_v2_x(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: &str,
  ) -> Result<()> {
    let (code, description) = if IncomingAction_V2_X::is_known_unsupported(
      action,
      self.config.protocol,
    ) {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Action `{action}` is known for {} but is not supported.",
          self.config.protocol.label()
        ),
      );
      (
        OcppErrorCode::NotSupported.as_str(),
        format!("Action `{action}` is outside the supported subset."),
      )
    } else {
      self.log(
        UiLogLevel::Warn,
        format!("Action `{action}` is not implemented."),
      );
      (
        OcppErrorCode::NotImplemented.as_str(),
        format!("Action `{action}` is not implemented."),
      )
    };
    self
      .send_call_error(write, message_id, code, &description, json!({}))
      .await
  }
}
