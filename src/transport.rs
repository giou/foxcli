use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

const FOX_INPUT_FILE: &str = "/system/bin/foxin";
const FOX_OUTPUT_FILE: &str = "/system/bin/foxout";
const PROGRESS_PREFIX: &str = "FOX_PROGRESS\t";

pub struct EngineResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub progress_percent: Option<u8>,
    /// Structured `data` events from the engine (status/list/etc.), in arrival
    /// order. Replaces scraping `FOX_*_BEGIN/END` text blocks out of stdout.
    pub data: Vec<DataEvent>,
}

pub struct DataEvent {
    pub name: String,
    pub value: Value,
}

impl EngineResponse {
    /// The payload of the first `data` event with the given name, if any.
    pub fn data(&self, name: &str) -> Option<&Value> {
        self.data.iter().find(|d| d.name == name).map(|d| &d.value)
    }

    /// The first non-`list` data payload, for generic rendering of status/info
    /// objects (`webserver`/`http`/`adb_devices`/…). The partition `list` is
    /// rendered by a dedicated table path, so it is excluded here.
    pub fn status_object(&self) -> Option<&Value> {
        self.data
            .iter()
            .find(|d| d.name != "list")
            .map(|d| &d.value)
    }
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

// Typed entry points: the request (op + args) is built by Command::to_request()
// with no argv round-trip.
pub fn send_request(op: &str, args: &Value, quiet: bool) -> io::Result<EngineResponse> {
    send_encoded(encode_request(op, args)?, quiet, None, true)
}

pub fn send_request_observed(
    op: &str,
    args: &Value,
    quiet: bool,
    observer: Option<&mut dyn FnMut(ResponseEvent)>,
) -> io::Result<EngineResponse> {
    send_encoded(encode_request(op, args)?, quiet, observer, true)
}

// Token entry points: used by the raw `fifo send` bridge and the internal
// status/page probes, which are genuinely token-shaped (no typed struct).
pub fn send_command(argv: &[String], quiet: bool) -> io::Result<EngineResponse> {
    send_encoded(encode_rpc_request(argv)?, quiet, None, true)
}

pub fn send_command_raw(argv: &[String], quiet: bool) -> io::Result<EngineResponse> {
    send_encoded(encode_rpc_request(argv)?, quiet, None, false)
}

fn send_encoded(
    request: Vec<u8>,
    quiet: bool,
    observer: Option<&mut dyn FnMut(ResponseEvent)>,
    filter_progress: bool,
) -> io::Result<EngineResponse> {
    let mut input = open_input_fifo()?;
    input.write_all(&request)?;
    drop(input);

    let mut output = OpenOptions::new().read(true).open(FOX_OUTPUT_FILE)?;
    read_response(&mut output, quiet, observer, filter_progress)
}

pub fn send_rpc_raw(request: &str) -> io::Result<i32> {
    let mut input = open_input_fifo()?;
    input.write_all(request.as_bytes())?;
    drop(input);

    let output = OpenOptions::new().read(true).open(FOX_OUTPUT_FILE)?;
    let mut output = BufReader::new(output);
    let mut line = String::new();
    let mut result_code = 0;
    while output.read_line(&mut line)? != 0 {
        io::stdout().write_all(line.as_bytes())?;
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if value.get("event").and_then(Value::as_str) == Some("result") {
                result_code = value.get("code").and_then(Value::as_i64).unwrap_or(1) as i32;
            }
        }
        line.clear();
    }
    Ok(result_code)
}

fn encode_request(op: &str, args: &Value) -> io::Result<Vec<u8>> {
    let request = json!({
        "v": 1,
        "op": op,
        "args": args,
    });
    let request = serde_json::to_vec(&request).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("failed to encode fox rpc request: {err}"),
        )
    })?;
    if request.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "encoded command is too long",
        ));
    }
    Ok(request)
}

// Token path for the raw bridge / probes: tokens → (op, args) → encoded request.
fn encode_rpc_request(argv: &[String]) -> io::Result<Vec<u8>> {
    let (op, args) = argv_to_rpc(argv)?;
    encode_request(op, &args)
}

