use std::collections::BTreeMap;

use serde_json::Number;

use crate::ocpp::{Measurand, MeterUnit, MeterValueLocation, OcppVersion};

/// Simulator retention policy for a stored reading, independent of wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterReadingLifetime {
  /// Persists until changed or the connector is removed.
  Connector,
  /// Belongs to the current transaction, or seeds the next one while idle.
  Transaction,
}

/// One locally stored measurement, independent of OCPP payload encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterReading {
  pub measurand: Measurand,
  pub value: Number,
  pub unit: String,
  pub location: Option<MeterValueLocation>,
}

impl MeterReading {
  /// Returns the retention policy shared by all commands for this measurand.
  pub fn lifetime(&self) -> MeterReadingLifetime {
    match self.measurand {
      Measurand::EnergyActiveExportRegister
      | Measurand::EnergyActiveImportRegister
      | Measurand::EnergyActiveImportLocalGenerationRegister
      | Measurand::EnergyActiveNet
      | Measurand::EnergyApparentExport
      | Measurand::EnergyApparentImport
      | Measurand::EnergyApparentNet
      | Measurand::EnergyReactiveExportRegister
      | Measurand::EnergyReactiveImportRegister
      | Measurand::EnergyReactiveNet
      | Measurand::Frequency
      | Measurand::Temperature
      | Measurand::Rpm => MeterReadingLifetime::Connector,
      Measurand::CurrentExport
      | Measurand::CurrentExportOffered
      | Measurand::CurrentExportMinimum
      | Measurand::CurrentImport
      | Measurand::CurrentImportOffered
      | Measurand::CurrentImportMinimum
      | Measurand::CurrentOffered
      | Measurand::DisplayPresentSoc
      | Measurand::DisplayMinimumSoc
      | Measurand::DisplayTargetSoc
      | Measurand::DisplayMaximumSoc
      | Measurand::DisplayRemainingTimeToMinimumSoc
      | Measurand::DisplayRemainingTimeToTargetSoc
      | Measurand::DisplayRemainingTimeToMaximumSoc
      | Measurand::DisplayChargingComplete
      | Measurand::DisplayBatteryEnergyCapacity
      | Measurand::DisplayInletHot
      | Measurand::EnergyActiveExportInterval
      | Measurand::EnergyActiveImportInterval
      | Measurand::EnergyActiveImportCableLoss
      | Measurand::EnergyActiveSetpointInterval
      | Measurand::EnergyReactiveExportInterval
      | Measurand::EnergyReactiveImportInterval
      | Measurand::EnergyRequestTarget
      | Measurand::EnergyRequestMinimum
      | Measurand::EnergyRequestMaximum
      | Measurand::EnergyRequestMinimumV2X
      | Measurand::EnergyRequestMaximumV2X
      | Measurand::EnergyRequestBulk
      | Measurand::PowerActiveExport
      | Measurand::PowerActiveImport
      | Measurand::PowerActiveSetpoint
      | Measurand::PowerActiveResidual
      | Measurand::PowerExportMinimum
      | Measurand::PowerExportOffered
      | Measurand::PowerFactor
      | Measurand::PowerImportOffered
      | Measurand::PowerImportMinimum
      | Measurand::PowerOffered
      | Measurand::PowerReactiveExport
      | Measurand::PowerReactiveImport
      | Measurand::Soc
      | Measurand::Voltage
      | Measurand::VoltageMinimum
      | Measurand::VoltageMaximum => MeterReadingLifetime::Transaction,
    }
  }

  /// Builds a reading in this command's canonical OCPP unit and location.
  pub fn new(measurand: Measurand, value: Number) -> Result<Self, String> {
    let reading = Self {
      measurand,
      value,
      unit: default_unit(measurand).to_string(),
      location: default_location(measurand),
    };
    reading.validate()?;
    Ok(reading)
  }

  /// Builds an energy register reading without losing integer precision.
  pub fn energy(value_wh: i64) -> Self {
    Self {
      measurand: Measurand::EnergyActiveImportRegister,
      value: value_wh.into(),
      unit: MeterUnit::Wh.as_str().to_string(),
      location: None,
    }
  }

  /// Builds an EV state-of-charge reading in percent, from 0 through 100.
  pub fn soc(percent: f64) -> Result<Self, String> {
    let value = Number::from_f64(percent).ok_or_else(|| {
      "SoC must be a finite percentage from 0 to 100.".to_string()
    })?;
    Self::new(Measurand::Soc, value)
  }

