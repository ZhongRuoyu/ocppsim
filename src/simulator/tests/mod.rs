use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{
  BootRegistrationStatus, ConfigurationKey, ConnectorStatus, OcppVersion,
  PendingCall, PendingContext, ResponseStatus, SecurityProfileFallback,
  Simulator, SimulatorCommand, SimulatorConfig, SimulatorConnectionConfig,
  TxEventType, UiEvent, UiLogLevel, now_timestamp, sanitized_trace_frame_text,
  validate_negotiated_subprotocol,
};

mod charging_profiles;
mod configuration;
mod schema_validation;
mod security;
mod state;
mod transactions;
mod ws_lifecycle;

fn simulator_test_config(protocol: OcppVersion) -> SimulatorConfig {
  SimulatorConfig {
    profile: None,
    ws_url: Some("ws://localhost:9000/ocpp".to_string()),
    cp_id: Some("CP-TEST".to_string()),
    protocol,
    connectors: 2,
    vendor: "ocppsim".to_string(),
    model: "test".to_string(),
    firmware: "0.0.0".to_string(),
    append_cp_id: false,
    trace_frames: false,
    strict: false,
    security_profile: None,
    basic_auth_password: None,
    ca_cert_path: None,
    client_cert_path: None,
    client_key_path: None,
    request_timeout: Duration::from_secs(30),
    heartbeat_seconds: Some(10),
    outbound_queue_limit: 1_000,
    security_event_limit: 1_000,
  }
}

fn simulator_for_tests_with_protocol(protocol: OcppVersion) -> Simulator {
  let (simulator, _ui_rx) = simulator_for_tests_with_protocol_and_ui(protocol);
  simulator
}

fn simulator_for_tests_with_protocol_and_ui(
  protocol: OcppVersion,
) -> (Simulator, mpsc::UnboundedReceiver<UiEvent>) {
  let config = simulator_test_config(protocol);
  let (ui_tx, ui_rx) = mpsc::unbounded_channel();
  let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
  (Simulator::new(config, ui_tx, cmd_tx), ui_rx)
}

fn simulator_for_tests() -> Simulator {
  simulator_for_tests_with_protocol(OcppVersion::V1_6)
}

fn drain_log_messages(
  ui_rx: &mut mpsc::UnboundedReceiver<UiEvent>,
) -> Vec<String> {
  let mut messages = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    if let UiEvent::Log { message, .. } = event {
      messages.push(message);
    }
  }
  messages
}

fn v2_x_protocols() -> [OcppVersion; 2] {
  [OcppVersion::V2_0_1, OcppVersion::V2_1]
}

fn for_each_v2_x_simulator(
  mut assert_case: impl FnMut(OcppVersion, Simulator),
) {
  for protocol in v2_x_protocols() {
    assert_case(protocol, simulator_for_tests_with_protocol(protocol));
  }
}

fn v2_x_schema_dir(protocol: OcppVersion) -> &'static str {
  match protocol {
    OcppVersion::V2_0_1 => "schemas/2.0.1",
    OcppVersion::V2_1 => "schemas/2.1",
    OcppVersion::V1_6 => panic!("expected v2.x protocol"),
  }
}

fn assert_schema_valid(relative_schema: &str, payload: &Value) {
  let schema_text = crate::embedded_schemas::schema_text(relative_schema)
    .unwrap_or_else(|| panic!("missing embedded schema {relative_schema}"));
  let schema: Value = serde_json::from_str(schema_text).expect("schema");
  let validator = jsonschema::validator_for(&schema).expect("compile schema");
  let errors = validator
    .iter_errors(payload)
    .map(|error| error.to_string())
    .collect::<Vec<_>>();
  assert!(
    errors.is_empty(),
    "payload did not match {}: {}\npayload={}",
    relative_schema,
    errors.join("; "),
    payload
  );
}

fn queued_payload(simulator: &Simulator, action: &str) -> Value {
  simulator
    .queue
    .iter()
    .find(|call| call.action == action)
    .map(|call| call.payload.clone())
    .expect("queued action")
}

fn schema_path(schema_dir: &str, file_name: &str) -> String {
  format!("{schema_dir}/{file_name}")
}

fn get_variable_data(variable: &str) -> Value {
  json!({
    "component": { "name": "ChargingStation" },
    "variable": { "name": variable },
  })
}

fn set_variable_data(variable: &str, value: &str) -> Value {
  json!({
    "component": { "name": "ChargingStation" },
    "variable": { "name": variable },
    "attributeValue": value,
  })
}
