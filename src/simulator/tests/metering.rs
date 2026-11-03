use super::*;
use crate::ocpp::{
  MeterValueLocation, TriggerMessage_V1_6, TriggerMessage_V2_X,
};

const PROTOCOLS: [OcppVersion; 3] =
  [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1];

fn validate_meter_message(
  protocol: OcppVersion,
  action: &str,
  payload: &Value,
) {
  let schema = match protocol {
    OcppVersion::V1_6 => format!("schemas/1.6/{action}.json"),
    _ => format!("{}/{action}Request.json", v2_x_schema_dir(protocol)),
  };
  assert_schema_valid(&schema, payload);
}

fn assert_soc_sample(protocol: OcppVersion, payload: &Value) {
  let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
  let sample = samples
    .iter()
    .find(|sample| sample["measurand"] == "SoC")
    .expect("SoC sample");
  assert_eq!(sample["location"], "EV");
  if protocol == OcppVersion::V1_6 {
    assert_eq!(sample["value"], "75.5");
    assert_eq!(sample["unit"], "Percent");
  } else {
    assert_eq!(sample["value"], json!(75.5));
    assert_eq!(sample["unitOfMeasure"]["unit"], "Percent");
  }
}

#[test]
fn soc_is_unset_and_invalid_updates_leave_meter_state_unchanged() {
  let mut simulator = simulator_for_tests();
  assert!(simulator.send_meter_value(1, Measurand::Soc, true).is_err());
  assert!(simulator.queue.is_empty());
  simulator
    .set_meter_value(1, MeterReading::energy(1234))
    .unwrap();
  simulator
    .set_meter_value(1, MeterReading::soc(75.5).unwrap())
    .unwrap();

  let mut invalid = MeterReading::soc(75.5).unwrap();
  invalid.value = (-1).into();
  assert!(simulator.set_meter_value(1, invalid).is_err());
  assert!(
    simulator
      .set_meter_value(99, MeterReading::soc(0.0).unwrap())
      .is_err()
  );
  let readings = &simulator.connectors[&1].meter_values;
  assert_eq!(readings.energy_wh(), 1234);
  assert_eq!(
    readings.get(Measurand::Soc).unwrap().value.as_f64(),
    Some(75.5)
  );
  assert!(
    simulator.connectors[&2]
      .meter_values
      .get(Measurand::Soc)
      .is_none()
  );
}

#[test]
fn soc_sends_use_percent_and_protocol_specific_value_types() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator.send_meter_value(1, Measurand::Soc, true).unwrap();
    let payload = queued_payload(&simulator, "MeterValues");
    validate_meter_message(protocol, "MeterValues", &payload);
    assert_soc_sample(protocol, &payload);
    assert_eq!(
      payload["meterValue"][0]["sampledValue"]
        .as_array()
        .unwrap()
        .len(),
      1
    );
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 0);

    simulator.queue.clear();
    simulator
      .send_meter_value(1, Measurand::EnergyActiveImportRegister, true)
      .unwrap();
    let payload = queued_payload(&simulator, "MeterValues");
    let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0]["measurand"], "Energy.Active.Import.Register");
  }
}

#[test]
fn soc_sends_preserve_active_transaction_context() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .unwrap();
    if protocol == OcppVersion::V1_6 {
      simulator.set_v1_6_transaction_id(1, 1, 123).unwrap();
    }
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator.queue.clear();
    let previous_seq_no = simulator.connectors[&1]
      .transaction
      .as_ref()
      .unwrap()
      .seq_no;
    simulator.send_meter_value(1, Measurand::Soc, true).unwrap();
    let action = if protocol == OcppVersion::V1_6 {
      "MeterValues"
    } else {
      "TransactionEvent"
    };
    let payload = queued_payload(&simulator, action);
    validate_meter_message(protocol, action, &payload);
    assert_soc_sample(protocol, &payload);
    assert_eq!(
      payload["meterValue"][0]["sampledValue"]
        .as_array()
        .unwrap()
        .len(),
      1
    );
    if protocol == OcppVersion::V1_6 {
      assert_eq!(payload["transactionId"], 123);
    } else {
      assert_eq!(payload["eventType"], "Updated");
      assert_eq!(payload["seqNo"], previous_seq_no + 1);
      assert_eq!(payload["evse"]["id"], 1);
      let transaction = simulator.connectors[&1].transaction.as_ref().unwrap();
      assert_eq!(
        payload["transactionInfo"]["transactionId"],
        transaction.transaction_uid
      );
    }
  }
}

