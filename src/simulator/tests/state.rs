use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

fn connection_config_for_state_tests(
  protocol: OcppVersion,
) -> SimulatorConnectionConfig {
  SimulatorConnectionConfig {
    ocpp_configs: std::collections::BTreeMap::new(),
    config_warnings: Vec::new(),
    profile: None,
    ws_url: "ws://localhost:9000/ocpp".to_string(),
    cp_id: "CP-TEST".to_string(),
    append_cp_id: false,
    connectors: 2,
    protocol,
    vendor: "ocppsim".to_string(),
    model: "test".to_string(),
    firmware: "0.0.0".to_string(),
    trace_frames: false,
    strict: false,
    request_timeout: Duration::from_secs(30),
    heartbeat_seconds: Some(10),
    outbound_queue_limit: 1_000,
    security_event_limit: 1_000,
    security_profile: None,
    basic_auth_password: None,
    ca_cert_path: None,
    client_cert_path: None,
    client_key_path: None,
  }
}

#[tokio::test]
async fn profile_reload_synchronizes_an_overridden_heartbeat() {
  for startup_interval in [None, Some(10)] {
    let mut simulator = simulator_for_tests();
    let mut config = connection_config_for_state_tests(OcppVersion::V1_6);
    config.heartbeat_seconds = startup_interval;
    simulator.apply_connection_config(config.clone());
    simulator
      .apply_call_result_context(
        &PendingContext::Boot,
        &json!({
          "status": "Accepted", "currentTime": now_timestamp(), "interval": 25
        }),
      )
      .unwrap();
    assert_eq!(simulator.heartbeat.as_ref().unwrap().seconds, 25);
    simulator.handle_disconnect("test disconnect");
    simulator.apply_connection_config(config);
    assert_eq!(
      simulator.heartbeat.as_ref().map(|task| task.seconds),
      startup_interval
    );
    let entry = &simulator.configuration[&ConfigurationKey::HeartbeatInterval];
    assert_eq!(entry.value, startup_interval.unwrap_or(30).to_string());
    assert_eq!(
      entry.origin,
      if startup_interval.is_some() {
        ConfigurationOrigin::Local
      } else {
        ConfigurationOrigin::Default
      }
    );
    simulator.stop_heartbeat();
  }
}

#[tokio::test]
async fn profile_reload_preserves_a_manual_pause_for_unchanged_settings() {
  let mut simulator = simulator_for_tests();
  simulator.start_local_heartbeat(20).unwrap();
  simulator.stop_heartbeat();
  simulator.apply_connection_config(connection_config_for_state_tests(
    OcppVersion::V1_6,
  ));
  assert!(simulator.heartbeat.is_none());
  assert_eq!(
    simulator.configuration[&ConfigurationKey::HeartbeatInterval].value,
    "10"
  );
}

#[test]
fn profile_reload_refreshes_derived_organization_defaults() {
  let mut simulator = simulator_for_tests();
  assert!(simulator.set_local_configuration("CpoName", "").is_err());
  let mut config = connection_config_for_state_tests(OcppVersion::V1_6);
  for vendor in ["First vendor", "Second vendor"] {
    config.vendor = vendor.to_string();
    simulator.apply_connection_config(config.clone());
    assert_eq!(simulator.security.cpo_name, vendor);
    assert!(simulator.security.cpo_name_is_derived);
    for key in [
      ConfigurationKey::CpoName,
      ConfigurationKey::OrganizationName,
    ] {
      assert_eq!(simulator.configuration[&key].value, vendor);
      assert_eq!(
        simulator.configuration[&key].origin,
        ConfigurationOrigin::Local
      );
    }
  }
}

