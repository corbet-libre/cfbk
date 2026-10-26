# cfbk — agent instructions

Work in a lane, never in ~/agents/scratch/ws. From the repo dir:
`cargo fmt --all --check`,
`cargo clippy --locked --all-targets -- -D warnings`,
`cargo test --locked`,
`cargo check --locked --target wasm32-unknown-unknown`,
`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test
--locked --target wasm32-unknown-unknown --lib`.
Pure logic: zero runtime deps, no I/O, clock, randomness or async runtime.
Consumers (workspace Cargo.tomls): cmsh, cphn, cpmt, cvld, cvch, cmbx, cgbl.
Coverage gate: .github/test-source-coverage.py + check-source-coverage.py.

Invariants:
- Active is the first ordered healthy candidate meeting the standard; a report
  returns Switch iff the active changed (tests/properties.rs:91).
- Never below the minimum or failed; empty selection is Unavailable, never a
  silent downgrade (tests/properties.rs:114).
- Deterministic: same config + report sequence gives the same outcome (:134).
- Recovery is explicit: Healthy report or reset only (:177).
- excluded() is exactly the ordered candidates below the minimum (:159).
- Costs (rules: docs/COSTS.md): unknown prices never dispatch; pending/uncertain
  outcomes keep the reservation and block fallback; over-bound actuals are
  terminal (src/cost/tests/mod.rs:127,203,238,281).
- Every selection-semantics change needs a property test against the reference
  model in tests/properties.rs.

Pointers: docs/COSTS.md, docs/COVERAGE.md, tests/properties.rs,
src/tests/mod.rs, src/cost/tests/mod.rs.