fn argv_to_rpc(argv: &[String]) -> io::Result<(&str, Value)> {
    let op = argv
        .first()
        .map(String::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty fox rpc command"))?;
    let rest = &argv[1..];
    let args = match op {
        "mount" | "unmount" => json!({ "paths": rest }),
        "flash" => flash_args(rest),
        "backup" => backup_args(rest),
        "restore" => restore_args(rest),
        "decrypt" => decrypt_args(rest),
        "wipe" => json!({ "parts": rest }),
        "format_data" => {
            json!({ "confirm": rest.iter().any(|v| v == "--yes" || v == "--confirm") })
        }
        "reboot" => json!({ "target": rest.first() }),
        "wlan" => wlan_args(rest),
        "web" | "http" | "https" | "http_redirect" | "adbd" | "ota" | "mtp" | "webserver"
        | "control" | "screenstream" => action_args(rest),
        "input" => input_args(rest),
        "screencap" => screencap_args(rest),
        "partition" => partition_args(rest),
        "addons" => addons_args(rest),
        "internal" => internal_args(rest),
        "ors" => json!({ "path": rest.first() }),
        "ors-cmd" => json!({ "raw": rest }),
        _ => json!({}),
    };
    Ok((op, args))
}

fn take_option(args: &[String], name: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            return args.get(i + 1).cloned();
        }
        if let Some(value) = args[i].strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
        i += 1;
    }
    None
}

fn positional(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if matches!(
            arg.as_str(),
            "--name"
                | "--path"
                | "--storage"
                | "--password"
                | "--port"
                | "--timeout"
                | "--fox-id"
                | "--type"
                | "--fps"
                | "--user"
        ) {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        out.push(arg.clone());
    }
    out
}

fn first_positional(args: &[String]) -> Option<String> {
    positional(args).into_iter().next()
}

fn flash_args(args: &[String]) -> Value {
    let verify = if args.iter().any(|v| v == "--verify") {
        Some(true)
    } else if args.iter().any(|v| v == "--no-verify") {
        Some(false)
    } else {
        None
    };
    let reflash = if args
        .iter()
        .any(|v| v == "--reflash-orangefox" || v == "--reflash")
    {
        Some(true)
    } else if args.iter().any(|v| v == "--no-reflash") {
        Some(false)
    } else {
        None
    };
    json!({
        "zips": positional(args),
        "verify": verify,
        "reflash": reflash,
        "unmount_system": args.iter().any(|v| v == "--unmount-system"),
        "unmount_vendor": args.iter().any(|v| v == "--unmount-vendor"),
    })
}

fn backup_args(args: &[String]) -> Value {
    json!({
        "parts": positional(args),
        "name": take_option(args, "--name"),
        "path": take_option(args, "--path"),
        "storage": take_option(args, "--storage"),
        "compress": args.iter().any(|v| v == "--compression" || v == "--compress"),
        "digest": !args.iter().any(|v| v == "--disable-digest-generation" || v == "--no-digest"),
    })
}

fn restore_args(args: &[String]) -> Value {
    json!({
        "parts": positional(args),
        "name": take_option(args, "--name"),
        "path": take_option(args, "--path"),
        "digest_check": !args.iter().any(|v| v == "--disable-digest-check" || v == "--no-digest-check"),
    })
}

fn decrypt_args(args: &[String]) -> Value {
    json!({
        "password": first_positional(args),
        "user": take_option(args, "--user").and_then(|v| v.parse::<u32>().ok()),
    })
}

fn wlan_args(args: &[String]) -> Value {
    let ssid = args.get(1..).and_then(first_positional);
    json!({
        "action": args.first(),
        "ssid": ssid,
        "password": take_option(args, "--password"),
    })
}

fn action_args(args: &[String]) -> Value {
    let positionals = positional(args);
    json!({
        "action": positionals.first(),
        "subaction": positionals.get(1),
        "values": positionals.iter().skip(2).cloned().collect::<Vec<_>>(),
        "port": take_option(args, "--port").and_then(|v| v.parse::<u16>().ok()),
        "timeout": take_option(args, "--timeout").and_then(|v| v.parse::<u32>().ok()),
        "fox_id": take_option(args, "--fox-id"),
        "type": take_option(args, "--type"),
        "fps": take_option(args, "--fps").and_then(|v| v.parse::<u32>().ok()),
        "no_auth": args.iter().any(|v| v == "--no-auth"),
        "generate": args.iter().any(|v| v == "--generate"),
    })
}

fn input_args(args: &[String]) -> Value {
    let nums: Vec<u32> = args.iter().skip(1).filter_map(|v| v.parse().ok()).collect();
    json!({
        "action": args.first(),
        "key": args.get(1).filter(|_| args.first().map(String::as_str) == Some("key")),
        "x": nums.first(),
        "y": nums.get(1),
        "x2": nums.get(2),
        "y2": nums.get(3),
        "duration": nums.get(4),
    })
}

