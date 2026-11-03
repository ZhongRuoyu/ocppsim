use serde_json::json;

use super::*;

#[test]
fn set_charging_profile_v1_6_updates_transaction_status() {
  let mut simulator = simulator_for_tests();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .expect("start should succeed");
  let suspend_payload = json!({
    "connectorId": 1,
    "csChargingProfiles": {
      "chargingSchedule": {
        "chargingSchedulePeriod": [
          { "startPeriod": 0, "limit": 0 }
        ]
      }
    }
  });
  let resume_payload = json!({
    "connectorId": 1,
    "csChargingProfiles": {
      "chargingSchedule": {
        "chargingSchedulePeriod": [
          { "startPeriod": 0, "limit": 16 }
        ]
      }
    }
  });

  let suspend_status = simulator
    .set_charging_profile_v1_6(&suspend_payload)
    .expect("suspend profile should parse");
  assert_eq!(suspend_status, ResponseStatus::Accepted);
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("SuspendedEVSE")
  );

  let resume_status = simulator
    .set_charging_profile_v1_6(&resume_payload)
    .expect("resume profile should parse");
  assert_eq!(resume_status, ResponseStatus::Accepted);
  assert_eq!(
    simulator
      .connectors
      .get(&1)
      .map(|item| item.status.display()),
    Some("Charging")
  );
}

#[test]
fn set_charging_profile_v2_x_updates_transaction_status() {
  for_each_v2_x_simulator(|_, mut simulator| {
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .expect("start should succeed");
    let suspend_payload = json!({
      "evseId": 1,
      "chargingProfile": {
        "chargingSchedule": [
          {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 0 }
            ]
          }
        ]
      }
    });
    let resume_payload = json!({
      "evseId": 1,
      "chargingProfile": {
        "chargingSchedule": [
          {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 16 }
            ]
          }
        ]
      }
    });

    let suspend_status = simulator
      .set_charging_profile_v2_x(&suspend_payload)
      .expect("suspend profile should parse");
    assert_eq!(suspend_status, ResponseStatus::Accepted);
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("SuspendedEVSE")
    );

    let resume_status = simulator
      .set_charging_profile_v2_x(&resume_payload)
      .expect("resume profile should parse");
    assert_eq!(resume_status, ResponseStatus::Accepted);
    assert_eq!(
      simulator
        .connectors
        .get(&1)
        .map(|item| item.status.display()),
      Some("Occupied")
    );
  });
}

#[test]
fn get_composite_schedule_v1_6_uses_profile_limit() {
  let mut simulator = simulator_for_tests();
  let schedule_payload = json!({ "connectorId": 1, "duration": 60 });
  let before = simulator
    .get_composite_schedule_v1_6(&schedule_payload)
    .expect("schedule request should parse");
  assert_eq!(before["status"], json!(ResponseStatus::Rejected.as_str()));

  let set_payload = json!({
    "connectorId": 1,
    "csChargingProfiles": {
      "chargingSchedule": {
        "chargingSchedulePeriod": [
          { "startPeriod": 0, "limit": 12.5 }
        ]
      }
    }
  });
  assert_eq!(
    simulator
      .set_charging_profile_v1_6(&set_payload)
      .expect("profile should parse"),
    ResponseStatus::Accepted
  );

  let after = simulator
    .get_composite_schedule_v1_6(&schedule_payload)
    .expect("schedule request should parse");
  assert_eq!(
    after["chargingSchedule"]["chargingSchedulePeriod"][0]["limit"],
    json!(12.5)
  );
}

#[test]
fn get_composite_schedule_v1_6_rejects_invalid_unit() {
  let simulator = simulator_for_tests();
  let error = simulator
    .get_composite_schedule_v1_6(&json!({
      "connectorId": 1,
      "duration": 60,
      "chargingRateUnit": "Wh",
    }))
    .expect_err("invalid unit should reject");

  assert!(
    error
      .to_string()
      .contains("chargingRateUnit must be A or W")
  );
}

