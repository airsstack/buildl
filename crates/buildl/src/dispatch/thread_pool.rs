//! A pool of worker threads that lives for one dispatch.
//!
//! Its own file because it is the only place in the crate where a thread that runs a task is
//! started. The threads are scoped: what a task borrows — the strategy, the store, the clock —
//! only has to live as long as the dispatch, and every worker has ended when the dispatch
//! returns.
//!
//! Responsibilities: [`ThreadPool`].

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread;

use buildl_core::{Diagnostic, Dispatcher, Error, PortName, RemoteJobs, Result, Turn, Workers};

/// Runs tasks on up to a fixed number of threads, started as tasks need them.
///
/// A dispatch that is handed no task starts no thread, and one whose tasks come one at a time
/// starts one. No thread outlives the dispatch that started it.
#[derive(Debug)]
pub struct ThreadPool {
    workers: NonZeroUsize,
}

impl ThreadPool {
    /// As many workers as `workers` says for this machine: one per core for
    /// [`Workers::PerCore`], and one where the number of cores cannot be read.
    #[must_use]
    pub fn new(workers: Workers) -> Self {
        Self {
            workers: Self::width_for(workers),
        }
    }

    /// A pool wide enough for a run that executes on the remote: the larger of what `workers`
    /// comes to and `jobs`, so every action the remote may hold at once has a thread to wait on.
    #[must_use]
    pub fn wide(workers: Workers, jobs: RemoteJobs) -> Self {
        Self {
            workers: Self::width_for(workers).max(jobs.threads()),
        }
    }

    /// How many tasks the pool runs at once, at most.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn width(&self) -> NonZeroUsize {
        self.workers
    }

    /// How many workers `workers` comes to on this machine: what [`ThreadPool::new`] starts the
    /// pool with.
    #[must_use]
    pub fn width_for(workers: Workers) -> NonZeroUsize {
        let cores = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
        workers.on(cores)
    }
}

impl Dispatcher for ThreadPool {
    fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>,
    {
        serve(self.workers, &work, step).map(|_started| ())
    }
}

/// What a worker hands back.
enum Report<M, R> {
    /// A running task said this.
    Told(M),
    /// A task is done, with this result.
    Done(R),
    /// A task panicked, and has no result.
    Lost,
}

/// Runs every task `step` yields through `work`, on up to `limit` threads, and returns how many
/// threads it started.
///
/// A task is handed to a worker only when one is free, so a task that waits is handed over
/// after `step` has seen every result that came before: what `step` decides on a result
/// reaches every task not yet with a worker. A thread is started only when a task is handed
/// over while every thread there is has one, so the count is the most tasks that ever ran
/// together, up to `limit`. Tasks leave in the order `step` returned them.
fn serve<T, M, R, W, S>(limit: NonZeroUsize, work: &W, mut step: S) -> Result<usize>
where
    T: Send,
    M: Send,
    R: Send,
    W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
    S: FnMut(Turn<M, R>) -> Vec<T>,
{
    let ready = step(Turn::Begin);
    if ready.is_empty() {
        return Ok(0);
    }
    let (tasks, queue) = mpsc::channel::<T>();
    let queue = Mutex::new(queue);
    let (back, reports) = mpsc::channel::<Report<M, R>>();
    thread::scope(|scope| {
        // Owned by this closure, so it is dropped when the closure returns on any path: each
        // worker's `recv` then fails, the worker ends, and the scope joins it.
        let tasks = tasks;
        let mut limit = limit.get();
        let mut started = 0_usize;
        // The tasks a worker has, and the tasks that wait for one.
        let mut busy = 0_usize;
        let mut waiting = VecDeque::from(ready);
        loop {
            while busy < limit && !waiting.is_empty() {
                if busy == started {
                    let (queue, back) = (&queue, back.clone());
                    let spawned = thread::Builder::new()
                        .name(format!("buildl-worker-{started}"))
                        .spawn_scoped(scope, move || worker(queue, work, &back));
                    match spawned {
                        Ok(_) => started += 1,
                        Err(error) if started == 0 => {
                            return Err(Error::Adapter {
                                port: PortName::Dispatcher,
                                diagnostic: Diagnostic::new(format!(
                                    "starting a worker thread: {error}"
                                )),
                            });
                        }
                        // The workers there are take the task once one of them is free.
                        Err(_) => {
                            limit = started;
                            continue;
                        }
                    }
                }
                if let Some(task) = waiting.pop_front() {
                    busy += 1;
                    // The queue outlives the scope, so this cannot fail.
                    let _ = tasks.send(task);
                }
            }
            if busy == 0 {
                return Ok(started);
            }
            // This function keeps a sender of its own, so the channel cannot close.
            let Ok(report) = reports.recv() else {
                return Ok(started);
            };
            waiting.extend(match report {
                Report::Told(message) => step(Turn::Told(message)),
                Report::Done(result) => {
                    busy -= 1;
                    step(Turn::Done(result))
                }
                // Nothing is handed to `step`: whoever handed the task out finds it has no
                // result.
                Report::Lost => {
                    busy -= 1;
                    Vec::new()
                }
            });
        }
    })
}

