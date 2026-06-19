mod args;
mod output;
mod patterns;
mod status;
mod transport;

use clap::Parser;
use std::process;

use args::{Cli, Command};
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
    match cli.command {
        None => show_start_page(renderer),
        Some(Command::Status) => show_start_page(renderer),
        Some(Command::Decrypt(decrypt)) if decrypt.password.is_none() => {
            renderer.decrypt_help(patterns::pattern_help());
            Ok(0)
        }
        Some(Command::Sideload) => {
            let message = "Successfully started sideload. You can use: adb sideload <path.zip>";
            renderer.sideload_notice(message);
            run_engine_command(
                "sideload",
                vec!["sideload".to_string()],
                renderer,
                Some(message),
            )
        }
        Some(command) => {
            let name = command.display_name();
            run_engine_command(name, command.into_engine_argv(), renderer, None)
        }
    }
}

fn show_start_page(renderer: &Renderer) -> std::io::Result<i32> {
    let response = send_command(&["status".to_string()], true)?;
    let snapshot = status::StatusSnapshot::parse(&response.stdout);
    renderer.start_page(&snapshot, response.exit_code);
    Ok(response.exit_code)
}

fn run_engine_command(
    name: &'static str,
    argv: Vec<String>,
    renderer: &Renderer,
    message: Option<&str>,
) -> std::io::Result<i32> {
    let quiet = renderer.json() || should_render_plain(&argv);
    let response = send_command(&argv, quiet)?;
    renderer.command_response(CommandResult {
        command: name,
        exit_code: response.exit_code,
        stdout: &response.stdout,
        message,
        streamed: !quiet,
    });
    Ok(response.exit_code)
}

fn should_render_plain(argv: &[String]) -> bool {
    matches!(
        (
            argv.get(0).map(String::as_str),
            argv.get(1).map(String::as_str)
        ),
        (Some("wlan"), Some("list" | "saved" | "info"))
    )
}
