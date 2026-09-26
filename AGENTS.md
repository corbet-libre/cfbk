# Agent instructions

Write all code comments and documentation in English.

## Product boundary

- Ordered fallback executor: a configured order, a minimum standard, consumer
  health reports, one active candidate, no silent downgrade.
- Consumers: `cmsh` (transport backends) and gate provider cascades via `cvld`.
- Pure logic: no I/O, clock, randomness, async runtime or dependencies. Keep it
  that way; timing and probing belong to the consumer.
- Never decides policy (order, standard); it executes what it is given.
- This crate is LGPL-3.0-only WITH LGPL-3.0-linking-exception. Do not add
  implementation code available only under the full GPL or AGPL, and never
  depend on FSL code.

## Quality boundary

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` and the wasm32 build — all green before every commit. No local
  workstation builds; use GHA, Crow as fallback.
- Every change to selection semantics needs a matching property test against
  the reference model in `tests/properties.rs`.
