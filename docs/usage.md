# Usage

`ocppsim` can run without a remote target, directly from CLI flags, or from a
named TOML profile.

## Local Mode

Run `ocppsim` without a startup connection target when you want to exercise
local simulator state before connecting to a CSMS endpoint.

```sh
ocppsim
```

Inside the terminal UI, connect later with a profile or direct WebSocket
arguments:

```text
connect some-profile
connect ws://csms.example.com/ocpp CP-001
```

Connection attempts keep processing commands while DNS, TCP, TLS, or the
WebSocket handshake is pending.
A second `connect` reports that an attempt is already in progress.
Use `disconnect` to cancel the attempt before selecting another target.
`exit` and `quit` cancel it and exit the simulator.

## Direct Mode

`ocppsim` can start with a CSMS endpoint directly from CLI flags.

```sh
ocppsim --ws-url ws://csms.example.com/ocpp --cp-id CP-001
```

These flags configure the connection target.
After the terminal UI opens, type `connect` to open the WebSocket connection.

By default, direct mode appends the charge point ID to the WebSocket URL path.
Use `--no-append-cp-id` when the CSMS endpoint already includes the final path.
Charge point ids must be non-empty.
For OCPP 2.0.1 and 2.1, ids are limited to 48 characters and cannot contain
`:`.
The `:` character is also rejected for Security Profile 1 and 2 because HTTP
Basic authentication uses it to separate the username from the password.
When Security Profile 1 or 2 is active, the final URL path identity must match
the charge point ID used as the Basic Auth username.

Some other useful flags include:

- `--protocol 1.6`, `--protocol 2.0.1`, or `--protocol 2.1`
- `--connectors <count>`
- `--log-path <path>`
- `--trace-frames`
- `--strict`
- `--request-timeout-seconds <seconds>`
- `--heartbeat-seconds <seconds>`
- `--outbound-queue-limit <count>`
- `--security-event-limit <count>`
- `--security-profile <1|2|3>`
- `--basic-auth-password <password>`
- `--ca-cert <path>`
- `--client-cert <path>` and `--client-key <path>`

`--request-timeout-seconds` limits each connection attempt, WebSocket write,
and OCPP response wait independently.
It defaults to 30 seconds; values below five seconds are raised to five.
Commands received during connection setup do not reset its deadline.
A stalled write disconnects when its deadline expires.
After exit is confirmed, the simulator allows one second for graceful shutdown,
then cancels any remaining connection work.

Run `ocppsim --help` for a full list of CLI options.

## Profile Mode

`ocppsim` can also load configuration from a TOML file, so you can maintain
multiple profiles for different CSMS endpoints and test scenarios, and run
`ocppsim` more conveniently like:

```sh
ocppsim some-profile
```

The profile configures the startup target and simulator settings.
Type `connect` inside the terminal UI to connect to that target.

## Configuration

`ocppsim` can be configured through a TOML file with global defaults and named
charge point profiles.
Global defaults apply in local, direct, and profile modes.
The default file is `~/.config/ocppsim/ocppsim.toml`; use `--config-path` to
select a different file.
Without a profile, a missing default file uses built-in defaults.
An explicit `--config-path` or a selected profile requires the file to exist.
Invalid configuration files report an error in every mode.
Profile values override global defaults, and CLI flags override both.

Below is a sample config file with all available options.

