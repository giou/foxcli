use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::thread;
use std::time::Duration;

const FOX_INPUT_FILE: &str = "/system/bin/foxin";
const FOX_OUTPUT_FILE: &str = "/system/bin/foxout";
const EXIT_SENTINEL: &[u8] = b"\x04EXIT ";
const PROGRESS_PREFIX: &str = "FOX_PROGRESS\t";

pub struct EngineResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub progress_percent: Option<u8>,
}

pub enum ResponseEvent {
    Output(Vec<u8>),
    Progress(ProgressUpdate),
}

/// A single parsed `FOX_PROGRESS` field: the overall operation percentage, or
/// the current item's (partition/file) percentage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressUpdate {
    Overall(u8),
    Item(u8),
}

pub fn send_command(argv: &[String], quiet: bool) -> io::Result<EngineResponse> {
    send_command_observed(argv, quiet, None)
}

pub fn send_command_raw(argv: &[String], quiet: bool) -> io::Result<EngineResponse> {
    send_command_inner(argv, quiet, None, false)
}

pub fn send_command_observed(
    argv: &[String],
    quiet: bool,
    observer: Option<&mut dyn FnMut(ResponseEvent)>,
) -> io::Result<EngineResponse> {
    send_command_inner(argv, quiet, observer, true)
}

fn send_command_inner(
    argv: &[String],
    quiet: bool,
    observer: Option<&mut dyn FnMut(ResponseEvent)>,
    filter_progress: bool,
) -> io::Result<EngineResponse> {
    let request = encode_argv(argv)?;
    let mut input = open_input_fifo()?;
    input.write_all(&request)?;
    drop(input);

    let mut output = OpenOptions::new().read(true).open(FOX_OUTPUT_FILE)?;
    read_response(&mut output, quiet, observer, filter_progress)
}

fn encode_argv(argv: &[String]) -> io::Result<Vec<u8>> {
    let mut request = Vec::new();
    for arg in argv {
        if arg.as_bytes().contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "arguments cannot contain NUL bytes",
            ));
        }
        request.extend_from_slice(arg.as_bytes());
        request.push(0);
    }
    request.push(0);
    if request.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "encoded command is too long",
        ));
    }
    Ok(request)
}

fn open_input_fifo() -> io::Result<std::fs::File> {
    loop {
        match OpenOptions::new().write(true).open(FOX_INPUT_FILE) {
            Ok(file) => return Ok(file),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                eprintln!(
                    "OrangeFox does not appear to be running. Waiting for OrangeFox to start . . ."
                );
                eprintln!("Press CTRL + C to quit.");
                break;
            }
            Err(err) => return Err(err),
        }
    }

    loop {
        match OpenOptions::new().write(true).open(FOX_INPUT_FILE) {
            Ok(file) => return Ok(file),
            Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    }
}

fn read_response(
    output: &mut std::fs::File,
    quiet: bool,
    mut observer: Option<&mut dyn FnMut(ResponseEvent)>,
    filter_progress: bool,
) -> io::Result<EngineResponse> {
    let mut pending = Vec::new();
    let mut captured = Vec::new();
    let mut filter = ResponseFilter::new(filter_progress);
    let mut chunk = [0_u8; 512];

    loop {
        let read = output.read(&mut chunk)?;
        if read == 0 {
            emit_filtered(
                &mut filter,
                &pending,
                true,
                quiet,
                &mut captured,
                &mut observer,
            )?;
            return Ok(EngineResponse {
                exit_code: 1,
                stdout: String::from_utf8_lossy(&captured).into_owned(),
                progress_percent: filter.progress_percent,
            });
        }

        pending.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_sentinel(&pending) {
            emit_filtered(
                &mut filter,
                &pending[..pos],
                true,
                quiet,
                &mut captured,
                &mut observer,
            )?;
            let mut exit_line = pending[pos + EXIT_SENTINEL.len()..].to_vec();
            output.read_to_end(&mut exit_line)?;
            return Ok(EngineResponse {
                exit_code: parse_exit_code(&exit_line),
                stdout: String::from_utf8_lossy(&captured).into_owned(),
                progress_percent: filter.progress_percent,
            });
        }

        let keep = EXIT_SENTINEL.len().saturating_sub(1);
        if pending.len() > keep {
            let write_len = pending.len() - keep;
            emit_filtered(
                &mut filter,
                &pending[..write_len],
                false,
                quiet,
                &mut captured,
                &mut observer,
            )?;
            pending.drain(..write_len);
        }
    }
}

