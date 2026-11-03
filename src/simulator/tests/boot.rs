use std::time::Instant;

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
