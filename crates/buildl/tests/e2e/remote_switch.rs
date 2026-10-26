//! The switch that turns the remote cache off for a command, under real threads.
//!
//! `buildl-core` runs no thread of its own, so the guarantee that matters to workers on several
//! threads, that exactly one of them is told it turned the remote off, is exercised here.

use std::sync::Barrier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use buildl::buildl_core::RemoteSwitch;

/// How many threads turn the switch off at once.
const THREADS: usize = 16;

/// How many fresh switches are raced.
const ROUNDS: usize = 200;

#[test]
fn exactly_one_of_many_threads_turning_the_remote_off_at_once_is_told_so() {
    for round in 0..ROUNDS {
        let switch = RemoteSwitch::new();
        let start = Barrier::new(THREADS);
        let told = AtomicUsize::new(0);

        thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    start.wait();
                    if switch.turn_off() {
                        told.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });

        assert_eq!(told.load(Ordering::SeqCst), 1, "round {round}");
        assert!(switch.is_off());
    }
}
