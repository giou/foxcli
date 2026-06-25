mod args;
mod cancel;
mod output;
mod patterns;
mod status;
mod terminal_ui;
mod transport;

use clap::Parser;
use std::io::Read;
use std::process;

use args::{Cli, Command, CommandPage};
use output::{CommandResult, Renderer};
use terminal_ui::{ProgressUi, Spinner};
use serde_json::{json, Value};
use transport::{
    send_command, send_command_raw, send_request, send_request_observed, send_rpc_raw,
    ProgressUpdate, ResponseEvent,
};

fn main() {
    let cli = Cli::parse();
    let renderer = Renderer::new(cli.json, cli.no_color);

    let code = match run(cli, &renderer) {
        Ok(code) => code,
        Err(err) => {
            renderer.error(&err.to_string());
            1
        }
    };
    process::exit(code);
}

fn run(cli: Cli, renderer: &Renderer) -> std::io::Result<i32> {
    if matches!(
        cli.command.as_ref(),
        Some(Command::Decrypt(args::Decrypt { password: None, .. }))
    ) {
        renderer.decrypt_help(patterns::pattern_help());
        return Ok(0);
    }

    if let Some(page) = cli.command.as_ref().and_then(Command::page) {
        return show_command_page(page, renderer);
    }

    match cli.command {
        None => show_start_page(renderer),
        Some(Command::Status) => show_start_page(renderer),
        Some(Command::Rpc(_)) => run_rpc_bridge(),
        Some(Command::Sideload(_)) => {
            // Printed before the engine starts streaming; also echoed into the
            // JSON object via `json_note` for non-streaming consumers.
            let note = "Successfully started sideload. You can use: adb sideload <path.zip>";
            renderer.sideload_notice(note);
            run_engine_command("sideload", "sideload", json!({}), renderer, Some(note), false)
        }
        // The raw debug bridge: send the verbatim tokens straight through.
        Some(Command::Fifo(fifo)) => run_fifo(fifo.into_tokens(), renderer),
        Some(command) => {
            let name = command.display_name();
            let render_plain = command.renders_plain();
            let (op, args) = command.to_request();
            run_engine_command(name, op, args, renderer, None, render_plain)
        }
    }
}

fn run_rpc_bridge() -> std::io::Result<i32> {
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request)?;
    send_rpc_raw(&request)
}

fn show_start_page(renderer: &Renderer) -> std::io::Result<i32> {
    let response = {
        let _spinner = Spinner::start(renderer.interactive(), "Reading recovery status...");
        send_command(&["status".to_string()], true)?
    };
    let snapshot = status::StatusSnapshot::from_data(response.data("status"));
    renderer.start_page(&snapshot, response.exit_code);
    Ok(response.exit_code)
}

fn show_command_page(page: CommandPage, renderer: &Renderer) -> std::io::Result<i32> {
    let probe = match page {
        CommandPage::Mount => Some(vec!["mount".to_string()]),
        CommandPage::Unmount => Some(vec!["unmount".to_string()]),
        CommandPage::Backup => Some(vec!["backup".to_string()]),
        CommandPage::Wipe => Some(vec!["wipe".to_string()]),
        CommandPage::Partition => Some(vec!["partition".to_string()]),
        CommandPage::Addons => Some(vec!["addons".to_string()]),
        CommandPage::Storages => Some(vec!["storages".to_string()]),
        CommandPage::Wlan => Some(vec!["wlan".to_string(), "info".to_string()]),
        CommandPage::Mtp => Some(vec!["mtp".to_string(), "status".to_string()]),
        CommandPage::Flash
        | CommandPage::Decrypt
        | CommandPage::Sideload
        | CommandPage::Restore
        | CommandPage::Reboot
        | CommandPage::FormatData
        | CommandPage::Log
        | CommandPage::Reflash
        | CommandPage::Fifo
        | CommandPage::Internal
        | CommandPage::Ors
        | CommandPage::OrsCmd => None,
    };

    let response = match probe {
        Some(argv) => {
            let response = {
                let _spinner = Spinner::start(renderer.interactive(), probe_message(page));
                send_command(&argv, true)?
            };
            Some(response)
        }
        None => None,
    };
    // Each page's primary structured result: the partition `list` for the
    // listing pages, the `mtp` object for the MTP page.
    let data = response.as_ref().and_then(|r| match page {
        CommandPage::Mtp => r.data("mtp"),
        _ => r.data("list"),
    });
    renderer.command_page(
        page,
        response.as_ref().map(|r| r.exit_code).unwrap_or(0),
        response.as_ref().map(|r| r.stdout.as_str()).unwrap_or(""),
        data,
    );
    Ok(response.map(|r| r.exit_code).unwrap_or(0))
}

