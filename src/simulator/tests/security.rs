use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::simulator::SimulatorConnectionConfig;

use super::*;

static TEMP_SECURITY_COUNTER: AtomicU64 = AtomicU64::new(0);
const TEST_CERTIFICATE: &str =
  "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----";
const ROOT_CERTIFICATE: &str =
  "-----BEGIN CERTIFICATE-----ROOT-----END CERTIFICATE-----";

fn change_configuration_v1_6(
  simulator: &mut Simulator,
  payload: &Value,
) -> ResponseStatus {
  simulator
    .change_configuration_v1_6(payload)
    .expect("change configuration should parse")
}

fn assert_change_configuration_status(
  simulator: &mut Simulator,
  key: &str,
  value: &str,
  expected: ResponseStatus,
) {
  assert_eq!(
    change_configuration_v1_6(
      simulator,
      &json!({
        "key": key,
        "value": value
      }),
    ),
    expected
  );
}

#[test]
fn certificate_install_list_and_delete_v1_6() {
  let mut simulator = simulator_for_tests();
  let install_status = simulator
    .install_certificate_from_payload(&json!({
      "certificateType": "CentralSystemRootCertificate",
      "certificate": TEST_CERTIFICATE
    }))
    .expect("install certificate");

  assert_eq!(install_status, ResponseStatus::Accepted);
  let listed = simulator
    .get_installed_certificate_ids_v1_6(&json!({
      "certificateType": "CentralSystemRootCertificate"
    }))
    .expect("certificate ids");
  assert_eq!(listed["status"], ResponseStatus::Accepted.as_str());
  assert_eq!(
    listed["certificateHashData"].as_array().map(Vec::len),
    Some(1)
  );
  assert_schema_valid(
    "schemas/1.6/GetInstalledCertificateIdsResponse.json",
    &listed,
  );

  let delete_status = simulator
    .delete_certificate_from_payload(&json!({
      "certificateHashData": listed["certificateHashData"][0].clone()
    }))
    .expect("delete certificate");
  assert_eq!(delete_status, ResponseStatus::Accepted);

  let empty = simulator
    .get_installed_certificate_ids_v1_6(&json!({
      "certificateType": "CentralSystemRootCertificate"
    }))
    .expect("empty certificate ids");
  assert_eq!(empty["status"], ResponseStatus::NotFound.as_str());
}

#[test]
fn certificate_install_list_and_delete_v2_x() {
  for_each_v2_x_simulator(|protocol, mut simulator| {
    let install_status = simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CSMSRootCertificate",
        "certificate": TEST_CERTIFICATE
      }))
      .expect("install certificate");
    assert_eq!(install_status, ResponseStatus::Accepted);

    let listed = simulator
      .get_installed_certificate_ids_v2_x(&json!({
        "certificateType": ["CSMSRootCertificate"]
      }))
      .expect("certificate ids");
    assert_eq!(listed["status"], ResponseStatus::Accepted.as_str());
    assert_eq!(
      listed["certificateHashDataChain"].as_array().map(Vec::len),
      Some(1)
    );

    let all = simulator
      .get_installed_certificate_ids_v2_x(&json!({}))
      .expect("all certificate ids");
    assert_eq!(all["status"], ResponseStatus::Accepted.as_str());
    assert_eq!(
      all["certificateHashDataChain"].as_array().map(Vec::len),
      Some(1)
    );
    assert!(
      simulator
        .get_installed_certificate_ids_v2_x(&json!({
          "certificateType": "CSMSRootCertificate"
        }))
        .is_err()
    );

    assert_schema_valid(
      &schema_path(
        v2_x_schema_dir(protocol),
        "GetInstalledCertificateIdsResponse.json",
      ),
      &listed,
    );

    let delete_status = simulator
      .delete_certificate_from_payload(&json!({
        "certificateHashData": listed["certificateHashDataChain"][0]
          ["certificateHashData"].clone()
      }))
      .expect("delete certificate");
    assert_eq!(delete_status, ResponseStatus::Accepted);

    let empty = simulator
      .get_installed_certificate_ids_v2_x(&json!({}))
      .expect("empty certificate ids");
    assert_eq!(empty["status"], ResponseStatus::NotFound.as_str());
  });
}

