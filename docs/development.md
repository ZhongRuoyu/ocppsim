# Development Guide

This guide records project conventions that should stay stable as the simulator
grows.

## Modularity Principles

Keep protocol mechanics, simulator state, and terminal UI behavior separated.
The current split is worth preserving:

- `src/ocpp.rs` owns OCPP-J frame parsing, protocol metadata, action
  manifests, and version labels.
- `src/simulator/types.rs` owns public simulator configuration/UI types and
  internal state types.
- `src/simulator/support.rs` owns protocol-neutral helper functions for
  defaults, timestamps, parsing, status mapping, and subprotocol validation.
- `src/simulator/incoming/` owns inbound CALL dispatch and request extraction.
- `src/simulator/workflow/` owns state transitions, outbound queueing,
  WebSocket I/O helpers, transaction workflows, and CALLRESULT/CALLERROR
  side effects.
- `src/simulator/payloads.rs` owns typed serde payload structs.
- `src/app/` owns terminal UI rendering, completion, and command history.

Prefer protocol-neutral state and version-specific translation at the edge.
Connector state, transactions, reservations, charging profiles, and
configuration entries should stay shared where the behavior is shared.
Outbound payload builders and inbound dispatchers should handle protocol wire
shape differences.

Keep the dispatcher explicit while the action set is modest.
Large `match` expressions are easier to audit than a clever action registry at
the current size.
Introduce tables or traits only when repeated boilerplate becomes a real
maintenance cost.

Keep typed payload structs for outbound and response payloads.
They make serde field names reviewable, keep schema-shaped data centralized,
and avoid fragile ad hoc JSON construction.

Keep request extractors side-effect free.
Inbound handlers should validate required fields before mutating simulator
state, so malformed supported requests can return `FormationViolation` without
partial state changes.

When adding or expanding an OCPP action, update the whole vertical slice:

- Add or update the checked-in schema under `schemas/` when needed.
- Add the action to the appropriate manifest in `src/ocpp.rs`.
- Add typed response or outbound payload structs in `src/simulator/payloads.rs`.
- Add side-effect-free request parsing in `src/simulator/incoming/request.rs`
  or a version-specific request module.
- Add explicit dispatch in the version-specific inbound handler.
- Add representative schema validation tests for emitted payloads.
- Add strict-mode tests for malformed inbound payloads when a request schema
  exists.
- Update `docs/ocpp-support.md` with the implemented behavior and limits.

Keep version-specific entry points where future protocol drift is plausible.
It is fine for OCPP 2.0.1 and OCPP 2.1 builders to call a shared `v2_x` body
when their current wire shape is identical.
The separate entry points make later differences local.

## Strict Validation

`--strict` and the `strict` TOML item enable inbound CALL payload validation
against checked-in schemas under `schemas/`.
Strict mode runs before protocol-specific dispatch and returns
`FormationViolation` when the payload does not match the request schema.

Keep strict mode optional.
The default mode remains pragmatic for CSMS development: handlers validate the
fields needed by implemented behavior and ignore optional fields outside that
behavior.

## Version Identifiers

Use explicit OCPP version spelling in code identifiers.

For snake_case identifiers, write the version suffix as `v1_6`, `v2_0_1`,
`v2_1`, or `v2_x`.
Prefer `v2_x` when an identifier covers both OCPP 2.0.1 and OCPP 2.1.

For CamelCase identifiers, write the version suffix as `V1_6`, `V2_0_1`,
`V2_1`, or `V2_X`.
Rust type names may use underscores in these version suffixes; add a local
`#[allow(non_camel_case_types)]` only where that spelling is necessary.

The version suffix should be separated by underscores on both sides to avoid
ambiguity.
For example, `IncomingAction_V2_X`, `v1_6_simulator`, or
`ListVersion_V2_X_Response`.

Use `OCPP` in identifiers only when it adds needed context.
Global constants should keep the prefix, for example
`OCPP_V2_1_UNSUPPORTED_ACTIONS`.
Identifiers whose surrounding type or module already makes the domain clear
should omit `ocpp`, for example `supported_v2_0_1_inbound_responses` or
`as_v1_6`.

Keep an `Ocpp_` prefix for narrowly scoped disambiguators where `V1_6` or
`V2_X` alone would be unclear, such as `SchemaNameStyle::Ocpp_V1_6`.

Avoid compact or ambiguous forms in identifiers:

- `ocpp16`, `ocpp2`, `ocpp2_0_1`
- `v2` or `V2` for shared OCPP 2.x behavior
- `v2x` or `V2X`

This rule applies to code identifiers.
Specification-defined wire tokens and paths, such as `ocpp2.0.1` WebSocket
subprotocol strings and `schemas/2.0.1`, stay in their standard form.

## Rust Style

Follow the existing 2-space indentation style and run `cargo fmt --all` before
submitting changes.

Run these checks before merging behavior changes:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
docker build .
```

Keep comments focused on intent or non-obvious behavior.
Prefer self-documenting names for straightforward assignments and branching.

Do not add new abstractions only to reduce a few lines of explicit protocol
handling.
Add an abstraction when it reduces meaningful duplication, clarifies a shared
state transition, or matches an established pattern in this codebase.
