//! Running one process to its end, and keeping what it printed.
//!
//! Its own file because waiting for a process is where a build can hang: a pipe that fills
//! while nobody reads it stops the process for good, and a pipe something else still holds
//! never reaches its end. Each of the two outputs is read on a thread of its own, and the
//! thread that started the process is the only one that touches it.
//!
//! Responsibilities: `capture` and `Capture`.
//!
//! Non-responsibilities: choosing the program, its arguments, its directory and its
//! environment, and saying what an ending means. The caller builds the command and reads the
//! status.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use buildl_core::{Channel, LiveSink};

/// How one process ended, and everything it printed.
#[derive(Debug)]
pub(crate) struct Capture {
    /// The status the process ended with.
    pub(crate) status: ExitStatus,
    /// Whether it ran longer than it was allowed to and was stopped. Then `status` is that of a
    /// process ended by the signal that stopped it.
    pub(crate) timed_out: bool,
    /// Everything it wrote to standard output.
    pub(crate) stdout: Vec<u8>,
    /// Everything it wrote to standard error.
    pub(crate) stderr: Vec<u8>,
}

/// What a reader hands the thread that waits for the process.
enum Piece {
    /// A run of bytes read from one of the two outputs.
    Chunk(Channel, Vec<u8>),
    /// That reader's output is at its end, or can no longer be read.
    Closed,
}

/// How much a reader asks for at a time.
const CHUNK: usize = 64 * 1024;

/// The longest the waiting thread sleeps without looking at the process.
const TICK: Duration = Duration::from_millis(20);

/// The first pause of a wait in which nothing can arrive to end it. Each pause after it is
/// twice as long, up to [`TICK`].
const FIRST_PAUSE: Duration = Duration::from_micros(100);

/// How long, once the process is gone, its outputs get to reach their end.
///
/// An output still open then is held by something the process started. What that writes
/// within this time is kept; after it the readers are left behind, and each of them ends when
/// whatever holds its output closes it or writes to it.
const DRAIN: Duration = Duration::from_secs(1);

/// Spawns `command` with an empty standard input and both outputs piped, and waits for it at
/// most `timeout`.
///
/// A process still running after `timeout` is killed. Only that process is: what it started is
/// not, and stays in this process's group, where a signal from the terminal reaches it. Once
/// the process is gone, an output something else still holds open is waited for [`DRAIN`] and
/// no longer, so what the process started cannot hold the caller.
///
/// With `live`, every run of bytes is handed to it as it arrives, with the output it was
/// written to, and then kept: `live` is called on the calling thread, once per run of bytes,
/// each output's in the order written, with exactly the bytes the result holds.
///
/// # Errors
///
/// Returns the failure to start the process, to start a reader, to wait or to kill. A process
/// that was started is killed and reaped, as far as it can be, before an error is returned.
pub(crate) fn capture(
    command: &mut Command,
    timeout: Duration,
    live: Option<LiveSink<'_>>,
) -> io::Result<Capture> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let waited = waited(&mut child, timeout, live);
    if waited.is_err() {
        // The error says what went wrong; whether these two succeed changes nothing.
        let _ = child.kill();
        let _ = child.wait();
    }
    waited
}

/// Reads both outputs of `child` and waits for it, at most `timeout`.
///
/// The calling thread is the only one that touches `child`. It wakes when a reader hands it
/// something, and otherwise at least every [`TICK`], to look at the process and at the time: a
/// process that ends closes its outputs, so one that ends early is noticed at once.
fn waited(child: &mut Child, timeout: Duration, live: Option<LiveSink<'_>>) -> io::Result<Capture> {
    // A monotonic reading, which a wall clock set back cannot stretch. A timeout too long to
    // add to it is no deadline at all.
    let deadline = Instant::now().checked_add(timeout);
    let (to, from) = mpsc::channel();
    let mut open = 0_u8;
    if let Some(pipe) = child.stdout.take() {
        read(Channel::Stdout, pipe, to.clone())?;
        open += 1;
    }
    if let Some(pipe) = child.stderr.take() {
        read(Channel::Stderr, pipe, to.clone())?;
        open += 1;
    }
    // Only the readers send now, so the channel closes when the last of them has ended.
    drop(to);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    // The status the process ended with, and when to stop waiting for its outputs.
    let mut ended: Option<(ExitStatus, Instant)> = None;
    let mut timed_out = false;
    let mut pause = FIRST_PAUSE;
    loop {
        let now = Instant::now();
        if ended.is_none() {
            // The process is looked at before the time is, so one that ends on the deadline
            // is not stopped.
            let mut status = child.try_wait()?;
            if status.is_none() && deadline.is_some_and(|deadline| now >= deadline) {
                child.kill()?;
                status = Some(child.wait()?);
                timed_out = true;
            }
            ended = status.map(|status| (status, now.checked_add(DRAIN).unwrap_or(now)));
        }
        let wait = match ended {
            Some((status, drained_by)) if open == 0 || now >= drained_by => {
                // Returning drops the receiver, which is what ends a reader left behind the
                // next time it has something to send.
                return Ok(Capture {
                    status,
                    timed_out,
                    stdout,
                    stderr,
                });
            }
            Some((_, drained_by)) => drained_by.saturating_duration_since(now),
            None => deadline.map_or(TICK, |deadline| {
                TICK.min(deadline.saturating_duration_since(now))
            }),
        };
        if open == 0 {
            // Both outputs are at their end and the process still runs: nothing can arrive to
            // wake this thread, so it sleeps, a little longer each time.
            thread::sleep(pause.min(wait));
            pause = pause.saturating_mul(2).min(TICK);
            continue;
        }
        match from.recv_timeout(wait) {
            Ok(Piece::Chunk(channel, bytes)) => {
                if let Some(live) = live {
                    live(channel, &bytes);
                }
                match channel {
                    Channel::Stdout => stdout.extend_from_slice(&bytes),
                    Channel::Stderr => stderr.extend_from_slice(&bytes),
                }
            }
            Ok(Piece::Closed) => open = open.saturating_sub(1),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => open = 0,
        }
    }
}

