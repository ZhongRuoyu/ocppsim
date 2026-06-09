use super::super::payloads::{
  ChargingSchedulePeriod, ChargingScheduleV1_6, ConfigurationKeyEntry,
  DataTransferResponse, GetCompositeScheduleV1_6Response,
  GetConfigurationV1_6Response, GetDiagnosticsV1_6Response, to_value,
};
use super::super::{
  ChargingRateUnit, ConfigurationKey, ResponseStatus, Result, Simulator,
  UiLogLevel, Value, anyhow, now_timestamp,
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
            value: configuration_value(config_key, &entry.value),
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
        value: configuration_value(*key, &entry.value),
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
    self.apply_change_availability(AvailabilityRequest::parse_v1_6(payload)?)
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
    if !self.supports_file_transfer_location(location) {
      return Ok(to_value(&GetDiagnosticsV1_6Response { file_name: None }));
    }
    self.enqueue_diagnostics_status_notification(
      ResponseStatus::Uploading.as_str(),
    );
    self.enqueue_diagnostics_status_notification(
      ResponseStatus::Uploaded.as_str(),
    );
    let cp_id = self.config.cp_id.as_deref().unwrap_or("unknown");
    let filename = format!("diagnostics-{cp_id}.log");
    Ok(to_value(&GetDiagnosticsV1_6Response {
      file_name: Some(&filename),
    }))
  }

  /// Handles `ReserveNow.req` and updates local reservation state.
  pub(in crate::simulator) fn reserve_now_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = ReserveNowRequestV1_6::parse(payload)?;
    self.reserve_connector(request.connector, request.reservation_id)
  }

  /// Handles `CancelReservation.req` and updates connector state.
  pub(in crate::simulator) fn cancel_reservation_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = CancelReservationRequest::parse_v1_6(payload)?;
    self.cancel_reservation(request.reservation_id)
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
    Ok(self.apply_local_list_version(request.list_version))
  }

  /// Handles `SetChargingProfile.req` and applies profile-derived limits.
  pub(in crate::simulator) fn set_charging_profile_v1_6(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = SetChargingProfileRequestV1_6::parse(payload)?;
    self.apply_set_charging_profile(request.connector, &request.profile)
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
}

fn configuration_value(key: ConfigurationKey, value: &str) -> Option<&str> {
  if matches!(
    key,
    ConfigurationKey::AuthorizationKey | ConfigurationKey::BasicAuthPassword
  ) {
    None
  } else {
    Some(value)
  }
}