#[test]
fn signed_firmware_invalid_signature_records_security_event() {
  let mut simulator = simulator_for_tests();
  let status = simulator
    .signed_update_firmware_v1_6(&json!({
      "requestId": 9,
      "firmware": {
        "location": "https://csms.example/firmware.bin",
        "retrieveDateTime": now_timestamp(),
        "signingCertificate": TEST_CERTIFICATE,
        "signature": "invalid-signature"
      }
    }))
    .expect("signed firmware request");

  assert_eq!(status, ResponseStatus::Accepted);
  assert_eq!(simulator.security.events.len(), 1);
  assert_eq!(
    simulator.security.events[0].event_type,
    "InvalidFirmwareSignature"
  );
  assert_eq!(
    queued_payload(&simulator, "SignedFirmwareStatusNotification")["status"],
    ResponseStatus::InvalidSignature.as_str()
  );
}

#[test]
fn update_firmware_invalid_certificate_rejects_immediately() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let status = simulator
      .update_firmware_v2_x(&json!({
        "requestId": 9,
        "firmware": {
          "location": "https://csms.example/firmware.bin",
          "retrieveDateTime": now_timestamp(),
          "signingCertificate": "invalid-certificate"
        }
      }))
      .expect("update firmware request");

    assert_eq!(status, ResponseStatus::InvalidCertificate);
    assert_eq!(simulator.security.events.len(), 1);
    assert_eq!(
      simulator.security.events[0].event_type,
      "InvalidFirmwareSigningCertificate"
    );
    assert!(simulator.queue.is_empty());
  });
}

#[test]
fn basic_auth_password_is_write_only() {
  let mut simulator = simulator_for_tests();
  let password = "0123456789abcdef0123456789abcdef";

  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    password,
    ResponseStatus::Accepted,
  );
  assert_eq!(
    simulator.security.basic_auth_password.as_deref(),
    Some(password)
  );

  let response = crate::simulator::payloads::to_value(
    &simulator.configuration_response_v1_6(&json!({
      "key": ["AuthorizationKey"]
    })),
  );
  assert!(response["configurationKey"][0].get("value").is_none());

  for_each_v2_x_simulator(|_, mut simulator| {
    let response = simulator
      .set_variables_v2_x(&json!({
        "setVariableData": [
          set_variable_data("BasicAuthPassword", password)
        ]
      }))
      .expect("set variables");
    assert_eq!(
      response["setVariableResult"][0]["attributeStatus"],
      ResponseStatus::Accepted.as_str()
    );

    let response = simulator
      .get_variables_v2_x(&json!({
        "getVariableData": [
          get_variable_data("BasicAuthPassword")
        ]
      }))
      .expect("get variables");
    assert_eq!(
      response["getVariableResult"][0]["attributeStatus"],
      ResponseStatus::Rejected.as_str()
    );
    assert!(
      response["getVariableResult"][0]
        .get("attributeValue")
        .is_none()
    );
  });
}

#[test]
fn basic_auth_password_rejects_non_hex_and_short_and_long_values() {
  let mut simulator = simulator_for_tests();

  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "not-a-hex-password",
    ResponseStatus::Rejected,
  );
  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "0123456789abcdef",
    ResponseStatus::Rejected,
  );
  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "0123456789abcdef0123456789abcdef012345678",
    ResponseStatus::Rejected,
  );
}

#[test]
fn basic_auth_password_v2_x_uses_password_string_rules() {
  let mut v2_0_1 = simulator_for_tests_with_protocol(OcppVersion::V2_0_1);
  let response = v2_0_1
    .set_variables_v2_x(&json!({
      "setVariableData": [
        set_variable_data("BasicAuthPassword", "not-a-hex-passwd")
      ]
    }))
    .expect("set variables");
  assert_eq!(
    response["setVariableResult"][0]["attributeStatus"],
    ResponseStatus::Accepted.as_str()
  );

  let response = v2_0_1
    .set_variables_v2_x(&json!({
      "setVariableData": [
        set_variable_data("BasicAuthPassword", "abcdefghijklmnop!")
      ]
    }))
    .expect("set variables");
  assert_eq!(
    response["setVariableResult"][0]["attributeStatus"],
    ResponseStatus::Rejected.as_str()
  );

  let mut v2_1 = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  let response = v2_1
    .set_variables_v2_x(&json!({
      "setVariableData": [
        set_variable_data("BasicAuthPassword", "abcdefghijklmnop!")
      ]
    }))
    .expect("set variables");
  assert_eq!(
    response["setVariableResult"][0]["attributeStatus"],
    ResponseStatus::Accepted.as_str()
  );
}

