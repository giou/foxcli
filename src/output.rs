use std::env;
use std::io::{self, IsTerminal, Write};

use serde::Serialize;

use crate::args::CommandPage;
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

    pub fn wlan_page(&self, exit_code: i32, stdout: &str) {
        let state = WlanState::from_response(exit_code, stdout);
        if self.json {
            print_json(&FeatureJson {
                ok: exit_code == 0 || state.available,
                exit_code,
                feature: "wlan",
                available: state.available,
                enabled: state.enabled,
                status: state.status,
                stdout,
                commands: WLAN_COMMANDS,
            });
            return;
        }

        println!("{}", self.paint(Color::Orange, "[WLAN]"));
        self.row("Status", state.status);
        if !state.available {
            println!(
                "  {} WLAN support is not available in this recovery build.",
                self.paint(Color::Yellow, "!")
            );
        } else if !state.enabled {
            println!(
                "  {} {}",
                self.paint(Color::Yellow, "!"),
                "Run `fox wlan enable` before scanning or connecting."
            );
        } else if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
                println!("  {} {}", self.paint(Color::Green, "*"), line);
            }
        }

        println!();
        self.command_page_sections(WLAN_COMMANDS);
    }

    pub fn mtp_page(&self, exit_code: i32, stdout: &str) {
        let enabled = stdout
            .lines()
            .find_map(|line| line.strip_prefix("mtp_enabled="))
            .map(|value| value.trim() == "1");
        let status = match enabled {
            Some(true) => "enabled",
            Some(false) => "disabled",
            None if exit_code == 0 => "unknown",
            None => "unavailable",
        };

        if self.json {
            print_json(&FeatureJson {
                ok: exit_code == 0,
                exit_code,
                feature: "mtp",
                available: exit_code == 0,
                enabled: enabled.unwrap_or(false),
                status,
                stdout,
                commands: MTP_COMMANDS,
            });
            return;
        }

        println!("{}", self.paint(Color::Orange, "[MTP]"));
        self.row("Status", status);
        if exit_code != 0 && !stdout.trim().is_empty() {
            println!("  {}", stdout.trim());
        }
        println!();
        self.command_page_sections(MTP_COMMANDS);
    }

    pub fn command_page(&self, page: CommandPage, exit_code: i32, stdout: &str) {
        if matches!(page, CommandPage::Wlan) {
            self.wlan_page(exit_code, stdout);
            return;
        }
        if matches!(page, CommandPage::Mtp) {
            self.mtp_page(exit_code, stdout);
            return;
        }

        let commands = page_commands(page);
        if self.json {
            print_json(&CommandPageJson {
                ok: exit_code == 0,
                exit_code,
                command: page_name(page),
                title: page_title(page),
                summary: page_summary(page),
                stdout,
                lines: stdout.lines().collect(),
                partitions: parse_partition_rows(stdout),
                commands,
            });
            return;
        }

        println!("{}", self.paint(Color::Orange, page_title(page)));
        println!("  {}", page_summary(page));
        if exit_code != 0 && !stdout.trim().is_empty() {
            println!();
            println!("  {}", self.paint(Color::Yellow, stdout.trim()));
        } else if let Some(rows) = parse_partition_rows(stdout) {
            println!();
            if matches!(page, CommandPage::Addons) {
                self.render_addons(&rows);
            } else if matches!(page, CommandPage::Mount) {
                self.render_partition_table(&rows, TableMode::MountState);
            } else {
                self.render_partition_table(&rows, TableMode::Plain);
            }
        } else if !stdout.trim().is_empty() {
            println!();
            for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
                println!("  {}", line);
            }
        }
        println!();
        self.command_page_sections(commands);
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
            } else if result.command == "mount" {
                self.render_partition_table(&rows, TableMode::MountState);
            } else {
                self.render_partition_table(&rows, TableMode::Plain);
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

    fn render_partition_table(&self, rows: &[PartitionRow], mode: TableMode) {
        if rows.is_empty() {
            println!("  {}", self.paint(Color::Dim, "(none)"));
            return;
        }
        let name_width = rows
            .iter()
            .map(|row| row.display_name.len())
            .max()
            .unwrap_or(0);
        let path_width = rows
            .iter()
            .filter(|row| !row.mount_point.is_empty() && row.mount_point != row.display_name)
            .map(|row| row.mount_point.len())
            .max()
            .unwrap_or(0);

        for row in rows {
            let name = self.paint(Color::Cyan, &format!("{:name_width$}", row.display_name));
            let mut line = match mode {
                TableMode::MountState => {
                    let mark = if row.mounted {
                        self.paint(Color::Green, "[x]")
                    } else {
                        self.paint(Color::Dim, "[ ]")
                    };
                    format!("  {mark}  {name}")
                }
                TableMode::Plain => format!("  {name}"),
            };
            if !row.mount_point.is_empty() && row.mount_point != row.display_name {
                line.push_str("  ");
                line.push_str(&self.paint(Color::Dim, &format!("{:path_width$}", row.mount_point)));
            } else if path_width > 0 {
                line.push_str("  ");
                line.push_str(&" ".repeat(path_width));
            }
            if !row.extra.is_empty() {
                line.push_str(&format!("  {}", self.partition_extra(&row.extra)));
            }
            println!("{line}");
        }
    }

    fn partition_extra(&self, extra: &str) -> String {
        extra
            .split(',')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let color = if part.starts_with("fs=") {
                    Color::Orange
                } else if part == "repair" || part == "resize" {
                    Color::Green
                } else if part == "wipe" || part == "format" || part.starts_with("change-fs") {
                    Color::Yellow
                } else {
                    Color::Gray
                };
                self.paint(color, part)
            })
            .collect::<Vec<_>>()
            .join(self.paint(Color::Dim, ",").as_str())
    }

    fn render_addons(&self, rows: &[PartitionRow]) {
        if rows.is_empty() {
            println!("  {}", self.paint(Color::Dim, "(no addons found)"));
            return;
        }
        let name_width = rows
            .iter()
            .map(|row| row.display_name.len())
            .max()
            .unwrap_or(0);
        for row in rows {
            // display_name = file name, mount_point = full path.
            println!(
                "  {} {:name_width$}  {}",
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
        println!(
            "  {}",
            self.paint(
                Color::Gray,
                "Fox <command> shows command status/help and its available options, for example: fox wipe shows wipeable partitions and command help."
            )
        );
        for group in COMMAND_GROUPS {
            println!();
            println!("  {}", self.paint(Color::Yellow, group));
            for info in COMMANDS.iter().filter(|info| info.group == *group) {
                self.command_info(info, 4);
            }
        }
    }

    fn feature_commands(&self, title: &str, commands: &[CommandInfo]) {
        println!("{}", self.paint(Color::Cyan, &format!("[{title}]")));
        for info in commands {
            self.command_info(info, 2);
        }
    }

    fn command_page_sections(&self, commands: &[CommandInfo]) {
        let has_commands = commands.iter().any(|info| !info.is_flag());
        let has_flags = commands.iter().any(CommandInfo::is_flag);

        if has_commands {
            println!("{}", self.paint(Color::Cyan, "[Commands]"));
            for info in commands.iter().filter(|info| !info.is_flag()) {
                self.command_info(info, 2);
            }
        }

        if has_flags {
            if has_commands {
                println!();
            }
            println!("{}", self.paint(Color::Cyan, "[Flags]"));
            for info in commands.iter().filter(|info| info.is_flag()) {
                self.flag_info(info, 2);
            }
        }
    }

    fn command_info(&self, info: &CommandInfo, indent: usize) {
        let pad = " ".repeat(indent);
        println!(
            "{}{} {}",
            pad,
            self.paint(Color::Green, info.usage),
            info.summary
        );
        for example in info.examples {
            println!("{}  {}", pad, self.paint(Color::Gray, example));
        }
    }

    fn flag_info(&self, info: &CommandInfo, indent: usize) {
        let pad = " ".repeat(indent);
        println!(
            "{}{} {}",
            pad,
            self.paint(Color::Green, &info.flag_usage()),
            info.summary
        );
        for example in info.examples {
            println!("{}  {}", pad, self.paint(Color::Gray, example));
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
            Color::Gray => "38;5;244",
            Color::Dim => "2",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

enum TableMode {
    Plain,
    MountState,
}

enum Color {
    Green,
    Yellow,
    Cyan,
    Orange,
    Gray,
    Dim,
}

/// A single row of the `fox` help listing.
///
/// `name` is the canonical command (matching `args::Command::display_name`, or
/// a non-command pseudo-entry like `fox`/`--json`); the drift test in args.rs
/// asserts every command variant appears here.
#[derive(Serialize)]
pub struct CommandInfo {
    pub group: &'static str,
    pub name: &'static str,
    #[serde(rename = "command")]
    pub usage: &'static str,
    pub summary: &'static str,
    pub examples: &'static [&'static str],
}

impl CommandInfo {
    fn is_flag(&self) -> bool {
        matches!(
            self.name,
            "verify"
                | "no-verify"
                | "reflash"
                | "name"
                | "storage"
                | "compression"
                | "path"
                | "digest"
                | "confirm"
        )
    }

    fn flag_usage(&self) -> String {
        self.usage
            .split_whitespace()
            .filter(|part| part.starts_with("--"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

const COMMAND_GROUPS: &[&str] = &[
    "Basic",
    "Partition management",
    "Backups",
    "Connectivity",
    "Other",
    "Misc",
];

const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Basic",
        name: "fox",
        usage: "fox",
        summary: "show device status and this command list",
        examples: &["fox"],
    },
    CommandInfo {
        group: "Basic",
        name: "status",
        usage: "fox status",
        summary: "show device status",
        examples: &["fox status"],
    },
    CommandInfo {
        group: "Basic",
        name: "decrypt",
        usage: "fox decrypt [password] [--user <id>]",
        summary: "decrypt storage or show pattern help",
        examples: &["fox decrypt", "fox decrypt 14789 --user 0"],
    },
    CommandInfo {
        group: "Basic",
        name: "sideload",
        usage: "fox sideload [start]",
        summary: "show help or start adb sideload mode",
        examples: &["fox sideload", "fox sideload start", "adb sideload rom.zip"],
    },
    CommandInfo {
        group: "Basic",
        name: "reboot",
        usage: "fox reboot [target]",
        summary: "reboot to a target",
        examples: &["fox reboot", "fox reboot bootloader"],
    },
    CommandInfo {
        group: "Partition management",
        name: "mount",
        usage: "fox mount [partition...]",
        summary: "mount partitions, or list mount state",
        examples: &[
            "fox mount: see available partitions",
            "fox mount /vendor: mount the vendor",
        ],
    },
    CommandInfo {
        group: "Partition management",
        name: "unmount",
        usage: "fox unmount [partition...]",
        summary: "unmount partitions, or list mount state",
        examples: &[
            "fox unmount: see available partitions",
            "fox unmount /system: unmount system",
        ],
    },
    CommandInfo {
        group: "Partition management",
        name: "wipe",
        usage: "fox wipe [partition...]",
        summary: "wipe partitions, or list wipeable",
        examples: &["fox wipe", "fox wipe cache dalvik"],
    },
    CommandInfo {
        group: "Partition management",
        name: "format_data",
        usage: "fox format_data --yes",
        summary: "full data format (removes encryption)",
        examples: &["fox format_data --yes"],
    },
    CommandInfo {
        group: "Partition management",
        name: "partition",
        usage: "fox partition [<path> <op>]",
        summary: "list partitions or repair/resize/change-fs/wipe",
        examples: &[
            "fox partition",
            "fox partition /data repair",
            "fox partition /data change-fs f2fs",
        ],
    },
    CommandInfo {
        group: "Backups",
        name: "backup",
        usage: "fox backup [part...] [--storage <name|path>]",
        summary: "back up partitions, or list backupable",
        examples: &[
            "fox backup",
            "fox backup boot data --storage microsd",
            "fox backup data --storage otg --compression",
        ],
    },
    CommandInfo {
        group: "Backups",
        name: "restore",
        usage: "fox restore --name <backup> [part...]",
        summary: "restore a backup",
        examples: &[
            "fox restore --name before-flash",
            "fox restore data --name before-flash",
        ],
    },
    CommandInfo {
        group: "Connectivity",
        name: "mtp",
        usage: "fox mtp [status|enable|disable]",
        summary: "show or control MTP",
        examples: &["fox mtp", "fox mtp enable", "fox mtp disable"],
    },
    CommandInfo {
        group: "Connectivity",
        name: "wlan",
        usage: "fox wlan [subcommand]",
        summary: "show or manage WLAN",
        examples: &[
            "fox wlan",
            "fox wlan list",
            "fox wlan connect MyWifi --password secret",
        ],
    },
    CommandInfo {
        group: "Basic",
        name: "flash",
        usage: "fox flash <zip...> [--no-verify] [--reflash-orangefox]",
        summary: "flash one or more zips",
        examples: &[
            "fox flash rom.zip",
            "fox flash magisk.zip --no-verify",
            "fox flash rom.zip --reflash-orangefox",
        ],
    },
    CommandInfo {
        group: "Other",
        name: "reflash",
        usage: "fox reflash [run]",
        summary: "show help or reflash the current OrangeFox",
        examples: &["fox reflash", "fox reflash run"],
    },
    CommandInfo {
        group: "Other",
        name: "addons",
        usage: "fox addons [install <name...>]",
        summary: "list or install Fox addons",
        examples: &["fox addons", "fox addons install addon.zip"],
    },
    CommandInfo {
        group: "Other",
        name: "log",
        usage: "fox log [save]",
        summary: "show help or save a recovery log zip",
        examples: &["fox log", "fox log save"],
    },
    CommandInfo {
        group: "Other",
        name: "storages",
        usage: "fox storages",
        summary: "show available backup/storage destinations",
        examples: &["fox storages", "fox backup data --storage microsd"],
    },
    CommandInfo {
        group: "Misc",
        name: "fifo",
        usage: "fox fifo send <argv...>",
        summary: "send a raw argv vector to foxin and print foxout",
        examples: &[],
    },
    CommandInfo {
        group: "Misc",
        name: "internal",
        usage: "fox internal <get|set|go>",
        summary: "read or update recovery internals",
        examples: &[],
    },
    CommandInfo {
        group: "Misc",
        name: "ors",
        usage: "fox ors <file>",
        summary: "run an OpenRecoveryScript file",
        examples: &[],
    },
    CommandInfo {
        group: "Misc",
        name: "ors-cmd",
        usage: "fox ors-cmd <command...>",
        summary: "run one raw OpenRecoveryScript command",
        examples: &[],
    },
    CommandInfo {
        group: "Misc",
        name: "--json",
        usage: "fox --json <command>",
        summary: "return machine-readable JSON",
        examples: &[],
    },
];

const WLAN_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "WLAN",
        name: "wlan",
        usage: "fox wlan",
        summary: "show WLAN status and commands",
        examples: &["fox wlan"],
    },
    CommandInfo {
        group: "WLAN",
        name: "enable",
        usage: "fox wlan enable",
        summary: "enable WLAN",
        examples: &["fox wlan enable"],
    },
    CommandInfo {
        group: "WLAN",
        name: "disable",
        usage: "fox wlan disable",
        summary: "disable WLAN",
        examples: &["fox wlan disable"],
    },
    CommandInfo {
        group: "WLAN",
        name: "list",
        usage: "fox wlan list",
        summary: "scan and list networks",
        examples: &["fox wlan list"],
    },
    CommandInfo {
        group: "WLAN",
        name: "connect",
        usage: "fox wlan connect <ssid> [--password <pass>]",
        summary: "connect to a scanned network",
        examples: &["fox wlan connect MyWifi --password secret"],
    },
    CommandInfo {
        group: "WLAN",
        name: "saved",
        usage: "fox wlan saved",
        summary: "list saved networks",
        examples: &["fox wlan saved"],
    },
    CommandInfo {
        group: "WLAN",
        name: "connect-saved",
        usage: "fox wlan connect-saved <ssid>",
        summary: "connect to a saved network",
        examples: &["fox wlan connect-saved MyWifi"],
    },
    CommandInfo {
        group: "WLAN",
        name: "forget",
        usage: "fox wlan forget <ssid>",
        summary: "forget a saved network",
        examples: &["fox wlan forget MyWifi"],
    },
    CommandInfo {
        group: "WLAN",
        name: "info",
        usage: "fox wlan info",
        summary: "show connection info",
        examples: &["fox wlan info"],
    },
    CommandInfo {
        group: "WLAN",
        name: "test",
        usage: "fox wlan test",
        summary: "test connectivity",
        examples: &["fox wlan test"],
    },
];

const MTP_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "MTP",
        name: "mtp",
        usage: "fox mtp",
        summary: "show MTP status and commands",
        examples: &["fox mtp"],
    },
    CommandInfo {
        group: "MTP",
        name: "status",
        usage: "fox mtp status",
        summary: "show whether MTP is enabled",
        examples: &["fox mtp status"],
    },
    CommandInfo {
        group: "MTP",
        name: "enable",
        usage: "fox mtp enable",
        summary: "enable MTP",
        examples: &["fox mtp enable"],
    },
    CommandInfo {
        group: "MTP",
        name: "disable",
        usage: "fox mtp disable",
        summary: "disable MTP",
        examples: &["fox mtp disable"],
    },
];

const MOUNT_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Mount",
        name: "mount",
        usage: "fox mount [partition...]",
        summary: "mount partitions, or list mount state",
        examples: &[
            "fox mount",
            "fox mount /vendor",
            "fox mount /system /product",
        ],
    },
    CommandInfo {
        group: "Mount",
        name: "unmount",
        usage: "fox unmount [partition...]",
        summary: "unmount partitions, or list mount state",
        examples: &["fox unmount", "fox unmount /vendor"],
    },
];

