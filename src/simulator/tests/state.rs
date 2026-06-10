use std::time::Instant;

use serde_json::json;

use super::*;

#[tokio::test]
async fn connect_without_target_warns_and_stays_offline() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.ws_url = None;
  simulator.config.cp_id = None;

  let outcome = simulator
    .handle_offline_command(SimulatorCommand::Connect { config: None })
    .await
    .expect("offline connect should be handled");

  assert!(matches!(outcome, super::super::OfflineOutcome::Continue));
  assert!(!simulator.connected);

  let mut events = Vec::new();
  while let Ok(event) = ui_rx.try_recv() {
    events.push(event);
  }
  assert!(events.iter().any(|event| {
    matches!(
      event,
      UiEvent::Log {
        level: UiLogLevel::Warn,
        message,
      } if message.contains("No connection target configured")
    )
  }));
}

#[test]
fn emit_snapshot_redacts_connection_url_secrets() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.ws_url =
    Some("wss://user:secret@example.test/ocpp?token=SECRET".to_string());

  simulator.emit_snapshot();

  let snapshot = loop {
    match ui_rx.try_recv().expect("snapshot event should be emitted") {
      UiEvent::Snapshot(snapshot) => break snapshot,
      UiEvent::RuntimeState(_) | UiEvent::Log { .. } => {}
    }
  };
  assert_eq!(
    snapshot.connection_url,
    "wss://<redacted>@example.test/ocpp?token=<redacted>"
  );
}

#[test]
fn get_diagnostics_redacts_location_url_secrets_in_logs() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  let payload = json!({
    "location": "https://user:secret@example.test/logs?token=SECRET"
  });

  let _ = simulator
    .get_diagnostics_v1_6(&payload)
    .expect("diagnostics response should be built");

  let messages = drain_log_messages(&mut ui_rx);
  assert!(
    messages
      .iter()
      .all(|message| !message.contains("secret") && !message.contains("SECRET"))
  );
  assert!(messages.iter().any(|message| {
    message.contains("https://<redacted>@example.test/logs?token=<redacted>")
  }));
}

#[test]
fn trace_frame_redacts_url_secrets_in_string_values() {
  let frame = json!([
    2,
    "m1",
    "GetDiagnostics",
    { "location": "https://user:secret@example.test/logs?token=SECRET" }
  ])
  .to_string();

  let trace = sanitized_trace_frame_text(&frame);

  assert!(!trace.contains("secret"));
  assert!(!trace.contains("SECRET"));
  assert!(
    trace.contains("https://<redacted>@example.test/logs?token=<redacted>")
  );
}

#[test]
fn trace_frame_redacts_token_aliases() {
  let frame = json!([
    3,
    "m1",
    {
      "idTagInfo": {
        "parentIdTag": "PARENT-TOKEN",
        "status": "Accepted"
      },
      "idToken": {
        "additionalInfo": [
          {
            "additionalIdToken": "ADDITIONAL-TOKEN",
            "type": "PaymentBrand"
          }
        ],
        "idToken": "PRIMARY-TOKEN",
        "type": "Central"
      }
    }
  ])
  .to_string();

  let trace = sanitized_trace_frame_text(&frame);

  for token in ["PARENT-TOKEN", "ADDITIONAL-TOKEN", "PRIMARY-TOKEN"] {
    assert!(!trace.contains(token), "{token} appeared in {trace}");
  }
  assert!(trace.contains("\"parentIdTag\":\"<redacted>\""));
  assert!(trace.contains("\"additionalIdToken\":\"<redacted>\""));
  assert!(trace.contains("\"idToken\":\"<redacted>\""));
}

#[test]
fn trace_frame_redacts_network_profile_secrets() {
  let frame = json!([
    2,
    "m1",
    "SetNetworkProfile",
    {
      "configurationSlot": 1,
      "connectionData": {
        "apn": {
          "apn": "internet",
          "apnAuthentication": "CHAP",
          "apnPassword": "APN-PASSWORD",
          "simPin": 9876
        },
        "basicAuthPassword": "BASIC-PASSWORD",
        "messageTimeout": 30,
        "ocppCsmsUrl": "wss://user:secret@example.test/ocpp?token=SECRET",
        "ocppInterface": "Wired0",
        "ocppTransport": "JSON",
        "ocppVersion": "OCPP20",
        "securityProfile": 2,
        "vpn": {
          "group": "group",
          "key": "VPN-KEY",
          "password": "VPN-PASSWORD",
          "server": "vpn.example.test",
          "type": "IKEv2",
          "user": "user"
        }
      }
    }
  ])
  .to_string();

  let trace = sanitized_trace_frame_text(&frame);

  for secret in [
    "APN-PASSWORD",
    "BASIC-PASSWORD",
    "VPN-KEY",
    "VPN-PASSWORD",
    "9876",
    "secret",
    "SECRET",
  ] {
    assert!(!trace.contains(secret), "{secret} appeared in {trace}");
  }
  assert!(trace.contains("\"apnPassword\":\"<redacted>\""));
  assert!(trace.contains("\"basicAuthPassword\":\"<redacted>\""));
  assert!(trace.contains("\"key\":\"<redacted>\""));
  assert!(trace.contains("\"password\":\"<redacted>\""));
  assert!(trace.contains("\"simPin\":\"<redacted>\""));
  assert!(
    trace.contains("wss://<redacted>@example.test/ocpp?token=<redacted>")
  );
}

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
fn accepted_boot_result_enqueues_v1_6_status_for_charge_point_and_connectors() {
  let mut simulator = simulator_for_tests();

  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Accepted",
        "currentTime": now_timestamp()
      }),
    )
    .expect("boot result");

  assert_eq!(queued_status_connector_ids(&simulator), vec![0, 1, 2]);
}

#[test]
fn rejected_boot_result_does_not_enqueue_initial_status_notifications() {
  let mut simulator = simulator_for_tests();

  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Rejected",
        "currentTime": now_timestamp(),
        "interval": 10
      }),
    )
    .expect("boot result");

  assert!(simulator.queue.is_empty());
}

#[test]
fn accepted_boot_result_enqueues_v2_x_connector_statuses_only() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .apply_call_result_context(
        &PendingContext::Boot,
        &json!({
          "status": "Accepted",
          "currentTime": now_timestamp()
        }),
      )
      .expect("boot result");

    assert_eq!(queued_status_evse_ids(&simulator), vec![1, 2]);
  });
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

fn queued_status_connector_ids(simulator: &Simulator) -> Vec<u64> {
  simulator
    .queue
    .iter()
    .filter(|call| call.action == "StatusNotification")
    .map(|call| call.payload["connectorId"].as_u64().expect("connector id"))
    .collect()
}

fn queued_status_evse_ids(simulator: &Simulator) -> Vec<u64> {
  simulator
    .queue
    .iter()
    .filter(|call| call.action == "StatusNotification")
    .map(|call| call.payload["evseId"].as_u64().expect("evse id"))
    .collect()
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
