use std::time::Instant;

use crate::ocpp::{OcppFrame, build_call};

use super::ws_lifecycle::{in_memory_ws_pair, read_ocpp_frame};
use super::*;

#[test]
fn failed_boot_requests_retry_after_backoff() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for failure in ["timeout", "call-error", "invalid-response"] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      simulator.connected = true;
      simulator
        .enqueue_boot_with_registration_state()
        .expect("queue initial boot");
      let call = simulator.queue.pop_front().expect("initial boot");
      simulator.pending = Some(PendingCall {
        message_id: "initial-boot".to_string(),
        sent_at: Instant::now()
          .checked_sub(simulator.config.request_timeout)
          .expect("request timeout fits past instant"),
        call,
      });

      match failure {
        "timeout" => simulator.check_pending_timeout(),
        "call-error" => simulator
          .handle_call_error("initial-boot", "InternalError", "Try later")
          .expect("handle failed boot"),
        "invalid-response" => simulator.reject_call_result("initial-boot"),
        _ => unreachable!(),
      }

      assert!(simulator.pending.is_none(), "{protocol:?}: {failure}");
      assert!(!simulator.post_boot_ocpp_requests_allowed());
      assert!(simulator.boot_retry_remaining().is_some());
      simulator.check_boot_retry();
      assert!(simulator.queue.is_empty(), "must respect backoff");

      simulator.boot_retry_after = Some(Instant::now());
      simulator.check_boot_retry();
      assert_eq!(simulator.queue.len(), 1);
      assert_eq!(simulator.queue[0].action, "BootNotification");
      assert_eq!(
        simulator.boot_registration_status,
        BootRegistrationStatus::AwaitingResponse
      );
      simulator.check_boot_retry();
      assert_eq!(simulator.queue.len(), 1, "only one retry may be queued");
    }
  }
}

#[test]
fn nonaccepted_boot_respects_response_interval() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for status in ["Pending", "Rejected"] {
      for seconds in [60, 172_800] {
        let mut simulator = simulator_for_tests_with_protocol(protocol);
        simulator.connected = true;
        simulator
          .apply_call_result_context(
            &PendingContext::Boot,
            &json!({
              "currentTime": "2026-11-03T00:00:00Z",
              "status": status,
              "interval": seconds,
            }),
          )
          .expect("apply boot response");

        let remaining = simulator.boot_retry_remaining().expect("backoff");
        assert!(remaining > Duration::from_secs(seconds - 1));
        simulator.check_boot_retry();
        simulator
          .handle_common_command(SimulatorCommand::Boot, true)
          .expect("manual boot respects interval");
        assert!(simulator.queue.is_empty());

        simulator.boot_retry_after = Some(Instant::now());
        simulator.check_boot_retry();
        assert_eq!(simulator.queue.len(), 1);
        assert_eq!(simulator.queue[0].action, "BootNotification");
      }
    }
  }
}

#[test]
fn zero_boot_interval_uses_backoff_and_trigger_bypasses_it() {
  for protocol in v2_x_protocols() {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator.connected = true;
    simulator
      .apply_call_result_context(
        &PendingContext::Boot,
        &json!({
          "currentTime": "2026-11-03T00:00:00Z",
          "status": "Pending",
          "interval": 0,
        }),
      )
      .expect("apply boot response");
    assert!(simulator.boot_retry_remaining().is_some());
    simulator.check_boot_retry();
    assert!(simulator.queue.is_empty());

    simulator
      .trigger_message_v2_x(
        crate::ocpp::TriggerMessage_V2_X::BootNotification,
        None,
      )
      .expect("CSMS can request a boot before the interval expires");
    assert_eq!(simulator.queue.len(), 1);
    assert_eq!(simulator.queue[0].action, "BootNotification");
    assert!(simulator.boot_retry_after.is_none());
    assert_eq!(
      simulator.boot_registration_status,
      BootRegistrationStatus::AwaitingResponse
    );
  }
}

#[tokio::test]
async fn pending_registration_sends_only_explicitly_triggered_reports() {
  for (protocol, action) in [
    (OcppVersion::V1_6, "TriggerMessage"),
    (OcppVersion::V1_6, "ExtendedTriggerMessage"),
    (OcppVersion::V2_0_1, "TriggerMessage"),
    (OcppVersion::V2_1, "TriggerMessage"),
  ] {
    let (mut write, _read, _server_write, mut server_read) =
      in_memory_ws_pair().await;
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator.connected = true;
    simulator.config.strict = true;
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator.enqueue_heartbeat();
    let payload = if protocol == OcppVersion::V1_6 {
      json!({"requestedMessage": "StatusNotification", "connectorId": 1})
    } else {
      json!({
        "requestedMessage": "StatusNotification",
        "evse": {"id": 1, "connectorId": 1},
      })
    };
    simulator
      .handle_ws_text(build_call("trigger", action, &payload), &mut write)
      .await
      .expect("handle CSMS trigger during Pending");
    let OcppFrame::CallResult { payload, .. } =
      read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected trigger response");
    };
    assert_eq!(payload["status"], "Accepted");

    simulator
      .try_send_next(&mut write)
      .await
      .expect("prioritize the triggered report over the local heartbeat");
    simulator
      .try_send_next(&mut write)
      .await
      .expect("send the report");
    let OcppFrame::Call {
      action, message_id, ..
    } = read_ocpp_frame(&mut server_read).await
    else {
      panic!("expected triggered report");
    };
    assert_eq!(action, "StatusNotification");
    simulator
      .handle_call_result(&message_id, &json!({}))
      .expect("acknowledge report");
    simulator
      .try_send_next(&mut write)
      .await
      .expect("keep the local heartbeat gated");
    assert!(simulator.pending.is_none());
    assert_eq!(simulator.queue.len(), 1);
    assert_eq!(simulator.queue[0].action, "Heartbeat");
    assert!(!simulator.queue[0].triggered_by_csms);
  }
}

#[test]
fn triggered_transaction_events_still_obey_registration_state() {
  for protocol in v2_x_protocols() {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("local transaction");
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator
      .trigger_message_v2_x(
        crate::ocpp::TriggerMessage_V2_X::TransactionEvent,
        Some(1),
      )
      .expect("trigger transaction event");
    assert!(simulator.queued_call_allowed(&simulator.queue[0]));

    for status in [
      BootRegistrationStatus::Rejected,
      BootRegistrationStatus::AwaitingResponse,
    ] {
      simulator.boot_registration_status = status;
      assert!(!simulator.queued_call_allowed(&simulator.queue[0]));
    }
  }
}