const FLASH_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Flash",
        name: "flash",
        usage: "fox flash <zip...>",
        summary: "flash one or more zip files",
        examples: &["fox flash rom.zip", "fox flash magisk.zip kernel.zip"],
    },
    CommandInfo {
        group: "Flash",
        name: "verify",
        usage: "fox flash <zip> --verify",
        summary: "force zip signature verification",
        examples: &["fox flash rom.zip --verify"],
    },
    CommandInfo {
        group: "Flash",
        name: "no-verify",
        usage: "fox flash <zip> --no-verify",
        summary: "skip zip signature verification",
        examples: &["fox flash test.zip --no-verify"],
    },
    CommandInfo {
        group: "Flash",
        name: "reflash",
        usage: "fox flash <zip> --reflash-orangefox",
        summary: "reflash OrangeFox after flashing",
        examples: &["fox flash rom.zip --reflash-orangefox"],
    },
];

const BACKUP_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Backup",
        name: "backup",
        usage: "fox backup [part...]",
        summary: "back up partitions, or list backupable partitions",
        examples: &[
            "fox backup",
            "fox backup boot data --storage microsd",
            "fox backup data --storage otg --compression",
        ],
    },
    CommandInfo {
        group: "Backup",
        name: "name",
        usage: "fox backup <part...> --name <name>",
        summary: "choose the backup folder name",
        examples: &["fox backup boot data --name before-flash"],
    },
    CommandInfo {
        group: "Backup",
        name: "storage",
        usage: "fox backup <part...> --storage <name|path>",
        summary: "choose destination storage; aliases include internal, microsd, otg, and usb",
        examples: &[
            "fox backup data --storage microsd",
            "fox backup data --storage /usb_otg",
        ],
    },
    CommandInfo {
        group: "Backup",
        name: "compression",
        usage: "fox backup <part...> --compression",
        summary: "enable backup compression",
        examples: &["fox backup data --compression"],
    },
];

