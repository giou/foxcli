use std::io::{self, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cancel;

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub fn start(enabled: bool, message: &'static str) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        if !enabled {
            return Self { done, handle: None };
        }

        let thread_done = Arc::clone(&done);
        let handle = thread::spawn(move || {
            let mut frame = 0;
            while !thread_done.load(Ordering::Relaxed) {
                eprint!("\r\x1b[2K{} {}", SPINNER[frame % SPINNER.len()], message);
                let _ = io::stderr().flush();
                frame += 1;
                thread::sleep(Duration::from_millis(90));
            }
            eprint!("\r\x1b[2K");
            let _ = io::stderr().flush();
        });

        Self {
            done,
            handle: Some(handle),
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A live multi-line progress display.
///
/// The progress block is rendered as a *sticky footer* on stderr: log lines
/// stream into the scrollback above it while the bars redraw in place below.
/// Both the animation timer and the byte observer drive rendering through the
/// same mutex, so stderr writes are serialized and the footer is never
/// duplicated.
pub struct ProgressUi {
    state: Arc<Mutex<ProgressState>>,
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    line_buf: String,
    enabled: bool,
}

impl ProgressUi {
    pub fn start(
        enabled: bool,
        color: bool,
        title: &'static str,
        items: Vec<String>,
        cancellable: bool,
    ) -> Self {
        let state = Arc::new(Mutex::new(ProgressState::new(title, items, color, cancellable)));
        let done = Arc::new(AtomicBool::new(false));

        if enabled {
            // Draw the initial footer immediately so it appears before any output.
            if let Ok(mut state) = state.lock() {
                state.render(None);
            }
        }

        let handle = if enabled {
            let thread_done = Arc::clone(&done);
            let thread_state = Arc::clone(&state);
            Some(thread::spawn(move || {
                while !thread_done.load(Ordering::Relaxed) {
                    let mut forward_cancel = false;
                    if let Ok(mut state) = thread_state.lock() {
                        state.frame = state.frame.wrapping_add(1);
                        // On the first observed Ctrl+C, forward it to the engine
                        // and switch the hint to "Cancelling...".
                        if state.cancellable && !state.cancel_sent && cancel::requested() {
                            state.cancel_sent = true;
                            state.cancelling = true;
                            forward_cancel = true;
                        }
                        state.render(None);
                    }
                    if forward_cancel {
                        cancel::send();
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }))
        } else {
            None
        };

        Self {
            state,
            done,
            handle,
            line_buf: String::new(),
            enabled,
        }
    }

    /// Feed raw response bytes. Complete log lines are streamed above the footer.
    pub fn observe(&mut self, bytes: &[u8]) {
        if !self.enabled {
            return;
        }
        let text = String::from_utf8_lossy(bytes);
        self.line_buf.push_str(&text);
        while let Some(pos) = self.line_buf.find('\n') {
            let line = self.line_buf[..pos].trim_end_matches('\r').to_string();
            self.line_buf.drain(..=pos);
            self.emit_log(&line);
        }
    }

    /// Feed an overall progress percentage from the FOX_PROGRESS channel.
    pub fn observe_progress(&mut self, percent: u8) {
        if !self.enabled {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.set_percent(percent);
            state.render(None);
        }
    }

    /// Feed the current item's own progress percentage from the FOX_PROGRESS
    /// channel (multi-partition backups/restores).
    pub fn observe_item(&mut self, percent: u8) {
        if !self.enabled {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.set_item(percent);
            state.render(None);
        }
    }

    fn emit_log(&self, line: &str) {
        let clean = strip_control(line).trim().to_string();
        if clean.is_empty() {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.note_log(&clean);
            state.render(Some(&clean));
        }
    }
}

impl Drop for ProgressUi {
    fn drop(&mut self) {
        if self.enabled && !self.line_buf.trim().is_empty() {
            let line = std::mem::take(&mut self.line_buf);
            self.emit_log(&line);
        }
        self.done.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        // Erase the footer, leaving the streamed log intact in the scrollback.
        if self.enabled {
            if let Ok(mut state) = self.state.lock() {
                state.clear_footer();
            }
        }
    }
}

struct ProgressState {
    title: &'static str,
    items: Vec<ProgressItem>,
    current_index: Option<usize>,
    overall_percent: Option<u8>,
    frame: usize,
    footer_lines: usize,
    color: bool,
    cancellable: bool,
    cancelling: bool,
    cancel_sent: bool,
}

impl ProgressState {
    fn new(title: &'static str, items: Vec<String>, color: bool, cancellable: bool) -> Self {
        let mut items: Vec<_> = items
            .into_iter()
            .map(|name| ProgressItem {
                name,
                percent: None,
                status: ItemStatus::Pending,
                explicit: false,
            })
            .collect();
        if items.is_empty() {
            items.push(ProgressItem {
                name: "operation".to_string(),
                percent: None,
                status: ItemStatus::Running,
                explicit: false,
            });
        }
        items[0].status = ItemStatus::Running;
        Self {
            title,
            items,
            current_index: Some(0),
            overall_percent: None,
            frame: 0,
            footer_lines: 0,
            color,
            cancellable,
            cancelling: false,
            cancel_sent: false,
        }
    }

    fn set_percent(&mut self, percent: u8) {
        self.overall_percent = Some(percent);
        if let Some(index) = self.current_index {
            // Mirror the overall onto the current item only until the engine
            // sends explicit per-item progress for it (so single-item flashes and
            // item-less restores still show a moving item bar).
            if !self.items[index].explicit {
                self.items[index].percent = Some(percent);
            }
            if matches!(self.items[index].status, ItemStatus::Pending) {
                self.items[index].status = ItemStatus::Running;
            }
        }
    }

    fn set_item(&mut self, percent: u8) {
        if let Some(index) = self.current_index {
            self.items[index].percent = Some(percent);
            self.items[index].explicit = true;
            if matches!(self.items[index].status, ItemStatus::Pending) {
                self.items[index].status = ItemStatus::Running;
            }
        }
    }

    /// Use a streamed log line to detect which item is now active, marking any
    /// item we move past as done.
    fn note_log(&mut self, clean: &str) {
        let lower = clean.to_ascii_lowercase();
        if let Some(index) = self.items.iter().position(|item| {
            !item.name.is_empty() && lower.contains(&item.name.to_ascii_lowercase())
        }) {
            if let Some(previous) = self.current_index {
                if previous != index {
                    self.items[previous].status = ItemStatus::Done;
                    self.items[previous].percent = Some(100);
                }
            }
            self.current_index = Some(index);
            if !matches!(self.items[index].status, ItemStatus::Done) {
                self.items[index].status = ItemStatus::Running;
            }
        }
    }

    /// Redraw the footer in place. When `log` is set, that line is first printed
    /// where the footer began so it scrolls up into the history.
    fn render(&mut self, log: Option<&str>) {
        let lines = self.build_lines();
        let stderr = io::stderr();
        let mut out = stderr.lock();
        if self.footer_lines > 0 {
            let _ = write!(out, "\x1b[{}A", self.footer_lines);
        }
        // Move to column 0 and clear everything from here to the end of screen,
        // wiping the previous footer before we repaint it.
        let _ = write!(out, "\r\x1b[0J");
        if let Some(line) = log {
            let _ = write!(out, "{line}\r\n");
        }
        for line in &lines {
            let _ = write!(out, "{line}\r\n");
        }
        self.footer_lines = lines.len();
        let _ = out.flush();
    }

    fn clear_footer(&mut self) {
        if self.footer_lines == 0 {
            return;
        }
        let stderr = io::stderr();
        let mut out = stderr.lock();
        let _ = write!(out, "\x1b[{}A\r\x1b[0J", self.footer_lines);
        self.footer_lines = 0;
        let _ = out.flush();
    }

    fn build_lines(&self) -> Vec<String> {
        let color = self.color;
        let spinner = SPINNER[self.frame % SPINNER.len()];
        let label_width = self
            .items
            .iter()
            .map(|item| truncate(&item.name, 18).chars().count())
            .chain(std::iter::once("overall".len()))
            .max()
            .unwrap_or(8);

        let mut lines = Vec::new();
        lines.push(style(color, Style::Title, self.title));

        if self.items.len() > 1 {
            // A plain-space marker keeps the bar aligned with the item rows.
            let marker = " ".to_string();
            lines.push(progress_row(
                color,
                marker,
                "overall",
                label_width,
                self.overall_percent,
            ));
        }
        for item in &self.items {
            let marker = match item.status {
                ItemStatus::Pending => " ".to_string(),
                ItemStatus::Done => style(color, Style::Done, "✓"),
                ItemStatus::Running => style(color, Style::Marker, spinner),
            };
            lines.push(progress_row(
                color,
                marker,
                &truncate(&item.name, 18),
                label_width,
                item.percent,
            ));
        }
        if self.cancellable {
            let hint = if self.cancelling {
                "Cancelling..."
            } else {
                "Press Ctrl + C to cancel"
            };
            lines.push(format!("  {}", style(color, Style::Hint, hint)));
        }
        lines
    }
}

struct ProgressItem {
    name: String,
    percent: Option<u8>,
    status: ItemStatus,
    // Set once the engine has sent explicit per-item progress for this item;
    // until then the overall percentage is mirrored onto it.
    explicit: bool,
}

#[derive(Clone, Copy)]
enum ItemStatus {
    Pending,
    Running,
    Done,
}

enum Style {
    Title,
    Marker,
    Done,
    Separator,
    BarFilled,
    BarEmpty,
    Hint,
}

/// One footer row: `  <marker> <label> | [bar] | <pct>`.
///
/// `marker` is a single display glyph (optionally ANSI-styled); it is inserted
/// without a width spec so its escape codes do not throw off the label padding.
fn progress_row(
    color: bool,
    marker: String,
    label: &str,
    label_width: usize,
    percent: Option<u8>,
) -> String {
    format!(
        "  {} {:label_width$} {} {} {} {}",
        marker,
        label,
        style(color, Style::Separator, "|"),
        bar(percent, color),
        style(color, Style::Separator, "|"),
        percent_text(percent)
    )
}

fn bar(percent: Option<u8>, color: bool) -> String {
    let width = 24_usize;
    match percent {
        Some(percent) => {
            let filled = (usize::from(percent) * width + 50) / 100;
            format!(
                "[{}{}]",
                style(color, Style::BarFilled, &"#".repeat(filled)),
                style(
                    color,
                    Style::BarEmpty,
                    &"-".repeat(width.saturating_sub(filled))
                )
            )
        }
        None => format!("[{}]", style(color, Style::BarEmpty, &"-".repeat(width))),
    }
}

fn percent_text(percent: Option<u8>) -> String {
    percent
        .map(|value| format!("{value:>3}%"))
        .unwrap_or_else(|| " --%".to_string())
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars && max_chars > 1 {
        out.pop();
        out.push('…');
    }
    out
}

fn strip_control(line: &str) -> String {
    line.chars()
        .filter(|ch| *ch == '\t' || !ch.is_control())
        .collect()
}

fn style(enabled: bool, style: Style, text: &str) -> String {
    if !enabled {
        return text.to_string();
    }
    let code = match style {
        Style::Title => "38;5;208",
        Style::Marker => "36",
        Style::Done => "32",
        Style::Separator => "2",
        Style::BarFilled => "32",
        Style::BarEmpty => "2",
        Style::Hint => "38;5;244",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}