#[test]
fn security_profile_v1_6_enforces_upgrade_prerequisites() {
  let mut simulator = simulator_for_tests();

  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Rejected,
  );

  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "0123456789abcdef0123456789abcdef",
    ResponseStatus::Accepted,
  );
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Accepted,
  );
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Rejected,
  );

  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "2",
    ResponseStatus::Rejected,
  );
  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CentralSystemRootCertificate",
        "certificate": ROOT_CERTIFICATE
      }))
      .expect("install root"),
    ResponseStatus::Accepted
  );
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "2",
    ResponseStatus::Accepted,
  );

  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Rejected,
  );
  assert_change_configuration_status(
    &mut simulator,
    "AllowSecurityProfileDowngrade",
    "true",
    ResponseStatus::Accepted,
  );
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Rejected,
  );

  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "3",
    ResponseStatus::Rejected,
  );
  simulator.config.client_cert_path = Some(PathBuf::from("cp.pem"));
  simulator.config.client_key_path = Some(PathBuf::from("cp-key.pem"));
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "3",
    ResponseStatus::Accepted,
  );
}

#[test]
fn security_profile_rejects_ambiguous_basic_auth_identity() {
  let mut config = simulator_test_config(OcppVersion::V1_6);
  config.cp_id = Some("CP:TEST".to_string());
  let (ui_tx, _ui_rx) = tokio::sync::mpsc::unbounded_channel();
  let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::unbounded_channel();
  let mut simulator = Simulator::new(config, ui_tx, cmd_tx);

  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "0123456789abcdef0123456789abcdef",
    ResponseStatus::Accepted,
  );
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "1",
    ResponseStatus::Rejected,
  );
  assert_eq!(simulator.security.security_profile, None);

  simulator.security.security_profile = Some(1);
  assert!(
    simulator
      .validate_connection_security(
        &url::Url::parse("ws://localhost:9000/ocpp").expect("ws url"),
      )
      .is_err()
  );
}

#[test]
fn offline_security_events_replay_when_connection_becomes_available() {
  let mut simulator = simulator_for_tests();

  simulator.record_security_event(
    "InvalidFirmwareSignature",
    Some("recorded while offline".to_string()),
  );
  assert!(simulator.queue.is_empty());

  simulator.connected = true;
  simulator.enqueue_pending_security_event_notifications();

  let payload = queued_payload(&simulator, "SecurityEventNotification");
  assert_eq!(payload["type"], "InvalidFirmwareSignature");
  assert_eq!(payload["techInfo"], "recorded while offline");

  let event_id = match &simulator.queue.front().expect("queued event").context {
    PendingContext::SecurityEventNotification { event_id } => *event_id,
    other => panic!("unexpected pending context: {other:?}"),
  };
  simulator.mark_security_event_notification_sent(event_id);
  simulator.queue.clear();
  simulator.enqueue_pending_security_event_notifications();
  assert!(simulator.queue.is_empty());
}

#[test]
fn queued_security_events_replay_after_disconnect_queue_clear() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;

  simulator.record_security_event(
    "ReconfigurationOfSecurityParameters",
    Some("queued before reconnect".to_string()),
  );
  assert_eq!(
    queued_payload(&simulator, "SecurityEventNotification")["techInfo"],
    "queued before reconnect"
  );

  simulator.handle_disconnect("test disconnect");
  assert!(simulator.queue.is_empty());

  simulator.connected = true;
  simulator.enqueue_pending_security_event_notifications();
  assert_eq!(
    queued_payload(&simulator, "SecurityEventNotification")["techInfo"],
    "queued before reconnect"
  );
}

