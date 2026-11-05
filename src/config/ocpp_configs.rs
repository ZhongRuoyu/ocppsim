use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};

use super::{
  ConfigFile, ConfigOverrides, ProfileConfig, normalize_heartbeat_seconds,
};
use crate::ocpp::{ConfigurationKey, validate_heartbeat_interval};

pub(super) struct ConfiguredValues {
  pub values: BTreeMap<ConfigurationKey, String>,
  pub warnings: Vec<String>,
  pub connectors: Option<u16>,
  pub heartbeat_seconds: Option<u64>,
  pub security_profile: Option<u8>,
  pub basic_auth_password: Option<String>,
}

struct ValueSource {
  value: String,
  name: String,
  ocpp: bool,
}

pub(super) fn resolve(
  global: &ConfigFile,
  profile: &ProfileConfig,
  profile_name: Option<&str>,
  overrides: &ConfigOverrides,
) -> Result<ConfiguredValues> {
  let mut warnings = Vec::new();
  let mut values = scope_values(
    &global.configs,
    &ConfigOverrides {
      connectors: global.connectors,
      heartbeat_seconds: global.heartbeat_seconds,
      security_profile: global.security_profile,
      basic_auth_password: global.basic_auth_password.clone(),
      ..ConfigOverrides::default()
    },
    "global config",
    &mut warnings,
  )?;
  if let Some(name) = profile_name {
    values.extend(scope_values(
      &profile.configs,
      &ConfigOverrides {
        connectors: profile.connectors,
        heartbeat_seconds: profile.heartbeat_seconds,
        security_profile: profile.security_profile,
        basic_auth_password: profile.basic_auth_password.clone(),
        ..ConfigOverrides::default()
      },
      &format!("profile `{name}`"),
      &mut warnings,
    )?);
  }
  apply_native(&mut values, overrides, None, &mut warnings);

  let connectors =
    number::<u16>(&values, ConfigurationKey::NumberOfConnectors)?;
  if connectors == Some(0) {
    if overrides.connectors == Some(0) {
      bail!("--connectors must be positive.");
    }
    bail!("connectors / NumberOfConnectors must be positive.");
  }
  let heartbeat = number::<u64>(&values, ConfigurationKey::HeartbeatInterval)?;
  if values
    .get(&ConfigurationKey::HeartbeatInterval)
    .is_some_and(|v| v.ocpp)
  {
    validate_heartbeat_interval(heartbeat.expect("present interval"))
      .map_err(anyhow::Error::msg)?;
  }
  let heartbeat_seconds = normalize_heartbeat_seconds(heartbeat)?;
  let security = number::<u8>(&values, ConfigurationKey::SecurityProfile)?;
  if security.is_some_and(|value| value > 3)
    || (security == Some(0)
      && values
        .get(&ConfigurationKey::SecurityProfile)
        .is_some_and(|v| !v.ocpp))
  {
    bail!(
      "security-profile must be 1, 2, or 3; configs.SecurityProfile also accepts 0."
    );
  }
  let basic_auth_password = values
    .get(&ConfigurationKey::AuthorizationKey)
    .map(|value| value.value.clone());
  Ok(ConfiguredValues {
    values: values
      .into_iter()
      .filter(|(_, value)| value.ocpp)
      .map(|(key, value)| (key, value.value))
      .collect(),
    warnings,
    connectors,
    heartbeat_seconds,
    security_profile: security.filter(|value| *value != 0),
    basic_auth_password,
  })
}

fn scope_values(
  configs: &BTreeMap<String, toml::Value>,
  native: &ConfigOverrides,
  scope: &str,
  warnings: &mut Vec<String>,
) -> Result<BTreeMap<ConfigurationKey, ValueSource>> {
  let mut values: BTreeMap<ConfigurationKey, ValueSource> = BTreeMap::new();
  for (name, value) in configs {
    let key = ConfigurationKey::parse(name)
      .ok_or_else(|| anyhow!("Unknown OCPP config `{name}` in {scope}."))?;
    let value = match value {
      toml::Value::String(value) => value.clone(),
      toml::Value::Boolean(value) => value.to_string(),
      toml::Value::Integer(value) => value.to_string(),
      _ => bail!(
        "OCPP config `{name}` in {scope} must be a string, boolean, or integer."
      ),
    };
    if let Some(previous) = values.get(&key.canonical()) {
      if !equivalent(key.canonical(), &previous.value, &value) {
        bail!(
          "Conflicting OCPP aliases `{}` and `{name}` in {scope}.",
          previous.name
        );
      }
    } else {
      values.insert(
        key.canonical(),
        ValueSource {
          value,
          name: key.as_str().to_string(),
          ocpp: true,
        },
      );
    }
  }
  apply_native(&mut values, native, Some(scope), warnings);
  Ok(values)
}

