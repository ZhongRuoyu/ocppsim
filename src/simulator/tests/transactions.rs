use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

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
    .stop_transaction(1, Some("Local".to_string()), false, true)
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
fn transaction_event_end_timeout_restores_v2_status() {
  let mut simulator = simulator_for_tests_v2_0_1();
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
    .stop_transaction(1, Some("Local".to_string()), false, true)
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
    .stop_transaction(1, Some("Local".to_string()), false, true)
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
fn end_ack_enqueues_scheduled_v2_status() {
  let mut simulator = simulator_for_tests_v2_0_1();
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
    .stop_transaction(1, Some("Local".to_string()), false, true)
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
      json!({
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
      json!({
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
  let mut simulator = simulator_for_tests_v2_0_1();
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
}

#[test]
fn v2_x_multi_connector_transactions_progress_independently() {
  let mut simulator = simulator_for_tests_v2_0_1();
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
    .stop_transaction(1, Some("Local".to_string()), false, true)
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
    .expect("first end acknowledgement");

  assert!(
    simulator
      .connectors
      .get(&1)
      .and_then(|connector| connector.transaction.as_ref())
      .is_none()
  );
  assert!(
    simulator
      .connectors
      .get(&2)
      .and_then(|connector| connector.transaction.as_ref())
      .is_some()
  );
  assert_eq!(
    simulator
      .connectors
      .get(&2)
      .map(|connector| connector.status.display()),
    Some("Occupied")
  );

  simulator.queue.clear();
  simulator
    .stop_transaction(2, Some("Local".to_string()), false, true)
    .expect("second stop should enqueue");
  let second_end_call = simulator
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
    .expect("second end call")
    .clone();
  simulator.queue.clear();
  simulator.pending = Some(PendingCall {
    message_id: "second-end-error".to_string(),
    sent_at: Instant::now(),
    call: second_end_call,
  });

  simulator
    .handle_call_error("second-end-error", "InternalError", "boom")
    .expect("second end error should restore transaction");
  assert!(
    simulator
      .connectors
      .get(&2)
      .and_then(|connector| connector.transaction.as_ref())
      .is_some()
  );
  assert_eq!(
    simulator
      .connectors
      .get(&2)
      .map(|connector| connector.status.display()),
    Some("Occupied")
  );
}

#[test]
fn stop_transaction_v2_0_1_queues_ended_transaction_event() {
  let mut simulator = simulator_for_tests_v2_0_1();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  simulator.queue.clear();

  simulator
    .stop_transaction(1, Some("Local".to_string()), false, true)
    .expect("stop should succeed");

  let payload = queued_payload(&simulator, "TransactionEvent");
  assert_eq!(payload["eventType"], json!("Ended"));
  assert_schema_valid("schemas/2.0.1/TransactionEventRequest.json", &payload);
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
}

#[test]
fn stop_transaction_v2_1_queues_ended_transaction_event() {
  let mut simulator = simulator_for_tests_v2_1();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .expect("start should succeed");
  simulator.queue.clear();

  simulator
    .stop_transaction(1, Some("Local".to_string()), false, true)
    .expect("stop should succeed");

  let payload = queued_payload(&simulator, "TransactionEvent");
  assert_eq!(payload["eventType"], json!("Ended"));
  assert_schema_valid("schemas/2.1/TransactionEventRequest.json", &payload);
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
}