#[test]
fn security_event_limit_drops_sent_events_first() {
  let mut simulator = simulator_for_tests();
  simulator.config.security_event_limit = 2;

  simulator.record_security_event("First", Some("sent".to_string()));
  simulator.record_security_event("Second", Some("pending".to_string()));
  simulator.mark_security_event_notification_sent(1);
  simulator.record_security_event("Third", Some("new".to_string()));

  let events = simulator
    .security
    .events
    .iter()
    .map(|event| event.event_type.as_str())
    .collect::<Vec<_>>();
  assert_eq!(events, vec!["Second", "Third"]);
}

#[test]
fn security_event_limit_drops_oldest_unsent_event_when_full() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.config.security_event_limit = 1;

  simulator.record_security_event("First", Some("old".to_string()));
  simulator.record_security_event("Second", Some("new".to_string()));

  assert_eq!(simulator.security.events.len(), 1);
  assert_eq!(simulator.security.events[0].event_type, "Second");
  let messages = drain_log_messages(&mut ui_rx);
  assert!(messages.iter().any(|message| {
    message.contains("Security event limit 1 reached")
      && message.contains("dropped 1 unsent")
  }));
}

#[test]
fn security_configuration_changes_request_reconnect_when_connected() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;

  assert_change_configuration_status(
    &mut simulator,
    "AuthorizationKey",
    "0123456789abcdef0123456789abcdef",
    ResponseStatus::Accepted,
  );
  let reconnect = simulator
    .security
    .pending_reconnect
    .expect("password reconnect");
  assert_eq!(
    reconnect.fallback_security_profile,
    SecurityProfileFallback::None
  );

  simulator.connected = false;
  simulator.security.pending_reconnect = None;
  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CentralSystemRootCertificate",
        "certificate": ROOT_CERTIFICATE
      }))
      .expect("install root"),
    ResponseStatus::Accepted
  );
  simulator.connected = true;
  assert_change_configuration_status(
    &mut simulator,
    "SecurityProfile",
    "2",
    ResponseStatus::Accepted,
  );
  let reconnect = simulator
    .security
    .pending_reconnect
    .expect("profile reconnect");
  assert_eq!(
    reconnect.fallback_security_profile,
    SecurityProfileFallback::Restore(None)
  );
}

#[test]
fn whitepaper_read_only_keys_reject_v1_6_configuration_writes() {
  let mut simulator = simulator_for_tests();
  let response = crate::simulator::payloads::to_value(
    &simulator.configuration_response_v1_6(&json!({
      "key": [
        "AdditionalRootCertificateCheck",
        "CertificateSignedMaxChainSize"
      ]
    })),
  );
  assert!(
    response["configurationKey"]
      .as_array()
      .is_some_and(|items| {
        items.iter().all(|item| item["readonly"] == json!(true))
      })
  );
  assert_change_configuration_status(
    &mut simulator,
    "AdditionalRootCertificateCheck",
    "true",
    ResponseStatus::Rejected,
  );
  assert_change_configuration_status(
    &mut simulator,
    "CertificateSignedMaxChainSize",
    "10000",
    ResponseStatus::Rejected,
  );
}

#[test]
fn protocol_switch_refreshes_security_variable_mutability() {
  let mut simulator = simulator_for_tests();
  for key in always_read_only_security_keys() {
    assert!(configuration_is_read_only(&simulator, key));
  }
  assert!(!configuration_is_read_only(
    &simulator,
    ConfigurationKey::SecurityProfile
  ));

  simulator.apply_connection_config(connection_config_for_protocol(
    OcppVersion::V2_0_1,
  ));
  for key in always_read_only_security_keys() {
    assert!(configuration_is_read_only(&simulator, key));
  }
  assert!(configuration_is_read_only(
    &simulator,
    ConfigurationKey::SecurityProfile
  ));

  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_0_1);
  for key in always_read_only_security_keys() {
    assert!(configuration_is_read_only(&simulator, key));
  }
  assert!(configuration_is_read_only(
    &simulator,
    ConfigurationKey::SecurityProfile
  ));

  simulator
    .apply_connection_config(connection_config_for_protocol(OcppVersion::V1_6));
  for key in always_read_only_security_keys() {
    assert!(configuration_is_read_only(&simulator, key));
  }
  assert!(!configuration_is_read_only(
    &simulator,
    ConfigurationKey::SecurityProfile
  ));
}

