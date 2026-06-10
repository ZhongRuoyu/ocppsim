use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{DuplexStream, duplex};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::{Message, Role};

use crate::ocpp::{OcppFrame, build_call, build_call_result, parse_frame};

use super::*;

type TestWsStream = WebSocketStream<DuplexStream>;
type TestWsWrite = SplitSink<TestWsStream, Message>;
type TestWsRead = SplitStream<TestWsStream>;

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
  simulator.enqueue_boot_notification();
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
    } if id_token == "TOKEN"
  ));
}

#[tokio::test]
async fn malformed_request_start_v2_x_returns_call_error() {
  let malformed_payloads = [
    json!({ "idToken": { "idToken": "TOKEN" } }),
    json!({ "remoteStartId": 11, "idToken": {} }),
    json!({
      "remoteStartId": 11,
      "idToken": { "idToken": "TOKEN" },
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
      assert_eq!(code, "FormationViolation");
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
    assert_eq!(code, "FormationViolation");
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );
  }
}

#[tokio::test]
async fn malformed_supported_requests_return_formation_violation() {
  let mut cases = vec![
    (
      OcppVersion::V1_6,
      "ReserveNow",
      json!({ "reservationId": 1 }),
    ),
    (OcppVersion::V1_6, "UnlockConnector", json!({})),
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
    assert_eq!(code, "FormationViolation");
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
    assert_eq!(code, "FormationViolation");
    assert!(simulator.queue.is_empty());
  }
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
          "idToken": { "idToken": "TOKEN" },
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
