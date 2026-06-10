use serde_json::{Value, json};

const OCPP_V1_6_AUTHORIZATION_KEY_MIN_LENGTH: usize = 32;
const OCPP_V1_6_AUTHORIZATION_KEY_MAX_LENGTH: usize = 40;
const OCPP_V2_X_BASIC_AUTH_PASSWORD_MIN_LENGTH: usize = 16;
const OCPP_V2_0_1_BASIC_AUTH_PASSWORD_MAX_LENGTH: usize = 40;
const OCPP_V2_1_BASIC_AUTH_PASSWORD_MAX_LENGTH: usize = 64;
const OCPP_MESSAGE_ID_MAX_CHARS: usize = 36;

/// Returns the human-readable Basic Auth password requirement for a protocol.
pub fn basic_auth_password_requirement(protocol: OcppVersion) -> &'static str {
  match protocol {
    OcppVersion::V1_6 => "32 to 40 ASCII hexadecimal characters",
    OcppVersion::V2_0_1 => "16 to 40 OCPP passwordString characters",
    OcppVersion::V2_1 => "16 to 64 UTF-8 passwordString characters",
  }
}

/// Returns whether a Basic Auth password fits the active OCPP protocol.
pub fn is_valid_basic_auth_password(
  protocol: OcppVersion,
  value: &str,
) -> bool {
  match protocol {
    OcppVersion::V1_6 => is_valid_v1_6_authorization_key(value),
    OcppVersion::V2_0_1 => is_valid_v2_0_1_basic_auth_password(value),
    OcppVersion::V2_1 => has_char_length(
      value,
      OCPP_V2_X_BASIC_AUTH_PASSWORD_MIN_LENGTH,
      OCPP_V2_1_BASIC_AUTH_PASSWORD_MAX_LENGTH,
    ),
  }
}

fn is_valid_v1_6_authorization_key(value: &str) -> bool {
  let bytes = value.as_bytes();
  (OCPP_V1_6_AUTHORIZATION_KEY_MIN_LENGTH
    ..=OCPP_V1_6_AUTHORIZATION_KEY_MAX_LENGTH)
    .contains(&bytes.len())
    && bytes.iter().all(u8::is_ascii_hexdigit)
}

fn is_valid_v2_0_1_basic_auth_password(value: &str) -> bool {
  has_char_length(
    value,
    OCPP_V2_X_BASIC_AUTH_PASSWORD_MIN_LENGTH,
    OCPP_V2_0_1_BASIC_AUTH_PASSWORD_MAX_LENGTH,
  ) && value.chars().all(is_v2_0_1_password_string_char)
}

fn has_char_length(value: &str, min: usize, max: usize) -> bool {
  (min..=max).contains(&value.chars().count())
}

fn is_v2_0_1_password_string_char(character: char) -> bool {
  character.is_ascii_alphanumeric()
    || matches!(
      character,
      '*' | '-' | '_' | '=' | ':' | '+' | '|' | '@' | '.'
    )
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppVersion {
  V1_6,
  V2_0_1,
  V2_1,
}

impl OcppVersion {
  /// Returns the standards label used in human-facing output.
  pub fn label(self) -> &'static str {
    match self {
      Self::V1_6 => "1.6",
      Self::V2_0_1 => "2.0.1",
      Self::V2_1 => "2.1",
    }
  }

  /// Returns the WebSocket subprotocol token for the selected version.
  pub fn subprotocol(self) -> &'static str {
    match self {
      Self::V1_6 => "ocpp1.6",
      Self::V2_0_1 => "ocpp2.0.1",
      Self::V2_1 => "ocpp2.1",
    }
  }
}

