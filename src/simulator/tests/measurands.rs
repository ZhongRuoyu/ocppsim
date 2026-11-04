use super::*;
use crate::metering::{MeterDelta, MeterReadings};
use crate::ocpp::MeterValueLocation;

const PROTOCOLS: [OcppVersion; 3] =
  [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1];

#[test]
fn all_supported_measurands_serialize_in_their_protocol() {
  for protocol in PROTOCOLS {
    let mut simulator = simulator_for_tests_with_protocol(protocol);
    let supported = Measurand::ALL
      .iter()
      .copied()
      .filter(|measurand| measurand.is_supported(protocol))
      .collect::<Vec<_>>();
    for measurand in &supported {
      simulator
        .set_meter_value(1, MeterReading::new(*measurand, 1.into()).unwrap())
        .unwrap();
    }
    simulator.enqueue_meter_values(1).unwrap();
    let payload = queued_payload(&simulator, "MeterValues");
    let schema = if protocol == OcppVersion::V1_6 {
      "schemas/1.6/MeterValues.json".to_string()
    } else {
      format!("{}/MeterValuesRequest.json", v2_x_schema_dir(protocol))
    };
    assert_schema_valid(&schema, &payload);
    let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
    assert_eq!(samples.len(), supported.len());
    for sample in samples {
      let measurand = sample["measurand"].as_str().unwrap();
      if protocol == OcppVersion::V1_6 {
        assert!(sample["value"].is_string());
        if ["Frequency", "RPM", "Power.Factor"].contains(&measurand) {
          assert!(sample.get("unit").is_none());
        }
      } else {
        assert!(sample["value"].is_number());
        if [
          "Power.Factor",
          "Display.ChargingComplete",
          "Display.InletHot",
        ]
        .contains(&measurand)
        {
          assert!(sample.get("unitOfMeasure").is_none());
        }
      }
    }
    if protocol != OcppVersion::V1_6 {
      simulator.queue.clear();
      simulator
        .start_transaction(1, "TOKEN".to_string(), false, None, true)
        .unwrap();
      assert_schema_valid(
        &format!("{}/TransactionEventRequest.json", v2_x_schema_dir(protocol)),
        &queued_payload(&simulator, "TransactionEvent"),
      );
    }
  }
}

#[test]
fn generic_readings_use_canonical_units_and_locations() {
  for (measurand, unit) in [
    (Measurand::EnergyActiveImportRegister, "Wh"),
    (Measurand::EnergyRequestMinimumV2X, "Wh"),
    (Measurand::DisplayBatteryEnergyCapacity, "Wh"),
    (Measurand::EnergyReactiveExportInterval, "varh"),
    (Measurand::EnergyApparentImport, "VAh"),
    (Measurand::PowerActiveSetpoint, "W"),
    (Measurand::PowerReactiveImport, "var"),
    (Measurand::CurrentImportOffered, "A"),
    (Measurand::VoltageMinimum, "V"),
    (Measurand::Frequency, "Hz"),
    (Measurand::Temperature, "Celsius"),
    (Measurand::Rpm, "RPM"),
    (Measurand::Soc, "Percent"),
    (Measurand::DisplayTargetSoc, "Percent"),
    (Measurand::DisplayRemainingTimeToTargetSoc, "s"),
    (Measurand::PowerFactor, ""),
    (Measurand::DisplayChargingComplete, ""),
  ] {
    assert_eq!(MeterReading::new(measurand, 1.into()).unwrap().unit, unit);
  }
  assert_eq!(
    MeterReading::new(Measurand::DisplayTargetSoc, 50.into())
      .unwrap()
      .location,
    Some(MeterValueLocation::Ev)
  );
  assert_eq!(
    MeterReading::new(Measurand::Temperature, (-5).into())
      .unwrap()
      .location,
    Some(MeterValueLocation::Body)
  );
}