fn run_engine_command(
    name: &'static str,
    op: &str,
    args: Value,
    renderer: &Renderer,
    json_note: Option<&str>,
    render_plain: bool,
) -> std::io::Result<i32> {
    let progress = progress_title(name);
    let quiet = renderer.json() || render_plain || (renderer.interactive() && progress.is_some());
    let response = if let Some(title) = progress {
        // Only install the Ctrl+C handler for interactive cancellable commands;
        // otherwise leave the default SIGINT (terminate) behavior in place.
        let can_cancel = command_cancellable(name);
        if can_cancel && renderer.interactive() {
            cancel::install();
        }
        let mut progress = ProgressUi::start(
            renderer.interactive(),
            renderer.color(),
            title,
            progress_items(&args),
            can_cancel,
        );
        let mut observer = |event: ResponseEvent| match event {
            ResponseEvent::Output(bytes) => progress.observe(&bytes),
            ResponseEvent::Progress(ProgressUpdate::Overall(percent)) => {
                progress.observe_progress(percent)
            }
            ResponseEvent::Progress(ProgressUpdate::Item(percent)) => {
                progress.observe_item(percent)
            }
        };
        send_request_observed(op, &args, quiet, Some(&mut observer))?
    } else {
        send_request(op, &args, quiet)?
    };
    renderer.command_response(CommandResult {
        command: name,
        exit_code: response.exit_code,
        stdout: &response.stdout,
        progress_percent: response.progress_percent,
        json_note,
        streamed: !quiet || (renderer.interactive() && progress.is_some()),
        list: response.data("list"),
        status: response.status_object(),
    });
    Ok(response.exit_code)
}

// The raw `fifo send` bridge: stream the verbatim tokens (argv_to_rpc builds the
// request engine-side of the FIFO) through the normal renderer.
fn run_fifo(tokens: Vec<String>, renderer: &Renderer) -> std::io::Result<i32> {
    let quiet = renderer.json();
    let response = send_command_raw(&tokens, quiet)?;
    renderer.command_response(CommandResult {
        command: "fifo",
        exit_code: response.exit_code,
        stdout: &response.stdout,
        progress_percent: response.progress_percent,
        json_note: None,
        streamed: !quiet,
        list: response.data("list"),
        status: response.status_object(),
    });
    Ok(response.exit_code)
}

fn probe_message(page: CommandPage) -> &'static str {
    match page {
        CommandPage::Mount | CommandPage::Unmount => "Reading mount state...",
        CommandPage::Backup => "Reading backup partitions...",
        CommandPage::Wipe => "Reading wipe targets...",
        CommandPage::Partition => "Reading partition details...",
        CommandPage::Addons => "Reading OrangeFox addons...",
        CommandPage::Storages => "Reading storage devices...",
        CommandPage::Wlan => "Reading WLAN status...",
        CommandPage::Mtp => "Reading MTP status...",
        _ => "Reading recovery details...",
    }
}

fn progress_title(command: &str) -> Option<&'static str> {
    match command {
        "backup" => Some("Backup"),
        "restore" => Some("Restore"),
        "flash" => Some("Flash"),
        _ => None,
    }
}

/// Commands the engine can safely abort mid-operation. The recovery side cancels
/// these via `PartitionManager.Cancel_Backup()` over the cancel FIFO. Flashing
/// is intentionally excluded -- interrupting a zip mid-write can brick a device.
fn command_cancellable(command: &str) -> bool {
    matches!(command, "backup")
}

fn progress_items(args: &Value) -> Vec<String> {
    // The progress UI seeds its item list from the partition/zip list of the
    // command being run (flash → zips, backup/restore → parts).
    let key = if args.get("zips").is_some() {
        "zips"
    } else {
        "parts"
    };
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(display_item)
                .collect()
        })
        .unwrap_or_default()
}

fn display_item(value: &str) -> String {
    value
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(value)
        .to_string()
}
