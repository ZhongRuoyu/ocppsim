use super::super::payloads::{
  ChargingSchedulePeriod, CompositeSchedule_V2_X, DataTransferResponse,
  GetCompositeSchedule_V2_X_Response, GetLog_V2_X_Response,
  GetVariables_V2_X_Response, SetVariables_V2_X_Response, VariableResult_V2_X,
  to_value,
};
use super::super::{
  ChargingRateUnit, ConnectorStatus, ResponseStatus, Result, Simulator,
  UiLogLevel, Value, VariableAttributeType, anyhow, json, normalize_identifier,
  now_timestamp,
};
use super::request::{
  AvailabilityRequest, CancelReservationRequest, CompositeScheduleRequest_V2_X,
  ReserveNowRequest_V2_X, SendLocalListRequest_V2_X,
  SetChargingProfileRequest_V2_X, UnlockConnectorRequest_V2_X,
  UpdateFirmwareRequest_V2_X,
};

impl Simulator {
  /// Handles `ChangeAvailability.req` for OCPP 2.x.
  pub(in crate::simulator) fn change_availability_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = AvailabilityRequest::parse_v2_x(payload)?;
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

  /// Handles `DataTransfer.req` response logic for OCPP 2.x.
  pub(in crate::simulator) fn data_transfer_v2_x(payload: &Value) -> Value {
    if payload.get("vendorId").and_then(Value::as_str).is_none() {
      return to_value(&DataTransferResponse {
        status: ResponseStatus::UnknownVendorId.as_str(),
        data: None,
      });
    }
    let data = payload.get("data").cloned();
    to_value(&DataTransferResponse {
      status: ResponseStatus::Accepted.as_str(),
      data,
    })
  }

