//! The pipeline: the phases run in sequence, each command a prefix of that sequence.
//!
//! Its own directory because it is the only module that names more than one phase: the phases
//! communicate through values, and the pipeline is what passes each one's output to the next.
//!
//! Responsibilities: [`Pipeline`]; the functions [`check`], [`graph`], [`query`],
//! [`doctor`](fn@doctor), [`log`](fn@log) and [`gc`](fn@gc); [`Shown`], the result of `log`;
//! and [`Collected`], the result of `gc`.
//!
//! Non-responsibilities: any phase's logic, and any port's implementation.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod doctor;
pub mod driver;
pub mod gc;
pub mod log;

pub use doctor::doctor;
pub use driver::{Pipeline, check, graph, query};
pub use gc::{Collected, gc};
pub use log::{Shown, log};