#[test]
fn profile_reload_preserves_explicit_organization_names() {
  for origin in [ConfigurationOrigin::Local, ConfigurationOrigin::Csms] {
    for key in [
      ConfigurationKey::CpoName,
      ConfigurationKey::OrganizationName,
    ] {
      for value in ["ocppsim", "Explicit organization"] {
        let mut simulator = simulator_for_tests();
        assert_eq!(
          simulator.set_configuration_value(key, value, origin),
          ResponseStatus::Accepted
        );
        let mut config = connection_config_for_state_tests(OcppVersion::V1_6);
        config.vendor = "New vendor".to_string();
        simulator.apply_connection_config(config);
        assert_eq!(simulator.security.cpo_name, value);
        assert!(!simulator.security.cpo_name_is_derived);
        for alias in [
          ConfigurationKey::CpoName,
          ConfigurationKey::OrganizationName,
        ] {
          assert_eq!(simulator.configuration[&alias].value, value);
          assert_eq!(simulator.configuration[&alias].origin, origin);
        }
      }
    }
  }
}

#[tokio::test]
async fn connection_settings_refresh_only_affected_configuration_origins() {
  let mut simulator = simulator_for_tests();
  for (key, value) in [
    ("HeartbeatInterval", "25"),
    ("AuthorizeRemoteTxRequests", "false"),
    ("CpoName", "CSMS organization"),
    ("AuthorizationKey", "0123456789abcdef0123456789abcdef"),
  ] {
    assert_eq!(
      simulator
        .change_configuration_v1_6(&json!({ "key": key, "value": value }))
        .expect("change configuration"),
      ResponseStatus::Accepted
    );
  }
  let preserved = [
    ConfigurationKey::AuthorizeRemoteTxRequests,
    ConfigurationKey::CpoName,
    ConfigurationKey::OrganizationName,
  ]
  .map(|key| (key, simulator.configuration[&key].clone()));
  for (key, entry) in &preserved {
    assert_eq!(entry.origin, ConfigurationOrigin::Csms, "{key:?}");
  }

  let mut config = connection_config_for_state_tests(OcppVersion::V2_0_1);
  config.heartbeat_seconds = Some(25);
  config.security_profile = Some(1);
  config.basic_auth_password = Some("local-password".to_string());
  simulator.apply_connection_config(config.clone());
  for key in [
    ConfigurationKey::HeartbeatInterval,
    ConfigurationKey::NumberOfConnectors,
    ConfigurationKey::SecurityProfile,
    ConfigurationKey::AuthorizationKey,
    ConfigurationKey::BasicAuthPassword,
  ] {
    assert_eq!(
      simulator.configuration[&key].origin,
      ConfigurationOrigin::Local
    );
  }
  assert_eq!(
    simulator.configuration[&ConfigurationKey::HeartbeatInterval].value,
    "25"
  );

  config.heartbeat_seconds = None;
  config.security_profile = None;
  config.basic_auth_password = None;
  simulator.apply_connection_config(config);
  for key in [
    ConfigurationKey::HeartbeatInterval,
    ConfigurationKey::SecurityProfile,
    ConfigurationKey::AuthorizationKey,
    ConfigurationKey::BasicAuthPassword,
  ] {
    assert_eq!(
      simulator.configuration[&key].origin,
      ConfigurationOrigin::Default
    );
  }
  for (key, entry) in preserved {
    assert_eq!(simulator.configuration[&key], entry);
  }
}

