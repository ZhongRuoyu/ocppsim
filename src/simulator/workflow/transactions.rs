use super::super::payloads::*;
use super::super::*;

impl Simulator {
  /// Starts a local transaction and enqueues protocol-specific start messages.
  pub(in crate::simulator) fn start_transaction(
    &mut self,
    connector: u16,
    id_token: String,
    remote_start: bool,
    remote_start_id: Option<i64>,
    is_connected: bool,
  ) -> Result<()> {
    self.validate_start_connector(connector)?;

    let status = if self.config.protocol == OcppVersion::V1_6 {
      ConnectorStatus::Charging
    } else {
      ConnectorStatus::Occupied
    };

    let local_tx_id = self.next_tx_id;
    self.next_tx_id = self.next_tx_id.saturating_add(1);

    {
      let connector_state = self.connector_mut(connector)?;
      let tx_uid = format!("tx-{}", local_tx_id);
      connector_state.transaction = Some(TransactionState {
        local_id: local_tx_id,
        transaction_uid: tx_uid,
        id_token: id_token.clone(),
        v1_6_transaction_id: None,
        remote_start_id,
        seq_no: 0,
      });
      connector_state.status = status;
    }

    self.log(
      UiLogLevel::Info,
      format!(
        "Transaction started locally on connector {} with idToken {}",
        connector, id_token
      ),
    );

    if !is_connected {
      self.log(
        UiLogLevel::Warn,
        "Not connected. Transaction state updated locally only.",
      );
      return Ok(());
    }

    match self.config.protocol {
      OcppVersion::V1_6 => {
        let meter_start = self
          .connectors
          .get(&connector)
          .map(|state| state.meter_wh)
          .unwrap_or(0);
        let timestamp = now_timestamp();
        let payload = to_value(&StartTransactionV1_6Request {
          connector_id: connector,
          id_tag: &id_token,
          meter_start,
          timestamp: &timestamp,
        });
        self.enqueue_call(
          OutgoingAction::StartTransaction.as_str(),
          payload,
          PendingContext::StartTxV1_6 {
            connector,
            local_tx_id,
          },
        );
      }
      OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
        self.bump_seq_no(connector, local_tx_id)?;
        let trigger_reason = if remote_start {
          TransactionTriggerReason::RemoteStart
        } else {
          TransactionTriggerReason::Authorized
        };
        self.enqueue_transaction_event(TransactionEventRequest {
          connector,
          local_tx_id,
          event_type: TxEventType::Started,
          trigger_reason,
          id_token: Some(id_token),
          remote_start_id,
          stopped_reason: None,
        })?;
      }
    }
    Ok(())
  }

  /// Stops an active transaction and enqueues protocol-specific stop messages.
  pub(in crate::simulator) fn stop_transaction(
    &mut self,
    connector: u16,
    reason: Option<String>,
    remote_stop: bool,
    is_connected: bool,
  ) -> Result<()> {
    let (local_tx_id, tx_uid, v1_6_tx_id, remote_start_id, token) = {
      let connector_state = self.connector_mut(connector)?;
      let Some(transaction) = connector_state.transaction.as_ref() else {
        return Err(anyhow!(
          "No active transaction on connector {}.",
          connector
        ));
      };
      (
        transaction.local_id,
        transaction.transaction_uid.clone(),
        transaction.v1_6_transaction_id,
        transaction.remote_start_id,
        transaction.id_token.clone(),
      )
    };

    if self.config.protocol != OcppVersion::V1_6 {
      self.bump_seq_no(connector, local_tx_id)?;
    }

    match self.config.protocol {
      OcppVersion::V1_6 => {
        self.connector_mut(connector)?.status = ConnectorStatus::Finishing;

        self.log(
          UiLogLevel::Info,
          format!("Transaction stopped locally on connector {}", connector),
        );

        if !is_connected {
          self.complete_transaction_stop(connector, local_tx_id)?;
          self.log(
            UiLogLevel::Warn,
            "Not connected. Transaction stop is local only.",
          );
          return Ok(());
        }

        let connector_state = self.connector_ref(connector)?;
        let tx_id = v1_6_tx_id.unwrap_or(local_tx_id as i64);
        let timestamp = now_timestamp();
        let stop_reason = map_stop_reason_v1_6(reason.as_deref(), remote_stop);
        let reason_str =
          stop_reason.as_v1_6().unwrap_or(StopReason::Local.as_str());
        let payload = to_value(&StopTransactionV1_6Request {
          transaction_id: tx_id,
          timestamp: &timestamp,
          meter_stop: connector_state.meter_wh,
          id_tag: &token,
          reason: reason_str,
        });
        self.enqueue_call(
          OutgoingAction::StopTransaction.as_str(),
          payload,
          PendingContext::StopTxV1_6 {
            connector,
            local_tx_id,
          },
        );
      }
      OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
        let _ = tx_uid;
        if !is_connected {
          self.complete_transaction_stop(connector, local_tx_id)?;
          self.log(
            UiLogLevel::Info,
            format!("Transaction stopped locally on connector {}", connector),
          );
          self.log(
            UiLogLevel::Warn,
            "Not connected. Transaction stop is local only.",
          );
          return Ok(());
        }

        let trigger_reason = if remote_stop {
          TransactionTriggerReason::RemoteStop
        } else {
          TransactionTriggerReason::StopAuthorized
        };
        let stopped_reason = map_stop_reason_v2_x(
          self.config.protocol,
          reason.as_deref(),
          remote_stop,
        );
        self.enqueue_transaction_event(TransactionEventRequest {
          connector,
          local_tx_id,
          event_type: TxEventType::Ended,
          trigger_reason,
          id_token: None,
          remote_start_id,
          stopped_reason: Some(stopped_reason),
        })?;

        self.connector_mut(connector)?.status = ConnectorStatus::Finishing;
        self.log(
          UiLogLevel::Info,
          format!("Transaction stopped locally on connector {}", connector),
        );
      }
    }
    Ok(())
  }

  /// Sets a connector meter reading in watt-hours.
  pub(in crate::simulator) fn set_meter(
    &mut self,
    connector: u16,
    value_wh: i64,
  ) -> Result<()> {
    let connector_state = self.connector_mut(connector)?;
    connector_state.meter_wh = value_wh;
    self.log(
      UiLogLevel::Info,
      format!("Connector {} meter set to {} Wh", connector, value_wh),
    );
    Ok(())
  }

  /// Sends meter data for a connector using protocol-specific message types.
  pub(in crate::simulator) fn send_meter(
    &mut self,
    connector: u16,
    is_connected: bool,
  ) -> Result<()> {
    if !is_connected {
      self.log(
        UiLogLevel::Warn,
        "Not connected. Connect first to send MeterValues.",
      );
      return Ok(());
    }

    let (has_tx, local_tx_id, remote_start_id) = {
      let connector_state = self.connector_ref(connector)?;
      let tx = connector_state.transaction.as_ref();
      (
        tx.is_some(),
        tx.map(|item| item.local_id).unwrap_or(0),
        tx.and_then(|item| item.remote_start_id),
      )
    };

    match self.config.protocol {
      OcppVersion::V1_6 => {
        self.enqueue_meter_values(connector)?;
      }
      OcppVersion::V2_0_1 | OcppVersion::V2_1 => {
        if has_tx {
          self.bump_seq_no(connector, local_tx_id)?;
          self.enqueue_transaction_event(TransactionEventRequest {
            connector,
            local_tx_id,
            event_type: TxEventType::Updated,
            trigger_reason: TransactionTriggerReason::MeterValuePeriodic,
            id_token: None,
            remote_start_id,
            stopped_reason: None,
          })?;
        } else {
          self.enqueue_meter_values(connector)?;
        }
      }
    }
    Ok(())
  }

  /// Sets connector status locally and optionally notifies CSMS.
  pub(in crate::simulator) fn set_connector_status(
    &mut self,
    connector: u16,
    status: &str,
    is_connected: bool,
  ) -> Result<()> {
    let status = ConnectorStatus::parse(status).ok_or_else(|| {
      anyhow!(
        "Invalid status. Use one of: Available, Preparing, Charging, \
         SuspendedEVSE, SuspendedEV, Finishing, Reserved, Unavailable, \
         Faulted, Occupied."
      )
    })?;
    let connector_state = self.connector_mut(connector)?;
    connector_state.status = status;
    self.log(
      UiLogLevel::Info,
      format!("Connector {} status set to {}", connector, status.display()),
    );
    if is_connected {
      self.enqueue_status_notification(connector)?;
    }
    Ok(())
  }
}
