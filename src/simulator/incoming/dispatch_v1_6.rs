use super::super::payloads::*;
use super::super::*;
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
    match IncomingAction_V1_6::parse(action) {
      Some(IncomingAction_V1_6::GetConfiguration) => {
        let response = self.configuration_response_v1_6(&payload);
        self
          .send_call_result(write, message_id, to_value(&response))
          .await?;
      }
      Some(IncomingAction_V1_6::ChangeConfiguration) => {
        let status = self.change_configuration_v1_6(&payload);
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
      Some(IncomingAction_V1_6::ClearCache) => {
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
      Some(IncomingAction_V1_6::ChangeAvailability) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.change_availability_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::DataTransfer) => {
        let response = self.data_transfer_v1_6(&payload);
        self.send_call_result(write, message_id, response).await?;
      }
      Some(IncomingAction_V1_6::GetDiagnostics) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.get_diagnostics_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::UpdateFirmware) => {
        match self.update_firmware_v1_6(&payload) {
          Ok(()) => {
            self
              .send_call_result(
                write,
                message_id,
                to_value(&HeartbeatRequest {}),
              )
              .await?;
          }
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
          }
        }
      }
      Some(IncomingAction_V1_6::RemoteStartTransaction) => {
        let request = match RemoteStartTransactionRequestV1_6::parse(&payload) {
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
        let result =
          self.start_transaction(connector, request.id_token, true, None, true);
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
      Some(IncomingAction_V1_6::RemoteStopTransaction) => {
        let request = match RemoteStopTransactionRequestV1_6::parse(&payload) {
          Ok(value) => value,
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
            return Ok(());
          }
        };
        let connector = self.find_v1_6_transaction(request.transaction_id);
        if let Some(connector_id) = connector {
          self.stop_transaction(
            connector_id,
            Some("Remote".to_string()),
            true,
            true,
          )?;
          self
            .send_call_result(
              write,
              message_id,
              to_value(&StatusPayload {
                status: ResponseStatus::Accepted.as_str(),
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
      Some(IncomingAction_V1_6::ReserveNow) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.reserve_now_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::CancelReservation) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.cancel_reservation_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::UnlockConnector) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.unlock_connector_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::GetLocalListVersion) => {
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
      Some(IncomingAction_V1_6::SendLocalList) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.send_local_list_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::SetChargingProfile) => {
        dispatch_status!(
          self,
          write,
          message_id,
          self.set_charging_profile_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::ClearChargingProfile) => {
        let status = self.clear_charging_profile_v1_6(&payload);
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
      Some(IncomingAction_V1_6::GetCompositeSchedule) => {
        dispatch_response!(
          self,
          write,
          message_id,
          self.get_composite_schedule_v1_6(&payload)
        );
      }
      Some(IncomingAction_V1_6::TriggerMessage) => {
        let request = match TriggerMessageRequestV1_6::parse(&payload) {
          Ok(value) => value,
          Err(error) => {
            self
              .send_formation_violation(write, message_id, &error.to_string())
              .await?;
            return Ok(());
          }
        };
        let parsed_message =
          TriggerMessage_V1_6::parse(&request.requested_message);
        match parsed_message {
          Some(message) => {
            match self.trigger_message_v1_6(message, request.connector) {
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
                  status: "NotImplemented",
                }),
              )
              .await?;
          }
        }
      }
      Some(IncomingAction_V1_6::Reset) => {
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
        if IncomingAction_V1_6::is_known_unsupported(action) {
          self.log(
            UiLogLevel::Warn,
            format!(
              "Action `{action}` is known for OCPP 1.6 but is not supported."
            ),
          );
          self
            .send_call_error(
              write,
              message_id,
              "NotSupported",
              &format!(
                "Action `{action}` is outside the supported base schemas."
              ),
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
              "NotImplemented",
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
