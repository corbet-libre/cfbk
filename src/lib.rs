//! Ordered fallback executor.
//!
//! `cfbk` executes a fallback decision that someone else made: a configured
//! **order** of candidates and a **minimum standard** (a set of properties every
//! selectable candidate must declare). The consumer reports health; [`Fallback`]
//! answers which candidate is active.
//!
//! Guarantees:
//!
//! - **No silent downgrade.** A candidate that lacks any property of the
//!   minimum standard is never selected, whatever fails. When nothing
//!   acceptable is left, selection fails with [`Unavailable`] instead.
//! - **Deterministic.** The answer depends only on the configuration and the
//!   reported health. There is no clock, randomness or I/O.
//! - **One active candidate.** The active candidate is the first candidate in
//!   the configured order that meets the standard and is not reported failed.
//!   A report that changes it returns a [`Switch`], so a change is never silent.
//!
//! Properties are opaque to `cfbk`: any ordered type works. Ordered qualities
//! such as a latency class are expressed by the consumer as monotone flags
//! (for example "latency at most interactive"), so the standard stays a plain
//! set inclusion that is easy to audit.
//!
//! Recovery is explicit: a failed candidate becomes selectable again only after
//! the consumer reports it healthy (or calls [`Fallback::reset`]). Once it is
//! healthy and earlier in the order, it is active again; the consumer decides
//! when that is appropriate by choosing when to report recovery.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt;

/// A candidate with the properties it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<Id, P> {
    /// Identifier, unique within one [`Fallback`].
    pub id: Id,
    /// Declared properties.
    pub properties: BTreeSet<P>,
}

impl<Id, P: Ord> Candidate<Id, P> {
    /// Declare a candidate with its properties.
    pub fn new(id: Id, properties: impl IntoIterator<Item = P>) -> Self {
        Self {
            id,
            properties: properties.into_iter().collect(),
        }
    }
}

/// The decision being executed: which candidates, in which order, and the
/// minimum standard every selected candidate must meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config<Id, P> {
    /// Preference order. Declared candidates missing here are never selected.
    pub order: Vec<Id>,
    /// Properties every selected candidate must declare.
    pub minimum: BTreeSet<P>,
}

impl<Id, P: Ord> Config<Id, P> {
    /// Build a configuration from an order and a minimum standard.
    pub fn new(order: impl IntoIterator<Item = Id>, minimum: impl IntoIterator<Item = P>) -> Self {
        Self {
            order: order.into_iter().collect(),
            minimum: minimum.into_iter().collect(),
        }
    }
}

/// Health as reported by the consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Usable (the initial state of every candidate).
    Healthy,
    /// Reported failed; never selected until reported healthy again.
    Failed,
}

/// A change of the active candidate caused by a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Switch<Id> {
    /// Active candidate before the report (`None`: nothing was selectable).
    pub from: Option<Id>,
    /// Active candidate after the report (`None`: nothing is selectable).
    pub to: Option<Id>,
}

/// Why nothing can be selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// No ordered candidate meets the required standard. This is a
    /// configuration fact and does not change with health reports.
    NoCandidateMeetsStandard,
    /// Candidates meet the standard, but all of them are reported failed.
    AllAcceptableFailed,
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoCandidateMeetsStandard => "no candidate meets the required standard",
            Self::AllAcceptableFailed => "every acceptable candidate is reported failed",
        })
    }
}

impl std::error::Error for Unavailable {}

/// Invalid configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError<Id> {
    /// Two candidates share an identifier.
    DuplicateCandidate(Id),
    /// The order names an identifier twice.
    DuplicateInOrder(Id),
    /// The order names an identifier that was not declared.
    UnknownInOrder(Id),
}

impl<Id: fmt::Debug> fmt::Display for ConfigError<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCandidate(id) => write!(f, "candidate {id:?} declared twice"),
            Self::DuplicateInOrder(id) => write!(f, "candidate {id:?} appears twice in the order"),
            Self::UnknownInOrder(id) => write!(f, "order names undeclared candidate {id:?}"),
        }
    }
}

impl<Id: fmt::Debug> std::error::Error for ConfigError<Id> {}

/// A report named a candidate that was never declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownCandidate;

impl fmt::Display for UnknownCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("report names an undeclared candidate")
    }
}

impl std::error::Error for UnknownCandidate {}

#[derive(Debug, Clone)]
struct Entry<Id, P> {
    candidate: Candidate<Id, P>,
    health: Health,
}

/// Executes one configured order under one minimum standard.
#[derive(Debug, Clone)]
pub struct Fallback<Id, P> {
    /// Every declared candidate, in declaration order.
    entries: Vec<Entry<Id, P>>,
    /// Indices into `entries`, in preference order.
    order: Vec<usize>,
    minimum: BTreeSet<P>,
}

