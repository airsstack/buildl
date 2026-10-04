//! A target's handle inside the graph's arenas.
//!
//! Its own file because it is the identity every phase after the graph is built uses in place of
//! a name: strings appear only at the edges, where something is parsed or reported.
//!
//! Responsibilities: [`NodeId`] and the one widening conversion slice addressing needs.
//!
//! Non-responsibilities: validity. An index is meaningful only against the arena that issued it,
//! so bounds are that arena's invariant and construction here cannot fail.

use serde::{Deserialize, Serialize};

/// An index into the target graph's arenas.
///
/// Construction is infallible: every `u32` is a legal index, and whether one is *in bounds* is a
/// property of the arena it indexes, not of this value. The newtype exists so the compiler
/// refuses to swap an index with a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(u32);

impl NodeId {
    /// Wraps an arena index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The index as declared.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The index widened for slice addressing.
    ///
    /// The one cast site, named rather than spelled `as usize` at each use.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::NodeId;

    #[test]
    fn carries_the_index_it_was_given() {
        assert_eq!(NodeId::new(0).get(), 0);
        assert_eq!(NodeId::new(7).get(), 7);
        assert_eq!(NodeId::new(u32::MAX).get(), u32::MAX);
    }

    #[test]
    fn widens_for_slice_addressing_without_loss() {
        assert_eq!(NodeId::new(7).index(), 7_usize);
        assert_eq!(NodeId::new(u32::MAX).index(), u32::MAX as usize);
    }

    #[test]
    fn serializes_as_a_bare_number() {
        let json = serde_json::to_string(&NodeId::new(7)).unwrap();
        assert_eq!(json, "7");
        assert_eq!(
            serde_json::from_str::<NodeId>(&json).unwrap(),
            NodeId::new(7)
        );
    }

    #[test]
    fn orders_numerically() {
        let mut ids = [NodeId::new(10), NodeId::new(2), NodeId::new(9)];
        ids.sort();
        assert_eq!(ids, [NodeId::new(2), NodeId::new(9), NodeId::new(10)]);
    }
}
