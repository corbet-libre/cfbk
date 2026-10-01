use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prop {
    Anonymous,
    Offline,
    Datagrams,
}

fn mesh() -> Fallback<&'static str, Prop> {
    Fallback::new(
        [
            Candidate::new("veilid", [Prop::Anonymous, Prop::Offline, Prop::Datagrams]),
            Candidate::new("tor", [Prop::Anonymous]),
            Candidate::new("direct", [Prop::Datagrams]),
        ],
        Config::new(["veilid", "tor", "direct"], [Prop::Anonymous]),
    )
    .unwrap()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selects_first_acceptable_in_order() {
    assert_eq!(mesh().select(), Ok(&"veilid"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn switches_on_failure_and_reports_it() {
    let mut fallback = mesh();
    let switch = fallback.report(&"veilid", Health::Failed).unwrap();
    assert_eq!(
        switch,
        Some(Switch {
            from: Some("veilid"),
            to: Some("tor")
        })
    );
    assert_eq!(fallback.select(), Ok(&"tor"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn never_downgrades_below_minimum() {
    let mut fallback = mesh();
    fallback.report(&"veilid", Health::Failed).unwrap();
    let switch = fallback.report(&"tor", Health::Failed).unwrap();
    assert_eq!(
        switch,
        Some(Switch {
            from: Some("tor"),
            to: None
        })
    );
    // "direct" is healthy but not anonymous: it must not be selected.
    assert_eq!(fallback.select(), Err(Unavailable::AllAcceptableFailed));
    assert!(!fallback.is_acceptable(&"direct"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn excluded_candidates_are_visible() {
    let fallback = mesh();
    let excluded: Vec<_> = fallback.excluded().collect();
    assert_eq!(
        excluded,
        vec![(&"direct", BTreeSet::from([Prop::Anonymous]))]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn recovery_is_explicit() {
    let mut fallback = mesh();
    fallback.report(&"veilid", Health::Failed).unwrap();
    assert_eq!(fallback.select(), Ok(&"tor"));
    let switch = fallback.report(&"veilid", Health::Healthy).unwrap();
    assert_eq!(
        switch,
        Some(Switch {
            from: Some("tor"),
            to: Some("veilid")
        })
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reports_on_inactive_candidates_do_not_switch() {
    let mut fallback = mesh();
    assert_eq!(fallback.report(&"tor", Health::Failed), Ok(None));
    assert_eq!(fallback.report(&"direct", Health::Failed), Ok(None));
    assert_eq!(fallback.select(), Ok(&"veilid"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stricter_per_use_requirement() {
    let mut fallback = mesh();
    let offline = BTreeSet::from([Prop::Offline]);
    assert_eq!(fallback.select_meeting(&offline), Ok(&"veilid"));
    fallback.report(&"veilid", Health::Failed).unwrap();
    assert_eq!(
        fallback.select_meeting(&offline),
        Err(Unavailable::AllAcceptableFailed)
    );
    // Datagrams: "direct" has them but is below the minimum standard.
    let datagrams = BTreeSet::from([Prop::Datagrams]);
    assert_eq!(
        fallback.select_meeting(&datagrams),
        Err(Unavailable::AllAcceptableFailed)
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn no_candidate_meets_standard() {
    let fallback: Fallback<&str, Prop> = Fallback::new(
        [Candidate::new("direct", [Prop::Datagrams])],
        Config::new(["direct"], [Prop::Anonymous]),
    )
    .unwrap();
    assert_eq!(
        fallback.select(),
        Err(Unavailable::NoCandidateMeetsStandard)
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unordered_candidates_are_never_selected() {
    let fallback: Fallback<&str, Prop> = Fallback::new(
        [
            Candidate::new("tor", [Prop::Anonymous]),
            Candidate::new("veilid", [Prop::Anonymous]),
        ],
        Config::new(["tor"], [Prop::Anonymous]),
    )
    .unwrap();
    assert_eq!(fallback.usable().collect::<Vec<_>>(), vec![&"tor"]);
    assert!(!fallback.is_acceptable(&"veilid"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn configuration_errors() {
    let duplicate = Fallback::<&str, Prop>::new(
        [Candidate::new("a", []), Candidate::new("a", [])],
        Config::new(["a"], []),
    );
    assert_eq!(duplicate.unwrap_err(), ConfigError::DuplicateCandidate("a"));
    let unknown = Fallback::<&str, Prop>::new([Candidate::new("a", [])], Config::new(["b"], []));
    assert_eq!(unknown.unwrap_err(), ConfigError::UnknownInOrder("b"));
    let twice = Fallback::<&str, Prop>::new([Candidate::new("a", [])], Config::new(["a", "a"], []));
    assert_eq!(twice.unwrap_err(), ConfigError::DuplicateInOrder("a"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unknown_report_is_rejected() {
    let mut fallback = mesh();
    assert_eq!(
        fallback.report(&"i2p", Health::Failed),
        Err(UnknownCandidate)
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reset_restores_preferred_candidate() {
    let mut fallback = mesh();
    fallback.report(&"veilid", Health::Failed).unwrap();
    fallback.report(&"tor", Health::Failed).unwrap();
    assert_eq!(
        fallback.reset(),
        Some(Switch {
            from: None,
            to: Some("veilid")
        })
    );
    assert_eq!(fallback.reset(), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn inspection_matches_the_configured_standard_and_reported_health() {
    let mut fallback = mesh();
    assert_eq!(fallback.minimum(), &BTreeSet::from([Prop::Anonymous]));
    assert_eq!(
        fallback.order().copied().collect::<Vec<_>>(),
        ["veilid", "tor", "direct"]
    );
    assert_eq!(
        fallback.properties(&"tor"),
        Some(&BTreeSet::from([Prop::Anonymous]))
    );
    assert_eq!(fallback.properties(&"unknown"), None);
    assert_eq!(fallback.health(&"tor"), Some(Health::Healthy));
    assert_eq!(fallback.health(&"unknown"), None);
    fallback.report(&"tor", Health::Failed).unwrap();
    assert_eq!(fallback.health(&"tor"), Some(Health::Failed));
    assert_eq!(fallback.usable().copied().collect::<Vec<_>>(), ["veilid"]);
    for error in [
        ConfigError::DuplicateCandidate("a"),
        ConfigError::DuplicateInOrder("a"),
        ConfigError::UnknownInOrder("a"),
    ] {
        assert!(error.to_string().contains('a'));
        assert!(std::error::Error::source(&error).is_none());
    }
    for error in [
        Unavailable::NoCandidateMeetsStandard,
        Unavailable::AllAcceptableFailed,
    ] {
        assert!(!error.to_string().is_empty());
        assert!(std::error::Error::source(&error).is_none());
    }
    assert!(!UnknownCandidate.to_string().is_empty());
    assert!(std::error::Error::source(&UnknownCandidate).is_none());
}