#[test]
fn generic_queries_select_set_readings_without_mutating_or_sending() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator.boot_registration_status = BootRegistrationStatus::Pending;
  simulator
    .set_meter_value(
      1,
      MeterReading::new(Measurand::EnergyActiveImportRegister, 1234.into())
        .unwrap(),
    )
    .unwrap();
  simulator
    .set_meter_value(
      1,
      MeterReading::new(Measurand::PowerActiveImport, 7200.into()).unwrap(),
    )
    .unwrap();
  simulator
    .set_meter_value(1, MeterReading::soc(75.5).unwrap())
    .unwrap();
  drain_log_messages(&mut ui_rx);
  simulator
    .handle_common_command(
      SimulatorCommand::GetMeasurands {
        connector: None,
        measurand: None,
      },
      true,
    )
    .unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    [
      "Connector 1 Energy.Active.Import.Register=1234Wh",
      "Connector 1 Power.Active.Import=7200W",
      "Connector 1 SoC=75.5Percent",
      "Connector 2 Energy.Active.Import.Register=0Wh",
    ]
  );
  simulator.show_measurands(Some(2), None).unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    ["Connector 2 Energy.Active.Import.Register=0Wh",]
  );
  simulator
    .show_measurands(Some(1), Some(Measurand::Voltage))
    .unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    ["Connector 1 Voltage=unset"]
  );
  simulator
    .show_measurands(Some(1), Some(Measurand::Soc))
    .unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    ["Connector 1 SoC=75.5Percent"]
  );
  assert!(simulator.show_measurands(Some(3), None).is_err());
  assert!(simulator.queue.is_empty());
  assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1234);
}

#[test]
fn generic_commands_share_meter_and_soc_state_and_lifetimes() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V2_1);
  simulator
    .set_meter_value(
      1,
      MeterReading::new(Measurand::EnergyActiveImportRegister, 1000.into())
        .unwrap(),
    )
    .unwrap();
  simulator
    .set_meter_value(1, MeterReading::new(Measurand::Soc, 50.into()).unwrap())
    .unwrap();
  simulator
    .update_meter_value(1, MeterUpdate::Adjust(Measurand::Soc, 5.into()))
    .unwrap();
  drain_log_messages(&mut ui_rx);
  simulator
    .show_meter_value(Some(1), Measurand::EnergyActiveImportRegister)
    .unwrap();
  simulator.show_meter_value(Some(1), Measurand::Soc).unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    ["Connector 1 meter=1000Wh", "Connector 1 SoC=55Percent",]
  );
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .unwrap();
  simulator.stop_transaction(1, None, false, false).unwrap();
  assert!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::Soc)
      .is_none()
  );
  assert_eq!(simulator.connectors[&1].meter_values.energy_wh(), 1000);
}

#[test]
fn protocol_changes_filter_stored_readings_and_reject_unsupported_names() {
  let (mut simulator, mut ui_rx) =
    simulator_for_tests_with_protocol_and_ui(OcppVersion::V1_6);
  simulator
    .set_meter_value(
      1,
      MeterReading::new(Measurand::Temperature, 25.into()).unwrap(),
    )
    .unwrap();
  simulator.config.protocol = OcppVersion::V2_0_1;
  assert!(
    simulator
      .set_meter_value(
        1,
        MeterReading::new(Measurand::PowerActiveSetpoint, (-100).into())
          .unwrap()
      )
      .is_err()
  );
  assert!(
    simulator
      .show_measurands(Some(1), Some(Measurand::Temperature))
      .is_err()
  );
  drain_log_messages(&mut ui_rx);
  simulator.show_measurands(Some(1), None).unwrap();
  assert_eq!(
    drain_log_messages(&mut ui_rx),
    ["Connector 1 Energy.Active.Import.Register=0Wh",]
  );
  assert!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::PowerActiveSetpoint)
      .is_none()
  );
}

#[test]
fn transaction_cleanup_expires_ev_values_and_keeps_cumulative_registers() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  let persistent = [
    Measurand::EnergyActiveExportRegister,
    Measurand::EnergyReactiveImportRegister,
    Measurand::EnergyApparentNet,
    Measurand::EnergyActiveNet,
    Measurand::EnergyActiveImportLocalGenerationRegister,
    Measurand::Frequency,
  ];
  let transaction = [
    Measurand::CurrentImport,
    Measurand::PowerActiveImport,
    Measurand::PowerOffered,
    Measurand::PowerFactor,
    Measurand::Voltage,
    Measurand::VoltageMaximum,
    Measurand::EnergyActiveImportInterval,
    Measurand::EnergyReactiveExportInterval,
    Measurand::Soc,
    Measurand::DisplayTargetSoc,
    Measurand::DisplayInletHot,
    Measurand::EnergyRequestTarget,
    Measurand::EnergyActiveImportCableLoss,
  ];
  for measurand in persistent.iter().chain(&transaction) {
    simulator
      .set_meter_value(1, MeterReading::new(*measurand, 1.into()).unwrap())
      .unwrap();
  }
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .unwrap();
  simulator.stop_transaction(1, None, false, false).unwrap();
  let readings = &simulator.connectors[&1].meter_values;
  for measurand in persistent {
    assert!(readings.get(measurand).is_some(), "{}", measurand.as_str());
  }
  for measurand in transaction {
    assert!(readings.get(measurand).is_none(), "{}", measurand.as_str());
  }
}

