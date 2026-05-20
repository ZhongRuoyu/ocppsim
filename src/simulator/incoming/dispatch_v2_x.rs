use super::super::payloads::*;
use super::super::*;
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
    match IncomingAction_V2_X::parse(action) {
      Some(IncomingAction_V2_X::ChangeAvailability) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.change_availability_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::ClearCache) => {
        self
          .send_call_result(
            write,
            message_id,
            to_value(&StatusPayload {
              status: ResponseStatus::Accepted.as_str(),
            }),
          )
          .await?;
      }
      Some(IncomingAction_V2_X::DataTransfer) => {
        let response = self.data_transfer_v2_x(&payload);
        self.send_call_result(write, message_id, response).await?;
      }
      Some(IncomingAction_V2_X::GetLocalListVersion) => {
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
      Some(IncomingAction_V2_X::GetLog) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.get_log_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::GetVariables) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.get_variables_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::RequestStartTransaction) => {
        let request = match RequestStartTransactionRequest_V2_X::parse(&payload)
        {
          Ok(value) => value,
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
            return Ok(());
          }
        };
        let connector = match request.connector {
          Some(value) => value,
          None => match self.first_startable_connector() {
            Some(value) => value,
            None => {
              self
                .send_call_result(
                  write,
                  message_id,
                  to_value(&StatusPayload {
                    status: ResponseStatus::Rejected.as_str(),
                  }),
                )
                .await?;
              return Ok(());
            }
          },
        };

        if let Some(existing) = self.active_transaction_uid(connector) {
          self
            .send_call_result(
              write,
              message_id,
              to_value(&RequestStartTransactionResponse {
                status: ResponseStatus::Accepted.as_str(),
                transaction_id: Some(&existing),
              }),
            )
            .await?;
        } else {
          let result = self.start_transaction(
            connector,
            request.id_token,
            true,
            Some(request.remote_start_id),
            true,
          );
          let status = if result.is_ok() {
            ResponseStatus::Accepted
          } else {
            ResponseStatus::Rejected
          };
          self
            .send_call_result(
              write,
              message_id,
              to_value(&StatusPayload {
                status: status.as_str(),
              }),
            )
            .await?;
        }
      }
      Some(IncomingAction_V2_X::RequestStopTransaction) => {
        let request = match RequestStopTransactionRequest_V2_X::parse(&payload)
        {
          Ok(value) => value,
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
            return Ok(());
          }
        };
        if let Some(connector) =
          self.find_transaction_by_uid(&request.transaction_id)
        {
          let result = self.stop_transaction(connector, None, true, true);
          let status = if result.is_ok() {
            ResponseStatus::Accepted
          } else {
            ResponseStatus::Rejected
          };
          self
            .send_call_result(
              write,
              message_id,
              to_value(&StatusPayload {
                status: status.as_str(),
              }),
            )
            .await?;
        } else {
          self
            .send_call_result(
              write,
              message_id,
              to_value(&StatusPayload {
                status: ResponseStatus::Rejected.as_str(),
              }),
            )
            .await?;
        }
      }
      Some(IncomingAction_V2_X::ReserveNow) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.reserve_now_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::CancelReservation) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.cancel_reservation_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::SendLocalList) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.send_local_list_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::SetChargingProfile) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.set_charging_profile_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::SetVariables) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.set_variables_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::ClearChargingProfile) => {
        let status = self.clear_charging_profile_v2_x(&payload);
        self
          .send_call_result(
            write,
            message_id,
            to_value(&StatusPayload {
              status: status.as_str(),
            }),
          )
          .await?;
      }
      Some(IncomingAction_V2_X::GetCompositeSchedule) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.get_composite_schedule_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::TriggerMessage) => {
        let request = match TriggerMessageRequest_V2_X::parse(&payload) {
          Ok(value) => value,
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
            return Ok(());
          }
        };
        let parsed_message =
          TriggerMessage_V2_X::parse(&request.requested_message);
        match parsed_message {
          Some(message) => {
            match self.trigger_message_v2_x(message, request.connector) {
              Ok(()) => {
                self
                  .send_call_result(
                    write,
                    message_id,
                    to_value(&StatusPayload {
                      status: ResponseStatus::Accepted.as_str(),
                    }),
                  )
                  .await?;
              }
              Err(error) => {
                self
                  .send_formation_violation(
                    write,
                    message_id,
                    &error.to_string(),
                  )
                  .await?;
              }
            }
          }
          None => {
            self
              .send_call_result(
                write,
                message_id,
                to_value(&StatusPayload {
                  status: ResponseStatus::NotImplemented.as_str(),
                }),
              )
              .await?;
          }
        }
      }
      Some(IncomingAction_V2_X::UnlockConnector) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.unlock_connector_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::UpdateFirmware) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.update_firmware_v2_x(&payload)
        );
      }
      Some(IncomingAction_V2_X::Reset) => {
        self.log(
          UiLogLevel::Info,
          "Received Reset request. Simulator will acknowledge only.",
        );
        self
          .send_call_result(
            write,
            message_id,
            to_value(&StatusPayload {
              status: ResponseStatus::Accepted.as_str(),
            }),
          )
          .await?;
      }
      None => {
        if IncomingAction_V2_X::is_known_unsupported(
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
          self
            .send_call_error(
              write,
              message_id,
              OcppErrorCode::NotSupported.as_str(),
              &format!("Action `{action}` is outside the supported subset."),
              json!({}),
            )
            .await?;
        } else {
          self.log(
            UiLogLevel::Warn,
            format!("Action `{action}` is not implemented."),
          );
          self
            .send_call_error(
              write,
              message_id,
              OcppErrorCode::NotImplemented.as_str(),
              &format!("Action `{action}` is not implemented."),
              json!({}),
            )
            .await?;
        }
      }
    }
    Ok(())
  }
}
