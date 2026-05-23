use serde_json::{Value, json};

use super::*;

type SchemaCase = (&'static str, Value);

#[test]
fn supported_v1_6_inbound_responses_validate_against_schemas() {
  let mut simulator = simulator_for_tests();
  let mut cases = v1_6_static_inbound_response_cases();
  cases.extend(v1_6_dynamic_inbound_response_cases(&mut simulator));
  assert_response_cases("schemas/1.6", cases);
}

#[test]
fn supported_v2_0_1_inbound_responses_validate_against_schemas() {
  assert_supported_v2_x_inbound_responses_validate(
    simulator_for_tests_v2_0_1(),
    "schemas/2.0.1",
  );
}

#[test]
fn supported_v2_1_inbound_responses_validate_against_schemas() {
  assert_supported_v2_x_inbound_responses_validate(
    simulator_for_tests_v2_1(),
    "schemas/2.1",
  );
}

fn assert_supported_v2_x_inbound_responses_validate(
  mut simulator: Simulator,
  schema_dir: &str,
) {
  let mut cases = v2_x_static_inbound_response_cases();
  cases.extend(v2_x_dynamic_inbound_response_cases(&mut simulator));
  assert_response_cases(schema_dir, cases);
}

fn assert_response_cases(schema_dir: &str, cases: Vec<SchemaCase>) {
  for (file_name, payload) in cases {
    assert_schema_valid(&schema_path(schema_dir, file_name), &payload);
  }
}

fn accepted_status_case(file_name: &'static str) -> SchemaCase {
  status_case(file_name, ResponseStatus::Accepted)
}

fn status_case(file_name: &'static str, status: ResponseStatus) -> SchemaCase {
  (
    file_name,
    json!({
      "status": status.as_str()
    }),
  )
}

fn v1_6_static_inbound_response_cases() -> Vec<SchemaCase> {
  vec![
    accepted_status_case("ChangeAvailabilityResponse.json"),
    accepted_status_case("ChangeConfigurationResponse.json"),
    accepted_status_case("ClearCacheResponse.json"),
    accepted_status_case("ClearChargingProfileResponse.json"),
    accepted_status_case("RemoteStartTransactionResponse.json"),
    accepted_status_case("RemoteStopTransactionResponse.json"),
    accepted_status_case("ReserveNowResponse.json"),
    accepted_status_case("CancelReservationResponse.json"),
    accepted_status_case("ResetResponse.json"),
    accepted_status_case("SendLocalListResponse.json"),
    accepted_status_case("SetChargingProfileResponse.json"),
    accepted_status_case("TriggerMessageResponse.json"),
    ("UpdateFirmwareResponse.json", json!({})),
  ]
}

fn v1_6_dynamic_inbound_response_cases(
  simulator: &mut Simulator,
) -> Vec<SchemaCase> {
  let configuration_response = crate::simulator::payloads::to_value(
    &simulator
      .configuration_response_v1_6(&json!({ "key": ["HeartbeatInterval"] })),
  );
  let diagnostics_response = simulator
    .get_diagnostics_v1_6(&json!({
      "location": "https://csms.example/logs"
    }))
    .expect("diagnostics response");
  let unlock_status = simulator
    .unlock_connector_v1_6(&json!({ "connectorId": 1 }))
    .expect("unlock response");

  vec![
    (
      "DataTransferResponse.json",
      Simulator::data_transfer_v1_6(&json!({
        "vendorId": "ocppsim",
        "data": "hello"
      })),
    ),
    (
      "GetCompositeScheduleResponse.json",
      simulator
        .get_composite_schedule_v1_6(&json!({
          "connectorId": 1,
          "duration": 60
        }))
        .expect("composite schedule response"),
    ),
    ("GetConfigurationResponse.json", configuration_response),
    ("GetDiagnosticsResponse.json", diagnostics_response),
    (
      "GetLocalListVersionResponse.json",
      json!({
        "listVersion": simulator.local_auth_list_version
      }),
    ),
    status_case("UnlockConnectorResponse.json", unlock_status),
  ]
}

fn v2_x_static_inbound_response_cases() -> Vec<SchemaCase> {
  vec![
    accepted_status_case("ChangeAvailabilityResponse.json"),
    accepted_status_case("ClearCacheResponse.json"),
    accepted_status_case("ClearChargingProfileResponse.json"),
    accepted_status_case("RequestStartTransactionResponse.json"),
    accepted_status_case("RequestStopTransactionResponse.json"),
    accepted_status_case("ReserveNowResponse.json"),
    accepted_status_case("CancelReservationResponse.json"),
    accepted_status_case("ResetResponse.json"),
    accepted_status_case("SendLocalListResponse.json"),
    accepted_status_case("SetChargingProfileResponse.json"),
    accepted_status_case("TriggerMessageResponse.json"),
    accepted_status_case("UpdateFirmwareResponse.json"),
  ]
}

fn v2_x_dynamic_inbound_response_cases(
  simulator: &mut Simulator,
) -> Vec<SchemaCase> {
  let get_log_response = simulator
    .get_log_v2_x(&json!({
      "requestId": 1,
      "log": {
        "remoteLocation": "https://csms.example/logs"
      }
    }))
    .expect("get log response");
  let get_variables_response = simulator
    .get_variables_v2_x(&json!({
      "getVariableData": [
        get_variable_data("HeartbeatInterval")
      ]
    }))
    .expect("get variables response");
  let set_variables_response = simulator
    .set_variables_v2_x(&json!({
      "setVariableData": [
        set_variable_data("HeartbeatInterval", "15")
      ]
    }))
    .expect("set variables response");
  let unlock_status = simulator
    .unlock_connector_v2_x(&json!({
      "evseId": 1,
      "connectorId": 1
    }))
    .expect("unlock response");

  vec![
    (
      "DataTransferResponse.json",
      Simulator::data_transfer_v2_x(&json!({
        "vendorId": "ocppsim",
        "data": "hello"
      })),
    ),
    (
      "GetCompositeScheduleResponse.json",
      simulator
        .get_composite_schedule_v2_x(&json!({
          "evseId": 1,
          "duration": 60
        }))
        .expect("composite schedule response"),
    ),
    (
      "GetLocalListVersionResponse.json",
      json!({
        "versionNumber": simulator.local_auth_list_version
      }),
    ),
    ("GetLogResponse.json", get_log_response),
    ("GetVariablesResponse.json", get_variables_response),
    ("SetVariablesResponse.json", set_variables_response),
    status_case("UnlockConnectorResponse.json", unlock_status),
  ]
}

#[test]
fn representative_v1_6_payloads_validate_against_schemas() {
  let mut simulator = simulator_for_tests();

  simulator.enqueue_boot_notification();
  assert_schema_valid(
    "schemas/1.6/BootNotification.json",
    &queued_payload(&simulator, "BootNotification"),
  );

  simulator.queue.clear();
  simulator.enqueue_authorize("TOKEN".to_string());
  assert_schema_valid(
    "schemas/1.6/Authorize.json",
    &queued_payload(&simulator, "Authorize"),
  );

  simulator.queue.clear();
  simulator.enqueue_heartbeat();
  assert_schema_valid(
    "schemas/1.6/Heartbeat.json",
    &queued_payload(&simulator, "Heartbeat"),
  );

  simulator.enqueue_data_transfer("ocppsim", Some("Message"), Some("hello"));
  assert_schema_valid(
    "schemas/1.6/DataTransfer.json",
    &queued_payload(&simulator, "DataTransfer"),
  );

  simulator.queue.clear();
  simulator
    .enqueue_status_notification(1)
    .expect("status notification");
  assert_schema_valid(
    "schemas/1.6/StatusNotification.json",
    &queued_payload(&simulator, "StatusNotification"),
  );

  simulator.queue.clear();
  simulator.enqueue_meter_values(1).expect("meter values");
  assert_schema_valid(
    "schemas/1.6/MeterValues.json",
    &queued_payload(&simulator, "MeterValues"),
  );

  simulator.queue.clear();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  assert_schema_valid(
    "schemas/1.6/StartTransaction.json",
    &queued_payload(&simulator, "StartTransaction"),
  );

  simulator.queue.clear();
  simulator
    .stop_transaction(1, Some("Local"), false, true)
    .expect("stop should succeed");
  assert_schema_valid(
    "schemas/1.6/StopTransaction.json",
    &queued_payload(&simulator, "StopTransaction"),
  );

  simulator.queue.clear();
  let diagnostics_response = simulator
    .get_diagnostics_v1_6(&json!({
      "location": "https://csms.example/logs"
    }))
    .expect("diagnostics response");
  assert_schema_valid(
    "schemas/1.6/GetDiagnosticsResponse.json",
    &diagnostics_response,
  );
  assert_schema_valid(
    "schemas/1.6/DiagnosticsStatusNotification.json",
    &queued_payload(&simulator, "DiagnosticsStatusNotification"),
  );

  simulator.queue.clear();
  simulator
    .update_firmware_v1_6(&json!({
      "location":
        "https://csms.example/firmware.bin",
      "retrieveDate": now_timestamp()
    }))
    .expect("firmware update");
  assert_schema_valid(
    "schemas/1.6/FirmwareStatusNotification.json",
    &queued_payload(&simulator, "FirmwareStatusNotification"),
  );
}

#[test]
fn representative_v2_0_1_payloads_validate_against_schemas() {
  let simulator = simulator_for_tests_v2_0_1();
  assert_representative_v2_x_payloads_validate(simulator, "schemas/2.0.1");
}

#[test]
fn representative_v2_1_payloads_validate_against_schemas() {
  let simulator = simulator_for_tests_v2_1();
  assert_representative_v2_x_payloads_validate(simulator, "schemas/2.1");
}

fn assert_representative_v2_x_payloads_validate(
  mut simulator: Simulator,
  schema_dir: &str,
) {
  simulator.enqueue_boot_notification();
  assert_schema_valid(
    &schema_path(schema_dir, "BootNotificationRequest.json"),
    &queued_payload(&simulator, "BootNotification"),
  );

  simulator.queue.clear();
  simulator.enqueue_authorize("TOKEN".to_string());
  assert_schema_valid(
    &schema_path(schema_dir, "AuthorizeRequest.json"),
    &queued_payload(&simulator, "Authorize"),
  );

  simulator.queue.clear();
  simulator.enqueue_heartbeat();
  assert_schema_valid(
    &schema_path(schema_dir, "HeartbeatRequest.json"),
    &queued_payload(&simulator, "Heartbeat"),
  );

  simulator.queue.clear();
  simulator.enqueue_data_transfer("ocppsim", Some("Message"), Some("hello"));
  assert_schema_valid(
    &schema_path(schema_dir, "DataTransferRequest.json"),
    &queued_payload(&simulator, "DataTransfer"),
  );

  simulator.queue.clear();
  simulator
    .enqueue_status_notification(1)
    .expect("status notification");
  assert_schema_valid(
    &schema_path(schema_dir, "StatusNotificationRequest.json"),
    &queued_payload(&simulator, "StatusNotification"),
  );

  simulator.queue.clear();
  simulator.enqueue_meter_values(1).expect("meter values");
  assert_schema_valid(
    &schema_path(schema_dir, "MeterValuesRequest.json"),
    &queued_payload(&simulator, "MeterValues"),
  );

  simulator.queue.clear();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  assert_schema_valid(
    &schema_path(schema_dir, "TransactionEventRequest.json"),
    &queued_payload(&simulator, "TransactionEvent"),
  );

  simulator.queue.clear();
  simulator
    .get_log_v2_x(&json!({
      "requestId": 1,
      "log": {
        "remoteLocation": "https://csms.example/logs"
      }
    }))
    .expect("get log response");
  assert_schema_valid(
    &schema_path(schema_dir, "LogStatusNotificationRequest.json"),
    &queued_payload(&simulator, "LogStatusNotification"),
  );

  simulator.queue.clear();
  simulator
    .update_firmware_v2_x(&json!({
      "requestId": 2,
      "firmware": {
        "location": "https://csms.example/firmware.bin",
        "retrieveDateTime": now_timestamp()
      }
    }))
    .expect("firmware update");
  assert_schema_valid(
    &schema_path(schema_dir, "FirmwareStatusNotificationRequest.json"),
    &queued_payload(&simulator, "FirmwareStatusNotification"),
  );
}

#[test]
fn get_composite_schedule_v2_0_1_validates_against_schema() {
  let simulator = simulator_for_tests_v2_0_1();
  let response = simulator
    .get_composite_schedule_v2_x(&json!({
      "evseId": 1,
      "duration": 60
    }))
    .expect("composite schedule response");

  assert_schema_valid(
    "schemas/2.0.1/GetCompositeScheduleResponse.json",
    &response,
  );
}

#[test]
fn get_composite_schedule_v2_1_validates_against_schema() {
  let simulator = simulator_for_tests_v2_1();
  let response = simulator
    .get_composite_schedule_v2_x(&json!({
      "evseId": 1,
      "duration": 60
    }))
    .expect("composite schedule response");

  assert_schema_valid(
    "schemas/2.1/GetCompositeScheduleResponse.json",
    &response,
  );
}

#[test]
fn transaction_event_update_and_end_v2_1_validate_against_schema() {
  let mut simulator = simulator_for_tests_v2_1();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  simulator.queue.clear();

  simulator.set_meter(1, 1200).expect("set meter");
  simulator
    .send_meter(1, true)
    .expect("meter update should enqueue");
  let update_payload = queued_payload(&simulator, "TransactionEvent");
  assert_eq!(update_payload["eventType"], json!("Updated"));
  assert_schema_valid(
    "schemas/2.1/TransactionEventRequest.json",
    &update_payload,
  );

  simulator.queue.clear();
  simulator
    .stop_transaction(1, Some("Local"), false, true)
    .expect("stop should enqueue");
  let end_payload = queued_payload(&simulator, "TransactionEvent");
  assert_eq!(end_payload["eventType"], json!("Ended"));
  assert_schema_valid("schemas/2.1/TransactionEventRequest.json", &end_payload);
}
