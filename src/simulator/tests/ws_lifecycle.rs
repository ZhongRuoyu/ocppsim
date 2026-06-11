use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{DuplexStream, duplex};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::{Message, Role};

use crate::ocpp::{
  OcppFrame, build_call, build_call_error, build_call_result, parse_frame,
};

use super::*;

type TestWsStream = WebSocketStream<DuplexStream>;
type TestWsWrite = SplitSink<TestWsStream, Message>;
type TestWsRead = SplitStream<TestWsStream>;

fn format_violation_code(protocol: OcppVersion) -> &'static str {
  match protocol {
    OcppVersion::V1_6 => "FormationViolation",
    OcppVersion::V2_0_1 | OcppVersion::V2_1 => "FormatViolation",
  }
}

async fn in_memory_ws_pair()
-> (TestWsWrite, TestWsRead, TestWsWrite, TestWsRead) {
  let (client, server) = duplex(64 * 1024);
  let client_ws =
    WebSocketStream::from_raw_socket(client, Role::Client, None).await;
  let server_ws =
    WebSocketStream::from_raw_socket(server, Role::Server, None).await;
  let (client_write, client_read) = client_ws.split();
  let (server_write, server_read) = server_ws.split();
  (client_write, client_read, server_write, server_read)
}

async fn read_ws_text(read: &mut TestWsRead) -> String {
  let message = read
    .next()
    .await
    .expect("response frame")
    .expect("response frame ok");
  message.to_text().expect("text frame").to_string()
}

async fn read_ocpp_frame(read: &mut TestWsRead) -> OcppFrame {
  parse_frame(&read_ws_text(read).await).expect("parse response")
}

async fn read_ocpp_frames(
  read: &mut TestWsRead,
  count: usize,
) -> Vec<OcppFrame> {
  let mut frames = Vec::new();
  for _ in 0..count {
    frames.push(read_ocpp_frame(read).await);
  }
  frames
}

async fn capture_inbound_call_response(
  protocol: OcppVersion,
  action: &str,
  payload: Value,
) -> (OcppFrame, Simulator) {
  let (frame, simulator, _events) =
    capture_inbound_call_response_with_events(protocol, false, action, payload)
      .await;
  (frame, simulator)
}

async fn capture_inbound_call_response_with_strict(
  protocol: OcppVersion,
  strict: bool,
  action: &str,
  payload: Value,
) -> (OcppFrame, Simulator) {
  let (frame, simulator, _events) = capture_inbound_call_response_with_events(
    protocol, strict, action, payload,
  )
  .await;
  (frame, simulator)
}

async fn capture_inbound_call_response_with_events(
  protocol: OcppVersion,
  strict: bool,
  action: &str,
  payload: Value,
) -> (OcppFrame, Simulator, Vec<UiEvent>) {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(protocol);
  simulator.config.strict = strict;
  simulator
    .handle_incoming_call(&mut write, "test-message", action, payload)
    .await
    .expect("handle inbound call");
  drop(write);

  let frame = read_ocpp_frame(&mut server_read).await;
  let mut events = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    events.push(event);
  }
  (frame, simulator, events)
}

async fn capture_ws_text_response_with_events(
  protocol: OcppVersion,
  text: String,
) -> (OcppFrame, Simulator, Vec<UiEvent>) {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(protocol);
  simulator.config.trace_frames = true;
  simulator
    .handle_ws_text(text, &mut write)
    .await
    .expect("handle text");
  drop(write);

  let frame = read_ocpp_frame(&mut server_read).await;
  let mut events = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    events.push(event);
  }
  (frame, simulator, events)
}

async fn capture_ws_text_events(
  protocol: OcppVersion,
  text: String,
) -> Vec<UiEvent> {
  let (mut write, read, _server_write, _server_read) =
    in_memory_ws_pair().await;
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(protocol);
  simulator.config.trace_frames = true;
  simulator
    .handle_ws_text(text, &mut write)
    .await
    .expect("handle text");
  drop(write);
  drop(read);

  let mut events = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    events.push(event);
  }
  events
}

fn assert_trace_redacted(events: &[UiEvent], secret: &str) {
  let log_messages = events
    .iter()
    .filter_map(|event| {
      if let UiEvent::Log { message, .. } = event {
        Some(message.as_str())
      } else {
        None
      }
    })
    .collect::<Vec<_>>();
  assert!(
    log_messages.iter().all(|message| !message.contains(secret)),
    "secret appeared in logs: {log_messages:?}"
  );
  assert!(
    log_messages
      .iter()
      .any(|message| message.contains("<redacted>")),
    "redacted marker missing from logs: {log_messages:?}"
  );
}

fn assert_log_contains(events: &[UiEvent], expected: &str) {
  let log_messages = events
    .iter()
    .filter_map(|event| {
      if let UiEvent::Log { message, .. } = event {
        Some(message.as_str())
      } else {
        None
      }
    })
    .collect::<Vec<_>>();
  assert!(
    log_messages
      .iter()
      .any(|message| message.contains(expected)),
    "expected `{expected}` in logs: {log_messages:?}"
  );
}

#[test]
fn validates_negotiated_subprotocol() {
  assert_eq!(
    validate_negotiated_subprotocol("ocpp1.6", Some("ocpp1.6"))
      .expect("subprotocol should match"),
    "ocpp1.6"
  );
  assert!(validate_negotiated_subprotocol("ocpp1.6", None).is_err());
  assert!(
    validate_negotiated_subprotocol("ocpp1.6", Some("ocpp2.0.1")).is_err()
  );
}

