use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

#[test]
fn local_start_logs_redacted_id_token() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);

  simulator
    .start_transaction(1, "SECRET-TOKEN".to_string(), false, None, false)
    .expect("start should succeed");

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
      .any(|message| message.contains("<redacted>")),
    "redacted marker missing from logs: {messages:?}"
  );
}

#[test]
fn authorize_result_logs_redacted_id_token() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator
    .enqueue_authorize("SECRET-TOKEN".to_string())
    .expect("authorize should validate");
  let authorize_call = simulator.queue.pop_front().expect("queued authorize");
  simulator.pending = Some(PendingCall {
    message_id: "auth-ack".to_string(),
    sent_at: Instant::now(),
    call: authorize_call,
  });

  simulator
    .handle_call_result(
      "auth-ack",
      &json!({
        "idTagInfo": { "status": "Accepted" }
      }),
    )
    .expect("authorization acknowledgement should apply");

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
      .any(|message| message.contains("<redacted>")),
    "redacted marker missing from logs: {messages:?}"
  );
}

#[test]
fn stop_transaction_timeout_restores_v1_6_status() {
  let mut simulator = simulator_for_tests();
  simulator.config.request_timeout = Duration::from_millis(1);
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("start should succeed");
  let local_tx_id = simulator
    .connectors
    .get(&1)
    .and_then(|state| state.transaction.as_ref())
    .map(|tx| tx.local_id)
    .expect("local transaction");
  simulator
    .stop_transaction(1, Some("Local"), false, true)
    .expect("stop should enqueue");
  let stop_call = simulator
    .queue
    .iter()
    .find(|call| call.action == "StopTransaction")
    .expect("queued stop")
    .clone();
  simulator.pending = Some(PendingCall {
    message_id: "stop-timeout".to_string(),
    sent_at: Instant::now()
      .checked_sub(Duration::from_secs(1))
      .expect("past instant"),
    call: stop_call,
  });

  simulator.check_pending_timeout();

  assert!(simulator.pending.is_none());
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .map(|tx| tx.local_id),
    Some(local_tx_id)
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Charging")
  );
  let status_payload = queued_payload(&simulator, "StatusNotification");
  assert_eq!(status_payload["status"], json!("Charging"));
}

#[test]
fn transaction_event_end_timeout_restores_v2_x_status() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator.config.request_timeout = Duration::from_millis(1);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("start should succeed");
    let local_tx_id = simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .map(|tx| tx.local_id)
      .expect("local transaction");
    simulator
      .stop_transaction(1, Some("Local"), false, true)
      .expect("stop should enqueue");
    let end_call = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          &call.context,
          PendingContext::TxEvent {
            event_type: TxEventType::Ended,
            ..
          }
        )
      })
      .expect("queued end event")
      .clone();
    simulator.pending = Some(PendingCall {
      message_id: "end-timeout".to_string(),
      sent_at: Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("past instant"),
      call: end_call,
    });

    simulator.check_pending_timeout();

    assert!(simulator.pending.is_none());
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .map(|tx| tx.local_id),
      Some(local_tx_id)
    );
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("Occupied")
    );
    let status_payload = queued_payload(&simulator, "StatusNotification");
    assert_eq!(status_payload["connectorStatus"], json!("Occupied"));
  });
}

#[test]
fn stop_ack_enqueues_scheduled_v1_6_status() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("start should succeed");
  assert_eq!(
    simulator
      .change_availability_v1_6(&json!({
        "connectorId": 1,
        "type": "Inoperative"
      }))
      .expect("change availability"),
    ResponseStatus::Scheduled
  );
  simulator
    .stop_transaction(1, Some("Local"), false, true)
    .expect("stop should enqueue");
  assert!(
    !simulator
      .queue
      .iter()
      .any(|call| { call.action == "StatusNotification" })
  );

  let context = simulator
    .queue
    .iter()
    .find(|call| call.action == "StopTransaction")
    .map(|call| call.context.clone())
    .expect("queued stop");
  simulator
    .apply_call_result_context(&context, &json!({}))
    .expect("stop acknowledgement should apply");

  let status_payload = queued_payload(&simulator, "StatusNotification");
  assert_eq!(status_payload["status"], json!("Unavailable"));
}

