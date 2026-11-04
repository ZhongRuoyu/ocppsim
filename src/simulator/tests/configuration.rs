use serde_json::json;

use super::*;

#[tokio::test]
async fn heartbeat_commands_keep_the_interval_and_origin_in_sync() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for running in [false, true] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      assert_eq!(
        simulator.set_configuration_value(
          ConfigurationKey::HeartbeatInterval,
          "17",
          ConfigurationOrigin::Csms,
        ),
        ResponseStatus::Accepted
      );
      if running {
        simulator.start_heartbeat(17);
      }
      simulator
        .handle_common_command(
          SimulatorCommand::StartHeartbeat { seconds: 20 },
          false,
        )
        .unwrap();
      assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 20);
      let entry =
        &simulator.configuration[&ConfigurationKey::HeartbeatInterval];
      assert_eq!(entry.value, "20");
      assert_eq!(entry.origin, ConfigurationOrigin::Local);
      let expected = entry.clone();
      simulator
        .handle_common_command(SimulatorCommand::StopHeartbeat, false)
        .unwrap();
      assert!(simulator.heartbeat.is_none());
      assert_eq!(
        simulator.configuration[&ConfigurationKey::HeartbeatInterval],
        expected
      );
      assert_eq!(
        simulator.set_configuration_value(
          ConfigurationKey::HeartbeatInterval,
          "25",
          ConfigurationOrigin::Local,
        ),
        ResponseStatus::Accepted
      );
      assert!(simulator.heartbeat.is_none());
      assert!(simulator.queue.is_empty());
    }
  }
}

#[tokio::test]
async fn invalid_heartbeat_commands_preserve_the_running_timer() {
  let mut simulator = simulator_for_tests();
  simulator.start_heartbeat(10);
  let original = simulator.configuration.clone();
  for seconds in [0, 2_147_483_648, u64::MAX] {
    assert!(
      simulator
        .handle_common_command(
          SimulatorCommand::StartHeartbeat { seconds },
          false,
        )
        .is_err()
    );
    assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 10);
    assert_eq!(simulator.configuration, original);
    simulator.start_heartbeat(seconds);
    assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 10);
  }
  simulator.stop_heartbeat();
}

#[test]
fn initial_configuration_distinguishes_defaults_and_local_settings() {
  let mut config = simulator_test_config(OcppVersion::V1_6);
  config.heartbeat_seconds = None;
  let entries = crate::simulator::default_configuration_entries(&config);
  for key in [
    ConfigurationKey::AuthorizeRemoteTxRequests,
    ConfigurationKey::HeartbeatInterval,
    ConfigurationKey::SecurityProfile,
    ConfigurationKey::AuthorizationKey,
    ConfigurationKey::BasicAuthPassword,
  ] {
    assert_eq!(entries[&key].origin, ConfigurationOrigin::Default);
  }
  for key in [
    ConfigurationKey::NumberOfConnectors,
    ConfigurationKey::CpoName,
    ConfigurationKey::OrganizationName,
  ] {
    assert_eq!(entries[&key].origin, ConfigurationOrigin::Local);
  }

  config.heartbeat_seconds = Some(30);
  config.security_profile = Some(1);
  config.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());
  let entries = crate::simulator::default_configuration_entries(&config);
  for key in [
    ConfigurationKey::HeartbeatInterval,
    ConfigurationKey::SecurityProfile,
    ConfigurationKey::AuthorizationKey,
    ConfigurationKey::BasicAuthPassword,
  ] {
    assert_eq!(entries[&key].origin, ConfigurationOrigin::Local);
  }
  assert_eq!(entries[&ConfigurationKey::AuthorizationKey].value, "");
  assert_eq!(entries[&ConfigurationKey::BasicAuthPassword].value, "");
}

#[test]
fn accepted_configuration_write_records_origin_even_if_value_is_unchanged() {
  let mut simulator = simulator_for_tests();
  let key = ConfigurationKey::AuthorizeRemoteTxRequests;
  assert_eq!(
    simulator.configuration[&key].origin,
    ConfigurationOrigin::Default
  );
  assert_eq!(
    simulator
      .change_configuration_v1_6(&json!({
        "key": key.as_str(), "value": "true"
      }))
      .expect("change configuration"),
    ResponseStatus::Accepted
  );
  assert_eq!(simulator.configuration[&key].value, "true");
  assert_eq!(
    simulator.configuration[&key].origin,
    ConfigurationOrigin::Csms
  );
}

#[test]
fn rejected_configuration_writes_preserve_values_and_origins() {
  let mut simulator = simulator_for_tests();
  let original = simulator.configuration.clone();
  for (key, value, status) in [
    ("HeartbeatInterval", "0", ResponseStatus::Rejected),
    ("NumberOfConnectors", "8", ResponseStatus::Rejected),
    ("AuthorizationKey", "invalid", ResponseStatus::Rejected),
    ("SecurityProfile", "9", ResponseStatus::Rejected),
    ("MissingKey", "true", ResponseStatus::NotSupported),
  ] {
    assert_eq!(
      simulator
        .change_configuration_v1_6(&json!({"key": key, "value": value}))
        .expect("change configuration"),
      status
    );
    assert_eq!(simulator.configuration, original);
  }
  assert!(
    simulator
      .change_configuration_v1_6(&json!({"key": "CpoName", "value": ""}))
      .is_err()
  );
  assert_eq!(simulator.configuration, original);
}

#[test]
fn get_configuration_empty_keys_returns_all_settings() {
  let simulator = simulator_for_tests();
  let all_settings =
    serde_json::to_value(simulator.configuration_response_v1_6(&json!({})))
      .expect("serialize all settings");
  assert_eq!(
    all_settings["configurationKey"].as_array().unwrap().len(),
    simulator.configuration.len()
  );

  for request in [json!({}), json!({ "key": [] })] {
    assert_schema_valid("schemas/1.6/GetConfiguration.json", &request);
    let response =
      serde_json::to_value(simulator.configuration_response_v1_6(&request))
        .expect("serialize configuration response");
    assert_schema_valid("schemas/1.6/GetConfigurationResponse.json", &response);
    assert_eq!(response, all_settings);
  }
}

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
  assert_eq!(entry.origin, ConfigurationOrigin::Csms);
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
        set_variable_data("AuthorizeRemoteTxRequests", "true"),
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
  for key in [
    ConfigurationKey::HeartbeatInterval,
    ConfigurationKey::AuthorizeRemoteTxRequests,
  ] {
    assert_eq!(
      simulator.configuration[&key].origin,
      ConfigurationOrigin::Csms
    );
  }
  assert_eq!(
    simulator.configuration[&ConfigurationKey::NumberOfConnectors].origin,
    ConfigurationOrigin::Local
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