#[test]
fn v2_x_security_ctrlr_read_only_variables_reject_writes() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let response = simulator
      .set_variables_v2_x(&json!({
        "setVariableData": [
          set_variable_data("SecurityProfile", "2"),
          set_variable_data("AdditionalRootCertificateCheck", "true"),
          set_variable_data("CertificateSignedMaxChainSize", "5600"),
          set_variable_data("MaxCertificateChainSize", "5600")
        ]
      }))
      .expect("set variables");
    let results = response["setVariableResult"]
      .as_array()
      .expect("set variable results");
    assert!(results.iter().all(|result| {
      result["attributeStatus"] == ResponseStatus::Rejected.as_str()
    }));
  });
}

#[test]
fn certificate_store_full_rejects_install_certificate() {
  let mut simulator = simulator_for_tests();
  simulator.security.certificate_store_max_length = 1;

  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CentralSystemRootCertificate",
        "certificate": "-----BEGIN CERTIFICATE-----A-----END CERTIFICATE-----"
      }))
      .expect("install first"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "ManufacturerRootCertificate",
        "certificate": "-----BEGIN CERTIFICATE-----B-----END CERTIFICATE-----"
      }))
      .expect("install second"),
    ResponseStatus::Rejected
  );
}

#[test]
fn install_certificate_rejects_oversized_payload_without_strict_mode() {
  let mut simulator = simulator_for_tests();
  let oversized_certificate = "A".repeat(5_501);

  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CentralSystemRootCertificate",
        "certificate": oversized_certificate
      }))
      .expect("install oversized"),
    ResponseStatus::Rejected
  );
}

#[test]
fn additional_root_check_keeps_central_root_fallback_only() {
  let mut simulator = simulator_for_tests();
  simulator.security.additional_root_certificate_check = true;

  for certificate in ["CENTRAL-A", "CENTRAL-B", "CENTRAL-C"] {
    assert_eq!(
      simulator
        .install_certificate_from_payload(&json!({
          "certificateType": "CentralSystemRootCertificate",
          "certificate": certificate
        }))
        .expect("install central root"),
      ResponseStatus::Accepted
    );
  }
  let central_roots = simulator
    .get_installed_certificate_ids_v1_6(&json!({
      "certificateType": "CentralSystemRootCertificate"
    }))
    .expect("central roots");
  assert_eq!(
    central_roots["certificateHashData"]
      .as_array()
      .map(Vec::len),
    Some(2)
  );

  for certificate in ["MANUFACTURER-A", "MANUFACTURER-B"] {
    assert_eq!(
      simulator
        .install_certificate_from_payload(&json!({
          "certificateType": "ManufacturerRootCertificate",
          "certificate": certificate
        }))
        .expect("install manufacturer root"),
      ResponseStatus::Accepted
    );
  }
  let manufacturer_roots = simulator
    .get_installed_certificate_ids_v1_6(&json!({
      "certificateType": "ManufacturerRootCertificate"
    }))
    .expect("manufacturer roots");
  assert_eq!(
    manufacturer_roots["certificateHashData"]
      .as_array()
      .map(Vec::len),
    Some(2)
  );
}

#[test]
fn get_log_rejects_unsupported_file_transfer_schemes() {
  let mut simulator = simulator_for_tests();
  let response = simulator
    .get_log_v1_6(&json!({
      "logType": "SecurityLog",
      "requestId": 1,
      "log": { "remoteLocation": "ftp://csms.example/security.log" }
    }))
    .expect("get log");
  assert_eq!(response["status"], ResponseStatus::Rejected.as_str());
  assert!(simulator.queue.is_empty());

  for_each_v2_x_simulator(|_, mut simulator| {
    let response = simulator
      .get_log_v2_x(&json!({
        "logType": "SecurityLog",
        "requestId": 1,
        "log": { "remoteLocation": "ftp://csms.example/security.log" }
      }))
      .expect("get log");
    assert_eq!(response["status"], ResponseStatus::Rejected.as_str());
    assert!(simulator.queue.is_empty());
  });
}

#[test]
fn get_diagnostics_rejects_unsupported_file_transfer_schemes() {
  let mut simulator = simulator_for_tests();
  let response = simulator
    .get_diagnostics_v1_6(&json!({
      "location": "ftp://csms.example/diagnostics.log"
    }))
    .expect("get diagnostics");

  assert!(response.get("fileName").is_none());
  assert!(simulator.queue.is_empty());
}