const RESTORE_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Restore",
        name: "restore",
        usage: "fox restore --name <backup> [part...]",
        summary: "restore all or selected partitions from a backup",
        examples: &[
            "fox restore --name before-flash",
            "fox restore data --name before-flash",
        ],
    },
    CommandInfo {
        group: "Restore",
        name: "path",
        usage: "fox restore --path <folder> [part...]",
        summary: "restore from an absolute backup folder",
        examples: &["fox restore --path /sdcard/Fox/BACKUPS/device/before-flash"],
    },
    CommandInfo {
        group: "Restore",
        name: "digest",
        usage: "fox restore --name <backup> --disable-digest-check",
        summary: "skip digest verification",
        examples: &["fox restore --name before-flash --disable-digest-check"],
    },
];

const WIPE_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "Wipe",
    name: "wipe",
    usage: "fox wipe [partition...]",
    summary: "wipe partitions, or list wipeable targets",
    examples: &["fox wipe", "fox wipe cache dalvik", "fox wipe /data"],
}];

const PARTITION_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Partition",
        name: "list",
        usage: "fox partition",
        summary: "list partitions with file system and supported operations",
        examples: &["fox partition"],
    },
    CommandInfo {
        group: "Partition",
        name: "repair",
        usage: "fox partition <path> repair",
        summary: "repair a file system",
        examples: &["fox partition /data repair"],
    },
    CommandInfo {
        group: "Partition",
        name: "resize",
        usage: "fox partition <path> resize",
        summary: "resize a file system",
        examples: &["fox partition /data resize"],
    },
    CommandInfo {
        group: "Partition",
        name: "change-fs",
        usage: "fox partition <path> change-fs <fs>",
        summary: "change file system type",
        examples: &["fox partition /data change-fs f2fs"],
    },
    CommandInfo {
        group: "Partition",
        name: "wipe",
        usage: "fox partition <path> wipe",
        summary: "wipe a partition through partition tools",
        examples: &["fox partition /metadata wipe"],
    },
];

