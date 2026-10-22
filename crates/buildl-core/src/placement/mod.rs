//! Placement: where every output lives, and what a command's placeholders stand for once they
//! are placed.
//!
//! Its own directory because it is shared, not a phase: Plan, Execute and verify each need an
//! output's path and a command expanded the same way, and Plan and Execute may not name each
//! other.
//!
//! Responsibilities: [`layout::Layout`], [`expand::expand`] and [`expand::expand_description`].
//!
//! Non-responsibilities: whether an expanded command names a program that may run, which is
//! Execute's; and putting anything at a path, which is a port's.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub(crate) mod expand;
pub(crate) mod layout;

#[cfg(test)]
mod fixtures;
