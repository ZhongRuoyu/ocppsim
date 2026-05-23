use super::super::{
  Result, Simulator, TriggerMessage_V1_6, TriggerMessage_V2_X,
};

impl Simulator {
  /// Handles OCPP 1.6 `TriggerMessage.req` by enqueueing requested messages.
  pub(in crate::simulator) fn trigger_message_v1_6(
    &mut self,
    requested: TriggerMessage_V1_6,
    connector: Option<u16>,
  ) -> Result<()> {
    match requested {
      TriggerMessage_V1_6::BootNotification => {
        self.enqueue_boot_notification();
      }
      TriggerMessage_V1_6::Heartbeat => {
        self.enqueue_heartbeat();
      }
      TriggerMessage_V1_6::MeterValues => {
        if let Some(connector_id) = connector {
          self.enqueue_meter_values(connector_id)?;
        } else {
          let connectors: Vec<u16> = self.connectors.keys().copied().collect();
          for connector_id in connectors {
            self.enqueue_meter_values(connector_id)?;
          }
        }
      }
      TriggerMessage_V1_6::StatusNotification => {
        if let Some(connector_id) = connector {
          self.enqueue_status_notification(connector_id)?;
        } else {
          let connectors: Vec<u16> = self.connectors.keys().copied().collect();
          for connector_id in connectors {
            self.enqueue_status_notification(connector_id)?;
          }
        }
      }
    }
    Ok(())
  }

  /// Handles OCPP 2.x `TriggerMessage.req` by enqueueing requested messages.
  pub(in crate::simulator) fn trigger_message_v2_x(
    &mut self,
    requested: TriggerMessage_V2_X,
    connector: Option<u16>,
  ) -> Result<()> {
    match requested {
      TriggerMessage_V2_X::BootNotification => {
        self.enqueue_boot_notification();
      }
      TriggerMessage_V2_X::Heartbeat => {
        self.enqueue_heartbeat();
      }
      TriggerMessage_V2_X::MeterValues => {
        if let Some(connector_id) = connector {
          self.enqueue_meter_values(connector_id)?;
        } else {
          let connectors: Vec<u16> = self.connectors.keys().copied().collect();
          for connector_id in connectors {
            self.enqueue_meter_values(connector_id)?;
          }
        }
      }
      TriggerMessage_V2_X::StatusNotification => {
        if let Some(connector_id) = connector {
          self.enqueue_status_notification(connector_id)?;
        } else {
          let connectors: Vec<u16> = self.connectors.keys().copied().collect();
          for connector_id in connectors {
            self.enqueue_status_notification(connector_id)?;
          }
        }
      }
    }
    Ok(())
  }
}
