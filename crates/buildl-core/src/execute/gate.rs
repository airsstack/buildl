//! A counting gate: at most a fixed number of runs in flight at once.
//!
//! Its own file because the bound on remote executions and the bound on local runs are the same
//! mechanism with two counts, and neither belongs to the worker that waits on it.
//!
//! Responsibilities: [`Gate`], which admits a fixed number of holders and makes the rest wait,
//! and [`Place`], the guard that frees a holder's place when it is dropped.
//!
//! Non-responsibilities: choosing the counts, and deciding what a worker does after the wait,
//! which the worker does.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// Admits at most a fixed number of holders at once; the others wait in [`Gate::enter`].
#[derive(Debug)]
pub(crate) struct Gate {
    free: Mutex<usize>,
    freed: Condvar,
}

/// A held place at a [`Gate`]; dropping it frees the place for a waiting thread.
#[derive(Debug)]
pub(crate) struct Place<'gate> {
    gate: &'gate Gate,
}

impl Gate {
    /// A gate that admits at most `places` holders at once.
    ///
    /// A gate of no places is made a gate of one, so that `enter` never waits forever.
    pub(crate) fn new(places: usize) -> Self {
        Self {
            free: Mutex::new(places.max(1)),
            freed: Condvar::new(),
        }
    }

    /// Waits until a place is free, takes it and returns the guard that frees it on drop.
    pub(crate) fn enter(&self) -> Place<'_> {
        let mut free = self.places();
        while *free == 0 {
            free = self
                .freed
                .wait(free)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *free -= 1;
        drop(free);
        Place { gate: self }
    }

    /// The places nobody holds.
    #[cfg(test)]
    pub(crate) fn free(&self) -> usize {
        *self.places()
    }

    /// The count of free places. The lock is held only while the count changes, never while a
    /// place is used, so even a poisoned lock holds a whole count.
    fn places(&self) -> MutexGuard<'_, usize> {
        self.free.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        *self.gate.places() += 1;
        self.gate.freed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::Gate;

    #[test]
    fn a_gate_admits_its_count_at_once() {
        let gate = Gate::new(3);
        let first = gate.enter();
        let second = gate.enter();
        assert_eq!(gate.free(), 1, "one place is left after two holders");
        let third = gate.enter();
        assert_eq!(gate.free(), 0, "a fourth holder would have to wait");
        drop((first, second, third));
        assert_eq!(
            gate.free(),
            3,
            "every place is freed once its holder leaves"
        );
    }

    #[test]
    fn a_place_is_freed_when_its_holder_leaves() {
        let gate = Gate::new(1);
        let first = gate.enter();
        assert_eq!(gate.free(), 0);
        drop(first);
        assert_eq!(gate.free(), 1);
        let _second = gate.enter();
        assert_eq!(gate.free(), 0);
    }

    #[test]
    fn a_gate_of_one_serializes() {
        let gate = Gate::new(1);
        let holder = gate.enter();
        assert_eq!(gate.free(), 0, "no place is left while the one is held");
        drop(holder);
        let _next = gate.enter();
        assert_eq!(gate.free(), 0);
    }

    #[test]
    fn a_gate_of_no_places_admits_one() {
        let gate = Gate::new(0);
        let _only = gate.enter();
        assert_eq!(gate.free(), 0);
    }
}