#[test]
fn signed_firmware_rejects_unsupported_file_transfer_schemes() {
  let mut simulator = simulator_for_tests();
  let status = simulator
    .signed_update_firmware_v1_6(&json!({
      "requestId": 9,
      "firmware": {
        "location": "ftp://csms.example/firmware.bin",
        "retrieveDateTime": now_timestamp(),
        "signingCertificate": TEST_CERTIFICATE,
        "signature": "abcdef"
      }
    }))
    .expect("signed firmware request");
  assert_eq!(status, ResponseStatus::Rejected);
  assert!(simulator.queue.is_empty());

  for_each_v2_x_simulator(|_, mut simulator| {
    let status = simulator
      .update_firmware_v2_x(&json!({
        "requestId": 7,
        "firmware": {
          "location": "ftp://csms.example/firmware.bin",
          "retrieveDateTime": now_timestamp()
        }
      }))
      .expect("update firmware");
    assert_eq!(status, ResponseStatus::Rejected);
    assert!(simulator.queue.is_empty());
  });
}

#[test]
fn certificate_signed_v2_1_accepts_optional_request_id() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);

  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "certificateChain": TEST_CERTIFICATE
      }))
      .expect("missing request ID"),
    ResponseStatus::Accepted
  );

  simulator.enqueue_sign_certificate(Some("ChargingStationCertificate"));
  let request_id = simulator.security.pending_signing_request_ids[0];
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id + 1,
        "certificateChain": TEST_CERTIFICATE
      }))
      .expect("unknown request ID"),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id,
        "certificateChain": TEST_CERTIFICATE
      }))
      .expect("matching request ID"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id,
        "certificateChain": TEST_CERTIFICATE
      }))
      .expect("duplicate request ID"),
    ResponseStatus::Rejected
  );
}

#[test]
fn sign_certificate_v2_1_ignores_dropped_queue_entries() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  simulator.config.outbound_queue_limit = 1;

  simulator.enqueue_heartbeat();
  simulator.enqueue_sign_certificate(Some("ChargingStationCertificate"));

  assert_eq!(simulator.queue.len(), 1);
  assert_eq!(simulator.security.pending_signing_request_ids.len(), 0);
  assert_eq!(simulator.security.next_signing_request_id, 1);

  simulator.queue.clear();
  simulator.enqueue_sign_certificate(Some("ChargingStationCertificate"));

  assert_eq!(simulator.security.pending_signing_request_ids, vec![1]);
  assert_eq!(simulator.security.next_signing_request_id, 2);
  assert_eq!(
    queued_payload(&simulator, "SignCertificate")["requestId"],
    1
  );
}

#[tokio::test]
async fn local_tls_setup_failure_does_not_record_security_event() {
  let path = write_temp_security_file("not a certificate");
  let mut simulator = simulator_for_tests();
  simulator.config.ws_url = Some("wss://127.0.0.1:9/ocpp/CP-TEST".to_string());
  simulator.config.ca_cert_path = Some(path.clone());
  simulator.security.security_profile = Some(2);
  simulator.security.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());

  let result = simulator.connect().await;
  assert!(result.is_err());
  assert!(simulator.security.events.is_empty());

  let _ = fs::remove_file(path);
}

#[test]
fn secure_connection_certificate_failure_records_version_specific_event() {
  for (protocol, event_type) in [
    (OcppVersion::V1_6, "InvalidCentralSystemCertificate"),
    (OcppVersion::V2_0_1, "InvalidCsmsCertificate"),
    (OcppVersion::V2_1, "InvalidCsmsCertificate"),
  ] {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator.security.security_profile = Some(2);
    let error = rustls_websocket_error(rustls::Error::InvalidCertificate(
      rustls::CertificateError::UnknownIssuer,
    ));

    simulator.record_secure_connection_failure(&error);

    assert_eq!(simulator.security.events.len(), 1);
    assert_eq!(simulator.security.events[0].event_type, event_type);
  }
}