pub const OCPP_V1_6_SUPPORTED_ACTIONS: &[&str] = &[
  "Authorize",
  "BootNotification",
  "CancelReservation",
  "CertificateSigned",
  "ChangeAvailability",
  "ChangeConfiguration",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "DeleteCertificate",
  "DiagnosticsStatusNotification",
  "ExtendedTriggerMessage",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetConfiguration",
  "GetDiagnostics",
  "GetInstalledCertificateIds",
  "GetLocalListVersion",
  "GetLog",
  "Heartbeat",
  "InstallCertificate",
  "LogStatusNotification",
  "MeterValues",
  "RemoteStartTransaction",
  "RemoteStopTransaction",
  "ReserveNow",
  "Reset",
  "SecurityEventNotification",
  "SendLocalList",
  "SetChargingProfile",
  "SignCertificate",
  "SignedFirmwareStatusNotification",
  "SignedUpdateFirmware",
  "StartTransaction",
  "StatusNotification",
  "StopTransaction",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V2_X_COMMON_SUPPORTED_ACTIONS: &[&str] = &[
  "Authorize",
  "BootNotification",
  "CancelReservation",
  "CertificateSigned",
  "ChangeAvailability",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "DeleteCertificate",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetInstalledCertificateIds",
  "GetLocalListVersion",
  "GetLog",
  "GetVariables",
  "Heartbeat",
  "InstallCertificate",
  "LogStatusNotification",
  "MeterValues",
  "RequestStartTransaction",
  "RequestStopTransaction",
  "ReserveNow",
  "Reset",
  "SecurityEventNotification",
  "SendLocalList",
  "SetChargingProfile",
  "SetVariables",
  "SignCertificate",
  "StatusNotification",
  "TransactionEvent",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V2_0_1_UNSUPPORTED_ACTIONS: &[&str] = &[
  "ClearDisplayMessage",
  "ClearVariableMonitoring",
  "ClearedChargingLimit",
  "CostUpdated",
  "CustomerInformation",
  "Get15118EVCertificate",
  "GetBaseReport",
  "GetCertificateStatus",
  "GetChargingProfiles",
  "GetDisplayMessages",
  "GetMonitoringReport",
  "GetReport",
  "GetTransactionStatus",
  "NotifyChargingLimit",
  "NotifyCustomerInformation",
  "NotifyDisplayMessages",
  "NotifyEVChargingNeeds",
  "NotifyEVChargingSchedule",
  "NotifyEvent",
  "NotifyMonitoringReport",
  "NotifyReport",
  "PublishFirmware",
  "PublishFirmwareStatusNotification",
  "ReportChargingProfiles",
  "ReservationStatusUpdate",
  "SetDisplayMessage",
  "SetMonitoringBase",
  "SetMonitoringLevel",
  "SetNetworkProfile",
  "SetVariableMonitoring",
  "UnpublishFirmware",
];

pub const OCPP_V2_1_UNSUPPORTED_ACTIONS: &[&str] = &[
  "AFRRSignal",
  "AdjustPeriodicEventStream",
  "BatterySwap",
  "ChangeTransactionTariff",
  "ClearDERControl",
  "ClearTariffs",
  "ClosePeriodicEventStream",
  "GetCertificateChainStatus",
  "GetDERControl",
  "GetPeriodicEventStream",
  "GetTariffs",
  "NotifyAllowedEnergyTransfer",
  "NotifyDERAlarm",
  "NotifyDERStartStop",
  "NotifyPeriodicEventStream",
  "NotifyPriorityCharging",
  "NotifySettlement",
  "NotifyWebPaymentStarted",
  "OpenPeriodicEventStream",
  "PullDynamicScheduleUpdate",
  "ReportDERControl",
  "RequestBatterySwap",
  "SetDERControl",
  "SetDefaultTariff",
  "UpdateDynamicSchedule",
  "UsePriorityCharging",
  "VatNumberValidation",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppMessageTypeId {
  Call,
  CallResult,
  CallError,
  CallResultError,
  Send,
}

impl OcppMessageTypeId {
  /// Parses a numeric OCPP `MessageTypeId` into a typed enum.
  pub fn from_i64(value: i64) -> Option<Self> {
    match value {
      2 => Some(Self::Call),
      3 => Some(Self::CallResult),
      4 => Some(Self::CallError),
      5 => Some(Self::CallResultError),
      6 => Some(Self::Send),
      _ => None,
    }
  }

  /// Returns the integer encoding for this OCPP `MessageTypeId`.
  pub fn value(self) -> i64 {
    match self {
      Self::Call => 2,
      Self::CallResult => 3,
      Self::CallError => 4,
      Self::CallResultError => 5,
      Self::Send => 6,
    }
  }
}

macro_rules! wire_enum {
  (
    $(#[$enum_attr:meta])*
    $vis:vis enum $name:ident {
      $(
        $variant:ident => $wire:tt,
      )+
    }
  ) => {
    // Exhaustive wire enums intentionally include tokens that this simulator
    // does not currently emit.
    #[allow(dead_code)]
    $(#[$enum_attr])*
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    $vis enum $name {
      $(
        $variant,
      )+
    }

    #[allow(dead_code)]
    impl $name {
      pub const ALL: &'static [Self] = &[
        $(
          Self::$variant,
        )+
      ];

      pub const fn as_str(self) -> &'static str {
        match self {
          $(
            Self::$variant => wire_enum!(@unwrap $wire),
          )+
        }
      }

      pub fn parse(value: &str) -> Option<Self> {
        match value {
          $(
            wire_enum!(@unwrap $wire) => Some(Self::$variant),
          )+
          _ => None,
        }
      }
    }
  };

  (@unwrap { $lit:literal }) => { $lit };
  (@unwrap $lit:literal) => { $lit };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigurationKey {
  AllowOfflineTxForUnknownId,
  AdditionalRootCertificateCheck,
  AuthorizationCacheEnabled,
  AuthorizationKey,
  AuthorizeRemoteTxRequests,
  BasicAuthPassword,
  BlinkRepeat,
  CertificateSignedMaxChainSize,
  CertificateStoreMaxLength,
  ClockAlignedDataInterval,
  ConnectionTimeOut,
  ConnectorPhaseRotation,
  ConnectorPhaseRotationMaxLength,
  CpoName,
  CertSigningRepeatTimes,
  CertSigningWaitMinimum,
  GetConfigurationMaxKeys,
  HeartbeatInterval,
  LightIntensity,
  LocalAuthorizeOffline,
  LocalPreAuthorize,
  MaxEnergyOnInvalidId,
  MeterValuesAlignedData,
  MeterValuesAlignedDataMaxLength,
  MeterValuesSampledData,
  MeterValuesSampledDataMaxLength,
  MeterValueSampleInterval,
  MinimumStatusDuration,
  NumberOfConnectors,
  ResetRetries,
  StopTransactionOnEvSideDisconnect,
  StopTransactionOnInvalidId,
  StopTxnAlignedData,
  StopTxnAlignedDataMaxLength,
  StopTxnSampledData,
  StopTxnSampledDataMaxLength,
  SupportedFeatureProfiles,
  SupportedFeatureProfilesMaxLength,
  TransactionMessageAttempts,
  TransactionMessageRetryInterval,
  UnlockConnectorOnEvSideDisconnect,
  WebSocketPingInterval,
  LocalAuthListEnabled,
  LocalAuthListMaxLength,
  SendLocalListMaxLength,
  ReserveConnectorZeroSupported,
  SecurityProfile,
  ChargeProfileMaxStackLevel,
  ChargingScheduleAllowedChargingRateUnit,
  ChargingScheduleMaxPeriods,
  ConnectorSwitch3to1PhaseSupported,
  MaxChargingProfilesInstalled,
  MaxCertificateChainSize,
  OrganizationName,
  SupportedFileTransferProtocols,
  AllowSecurityProfileDowngrade,
}

impl ConfigurationKey {
  // Kept as a manifest of standard keys, including keys this simulator does
  // not currently expose.
  #[allow(dead_code)]
  pub const V1_6_STANDARD: &'static [Self] = &[
    Self::AllowOfflineTxForUnknownId,
    Self::AdditionalRootCertificateCheck,
    Self::AuthorizationCacheEnabled,
    Self::AuthorizationKey,
    Self::AuthorizeRemoteTxRequests,
    Self::BlinkRepeat,
    Self::ClockAlignedDataInterval,
    Self::ConnectionTimeOut,
    Self::ConnectorPhaseRotation,
    Self::ConnectorPhaseRotationMaxLength,
    Self::GetConfigurationMaxKeys,
    Self::HeartbeatInterval,
    Self::LightIntensity,
    Self::LocalAuthorizeOffline,
    Self::LocalPreAuthorize,
    Self::MaxEnergyOnInvalidId,
    Self::MeterValuesAlignedData,
    Self::MeterValuesAlignedDataMaxLength,
    Self::MeterValuesSampledData,
    Self::MeterValuesSampledDataMaxLength,
    Self::MeterValueSampleInterval,
    Self::MinimumStatusDuration,
    Self::NumberOfConnectors,
    Self::ResetRetries,
    Self::StopTransactionOnEvSideDisconnect,
    Self::StopTransactionOnInvalidId,
    Self::StopTxnAlignedData,
    Self::StopTxnAlignedDataMaxLength,
    Self::StopTxnSampledData,
    Self::StopTxnSampledDataMaxLength,
    Self::SupportedFeatureProfiles,
    Self::SupportedFeatureProfilesMaxLength,
    Self::TransactionMessageAttempts,
    Self::TransactionMessageRetryInterval,
    Self::UnlockConnectorOnEvSideDisconnect,
    Self::WebSocketPingInterval,
    Self::LocalAuthListEnabled,
    Self::LocalAuthListMaxLength,
    Self::SendLocalListMaxLength,
    Self::ReserveConnectorZeroSupported,
    Self::SecurityProfile,
    Self::ChargeProfileMaxStackLevel,
    Self::ChargingScheduleAllowedChargingRateUnit,
    Self::ChargingScheduleMaxPeriods,
    Self::ConnectorSwitch3to1PhaseSupported,
    Self::MaxChargingProfilesInstalled,
  ];

  pub const fn as_str(self) -> &'static str {
    match self {
      Self::AllowOfflineTxForUnknownId => "AllowOfflineTxForUnknownId",
      Self::AdditionalRootCertificateCheck => "AdditionalRootCertificateCheck",
      Self::AuthorizationCacheEnabled => "AuthorizationCacheEnabled",
      Self::AuthorizationKey => "AuthorizationKey",
      Self::AuthorizeRemoteTxRequests => "AuthorizeRemoteTxRequests",
      Self::BasicAuthPassword => "BasicAuthPassword",
      Self::BlinkRepeat => "BlinkRepeat",
      Self::CertificateSignedMaxChainSize => "CertificateSignedMaxChainSize",
      Self::CertificateStoreMaxLength => "CertificateStoreMaxLength",
      Self::ClockAlignedDataInterval => "ClockAlignedDataInterval",
      Self::ConnectionTimeOut => "ConnectionTimeOut",
      Self::ConnectorPhaseRotation => "ConnectorPhaseRotation",
      Self::ConnectorPhaseRotationMaxLength => {
        "ConnectorPhaseRotationMaxLength"
      }
      Self::CpoName => "CpoName",
      Self::CertSigningRepeatTimes => "CertSigningRepeatTimes",
      Self::CertSigningWaitMinimum => "CertSigningWaitMinimum",
      Self::GetConfigurationMaxKeys => "GetConfigurationMaxKeys",
      Self::HeartbeatInterval => "HeartbeatInterval",
      Self::LightIntensity => "LightIntensity",
      Self::LocalAuthorizeOffline => "LocalAuthorizeOffline",
      Self::LocalPreAuthorize => "LocalPreAuthorize",
      Self::MaxEnergyOnInvalidId => "MaxEnergyOnInvalidId",
      Self::MeterValuesAlignedData => "MeterValuesAlignedData",
      Self::MeterValuesAlignedDataMaxLength => {
        "MeterValuesAlignedDataMaxLength"
      }
      Self::MeterValuesSampledData => "MeterValuesSampledData",
      Self::MeterValuesSampledDataMaxLength => {
        "MeterValuesSampledDataMaxLength"
      }
      Self::MeterValueSampleInterval => "MeterValueSampleInterval",
      Self::MinimumStatusDuration => "MinimumStatusDuration",
      Self::NumberOfConnectors => "NumberOfConnectors",
      Self::ResetRetries => "ResetRetries",
      Self::StopTransactionOnEvSideDisconnect => {
        "StopTransactionOnEVSideDisconnect"
      }
      Self::StopTransactionOnInvalidId => "StopTransactionOnInvalidId",
      Self::StopTxnAlignedData => "StopTxnAlignedData",
      Self::StopTxnAlignedDataMaxLength => "StopTxnAlignedDataMaxLength",
      Self::StopTxnSampledData => "StopTxnSampledData",
      Self::StopTxnSampledDataMaxLength => "StopTxnSampledDataMaxLength",
      Self::SupportedFeatureProfiles => "SupportedFeatureProfiles",
      Self::SupportedFeatureProfilesMaxLength => {
        "SupportedFeatureProfilesMaxLength"
      }
      Self::TransactionMessageAttempts => "TransactionMessageAttempts",
      Self::TransactionMessageRetryInterval => {
        "TransactionMessageRetryInterval"
      }
      Self::UnlockConnectorOnEvSideDisconnect => {
        "UnlockConnectorOnEVSideDisconnect"
      }
      Self::WebSocketPingInterval => "WebSocketPingInterval",
      Self::LocalAuthListEnabled => "LocalAuthListEnabled",
      Self::LocalAuthListMaxLength => "LocalAuthListMaxLength",
      Self::SendLocalListMaxLength => "SendLocalListMaxLength",
      Self::ReserveConnectorZeroSupported => "ReserveConnectorZeroSupported",
      Self::SecurityProfile => "SecurityProfile",
      Self::ChargeProfileMaxStackLevel => "ChargeProfileMaxStackLevel",
      Self::ChargingScheduleAllowedChargingRateUnit => {
        "ChargingScheduleAllowedChargingRateUnit"
      }
      Self::ChargingScheduleMaxPeriods => "ChargingScheduleMaxPeriods",
      Self::ConnectorSwitch3to1PhaseSupported => {
        "ConnectorSwitch3to1PhaseSupported"
      }
      Self::MaxChargingProfilesInstalled => "MaxChargingProfilesInstalled",
      Self::MaxCertificateChainSize => "MaxCertificateChainSize",
      Self::OrganizationName => "OrganizationName",
      Self::SupportedFileTransferProtocols => "SupportedFileTransferProtocols",
      Self::AllowSecurityProfileDowngrade => "AllowSecurityProfileDowngrade",
    }
  }

  pub fn parse(value: &str) -> Option<Self> {
    match normalize_wire_identifier(value).as_str() {
      "allowofflinetxforunknownid" => Some(Self::AllowOfflineTxForUnknownId),
      "additionalrootcertificatecheck" => {
        Some(Self::AdditionalRootCertificateCheck)
      }
      "authorizationcacheenabled" => Some(Self::AuthorizationCacheEnabled),
      "authorizationkey" => Some(Self::AuthorizationKey),
      "authorizeremotetxrequests" | "authorizationremotetxrequests" => {
        Some(Self::AuthorizeRemoteTxRequests)
      }
      "basicauthpassword" => Some(Self::BasicAuthPassword),
      "blinkrepeat" => Some(Self::BlinkRepeat),
      "certificatesignedmaxchainsize" => {
        Some(Self::CertificateSignedMaxChainSize)
      }
      "certificatestoremaxlength" => Some(Self::CertificateStoreMaxLength),
      "clockaligneddatainterval" => Some(Self::ClockAlignedDataInterval),
      "connectiontimeout" => Some(Self::ConnectionTimeOut),
      "connectorphaserotation" => Some(Self::ConnectorPhaseRotation),
      "connectorphaserotationmaxlength" => {
        Some(Self::ConnectorPhaseRotationMaxLength)
      }
      "cponame" => Some(Self::CpoName),
      "certsigningrepeattimes" => Some(Self::CertSigningRepeatTimes),
      "certsigningwaitminimum" => Some(Self::CertSigningWaitMinimum),
      "getconfigurationmaxkeys" => Some(Self::GetConfigurationMaxKeys),
      "heartbeatinterval" => Some(Self::HeartbeatInterval),
      "lightintensity" => Some(Self::LightIntensity),
      "localauthorizeoffline" => Some(Self::LocalAuthorizeOffline),
      "localpreauthorize" => Some(Self::LocalPreAuthorize),
      "maxenergyoninvalidid" => Some(Self::MaxEnergyOnInvalidId),
      "metervaluesaligneddata" => Some(Self::MeterValuesAlignedData),
      "metervaluesaligneddatamaxlength" => {
        Some(Self::MeterValuesAlignedDataMaxLength)
      }
      "metervaluessampleddata" => Some(Self::MeterValuesSampledData),
      "metervaluessampleddatamaxlength" => {
        Some(Self::MeterValuesSampledDataMaxLength)
      }
      "metervaluesampleinterval" => Some(Self::MeterValueSampleInterval),
      "minimumstatusduration" => Some(Self::MinimumStatusDuration),
      "numberofconnectors" => Some(Self::NumberOfConnectors),
      "resetretries" => Some(Self::ResetRetries),
      "stoptransactiononevsidedisconnect" => {
        Some(Self::StopTransactionOnEvSideDisconnect)
      }
      "stoptransactiononinvalidid" => Some(Self::StopTransactionOnInvalidId),
      "stoptxnaligneddata" => Some(Self::StopTxnAlignedData),
      "stoptxnaligneddatamaxlength" => Some(Self::StopTxnAlignedDataMaxLength),
      "stoptxnsampleddata" => Some(Self::StopTxnSampledData),
      "stoptxnsampleddatamaxlength" => Some(Self::StopTxnSampledDataMaxLength),
      "supportedfeatureprofiles" => Some(Self::SupportedFeatureProfiles),
      "supportedfeatureprofilesmaxlength" => {
        Some(Self::SupportedFeatureProfilesMaxLength)
      }
      "transactionmessageattempts" => Some(Self::TransactionMessageAttempts),
      "transactionmessageretryinterval" => {
        Some(Self::TransactionMessageRetryInterval)
      }
      "unlockconnectoronevsidedisconnect" => {
        Some(Self::UnlockConnectorOnEvSideDisconnect)
      }
      "websocketpinginterval" => Some(Self::WebSocketPingInterval),
      "localauthlistenabled" => Some(Self::LocalAuthListEnabled),
      "localauthlistmaxlength" => Some(Self::LocalAuthListMaxLength),
      "sendlocallistmaxlength" => Some(Self::SendLocalListMaxLength),
      "reserveconnectorzerosupported" => {
        Some(Self::ReserveConnectorZeroSupported)
      }
      "securityprofile" => Some(Self::SecurityProfile),
      "chargeprofilemaxstacklevel" => Some(Self::ChargeProfileMaxStackLevel),
      "chargingscheduleallowedchargingrateunit" => {
        Some(Self::ChargingScheduleAllowedChargingRateUnit)
      }
      "chargingschedulemaxperiods" => Some(Self::ChargingScheduleMaxPeriods),
      "connectorswitch3to1phasesupported" => {
        Some(Self::ConnectorSwitch3to1PhaseSupported)
      }
      "maxchargingprofilesinstalled" => {
        Some(Self::MaxChargingProfilesInstalled)
      }
      "maxcertificatechainsize" => Some(Self::MaxCertificateChainSize),
      "organizationname" => Some(Self::OrganizationName),
      "supportedfiletransferprotocols" => {
        Some(Self::SupportedFileTransferProtocols)
      }
      "allowsecurityprofiledowngrade" => {
        Some(Self::AllowSecurityProfileDowngrade)
      }
      _ => None,
    }
  }
}

wire_enum! {
  /// Connector status values carried by OCPP `StatusNotification` messages.
  pub enum ConnectorStatus {
    Available => "Available",
    Preparing => "Preparing",
    Charging => "Charging",
    SuspendedEvse => "SuspendedEVSE",
    SuspendedEv => "SuspendedEV",
    Finishing => "Finishing",
    Reserved => "Reserved",
    Unavailable => "Unavailable",
    Faulted => "Faulted",
    Occupied => "Occupied",
  }
}

impl ConnectorStatus {
  /// OCPP 1.6 `StatusNotification.status` values.
  pub const V1_6: &'static [Self] = &[
    Self::Available,
    Self::Preparing,
    Self::Charging,
    Self::SuspendedEvse,
    Self::SuspendedEv,
    Self::Finishing,
    Self::Reserved,
    Self::Unavailable,
    Self::Faulted,
  ];

  /// OCPP 2.0.1 and 2.1 `StatusNotification.connectorStatus` values.
  pub const V2_X: &'static [Self] = &[
    Self::Available,
    Self::Occupied,
    Self::Reserved,
    Self::Unavailable,
    Self::Faulted,
  ];

  /// Returns this value when it is valid for OCPP 1.6 status payloads.
  pub fn as_v1_6(self) -> Option<&'static str> {
    match self {
      Self::Available
      | Self::Preparing
      | Self::Charging
      | Self::SuspendedEvse
      | Self::SuspendedEv
      | Self::Finishing
      | Self::Reserved
      | Self::Unavailable
      | Self::Faulted => Some(self.as_str()),
      Self::Occupied => None,
    }
  }

  /// Returns this value when it is valid for OCPP 2.x status payloads.
  pub fn as_v2_x(self) -> Option<&'static str> {
    match self {
      Self::Available
      | Self::Occupied
      | Self::Reserved
      | Self::Unavailable
      | Self::Faulted => Some(self.as_str()),
      Self::Preparing
      | Self::Charging
      | Self::SuspendedEvse
      | Self::SuspendedEv
      | Self::Finishing => None,
    }
  }
}