#[tokio::test]
async fn trace_frames_redacts_change_configuration_authorization_key() {
  let password = "0123456789abcdef0123456789abcdef";
  let frame = build_call(
    "secret-message",
    "ChangeConfiguration",
    &json!({
      "key": "AuthorizationKey",
      "value": password
    }),
  );

  let (_response, simulator, events) =
    capture_ws_text_response_with_events(OcppVersion::V1_6, frame).await;

  assert_eq!(
    simulator.security.basic_auth_password.as_deref(),
    Some(password)
  );
  assert_trace_redacted(&events, password);
}

#[tokio::test]
async fn trace_frames_redacts_call_error_details() {
  let password = "0123456789abcdef0123456789abcdef";
  let frame = json!([
    4,
    "secret-message",
    "GenericError",
    "failed",
    { "BasicAuthPassword": password }
  ])
  .to_string();

  let events = capture_ws_text_events(OcppVersion::V1_6, frame).await;

  assert_trace_redacted(&events, password);
  assert_log_contains(&events, "CALLERROR details=");
  assert_log_contains(&events, "\"BasicAuthPassword\":\"<redacted>\"");
}

#[tokio::test]
async fn trace_frames_redacts_call_result_error_details() {
  let password = "0123456789abcdef0123456789abcdef";
  let frame = json!([
    5,
    "secret-message",
    "GenericError",
    "failed",
    { "AuthorizationKey": password }
  ])
  .to_string();

  let events = capture_ws_text_events(OcppVersion::V2_1, frame).await;

  assert_trace_redacted(&events, password);
  assert_log_contains(&events, "CALLRESULTERROR details=");
  assert_log_contains(&events, "\"AuthorizationKey\":\"<redacted>\"");
}

#[tokio::test]
async fn trace_frames_redacts_send_payload() {
  let password = "0123456789abcdef0123456789abcdef";
  let frame = json!([
    6,
    "secret-message",
    "SetVariables",
    {
      "setVariableData": [
        set_variable_data("BasicAuthPassword", password)
      ]
    }
  ])
  .to_string();

  let events = capture_ws_text_events(OcppVersion::V2_1, frame).await;

  assert_trace_redacted(&events, password);
  assert_log_contains(&events, "SEND payload=");
  assert_log_contains(&events, "\"attributeValue\":\"<redacted>\"");
}

#[tokio::test]
async fn older_protocol_rejects_call_result_error_message_type() {
  let frame =
    json!([5, "unsupported-message", "GenericError", "failed", {}]).to_string();

  let (response, _, events) =
    capture_ws_text_response_with_events(OcppVersion::V2_0_1, frame).await;

  let OcppFrame::CallError {
    message_id, code, ..
  } = response
  else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(message_id, "unsupported-message");
  assert_eq!(code, "MessageTypeNotSupported");
  assert_log_contains(&events, "Unsupported message type 5.");
}

#[tokio::test]
async fn older_protocol_rejects_send_message_type() {
  let frame = json!([6, "unsupported-message", "SetVariables", {}]).to_string();

  let (response, _, events) =
    capture_ws_text_response_with_events(OcppVersion::V1_6, frame).await;

  let OcppFrame::CallError {
    message_id, code, ..
  } = response
  else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(message_id, "unsupported-message");
  assert_eq!(code, "MessageTypeNotSupported");
  assert_log_contains(&events, "Unsupported message type 6.");
}

#[tokio::test]
async fn duplicate_inbound_call_ids_return_call_error_without_dispatch() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  let frame = build_call(
    "duplicate-message",
    "RemoteStartTransaction",
    &json!({ "idTag": "TOKEN" }),
  );

  simulator
    .handle_ws_text(frame.clone(), &mut write)
    .await
    .expect("handle first request");

  let OcppFrame::CallResult { payload, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  assert_eq!(simulator.queue.len(), 1);

  simulator
    .handle_ws_text(frame, &mut write)
    .await
    .expect("handle duplicate request");

  let OcppFrame::CallError { code, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "OccurrenceConstraintViolation");
  assert_eq!(simulator.queue.len(), 1);
}

#[tokio::test]
async fn duplicate_inbound_send_ids_are_logged_and_dropped() {
  let (mut write, read, _server_write, _server_read) =
    in_memory_ws_pair().await;
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V2_1);
  let frame = json!([6, "duplicate-send", "NotifyEvent", {}]).to_string();

  simulator
    .handle_ws_text(frame.clone(), &mut write)
    .await
    .expect("handle first send");
  simulator
    .handle_ws_text(frame, &mut write)
    .await
    .expect("handle duplicate send");
  drop(write);
  drop(read);

  assert_eq!(simulator.incoming_message_ids.len(), 1);
  let messages = drain_log_messages(&mut ui_rx);
  assert!(
    messages
      .iter()
      .any(|message| message.contains("Duplicate inbound OCPP messageId")),
    "duplicate SEND log missing: {messages:?}"
  );
}

#[tokio::test]
async fn trace_frames_redacts_set_variables_basic_auth_password() {
  let password = "0123456789abcdef0123456789abcdef";
  let frame = build_call(
    "secret-message",
    "SetVariables",
    &json!({
      "setVariableData": [
        set_variable_data("BasicAuthPassword", password)
      ]
    }),
  );

  let (_response, simulator, events) =
    capture_ws_text_response_with_events(OcppVersion::V2_0_1, frame).await;

  assert_eq!(
    simulator.security.basic_auth_password.as_deref(),
    Some(password)
  );
  assert_trace_redacted(&events, password);
}

