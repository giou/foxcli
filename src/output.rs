use std::env;
use std::io::{self, IsTerminal, Write};

use crate::status::StatusSnapshot;

pub struct CommandResult<'a> {
    pub command: &'a str,
    pub exit_code: i32,
    pub stdout: &'a str,
    /// Surfaced only in JSON output. In plain mode any such notice is printed
    /// by the caller before the engine streams (see `sideload_notice`).
    pub json_note: Option<&'a str>,
    pub streamed: bool,
}

pub struct Renderer {
    json: bool,
    color: bool,
}

impl Renderer {
    pub fn new(json: bool, no_color: bool) -> Self {
        let term_ok = io::stdout().is_terminal()
            && env::var_os("NO_COLOR").is_none()
            && env::var("TERM").map(|term| term != "dumb").unwrap_or(true);
        Self {
            json,
            color: !json && !no_color && term_ok,
        }
    }

    pub fn json(&self) -> bool {
        self.json
    }

    pub fn error(&self, message: &str) {
        if self.json {
            println!(
                "{{\"ok\":false,\"exit_code\":1,\"error\":{}}}",
                json_string(message)
            );
        } else {
            eprintln!("fox: {message}");
        }
    }

    pub fn sideload_notice(&self, message: &str) {
        if !self.json {
            println!("{}", self.paint(Color::Green, message));
        }
    }

    pub fn command_response(&self, result: CommandResult<'_>) {
        if self.json {
            println!(
                "{{\"ok\":{},\"exit_code\":{},\"command\":{},\"message\":{},\"stdout\":{},\"lines\":{}}}",
                result.exit_code == 0,
                result.exit_code,
                json_string(result.command),
                json_option(result.json_note),
                json_string(result.stdout),
                json_lines(result.stdout),
            );
        } else if !result.streamed {
            self.render_plain_command(result);
        }
    }

    pub fn decrypt_help(&self, help: &str) {
        if self.json {
            println!(
                "{{\"ok\":true,\"exit_code\":0,\"command\":\"decrypt\",\"help\":{}}}",
                json_string(help)
            );
            return;
        }
        println!("{}", self.paint(Color::Yellow, "fox decrypt"));
        println!("{help}");
    }

    pub fn start_page(&self, status: &StatusSnapshot, exit_code: i32) {
        if self.json {
            self.start_page_json(status, exit_code);
            return;
        }

        println!("{}", self.paint(Color::Orange, "[OrangeFox Recovery]"));
        if exit_code != 0 {
            println!(
                "{} status command returned a non-zero exit code ({exit_code}); data may be incomplete.",
                self.paint(Color::Yellow, "!")
            );
        }
        println!(
            "{} {}  {} {}  {} {}",
            self.label("Release"),
            empty_dash(&status.release),
            self.label("Recovery"),
            empty_dash(&status.recovery_version),
            self.label("Variant"),
            empty_dash(&status.variant),
        );
        println!(
            "{} {}  {} {}  {} {}",
            self.label("Build"),
            empty_dash(&status.build_type),
            self.label("Branch"),
            empty_dash(&status.branch),
            self.label("Built"),
            empty_dash(&status.build_date),
        );
        println!(
            "{} {}",
            self.label("Codebase"),
            format!(
                "SDK {} {}",
                empty_dash(&status.codebase_sdk),
                empty_dash(&status.codebase)
            ),
        );

        println!();
        println!("{}", self.paint(Color::Cyan, "[Device]"));
        self.row("Model", &status.device_model);
        self.row("Code name", &status.device_code);
        self.row("Boot slot", &status.boot_slot);
        self.row_bool("Dynamic partitions", status.dynamic_partitions);
        self.row_bool("Virtual A/B", status.virtual_ab);
        self.row("Kernel", &status.kernel);

        println!();
        println!("{}", self.paint(Color::Cyan, "[Storage]"));
        if status.storage_encrypted {
            self.row("Encryption", "encrypted");
            self.row_bool("Decrypted", status.storage_decrypted);
            if !status.storage_decrypted {
                println!(
                    "  {} {}",
                    self.paint(Color::Yellow, "!"),
                    "Run `fox decrypt` for pattern help, or `fox decrypt <password>`."
                );
            }
        } else {
            self.row("Encryption", "not encrypted");
        }

        println!();
        println!("{}", self.paint(Color::Cyan, "[Installed ROM]"));
        self.row("Name", &status.rom_name);
        self.row("Incremental", &status.rom_incremental);
        self.row("Android", &status.android_release);
        self.row("SDK", &status.rom_sdk);
        self.row_bool("MIUI", status.rom_miui);
        self.row("Fingerprint", &status.fingerprint);

        println!();
        self.commands();
    }