const ADDONS_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Addons",
        name: "list",
        usage: "fox addons",
        summary: "list available Fox addons",
        examples: &["fox addons"],
    },
    CommandInfo {
        group: "Addons",
        name: "install",
        usage: "fox addons install <name...>",
        summary: "install addon zip files by name or path",
        examples: &["fox addons install addon.zip"],
    },
];

const DECRYPT_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Decrypt",
        name: "help",
        usage: "fox decrypt",
        summary: "show password and pattern entry help",
        examples: &["fox decrypt"],
    },
    CommandInfo {
        group: "Decrypt",
        name: "password",
        usage: "fox decrypt <password> [--user <id>]",
        summary: "decrypt storage for the selected Android user",
        examples: &["fox decrypt 14789 --user 0", "fox decrypt my-password"],
    },
];

const SIDELOAD_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Sideload",
        name: "help",
        usage: "fox sideload",
        summary: "show sideload help",
        examples: &["fox sideload"],
    },
    CommandInfo {
        group: "Sideload",
        name: "start",
        usage: "fox sideload start",
        summary: "start adb sideload mode",
        examples: &["fox sideload start"],
    },
];

const REBOOT_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "Reboot",
    name: "target",
    usage: "fox reboot <target>",
    summary: "reboot to a target",
    examples: &[
        "fox reboot system",
        "fox reboot recovery",
        "fox reboot bootloader",
    ],
}];

