use std::time::Instant;

use serde_json::json;

use super::*;

#[test]
fn reserve_and_cancel_updates_connector_status() {
  let mut simulator = simulator_for_tests();
  let reserve_payload = json!({
    "connectorId": 1,
    "expiryDate": now_timestamp(),
    "idTag": "TOKEN",
    "reservationId": 42
  });
  let cancel_payload = json!({
    "reservationId": 42
  });

  let reserve_status = simulator
    .reserve_now_v1_6(&reserve_payload)
    .expect("reserve should succeed");
  assert_eq!(reserve_status, ResponseStatus::Accepted);
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Reserved")
  );

  let cancel_status = simulator
    .cancel_reservation_v1_6(&cancel_payload)
    .expect("cancel should succeed");
  assert_eq!(cancel_status, ResponseStatus::Accepted);
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Available")
  );
}

#[test]
fn duplicate_reservation_id_is_rejected() {
  let mut simulator = simulator_for_tests();
  let first = json!({
    "connectorId": 1,
    "expiryDate": now_timestamp(),
    "idTag": "TOKEN",
    "reservationId": 42
  });
  let duplicate = json!({
    "connectorId": 2,
    "expiryDate": now_timestamp(),
    "idTag": "TOKEN",
    "reservationId": 42
  });

  assert_eq!(
    simulator.reserve_now_v1_6(&first).expect("reserve first"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .reserve_now_v1_6(&duplicate)
      .expect("reserve duplicate"),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Reserved")
  );
  assert_eq!(
    simulator
      .connectors
      .get(&2)
      .map(|item| item.status.display()),
    Some("Available")
  );
}

#[test]
fn scheduled_availability_applies_after_stop() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("start should succeed");
  let status = simulator
    .change_availability_v1_6(&json!({
      "connectorId": 1,
      "type": "Inoperative"
    }))
    .expect("change availability");

  assert_eq!(status, ResponseStatus::Scheduled);
  simulator
    .stop_transaction(1, Some("Local"), false, false)
    .expect("stop should succeed");
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Unavailable")
  );
}

#[test]
fn change_availability_v1_6_connector_zero_updates_all_connectors() {
  let mut simulator = simulator_for_tests();
  let status = simulator
    .change_availability_v1_6(&json!({
      "connectorId": 0,
      "type": "Inoperative"
    }))
    .expect("change availability");

  assert_eq!(status, ResponseStatus::Accepted);
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| { connector.status == ConnectorStatus::Unavailable })
  );
  assert_eq!(
    simulator
      .queue
      .iter()
      .filter(|call| call.action == "StatusNotification")
      .count(),
    2
  );
}

#[test]
fn change_availability_v2_x_updates_target_evse() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let payload = json!({
      "operationalStatus": "Inoperative",
      "evse": { "id": 2 }
    });
    let status = simulator
      .change_availability_v2_x(&payload)
      .expect("request should succeed");

    assert_eq!(status, ResponseStatus::Accepted);
    assert_eq!(
      simulator
        .connectors
        .get(&2)
        .map(|item| item.status.display()),
      Some("Unavailable")
    );
  });
}

#[test]
fn reserve_and_cancel_v2_x_auto_selects_available_evse() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let reserve_status = simulator
      .reserve_now_v2_x(&json!({
        "id": 77,
        "expiryDateTime": now_timestamp(),
        "idToken": {
          "idToken": "TOKEN",
          "type": "Central"
        }
      }))
      .expect("reserve should succeed");

    assert_eq!(reserve_status, ResponseStatus::Accepted);
    assert_eq!(simulator.reservations.get(&77), Some(&1));
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("Reserved")
    );

    let cancel_status = simulator
      .cancel_reservation_v2_x(&json!({ "reservationId": 77 }))
      .expect("cancel should succeed");
    assert_eq!(cancel_status, ResponseStatus::Accepted);
    assert!(!simulator.reservations.contains_key(&77));
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("Available")
    );
  });
}

#[test]
fn send_local_list_v2_x_updates_version() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let payload = json!({ "versionNumber": 7, "updateType": "Full" });
    let status = simulator
      .send_local_list_v2_x(&payload)
      .expect("send local list should parse");

    assert_eq!(status, ResponseStatus::Accepted);
    assert_eq!(simulator.local_auth_list_version, 7);
  });
}

#[test]
fn disconnect_clears_pending_queue_and_status() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator.enqueue_heartbeat();
  let call = simulator.queue.front().expect("queued call").clone();
  simulator.pending = Some(PendingCall {
    message_id: "pending".to_string(),
    sent_at: Instant::now(),
    call,
  });

  simulator.handle_disconnect("lost connection");

  assert!(!simulator.connected);
  assert!(simulator.pending.is_none());
  assert!(simulator.queue.is_empty());
}

#[test]
fn repeated_pending_timeouts_unblock_queued_calls() {
  let mut simulator = simulator_for_tests();
  simulator.config.request_timeout = std::time::Duration::from_millis(1);
  simulator.enqueue_heartbeat();
  simulator.enqueue_data_transfer("ocppsim", Some("Message"), Some("hello"));
  simulator.enqueue_authorize("TOKEN".to_string());

  let mut timed_out_actions = Vec::new();
  while let Some(call) = simulator.queue.pop_front() {
    timed_out_actions.push(call.action.clone());
    simulator.pending = Some(PendingCall {
      message_id: format!("timeout-{}", timed_out_actions.len()),
      sent_at: Instant::now()
        .checked_sub(std::time::Duration::from_secs(1))
        .expect("past instant"),
      call,
    });

    simulator.check_pending_timeout();
    assert!(simulator.pending.is_none());
  }

  assert_eq!(
    timed_out_actions,
    vec![
      "Heartbeat".to_string(),
      "DataTransfer".to_string(),
      "Authorize".to_string(),
    ]
  );
}

#[test]
fn start_transaction_rejects_unstartable_connectors() {
  let mut simulator = simulator_for_tests();
  simulator.connector_mut(1).expect("connector").status =
    ConnectorStatus::Unavailable;
  assert!(
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .is_err()
  );
  simulator.connector_mut(1).expect("connector").status =
    ConnectorStatus::Faulted;
  assert!(
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .is_err()
  );

  let reserve_payload = json!({
    "connectorId": 2,
    "expiryDate": now_timestamp(),
    "idTag": "TOKEN",
    "reservationId": 7
  });
  assert_eq!(
    simulator
      .reserve_now_v1_6(&reserve_payload)
      .expect("reserve connector"),
    ResponseStatus::Accepted
  );
  assert!(
    simulator
      .start_transaction(2, "TOKEN".to_string(), false, None, false)
      .is_err()
  );
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );
}
