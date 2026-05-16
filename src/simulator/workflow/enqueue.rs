use super::super::payloads::*;
use super::super::*;

impl Simulator {
  /// Enqueues a CALL with its payload and post-response context.
  pub(in crate::simulator) fn enqueue_call(
    &mut self,
    action: &str,
    payload: Value,
    context: PendingContext,
  ) {
    self.queue.push_back(QueuedCall {
      action: action.to_string(),
      payload,
      context,
    });
    if self.queue.len() == QUEUE_DEPTH_WARN_THRESHOLD {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Outbound OCPP queue reached {} messages; \
          check CSMS responses or reduce command rate.",
          QUEUE_DEPTH_WARN_THRESHOLD
        ),
      );
    }
  }

  /// Enqueues a protocol-version-specific `BootNotification` request.
  pub(in crate::simulator) fn enqueue_boot_notification(&mut self) {
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => self.boot_notification_v1_6_payload(),
      OcppVersion::V2_0_1 => self.boot_notification_v2_0_1_payload(),
      OcppVersion::V2_1 => self.boot_notification_v2_1_payload(),
    };
    self.enqueue_call("BootNotification", payload, PendingContext::Boot);
  }

  /// Enqueues an `Authorize` request for the provided id token.
  pub(in crate::simulator) fn enqueue_authorize(&mut self, id_token: String) {
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => Self::authorize_v1_6_payload(&id_token),
      OcppVersion::V2_0_1 => Self::authorize_v2_0_1_payload(&id_token),
      OcppVersion::V2_1 => Self::authorize_v2_1_payload(&id_token),
    };
    self.enqueue_call(
      "Authorize",
      payload,
      PendingContext::Authorize { id_token },
    );
  }

  /// Enqueues a one-shot `Heartbeat` request.
  pub(in crate::simulator) fn enqueue_heartbeat(&mut self) {
    self.enqueue_call(
      "Heartbeat",
      to_value(&HeartbeatRequest {}),
      PendingContext::Heartbeat,
    );
  }

  /// Enqueues a charge-point initiated `DataTransfer` request.
  pub(in crate::simulator) fn enqueue_data_transfer(
    &mut self,
    vendor_id: String,
    message_id: Option<String>,
    data: Option<String>,
  ) {
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => Self::data_transfer_v1_6_payload(
        &vendor_id,
        message_id.as_deref(),
        data.as_deref(),
      ),
      OcppVersion::V2_0_1 => Self::data_transfer_v2_0_1_payload(
        &vendor_id,
        message_id.as_deref(),
        data.as_deref(),
      ),
      OcppVersion::V2_1 => Self::data_transfer_v2_1_payload(
        &vendor_id,
        message_id.as_deref(),
        data.as_deref(),
      ),
    };
    self.enqueue_call("DataTransfer", payload, PendingContext::DataTransfer);
  }

  /// Enqueues an OCPP 1.6 `DiagnosticsStatusNotification` request.
  pub(in crate::simulator) fn enqueue_diagnostics_status_notification(
    &mut self,
    status: &str,
  ) {
    self.enqueue_call(
      "DiagnosticsStatusNotification",
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
      "FirmwareStatusNotification",
      payload,
      PendingContext::FirmwareStatusNotification,
    );
  }

  /// Enqueues an OCPP 2.x `LogStatusNotification` request.
  pub(in crate::simulator) fn enqueue_log_status_notification(
    &mut self,
    status: &str,
    request_id: Option<i64>,
  ) {
    let payload = to_value(&LogStatusPayload { status, request_id });
    self.enqueue_call(
      "LogStatusNotification",
      payload,
      PendingContext::LogStatusNotification,
    );
  }

  /// Enqueues a `StatusNotification` for one connector.
  pub(in crate::simulator) fn enqueue_status_notification(
    &mut self,
    connector: u16,
  ) -> Result<()> {
    let connector_state = self.connector_ref(connector)?;

    let status = connector_state.status;
    let payload = match self.config.protocol {
      OcppVersion::V1_6 => {
        self.status_notification_v1_6_payload(connector, status)
      }
      OcppVersion::V2_0_1 => {
        self.status_notification_v2_0_1_payload(connector, status)
      }
      OcppVersion::V2_1 => {
        self.status_notification_v2_1_payload(connector, status)
      }
    };
    self.enqueue_call(
      "StatusNotification",
      payload,
      PendingContext::StatusNotification { connector },
    );
    Ok(())
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
          "MeterValues",
          payload,
          PendingContext::MeterValues { connector },
        );
      }
      OcppVersion::V2_0_1 => {
        let payload =
          Self::meter_values_v2_0_1_payload(connector, state, &timestamp);
        self.enqueue_call(
          "MeterValues",
          payload,
          PendingContext::MeterValues { connector },
        );
      }
      OcppVersion::V2_1 => {
        let payload =
          Self::meter_values_v2_1_payload(connector, state, &timestamp);
        self.enqueue_call(
          "MeterValues",
          payload,
          PendingContext::MeterValues { connector },
        );
      }
    }
    Ok(())
  }

  /// Enqueues an OCPP 2.x `TransactionEvent` request.
  ///
  /// Inputs describe event semantics plus optional id token and stop reason.
  pub(in crate::simulator) fn enqueue_transaction_event(
    &mut self,
    request: TransactionEventRequest,
  ) -> Result<()> {
    let state = self.connector_ref(request.connector)?;

    let transaction = state.transaction.as_ref().ok_or_else(|| {
      anyhow!("No active transaction on connector {}.", request.connector)
    })?;

    let timestamp = now_timestamp();
    let context = match request.event_type {
      TxEventType::Started => "Transaction.Begin",
      TxEventType::Updated => "Sample.Periodic",
      TxEventType::Ended => "Transaction.End",
    };
    let event_type_label = match request.event_type {
      TxEventType::Started => "Started",
      TxEventType::Updated => "Updated",
      TxEventType::Ended => "Ended",
    };

    let payload = match self.config.protocol {
      OcppVersion::V1_6 => unreachable!("TransactionEvent is not OCPP 1.6"),
      OcppVersion::V2_0_1 => Self::transaction_event_v2_0_1_payload(
        &request,
        transaction,
        state.meter_wh,
        &timestamp,
        context,
        event_type_label,
      ),
      OcppVersion::V2_1 => Self::transaction_event_v2_1_payload(
        &request,
        transaction,
        state.meter_wh,
        &timestamp,
        context,
        event_type_label,
      ),
    };

    self.enqueue_call(
      "TransactionEvent",
      payload,
      PendingContext::TxEvent {
        connector: request.connector,
        local_tx_id: request.local_tx_id,
        event_type: request.event_type,
      },
    );
    Ok(())
  }

  fn boot_notification_v1_6_payload(&self) -> Value {
    to_value(&BootNotificationV1_6Request {
      charge_point_vendor: &self.config.vendor,
      charge_point_model: &self.config.model,
      firmware_version: &self.config.firmware,
    })
  }

  fn boot_notification_v2_0_1_payload(&self) -> Value {
    to_value(&BootNotification_V2_X_Request {
      reason: "PowerUp",
      charging_station: ChargingStationInfo {
        vendor_name: &self.config.vendor,
        model: &self.config.model,
        firmware_version: &self.config.firmware,
      },
    })
  }

  fn boot_notification_v2_1_payload(&self) -> Value {
    to_value(&BootNotification_V2_X_Request {
      reason: "PowerUp",
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
        token_type: "Central",
      },
    })
  }

  fn authorize_v2_1_payload(id_token: &str) -> Value {
    to_value(&Authorize_V2_X_Request {
      id_token: IdTokenPayload {
        id_token,
        token_type: "Central",
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
    &self,
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotificationV1_6Request {
      connector_id: connector,
      error_code: "NoError",
      status: status.as_v1_6(),
      timestamp: &timestamp,
    })
  }

  fn status_notification_v2_0_1_payload(
    &self,
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotification_V2_X_Request {
      timestamp: &timestamp,
      connector_status: status.as_v2_x(),
      evse_id: connector,
      connector_id: 1,
    })
  }

  fn status_notification_v2_1_payload(
    &self,
    connector: u16,
    status: ConnectorStatus,
  ) -> Value {
    let timestamp = now_timestamp();
    to_value(&StatusNotification_V2_X_Request {
      timestamp: &timestamp,
      connector_status: status.as_v2_x(),
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
          context: "Sample.Periodic",
          measurand: "Energy.Active.Import.Register",
          unit: "Wh",
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
          context: "Sample.Periodic",
          measurand: "Energy.Active.Import.Register",
          unit_of_measure: UnitOfMeasure { unit: "Wh" },
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
  ) -> Value {
    Self::transaction_event_v2_x_payload(
      request,
      transaction,
      meter_wh,
      timestamp,
      context,
      event_type_label,
    )
  }

  fn transaction_event_v2_1_payload(
    request: &TransactionEventRequest,
    transaction: &TransactionState,
    meter_wh: i64,
    timestamp: &str,
    context: &str,
    event_type_label: &str,
  ) -> Value {
    Self::transaction_event_v2_x_payload(
      request,
      transaction,
      meter_wh,
      timestamp,
      context,
      event_type_label,
    )
  }

  fn transaction_event_v2_x_payload(
    request: &TransactionEventRequest,
    transaction: &TransactionState,
    meter_wh: i64,
    timestamp: &str,
    context: &str,
    event_type_label: &str,
  ) -> Value {
    to_value(&TransactionEvent_V2_X_Request {
      event_type: event_type_label,
      timestamp,
      trigger_reason: request.trigger_reason,
      seq_no: transaction.seq_no,
      transaction_info: TransactionInfoPayload {
        transaction_id: &transaction.transaction_uid,
        remote_start_id: request.remote_start_id,
        stopped_reason: request.stopped_reason,
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
          measurand: "Energy.Active.Import.Register",
          unit_of_measure: UnitOfMeasure { unit: "Wh" },
        }],
      }],
      id_token: request.id_token.as_deref().map(|token| IdTokenPayload {
        id_token: token,
        token_type: "Central",
      }),
    })
  }
}