```toml
# Optional global defaults that can be overridden by charge point configs.
# CLI options take precedence over config file items.
protocol = "2.1"
vendor = "ocppsim"
model = "ocppsim"
firmware = "0.1.0"
log-path = "./ocppsim.log"
trace-frames = false
strict = false
request-timeout-seconds = 30
heartbeat-seconds = 0
outbound-queue-limit = 1000
security-event-limit = 1000
security-profile = 2
basic-auth-password = "0123456789abcdef0123456789abcdef"
ca-cert = "./csms-root.pem"
client-cert = "./charge-point.pem"
client-key = "./charge-point-key.pem"

[charge-points.example]
ws-url = "wss://csms.example.com/ocpp"
id = "CP-001"
append-cp-id = true
connectors = 1

[charge-points.yard]
ws-url = "wss://csms.example.com/ocpp"
id = "CP-002"
append-cp-id = true
protocol = "1.6"
connectors = 2

[charge-points.lab]
ws-url = "wss://localhost:9000/ocpp/CP-003"
id = "CP-003"
append-cp-id = false
protocol = "2.0.1"
connectors = 1
log-path = "./lab-ocppsim.log"
trace-frames = true
strict = true
outbound-queue-limit = 0
security-event-limit = 5000
```

CLI flags override profile values where both are provided.

## Logs

File logs are appended when `--log-path`, global `log-path`, or the startup
profile's `log-path` is set.
`ocppsim` does not rotate or truncate log files.
Use a unique path per long-running profile, or rotate the file with external
tooling such as `logrotate`, `newsyslog`, or a shell wrapper that moves the
old file before startup.

## Interactive Commands

After startup, type commands into the terminal UI.
Logs are printed into normal terminal scrollback, so they can be selected with
the mouse and remain available after `ocppsim` exits.

- `status`: show current simulator state.
- `connect`: open the configured WebSocket target.
  A new `BootNotification` is sent when registration is not accepted yet or the
  boot payload changed since the previous connection.
- `connect <profile>`: load a profile and connect to its WebSocket target.
- `connect <ws-url> <cp-id>`: connect to a direct WebSocket target.
- `disconnect`: close the active WebSocket connection.
- `boot`: send `BootNotification` if the registration retry interval has
  expired.
- `authorize <idToken>`: send `Authorize`.
- `data-transfer <vendorId> [messageId] [data...]`: send `DataTransfer`.
- `start <connector> <idToken>`: start a local transaction.
- `stop <connector> [reason]`: stop a local transaction.
- `connector-status <connector> <status>`: set local connector status.
- `meter`: show the local meter counters in Wh for all connectors.
- `meter <connector>`: show the local meter counter in Wh for one connector.
- `meter <connector> <wh>`: set the local meter counter.
  Use `+=<wh>` or `-=<wh>` to increase or decrease the current reading.
  For example, `meter 1 +=100` adds 100 Wh and `meter 1 -=50` subtracts 50 Wh.
  A plain sign is part of an absolute value: `meter 1 -50` sets -50 Wh.
- `send-meter <connector> [<wh>]`: send the stored energy reading.
  If a value is given, first set or adjust the reading as with `meter`.
- `soc`: show local EV state of charge for all connectors.
- `soc <connector>`: show local EV state of charge for one connector.
  Unknown readings are shown as `unset`.
- `soc <connector> <percent>`: set local EV state of charge from 0 to 100.
  Decimal percentages are accepted, so `75.5` means 75.5%.
  Use `+=<percent>` or `-=<percent>` to adjust it by percentage points.
  For example, `soc 1 +=5` increases 75% to 80%.
  `soc 1 +5` sets 5%; a negative absolute SoC is rejected.
  A value set while idle becomes the initial SoC of the next transaction.
- `send-soc <connector> [<percent>]`: send the stored SoC reading.
  If a value is given, first set or adjust the reading as with `soc`.
  SoC is initially unset; set it before adjusting or sending it.
  Adjustments must leave SoC between 0 and 100.
  Finalizing a transaction clears SoC; failed stops and reconnects preserve it.
- `heartbeat`: send one heartbeat.
- `heartbeat start <seconds>`: start periodic heartbeats.
- `heartbeat stop`: stop periodic heartbeats.
- `standards`: show the active protocol summary.
- `help`: show command help.
- `exit`: exit the simulator. If active transactions, pending OCPP messages,
  or other active CSMS state exists, the input line asks for `[y/N]`
  confirmation. Press `y` or `Y` to exit; any other non-control key cancels.
  Ctrl-C and Ctrl-D leave the confirmation active.

