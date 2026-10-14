//! The host wall clock, behind the port a build reads the time through.
//!
//! Its own module because reading the time is the one thing here that no test can pin: a build
//! reads it only to stamp when an action started and finished, and nothing a build stores or
//! compares depends on the value. Keeping the read in one adapter is what lets every other
//! module stay free of it.
//!
//! Responsibilities: [`SystemClock`], the implementation of `buildl_core::Clock`.
//!
//! Non-responsibilities: measuring how long an action may run. That is a monotonic reading the
//! module that runs actions takes for itself; it reaches no key, file or log.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod system_clock;

pub use system_clock::SystemClock;