#[test]
fn secure_connection_tls_failures_record_specific_security_events() {
  let mut simulator = simulator_for_tests();
  simulator.security.security_profile = Some(2);
  let version_error = rustls_websocket_error(rustls::Error::PeerIncompatible(
    rustls::PeerIncompatible::ServerDoesNotSupportTls12Or13,
  ));
  let cipher_error = rustls_websocket_error(rustls::Error::PeerIncompatible(
    rustls::PeerIncompatible::NoCipherSuitesInCommon,
  ));

  simulator.record_secure_connection_failure(&version_error);
  simulator.record_secure_connection_failure(&cipher_error);

  assert_eq!(simulator.security.events.len(), 2);
  assert_eq!(simulator.security.events[0].event_type, "InvalidTLSVersion");
  assert_eq!(
    simulator.security.events[1].event_type,
    "InvalidTLSCipherSuite"
  );
}

#[test]
fn secure_connection_non_tls_failure_does_not_record_security_event() {
  let mut simulator = simulator_for_tests();
  simulator.security.security_profile = Some(2);
  let error = tokio_tungstenite::tungstenite::Error::Io(std::io::Error::new(
    std::io::ErrorKind::ConnectionRefused,
    "refused",
  ));

  simulator.record_secure_connection_failure(&error);

  assert!(simulator.security.events.is_empty());
}

#[test]
fn security_profile_transport_validation_and_basic_auth() {
  let mut simulator = simulator_for_tests();
  simulator.security.security_profile = Some(1);
  simulator.security.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());

  let ws_url =
    url::Url::parse("ws://localhost:9000/ocpp/CP-TEST").expect("ws url");
  let secure_url =
    url::Url::parse("wss://localhost:9000/ocpp").expect("wss url");
  assert!(simulator.validate_connection_security(&ws_url).is_ok());
  assert!(simulator.validate_connection_security(&secure_url).is_err());

  let header = simulator
    .basic_auth_header()
    .expect("auth header")
    .expect("auth header present");
  assert!(
    header
      .to_str()
      .expect("header string")
      .starts_with("Basic ")
  );
}

#[test]
fn basic_auth_identity_must_match_final_url_path() {
  let mut simulator = simulator_for_tests();
  simulator.security.security_profile = Some(1);
  simulator.security.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());

  let matching =
    url::Url::parse("ws://localhost:9000/ocpp/CP-TEST").expect("matching url");
  let encoded =
    url::Url::parse("ws://localhost:9000/ocpp/%43P-TEST").expect("encoded url");
  let mismatched =
    url::Url::parse("ws://localhost:9000/ocpp/OTHER").expect("mismatched url");

  assert!(simulator.validate_connection_security(&matching).is_ok());
  assert!(simulator.validate_connection_security(&encoded).is_ok());
  assert!(
    simulator
      .validate_connection_security(&mismatched)
      .expect_err("identity mismatch should fail")
      .to_string()
      .contains("Basic Auth username")
  );
}

fn write_temp_security_file(content: &str) -> PathBuf {
  let base = std::env::current_dir().expect("cwd");
  let timestamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("time")
    .as_nanos();
  let sequence = TEMP_SECURITY_COUNTER.fetch_add(1, Ordering::Relaxed);
  let pid = std::process::id();
  let path = base.join(format!(
    ".tmp-ocppsim-security-{pid}-{timestamp}-{sequence}.pem"
  ));
  fs::write(&path, content).expect("write temp file");
  path
}

fn rustls_websocket_error(
  error: rustls::Error,
) -> tokio_tungstenite::tungstenite::Error {
  tokio_tungstenite::tungstenite::Error::Tls(
    tokio_tungstenite::tungstenite::error::TlsError::Rustls(Box::new(error)),
  )
}

fn always_read_only_security_keys() -> [ConfigurationKey; 3] {
  [
    ConfigurationKey::AdditionalRootCertificateCheck,
    ConfigurationKey::CertificateSignedMaxChainSize,
    ConfigurationKey::MaxCertificateChainSize,
  ]
}

fn configuration_is_read_only(
  simulator: &Simulator,
  key: ConfigurationKey,
) -> bool {
  simulator
    .configuration
    .get(&key)
    .expect("configuration entry")
    .read_only
}

fn connection_config_for_protocol(
  protocol: OcppVersion,
) -> SimulatorConnectionConfig {
  SimulatorConnectionConfig {
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
    request_timeout: std::time::Duration::from_secs(30),
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