#[tokio::test]
async fn connect_without_target_warns_and_stays_offline() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.ws_url = None;
  simulator.config.cp_id = None;
  let (_cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

  let outcome = simulator
    .handle_offline_command(
      SimulatorCommand::Connect { config: None },
      &mut cmd_rx,
    )
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
fn meter_queries_show_local_readings_online_and_offline() {
  for connected in [false, true] {
    let (mut simulator, mut ui_rx) =
      simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
    simulator.connected = connected;
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator
      .set_meter_value(1, MeterReading::energy(1234))
      .unwrap();
    drain_log_messages(&mut ui_rx);

    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector: None,
        },
        connected,
      )
      .unwrap();
    assert_eq!(
      drain_log_messages(&mut ui_rx),
      ["Connector 1 meter=1234Wh", "Connector 2 meter=0Wh",]
    );

    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector: Some(2),
        },
        connected,
      )
      .unwrap();
    assert_eq!(drain_log_messages(&mut ui_rx), ["Connector 2 meter=0Wh"]);

    simulator
      .set_meter_value(1, MeterReading::energy(5678))
      .unwrap();
    drain_log_messages(&mut ui_rx);
    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector: Some(1),
        },
        connected,
      )
      .unwrap();
    assert_eq!(drain_log_messages(&mut ui_rx), ["Connector 1 meter=5678Wh"]);
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 5678);
    assert_eq!(simulator.connectors[&2].meter_values.energy_wh(), 0);
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn meter_adjustments_use_the_latest_reading_online_and_offline() {
  for connected in [false, true] {
    let mut simulator = simulator_for_tests();
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    for (update, expected) in [
      (MeterUpdate::Set(MeterReading::energy(1000)), 1000),
      (
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (250).into(),
        ),
        1250,
      ),
      (
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (-100).into(),
        ),
        1150,
      ),
      (
        MeterUpdate::Adjust(Measurand::EnergyActiveImportRegister, (0).into()),
        1150,
      ),
      (MeterUpdate::Set(MeterReading::energy(200)), 200),
      (
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (-250).into(),
        ),
        -50,
      ),
      (MeterUpdate::Set(MeterReading::energy(0)), 0),
      (
        MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (i64::MIN).into(),
        ),
        i64::MIN,
      ),
    ] {
      simulator
        .handle_common_command(
          SimulatorCommand::UpdateMeterValue {
            connector: 1,
            update,
          },
          connected,
        )
        .unwrap();
      assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), expected);
      assert_eq!(simulator.connectors[&2].meter_values.energy_wh(), 0);
      assert!(simulator.queue.is_empty());
    }
  }
}

#[test]
fn invalid_meter_adjustments_do_not_mutate_or_send() {
  let mut simulator = simulator_for_tests();
  for (current, delta) in [(i64::MAX, 1), (i64::MIN, -1)] {
    simulator
      .set_meter_value(1, MeterReading::energy(current))
      .unwrap();
    for command in [
      SimulatorCommand::UpdateMeterValue {
        connector: 1,
        update: MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (delta).into(),
        ),
      },
      SimulatorCommand::SendMeterValue {
        measurand: Measurand::EnergyActiveImportRegister,
        connector: 1,
        update: Some(MeterUpdate::Adjust(
          Measurand::EnergyActiveImportRegister,
          (delta).into(),
        )),
      },
    ] {
      let error = simulator.handle_common_command(command, true).unwrap_err();
      assert!(error.to_string().contains("out of range"));
      assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), current);
      assert!(simulator.queue.is_empty());
    }
  }
  assert!(
    simulator
      .update_meter_value(
        3,
        MeterUpdate::Adjust(Measurand::EnergyActiveImportRegister, (1).into())
      )
      .is_err()
  );
}

#[test]
fn send_meter_adjustments_update_even_when_sending_is_unavailable() {
  for connected in [false, true] {
    let mut simulator = simulator_for_tests();
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator
      .set_meter_value(1, MeterReading::energy(1000))
      .unwrap();
    simulator
      .handle_common_command(
        SimulatorCommand::SendMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector: 1,
          update: Some(MeterUpdate::Adjust(
            Measurand::EnergyActiveImportRegister,
            (250).into(),
          )),
        },
        connected,
      )
      .unwrap();
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1250);
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn send_meter_adjustments_send_the_updated_reading_in_each_protocol() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::energy(1000))
      .unwrap();
    for active in [false, true] {
      if active {
        simulator
          .start_transaction(1, "TOKEN".to_string(), false, None, true)
          .unwrap();
      }
      simulator.queue.clear();
      simulator
        .handle_common_command(
          SimulatorCommand::SendMeterValue {
            measurand: Measurand::EnergyActiveImportRegister,
            connector: 1,
            update: Some(MeterUpdate::Adjust(
              Measurand::EnergyActiveImportRegister,
              (250).into(),
            )),
          },
          true,
        )
        .unwrap();
      let action = if active && protocol != OcppVersion::V1_6 {
        "TransactionEvent"
      } else {
        "MeterValues"
      };
      let payload = queued_payload(&simulator, action);
      let expected = if active { 1500 } else { 1250 };
      let value = &payload["meterValue"][0]["sampledValue"][0]["value"];
      if protocol == OcppVersion::V1_6 {
        assert_eq!(value, &json!(expected.to_string()));
      } else {
        assert_eq!(value, &json!(expected));
      }
      assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), expected);
    }
  }
}