fn emit_filtered(
    filter: &mut ResponseFilter,
    bytes: &[u8],
    flush: bool,
    quiet: bool,
    captured: &mut Vec<u8>,
    observer: &mut Option<&mut dyn FnMut(ResponseEvent)>,
) -> io::Result<()> {
    for event in filter.process(bytes, flush) {
        match event {
            ResponseEvent::Output(bytes) => {
                observe(observer, ResponseEvent::Output(bytes.clone()));
                write_plain(&bytes, quiet)?;
                captured.extend_from_slice(&bytes);
            }
            ResponseEvent::Progress(percent) => {
                observe(observer, ResponseEvent::Progress(percent));
            }
        }
    }
    Ok(())
}

fn observe(observer: &mut Option<&mut dyn FnMut(ResponseEvent)>, event: ResponseEvent) {
    if let Some(callback) = observer.as_deref_mut() {
        callback(event);
    }
}

fn write_plain(bytes: &[u8], quiet: bool) -> io::Result<()> {
    if !quiet {
        io::stdout().write_all(bytes)?;
    }
    Ok(())
}

fn parse_exit_code(line: &[u8]) -> i32 {
    let code_end = line.iter().position(|&b| b == b'\n').unwrap_or(line.len());
    let code_text = std::str::from_utf8(&line[..code_end]).unwrap_or("1");
    code_text.trim().parse().unwrap_or(1)
}

fn find_sentinel(buf: &[u8]) -> Option<usize> {
    buf.windows(EXIT_SENTINEL.len())
        .position(|window| window == EXIT_SENTINEL)
}

struct ResponseFilter {
    enabled: bool,
    line_buf: Vec<u8>,
    progress_percent: Option<u8>,
}

impl ResponseFilter {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            line_buf: Vec::new(),
            progress_percent: None,
        }
    }

    fn process(&mut self, bytes: &[u8], flush: bool) -> Vec<ResponseEvent> {
        if !self.enabled {
            return if bytes.is_empty() {
                Vec::new()
            } else {
                vec![ResponseEvent::Output(bytes.to_vec())]
            };
        }

        let mut events = Vec::new();
        for &byte in bytes {
            self.line_buf.push(byte);
            if byte == b'\n' {
                self.flush_line(&mut events);
            }
        }
        if flush && !self.line_buf.is_empty() {
            self.flush_line(&mut events);
        }
        events
    }

    fn flush_line(&mut self, events: &mut Vec<ResponseEvent>) {
        let line = std::mem::take(&mut self.line_buf);
        if let Some(updates) = parse_progress_line(&line) {
            for update in updates {
                if let ProgressUpdate::Overall(percent) = update {
                    self.progress_percent = Some(percent);
                }
                events.push(ResponseEvent::Progress(update));
            }
        } else {
            events.push(ResponseEvent::Output(line));
        }
    }
}