#[test]
fn soc_updates_are_local_when_sending_is_unavailable() {
  for protocol in PROTOCOLS {
    for connected in [false, true] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      simulator.boot_registration_status = BootRegistrationStatus::Pending;
      simulator
        .handle_common_command(
          SimulatorCommand::UpdateMeterValue {
            connector: 1,
            update: MeterUpdate::Set(MeterReading::soc(75.5).unwrap()),
          },
          connected,
        )
        .unwrap();
      simulator
        .handle_common_command(
          SimulatorCommand::SendMeterValue {
            connector: 1,
            measurand: Measurand::Soc,
            update: None,
          },
          connected,
        )
        .unwrap();
      assert!(simulator.queue.is_empty());
      assert_eq!(
        simulator.connectors[&1]
          .meter_values
          .get(Measurand::Soc)
          .unwrap()
          .value
          .as_f64(),
        Some(75.5)
      );
    }
  }
}

#[test]
fn triggered_meter_values_include_energy_and_known_soc_without_precision_loss()
{
  let energy_wh = 9_007_199_254_740_993_i64;
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::energy(energy_wh))
      .unwrap();
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    match protocol {
      OcppVersion::V1_6 => simulator.trigger_message_v1_6_standard(
        TriggerMessage_V1_6::MeterValues,
        Some(1),
      ),
      _ => simulator
        .trigger_message_v2_x(TriggerMessage_V2_X::MeterValues, Some(1)),
    }
    .unwrap();
    let payload = queued_payload(&simulator, "MeterValues");
    validate_meter_message(protocol, "MeterValues", &payload);
    assert_soc_sample(protocol, &payload);
    let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    if protocol == OcppVersion::V1_6 {
      assert_eq!(samples[0]["value"], energy_wh.to_string());
    } else {
      assert_eq!(samples[0]["value"].as_i64(), Some(energy_wh));
    }
  }
}

