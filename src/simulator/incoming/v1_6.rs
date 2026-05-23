use super::super::payloads::{
  ChargingSchedulePeriod, ChargingScheduleV1_6, ConfigurationKeyEntry,
  DataTransferResponse, GetCompositeScheduleV1_6Response,
  GetConfigurationV1_6Response, GetDiagnosticsV1_6Response, to_value,
};
use super::super::{
  ChargingRateUnit, ConfigurationKey, ConnectorStatus, ResponseStatus, Result,
  Simulator, UiLogLevel, Value, anyhow, now_timestamp, optional_u16_field,
};
use super::request::{
  AvailabilityRequest, CancelReservationRequest, CompositeScheduleRequestV1_6,
  ReserveNowRequestV1_6, SendLocalListRequestV1_6,
  SetChargingProfileRequestV1_6, UnlockConnectorRequestV1_6,
};

impl Simulator {
  /// Builds `GetConfiguration.conf` data for OCPP 1.6.
  pub(in crate::simulator) fn configuration_response_v1_6(
    &self,
    payload: &Value,
  ) -> GetConfigurationV1_6Response<'_> {
    let requested_keys: Option<Vec<String>> =
      payload.get("key").and_then(Value::as_array).map(|items| {
        items
          .iter()
          .filter_map(Value::as_str)
          .map(ToOwned::to_owned)
          .collect()
      });

    let mut configuration_key = Vec::new();
    let mut unknown_key = Vec::new();

    if let Some(keys) = requested_keys {
      for key in keys {
        if let Some(config_key) = ConfigurationKey::parse(&key)
          && let Some(entry) = self.configuration.get(&config_key)
        {
          configuration_key.push(ConfigurationKeyEntry {
            key: config_key.as_str(),
            readonly: entry.read_only,
            value: &entry.value,
          });
        } else {
          unknown_key.push(key);
        }
      }
      return GetConfigurationV1_6Response {
        configuration_key,
        unknown_key,
      };
    }

