use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fox", about = "FoxCLI (modern OrangeFox cli)", version)]
pub struct Cli {
    #[arg(long, global = true, help = "Return command output as JSON")]
    pub json: bool,
    #[arg(long, global = true, help = "Disable colored plain-text output")]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    Status,
    Mount(Paths),
    #[command(alias = "umount")]
    Unmount(Paths),
    #[command(alias = "install")]
    Flash(Flash),
    Backup(Backup),
    Restore(Restore),
    Decrypt(Decrypt),
    Sideload,
    Wlan(Wlan),
    Reboot(Reboot),
    Internal(Internal),
    Wipe(Wipe),
    #[command(name = "format_data")]
    FormatData(FormatData),
    Reflash,
    Mtp(Mtp),
    Log,
    Partition(Partition),
    Addons(Addons),
    Ors(Script),
    #[command(name = "ors-cmd")]
    OrsCmd(Raw),
}

impl Command {
    pub fn display_name(&self) -> &'static str {
        match self {
            Command::Status => "status",
            Command::Mount(_) => "mount",
            Command::Unmount(_) => "unmount",
            Command::Flash(_) => "flash",
            Command::Backup(_) => "backup",
            Command::Restore(_) => "restore",
            Command::Decrypt(_) => "decrypt",
            Command::Sideload => "sideload",
            Command::Wlan(_) => "wlan",
            Command::Reboot(_) => "reboot",
            Command::Internal(_) => "internal",
            Command::Wipe(_) => "wipe",
            Command::FormatData(_) => "format_data",
            Command::Reflash => "reflash",
            Command::Mtp(_) => "mtp",
            Command::Log => "log",
            Command::Partition(_) => "partition",
            Command::Addons(_) => "addons",
            Command::Ors(_) => "ors",
            Command::OrsCmd(_) => "ors-cmd",
        }
    }

    /// Whether the engine's output should be captured and rendered by the CLI
    /// (plain mode) rather than streamed through verbatim. Derived from the
    /// typed command so it can't drift from the argv builder below.
    pub fn renders_plain(&self) -> bool {
        match self {
            Command::Wlan(wlan) => wlan.command.renders_plain(),
            // No-argument listing forms emit a FOX_LIST block to be tabulated.
            Command::Mount(p) | Command::Unmount(p) => p.path.is_empty(),
            Command::Backup(b) => b.part.is_empty(),
            Command::Wipe(w) => w.target.is_empty(),
            Command::Partition(p) => p.path.is_none(),
            Command::Addons(a) => a.action.is_none(),
            _ => false,
        }
    }

    pub fn into_engine_argv(self) -> Vec<String> {
        match self {
            Command::Status => vec!["status".to_string()],
            Command::Mount(paths) => prepend("mount", paths.path),
            Command::Unmount(paths) => prepend("unmount", paths.path),
            Command::Flash(flash) => {
                let mut out = vec!["flash".to_string()];
                out.extend(flash.zip);
                if flash.no_verify {
                    out.push("--no-verify".to_string());
                }
                if flash.verify {
                    out.push("--verify".to_string());
                }
                if flash.reflash {
                    out.push("--reflash".to_string());
                }
                if flash.no_reflash {
                    out.push("--no-reflash".to_string());
                }
                if flash.unmount_system {
                    out.push("--unmount-system".to_string());
                }
                if flash.unmount_vendor {
                    out.push("--unmount-vendor".to_string());
                }
                out
            }
            Command::Backup(backup) => {
                let mut out = vec!["backup".to_string()];
                out.extend(backup.part);
                if let Some(name) = backup.name {
                    out.extend(["--name".to_string(), name]);
                }
                if let Some(path) = backup.path {
                    out.extend(["--path".to_string(), path]);
                }
                if backup.compression {
                    out.push("--compression".to_string());
                }
                if backup.disable_digest_generation {
                    out.push("--disable-digest-generation".to_string());
                }
                out
            }
            Command::Restore(restore) => {
                let mut out = vec!["restore".to_string()];
                out.extend(restore.part);
                if let Some(name) = restore.name {
                    out.extend(["--name".to_string(), name]);
                }
                if let Some(path) = restore.path {
                    out.extend(["--path".to_string(), path]);
                }
                if restore.disable_digest_check {
                    out.push("--disable-digest-check".to_string());
                }
                out
            }
            Command::Decrypt(decrypt) => {
                let mut out = vec!["decrypt".to_string()];
                if let Some(password) = decrypt.password {
                    out.push(password);
                }
                if let Some(user) = decrypt.user {
                    out.extend(["--user".to_string(), user.to_string()]);
                }
                out
            }
            Command::Sideload => vec!["sideload".to_string()],
            Command::Wlan(wlan) => flatten_wlan(wlan.command),
            Command::Reboot(reboot) => {
                let mut out = vec!["reboot".to_string()];
                if let Some(target) = reboot.target {
                    out.push(target);
                }
                out
            }
            Command::Internal(internal) => flatten_internal(internal.command),
            Command::Wipe(wipe) => prepend("wipe", wipe.target),
            Command::FormatData(fd) => {
                let mut out = vec!["format_data".to_string()];
                if fd.yes {
                    out.push("--yes".to_string());
                }
                out
            }
            Command::Reflash => vec!["reflash".to_string()],
            Command::Mtp(mtp) => {
                let mut out = vec!["mtp".to_string()];
                if let Some(action) = mtp.action {
                    out.push(action.to_string());
                }
                out
            }
            Command::Log => vec!["log".to_string()],
            Command::Partition(partition) => {
                let mut out = vec!["partition".to_string()];
                if let Some(path) = partition.path {
                    out.push(path);
                    out.extend(partition.op);
                }
                out
            }
            Command::Addons(addons) => {
                let mut out = vec!["addons".to_string()];
                if let Some(action) = addons.action {
                    out.push(action);
                    out.extend(addons.names);
                }
                out
            }
            Command::Ors(script) => vec!["ors".to_string(), script.script_file],
            Command::OrsCmd(raw) => prepend("ors-cmd", raw.raw),
        }
    }
}

