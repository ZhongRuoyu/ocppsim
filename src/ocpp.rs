use serde_json::{Value, json};

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
  "ChangeAvailability",
  "ChangeConfiguration",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "DiagnosticsStatusNotification",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetConfiguration",
  "GetDiagnostics",
  "GetLocalListVersion",
  "Heartbeat",
  "MeterValues",
  "RemoteStartTransaction",
  "RemoteStopTransaction",
  "ReserveNow",
  "Reset",
  "SendLocalList",
  "SetChargingProfile",
  "StartTransaction",
  "StatusNotification",
  "StopTransaction",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS: &[&str] = &[
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
];

pub const OCPP_V2_X_COMMON_SUPPORTED_ACTIONS: &[&str] = &[
  "Authorize",
  "BootNotification",
  "CancelReservation",
  "ChangeAvailability",
  "ClearCache",
  "ClearChargingProfile",
  "DataTransfer",
  "FirmwareStatusNotification",
  "GetCompositeSchedule",
  "GetLocalListVersion",
  "GetLog",
  "GetVariables",
  "Heartbeat",
  "LogStatusNotification",
  "MeterValues",
  "RequestStartTransaction",
  "RequestStopTransaction",
  "ReserveNow",
  "Reset",
  "SendLocalList",
  "SetChargingProfile",
  "SetVariables",
  "StatusNotification",
  "TransactionEvent",
  "TriggerMessage",
  "UnlockConnector",
  "UpdateFirmware",
];

pub const OCPP_V2_0_1_UNSUPPORTED_ACTIONS: &[&str] = &[
  "CertificateSigned",
  "ClearDisplayMessage",
  "ClearVariableMonitoring",
  "ClearedChargingLimit",
  "CostUpdated",
  "CustomerInformation",
  "DeleteCertificate",
  "Get15118EVCertificate",
  "GetBaseReport",
  "GetCertificateStatus",
  "GetChargingProfiles",
  "GetDisplayMessages",
  "GetInstalledCertificateIds",
  "GetMonitoringReport",
  "GetReport",
  "GetTransactionStatus",
  "InstallCertificate",
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
  "SecurityEventNotification",
  "SetDisplayMessage",
  "SetMonitoringBase",
  "SetMonitoringLevel",
  "SetNetworkProfile",
  "SetVariableMonitoring",
  "SignCertificate",
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
  AuthorizationCacheEnabled,
  AuthorizeRemoteTxRequests,
  BlinkRepeat,
  ClockAlignedDataInterval,
  ConnectionTimeOut,
  ConnectorPhaseRotation,
  ConnectorPhaseRotationMaxLength,
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
  ChargeProfileMaxStackLevel,
  ChargingScheduleAllowedChargingRateUnit,
  ChargingScheduleMaxPeriods,
  ConnectorSwitch3to1PhaseSupported,
  MaxChargingProfilesInstalled,
}

impl ConfigurationKey {
  // Kept as a manifest of standard keys, including keys this simulator does
  // not currently expose.
  #[allow(dead_code)]
  pub const V1_6_STANDARD: &'static [Self] = &[
    Self::AllowOfflineTxForUnknownId,
    Self::AuthorizationCacheEnabled,
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
    Self::ChargeProfileMaxStackLevel,
    Self::ChargingScheduleAllowedChargingRateUnit,
    Self::ChargingScheduleMaxPeriods,
    Self::ConnectorSwitch3to1PhaseSupported,
    Self::MaxChargingProfilesInstalled,
  ];

  pub const fn as_str(self) -> &'static str {
    match self {
      Self::AllowOfflineTxForUnknownId => "AllowOfflineTxForUnknownId",
      Self::AuthorizationCacheEnabled => "AuthorizationCacheEnabled",
      Self::AuthorizeRemoteTxRequests => "AuthorizeRemoteTxRequests",
      Self::BlinkRepeat => "BlinkRepeat",
      Self::ClockAlignedDataInterval => "ClockAlignedDataInterval",
      Self::ConnectionTimeOut => "ConnectionTimeOut",
      Self::ConnectorPhaseRotation => "ConnectorPhaseRotation",
      Self::ConnectorPhaseRotationMaxLength => {
        "ConnectorPhaseRotationMaxLength"
      }
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
      Self::ChargeProfileMaxStackLevel => "ChargeProfileMaxStackLevel",
      Self::ChargingScheduleAllowedChargingRateUnit => {
        "ChargingScheduleAllowedChargingRateUnit"
      }
      Self::ChargingScheduleMaxPeriods => "ChargingScheduleMaxPeriods",
      Self::ConnectorSwitch3to1PhaseSupported => {
        "ConnectorSwitch3to1PhaseSupported"
      }
      Self::MaxChargingProfilesInstalled => "MaxChargingProfilesInstalled",
    }
  }

  pub fn parse(value: &str) -> Option<Self> {
    match normalize_wire_identifier(value).as_str() {
      "allowofflinetxforunknownid" => Some(Self::AllowOfflineTxForUnknownId),
      "authorizationcacheenabled" => Some(Self::AuthorizationCacheEnabled),
      "authorizeremotetxrequests" | "authorizationremotetxrequests" => {
        Some(Self::AuthorizeRemoteTxRequests)
      }
      "blinkrepeat" => Some(Self::BlinkRepeat),
      "clockaligneddatainterval" => Some(Self::ClockAlignedDataInterval),
      "connectiontimeout" => Some(Self::ConnectionTimeOut),
      "connectorphaserotation" => Some(Self::ConnectorPhaseRotation),
      "connectorphaserotationmaxlength" => {
        Some(Self::ConnectorPhaseRotationMaxLength)
      }
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
      _ => None,
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
    StartTransaction => "StartTransaction",
    StatusNotification => "StatusNotification",
    StopTransaction => "StopTransaction",
    TransactionEvent => "TransactionEvent",
  }
}