  /// Checks the invariants needed by the built-in measurement commands.
  fn validate(&self) -> Result<(), String> {
    match self.measurand {
      Measurand::Soc
      | Measurand::DisplayPresentSoc
      | Measurand::DisplayMinimumSoc
      | Measurand::DisplayTargetSoc
      | Measurand::DisplayMaximumSoc => {
        if self.unit != MeterUnit::Percent.as_str()
          || !self
            .value
            .as_f64()
            .is_some_and(|value| (0.0..=100.0).contains(&value))
        {
          return Err("SoC must be a percentage from 0 to 100.".to_string());
        }
      }
      // OCPP 1.6 transaction boundaries require an integer Wh register.
      Measurand::EnergyActiveImportRegister
        if self.unit != MeterUnit::Wh.as_str()
          || self.value.as_i64().is_none() =>
      {
        return Err(
          "The energy register must be an integer in Wh.".to_string(),
        );
      }
      Measurand::DisplayChargingComplete | Measurand::DisplayInletHot
        if !self
          .value
          .as_f64()
          .is_some_and(|value| [0.0, 1.0].contains(&value)) =>
      {
        return Err("Display flags must be 0 or 1.".to_string());
      }
      Measurand::PowerFactor
        if !self
          .value
          .as_f64()
          .is_some_and(|value| (-1.0..=1.0).contains(&value)) =>
      {
        return Err("Power.Factor must be a ratio from -1 to 1.".to_string());
      }
      _ => {}
    }
    Ok(())
  }
}

/// A signed amount to add to a reading in its stored unit.
///
/// Integer deltas are wider than JSON numbers so negating any supported
/// unsigned reading remains exact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeterDelta {
  Integer(i128),
  Float(f64),
}

impl MeterDelta {
  /// Reverses a delta's sign, rejecting overflow and non-finite values.
  pub fn negated(self) -> Result<Self, String> {
    match self {
      Self::Integer(value) => value.checked_neg().map(Self::Integer),
      Self::Float(value) if value.is_finite() => Some(Self::Float(-value)),
      Self::Float(_) => None,
    }
    .ok_or_else(|| "Numeric adjustment is out of range.".to_string())
  }

  fn is_zero(self) -> bool {
    match self {
      Self::Integer(value) => value == 0,
      Self::Float(value) => value == 0.0,
    }
  }

  fn float_value(self) -> Result<f64, String> {
    match self {
      Self::Integer(value) => exact_integer_float(value),
      Self::Float(value) if value.is_finite() => Ok(value),
      Self::Float(_) => Err("Numeric adjustment is out of range.".to_string()),
    }
  }
}

impl From<Number> for MeterDelta {
  fn from(value: Number) -> Self {
    integer_value(&value).map_or_else(
      || Self::Float(value.as_f64().expect("JSON numbers are finite")),
      Self::Integer,
    )
  }
}

impl From<i32> for MeterDelta {
  fn from(value: i32) -> Self {
    Self::Integer(value.into())
  }
}

impl From<i64> for MeterDelta {
  fn from(value: i64) -> Self {
    Self::Integer(value.into())
  }
}

impl From<u64> for MeterDelta {
  fn from(value: u64) -> Self {
    Self::Integer(value.into())
  }
}

/// An absolute reading or a signed adjustment in the stored reading's unit.
#[derive(Debug, Clone, PartialEq)]
pub enum MeterUpdate {
  Set(MeterReading),
  Adjust(Measurand, MeterDelta),
}

/// Connector readings keyed by measurand, with an initialized energy register.
#[derive(Debug, Clone)]
pub struct MeterReadings {
  readings: BTreeMap<Measurand, MeterReading>,
}

impl Default for MeterReadings {
  fn default() -> Self {
    let energy = MeterReading::energy(0);
    Self {
      readings: BTreeMap::from([(energy.measurand, energy)]),
    }
  }
}

impl MeterReadings {
  /// Validates and replaces one reading without changing other measurands.
  pub fn set(&mut self, reading: MeterReading) -> Result<(), String> {
    reading.validate()?;
    self.readings.insert(reading.measurand, reading);
    Ok(())
  }