  /// Handles `GetLog.req` by logging and returning a synthetic filename.
  pub(in crate::simulator) fn get_log_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<Value> {
    let request_id = payload
      .get("requestId")
      .and_then(Value::as_i64)
      .ok_or_else(|| anyhow!("requestId is required."))?;
    let location = payload
      .get("log")
      .and_then(Value::as_object)
      .and_then(|log| log.get("remoteLocation"))
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("log.remoteLocation is required."))?;
    self.log(
      UiLogLevel::Info,
      format!("Received GetLog request for {location}"),
    );
    self.enqueue_log_status_notification(
      ResponseStatus::Uploading.as_str(),
      Some(request_id),
    );
    self.enqueue_log_status_notification(
      ResponseStatus::Uploaded.as_str(),
      Some(request_id),
    );
    let filename = format!("log-{}.txt", self.config.cp_id);
    Ok(to_value(&GetLog_V2_X_Response {
      status: ResponseStatus::Accepted.as_str(),
      filename: &filename,
    }))
  }

  /// Handles `GetVariables.req` for configuration-backed variables.
  pub(in crate::simulator) fn get_variables_v2_x(
    &self,
    payload: &Value,
  ) -> Result<Value> {
    let entries = payload
      .get("getVariableData")
      .and_then(Value::as_array)
      .ok_or_else(|| anyhow!("getVariableData is required."))?;
    if entries.is_empty() {
      return Err(anyhow!("getVariableData must not be empty."));
    }
    let mut results = Vec::with_capacity(entries.len());

    for entry in entries {
      results.push(self.get_variable_result_v2_x(entry)?);
    }

    Ok(to_value(&GetVariables_V2_X_Response {
      get_variable_result: results,
    }))
  }

  /// Handles `SetVariables.req` for writable configuration-backed variables.
  pub(in crate::simulator) fn set_variables_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<Value> {
    let entries = payload
      .get("setVariableData")
      .and_then(Value::as_array)
      .ok_or_else(|| anyhow!("setVariableData is required."))?;
    if entries.is_empty() {
      return Err(anyhow!("setVariableData must not be empty."));
    }
    let mut results = Vec::with_capacity(entries.len());

    for entry in entries {
      results.push(self.set_variable_result_v2_x(entry)?);
    }

    Ok(to_value(&SetVariables_V2_X_Response {
      set_variable_result: results,
    }))
  }

  /// Handles `SendLocalList.req` by storing `versionNumber`.
  pub(in crate::simulator) fn send_local_list_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = SendLocalListRequest_V2_X::parse(payload)?;
    self.local_auth_list_version = request.version_number;
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `UnlockConnector.req` for OCPP 2.x connector addressing.
  pub(in crate::simulator) fn unlock_connector_v2_x(
    &self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = UnlockConnectorRequest_V2_X::parse(payload)?;
    if request.connector_id != 1 {
      return Ok(ResponseStatus::UnknownConnector);
    }
    let Some(state) = self.connectors.get(&request.evse_id) else {
      return Ok(ResponseStatus::UnknownConnector);
    };
    if state.transaction.is_some() {
      Ok(ResponseStatus::OngoingAuthorizedTransaction)
    } else {
      Ok(ResponseStatus::Unlocked)
    }
  }

  /// Handles `UpdateFirmware.req`.
  pub(in crate::simulator) fn update_firmware_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = UpdateFirmwareRequest_V2_X::parse(payload)?;
    let location = payload
      .get("firmware")
      .and_then(Value::as_object)
      .and_then(|item| item.get("location"))
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("firmware.location is required."))?;
    self.log(
      UiLogLevel::Info,
      format!("Received UpdateFirmware request from {location}"),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloading.as_str(),
      Some(request.request_id),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Downloaded.as_str(),
      Some(request.request_id),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installing.as_str(),
      Some(request.request_id),
    );
    self.enqueue_firmware_status_notification(
      ResponseStatus::Installed.as_str(),
      Some(request.request_id),
    );
    Ok(ResponseStatus::Accepted)
  }

  /// Handles `ReserveNow.req` by translating to shared reservation logic.
  pub(in crate::simulator) fn reserve_now_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = ReserveNowRequest_V2_X::parse(payload)?;
    self
      .reserve_connector(request.connector.unwrap_or(0), request.reservation_id)
  }

  /// Handles `CancelReservation.req` by translating to shared logic.
  pub(in crate::simulator) fn cancel_reservation_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = CancelReservationRequest::parse_v2_x(payload)?;
    self.cancel_reservation(request.reservation_id)
  }

  /// Handles `SetChargingProfile.req` and applies profile-derived limits.
  pub(in crate::simulator) fn set_charging_profile_v2_x(
    &mut self,
    payload: &Value,
  ) -> Result<ResponseStatus> {
    let request = SetChargingProfileRequest_V2_X::parse(payload)?;
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
  pub(in crate::simulator) fn clear_charging_profile_v2_x(
    &mut self,
    payload: &Value,
  ) -> ResponseStatus {
    let profile_id = payload.get("chargingProfileId").and_then(Value::as_i64);
    let criteria = payload
      .get("chargingProfileCriteria")
      .and_then(Value::as_object);
    let targets = if let Some(criteria_payload) = criteria {
      self.clear_profile_targets(&json!(criteria_payload), "evseId")
    } else {
      Some(self.connectors.keys().copied().collect())
    };
    let Some(targets) = targets else {
      return ResponseStatus::Unknown;
    };
    let purpose = criteria
      .and_then(|value| value.get("chargingProfilePurpose"))
      .and_then(Value::as_str);
    let stack_level = criteria
      .and_then(|value| value.get("stackLevel"))
      .and_then(Value::as_i64);

    self.clear_charging_profiles_matching(targets, |profile| {
      profile_id.is_none_or(|value| {
        profile.get("id").and_then(Value::as_i64) == Some(value)
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

  /// Handles `GetCompositeSchedule.req` for OCPP 2.x.
  pub(in crate::simulator) fn get_composite_schedule_v2_x(
    &self,
    payload: &Value,
  ) -> Result<Value> {
    let request = CompositeScheduleRequest_V2_X::parse(payload)?;
    let Some(state) = self.connectors.get(&request.connector) else {
      return Ok(to_value(&GetCompositeSchedule_V2_X_Response {
        status: ResponseStatus::Rejected.as_str(),
        schedule: None,
      }));
    };
    let Some(limit) = state.offered_limit else {
      return Ok(to_value(&GetCompositeSchedule_V2_X_Response {
        status: ResponseStatus::Rejected.as_str(),
        schedule: None,
      }));
    };
    let timestamp = now_timestamp();

    Ok(to_value(&GetCompositeSchedule_V2_X_Response {
      status: ResponseStatus::Accepted.as_str(),
      schedule: Some(CompositeSchedule_V2_X {
        evse_id: request.connector,
        duration: request.duration,
        schedule_start: &timestamp,
        charging_rate_unit: ChargingRateUnit::W.as_str(),
        charging_schedule_period: vec![ChargingSchedulePeriod {
          start_period: 0,
          limit,
        }],
      }),
    }))
  }

  fn get_variable_result_v2_x(
    &self,
    entry: &Value,
  ) -> Result<VariableResult_V2_X> {
    let component = variable_component(entry)?;
    let variable = variable_name(entry)?;
    let attribute_type = variable_attribute_type(entry);
    let base = VariableResult_V2_X::from_entry(
      entry,
      ResponseStatus::UnknownVariable.as_str(),
    )?;

    if !is_supported_variable_component(component) {
      return Ok(VariableResult_V2_X {
        attribute_status: ResponseStatus::UnknownComponent.as_str(),
        ..base
      });
    }
    if !is_supported_variable_attribute(attribute_type) {
      return Ok(VariableResult_V2_X {
        attribute_status: ResponseStatus::NotSupportedAttributeType.as_str(),
        ..base
      });
    }
    if let Some((_, configuration)) = self.configuration_entry(variable) {
      return Ok(VariableResult_V2_X {
        attribute_status: ResponseStatus::Accepted.as_str(),
        attribute_value: Some(configuration.value.clone()),
        ..base
      });
    }

    Ok(base)
  }

  fn set_variable_result_v2_x(
    &mut self,
    entry: &Value,
  ) -> Result<VariableResult_V2_X> {
    let component = variable_component(entry)?;
    let variable = variable_name(entry)?;
    let attribute_type = variable_attribute_type(entry);
    let attribute_value = entry
      .get("attributeValue")
      .and_then(Value::as_str)
      .ok_or_else(|| {
      anyhow!("setVariableData.attributeValue is required.")
    })?;
    let base = VariableResult_V2_X::from_entry(
      entry,
      ResponseStatus::UnknownVariable.as_str(),
    )?;

    if !is_supported_variable_component(component) {
      return Ok(VariableResult_V2_X {
        attribute_status: ResponseStatus::UnknownComponent.as_str(),
        ..base
      });
    }
    if !is_supported_variable_attribute(attribute_type) {
      return Ok(VariableResult_V2_X {
        attribute_status: ResponseStatus::NotSupportedAttributeType.as_str(),
        ..base
      });
    }

    let Some(configuration_key) = self.configuration_key(variable) else {
      return Ok(base);
    };
    Ok(VariableResult_V2_X {
      attribute_status: self
        .set_configuration_value(configuration_key, attribute_value)
        .as_str(),
      ..base
    })
  }
}

fn variable_component(entry: &Value) -> Result<&str> {
  entry
    .get("component")
    .and_then(Value::as_object)
    .and_then(|component| component.get("name"))
    .and_then(Value::as_str)
    .ok_or_else(|| anyhow!("component.name is required."))
}

fn variable_name(entry: &Value) -> Result<&str> {
  entry
    .get("variable")
    .and_then(Value::as_object)
    .and_then(|variable| variable.get("name"))
    .and_then(Value::as_str)
    .ok_or_else(|| anyhow!("variable.name is required."))
}

fn variable_attribute_type(entry: &Value) -> Option<VariableAttributeType> {
  entry.get("attributeType").and_then(Value::as_str).map_or(
    Some(VariableAttributeType::Actual),
    VariableAttributeType::parse,
  )
}

fn is_supported_variable_component(component: &str) -> bool {
  normalize_identifier(component) == "chargingstation"
}

fn is_supported_variable_attribute(
  attribute_type: Option<VariableAttributeType>,
) -> bool {
  matches!(
    attribute_type,
    Some(VariableAttributeType::Actual | VariableAttributeType::Target)
  )
}