    fn start_page_json(&self, status: &StatusSnapshot, exit_code: i32) {
        print!(
            "{{\"ok\":{},\"exit_code\":{},\"command\":\"status\",\"status\":{{",
            exit_code == 0,
            exit_code
        );
        for (idx, (key, value)) in status.raw.iter().enumerate() {
            if idx > 0 {
                print!(",");
            }
            if StatusSnapshot::is_bool_key(key) {
                print!("{}:{}", json_string(key), value == "1");
            } else {
                print!("{}:{}", json_string(key), json_string(value));
            }
        }
        print!("}},\"commands\":");
        json_commands();
        println!("}}");
    }

    fn render_plain_command(&self, result: CommandResult<'_>) {
        if result.command == "wlan" {
            for line in result.stdout.lines().filter(|line| !line.trim().is_empty()) {
                println!("  {} {}", self.paint(Color::Green, "*"), line);
            }
            return;
        }

        print!("{}", result.stdout);
        let _ = io::stdout().flush();
    }

    fn row(&self, label: &str, value: &str) {
        println!("  {} {}", self.label(label), empty_dash(value));
    }

    fn row_bool(&self, label: &str, value: bool) {
        let text = if value { "yes" } else { "no" };
        let color = if value { Color::Green } else { Color::Dim };
        println!("  {} {}", self.label(label), self.paint(color, text));
    }

    fn label(&self, label: &str) -> String {
        self.paint(Color::Dim, &format!("{label}:"))
    }

    fn commands(&self) {
        println!("{}", self.paint(Color::Cyan, "[Commands]"));
        for info in COMMANDS {
            println!("  {} {}", self.paint(Color::Green, info.usage), info.summary);
        }
    }

    fn paint(&self, color: Color, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        let code = match color {
            Color::Green => "32",
            Color::Yellow => "33",
            Color::Cyan => "36",
            Color::Orange => "38;5;208",
            Color::Dim => "2",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

enum Color {
    Green,
    Yellow,
    Cyan,
    Orange,
    Dim,
}

/// A single row of the `fox` help listing.
///
/// `name` is the canonical command (matching `args::Command::display_name`, or
/// a non-command pseudo-entry like `fox`/`--json`); the drift test in args.rs
/// asserts every command variant appears here.
pub struct CommandInfo {
    pub name: &'static str,
    pub usage: &'static str,
    pub summary: &'static str,
}

const COMMANDS: &[CommandInfo] = &[
    CommandInfo { name: "fox", usage: "fox", summary: "show device status and this command list" },
    CommandInfo { name: "status", usage: "fox status", summary: "show device status" },
    CommandInfo { name: "mount", usage: "fox mount <path...>", summary: "mount one or more partitions" },
    CommandInfo { name: "unmount", usage: "fox unmount <path...>", summary: "unmount one or more partitions" },
    CommandInfo { name: "flash", usage: "fox flash <zip> [--no-verify]", summary: "flash a zip" },
    CommandInfo { name: "backup", usage: "fox backup <part...>", summary: "create a backup" },
    CommandInfo { name: "restore", usage: "fox restore --name <backup> [part...]", summary: "restore a backup" },
    CommandInfo { name: "decrypt", usage: "fox decrypt [password] [--user <id>]", summary: "decrypt storage or show pattern help" },
    CommandInfo { name: "sideload", usage: "fox sideload", summary: "start adb sideload mode" },
    CommandInfo { name: "wlan", usage: "fox wlan <subcommand>", summary: "manage WLAN" },
    CommandInfo { name: "reboot", usage: "fox reboot [target]", summary: "reboot to a target" },
    CommandInfo { name: "internal", usage: "fox internal <get|set|go>", summary: "read or update recovery internals" },
    CommandInfo { name: "ors", usage: "fox ors <file>", summary: "run an OpenRecoveryScript file" },
    CommandInfo { name: "ors-cmd", usage: "fox ors-cmd <command...>", summary: "run one raw OpenRecoveryScript command" },
    CommandInfo { name: "--json", usage: "fox --json <command>", summary: "return machine-readable JSON" },
];

/// Canonical command names in the help catalog (used by the args.rs drift test).
pub fn command_names() -> impl Iterator<Item = &'static str> {
    COMMANDS.iter().map(|info| info.name)
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

fn json_option(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn json_lines(value: &str) -> String {
    let mut out = String::from("[");
    for (idx, line) in value.lines().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&json_string(line));
    }
    out.push(']');
    out
}

fn json_commands() {
    print!("[");
    for (idx, info) in COMMANDS.iter().enumerate() {
        if idx > 0 {
            print!(",");
        }
        print!(
            "{{\"name\":{},\"command\":{},\"summary\":{}}}",
            json_string(info.name),
            json_string(info.usage),
            json_string(info.summary)
        );
    }
    print!("]");
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if ch < '\u{20}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
