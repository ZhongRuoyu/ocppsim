use super::super::payloads::{
  Authorize_V2_X_Request, AuthorizeV1_6Request, BootNotification_V2_X_Request,
  BootNotificationV1_6Request, ChargingStationInfo, DataTransferRequestPayload,
  EvsePayload, FirmwareStatus_V2_X_Payload, HeartbeatRequest, IdTokenPayload,
  LogStatusPayload, MeterValueEntry, MeterValues_V2_X_Request,
  MeterValuesV1_6Request, SampledValue_V2_X, SampledValueV1_6,
  StatusNotification_V2_X_Request, StatusNotificationV1_6Request,
  StatusPayload, TransactionEvent_V2_X_Request, TransactionInfoPayload,
  UnitOfMeasure, to_value,
};
use super::super::{
  BootReason, ConnectorState, ConnectorStatus, IdTokenType, Measurand,
  MeterUnit, OcppVersion, OutgoingAction, PendingContext, QueuedCall,
  ReadingContext, Result, Simulator, StatusNotificationErrorCode,
  TransactionEventRequest, TransactionState, TxEventType, UiLogLevel, Value,
  anyhow, now_timestamp, validate_boot_notification_fields,
  validate_data_transfer_fields, validate_outbound_id_token,
};

impl Simulator {
  /// Enqueues a CALL with its payload and post-response context.
  pub(in crate::simulator) fn enqueue_call(
    &mut self,
    action: &str,
    payload: Value,
    context: PendingContext,
  ) -> bool {
    if let Some(limit) = self.outbound_queue_limit_reached() {
      self.log_outbound_queue_limit_reached(action, limit);
      return false;
    }

    let limit = self.config.outbound_queue_limit;
    self.queue.push_back(QueuedCall {
      action: action.to_string(),
      payload,
      context,
    });
    if limit != 0 && self.queue.len() == limit {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Outbound OCPP queue reached limit {limit}; later messages will be \
          dropped until the CSMS responds."
        ),
      );
    }
    self.emit_runtime_state();
    true
  }

  pub(in crate::simulator) fn ensure_outbound_queue_capacity(
    &mut self,
    action: &str,
  ) -> Result<()> {
    if let Some(limit) = self.outbound_queue_limit_reached() {
      self.log_outbound_queue_limit_reached(action, limit);
      return Err(anyhow!(
        "Outbound OCPP queue limit {limit} reached; cannot queue {action} \
        request."
      ));
    }
    Ok(())
  }

  fn try_enqueue_call(
    &mut self,
    action: &str,
    payload: Value,
    context: PendingContext,
  ) -> Result<()> {
    if self.enqueue_call(action, payload, context) {
      return Ok(());
    }
    Err(anyhow!(
      "Outbound OCPP queue limit {} reached; cannot queue {action} request.",
      self.config.outbound_queue_limit
    ))
  }

  fn outbound_queue_limit_reached(&self) -> Option<usize> {
    let limit = self.config.outbound_queue_limit;
    (limit != 0 && self.queue.len() >= limit).then_some(limit)
  }

  fn log_outbound_queue_limit_reached(&mut self, action: &str, limit: usize) {
    self.log(
      UiLogLevel::Warn,
      format!(
        "Outbound OCPP queue limit {limit} reached; dropping {action} \
        request."
      ),
    );
    self.emit_runtime_state();
  }

  /// Enqueues a protocol-version-specific `BootNotification` request.
  pub(in crate::simulator) fn enqueue_boot_notification(
    &mut self,
  ) -> Result<()> {
    let payload = self.boot_notification_payload()?;
    if self.enqueue_call(
      OutgoingAction::BootNotification.as_str(),
      payload.clone(),
      PendingContext::Boot,
    ) {
      self.last_boot_notification_payload = Some(payload);
    }
    Ok(())
  }

  pub(in crate::simulator) fn boot_notification_changed(&self) -> Result<bool> {
    let payload = self.boot_notification_payload()?;
    Ok(
      self
        .last_boot_notification_payload
        .as_ref()
        .is_none_or(|last_payload| last_payload != &payload),
    )
  }

  /// Enqueues an `Authorize` request for the provided ID token.
  pub(in crate::simulator) fn enqueue_authorize(
    &mut self,
    id_token: String,
  ) -> Result<()> {
    validate_outbound_id_token(self.config.protocol, &id_token)
      .map_err(anyhow::Error::msg)?;
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => Self::authorize_v1_6_payload(&id_token),
      OcppVersion::V2_0_1 => Self::authorize_v2_0_1_payload(&id_token),
      OcppVersion::V2_1 => Self::authorize_v2_1_payload(&id_token),
    };
    self.enqueue_call(
      OutgoingAction::Authorize.as_str(),
      payload,
      PendingContext::Authorize { id_token },
    );
    Ok(())
  }

  /// Enqueues the authorization step required before OCPP 1.6 remote start.
  pub(in crate::simulator) fn enqueue_remote_start_authorize_v1_6(
    &mut self,
    connector: u16,
    id_token: String,
    charging_profile: Option<Value>,
  ) -> Result<()> {
    validate_outbound_id_token(self.config.protocol, &id_token)
      .map_err(anyhow::Error::msg)?;
    Self::validate_remote_start_charging_profile(charging_profile.as_ref())?;
    let payload = Self::authorize_v1_6_payload(&id_token);
    self.try_enqueue_call(
      OutgoingAction::Authorize.as_str(),
      payload,
      PendingContext::RemoteStartAuthorizeV1_6 {
        connector,
        id_token,
        charging_profile,
      },
    )
  }

  /// Enqueues a one-shot `Heartbeat` request.
  pub(in crate::simulator) fn enqueue_heartbeat(&mut self) {
    self.enqueue_call(
      OutgoingAction::Heartbeat.as_str(),
      to_value(&HeartbeatRequest {}),
      PendingContext::Heartbeat,
    );
  }

  /// Enqueues a charge-point initiated `DataTransfer` request.
  pub(in crate::simulator) fn enqueue_data_transfer(
    &mut self,
    vendor_id: &str,
    message_id: Option<&str>,
    data: Option<&str>,
  ) -> Result<()> {
    validate_data_transfer_fields(vendor_id, message_id)
      .map_err(anyhow::Error::msg)?;
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => {
        Self::data_transfer_v1_6_payload(vendor_id, message_id, data)
      }
      OcppVersion::V2_0_1 => {
        Self::data_transfer_v2_0_1_payload(vendor_id, message_id, data)
      }
      OcppVersion::V2_1 => {
        Self::data_transfer_v2_1_payload(vendor_id, message_id, data)
      }
    };
    self.enqueue_call(
      OutgoingAction::DataTransfer.as_str(),
      payload,
      PendingContext::DataTransfer,
    );
    Ok(())
  }

  /// Enqueues an OCPP 1.6 `DiagnosticsStatusNotification` request.
  pub(in crate::simulator) fn enqueue_diagnostics_status_notification(
    &mut self,
    status: &str,
  ) {
    self.enqueue_call(
      OutgoingAction::DiagnosticsStatusNotification.as_str(),
      to_value(&StatusPayload { status }),
      PendingContext::DiagnosticsStatusNotification,
    );
  }

  /// Enqueues a protocol-version-specific `FirmwareStatusNotification`.
  pub(in crate::simulator) fn enqueue_firmware_status_notification(
    &mut self,
    status: &str,
    request_id: Option<i64>,
  ) {
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => Self::firmware_status_v1_6_payload(status),
      OcppVersion::V2_0_1 => {
        Self::firmware_status_v2_0_1_payload(status, request_id)
      }
      OcppVersion::V2_1 => {
        Self::firmware_status_v2_1_payload(status, request_id)
      }
    };
    self.enqueue_call(
      OutgoingAction::FirmwareStatusNotification.as_str(),
      payload,
      PendingContext::FirmwareStatusNotification,
    );
  }

  /// Enqueues a protocol-version-specific `LogStatusNotification` request.
  pub(in crate::simulator) fn enqueue_log_status_notification(
    &mut self,
    status: &str,
    request_id: Option<i64>,
  ) {
    let payload = to_value(&LogStatusPayload { status, request_id });
    self.enqueue_call(
      OutgoingAction::LogStatusNotification.as_str(),
      payload,
      PendingContext::LogStatusNotification,
    );
  }

  /// Enqueues an OCPP 1.6 security `SignedFirmwareStatusNotification`.
  pub(in crate::simulator) fn enqueue_signed_firmware_status_notification(
    &mut self,
    status: &str,
    request_id: Option<i64>,
  ) {
    let payload = to_value(&FirmwareStatus_V2_X_Payload { status, request_id });
    self.enqueue_call(
      OutgoingAction::SignedFirmwareStatusNotification.as_str(),
      payload,
      PendingContext::SignedFirmwareStatusNotification,
    );
  }

  /// Enqueues a `StatusNotification` for one connector.
  pub(in crate::simulator) fn enqueue_status_notification(
    &mut self,
    connector: u16,
  ) -> Result<()> {
    let status = self.connector_ref(connector)?.status;
    self.enqueue_status_notification_with_status(connector, status);
    Ok(())
  }

  /// Enqueues the initial status report required after an accepted boot.
  pub(in crate::simulator) fn enqueue_boot_status_notifications(
    &mut self,
  ) -> Result<()> {
    if self.config.protocol == OcppVersion::V1_6 {
      self.enqueue_status_notification_with_status(
        0,
        self.charge_point_status_v1_6(),
      );
    }
    let connectors: Vec<u16> = self.connectors.keys().copied().collect();
    for connector in connectors {
      self.enqueue_status_notification(connector)?;
    }
    Ok(())
  }

  fn enqueue_status_notification_with_status(
    &mut self,
    connector: u16,
    status: ConnectorStatus,
  ) {
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => {
        Self::status_notification_v1_6_payload(connector, status)
      }
      OcppVersion::V2_0_1 => {
        Self::status_notification_v2_0_1_payload(connector, status)
      }
      OcppVersion::V2_1 => {
        Self::status_notification_v2_1_payload(connector, status)
      }
    };
    self.enqueue_call(
      OutgoingAction::StatusNotification.as_str(),
      payload,
      PendingContext::StatusNotification { connector },
    );
  }

  fn charge_point_status_v1_6(&self) -> ConnectorStatus {
    if self
      .connectors
      .values()
      .any(|connector| connector.status == ConnectorStatus::Faulted)
    {
      ConnectorStatus::Faulted
    } else if self
      .connectors
      .values()
      .all(|connector| connector.status == ConnectorStatus::Unavailable)
    {
      ConnectorStatus::Unavailable
    } else {
      ConnectorStatus::Available
    }
  }

  /// Enqueues a `MeterValues` request for one connector.
  pub(in crate::simulator) fn enqueue_meter_values(
    &mut self,
    connector: u16,
  ) -> Result<()> {
    let state = self.connector_ref(connector)?;
    let timestamp = now_timestamp();
    match self.config.protocol {
      OcppVersion::V1_6 => {
        let payload =
          Self::meter_values_v1_6_payload(connector, state, &timestamp);
        self.enqueue_call(
          OutgoingAction::MeterValues.as_str(),
          payload,
          PendingContext::MeterValues { connector },
        );
      }
      OcppVersion::V2_0_1 => {
        let payload =
          Self::meter_values_v2_0_1_payload(connector, state, &timestamp);
        self.enqueue_call(
          OutgoingAction::MeterValues.as_str(),
          payload,
          PendingContext::MeterValues { connector },
        );
      }
      OcppVersion::V2_1 => {
        let payload =
          Self::meter_values_v2_1_payload(connector, state, &timestamp);
        self.enqueue_call(
          OutgoingAction::MeterValues.as_str(),
          payload,
          PendingContext::MeterValues { connector },
        );
      }
    }
    Ok(())
  }

  /// Enqueues an OCPP 2.x `TransactionEvent` request.
  ///
  /// Inputs describe event semantics plus optional ID token and stop reason.
  pub(in crate::simulator) fn enqueue_transaction_event(
    &mut self,
    request: &TransactionEventRequest,
  ) -> Result<()> {
    let state = self.connector_ref(request.connector)?;

    let transaction = state.transaction.as_ref().ok_or_else(|| {
      anyhow!("No active transaction on connector {}.", request.connector)
    })?;

    let timestamp = now_timestamp();
    let context = match request.event_type {
      TxEventType::Started => ReadingContext::TransactionBegin,
      TxEventType::Updated => ReadingContext::SamplePeriodic,
      TxEventType::Ended => ReadingContext::TransactionEnd,
    };
    let event_type_label = request.event_type.as_str();

    let payload = match self.config.protocol {
      OcppVersion::V1_6 => unreachable!("TransactionEvent is not OCPP 1.6"),
      OcppVersion::V2_0_1 => Self::transaction_event_v2_0_1_payload(
        request,
        transaction,
        state.meter_wh,
        &timestamp,
        context.as_str(),
        event_type_label,
        OcppVersion::V2_0_1,
      ),
      OcppVersion::V2_1 => Self::transaction_event_v2_1_payload(
        request,
        transaction,
        state.meter_wh,
        &timestamp,
        context.as_str(),
        event_type_label,
        OcppVersion::V2_1,
      ),
    };

    self.try_enqueue_call(
      OutgoingAction::TransactionEvent.as_str(),
      payload,
      PendingContext::TxEvent {
        connector: request.connector,
        local_tx_id: request.local_tx_id,
        event_type: request.event_type,
      },
    )
  }

  fn boot_notification_v1_6_payload(&self) -> Value {
    to_value(&BootNotificationV1_6Request {
      charge_point_vendor: &self.config.vendor,
      charge_point_model: &self.config.model,
      firmware_version: &self.config.firmware,
    })
  }

  fn boot_notification_payload(&self) -> Result<Value> {
    validate_boot_notification_fields(
      self.config.protocol,
      &self.config.vendor,
      &self.config.model,
      &self.config.firmware,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(match self.config.protocol {
      OcppVersion::V1_6 => self.boot_notification_v1_6_payload(),
      OcppVersion::V2_0_1 => self.boot_notification_v2_0_1_payload(),
      OcppVersion::V2_1 => self.boot_notification_v2_1_payload(),
    })
  }

  fn boot_notification_v2_0_1_payload(&self) -> Value {
    to_value(&BootNotification_V2_X_Request {
      reason: BootReason::PowerUp.as_str(),
      charging_station: ChargingStationInfo {
        vendor_name: &self.config.vendor,
        model: &self.config.model,
        firmware_version: &self.config.firmware,
      },
    })
  }

  fn boot_notification_v2_1_payload(&self) -> Value {
    to_value(&BootNotification_V2_X_Request {
      reason: BootReason::PowerUp.as_str(),
      charging_station: ChargingStationInfo {
        vendor_name: &self.config.vendor,
        model: &self.config.model,
        firmware_version: &self.config.firmware,
      },
    })
  }

  fn authorize_v1_6_payload(id_token: &str) -> Value {
    to_value(&AuthorizeV1_6Request { id_tag: id_token })
  }

  fn authorize_v2_0_1_payload(id_token: &str) -> Value {
    to_value(&Authorize_V2_X_Request {
      id_token: IdTokenPayload {
        id_token,
        token_type: IdTokenType::Central.as_str(),
      },
    })
  }

  fn authorize_v2_1_payload(id_token: &str) -> Value {
    to_value(&Authorize_V2_X_Request {
      id_token: IdTokenPayload {
        id_token,
        token_type: IdTokenType::Central.as_str(),
      },
    })
  }

  fn data_transfer_v1_6_payload(
    vendor_id: &str,
    message_id: Option<&str>,
    data: Option<&str>,
  ) -> Value {
    Self::data_transfer_payload(vendor_id, message_id, data)
  }

  fn data_transfer_v2_0_1_payload(
    vendor_id: &str,
    message_id: Option<&str>,
    data: Option<&str>,
  ) -> Value {
    Self::data_transfer_payload(vendor_id, message_id, data)
  }

  fn data_transfer_v2_1_payload(
    vendor_id: &str,
    message_id: Option<&str>,
    data: Option<&str>,
  ) -> Value {
    Self::data_transfer_payload(vendor_id, message_id, data)
  }

  fn data_transfer_payload(
    vendor_id: &str,
    message_id: Option<&str>,
    data: Option<&str>,
  ) -> Value {
    to_value(&DataTransferRequestPayload {
      vendor_id,
      message_id,
      data,
    })
  }

  fn firmware_status_v1_6_payload(status: &str) -> Value {
    to_value(&StatusPayload { status })
  }

  fn firmware_status_v2_0_1_payload(
    status: &str,
    request_id: Option<i64>,
  ) -> Value {
    Self::firmware_status_v2_x_payload(status, request_id)
  }

  fn firmware_status_v2_1_payload(
    status: &str,
    request_id: Option<i64>,
  ) -> Value {
    Self::firmware_status_v2_x_payload(status, request_id)
  }

  fn firmware_status_v2_x_payload(
    status: &str,
    request_id: Option<i64>,
  ) -> Value {
    to_value(&FirmwareStatus_V2_X_Payload { status, request_id })
  }

  fn status_notification_v1_6_payload(
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotificationV1_6Request {
      connector_id: connector,
      error_code: StatusNotificationErrorCode::NoError.as_str(),
      status: status
        .as_v1_6()
        .as_v1_6()
        .expect("simulator status maps to OCPP 1.6 status"),
      timestamp: &timestamp,
    })
  }

  fn status_notification_v2_0_1_payload(
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotification_V2_X_Request {
      timestamp: &timestamp,
      connector_status: status
        .as_v2_x()
        .as_v2_x()
        .expect("simulator status maps to OCPP 2.x status"),
      evse_id: connector,
      connector_id: 1,
    })
  }

  fn status_notification_v2_1_payload(
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotification_V2_X_Request {
      timestamp: &timestamp,
      connector_status: status
        .as_v2_x()
        .as_v2_x()
        .expect("simulator status maps to OCPP 2.x status"),
      evse_id: connector,
      connector_id: 1,
    })
  }

  fn meter_values_v1_6_payload(
    connector: u16,
    state: &ConnectorState,
    timestamp: &str,
  ) -> Value {
    let meter_wh_str = state.meter_wh.to_string();
    to_value(&MeterValuesV1_6Request {
      connector_id: connector,
      transaction_id: state
        .transaction
        .as_ref()
        .and_then(|tx| tx.v1_6_transaction_id),
      meter_value: vec![MeterValueEntry {
        timestamp,
        sampled_value: vec![SampledValueV1_6 {
          value: &meter_wh_str,
          context: ReadingContext::SamplePeriodic.as_str(),
          measurand: Measurand::EnergyActiveImportRegister.as_str(),
          unit: MeterUnit::Wh.as_str(),
        }],
      }],
    })
  }

  fn meter_values_v2_0_1_payload(
    connector: u16,
    state: &ConnectorState,
    timestamp: &str,
  ) -> Value {
    Self::meter_values_v2_x_payload(connector, state, timestamp)
  }

  fn meter_values_v2_1_payload(
    connector: u16,
    state: &ConnectorState,
    timestamp: &str,
  ) -> Value {
    Self::meter_values_v2_x_payload(connector, state, timestamp)
  }

  fn meter_values_v2_x_payload(
    connector: u16,
    state: &ConnectorState,
    timestamp: &str,
  ) -> Value {
    to_value(&MeterValues_V2_X_Request {
      evse_id: connector,
      meter_value: vec![MeterValueEntry {
        timestamp,
        sampled_value: vec![SampledValue_V2_X {
          value: state.meter_wh,
          context: ReadingContext::SamplePeriodic.as_str(),
          measurand: Measurand::EnergyActiveImportRegister.as_str(),
          unit_of_measure: UnitOfMeasure {
            unit: MeterUnit::Wh.as_str(),
          },
        }],
      }],
    })
  }

  fn transaction_event_v2_0_1_payload(
    request: &TransactionEventRequest,
    transaction: &TransactionState,
    meter_wh: i64,
    timestamp: &str,
    context: &str,
    event_type_label: &str,
    protocol: OcppVersion,
  ) -> Value {
    Self::transaction_event_v2_x_payload(
      request,
      transaction,
      meter_wh,
      timestamp,
      context,
      event_type_label,
      protocol,
    )
  }

  fn transaction_event_v2_1_payload(
    request: &TransactionEventRequest,
    transaction: &TransactionState,
    meter_wh: i64,
    timestamp: &str,
    context: &str,
    event_type_label: &str,
    protocol: OcppVersion,
  ) -> Value {
    Self::transaction_event_v2_x_payload(
      request,
      transaction,
      meter_wh,
      timestamp,
      context,
      event_type_label,
      protocol,
    )
  }

  fn transaction_event_v2_x_payload(
    request: &TransactionEventRequest,
    transaction: &TransactionState,
    meter_wh: i64,
    timestamp: &str,
    context: &str,
    event_type_label: &str,
    protocol: OcppVersion,
  ) -> Value {
    let stopped_reason = request
      .stopped_reason
      .and_then(|reason| reason.as_v2_x(protocol));

    to_value(&TransactionEvent_V2_X_Request {
      event_type: event_type_label,
      timestamp,
      trigger_reason: request.trigger_reason.as_str(),
      seq_no: transaction.seq_no,
      transaction_info: TransactionInfoPayload {
        transaction_id: &transaction.transaction_uid,
        remote_start_id: request.remote_start_id,
        stopped_reason,
      },
      evse: EvsePayload {
        id: request.connector,
        connector_id: 1,
      },
      meter_value: vec![MeterValueEntry {
        timestamp,
        sampled_value: vec![SampledValue_V2_X {
          value: meter_wh,
          context,
          measurand: Measurand::EnergyActiveImportRegister.as_str(),
          unit_of_measure: UnitOfMeasure {
            unit: MeterUnit::Wh.as_str(),
          },
        }],
      }],
      id_token: request.id_token.as_deref().map(|token| IdTokenPayload {
        id_token: token,
        token_type: IdTokenType::Central.as_str(),
      }),
    })
  }
}
