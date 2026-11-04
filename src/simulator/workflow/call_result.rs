use std::time::Instant;

use crate::ocpp::validate_heartbeat_interval;

use super::super::{
  BootRegistrationStatus, ConfigurationKey, ConfigurationOrigin,
  ConnectorStatus, DEFAULT_BOOT_RETRY_INTERVAL, MAX_BOOT_RETRY_INTERVAL,
  PendingContext, REDACTED_SENSITIVE_VALUE, ResponseStatus, Result, Simulator,
  StopReason, TransactionTriggerReason, TxEventType, UiLogLevel, Value,
  authorize_status,
};

impl Simulator {
  /// Applies side effects for a CALLRESULT based on the originating context.
  pub(in crate::simulator) fn apply_call_result_context(
    &mut self,
    context: &PendingContext,
    payload: &Value,
  ) -> Result<()> {
    match context {
      PendingContext::Boot => self.apply_boot_call_result(payload)?,
      PendingContext::Heartbeat => self.log_heartbeat_call_result(payload),
      PendingContext::DataTransfer => {
        self.log_data_transfer_call_result(payload);
      }
      PendingContext::DiagnosticsStatusNotification => {
        self.log_acknowledged("DiagnosticsStatusNotification");
      }
      PendingContext::FirmwareStatusNotification => {
        self.log_acknowledged("FirmwareStatusNotification");
      }
      PendingContext::LogStatusNotification => {
        self.log_acknowledged("LogStatusNotification");
      }
      PendingContext::SecurityEventNotification { event_id } => {
        self.mark_security_event_notification_sent(*event_id);
        self.log_acknowledged("SecurityEventNotification");
      }
      PendingContext::SignCertificate => {
        self.log_sign_certificate_call_result(payload);
      }
      PendingContext::SignedFirmwareStatusNotification => {
        self.log_acknowledged("SignedFirmwareStatusNotification");
      }
      PendingContext::Authorize { id_token } => {
        self.log_authorize_call_result(id_token, payload);
      }
      PendingContext::RemoteStartAuthorizeV1_6 {
        connector,
        id_token,
        charging_profile,
      } => {
        self.apply_remote_start_authorize_call_result(
          *connector,
          id_token,
          charging_profile.as_ref(),
          payload,
        );
      }
      PendingContext::StatusNotification { connector } => {
        self.log_status_notification_call_result(*connector);
      }
      PendingContext::StartTxV1_6 {
        connector,
        local_tx_id,
      } => {
        self.apply_start_transaction_call_result(
          *connector,
          *local_tx_id,
          payload,
        )?;
      }
      PendingContext::StopTxV1_6 {
        connector,
        local_tx_id,
      } => {
        self.apply_stop_transaction_call_result(*connector, *local_tx_id)?;
      }
      PendingContext::MeterValues { connector } => {
        self.log_meter_values_call_result(*connector);
      }
      PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type,
      } => {
        self.apply_transaction_event_call_result(
          *connector,
          *local_tx_id,
          *event_type,
          payload,
        )?;
      }
    }
    Ok(())
  }

  fn apply_boot_call_result(&mut self, payload: &Value) -> Result<()> {
    let status = payload
      .get("status")
      .and_then(Value::as_str)
      .and_then(ResponseStatus::parse)
      .unwrap_or(ResponseStatus::Unknown);
    let interval = payload.get("interval").and_then(Value::as_u64);
    self.log(
      UiLogLevel::Info,
      format!(
        "BootNotification response status={} interval={}",
        status.as_str(),
        interval.map_or_else(|| "-".to_string(), |value| value.to_string())
      ),
    );
    self.apply_boot_registration_status(status);
    if status == ResponseStatus::Accepted {
      self.boot_retry_after = None;
      if let Some(seconds) = interval {
        self.apply_boot_heartbeat_interval(seconds);
      }
      self.enqueue_boot_status_notifications()?;
      self.enqueue_pending_security_event_notifications();
    } else {
      self.apply_boot_retry_interval(interval);
    }
    Ok(())
  }

  /// Schedules registration retry using the response interval or a fallback.
  pub(in crate::simulator) fn apply_boot_retry_interval(
    &mut self,
    interval: Option<u64>,
  ) {
    let retry_interval = interval
      .filter(|seconds| *seconds > 0)
      .map_or(DEFAULT_BOOT_RETRY_INTERVAL, std::time::Duration::from_secs)
      .min(MAX_BOOT_RETRY_INTERVAL);
    self.boot_retry_after = Instant::now().checked_add(retry_interval);
  }

  fn apply_boot_registration_status(&mut self, status: ResponseStatus) {
    self.boot_registration_status = match status {
      ResponseStatus::Accepted => BootRegistrationStatus::Accepted,
      ResponseStatus::Pending => BootRegistrationStatus::Pending,
      _ => BootRegistrationStatus::Rejected,
    };
  }

  fn log_heartbeat_call_result(&mut self, payload: &Value) {
    let time = payload
      .get("currentTime")
      .and_then(Value::as_str)
      .unwrap_or("<unknown>");
    self.log(UiLogLevel::Info, format!("Heartbeat response time={time}"));
  }

  fn log_data_transfer_call_result(&mut self, payload: &Value) {
    let status = payload
      .get("status")
      .and_then(Value::as_str)
      .unwrap_or(ResponseStatus::Unknown.as_str());
    self.log(UiLogLevel::Info, format!("DataTransfer status={status}"));
  }

  fn log_sign_certificate_call_result(&mut self, payload: &Value) {
    let status = payload
      .get("status")
      .and_then(Value::as_str)
      .unwrap_or(ResponseStatus::Unknown.as_str());
    self.log(UiLogLevel::Info, format!("SignCertificate status={status}"));
  }

  fn log_acknowledged(&mut self, action: &str) {
    self.log(UiLogLevel::Info, format!("{action} acknowledged."));
  }

  fn log_authorize_call_result(&mut self, _id_token: &str, payload: &Value) {
    let status = authorize_status(self.config.protocol, payload);
    self.log(
      UiLogLevel::Info,
      format!("Authorize {REDACTED_SENSITIVE_VALUE} status={status}"),
    );
  }

  fn apply_remote_start_authorize_call_result(
    &mut self,
    connector: u16,
    id_token: &str,
    charging_profile: Option<&Value>,
    payload: &Value,
  ) {
    let status = parse_v1_6_id_tag_status(payload);
    self.log(
      UiLogLevel::Info,
      format!(
        "RemoteStartTransaction authorization {} status={}",
        REDACTED_SENSITIVE_VALUE,
        status.as_str()
      ),
    );
    if status == ResponseStatus::Accepted {
      if let Err(error) =
        Self::validate_remote_start_charging_profile(charging_profile)
      {
        self.log(
          UiLogLevel::Warn,
          format!(
            "RemoteStartTransaction authorization accepted but charging \
            profile was rejected on connector {connector}: {error}"
          ),
        );
        return;
      }
      if let Err(error) = self
        .start_transaction(connector, id_token.to_string(), true, None, true)
        .and_then(|()| {
          self.apply_remote_start_charging_profile(connector, charging_profile)
        })
      {
        self.log(
          UiLogLevel::Warn,
          format!(
            "RemoteStartTransaction authorization accepted but \
            start/profile application failed on connector {connector}: \
            {error}"
          ),
        );
      }
    } else {
      self.log(
        UiLogLevel::Warn,
        format!(
          "RemoteStartTransaction not started on connector {}: \
          authorization status={}",
          connector,
          status.as_str()
        ),
      );
    }
  }

  fn log_status_notification_call_result(&mut self, connector: u16) {
    self.log(
      UiLogLevel::Info,
      format!("StatusNotification acknowledged for connector {connector}"),
    );
  }

  fn apply_start_transaction_call_result(
    &mut self,
    connector: u16,
    local_tx_id: u64,
    payload: &Value,
  ) -> Result<()> {
    let status = parse_v1_6_id_tag_status(payload);
    let Some(transaction_id) =
      payload.get("transactionId").and_then(Value::as_i64)
    else {
      self.log(
        UiLogLevel::Warn,
        "StartTransaction response omitted transactionId.",
      );
      self.cancel_transaction_start(connector, local_tx_id)?;
      self.enqueue_status_notification(connector)?;
      return Ok(());
    };
    self.set_v1_6_transaction_id(connector, local_tx_id, transaction_id)?;
    self.log(
      UiLogLevel::Info,
      format!(
        "StartTransaction status={} on connector {connector} \
        transactionId={transaction_id}",
        status.as_str()
      ),
    );
    if status != ResponseStatus::Accepted {
      self.enqueue_deauthorized_stop_v1_6(connector, local_tx_id)?;
    }
    self.enqueue_status_notification(connector)?;
    Ok(())
  }

  fn enqueue_deauthorized_stop_v1_6(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    if let Some(transaction) =
      self.connector_mut(connector)?.transaction.as_mut()
    {
      transaction.deauthorized = true;
    }
    // A local stop can already be waiting behind StartTransaction. Reuse its
    // original time and meter sample instead of reporting the transaction twice.
    if let Some(stop) = self.queue.iter_mut().find(|call| {
      matches!(
        call.context,
        PendingContext::StopTxV1_6 {
          connector: target,
          local_tx_id: transaction,
        } if target == connector && transaction == local_tx_id
      )
    }) {
      stop.payload["reason"] = StopReason::DeAuthorized.as_str().into();
      return Ok(());
    }
    self.connector_mut(connector)?.status = ConnectorStatus::SuspendedEvse;
    if let Err(error) = self.stop_transaction(
      connector,
      Some(StopReason::DeAuthorized.as_str()),
      false,
      true,
    ) {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Deauthorized transaction on connector {connector} remains \
          suspended; retry stop after the queue drains: {error}"
        ),
      );
      self.emit_snapshot();
      // The response is valid. Keep the connection alive so queued calls can
      // drain and the operator can retry this stop with its retained CSMS ID.
    }
    Ok(())
  }

  fn apply_stop_transaction_call_result(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    self.complete_transaction_stop(connector, local_tx_id)?;
    self.enqueue_status_notification(connector)?;
    self.log(
      UiLogLevel::Info,
      format!(
        "StopTransaction acknowledged on connector {connector} \
        localTx={local_tx_id}"
      ),
    );
    Ok(())
  }

  fn log_meter_values_call_result(&mut self, connector: u16) {
    self.log(
      UiLogLevel::Info,
      format!("MeterValues acknowledged on connector {connector}"),
    );
  }

  fn apply_transaction_event_call_result(
    &mut self,
    connector: u16,
    local_tx_id: u64,
    event_type: TxEventType,
    payload: &Value,
  ) -> Result<()> {
    if self.apply_transaction_event_authorization_status(
      connector,
      local_tx_id,
      event_type,
      payload,
    )? {
      return Ok(());
    }
    match event_type {
      TxEventType::Started => {
        self.enqueue_status_notification(connector)?;
      }
      TxEventType::Ended => {
        self.complete_transaction_stop(connector, local_tx_id)?;
        self.enqueue_status_notification(connector)?;
      }
      TxEventType::Updated => {}
    }
    self.log(
      UiLogLevel::Info,
      format!(
        "TransactionEvent {event_type:?} acknowledged \
        connector={connector} localTx={local_tx_id}"
      ),
    );
    Ok(())
  }

  fn apply_transaction_event_authorization_status(
    &mut self,
    connector: u16,
    local_tx_id: u64,
    event_type: TxEventType,
    payload: &Value,
  ) -> Result<bool> {
    let Some(status) = parse_v2_x_id_token_status(payload) else {
      return Ok(false);
    };
    if status == ResponseStatus::Accepted {
      self.mark_transaction_authorized(connector, local_tx_id);
      return Ok(false);
    }

    self.log(
      UiLogLevel::Warn,
      format!(
        "TransactionEvent {event_type:?} authorization status={} \
        connector={connector} localTx={local_tx_id}; stopping transaction.",
        status.as_str()
      ),
    );
    if matches!(event_type, TxEventType::Started | TxEventType::Updated) {
      self.enqueue_deauthorized_transaction_end(connector, local_tx_id)?;
      return Ok(true);
    }
    Ok(false)
  }

  fn mark_transaction_authorized(&mut self, connector: u16, local_tx_id: u64) {
    // A response to an older queued event cannot revive a deauthorized session.
    let Some(transaction) = self
      .connectors
      .get_mut(&connector)
      .and_then(|state| state.transaction.as_mut())
      .filter(|transaction| {
        transaction.local_id == local_tx_id && !transaction.deauthorized
      })
    else {
      return;
    };
    transaction.authorization_accepted = true;
  }

  fn enqueue_deauthorized_transaction_end(
    &mut self,
    connector: u16,
    local_tx_id: u64,
  ) -> Result<()> {
    let Some(transaction) = self
      .connectors
      .get_mut(&connector)
      .and_then(|state| state.transaction.as_mut())
      .filter(|transaction| transaction.local_id == local_tx_id)
    else {
      return Ok(());
    };
    // Persist the decision before queueing so failures cannot restore charging.
    transaction.deauthorized = true;
    transaction.authorization_accepted = false;
    self.connector_mut(connector)?.status = ConnectorStatus::SuspendedEvse;

    // A stop may already be queued behind the event being rejected. Preserve
    // its original sequence number, timestamp, and meter reading.
    if let Some(end) = self.queue.iter_mut().find(|call| {
      matches!(
        call.context,
        PendingContext::TxEvent {
          connector: target,
          local_tx_id: transaction,
          event_type: TxEventType::Ended,
        } if target == connector && transaction == local_tx_id
      )
    }) {
      end.payload["triggerReason"] =
        TransactionTriggerReason::Deauthorized.as_str().into();
      end.payload["transactionInfo"]["stoppedReason"] =
        StopReason::DeAuthorized.as_str().into();
      self.connector_mut(connector)?.status = ConnectorStatus::Finishing;
    } else if let Err(error) =
      self.stop_transaction(connector, None, false, true)
    {
      // The CSMS response is valid. Keep the transport alive to drain queued
      // work and allow a later local or remote stop to retry the terminal event.
      self.log(
        UiLogLevel::Warn,
        format!(
          "Deauthorized transaction on connector {connector} remains \
          suspended; retry stop after the queue drains: {error}"
        ),
      );
      self.emit_snapshot();
    }
    self.emit_runtime_state();
    Ok(())
  }

  /// Applies an accepted `BootNotification.conf.interval` to heartbeats.
  fn apply_boot_heartbeat_interval(&mut self, seconds: u64) {
    if let Err(error) = validate_heartbeat_interval(seconds) {
      self.log(
        UiLogLevel::Warn,
        format!(
          "Ignoring BootNotification heartbeat interval {seconds}: {error}"
        ),
      );
      return;
    }
    self.update_configuration_value(
      ConfigurationKey::HeartbeatInterval,
      &seconds.to_string(),
      ConfigurationOrigin::Csms,
    );
    self.start_heartbeat(seconds);
  }
}

fn parse_v1_6_id_tag_status(payload: &Value) -> ResponseStatus {
  payload
    .get("idTagInfo")
    .and_then(Value::as_object)
    .and_then(|info| info.get("status"))
    .and_then(Value::as_str)
    .and_then(ResponseStatus::parse)
    .unwrap_or(ResponseStatus::Unknown)
}

fn parse_v2_x_id_token_status(payload: &Value) -> Option<ResponseStatus> {
  let info = payload.get("idTokenInfo")?.as_object()?;
  Some(
    info
      .get("status")
      .and_then(Value::as_str)
      .and_then(ResponseStatus::parse)
      .unwrap_or(ResponseStatus::Unknown),
  )
}
