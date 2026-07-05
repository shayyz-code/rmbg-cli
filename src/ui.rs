use std::env;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anstream::{AutoStream, ColorChoice as StreamColorChoice};
use clap::builder::styling::{AnsiColor, Color, RgbColor, Style, Styles};
use clap::ColorChoice;
use terminal_size::{terminal_size, Width};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::cli::OutputArgs;
use crate::runtime::{DoctorReport, ProgressEvent};

const PURPLE: RgbColor = RgbColor(168, 85, 247);
const PINK: RgbColor = RgbColor(236, 72, 153);
const PALE_PINK: RgbColor = RgbColor(245, 208, 254);
const MUTED: RgbColor = RgbColor(148, 163, 184);
const RED: RgbColor = RgbColor(248, 113, 113);
const GREEN: RgbColor = RgbColor(74, 222, 128);
const SHIMMER: [RgbColor; 5] = [
    RgbColor(126, 34, 206),
    PURPLE,
    PALE_PINK,
    PINK,
    RgbColor(190, 24, 93),
];
const CLEAR_PROGRESS: &[u8] =
    b"\r\x1b[2K\x1b[1A\r\x1b[2K\x1b[1A\r\x1b[2K\x1b[1A\r\x1b[2K\x1b[1A\r\x1b[2K";

pub fn help_styles() -> Styles {
    Styles::styled()
        .header(Style::new().fg_color(Some(Color::Rgb(PURPLE))).bold())
        .usage(Style::new().fg_color(Some(Color::Rgb(PURPLE))).bold())
        .literal(Style::new().fg_color(Some(Color::Rgb(PINK))).bold())
        .placeholder(Style::new().fg_color(Some(Color::Rgb(PALE_PINK))))
        .error(AnsiColor::Red.on_default().bold())
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Yellow.on_default())
}

pub fn configure_color(choice: ColorChoice) {
    stream_choice(choice).write_global();
}

#[derive(Clone, Copy)]
pub struct Ui {
    color: ColorChoice,
    interactive: bool,
    quiet: bool,
}

impl Ui {
    pub fn new(args: &OutputArgs) -> Self {
        let interactive = !args.json
            && io::stderr().is_terminal()
            && env::var("TERM").map_or(true, |term| !term.eq_ignore_ascii_case("dumb"));
        Self {
            color: if args.json {
                ColorChoice::Never
            } else {
                args.color
            },
            interactive,
            quiet: args.quiet || args.json,
        }
    }

    pub fn detail(self, label: &str, value: &str) {
        if !self.quiet {
            self.line(format!("  {}: {}", sanitize(label), sanitize(value)));
        }
    }