#[test]
fn meter_query_rejects_unknown_connectors() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  for connector in [0, 3] {
    let error = simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          measurand: Measurand::EnergyActiveImportRegister,
          connector: Some(connector),
        },
        false,
      )
      .unwrap_err();
    assert_eq!(
      error.to_string(),
      format!("Connector {connector} does not exist.")
    );
    assert_eq!(drain_log_messages(&mut ui_rx), Vec::<String>::new());
    assert!(simulator.queue.is_empty());
  }
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
fn outbound_queue_limit_drops_new_calls() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.outbound_queue_limit = 1;

  simulator.enqueue_heartbeat();
  simulator
    .enqueue_authorize("TOKEN".to_string())
    .expect("authorize should validate");

  assert_eq!(simulator.queue.len(), 1);
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("Heartbeat")
  );
  let messages = drain_log_messages(&mut ui_rx);
  assert!(messages.iter().any(|message| {
    message.contains("Outbound OCPP queue limit 1 reached")
      && message.contains("Authorize")
  }));
}

#[test]
fn boot_state_is_unchanged_when_queue_is_full() {
  let mut simulator = simulator_for_tests();
  simulator.config.outbound_queue_limit = 1;
  simulator.boot_registration_status = BootRegistrationStatus::Accepted;
  simulator.enqueue_heartbeat();

  let error = simulator
    .enqueue_boot_with_registration_state()
    .expect_err("boot should not enqueue into a full queue");

  assert!(error.to_string().contains("BootNotification"));
  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Accepted
  );
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("Heartbeat")
  );
}

#[test]
fn reconnect_boot_depends_on_registration_and_payload_changes() {
  let mut simulator = simulator_for_tests();
  assert!(
    simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );

  simulator
    .enqueue_boot_notification()
    .expect("boot should validate");
  simulator.queue.clear();
  simulator.boot_registration_status = BootRegistrationStatus::Accepted;

  assert!(
    !simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );

  simulator.boot_registration_status = BootRegistrationStatus::Pending;
  assert!(
    simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );

  simulator.boot_registration_status = BootRegistrationStatus::Accepted;
  simulator.config.firmware = "1.2.3".to_string();
  assert!(
    simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );
}

#[test]
fn connection_scope_change_resets_boot_registration_state() {
  let mut simulator = simulator_for_tests();
  simulator
    .enqueue_boot_notification()
    .expect("boot should validate");
  simulator.queue.clear();
  simulator.boot_registration_status = BootRegistrationStatus::Accepted;

  assert!(
    !simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );

  simulator.apply_connection_config(connection_config_for_state_tests(
    OcppVersion::V1_6,
  ));

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Accepted
  );
  assert!(
    !simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );

  let mut next_config = connection_config_for_state_tests(OcppVersion::V1_6);
  next_config.cp_id = "CP-OTHER".to_string();
  simulator.apply_connection_config(next_config);

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Rejected
  );
  assert!(simulator.last_boot_notification_payload.is_none());
  assert!(
    simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );
}

