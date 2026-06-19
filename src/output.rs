use std::env;
use std::io::{self, IsTerminal, Write};

use serde::Serialize;

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
            print_json(&ErrorJson {
                ok: false,
                exit_code: 1,
                error: message,
            });
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
            // List commands emit a FOX_LIST block; surface it as a structured
            // array rather than only as raw stdout text.
            let partitions = parse_partition_rows(result.stdout);
            print_json(&CommandJson {
                ok: result.exit_code == 0,
                exit_code: result.exit_code,
                command: result.command,
                message: result.json_note,
                stdout: result.stdout,
                lines: result.stdout.lines().collect(),
                partitions,
            });
        } else if !result.streamed {
            self.render_plain_command(result);
        }
    }

    pub fn decrypt_help(&self, help: &str) {
        if self.json {
            print_json(&DecryptHelpJson {
                ok: true,
                exit_code: 0,
                command: "decrypt",
                help,
            });
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
        print_json(&StatusJson {
            ok: exit_code == 0,
            exit_code,
            command: "status",
            status,
            commands: COMMANDS,
        });
    }

    fn render_plain_command(&self, result: CommandResult<'_>) {
        if let Some(rows) = parse_partition_rows(result.stdout) {
            if result.command == "addons" {
                self.render_addons(&rows);
            } else {
                self.render_partition_table(&rows);
            }
            return;
        }

        if result.command == "wlan" {
            for line in result.stdout.lines().filter(|line| !line.trim().is_empty()) {
                println!("  {} {}", self.paint(Color::Green, "*"), line);
            }
            return;
        }

        print!("{}", result.stdout);
        let _ = io::stdout().flush();
    }

    fn render_partition_table(&self, rows: &[PartitionRow]) {
        if rows.is_empty() {
            println!("  {}", self.paint(Color::Dim, "(none)"));
            return;
        }
        for row in rows {
            let mark = if row.mounted {
                self.paint(Color::Green, "[x]")
            } else {
                self.paint(Color::Dim, "[ ]")
            };
            let mut line = format!("  {} {}", mark, row.display_name);
            if !row.mount_point.is_empty() && row.mount_point != row.display_name {
                line.push_str(&format!("  {}", self.paint(Color::Dim, &row.mount_point)));
            }
            if !row.extra.is_empty() {
                line.push_str(&format!("  {}", self.paint(Color::Dim, &row.extra)));
            }
            println!("{line}");
        }
    }

    fn render_addons(&self, rows: &[PartitionRow]) {
        if rows.is_empty() {
            println!("  {}", self.paint(Color::Dim, "(no addons found)"));
            return;
        }
        for row in rows {
            // display_name = file name, mount_point = full path.
            println!(
                "  {} {}  {}",
                self.paint(Color::Green, "*"),
                row.display_name,
                self.paint(Color::Dim, &row.mount_point)
            );
        }
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
#[derive(Serialize)]
pub struct CommandInfo {
    pub name: &'static str,
    #[serde(rename = "command")]
    pub usage: &'static str,
    pub summary: &'static str,
}

const COMMANDS: &[CommandInfo] = &[
    CommandInfo { name: "fox", usage: "fox", summary: "show device status and this command list" },
    CommandInfo { name: "status", usage: "fox status", summary: "show device status" },
    CommandInfo { name: "mount", usage: "fox mount [partition...]", summary: "mount partitions, or list mount state" },
    CommandInfo { name: "unmount", usage: "fox unmount [partition...]", summary: "unmount partitions, or list mount state" },
    CommandInfo { name: "flash", usage: "fox flash <zip...> [--no-verify] [--reflash]", summary: "flash one or more zips" },
    CommandInfo { name: "backup", usage: "fox backup [part...] [--name <n>] [--compression]", summary: "back up partitions, or list backupable" },
    CommandInfo { name: "restore", usage: "fox restore --name <backup> [part...]", summary: "restore a backup" },
    CommandInfo { name: "wipe", usage: "fox wipe [partition...]", summary: "wipe partitions, or list wipeable" },
    CommandInfo { name: "format_data", usage: "fox format_data --yes", summary: "full data format (removes encryption)" },
    CommandInfo { name: "partition", usage: "fox partition [<path> <op>]", summary: "list partitions or repair/resize/change-fs/wipe" },
    CommandInfo { name: "decrypt", usage: "fox decrypt [password] [--user <id>]", summary: "decrypt storage or show pattern help" },
    CommandInfo { name: "sideload", usage: "fox sideload", summary: "start adb sideload mode" },
    CommandInfo { name: "mtp", usage: "fox mtp <status|enable|disable>", summary: "control MTP" },
    CommandInfo { name: "wlan", usage: "fox wlan <subcommand>", summary: "manage WLAN" },
    CommandInfo { name: "log", usage: "fox log", summary: "save a recovery log zip" },
    CommandInfo { name: "reflash", usage: "fox reflash", summary: "reflash the current OrangeFox" },
    CommandInfo { name: "addons", usage: "fox addons [install <name...>]", summary: "list or install Fox addons" },
    CommandInfo { name: "reboot", usage: "fox reboot [target]", summary: "reboot to a target" },
    CommandInfo { name: "internal", usage: "fox internal <get|set|go>", summary: "read or update recovery internals" },
    CommandInfo { name: "ors", usage: "fox ors <file>", summary: "run an OpenRecoveryScript file" },
    CommandInfo { name: "ors-cmd", usage: "fox ors-cmd <command...>", summary: "run one raw OpenRecoveryScript command" },
    CommandInfo { name: "--json", usage: "fox --json <command>", summary: "return machine-readable JSON" },
];

/// Canonical command names in the help catalog (used by the args.rs drift test).
#[cfg(test)]
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

/// Serialize `payload` to a single line of JSON on stdout. Serialization of
/// these plain data structures cannot fail, so a failure is a programmer error.
fn print_json<T: Serialize>(payload: &T) {
    match serde_json::to_string(payload) {
        Ok(json) => println!("{json}"),
        Err(err) => eprintln!("fox: failed to encode JSON: {err}"),
    }
}

#[derive(Serialize)]
struct ErrorJson<'a> {
    ok: bool,
    exit_code: i32,
    error: &'a str,
}

#[derive(Serialize)]
struct CommandJson<'a> {
    ok: bool,
    exit_code: i32,
    command: &'a str,
    message: Option<&'a str>,
    stdout: &'a str,
    lines: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partitions: Option<Vec<PartitionRow>>,
}