impl<Id, P> Fallback<Id, P>
where
    Id: Clone + Eq + fmt::Debug,
    P: Ord + Clone + fmt::Debug,
{
    /// Validate the configuration against the declared candidates.
    ///
    /// A candidate in the order that does not meet the minimum standard is
    /// accepted but never selected; [`Fallback::excluded`] lists it, so the
    /// configuration mistake stays visible.
    pub fn new(
        candidates: impl IntoIterator<Item = Candidate<Id, P>>,
        config: Config<Id, P>,
    ) -> Result<Self, ConfigError<Id>> {
        let mut entries: Vec<Entry<Id, P>> = Vec::new();
        for candidate in candidates {
            if entries
                .iter()
                .any(|entry| entry.candidate.id == candidate.id)
            {
                return Err(ConfigError::DuplicateCandidate(candidate.id));
            }
            entries.push(Entry {
                candidate,
                health: Health::Healthy,
            });
        }
        let mut order = Vec::with_capacity(config.order.len());
        for id in config.order {
            let index = entries
                .iter()
                .position(|entry| entry.candidate.id == id)
                .ok_or_else(|| ConfigError::UnknownInOrder(id.clone()))?;
            if order.contains(&index) {
                return Err(ConfigError::DuplicateInOrder(id));
            }
            order.push(index);
        }
        Ok(Self {
            entries,
            order,
            minimum: config.minimum,
        })
    }

    /// The minimum standard.
    pub fn minimum(&self) -> &BTreeSet<P> {
        &self.minimum
    }

    /// The configured preference order.
    pub fn order(&self) -> impl Iterator<Item = &Id> {
        self.order
            .iter()
            .map(|&index| &self.entries[index].candidate.id)
    }

    /// Declared properties of a candidate.
    pub fn properties(&self, id: &Id) -> Option<&BTreeSet<P>> {
        self.entry(id).map(|entry| &entry.candidate.properties)
    }

    /// Reported health of a candidate.
    pub fn health(&self, id: &Id) -> Option<Health> {
        self.entry(id).map(|entry| entry.health)
    }

    /// Whether a candidate is in the order and meets the minimum standard.
    pub fn is_acceptable(&self, id: &Id) -> bool {
        self.order
            .iter()
            .map(|&index| &self.entries[index].candidate)
            .any(|candidate| candidate.id == *id && self.meets(candidate, None))
    }

    /// Ordered candidates below the minimum standard, with the properties they
    /// lack. They are never selected.
    pub fn excluded(&self) -> impl Iterator<Item = (&Id, BTreeSet<P>)> {
        self.order.iter().filter_map(|&index| {
            let candidate = &self.entries[index].candidate;
            let missing: BTreeSet<P> = self
                .minimum
                .difference(&candidate.properties)
                .cloned()
                .collect();
            (!missing.is_empty()).then_some((&candidate.id, missing))
        })
    }

    /// The active candidate: the first in order that meets the minimum
    /// standard and is not reported failed.
    pub fn select(&self) -> Result<&Id, Unavailable> {
        self.select_meeting(&BTreeSet::new())
    }

    /// Like [`Fallback::select`], but the candidate must also declare every
    /// property in `extra` (a stricter, per-use requirement). The minimum
    /// standard always applies; `extra` can only raise it.
    pub fn select_meeting(&self, extra: &BTreeSet<P>) -> Result<&Id, Unavailable> {
        if let Some(id) = self.usable_meeting(extra).next() {
            return Ok(id);
        }
        let any_acceptable = self
            .order
            .iter()
            .any(|&index| self.meets(&self.entries[index].candidate, Some(extra)));
        Err(if any_acceptable {
            Unavailable::AllAcceptableFailed
        } else {
            Unavailable::NoCandidateMeetsStandard
        })
    }

    /// Every currently selectable candidate, in preference order. The first
    /// item is the active candidate.
    pub fn usable(&self) -> impl Iterator<Item = &Id> {
        self.order.iter().filter_map(move |&index| {
            let entry = &self.entries[index];
            (entry.health == Health::Healthy && self.meets(&entry.candidate, None))
                .then_some(&entry.candidate.id)
        })
    }

    /// Every currently selectable candidate that also declares `extra`, in
    /// preference order.
    pub fn usable_meeting<'a>(&'a self, extra: &BTreeSet<P>) -> impl Iterator<Item = &'a Id> {
        self.order.iter().filter_map(move |&index| {
            let entry = &self.entries[index];
            (entry.health == Health::Healthy && self.meets(&entry.candidate, Some(extra)))
                .then_some(&entry.candidate.id)
        })
    }

    /// Record a health report. Returns the resulting switch of the active
    /// candidate, if the report changed it.
    pub fn report(
        &mut self,
        id: &Id,
        health: Health,
    ) -> Result<Option<Switch<Id>>, UnknownCandidate> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.candidate.id == *id)
            .ok_or(UnknownCandidate)?;
        let before = self.select().ok().cloned();
        self.entries[index].health = health;
        Ok(self.switch_from(before))
    }

    /// Report every candidate healthy again.
    pub fn reset(&mut self) -> Option<Switch<Id>> {
        let before = self.select().ok().cloned();
        for entry in &mut self.entries {
            entry.health = Health::Healthy;
        }
        self.switch_from(before)
    }

    fn switch_from(&self, before: Option<Id>) -> Option<Switch<Id>> {
        let after = self.select().ok().cloned();
        (before != after).then_some(Switch {
            from: before,
            to: after,
        })
    }

    fn meets(&self, candidate: &Candidate<Id, P>, extra: Option<&BTreeSet<P>>) -> bool {
        self.minimum.is_subset(&candidate.properties)
            && extra.is_none_or(|extra| extra.is_subset(&candidate.properties))
    }

    fn entry(&self, id: &Id) -> Option<&Entry<Id, P>> {
        self.entries.iter().find(|entry| entry.candidate.id == *id)
    }
}

#[cfg(test)]
mod tests;