#[test]
fn end_ack_enqueues_scheduled_v2_x_status() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("start should succeed");
    assert_eq!(
      simulator
        .change_availability_v2_x(&json!({
          "operationalStatus": "Inoperative",
          "evse": { "id": 1 }
        }))
        .expect("change availability"),
      ResponseStatus::Scheduled
    );
    simulator
      .stop_transaction(1, Some("Local"), false, true)
      .expect("stop should enqueue");
    assert!(
      !simulator
        .queue
        .iter()
        .any(|call| { call.action == "StatusNotification" })
    );

    let context = simulator
      .queue
      .iter()
      .find(|call| matches!(call.context, PendingContext::TxEvent { .. }))
      .map(|call| call.context.clone())
      .expect("queued transaction event");
    simulator
      .apply_call_result_context(&context, &json!({}))
      .expect("end acknowledgement should apply");

    let status_payload = queued_payload(&simulator, "StatusNotification");
    assert_eq!(status_payload["connectorStatus"], json!("Unavailable"));
  });
}

#[test]
fn stop_transaction_v1_6_remote_reason_is_preserved() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  simulator.queue.clear();

  simulator
    .stop_transaction(1, Some("Remote"), false, true)
    .expect("stop should enqueue");

  let payload = queued_payload(&simulator, "StopTransaction");
  assert_eq!(payload["reason"], json!("Remote"));
}

#[test]
fn stop_transaction_v2_x_remote_reason_is_preserved() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start should succeed");
    simulator.queue.clear();

    simulator
      .stop_transaction(1, Some("Remote"), false, true)
      .expect("stop should enqueue");

    let payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(payload["eventType"], json!("Ended"));
    assert_eq!(payload["transactionInfo"]["stoppedReason"], json!("Remote"));
  });
}

#[test]
fn start_transaction_v1_6_full_queue_does_not_mutate_state() {
  let mut simulator = simulator_for_tests();
  simulator.config.outbound_queue_limit = 1;
  simulator.enqueue_heartbeat();
  let next_tx_id = simulator.next_tx_id;

  let error = simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect_err("start should fail when the queue is full");

  assert!(error.to_string().contains("StartTransaction"));
  assert_eq!(simulator.next_tx_id, next_tx_id);
  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .is_none()
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|state| state.status.display()),
    Some("Available")
  );
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("Heartbeat")
  );
}

#[test]
fn start_transaction_v2_x_full_queue_does_not_mutate_state() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator.config.outbound_queue_limit = 1;
    simulator.enqueue_heartbeat();
    let next_tx_id = simulator.next_tx_id;

    let error = simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect_err("start should fail when the queue is full");

    assert!(error.to_string().contains("TransactionEvent"));
    assert_eq!(simulator.next_tx_id, next_tx_id);
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .is_none()
    );
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|state| state.status.display()),
      Some("Available")
    );
    assert_eq!(
      simulator.queue.front().map(|call| call.action.as_str()),
      Some("Heartbeat")
    );
  });
}

#[test]
fn stop_transaction_full_queue_keeps_transaction_active() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("offline start should mutate local state");
    simulator.config.outbound_queue_limit = 1;
    simulator.enqueue_heartbeat();

    let error = simulator
      .stop_transaction(1, Some("Local"), false, true)
      .expect_err("stop should fail when the queue is full");

    assert!(
      error
        .to_string()
        .contains(if protocol == OcppVersion::V1_6 {
          "StopTransaction"
        } else {
          "TransactionEvent"
        })
    );
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .is_some()
    );
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|state| state.status.display()),
      Some(if protocol == OcppVersion::V1_6 {
        "Charging"
      } else {
        "Occupied"
      })
    );
  }
}

#[test]
fn start_transaction_v1_6_ack_stores_remote_transaction_id() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should enqueue");
  let start_call = simulator
    .queue
    .iter()
    .find(|call| call.action == "StartTransaction")
    .expect("queued start")
    .clone();
  simulator.queue.clear();
  simulator.pending = Some(PendingCall {
    message_id: "start-ack".to_string(),
    sent_at: Instant::now(),
    call: start_call,
  });

  simulator
    .handle_call_result(
      "start-ack",
      &json!({
        "idTagInfo": { "status": "Accepted" },
        "transactionId": 1234
      }),
    )
    .expect("start acknowledgement should apply");

  assert!(simulator.pending.is_none());
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .and_then(|tx| tx.v1_6_transaction_id),
    Some(1234)
  );
  let status_payload = queued_payload(&simulator, "StatusNotification");
  assert_eq!(status_payload["status"], json!("Charging"));
}

