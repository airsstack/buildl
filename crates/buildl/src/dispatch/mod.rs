//! The worker threads a build runs its actions on.
//!
//! Its own module because starting a thread is something only this crate may do: the pipeline
//! says what to do with one task and with one result, through `buildl_core::Dispatcher`, and
//! this adapter decides how many tasks are worked on at a time.
//!
//! Responsibilities: [`ThreadPool`], the implementation of `buildl_core::Dispatcher`.
//!
//! Non-responsibilities: knowing what a task is, stopping a build at a failure, and cancelling
//! anything that runs. The pool runs every task it is handed and hands back every result; what a
//! result means for the rest of a build is the pipeline's to decide.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod thread_pool;

pub use thread_pool::ThreadPool;