#[test]
fn clear_charging_profile_v1_6_honors_connector_filter() {
  let mut simulator = simulator_for_tests();
  assert_eq!(
    simulator
      .set_charging_profile_v1_6(&json!({
        "connectorId": 1,
        "csChargingProfiles": {
          "chargingProfileId": 10,
          "chargingSchedule": {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 6.0 }
            ]
          }
        }
      }))
      .expect("profile should parse"),
    ResponseStatus::Accepted
  );
  assert_eq!(
    simulator
      .set_charging_profile_v1_6(&json!({
        "connectorId": 2,
        "csChargingProfiles": {
          "chargingProfileId": 20,
          "chargingSchedule": {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 10.0 }
            ]
          }
        }
      }))
      .expect("profile should parse"),
    ResponseStatus::Accepted
  );

  assert_eq!(
    simulator
      .clear_charging_profile_v1_6(&json!({ "connectorId": 1 }))
      .expect("clear request should parse"),
    ResponseStatus::Accepted
  );
  assert!(!simulator.charging_profiles.contains_key(&1));
  assert!(simulator.charging_profiles.contains_key(&2));
}

#[test]
fn clear_charging_profile_v1_6_honors_purpose_and_stack_filters() {
  let mut simulator = simulator_for_tests();
  assert_eq!(
    simulator
      .set_charging_profile_v1_6(&json!({
        "connectorId": 1,
        "csChargingProfiles": {
          "chargingProfileId": 10,
          "chargingProfilePurpose": "TxProfile",
          "stackLevel": 2,
          "chargingSchedule": {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 6.0 }
            ]
          }
        }
      }))
      .expect("profile should parse"),
    ResponseStatus::Accepted
  );

  assert_eq!(
    simulator
      .clear_charging_profile_v1_6(&json!({
        "chargingProfilePurpose": "ChargePointMaxProfile",
        "stackLevel": 2
      }))
      .expect("clear request should parse"),
    ResponseStatus::Unknown
  );
  assert!(simulator.charging_profiles.contains_key(&1));

  assert_eq!(
    simulator
      .clear_charging_profile_v1_6(&json!({
        "chargingProfilePurpose": "TxProfile",
        "stackLevel": 2
      }))
      .expect("clear request should parse"),
    ResponseStatus::Accepted
  );
  assert!(!simulator.charging_profiles.contains_key(&1));
}

#[test]
fn get_composite_schedule_v2_x_uses_profile_limit() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let schedule_payload = json!({ "evseId": 1, "duration": 60 });
    let before = simulator
      .get_composite_schedule_v2_x(&schedule_payload)
      .expect("schedule request should parse");
    assert_eq!(before["status"], json!(ResponseStatus::Rejected.as_str()));

    let set_payload = json!({
      "evseId": 1,
      "chargingProfile": {
        "chargingSchedule": [
          {
            "chargingSchedulePeriod": [
              { "startPeriod": 0, "limit": 8.0 }
            ]
          }
        ]
      }
    });
    assert_eq!(
      simulator
        .set_charging_profile_v2_x(&set_payload)
        .expect("profile should parse"),
      ResponseStatus::Accepted
    );

    let after = simulator
      .get_composite_schedule_v2_x(&schedule_payload)
      .expect("schedule request should parse");
    assert_eq!(
      after["schedule"]["chargingSchedulePeriod"][0]["limit"],
      json!(8.0)
    );
  });
}

#[test]
fn clear_charging_profile_v2_x_honors_profile_filter() {
  for_each_v2_x_simulator(|_, mut simulator| {
    assert_eq!(
      simulator
        .set_charging_profile_v2_x(&json!({
          "evseId": 1,
          "chargingProfile": {
            "id": 10,
            "chargingSchedule": [
              {
                "chargingSchedulePeriod": [
                  { "startPeriod": 0, "limit": 6.0 }
                ]
              }
            ]
          }
        }))
        .expect("profile should parse"),
      ResponseStatus::Accepted
    );
    assert_eq!(
      simulator
        .clear_charging_profile_v2_x(&json!({
          "chargingProfileId": 99
        }))
        .expect("clear request should parse"),
      ResponseStatus::Unknown
    );
    assert!(simulator.charging_profiles.contains_key(&1));

    assert_eq!(
      simulator
        .clear_charging_profile_v2_x(&json!({
          "chargingProfileId": 10
        }))
        .expect("clear request should parse"),
      ResponseStatus::Accepted
    );
    assert!(!simulator.charging_profiles.contains_key(&1));
  });
}