#[test]
fn starting_a_transaction_resets_configured_cable_loss_only() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  for (measurand, value) in [
    (Measurand::EnergyActiveImportCableLoss, 12),
    (Measurand::EnergyActiveImportLocalGenerationRegister, 1234),
    (Measurand::Soc, 50),
  ] {
    simulator
      .set_meter_value(1, MeterReading::new(measurand, value.into()).unwrap())
      .unwrap();
  }
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, true)
    .unwrap();
  let readings = &simulator.connectors[&1].meter_values;
  assert_eq!(
    readings
      .get(Measurand::EnergyActiveImportCableLoss)
      .unwrap()
      .value
      .as_i64(),
    Some(0)
  );
  assert_eq!(
    readings
      .get(Measurand::EnergyActiveImportLocalGenerationRegister)
      .unwrap()
      .value
      .as_i64(),
    Some(1234)
  );
  assert_eq!(
    readings.get(Measurand::Soc).unwrap().value.as_i64(),
    Some(50)
  );
  let payload = queued_payload(&simulator, "TransactionEvent");
  let samples = payload["meterValue"][0]["sampledValue"].as_array().unwrap();
  let loss = samples
    .iter()
    .find(|sample| sample["measurand"] == "Energy.Active.Import.CableLoss")
    .unwrap();
  assert_eq!(loss["value"], 0);
}

fn apply_generic_update(
  simulator: &mut Simulator,
  input: &str,
) -> anyhow::Result<()> {
  let command =
    crate::commands::parse_command(input).map_err(anyhow::Error::msg)?;
  let crate::commands::UserCommand::UpdateMeterValue { connector, update } =
    command
  else {
    panic!("expected a measurement update");
  };
  simulator.handle_common_command(
    SimulatorCommand::UpdateMeterValue { connector, update },
    false,
  )
}

#[test]
fn generic_relative_updates_apply_both_operators_in_the_stored_unit() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  apply_generic_update(
    &mut simulator,
    "measurand 1 Power.Active.Setpoint 1000",
  )
  .unwrap();
  for (value, expected) in [
    ("+=200", 1200.0),
    ("-=250", 950.0),
    ("-=-50", 1000.0),
    ("+=1.5", 1001.5),
    ("-=0.5", 1001.0),
  ] {
    apply_generic_update(
      &mut simulator,
      &format!("measurand 1 Power.Active.Setpoint {value}"),
    )
    .unwrap();
    let reading = simulator.connectors[&1]
      .meter_values
      .get(Measurand::PowerActiveSetpoint)
      .unwrap();
    assert_eq!(reading.value.as_f64(), Some(expected));
    assert_eq!(reading.unit, "W");
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn unsigned_readings_support_large_signed_deltas() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  apply_generic_update(
    &mut simulator,
    "measurand 1 Energy.Active.Net 18446744073709551615",
  )
  .unwrap();
  apply_generic_update(&mut simulator, "measurand 1 Energy.Active.Net -=1")
    .unwrap();
  assert_eq!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::EnergyActiveNet)
      .unwrap()
      .value
      .as_u64(),
    Some(u64::MAX - 1)
  );
  simulator.enqueue_meter_values(1).unwrap();
  let payload = queued_payload(&simulator, "MeterValues");
  let sample = payload["meterValue"][0]["sampledValue"]
    .as_array()
    .unwrap()
    .iter()
    .find(|sample| sample["measurand"] == "Energy.Active.Net")
    .unwrap();
  assert_eq!(sample["value"].as_u64(), Some(u64::MAX - 1));
  simulator.queue.clear();

  apply_generic_update(&mut simulator, "measurand 1 Energy.Active.Net +=1")
    .unwrap();
  let before = simulator.connectors[&1].meter_values.snapshot();
  assert!(
    apply_generic_update(&mut simulator, "measurand 1 Energy.Active.Net +=1")
      .is_err()
  );
  assert_eq!(simulator.connectors[&1].meter_values.snapshot(), before);
  apply_generic_update(
    &mut simulator,
    "measurand 1 Energy.Active.Net -=18446744073709551615",
  )
  .unwrap();
  assert_eq!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::EnergyActiveNet)
      .unwrap()
      .value
      .as_i64(),
    Some(0)
  );

  apply_generic_update(
    &mut simulator,
    "measurand 1 Energy.Active.Net -9223372036854775808",
  )
  .unwrap();
  apply_generic_update(
    &mut simulator,
    "measurand 1 Energy.Active.Net -=-9223372036854775808",
  )
  .unwrap();
  assert_eq!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::EnergyActiveNet)
      .unwrap()
      .value
      .as_i64(),
    Some(0)
  );
}

