//! Property tests: the executor against an independent reference model.
#![cfg(not(target_arch = "wasm32"))]
use cfbk::{Candidate, Config, Fallback, Health, Switch, Unavailable};
use proptest::prelude::*;
use std::collections::BTreeSet;

const CANDIDATES: u8 = 6;
const PROPERTIES: u8 = 4;

#[derive(Debug, Clone)]
struct Scenario {
    /// Declared properties per candidate id (index = id).
    properties: Vec<BTreeSet<u8>>,
    order: Vec<u8>,
    minimum: BTreeSet<u8>,
    extra: BTreeSet<u8>,
    reports: Vec<(u8, bool)>,
}

fn property_set() -> impl Strategy<Value = BTreeSet<u8>> {
    // Each property independently present or absent (no rejection sampling).
    proptest::collection::vec(any::<bool>(), PROPERTIES as usize).prop_map(|present| {
        (0..PROPERTIES)
            .filter(|&property| present[property as usize])
            .collect()
    })
}

fn scenario() -> impl Strategy<Value = Scenario> {
    (
        proptest::collection::vec(property_set(), CANDIDATES as usize),
        Just((0..CANDIDATES).collect::<Vec<u8>>()).prop_shuffle(),
        0..=CANDIDATES as usize,
        property_set(),
        property_set(),
        proptest::collection::vec((0..CANDIDATES, any::<bool>()), 0..40),
    )
        .prop_map(
            |(properties, shuffled, length, minimum, extra, reports)| Scenario {
                properties,
                order: shuffled[..length].to_vec(),
                minimum,
                extra,
                reports,
            },
        )
}

fn build(scenario: &Scenario) -> Fallback<u8, u8> {
    let candidates = scenario
        .properties
        .iter()
        .enumerate()
        .map(|(id, properties)| Candidate::new(id as u8, properties.iter().copied()));
    Fallback::new(
        candidates,
        Config::new(
            scenario.order.iter().copied(),
            scenario.minimum.iter().copied(),
        ),
    )
    .expect("generated configurations are valid")
}

/// Reference: first ordered candidate that is healthy and meets the standard.
fn model_select(
    scenario: &Scenario,
    failed: &BTreeSet<u8>,
    extra: &BTreeSet<u8>,
) -> Result<u8, Unavailable> {
    let meets = |id: u8| {
        let properties = &scenario.properties[id as usize];
        scenario.minimum.is_subset(properties) && extra.is_subset(properties)
    };
    if let Some(&id) = scenario
        .order
        .iter()
        .find(|&&id| meets(id) && !failed.contains(&id))
    {
        return Ok(id);
    }
    if scenario.order.iter().any(|&id| meets(id)) {
        Err(Unavailable::AllAcceptableFailed)
    } else {
        Err(Unavailable::NoCandidateMeetsStandard)
    }
}

proptest! {
    #[test]
    fn matches_reference_model_after_every_report(scenario in scenario()) {
        let mut fallback = build(&scenario);
        let mut failed = BTreeSet::new();
        let none = BTreeSet::new();
        prop_assert_eq!(fallback.select().copied(), model_select(&scenario, &failed, &none));
        for &(id, healthy) in &scenario.reports {
            let before = model_select(&scenario, &failed, &none).ok();
            if healthy { failed.remove(&id); } else { failed.insert(id); }
            let after = model_select(&scenario, &failed, &none).ok();
            let health = if healthy { Health::Healthy } else { Health::Failed };
            let switch = fallback.report(&id, health).unwrap();
            // A switch is reported exactly when the active candidate changes.
            let expected = (before != after).then_some(Switch { from: before, to: after });
            prop_assert_eq!(switch, expected);
            prop_assert_eq!(fallback.select().copied(), model_select(&scenario, &failed, &none));
            prop_assert_eq!(
                fallback.select_meeting(&scenario.extra).copied(),
                model_select(&scenario, &failed, &scenario.extra)
            );
        }
    }

    #[test]
    fn never_selects_below_minimum_or_failed(scenario in scenario()) {
        let mut fallback = build(&scenario);
        for &(id, healthy) in &scenario.reports {
            let health = if healthy { Health::Healthy } else { Health::Failed };
            fallback.report(&id, health).unwrap();
            if let Ok(&selected) = fallback.select_meeting(&scenario.extra) {
                let properties = &scenario.properties[selected as usize];
                prop_assert!(scenario.minimum.is_subset(properties));
                prop_assert!(scenario.extra.is_subset(properties));
                prop_assert_eq!(fallback.health(&selected), Some(Health::Healthy));
                prop_assert!(scenario.order.contains(&selected));
            }
            for &usable in fallback.usable() {
                prop_assert!(fallback.is_acceptable(&usable));
                prop_assert_eq!(fallback.health(&usable), Some(Health::Healthy));
            }
        }
    }

    #[test]
    fn deterministic(scenario in scenario()) {
        let mut first = build(&scenario);
        let mut second = build(&scenario);
        for &(id, healthy) in &scenario.reports {
            let health = if healthy { Health::Healthy } else { Health::Failed };
            prop_assert_eq!(first.report(&id, health), second.report(&id, health));
            prop_assert_eq!(first.select(), second.select());
            prop_assert_eq!(
                first.usable().collect::<Vec<_>>(),
                second.usable().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn active_is_first_usable(scenario in scenario()) {
        let mut fallback = build(&scenario);
        for &(id, healthy) in &scenario.reports {
            let health = if healthy { Health::Healthy } else { Health::Failed };
            fallback.report(&id, health).unwrap();
            prop_assert_eq!(fallback.select().ok(), fallback.usable().next());
        }
    }

    #[test]
    fn excluded_is_exactly_ordered_below_minimum(scenario in scenario()) {
        let fallback = build(&scenario);
        let excluded: Vec<u8> = fallback.excluded().map(|(&id, _)| id).collect();
        let expected: Vec<u8> = scenario
            .order
            .iter()
            .copied()
            .filter(|&id| !scenario.minimum.is_subset(&scenario.properties[id as usize]))
            .collect();
        prop_assert_eq!(excluded, expected);
        for (&id, missing) in fallback.excluded() {
            prop_assert!(!missing.is_empty());
            prop_assert!(missing.is_disjoint(&scenario.properties[id as usize]));
            prop_assert!(missing.is_subset(&scenario.minimum));
        }
    }

    #[test]
    fn reset_heals_everything(scenario in scenario()) {
        let mut fallback = build(&scenario);
        for &(id, healthy) in &scenario.reports {
            let health = if healthy { Health::Healthy } else { Health::Failed };
            fallback.report(&id, health).unwrap();
        }
        fallback.reset();
        let none = BTreeSet::new();
        prop_assert_eq!(fallback.select().copied(), model_select(&scenario, &none, &none));
    }
}