    for (key, entry) in &self.configuration {
      configuration_key.push(ConfigurationKeyEntry {
        key: key.as_str(),
        readonly: entry.read_only,
        value: &entry.value,
      });
    }
    GetConfigurationV1_6Response {
      configuration_key,
      unknown_key,
    }
  }

  /// Applies `ChangeConfiguration.req` semantics for OCPP 1.6.
  pub(in crate::simulator) fn change_configuration_v1_6(
    &mut self,
    payload: &Value,
  ) -> ResponseStatus {
    let key = payload.get("key").and_then(Value::as_str).unwrap_or("");
    let value = payload.get("value").and_then(Value::as_str).unwrap_or("");
    if key.is_empty() {
      return ResponseStatus::Rejected;
    }

    let Some(configuration_key) = ConfigurationKey::parse(key) else {
      return ResponseStatus::NotSupported;
    };
    self.set_configuration_value(configuration_key, value)
  }

  /// Applies `ChangeAvailability.req` for one or all connectors in OCPP 1.6.
  pub(in crate::simulator) fn change_availability_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = AvailabilityRequest::parse_v1_6(payload)?;
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
      if has_active_tx && target_status == ConnectorStatus::Unavailable {
        self.schedule_availability_status(connector, target_status)?;
        scheduled = true;
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

  /// Handles `DataTransfer.req` response logic for OCPP 1.6.
  pub(in crate::simulator) fn data_transfer_v1_6(payload: &Value) -> Value {
    if payload.get("vendorId").and_then(Value::as_str).is_none() {
      return to_value(&DataTransferResponse {
        status: ResponseStatus::UnknownVendorId.as_str(),
        data: None,
      });
    }
    let data = payload
      .get("data")
      .and_then(Value::as_str)
      .map(|d| Value::String(d.to_owned()));
    to_value(&DataTransferResponse {
      status: ResponseStatus::Accepted.as_str(),
      data,
    })
  }

  /// Handles `GetDiagnostics.req` by logging and returning a fake filename.
  pub(in crate::simulator) fn get_diagnostics_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<Value> {
    let location = payload
      .get("location")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("location is required."))?;
    self.log(
      UiLogLevel::Info,
      format!("Received GetDiagnostics request for {location}"),
    );
    self.enqueue_diagnostics_status_notification(
      ResponseStatus::Uploading.as_str(),
    );
    self.enqueue_diagnostics_status_notification(
      ResponseStatus::Uploaded.as_str(),
    );
    let filename = format!("diagnostics-{}.log", self.config.cp_id);
    Ok(to_value(&GetDiagnosticsV1_6Response {
      file_name: &filename,
    }))
  }

  /// Handles `UpdateFirmware.req` with simulated status notifications.
  pub(in crate::simulator) fn update_firmware_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<()> {
    let location = payload
      .get("location")
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("location is required."))?;
    if payload
      .get("retrieveDate")
      .and_then(Value::as_str)
      .is_none()
    {
      return Err(anyhow!("retrieveDate is required."));
    }
    self.log(
      UiLogLevel::Info,
      format!("Received UpdateFirmware request from {location}"),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloading.as_str(),
      None,
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloaded.as_str(),
      None,
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installing.as_str(),
      None,
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installed.as_str(),
      None,
    );
    Ok(())
  }

  /// Handles `ReserveNow.req` and updates local reservation state.
  pub(in crate::simulator) fn reserve_now_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = ReserveNowRequestV1_6::parse(payload)?;
    self.reserve_connector(request.connector, request.reservation_id)
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
      if state.status == ConnectorStatus::Unavailable
        || state.status == ConnectorStatus::Faulted
      {
        return Ok(ResponseStatus::Unavailable);
      }
      state.status = ConnectorStatus::Reserved;
    }

    self.reservations.insert(reservation_id, connector);
    self.enqueue_status_notification(connector)?;
    self.emit_snapshot();
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `CancelReservation.req` and updates connector state.
  pub(in crate::simulator) fn cancel_reservation_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = CancelReservationRequest::parse_v1_6(payload)?;
    self.cancel_reservation(request.reservation_id)
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
      state.status = ConnectorStatus::Available;
      self.enqueue_status_notification(connector)?;
    }
    self.emit_snapshot();
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `UnlockConnector.req` with a transaction-state based response.
  pub(in crate::simulator) fn unlock_connector_v1_6(
    &self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = UnlockConnectorRequestV1_6::parse(payload)?;
    let Some(state) = self.connectors.get(&request.connector) else {
      return Ok(ResponseStatus::NotSupported);
    };
    if state.transaction.is_some() {
      Ok(ResponseStatus::UnlockFailed)
    } else {
      Ok(ResponseStatus::Unlocked)
    }
  }

  /// Handles `SendLocalList.req` by storing the new local list version.
  pub(in crate::simulator) fn send_local_list_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = SendLocalListRequestV1_6::parse(payload)?;
    self.local_auth_list_version = request.list_version;
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `SetChargingProfile.req` and applies profile-derived limits.
  pub(in crate::simulator) fn set_charging_profile_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = SetChargingProfileRequestV1_6::parse(payload)?;
    let targets: Vec<u16> = if request.connector == 0 {
      self.connectors.keys().copied().collect()
    } else if self.connectors.contains_key(&request.connector) {
      vec![request.connector]
    } else {
      return Ok(ResponseStatus::Rejected);
    };

    let limit = Self::extract_profile_limit(&request.profile);
    for target in targets {
      self
        .charging_profiles
        .insert(target, request.profile.clone());
      if let Some(limit_value) = limit {
        self.set_offered_limit(target, Some(limit_value))?;
        self.apply_charging_profile_state(target)?;
      }
    }
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `ClearChargingProfile.req` for matching connector/profile data.
  pub(in crate::simulator) fn clear_charging_profile_v1_6(
    &mut self,
    payload: &Value,
  ) -> ResponseStatus {
    let Some(targets) = self.clear_profile_targets(payload, "connectorId")
    else {
      return ResponseStatus::Unknown;
    };
    let profile_id = payload.get("id").and_then(Value::as_i64);
    let purpose = payload
      .get("chargingProfilePurpose")
      .and_then(Value::as_str);
    let stack_level = payload.get("stackLevel").and_then(Value::as_i64);

    self.clear_charging_profiles_matching(targets, |profile| {
      profile_id.is_none_or(|value| {
        profile.get("chargingProfileId").and_then(Value::as_i64) == Some(value)
      }) && purpose.is_none_or(|value| {
        profile
          .get("chargingProfilePurpose")
          .and_then(Value::as_str)
          == Some(value)
      }) && stack_level.is_none_or(|value| {
        profile.get("stackLevel").and_then(Value::as_i64) == Some(value)
      })
    })
  }

  /// Builds target connectors for a charging-profile clear request.
  pub(in crate::simulator) fn clear_profile_targets(
    &self,
    payload: &Value,
    field: &str,
  ) -> Option<Vec<u16>> {
    match optional_u16_field(payload, field) {
      Ok(Some(0) | None) => Some(self.connectors.keys().copied().collect()),
      Ok(Some(connector)) => {
        if self.connectors.contains_key(&connector) {
          Some(vec![connector])
        } else {
          None
        }
      }
      Err(_) => None,
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

  /// Handles `GetCompositeSchedule.req` for OCPP 1.6.
  pub(in crate::simulator) fn get_composite_schedule_v1_6(
    &self,
    payload: &Value,
  ) -> Result<Value> {
    let request = CompositeScheduleRequestV1_6::parse(payload)?;
    let Some(state) = self.connectors.get(&request.connector) else {
      return Ok(to_value(&GetCompositeScheduleV1_6Response {
        status: ResponseStatus::Rejected.as_str(),
        connector_id: None,
        schedule_start: None,
        charging_schedule: None,
      }));
    };
    let Some(limit) = state.offered_limit else {
      return Ok(to_value(&GetCompositeScheduleV1_6Response {
        status: ResponseStatus::Rejected.as_str(),
        connector_id: None,
        schedule_start: None,
        charging_schedule: None,
      }));
    };
    let charging_rate_unit = request
      .charging_rate_unit
      .as_deref()
      .unwrap_or(ChargingRateUnit::W.as_str());
    let timestamp = now_timestamp();

    Ok(to_value(&GetCompositeScheduleV1_6Response {
      status: ResponseStatus::Accepted.as_str(),
      connector_id: Some(request.connector),
      schedule_start: Some(&timestamp),
      charging_schedule: Some(ChargingScheduleV1_6 {
        duration: request.duration,
        charging_rate_unit,
        charging_schedule_period: vec![ChargingSchedulePeriod {
          start_period: 0,
          limit,
        }],
      }),
    }))
  }

  /// Extracts the first charging limit value from supported profile shapes.
  pub(in crate::simulator) fn extract_profile_limit(
    profile: &Value,
  ) -> Option<f64> {
    let path_1 = profile
      .get("chargingSchedule")
      .and_then(Value::as_object)
      .and_then(|value| value.get("chargingSchedulePeriod"))
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path_1 {
      return Self::extract_limit_value(limit);
    }

    let path_2 = profile
      .get("chargingSchedule")
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(Value::as_object)
      .and_then(|value| value.get("chargingSchedulePeriod"))
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path_2 {
      return Self::extract_limit_value(limit);
    }

    let path_3 = profile
      .get("chargingSchedulePeriod")
      .and_then(Value::as_array)
      .and_then(|value| value.first())
      .and_then(|value| value.get("limit"));
    if let Some(limit) = path_3 {
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