#[test]
fn transaction_boundaries_keep_energy_and_include_known_soc_in_v2_x() {
  for protocol in PROTOCOLS {
    let (mut simulator, mut ui_rx) =
      simulator_for_tests_with_protocol_and_ui(protocol);
    simulator
      .set_meter_value(1, MeterReading::energy(1200))
      .unwrap();
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .unwrap();
    let start_action = if protocol == OcppVersion::V1_6 {
      "StartTransaction"
    } else {
      "TransactionEvent"
    };
    let started = queued_payload(&simulator, start_action);
    if protocol == OcppVersion::V1_6 {
      assert_eq!(started["meterStart"], 1200);
      simulator.set_v1_6_transaction_id(1, 1, 123).unwrap();
    } else {
      assert_soc_sample(protocol, &started);
      validate_meter_message(protocol, "TransactionEvent", &started);
      assert_eq!(started["meterValue"][0]["sampledValue"][0]["value"], 1200);
    }
    simulator.queue.clear();
    simulator
      .set_meter_value(1, MeterReading::energy(1300))
      .unwrap();
    simulator.stop_transaction(1, None, false, true).unwrap();
    let stop_action = if protocol == OcppVersion::V1_6 {
      "StopTransaction"
    } else {
      "TransactionEvent"
    };
    let stopped = queued_payload(&simulator, stop_action);
    if protocol == OcppVersion::V1_6 {
      assert_eq!(stopped["meterStop"], 1300);
    } else {
      assert_soc_sample(protocol, &stopped);
      validate_meter_message(protocol, "TransactionEvent", &stopped);
      assert_eq!(stopped["meterValue"][0]["sampledValue"][0]["value"], 1300);
    }
    // The final message retains its sample even if later local readings change.
    simulator
      .set_meter_value(1, MeterReading::soc(80.0).unwrap())
      .unwrap();
    let context = simulator
      .queue
      .iter()
      .find(|call| call.action == stop_action)
      .unwrap()
      .context
      .clone();
    simulator
      .apply_call_result_context(&context, &json!({}))
      .unwrap();
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1300);
    assert_eq!(queued_payload(&simulator, stop_action), stopped);

    drain_log_messages(&mut ui_rx);
    simulator.show_meter_value(Some(1), Measurand::Soc).unwrap();
    assert_eq!(drain_log_messages(&mut ui_rx), ["Connector 1 SoC=unset"]);
    simulator.queue.clear();
    assert!(
      simulator
        .update_meter_value(1, soc_adjustment(1.0))
        .is_err()
    );
    assert!(simulator.send_meter_value(1, Measurand::Soc, true).is_err());
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn generic_readings_are_filtered_when_the_protocol_changes() {
  let mut simulator = simulator_for_tests();
  let temperature = MeterReading {
    measurand: Measurand::Temperature,
    value: 25.into(),
    unit: "Celsius".to_string(),
    location: Some(MeterValueLocation::Body),
  };
  simulator.set_meter_value(1, temperature.clone()).unwrap();
  simulator.config.protocol = OcppVersion::V2_0_1;
  assert!(simulator.set_meter_value(2, temperature).is_err());
  assert!(
    simulator
      .send_meter_value(1, Measurand::Temperature, true)
      .is_err()
  );
  simulator.enqueue_meter_values(1).unwrap();
  let payload = queued_payload(&simulator, "MeterValues");
  validate_meter_message(OcppVersion::V2_0_1, "MeterValues", &payload);
  let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
  assert_eq!(samples.len(), 1);
  assert_eq!(samples[0]["measurand"], "Energy.Active.Import.Register");
}

fn soc_adjustment(delta: f64) -> MeterUpdate {
  MeterUpdate::Adjust(
    Measurand::Soc,
    serde_json::Number::from_f64(delta).unwrap(),
  )
}

#[test]
fn soc_queries_show_known_and_unset_readings() {
  for connected in [false, true] {
    let (mut simulator, mut ui_rx) =
      simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          connector: None,
          measurand: Measurand::Soc,
        },
        connected,
      )
      .unwrap();
    assert_eq!(
      drain_log_messages(&mut ui_rx),
      ["Connector 1 SoC=unset", "Connector 2 SoC=unset",]
    );

    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    drain_log_messages(&mut ui_rx);
    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          connector: Some(1),
          measurand: Measurand::Soc,
        },
        connected,
      )
      .unwrap();
    assert_eq!(
      drain_log_messages(&mut ui_rx),
      ["Connector 1 SoC=75.5Percent"]
    );

    simulator
      .handle_common_command(
        SimulatorCommand::GetMeterValue {
          connector: None,
          measurand: Measurand::Soc,
        },
        connected,
      )
      .unwrap();
    assert_eq!(
      drain_log_messages(&mut ui_rx),
      ["Connector 1 SoC=75.5Percent", "Connector 2 SoC=unset",]
    );
    assert!(simulator.show_meter_value(Some(3), Measurand::Soc).is_err());
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn soc_adjustments_require_an_existing_reading() {
  let mut simulator = simulator_for_tests();
  for command in [
    SimulatorCommand::UpdateMeterValue {
      connector: 1,
      update: soc_adjustment(5.0),
    },
    SimulatorCommand::SendMeterValue {
      connector: 1,
      measurand: Measurand::Soc,
      update: Some(soc_adjustment(5.0)),
    },
  ] {
    assert!(simulator.handle_common_command(command, true).is_err());
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn soc_adjustments_use_percentage_points_and_allow_boundaries() {
  let mut simulator = simulator_for_tests();
  simulator
    .set_meter_value(1, MeterReading::energy(1234))
    .unwrap();
  simulator
    .set_meter_value(1, MeterReading::soc(75.5).unwrap())
    .unwrap();
  for (delta, expected) in [
    (2.5, 78.0),
    (-3.0, 75.0),
    (0.0, 75.0),
    (25.0, 100.0),
    (-100.0, 0.0),
  ] {
    simulator
      .update_meter_value(1, soc_adjustment(delta))
      .unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(expected)
    );
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1234);
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn invalid_soc_adjustments_leave_the_reading_unchanged_and_cancel_sending() {
  let mut simulator = simulator_for_tests();
  for (current, delta) in [(99.5, 1.0), (0.5, -1.0), (50.0, 1e308)] {
    simulator
      .set_meter_value(1, MeterReading::soc(current).unwrap())
      .unwrap();
    for command in [
      SimulatorCommand::UpdateMeterValue {
        connector: 1,
        update: soc_adjustment(delta),
      },
      SimulatorCommand::SendMeterValue {
        connector: 1,
        measurand: Measurand::Soc,
        update: Some(soc_adjustment(delta)),
      },
    ] {
      assert!(simulator.handle_common_command(command, true).is_err());
      assert_eq!(
        simulator.connectors[&1]
          .meter_values
          .get(Measurand::Soc)
          .unwrap()
          .value
          .as_f64(),
        Some(current)
      );
      assert!(simulator.queue.is_empty());
    }
  }
}

#[test]
fn send_soc_adjustments_report_updated_values_in_all_protocols() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    for (active, delta, expected) in [(false, 2.5, 78.0), (true, -2.5, 75.5)] {
      if active {
        simulator
          .start_transaction(1, "TOKEN".to_string(), false, None, true)
          .unwrap();
      }
      simulator.queue.clear();
      simulator
        .handle_common_command(
          SimulatorCommand::SendMeterValue {
            connector: 1,
            measurand: Measurand::Soc,
            update: Some(soc_adjustment(delta)),
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
      validate_meter_message(protocol, action, &payload);
      let value = &payload["meterValue"][0]["sampledValue"][0]["value"];
      let actual = if protocol == OcppVersion::V1_6 {
        value.as_str().unwrap().parse::<f64>().ok()
      } else {
        value.as_f64()
      };
      assert_eq!(actual, Some(expected));
    }
  }
}

#[test]
fn send_soc_adjustments_remain_local_when_sending_is_unavailable() {
  for connected in [false, true] {
    let mut simulator = simulator_for_tests();
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .handle_common_command(
        SimulatorCommand::SendMeterValue {
          connector: 1,
          measurand: Measurand::Soc,
          update: Some(soc_adjustment(2.5)),
        },
        connected,
      )
      .unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(78.0)
    );
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn offline_stop_does_not_pass_soc_to_the_next_transaction() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::energy(1200))
      .unwrap();
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .set_meter_value(2, MeterReading::soc(30.0).unwrap())
      .unwrap();
    simulator
      .start_transaction(1, "FIRST".to_string(), false, None, false)
      .unwrap();
    simulator.stop_transaction(1, None, false, false).unwrap();
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1200);
    assert_eq!(
      simulator.connectors[&2]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(30.0)
    );

    simulator
      .set_connector_status(1, "Available", false)
      .unwrap();
    simulator
      .start_transaction(1, "SECOND".to_string(), false, None, true)
      .unwrap();
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
    if protocol != OcppVersion::V1_6 {
      let payload = queued_payload(&simulator, "TransactionEvent");
      let samples =
        payload["meterValue"][0]["sampledValue"].as_array().unwrap();
      assert_eq!(samples.len(), 1);
      assert_eq!(samples[0]["measurand"], "Energy.Active.Import.Register");
    }
  }
}

