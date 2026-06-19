mod args;
mod output;
mod patterns;
mod status;
mod transport;

use clap::Parser;
use std::process;

use args::{Cli, Command, CommandPage};
use output::{CommandResult, Renderer};
use transport::send_command;

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
    let response = send_command(&["status".to_string()], true)?;
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
        Some(argv) => Some(send_command(&argv, true)?),
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
    let quiet = renderer.json() || render_plain;
    let response = send_command(&argv, quiet)?;
    renderer.command_response(CommandResult {
        command: name,
        exit_code: response.exit_code,
        stdout: &response.stdout,
        json_note,
        streamed: !quiet,
    });
    Ok(response.exit_code)
}