#[test]
fn trace_frame_text_redacts_v1_6_transaction_id_tags() {
  let start = build_call(
    "start-message",
    "StartTransaction",
    &json!({
      "connectorId": 1,
      "idTag": "SECRET-TOKEN",
      "meterStart": 0,
      "timestamp": now_timestamp()
    }),
  );
  let stop = build_call(
    "stop-message",
    "StopTransaction",
    &json!({
      "idTag": "SECRET-TOKEN",
      "meterStop": 0,
      "timestamp": now_timestamp(),
      "transactionId": 42
    }),
  );

  for trace in [
    sanitized_trace_frame_text(&start),
    sanitized_trace_frame_text(&stop),
  ] {
    assert!(!trace.contains("SECRET-TOKEN"), "idTag appeared in {trace}");
    assert!(trace.contains("\"idTag\":\"<redacted>\""));
  }
}

#[test]
fn trace_frame_text_redacts_v2_x_id_tokens() {
  let frame = build_call(
    "auth-message",
    "Authorize",
    &json!({
      "idToken": {
        "idToken": "SECRET-TOKEN",
        "type": "ISO14443"
      }
    }),
  );

  let trace = sanitized_trace_frame_text(&frame);

  assert!(
    !trace.contains("SECRET-TOKEN"),
    "idToken appeared in {trace}"
  );
  assert!(trace.contains("\"idToken\":\"<redacted>\""));
  assert!(trace.contains("\"type\":\"ISO14443\""));
}

#[tokio::test]
async fn outbound_trace_logs_redacted_frame_but_sends_real_frame() {
  let (mut write, read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.trace_frames = true;
  simulator
    .start_transaction(1, "SECRET-TOKEN".to_string(), false, None, true)
    .expect("start should enqueue");
  simulator
    .try_send_next(&mut write)
    .await
    .expect("send start");
  drop(write);
  drop(read);

  let wire_frame = read_ws_text(&mut server_read).await;
  assert!(wire_frame.contains("\"idTag\":\"SECRET-TOKEN\""));

  let messages = drain_log_messages(&mut ui_rx);
  assert!(
    messages
      .iter()
      .all(|message| !message.contains("SECRET-TOKEN")),
    "ID token appeared in logs: {messages:?}"
  );
  assert!(
    messages
      .iter()
      .any(|message| message.contains("\"idTag\":\"<redacted>\"")),
    "redacted idTag missing from logs: {messages:?}"
  );
}

#[tokio::test]
async fn pending_security_events_enqueue_when_queue_space_opens() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator.config.outbound_queue_limit = 1;

  simulator.enqueue_heartbeat();
  simulator.record_security_event("InvalidFirmwareSignature", None);
  assert_eq!(simulator.queue.len(), 1);

  simulator
    .try_send_next(&mut write)
    .await
    .expect("send heartbeat");

  let OcppFrame::Call { action, .. } = read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALL frame");
  };
  assert_eq!(action, "Heartbeat");
  assert_eq!(simulator.queue.len(), 1);
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("SecurityEventNotification")
  );
}

#[tokio::test]
async fn boot_retry_is_prioritized_when_registration_is_not_accepted() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator.boot_registration_status = BootRegistrationStatus::Pending;

  simulator.enqueue_heartbeat();
  simulator
    .handle_common_command(SimulatorCommand::Boot, true)
    .expect("boot command");
  assert_eq!(
    simulator
      .queue
      .iter()
      .map(|call| call.action.as_str())
      .collect::<Vec<_>>(),
    vec!["Heartbeat", "BootNotification"]
  );

  simulator
    .try_send_next(&mut write)
    .await
    .expect("prioritize boot retry");
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("BootNotification")
  );

  simulator
    .try_send_next(&mut write)
    .await
    .expect("send boot retry");

  let OcppFrame::Call { action, .. } = read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALL frame");
  };
  assert_eq!(action, "BootNotification");
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("Heartbeat")
  );
}

#[tokio::test]
async fn boot_response_starts_heartbeat_from_interval() {
  let mut simulator = simulator_for_tests();
  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Accepted",
        "currentTime": now_timestamp(),
        "interval": 17
      }),
    )
    .expect("boot response should apply");

  assert_eq!(
    simulator.heartbeat.as_ref().map(|item| item.seconds),
    Some(17)
  );
  assert_eq!(
    simulator
      .configuration
      .get(&ConfigurationKey::HeartbeatInterval)
      .map(|entry| entry.value.as_str()),
    Some("17")
  );
  simulator.stop_heartbeat();
}

#[tokio::test]
async fn mock_csms_boot_lifecycle_updates_heartbeat() {
  let (mut write, mut read, mut server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator
    .enqueue_boot_notification()
    .expect("boot should validate");
  assert_eq!(
    simulator
      .queue
      .iter()
      .map(|call| call.action.as_str())
      .collect::<Vec<_>>(),
    vec!["BootNotification"]
  );
  simulator
    .try_send_next(&mut write)
    .await
    .expect("send boot");
  let frame = read_ocpp_frame(&mut server_read).await;
  let OcppFrame::Call {
    message_id, action, ..
  } = frame
  else {
    panic!("expected CALL frame");
  };
  assert_eq!(action, "BootNotification");
  let response = build_call_result(
    &message_id,
    &json!({
      "status": "Accepted",
      "currentTime": now_timestamp(),
      "interval": 9
    }),
  );
  server_write
    .send(Message::Text(response.into()))
    .await
    .expect("send boot response");
  let message = read
    .next()
    .await
    .expect("boot response")
    .expect("boot response ok");
  simulator
    .handle_ws_message(message, &mut write)
    .await
    .expect("handle response");

  assert_eq!(
    simulator
      .queue
      .iter()
      .filter(|call| call.action == "StatusNotification")
      .map(|call| {
        call.payload["connectorId"].as_u64().expect("connector ID")
      })
      .collect::<Vec<_>>(),
    vec![0, 1, 2]
  );
  assert_eq!(
    simulator.heartbeat.as_ref().map(|item| item.seconds),
    Some(9)
  );
  simulator.stop_heartbeat();
}

#[tokio::test]
async fn malformed_remote_start_returns_call_error() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "RemoteStartTransaction",
    json!({}),
  )
  .await;
  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
}