/// One worker thread: takes tasks off `queue` until it closes, and hands back what each one
/// says and then its result, or that it was lost.
fn worker<T, M, R, W>(queue: &Mutex<Receiver<T>>, work: &W, back: &Sender<Report<M, R>>)
where
    M: Send,
    R: Send,
    W: Fn(T, &(dyn Fn(M) + Sync)) -> R,
{
    loop {
        // The lock is held to take one task and released before the task runs.
        let next = queue.lock().unwrap_or_else(PoisonError::into_inner).recv();
        let Ok(task) = next else {
            return;
        };
        // A sender may be used from several threads at once, so a task may say things from
        // threads of its own. Each message is sent before the result, on the one channel.
        let tell = |message: M| {
            let _ = back.send(Report::Told(message));
        };
        // A task that panics is caught here, so the thread lives on, the pool keeps its size,
        // and the scope never sees a panic.
        let report =
            catch_unwind(AssertUnwindSafe(|| work(task, &tell))).map_or(Report::Lost, Report::Done);
        if back.send(report).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::num::NonZeroUsize;
    use std::panic::resume_unwind;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use buildl_core::{Dispatcher, RemoteJobs, Turn, Workers};

    use super::{ThreadPool, serve};

    fn limit(workers: usize) -> NonZeroUsize {
        NonZeroUsize::new(workers).unwrap()
    }

    /// A task that says nothing and returns twice its number.
    fn doubled(task: u32, _tell: &(dyn Fn(()) + Sync)) -> u32 {
        task * 2
    }

    #[test]
    fn width_for_matches_new() {
        for workers in [Workers::PerCore, Workers::of(1), Workers::of(3)] {
            assert_eq!(
                ThreadPool::new(workers).workers,
                ThreadPool::width_for(workers)
            );
        }
        assert_eq!(ThreadPool::width_for(Workers::of(3)), limit(3));
    }

    /// The most tasks `pool` has running at once over twelve tasks that each wait until `expect`
    /// of them have started, or ten seconds. A pool that is too narrow to start `expect` at
    /// once makes the waiting tasks time out, and its peak comes out short.
    fn peak_when_held_until(pool: &mut ThreadPool, expect: usize) -> usize {
        let started = AtomicUsize::new(0);
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);

        pool.dispatch(
            |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                started.fetch_add(1, Ordering::SeqCst);
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(10);
                while started.load(Ordering::SeqCst) < expect && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                running.fetch_sub(1, Ordering::SeqCst);
                task
            },
            |turn| match turn {
                Turn::Begin => (0..12).collect(),
                Turn::Told(()) | Turn::Done(_) => Vec::new(),
            },
        )
        .unwrap();

        most.load(Ordering::SeqCst)
    }

    #[test]
    fn a_wide_pool_runs_as_many_tasks_at_once_as_its_larger_count() {
        let mut pool = ThreadPool::wide(Workers::of(2), RemoteJobs::from_integer(6).unwrap());

        assert_eq!(pool.workers, limit(6));
        assert_eq!(peak_when_held_until(&mut pool, 6), 6);
    }

    #[test]
    fn a_wide_pool_with_fewer_jobs_than_workers_keeps_the_workers() {
        let mut pool = ThreadPool::wide(Workers::of(4), RemoteJobs::from_integer(2).unwrap());

        assert_eq!(pool.workers, limit(4));
        assert_eq!(peak_when_held_until(&mut pool, 4), 4);
    }

    #[test]
    fn every_task_is_run_and_every_result_reaches_step() {
        let mut pool = ThreadPool::new(Workers::of(4));
        let mut results = Vec::new();

        // Each of the first five results hands out one more task, as a finished target releases
        // the targets that waited on it.
        pool.dispatch(doubled, |turn| match turn {
            Turn::Begin => (0..20).collect(),
            Turn::Told(()) => Vec::new(),
            Turn::Done(result) => {
                results.push(result);
                if result < 10 {
                    vec![1000 + result]
                } else {
                    Vec::new()
                }
            }
        })
        .unwrap();

        results.sort_unstable();
        let mut expected: Vec<u32> = (0..20).map(|task| task * 2).collect();
        expected.extend([2000, 2004, 2008, 2012, 2016]);
        assert_eq!(results, expected);
    }

    #[test]
    fn step_is_only_called_on_the_calling_thread() {
        let caller = thread::current().id();
        let mut pool = ThreadPool::new(Workers::of(4));
        let worked_on = Mutex::new(Vec::new());
        let mut stepped_on = Vec::new();

        pool.dispatch(
            |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                worked_on.lock().unwrap().push(thread::current().id());
                task
            },
            |turn| {
                stepped_on.push(thread::current().id());
                match turn {
                    Turn::Begin => (0..12).collect(),
                    Turn::Told(()) | Turn::Done(_) => Vec::new(),
                }
            },
        )
        .unwrap();

        assert_eq!(stepped_on.len(), 13);
        assert!(stepped_on.iter().all(|thread| *thread == caller));
        let worked_on = worked_on.into_inner().unwrap();
        assert_eq!(worked_on.len(), 12);
        assert!(worked_on.iter().all(|thread| *thread != caller));
    }

    #[test]
    fn no_more_tasks_run_at_once_than_there_are_workers() {
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);

        let started = serve(
            limit(2),
            &|task: u32, _tell: &(dyn Fn(()) + Sync)| {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(2));
                running.fetch_sub(1, Ordering::SeqCst);
                task
            },
            |turn| match turn {
                Turn::Begin => (0..16).collect(),
                Turn::Told(()) | Turn::Done(_) => Vec::new(),
            },
        )
        .unwrap();

        // Sixteen tasks were handed out at once, so both workers were started and no third.
        assert_eq!(started, 2);
        assert!(most.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn no_task_starts_no_thread() {
        let mut turns = 0;

        let started = serve(limit(4), &doubled, |_turn| {
            turns += 1;
            Vec::new()
        })
        .unwrap();

        assert_eq!(started, 0);
        assert_eq!(turns, 1);
    }

    #[test]
    fn one_task_at_a_time_uses_one_thread() {
        let mut results = Vec::new();

        // Each result hands out the next task, so no two tasks ever wait together.
        let started = serve(limit(8), &doubled, |turn| match turn {
            Turn::Begin => vec![0],
            Turn::Told(()) => Vec::new(),
            Turn::Done(result) => {
                results.push(result);
                if result < 18 {
                    vec![result / 2 + 1]
                } else {
                    Vec::new()
                }
            }
        })
        .unwrap();

        assert_eq!(started, 1);
        assert_eq!(results, [0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
    }

    #[test]
    fn one_worker_runs_tasks_in_the_order_they_were_handed_out() {
        let mut pool = ThreadPool::new(Workers::of(1));
        let order = Mutex::new(Vec::new());

        pool.dispatch(
            |task: u32, _tell: &(dyn Fn(()) + Sync)| order.lock().unwrap().push(task),
            |turn| match turn {
                Turn::Begin => (0..50).collect(),
                Turn::Told(()) | Turn::Done(()) => Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(order.into_inner().unwrap(), (0..50).collect::<Vec<u32>>());
    }

    #[test]
    fn a_task_is_handed_to_a_worker_only_after_step_has_seen_every_result_before_it() {
        let stopped = AtomicBool::new(false);
        let mut results = Vec::new();

        // The first result sets a flag, as a failed action stops a build. Eight tasks are handed
        // out together to one worker, so seven of them wait while the first runs, and each of
        // the seven must find the flag set.
        serve(
            limit(1),
            &|task: u32, _tell: &(dyn Fn(()) + Sync)| (task, stopped.load(Ordering::SeqCst)),
            |turn| match turn {
                Turn::Begin => (0..8).collect(),
                Turn::Told(()) => Vec::new(),
                Turn::Done(result) => {
                    stopped.store(true, Ordering::SeqCst);
                    results.push(result);
                    Vec::new()
                }
            },
        )
        .unwrap();

        let expected: Vec<(u32, bool)> = (0..8).map(|task| (task, task > 0)).collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn workers_per_core_is_the_machines_parallelism() {
        let cores = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

        assert_eq!(ThreadPool::new(Workers::PerCore).workers, cores);
        assert_eq!(ThreadPool::new(Workers::of(3)).workers, limit(3));
    }

    #[test]
    fn what_a_task_says_reaches_step_before_its_result_and_in_order() {
        let mut pool = ThreadPool::new(Workers::of(4));
        let mut log: Vec<(u32, u32)> = Vec::new();

        // Every task says three things, the last from a thread of its own, and its result is
        // logged as a fourth. The first thing tasks 0 and 1 say hands out one more task each.
        pool.dispatch(
            |task: u32, tell: &(dyn Fn((u32, u32)) + Sync)| {
                tell((task, 0));
                tell((task, 1));
                thread::scope(|scope| {
                    scope.spawn(|| tell((task, 2)));
                });
                task
            },
            |turn| match turn {
                Turn::Begin => (0..8).collect(),
                Turn::Told(said) => {
                    log.push(said);
                    match said {
                        (task @ (0 | 1), 0) => vec![100 + task],
                        _ => Vec::new(),
                    }
                }
                Turn::Done(task) => {
                    log.push((task, 3));
                    Vec::new()
                }
            },
        )
        .unwrap();

        for task in (0..8).chain([100, 101]) {
            let of_task: Vec<u32> = log
                .iter()
                .filter(|(from, _)| *from == task)
                .map(|(_, what)| *what)
                .collect();
            assert_eq!(of_task, [0, 1, 2, 3], "task {task}");
        }
        assert_eq!(log.len(), 40);
    }

    #[test]
    fn a_task_that_panics_is_lost_and_the_rest_are_reported() {
        // The dispatch runs on a thread of its own, so a pool that waited for the lost task's
        // result for ever fails this test instead of hanging it.
        let (done, waiting) = mpsc::channel();
        thread::spawn(move || {
            let mut pool = ThreadPool::new(Workers::of(2));
            let mut results = Vec::new();
            let ended = pool.dispatch(
                |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                    if task == 3 {
                        // Unwinds as a panic does, without the message a panic prints.
                        resume_unwind(Box::new("the task panics"));
                    }
                    task
                },
                |turn| match turn {
                    Turn::Begin => (0..8).collect(),
                    Turn::Told(()) => Vec::new(),
                    Turn::Done(task) => {
                        results.push(task);
                        Vec::new()
                    }
                },
            );
            let _ = done.send((ended.is_ok(), results));
        });

        let (returned_ok, mut results) = waiting.recv_timeout(Duration::from_secs(20)).unwrap();

        assert!(returned_ok);
        results.sort_unstable();
        assert_eq!(results, [0, 1, 2, 4, 5, 6, 7]);
    }
}