#[derive(Args)]
pub struct Paths {
    /// Partition(s) to act on. With none, lists partitions and their mount state.
    path: Vec<String>,
}

#[derive(Args)]
pub struct Flash {
    #[arg(required = true)]
    zip: Vec<String>,
    #[arg(long)]
    no_verify: bool,
    #[arg(long)]
    verify: bool,
    /// Reflash OrangeFox after flashing (overrides the UI default).
    #[arg(long)]
    reflash: bool,
    #[arg(long)]
    no_reflash: bool,
    #[arg(long)]
    unmount_system: bool,
    #[arg(long)]
    unmount_vendor: bool,
}

#[derive(Args)]
pub struct Backup {
    /// Partition(s) to back up. With none, lists what can be backed up.
    part: Vec<String>,
    #[arg(long)]
    name: Option<String>,
    /// Destination storage path for the backup.
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    compression: bool,
    #[arg(long)]
    disable_digest_generation: bool,
}

#[derive(Args)]
pub struct Restore {
    part: Vec<String>,
    /// Backup name under the storage's backups folder.
    #[arg(long)]
    name: Option<String>,
    /// Absolute path to a backup folder.
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    disable_digest_check: bool,
}

#[derive(Args)]
pub struct Wipe {
    /// Partition(s) or keywords (dalvik, cache) to wipe. With none, lists wipeable partitions.
    target: Vec<String>,
}