#[test]
fn stale_transaction_cleanup_keeps_prepared_and_current_soc() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .start_transaction(1, "FIRST".to_string(), false, None, false)
      .unwrap();
    let old_id = simulator.connectors[&1]
      .transaction
      .as_ref()
      .unwrap()
      .local_id;
    simulator.stop_transaction(1, None, false, false).unwrap();
    simulator
      .set_meter_value(1, MeterReading::soc(20.0).unwrap())
      .unwrap();

    // Repeated old callbacks must not erase an idle seed for the next session.
    simulator.complete_transaction_stop(1, old_id).unwrap();
    simulator.cancel_transaction_start(1, old_id).unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(20.0)
    );

    simulator
      .set_connector_status(1, "Available", false)
      .unwrap();
    simulator
      .start_transaction(1, "SECOND".to_string(), false, None, false)
      .unwrap();
    let new_id = simulator.connectors[&1]
      .transaction
      .as_ref()
      .unwrap()
      .local_id;
    assert_ne!(new_id, old_id);
    simulator.complete_transaction_stop(1, old_id).unwrap();
    simulator.cancel_transaction_start(1, old_id).unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(20.0)
    );

    simulator.stop_transaction(1, None, false, false).unwrap();
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
  }
}

#[test]
fn soc_seed_survives_rejected_start_but_expires_after_cancelled_start() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::energy(1200))
      .unwrap();
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .set_connector_status(1, "Unavailable", false)
      .unwrap();
    assert!(
      simulator
        .start_transaction(1, "TOKEN".to_string(), false, None, true,)
        .is_err()
    );
    assert!(simulator.connectors[&1].transaction.is_none());
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(75.5)
    );

    simulator
      .set_connector_status(1, "Available", false)
      .unwrap();
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, true)
      .unwrap();
    let start = simulator.queue.pop_front().unwrap();
    simulator
      .handle_pending_failure_context(&start.context)
      .unwrap();
    assert!(simulator.connectors[&1].transaction.is_none());
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
    assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1200);
  }
}

#[test]
fn continuing_transaction_keeps_soc_through_pause_disconnect_and_failed_stop() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    simulator
      .set_meter_value(1, MeterReading::soc(75.5).unwrap())
      .unwrap();
    simulator
      .start_transaction(1, "TOKEN".to_string(), false, None, false)
      .unwrap();
    let local_id = simulator.connectors[&1]
      .transaction
      .as_ref()
      .unwrap()
      .local_id;
    if protocol == OcppVersion::V1_6 {
      simulator.set_v1_6_transaction_id(1, local_id, 123).unwrap();
    }
    simulator
      .set_connector_status(1, "SuspendedEV", false)
      .unwrap();
    simulator.handle_disconnect("connection interrupted");
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(75.5)
    );

    simulator.connected = true;
    simulator.stop_transaction(1, None, false, true).unwrap();
    let stop = simulator.queue.pop_front().unwrap();
    simulator
      .handle_pending_failure_context(&stop.context)
      .unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .transaction
        .as_ref()
        .unwrap()
        .local_id,
      local_id
    );
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .unwrap()
        .value
        .as_f64(),
      Some(75.5)
    );

    simulator.queue.clear();
    simulator.stop_transaction(1, None, false, true).unwrap();
    let stop = simulator.queue.pop_front().unwrap();
    simulator
      .apply_call_result_context(&stop.context, &json!({}))
      .unwrap();
    assert!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::Soc)
        .is_none()
    );
  }
}
