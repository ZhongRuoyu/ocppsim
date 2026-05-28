use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::*;

static TEMP_SECURITY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn certificate_install_list_and_delete_v1_6() {
  let mut simulator = simulator_for_tests();
  let install_status = simulator
    .install_certificate_from_payload(&json!({
      "certificateType": "CentralSystemRootCertificate",
      "certificate": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
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
        "certificate": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
      }))
      .expect("install certificate");
    assert_eq!(install_status, ResponseStatus::Accepted);

    let listed = simulator
      .get_installed_certificate_ids_v2_x(&json!({
        "certificateType": "CSMSRootCertificate"
      }))
      .expect("certificate ids");
    assert_eq!(listed["status"], ResponseStatus::Accepted.as_str());
    assert_eq!(
      listed["certificateHashDataChain"].as_array().map(Vec::len),
      Some(1)
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
        "signingCertificate": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----",
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
fn basic_auth_password_is_write_only() {
  let mut simulator = simulator_for_tests();
  let password = "0123456789abcdef0123456789abcdef";

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": password
    })),
    ResponseStatus::Accepted
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
      ResponseStatus::WriteOnly.as_str()
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

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": "not-a-hex-password"
    })),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": "0123456789abcdef"
    })),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": "0123456789abcdef0123456789abcdef012345678"
    })),
    ResponseStatus::Rejected
  );
}

#[test]
fn security_profile_v1_6_enforces_upgrade_prerequisites() {
  let mut simulator = simulator_for_tests();

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "1"
    })),
    ResponseStatus::Rejected
  );

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": "0123456789abcdef0123456789abcdef"
    })),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "1"
    })),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "1"
    })),
    ResponseStatus::Rejected
  );

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "2"
    })),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator
      .install_certificate_from_payload(&json!({
        "certificateType": "CentralSystemRootCertificate",
        "certificate": "-----BEGIN CERTIFICATE-----ROOT-----END CERTIFICATE-----"
      }))
      .expect("install root"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "2"
    })),
    ResponseStatus::Accepted
  );

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "1"
    })),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AllowSecurityProfileDowngrade",
      "value": "true"
    })),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "1"
    })),
    ResponseStatus::Rejected
  );

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "3"
    })),
    ResponseStatus::Rejected
  );
  simulator.config.client_cert_path = Some(PathBuf::from("cp.pem"));
  simulator.config.client_key_path = Some(PathBuf::from("cp-key.pem"));
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "3"
    })),
    ResponseStatus::Accepted
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
fn security_configuration_changes_request_reconnect_when_connected() {
  let mut simulator = simulator_for_tests();
  simulator.connected = true;

  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AuthorizationKey",
      "value": "0123456789abcdef0123456789abcdef"
    })),
    ResponseStatus::Accepted
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
        "certificate": "-----BEGIN CERTIFICATE-----ROOT-----END CERTIFICATE-----"
      }))
      .expect("install root"),
    ResponseStatus::Accepted
  );
  simulator.connected = true;
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "SecurityProfile",
      "value": "2"
    })),
    ResponseStatus::Accepted
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
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "AdditionalRootCertificateCheck",
      "value": "true"
    })),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator.change_configuration_v1_6(&json!({
      "key": "CertificateSignedMaxChainSize",
      "value": "10000"
    })),
    ResponseStatus::Rejected
  );
}

#[test]
fn certificate_chain_size_rejects_values_over_whitepaper_maximum() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let rejected = simulator
      .set_variables_v2_x(&json!({
        "setVariableData": [
          set_variable_data("MaxCertificateChainSize", "10001")
        ]
      }))
      .expect("set variables");
    assert_eq!(
      rejected["setVariableResult"][0]["attributeStatus"],
      ResponseStatus::Rejected.as_str()
    );

    let accepted = simulator
      .set_variables_v2_x(&json!({
        "setVariableData": [
          set_variable_data("MaxCertificateChainSize", "10000")
        ]
      }))
      .expect("set variables");
    assert_eq!(
      accepted["setVariableResult"][0]["attributeStatus"],
      ResponseStatus::Accepted.as_str()
    );
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
fn signed_firmware_rejects_unsupported_file_transfer_schemes() {
  let mut simulator = simulator_for_tests();
  let status = simulator
    .signed_update_firmware_v1_6(&json!({
      "requestId": 9,
      "firmware": {
        "location": "ftp://csms.example/firmware.bin",
        "retrieveDateTime": now_timestamp(),
        "signingCertificate": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----",
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
fn certificate_signed_v2_1_correlates_request_ids() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);

  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "certificateChain": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
      }))
      .expect("missing request id"),
    ResponseStatus::Rejected
  );

  simulator.enqueue_sign_certificate(Some("ChargingStationCertificate"));
  let request_id = simulator.security.pending_signing_request_ids[0];
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id + 1,
        "certificateChain": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
      }))
      .expect("unknown request id"),
    ResponseStatus::Rejected
  );
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id,
        "certificateChain": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
      }))
      .expect("matching request id"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .certificate_signed_v2_x(&json!({
        "requestId": request_id,
        "certificateChain": "-----BEGIN CERTIFICATE-----TEST-----END CERTIFICATE-----"
      }))
      .expect("duplicate request id"),
    ResponseStatus::Rejected
  );
}

#[tokio::test]
async fn secure_connection_setup_failure_records_local_security_event() {
  let path = write_temp_security_file("not a certificate");
  let mut simulator = simulator_for_tests();
  simulator.config.ws_url = "wss://127.0.0.1:9/ocpp".to_string();
  simulator.config.ca_cert_path = Some(path.clone());
  simulator.security.security_profile = Some(2);
  simulator.security.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());

  let result = simulator.connect().await;
  assert!(result.is_err());
  assert!(
    simulator
      .security
      .events
      .iter()
      .any(|event| { event.event_type == "InvalidCentralSystemCertificate" })
  );

  let _ = fs::remove_file(path);
}

#[test]
fn security_profile_transport_validation_and_basic_auth() {
  let mut simulator = simulator_for_tests();
  simulator.security.security_profile = Some(1);
  simulator.security.basic_auth_password =
    Some("0123456789abcdef0123456789abcdef".to_string());

  let ws_url = url::Url::parse("ws://localhost:9000/ocpp").expect("ws url");
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