#[test]
fn generic_overflow_and_precision_loss_leave_all_stored_readings_unchanged() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  for (initial, adjustment) in [
    ("-9223372036854775808", "-=1"),
    ("18446744073709551615", "+=1"),
    ("18446744073709551615", "-=0.5"),
    ("1e308", "+=1e308"),
    ("1e308", "-=-1e308"),
    ("1e20", "+=1"),
  ] {
    apply_generic_update(
      &mut simulator,
      &format!("measurand 1 Energy.Active.Net {initial}"),
    )
    .unwrap();
    let before = simulator.connectors[&1].meter_values.snapshot();
    assert!(
      apply_generic_update(
        &mut simulator,
        &format!("measurand 1 Energy.Active.Net {adjustment}")
      )
      .is_err(),
      "{initial} {adjustment}"
    );
    assert_eq!(simulator.connectors[&1].meter_values.snapshot(), before);
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn generic_adjustments_use_the_same_result_validation_as_absolute_values() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  for (name, initial, invalid, adjustment) in [
    ("SoC", "95", "105", "+=10"),
    ("Display.TargetSOC", "5", "-5", "-=10"),
    ("Power.Factor", "0.75", "1.25", "+=0.5"),
    ("Energy.Active.Import.Register", "10", "10.5", "+=0.5"),
  ] {
    apply_generic_update(
      &mut simulator,
      &format!("measurand 1 {name} {initial}"),
    )
    .unwrap();
    let before = simulator.connectors[&1].meter_values.snapshot();
    assert!(
      apply_generic_update(
        &mut simulator,
        &format!("measurand 1 {name} {invalid}")
      )
      .is_err()
    );
    assert!(
      apply_generic_update(
        &mut simulator,
        &format!("measurand 1 {name} {adjustment}")
      )
      .is_err()
    );
    assert_eq!(simulator.connectors[&1].meter_values.snapshot(), before);
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn generic_adjustments_reject_unset_and_expired_readings_even_for_zero() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  assert!(
    apply_generic_update(&mut simulator, "measurand 1 Current.Import +=0")
      .is_err()
  );
  apply_generic_update(&mut simulator, "measurand 1 Current.Import 16")
    .unwrap();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .unwrap();
  simulator.stop_transaction(1, None, false, false).unwrap();
  assert!(
    apply_generic_update(&mut simulator, "measurand 1 Current.Import -=0")
      .is_err()
  );
  assert!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::CurrentImport)
      .is_none()
  );
  assert!(simulator.queue.is_empty());
}

#[test]
fn generic_boolean_flags_remain_absolute_only_for_every_adjustment() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  for name in ["Display.ChargingComplete", "Display.InletHot"] {
    for absolute in [0, 1] {
      apply_generic_update(
        &mut simulator,
        &format!("measurand 1 {name} {absolute}"),
      )
      .unwrap();
      let before = simulator.connectors[&1].meter_values.snapshot();
      for adjustment in ["+=0", "-=0", "+=1", "-=1", "+=-1", "-=-1"] {
        let error = apply_generic_update(
          &mut simulator,
          &format!("measurand 1 {name} {adjustment}"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("absolute assignments only"));
        assert_eq!(simulator.connectors[&1].meter_values.snapshot(), before);
        assert!(simulator.queue.is_empty());
      }
    }
  }
}

