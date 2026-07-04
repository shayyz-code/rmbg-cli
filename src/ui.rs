use std::env;
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anstream::{AutoStream, ColorChoice as StreamColorChoice};
use clap::builder::styling::{AnsiColor, Color, RgbColor, Style, Styles};
use clap::ColorChoice;

const PURPLE: RgbColor = RgbColor(168, 85, 247);
const PINK: RgbColor = RgbColor(236, 72, 153);
const PALE_PINK: RgbColor = RgbColor(245, 208, 254);
const MUTED: RgbColor = RgbColor(148, 163, 184);
const RED: RgbColor = RgbColor(248, 113, 113);
const GREEN: RgbColor = RgbColor(74, 222, 128);

const SPINNER_GLYPHS: [&str; 10] = ["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"];
const SHIMMER: [RgbColor; 5] = [
    RgbColor(126, 34, 206),
    PURPLE,
    PALE_PINK,
    PINK,
    RgbColor(190, 24, 93),
];

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
}

impl Ui {
    pub fn new(color: ColorChoice) -> Self {
        let interactive = io::stderr().is_terminal()
            && env::var("TERM").map_or(true, |term| !term.eq_ignore_ascii_case("dumb"));
        Self { color, interactive }
    }

    pub fn is_interactive(self) -> bool {
        self.interactive
    }

    pub fn detail(self, label: &str, value: &str) {
        self.line(format!(
            "  {}{}{} {value}",
            style(MUTED),
            label,
            style(MUTED).render_reset()
        ));
    }

    pub fn step(self, current: usize, total: usize, message: &str) {
        self.line(format!(
            "{}[{current}/{total}]{} {message}",
            style(PINK),
            style(PINK).render_reset()
        ));
    }

    pub fn notice(self, message: &str) {
        self.line(format!(
            "{}!{} {message}",
            style(PINK).bold(),
            style(PINK).bold().render_reset()
        ));
    }

    pub fn success(self, message: &str) {
        self.line(format!(
            "{}✓{} {message}",
            style(GREEN).bold(),
            style(GREEN).bold().render_reset()
        ));
    }

    pub fn error(self, message: &str) {
        self.line(format!(
            "{}error:{} {message}",
            style(RED).bold(),
            style(RED).bold().render_reset()
        ));
    }

    pub fn processing(self, filename: &str, verbose: bool) -> ProcessingStatus {
        let visible = self.interactive || verbose;
        if !visible {
            return ProcessingStatus::hidden();
        }

        let started = Instant::now();
        if !self.interactive {
            self.line(format!("Removing background from {filename}..."));
            return ProcessingStatus {
                started,
                animation: None,
            };
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let filename = filename.to_owned();
        let choice = stream_choice(self.color);
        let handle = thread::spawn(move || {
            let mut frame = 0;
            while !thread_stop.load(Ordering::Relaxed) {
                let line = render_processing_frame(frame, &filename, started.elapsed());
                clear_status_line();
                write_stderr(choice, &line);
                frame = frame.wrapping_add(1);
                for _ in 0..9 {
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });

        ProcessingStatus {
            started,
            animation: Some(Animation {
                stop,
                handle: Some(handle),
            }),
        }
    }

    fn line(self, line: String) {
        write_stderr(stream_choice(self.color), &format!("{line}\n"));
    }
}

pub struct ProcessingStatus {
    started: Instant,
    animation: Option<Animation>,
}

impl ProcessingStatus {
    fn hidden() -> Self {
        Self {
            started: Instant::now(),
            animation: None,
        }
    }

    pub fn stop(&mut self) -> Duration {
        if let Some(mut animation) = self.animation.take() {
            animation.stop();
        }
        self.started.elapsed()
    }
}

impl Drop for ProcessingStatus {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Animation {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Animation {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        clear_status_line();
    }
}

fn render_processing_frame(frame: usize, filename: &str, elapsed: Duration) -> String {
    let mut pixels = String::new();
    for offset in 0..5 {
        let glyph = SPINNER_GLYPHS[(frame + offset) % SPINNER_GLYPHS.len()];
        let color = style(SHIMMER[(frame + offset) % SHIMMER.len()]);
        let _ = write!(pixels, "{color}{glyph}{color:#}");
    }

    format!(
        "{pixels} {}Removing background{} {}{filename} · {}{}",
        style(PURPLE).bold(),
        style(PURPLE).bold().render_reset(),
        style(MUTED),
        format_duration(elapsed),
        style(MUTED).render_reset()
    )
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

fn clear_status_line() {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(b"\r\x1b[2K");
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
    fn processing_frames_contain_gradient_pixels_and_context() {
        let frame = render_processing_frame(0, "photo.jpg", Duration::from_secs(3));
        assert!(frame.contains("·"));
        assert!(frame.contains("✢"));
        assert!(frame.contains("Removing background"));
        assert!(frame.contains("photo.jpg · 3s"));
        assert!(frame.contains("\x1b["));
    }
}
