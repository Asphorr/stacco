//! The click engine: a dedicated worker thread driven by a command channel.
//!
//! Design notes:
//! * Clicking happens on its own thread so the UI never blocks.
//! * Control messages travel over an [`mpsc`] channel; the worker waits for the
//!   inter-click interval with [`Receiver::recv_timeout`], which makes it react
//!   to `Stop`/`Shutdown` *immediately* instead of after the current interval.
//! * Observable state (`running`, `clicks`) lives in lock-free atomics so the UI
//!   can poll it cheaply without contending with the worker.
//! * The input backend is shared (`Arc`) so the UI thread can still query the
//!   cursor position while the worker owns the clicking.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::config::{ClickConfig, ClickType, Position, Repeat};
use crate::error::{Error, Result};
use crate::input::{InputBackend, Point};

/// A snapshot of engine state, sent to the frontend on every poll.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Status {
    pub running: bool,
    pub clicks: u64,
}

/// Messages the public API sends to the worker thread.
#[derive(Debug)]
enum Command {
    Start(ClickConfig),
    Stop,
    Shutdown,
}

/// State shared between the API surface and the worker thread.
#[derive(Debug, Default)]
struct Shared {
    running: AtomicBool,
    clicks: AtomicU64,
}

/// Owns the worker thread and exposes a thread-safe control surface.
///
/// Dropping the engine cleanly shuts the worker down and joins it.
pub struct ClickerEngine {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    backend: Arc<dyn InputBackend>,
    worker: Option<JoinHandle<()>>,
}

impl ClickerEngine {
    /// Spawns the worker thread and returns a handle to it.
    #[must_use]
    pub fn new(backend: Arc<dyn InputBackend>) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());

        let worker_shared = Arc::clone(&shared);
        let worker_backend = Arc::clone(&backend);
        let worker = thread::Builder::new()
            .name("clicker-worker".to_owned())
            .spawn(move || worker_loop(&rx, &worker_shared, worker_backend.as_ref()))
            .expect("spawning the clicker worker thread should not fail");

        Self {
            tx,
            shared,
            backend,
            worker: Some(worker),
        }
    }

    /// Validates `config` and starts (or restarts) clicking.
    ///
    /// State is updated synchronously so an immediately-following [`Self::is_running`]
    /// observes the change without racing the worker.
    ///
    /// # Errors
    /// Returns [`Error::InvalidConfig`] if `config` is invalid, or
    /// [`Error::WorkerUnavailable`] if the worker thread has gone away.
    pub fn start(&self, config: ClickConfig) -> Result<()> {
        config.validate()?;
        self.shared.clicks.store(0, Ordering::Release);
        self.shared.running.store(true, Ordering::Release);
        self.tx
            .send(Command::Start(config))
            .map_err(|_| Error::WorkerUnavailable)
    }

    /// Stops clicking. Idempotent.
    ///
    /// # Errors
    /// Returns [`Error::WorkerUnavailable`] if the worker thread has gone away.
    pub fn stop(&self) -> Result<()> {
        self.shared.running.store(false, Ordering::Release);
        self.tx
            .send(Command::Stop)
            .map_err(|_| Error::WorkerUnavailable)
    }

    /// Starts if stopped, stops if started. Returns the new running state.
    ///
    /// # Errors
    /// Propagates errors from [`Self::start`] / [`Self::stop`].
    pub fn toggle(&self, config: ClickConfig) -> Result<bool> {
        if self.is_running() {
            self.stop()?;
            Ok(false)
        } else {
            self.start(config)?;
            Ok(true)
        }
    }

    /// Whether the engine is currently clicking.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// A consistent snapshot of `(running, clicks)`.
    #[must_use]
    pub fn status(&self) -> Status {
        Status {
            running: self.shared.running.load(Ordering::Acquire),
            clicks: self.shared.clicks.load(Ordering::Acquire),
        }
    }

    /// Reads the current cursor position (used by the "capture point" button).
    ///
    /// # Errors
    /// Propagates backend failures.
    pub fn cursor_position(&self) -> Result<Point> {
        self.backend.cursor_position()
    }
}

