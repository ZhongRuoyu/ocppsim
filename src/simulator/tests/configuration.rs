use serde_json::json;

use super::*;

#[test]
fn get_configuration_reports_unknown_keys() {
  let simulator = simulator_for_tests();
  let payload = json!({ "key": ["HeartbeatInterval", "MissingKey"] });
  let response = simulator.configuration_response_v1_6(&payload);

  assert_eq!(response.configuration_key.len(), 1);
  assert_eq!(response.unknown_key, vec!["MissingKey".to_string()]);
}

#[test]
fn change_configuration_updates_writable_key() {
  let mut simulator = simulator_for_tests();
  let payload = json!({
    "key": "HeartbeatInterval",
    "value": "25"
  });
  let status = simulator
    .change_configuration_v1_6(&payload)
    .expect("change configuration should parse");

  assert_eq!(status, ResponseStatus::Accepted);
  let entry = simulator
    .configuration
    .get(&ConfigurationKey::HeartbeatInterval)
    .expect("key must exist");
  assert_eq!(entry.value, "25");
}

#[test]
fn change_configuration_rejects_read_only_key() {
  let mut simulator = simulator_for_tests();
  let payload = json!({
    "key": "NumberOfConnectors",
    "value": "8"
  });
  let status = simulator
    .change_configuration_v1_6(&payload)
    .expect("change configuration should parse");

  assert_eq!(status, ResponseStatus::Rejected);
}

#[test]
fn change_configuration_rejects_missing_value_without_mutating() {
  let mut simulator = simulator_for_tests();
  let original = simulator
    .configuration
    .get(&ConfigurationKey::MeterValueSampleInterval)
    .expect("key must exist")
    .value
    .clone();

  let error = simulator
    .change_configuration_v1_6(&json!({
      "key": "MeterValueSampleInterval"
    }))
    .expect_err("missing value should fail");

  assert!(error.to_string().contains("value is required"));
  assert_eq!(
    simulator
      .configuration
      .get(&ConfigurationKey::MeterValueSampleInterval)
      .expect("key must exist")
      .value,
    original
  );
}

#[test]
fn change_configuration_rejects_non_string_value_without_mutating() {
  let mut simulator = simulator_for_tests();
  let original = simulator
    .configuration
    .get(&ConfigurationKey::MeterValueSampleInterval)
    .expect("key must exist")
    .value
    .clone();

  let error = simulator
    .change_configuration_v1_6(&json!({
      "key": "MeterValueSampleInterval",
      "value": 30
    }))
    .expect_err("non-string value should fail");

  assert!(error.to_string().contains("value is required"));
  assert_eq!(
    simulator
      .configuration
      .get(&ConfigurationKey::MeterValueSampleInterval)
      .expect("key must exist")
      .value,
    original
  );
}

#[test]
fn get_variables_v2_x_reads_configuration() {
  for_each_v2_x_simulator(|protocol, simulator| {
    let schema =
      schema_path(v2_x_schema_dir(protocol), "GetVariablesResponse.json");
    assert_get_variables_reads_configuration(&simulator, &schema);
  });
}

fn assert_get_variables_reads_configuration(
  simulator: &Simulator,
  schema: &str,
) {
  let response = simulator
    .get_variables_v2_x(&json!({
      "getVariableData": [
        get_variable_data("HeartbeatInterval"),
        get_variable_data("MissingKey"),
      ]
    }))
    .expect("get variables response");

  assert_eq!(
    response["getVariableResult"][0]["attributeStatus"],
    "Accepted"
  );
  assert_eq!(response["getVariableResult"][0]["attributeValue"], "10");
  assert_eq!(
    response["getVariableResult"][1]["attributeStatus"],
    "UnknownVariable"
  );
  assert_schema_valid(schema, &response);
}

#[test]
fn get_variables_v2_x_reports_component_and_attribute_errors() {
  for_each_v2_x_simulator(|protocol, simulator| {
    let response = simulator
      .get_variables_v2_x(&json!({
        "getVariableData": [
          {
            "component": { "name": "Connector" },
            "variable": { "name": "HeartbeatInterval" }
          },
          {
            "component": { "name": "ChargingStation" },
            "variable": { "name": "HeartbeatInterval" },
            "attributeType": "MinSet"
          }
        ]
      }))
      .expect("get variables response");

    assert_eq!(
      response["getVariableResult"][0]["attributeStatus"],
      "UnknownComponent"
    );
    assert_eq!(
      response["getVariableResult"][1]["attributeStatus"],
      "NotSupportedAttributeType"
    );
    assert_schema_valid(
      &schema_path(v2_x_schema_dir(protocol), "GetVariablesResponse.json"),
      &response,
    );
  });
}

#[test]
fn set_variables_v2_x_updates_configuration() {
  for_each_v2_x_simulator(|protocol, simulator| {
    let schema =
      schema_path(v2_x_schema_dir(protocol), "SetVariablesResponse.json");
    assert_set_variables_updates_configuration(simulator, &schema);
  });
}

fn assert_set_variables_updates_configuration(
  mut simulator: Simulator,
  schema: &str,
) {
  let response = simulator
    .set_variables_v2_x(&json!({
      "setVariableData": [
        set_variable_data("HeartbeatInterval", "20"),
        set_variable_data("NumberOfConnectors", "8"),
      ]
    }))
    .expect("set variables response");

  assert_eq!(
    response["setVariableResult"][0]["attributeStatus"],
    "Accepted"
  );
  assert_eq!(
    response["setVariableResult"][1]["attributeStatus"],
    "Rejected"
  );
  assert_eq!(
    simulator
      .configuration
      .get(&ConfigurationKey::HeartbeatInterval)
      .map(|entry| entry.value.as_str()),
    Some("20")
  );
  assert_schema_valid(schema, &response);
}

#[test]
fn data_transfer_v1_6_rejects_missing_vendor() {
  let error = Simulator::data_transfer_v1_6(&json!({}))
    .expect_err("missing vendorId should fail");
  assert!(error.to_string().contains("vendorId is required"));
}

#[test]
fn data_transfer_v2_x_rejects_missing_vendor() {
  for _protocol in v2_x_protocols() {
    let error = Simulator::data_transfer_v2_x(&json!({}))
      .expect_err("missing vendorId should fail");
    assert!(error.to_string().contains("vendorId is required"));
  }
}

#[tokio::test]
async fn set_variables_v2_x_restarts_active_heartbeat() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator.start_heartbeat(10);

    let response = simulator
      .set_variables_v2_x(&json!({
        "setVariableData": [
          set_variable_data("HeartbeatInterval", "22")
        ]
      }))
      .expect("set variables response");

    assert_eq!(
      response["setVariableResult"][0]["attributeStatus"],
      ResponseStatus::Accepted.as_str()
    );
    assert_eq!(
      simulator.heartbeat.as_ref().map(|item| item.seconds),
      Some(22)
    );
    simulator.stop_heartbeat();
  });
}