#[tokio::test]
async fn remote_start_v1_6_authorizes_before_start_when_configured() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "RemoteStartTransaction",
    json!({ "idTag": "TOKEN" }),
  )
  .await;

  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
  let authorize = simulator.queue.front().expect("queued authorize");
  assert_eq!(authorize.action, "Authorize");
  assert!(matches!(
    &authorize.context,
    PendingContext::RemoteStartAuthorizeV1_6 {
      connector: 1,
      id_token,
      charging_profile,
    } if id_token == "TOKEN" && charging_profile.is_none()
  ));
}

#[tokio::test]
async fn remote_start_v1_6_rejects_invalid_profile_before_authorize() {
  let profile = json!({
    "chargingProfileId": 1,
    "chargingProfilePurpose": "TxProfile",
    "chargingProfileKind": "Absolute",
    "chargingSchedule": {
      "chargingRateUnit": "A",
      "chargingSchedulePeriod": [{ "startPeriod": 0 }]
    }
  });
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "RemoteStartTransaction",
    json!({
      "idTag": "TOKEN",
      "chargingProfile": profile
    }),
  )
  .await;

  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
  assert!(simulator.queue.is_empty());
  assert!(simulator.charging_profiles.is_empty());
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
}

#[tokio::test]
async fn remote_start_v1_6_rejects_invalid_profile_before_immediate_start() {
  let profile = json!({
    "chargingProfileId": 1,
    "chargingProfilePurpose": "TxProfile",
    "chargingProfileKind": "Absolute",
    "chargingSchedule": {
      "chargingRateUnit": "A",
      "chargingSchedulePeriod": [{ "startPeriod": 0 }]
    }
  });
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  assert_eq!(
    simulator.set_configuration_value(
      ConfigurationKey::AuthorizeRemoteTxRequests,
      "false",
    ),
    ResponseStatus::Accepted
  );

  simulator
    .handle_incoming_call(
      &mut write,
      "test-message",
      "RemoteStartTransaction",
      json!({
        "idTag": "TOKEN",
        "chargingProfile": profile
      }),
    )
    .await
    .expect("handle remote start");

  let OcppFrame::CallResult { payload, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
  assert!(simulator.queue.is_empty());
  assert!(simulator.charging_profiles.is_empty());
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
}

#[tokio::test]
async fn remote_start_stop_v1_6_rejected_until_boot_accepted() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.boot_registration_status = BootRegistrationStatus::Pending;

  simulator
    .handle_incoming_call_v1_6(
      &mut write,
      "start-before-boot",
      "RemoteStartTransaction",
      json!({ "connectorId": 1, "idTag": "TOKEN" }),
    )
    .await
    .expect("handle remote start");

  let OcppFrame::CallResult { payload, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
  assert!(simulator.queue.is_empty());

  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("local transaction");

  simulator
    .handle_incoming_call_v1_6(
      &mut write,
      "stop-before-boot",
      "RemoteStopTransaction",
      json!({ "transactionId": 1 }),
    )
    .await
    .expect("handle remote stop");

  let OcppFrame::CallResult { payload, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .is_some()
  );
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn remote_start_v1_6_rejects_unstartable_connectors() {
  for connector_id in [0, 999] {
    let (frame, simulator) = capture_inbound_call_response(
      OcppVersion::V1_6,
      "RemoteStartTransaction",
      json!({
        "connectorId": connector_id,
        "idTag": "TOKEN"
      }),
    )
    .await;

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
    assert!(simulator.queue.is_empty());
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );
  }
}

#[tokio::test]
async fn remote_start_v1_6_applies_charging_profile_after_authorize() {
  let profile = json!({
    "chargingProfileId": 1,
    "chargingProfilePurpose": "TxProfile",
    "chargingProfileKind": "Absolute",
    "chargingSchedule": {
      "chargingRateUnit": "A",
      "chargingSchedulePeriod": [
        { "startPeriod": 0, "limit": 0 }
      ]
    }
  });
  let (frame, mut simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "RemoteStartTransaction",
    json!({
      "idTag": "TOKEN",
      "chargingProfile": profile.clone()
    }),
  )
  .await;

  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  let authorize = simulator.queue.pop_front().expect("queued authorize");
  assert!(matches!(
    &authorize.context,
    PendingContext::RemoteStartAuthorizeV1_6 {
      connector: 1,
      id_token,
      charging_profile: Some(queued_profile),
    } if id_token == "TOKEN" && queued_profile == &profile
  ));

  simulator
    .apply_call_result_context(
      &authorize.context,
      &json!({
        "idTagInfo": { "status": "Accepted" }
      }),
    )
    .expect("remote-start authorization should apply");

  assert_eq!(simulator.charging_profiles.get(&1), Some(&profile));
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .and_then(|item| item.offered_limit),
    Some(0.0)
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("SuspendedEVSE")
  );
  assert!(
    simulator
      .queue
      .iter()
      .any(|call| call.action == "StartTransaction")
  );
  let status_payload = queued_payload(&simulator, "StatusNotification");
  assert_eq!(status_payload["status"], json!("SuspendedEVSE"));
}

