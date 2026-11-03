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
      Measurand::Soc => MeterReadingLifetime::Transaction,
      _ => MeterReadingLifetime::Connector,
    }
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
    let reading = Self {
      measurand: Measurand::Soc,
      value,
      unit: MeterUnit::Percent.as_str().to_string(),
      location: Some(MeterValueLocation::Ev),
    };
    reading.validate()?;
    Ok(reading)
  }

  /// Checks the invariants needed by the built-in measurement commands.
  fn validate(&self) -> Result<(), String> {
    match self.measurand {
      Measurand::Soc => {
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
      _ => {}
    }
    Ok(())
  }
}

/// An absolute reading or an adjustment in the stored reading's unit.
#[derive(Debug, Clone, PartialEq)]
pub enum MeterUpdate {
  Set(MeterReading),
  Adjust(Measurand, Number),
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

  /// Computes a validated adjusted reading without modifying stored state.
  ///
  /// Integer readings use checked integer addition to preserve precision.
  pub fn adjusted(
    &self,
    measurand: Measurand,
    delta: &Number,
  ) -> Result<MeterReading, String> {
    let mut reading = self.get(measurand).cloned().ok_or_else(|| {
      format!(
        "No {} reading is set. Set it before adjusting.",
        measurand.as_str()
      )
    })?;
    let out_of_range =
      || format!("{} adjustment is out of range.", measurand.as_str());
    reading.value = if let (Some(value), Some(delta)) =
      (reading.value.as_i64(), delta.as_i64())
    {
      value
        .checked_add(delta)
        .map(Number::from)
        .ok_or_else(out_of_range)?
    } else {
      let value = reading.value.as_f64().ok_or_else(out_of_range)?;
      let delta = delta.as_f64().ok_or_else(out_of_range)?;
      Number::from_f64(value + delta).ok_or_else(out_of_range)?
    };
    reading.validate()?;
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