#[test]
fn remote_start_authorization_acceptance_starts_v1_6_transaction() {
  let mut simulator = simulator_for_tests();
  simulator
    .enqueue_remote_start_authorize_v1_6(1, "TOKEN".to_string(), None)
    .expect("authorize should enqueue");
  let authorize_call = simulator.queue.pop_front().expect("queued authorize");
  simulator.pending = Some(PendingCall {
    message_id: "auth-ack".to_string(),
    sent_at: Instant::now(),
    call: authorize_call,
  });

  simulator
    .handle_call_result(
      "auth-ack",
      &json!({
        "idTagInfo": { "status": "Accepted" }
      }),
    )
    .expect("authorization acknowledgement should apply");

  assert!(simulator.pending.is_none());
  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .is_some()
  );
  assert!(
    simulator
      .queue
      .iter()
      .any(|call| call.action == "StartTransaction")
  );
}

#[test]
fn remote_start_authorization_invalid_profile_does_not_start_v1_6() {
  let mut simulator = simulator_for_tests();
  let invalid_profile = json!({
    "chargingProfileId": 1,
    "chargingProfilePurpose": "TxProfile",
    "chargingProfileKind": "Absolute",
    "chargingSchedule": {
      "chargingRateUnit": "A",
      "chargingSchedulePeriod": [{ "startPeriod": 0 }]
    }
  });
  assert!(simulator.enqueue_call(
    "Authorize",
    json!({}),
    PendingContext::RemoteStartAuthorizeV1_6 {
      connector: 1,
      id_token: "TOKEN".to_string(),
      charging_profile: Some(invalid_profile),
    },
  ));
  let authorize_call = simulator.queue.pop_front().expect("queued authorize");
  simulator.pending = Some(PendingCall {
    message_id: "auth-invalid-profile".to_string(),
    sent_at: Instant::now(),
    call: authorize_call,
  });

  simulator
    .handle_call_result(
      "auth-invalid-profile",
      &json!({
        "idTagInfo": { "status": "Accepted" }
      }),
    )
    .expect("authorization acknowledgement should apply");

  assert!(simulator.pending.is_none());
  assert!(simulator.queue.is_empty());
  assert!(simulator.charging_profiles.is_empty());
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
}

#[test]
fn remote_start_authorization_concurrent_tx_does_not_start_v1_6() {
  let mut simulator = simulator_for_tests();
  simulator
    .enqueue_remote_start_authorize_v1_6(1, "TOKEN".to_string(), None)
    .expect("authorize should enqueue");
  let authorize_call = simulator.queue.pop_front().expect("queued authorize");
  simulator.pending = Some(PendingCall {
    message_id: "auth-concurrent".to_string(),
    sent_at: Instant::now(),
    call: authorize_call,
  });

  simulator
    .handle_call_result(
      "auth-concurrent",
      &json!({
        "idTagInfo": { "status": "ConcurrentTx" }
      }),
    )
    .expect("authorization acknowledgement should apply");

  assert!(simulator.pending.is_none());
  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .is_none()
  );
  assert!(
    !simulator
      .queue
      .iter()
      .any(|call| call.action == "StartTransaction")
  );
}

#[test]
fn start_transaction_v1_6_rejection_rolls_back_local_state() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should enqueue");
  let start_call = simulator
    .queue
    .iter()
    .find(|call| call.action == "StartTransaction")
    .expect("queued start")
    .clone();
  simulator.queue.clear();
  simulator.pending = Some(PendingCall {
    message_id: "start-rejected".to_string(),
    sent_at: Instant::now(),
    call: start_call,
  });

  simulator
    .handle_call_result(
      "start-rejected",
      &json!({
        "idTagInfo": { "status": "Rejected" }
      }),
    )
    .expect("start rejection should apply");

  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|state| state.transaction.as_ref())
      .is_none()
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|state| state.status.display()),
    Some("Available")
  );
  let status_payload = queued_payload(&simulator, "StatusNotification");
  assert_eq!(status_payload["status"], json!("Available"));
}

#[test]
fn transaction_event_start_error_rolls_back_v2_x_local_state() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start should enqueue");
    let start_call = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          call.context,
          PendingContext::TxEvent {
            event_type: TxEventType::Started,
            ..
          }
        )
      })
      .expect("queued start event")
      .clone();
    simulator.queue.clear();
    simulator.pending = Some(PendingCall {
      message_id: "event-error".to_string(),
      sent_at: Instant::now(),
      call: start_call,
    });

    simulator
      .handle_call_error("event-error", "InternalError", "boom")
      .expect("start error should roll back");

    assert!(simulator.pending.is_none());
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .is_none()
    );
    let status_payload = queued_payload(&simulator, "StatusNotification");
    assert_eq!(status_payload["connectorStatus"], json!("Available"));
  });
}