fn apply_native(
  values: &mut BTreeMap<ConfigurationKey, ValueSource>,
  native: &ConfigOverrides,
  scope: Option<&str>,
  warnings: &mut Vec<String>,
) {
  for (key, name, value) in [
    (
      ConfigurationKey::HeartbeatInterval,
      "heartbeat-seconds",
      native.heartbeat_seconds.map(|v| v.to_string()),
    ),
    (
      ConfigurationKey::NumberOfConnectors,
      "connectors",
      native.connectors.map(|v| v.to_string()),
    ),
    (
      ConfigurationKey::SecurityProfile,
      "security-profile",
      native.security_profile.map(|v| v.to_string()),
    ),
    (
      ConfigurationKey::AuthorizationKey,
      "basic-auth-password",
      native.basic_auth_password.clone(),
    ),
  ] {
    let Some(value) = value else { continue };
    if let Some(scope) = scope
      && let Some(previous) = values.get(&key)
      && !equivalent(key, &previous.value, &value)
    {
      warnings.push(format!(
        "In {scope}, `configs.{}` conflicts with `{name}`; \
        `{name}` takes precedence within this scope.",
        previous.name,
      ));
    }
    values.insert(
      key,
      ValueSource {
        value,
        name: name.to_string(),
        ocpp: false,
      },
    );
  }
}

fn number<T: std::str::FromStr>(
  values: &BTreeMap<ConfigurationKey, ValueSource>,
  key: ConfigurationKey,
) -> Result<Option<T>> {
  values
    .get(&key)
    .map(|value| {
      value
        .value
        .parse()
        .map_err(|_| anyhow!("Invalid integer for `{}`.", value.name))
    })
    .transpose()
}