const FORMAT_DATA_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "Format Data",
    name: "confirm",
    usage: "fox format_data --yes",
    summary: "erase data and remove encryption",
    examples: &["fox format_data --yes"],
}];

const LOG_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "Log",
    name: "save",
    usage: "fox log save",
    summary: "save a recovery log zip",
    examples: &["fox log save"],
}];

const REFLASH_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "Reflash",
    name: "run",
    usage: "fox reflash run",
    summary: "reflash the current OrangeFox",
    examples: &["fox reflash run"],
}];

const STORAGES_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Storages",
        name: "list",
        usage: "fox storages",
        summary: "list available storage destinations",
        examples: &["fox storages"],
    },
    CommandInfo {
        group: "Storages",
        name: "storage",
        usage: "fox backup <part...> --storage <name|path>",
        summary: "use a storage alias or direct path for backups",
        examples: &[
            "fox backup boot data --storage internal",
            "fox backup data --storage microsd",
            "fox backup data --storage otg",
        ],
    },
];

const FIFO_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "FIFO",
    name: "send",
    usage: "fox fifo send <argv...>",
    summary: "send raw argv directly to foxin and print foxout",
    examples: &[
        "fox fifo send status",
        "fox fifo send mount /vendor",
        "fox fifo send flash /sdcard/rom.zip --no-verify",
    ],
}];

