use super::super::{
  CertificateType, ExtendedTriggerMessage_V1_6, ResponseStatus, Result,
  Simulator, TransactionEventRequest, TransactionTriggerReason,
  TriggerMessage_V1_6, TriggerMessage_V2_X, TxEventType,
};

impl Simulator {
  /// Handles OCPP 1.6 `TriggerMessage.req` by enqueueing requested messages.
  pub(in crate::simulator) fn trigger_message_v1_6_standard(
    &mut self,
    requested: TriggerMessage_V1_6,
    connector: Option<u16>,
  ) -> Result<ResponseStatus> {
    match requested {
      TriggerMessage_V1_6::BootNotification => {
        self.enqueue_boot_notification();
      }
      TriggerMessage_V1_6::DiagnosticsStatusNotification => {
        self.enqueue_diagnostics_status_notification(
          ResponseStatus::Idle.as_str(),
        );
      }
      TriggerMessage_V1_6::FirmwareStatusNotification => {
        self.enqueue_firmware_status_notification("Idle", None);
      }
      TriggerMessage_V1_6::Heartbeat => {
        self.enqueue_heartbeat();
      }
      TriggerMessage_V1_6::MeterValues => {
        self.enqueue_meter_values_for_trigger(connector)?;
      }
      TriggerMessage_V1_6::StatusNotification => {
        self.enqueue_status_notifications_for_trigger(connector)?;
      }
    }
    Ok(ResponseStatus::Accepted)
  }

  /// Handles OCPP 1.6 security `ExtendedTriggerMessage.req`.
  pub(in crate::simulator) fn extended_trigger_message_v1_6(
    &mut self,
    requested: ExtendedTriggerMessage_V1_6,
    connector: Option<u16>,
  ) -> Result<ResponseStatus> {
    match requested {
      ExtendedTriggerMessage_V1_6::BootNotification => {
        self.enqueue_boot_notification();
      }
      ExtendedTriggerMessage_V1_6::FirmwareStatusNotification => {
        self.enqueue_firmware_status_notification("Idle", None);
      }
      ExtendedTriggerMessage_V1_6::Heartbeat => {
        self.enqueue_heartbeat();
      }
      ExtendedTriggerMessage_V1_6::LogStatusNotification => {
        self.enqueue_log_status_notification("Idle", None);
      }
      ExtendedTriggerMessage_V1_6::MeterValues => {
        self.enqueue_meter_values_for_trigger(connector)?;
      }
      ExtendedTriggerMessage_V1_6::StatusNotification => {
        self.enqueue_status_notifications_for_trigger(connector)?;
      }
      ExtendedTriggerMessage_V1_6::SignChargePointCertificate => {
        self.enqueue_sign_certificate(None);
      }
    }
    Ok(ResponseStatus::Accepted)
  }

  /// Handles OCPP 2.x `TriggerMessage.req` by enqueueing requested messages.
  pub(in crate::simulator) fn trigger_message_v2_x(
    &mut self,
    requested: TriggerMessage_V2_X,
    connector: Option<u16>,
  ) -> Result<ResponseStatus> {
    match requested {
      TriggerMessage_V2_X::BootNotification => {
        self.enqueue_boot_notification();
      }
      TriggerMessage_V2_X::FirmwareStatusNotification => {
        self.enqueue_firmware_status_notification("Idle", None);
      }
      TriggerMessage_V2_X::Heartbeat => {
        self.enqueue_heartbeat();
      }
      TriggerMessage_V2_X::LogStatusNotification => {
        self.enqueue_log_status_notification("Idle", None);
      }
      TriggerMessage_V2_X::MeterValues => {
        self.enqueue_meter_values_for_trigger(connector)?;
      }
      TriggerMessage_V2_X::StatusNotification => {
        self.enqueue_status_notifications_for_trigger(connector)?;
      }
      TriggerMessage_V2_X::SignChargingStationCertificate => {
        self.enqueue_sign_certificate(Some(
          CertificateType::ChargingStationCertificate.as_str(),
        ));
      }
      TriggerMessage_V2_X::SignV2G20Certificate => {
        self.enqueue_sign_certificate(Some("V2G20Certificate"));
      }
      TriggerMessage_V2_X::SignV2GCertificate => {
        self.enqueue_sign_certificate(Some(
          CertificateType::V2GCertificate.as_str(),
        ));
      }
      TriggerMessage_V2_X::TransactionEvent => {
        return self.trigger_transaction_event_v2_x(connector);
      }
      TriggerMessage_V2_X::PublishFirmwareStatusNotification
      | TriggerMessage_V2_X::SignCombinedCertificate
      | TriggerMessage_V2_X::CustomTrigger => {
        return Ok(ResponseStatus::NotImplemented);
      }
    }
    Ok(ResponseStatus::Accepted)
  }

  fn enqueue_meter_values_for_trigger(
    &mut self,
    connector: Option<u16>,
  ) -> Result<()> {
    if let Some(connector_id) = connector {
      self.enqueue_meter_values(connector_id)?;
    } else {
      let connectors: Vec<u16> = self.connectors.keys().copied().collect();
      for connector_id in connectors {
        self.enqueue_meter_values(connector_id)?;
      }
    }
    Ok(())
  }

  fn enqueue_status_notifications_for_trigger(
    &mut self,
    connector: Option<u16>,
  ) -> Result<()> {
    if let Some(connector_id) = connector {
      self.enqueue_status_notification(connector_id)?;
    } else {
      let connectors: Vec<u16> = self.connectors.keys().copied().collect();
      for connector_id in connectors {
        self.enqueue_status_notification(connector_id)?;
      }
    }
    Ok(())
  }

  fn trigger_transaction_event_v2_x(
    &mut self,
    connector: Option<u16>,
  ) -> Result<ResponseStatus> {
    let connectors = if let Some(connector_id) = connector {
      vec![connector_id]
    } else {
      self.connectors.keys().copied().collect()
    };
    let mut enqueued = false;
    for connector_id in connectors {
      let Some(local_tx_id) = self
        .connectors
        .get(&connector_id)
        .and_then(|state| state.transaction.as_ref())
        .map(|transaction| transaction.local_id)
      else {
        continue;
      };
      self.enqueue_transaction_event(&TransactionEventRequest {
        connector: connector_id,
        local_tx_id,
        event_type: TxEventType::Updated,
        trigger_reason: TransactionTriggerReason::Trigger,
        id_token: None,
        remote_start_id: None,
        stopped_reason: None,
      })?;
      enqueued = true;
    }
    Ok(if enqueued {
      ResponseStatus::Accepted
    } else {
      ResponseStatus::Rejected
    })
  }
}