#[test]
fn protocol_security_scope_changes_reset_boot_registration_state() {
  let mut simulator = simulator_for_tests();
  simulator
    .enqueue_boot_notification()
    .expect("boot should validate");
  simulator.queue.clear();
  simulator.boot_registration_status = BootRegistrationStatus::Accepted;

  let mut next_config = connection_config_for_state_tests(OcppVersion::V2_0_1);
  next_config.security_profile = Some(2);
  next_config.basic_auth_password = Some("abcdefghijklmnop".to_string());

  simulator.apply_connection_config(next_config);

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Rejected
  );
  assert!(simulator.last_boot_notification_payload.is_none());
  assert!(
    simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );
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
fn cancel_reservation_preserves_inoperative_connector_status() {
  let mut simulator = simulator_for_tests();
  let reserve_payload = json!({
    "connectorId": 1,
    "expiryDate": now_timestamp(),
    "idTag": "TOKEN",
    "reservationId": 42
  });
  let inoperative_payload = json!({
    "connectorId": 1,
    "type": "Inoperative"
  });
  let cancel_payload = json!({
    "reservationId": 42
  });

  assert_eq!(
    simulator
      .reserve_now_v1_6(&reserve_payload)
      .expect("reserve should succeed"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .change_availability_v1_6(&inoperative_payload)
      .expect("inoperative should succeed"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .cancel_reservation_v1_6(&cancel_payload)
      .expect("cancel should succeed"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Unavailable")
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
fn reserve_now_reports_faulted_connector_status() {
  let mut simulator = simulator_for_tests();
  simulator.connector_mut(1).expect("connector").status =
    ConnectorStatus::Faulted;
  assert_eq!(
    simulator
      .reserve_now_v1_6(&json!({
        "connectorId": 1,
        "expiryDate": now_timestamp(),
        "idTag": "TOKEN",
        "reservationId": 42
      }))
      .expect("reserve faulted connector"),
    ResponseStatus::Faulted
  );
  assert!(simulator.queue.is_empty());

  for_each_v2_x_simulator(|_, mut simulator| {
    simulator.connector_mut(1).expect("connector").status =
      ConnectorStatus::Faulted;
    assert_eq!(
      simulator
        .reserve_now_v2_x(&json!({
          "evseId": 1,
          "expiryDateTime": now_timestamp(),
          "id": 42,
          "idToken": {
            "idToken": "TOKEN",
            "type": "Central"
          }
        }))
        .expect("reserve faulted evse"),
      ResponseStatus::Faulted
    );
    assert!(simulator.queue.is_empty());
  });
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
fn operative_availability_preserves_active_transaction_status() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("start should succeed");
  simulator.queue.clear();

  let status = simulator
    .change_availability_v1_6(&json!({
      "connectorId": 1,
      "type": "Operative"
    }))
    .expect("change availability");

  assert_eq!(status, ResponseStatus::Accepted);
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Charging")
  );
  assert!(simulator.queue.is_empty());
}

#[test]
fn operative_availability_cancels_scheduled_inoperative_change() {
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
      .expect("schedule availability"),
    ResponseStatus::Scheduled
  );

  assert_eq!(
    simulator
      .change_availability_v1_6(&json!({
        "connectorId": 1,
        "type": "Operative"
      }))
      .expect("cancel scheduled availability"),
    ResponseStatus::Accepted
  );
  simulator
    .stop_transaction(1, Some("Local"), false, false)
    .expect("stop should succeed");
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Finishing")
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
fn change_availability_v1_6_requires_connector_id() {
  let mut simulator = simulator_for_tests();
  let result = simulator.change_availability_v1_6(&json!({
    "type": "Inoperative"
  }));

  assert!(result.is_err());
  assert!(simulator.queue.is_empty());
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| { connector.status == ConnectorStatus::Available })
  );
}

#[test]
fn accepted_boot_result_enqueues_v1_6_status_for_charge_point_and_connectors() {
  let mut simulator = simulator_for_tests();
  simulator.boot_retry_after = Some(Instant::now() + Duration::from_secs(10));

  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Accepted",
        "currentTime": now_timestamp()
      }),
    )
    .expect("boot result");

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Accepted
  );
  assert!(simulator.boot_retry_after.is_none());
  assert_eq!(queued_status_connector_ids(&simulator), vec![0, 1, 2]);
}

#[test]
fn rejected_boot_result_does_not_enqueue_initial_status_notifications() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;

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

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Rejected
  );
  assert!(simulator.boot_retry_after.is_some());
  assert!(
    !simulator
      .should_enqueue_boot_on_connect()
      .expect("boot check")
  );
  assert!(simulator.queue.is_empty());

  simulator
    .handle_common_command(SimulatorCommand::Boot, true)
    .expect("boot command");

  assert!(simulator.queue.is_empty());
  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Rejected
  );
}