#[derive(Args)]
pub struct FormatData {
    /// Confirm the destructive full data format (erases everything, removes encryption).
    #[arg(long)]
    yes: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum MtpAction {
    Status,
    Enable,
    Disable,
}

impl MtpAction {
    fn to_string(self) -> String {
        match self {
            MtpAction::Status => "status",
            MtpAction::Enable => "enable",
            MtpAction::Disable => "disable",
        }
        .to_string()
    }
}

#[derive(Args)]
pub struct Mtp {
    /// status (default), enable, or disable.
    action: Option<MtpAction>,
}

#[derive(Args)]
pub struct Partition {
    /// Partition path. With none, lists partitions with file system and capabilities.
    path: Option<String>,
    /// Operation: repair | resize | change-fs <fs> | wipe.
    #[arg(trailing_var_arg = true)]
    op: Vec<String>,
}

#[derive(Args)]
pub struct Addons {
    /// list (default) or install.
    action: Option<String>,
    /// Addon zip name(s) or path(s) when installing.
    names: Vec<String>,
}

#[derive(Args)]
pub struct Decrypt {
    pub password: Option<String>,
    #[arg(long)]
    pub user: Option<u32>,
}

#[derive(Args)]
pub struct Reboot {
    target: Option<String>,
}

#[derive(Args)]
pub struct Script {
    script_file: String,
}

#[derive(Args)]
pub struct Raw {
    #[arg(required = true, allow_hyphen_values = true, trailing_var_arg = true)]
    raw: Vec<String>,
}

#[derive(Subcommand)]
enum WlanCommand {
    Enable,
    Disable,
    List,
    Connect(WlanConnect),
    Saved,
    ConnectSaved(WlanSsid),
    Forget(WlanSsid),
    Info,
    Test,
}

impl WlanCommand {
    /// Read-only queries whose output is captured and pretty-printed.
    fn renders_plain(&self) -> bool {
        matches!(self, WlanCommand::List | WlanCommand::Saved | WlanCommand::Info)
    }
}

#[derive(Args)]
pub struct Wlan {
    #[command(subcommand)]
    command: WlanCommand,
}

#[derive(Args)]
struct WlanConnect {
    ssid: String,
    #[arg(long)]
    password: Option<String>,
}

#[derive(Args)]
struct WlanSsid {
    ssid: String,
}

#[derive(Subcommand)]
enum InternalCommand {
    Get(InternalGet),
    Set(InternalSet),
    Go(InternalGo),
}

#[derive(Args)]
pub struct Internal {
    #[command(subcommand)]
    command: InternalCommand,
}

#[derive(Args)]
struct InternalGet {
    var: String,
}

#[derive(Args)]
struct InternalSet {
    var: String,
    value: Option<String>,
}

#[derive(Args)]
struct InternalGo {
    page: String,
}

fn flatten_wlan(command: WlanCommand) -> Vec<String> {
    match command {
        WlanCommand::Enable => vec!["wlan".to_string(), "enable".to_string()],
        WlanCommand::Disable => vec!["wlan".to_string(), "disable".to_string()],
        WlanCommand::List => vec!["wlan".to_string(), "list".to_string()],
        WlanCommand::Connect(connect) => {
            let mut out = vec!["wlan".to_string(), "connect".to_string(), connect.ssid];
            if let Some(password) = connect.password {
                out.extend(["--password".to_string(), password]);
            }
            out
        }
        WlanCommand::Saved => vec!["wlan".to_string(), "saved".to_string()],
        WlanCommand::ConnectSaved(ssid) => {
            vec!["wlan".to_string(), "connect-saved".to_string(), ssid.ssid]
        }
        WlanCommand::Forget(ssid) => vec!["wlan".to_string(), "forget".to_string(), ssid.ssid],
        WlanCommand::Info => vec!["wlan".to_string(), "info".to_string()],
        WlanCommand::Test => vec!["wlan".to_string(), "test".to_string()],
    }
}

fn flatten_internal(command: InternalCommand) -> Vec<String> {
    match command {
        InternalCommand::Get(get) => vec!["internal".to_string(), "get".to_string(), get.var],
        InternalCommand::Set(set) => {
            let mut out = vec!["internal".to_string(), "set".to_string(), set.var];
            if let Some(value) = set.value {
                out.push(value);
            }
            out
        }
        InternalCommand::Go(go) => vec!["internal".to_string(), "go".to_string(), go.page],
    }
}

fn prepend(verb: &str, values: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(values.len() + 1);
    out.push(verb.to_string());
    out.extend(values);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // One value per `Command` variant. The exhaustive match means adding or
    // removing a variant forces this list (and the help catalog) to be updated.
    fn one_of_each() -> Vec<Command> {
        let all = vec![
            Command::Status,
            Command::Mount(Paths { path: vec![] }),
            Command::Unmount(Paths { path: vec![] }),
            Command::Flash(Flash {
                zip: vec![],
                no_verify: false,
                verify: false,
                reflash: false,
                no_reflash: false,
                unmount_system: false,
                unmount_vendor: false,
            }),
            Command::Backup(Backup {
                part: vec![],
                name: None,
                path: None,
                compression: false,
                disable_digest_generation: false,
            }),
            Command::Restore(Restore {
                part: vec![],
                name: None,
                path: None,
                disable_digest_check: false,
            }),
            Command::Decrypt(Decrypt {
                password: None,
                user: None,
            }),
            Command::Sideload,
            Command::Wlan(Wlan {
                command: WlanCommand::Info,
            }),
            Command::Reboot(Reboot { target: None }),
            Command::Internal(Internal {
                command: InternalCommand::Go(InternalGo { page: String::new() }),
            }),
            Command::Wipe(Wipe { target: vec![] }),
            Command::FormatData(FormatData { yes: false }),
            Command::Reflash,
            Command::Mtp(Mtp { action: None }),
            Command::Log,
            Command::Partition(Partition {
                path: None,
                op: vec![],
            }),
            Command::Addons(Addons {
                action: None,
                names: vec![],
            }),
            Command::Ors(Script {
                script_file: String::new(),
            }),
            Command::OrsCmd(Raw { raw: vec![] }),
        ];
        for command in &all {
            // Compile-time exhaustiveness guard: a new variant breaks the build
            // here until it is added above and to the help catalog.
            match command {
                Command::Status
                | Command::Mount(_)
                | Command::Unmount(_)
                | Command::Flash(_)
                | Command::Backup(_)
                | Command::Restore(_)
                | Command::Decrypt(_)
                | Command::Sideload
                | Command::Wlan(_)
                | Command::Reboot(_)
                | Command::Internal(_)
                | Command::Wipe(_)
                | Command::FormatData(_)
                | Command::Reflash
                | Command::Mtp(_)
                | Command::Log
                | Command::Partition(_)
                | Command::Addons(_)
                | Command::Ors(_)
                | Command::OrsCmd(_) => {}
            }
        }
        all
    }

    #[test]
    fn every_command_is_in_help_catalog() {
        let names: Vec<&str> = crate::output::command_names().collect();
        for command in one_of_each() {
            let name = command.display_name();
            assert!(
                names.contains(&name),
                "command `{name}` is missing from the help catalog in output.rs"
            );
        }
    }
}
