use super::super::{
  ConnectorStatus, ResponseStatus, Result, Simulator, Value, anyhow,
};
use super::request::AvailabilityRequest;

impl Simulator {
  /// Applies shared availability semantics after protocol-specific parsing.
  pub(in crate::simulator) fn apply_change_availability(
    &mut self,
    request: AvailabilityRequest,
  ) -> Result<ResponseStatus> {
    let Some(target_status) = request.target_status else {
      return Ok(ResponseStatus::Rejected);
    };
    let targets: Vec<u16> = if request.connector.unwrap_or(0) == 0 {
      self.connectors.keys().copied().collect()
    } else if let Some(connector) = request.connector
      && self.connectors.contains_key(&connector)
    {
      vec![connector]
    } else {
      return Ok(ResponseStatus::Rejected);
    };

    let mut scheduled = false;
    let mut changed = Vec::new();
    for connector in targets {
      let has_active_tx = self
        .connectors
        .get(&connector)
        .and_then(|item| item.transaction.as_ref())
        .is_some();
      if has_active_tx {
        if target_status == ConnectorStatus::Unavailable {
          self.schedule_availability_status(connector, target_status)?;
          scheduled = true;
          continue;
        }
        self.connector_mut(connector)?.scheduled_availability = None;
        continue;
      }

      self.apply_availability_status(connector, target_status)?;
      changed.push(connector);
    }

    for connector in changed {
      self.enqueue_status_notification(connector)?;
    }
    self.emit_snapshot();

    if scheduled {
      Ok(ResponseStatus::Scheduled)
    } else {
      Ok(ResponseStatus::Accepted)
    }
  }

  /// Applies shared reservation semantics for one connector or auto-pick.
  pub(in crate::simulator) fn reserve_connector(
    &mut self,
    requested_connector: u16,
    reservation_id: i64,
  ) -> Result<ResponseStatus> {
    let connector = if requested_connector == 0 {
      let mut chosen = None;
      for (connector_id, state) in &self.connectors {
        let is_reserved = self.connector_has_reservation(*connector_id);
        let is_available = !matches!(
          state.status,
          ConnectorStatus::Unavailable | ConnectorStatus::Faulted
        );
        if state.transaction.is_none() && !is_reserved && is_available {
          chosen = Some(*connector_id);
          break;
        }
      }
      let Some(connector_id) = chosen else {
        return Ok(ResponseStatus::Occupied);
      };
      connector_id
    } else if self.connectors.contains_key(&requested_connector) {
      requested_connector
    } else {
      return Ok(ResponseStatus::Rejected);
    };

    if self.reservations.contains_key(&reservation_id) {
      return Ok(ResponseStatus::Rejected);
    }

    if self.connector_has_reservation(connector) {
      return Ok(ResponseStatus::Occupied);
    }

    {
      let state = self.connector_mut(connector)?;
      if state.transaction.is_some() {
        return Ok(ResponseStatus::Occupied);
      }
      if state.status == ConnectorStatus::Faulted {
        return Ok(ResponseStatus::Faulted);
      }
      if state.status == ConnectorStatus::Unavailable {
        return Ok(ResponseStatus::Unavailable);
      }
      state.status = ConnectorStatus::Reserved;
    }

    self.reservations.insert(reservation_id, connector);
    self.enqueue_status_notification(connector)?;
    self.emit_snapshot();
    Ok(ResponseStatus::Accepted)
  }

  /// Applies shared reservation cancellation semantics.
  pub(in crate::simulator) fn cancel_reservation(
    &mut self,
    reservation_id: i64,
  ) -> Result<ResponseStatus> {
    let Some(connector) = self.reservations.remove(&reservation_id) else {
      return Ok(ResponseStatus::Rejected);
    };

    let has_active_tx = self
      .connectors
      .get(&connector)
      .and_then(|item| item.transaction.as_ref())
      .is_some();
    if !has_active_tx {
      let state = self.connector_mut(connector)?;
      if state.status == ConnectorStatus::Reserved {
        state.status = ConnectorStatus::Available;
        self.enqueue_status_notification(connector)?;
      }
    }
    self.emit_snapshot();
    Ok(ResponseStatus::Accepted)
  }

  /// Stores the current local auth list version using shared semantics.
  pub(in crate::simulator) fn apply_local_list_version(
    &mut self,
    list_version: i64,
  ) -> ResponseStatus {
    self.local_auth_list_version = list_version;
    ResponseStatus::Accepted
  }