#[test]
fn mixed_adjustments_accept_large_integers_when_conversion_is_exact() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  for (adjustment, expected) in [
    ("+=4096.0", 2_f64.powi(63) + 4096.0),
    ("-=4096.0", 2_f64.powi(63) - 4096.0),
  ] {
    apply_generic_update(
      &mut simulator,
      "measurand 1 Energy.Active.Net 9223372036854775808",
    )
    .unwrap();
    apply_generic_update(
      &mut simulator,
      &format!("measurand 1 Energy.Active.Net {adjustment}"),
    )
    .unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::EnergyActiveNet)
        .unwrap()
        .value
        .as_f64(),
      Some(expected)
    );
  }
}

#[test]
fn public_adjusted_applies_signed_deltas_without_mutation() {
  let mut readings = MeterReadings::default();
  readings
    .set(
      MeterReading::new(Measurand::EnergyActiveNet, u64::MAX.into()).unwrap(),
    )
    .unwrap();
  let adjusted = readings
    .adjusted(
      Measurand::EnergyActiveNet,
      &MeterDelta::Integer(-i128::from(u64::MAX)),
    )
    .unwrap();
  assert_eq!(adjusted.value.as_i64(), Some(0));
  assert_eq!(
    readings
      .get(Measurand::EnergyActiveNet)
      .unwrap()
      .value
      .as_u64(),
    Some(u64::MAX)
  );

  readings.set(MeterReading::soc(75.5).unwrap()).unwrap();
  for (delta, expected) in [(-2.5, 73.0), (2.5, 78.0)] {
    let adjusted = readings
      .adjusted(Measurand::Soc, &MeterDelta::Float(delta))
      .unwrap();
    assert_eq!(adjusted.value.as_f64(), Some(expected));
  }
  assert_eq!(
    readings.get(Measurand::Soc).unwrap().value.as_f64(),
    Some(75.5)
  );

  readings
    .set(
      MeterReading::new(
        Measurand::EnergyActiveNet,
        serde_json::Number::from_f64(2_f64.powi(63)).unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let adjusted = readings
    .adjusted(
      Measurand::EnergyActiveNet,
      &MeterDelta::Integer(-(1_i128 << 63) - 2048),
    )
    .unwrap();
  assert_eq!(adjusted.value.as_f64(), Some(-2048.0));
}

#[test]
fn invalid_signed_deltas_and_negation_overflow_are_rejected() {
  assert!(MeterDelta::Integer(i128::MIN).negated().is_err());
  assert!(MeterDelta::Float(f64::NAN).negated().is_err());
  assert!(MeterDelta::Float(f64::INFINITY).negated().is_err());

  let mut readings = MeterReadings::default();
  readings
    .set(
      MeterReading::new(Measurand::EnergyActiveNet, u64::MAX.into()).unwrap(),
    )
    .unwrap();
  let before = readings.snapshot();
  for delta in [
    MeterDelta::Integer(i128::MIN),
    MeterDelta::Integer(i128::MAX),
    MeterDelta::Float(f64::NAN),
    MeterDelta::Float(f64::INFINITY),
  ] {
    assert!(
      readings
        .adjusted(Measurand::EnergyActiveNet, &delta)
        .is_err()
    );
    assert_eq!(readings.snapshot(), before);
  }
}

fn send_generic_reading(
  simulator: &mut Simulator,
  input: &str,
  connected: bool,
) -> anyhow::Result<()> {
  let command =
    crate::commands::parse_command(input).map_err(anyhow::Error::msg)?;
  let crate::commands::UserCommand::SendMeterValue {
    connector,
    measurand,
    update,
  } = command
  else {
    panic!("expected a measurement send");
  };
  simulator.handle_common_command(
    SimulatorCommand::SendMeterValue {
      connector,
      measurand,
      update,
    },
    connected,
  )
}

#[test]
fn send_measurand_sends_only_the_named_reading_with_protocol_context() {
  for protocol in PROTOCOLS {
    for active in [false, true] {
      let mut simulator = simulator_for_tests_with_protocol(protocol);
      if active {
        simulator
          .start_transaction(1, "TOKEN".to_string(), false, None, false)
          .unwrap();
        if protocol == OcppVersion::V1_6 {
          simulator.set_v1_6_transaction_id(1, 1, 123).unwrap();
        }
      }
      apply_generic_update(
        &mut simulator,
        "measurand 1 Power.Active.Import 7200",
      )
      .unwrap();
      for (suffix, expected) in [
        ("", 7200),
        (" 7300", 7300),
        (" +=200", 7500),
        (" -=100", 7400),
      ] {
        simulator.queue.clear();
        send_generic_reading(
          &mut simulator,
          &format!("send-measurand 1 pOWER.aCTIVE.iMPORT{suffix}"),
          true,
        )
        .unwrap();
        let action = if active && protocol != OcppVersion::V1_6 {
          "TransactionEvent"
        } else {
          "MeterValues"
        };
        let payload = queued_payload(&simulator, action);
        let schema = if protocol == OcppVersion::V1_6 {
          format!("schemas/1.6/{action}.json")
        } else {
          format!("{}/{action}Request.json", v2_x_schema_dir(protocol))
        };
        assert_schema_valid(&schema, &payload);
        let samples =
          payload["meterValue"][0]["sampledValue"].as_array().unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0]["measurand"], "Power.Active.Import");
        let expected_value = if protocol == OcppVersion::V1_6 {
          json!(expected.to_string())
        } else {
          json!(expected)
        };
        assert_eq!(samples[0]["value"], expected_value);
        if active && protocol == OcppVersion::V1_6 {
          assert_eq!(payload["transactionId"], 123);
        } else if active {
          assert_eq!(payload["eventType"], "Updated");
          assert_eq!(payload["evse"]["id"], 1);
        }
        assert_eq!(
          simulator.connectors[&1]
            .meter_values
            .get(Measurand::PowerActiveImport)
            .unwrap()
            .value
            .as_i64(),
          Some(expected)
        );
      }
    }
  }
}