#[tokio::test]
async fn malformed_request_start_v2_x_returns_call_error() {
  let malformed_payloads = [
    json!({
      "idToken": {
        "idToken": "TOKEN",
        "type": "Central"
      }
    }),
    json!({
      "remoteStartId": 11,
      "idToken": {
        "idToken": "TOKEN"
      }
    }),
    json!({
      "remoteStartId": 11,
      "idToken": {
        "type": "Central"
      }
    }),
    json!({
      "remoteStartId": 11,
      "idToken": {
        "idToken": "TOKEN",
        "type": "Central"
      },
      "evseId": "bad"
    }),
  ];

  for protocol in v2_x_protocols() {
    for payload in malformed_payloads.clone() {
      let (frame, simulator) = capture_inbound_call_response(
        protocol,
        "RequestStartTransaction",
        payload,
      )
      .await;
      let OcppFrame::CallError { code, .. } = frame else {
        panic!("expected CALLERROR frame");
      };
      assert_eq!(code, format_violation_code(protocol));
      assert!(
        simulator
          .connectors
          .values()
          .all(|connector| connector.transaction.is_none())
      );
    }
  }
}

#[tokio::test]
async fn request_start_stop_v2_x_rejected_until_boot_accepted() {
  for protocol in v2_x_protocols() {
    let (mut write, _read, _server_write, mut server_read) =
      in_memory_ws_pair().await;
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator.boot_registration_status = BootRegistrationStatus::Pending;

    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "start-before-boot",
        "RequestStartTransaction",
        json!({
          "remoteStartId": 12,
          "idToken": {
            "idToken": "TOKEN",
            "type": "Central"
          },
          "evseId": 1
        }),
      )
      .await
      .expect("handle request start");

    let OcppFrame::CallResult { payload, .. } =
      read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );

    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("local transaction");
    let transaction_id = simulator
      .active_transaction_uid(1)
      .expect("active transaction uid");

    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "stop-before-boot",
        "RequestStopTransaction",
        json!({ "transactionId": transaction_id }),
      )
      .await
      .expect("handle request stop");

    let OcppFrame::CallResult { payload, .. } =
      read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .is_some()
    );
  }
}

#[tokio::test]
async fn request_start_v2_x_applies_charging_profile() {
  for protocol in v2_x_protocols() {
    let profile = json!({
      "id": 1,
      "stackLevel": 0,
      "chargingProfilePurpose": "TxProfile",
      "chargingProfileKind": "Absolute",
      "chargingSchedule": [
        {
          "id": 1,
          "chargingRateUnit": "A",
          "chargingSchedulePeriod": [
            { "startPeriod": 0, "limit": 0 }
          ]
        }
      ]
    });
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "RequestStartTransaction",
      json!({
        "remoteStartId": 12,
        "idToken": {
          "idToken": "TOKEN",
          "type": "Central"
        },
        "evseId": 1,
        "chargingProfile": profile.clone()
      }),
    )
    .await;

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
    assert_eq!(simulator.charging_profiles.get(&1), Some(&profile));
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .and_then(|item| item.offered_limit),
      Some(0.0)
    );
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("SuspendedEVSE")
    );
    assert!(
      simulator
        .queue
        .iter()
        .any(|call| call.action == "TransactionEvent")
    );
    let status_payload = queued_payload(&simulator, "StatusNotification");
    assert_eq!(status_payload["connectorStatus"], json!("Occupied"));
  }
}

#[tokio::test]
async fn request_start_v2_x_rejects_invalid_profile_before_start() {
  for protocol in v2_x_protocols() {
    let profile = json!({
      "id": 1,
      "stackLevel": 0,
      "chargingProfilePurpose": "TxProfile",
      "chargingProfileKind": "Absolute",
      "chargingSchedule": [
        {
          "id": 1,
          "chargingRateUnit": "A",
          "chargingSchedulePeriod": [{ "startPeriod": 0 }]
        }
      ]
    });
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "RequestStartTransaction",
      json!({
        "remoteStartId": 12,
        "idToken": {
          "idToken": "TOKEN",
          "type": "Central"
        },
        "evseId": 1,
        "chargingProfile": profile
      }),
    )
    .await;

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
    assert!(simulator.queue.is_empty());
    assert!(simulator.charging_profiles.is_empty());
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );
  }
}

#[tokio::test]
async fn request_start_v2_x_accepts_not_yet_authorized_active_evse() {
  for protocol in v2_x_protocols() {
    let (mut write, _read, _server_write, mut server_read) =
      in_memory_ws_pair().await;
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start transaction");
    let transaction_id = simulator
      .active_transaction_uid(1)
      .expect("active transaction uid");

    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "duplicate-start",
        "RequestStartTransaction",
        json!({
          "remoteStartId": 12,
          "idToken": {
            "idToken": "TOKEN",
            "type": "Central"
          },
          "evseId": 1
        }),
      )
      .await
      .expect("handle request start");

    let OcppFrame::CallResult { payload, .. } =
      read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
    assert_eq!(payload["transactionId"], json!(transaction_id));
  }
}

#[tokio::test]
async fn request_start_v2_x_rejects_authorized_active_evse() {
  for protocol in v2_x_protocols() {
    let (mut write, _read, _server_write, mut server_read) =
      in_memory_ws_pair().await;
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start transaction");
    let context = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          &call.context,
          PendingContext::TxEvent {
            event_type: TxEventType::Started,
            ..
          }
        )
      })
      .map(|call| call.context.clone())
      .expect("queued started event");
    simulator
      .apply_call_result_context(
        &context,
        &json!({
          "idTokenInfo": { "status": "Accepted" }
        }),
      )
      .expect("apply transaction event response");
    simulator.queue.clear();

    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "duplicate-start",
        "RequestStartTransaction",
        json!({
          "remoteStartId": 13,
          "idToken": {
            "idToken": "TOKEN",
            "type": "Central"
          },
          "evseId": 1
        }),
      )
      .await
      .expect("handle request start");

    let OcppFrame::CallResult { payload, .. } =
      read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Rejected.as_str()));
    assert!(payload.get("transactionId").is_none());
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn malformed_request_stop_v2_x_returns_call_error() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "RequestStopTransaction",
      json!({}),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );
  }
}