  /// Resets configured cable-loss energy at the start of a transaction.
  ///
  /// OCPP 2.1 defines cable loss as a per-transaction counter. Other prepared
  /// readings, including `SoC`, retain their supplied starting values.
  pub fn begin_transaction(&mut self) {
    if let Some(reading) = self
      .readings
      .get_mut(&Measurand::EnergyActiveImportCableLoss)
    {
      reading.value = 0.into();
    }
  }

  /// Clears readings scoped to a transaction after it ends or is cancelled.
  ///
  /// Call when finalizing the matching transaction, not when queuing a stop.
  pub fn clear_transaction_readings(&mut self) {
    self.readings.retain(|_, reading| {
      reading.lifetime() != MeterReadingLifetime::Transaction
    });
  }

  /// Returns a previously set reading, or none when it is unknown.
  pub fn get(&self, measurand: Measurand) -> Option<&MeterReading> {
    self.readings.get(&measurand)
  }

  /// Returns a validated reading after adding a signed delta without mutation.
  ///
  /// Unset readings, boolean flags, invalid results, and overflow are rejected.
  pub fn adjusted(
    &self,
    measurand: Measurand,
    delta: &MeterDelta,
  ) -> Result<MeterReading, String> {
    if matches!(
      measurand,
      Measurand::DisplayChargingComplete | Measurand::DisplayInletHot
    ) {
      return Err(format!(
        "{} supports absolute assignments only.",
        measurand.as_str(),
      ));
    }
    let mut reading = self.get(measurand).cloned().ok_or_else(|| {
      format!(
        "No {} reading is set. Set it before adjusting.",
        measurand.as_str(),
      )
    })?;
    reading.value = checked_adjustment(&reading.value, delta)?;
    reading.validate().map_err(|error| {
      format!("{} adjustment is out of range: {error}", measurand.as_str())
    })?;
    Ok(reading)
  }

  /// Snapshots all known readings in a stable order for outbound messages.
  pub fn snapshot(&self) -> Vec<MeterReading> {
    self.readings.values().cloned().collect()
  }

  /// Snapshots only readings defined by the selected OCPP version.
  pub fn for_protocol(&self, protocol: OcppVersion) -> Vec<MeterReading> {
    self
      .readings
      .values()
      .filter(|reading| reading.measurand.is_supported(protocol))
      .cloned()
      .collect()
  }

  /// Returns the integer Wh register used by OCPP 1.6 transaction boundaries.
  pub fn energy_wh(&self) -> i64 {
    self.readings[&Measurand::EnergyActiveImportRegister]
      .value
      .as_i64()
      .expect("the energy register is initialized and validated on update")
  }
}

/// Canonical input units; OCPP also permits scaled alternatives on the wire.
fn default_unit(measurand: Measurand) -> &'static str {
  match measurand {
    Measurand::CurrentExport
    | Measurand::CurrentExportOffered
    | Measurand::CurrentExportMinimum
    | Measurand::CurrentImport
    | Measurand::CurrentImportOffered
    | Measurand::CurrentImportMinimum
    | Measurand::CurrentOffered => "A",
    Measurand::DisplayPresentSoc
    | Measurand::DisplayMinimumSoc
    | Measurand::DisplayTargetSoc
    | Measurand::DisplayMaximumSoc
    | Measurand::Soc => "Percent",
    Measurand::DisplayRemainingTimeToMinimumSoc
    | Measurand::DisplayRemainingTimeToTargetSoc
    | Measurand::DisplayRemainingTimeToMaximumSoc => "s",
    Measurand::DisplayChargingComplete
    | Measurand::DisplayInletHot
    | Measurand::PowerFactor => "",
    Measurand::DisplayBatteryEnergyCapacity
    | Measurand::EnergyActiveExportInterval
    | Measurand::EnergyActiveExportRegister
    | Measurand::EnergyActiveImportInterval
    | Measurand::EnergyActiveImportRegister
    | Measurand::EnergyActiveImportCableLoss
    | Measurand::EnergyActiveImportLocalGenerationRegister
    | Measurand::EnergyActiveNet
    | Measurand::EnergyActiveSetpointInterval
    | Measurand::EnergyRequestTarget
    | Measurand::EnergyRequestMinimum
    | Measurand::EnergyRequestMaximum
    | Measurand::EnergyRequestMinimumV2X
    | Measurand::EnergyRequestMaximumV2X
    | Measurand::EnergyRequestBulk => "Wh",
    Measurand::EnergyApparentExport
    | Measurand::EnergyApparentImport
    | Measurand::EnergyApparentNet => "VAh",
    Measurand::EnergyReactiveExportInterval
    | Measurand::EnergyReactiveExportRegister
    | Measurand::EnergyReactiveImportInterval
    | Measurand::EnergyReactiveImportRegister
    | Measurand::EnergyReactiveNet => "varh",
    Measurand::Frequency => "Hz",
    Measurand::PowerActiveExport
    | Measurand::PowerActiveImport
    | Measurand::PowerActiveSetpoint
    | Measurand::PowerActiveResidual
    | Measurand::PowerExportMinimum
    | Measurand::PowerExportOffered
    | Measurand::PowerImportOffered
    | Measurand::PowerImportMinimum
    | Measurand::PowerOffered => "W",
    Measurand::PowerReactiveExport | Measurand::PowerReactiveImport => "var",
    Measurand::Voltage
    | Measurand::VoltageMinimum
    | Measurand::VoltageMaximum => "V",
    Measurand::Temperature => "Celsius",
    Measurand::Rpm => "RPM",
  }
}

