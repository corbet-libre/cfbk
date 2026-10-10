# cfbk

**Ordered fallback executor.**

`cfbk` executes a fallback decision made elsewhere: a configured order of
candidates and a minimum standard every selected candidate must meet. The
consumer reports health; `cfbk` answers which candidate is active and tells the
consumer whenever that changes.

First consumer: the `cmsh` transport facade (Veilid and Tor backends). Planned
second consumer: provider cascades in gates via `cvld` (for example SMS
providers in `cphn`, payment providers in `cpmt`).

Status: implemented (0.0.0, not published). Pure logic, no dependencies.

`cost` adds request-bound cost previews, caller-selected cheapest/configured
ordering and a total-budget cascade. Unknown prices cannot authorize dispatch;
pending or uncertain effects block fallback. Simulation, provider sandbox and
production have distinct dispatch permissions. See [costed actions](docs/COSTS.md).

## Model

- **Candidate:** an identifier plus the set of properties it declares (for a
  transport: anonymous, offline delivery, datagrams, a latency class).
- **Config:** the preference order and the minimum standard (a property set).
- **Health:** `Healthy` (initial) or `Failed`, reported by the consumer.
- **Active candidate:** the first candidate in the order that declares every
  property of the minimum standard and is not reported failed. There is exactly
  one at a time, or none.
- **Switch:** returned by a report that changes the active candidate, so a
  change is never silent.

Ordered qualities are expressed by the consumer as monotone flags (for example
"latency at most interactive"), which keeps the standard a plain set inclusion.
`select_meeting` adds a stricter per-use requirement on top of the minimum
standard; it can only raise the bar.

## Guarantees

- Never selects a candidate below the minimum standard. When nothing acceptable
  is left, selection fails with `Unavailable` (either no ordered candidate meets
  the standard, or all acceptable ones are reported failed).
- Deterministic: no clock, randomness or I/O.
- Recovery is explicit: a failed candidate is selectable again only after the
  consumer reports it healthy (or calls `reset`).
- Ordered candidates below the standard stay visible through `excluded()`.

Unit tests cover the scenarios; property tests (`proptest`) check the executor
against an independent reference model after every report, plus the
no-downgrade, determinism and switch-reporting invariants.

## Boundaries

What it does:

- Executes an order and a minimum standard over consumer-reported health.

What it never does:

- Never decides the order or the standard (the caller's policy decides).
- Never probes, retries, times or measures anything: no I/O of any kind.
- Never interprets candidates or properties beyond set inclusion.

## License

Copyright 2026 Julian Y. Richard Corbet. Licensed under
[LGPL-3.0-only](LICENSES/LGPL-3.0-only.txt)
[WITH LGPL-3.0-linking-exception](LICENSES/LGPL-3.0-linking-exception.txt).
See [LICENSE.md](LICENSE.md). LGPL ("LGPL executes, FSL decides") so that LGPL
gate leaves can use it without depending on FSL code.

## Continuous verification

Dependency updates follow main and are tested against one CI-resolved lockfile.
Line and branch coverage target 100%; failures remain blocking. See
[the coverage contract](docs/COVERAGE.md) for measurement and exclusions.

The same deterministic lifecycle/refusal vectors execute natively and as real
Wasm in Node. Native proptest still compares selection against the independent
reference model. Wasm test tooling is a target-specific development dependency;
the production executor retains no dependencies, clock, I/O or randomness.
