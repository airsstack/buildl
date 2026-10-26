//! When a remote call is tried again, and how long to wait first.
//!
//! A call that fails with a status the cache may recover from is made again after a pause that
//! doubles each time; any other failure, and the last of the allowed retries, ends the call at
//! once. A stream that delivers messages is moving, so a try that recorded a message on its
//! [`Progress`] starts the count over: only a stream that stops uses its retries up.

use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use buildl_core::RetryCount;
use tonic::Code;

use super::client::Progress;

/// The pause before the first retry; each later one doubles it.
const BASE_DELAY: Duration = Duration::from_millis(100);

/// The longest pause before a retry, before the random part is added.
const MAX_DELAY: Duration = Duration::from_secs(5);

/// The random part added to a pause is below this many milliseconds.
const JITTER_MILLIS: u64 = 50;

/// A failure that may carry the status a cache answered with.
pub(super) trait Failure {
    /// The status code of the failure, `None` for a failure that is not an answer of the cache.
    fn code(&self) -> Option<Code>;
}

/// A call that failed for good: the error of the last try and how many tries were made.
#[derive(Debug)]
pub(super) struct Failed<E> {
    /// The error of the last try.
    pub(super) error: E,
    /// The number of times the call was made.
    pub(super) calls: u32,
}

/// How often and how patiently a failed remote call is made again.
#[derive(Debug, Clone, Copy)]
pub(super) struct Retry {
    retries: u8,
    base: Duration,
    jitter_millis: u64,
}

impl Retry {
    /// The rule that allows `count` retries after the first call.
    pub(super) const fn new(count: RetryCount) -> Self {
        Self {
            retries: count.get(),
            base: BASE_DELAY,
            jitter_millis: JITTER_MILLIS,
        }
    }

    /// The rule of `count` retries that never waits, so tests take no real time.
    #[cfg(test)]
    pub(super) const fn without_delay(count: RetryCount) -> Self {
        Self {
            retries: count.get(),
            base: Duration::ZERO,
            jitter_millis: 0,
        }
    }

    /// Whether a failure with `code` is one the cache may recover from.
    pub(super) const fn transient(code: Code) -> bool {
        matches!(
            code,
            Code::Unavailable | Code::ResourceExhausted | Code::Aborted | Code::Cancelled
        )
    }

    /// The pause before the `n`-th retry, counting from 1, without the random part.
    fn backoff(&self, n: u32) -> Duration {
        let doublings = 2u32.saturating_pow(n.saturating_sub(1));
        self.base.saturating_mul(doublings).min(MAX_DELAY)
    }

    /// A random part to add to a pause, so callers that failed together do not retry together.
    fn jitter(&self) -> Duration {
        if self.jitter_millis == 0 {
            return Duration::ZERO;
        }
        // Every `RandomState` is seeded differently, so a fresh one is a cheap source of
        // unpredictable numbers that needs no crate.
        let draw = RandomState::new().build_hasher().finish();
        Duration::from_millis(draw % self.jitter_millis)
    }

    /// The pause before the `n`-th retry, counting from 1.
    fn delay(&self, n: u32) -> Duration {
        self.backoff(n) + self.jitter()
    }