#[test]
fn transaction_event_rejection_deauthorizes_v2_x_transaction() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start should enqueue");
    let start_call = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          call.context,
          PendingContext::TxEvent {
            event_type: TxEventType::Started,
            ..
          }
        )
      })
      .expect("queued start event")
      .clone();
    simulator.queue.clear();
    simulator.pending = Some(PendingCall {
      message_id: "event-rejected".to_string(),
      sent_at: Instant::now(),
      call: start_call,
    });

    simulator
      .handle_call_result(
        "event-rejected",
        &json!({
          "idTokenInfo": { "status": "Blocked" }
        }),
      )
      .expect("start rejection should deauthorize");

    assert!(simulator.pending.is_none());
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|state| state.status.display()),
      Some("Finishing")
    );
    let payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(payload["eventType"], json!("Ended"));
    assert_eq!(payload["triggerReason"], json!("Deauthorized"));
    assert_eq!(
      payload["transactionInfo"]["stoppedReason"],
      json!("DeAuthorized")
    );
  });
}

#[test]
fn v2_x_multi_connector_transactions_progress_independently() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN-1".to_string(), false, None, true)
      .expect("first start should succeed");
    simulator
      .start_transaction(2, "TOKEN-2".to_string(), false, None, true)
      .expect("second start should succeed");
    simulator.queue.clear();

    simulator.set_meter(1, 1200).expect("set meter");
    simulator
      .send_meter(1, true)
      .expect("meter update should enqueue");
    let update_payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(update_payload["eventType"], json!("Updated"));
    assert_eq!(update_payload["evse"]["id"], json!(1));
    simulator.queue.clear();

    simulator
      .stop_transaction(1, Some("Local"), false, true)
      .expect("first stop should enqueue");
    let first_end_context = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          call.context,
          PendingContext::TxEvent {
            connector: 1,
            event_type: TxEventType::Ended,
            ..
          }
        )
      })
      .map(|call| call.context.clone())
      .expect("first end context");
    simulator
      .apply_call_result_context(&first_end_context, &json!({}))
      .expect("first end ack should apply");
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|state| state.transaction.as_ref())
        .is_none()
    );
    assert!(
      simulator
        .connectors
        .get(&2)
        .and_then(|state| state.transaction.as_ref())
        .is_some()
    );
    assert_eq!(
      simulator
        .connectors
        .get(&2)
        .map(|state| state.status.display()),
      Some("Occupied")
    );

    simulator.queue.clear();
    simulator
      .stop_transaction(2, Some("Local"), false, true)
      .expect("second stop should enqueue");
    let second_end_context = simulator
      .queue
      .iter()
      .find(|call| {
        matches!(
          call.context,
          PendingContext::TxEvent {
            connector: 2,
            event_type: TxEventType::Ended,
            ..
          }
        )
      })
      .map(|call| call.context.clone())
      .expect("second end context");
    simulator
      .apply_call_result_context(&second_end_context, &json!({}))
      .expect("second end ack should apply");
    assert!(
      simulator
        .connectors
        .values()
        .all(|state| state.transaction.is_none())
    );
  });
}

#[test]
fn stop_transaction_v2_x_queues_ended_transaction_event() {
  for_each_v2_x_simulator(|protocol, mut simulator| {
    let schema =
      schema_path(v2_x_schema_dir(protocol), "TransactionEventRequest.json");
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .expect("start should succeed");
    simulator.queue.clear();

    simulator
      .stop_transaction(1, Some("Local"), false, true)
      .expect("stop should succeed");

    let payload = queued_payload(&simulator, "TransactionEvent");
    assert_eq!(payload["eventType"], json!("Ended"));
    assert_schema_valid(&schema, &payload);
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|connector| connector.transaction.as_ref())
        .is_some()
    );

    let context = simulator
      .queue
      .iter()
      .find(|call| call.action == "TransactionEvent")
      .map(|call| call.context.clone())
      .expect("transaction event context");
    simulator
      .apply_call_result_context(&context, &json!({}))
      .expect("apply acknowledgement");
    assert!(
      simulator
        .connectors
        .get(&1)
        .and_then(|connector| connector.transaction.as_ref())
        .is_none()
    );
  });
}