fn screencap_args(args: &[String]) -> Value {
    json!({
        "path": first_positional(args),
        "base64": args.iter().any(|v| v == "--base64"),
    })
}

fn partition_args(args: &[String]) -> Value {
    json!({
        "path": args.first(),
        "action": args.get(1),
        "fs": args.get(2),
    })
}

fn addons_args(args: &[String]) -> Value {
    let names: Vec<String> = args.iter().skip(1).cloned().collect();
    json!({
        "action": args.first(),
        "names": names,
    })
}

fn internal_args(args: &[String]) -> Value {
    json!({
        "action": args.first(),
        "name": args.get(1),
        "value": args.get(2),
        "page": args.get(1),
    })
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
    let mut pending = String::new();
    let mut captured = Vec::new();
    let mut data: Vec<DataEvent> = Vec::new();
    let mut filter = ResponseFilter::new(filter_progress);
    let mut chunk = [0_u8; 512];

    loop {
        let read = output.read(&mut chunk)?;
        if read == 0 {
            if !pending.is_empty() {
                process_event_line(
                    &pending,
                    &mut filter,
                    quiet,
                    &mut captured,
                    &mut data,
                    &mut observer,
                )?;
            }
            return Ok(EngineResponse {
                exit_code: 1,
                stdout: String::from_utf8_lossy(&captured).into_owned(),
                progress_percent: filter.progress_percent,
                data,
            });
        }

        pending.push_str(&String::from_utf8_lossy(&chunk[..read]));
        while let Some(pos) = pending.find('\n') {
            let line = pending[..pos].to_string();
            pending.drain(..=pos);
            if let Some(code) = process_event_line(
                &line,
                &mut filter,
                quiet,
                &mut captured,
                &mut data,
                &mut observer,
            )? {
                return Ok(EngineResponse {
                    exit_code: code,
                    stdout: String::from_utf8_lossy(&captured).into_owned(),
                    progress_percent: filter.progress_percent,
                    data,
                });
            }
        }
    }
}

fn process_event_line(
    line: &str,
    filter: &mut ResponseFilter,
    quiet: bool,
    captured: &mut Vec<u8>,
    data: &mut Vec<DataEvent>,
    observer: &mut Option<&mut dyn FnMut(ResponseEvent)>,
) -> io::Result<Option<i32>> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => {
            emit_filtered(filter, line.as_bytes(), true, quiet, captured, observer)?;
            return Ok(None);
        }
    };
    match value.get("event").and_then(Value::as_str) {
        Some("log") => {
            if let Some(text) = value.get("text").and_then(Value::as_str) {
                emit_filtered(filter, text.as_bytes(), true, quiet, captured, observer)?;
            }
            Ok(None)
        }
        Some("data") => {
            let name = value.get("name").and_then(Value::as_str).unwrap_or("");
            if name == "screencap" {
                // Screencap stays a stdout stream (the base64 image bytes).
                if let Some(base64) = value
                    .get("value")
                    .and_then(|v| v.get("base64"))
                    .and_then(Value::as_str)
                {
                    let mut line = base64.as_bytes().to_vec();
                    line.push(b'\n');
                    emit_filtered(filter, &line, true, quiet, captured, observer)?;
                }
            } else if let Some(payload) = value.get("value") {
                // Structured result (status/list/etc.) — collect for the renderer.
                data.push(DataEvent {
                    name: name.to_string(),
                    value: payload.clone(),
                });
            }
            Ok(None)
        }
        Some("progress") => {
            if let Some(percent) = value.get("percent").and_then(Value::as_u64) {
                let percent = percent.min(100) as u8;
                let update = if value.get("phase").and_then(Value::as_str) == Some("item") {
                    ProgressUpdate::Item(percent)
                } else {
                    filter.progress_percent = Some(percent);
                    ProgressUpdate::Overall(percent)
                };
                observe(observer, ResponseEvent::Progress(update));
            }
            Ok(None)
        }
        Some("error") => {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("fox rpc error");
            let mut line = message.as_bytes().to_vec();
            line.push(b'\n');
            emit_filtered(filter, &line, true, quiet, captured, observer)?;
            Ok(None)
        }
        Some("result") => Ok(Some(
            value.get("code").and_then(Value::as_i64).unwrap_or(1) as i32
        )),
        _ => Ok(None),
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
        let (out, progress, final_progress) = collect(b"FOX_PROGRESS\tpercent=42\titem=12\n", true);
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