    /// Calls `op` until it succeeds, fails for good, or has been retried as often as allowed.
    ///
    /// A failure is retried when its code is [`transient`](Self::transient); the pause before
    /// the `n`-th retry is [`delay`](Self::delay). Each try of `op` is handed a fresh
    /// [`Progress`] to record each message of a stream on; a try that fails after recording
    /// leaves all the retries to come.
    /// Must be polled by the remote runtime, as the pauses use its timer.
    ///
    /// # Errors
    ///
    /// Returns the last error with the number of calls made.
    pub(super) async fn run<T, E, Fut>(
        &self,
        mut op: impl FnMut(Progress) -> Fut,
    ) -> Result<T, Failed<E>>
    where
        Fut: Future<Output = Result<T, E>>,
        E: Failure,
    {
        let mut calls = 0u32;
        let mut retried = 0u8;
        loop {
            calls = calls.saturating_add(1);
            let progress = Progress::start();
            let started = progress.last();
            let error = match op(progress.clone()).await {
                Ok(answer) => return Ok(answer),
                Err(error) => error,
            };
            if progress.last() > started {
                retried = 0;
            }
            let transient = error.code().is_some_and(Self::transient);
            if !transient || retried >= self.retries {
                return Err(Failed { error, calls });
            }
            retried += 1;
            tokio::time::sleep(self.delay(u32::from(retried))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::cell::Cell;
    use std::time::{Duration, Instant};

    use buildl_core::RetryCount;
    use tonic::{Code, Status};

    use super::{Failure, Retry};
    use crate::remote::runtime::Runtime;

    impl Failure for Status {
        fn code(&self) -> Option<Code> {
            Some(Self::code(self))
        }
    }

    fn count(n: i64) -> RetryCount {
        RetryCount::from_integer(n).unwrap()
    }

    /// Runs `retry` over an op that fails with `code` every time and reports how often it was
    /// called.
    fn calls_until_given_up(retry: &Retry, code: Code) -> u32 {
        let calls = Cell::new(0u32);
        let outcome = Runtime::new().unwrap().block_on(retry.run(|_| {
            calls.set(calls.get() + 1);
            async { Err::<(), _>(Status::new(code, "no")) }
        }));
        let failed = outcome.unwrap_err();
        assert_eq!(failed.error.code(), code);
        assert_eq!(failed.calls, calls.get());
        calls.get()
    }

    #[test]
    fn exactly_four_codes_are_transient() {
        for code in [
            Code::Unavailable,
            Code::ResourceExhausted,
            Code::Aborted,
            Code::Cancelled,
        ] {
            assert!(Retry::transient(code), "{code:?}");
        }
        for code in [
            Code::Internal,
            Code::Unknown,
            Code::NotFound,
            Code::PermissionDenied,
            Code::Unauthenticated,
            Code::InvalidArgument,
            Code::FailedPrecondition,
            Code::DeadlineExceeded,
        ] {
            assert!(!Retry::transient(code), "{code:?}");
        }
    }

    #[test]
    fn delays_double_from_100ms_up_to_5s() {
        let retry = Retry::new(count(10));
        let waits: Vec<_> = (1..=7).map(|n| retry.backoff(n)).collect();
        let expected = [100, 200, 400, 800, 1600, 3200, 5000].map(Duration::from_millis);
        assert_eq!(waits, expected);
    }

    #[test]
    fn the_random_part_is_under_50ms() {
        let retry = Retry::new(count(5));
        let draws: Vec<_> = (0..1000).map(|_| retry.jitter()).collect();
        assert!(draws.iter().all(|extra| *extra < Duration::from_millis(50)));
        assert!(draws.iter().any(|extra| *extra != draws[0]), "never varies");
        let delay = retry.delay(1);
        assert!(delay >= Duration::from_millis(100) && delay < Duration::from_millis(150));
    }

    #[test]
    fn five_retries_make_at_most_six_calls() {
        let retry = Retry::without_delay(count(5));
        assert_eq!(calls_until_given_up(&retry, Code::Unavailable), 6);
    }

    #[test]
    fn zero_retries_make_one_call() {
        let retry = Retry::without_delay(count(0));
        assert_eq!(calls_until_given_up(&retry, Code::Unavailable), 1);
    }

    #[test]
    fn a_permanent_failure_is_not_retried() {
        let retry = Retry::without_delay(count(5));
        assert_eq!(calls_until_given_up(&retry, Code::PermissionDenied), 1);
    }

    #[test]
    fn a_transient_failure_then_success_succeeds() {
        let retry = Retry::without_delay(count(5));
        let calls = Cell::new(0u32);
        let answer = Runtime::new().unwrap().block_on(retry.run(|_| {
            calls.set(calls.get() + 1);
            let fail = calls.get() < 3;
            async move {
                if fail {
                    Err(Status::unavailable("busy"))
                } else {
                    Ok(7)
                }
            }
        }));
        assert_eq!(answer.unwrap(), 7);
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn progress_resets_the_count() {
        let retry = Retry::without_delay(count(2));
        let calls = Cell::new(0u32);
        let outcome = Runtime::new().unwrap().block_on(retry.run(|progress| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                // The clock must tick between the start of the try and the message.
                std::thread::sleep(Duration::from_millis(2));
                progress.record();
            }
            async { Err::<(), _>(Status::unavailable("busy")) }
        }));
        // Without the progress at call 2 the rule would stop at 3 calls; after it, two more
        // retries are allowed.
        assert_eq!(calls.get(), 4);
        assert_eq!(outcome.unwrap_err().calls, 4);
    }

    #[test]
    fn each_retry_waits_out_its_pause() {
        let retry = Retry {
            retries: 3,
            base: Duration::from_millis(1),
            jitter_millis: 0,
        };
        let began = Instant::now();
        let calls = calls_until_given_up(&retry, Code::Unavailable);
        // Pauses of 1, 2 and 4 ms come before the three retries.
        assert_eq!(calls, 4);
        assert!(
            began.elapsed() >= Duration::from_millis(7),
            "{:?}",
            began.elapsed()
        );
    }
}