#[test]
fn send_measurand_keeps_valid_updates_when_sending_is_unavailable() {
  for connected in [false, true] {
    let mut simulator = simulator_for_tests();
    simulator.boot_registration_status = BootRegistrationStatus::Pending;
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Current.Import 16",
      connected,
    )
    .unwrap();
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Current.Import +=2",
      connected,
    )
    .unwrap();
    assert_eq!(
      simulator.connectors[&1]
        .meter_values
        .get(Measurand::CurrentImport)
        .unwrap()
        .value
        .as_i64(),
      Some(18)
    );
    assert!(simulator.queue.is_empty());
  }
}

#[test]
fn send_measurand_rejects_unset_expired_and_unsupported_readings() {
  let mut simulator = simulator_for_tests();
  assert!(
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Current.Import",
      true,
    )
    .is_err()
  );
  assert!(
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Power.Active.Setpoint -100",
      true,
    )
    .is_err()
  );
  assert!(
    simulator.connectors[&1]
      .meter_values
      .get(Measurand::PowerActiveSetpoint)
      .is_none()
  );

  apply_generic_update(&mut simulator, "measurand 1 Current.Import 16")
    .unwrap();
  simulator
    .start_transaction(1, "TOKEN".to_string(), false, None, false)
    .unwrap();
  simulator.stop_transaction(1, None, false, false).unwrap();
  assert!(
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Current.Import",
      true,
    )
    .is_err()
  );
  assert!(
    send_generic_reading(
      &mut simulator,
      "send-measurand 1 Current.Import +=1",
      true,
    )
    .is_err()
  );
  assert!(simulator.queue.is_empty());
}

#[test]
fn send_measurand_invalid_updates_cancel_sending_without_mutation() {
  let mut simulator = simulator_for_tests_with_protocol(OcppVersion::V2_1);
  apply_generic_update(&mut simulator, "measurand 1 SoC 95").unwrap();
  apply_generic_update(&mut simulator, "measurand 1 Display.InletHot 1")
    .unwrap();
  apply_generic_update(
    &mut simulator,
    "measurand 1 Energy.Active.Import.Register 9223372036854775807",
  )
  .unwrap();
  let before = simulator.connectors[&1].meter_values.snapshot();
  for input in [
    "send-measurand 1 SoC +=10",
    "send-measurand 1 Display.InletHot -=1",
    "send-measurand 1 Energy.Active.Import.Register +=1",
  ] {
    assert!(send_generic_reading(&mut simulator, input, true).is_err());
    assert_eq!(simulator.connectors[&1].meter_values.snapshot(), before);
    assert!(simulator.queue.is_empty());
  }
}
