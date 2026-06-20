mod args;
mod cancel;
mod output;
mod patterns;
mod status;
mod terminal_ui;
mod transport;

use clap::Parser;
use std::process;

use args::{Cli, Command, CommandPage};
use output::{CommandResult, Renderer};
use terminal_ui::{ProgressUi, Spinner};
use transport::{send_command, send_command_observed, send_command_raw, ProgressUpdate, ResponseEvent};

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
        Some(Command::Sideload(_)) => {
            // Printed before the engine starts streaming; also echoed into the
            // JSON object via `json_note` for non-streaming consumers.
            let note = "Successfully started sideload. You can use: adb sideload <path.zip>";
            renderer.sideload_notice(note);
            run_engine_command(
                "sideload",
                vec!["sideload".to_string()],
                renderer,
                Some(note),
                false,
            )
        }
        Some(command) => {
            let name = command.display_name();
            let render_plain = command.renders_plain();
            run_engine_command(
                name,
                command.into_engine_argv(),
                renderer,
                None,
                render_plain,
            )
        }
    }
}

fn show_start_page(renderer: &Renderer) -> std::io::Result<i32> {
    let response = {
        let _spinner = Spinner::start(renderer.interactive(), "Reading recovery status...");
        send_command(&["status".to_string()], true)?
    };
    let snapshot = status::StatusSnapshot::parse(&response.stdout);
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
    renderer.command_page(
        page,
        response.as_ref().map(|r| r.exit_code).unwrap_or(0),
        response.as_ref().map(|r| r.stdout.as_str()).unwrap_or(""),
    );
    Ok(response.map(|r| r.exit_code).unwrap_or(0))
}

fn run_engine_command(
    name: &'static str,
    argv: Vec<String>,
    renderer: &Renderer,
    json_note: Option<&str>,
    render_plain: bool,
) -> std::io::Result<i32> {
    let progress = progress_title(name);
    let quiet = renderer.json() || render_plain || (renderer.interactive() && progress.is_some());
    let response = if name == "fifo" {
        send_command_raw(&argv, quiet)?
    } else if let Some(title) = progress {
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
            progress_items(&argv),
            can_cancel,
        );
        let mut observer = |event: ResponseEvent| match event {
            ResponseEvent::Output(bytes) => progress.observe(&bytes),
            ResponseEvent::Progress(ProgressUpdate::Overall(percent)) => {
                progress.observe_progress(percent)
            }
            ResponseEvent::Progress(ProgressUpdate::Item(percent)) => progress.observe_item(percent),
        };
        send_command_observed(&argv, quiet, Some(&mut observer))?
    } else {
        send_command(&argv, quiet)?
    };
    renderer.command_response(CommandResult {
        command: name,
        exit_code: response.exit_code,
        stdout: &response.stdout,
        progress_percent: response.progress_percent,
        json_note,
        streamed: !quiet || (renderer.interactive() && progress.is_some()),
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

fn progress_items(argv: &[String]) -> Vec<String> {
    let mut items = Vec::new();
    let mut skip_next = false;
    for arg in argv.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if matches!(arg.as_str(), "--name" | "--path" | "--storage") {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        items.push(display_item(arg));
    }
    items
}

fn display_item(value: &str) -> String {
    value
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(value)
        .to_string()
}
