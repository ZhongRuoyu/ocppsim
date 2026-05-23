use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use http::header::SEC_WEBSOCKET_PROTOCOL;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::handshake::server::{
  ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{accept_async, accept_hdr_async, connect_async};

use crate::ocpp::{OcppFrame, build_call_result, parse_frame};

use super::*;

// The tungstenite handshake callback trait requires this large error type
// and result wrapping.
#[allow(clippy::result_large_err, clippy::unnecessary_wraps)]
fn accept_v1_6_subprotocol(
  _request: &Request,
  mut response: Response,
) -> Result<Response, ErrorResponse> {
  response
    .headers_mut()
    .insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("ocpp1.6"));
  Ok(response)
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
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind listener");
  let address = listener.local_addr().expect("local address");
  let server = tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept client");
    let websocket = accept_async(stream).await.expect("accept websocket");
    let (_server_write, mut server_read) = websocket.split();
    let message = server_read
      .next()
      .await
      .expect("response frame")
      .expect("response frame ok");
    parse_frame(message.to_text().expect("text frame")).expect("parse response")
  });

  let (client_stream, _) = connect_async(format!("ws://{address}"))
    .await
    .expect("connect client");
  let (mut write, _read) = client_stream.split();
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(protocol);
  simulator.config.strict = strict;
  simulator
    .handle_incoming_call(&mut write, "test-message", action, payload)
    .await
    .expect("handle inbound call");
  drop(write);

  let frame = server.await.expect("server task");
  let mut events = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    events.push(event);
  }
  (frame, simulator, events)
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
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind listener");
  let address = listener.local_addr().expect("local address");
  let server = tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept client");
    let mut websocket = accept_hdr_async(stream, accept_v1_6_subprotocol)
      .await
      .expect("accept websocket");
    let message = websocket
      .next()
      .await
      .expect("boot frame")
      .expect("boot frame ok");
    let frame =
      parse_frame(message.to_text().expect("text frame")).expect("parse frame");
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
    websocket
      .send(Message::Text(response.into()))
      .await
      .expect("send boot response");
  });

  let mut simulator = simulator_for_tests();
  simulator.config.ws_url = format!("ws://{address}");
  let mut connection = simulator.connect().await.expect("connect");
  simulator
    .try_send_next(&mut connection.write)
    .await
    .expect("send boot");
  let message = connection
    .read
    .next()
    .await
    .expect("boot response")
    .expect("boot response ok");
  simulator
    .handle_ws_message(message, &mut connection.write)
    .await
    .expect("handle response");

  assert_eq!(
    simulator.heartbeat.as_ref().map(|item| item.seconds),
    Some(9)
  );
  simulator.stop_heartbeat();
  server.await.expect("server task");
}