Adjustment operators and their operands form one token, such as `-=2.5`.
Both send commands accept the same absolute and adjustment forms.
An invalid adjustment leaves the reading unchanged and cancels the send.
The `meter` and `soc` query forms work online or offline and do not change
readings or send OCPP messages.

Commands that send OCPP messages require an active connection.
Local state commands can still update simulator state while disconnected.
The optional values in `send-meter` and `send-soc` are stored locally even
when sending is unavailable.
Each command sends only its named measurand.
During an active OCPP 2.x transaction, these sends use `TransactionEvent`;
otherwise, they use `MeterValues`.

## Shell Completions

`ocppsim` can generate shell completion setup scripts:

```sh
# Bash
source <(ocppsim completions bash)
# Zsh
source <(ocppsim completions zsh)
# Fish
ocppsim completions fish | source
# PowerShell
ocppsim completions powershell | Out-String | Invoke-Expression
```

The generated completion script supports completing profile names from
`~/.config/ocppsim/ocppsim.toml` while you type.

## Interoperability Notes

The simulator requests exactly one OCPP-J WebSocket subprotocol:
`ocpp1.6`, `ocpp2.0.1`, or `ocpp2.1`.
The connection fails if the CSMS does not negotiate the requested token.

Security profile settings validate the transport before connecting.
Profile 1 requires `ws://` plus HTTP Basic authentication.
Profile 2 requires `wss://` plus HTTP Basic authentication.
Profile 3 requires `wss://` plus `--client-cert` and `--client-key`.

Basic Auth password validation follows the selected OCPP version.
OCPP 1.6 `AuthorizationKey` values must contain an even number of ASCII
hexadecimal characters, from 32 to 40, representing a 16- to 20-byte key.
The simulator decodes that key before encoding the HTTP Basic credentials.
OCPP 2.0.1 `BasicAuthPassword` values must be 16 to 40 OCPP
`passwordString` characters.
OCPP 2.1 `BasicAuthPassword` values must be 16 to 64 UTF-8 `passwordString`
characters.
For OCPP 2.1 this is a compatibility variable exposed through
`SecurityCtrlr`.
The simulator does not implement the full 2.1 `NetworkConfiguration`
component or network-profile model.
OCPP 2.x passwords are used as UTF-8 text without hexadecimal decoding.

Profile 1 sends Basic Auth over an unencrypted WebSocket; the password is only
Base64 encoded on the wire.
Use profile 1 only on trusted lab networks, tunnels, or VPNs.
Prefer storing them in a protected profile file over passing them on the
command line, because shell history and process listings can expose CLI
arguments.

When certificate paths are configured, the simulator builds a rustls connector
using the WebPKI root store, the optional `--ca-cert`, and the optional client
certificate and key.
For OCPP-level security-profile upgrades, a configured CA path counts as the
Central System root prerequisite and configured client certificate/key paths
count as the profile 3 Charge Point certificate material.
Security-profile passwords are not included in frame logs or configuration
readback.

When a connected CSMS changes `AuthorizationKey`, `BasicAuthPassword`, or an
accepted higher OCPP 1.6 `SecurityProfile`, the simulator closes the current
connection and reconnects using the new security settings.
If a profile-upgrade reconnect fails, the simulator restores the previous
profile and attempts one fallback reconnect.

An accepted `BootNotification` response starts or restarts periodic
heartbeats using the CSMS-provided `interval` value.
Use `heartbeat stop` after boot when you want to suppress periodic heartbeats
for a manual test.

## Resource Limits

`--outbound-queue-limit` and profile `outbound-queue-limit` cap the number of
outbound OCPP CALL messages waiting behind the active pending request.
The default is `1000`.
When the limit is reached, later outbound messages are dropped with a warning
until the CSMS responds and queue space becomes available.
Set the value to `0` to disable this cap for deliberate stress tests.