#[test]
fn pending_boot_result_keeps_v1_6_registration_pending() {
  let mut simulator = simulator_for_tests();

  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Pending",
        "currentTime": now_timestamp(),
        "interval": 10
      }),
    )
    .expect("boot result");

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::Pending
  );
  assert!(simulator.boot_retry_after.is_some());
  assert!(simulator.queue.is_empty());
}

#[test]
fn expired_boot_retry_interval_enqueues_v1_6_boot_retry() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator.boot_registration_status = BootRegistrationStatus::Pending;
  simulator.boot_retry_after = Some(
    Instant::now()
      .checked_sub(Duration::from_secs(1))
      .expect("retry instant"),
  );

  simulator.check_boot_retry();

  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::AwaitingResponse
  );
  assert!(simulator.boot_retry_after.is_none());
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("BootNotification")
  );
}

#[test]
fn trigger_message_boot_bypasses_v1_6_retry_interval() {
  let mut simulator = simulator_for_tests();
  simulator.boot_registration_status = BootRegistrationStatus::Pending;
  simulator.boot_retry_after = Some(Instant::now() + Duration::from_secs(10));

  let status = simulator
    .trigger_message_v1_6_standard(
      crate::ocpp::TriggerMessage_V1_6::BootNotification,
      None,
    )
    .expect("trigger boot");

  assert_eq!(status, ResponseStatus::Accepted);
  assert_eq!(
    simulator.boot_registration_status,
    BootRegistrationStatus::AwaitingResponse
  );
  assert!(simulator.boot_retry_after.is_none());
  assert_eq!(
    simulator.queue.front().map(|call| call.action.as_str()),
    Some("BootNotification")
  );
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
fn v1_6_commands_wait_for_boot_acceptance() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;
  simulator.boot_registration_status = BootRegistrationStatus::Pending;

  simulator
    .handle_common_command(
      SimulatorCommand::StartTransaction {
        connector: 1,
        id_token: "TOKEN".to_string(),
      },
      true,
    )
    .expect("start command should be handled");

  assert!(simulator.queue.is_empty());
  assert!(
    simulator
      .connectors
      .values()
      .all(|connector| connector.transaction.is_none())
  );

  simulator
    .apply_call_result_context(
      &PendingContext::Boot,
      &json!({
        "status": "Accepted",
        "currentTime": now_timestamp()
      }),
    )
    .expect("boot result");
  simulator.queue.clear();

  simulator
    .handle_common_command(
      SimulatorCommand::StartTransaction {
        connector: 1,
        id_token: "TOKEN".to_string(),
      },
      true,
    )
    .expect("start command should be handled after boot");

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
fn v2_x_commands_wait_for_boot_acceptance() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator.connected = true;
    simulator.boot_registration_status = BootRegistrationStatus::Pending;

    simulator
      .handle_common_command(
        SimulatorCommand::StartTransaction {
          connector: 1,
          id_token: "TOKEN".to_string(),
        },
        true,
      )
      .expect("start command should be handled");

    assert!(simulator.queue.is_empty());
    assert!(
      simulator
        .connectors
        .values()
        .all(|connector| connector.transaction.is_none())
    );

    simulator
      .apply_call_result_context(
        &PendingContext::Boot,
        &json!({
          "status": "Accepted",
          "currentTime": now_timestamp()
        }),
      )
      .expect("boot result");
    simulator.queue.clear();

    simulator
      .handle_common_command(
        SimulatorCommand::StartTransaction {
          connector: 1,
          id_token: "TOKEN".to_string(),
        },
        true,
      )
      .expect("start command should be handled after boot");

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
        .any(|call| call.action == "TransactionEvent")
    );
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
    .map(|call| call.payload["connectorId"].as_u64().expect("connector ID"))
    .collect()
}

fn queued_status_evse_ids(simulator: &Simulator) -> Vec<u64> {
  simulator
    .queue
    .iter()
    .filter(|call| call.action == "StatusNotification")
    .map(|call| call.payload["evseId"].as_u64().expect("evse ID"))
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
  simulator
    .enqueue_data_transfer("ocppsim", Some("Message"), Some("hello"))
    .expect("data transfer should validate");
  simulator
    .enqueue_authorize("TOKEN".to_string())
    .expect("authorize should validate");

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
