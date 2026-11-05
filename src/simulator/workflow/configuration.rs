use anyhow::{Result, anyhow, bail};

use super::super::{
  ConfigurationEntry, ConfigurationKey, ConfigurationOrigin, OcppVersion,
  REDACTED_SENSITIVE_VALUE, ResponseStatus, Simulator, UiLogLevel,
};
use crate::ocpp::{is_valid_basic_auth_password, validate_heartbeat_interval};

impl Simulator {
  /// Sets a local heartbeat interval and starts the timer, including if paused.
  pub(in crate::simulator) fn start_local_heartbeat(
    &mut self,
    seconds: u64,
  ) -> Result<()> {
    let was_running = self.heartbeat.is_some();
    self.set_local_configuration(
      ConfigurationKey::HeartbeatInterval.as_str(),
      &seconds.to_string(),
    )?;
    if !was_running {
      self.start_heartbeat(seconds);
    }
    Ok(())
  }

  /// Displays one or all configuration entries without exposing passwords.
  pub(in crate::simulator) fn show_configuration(
    &self,
    key: Option<&str>,
  ) -> Result<()> {
    if let Some(name) = key {
      let key = self.resolve_local_configuration_key(name)?;
      self.show_configuration_entry(key, &self.configuration[&key]);
    } else {
      let mut entries = self.configuration.iter().collect::<Vec<_>>();
      entries.sort_unstable_by_key(|(key, _)| key.as_str());
      for (key, entry) in entries {
        self.show_configuration_entry(*key, entry);
      }
    }
    Ok(())
  }

  /// Validates and applies a local write, then displays its resulting value.
  pub(in crate::simulator) fn set_local_configuration(
    &mut self,
    name: &str,
    value: &str,
  ) -> Result<()> {
    let key = self.resolve_local_configuration_key(name)?;
    if self.configuration[&key].read_only {
      bail!("Configuration item {} is read-only.", key.as_str());
    }
    validate_configuration_value(self.config.protocol, key, value).map_err(
      |error| anyhow!("Invalid value for {}: {error}", key.as_str()),
    )?;
    let status =
      self.set_configuration_value(key, value, ConfigurationOrigin::Local);
    if status != ResponseStatus::Accepted {
      bail!(
        "Configuration item {} rejected the change; check its prerequisites \
        and the active protocol.",
        key.as_str()
      );
    }
    self.show_configuration_entry(key, &self.configuration[&key]);
    Ok(())
  }

  fn resolve_local_configuration_key(
    &self,
    name: &str,
  ) -> Result<ConfigurationKey> {
    self
      .configuration_key(name)
      .ok_or_else(|| anyhow!("Unsupported configuration item: {name}."))
  }

  fn show_configuration_entry(
    &self,
    key: ConfigurationKey,
    entry: &ConfigurationEntry,
  ) {
    let (value, access) = if key.is_sensitive() {
      (REDACTED_SENSITIVE_VALUE, "write-only")
    } else {
      (
        entry.value.as_str(),
        if entry.read_only {
          "read-only"
        } else {
          "read-write"
        },
      )
    };
    self.log(
      UiLogLevel::Info,
      format!(
        "{}={value} ({access}, origin={})",
        key.as_str(),
        entry.origin.label()
      ),
    );
  }
}

/// Checks supported value formats before any local or CSMS write takes effect.
pub(in crate::simulator) fn validate_configuration_value(
  protocol: OcppVersion,
  key: ConfigurationKey,
  value: &str,
) -> Result<()> {
  let max_length = match protocol {
    OcppVersion::V1_6 => 500,
    OcppVersion::V2_0_1 => 1_000,
    OcppVersion::V2_1 => 2_500,
  };
  if value.chars().count() > max_length || value.chars().any(char::is_control) {
    bail!(
      "Expected at most {max_length} characters without control characters."
    );
  }
  match key {
    ConfigurationKey::AllowOfflineTxForUnknownId
    | ConfigurationKey::AuthorizeRemoteTxRequests
    | ConfigurationKey::AllowSecurityProfileDowngrade
    | ConfigurationKey::AdditionalRootCertificateCheck => {
      if !key
        .value_options()
        .iter()
        .any(|option| value.eq_ignore_ascii_case(option))
      {
        bail!("Expected true or false.");
      }
    }
    ConfigurationKey::HeartbeatInterval => {
      let seconds = value
        .parse::<u64>()
        .map_err(|_| anyhow!("Expected a positive integer."))?;
      validate_heartbeat_interval(seconds).map_err(anyhow::Error::msg)?;
    }
    ConfigurationKey::MeterValueSampleInterval
    | ConfigurationKey::WebSocketPingInterval
    | ConfigurationKey::CertSigningRepeatTimes
    | ConfigurationKey::CertSigningWaitMinimum => {
      if !value.parse::<i32>().is_ok_and(|number| number >= 0) {
        bail!("Expected an integer from 0 to {}.", i32::MAX);
      }
    }
    ConfigurationKey::AuthorizationKey
    | ConfigurationKey::BasicAuthPassword => {
      if !is_valid_basic_auth_password(protocol, value) {
        bail!("Invalid password format for OCPP {}.", protocol.label());
      }
    }
    ConfigurationKey::SecurityProfile => {
      if !value.parse::<u8>().is_ok_and(|number| number <= 3) {
        bail!("Expected a security profile from 0 to 3.");
      }
    }
    ConfigurationKey::CpoName | ConfigurationKey::OrganizationName => {
      if value.trim().is_empty() {
        bail!("Expected a nonempty organization name.");
      }
    }
    ConfigurationKey::SupportedFileTransferProtocols
      if !value.split(',').all(|item| {
        key
          .value_options()
          .iter()
          .any(|allowed| item.trim().eq_ignore_ascii_case(allowed))
      }) =>
    {
      bail!("Expected a comma-separated list of HTTP, HTTPS, FTP, or FTPS.");
    }
    _ => {}
  }
  Ok(())
}