`--security-event-limit` and profile `security-event-limit` cap retained
security events used for `SecurityEventNotification` replay.
The default is `1000`.
When trimming is required, sent events are discarded before unsent events.
Set the value to `0` to retain events without a simulator-imposed cap.

## Tracing And Strict Validation

Use `--trace-frames` when debugging interoperability.
It logs complete JSON CALL, CALLRESULT, and CALLERROR frames in addition to the
normal summary lines.

Known sensitive fields are redacted before frames are logged in either
direction, including OCPP 1.6 `AuthorizationKey` changes, OCPP 2.x
`BasicAuthPassword` variable writes, OCPP `idTag` or `idToken` values, and
URL-contained credentials.

Use `--strict` or profile `strict = true` when you want inbound CSMS requests
and responses validated against the checked-in JSON schemas before simulator
dispatch or response side effects.
Strict mode returns `FormationViolation` on OCPP 1.6 and `FormatViolation` on
OCPP 2.x for schema-invalid inbound CALL payloads.
Schema-invalid CALLRESULT payloads are not applied, and OCPP 2.1 also receives
`CALLRESULTERROR`.

Without strict mode, the simulator validates only the fields needed by its
implemented behavior and ignores optional fields outside that behavior.

Certificate management, signing, signed firmware, log retrieval, and security
event workflows are simulator-level implementations.
They validate protocol structure, maintain an in-memory synthetic certificate
store, emit expected status notifications, and record obvious security events.

Recorded security events are queued for `SecurityEventNotification` delivery
until acknowledged, including events recorded while offline or immediately
before a security reconnect.
They do not perform full PKI validation, OCSP/CRL checks, real CSR generation,
firmware binary verification, or file transfer.

Use the string `invalid` in simulated certificates or signatures when you want
to drive the invalid-certificate or invalid-signature paths.
For OCPP 1.6 Security Whitepaper firmware flows, use
`SignedUpdateFirmware`.
The original OCPP 1.6 `UpdateFirmware` request returns CALLERROR
`NotSupported`.

## Troubleshooting

If the connection opens and closes immediately, confirm that the CSMS accepts
the selected OCPP-J subprotocol token: `ocpp1.6`, `ocpp2.0.1`, or `ocpp2.1`.
The simulator closes the connection when the server does not negotiate the
requested token.

If the connection is refused, verify the host, port, path, container network,
and whether `append-cp-id` should be enabled for the target CSMS.
For Docker on macOS, `host.docker.internal` reaches services running on the
host.

If a `wss://` endpoint fails during the TLS handshake, test the same endpoint
with a WebSocket client that can show certificate diagnostics.
The simulator uses the bundled WebPKI root store by default.
Use `--ca-cert` when the CSMS uses a private CA, and use `--client-cert` plus
`--client-key` for profile 3 client-certificate authentication.
There is no flag to bypass certificate validation.

If requests time out, increase `--request-timeout-seconds` for slow CSMS test
systems and enable `--trace-frames` to confirm whether a CALLRESULT or
CALLERROR was received.

If a command appears to do nothing, check the connection state first.
Commands that send OCPP messages require an active WebSocket connection;
offline local commands only update simulator state.

If the CSMS rejects remote requests with `FormationViolation` on OCPP 1.6 or
`FormatViolation` on OCPP 2.x, inspect the raw frame with `--trace-frames`.
Supported inbound actions validate schema-required fields before mutating
state, including connector or EVSE ids, reservation ids, trigger-message
names, local-list versions, firmware locations, and smart-charging schedules.
When `--strict` is enabled, optional schema fields are validated as well.

If smart-charging composite schedules return `Rejected`, no effective charging
profile is stored for that connector.
Set a charging profile first.
A stored zero-watt limit is distinct from no profile and intentionally
suspends the connector.

If the terminal output grows noisy during stress tests, lower the message rate
or increase the CSMS response speed.
The simulator drops new outbound messages once `outbound-queue-limit` is
reached.
Set `outbound-queue-limit = 0` for scripted tests that intentionally need an
unbounded queue.