/// One record from a FOX_LIST block emitted by listing commands (mount, backup,
/// wipe, partition, addons). `extra` carries fs/capability flags when present.
#[derive(Serialize)]
pub struct PartitionRow {
    pub mount_point: String,
    pub display_name: String,
    pub mounted: bool,
    pub extra: String,
}

/// Parse a `FOX_LIST_BEGIN`/`FOX_LIST_END` block into rows. Returns `None` when
/// no block is present (so callers fall back to raw output).
fn parse_partition_rows(stdout: &str) -> Option<Vec<PartitionRow>> {
    let mut in_block = false;
    let mut rows = Vec::new();
    for line in stdout.lines() {
        match line {
            "FOX_LIST_BEGIN" => {
                in_block = true;
                continue;
            }
            "FOX_LIST_END" => return Some(rows),
            _ => {}
        }
        if !in_block {
            continue;
        }
        let mut fields = line.splitn(4, '\t');
        rows.push(PartitionRow {
            mount_point: fields.next().unwrap_or("").to_string(),
            display_name: fields.next().unwrap_or("").to_string(),
            mounted: fields.next().unwrap_or("0") == "1",
            extra: fields.next().unwrap_or("").to_string(),
        });
    }
    if in_block {
        Some(rows)
    } else {
        None
    }
}

#[derive(Serialize)]
struct DecryptHelpJson<'a> {
    ok: bool,
    exit_code: i32,
    command: &'a str,
    help: &'a str,
}

#[derive(Serialize)]
struct StatusJson<'a> {
    ok: bool,
    exit_code: i32,
    command: &'a str,
    status: &'a StatusSnapshot,
    commands: &'a [CommandInfo],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fox_list_block() {
        let s = "FOX_LIST_BEGIN\n/system\tSystem\t1\tfs=ext4,repair\n\
                 /data\tData (2 GB)\t0\t\nFOX_LIST_END\n";
        let rows = parse_partition_rows(s).expect("a list block");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].mount_point, "/system");
        assert!(rows[0].mounted);
        assert_eq!(rows[0].extra, "fs=ext4,repair");
        assert_eq!(rows[1].display_name, "Data (2 GB)");
        assert!(!rows[1].mounted);
    }

    #[test]
    fn no_block_returns_none() {
        assert!(parse_partition_rows("just streamed text\n").is_none());
    }
}
