use super::super::*;

impl Simulator {
  /// Applies side effects for a CALLRESULT based on the originating context.
  pub(in crate::simulator) fn apply_call_result_context(
    &mut self,
    context: &PendingContext,
    payload: &Value,
  ) -> Result<()> {
    match context {
      PendingContext::Boot => {
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
            interval
              .map(|value| value.to_string())
              .unwrap_or_else(|| "-".to_string())
          ),
        );
        if status == ResponseStatus::Accepted
          && let Some(seconds) = interval
        {
          self.apply_boot_heartbeat_interval(seconds);
        }
      }
      PendingContext::Heartbeat => {
        let time = payload
          .get("currentTime")
          .and_then(Value::as_str)
          .unwrap_or("<unknown>");
        self.log(UiLogLevel::Info, format!("Heartbeat response time={time}"));
      }
      PendingContext::DataTransfer => {
        let status = payload
          .get("status")
          .and_then(Value::as_str)
          .unwrap_or(ResponseStatus::Unknown.as_str());
        self.log(UiLogLevel::Info, format!("DataTransfer status={status}"));
      }
      PendingContext::DiagnosticsStatusNotification => {
        self.log(
          UiLogLevel::Info,
          "DiagnosticsStatusNotification acknowledged.",
        );
      }
      PendingContext::FirmwareStatusNotification => {
        self.log(UiLogLevel::Info, "FirmwareStatusNotification acknowledged.");
      }
      PendingContext::LogStatusNotification => {
        self.log(UiLogLevel::Info, "LogStatusNotification acknowledged.");
      }
      PendingContext::Authorize { id_token } => {
        let status = authorize_status(self.config.protocol, payload);
        self.log(
          UiLogLevel::Info,
          format!("Authorize {} status={}", id_token, status),
        );
      }
      PendingContext::RemoteStartAuthorizeV1_6 {
        connector,
        id_token,
      } => {
        let status = payload
          .get("idTagInfo")
          .and_then(Value::as_object)
          .and_then(|info| info.get("status"))
          .and_then(Value::as_str)
          .and_then(ResponseStatus::parse)
          .unwrap_or(ResponseStatus::Unknown);
        self.log(
          UiLogLevel::Info,
          format!(
            "RemoteStartTransaction authorization {} status={}",
            id_token,
            status.as_str()
          ),
        );
        if status == ResponseStatus::Accepted {
          if let Err(error) = self.start_transaction(
            *connector,
            id_token.clone(),
            true,
            None,
            true,
          ) {
            self.log(
              UiLogLevel::Warn,
              format!(
                "RemoteStartTransaction authorization accepted but start \
                failed on connector {}: {}",
                connector, error
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
      PendingContext::StatusNotification { connector } => {
        self.log(
          UiLogLevel::Info,
          format!(
            "StatusNotification acknowledged for connector {}",
            connector
          ),
        );
      }
      PendingContext::StartTxV1_6 {
        connector,
        local_tx_id,
      } => {
        let status = payload
          .get("idTagInfo")
          .and_then(Value::as_object)
          .and_then(|info| info.get("status"))
          .and_then(Value::as_str)
          .and_then(ResponseStatus::parse)
          .unwrap_or(ResponseStatus::Unknown);
        let transaction_id =
          payload.get("transactionId").and_then(Value::as_i64);
        if status == ResponseStatus::Accepted {
          if let Some(tx_id) = transaction_id {
            self.set_v1_6_transaction_id(*connector, *local_tx_id, tx_id)?;
            self.log(
              UiLogLevel::Info,
              format!(
                "StartTransaction accepted on connector {} transactionId={}",
                connector, tx_id
              ),
            );
          } else {
            self.log(
              UiLogLevel::Warn,
              "StartTransaction accepted without transactionId.",
            );
          }
          self.enqueue_status_notification(*connector)?;
        } else {
          self.cancel_transaction_start(*connector, *local_tx_id)?;
          self.enqueue_status_notification(*connector)?;
          self.log(
            UiLogLevel::Warn,
            format!(
              "StartTransaction status={} on connector {}.",
              status.as_str(),
              connector
            ),
          );
        }
      }
      PendingContext::StopTxV1_6 {
        connector,
        local_tx_id,
      } => {
        self.complete_transaction_stop(*connector, *local_tx_id)?;
        self.enqueue_status_notification(*connector)?;
        self.log(
          UiLogLevel::Info,
          format!(
            "StopTransaction acknowledged on connector {} localTx={}",
            connector, local_tx_id
          ),
        );
      }
      PendingContext::MeterValues { connector } => {
        self.log(
          UiLogLevel::Info,
          format!("MeterValues acknowledged on connector {}", connector),
        );
      }
      PendingContext::TxEvent {
        connector,
        local_tx_id,
        event_type,
      } => {
        match event_type {
          TxEventType::Started => {
            self.enqueue_status_notification(*connector)?;
          }
          TxEventType::Ended => {
            self.complete_transaction_stop(*connector, *local_tx_id)?;
            self.enqueue_status_notification(*connector)?;
          }
          TxEventType::Updated => {}
        }
        self.log(
          UiLogLevel::Info,
          format!(
            "TransactionEvent {:?} acknowledged connector={} localTx={}",
            event_type, connector, local_tx_id
          ),
        );
      }
    }
    Ok(())
  }

  /// Applies an accepted `BootNotification.conf.interval` to heartbeats.
  fn apply_boot_heartbeat_interval(&mut self, seconds: u64) {
    if seconds == 0 {
      self.log(
        UiLogLevel::Warn,
        "BootNotification returned heartbeat interval 0; ignoring.",
      );
      return;
    }
    if let Some(entry) = self
      .configuration
      .get_mut(&ConfigurationKey::HeartbeatInterval)
    {
      entry.value = seconds.to_string();
    }
    self.start_heartbeat(seconds);
  }
}