/// Vehicle parameters are distinguished from station diagnostics and outlets.
fn default_location(measurand: Measurand) -> Option<MeterValueLocation> {
  match measurand {
    Measurand::DisplayPresentSoc
    | Measurand::DisplayMinimumSoc
    | Measurand::DisplayTargetSoc
    | Measurand::DisplayMaximumSoc
    | Measurand::DisplayRemainingTimeToMinimumSoc
    | Measurand::DisplayRemainingTimeToTargetSoc
    | Measurand::DisplayRemainingTimeToMaximumSoc
    | Measurand::DisplayChargingComplete
    | Measurand::DisplayBatteryEnergyCapacity
    | Measurand::DisplayInletHot
    | Measurand::EnergyRequestTarget
    | Measurand::EnergyRequestMinimum
    | Measurand::EnergyRequestMaximum
    | Measurand::EnergyRequestMinimumV2X
    | Measurand::EnergyRequestMaximumV2X
    | Measurand::EnergyRequestBulk
    | Measurand::Soc => Some(MeterValueLocation::Ev),
    Measurand::Frequency => Some(MeterValueLocation::Inlet),
    Measurand::Temperature | Measurand::Rpm => Some(MeterValueLocation::Body),
    _ => None,
  }
}

/// Interprets either JSON integer representation without converting to a float.
fn integer_value(value: &Number) -> Option<i128> {
  value
    .as_i64()
    .map(i128::from)
    .or_else(|| value.as_u64().map(i128::from))
}

/// Adds a signed delta within the JSON number range using exact integer math.
fn checked_adjustment(
  value: &Number,
  delta: &MeterDelta,
) -> Result<Number, String> {
  let out_of_range = || "Numeric adjustment is out of range.".to_string();
  if let (Some(value), MeterDelta::Integer(delta)) =
    (integer_value(value), delta)
  {
    let result = value.checked_add(*delta).ok_or_else(out_of_range)?;
    return i64::try_from(result)
      .map(Number::from)
      .or_else(|_| u64::try_from(result).map(Number::from))
      .map_err(|_| out_of_range());
  }

  // Zero is a no-op even when a large integer cannot safely become a float.
  if delta.is_zero() {
    return Ok(value.clone());
  }
  let value = float_value_for_adjustment(value)?;
  let result = value + delta.float_value()?;
  if result.total_cmp(&value).is_eq() {
    return Err(
      "Adjustment is too small to represent without losing precision."
        .to_string(),
    );
  }
  Number::from_f64(result).ok_or_else(out_of_range)
}

/// Rejects float conversions that would round an integer reading.
fn float_value_for_adjustment(value: &Number) -> Result<f64, String> {
  if let Some(integer) = integer_value(value) {
    return exact_integer_float(integer);
  }
  value
    .as_f64()
    .ok_or_else(|| "Numeric adjustment is out of range.".to_string())
}

/// Converts an integer only when all significant bits fit in a float.
fn exact_integer_float(integer: i128) -> Result<f64, String> {
  let magnitude = integer.unsigned_abs();
  let significant_bits = magnitude.bit_width();
  if magnitude.trailing_zeros() < significant_bits.saturating_sub(53) {
    return Err("Adjustment would lose integer precision.".to_string());
  }
  // The representability check above proves this numeric cast is exact.
  #[allow(clippy::cast_precision_loss)]
  let value = integer as f64;
  Ok(value)
}
