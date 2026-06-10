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