#[tokio::test]
async fn malformed_supported_requests_return_format_violation_code() {
  let mut cases = vec![
    (
      OcppVersion::V1_6,
      "ReserveNow",
      json!({ "reservationId": 1 }),
    ),
    (OcppVersion::V1_6, "UnlockConnector", json!({})),
    (
      OcppVersion::V1_6,
      "ChangeConfiguration",
      json!({ "key": "MeterValueSampleInterval" }),
    ),
    (
      OcppVersion::V1_6,
      "ChangeAvailability",
      json!({ "type": "Inoperative" }),
    ),
    (
      OcppVersion::V1_6,
      "SetChargingProfile",
      json!({ "csChargingProfiles": {} }),
    ),
    (
      OcppVersion::V1_6,
      "GetCompositeSchedule",
      json!({ "connectorId": 1 }),
    ),
    (OcppVersion::V1_6, "TriggerMessage", json!({})),
  ];

  for protocol in v2_x_protocols() {
    cases.extend([
      (protocol, "UnlockConnector", json!({ "evseId": 1 })),
      (
        protocol,
        "SetChargingProfile",
        json!({ "chargingProfile": {} }),
      ),
      (protocol, "GetCompositeSchedule", json!({ "evseId": 1 })),
      (protocol, "TriggerMessage", json!({})),
    ]);
  }

  for (protocol, action, payload) in cases {
    let (frame, simulator) =
      capture_inbound_call_response(protocol, action, payload).await;
    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame for {action}");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );
  }
}

#[tokio::test]
async fn v2_x_unlock_rejects_unsupported_connector_id() {
  let cases = [
    (OcppVersion::V2_0_1, 0),
    (OcppVersion::V2_0_1, 2),
    (OcppVersion::V2_1, 2),
  ];

  for (protocol, connector_id) in cases {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "UnlockConnector",
      json!({
        "evseId": 1,
        "connectorId": connector_id,
      }),
    )
    .await;

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(
      payload["status"],
      json!(ResponseStatus::UnknownConnector.as_str())
    );
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn unsupported_actions_return_protocol_call_errors() {
  let cases = [
    (OcppVersion::V2_0_1, "GetBaseReport", "NotSupported"),
    (OcppVersion::V2_1, "BatterySwap", "NotSupported"),
    (OcppVersion::V1_6, "TotallyUnknown", "NotImplemented"),
    (OcppVersion::V2_0_1, "TotallyUnknown", "NotImplemented"),
  ];

  for (protocol, action, expected_code) in cases {
    let (frame, _) =
      capture_inbound_call_response(protocol, action, json!({})).await;
    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame for {action}");
    };
    assert_eq!(code, expected_code);
  }
}

#[tokio::test]
async fn v1_6_update_firmware_returns_not_supported_for_whitepaper() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "UpdateFirmware",
    json!({
      "location": "https://csms.example/firmware.bin",
      "retrieveDate": now_timestamp()
    }),
  )
  .await;

  let OcppFrame::CallError {
    code, description, ..
  } = frame
  else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "NotSupported");
  assert!(description.contains("SignedUpdateFirmware"));
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn strict_mode_rejects_schema_invalid_v1_6_requests() {
  let (frame, simulator) = capture_inbound_call_response_with_strict(
    OcppVersion::V1_6,
    true,
    "DataTransfer",
    json!({
      "vendorId": "ocppsim",
      "data": { "not": "a string" }
    }),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn strict_mode_rejects_oversized_install_certificate_v1_6() {
  let (frame, simulator) = capture_inbound_call_response_with_strict(
    OcppVersion::V1_6,
    true,
    "InstallCertificate",
    json!({
      "certificateType": "CentralSystemRootCertificate",
      "certificate": "A".repeat(5_501)
    }),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(simulator.security.certificates.is_empty());
}

#[tokio::test]
async fn strict_mode_rejects_schema_invalid_v2_x_requests() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response_with_strict(
      protocol,
      true,
      "Reset",
      json!({}),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn strict_mode_caches_request_schema_validator_per_action() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  simulator.config.strict = true;

  simulator
    .handle_incoming_call(&mut write, "message-1", "Reset", json!({}))
    .await
    .expect("first strict call");

  let first_frame = read_ocpp_frame(&mut server_read).await;
  let OcppFrame::CallError { code, .. } = first_frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormatViolation");

  let cache_key = format!("{}:Reset", OcppVersion::V2_1.subprotocol());
  assert_eq!(simulator.incoming_request_validators.len(), 1);
  assert!(
    simulator
      .incoming_request_validators
      .contains_key(&cache_key)
  );

  simulator
    .handle_incoming_call(&mut write, "message-2", "Reset", json!({}))
    .await
    .expect("second strict call");

  let second_frame = read_ocpp_frame(&mut server_read).await;
  let OcppFrame::CallError { code, .. } = second_frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormatViolation");
  assert_eq!(simulator.incoming_request_validators.len(), 1);
  assert!(
    simulator
      .incoming_request_validators
      .contains_key(&cache_key)
  );
}

#[tokio::test]
async fn strict_mode_rejects_schema_invalid_v1_6_call_result() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.config.strict = true;
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start transaction");
  simulator
    .try_send_next(&mut write)
    .await
    .expect("send start transaction");
  let OcppFrame::Call { message_id, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALL frame");
  };

  simulator
    .handle_ws_text(
      build_call_result(
        &message_id,
        &json!({
          "idTagInfo": { "status": "Accepted" }
        }),
      ),
      &mut write,
    )
    .await
    .expect("handle invalid response");

  assert!(simulator.pending.is_none());
  assert!(simulator.connector_ref(1).unwrap().transaction.is_none());
  assert_eq!(
    simulator.connector_ref(1).unwrap().status,
    ConnectorStatus::Available
  );
}

#[tokio::test]
async fn strict_mode_sends_call_result_error_for_invalid_v2_1_response() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  simulator.config.strict = true;
  simulator.connected = true;
  simulator
    .enqueue_boot_notification()
    .expect("boot should validate");
  simulator
    .try_send_next(&mut write)
    .await
    .expect("send boot");
  let OcppFrame::Call { message_id, .. } =
    read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALL frame");
  };

  simulator
    .handle_ws_text(
      build_call_result(&message_id, &json!({ "status": "Accepted" })),
      &mut write,
    )
    .await
    .expect("handle invalid response");

  let OcppFrame::CallResultError {
    message_id: error_message_id,
    code,
    ..
  } = read_ocpp_frame(&mut server_read).await
  else {
    panic!("expected CALLRESULTERROR frame");
  };
  assert_eq!(error_message_id, message_id);
  assert_eq!(code, "FormatViolation");
  assert!(simulator.pending.is_none());
  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Accepted
  );
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn strict_mode_warns_when_request_schema_is_missing() {
  let (frame, simulator, events) = capture_inbound_call_response_with_events(
    OcppVersion::V1_6,
    true,
    "TotallyUnknown",
    json!({}),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "NotImplemented");
  assert!(simulator.queue.is_empty());
  assert!(events.iter().any(|event| {
    matches!(
      event,
      UiEvent::Log {
        level: UiLogLevel::Warn,
        message,
      } if message.contains("Strict schema coverage is missing")
        && message.contains("TotallyUnknown")
    )
  }));
}