  /// Applies a charging profile to one connector or to all connectors.
  pub(in crate::simulator) fn apply_set_charging_profile(
    &mut self,
    requested_connector: u16,
    profile: &Value,
  ) -> Result<ResponseStatus> {
    let targets: Vec<u16> = if requested_connector == 0 {
      self.connectors.keys().copied().collect()
    } else if self.connectors.contains_key(&requested_connector) {
      vec![requested_connector]
    } else {
      return Ok(ResponseStatus::Rejected);
    };

    let Some(limit) = Self::extract_profile_limit(profile) else {
      return Err(anyhow!(
        "charging profile must include one numeric limit value."
      ));
    };
    for target in targets {
      self.charging_profiles.insert(target, profile.clone());
      self.set_offered_limit(target, Some(limit))?;
      self.apply_charging_profile_state(target)?;
    }
    Ok(ResponseStatus::Accepted)
  }

  /// Applies an accepted remote-start charging profile.
  pub(in crate::simulator) fn apply_remote_start_charging_profile(
    &mut self,
    connector: u16,
    profile: Option<&Value>,
  ) -> Result<()> {
    let Some(profile) = profile else {
      return Ok(());
    };
    let status = self.apply_set_charging_profile(connector, profile)?;
    if status == ResponseStatus::Accepted {
      return Ok(());
    }
    Err(anyhow!(
      "Charging profile rejected on connector {} status={}",
      connector,
      status.as_str()
    ))
  }

  /// Validates a remote-start charging profile before transaction side effects.
  pub(in crate::simulator) fn validate_remote_start_charging_profile(
    profile: Option<&Value>,
  ) -> Result<()> {
    let Some(profile) = profile else {
      return Ok(());
    };
    if Self::extract_profile_limit(profile).is_some() {
      return Ok(());
    }
    Err(anyhow!(
      "charging profile must include one numeric limit value."
    ))
  }

  /// Builds target connectors for a charging-profile clear request.
  pub(in crate::simulator) fn clear_profile_targets(
    &self,
    connector: Option<u16>,
  ) -> Option<Vec<u16>> {
    match connector {
      Some(0) | None => Some(self.connectors.keys().copied().collect()),
      Some(connector) => self
        .connectors
        .contains_key(&connector)
        .then_some(vec![connector]),
    }
  }

  /// Clears stored charging profiles that match a predicate.
  pub(in crate::simulator) fn clear_charging_profiles_matching<F>(
    &mut self,
    targets: Vec<u16>,
    matches_profile: F,
  ) -> ResponseStatus
  where
    F: Fn(&Value) -> bool,
  {
    let mut cleared = Vec::new();
    for connector in targets {
      let should_remove = self
        .charging_profiles
        .get(&connector)
        .is_some_and(&matches_profile);
      if should_remove {
        self.charging_profiles.remove(&connector);
        cleared.push(connector);
      }
    }

    if cleared.is_empty() {
      return ResponseStatus::Unknown;
    }

    for connector in cleared {
      let _ = self.set_offered_limit(connector, None);
      let _ = self.apply_charging_profile_state(connector);
    }
    ResponseStatus::Accepted
  }

  /// Extracts the first charging limit value from supported profile shapes.
  pub(in crate::simulator) fn extract_profile_limit(
    profile: &Value,
  ) -> Option<f64> {
    let path = profile
      .get("chargingSchedule")
      .and_then(Value::as_object)
      .and_then(|value| value.get("chargingSchedulePeriod"))
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path {
      return Self::extract_limit_value(limit);
    }

    let path = profile
      .get("chargingSchedule")
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(Value::as_object)
      .and_then(|value| value.get("chargingSchedulePeriod"))
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path {
      return Self::extract_limit_value(limit);
    }

    let path = profile
      .get("chargingSchedulePeriod")
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path {
      return Self::extract_limit_value(limit);
    }

    None
  }

  /// Parses a charging limit from JSON number or numeric string.
  pub(in crate::simulator) fn extract_limit_value(
    value: &Value,
  ) -> Option<f64> {
    if let Some(limit) = value.as_f64() {
      return Some(limit);
    }
    value.as_str().and_then(|limit| limit.parse::<f64>().ok())
  }
}
