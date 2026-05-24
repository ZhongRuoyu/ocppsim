# ocppsim

`ocppsim` is a terminal OCPP-J (Open Charge Point Protocol JSON) charge point
simulator written in Rust.
It connects to a CSMS over WebSocket, negotiates an OCPP subprotocol, and lets
an operator drive common charge point workflows from an interactive terminal UI.

The simulator is focused on practical protocol development and CSMS testing.
It keeps local connector, transaction, meter, reservation, charging profile, and
configuration state, then emits protocol-shaped OCPP messages from that state.

## Installation

- Cargo

  ```sh
  cargo install ocppsim
  ```

- Homebrew

  ```sh
  brew install zhongruoyu/tap/ocppsim
  ```

- Prebuilt binaries

  Download prebuilt binaries for Linux, macOS, and Windows from `ocppsim`'s
  [latest release](https://github.com/ZhongRuoyu/ocppsim/releases/latest) on
  GitHub.

- Docker

  Docker images for `ocppsim` are also available on Docker Hub as
  [`zhongruoyu/ocppsim`](https://hub.docker.com/r/zhongruoyu/ocppsim),
  and on GitHub Container Registry as
  [`ghcr.io/zhongruoyu/ocppsim`](https://ghcr.io/zhongruoyu/ocppsim).
  Use the `latest` tag or a specific version tag like `v0.1.0` to track stable
  releases, and `main` to track the latest commit on the main branch.

## Quick Start

Run `ocppsim` directly against a CSMS endpoint:

```sh
ocppsim --ws-url ws://csms.example.com/ocpp --cp-id CP-001
```

Or use a named profile from a [TOML config file](docs/usage.md#configuration)
in `~/.config/ocppsim/ocppsim.toml` or a custom path:

```sh
ocppsim some-profile
ocppsim some-profile --config-path ./ocppsim.toml
```

In a Docker container, you can run `ocppsim` with a mounted config file like:

```sh
docker run --rm -it \
  -v "$PWD/ocppsim.toml:/config/ocppsim.toml:ro" \
  zhongruoyu/ocppsim some-profile --config-path /config/ocppsim.toml
```

Run `ocppsim --help` for more CLI options.
Inside the terminal UI, type `help` to see available simulator commands.
See [documentation](docs/usage.md) for more details on usage.

[Shell completions](docs/usage.md#shell-completions) can be enabled by adding the
relevant command to your shell profile:

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

## Documentation

Documentation on design, protocol support, usage, and more is available in the
[`docs`](docs/README.md) directory.

## Protocol Scope

OCPP 1.6 base JSON schemas are the primary complete target.
The OCPP 1.6 Security Whitepaper extension is intentionally out of scope for
now and is reported as unsupported when recognized.

OCPP 2.0.1 and OCPP 2.1 support the feature subset that maps to already
implemented OCPP 1.6 behavior, plus `GetVariables` and `SetVariables` for the
configuration-equivalent device model surface.
Other 2.x actions are explicitly treated as unsupported until they are
implemented and tested.

Schema validation tests use files under [`schemas`](schemas/README.md) as the
source of truth.

## License

This project is licensed under the MIT License.
See [LICENSE](./LICENSE) for details.
