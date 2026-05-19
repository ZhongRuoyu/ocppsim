use super::super::*;

impl Simulator {
  /// Updates a configuration value and applies key-specific side effects.
  pub(in crate::simulator) fn set_configuration_value(
    &mut self,
    key: &str,
    value: &str,
  ) -> ResponseStatus {
    let Some(entry) = self.configuration.get(key) else {
      return ResponseStatus::NotSupported;
    };
    if entry.read_only {
      return ResponseStatus::Rejected;
    }

    if key == "HeartbeatInterval" {
      let Some(seconds) = value.parse::<u64>().ok().filter(|item| *item > 0)
      else {
        return ResponseStatus::Rejected;
      };
      if let Some(entry) = self.configuration.get_mut(key) {
        entry.value = value.to_string();
      }
      if self.heartbeat.is_some() {
        self.start_heartbeat(seconds);
      }
      return ResponseStatus::Accepted;
    }

    if let Some(entry) = self.configuration.get_mut(key) {
      entry.value = value.to_string();
    }
    ResponseStatus::Accepted
  }

  /// Finds a configuration entry by case-insensitive variable name.
  pub(in crate::simulator) fn configuration_entry(
    &self,
    variable: &str,
  ) -> Option<(&str, &ConfigurationEntry)> {
    self
      .configuration
      .iter()
      .find(|(key, _)| key.eq_ignore_ascii_case(variable))
      .map(|(key, value)| (key.as_str(), value))
  }

  /// Finds the canonical configuration key for a variable name.
  pub(in crate::simulator) fn configuration_key(
    &self,
    variable: &str,
  ) -> Option<String> {
    self
      .configuration
      .keys()
      .find(|key| key.eq_ignore_ascii_case(variable))
      .cloned()
  }

  /// Returns true when any active reservation targets this connector.
  pub(in crate::simulator) fn connector_has_reservation(
    &self,
    connector: u16,
  ) -> bool {
    self.reservations.values().any(|item| *item == connector)
  }

  /// Returns whether OCPP 1.6 remote starts must authorize before starting.
  pub(in crate::simulator) fn authorize_remote_tx_requests(&self) -> bool {
    self
      .configuration
      .get("AuthorizeRemoteTxRequests")
      .map(|entry| entry.value.eq_ignore_ascii_case("true"))
      .unwrap_or(true)
  }

  /// Finds the first connector currently eligible for a new transaction.
  pub(in crate::simulator) fn first_startable_connector(&self) -> Option<u16> {
    self.connectors.iter().find_map(|(connector, _)| {
      if self.validate_start_connector(*connector).is_ok() {
        Some(*connector)
      } else {
        None
      }
    })
  }

  /// Validates connector state before starting a transaction.
  pub(in crate::simulator) fn validate_start_connector(
    &self,
    connector: u16,
  ) -> Result<()> {
    let state = self
      .connectors
      .get(&connector)
      .ok_or_else(|| anyhow!("Connector {} does not exist.", connector))?;
    if state.transaction.is_some() {
      return Err(anyhow!(
        "Connector {} already has an active transaction.",
        connector
      ));
    }
    if self.connector_has_reservation(connector) {
      return Err(anyhow!("Connector {} is reserved.", connector));
    }
    match state.status {
      ConnectorStatus::Available | ConnectorStatus::Preparing => Ok(()),
      other => Err(anyhow!(
        "Connector {} is not startable while {}.",
        connector,
        other.display()
      )),
    }
  }

  /// Applies an immediate availability state to one connector.
  pub(in crate::simulator) fn apply_availability_status(
    &mut self,
    connector: u16,
    target_status: ConnectorStatus,
  ) -> Result<()> {
    let has_reservation = self.connector_has_reservation(connector);
    let connector_state = self.connector_mut(connector)?;
    connector_state.scheduled_availability = None;
    connector_state.status =
      if target_status == ConnectorStatus::Available && has_reservation {
        ConnectorStatus::Reserved
      } else {
        target_status
      };
    Ok(())
  }