fn normalize_wire_identifier(value: &str) -> String {
  value
    .chars()
    .filter(|ch| ch.is_ascii_alphanumeric())
    .map(|ch| ch.to_ascii_lowercase())
    .collect()
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V1_6 {
  CancelReservation,
  ChangeAvailability,
  ChangeConfiguration,
  ClearCache,
  ClearChargingProfile,
  DataTransfer,
  GetCompositeSchedule,
  GetConfiguration,
  GetDiagnostics,
  GetLocalListVersion,
  RemoteStartTransaction,
  RemoteStopTransaction,
  ReserveNow,
  Reset,
  SendLocalList,
  SetChargingProfile,
  TriggerMessage,
  UnlockConnector,
  UpdateFirmware,
}

impl IncomingAction_V1_6 {
  /// Parses an incoming OCPP 1.6 action name.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "CancelReservation" => Some(Self::CancelReservation),
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "ChangeConfiguration" => Some(Self::ChangeConfiguration),
      "ClearCache" => Some(Self::ClearCache),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "DataTransfer" => Some(Self::DataTransfer),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "GetConfiguration" => Some(Self::GetConfiguration),
      "GetDiagnostics" => Some(Self::GetDiagnostics),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "RemoteStartTransaction" => Some(Self::RemoteStartTransaction),
      "RemoteStopTransaction" => Some(Self::RemoteStopTransaction),
      "ReserveNow" => Some(Self::ReserveNow),
      "Reset" => Some(Self::Reset),
      "SendLocalList" => Some(Self::SendLocalList),
      "SetChargingProfile" => Some(Self::SetChargingProfile),
      "TriggerMessage" => Some(Self::TriggerMessage),
      "UnlockConnector" => Some(Self::UnlockConnector),
      "UpdateFirmware" => Some(Self::UpdateFirmware),
      _ => None,
    }
  }

  /// Returns true when an OCPP 1.6 action belongs to a known extension that
  /// is intentionally out of scope for the base-schema implementation.
  pub fn is_known_unsupported(value: &str) -> bool {
    OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS.contains(&value)
  }
}

// Preserve explicit protocol version formatting for type names.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingAction_V2_X {
  CancelReservation,
  ChangeAvailability,
  ClearCache,
  ClearChargingProfile,
  DataTransfer,
  GetCompositeSchedule,
  GetLocalListVersion,
  GetLog,
  GetVariables,
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
      "ChangeAvailability" => Some(Self::ChangeAvailability),
      "ClearCache" => Some(Self::ClearCache),
      "ClearChargingProfile" => Some(Self::ClearChargingProfile),
      "DataTransfer" => Some(Self::DataTransfer),
      "GetCompositeSchedule" => Some(Self::GetCompositeSchedule),
      "GetLocalListVersion" => Some(Self::GetLocalListVersion),
      "GetLog" => Some(Self::GetLog),
      "GetVariables" => Some(Self::GetVariables),
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
  Heartbeat,
  MeterValues,
  StatusNotification,
}

