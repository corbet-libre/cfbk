# Costed actions and simulation

`cost::Plan` accepts explicitly enabled candidate capabilities, request-specific
quotes, ordering, one total budget, a maximum attempt count, mode and caller time.
Planning is local pure Rust; no provider, credential, clock or database is used.
The caller selects `Configured` or `Cheapest`; ties preserve configured order.
Unconfigured providers and candidates below the required capability cannot run.

A quote is an exact price or defensible upper bound with source, exclusive
expiry and an opaque request binding. Unknown/malformed/expired prices and
incompatible denominations are excluded. Each integer amount declares its unit
and decimal scale. There is no implicit currency conversion or floating-point
rounding. Test prices in this repository are inert fixtures, not service tariffs.

The domain library derives the quote binding from the provider/account instance,
execution mode, operation, authenticated context and all cost-relevant input.
It checks the independently derived binding against `DispatchPermit::validate`
at the actual dispatch edge. A member cannot supply trusted prices or bindings.
Provider quotes and configured tariffs are authenticated/configuration inputs;
`cfbk` does not discover or validate external price sources. If obtaining a quote
itself incurs a cost, that is a separate explicitly costed operation.

`Plan::start` creates a `Cascade`. `begin` reserves the attempt's maximum charge
before returning its move-only `DispatchPermit`. The same cascade cannot start
another attempt while one remains pending. `complete` records `Success`,
`SafeFailure`, `Refused`, `Pending` or `Uncertain`, with an optional established
actual cost. Missing actual cost retains the full bound. Pending/uncertain
outcomes never release exposure, even when accompanied by a preliminary cost.
An excessive/incompatible actual charge is a terminal refusal. A failed attempt
can still cost money. Success and refusal stop the cascade; only a definitive
safe failure can advance it, within the remaining total budget and attempt cap.

This state is RAM-only and applies to one instance. **It does not enforce a
durable or cross-process spending limit.** The enclosing application owns atomic
operation/budget reservations, stable operation IDs and uncertain-outcome
recovery. Domain dispatch must reserve those externally before performing a
side effect. Reconstructing a plan cannot be used to retry a spent or uncertain
operation. `DispatchPermit` is authorization metadata, not a payment receipt or
identity proof; it conveys no claim that an upstream operation succeeded.

`Simulation` uses these same pure planning and transition methods with explicit
fixture outcomes and no network. It is useful for showing the selected path,
excluded offers, potential attempt costs and budget stops. It cannot predict
which real provider will succeed. Domain libraries must give simulated results
a separate type which real proof-verification paths cannot accept. `Sandbox`
means the provider's explicit test environment; it is not presumed free and
cannot be promoted to `Production`. The domain library validates the actual mode
and returns real evidence only for its approved production verification path.

Tests exercise the complete transition contract with dummy tariffs and actual
pure code, including an independent integer-budget property model. They require
no service accounts or provider calls. No live sandbox, billing or deployment
is part of verification. Existing transport fallback APIs remain unchanged.

A reported charge outside the authorized bound or in another unit terminates the
cascade with `Error::Charge`. `charge_violation()` retains the earlier exposure,
authorized attempt maximum and exact reported amount/unit. After that error,
`exposure()` describes the authorization ledger, not the actual overrun total.
The host reconciles prior plus reported cost, using a wider total or multiple
units as needed; no provider fee is silently discarded or converted.
