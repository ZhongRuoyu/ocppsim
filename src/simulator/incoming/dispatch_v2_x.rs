use super::super::payloads::{
  ListVersion_V2_X_Response, RequestStartTransactionResponse, to_value,
};
use super::super::{
  IncomingAction_V2_X, OcppErrorCode, ResponseStatus, Result, Simulator,
  TriggerMessage_V2_X, UiLogLevel, Value, WsMessageSink, json,
};
use super::request::{
  RequestStartTransactionRequest_V2_X, RequestStopTransactionRequest_V2_X,
  TriggerMessageRequest_V2_X,
};

impl Simulator {
  /// Dispatches an inbound OCPP 2.x CALL action and sends its response.
  pub(in crate::simulator) async fn handle_incoming_call_v2_x(
    &mut self,
    write: &mut impl WsMessageSink,
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
    write: &mut impl WsMessageSink,
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
      IncomingAction_V2_X::DataTransfer => dispatch_response!(
        self,
        write,
        message_id,
        Self::data_transfer_v2_x(payload)
      ),
      IncomingAction_V2_X::CertificateSigned => dispatch_status!(
        self,
        write,
        message_id,
        self.certificate_signed_v2_x(payload)
      ),
      IncomingAction_V2_X::DeleteCertificate => dispatch_status!(
        self,
        write,
        message_id,
        self.delete_certificate_from_payload(payload)
      ),
      IncomingAction_V2_X::GetInstalledCertificateIds => dispatch_response!(
        self,
        write,
        message_id,
        self.get_installed_certificate_ids_v2_x(payload)
      ),
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
      IncomingAction_V2_X::InstallCertificate => dispatch_status!(
        self,
        write,
        message_id,
        self.install_certificate_from_payload(payload)
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
    write: &mut impl WsMessageSink,
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
      IncomingAction_V2_X::ClearChargingProfile => dispatch_status!(
        self,
        write,
        message_id,
        self.clear_charging_profile_v2_x(payload)
      ),
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
        self.handle_reset_only_call(write, message_id).await?;
      }
      IncomingAction_V2_X::ChangeAvailability
      | IncomingAction_V2_X::ClearCache
      | IncomingAction_V2_X::DataTransfer
      | IncomingAction_V2_X::CertificateSigned
      | IncomingAction_V2_X::DeleteCertificate
      | IncomingAction_V2_X::GetInstalledCertificateIds
      | IncomingAction_V2_X::GetLocalListVersion
      | IncomingAction_V2_X::GetLog
      | IncomingAction_V2_X::GetVariables
      | IncomingAction_V2_X::InstallCertificate
      | IncomingAction_V2_X::RequestStartTransaction
      | IncomingAction_V2_X::RequestStopTransaction => unreachable!(),
    }
    Ok(())
  }

  async fn handle_request_start_transaction_call_v2_x(
    &mut self,
    write: &mut impl WsMessageSink,
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
    if !self.post_boot_ocpp_requests_allowed() {
      return self
        .send_status_response(write, message_id, ResponseStatus::Rejected)
        .await;
    }
    let Some(connector) = self
      .resolve_start_connector_or_reject(write, message_id, request.connector)
      .await?
    else {
      return Ok(());
    };

    if let Some(existing) = self.active_transaction_uid(connector) {
      let status = if self.active_transaction_authorized(connector) {
        ResponseStatus::Rejected
      } else if let Err(error) = self.apply_remote_start_charging_profile(
        connector,
        request.charging_profile.as_ref(),
      ) {
        self.log(
          UiLogLevel::Warn,
          format!(
            "RequestStartTransaction charging profile rejected on EVSE \
            {connector}: {error}"
          ),
        );
        ResponseStatus::Rejected
      } else {
        ResponseStatus::Accepted
      };
      let transaction_id =
        (status == ResponseStatus::Accepted).then_some(existing.as_str());
      return self
        .send_call_result(
          write,
          message_id,
          to_value(&RequestStartTransactionResponse {
            status: status.as_str(),
            transaction_id,
          }),
        )
        .await;
    }

    let RequestStartTransactionRequest_V2_X {
      connector: _,
      remote_start_id,
      id_token,
      charging_profile,
    } = request;
    let status = if self
      .start_transaction(connector, id_token, true, Some(remote_start_id), true)
      .and_then(|()| {
        self.apply_remote_start_charging_profile(
          connector,
          charging_profile.as_ref(),
        )
      })
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
    write: &mut impl WsMessageSink,
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
    if !self.post_boot_ocpp_requests_allowed() {
      return self
        .send_status_response(write, message_id, ResponseStatus::Rejected)
        .await;
    }
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
    write: &mut impl WsMessageSink,
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
    let Some(message) = TriggerMessage_V2_X::parse(
      &request.requested_message,
      self.config.protocol,
    ) else {
      return self
        .send_status_response(write, message_id, ResponseStatus::NotImplemented)
        .await;
    };
    match self.trigger_message_v2_x(message, request.connector) {
      Ok(status) => self.send_status_response(write, message_id, status).await,
      Err(error) => {
        self
          .send_formation_violation(write, message_id, &error.to_string())
          .await
      }
    }
  }

  async fn handle_unknown_incoming_action_v2_x(
    &mut self,
    write: &mut impl WsMessageSink,
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
