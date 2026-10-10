//! Resolve, the second phase: from the declarations of every build file to the target graph.
//!
//! Its own directory because it is a phase, and each phase has one home that names no other.
//!
//! Responsibilities: the parts the phase is assembled from. Nothing is exported yet.
//!
//! This file holds only module declarations, so it carries no logic to unit-test.

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "called by the function that builds the graph, which is not written yet"
    )
)]
mod cycle;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "called by the function that builds the graph, which is not written yet"
    )
)]
mod suggest;