wire_enum! {
  pub enum BootReason {
    ApplicationReset => "ApplicationReset",
    FirmwareUpdate => "FirmwareUpdate",
    LocalReset => "LocalReset",
    PowerUp => "PowerUp",
    RemoteReset => "RemoteReset",
    ScheduledReset => "ScheduledReset",
    Triggered => "Triggered",
    Unknown => "Unknown",
    Watchdog => "Watchdog",
  }
}

wire_enum! {
  pub enum IdTokenType {
    Central => "Central",
    EMaid => "eMAID",
    Iso14443 => "ISO14443",
    Iso15693 => "ISO15693",
    KeyCode => "KeyCode",
    Local => "Local",
    MacAddress => "MacAddress",
    NoAuthorization => "NoAuthorization",
  }
}

wire_enum! {
  pub enum StopReason {
    DeAuthorized => "DeAuthorized",
    EmergencyStop => "EmergencyStop",
    EnergyLimitReached => "EnergyLimitReached",
    EvDisconnected => "EVDisconnected",
    GroundFault => "GroundFault",
    HardReset => "HardReset",
    ImmediateReset => "ImmediateReset",
    Local => "Local",
    LocalOutOfCredit => "LocalOutOfCredit",
    MasterPass => "MasterPass",
    Other => "Other",
    OvercurrentFault => "OvercurrentFault",
    PowerLoss => "PowerLoss",
    PowerQuality => "PowerQuality",
    Reboot => "Reboot",
    Remote => "Remote",
    ReqEnergyTransferRejected => "ReqEnergyTransferRejected",
    SocLimitReached => "SOCLimitReached",
    SoftReset => "SoftReset",
    StoppedByEv => "StoppedByEV",
    TimeLimitReached => "TimeLimitReached",
    Timeout => "Timeout",
    UnlockCommand => "UnlockCommand",
  }
}