    pub fn diagnostics(self, text: &str) {
        if self.quiet {
            return;
        }
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            self.detail("runtime", line);
        }
    }

    pub fn step(self, current: usize, total: usize, message: &str) {
        if !self.quiet {
            self.line(format!(
                "{}[{current}/{total}]{} {}",
                style(PINK),
                style(PINK).render_reset(),
                sanitize(message)
            ));
        }
    }

    pub fn notice(self, message: &str) {
        if !self.quiet {
            self.line(format!(
                "{}!{} {}",
                style(PINK).bold(),
                style(PINK).bold().render_reset(),
                sanitize(message)
            ));
        }
    }

    pub fn success(self, message: &str) {
        if !self.quiet {
            self.line(format!(
                "{}✓{} {}",
                style(GREEN).bold(),
                style(GREEN).bold().render_reset(),
                sanitize(message)
            ));
        }
    }

    pub fn error(self, message: &str) {
        self.line(format!(
            "{}error:{} {}",
            style(RED).bold(),
            style(RED).bold().render_reset(),
            sanitize(message)
        ));
    }

    pub fn doctor(self, report: &DoctorReport) {
        for check in &report.checks {
            if self.quiet
                && !matches!(
                    check.status,
                    crate::runtime::CheckStatus::ActionRequired
                        | crate::runtime::CheckStatus::Error
                )
            {
                continue;
            }
            self.line(format!(
                "[{}] {}: {}",
                serde_json::to_value(check.status)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "error".to_owned()),
                check.name,
                sanitize(&check.detail)
            ));
        }
    }

    pub fn progress(self, filename: &str) -> ProgressDisplay {
        if self.quiet {
            return ProgressDisplay::hidden();
        }
        if !self.interactive {
            return ProgressDisplay {
                mode: ProgressMode::Lines { ui: self },
                started: Instant::now(),
            };
        }
        let state = Arc::new(Mutex::new(ProgressState {
            completed: 0,
            label: "Starting".to_owned(),
            filename: sanitize(filename),
            frame: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let choice = stream_choice(self.color);
        let started = Instant::now();
        let handle = thread::spawn(move || {
            let mut first = true;
            while !thread_stop.load(Ordering::Relaxed) {
                let frame = {
                    let mut state = thread_state.lock().expect("progress state poisoned");
                    state.frame = state.frame.wrapping_add(1);
                    render_progress_frame(&state, started.elapsed())
                };
                if !first {
                    clear_progress_rows();
                }
                first = false;
                write_stderr(choice, &frame);
                thread::sleep(Duration::from_millis(90));
            }
        });
        ProgressDisplay {
            started,
            mode: ProgressMode::Interactive {
                state,
                stop,
                choice,
                handle: Some(handle),
            },
        }
    }

    fn line(self, line: String) {
        write_stderr(stream_choice(self.color), &format!("{line}\n"));
    }
}

struct ProgressState {
    completed: u8,
    label: String,
    filename: String,
    frame: usize,
}

enum ProgressMode {
    Hidden,
    Lines {
        ui: Ui,
    },
    Interactive {
        state: Arc<Mutex<ProgressState>>,
        stop: Arc<AtomicBool>,
        choice: StreamColorChoice,
        handle: Option<JoinHandle<()>>,
    },
}

pub struct ProgressDisplay {
    mode: ProgressMode,
    started: Instant,
}

impl ProgressDisplay {
    fn hidden() -> Self {
        Self {
            mode: ProgressMode::Hidden,
            started: Instant::now(),
        }
    }

    pub fn update(&mut self, event: &ProgressEvent) {
        match &self.mode {
            ProgressMode::Hidden => {}
            ProgressMode::Lines { ui } => ui.line(format!(
                "[{}/5] {}{}",
                event.completed,
                sanitize(&event.label),
                event
                    .device
                    .as_ref()
                    .map(|device| format!(" ({})", sanitize(device)))
                    .unwrap_or_default()
            )),
            ProgressMode::Interactive { state, .. } => {
                let mut state = state.lock().expect("progress state poisoned");
                state.completed = event.completed;
                state.label = sanitize(&event.label);
            }
        }
    }

    pub fn complete(&mut self) -> Duration {
        let event = ProgressEvent {
            completed: 5,
            total: 5,
            stage: "output_committed".to_owned(),
            label: "Output committed".to_owned(),
            device: None,
        };
        self.update(&event);
        self.stop(true);
        self.started.elapsed()
    }

    pub fn fail(&mut self) {
        self.stop(false);
    }

    fn stop(&mut self, success: bool) {
        if let ProgressMode::Interactive {
            state,
            stop,
            choice,
            handle,
        } = &mut self.mode
        {
            stop.store(true, Ordering::Relaxed);
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
            clear_progress_rows();
            if success {
                let frame = render_progress_frame(
                    &state.lock().expect("progress state poisoned"),
                    self.started.elapsed(),
                );
                write_stderr(*choice, &format!("{frame}\n"));
            }
        }
    }
}

impl Drop for ProgressDisplay {
    fn drop(&mut self) {
        self.fail();
    }
}

fn render_progress_frame(state: &ProgressState, elapsed: Duration) -> String {
    let terminal = terminal_size()
        .map(|(Width(width), _)| width as usize)
        .unwrap_or(62);
    let inner = terminal.saturating_sub(2).clamp(20, 60);
    render_progress_frame_at_width(state, elapsed, inner)
}

fn render_progress_frame_at_width(
    state: &ProgressState,
    elapsed: Duration,
    inner: usize,
) -> String {
    let percent = usize::from(state.completed) * 20;
    let header = format!("{percent:>3}% {}", state.label);
    let footer = format!("{} · {}", state.filename, format_duration(elapsed));
    let bar_width = inner.saturating_sub(2);
    let filled = bar_width * usize::from(state.completed) / 5;
    let mut bar = String::new();
    for index in 0..bar_width {
        if index < filled {
            let color = SHIMMER[(index + state.frame) % SHIMMER.len()];
            let _ = write!(bar, "{}█{}", style(color), style(color).render_reset());
        } else {
            let _ = write!(bar, "{}░{}", style(MUTED), style(MUTED).render_reset());
        }
    }
    let border = "─".repeat(inner);
    format!(
        "╭{border}╮\n│{}│\n│ {bar} │\n│{}│\n╰{border}╯",
        pad_line(&header, inner),
        pad_line(&footer, inner)
    )
}

fn pad_line(value: &str, width: usize) -> String {
    let value = truncate_width(value, width.saturating_sub(2));
    let padding = width.saturating_sub(2 + UnicodeWidthStr::width(value.as_str()));
    format!(" {value}{} ", " ".repeat(padding))
}

fn truncate_width(value: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for character in value.chars() {
        let next = character.width().unwrap_or(0);
        if used + next > width {
            break;
        }
        result.push(character);
        used += next;
    }
    result
}

pub fn sanitize(value: &str) -> String {
    let mut result = String::new();
    let mut escape = 0u8;
    for character in value.chars() {
        if escape == 1 {
            if character == '[' || character == ']' {
                escape = 2;
            } else {
                escape = 0;
            }
            continue;
        }
        if escape == 2 {
            if ('@'..='~').contains(&character) {
                escape = 0;
            }
            continue;
        }
        if character == '\u{1b}' {
            escape = 1;
        } else if !character.is_control() || character == '\t' {
            result.push(character);
        }
    }
    result
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

fn style(color: RgbColor) -> Style {
    Style::new().fg_color(Some(Color::Rgb(color)))
}

fn stream_choice(choice: ColorChoice) -> StreamColorChoice {
    match choice {
        ColorChoice::Auto => StreamColorChoice::Auto,
        ColorChoice::Always => StreamColorChoice::Always,
        ColorChoice::Never => StreamColorChoice::Never,
    }
}

fn write_stderr(choice: StreamColorChoice, text: &str) {
    let mut stderr = AutoStream::new(io::stderr(), choice);
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}

fn clear_progress_rows() {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(CLEAR_PROGRESS);
    let _ = stderr.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(format_duration(Duration::from_secs(7)), "7s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 05s");
    }

    #[test]
    fn large_progress_has_three_rows_and_real_percentage() {
        let state = ProgressState {
            completed: 2,
            label: "Model loaded".to_owned(),
            filename: "photo.jpg".to_owned(),
            frame: 0,
        };
        let frame = render_progress_frame_at_width(&state, Duration::from_secs(3), 40);
        assert_eq!(frame.lines().count(), 5);
        assert!(frame.contains(" 40% Model loaded"));
        assert!(frame.contains("photo.jpg · 3s"));
        assert_eq!(frame.matches('█').count(), 15);
    }

    #[test]
    fn sanitizes_terminal_controls() {
        assert_eq!(sanitize("bad\x1b[2Jname\r\n"), "badname");
    }

    #[test]
    fn cleanup_erases_all_five_progress_rows() {
        let cleanup = String::from_utf8_lossy(CLEAR_PROGRESS);
        assert_eq!(cleanup.matches("\x1b[2K").count(), 5);
        assert_eq!(cleanup.matches("\x1b[1A").count(), 4);
    }
}
