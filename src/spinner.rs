//! Minimal single-thread spinner with no external dependencies.
//!
//! Animates a braille frame on stderr while a command runs, then clears the
//! line. `start(None)` or a non-TTY stderr yields `None` (no animation), so
//! piped/CI output behaves exactly as before.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const TICK: Duration = Duration::from_millis(80);

/// Spinner backed by a background thread drawing on stderr.
pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Starts the animation with `label`. Returns `None` when there is nothing
    /// to animate for (no label) or stderr is not a terminal.
    pub fn start(label: Option<&str>) -> Option<Spinner> {
        let label = label?;
        if !io::stderr().is_terminal() {
            return None;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let label = label.to_string();
        let handle = thread::spawn(move || {
            let mut i = 0usize;
            while !flag.load(Ordering::Relaxed) {
                eprint!("\r\x1b[2K  {} {}...", FRAMES[i % FRAMES.len()], label);
                let _ = io::stderr().flush();
                i += 1;
                thread::sleep(TICK);
            }
        });
        Some(Spinner { stop, handle: Some(handle) })
    }

    /// Stops the animation, joins the thread and erases the spinner line.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        // Guarantee the thread is joined and the line cleaned even on early
        // returns or panics inside the caller.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
    }
}