impl StopReason {
  /// OCPP 1.6 `StopTransaction.reason` values.
  pub const V1_6: &'static [Self] = &[
    Self::EmergencyStop,
    Self::EvDisconnected,
    Self::HardReset,
    Self::Local,
    Self::Other,
    Self::PowerLoss,
    Self::Reboot,
    Self::Remote,
    Self::SoftReset,
    Self::UnlockCommand,
    Self::DeAuthorized,
  ];

  /// OCPP 2.0.1 `TransactionEvent.stoppedReason` values.
  pub const V2_0_1: &'static [Self] = &[
    Self::DeAuthorized,
    Self::EmergencyStop,
    Self::EnergyLimitReached,
    Self::EvDisconnected,
    Self::GroundFault,
    Self::ImmediateReset,
    Self::Local,
    Self::LocalOutOfCredit,
    Self::MasterPass,
    Self::Other,
    Self::OvercurrentFault,
    Self::PowerLoss,
    Self::PowerQuality,
    Self::Reboot,
    Self::Remote,
    Self::SocLimitReached,
    Self::StoppedByEv,
    Self::TimeLimitReached,
    Self::Timeout,
  ];

  /// OCPP 2.1 `TransactionEvent.stoppedReason` values.
  pub const V2_1: &'static [Self] = &[
    Self::DeAuthorized,
    Self::EmergencyStop,
    Self::EnergyLimitReached,
    Self::EvDisconnected,
    Self::GroundFault,
    Self::ImmediateReset,
    Self::MasterPass,
    Self::Local,
    Self::LocalOutOfCredit,
    Self::Other,
    Self::OvercurrentFault,
    Self::PowerLoss,
    Self::PowerQuality,
    Self::Reboot,
    Self::Remote,
    Self::SocLimitReached,
    Self::StoppedByEv,
    Self::TimeLimitReached,
    Self::Timeout,
    Self::ReqEnergyTransferRejected,
  ];

  pub fn as_v1_6(self) -> Option<&'static str> {
    match self {
      Self::DeAuthorized
      | Self::EmergencyStop
      | Self::EvDisconnected
      | Self::HardReset
      | Self::Local
      | Self::Other
      | Self::PowerLoss
      | Self::Reboot
      | Self::Remote
      | Self::SoftReset
      | Self::UnlockCommand => Some(self.as_str()),
      Self::EnergyLimitReached
      | Self::GroundFault
      | Self::ImmediateReset
      | Self::LocalOutOfCredit
      | Self::MasterPass
      | Self::OvercurrentFault
      | Self::PowerQuality
      | Self::ReqEnergyTransferRejected
      | Self::SocLimitReached
      | Self::StoppedByEv
      | Self::TimeLimitReached
      | Self::Timeout => None,
    }
  }

  pub fn as_v2_x(self, version: OcppVersion) -> Option<&'static str> {
    match self {
      Self::DeAuthorized
      | Self::EmergencyStop
      | Self::EnergyLimitReached
      | Self::EvDisconnected
      | Self::GroundFault
      | Self::ImmediateReset
      | Self::Local
      | Self::LocalOutOfCredit
      | Self::MasterPass
      | Self::Other
      | Self::OvercurrentFault
      | Self::PowerLoss
      | Self::PowerQuality
      | Self::Reboot
      | Self::Remote
      | Self::SocLimitReached
      | Self::StoppedByEv
      | Self::TimeLimitReached
      | Self::Timeout => Some(self.as_str()),
      Self::ReqEnergyTransferRejected if version == OcppVersion::V2_1 => {
        Some(self.as_str())
      }
      Self::ReqEnergyTransferRejected
      | Self::HardReset
      | Self::SoftReset
      | Self::UnlockCommand => None,
    }
  }

  pub fn parse_user_input(value: &str) -> Option<Self> {
    if let Some(reason) = Self::parse(value) {
      return Some(reason);
    }

    match normalize_wire_identifier(value).as_str() {
      "deauthorized" | "deauthorised" => Some(Self::DeAuthorized),
      "emergencystop" => Some(Self::EmergencyStop),
      "energylimitreached" => Some(Self::EnergyLimitReached),
      "evdisconnected" => Some(Self::EvDisconnected),
      "groundfault" => Some(Self::GroundFault),
      "hardreset" => Some(Self::HardReset),
      "immediatereset" => Some(Self::ImmediateReset),
      "local" => Some(Self::Local),
      "localoutofcredit" => Some(Self::LocalOutOfCredit),
      "masterpass" => Some(Self::MasterPass),
      "other" => Some(Self::Other),
      "overcurrentfault" => Some(Self::OvercurrentFault),
      "powerloss" => Some(Self::PowerLoss),
      "powerquality" => Some(Self::PowerQuality),
      "reboot" => Some(Self::Reboot),
      "remote" => Some(Self::Remote),
      "reqenergytransferrejected" => Some(Self::ReqEnergyTransferRejected),
      "soclimitreached" => Some(Self::SocLimitReached),
      "softreset" => Some(Self::SoftReset),
      "stoppedbyev" => Some(Self::StoppedByEv),
      "timelimitreached" => Some(Self::TimeLimitReached),
      "timeout" => Some(Self::Timeout),
      "unlockcommand" => Some(Self::UnlockCommand),
      _ => None,
    }
  }
}

wire_enum! {
  pub enum TransactionTriggerReason {
    AbnormalCondition => "AbnormalCondition",
    Authorized => "Authorized",
    CablePluggedIn => "CablePluggedIn",
    ChargingRateChanged => "ChargingRateChanged",
    ChargingStateChanged => "ChargingStateChanged",
    CostLimitReached => "CostLimitReached",
    Deauthorized => "Deauthorized",
    EnergyLimitReached => "EnergyLimitReached",
    EvCommunicationLost => "EVCommunicationLost",
    EvConnectTimeout => "EVConnectTimeout",
    EvDeparted => "EVDeparted",
    EvDetected => "EVDetected",
    LimitSet => "LimitSet",
    MeterValueClock => "MeterValueClock",
    MeterValuePeriodic => "MeterValuePeriodic",
    OperationModeChanged => "OperationModeChanged",
    RemoteStart => "RemoteStart",
    RemoteStop => "RemoteStop",
    ResetCommand => "ResetCommand",
    RunningCost => "RunningCost",
    SignedDataReceived => "SignedDataReceived",
    SocLimitReached => "SoCLimitReached",
    StopAuthorized => "StopAuthorized",
    TariffChanged => "TariffChanged",
    TariffNotAccepted => "TariffNotAccepted",
    TimeLimitReached => "TimeLimitReached",
    Trigger => "Trigger",
    TxResumed => "TxResumed",
    UnlockCommand => "UnlockCommand",
  }
}

wire_enum! {
  pub enum ReadingContext {
    InterruptionBegin => "Interruption.Begin",
    InterruptionEnd => "Interruption.End",
    SampleClock => "Sample.Clock",
    SamplePeriodic => "Sample.Periodic",
    TransactionBegin => "Transaction.Begin",
    TransactionEnd => "Transaction.End",
    Trigger => "Trigger",
    Other => "Other",
  }
}

wire_enum! {
  pub enum SampledValueFormat {
    Raw => "Raw",
    SignedData => "SignedData",
  }
}

wire_enum! {
  pub enum Measurand {
    CurrentExport => "Current.Export",
    CurrentExportOffered => "Current.Export.Offered",
    CurrentExportMinimum => "Current.Export.Minimum",
    CurrentImport => "Current.Import",
    CurrentImportOffered => "Current.Import.Offered",
    CurrentImportMinimum => "Current.Import.Minimum",
    CurrentOffered => "Current.Offered",
    DisplayPresentSoc => "Display.PresentSOC",
    DisplayMinimumSoc => "Display.MinimumSOC",
    DisplayTargetSoc => "Display.TargetSOC",
    DisplayMaximumSoc => "Display.MaximumSOC",
    DisplayRemainingTimeToMinimumSoc => "Display.RemainingTimeToMinimumSOC",
    DisplayRemainingTimeToTargetSoc => "Display.RemainingTimeToTargetSOC",
    DisplayRemainingTimeToMaximumSoc => "Display.RemainingTimeToMaximumSOC",
    DisplayChargingComplete => "Display.ChargingComplete",
    DisplayBatteryEnergyCapacity => "Display.BatteryEnergyCapacity",
    DisplayInletHot => "Display.InletHot",
    EnergyActiveExportInterval => "Energy.Active.Export.Interval",
    EnergyActiveExportRegister => "Energy.Active.Export.Register",
    EnergyActiveImportInterval => "Energy.Active.Import.Interval",
    EnergyActiveImportRegister => "Energy.Active.Import.Register",
    EnergyActiveImportCableLoss => "Energy.Active.Import.CableLoss",
    EnergyActiveImportLocalGenerationRegister => {
      "Energy.Active.Import.LocalGeneration.Register"
    },
    EnergyActiveNet => "Energy.Active.Net",
    EnergyActiveSetpointInterval => "Energy.Active.Setpoint.Interval",
    EnergyApparentExport => "Energy.Apparent.Export",
    EnergyApparentImport => "Energy.Apparent.Import",
    EnergyApparentNet => "Energy.Apparent.Net",
    EnergyReactiveExportInterval => "Energy.Reactive.Export.Interval",
    EnergyReactiveExportRegister => "Energy.Reactive.Export.Register",
    EnergyReactiveImportInterval => "Energy.Reactive.Import.Interval",
    EnergyReactiveImportRegister => "Energy.Reactive.Import.Register",
    EnergyReactiveNet => "Energy.Reactive.Net",
    EnergyRequestTarget => "EnergyRequest.Target",
    EnergyRequestMinimum => "EnergyRequest.Minimum",
    EnergyRequestMaximum => "EnergyRequest.Maximum",
    EnergyRequestMinimumV2X => "EnergyRequest.Minimum.V2X",
    EnergyRequestMaximumV2X => "EnergyRequest.Maximum.V2X",
    EnergyRequestBulk => "EnergyRequest.Bulk",
    Frequency => "Frequency",
    PowerActiveExport => "Power.Active.Export",
    PowerActiveImport => "Power.Active.Import",
    PowerActiveSetpoint => "Power.Active.Setpoint",
    PowerActiveResidual => "Power.Active.Residual",
    PowerExportMinimum => "Power.Export.Minimum",
    PowerExportOffered => "Power.Export.Offered",
    PowerFactor => "Power.Factor",
    PowerImportOffered => "Power.Import.Offered",
    PowerImportMinimum => "Power.Import.Minimum",
    PowerOffered => "Power.Offered",
    PowerReactiveExport => "Power.Reactive.Export",
    PowerReactiveImport => "Power.Reactive.Import",
    Soc => "SoC",
    Temperature => "Temperature",
    Voltage => "Voltage",
    VoltageMinimum => "Voltage.Minimum",
    VoltageMaximum => "Voltage.Maximum",
    Rpm => "RPM",
  }
}

wire_enum! {
  pub enum MeterValuePhase {
    L1 => "L1",
    L2 => "L2",
    L3 => "L3",
    N => "N",
    L1N => "L1-N",
    L2N => "L2-N",
    L3N => "L3-N",
    L1L2 => "L1-L2",
    L2L3 => "L2-L3",
    L3L1 => "L3-L1",
  }
}

wire_enum! {
  pub enum MeterValueLocation {
    Body => "Body",
    Cable => "Cable",
    Ev => "EV",
    Inlet => "Inlet",
    Outlet => "Outlet",
    Upstream => "Upstream",
  }
}

wire_enum! {
  pub enum MeterUnit {
    Wh => "Wh",
    KWh => "kWh",
    Varh => "varh",
    Kvarh => "kvarh",
    W => "W",
    KW => "kW",
    Va => "VA",
    Kva => "kVA",
    Var => "var",
    Kvar => "kvar",
    A => "A",
    V => "V",
    K => "K",
    Celcius => "Celcius",
    Celsius => "Celsius",
    Fahrenheit => "Fahrenheit",
    Percent => "Percent",
  }
}

wire_enum! {
  pub enum ChargingRateUnit {
    A => "A",
    W => "W",
  }
}