#[test]
fn clear_charging_profile_v2_x_honors_criteria_filters() {
  for_each_v2_x_simulator(|_, mut simulator| {
    assert_eq!(
      simulator
        .set_charging_profile_v2_x(&json!({
          "evseId": 2,
          "chargingProfile": {
            "id": 20,
            "chargingProfilePurpose": "TxProfile",
            "stackLevel": 3,
            "chargingSchedule": [
              {
                "chargingSchedulePeriod": [
                  { "startPeriod": 0, "limit": 10.0 }
                ]
              }
            ]
          }
        }))
        .expect("profile should parse"),
      ResponseStatus::Accepted
    );

    assert_eq!(
      simulator
        .clear_charging_profile_v2_x(&json!({
          "chargingProfileCriteria": {
            "evseId": 1,
            "chargingProfilePurpose": "TxProfile",
            "stackLevel": 3
          }
        }))
        .expect("clear request should parse"),
      ResponseStatus::Unknown
    );
    assert!(simulator.charging_profiles.contains_key(&2));

    assert_eq!(
      simulator
        .clear_charging_profile_v2_x(&json!({
          "chargingProfileCriteria": {
            "evseId": 2,
            "chargingProfilePurpose": "TxProfile",
            "stackLevel": 3
          }
        }))
        .expect("clear request should parse"),
      ResponseStatus::Accepted
    );
    assert!(!simulator.charging_profiles.contains_key(&2));
  });
}

#[test]
fn composite_schedule_preserves_profile_units() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for (unit, other_unit) in [("A", "W"), ("W", "A")] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      let schedule = json!({
        "id": 1,
        "chargingRateUnit": unit,
        "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 16}]
      });
      let profile = if protocol == OcppVersion::V1_6 {
        json!({"chargingSchedule": schedule})
      } else {
        json!({"chargingSchedule": [schedule]})
      };
      simulator.apply_set_charging_profile(1, &profile).unwrap();

      for requested_unit in [None, Some(unit), Some(other_unit)] {
        let mut request = json!({
          "connectorId": 1,
          "evseId": 1,
          "duration": 60
        });
        if let Some(requested_unit) = requested_unit {
          request["chargingRateUnit"] = json!(requested_unit);
        }
        let response = if protocol == OcppVersion::V1_6 {
          simulator.get_composite_schedule_v1_6(&request).unwrap()
        } else {
          simulator.get_composite_schedule_v2_x(&request).unwrap()
        };
        if requested_unit == Some(other_unit) {
          assert_eq!(response["status"], "Rejected");
          assert!(response.get("schedule").is_none());
          assert!(response.get("chargingSchedule").is_none());
        } else {
          assert_eq!(response["status"], "Accepted");
          let schedule = if protocol == OcppVersion::V1_6 {
            &response["chargingSchedule"]
          } else {
            &response["schedule"]
          };
          assert_eq!(schedule["chargingRateUnit"], unit);
          assert_eq!(schedule["chargingSchedulePeriod"][0]["limit"], 16.0);
        }
        let schema = if protocol == OcppVersion::V1_6 {
          "schemas/1.6/GetCompositeScheduleResponse.json".to_string()
        } else {
          schema_path(
            v2_x_schema_dir(protocol),
            "GetCompositeScheduleResponse.json",
          )
        };
        assert_schema_valid(&schema, &response);
      }
    }
  }
}

#[test]
fn invalid_profile_unit_is_rejected_before_mutation() {
  for_each_v2_x_simulator(|_, mut simulator| {
    let profile = json!({
      "chargingSchedule": [{
        "chargingRateUnit": "Wh",
        "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 16}]
      }]
    });
    assert!(simulator.apply_set_charging_profile(1, &profile).is_err());
    assert!(simulator.charging_profiles.is_empty());
    assert!(simulator.connectors[&1].offered_limit.is_none());
  });
}