#[tokio::test]
async fn invalid_composite_schedule_unit_returns_property_constraint() {
  for strict in [false, true] {
    let (frame, _) = capture_inbound_call_response_with_strict(
      OcppVersion::V1_6,
      strict,
      "GetCompositeSchedule",
      json!({
        "connectorId": 1,
        "duration": 60,
        "chargingRateUnit": "Wh",
      }),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, "PropertyConstraintViolation");
  }
}

#[tokio::test]
async fn non_strict_mode_rejects_data_transfer_without_vendor_id() {
  let protocols = [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1];

  for protocol in protocols {
    let (frame, simulator) =
      capture_inbound_call_response(protocol, "DataTransfer", json!({})).await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn non_strict_mode_rejects_non_string_data_transfer_data_v1_6() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "DataTransfer",
    json!({
      "vendorId": "ocppsim",
      "data": { "not": "a string" }
    }),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn non_strict_mode_rejects_malformed_clear_charging_profile_v1_6() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "ClearChargingProfile",
    json!({ "connectorId": "1" }),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn non_strict_mode_rejects_malformed_clear_charging_profile_v2_x() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "ClearChargingProfile",
      json!({
        "chargingProfileCriteria": {
          "evseId": "1"
        }
      }),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn non_strict_mode_rejects_non_integer_clear_charging_profile_id_v2_x() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "ClearChargingProfile",
      json!({
        "chargingProfileId": "123"
      }),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(simulator.queue.is_empty());
  }
}

#[tokio::test]
async fn non_strict_mode_rejects_set_charging_profile_without_limit_v1_6() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "SetChargingProfile",
    json!({
      "connectorId": 1,
      "csChargingProfiles": {
        "chargingSchedule": {
          "chargingSchedulePeriod": [
            { "startPeriod": 0 }
          ]
        }
      }
    }),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "FormationViolation");
  assert!(!simulator.charging_profiles.contains_key(&1));
}

#[tokio::test]
async fn non_strict_mode_rejects_set_charging_profile_without_limit_v2_x() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "SetChargingProfile",
      json!({
        "evseId": 1,
        "chargingProfile": {
          "chargingSchedule": [
            {
              "chargingSchedulePeriod": [
                { "startPeriod": 0 }
              ]
            }
          ]
        }
      }),
    )
    .await;

    let OcppFrame::CallError { code, .. } = frame else {
      panic!("expected CALLERROR frame");
    };
    assert_eq!(code, format_violation_code(protocol));
    assert!(!simulator.charging_profiles.contains_key(&1));
  }
}

#[tokio::test]
async fn non_strict_mode_keeps_pragmatic_v2_x_request_handling() {
  for protocol in v2_x_protocols() {
    let (frame, _) =
      capture_inbound_call_response(protocol, "Reset", json!({})).await;

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  }
}

#[tokio::test]
async fn rejected_v1_6_registration_ignores_inbound_calls_without_response() {
  let (mut write, _read, _server_write, mut server_read) =
    in_memory_ws_pair().await;
  let mut simulator = simulator_for_tests();
  simulator.boot_registration_status = BootRegistrationStatus::Rejected;

  simulator
    .handle_ws_text(
      build_call("blocked", "GetConfiguration", &json!({})),
      &mut write,
    )
    .await
    .expect("handle inbound call");

  let read_result =
    tokio::time::timeout(Duration::from_millis(20), server_read.next()).await;
  assert!(read_result.is_err());
  assert!(simulator.queue.is_empty());
}