wire_enum! {
  pub enum StatusNotificationErrorCode {
    ConnectorLockFailure => "ConnectorLockFailure",
    EvCommunicationError => "EVCommunicationError",
    GroundFailure => "GroundFailure",
    HighTemperature => "HighTemperature",
    InternalError => "InternalError",
    LocalListConflict => "LocalListConflict",
    NoError => "NoError",
    OtherError => "OtherError",
    OverCurrentFailure => "OverCurrentFailure",
    PowerMeterFailure => "PowerMeterFailure",
    PowerSwitchFailure => "PowerSwitchFailure",
    ReaderFailure => "ReaderFailure",
    ResetFailure => "ResetFailure",
    UnderVoltage => "UnderVoltage",
    OverVoltage => "OverVoltage",
    WeakSignal => "WeakSignal",
  }
}

wire_enum! {
  pub enum VariableAttributeType {
    Actual => "Actual",
    Target => "Target",
    MinSet => "MinSet",
    MaxSet => "MaxSet",
  }
}

wire_enum! {
  pub enum OcppErrorCode {
    NotImplemented => "NotImplemented",
    NotSupported => "NotSupported",
    InternalError => "InternalError",
    ProtocolError => "ProtocolError",
    SecurityError => "SecurityError",
    FormationViolation => "FormationViolation",
    PropertyConstraintViolation => "PropertyConstraintViolation",
    OccurrenceConstraintViolation => "OccurrenceConstraintViolation",
    TypeConstraintViolation => "TypeConstraintViolation",
    GenericError => "GenericError",
    MessageTypeNotSupported => "MessageTypeNotSupported",
    RpcFrameworkError => "RpcFrameworkError",
  }
}

wire_enum! {
  pub enum OutgoingAction {
    Authorize => "Authorize",
    BootNotification => "BootNotification",
    DataTransfer => "DataTransfer",
    DiagnosticsStatusNotification => "DiagnosticsStatusNotification",
    FirmwareStatusNotification => "FirmwareStatusNotification",
    Heartbeat => "Heartbeat",
    LogStatusNotification => "LogStatusNotification",
    MeterValues => "MeterValues",
    SecurityEventNotification => "SecurityEventNotification",
    SignCertificate => "SignCertificate",
    SignedFirmwareStatusNotification => "SignedFirmwareStatusNotification",
    StartTransaction => "StartTransaction",
    StatusNotification => "StatusNotification",
    StopTransaction => "StopTransaction",
    TransactionEvent => "TransactionEvent",
  }
}

wire_enum! {
  pub enum CertificateType {
    CentralSystemRootCertificate => "CentralSystemRootCertificate",
    CSMSRootCertificate => "CSMSRootCertificate",
    ChargePointCertificate => "ChargePointCertificate",
    ChargingStationCertificate => "ChargingStationCertificate",
    ManufacturerRootCertificate => "ManufacturerRootCertificate",
    MORootCertificate => "MORootCertificate",
    V2GCertificate => "V2GCertificate",
    V2GCertificateChain => "V2GCertificateChain",
    V2GRootCertificate => "V2GRootCertificate",
  }
}

impl CertificateType {
  pub const fn is_central_system_root(self) -> bool {
    matches!(
      self,
      Self::CentralSystemRootCertificate | Self::CSMSRootCertificate
    )
  }
}