const INTERNAL_COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        group: "Internal",
        name: "get",
        usage: "fox internal get <var>",
        summary: "read a recovery variable",
        examples: &["fox internal get tw_version"],
    },
    CommandInfo {
        group: "Internal",
        name: "set",
        usage: "fox internal set <var> [value]",
        summary: "write a recovery variable",
        examples: &["fox internal set tw_mtp_enabled 1"],
    },
    CommandInfo {
        group: "Internal",
        name: "go",
        usage: "fox internal go <page>",
        summary: "trigger a recovery GUI action/page",
        examples: &["fox internal go reboot"],
    },
];

const ORS_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "OpenRecoveryScript",
    name: "ors",
    usage: "fox ors <file>",
    summary: "run an OpenRecoveryScript file",
    examples: &["fox ors /tmp/openrecoveryscript"],
}];

const ORS_CMD_COMMANDS: &[CommandInfo] = &[CommandInfo {
    group: "OpenRecoveryScript",
    name: "ors-cmd",
    usage: "fox ors-cmd <command...>",
    summary: "run one raw OpenRecoveryScript command",
    examples: &[
        "fox ors-cmd mount /system",
        "fox ors-cmd install /sdcard/rom.zip",
    ],
}];

fn page_commands(page: CommandPage) -> &'static [CommandInfo] {
    match page {
        CommandPage::Mount | CommandPage::Unmount => MOUNT_COMMANDS,
        CommandPage::Flash => FLASH_COMMANDS,
        CommandPage::Backup => BACKUP_COMMANDS,
        CommandPage::Decrypt => DECRYPT_COMMANDS,
        CommandPage::Sideload => SIDELOAD_COMMANDS,
        CommandPage::Restore => RESTORE_COMMANDS,
        CommandPage::Reboot => REBOOT_COMMANDS,
        CommandPage::Wipe => WIPE_COMMANDS,
        CommandPage::FormatData => FORMAT_DATA_COMMANDS,
        CommandPage::Partition => PARTITION_COMMANDS,
        CommandPage::Addons => ADDONS_COMMANDS,
        CommandPage::Wlan => WLAN_COMMANDS,
        CommandPage::Mtp => MTP_COMMANDS,
        CommandPage::Log => LOG_COMMANDS,
        CommandPage::Reflash => REFLASH_COMMANDS,
        CommandPage::Storages => STORAGES_COMMANDS,
        CommandPage::Fifo => FIFO_COMMANDS,
        CommandPage::Internal => INTERNAL_COMMANDS,
        CommandPage::Ors => ORS_COMMANDS,
        CommandPage::OrsCmd => ORS_CMD_COMMANDS,
    }
}