impl Drop for ClickerEngine {
    fn drop(&mut self) {
        // Ask the worker to exit, then wait for it so the thread never outlives
        // the engine. Both steps are best-effort: if the channel is already
        // closed or the thread already gone, there is nothing to clean up.
        let _ = self.tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Top-level worker loop: idle until told to start, then run a click session.
fn worker_loop(rx: &Receiver<Command>, shared: &Shared, backend: &dyn InputBackend) {
    loop {
        // Block with no CPU cost until there is something to do.
        let config = match rx.recv() {
            Ok(Command::Start(config)) => config,
            Ok(Command::Stop) => continue, // already idle
            Ok(Command::Shutdown) | Err(_) => return,
        };
        if run_session(rx, shared, backend, config).is_shutdown() {
            return;
        }
    }
}

/// How a click session ended.
enum SessionEnd {
    /// Return to the idle state and wait for the next `Start`.
    Idle,
    /// Terminate the worker thread.
    Shutdown,
}

impl SessionEnd {
    fn is_shutdown(&self) -> bool {
        matches!(self, SessionEnd::Shutdown)
    }
}

/// A tiny non-cryptographic PRNG (xorshift64*) used only for click jitter.
///
/// Jitter just needs cheap "random enough" noise, so we avoid pulling in the
/// `rand` crate.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        // Seed from the wall clock; jitter does not need a strong seed.
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64);
        Self(nanos | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in the open interval (-1.0, 1.0).
    fn signed_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// How long to wait before the next click, with interval jitter applied.
fn next_interval(config: &ClickConfig, rng: &mut Rng) -> Duration {
    let pct = f64::from(config.jitter.interval_pct);
    if pct <= 0.0 {
        return config.interval();
    }
    let base = config.interval_ms as f64;
    let ms = (base + base * (pct / 100.0) * rng.signed_unit()).round();
    Duration::from_millis(ms.max(1.0) as u64)
}

/// A random position offset within ±`px` on each axis (0 → no offset).
fn jitter_offset(px: u32, rng: &mut Rng) -> (i32, i32) {
    if px == 0 {
        return (0, 0);
    }
    let r = f64::from(px);
    (
        (r * rng.signed_unit()).round() as i32,
        (r * rng.signed_unit()).round() as i32,
    )
}

/// Runs one clicking session until it is stopped, completes its count, or the
/// worker is told to shut down.
fn run_session(
    rx: &Receiver<Command>,
    shared: &Shared,
    backend: &dyn InputBackend,
    mut config: ClickConfig,
) -> SessionEnd {
    shared.clicks.store(0, Ordering::Release);
    let mut rng = Rng::new();

    loop {
        if let Err(e) = perform_click(backend, &config, &mut rng) {
            log::error!("click failed, stopping session: {e}");
            shared.running.store(false, Ordering::Release);
            return SessionEnd::Idle;
        }

        let done = shared.clicks.fetch_add(1, Ordering::AcqRel) + 1;
        if let Repeat::Count { times } = config.repeat {
            if done >= times {
                shared.running.store(false, Ordering::Release);
                return SessionEnd::Idle;
            }
        }

        // Wait the inter-click interval, but wake instantly for a command.
        match rx.recv_timeout(next_interval(&config, &mut rng)) {
            Err(RecvTimeoutError::Timeout) => {} // interval elapsed: click again
            Ok(Command::Start(new_config)) => {
                // Live reconfiguration: adopt the new settings and restart count.
                config = new_config;
                shared.clicks.store(0, Ordering::Release);
            }
            Ok(Command::Stop) => {
                shared.running.store(false, Ordering::Release);
                return SessionEnd::Idle;
            }
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                shared.running.store(false, Ordering::Release);
                return SessionEnd::Shutdown;
            }
        }
    }
}

/// Performs one logical click (moving first for a fixed position, and twice for
/// a double click).
fn perform_click(backend: &dyn InputBackend, config: &ClickConfig, rng: &mut Rng) -> Result<()> {
    if let Position::Fixed { x, y } = config.position {
        let (dx, dy) = jitter_offset(config.jitter.position_px, rng);
        backend.move_cursor(Point {
            x: x + dx,
            y: y + dy,
        })?;
    }
    backend.click(config.button)?;
    if config.click_type == ClickType::Double {
        backend.click(config.button)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MouseButton;
    use std::time::{Duration, Instant};

    /// A backend that counts clicks instead of touching the OS.
    #[derive(Default)]
    struct CountingBackend {
        clicks: AtomicU64,
    }

    impl InputBackend for CountingBackend {
        fn click(&self, _button: MouseButton) -> Result<()> {
            self.clicks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn move_cursor(&self, _point: Point) -> Result<()> {
            Ok(())
        }
        fn cursor_position(&self) -> Result<Point> {
            Ok(Point { x: 0, y: 0 })
        }
    }

    /// Polls `cond` until it holds or `timeout` elapses.
    fn wait_until(cond: impl Fn() -> bool, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        cond()
    }

    fn fast_config() -> ClickConfig {
        ClickConfig {
            interval_ms: 2,
            ..Default::default()
        }
    }

    #[test]
    fn performs_exactly_the_requested_count() {
        let backend = Arc::new(CountingBackend::default());
        let engine = ClickerEngine::new(backend.clone());

        engine
            .start(ClickConfig {
                repeat: Repeat::Count { times: 5 },
                ..fast_config()
            })
            .unwrap();

        assert!(
            wait_until(|| !engine.is_running(), Duration::from_secs(2)),
            "session did not finish in time"
        );
        assert_eq!(engine.status().clicks, 5);
        assert_eq!(backend.clicks.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn double_click_actuates_twice_per_tick() {
        let backend = Arc::new(CountingBackend::default());
        let engine = ClickerEngine::new(backend.clone());

        engine
            .start(ClickConfig {
                click_type: ClickType::Double,
                repeat: Repeat::Count { times: 3 },
                ..fast_config()
            })
            .unwrap();

        assert!(wait_until(|| !engine.is_running(), Duration::from_secs(2)));
        // 3 logical clicks, each firing the button twice.
        assert_eq!(engine.status().clicks, 3);
        assert_eq!(backend.clicks.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn stop_halts_clicking_promptly() {
        let backend = Arc::new(CountingBackend::default());
        let engine = ClickerEngine::new(backend);

        engine
            .start(ClickConfig {
                interval_ms: 5,
                ..Default::default()
            })
            .unwrap();
        assert!(wait_until(|| engine.status().clicks >= 2, Duration::from_secs(2)));

        engine.stop().unwrap();
        let at_stop = engine.status().clicks;
        thread::sleep(Duration::from_millis(60));
        // At most one in-flight click may land after the stop request.
        assert!(engine.status().clicks - at_stop <= 1);
        assert!(!engine.is_running());
    }

    #[test]
    fn start_rejects_invalid_config() {
        let backend = Arc::new(CountingBackend::default());
        let engine = ClickerEngine::new(backend);
        let result = engine.start(ClickConfig {
            interval_ms: 0,
            ..Default::default()
        });
        assert!(result.is_err());
        assert!(!engine.is_running());
    }
}
