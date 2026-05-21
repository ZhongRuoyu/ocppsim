# Usage

`ocppsim` can run directly from CLI flags or from a named TOML profile.

## Direct Mode

`ocppsim` can connect to a CSMS endpoint directly from CLI flags.

```sh
ocppsim --ws-url ws://csms.example.com/ocpp --cp-id CP-001
```

By default, direct mode appends the charge point id to the WebSocket URL path.
Use `--no-append-cp-id` when the CSMS endpoint already includes the final path.

Some other useful flags include:

- `--protocol 1.6`, `--protocol 2.0.1`, or `--protocol 2.1`
- `--connectors <count>`
- `--log-path <path>`
- `--trace-frames`
- `--strict`
- `--request-timeout-seconds <seconds>`
- `--heartbeat-seconds <seconds>`

Run `ocppsim --help` for a full list of CLI options.

## Profile Mode

`ocppsim` can also load configuration from a TOML file, so you can maintain
multiple profiles for different CSMS endpoints and test scenarios, and run
`ocppsim` more conveniently like:

```sh
ocppsim some-profile
```

## Configuration

`ocppsim` can be configured through a TOML file with global defaults and named
charge point profiles.
It expects the file to be present at `~/.config/ocppsim/ocppsim.toml` by
default, but you can specify a custom path with `--config-path`.

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
ws-url = "ws://localhost:9000/ocpp/CP-003"
id = "CP-003"
append-cp-id = false
protocol = "2.0.1"
connectors = 1
log-path = "./lab-ocppsim.log"
trace-frames = true
strict = true
```

CLI flags override profile values where both are provided.

## Logs

File logs are appended when `--log-path` or profile `log-path` is set.
`ocppsim` does not rotate or truncate log files.
Use a unique path per long-running profile, or rotate the file with external
tooling such as `logrotate`, `newsyslog`, or a shell wrapper that moves the
old file before startup.

## Interactive Commands

After startup, type commands into the terminal UI.

- `status`: show current simulator state.
- `connect`: open the WebSocket and enqueue boot/status messages.
- `disconnect`: close the active WebSocket connection.
- `boot`: send `BootNotification` immediately.
- `authorize <idToken>`: send `Authorize`.
- `data-transfer <vendorId> [messageId] [data...]`: send `DataTransfer`.
- `start <connector> <idToken>`: start a local transaction.
- `stop <connector> [reason]`: stop a local transaction.
- `connector-status <connector> <status>`: set local connector status.
- `meter <connector> <wh>`: set the local meter counter.
- `send-meter <connector>`: send current meter data.
- `heartbeat`: send one heartbeat.
- `heartbeat start <seconds>`: start periodic heartbeats.
- `heartbeat stop`: stop periodic heartbeats.
- `standards`: show the active protocol summary.
- `help`: show command help.
- `exit`: exit the simulator.

Commands that send OCPP messages require an active connection.
Local state commands can still update simulator state while disconnected.

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

An accepted `BootNotification` response starts or restarts periodic
heartbeats using the CSMS-provided `interval` value.
Use `heartbeat stop` after boot when you want to suppress periodic heartbeats
for a manual test.

Use `--trace-frames` when debugging interoperability.
It logs complete JSON CALL, CALLRESULT, and CALLERROR frames in addition to the
normal summary lines.

Use `--strict` or profile `strict = true` when you want inbound CSMS requests
validated against the checked-in JSON schemas before simulator dispatch.
Strict mode returns `FormationViolation` for schema-invalid inbound CALL
payloads.
Without strict mode, the simulator validates only the fields needed by its
implemented behavior and ignores optional fields outside that behavior.

The simulator supports plain OCPP-J WebSocket subprotocol negotiation.
It does not implement OCPP security profiles, client-certificate
authentication, certificate-management actions, or security event workflows.

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
The simulator uses the bundled WebPKI root store and does not provide flags to
bypass certificate validation.

If requests time out, increase `--request-timeout-seconds` for slow CSMS test
systems and enable `--trace-frames` to confirm whether a CALLRESULT or
CALLERROR was received.

If a command appears to do nothing, check the connection state first.
Commands that send OCPP messages require an active WebSocket connection;
offline local commands only update simulator state.

If the CSMS rejects remote requests with `FormationViolation`, inspect the raw
frame with `--trace-frames`.
Supported inbound actions validate schema-required fields before mutating
state, including connector or EVSE ids, reservation ids, trigger-message
names, local-list versions, firmware locations, and smart-charging schedules.
When `--strict` is enabled, optional schema fields are validated as well.

If smart-charging composite schedules return `Rejected`, no effective charging
profile is stored for that connector.
Set a charging profile first.
A stored zero-watt limit is distinct from no profile and intentionally
suspends the connector.

If the terminal UI grows noisy during stress tests, lower the message rate or
increase the CSMS response speed.
The simulator warns when the outbound queue grows past the built-in warning
threshold, but the queue is intentionally not capped so scripted tests do not
drop messages silently.