/// Starts a thread that reads `pipe` to its end and sends what it reads to `to`.
///
/// The thread is not joined. It ends at the end of the stream, at a read error, or when what
/// it sends has nobody left to take it.
fn read<R: Read + Send + 'static>(
    channel: Channel,
    mut pipe: R,
    to: Sender<Piece>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("buildl-capture".to_owned())
        .spawn(move || {
            let mut buffer = vec![0_u8; CHUNK];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let chunk = buffer[..count].to_vec();
                        if to.send(Piece::Chunk(channel, chunk)).is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = to.send(Piece::Closed);
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::process::Command;
    use std::sync::Mutex;
    use std::thread;
    use std::time::{Duration, Instant};

    use buildl_core::Channel;

    use super::{Capture, capture};

    /// Longer than any command of these tests runs when nothing is wrong.
    const PATIENT: Duration = Duration::from_secs(60);

    /// The command `sh -c script`, with an empty environment. A tool the script names is
    /// written by its path, because the child has no `PATH`.
    fn sh(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.env_clear().args(["-c", script]);
        command
    }

    fn run(command: &mut Command) -> Capture {
        capture(command, PATIENT, None).unwrap()
    }

    #[test]
    fn standard_input_is_empty() {
        // `cat` copies its input until the input ends, so it returns at once, with nothing,
        // only when the input it was handed is already at its end.
        let ended = run(Command::new("/bin/cat").env_clear());

        assert!(ended.status.success());
        assert_eq!(ended.stdout, b"");
        assert_eq!(ended.stderr, b"");
    }

    #[test]
    fn ten_megabytes_on_each_stream_do_not_block() {
        // Far more than a pipe holds, on both at once: read one after the other, the process
        // would stop on the pipe nobody is reading.
        let ended = run(&mut sh(
            "/usr/bin/head -c 10000000 /dev/zero & /usr/bin/head -c 10000000 /dev/zero >&2; wait",
        ));

        assert!(ended.status.success());
        assert_eq!(ended.stdout.len(), 10_000_000);
        assert_eq!(ended.stderr.len(), 10_000_000);
    }

    #[test]
    fn a_child_that_closes_its_streams_is_still_timed_out() {
        let started = Instant::now();

        // Both outputs reach their end at once, and the process runs on for half a minute.
        let ended = capture(
            &mut sh("exec >&- 2>&-; exec /bin/sleep 30"),
            Duration::from_millis(300),
            None,
        )
        .unwrap();

        assert!(ended.timed_out);
        assert_eq!(ended.stdout, b"");
        // Stopped at the timeout, not when the process would have ended by itself.
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_grandchild_holding_the_pipes_does_not_hang_the_run() {
        let started = Instant::now();

        // The shell ends at once. The `sleep` it started keeps both outputs open for half a
        // minute.
        let ended = run(&mut sh("/bin/sleep 30 & echo hi; exit 0"));

        assert!(ended.status.success());
        assert!(!ended.timed_out);
        // What was written before the process ended is kept.
        assert_eq!(ended.stdout, b"hi\n");
        // Returned once the outputs had their time to drain, not when their holder let go.
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_live_sink_is_called_on_the_waiting_thread_with_what_is_captured() {
        let caller = thread::current().id();
        let seen = Mutex::new(Vec::new());
        let live = |channel: Channel, bytes: &[u8]| {
            seen.lock()
                .unwrap()
                .push((thread::current().id(), channel, bytes.to_vec()));
        };

        let ended = capture(&mut sh("echo out; echo err >&2"), PATIENT, Some(&live)).unwrap();

        let seen = seen.into_inner().unwrap();
        // Never from a reader: whoever is handed the bytes needs no lock of its own.
        assert!(seen.iter().all(|(thread, _, _)| *thread == caller));
        let joined = |wanted: Channel| -> Vec<u8> {
            seen.iter()
                .filter(|(_, channel, _)| *channel == wanted)
                .flat_map(|(_, _, bytes)| bytes.iter().copied())
                .collect()
        };
        assert_eq!(joined(Channel::Stdout), ended.stdout);
        assert_eq!(joined(Channel::Stderr), ended.stderr);
        assert_eq!(ended.stdout, b"out\n");
        assert_eq!(ended.stderr, b"err\n");
    }
}
