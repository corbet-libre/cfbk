//! Costed fallback planning, without I/O or a clock.
//!
//! Consumers supply authenticated quotes or explicitly configured tariffs. A
//! quote's binding must cover provider, account, mode, operation and request
//! terms. The executing domain checks that binding before any side effect.
//! Persistent operation identity and cross-process budgets belong to the host;
//! this module only accounts for one in-memory cascade.

use crate::{Candidate, Config, Fallback};
use std::fmt;

mod execution;
pub use execution::{Cascade, ChargeViolation, DispatchPermit, Outcome};

/// Explicit denomination. Different scales are never implicitly converted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// Currency or another documented cost unit, for example `USD`.
    pub code: String,
    /// Number of decimal places represented by one integer atom.
    pub scale: u8,
}

/// Exact integer quantity. Payment principal is separate from service cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amount {
    /// Unit and scale, identical throughout a cascade.
    pub unit: Unit,
    /// Nonnegative count of the smallest declared unit.
    pub atoms: u64,
}

impl Amount {
    fn valid(&self) -> bool {
        !self.unit.code.is_empty()
            && self.unit.code.len() <= 64
            && self.unit.code.bytes().all(|b| b.is_ascii_graphic())
            && self.unit.scale <= 18
    }
}

/// What is known before dispatch. Unknown never means free.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Price {
    /// Exact known charge.
    Exact(Amount),
    /// Defensible maximum charge, including possible failed-attempt costs.
    AtMost(Amount),
    /// No usable bound; never authorizes a real or simulated dispatch.
    Unknown,
}

impl Price {
    /// Known maximum exposure, if available.
    pub fn maximum(&self) -> Option<&Amount> {
        match self {
            Self::Exact(value) | Self::AtMost(value) => Some(value),
            Self::Unknown => None,
        }
    }
}

/// Caller-supplied quote or tariff evaluated for the exact request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    /// Exact cost, upper bound, or explicitly unknown.
    pub price: Price,
    /// Price provenance, such as a provider quote or configured tariff revision.
    pub source: String,
    /// Exclusive expiry, using the caller's clock convention.
    pub valid_until: u64,
    /// Opaque digest binding provider/account/mode/operation/request terms.
    /// Never put raw member data here.
    pub binding: String,
}

/// Execution modes are distinct, including when the quoted cost is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Deterministic local fixtures only. No provider I/O or production proof.
    Simulation,
    /// Explicit provider sandbox. It is not presumed free.
    Sandbox,
    /// Explicit production execution.
    Production,
}

/// A provider/channel candidate and its request-specific price.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer<Id, P> {
    /// Identity and capabilities declared by the adapter.
    pub candidate: Candidate<Id, P>,
    /// Quote covering the actual dispatch, including provider-managed fallback.
    pub quote: Quote,
}

/// Caller-selected ordering policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostOrder {
    /// Respect the supplied candidate order.
    Configured,
    /// Lowest maximum cost first; configured order breaks equal-cost ties.
    Cheapest,
}

/// Why an offer cannot appear in an executable plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    /// Lacks a required capability.
    Capability,
    /// Missing, malformed or expired price information.
    Quote,
    /// Cost denomination differs from the caller's budget.
    Unit,
    /// One attempt alone exceeds the whole action budget.
    Budget,
}

/// Explicit failure or stop. Errors contain no provider/member payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Malformed policy, duplicate candidate, or invalid amount.
    Configuration,
    /// No configured candidate has a usable bounded cost and capability.
    Unavailable,
    /// An attempt is already pending or its outcome remains uncertain.
    Pending,
    /// This cascade has completed or stopped.
    Finished,
    /// No further attempt fits the remaining budget or attempt count.
    Budget,
    /// The next quote expired before dispatch.
    Expired,
    /// Dispatch context does not match the planned mode or request.
    Binding,
    /// Actual charge exceeded the advertised bound or used another unit.
    Charge,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "costed fallback refused: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Inspectable, inert plan. Constructing it never contacts a provider.
#[derive(Clone, Debug)]
pub struct Plan<Id, P> {
    offers: Vec<Offer<Id, P>>,
    excluded: Vec<(Id, Exclusion)>,
    budget: Amount,
    attempts: usize,
    mode: Mode,
}

impl<Id: Clone + Eq + fmt::Debug, P: Ord + Clone + fmt::Debug> Plan<Id, P> {
    /// Plan explicitly enabled offers against capabilities, quotes and budget.
    /// The budget is for the entire cascade, not each candidate separately.
    pub fn new(
        offers: Vec<Offer<Id, P>>,
        config: Config<Id, P>,
        order: CostOrder,
        budget: Amount,
        max_attempts: usize,
        mode: Mode,
        now: u64,
    ) -> Result<Self, Error> {
        if !budget.valid() || max_attempts == 0 {
            return Err(Error::Configuration);
        }
        let fallback = Fallback::new(offers.iter().map(|o| o.candidate.clone()), config)
            .map_err(|_| Error::Configuration)?;
        let mut selected = Vec::new();
        let mut excluded = Vec::new();
        for id in fallback.order() {
            // Fallback validation guarantees the configured candidate exists.
            let offer = offers
                .iter()
                .find(|o| &o.candidate.id == id)
                .ok_or(Error::Configuration)?;
            let reason = if !fallback.is_acceptable(id) {
                Some(Exclusion::Capability)
            } else {
                match offer.quote.price.maximum() {
                    None => Some(Exclusion::Quote),
                    Some(cost)
                        if !cost.valid()
                            || offer.quote.source.is_empty()
                            || offer.quote.binding.is_empty()
                            || offer.quote.valid_until <= now =>
                    {
                        Some(Exclusion::Quote)
                    }
                    Some(cost) if cost.unit != budget.unit => Some(Exclusion::Unit),
                    Some(cost) if cost.atoms > budget.atoms => Some(Exclusion::Budget),
                    Some(_) => None,
                }
            };
            if let Some(reason) = reason {
                excluded.push((id.clone(), reason));
            } else {
                selected.push(offer.clone());
            }
        }
        if selected.is_empty() {
            return Err(Error::Unavailable);
        }
        if order == CostOrder::Cheapest {
            selected.sort_by_key(|o| o.quote.price.maximum().map(|c| c.atoms));
        }
        Ok(Self {
            offers: selected,
            excluded,
            budget,
            attempts: max_attempts,
            mode,
        })
    }

    /// Eligible offers, in the precise order execution will follow.
    pub fn offers(&self) -> &[Offer<Id, P>] {
        &self.offers
    }
    /// Configured offers excluded before execution, with public reason codes.
    pub fn excluded(&self) -> &[(Id, Exclusion)] {
        &self.excluded
    }
    /// Total authorized upper bound; actual dispatches also consume this budget.
    pub fn budget(&self) -> &Amount {
        &self.budget
    }
    /// Maximum number of attempts authorized by this plan.
    pub fn max_attempts(&self) -> usize {
        self.attempts
    }
    /// Explicit mode used by dispatch permits.
    pub fn mode(&self) -> Mode {
        self.mode
    }
    /// Start the pure in-memory execution state machine. Host state must prevent
    /// restarting a spent operation or sharing an unreserved external budget.
    pub fn start(self) -> Cascade<Id, P> {
        Cascade::new(self)
    }
}

#[cfg(test)]
mod tests;