#[tokio::test]
async fn trigger_message_v1_6_enqueues_requested_simulator_calls() {
  let (v1_6_frame, v1_6_simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "TriggerMessage",
    json!({ "requestedMessage": "Heartbeat" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = v1_6_frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  assert!(
    v1_6_simulator
      .queue
      .iter()
      .any(|call| call.action == "Heartbeat")
  );
}

#[tokio::test]
async fn trigger_message_v1_6_standard_and_extended_are_separate() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "TriggerMessage",
    json!({ "requestedMessage": "DiagnosticsStatusNotification" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  assert!(
    simulator
      .queue
      .iter()
      .any(|call| { call.action == "DiagnosticsStatusNotification" })
  );

  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "TriggerMessage",
    json!({ "requestedMessage": "LogStatusNotification" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(
    payload["status"],
    json!(ResponseStatus::NotImplemented.as_str())
  );
  assert!(simulator.queue.is_empty());

  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V1_6,
    "ExtendedTriggerMessage",
    json!({ "requestedMessage": "LogStatusNotification" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  assert!(
    simulator
      .queue
      .iter()
      .any(|call| { call.action == "LogStatusNotification" })
  );
}

#[tokio::test]
async fn trigger_message_v2_x_enqueues_requested_simulator_calls() {
  for protocol in v2_x_protocols() {
    let (frame, simulator) = capture_inbound_call_response(
      protocol,
      "TriggerMessage",
      json!({
        "requestedMessage": "StatusNotification",
        "evse": { "id": 2 }
      }),
    )
    .await;
    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
    let status_payload = queued_payload(&simulator, "StatusNotification");
    assert_eq!(status_payload["evseId"], json!(2));
  }
}

#[tokio::test]
async fn trigger_message_v2_x_uses_version_specific_values() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V2_0_1,
    "TriggerMessage",
    json!({ "requestedMessage": "SignV2G20Certificate" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(
    payload["status"],
    json!(ResponseStatus::NotImplemented.as_str())
  );
  assert!(simulator.queue.is_empty());

  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V2_1,
    "TriggerMessage",
    json!({ "requestedMessage": "SignV2G20Certificate" }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  let sign_payload = queued_payload(&simulator, "SignCertificate");
  assert_eq!(sign_payload["certificateType"], "V2G20Certificate");
}

#[test]
fn trigger_message_v2_x_can_trigger_active_transaction_event() {
  for protocol in v2_x_protocols() {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start transaction");
    simulator.queue.clear();

    let status = simulator
      .trigger_message_v2_x(
        crate::ocpp::TriggerMessage_V2_X::TransactionEvent,
        Some(1),
      )
      .expect("trigger transaction event");
    assert_eq!(status, ResponseStatus::Accepted);
    let payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(payload["triggerReason"], "Trigger");
    assert_eq!(payload["eventType"], "Updated");
    assert_eq!(payload["seqNo"], 2);

    simulator.queue.clear();
    let status = simulator
      .trigger_message_v2_x(
        crate::ocpp::TriggerMessage_V2_X::TransactionEvent,
        Some(1),
      )
      .expect("trigger transaction event again");
    assert_eq!(status, ResponseStatus::Accepted);
    let payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(payload["seqNo"], 3);
  }
}

#[tokio::test]
async fn mock_csms_remote_start_meter_and_stop_v2_x_lifecycle() {
  for protocol in v2_x_protocols() {
    let (mut write, _read, _server_write, mut server_read) =
      in_memory_ws_pair().await;
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "remote-start",
        "RequestStartTransaction",
        json!({
          "remoteStartId": 99,
          "idToken": {
            "idToken": "TOKEN",
            "type": "Central"
          },
          "evseId": 1
        }),
      )
      .await
      .expect("handle request start");
    simulator
      .send_meter(1, true)
      .expect("meter should enqueue update");
    let transaction_id = simulator
      .active_transaction_uid(1)
      .expect("active transaction uid");
    simulator
      .handle_incoming_call_v2_x(
        &mut write,
        "remote-stop",
        "RequestStopTransaction",
        json!({ "transactionId": transaction_id }),
      )
      .await
      .expect("handle request stop");
    drop(write);

    let frames = read_ocpp_frames(&mut server_read, 2).await;
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|frame| {
      matches!(
        frame,
        OcppFrame::CallResult {
          payload,
          ..
        } if payload.get("status").and_then(Value::as_str)
          == Some(ResponseStatus::Accepted.as_str())
      )
    }));
    assert!(
      simulator
        .queue
        .iter()
        .filter(|call| call.action == "TransactionEvent")
        .count()
        >= 3
    );
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("Finishing")
    );
  }
}

#[tokio::test]
async fn malformed_ws_text_returns_protocol_error() {
  let (frame, _, _) = capture_ws_text_response_with_events(
    OcppVersion::V1_6,
    "not-json".to_string(),
  )
  .await;
  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "ProtocolError");
}

#[tokio::test]
async fn call_error_logs_escape_control_characters() {
  let events = capture_ws_text_events(
    OcppVersion::V2_1,
    build_call_error(
      "m1",
      "InternalError",
      "bad\u{1b}[31m\nline\rend",
      &json!({}),
    ),
  )
  .await;

  let log_messages = events
    .iter()
    .filter_map(|event| {
      if let UiEvent::Log { message, .. } = event {
        Some(message)
      } else {
        None
      }
    })
    .collect::<Vec<_>>();

  assert!(
    log_messages
      .iter()
      .all(|message| !message.contains('\u{1b}'))
  );
  assert!(log_messages.iter().all(|message| !message.contains('\r')));
  assert!(
    log_messages
      .iter()
      .any(|message| message.contains("\\u001b"))
  );
  assert!(
    log_messages
      .iter()
      .any(|message| message.contains("\\u000a"))
  );
  assert!(
    log_messages
      .iter()
      .any(|message| message.contains("\\u000d"))
  );
}