fn page_name(page: CommandPage) -> &'static str {
    match page {
        CommandPage::Mount => "mount",
        CommandPage::Unmount => "unmount",
        CommandPage::Flash => "flash",
        CommandPage::Backup => "backup",
        CommandPage::Decrypt => "decrypt",
        CommandPage::Sideload => "sideload",
        CommandPage::Restore => "restore",
        CommandPage::Reboot => "reboot",
        CommandPage::Wipe => "wipe",
        CommandPage::FormatData => "format_data",
        CommandPage::Partition => "partition",
        CommandPage::Addons => "addons",
        CommandPage::Wlan => "wlan",
        CommandPage::Mtp => "mtp",
        CommandPage::Log => "log",
        CommandPage::Reflash => "reflash",
        CommandPage::Storages => "storages",
        CommandPage::Fifo => "fifo",
        CommandPage::Internal => "internal",
        CommandPage::Ors => "ors",
        CommandPage::OrsCmd => "ors-cmd",
    }
}

fn page_title(page: CommandPage) -> &'static str {
    match page {
        CommandPage::Mount => "[Mount]",
        CommandPage::Unmount => "[Unmount]",
        CommandPage::Flash => "[Flash]",
        CommandPage::Backup => "[Backup]",
        CommandPage::Decrypt => "[Decrypt]",
        CommandPage::Sideload => "[Sideload]",
        CommandPage::Restore => "[Restore]",
        CommandPage::Reboot => "[Reboot]",
        CommandPage::Wipe => "[Wipe]",
        CommandPage::FormatData => "[Format Data]",
        CommandPage::Partition => "[Partition Management]",
        CommandPage::Addons => "[Addons]",
        CommandPage::Wlan => "[WLAN]",
        CommandPage::Mtp => "[MTP]",
        CommandPage::Log => "[Log]",
        CommandPage::Reflash => "[Reflash]",
        CommandPage::Storages => "[Storages]",
        CommandPage::Fifo => "[FIFO]",
        CommandPage::Internal => "[Internal]",
        CommandPage::Ors => "[OpenRecoveryScript]",
        CommandPage::OrsCmd => "[OpenRecoveryScript Command]",
    }
}