#[test]
fn stored_zero_profile_suspends_subsequent_transaction_start() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for remote_start in [false, true] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      simulator
        .apply_set_charging_profile(1, &zero_limit_profile(protocol))
        .unwrap();
      assert_eq!(simulator.connectors[&1].status, ConnectorStatus::Available);
      simulator
        .start_transaction(1, "TOKEN".into(), remote_start, None, true)
        .unwrap();
      assert_eq!(
        simulator.connectors[&1].status,
        ConnectorStatus::SuspendedEvse
      );

      let context = simulator.queue.pop_front().unwrap().context;
      let response = if protocol == OcppVersion::V1_6 {
        json!({"transactionId": 42, "idTagInfo": {"status": "Accepted"}})
      } else {
        json!({"idTokenInfo": {"status": "Accepted"}})
      };
      simulator
        .apply_call_result_context(&context, &response)
        .unwrap();
      let status = queued_payload(&simulator, "StatusNotification");
      if protocol == OcppVersion::V1_6 {
        assert_eq!(status["status"], "SuspendedEVSE");
      } else {
        assert_eq!(status["connectorStatus"], "Occupied");
      }

      assert_eq!(
        simulator.clear_charging_profiles_matching(vec![1], |_| true),
        ResponseStatus::Accepted
      );
      let expected = if protocol == OcppVersion::V1_6 {
        ConnectorStatus::Charging
      } else {
        ConnectorStatus::Occupied
      };
      assert_eq!(simulator.connectors[&1].status, expected);
      assert!(simulator.connectors[&1].transaction.is_some());
    }
  }
}

#[test]
fn zero_limit_remains_effective_after_stop_failure() {
  for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1] {
    for failure in ["timeout", "error"] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      simulator
        .apply_set_charging_profile(1, &zero_limit_profile(protocol))
        .unwrap();
      simulator
        .start_transaction(1, "TOKEN".to_string(), false, None, true)
        .unwrap();
      let context = simulator.queue.pop_front().unwrap().context;
      let response = if protocol == OcppVersion::V1_6 {
        json!({"transactionId": 42, "idTagInfo": {"status": "Accepted"}})
      } else {
        json!({"idTokenInfo": {"status": "Accepted"}})
      };
      simulator
        .apply_call_result_context(&context, &response)
        .unwrap();
      simulator.queue.clear();
      simulator.stop_transaction(1, None, false, true).unwrap();
      simulator.pending = Some(PendingCall {
        message_id: "stop".to_string(),
        sent_at: std::time::Instant::now(),
        call: simulator.queue.pop_front().unwrap(),
      });
      if failure == "timeout" {
        simulator.config.request_timeout = Duration::ZERO;
        simulator.check_pending_timeout();
      } else {
        simulator
          .handle_call_error("stop", "InternalError", "retry")
          .unwrap();
      }
      assert_eq!(
        simulator.connectors[&1].status,
        ConnectorStatus::SuspendedEvse
      );
      assert!(simulator.connectors[&1].transaction.is_some());
    }
  }
}

fn zero_limit_profile(protocol: OcppVersion) -> Value {
  let mut schedule = json!({
    "startSchedule": "2026-11-03T00:00:00Z",
    "chargingRateUnit": "W",
    "chargingSchedulePeriod": [{"startPeriod": 0, "limit": 0}],
  });
  let mut profile = json!({
    "stackLevel": 0,
    "chargingProfilePurpose": "TxDefaultProfile",
    "chargingProfileKind": "Absolute",
  });
  if protocol == OcppVersion::V1_6 {
    profile["chargingProfileId"] = json!(1);
    profile["chargingSchedule"] = schedule;
    assert_schema_valid(
      "schemas/1.6/SetChargingProfile.json",
      &json!({"connectorId": 1, "csChargingProfiles": profile}),
    );
  } else {
    schedule["id"] = json!(1);
    profile["id"] = json!(1);
    profile["chargingSchedule"] = json!([schedule]);
    assert_schema_valid(
      &schema_path(v2_x_schema_dir(protocol), "SetChargingProfileRequest.json"),
      &json!({"evseId": 1, "chargingProfile": profile}),
    );
  }
  profile
}