fn normalize_wire_identifier(value: &str) -> String {
  value
    .chars()
    .filter(char::is_ascii_alphanumeric)
    .map(|ch| ch.to_ascii_lowercase())
    .collect()
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V1_6 {
  CancelReservation,
  CertificateSigned,
  ChangeAvailability,
  ChangeConfiguration,
  ClearCache,
  ClearChargingProfile,
  DataTransfer,
  DeleteCertificate,
  ExtendedTriggerMessage,
  GetCompositeSchedule,
  GetConfiguration,
  GetDiagnostics,
  GetInstalledCertificateIds,
  GetLocalListVersion,
  GetLog,
  RemoteStartTransaction,
  RemoteStopTransaction,
  ReserveNow,
  Reset,
  InstallCertificate,
  SendLocalList,
  SetChargingProfile,
  SignedUpdateFirmware,
  TriggerMessage,
  UnlockConnector,
  UpdateFirmware,
}

impl IncomingAction_V1_6 {
  /// Parses an incoming OCPP 1.6 action name.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "CancelReservation" => Some(Self::CancelReservation),
      "CertificateSigned" => Some(Self::CertificateSigned),
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "ChangeConfiguration" => Some(Self::ChangeConfiguration),
      "ClearCache" => Some(Self::ClearCache),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "DataTransfer" => Some(Self::DataTransfer),
      "DeleteCertificate" => Some(Self::DeleteCertificate),
      "ExtendedTriggerMessage" => Some(Self::ExtendedTriggerMessage),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "GetConfiguration" => Some(Self::GetConfiguration),
      "GetDiagnostics" => Some(Self::GetDiagnostics),
      "GetInstalledCertificateIds" => Some(Self::GetInstalledCertificateIds),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "GetLog" => Some(Self::GetLog),
      "InstallCertificate" => Some(Self::InstallCertificate),
      "RemoteStartTransaction" => Some(Self::RemoteStartTransaction),
      "RemoteStopTransaction" => Some(Self::RemoteStopTransaction),
      "ReserveNow" => Some(Self::ReserveNow),
      "Reset" => Some(Self::Reset),
      "SendLocalList" => Some(Self::SendLocalList),
      "SetChargingProfile" => Some(Self::SetChargingProfile),
      "SignedUpdateFirmware" => Some(Self::SignedUpdateFirmware),
      "TriggerMessage" => Some(Self::TriggerMessage),
      "UnlockConnector" => Some(Self::UnlockConnector),
      "UpdateFirmware" => Some(Self::UpdateFirmware),
      _ => None,
    }
  }

  /// Returns true when an OCPP 1.6 action belongs to a known extension that
  /// is intentionally out of scope for the base-schema implementation.
  pub fn is_known_unsupported(_value: &str) -> bool {
    false
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V2_X {
  CancelReservation,
  CertificateSigned,
  ChangeAvailability,
  ClearCache,
  ClearChargingProfile,
  DataTransfer,
  DeleteCertificate,
  GetCompositeSchedule,
  GetInstalledCertificateIds,
  GetLocalListVersion,
  GetLog,
  GetVariables,
  InstallCertificate,
  RequestStartTransaction,
  RequestStopTransaction,
  ReserveNow,
  Reset,
  SendLocalList,
  SetChargingProfile,
  SetVariables,
  TriggerMessage,
  UnlockConnector,
  UpdateFirmware,
}

impl IncomingAction_V2_X {
  /// Parses an incoming OCPP 2.x action name.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "CancelReservation" => Some(Self::CancelReservation),
      "CertificateSigned" => Some(Self::CertificateSigned),
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "ClearCache" => Some(Self::ClearCache),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "DataTransfer" => Some(Self::DataTransfer),
      "DeleteCertificate" => Some(Self::DeleteCertificate),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "GetInstalledCertificateIds" => Some(Self::GetInstalledCertificateIds),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "GetLog" => Some(Self::GetLog),
      "GetVariables" => Some(Self::GetVariables),
      "InstallCertificate" => Some(Self::InstallCertificate),
      "RequestStartTransaction" => Some(Self::RequestStartTransaction),
      "RequestStopTransaction" => Some(Self::RequestStopTransaction),
      "ReserveNow" => Some(Self::ReserveNow),
      "Reset" => Some(Self::Reset),
      "SendLocalList" => Some(Self::SendLocalList),
      "SetChargingProfile" => Some(Self::SetChargingProfile),
      "SetVariables" => Some(Self::SetVariables),
      "TriggerMessage" => Some(Self::TriggerMessage),
      "UnlockConnector" => Some(Self::UnlockConnector),
      "UpdateFirmware" => Some(Self::UpdateFirmware),
      _ => None,
    }
  }

  /// Returns true when an OCPP 2.x action is known but intentionally out of
  /// scope for the simulator's current 1.6-derived feature subset.
  pub fn is_known_unsupported(value: &str, version: OcppVersion) -> bool {
    match version {
      OcppVersion::V1_6 => false,
      OcppVersion::V2_0_1 => OCPP_V2_0_1_UNSUPPORTED_ACTIONS.contains(&value),
      OcppVersion::V2_1 => {
        OCPP_V2_0_1_UNSUPPORTED_ACTIONS.contains(&value)
          || OCPP_V2_1_UNSUPPORTED_ACTIONS.contains(&value)
      }
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMessage_V1_6 {
  BootNotification,
  DiagnosticsStatusNotification,
  FirmwareStatusNotification,
  Heartbeat,
  MeterValues,
  StatusNotification,
}

impl TriggerMessage_V1_6 {
  /// Parses standard OCPP 1.6 `TriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "DiagnosticsStatusNotification" => {
        Some(Self::DiagnosticsStatusNotification)
      }
      "FirmwareStatusNotification" => Some(Self::FirmwareStatusNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "MeterValues" => Some(Self::MeterValues),
      "StatusNotification" => Some(Self::StatusNotification),
      _ => None,
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendedTriggerMessage_V1_6 {
  BootNotification,
  FirmwareStatusNotification,
  Heartbeat,
  LogStatusNotification,
  MeterValues,
  SignChargePointCertificate,
  StatusNotification,
}

impl ExtendedTriggerMessage_V1_6 {
  /// Parses OCPP 1.6 security `ExtendedTriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "FirmwareStatusNotification" => Some(Self::FirmwareStatusNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "LogStatusNotification" => Some(Self::LogStatusNotification),
      "MeterValues" => Some(Self::MeterValues),
      "SignChargePointCertificate" => Some(Self::SignChargePointCertificate),
      "StatusNotification" => Some(Self::StatusNotification),
      _ => None,
    }
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMessage_V2_X {
  BootNotification,
  FirmwareStatusNotification,
  Heartbeat,
  LogStatusNotification,
  MeterValues,
  PublishFirmwareStatusNotification,
  SignCombinedCertificate,
  SignChargingStationCertificate,
  SignV2G20Certificate,
  SignV2GCertificate,
  StatusNotification,
  TransactionEvent,
  CustomTrigger,
}

impl TriggerMessage_V2_X {
  /// Parses OCPP 2.x `TriggerMessage` request variants.
  pub fn parse(value: &str, version: OcppVersion) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "FirmwareStatusNotification" => Some(Self::FirmwareStatusNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "LogStatusNotification" => Some(Self::LogStatusNotification),
      "MeterValues" => Some(Self::MeterValues),
      "PublishFirmwareStatusNotification" => {
        Some(Self::PublishFirmwareStatusNotification)
      }
      "SignCombinedCertificate" => Some(Self::SignCombinedCertificate),
      "SignChargingStationCertificate" => {
        Some(Self::SignChargingStationCertificate)
      }
      "SignV2G20Certificate" if version == OcppVersion::V2_1 => {
        Some(Self::SignV2G20Certificate)
      }
      "SignV2GCertificate" => Some(Self::SignV2GCertificate),
      "StatusNotification" => Some(Self::StatusNotification),
      "TransactionEvent" => Some(Self::TransactionEvent),
      "CustomTrigger" if version == OcppVersion::V2_1 => {
        Some(Self::CustomTrigger)
      }
      _ => None,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
  Accepted,
  AcceptedCanceled,
  Available,
  BadMessage,
  Blocked,
  Canceled,
  CertChainError,
  CertificateExpired,
  CertificateRevoked,
  Charging,
  ChecksumVerified,
  ConcurrentTx,
  ConditionNotSupported,
  ContractCancelled,
  Crl,
  Downloaded,
  DownloadFailed,
  Downloading,
  DownloadOngoing,
  DownloadPaused,
  DownloadScheduled,
  Duplicate,
  DuplicateTariffId,
  EmptyResultSet,
  Expired,
  Failed,
  Faulted,
  Finishing,
  Good,
  Idle,
  Inoperative,
  InstallationFailed,
  Installed,
  Installing,
  InstallRebooting,
  InstallScheduled,
  InstallVerificationFailed,
  Invalid,
  InvalidCertificate,
  InvalidChecksum,
  InvalidSignature,
  LanguageNotSupported,
  NoCertificateAvailable,
  NoChargingProfile,
  NoCredit,
  NoCurrencyChange,
  NoFirmware,
  NoProfile,
  NoProfiles,
  NotAllowedTypeEVSE,
  NoTariff,
  NotAtThisLocation,
  NotAtThisTime,
  NotFound,
  NotImplemented,
  NoTransaction,
  NotReady,
  NotSupported,
  NotSupportedAttributeType,
  NotSupportedMessageFormat,
  NotSupportedOperation,
  NotSupportedPriority,
  NotSupportedState,
  Occupied,
  Ocsp,
  OngoingAuthorizedTransaction,
  Operative,
  Pending,
  PermissionDenied,
  Preconditioning,
  Preparing,
  Processing,
  Published,
  PublishFailed,
  Ready,
  RebootRequired,
  Rejected,
  Removed,
  Reserved,
  Revoked,
  RevokedCertificate,
  Scheduled,
  Settled,
  SignatureError,
  SignatureVerified,
  SuspendedEV,
  SuspendedEVSE,
  TooManyElements,
  TxNotFound,
  Unavailable,
  Unknown,
  UnknownComponent,
  UnknownConnector,
  UnknownMessageId,
  UnknownTransaction,
  UnknownVariable,
  UnknownVendorId,
  Unlocked,
  UnlockFailed,
  Unpublished,
  Unsupported,
  UnsupportedMonitorType,
  Uploaded,
  UploadFailed,
  UploadFailure,
  Uploading,
  VersionMismatch,
  WriteOnly,
}

macro_rules! response_status_map {
  ($callback:ident, $value:expr) => {
    $callback! {
      $value;
      Accepted => "Accepted",
      AcceptedCanceled => "AcceptedCanceled",
      Available => "Available",
      BadMessage => "BadMessage",
      Blocked => "Blocked",
      Canceled => "Canceled",
      CertChainError => "CertChainError",
      CertificateExpired => "CertificateExpired",
      CertificateRevoked => "CertificateRevoked",
      Charging => "Charging",
      ChecksumVerified => "ChecksumVerified",
      ConcurrentTx => "ConcurrentTx",
      ConditionNotSupported => "ConditionNotSupported",
      ContractCancelled => "ContractCancelled",
      Crl => "CRL",
      Downloaded => "Downloaded",
      DownloadFailed => "DownloadFailed",
      Downloading => "Downloading",
      DownloadOngoing => "DownloadOngoing",
      DownloadPaused => "DownloadPaused",
      DownloadScheduled => "DownloadScheduled",
      Duplicate => "Duplicate",
      DuplicateTariffId => "DuplicateTariffId",
      EmptyResultSet => "EmptyResultSet",
      Expired => "Expired",
      Failed => "Failed",
      Faulted => "Faulted",
      Finishing => "Finishing",
      Good => "Good",
      Idle => "Idle",
      Inoperative => "Inoperative",
      InstallationFailed => "InstallationFailed",
      Installed => "Installed",
      Installing => "Installing",
      InstallRebooting => "InstallRebooting",
      InstallScheduled => "InstallScheduled",
      InstallVerificationFailed => "InstallVerificationFailed",
      Invalid => "Invalid",
      InvalidCertificate => "InvalidCertificate",
      InvalidChecksum => "InvalidChecksum",
      InvalidSignature => "InvalidSignature",
      LanguageNotSupported => "LanguageNotSupported",
      NoCertificateAvailable => "NoCertificateAvailable",
      NoChargingProfile => "NoChargingProfile",
      NoCredit => "NoCredit",
      NoCurrencyChange => "NoCurrencyChange",
      NoFirmware => "NoFirmware",
      NoProfile => "NoProfile",
      NoProfiles => "NoProfiles",
      NotAllowedTypeEVSE => "NotAllowedTypeEVSE",
      NoTariff => "NoTariff",
      NotAtThisLocation => "NotAtThisLocation",
      NotAtThisTime => "NotAtThisTime",
      NotFound => "NotFound",
      NotImplemented => "NotImplemented",
      NoTransaction => "NoTransaction",
      NotReady => "NotReady",
      NotSupported => "NotSupported",
      NotSupportedAttributeType => "NotSupportedAttributeType",
      NotSupportedMessageFormat => "NotSupportedMessageFormat",
      NotSupportedOperation => "NotSupportedOperation",
      NotSupportedPriority => "NotSupportedPriority",
      NotSupportedState => "NotSupportedState",
      Occupied => "Occupied",
      Ocsp => "OCSP",
      OngoingAuthorizedTransaction => "OngoingAuthorizedTransaction",
      Operative => "Operative",
      Pending => "Pending",
      PermissionDenied => "PermissionDenied",
      Preconditioning => "Preconditioning",
      Preparing => "Preparing",
      Processing => "Processing",
      Published => "Published",
      PublishFailed => "PublishFailed",
      Ready => "Ready",
      RebootRequired => "RebootRequired",
      Rejected => "Rejected",
      Removed => "Removed",
      Reserved => "Reserved",
      Revoked => "Revoked",
      RevokedCertificate => "RevokedCertificate",
      Scheduled => "Scheduled",
      Settled => "Settled",
      SignatureError => "SignatureError",
      SignatureVerified => "SignatureVerified",
      SuspendedEV => "SuspendedEV",
      SuspendedEVSE => "SuspendedEVSE",
      TooManyElements => "TooManyElements",
      TxNotFound => "TxNotFound",
      Unavailable => "Unavailable",
      Unknown => "Unknown",
      UnknownComponent => "UnknownComponent",
      UnknownConnector => "UnknownConnector",
      UnknownMessageId => "UnknownMessageId",
      UnknownTransaction => "UnknownTransaction",
      UnknownVariable => "UnknownVariable",
      UnknownVendorId => "UnknownVendorId",
      Unlocked => "Unlocked",
      UnlockFailed => "UnlockFailed",
      Unpublished => "Unpublished",
      Unsupported => "Unsupported",
      UnsupportedMonitorType => "UnsupportedMonitorType",
      Uploaded => "Uploaded",
      UploadFailed => "UploadFailed",
      UploadFailure => "UploadFailure",
      Uploading => "Uploading",
      VersionMismatch => "VersionMismatch",
      WriteOnly => "WriteOnly",
    }
  };
}

macro_rules! response_status_as_str_impl {
  ($value:expr; $($variant:ident => $token:literal,)*) => {
    match $value {
      $(ResponseStatus::$variant => $token,)*
    }
  };
}

macro_rules! response_status_parse_impl {
  ($value:expr; $($variant:ident => $token:literal,)*) => {
    match $value {
      $($token => Some(ResponseStatus::$variant),)*
      _ => None,
    }
  };
}

impl ResponseStatus {
  /// Returns the wire-format status token used in OCPP payloads.
  pub fn as_str(self) -> &'static str {
    response_status_map!(response_status_as_str_impl, self)
  }

  /// Parses a wire-format status token into the internal enum.
  pub fn parse(value: &str) -> Option<Self> {
    response_status_map!(response_status_parse_impl, value)
  }
}

#[derive(Debug, Clone)]
pub enum OcppFrame {
  Call {
    message_id: String,
    action: String,
    payload: Value,
  },
  CallResult {
    message_id: String,
    payload: Value,
  },
  CallError {
    message_id: String,
    code: String,
    description: String,
    details: Value,
  },
  CallResultError {
    message_id: String,
    code: String,
    description: String,
    details: Value,
  },
  Send {
    message_id: String,
    action: String,
    payload: Value,
  },
  Unsupported {
    message_type: i64,
    message_id: Option<String>,
  },
}

/// Parses raw JSON text into a typed OCPP frame.
///
/// Expected input is an OCPP-J array frame. The function validates shape and
/// basic field types, and normalizes `null` payload objects to `{}`.
pub fn parse_frame(text: &str) -> Result<OcppFrame, String> {
  let value: Value =
    serde_json::from_str(text).map_err(|err| format!("Invalid JSON: {err}"))?;
  let array = value
    .as_array()
    .ok_or_else(|| "OCPP frame must be a JSON array.".to_string())?;
  if array.len() < 2 {
    return Err("OCPP frame is too short.".to_string());
  }

  let message_type = array[0]
    .as_i64()
    .ok_or_else(|| "MessageTypeId must be an integer.".to_string())?;
  let message_id = array[1].as_str().map(validate_message_id).transpose()?;

  match OcppMessageTypeId::from_i64(message_type) {
    Some(OcppMessageTypeId::Call | OcppMessageTypeId::Send) => {
      if array.len() != 4 {
        return Err(format!(
          "CALL/SEND frame must have 4 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let action = array[2]
        .as_str()
        .ok_or_else(|| "Action must be a string.".to_string())?
        .to_string();
      let payload = parse_payload_object(&array[3])?;
      if OcppMessageTypeId::from_i64(message_type)
        == Some(OcppMessageTypeId::Call)
      {
        Ok(OcppFrame::Call {
          message_id: parsed_id,
          action,
          payload,
        })
      } else {
        Ok(OcppFrame::Send {
          message_id: parsed_id,
          action,
          payload,
        })
      }
    }
    Some(OcppMessageTypeId::CallResult) => {
      if array.len() != 3 {
        return Err(format!(
          "CALLRESULT frame must have 3 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let payload = parse_payload_object(&array[2])?;
      Ok(OcppFrame::CallResult {
        message_id: parsed_id,
        payload,
      })
    }
    Some(OcppMessageTypeId::CallError | OcppMessageTypeId::CallResultError) => {
      if array.len() != 5 {
        return Err(format!(
          "CALLERROR frame must have 5 items, got {}.",
          array.len()
        ));
      }
      let parsed_id =
        message_id.ok_or_else(|| "MessageId must be a string.".to_string())?;
      let code = array[2]
        .as_str()
        .ok_or_else(|| "ErrorCode must be a string.".to_string())?
        .to_string();
      let description = array[3]
        .as_str()
        .ok_or_else(|| "ErrorDescription must be a string.".to_string())?
        .to_string();
      let details = parse_payload_object(&array[4])?;
      if OcppMessageTypeId::from_i64(message_type)
        == Some(OcppMessageTypeId::CallError)
      {
        Ok(OcppFrame::CallError {
          message_id: parsed_id,
          code,
          description,
          details,
        })
      } else {
        Ok(OcppFrame::CallResultError {
          message_id: parsed_id,
          code,
          description,
          details,
        })
      }
    }
    None => Ok(OcppFrame::Unsupported {
      message_type,
      message_id,
    }),
  }
}

/// Builds a CALL frame string (`[2, messageId, action, payload]`).
pub fn build_call(message_id: &str, action: &str, payload: &Value) -> String {
  json!([OcppMessageTypeId::Call.value(), message_id, action, payload])
    .to_string()
}

/// Builds a CALLRESULT frame string (`[3, messageId, payload]`).
pub fn build_call_result(message_id: &str, payload: &Value) -> String {
  json!([OcppMessageTypeId::CallResult.value(), message_id, payload])
    .to_string()
}

/// Builds a CALLERROR frame string (`[4, messageId, code, desc, details]`).
pub fn build_call_error(
  message_id: &str,
  code: &str,
  description: &str,
  details: &Value,
) -> String {
  json!([
    OcppMessageTypeId::CallError.value(),
    message_id,
    code,
    description,
    details
  ])
  .to_string()
}

/// Validates and normalizes the payload object field of an OCPP frame.
///
/// `null` is accepted and converted to an empty object for convenience.
fn parse_payload_object(value: &Value) -> Result<Value, String> {
  if value.is_null() {
    return Ok(json!({}));
  }
  if !value.is_object() {
    return Err("Payload must be a JSON object.".to_string());
  }
  Ok(value.clone())
}

fn validate_message_id(value: &str) -> Result<String, String> {
  if value.chars().count() > OCPP_MESSAGE_ID_MAX_CHARS {
    return Err(format!(
      "MessageId must be at most {OCPP_MESSAGE_ID_MAX_CHARS} characters."
    ));
  }
  Ok(value.to_string())
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use serde_json::{Value, json};

  use crate::embedded_schemas::EmbeddedSchemaType;

  use super::{
    BootReason, ChargingRateUnit, ConnectorStatus, IdTokenType, Measurand,
    MeterUnit, MeterValueLocation, MeterValuePhase,
    OCPP_V1_6_SUPPORTED_ACTIONS, OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
    OCPP_V2_1_UNSUPPORTED_ACTIONS, OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
    OcppFrame, OcppVersion, ReadingContext, ResponseStatus, SampledValueFormat,
    StatusNotificationErrorCode, StopReason, TransactionTriggerReason,
    VariableAttributeType, build_call, parse_frame,
  };

  #[test]
  /// Verifies OCPP 1.6 base schema actions are all in the supported manifest.
  fn v1_6_manifest_covers_base_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V1_6);
    let supported = set_from_slice(OCPP_V1_6_SUPPORTED_ACTIONS);

    assert_eq!(schema_actions, supported);
  }

  #[test]
  /// Verifies OCPP 2.0.1 actions are explicitly supported or unsupported.
  fn v2_0_1_manifest_covers_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V2_0_1);
    let manifest = set_from_slices(&[
      OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
      OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
    ]);

    assert_eq!(schema_actions, manifest);
  }

  #[test]
  /// Verifies OCPP 2.1 actions are explicitly supported or unsupported.
  fn v2_1_manifest_covers_schema_actions() {
    let schema_actions = request_schema_actions(OcppVersion::V2_1);
    let manifest = set_from_slices(&[
      OCPP_V2_X_COMMON_SUPPORTED_ACTIONS,
      OCPP_V2_0_1_UNSUPPORTED_ACTIONS,
      OCPP_V2_1_UNSUPPORTED_ACTIONS,
    ]);

    assert_eq!(schema_actions, manifest);
  }

  #[test]
  /// Verifies OCPP 1.6 security whitepaper actions are in the supported list.
  fn v1_6_security_extension_actions_are_supported() {
    for action in [
      "CertificateSigned",
      "DeleteCertificate",
      "ExtendedTriggerMessage",
      "GetInstalledCertificateIds",
      "GetLog",
      "InstallCertificate",
      "LogStatusNotification",
      "SecurityEventNotification",
      "SignCertificate",
      "SignedFirmwareStatusNotification",
      "SignedUpdateFirmware",
    ] {
      assert!(OCPP_V1_6_SUPPORTED_ACTIONS.contains(&action));
    }
  }

  #[test]
  /// Verifies all schema status tokens are recognized by `ResponseStatus`.
  fn response_status_covers_schema_status_enums() {
    let mut statuses = BTreeSet::new();
    for protocol in [OcppVersion::V1_6, OcppVersion::V2_0_1, OcppVersion::V2_1]
    {
      for schema_type in
        [EmbeddedSchemaType::Request, EmbeddedSchemaType::Response]
      {
        for schema in crate::embedded_schemas::schemas(protocol, schema_type) {
          let value: Value = serde_json::from_str(schema.text)
            .unwrap_or_else(|error| panic!("{}: {error}", schema.file_name));
          collect_status_enum_tokens(&value, &mut Vec::new(), &mut statuses);
        }
      }
    }

    for status in statuses {
      let parsed = ResponseStatus::parse(&status)
        .unwrap_or_else(|| panic!("missing ResponseStatus for {status}"));
      assert_eq!(parsed.as_str(), status);
    }
  }

  #[test]
  /// Verifies all schema stop reason tokens are represented by `StopReason`.
  fn stop_reason_covers_schema_reason_enums() {
    let mut reasons = BTreeSet::new();
    reasons.extend(enum_tokens_at_path(
      "schemas/1.6/StopTransaction.json",
      &["properties", "reason", "enum"],
    ));
    reasons.extend(definition_enum_tokens(
      "schemas/2.0.1/TransactionEventRequest.json",
      "ReasonEnumType",
    ));
    reasons.extend(definition_enum_tokens(
      "schemas/2.1/TransactionEventRequest.json",
      "ReasonEnumType",
    ));

    assert_wire_enum_tokens(reasons, StopReason::parse, StopReason::as_str);
  }

  #[test]
  /// Verifies all schema connector status tokens are typed.
  fn connector_status_covers_status_notification_schema_enums() {
    let mut statuses = BTreeSet::new();
    statuses.extend(enum_tokens_at_path(
      "schemas/1.6/StatusNotification.json",
      &["properties", "status", "enum"],
    ));
    statuses.extend(definition_enum_tokens(
      "schemas/2.0.1/StatusNotificationRequest.json",
      "ConnectorStatusEnumType",
    ));
    statuses.extend(definition_enum_tokens(
      "schemas/2.1/StatusNotificationRequest.json",
      "ConnectorStatusEnumType",
    ));

    assert_wire_enum_tokens(
      statuses,
      ConnectorStatus::parse,
      ConnectorStatus::as_str,
    );
  }

  #[test]
  /// Verifies all schema transaction trigger tokens are typed.
  fn transaction_trigger_reason_covers_schema_enums() {
    let mut triggers = BTreeSet::new();
    triggers.extend(definition_enum_tokens(
      "schemas/2.0.1/TransactionEventRequest.json",
      "TriggerReasonEnumType",
    ));
    triggers.extend(definition_enum_tokens(
      "schemas/2.1/TransactionEventRequest.json",
      "TriggerReasonEnumType",
    ));

    assert_wire_enum_tokens(
      triggers,
      TransactionTriggerReason::parse,
      TransactionTriggerReason::as_str,
    );
  }

  #[test]
  /// Verifies meter value enum tokens from all embedded schemas are typed.
  fn meter_value_enums_cover_schema_tokens() {
    let mut contexts = BTreeSet::new();
    contexts.extend(enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("context"),
    ));
    contexts.extend(definition_enum_tokens(
      "schemas/2.0.1/MeterValuesRequest.json",
      "ReadingContextEnumType",
    ));
    contexts.extend(definition_enum_tokens(
      "schemas/2.1/MeterValuesRequest.json",
      "ReadingContextEnumType",
    ));
    assert_wire_enum_tokens(
      contexts,
      ReadingContext::parse,
      ReadingContext::as_str,
    );

    let mut measurands = BTreeSet::new();
    measurands.extend(enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("measurand"),
    ));
    measurands.extend(definition_enum_tokens(
      "schemas/2.0.1/MeterValuesRequest.json",
      "MeasurandEnumType",
    ));
    measurands.extend(definition_enum_tokens(
      "schemas/2.1/MeterValuesRequest.json",
      "MeasurandEnumType",
    ));
    assert_wire_enum_tokens(measurands, Measurand::parse, Measurand::as_str);

    let mut phases = BTreeSet::new();
    phases.extend(enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("phase"),
    ));
    phases.extend(definition_enum_tokens(
      "schemas/2.0.1/MeterValuesRequest.json",
      "PhaseEnumType",
    ));
    phases.extend(definition_enum_tokens(
      "schemas/2.1/MeterValuesRequest.json",
      "PhaseEnumType",
    ));
    assert_wire_enum_tokens(
      phases,
      MeterValuePhase::parse,
      MeterValuePhase::as_str,
    );

    let mut locations = BTreeSet::new();
    locations.extend(enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("location"),
    ));
    locations.extend(definition_enum_tokens(
      "schemas/2.0.1/MeterValuesRequest.json",
      "LocationEnumType",
    ));
    locations.extend(definition_enum_tokens(
      "schemas/2.1/MeterValuesRequest.json",
      "LocationEnumType",
    ));
    assert_wire_enum_tokens(
      locations,
      MeterValueLocation::parse,
      MeterValueLocation::as_str,
    );

    let formats = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("format"),
    );
    assert_wire_enum_tokens(
      formats,
      SampledValueFormat::parse,
      SampledValueFormat::as_str,
    );

    let units = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("unit"),
    );
    assert_wire_enum_tokens(units, MeterUnit::parse, MeterUnit::as_str);
  }

  #[test]
  /// Verifies common OCPP scalar tokens are typed from schemas.
  fn scalar_enums_cover_schema_tokens() {
    let mut boot_reasons = BTreeSet::new();
    boot_reasons.extend(definition_enum_tokens(
      "schemas/2.0.1/BootNotificationRequest.json",
      "BootReasonEnumType",
    ));
    boot_reasons.extend(definition_enum_tokens(
      "schemas/2.1/BootNotificationRequest.json",
      "BootReasonEnumType",
    ));
    assert_wire_enum_tokens(
      boot_reasons,
      BootReason::parse,
      BootReason::as_str,
    );

    let mut id_token_types = BTreeSet::new();
    id_token_types.extend(definition_enum_tokens(
      "schemas/2.0.1/AuthorizeRequest.json",
      "IdTokenEnumType",
    ));
    assert_wire_enum_tokens(
      id_token_types,
      IdTokenType::parse,
      IdTokenType::as_str,
    );

    let mut attribute_types = BTreeSet::new();
    attribute_types.extend(definition_enum_tokens(
      "schemas/2.0.1/GetVariablesRequest.json",
      "AttributeEnumType",
    ));
    attribute_types.extend(definition_enum_tokens(
      "schemas/2.1/GetVariablesRequest.json",
      "AttributeEnumType",
    ));
    assert_wire_enum_tokens(
      attribute_types,
      VariableAttributeType::parse,
      VariableAttributeType::as_str,
    );

    let mut charging_rate_units = BTreeSet::new();
    charging_rate_units.extend(enum_tokens_at_path(
      "schemas/1.6/GetCompositeScheduleResponse.json",
      &[
        "properties",
        "chargingSchedule",
        "properties",
        "chargingRateUnit",
        "enum",
      ],
    ));
    charging_rate_units.extend(definition_enum_tokens(
      "schemas/2.0.1/GetCompositeScheduleResponse.json",
      "ChargingRateUnitEnumType",
    ));
    charging_rate_units.extend(definition_enum_tokens(
      "schemas/2.1/GetCompositeScheduleResponse.json",
      "ChargingRateUnitEnumType",
    ));
    assert_wire_enum_tokens(
      charging_rate_units,
      ChargingRateUnit::parse,
      ChargingRateUnit::as_str,
    );
  }

  #[test]
  /// Verifies OCPP 1.6 `StatusNotification` error codes are typed.
  fn v1_6_status_error_codes_cover_schema_tokens() {
    let error_codes = enum_tokens_at_path(
      "schemas/1.6/StatusNotification.json",
      &["properties", "errorCode", "enum"],
    );
    assert_wire_enum_tokens(
      error_codes,
      StatusNotificationErrorCode::parse,
      StatusNotificationErrorCode::as_str,
    );
  }

  #[test]
  /// Verifies OCPP-J CALL builders round-trip through frame parsing.
  fn call_builder_round_trips_through_parser() {
    let text = build_call("m1", "Heartbeat", &json!({}));
    let frame = parse_frame(&text).expect("parse built call");

    let OcppFrame::Call {
      message_id,
      action,
      payload,
    } = frame
    else {
      panic!("expected CALL frame");
    };
    assert_eq!(message_id, "m1");
    assert_eq!(action, "Heartbeat");
    assert_eq!(payload, json!({}));
  }

  #[test]
  /// Verifies null payloads are normalized to empty objects.
  fn parser_normalizes_null_payload_objects() {
    let frame = parse_frame(r#"[3,"m1",null]"#).expect("parse call result");

    let OcppFrame::CallResult { payload, .. } = frame else {
      panic!("expected CALLRESULT frame");
    };
    assert_eq!(payload, json!({}));
  }

  #[test]
  /// Verifies malformed OCPP-J frames fail before dispatch.
  fn parser_rejects_invalid_frame_shapes() {
    assert!(parse_frame(r#"{"messageTypeId":2}"#).is_err());
    assert!(parse_frame(r#"[2,"m1","Heartbeat"]"#).is_err());
    assert!(parse_frame(r#"[3,"m1",[]]"#).is_err());
    assert!(parse_frame(r#"[4,"m1","Error",{},{}]"#).is_err());
  }

  #[test]
  /// Verifies OCPP-J wrapper limits are enforced before dispatch.
  fn parser_rejects_oversized_message_id() {
    let oversized = "m".repeat(37);
    let frame = format!(r#"[2,"{oversized}","Heartbeat",{{}}]"#);

    let error = parse_frame(&frame).expect_err("oversized id should fail");

    assert!(error.contains("MessageId must be at most 36 characters"));
  }

  fn request_schema_actions(protocol: OcppVersion) -> BTreeSet<String> {
    crate::embedded_schemas::incoming_request_schemas(protocol)
      .iter()
      .map(|schema| schema.action.to_string())
      .collect()
  }

  fn set_from_slice(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
  }

  fn set_from_slices(slices: &[&[&str]]) -> BTreeSet<String> {
    slices
      .iter()
      .flat_map(|items| items.iter().copied())
      .map(ToOwned::to_owned)
      .collect()
  }

  fn schema_value(relative_schema: &str) -> Value {
    let schema_text = crate::embedded_schemas::schema_text(relative_schema)
      .unwrap_or_else(|| panic!("missing embedded schema {relative_schema}"));
    serde_json::from_str(schema_text)
      .unwrap_or_else(|error| panic!("{relative_schema}: {error}"))
  }

  fn definition_enum_tokens(
    relative_schema: &str,
    definition_name: &str,
  ) -> BTreeSet<String> {
    enum_tokens_at_path(
      relative_schema,
      &["definitions", definition_name, "enum"],
    )
  }

  fn enum_tokens_at_path(
    relative_schema: &str,
    path: &[&str],
  ) -> BTreeSet<String> {
    let schema = schema_value(relative_schema);
    let mut value = &schema;
    for part in path {
      value = value.get(*part).unwrap_or_else(|| {
        panic!("missing schema path {relative_schema}: {}", path.join("."))
      });
    }
    value
      .as_array()
      .unwrap_or_else(|| {
        panic!("schema path is not an enum array: {}", path.join("."))
      })
      .iter()
      .map(|item| {
        item
          .as_str()
          .unwrap_or_else(|| panic!("enum token is not a string: {item}"))
          .to_string()
      })
      .collect()
  }

  fn meter_sampled_value_path(field: &str) -> Vec<&str> {
    vec![
      "properties",
      "meterValue",
      "items",
      "properties",
      "sampledValue",
      "items",
      "properties",
      field,
      "enum",
    ]
  }

  fn assert_wire_enum_tokens<T>(
    tokens: BTreeSet<String>,
    parse: fn(&str) -> Option<T>,
    as_str: fn(T) -> &'static str,
  ) where
    T: Copy,
  {
    for token in tokens {
      let parsed = parse(&token)
        .unwrap_or_else(|| panic!("missing protocol enum variant for {token}"));
      assert_eq!(as_str(parsed), token);
    }
  }

  fn collect_status_enum_tokens(
    value: &Value,
    path: &mut Vec<String>,
    statuses: &mut BTreeSet<String>,
  ) {
    match value {
      Value::Object(map) => {
        if path
          .iter()
          .any(|part| part.to_lowercase().contains("status"))
          && let Some(Value::Array(items)) = map.get("enum")
        {
          statuses
            .extend(items.iter().filter_map(Value::as_str).map(str::to_string));
        }
        for (key, child) in map {
          path.push(key.clone());
          collect_status_enum_tokens(child, path, statuses);
          path.pop();
        }
      }
      Value::Array(items) => {
        for child in items {
          collect_status_enum_tokens(child, path, statuses);
        }
      }
      _ => {}
    }
  }
}