#[tokio::test]
async fn malformed_remote_start_returns_call_error() {
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind listener");
  let address = listener.local_addr().expect("local address");
  let server = tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept client");
    let websocket = accept_async(stream).await.expect("accept websocket");
    let (_server_write, mut server_read) = websocket.split();
    let message = server_read
      .next()
      .await
      .expect("response frame")
      .expect("response frame ok");
    parse_frame(message.to_text().expect("text frame")).expect("parse response")
  });

  let (client_stream, _) = connect_async(format!("ws://{address}"))
    .await
    .expect("connect client");
  let (mut write, _read) = client_stream.split();
  let mut simulator = simulator_for_tests();
  simulator
    .handle_incoming_call_v1_6(
      &mut write,
      "bad-remote-start",
      "RemoteStartTransaction",
      json!({}),
    )
    .await
    .expect("handle malformed remote start");
  drop(write);

  let frame = server.await.expect("server task");
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
async fn malformed_request_start_v2_returns_call_error() {
  let malformed_payloads = [
    json!({ "idToken": { "idToken": "TOKEN" } }),
    json!({ "remoteStartId": 11, "idToken": {} }),
    json!({
      "remoteStartId": 11,
      "idToken": { "idToken": "TOKEN" },
      "evseId": "bad"
    }),
  ];

  for payload in malformed_payloads {
    let (frame, simulator) = capture_inbound_call_response(
      OcppVersion::V2_0_1,
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

#[tokio::test]
async fn malformed_request_stop_v2_returns_call_error() {
  let (frame, simulator) = capture_inbound_call_response(
    OcppVersion::V2_0_1,
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

#[tokio::test]
async fn malformed_supported_requests_return_formation_violation() {
  let cases = [
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
    (
      OcppVersion::V2_0_1,
      "UnlockConnector",
      json!({ "evseId": 1 }),
    ),
    (
      OcppVersion::V2_0_1,
      "SetChargingProfile",
      json!({ "chargingProfile": {} }),
    ),
    (
      OcppVersion::V2_0_1,
      "GetCompositeSchedule",
      json!({ "evseId": 1 }),
    ),
    (OcppVersion::V2_0_1, "TriggerMessage", json!({})),
  ];

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
    (OcppVersion::V1_6, "CertificateSigned", "NotSupported"),
    (OcppVersion::V2_0_1, "CertificateSigned", "NotSupported"),
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
async fn strict_mode_rejects_schema_invalid_v2_x_requests() {
  let (frame, simulator) = capture_inbound_call_response_with_strict(
    OcppVersion::V2_0_1,
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

#[tokio::test]
async fn strict_mode_warns_when_request_schema_is_missing() {
  let (frame, simulator, events) = capture_inbound_call_response_with_events(
    OcppVersion::V1_6,
    true,
    "CertificateSigned",
    json!({}),
  )
  .await;

  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "NotSupported");
  assert!(simulator.queue.is_empty());
  assert!(events.iter().any(|event| {
    matches!(
      event,
      UiEvent::Log {
        level: UiLogLevel::Warn,
        message,
      } if message.contains("Strict schema coverage is missing")
        && message.contains("CertificateSigned")
    )
  }));
}

#[tokio::test]
async fn non_strict_mode_keeps_pragmatic_request_handling() {
  let (frame, _) =
    capture_inbound_call_response(OcppVersion::V2_0_1, "Reset", json!({}))
      .await;

  let OcppFrame::CallResult { payload, .. } = frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
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
async fn trigger_message_v2_0_1_enqueues_requested_simulator_calls() {
  let (v2_0_1_frame, v2_0_1_simulator) = capture_inbound_call_response(
    OcppVersion::V2_0_1,
    "TriggerMessage",
    json!({
      "requestedMessage": "StatusNotification",
      "evse": { "id": 2 }
    }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = v2_0_1_frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  let status_payload = queued_payload(&v2_0_1_simulator, "StatusNotification");
  assert_eq!(status_payload["evseId"], json!(2));
}

#[tokio::test]
async fn trigger_message_v2_1_enqueues_requested_simulator_calls() {
  let (v2_1_frame, v2_1_simulator) = capture_inbound_call_response(
    OcppVersion::V2_1,
    "TriggerMessage",
    json!({
      "requestedMessage": "StatusNotification",
      "evse": { "id": 2 }
    }),
  )
  .await;
  let OcppFrame::CallResult { payload, .. } = v2_1_frame else {
    panic!("expected CALLRESULT frame");
  };
  assert_eq!(payload["status"], json!(ResponseStatus::Accepted.as_str()));
  let status_payload = queued_payload(&v2_1_simulator, "StatusNotification");
  assert_eq!(status_payload["evseId"], json!(2));
}

#[tokio::test]
async fn mock_csms_remote_start_meter_and_stop_lifecycle() {
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind listener");
  let address = listener.local_addr().expect("local address");
  let server = tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept client");
    let websocket = accept_async(stream).await.expect("accept websocket");
    let (_server_write, mut server_read) = websocket.split();
    let mut frames = Vec::new();
    for _ in 0..2 {
      let message = server_read
        .next()
        .await
        .expect("response frame")
        .expect("response frame ok");
      frames.push(
        parse_frame(message.to_text().expect("text frame"))
          .expect("parse response"),
      );
    }
    frames
  });

  let (client_stream, _) = connect_async(format!("ws://{address}"))
    .await
    .expect("connect client");
  let (mut write, _read) = client_stream.split();
  let mut simulator = simulator_for_tests_v2_0_1();
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

  let frames = server.await.expect("server task");
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

#[tokio::test]
async fn malformed_ws_text_returns_protocol_error() {
  let listener = TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind listener");
  let address = listener.local_addr().expect("local address");
  let server = tokio::spawn(async move {
    let (stream, _) = listener.accept().await.expect("accept client");
    let websocket = accept_async(stream).await.expect("accept websocket");
    let (_server_write, mut server_read) = websocket.split();
    let message = server_read
      .next()
      .await
      .expect("response frame")
      .expect("response frame ok");
    parse_frame(message.to_text().expect("text frame")).expect("parse response")
  });

  let (client_stream, _) = connect_async(format!("ws://{address}"))
    .await
    .expect("connect client");
  let (mut write, _read) = client_stream.split();
  let mut simulator = simulator_for_tests();
  simulator
    .handle_ws_text("not-json".to_string(), &mut write)
    .await
    .expect("handle malformed text");
  drop(write);

  let frame = server.await.expect("server task");
  let OcppFrame::CallError { code, .. } = frame else {
    panic!("expected CALLERROR frame");
  };
  assert_eq!(code, "ProtocolError");
}
