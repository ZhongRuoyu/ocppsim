use super::super::payloads::{ListVersionV1_6Response, to_value};
use super::super::{
  IncomingAction_V1_6, OcppErrorCode, ResponseStatus, Result, Simulator,
  TriggerMessage_V1_6, UiLogLevel, Value, WsWrite, json,
};
use super::request::{
  RemoteStartTransactionRequestV1_6, RemoteStopTransactionRequestV1_6,
  TriggerMessageRequestV1_6,
};

impl Simulator {
  /// Dispatches an inbound OCPP 1.6 CALL action and sends its response.
  pub(in crate::simulator) async fn handle_incoming_call_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: &str,
    payload: Value,
  ) -> Result<()> {
    let Some(parsed_action) = IncomingAction_V1_6::parse(action) else {
      return self
        .handle_unknown_incoming_action_v1_6(write, message_id, action)
        .await;
    };

    self
      .handle_parsed_incoming_call_v1_6_primary(
        write,
        message_id,
        parsed_action,
        &payload,
      )
      .await
  }

  async fn handle_parsed_incoming_call_v1_6_primary(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: IncomingAction_V1_6,
    payload: &Value,
  ) -> Result<()> {
    match action {
      IncomingAction_V1_6::GetConfiguration => {
        let response = self.configuration_response_v1_6(payload);
        self
          .send_call_result(write, message_id, to_value(&response))
          .await?;
      }
      IncomingAction_V1_6::ChangeConfiguration => {
        let status = self.change_configuration_v1_6(payload);
        self.send_status_response(write, message_id, status).await?;
      }
      IncomingAction_V1_6::ClearCache => {
        self
          .send_status_response(write, message_id, ResponseStatus::Accepted)
          .await?;
      }
      IncomingAction_V1_6::ChangeAvailability => dispatch_status!(
        self,
        write,
        message_id,
        self.change_availability_v1_6(payload)
      ),
      IncomingAction_V1_6::DataTransfer => {
        self
          .send_call_result(
            write,
            message_id,
            Self::data_transfer_v1_6(payload),
          )
          .await?;
      }
      IncomingAction_V1_6::GetDiagnostics => dispatch_response!(
        self,
        write,
        message_id,
        self.get_diagnostics_v1_6(payload)
      ),
      IncomingAction_V1_6::UpdateFirmware => {
        self
          .handle_update_firmware_call_v1_6(write, message_id, payload)
          .await?;
      }
      IncomingAction_V1_6::RemoteStartTransaction => {
        self
          .handle_remote_start_transaction_call_v1_6(write, message_id, payload)
          .await?;
      }
      IncomingAction_V1_6::RemoteStopTransaction => {
        self
          .handle_remote_stop_transaction_call_v1_6(write, message_id, payload)
          .await?;
      }
      other => {
        self
          .handle_parsed_incoming_call_v1_6_secondary(
            write, message_id, other, payload,
          )
          .await?;
      }
    }
    Ok(())
  }

  async fn handle_parsed_incoming_call_v1_6_secondary(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: IncomingAction_V1_6,
    payload: &Value,
  ) -> Result<()> {
    match action {
      IncomingAction_V1_6::ReserveNow => dispatch_status!(
        self,
        write,
        message_id,
        self.reserve_now_v1_6(payload)
      ),
      IncomingAction_V1_6::CancelReservation => dispatch_status!(
        self,
        write,
        message_id,
        self.cancel_reservation_v1_6(payload)
      ),
      IncomingAction_V1_6::UnlockConnector => dispatch_status!(
        self,
        write,
        message_id,
        self.unlock_connector_v1_6(payload)
      ),
      IncomingAction_V1_6::GetLocalListVersion => {
        self
          .send_call_result(
            write,
            message_id,
            to_value(&ListVersionV1_6Response {
              list_version: self.local_auth_list_version,
            }),
          )
          .await?;
      }
      IncomingAction_V1_6::SendLocalList => dispatch_status!(
        self,
        write,
        message_id,
        self.send_local_list_v1_6(payload)
      ),
      IncomingAction_V1_6::SetChargingProfile => dispatch_status!(
        self,
        write,
        message_id,
        self.set_charging_profile_v1_6(payload)
      ),
      IncomingAction_V1_6::ClearChargingProfile => {
        let status = self.clear_charging_profile_v1_6(payload);
        self.send_status_response(write, message_id, status).await?;
      }
      IncomingAction_V1_6::GetCompositeSchedule => dispatch_response!(
        self,
        write,
        message_id,
        self.get_composite_schedule_v1_6(payload)
      ),
      IncomingAction_V1_6::TriggerMessage => {
        self
          .handle_trigger_message_call_v1_6(write, message_id, payload)
          .await?;
      }
      IncomingAction_V1_6::Reset => {
        self.log(
          UiLogLevel::Info,
          "Received Reset request. Simulator will acknowledge only.",
        );
        self
          .send_status_response(write, message_id, ResponseStatus::Accepted)
          .await?;
      }
      IncomingAction_V1_6::GetConfiguration
      | IncomingAction_V1_6::ChangeConfiguration
      | IncomingAction_V1_6::ClearCache
      | IncomingAction_V1_6::ChangeAvailability
      | IncomingAction_V1_6::DataTransfer
      | IncomingAction_V1_6::GetDiagnostics
      | IncomingAction_V1_6::UpdateFirmware
      | IncomingAction_V1_6::RemoteStartTransaction
      | IncomingAction_V1_6::RemoteStopTransaction => unreachable!(),
    }
    Ok(())
  }

  async fn handle_update_firmware_call_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    match self.update_firmware_v1_6(payload) {
      Ok(()) => self.send_call_result(write, message_id, json!({})).await,
      Err(error) => {
        self
          .send_formation_violation(write, message_id, &error.to_string())
          .await
      }
    }
  }

  async fn handle_remote_start_transaction_call_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match RemoteStartTransactionRequestV1_6::parse(payload) {
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
    let status = if self.authorize_remote_tx_requests() {
      self.enqueue_remote_start_authorize_v1_6(connector, request.id_token);
      ResponseStatus::Accepted
    } else if self
      .start_transaction(connector, request.id_token, true, None, true)
      .is_ok()
    {
      ResponseStatus::Accepted
    } else {
      ResponseStatus::Rejected
    };
    self.send_status_response(write, message_id, status).await
  }

  async fn handle_remote_stop_transaction_call_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match RemoteStopTransactionRequestV1_6::parse(payload) {
      Ok(value) => value,
      Err(error) => {
        return self
          .send_formation_violation(write, message_id, &error.to_string())
          .await;
      }
    };
    let status = if let Some(connector_id) =
      self.find_v1_6_transaction(request.transaction_id)
    {
      self.stop_transaction(connector_id, None, true, true)?;
      ResponseStatus::Accepted
    } else {
      ResponseStatus::Rejected
    };
    self.send_status_response(write, message_id, status).await
  }

  async fn handle_trigger_message_call_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    payload: &Value,
  ) -> Result<()> {
    let request = match TriggerMessageRequestV1_6::parse(payload) {
      Ok(value) => value,
      Err(error) => {
        return self
          .send_formation_violation(write, message_id, &error.to_string())
          .await;
      }
    };
    let Some(message) = TriggerMessage_V1_6::parse(&request.requested_message)
    else {
      return self
        .send_status_response(write, message_id, ResponseStatus::NotImplemented)
        .await;
    };
    match self.trigger_message_v1_6(message, request.connector) {
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

  async fn handle_unknown_incoming_action_v1_6(
    &mut self,
    write: &mut WsWrite,
    message_id: &str,
    action: &str,
  ) -> Result<()> {
    let (code, description) =
      if IncomingAction_V1_6::is_known_unsupported(action) {
        self.log(
          UiLogLevel::Warn,
          format!(
            "Action `{action}` is known for OCPP 1.6 but is not supported."
          ),
        );
        (
          OcppErrorCode::NotSupported.as_str(),
          format!("Action `{action}` is outside the supported base schemas."),
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
