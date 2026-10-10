use super::{Amount, Error, Mode, Plan, Quote};
use std::fmt;

/// A trusted consumer's report of an attempt's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The requested task is complete; no fallback follows.
    Success,
    /// Definitive failure for which the adapter established safe fallback.
    SafeFailure,
    /// A terminal refusal, including failed authentication; no automatic bypass.
    Refused,
    /// Waiting for user/provider action. The reservation remains outstanding.
    Pending,
    /// Side effects may have happened. Reconcile before trying anything else.
    Uncertain,
}

/// Move-only permission for a bounded attempt. It is not a verified domain proof.
/// Obtain it through [`Cascade::begin`]; validate it at the actual dispatch edge.
#[derive(Debug)]
pub struct DispatchPermit<Id> {
    candidate: Id,
    quote: Quote,
    mode: Mode,
}

impl<Id> DispatchPermit<Id> {
    /// Selected provider/channel instance.
    pub fn candidate(&self) -> &Id {
        &self.candidate
    }
    /// Maximum authorized cost and its provenance/binding.
    pub fn quote(&self) -> &Quote {
        &self.quote
    }
    /// Mode cannot be changed by the adapter.
    pub fn mode(&self) -> Mode {
        self.mode
    }
    /// Check the independently derived request binding, actual execution mode
    /// and current time immediately before any potentially billable call.
    pub fn validate(&self, binding: &str, mode: Mode, now: u64) -> Result<(), Error> {
        if self.quote.binding != binding || self.mode != mode {
            return Err(Error::Binding);
        }
        if now >= self.quote.valid_until {
            return Err(Error::Expired);
        }
        Ok(())
    }
}

/// Deterministic state machine. It never calls a provider or reads a clock.
/// Dropping/reconstructing it is not a durable rollback mechanism: the host
/// must preserve uncertain operation and budget reservations across restarts.
#[derive(Debug)]
pub struct Cascade<Id, P> {
    plan: Plan<Id, P>,
    next: usize,
    begun: usize,
    charged: u64,
    pending: Option<u64>,
    finished: bool,
}

impl<Id: Clone + Eq + fmt::Debug, P: Ord + Clone + fmt::Debug> Cascade<Id, P> {
    pub(super) fn new(plan: Plan<Id, P>) -> Self {
        Self {
            plan,
            next: 0,
            begun: 0,
            charged: 0,
            pending: None,
            finished: false,
        }
    }

    /// Reserve maximum exposure before returning a dispatch permission.
    /// Only one attempt may be outstanding. Expiry causes replanning, not a
    /// silent switch to another quoted offer.
    pub fn begin(&mut self, now: u64) -> Result<DispatchPermit<Id>, Error> {
        if self.finished {
            return Err(Error::Finished);
        }
        if self.pending.is_some() {
            return Err(Error::Pending);
        }
        if self.begun >= self.plan.attempts {
            return Err(Error::Budget);
        }
        let offer = self.plan.offers.get(self.next).ok_or(Error::Finished)?;
        if now >= offer.quote.valid_until {
            return Err(Error::Expired);
        }
        let maximum = offer.quote.price.maximum().ok_or(Error::Unavailable)?.atoms;
        if maximum > self.plan.budget.atoms - self.charged {
            return Err(Error::Budget);
        }
        self.charged += maximum;
        self.pending = Some(maximum);
        self.next += 1;
        self.begun += 1;
        Ok(DispatchPermit {
            candidate: offer.candidate.id.clone(),
            quote: offer.quote.clone(),
            mode: self.plan.mode,
        })
    }

    /// Report actual cost only when authenticated provider evidence or the
    /// configured tariff establishes it. `None` retains the entire reservation.
    /// Pending/uncertain outcomes never release exposure, even if a preliminary
    /// cost is reported; final reconciliation can call this method again.
    pub fn complete(&mut self, outcome: Outcome, actual: Option<Amount>) -> Result<(), Error> {
        if self.finished {
            return Err(Error::Finished);
        }
        let reserved = self.pending.ok_or(Error::Finished)?;
        if let Some(cost) = &actual
            && (cost.unit != self.plan.budget.unit || cost.atoms > reserved)
        {
            self.finished = true;
            return Err(Error::Charge);
        }
        if matches!(outcome, Outcome::Pending | Outcome::Uncertain) {
            return Ok(());
        }
        if let Some(cost) = actual {
            self.charged -= reserved - cost.atoms;
        }
        self.pending = None;
        self.finished = matches!(outcome, Outcome::Success | Outcome::Refused);
        Ok(())
    }

    /// Total known charge plus conservatively reserved unknown exposure.
    pub fn exposure(&self) -> Amount {
        Amount {
            unit: self.plan.budget.unit.clone(),
            atoms: self.charged,
        }
    }
    /// Whether an outstanding attempt still blocks every fallback.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    /// Number of attempts for which exposure was reserved.
    pub fn attempts(&self) -> usize {
        self.begun
    }
}
