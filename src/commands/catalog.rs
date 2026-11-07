//! Command identities, spellings, and presentation order.

// Keep names and enum variants in one declaration, as with OCPP wire enums.
macro_rules! command_catalog {
  ($($variant:ident => $name:literal [$($alias:literal),*]),+ $(,)?) => {
    /// A command name before its arguments are parsed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum CommandName {
      $($variant,)+
    }

    impl CommandName {
      /// Canonical commands in their established completion order.
      pub(crate) const ALL: &'static [Self] = &[$(Self::$variant,)+];

      /// Returns the canonical spelling.
      pub(crate) const fn as_str(self) -> &'static str {
        match self { $(Self::$variant => $name,)+ }
      }

      /// Returns alternate spellings, hidden from empty-prefix completion.
      pub(crate) const fn aliases(self) -> &'static [&'static str] {
        match self { $(Self::$variant => &[$($alias),*],)+ }
      }
    }
  };
}

command_catalog! {
  Status => "status" [],
  Config => "config" [],
  Connect => "connect" [],
  Disconnect => "disconnect" [],
  Boot => "boot" [],
  Authorize => "authorize" [],
  DataTransfer => "data-transfer" [],
  Start => "start" [],
  Stop => "stop" [],
  Meter => "meter" [],
  SendMeter => "send-meter" [],
  Soc => "soc" [],
  SendSoc => "send-soc" [],
  Measurand => "measurand" [],
  SendMeasurand => "send-measurand" [],
  Heartbeat => "heartbeat" [],
  ConnectorStatus => "connector-status" [],
  Clear => "clear" [],
  Standards => "standards" [],
  Help => "help" ["h", "?"],
  Exit => "exit" ["quit"],
}

impl CommandName {
  /// Resolves a full canonical name or alias, ignoring ASCII case.
  pub(crate) fn parse(word: &str) -> Option<Self> {
    Self::ALL.iter().copied().find(|command| {
      command.as_str().eq_ignore_ascii_case(word)
        || command
          .aliases()
          .iter()
          .any(|alias| alias.eq_ignore_ascii_case(word))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::CommandName;

  #[test]
  fn canonical_names_keep_the_existing_order() {
    let names = CommandName::ALL
      .iter()
      .map(|command| command.as_str())
      .collect::<Vec<_>>();
    assert_eq!(
      names,
      [
        "status",
        "config",
        "connect",
        "disconnect",
        "boot",
        "authorize",
        "data-transfer",
        "start",
        "stop",
        "meter",
        "send-meter",
        "soc",
        "send-soc",
        "measurand",
        "send-measurand",
        "heartbeat",
        "connector-status",
        "clear",
        "standards",
        "help",
        "exit",
      ]
    );
  }

  #[test]
  fn every_spelling_resolves_to_its_command() {
    for &command in CommandName::ALL {
      for name in std::iter::once(command.as_str())
        .chain(command.aliases().iter().copied())
      {
        assert_eq!(CommandName::parse(name), Some(command));
        assert_eq!(
          CommandName::parse(&name.to_ascii_uppercase()),
          Some(command)
        );
      }
    }
    assert_eq!(CommandName::parse("qu"), None);
    assert_eq!(CommandName::parse("unknown"), None);
  }
}
