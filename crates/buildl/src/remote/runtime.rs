//! The tokio runtime the remote adapter owns.
//!
//! The adapter's callers are plain threads. A caller drives a gRPC call to completion with
//! [`Runtime::block_on`], and any number of threads may do so at once: each polls its own
//! future on its own thread, while the runtime's workers carry the connection's I/O. Nothing
//! outside this module sees tokio.

use std::future::Future;
use std::io;

/// Worker threads of the runtime. They only carry the channel's connection tasks, since the
/// calling threads poll their own futures, so a few are enough however many callers there are.
const WORKER_THREADS: usize = 4;

/// A multi-thread tokio runtime that plain threads block on.
#[derive(Debug)]
pub(crate) struct Runtime {
    inner: tokio::runtime::Runtime,
}

impl Runtime {
    /// Starts the runtime's worker threads.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the threads or the I/O driver cannot be
    /// created.
    pub(crate) fn new() -> io::Result<Self> {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .thread_name("buildl-remote")
            .enable_all()
            .build()
            .map(|inner| Self { inner })
    }

    /// Runs `future` to completion on the calling thread and returns what it yields.
    ///
    /// Safe to call from several threads at once. The caller must not be inside a future
    /// that the runtime is already polling. A timer or socket is created inside `future`,
    /// not before the call: only inside it is a runtime running.
    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.inner.block_on(future)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::sync::Barrier;
    use std::thread;
    use std::time::Duration;

    use super::Runtime;

    #[test]
    fn block_on_returns_what_the_future_yields() {
        let runtime = Runtime::new().unwrap();
        assert_eq!(runtime.block_on(async { 6 * 7 }), 42);
    }

    #[test]
    fn sleeping_callers_overlap_on_one_runtime() {
        const CALLERS: u32 = 8;
        let runtime = Runtime::new().unwrap();
        let barrier = Barrier::new(CALLERS as usize);
        let nap = Duration::from_millis(300);

        let started = std::time::Instant::now();
        thread::scope(|scope| {
            let handles: Vec<_> = (0..CALLERS)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        runtime.block_on(async { tokio::time::sleep(nap).await });
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
        });

        // Run one after another the naps would take CALLERS times as long.
        assert!(started.elapsed() < nap * CALLERS / 2);
    }

    #[cfg(feature = "test-server")]
    #[test]
    fn many_threads_block_on_one_runtime_at_once() {
        use tonic::transport::Endpoint;

        use crate::remote::proto::build::bazel::remote::execution::v2::GetCapabilitiesRequest;
        use crate::remote::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
        use crate::remote::test_server::{Method, TestServer};

        const CALLERS: usize = 8;
        let server = TestServer::start().unwrap();
        // Held open on the server, so every call is in flight before any can finish.
        server.stall(Method::GetCapabilities, Duration::from_millis(500));

        let runtime = Runtime::new().unwrap();
        let endpoint = Endpoint::from_shared(format!("http://{}", server.address())).unwrap();
        let channel = runtime.block_on(endpoint.connect()).unwrap();
        let barrier = Barrier::new(CALLERS);

        thread::scope(|scope| {
            let handles: Vec<_> = (0..CALLERS)
                .map(|_| {
                    scope.spawn(|| {
                        let mut client = CapabilitiesClient::new(channel.clone());
                        barrier.wait();
                        runtime.block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
                    })
                })
                .collect();
            for handle in handles {
                assert!(handle.join().unwrap().is_ok());
            }
        });

        assert_eq!(server.peak_in_flight(), CALLERS);
    }
}