fn page_summary(page: CommandPage) -> &'static str {
    match page {
        CommandPage::Mount => "Mount partitions by path, or use the list below to check current mount state.",
        CommandPage::Unmount => "Unmount partitions by path, or use the list below to check current mount state.",
        CommandPage::Flash => "Flash zip packages. Add verification and reflash flags only when you need to override defaults.",
        CommandPage::Backup => "Create backups from selected partitions. Let OrangeFox generate the backup name unless you need a specific one.",
        CommandPage::Decrypt => "Decrypt storage, or inspect pattern/password entry help.",
        CommandPage::Sideload => "Start sideload explicitly, then use adb sideload from the host.",
        CommandPage::Restore => "Restore a named backup or a backup folder. Add partition names to restore only those parts.",
        CommandPage::Reboot => "Choose a reboot target explicitly.",
        CommandPage::Wipe => "Wipe selected targets. The list below shows what this recovery exposes as wipeable.",
        CommandPage::FormatData => "Destructive full data format. Requires --yes so it cannot happen by accident.",
        CommandPage::Partition => "Inspect and repair partition file systems, resize them, or change file system type.",
        CommandPage::Addons => "List available Fox addons or install addon zip files.",
        CommandPage::Wlan => "Show WLAN state and common network actions.",
        CommandPage::Mtp => "Show MTP state and common transfer-mode actions.",
        CommandPage::Log => "Save recovery logs when you explicitly ask for it.",
        CommandPage::Reflash => "Reflash the currently installed OrangeFox when you explicitly ask for it.",
        CommandPage::Storages => "List storage destinations that can be used with backup --storage.",
        CommandPage::Fifo => "Send a raw argv vector through foxin and stream foxout without typed foxcli parsing.",
        CommandPage::Internal => "Read or change recovery internals. This is mainly for scripts and debugging.",
        CommandPage::Ors => "Run an OpenRecoveryScript file through the recovery engine.",
        CommandPage::OrsCmd => "Run one raw OpenRecoveryScript command through the recovery engine.",
    }
}

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

#[derive(Serialize)]
struct FeatureJson<'a> {
    ok: bool,
    exit_code: i32,
    feature: &'a str,
    available: bool,
    enabled: bool,
    status: &'a str,
    stdout: &'a str,
    commands: &'a [CommandInfo],
}

#[derive(Serialize)]
struct CommandPageJson<'a> {
    ok: bool,
    exit_code: i32,
    command: &'a str,
    title: &'a str,
    summary: &'a str,
    stdout: &'a str,
    lines: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partitions: Option<Vec<PartitionRow>>,
    commands: &'a [CommandInfo],
}

struct WlanState<'a> {
    available: bool,
    enabled: bool,
    status: &'a str,
}

impl<'a> WlanState<'a> {
    fn from_response(exit_code: i32, stdout: &'a str) -> Self {
        if stdout.contains("WLAN support is not enabled") {
            return Self {
                available: false,
                enabled: false,
                status: "unavailable",
            };
        }
        if stdout.contains("WLAN is not enabled") {
            return Self {
                available: true,
                enabled: false,
                status: "disabled",
            };
        }
        if exit_code == 0 {
            return Self {
                available: true,
                enabled: true,
                status: "enabled",
            };
        }
        Self {
            available: true,
            enabled: false,
            status: "unknown",
        }
    }
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