impl TriggerMessage_V1_6 {
  /// Parses OCPP 1.6 `TriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
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
pub enum TriggerMessage_V2_X {
  BootNotification,
  Heartbeat,
  MeterValues,
  StatusNotification,
}

impl TriggerMessage_V2_X {
  /// Parses OCPP 2.x `TriggerMessage` request variants.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "BootNotification" => Some(Self::BootNotification),
      "Heartbeat" => Some(Self::Heartbeat),
      "MeterValues" => Some(Self::MeterValues),
      "StatusNotification" => Some(Self::StatusNotification),
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
}

impl ResponseStatus {
  /// Returns the wire-format status token used in OCPP payloads.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Accepted => "Accepted",
      Self::AcceptedCanceled => "AcceptedCanceled",
      Self::Available => "Available",
      Self::BadMessage => "BadMessage",
      Self::Blocked => "Blocked",
      Self::Canceled => "Canceled",
      Self::CertChainError => "CertChainError",
      Self::CertificateExpired => "CertificateExpired",
      Self::CertificateRevoked => "CertificateRevoked",
      Self::Charging => "Charging",
      Self::ChecksumVerified => "ChecksumVerified",
      Self::ConcurrentTx => "ConcurrentTx",
      Self::ConditionNotSupported => "ConditionNotSupported",
      Self::ContractCancelled => "ContractCancelled",
      Self::Crl => "CRL",
      Self::Downloaded => "Downloaded",
      Self::DownloadFailed => "DownloadFailed",
      Self::Downloading => "Downloading",
      Self::DownloadOngoing => "DownloadOngoing",
      Self::DownloadPaused => "DownloadPaused",
      Self::DownloadScheduled => "DownloadScheduled",
      Self::Duplicate => "Duplicate",
      Self::DuplicateTariffId => "DuplicateTariffId",
      Self::EmptyResultSet => "EmptyResultSet",
      Self::Expired => "Expired",
      Self::Failed => "Failed",
      Self::Faulted => "Faulted",
      Self::Finishing => "Finishing",
      Self::Good => "Good",
      Self::Idle => "Idle",
      Self::Inoperative => "Inoperative",
      Self::InstallationFailed => "InstallationFailed",
      Self::Installed => "Installed",
      Self::Installing => "Installing",
      Self::InstallRebooting => "InstallRebooting",
      Self::InstallScheduled => "InstallScheduled",
      Self::InstallVerificationFailed => "InstallVerificationFailed",
      Self::Invalid => "Invalid",
      Self::InvalidCertificate => "InvalidCertificate",
      Self::InvalidChecksum => "InvalidChecksum",
      Self::InvalidSignature => "InvalidSignature",
      Self::LanguageNotSupported => "LanguageNotSupported",
      Self::NoCertificateAvailable => "NoCertificateAvailable",
      Self::NoChargingProfile => "NoChargingProfile",
      Self::NoCredit => "NoCredit",
      Self::NoCurrencyChange => "NoCurrencyChange",
      Self::NoFirmware => "NoFirmware",
      Self::NoProfile => "NoProfile",
      Self::NoProfiles => "NoProfiles",
      Self::NotAllowedTypeEVSE => "NotAllowedTypeEVSE",
      Self::NoTariff => "NoTariff",
      Self::NotAtThisLocation => "NotAtThisLocation",
      Self::NotAtThisTime => "NotAtThisTime",
      Self::NotFound => "NotFound",
      Self::NotImplemented => "NotImplemented",
      Self::NoTransaction => "NoTransaction",
      Self::NotReady => "NotReady",
      Self::NotSupported => "NotSupported",
      Self::NotSupportedAttributeType => "NotSupportedAttributeType",
      Self::NotSupportedMessageFormat => "NotSupportedMessageFormat",
      Self::NotSupportedOperation => "NotSupportedOperation",
      Self::NotSupportedPriority => "NotSupportedPriority",
      Self::NotSupportedState => "NotSupportedState",
      Self::Occupied => "Occupied",
      Self::Ocsp => "OCSP",
      Self::OngoingAuthorizedTransaction => "OngoingAuthorizedTransaction",
      Self::Operative => "Operative",
      Self::Pending => "Pending",
      Self::PermissionDenied => "PermissionDenied",
      Self::Preconditioning => "Preconditioning",
      Self::Preparing => "Preparing",
      Self::Processing => "Processing",
      Self::Published => "Published",
      Self::PublishFailed => "PublishFailed",
      Self::Ready => "Ready",
      Self::RebootRequired => "RebootRequired",
      Self::Rejected => "Rejected",
      Self::Removed => "Removed",
      Self::Reserved => "Reserved",
      Self::Revoked => "Revoked",
      Self::RevokedCertificate => "RevokedCertificate",
      Self::Scheduled => "Scheduled",
      Self::Settled => "Settled",
      Self::SignatureError => "SignatureError",
      Self::SignatureVerified => "SignatureVerified",
      Self::SuspendedEV => "SuspendedEV",
      Self::SuspendedEVSE => "SuspendedEVSE",
      Self::TooManyElements => "TooManyElements",
      Self::TxNotFound => "TxNotFound",
      Self::Unavailable => "Unavailable",
      Self::Unknown => "Unknown",
      Self::UnknownComponent => "UnknownComponent",
      Self::UnknownConnector => "UnknownConnector",
      Self::UnknownMessageId => "UnknownMessageId",
      Self::UnknownTransaction => "UnknownTransaction",
      Self::UnknownVariable => "UnknownVariable",
      Self::UnknownVendorId => "UnknownVendorId",
      Self::Unlocked => "Unlocked",
      Self::UnlockFailed => "UnlockFailed",
      Self::Unpublished => "Unpublished",
      Self::Unsupported => "Unsupported",
      Self::UnsupportedMonitorType => "UnsupportedMonitorType",
      Self::Uploaded => "Uploaded",
      Self::UploadFailed => "UploadFailed",
      Self::UploadFailure => "UploadFailure",
      Self::Uploading => "Uploading",
      Self::VersionMismatch => "VersionMismatch",
    }
  }

  /// Parses a wire-format status token into the internal enum.
  pub fn parse(value: &str) -> Option<Self> {
    match value {
      "Accepted" => Some(Self::Accepted),
      "AcceptedCanceled" => Some(Self::AcceptedCanceled),
      "Available" => Some(Self::Available),
      "BadMessage" => Some(Self::BadMessage),
      "Blocked" => Some(Self::Blocked),
      "Canceled" => Some(Self::Canceled),
      "CertChainError" => Some(Self::CertChainError),
      "CertificateExpired" => Some(Self::CertificateExpired),
      "CertificateRevoked" => Some(Self::CertificateRevoked),
      "Charging" => Some(Self::Charging),
      "ChecksumVerified" => Some(Self::ChecksumVerified),
      "ConcurrentTx" => Some(Self::ConcurrentTx),
      "ConditionNotSupported" => Some(Self::ConditionNotSupported),
      "ContractCancelled" => Some(Self::ContractCancelled),
      "CRL" => Some(Self::Crl),
      "Downloaded" => Some(Self::Downloaded),
      "DownloadFailed" => Some(Self::DownloadFailed),
      "Downloading" => Some(Self::Downloading),
      "DownloadOngoing" => Some(Self::DownloadOngoing),
      "DownloadPaused" => Some(Self::DownloadPaused),
      "DownloadScheduled" => Some(Self::DownloadScheduled),
      "Duplicate" => Some(Self::Duplicate),
      "DuplicateTariffId" => Some(Self::DuplicateTariffId),
      "EmptyResultSet" => Some(Self::EmptyResultSet),
      "Expired" => Some(Self::Expired),
      "Failed" => Some(Self::Failed),
      "Faulted" => Some(Self::Faulted),
      "Finishing" => Some(Self::Finishing),
      "Good" => Some(Self::Good),
      "Idle" => Some(Self::Idle),
      "Inoperative" => Some(Self::Inoperative),
      "InstallationFailed" => Some(Self::InstallationFailed),
      "Installed" => Some(Self::Installed),
      "Installing" => Some(Self::Installing),
      "InstallRebooting" => Some(Self::InstallRebooting),
      "InstallScheduled" => Some(Self::InstallScheduled),
      "InstallVerificationFailed" => Some(Self::InstallVerificationFailed),
      "Invalid" => Some(Self::Invalid),
      "InvalidCertificate" => Some(Self::InvalidCertificate),
      "InvalidChecksum" => Some(Self::InvalidChecksum),
      "InvalidSignature" => Some(Self::InvalidSignature),
      "LanguageNotSupported" => Some(Self::LanguageNotSupported),
      "NoCertificateAvailable" => Some(Self::NoCertificateAvailable),
      "NoChargingProfile" => Some(Self::NoChargingProfile),
      "NoCredit" => Some(Self::NoCredit),
      "NoCurrencyChange" => Some(Self::NoCurrencyChange),
      "NoFirmware" => Some(Self::NoFirmware),
      "NoProfile" => Some(Self::NoProfile),
      "NoProfiles" => Some(Self::NoProfiles),
      "NotAllowedTypeEVSE" => Some(Self::NotAllowedTypeEVSE),
      "NoTariff" => Some(Self::NoTariff),
      "NotAtThisLocation" => Some(Self::NotAtThisLocation),
      "NotAtThisTime" => Some(Self::NotAtThisTime),
      "NotFound" => Some(Self::NotFound),
      "NotImplemented" => Some(Self::NotImplemented),
      "NoTransaction" => Some(Self::NoTransaction),
      "NotReady" => Some(Self::NotReady),
      "NotSupported" => Some(Self::NotSupported),
      "NotSupportedAttributeType" => Some(Self::NotSupportedAttributeType),
      "NotSupportedMessageFormat" => Some(Self::NotSupportedMessageFormat),
      "NotSupportedOperation" => Some(Self::NotSupportedOperation),
      "NotSupportedPriority" => Some(Self::NotSupportedPriority),
      "NotSupportedState" => Some(Self::NotSupportedState),
      "Occupied" => Some(Self::Occupied),
      "OCSP" => Some(Self::Ocsp),
      "OngoingAuthorizedTransaction" => {
        Some(Self::OngoingAuthorizedTransaction)
      }
      "Operative" => Some(Self::Operative),
      "Pending" => Some(Self::Pending),
      "PermissionDenied" => Some(Self::PermissionDenied),
      "Preconditioning" => Some(Self::Preconditioning),
      "Preparing" => Some(Self::Preparing),
      "Processing" => Some(Self::Processing),
      "Published" => Some(Self::Published),
      "PublishFailed" => Some(Self::PublishFailed),
      "Ready" => Some(Self::Ready),
      "RebootRequired" => Some(Self::RebootRequired),
      "Rejected" => Some(Self::Rejected),
      "Removed" => Some(Self::Removed),
      "Reserved" => Some(Self::Reserved),
      "Revoked" => Some(Self::Revoked),
      "RevokedCertificate" => Some(Self::RevokedCertificate),
      "Scheduled" => Some(Self::Scheduled),
      "Settled" => Some(Self::Settled),
      "SignatureError" => Some(Self::SignatureError),
      "SignatureVerified" => Some(Self::SignatureVerified),
      "SuspendedEV" => Some(Self::SuspendedEV),
      "SuspendedEVSE" => Some(Self::SuspendedEVSE),
      "TooManyElements" => Some(Self::TooManyElements),
      "TxNotFound" => Some(Self::TxNotFound),
      "Unavailable" => Some(Self::Unavailable),
      "Unknown" => Some(Self::Unknown),
      "UnknownComponent" => Some(Self::UnknownComponent),
      "UnknownConnector" => Some(Self::UnknownConnector),
      "UnknownMessageId" => Some(Self::UnknownMessageId),
      "UnknownTransaction" => Some(Self::UnknownTransaction),
      "UnknownVariable" => Some(Self::UnknownVariable),
      "UnknownVendorId" => Some(Self::UnknownVendorId),
      "Unlocked" => Some(Self::Unlocked),
      "UnlockFailed" => Some(Self::UnlockFailed),
      "Unpublished" => Some(Self::Unpublished),
      "Unsupported" => Some(Self::Unsupported),
      "UnsupportedMonitorType" => Some(Self::UnsupportedMonitorType),
      "Uploaded" => Some(Self::Uploaded),
      "UploadFailed" => Some(Self::UploadFailed),
      "UploadFailure" => Some(Self::UploadFailure),
      "Uploading" => Some(Self::Uploading),
      "VersionMismatch" => Some(Self::VersionMismatch),
      _ => None,
    }
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
  let message_id = array[1].as_str().map(ToOwned::to_owned);

  match OcppMessageTypeId::from_i64(message_type) {
    Some(OcppMessageTypeId::Call) | Some(OcppMessageTypeId::Send) => {
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
    Some(OcppMessageTypeId::CallError)
    | Some(OcppMessageTypeId::CallResultError) => {
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
pub fn build_call(message_id: &str, action: &str, payload: Value) -> String {
  json!([OcppMessageTypeId::Call.value(), message_id, action, payload])
    .to_string()
}

/// Builds a CALLRESULT frame string (`[3, messageId, payload]`).
pub fn build_call_result(message_id: &str, payload: Value) -> String {
  json!([OcppMessageTypeId::CallResult.value(), message_id, payload])
    .to_string()
}

/// Builds a CALLERROR frame string (`[4, messageId, code, desc, details]`).
pub fn build_call_error(
  message_id: &str,
  code: &str,
  description: &str,
  details: Value,
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

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use serde_json::{Value, json};

  use crate::embedded_schemas::EmbeddedSchemaType;

  use super::{
    BootReason, ChargingRateUnit, IdTokenType, IncomingAction_V1_6, Measurand,
    MeterUnit, MeterValueLocation, MeterValuePhase,
    OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS, OCPP_V1_6_SUPPORTED_ACTIONS,
    OCPP_V2_0_1_UNSUPPORTED_ACTIONS, OCPP_V2_1_UNSUPPORTED_ACTIONS,
    OCPP_V2_X_COMMON_SUPPORTED_ACTIONS, OcppFrame, OcppVersion, ReadingContext,
    ResponseStatus, SampledValueFormat, StatusNotificationErrorCode,
    StopReason, TransactionTriggerReason, VariableAttributeType, build_call,
    parse_frame,
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
  /// Verifies OCPP 1.6 security whitepaper actions are known out of scope.
  fn v1_6_security_extension_actions_are_not_supported() {
    for action in OCPP_V1_6_SECURITY_UNSUPPORTED_ACTIONS {
      assert!(IncomingAction_V1_6::is_known_unsupported(action));
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
    let mut reasons = enum_tokens_at_path(
      "schemas/1.6/StopTransaction.json",
      &["properties", "reason", "enum"],
    );
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
  /// Verifies all schema transaction trigger tokens are typed.
  fn transaction_trigger_reason_covers_schema_enums() {
    let mut triggers = definition_enum_tokens(
      "schemas/2.0.1/TransactionEventRequest.json",
      "TriggerReasonEnumType",
    );
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
    let mut contexts = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("context"),
    );
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

    let mut measurands = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("measurand"),
    );
    measurands.extend(definition_enum_tokens(
      "schemas/2.0.1/MeterValuesRequest.json",
      "MeasurandEnumType",
    ));
    measurands.extend(definition_enum_tokens(
      "schemas/2.1/MeterValuesRequest.json",
      "MeasurandEnumType",
    ));
    assert_wire_enum_tokens(measurands, Measurand::parse, Measurand::as_str);

    let mut phases = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("phase"),
    );
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

    let mut locations = enum_tokens_at_path(
      "schemas/1.6/MeterValues.json",
      &meter_sampled_value_path("location"),
    );
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
  /// Verifies common OCPP 2.x scalar tokens are typed from schemas.
  fn v2_x_scalar_enums_cover_schema_tokens() {
    let mut boot_reasons = definition_enum_tokens(
      "schemas/2.0.1/BootNotificationRequest.json",
      "BootReasonEnumType",
    );
    boot_reasons.extend(definition_enum_tokens(
      "schemas/2.1/BootNotificationRequest.json",
      "BootReasonEnumType",
    ));
    assert_wire_enum_tokens(
      boot_reasons,
      BootReason::parse,
      BootReason::as_str,
    );

    let id_token_types = definition_enum_tokens(
      "schemas/2.0.1/AuthorizeRequest.json",
      "IdTokenEnumType",
    );
    assert_wire_enum_tokens(
      id_token_types,
      IdTokenType::parse,
      IdTokenType::as_str,
    );

    let mut attribute_types = definition_enum_tokens(
      "schemas/2.0.1/GetVariablesRequest.json",
      "AttributeEnumType",
    );
    attribute_types.extend(definition_enum_tokens(
      "schemas/2.1/GetVariablesRequest.json",
      "AttributeEnumType",
    ));
    assert_wire_enum_tokens(
      attribute_types,
      VariableAttributeType::parse,
      VariableAttributeType::as_str,
    );

    let mut charging_rate_units = enum_tokens_at_path(
      "schemas/1.6/GetCompositeScheduleResponse.json",
      &[
        "properties",
        "chargingSchedule",
        "properties",
        "chargingRateUnit",
        "enum",
      ],
    );
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
  /// Verifies OCPP 1.6 StatusNotification error codes are typed.
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
    let text = build_call("m1", "Heartbeat", json!({}));
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
