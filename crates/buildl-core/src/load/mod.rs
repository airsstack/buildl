//! Load, the first phase: from a workspace root to the validated declarations of every build file
//! it reaches.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: [`load`].
//!
//! Non-responsibilities: evaluating a build file, which is the
//! [`DeclarationSource`](crate::ports::DeclarationSource) port's; and relationships between
//! declarations, which belong to the phase that builds the graph.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod traversal;

pub use traversal::load;

pub(crate) use traversal::declaration_order;