/// Parse a `FOX_PROGRESS\t<field>=<n>...` line into its updates.
///
/// Returns `None` (so the line is kept as ordinary output) when the line is not
/// a progress line or is malformed -- a field without `=`, an unparsable
/// number, an out-of-range value, or a line with no recognized fields. A
/// well-formed but unknown field (a future extension) is ignored, so adding new
/// fields stays backward compatible as long as a known field is present.
fn parse_progress_line(line: &[u8]) -> Option<Vec<ProgressUpdate>> {
    let text = std::str::from_utf8(line)
        .ok()?
        .trim_end_matches(|ch| ch == '\r' || ch == '\n');
    let fields = text.strip_prefix(PROGRESS_PREFIX)?;
    let mut updates = Vec::new();
    for field in fields.split('\t') {
        if field.is_empty() {
            continue;
        }
        let (key, value) = field.split_once('=')?;
        let percent = value.parse::<f32>().ok()?;
        if !(0.0..=100.0).contains(&percent) {
            return None;
        }
        let percent = percent.round() as u8;
        match key {
            "percent" => updates.push(ProgressUpdate::Overall(percent)),
            "item" => updates.push(ProgressUpdate::Item(percent)),
            _ => {} // ignore unknown (forward-compatible) fields
        }
    }
    if updates.is_empty() {
        None
    } else {
        Some(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProgressUpdate, ResponseEvent, ResponseFilter};

    fn collect(bytes: &[u8], flush: bool) -> (String, Vec<ProgressUpdate>, Option<u8>) {
        let mut filter = ResponseFilter::new(true);
        let mut out = Vec::new();
        let mut progress = Vec::new();
        for event in filter.process(bytes, flush) {
            match event {
                ResponseEvent::Output(bytes) => out.extend_from_slice(&bytes),
                ResponseEvent::Progress(update) => progress.push(update),
            }
        }
        (
            String::from_utf8_lossy(&out).into_owned(),
            progress,
            filter.progress_percent,
        )
    }

    #[test]
    fn consumes_progress_lines() {
        let (out, progress, final_progress) = collect(b"FOX_PROGRESS\tpercent=42\n", true);
        assert_eq!(out, "");
        assert_eq!(progress, vec![ProgressUpdate::Overall(42)]);
        assert_eq!(final_progress, Some(42));
    }

    #[test]
    fn consumes_item_progress_lines() {
        let (out, progress, final_progress) = collect(b"FOX_PROGRESS\titem=12\n", true);
        assert_eq!(out, "");
        assert_eq!(progress, vec![ProgressUpdate::Item(12)]);
        // item-only lines do not update the overall percentage
        assert_eq!(final_progress, None);
    }

    #[test]
    fn consumes_combined_progress_lines() {
        let (out, progress, final_progress) =
            collect(b"FOX_PROGRESS\tpercent=42\titem=12\n", true);
        assert_eq!(out, "");
        assert_eq!(
            progress,
            vec![ProgressUpdate::Overall(42), ProgressUpdate::Item(12)]
        );
        assert_eq!(final_progress, Some(42));
    }

    #[test]
    fn ignores_unknown_progress_fields() {
        // A known field alongside an unknown one is consumed; the unknown field
        // is dropped rather than leaking into the log.
        let (out, progress, _) = collect(b"FOX_PROGRESS\tpercent=42\tspeed=5\n", true);
        assert_eq!(out, "");
        assert_eq!(progress, vec![ProgressUpdate::Overall(42)]);
    }

    #[test]
    fn keeps_malformed_progress_lines() {
        let (out, progress, final_progress) = collect(b"FOX_PROGRESS\tpercent=nope\n", true);
        assert_eq!(out, "FOX_PROGRESS\tpercent=nope\n");
        assert!(progress.is_empty());
        assert_eq!(final_progress, None);
    }

    #[test]
    fn handles_mixed_log_and_progress_lines() {
        let (out, progress, final_progress) =
            collect(b"Backing up Data...\nFOX_PROGRESS\tpercent=7\nDone\n", true);
        assert_eq!(out, "Backing up Data...\nDone\n");
        assert_eq!(progress, vec![ProgressUpdate::Overall(7)]);
        assert_eq!(final_progress, Some(7));
    }

    #[test]
    fn raw_mode_preserves_progress_lines() {
        let mut filter = ResponseFilter::new(false);
        let events = filter.process(b"FOX_PROGRESS\tpercent=42\n", true);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ResponseEvent::Output(bytes) => {
                assert_eq!(String::from_utf8_lossy(bytes), "FOX_PROGRESS\tpercent=42\n");
            }
            ResponseEvent::Progress(update) => panic!("unexpected progress event {update:?}"),
        }
        assert_eq!(filter.progress_percent, None);
    }
}
