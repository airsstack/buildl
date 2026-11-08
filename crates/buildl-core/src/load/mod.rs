//! Load, the first phase: from a workspace root to the validated declarations of every build file
//! it reaches.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: [`load`]; running one build file through the source and validation, as a
//! value; deciding from the results, in any arrival order, what to evaluate next and what the
//! load came to; and refusing, once declarations are loaded, a command or a discovery output that
//! holds one of this machine's folders (`machine_paths`).
//!
//! Non-responsibilities: evaluating a build file, which is the
//! [`DeclarationSource`](crate::ports::DeclarationSource) port's; and relationships between
//! declarations, which belong to the phase that builds the graph.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod evaluation;
mod loading;
pub(crate) mod machine_paths;
pub mod traversal;

pub use traversal::load;

pub(crate) use traversal::declaration_order;
