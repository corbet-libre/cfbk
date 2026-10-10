use super::*;

fn amount(atoms: u64) -> Amount {
    Amount {
        unit: Unit {
            code: "TEST".into(),
            scale: 3,
        },
        atoms,
    }
}

fn offer(id: &str, atoms: u64) -> Offer<String, String> {
    Offer {
        candidate: Candidate::new(id.into(), ["phone".into()]),
        quote: Quote {
            price: Price::AtMost(amount(atoms)),
            source: "inert test tariff".into(),
            valid_until: 100,
            binding: format!("request/{id}"),
        },
    }
}

fn plan(
    offers: Vec<Offer<String, String>>,
    budget: u64,
    attempts: usize,
    mode: Mode,
) -> Result<Plan<String, String>, Error> {
    let config = Config::new(
        offers.iter().map(|o| o.candidate.id.clone()),
        ["phone".into()],
    );
    Plan::new(
        offers,
        config,
        CostOrder::Cheapest,
        amount(budget),
        attempts,
        mode,
        10,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn planning_explains_exclusions_and_only_compares_compatible_bounded_prices() {
    let mut missing = offer("missing", 1);
    missing.candidate.properties.clear();
    let mut unknown = offer("unknown", 0);
    unknown.quote.price = Price::Unknown;
    let mut expired = offer("expired", 0);
    expired.quote.valid_until = 10;
    let mut foreign = offer("foreign", 0);
    foreign.quote.price = Price::Exact(Amount {
        unit: Unit {
            code: "OTHER".into(),
            scale: 3,
        },
        atoms: 1,
    });
    let mut exact = offer("exact", 4);
    exact.quote.price = Price::Exact(amount(4));
    let p = plan(
        vec![
            offer("expensive", 8),
            missing,
            unknown,
            expired,
            foreign,
            offer("over", 11),
            exact,
            offer("equal", 4),
        ],
        10,
        3,
        Mode::Simulation,
    )
    .unwrap();
    assert_eq!(
        p.offers()
            .iter()
            .map(|o| o.candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["exact", "equal", "expensive"]
    );
    assert_eq!(
        p.excluded(),
        &[
            ("missing".into(), Exclusion::Capability),
            ("unknown".into(), Exclusion::Quote),
            ("expired".into(), Exclusion::Quote),
            ("foreign".into(), Exclusion::Unit),
            ("over".into(), Exclusion::Budget)
        ]
    );
    assert_eq!(p.budget(), &amount(10));
    assert_eq!(p.max_attempts(), 3);
    assert_eq!(p.mode(), Mode::Simulation);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn configured_order_and_disabled_candidates_are_preserved() {
    let p = Plan::new(
        vec![offer("a", 9), offer("b", 2), offer("disabled", 0)],
        Config::new(["a".into(), "b".into()], ["phone".into()]),
        CostOrder::Configured,
        amount(20),
        2,
        Mode::Production,
        1,
    )
    .unwrap();
    assert_eq!(
        p.offers()
            .iter()
            .map(|o| o.candidate.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_policy_or_quotes_never_create_a_dispatch() {
    assert_eq!(
        plan(vec![offer("a", 1)], 10, 0, Mode::Simulation).unwrap_err(),
        Error::Configuration
    );
    assert_eq!(
        plan(vec![offer("a", 1), offer("a", 1)], 10, 1, Mode::Simulation).unwrap_err(),
        Error::Configuration
    );
    assert_eq!(
        plan(vec![], 10, 1, Mode::Simulation).unwrap_err(),
        Error::Unavailable
    );
    for unit in [
        Unit {
            code: String::new(),
            scale: 2,
        },
        Unit {
            code: "x".repeat(65),
            scale: 2,
        },
        Unit {
            code: "a b".into(),
            scale: 2,
        },
        Unit {
            code: "TEST".into(),
            scale: 19,
        },
    ] {
        let bad = Amount { unit, atoms: 1 };
        assert_eq!(
            Plan::new(
                vec![offer("a", 1)],
                Config::new(["a".into()], ["phone".into()]),
                CostOrder::Cheapest,
                bad.clone(),
                1,
                Mode::Production,
                1
            )
            .unwrap_err(),
            Error::Configuration
        );
        let mut o = offer("a", 1);
        o.quote.price = Price::Exact(bad);
        assert_eq!(
            plan(vec![o], 10, 1, Mode::Production).unwrap_err(),
            Error::Unavailable
        );
    }
    let mut source = offer("a", 1);
    source.quote.source.clear();
    let mut binding = offer("a", 1);
    binding.quote.binding.clear();
    assert_eq!(
        plan(vec![source, binding], 10, 1, Mode::Production).unwrap_err(),
        Error::Configuration
    );
    for field in [0, 1] {
        let mut o = offer("a", 1);
        if field == 0 {
            o.quote.source.clear()
        } else {
            o.quote.binding.clear()
        }
        assert_eq!(
            plan(vec![o], 10, 1, Mode::Production).unwrap_err(),
            Error::Unavailable
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_attempts_consume_the_total_budget_instead_of_resetting_it() {
    let mut c = plan(vec![offer("a", 6), offer("b", 7)], 10, 2, Mode::Production)
        .unwrap()
        .start();
    let permit = c.begin(11).unwrap();
    assert_eq!(permit.candidate(), "a");
    assert_eq!(permit.quote().price.maximum(), Some(&amount(6)));
    assert_eq!(permit.mode(), Mode::Production);
    assert_eq!(c.exposure(), amount(6));
    assert_eq!(c.attempts(), 1);
    assert_eq!(c.begin(11).unwrap_err(), Error::Pending);
    c.complete(Outcome::SafeFailure, None).unwrap();
    assert_eq!(c.begin(12).unwrap_err(), Error::Budget);
    assert_eq!(c.exposure(), amount(6));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn known_actual_charge_releases_only_unused_exposure() {
    let mut c = plan(vec![offer("a", 6), offer("b", 7)], 10, 2, Mode::Production)
        .unwrap()
        .start();
    c.begin(11).unwrap();
    c.complete(Outcome::SafeFailure, Some(amount(3))).unwrap();
    assert_eq!(c.exposure(), amount(3));
    assert_eq!(c.begin(12).unwrap().candidate(), "b");
    assert_eq!(c.exposure(), amount(10));
    c.complete(Outcome::Success, Some(amount(4))).unwrap();
    assert_eq!(c.exposure(), amount(7));
    assert_eq!(c.begin(12).unwrap_err(), Error::Finished);
    assert_eq!(c.complete(Outcome::SafeFailure, None), Err(Error::Finished));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn uncertain_or_pending_outcomes_preserve_the_reservation_until_reconciled() {
    for outcome in [Outcome::Uncertain, Outcome::Pending] {
        let mut c = plan(vec![offer("a", 3), offer("b", 4)], 10, 2, Mode::Sandbox)
            .unwrap()
            .start();
        c.begin(11).unwrap();
        c.complete(outcome, Some(amount(0))).unwrap();
        assert!(c.is_pending());
        assert_eq!(c.exposure(), amount(3));
        assert_eq!(c.begin(12).unwrap_err(), Error::Pending);
        c.complete(Outcome::Refused, Some(amount(2))).unwrap();
        assert!(!c.is_pending());
        assert_eq!(c.exposure(), amount(2));
        assert_eq!(c.begin(12).unwrap_err(), Error::Finished);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn quote_expiry_mode_and_request_binding_are_checked_at_the_dispatch_edge() {
    for mode in [Mode::Production, Mode::Sandbox, Mode::Simulation] {
        let mut c = plan(vec![offer("a", 0)], 0, 1, mode).unwrap().start();
        let permit = c.begin(99).unwrap();
        assert_eq!(permit.validate("request/a", mode, 99), Ok(()));
        assert_eq!(permit.validate("request/a", mode, 100), Err(Error::Expired));
        assert_eq!(permit.validate("request/b", mode, 99), Err(Error::Binding));
        let other = if mode == Mode::Production {
            Mode::Simulation
        } else {
            Mode::Production
        };
        assert_eq!(permit.validate("request/a", other, 99), Err(Error::Binding));
    }
    let mut c = plan(vec![offer("a", 1)], 10, 1, Mode::Production)
        .unwrap()
        .start();
    assert_eq!(c.begin(100).unwrap_err(), Error::Expired);
    assert_eq!(c.attempts(), 0);
    assert_eq!(c.complete(Outcome::Success, None), Err(Error::Finished));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unexpected_charges_are_terminal_and_cannot_be_reconciled_into_more_spending() {
    for actual in [
        amount(4),
        Amount {
            unit: Unit {
                code: "OTHER".into(),
                scale: 3,
            },
            atoms: 1,
        },
    ] {
        let mut c = plan(vec![offer("a", 3), offer("b", 4)], 10, 2, Mode::Production)
            .unwrap()
            .start();
        c.begin(11).unwrap();
        assert_eq!(
            c.complete(Outcome::SafeFailure, Some(actual)),
            Err(Error::Charge)
        );
        assert_eq!(
            c.complete(Outcome::SafeFailure, Some(amount(0))),
            Err(Error::Finished)
        );
        assert_eq!(c.begin(12).unwrap_err(), Error::Finished);
        assert_eq!(c.exposure(), amount(3));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn attempt_limit_and_candidate_exhaustion_are_distinct_from_success() {
    let mut limited = plan(vec![offer("a", 1), offer("b", 2)], 10, 1, Mode::Simulation)
        .unwrap()
        .start();
    limited.begin(11).unwrap();
    limited
        .complete(Outcome::SafeFailure, Some(amount(0)))
        .unwrap();
    assert_eq!(limited.begin(12).unwrap_err(), Error::Budget);
    let mut exhausted = plan(vec![offer("a", 1)], 10, 2, Mode::Simulation)
        .unwrap()
        .start();
    exhausted.begin(11).unwrap();
    exhausted.complete(Outcome::SafeFailure, None).unwrap();
    assert_eq!(exhausted.begin(12).unwrap_err(), Error::Finished);
    for error in [
        Error::Configuration,
        Error::Unavailable,
        Error::Pending,
        Error::Finished,
        Error::Budget,
        Error::Expired,
        Error::Binding,
        Error::Charge,
    ] {
        assert!(error.to_string().starts_with("costed fallback refused:"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
proptest::proptest! {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
    fn cheapest_safe_failures_match_an_independent_integer_budget_model(costs in proptest::collection::vec(0u16..1000,1..20), budget in 0u16..4000, attempts in 1usize..25) {
        let offers=costs.iter().enumerate().map(|(i,c)|offer(&i.to_string(),u64::from(*c))).collect::<Vec<_>>();
        let mut expected=costs.iter().enumerate().filter(|(_,c)|**c<=budget).map(|(i,c)|(i.to_string(),u64::from(*c))).collect::<Vec<_>>();
        expected.sort_by_key(|(_,c)|*c);
        let planned=plan(offers,u64::from(budget),attempts,Mode::Simulation);
        if expected.is_empty() {proptest::prop_assert_eq!(planned.unwrap_err(),Error::Unavailable);}
        else {
            let mut c=planned.unwrap().start(); let mut used=0;
            for (id,price) in expected.into_iter().take(attempts) {
                if price>u64::from(budget)-used {proptest::prop_assert_eq!(c.begin(11).unwrap_err(),Error::Budget);break;}
                let permit=c.begin(11).unwrap();
                proptest::prop_assert_eq!(permit.candidate(),&id);
                used+=price; c.complete(Outcome::SafeFailure,None).unwrap();
                proptest::prop_assert_eq!(c.exposure().atoms,used);
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn charge_violations_retain_every_known_fee_without_implicit_conversion() {
    let mut cascade = plan(vec![offer("a", 2), offer("b", 3)], 5, 2, Mode::Production)
        .unwrap()
        .start();
    assert!(cascade.charge_violation().is_none());
    cascade.begin(10).unwrap();
    cascade
        .complete(Outcome::SafeFailure, Some(amount(1)))
        .unwrap();
    cascade.begin(10).unwrap();
    assert_eq!(
        cascade.complete(Outcome::Success, Some(amount(4))),
        Err(Error::Charge)
    );
    let violation = cascade.charge_violation().unwrap();
    assert_eq!(violation.prior, amount(1));
    assert_eq!(violation.reserved, amount(3));
    assert_eq!(violation.reported, amount(4));
    assert_eq!(violation.clone(), *violation);
    assert!(!format!("{violation:?}").is_empty());
    assert_eq!(cascade.begin(10).unwrap_err(), Error::Finished);
    let mut cascade = plan(vec![offer("a", u64::MAX)], u64::MAX, 1, Mode::Production)
        .unwrap()
        .start();
    cascade.begin(10).unwrap();
    let mut foreign = amount(u64::MAX);
    foreign.unit.code = "OTHER".into();
    assert_eq!(
        cascade.complete(Outcome::Uncertain, Some(foreign.clone())),
        Err(Error::Charge)
    );
    assert_eq!(cascade.charge_violation().unwrap().reported, foreign);
}