  /// Stores an availability state to apply after the current transaction.
  pub(in crate::simulator) fn schedule_availability_status(
    &mut self,
    connector: u16,
    target_status: ConnectorStatus,
  ) -> Result<()> {
    self.connector_mut(connector)?.scheduled_availability = Some(target_status);
    Ok(())
  }

  /// Returns connector status after a transaction is no longer active.
  pub(in crate::simulator) fn inactive_connector_status(
    &mut self,
    connector: u16,
    default_status: ConnectorStatus,
  ) -> Result<ConnectorStatus> {
    let has_reservation = self.connector_has_reservation(connector);
    let connector_state = self.connector_mut(connector)?;
    if let Some(status) = connector_state.scheduled_availability.take() {
      return Ok(status);
    }
    if has_reservation {
      return Ok(ConnectorStatus::Reserved);
    }
    Ok(default_status)
  }

  /// Increments the transaction sequence number for one active transaction.
  pub(in crate::simulator) fn bump_seq_no(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    let connector_state = self.connector_mut(connector)?;
    let tx = connector_state.transaction.as_mut().ok_or_else(|| {
      anyhow!("No active transaction on connector {}.", connector)
    })?;
    if tx.local_id != local_tx_id {
      return Err(anyhow!(
        "Transaction mismatch on connector {} (expected {}, got {}).",
        connector,
        local_tx_id,
        tx.local_id,
      ));
    }
    tx.seq_no = tx.seq_no.saturating_add(1);
    Ok(())
  }

  /// Stores the transaction id returned by OCPP 1.6 StartTransaction.
  pub(in crate::simulator) fn set_v1_6_transaction_id(
    &mut self,
    connector: u16,
    local_tx_id: u64,
    transaction_id: i64,
  ) -> Result<()> {
    let connector_state = self.connector_mut(connector)?;
    let tx = connector_state.transaction.as_mut().ok_or_else(|| {
      anyhow!("No active transaction on connector {}.", connector)
    })?;
    if tx.local_id != local_tx_id {
      return Err(anyhow!(
        "Transaction mismatch on connector {} (expected {}, got {}).",
        connector,
        local_tx_id,
        tx.local_id,
      ));
    }
    tx.v1_6_transaction_id = Some(transaction_id);
    Ok(())
  }

  /// Rolls back local start state when start is rejected or fails.
  pub(in crate::simulator) fn cancel_transaction_start(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    let should_cancel = self
      .connectors
      .get(&connector)
      .ok_or_else(|| anyhow!("Connector {} does not exist.", connector))?
      .transaction
      .as_ref()
      .is_some_and(|tx| tx.local_id == local_tx_id);
    if should_cancel {
      let target_status = self
        .inactive_connector_status(connector, ConnectorStatus::Available)?;
      let connector_state = self.connector_mut(connector)?;
      connector_state.transaction = None;
      connector_state.status = target_status;
    }
    Ok(())
  }

  /// Completes a stop/end flow once the CSMS acknowledges the terminal event.
  pub(in crate::simulator) fn complete_transaction_stop(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    let should_complete = self
      .connectors
      .get(&connector)
      .ok_or_else(|| anyhow!("Connector {} does not exist.", connector))?
      .transaction
      .as_ref()
      .is_some_and(|tx| tx.local_id == local_tx_id);
    if !should_complete {
      return Ok(());
    }
    let target_status =
      self.inactive_connector_status(connector, ConnectorStatus::Finishing)?;
    let connector_state = self.connector_mut(connector)?;
    connector_state.transaction = None;
    connector_state.status = target_status;
    Ok(())
  }