fn equivalent(key: ConfigurationKey, first: &str, second: &str) -> bool {
  if first == second {
    return true;
  }
  if matches!(
    key,
    ConfigurationKey::HeartbeatInterval
      | ConfigurationKey::SecurityProfile
      | ConfigurationKey::NumberOfConnectors
  ) {
    return first
      .parse::<u64>()
      .is_ok_and(|value| Some(value) == second.parse::<u64>().ok());
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn heartbeat_precedence_covers_all_five_layers() {
    for mask in 0..32 {
      let mut global = ConfigFile::default();
      let mut profile = ProfileConfig::default();
      let mut cli = ConfigOverrides::default();
      if mask & 1 != 0 {
        global
          .configs
          .insert("HeartbeatInterval".to_string(), 10.into());
      }
      global.heartbeat_seconds = (mask & 2 != 0).then_some(20);
      if mask & 4 != 0 {
        profile
          .configs
          .insert("HeartbeatInterval".to_string(), 30.into());
      }
      profile.heartbeat_seconds = (mask & 8 != 0).then_some(40);
      cli.heartbeat_seconds = (mask & 16 != 0).then_some(50);
      let resolved = resolve(&global, &profile, Some("example"), &cli).unwrap();
      let expected = (0..5)
        .rev()
        .find(|tier| mask & (1 << tier) != 0)
        .map(|tier| (tier + 1) * 10);
      assert_eq!(resolved.heartbeat_seconds, expected, "mask={mask}");
      let warning_count =
        usize::from(mask & 0b11 == 0b11) + usize::from(mask & 0b1100 == 0b1100);
      assert_eq!(resolved.warnings.len(), warning_count, "mask={mask}");
      let from_ocpp = matches!(expected, Some(10 | 30));
      assert_eq!(
        resolved
          .values
          .contains_key(&ConfigurationKey::HeartbeatInterval),
        from_ocpp
      );
    }
  }

  #[test]
  fn same_scope_conflicts_warn_without_disclosing_values() {
    let global: ConfigFile = toml::from_str(
      r#"
heartbeat-seconds = 20
basic-auth-password = "native-password-secret"
[configs]
HeartbeatInterval = 10
BasicAuthPassword = "ocpp-password-secret"
"#,
    )
    .unwrap();
    let profile: ProfileConfig = toml::from_str(
      r"
heartbeat-seconds = 40
[configs]
HeartbeatInterval = 30
",
    )
    .unwrap();
    let resolved = resolve(
      &global,
      &profile,
      Some("example"),
      &ConfigOverrides {
        heartbeat_seconds: Some(50),
        ..ConfigOverrides::default()
      },
    )
    .unwrap();
    assert_eq!(resolved.heartbeat_seconds, Some(50));
    assert_eq!(resolved.warnings.len(), 3);
    assert!(resolved.warnings[0].contains("global config"));
    assert!(resolved.warnings[2].contains("profile `example`"));
    assert!(
      resolved
        .warnings
        .iter()
        .all(|warning| !warning.contains("-password-secret"))
    );
  }

  #[test]
  fn equal_values_and_cross_scope_overrides_do_not_warn() {
    let global: ConfigFile = toml::from_str(
      r#"
heartbeat-seconds = 20
vendor = "different-vendor"
[configs]
HeartbeatInterval = "020"
CpoName = "global-cpo"
AuthorizeRemoteTxRequests = true
"#,
    )
    .unwrap();
    let profile: ProfileConfig = toml::from_str(
      r#"
[configs]
HeartbeatInterval = 30
OrganizationName = "profile-cpo"
AuthorizeRemoteTxRequests = false
"#,
    )
    .unwrap();
    let resolved = resolve(
      &global,
      &profile,
      Some("example"),
      &ConfigOverrides::default(),
    )
    .unwrap();
    assert_eq!(resolved.heartbeat_seconds, Some(30));
    assert_eq!(resolved.values[&ConfigurationKey::CpoName], "profile-cpo");
    assert_eq!(
      resolved.values[&ConfigurationKey::AuthorizeRemoteTxRequests],
      "false"
    );
    assert_eq!(resolved.warnings, Vec::<String>::new());
  }

  #[test]
  fn native_aliases_and_disabled_heartbeat_obey_precedence() {
    let global: ConfigFile = toml::from_str(
      r#"
security-profile = 2
basic-auth-password = "global-password"
connectors = 2
[configs]
HeartbeatInterval = 10
"#,
    )
    .unwrap();
    let profile: ProfileConfig = toml::from_str(
      r#"
[configs]
SecurityProfile = 0
BasicAuthPassword = "profile-password"
NumberOfConnectors = 3
"#,
    )
    .unwrap();
    let resolved = resolve(
      &global,
      &profile,
      Some("example"),
      &ConfigOverrides {
        heartbeat_seconds: Some(0),
        ..ConfigOverrides::default()
      },
    )
    .unwrap();
    assert_eq!(resolved.heartbeat_seconds, None);
    assert!(
      !resolved
        .values
        .contains_key(&ConfigurationKey::HeartbeatInterval)
    );
    assert_eq!(resolved.security_profile, None);
    assert_eq!(resolved.connectors, Some(3));
    assert_eq!(
      resolved.basic_auth_password.as_deref(),
      Some("profile-password")
    );
    let cli = ConfigOverrides {
      security_profile: Some(3),
      connectors: Some(4),
      basic_auth_password: Some("cli-password".to_string()),
      ..ConfigOverrides::default()
    };
    let resolved = resolve(&global, &profile, Some("example"), &cli).unwrap();
    assert_eq!(resolved.security_profile, Some(3));
    assert_eq!(resolved.connectors, Some(4));
    assert_eq!(
      resolved.basic_auth_password.as_deref(),
      Some("cli-password")
    );
    assert!(
      !resolved
        .values
        .contains_key(&ConfigurationKey::AuthorizationKey)
    );
  }

  #[test]
  fn invalid_types_unknown_keys_and_conflicting_aliases_fail() {
    for text in [
      "[configs]\nMissingKey = true",
      "[configs]\nCpoName = []",
      "[configs]\nCpoName = 1.5",
      "[configs]\nCpoName = { value = 'example' }",
      "[configs]\nCpoName = 'a'\nOrganizationName = 'b'",
    ] {
      let global = toml::from_str(text).unwrap();
      assert!(
        resolve(
          &global,
          &ProfileConfig::default(),
          None,
          &ConfigOverrides::default()
        )
        .is_err()
      );
    }
    let global =
      toml::from_str("[configs]\nHeartbeatInterval = 'invalid'").unwrap();
    assert_eq!(
      resolve(
        &global,
        &ProfileConfig::default(),
        None,
        &ConfigOverrides {
          heartbeat_seconds: Some(15),
          ..ConfigOverrides::default()
        }
      )
      .unwrap()
      .heartbeat_seconds,
      Some(15)
    );
  }
}
