//! Interactive "Shpell mode": a chat-style loop run on the user's terminal.
//!
//! The shell integration launches `shpell compose` with stdin and stderr attached
//! to the tty and captures stdout. All UI (icons, streaming, spinner) is drawn on
//! stderr, completely outside zle/readline — so the natural-language text never meets
//! shell parsing, syntax highlighting or history expansion. The only thing
//! ever written to stdout is the accepted command, and the exit code tells
//! the widget what to do with it:
//!
//!   0   put the command on the prompt — the user decides whether to run,
//!       edit or discard it
//!   1   cancel (Esc / Ctrl-C / Ctrl-D)

use crate::config::Config;
use crate::provider::{self, GenRequest, Provider};
use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use std::fs::File;
use std::io::Write;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const EXIT_CANCEL: i32 = 1;
// Claude Code-style pulsing sparkle, shown in place of the AI icon while
// generating (ping-pong order so the pulse breathes instead of jumping)
const SPARKLE: &[&str] = &["✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳"];

fn icon(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.into())
}

pub fn run(shell: &str, initial: Option<String>) -> Result<()> {
    let cfg = Config::load()?;
    let provider = provider::from_config(&cfg)?;
    let user_icon = icon("SHPELL_USER_ICON", "❯");
    let ai_icon = icon("SHPELL_AI_ICON", "✻");

    // take over the line the shell prompt was sitting on
    eprint!("\r\x1b[K");
    let _ = std::io::stderr().flush();

    // rustyline reads stdin (the widget attaches it to the tty) and paints
    // on stdout — but stdout must stay a clean result channel for the widget
    // to capture. So park the real stdout on a spare fd and point fd 1 at
    // stderr (the tty) for the rest of the session; only the accepted
    // command is written to the saved fd. Reading /dev/tty directly
    // (Behavior::PreferTerm) is not an option: macOS poll(2) always reports
    // POLLNVAL on /dev/tty, so rustyline never sees a lone Esc time out —
    // it blocks and fuses Esc with the next key into a Meta chord.
    let mut result_out = unsafe {
        let saved = libc::dup(1);
        if saved < 0 || libc::dup2(2, 1) < 0 {
            return Err(std::io::Error::last_os_error()).context("shpell: cannot set up stdout");
        }
        File::from_raw_fd(saved)
    };

    // rustyline provides real line editing for the query (arrow keys,
    // bracketed paste, Up-arrow recall of earlier queries).
    // keyseq_timeout lets a lone Esc press surface as a key instead of
    // waiting forever for the rest of an escape sequence (arrow keys still
    // work: their bytes arrive together, well within the timeout)
    let mut rl = rustyline::DefaultEditor::with_config(
        rustyline::Config::builder()
            .keyseq_timeout(Some(25))
            .build(),
    )?;
    // Esc cancels, same as Ctrl-C
    rl.bind_sequence(
        rustyline::KeyEvent(rustyline::KeyCode::Esc, rustyline::Modifiers::NONE),
        rustyline::Cmd::Interrupt,
    );

    let mut command = String::new();
    let mut hinted = false;
    // a request passed on the command line is submitted as the first turn
    let mut pending = initial
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    loop {
        let input = match pending.take() {
            Some(q) => {
                // echo it where the typed query would have appeared
                eprintln!("{user_icon} {q}");
                q
            }
            None => {
                let line = match rl.readline(&format!("{user_icon} ")) {
                    Ok(l) => l,
                    // Esc / Ctrl-C / Ctrl-D
                    Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                        std::process::exit(EXIT_CANCEL)
                    }
                    Err(e) => return Err(e.into()),
                };
                let input = line.trim().to_string();
                if input.is_empty() {
                    if command.is_empty() {
                        std::process::exit(EXIT_CANCEL);
                    }
                    writeln!(result_out, "{command}")?;
                    return Ok(()); // exit 0: put it on the prompt
                }
                input
            }
        };
        let input = input.as_str();
        let _ = rl.add_history_entry(input);
        let query = if command.is_empty() {
            input.to_string()
        } else {
            format!("Previously generated command: `{command}`. Revise it per this request: {input}")
        };
        let req = GenRequest {
            query,
            shell: shell.to_string(),
            os: std::env::consts::OS.to_string(),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        };
        match stream(provider.as_ref(), &req, &ai_icon) {
            Ok(cmd) => {
                command = cmd;
                if !hinted {
                    hinted = true;
                    eprintln!("\x1b[2m  ↵ accept · type to refine · esc cancel\x1b[0m");
                }
            }
            Err(e) => eprintln!("\r\x1b[K\x1b[31mshpell: {e:#}\x1b[0m"),
        }
    }
}

/// Stream one generation onto a single line — `✻ <command-so-far>` — with the
/// leading icon pulsing through sparkle frames (Claude Code style) until the
/// model finishes, then settling on the static AI icon. A painter thread
/// redraws the line every 120ms so the pulse animates even while the model
/// reasons silently; the generate callback just updates the shared snapshot.
fn stream(provider: &dyn Provider, req: &GenRequest, ai_icon: &str) -> Result<String> {
    let cols: usize = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(80);
    let snapshot = Arc::new(Mutex::new(String::new()));
    let done = Arc::new(AtomicBool::new(false));

    let painter = {
        let snapshot = Arc::clone(&snapshot);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut frame = 0;
            while !done.load(Ordering::Relaxed) {
                let snap = snapshot.lock().unwrap().clone();
                paint(SPARKLE[frame % SPARKLE.len()], &snap, cols);
                frame += 1;
                std::thread::sleep(Duration::from_millis(120));
            }
        })
    };

    let result = provider.generate(req, &mut |s: &str| {
        *snapshot.lock().unwrap() = s.to_string();
    });
    done.store(true, Ordering::Relaxed);
    let _ = painter.join();

    let cmd = result?;
    paint(ai_icon, &cmd, usize::MAX);
    eprintln!();
    Ok(cmd)
}

/// Redraw the response line in place, truncated to the terminal width so the
/// `\r` redraw never wraps onto a second line mid-stream.
fn paint(icon: &str, text: &str, cols: usize) {
    let budget = cols.saturating_sub(4); // icon, spaces, ellipsis
    let mut shown: String = text.chars().take(budget).collect();
    if shown.chars().count() < text.chars().count() {
        shown.push('…');
    }
    eprint!("\r\x1b[K\x1b[1;36m{icon}\x1b[0m {shown}");
    let _ = std::io::stderr().flush();
}