  /// Restores the active status after a stop/end request fails or times out.
  pub(in crate::simulator) fn restore_active_transaction_status(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    let protocol = self.config.protocol;
    let connector_state = self.connector_mut(connector)?;
    let Some(transaction) = connector_state.transaction.as_ref() else {
      return Ok(());
    };
    if transaction.local_id != local_tx_id {
      return Ok(());
    }
    connector_state.status = if protocol == OcppVersion::V1_6 {
      ConnectorStatus::Charging
    } else {
      ConnectorStatus::Occupied
    };
    Ok(())
  }

  /// Finds a connector by OCPP 1.6 transaction id or local fallback id.
  pub(in crate::simulator) fn find_v1_6_transaction(
    &self,
    transaction_id: i64,
  ) -> Option<u16> {
    self.connectors.iter().find_map(|(connector, state)| {
      state.transaction.as_ref().and_then(|tx| {
        if tx.v1_6_transaction_id == Some(transaction_id)
          || tx.local_id as i64 == transaction_id
        {
          Some(*connector)
        } else {
          None
        }
      })
    })
  }

  /// Returns the active OCPP 2.x transaction id for one connector.
  pub(in crate::simulator) fn active_transaction_uid(
    &self,
    connector: u16,
  ) -> Option<String> {
    self.connectors.get(&connector).and_then(|state| {
      state
        .transaction
        .as_ref()
        .map(|tx| tx.transaction_uid.clone())
    })
  }

  /// Finds a connector by OCPP 2.x transaction id.
  pub(in crate::simulator) fn find_transaction_by_uid(
    &self,
    transaction_uid: &str,
  ) -> Option<u16> {
    self.connectors.iter().find_map(|(connector, state)| {
      state.transaction.as_ref().and_then(|tx| {
        if tx.transaction_uid == transaction_uid {
          Some(*connector)
        } else {
          None
        }
      })
    })
  }

  /// Returns a shared connector state or an error if connector is unknown.
  pub(in crate::simulator) fn connector_ref(
    &self,
    connector: u16,
  ) -> Result<&ConnectorState> {
    self
      .connectors
      .get(&connector)
      .ok_or_else(|| anyhow!("Connector {} does not exist.", connector))
  }

  /// Returns a mutable connector state or an error if connector is unknown.
  pub(in crate::simulator) fn connector_mut(
    &mut self,
    connector: u16,
  ) -> Result<&mut ConnectorState> {
    self
      .connectors
      .get_mut(&connector)
      .ok_or_else(|| anyhow!("Connector {} does not exist.", connector))
  }

  /// Sets the offered power limit for a connector.
  pub(in crate::simulator) fn set_offered_limit(
    &mut self,
    connector: u16,
    limit: Option<f64>,
  ) -> Result<()> {
    let state = self.connector_mut(connector)?;
    state.offered_limit = limit;
    Ok(())
  }

  /// Applies connector status transitions implied by charging profile
  /// enable/disable.
  pub(in crate::simulator) fn apply_charging_profile_state(
    &mut self,
    connector: u16,
  ) -> Result<()> {
    let Some(current) = self.connectors.get(&connector).map(|item| item.status)
    else {
      return Ok(());
    };
    let offered_limit = self
      .connectors
      .get(&connector)
      .and_then(|item| item.offered_limit);
    let has_active_tx = self
      .connectors
      .get(&connector)
      .and_then(|item| item.transaction.as_ref())
      .is_some();
    let target = if has_active_tx {
      if offered_limit.is_some_and(|limit| limit <= 0.0) {
        ConnectorStatus::SuspendedEvse
      } else if self.config.protocol == OcppVersion::V1_6 {
        ConnectorStatus::Charging
      } else {
        ConnectorStatus::Occupied
      }
    } else {
      match current {
        ConnectorStatus::Reserved
        | ConnectorStatus::Unavailable
        | ConnectorStatus::Faulted => current,
        _ => ConnectorStatus::Available,
      }
    };

    if target == current {
      return Ok(());
    }

    let state = self.connector_mut(connector)?;
    state.status = target;
    self.enqueue_status_notification(connector)?;
    self.emit_snapshot();
    Ok(())
  }
}
