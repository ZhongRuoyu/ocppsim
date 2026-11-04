use crate::metering::{MeterReading, MeterUpdate};
use crate::ocpp::Measurand;

use super::super::{
  OcppVersion, Result, Simulator, TransactionEventRequest,
  TransactionTriggerReason, TxEventType, UiLogLevel, anyhow,
};

impl Simulator {
  /// Displays one measurand for selected connectors without sending.
  pub(in crate::simulator) fn show_meter_value(
    &self,
    connector: Option<u16>,
    measurand: Measurand,
  ) -> Result<()> {
    self.ensure_measurand_supported(measurand)?;
    let connectors = connector.map_or_else(
      || self.connectors.keys().copied().collect(),
      |connector| vec![connector],
    );
    let label = if measurand == Measurand::EnergyActiveImportRegister {
      "meter"
    } else {
      measurand.as_str()
    };
    for connector in connectors {
      let state = self.connector_ref(connector)?;
      self.log_meter_reading(
        connector,
        label,
        state.meter_values.get(measurand),
      );
    }
    Ok(())
  }

  /// Shows set readings, optionally selecting a connector or measurand.
  pub(in crate::simulator) fn show_measurands(
    &self,
    connector: Option<u16>,
    measurand: Option<Measurand>,
  ) -> Result<()> {
    if let Some(measurand) = measurand {
      self.ensure_measurand_supported(measurand)?;
    }
    let connectors = connector.map_or_else(
      || self.connectors.keys().copied().collect(),
      |connector| vec![connector],
    );
    for connector in connectors {
      let readings = &self.connector_ref(connector)?.meter_values;
      if let Some(measurand) = measurand {
        self.log_meter_reading(
          connector,
          measurand.as_str(),
          readings.get(measurand),
        );
      } else {
        for reading in readings.for_protocol(self.config.protocol) {
          self.log_meter_reading(
            connector,
            reading.measurand.as_str(),
            Some(&reading),
          );
        }
      }
    }
    Ok(())
  }

  fn log_meter_reading(
    &self,
    connector: u16,
    label: &str,
    reading: Option<&MeterReading>,
  ) {
    let value = reading.map_or_else(
      || "unset".to_string(),
      |reading| format!("{}{}", reading.value, reading.unit),
    );
    self.log(
      UiLogLevel::Info,
      format!("Connector {connector} {label}={value}"),
    );
  }

  /// Applies a reading or a checked adjustment to the latest local value.
  ///
  /// Unset readings, invalid results, and unknown connectors are not modified.
  pub(in crate::simulator) fn update_meter_value(
    &mut self,
    connector: u16,
    update: MeterUpdate,
  ) -> Result<()> {
    let measurand = match &update {
      MeterUpdate::Set(reading) => reading.measurand,
      MeterUpdate::Adjust(measurand, _) => *measurand,
    };
    self.ensure_measurand_supported(measurand)?;
    let reading = match update {
      MeterUpdate::Set(reading) => reading,
      MeterUpdate::Adjust(measurand, delta) => self
        .connector_ref(connector)?
        .meter_values
        .adjusted(measurand, &delta)
        .map_err(anyhow::Error::msg)?,
    };
    self.set_meter_value(connector, reading)
  }

  /// Stores one connector reading without sending an OCPP message.
  pub(in crate::simulator) fn set_meter_value(
    &mut self,
    connector: u16,
    reading: MeterReading,
  ) -> Result<()> {
    self.ensure_measurand_supported(reading.measurand)?;
    let message = format!(
      "Connector {connector} {} set to {} {}",
      reading.measurand.as_str(),
      reading.value,
      reading.unit,
    );
    self
      .connector_mut(connector)?
      .meter_values
      .set(reading)
      .map_err(anyhow::Error::msg)?;
    self.log(UiLogLevel::Info, message);
    Ok(())
  }

  /// Applies an optional update before checking whether its report can be sent.
  pub(in crate::simulator) fn send_meter_update(
    &mut self,
    connector: u16,
    measurand: Measurand,
    update: Option<MeterUpdate>,
    is_connected: bool,
  ) -> Result<()> {
    if let Some(update) = update {
      self.update_meter_value(connector, update)?;
    }
    if is_connected
      && !self.ensure_post_boot_ocpp_requests_allowed("MeterValues")
    {
      return Ok(());
    }
    self.send_meter_value(connector, measurand, is_connected)
  }

  /// Sends one stored measurand using the protocol's transaction context.
  pub(in crate::simulator) fn send_meter_value(
    &mut self,
    connector: u16,
    measurand: Measurand,
    is_connected: bool,
  ) -> Result<()> {
    if !is_connected {
      self.log(
        UiLogLevel::Warn,
        "Not connected. Connect first to send MeterValues.",
      );
      return Ok(());
    }

    self.ensure_measurand_supported(measurand)?;
    let state = self.connector_ref(connector)?;
    let reading =
      state.meter_values.get(measurand).cloned().ok_or_else(|| {
        anyhow!(
          "No {} reading set for connector {connector}. Set it before sending.",
          measurand.as_str(),
        )
      })?;
    let transaction = state.transaction.as_ref();
    let local_tx_id = transaction.map(|transaction| transaction.local_id);
    let remote_start_id =
      transaction.and_then(|transaction| transaction.remote_start_id);
    let readings = [reading];

    match (self.config.protocol, local_tx_id) {
      (OcppVersion::V2_0_1 | OcppVersion::V2_1, Some(local_tx_id)) => {
        self.bump_seq_no(connector, local_tx_id)?;
        self.enqueue_transaction_event_with_readings(
          &TransactionEventRequest {
            connector,
            local_tx_id,
            event_type: TxEventType::Updated,
            trigger_reason: TransactionTriggerReason::MeterValuePeriodic,
            id_token: None,
            remote_start_id,
            stopped_reason: None,
          },
          &readings,
        )?;
      }
      _ => self.enqueue_meter_values_with_readings(connector, &readings)?,
    }
    Ok(())
  }

  fn ensure_measurand_supported(&self, measurand: Measurand) -> Result<()> {
    if !measurand.is_supported(self.config.protocol) {
      return Err(anyhow!(
        "{} is not supported by OCPP {}.",
        measurand.as_str(),
        self.config.protocol.label(),
      ));
    }
    Ok(())
  }
}
