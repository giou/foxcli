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
    Sideload(Sideload),
    Wlan(Wlan),
    Reboot(Reboot),
    Internal(Internal),
    Wipe(Wipe),
    #[command(name = "format_data")]
    FormatData(FormatData),
    Reflash(Reflash),
    Mtp(Mtp),
    Log(Log),
    Partition(Partition),
    Addons(Addons),
    Storages(Storages),
    Ors(Script),
    #[command(name = "ors-cmd")]
    OrsCmd(Raw),
}

#[derive(Clone, Copy)]
pub enum CommandPage {
    Mount,
    Unmount,
    Flash,
    Backup,
    Decrypt,
    Sideload,
    Restore,
    Reboot,
    Wipe,
    FormatData,
    Partition,
    Addons,
    Wlan,
    Mtp,
    Log,
    Reflash,
    Storages,
    Internal,
    Ors,
    OrsCmd,
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
            Command::Sideload(_) => "sideload",
            Command::Wlan(_) => "wlan",
            Command::Reboot(_) => "reboot",
            Command::Internal(_) => "internal",
            Command::Wipe(_) => "wipe",
            Command::FormatData(_) => "format_data",
            Command::Reflash(_) => "reflash",
            Command::Mtp(_) => "mtp",
            Command::Log(_) => "log",
            Command::Partition(_) => "partition",
            Command::Addons(_) => "addons",
            Command::Storages(_) => "storages",
            Command::Ors(_) => "ors",
            Command::OrsCmd(_) => "ors-cmd",
        }
    }

    /// Whether the engine's output should be captured and rendered by the CLI
    /// (plain mode) rather than streamed through verbatim. Derived from the
    /// typed command so it can't drift from the argv builder below.
    pub fn renders_plain(&self) -> bool {
        match self {
            Command::Wlan(wlan) => wlan
                .command
                .as_ref()
                .map(WlanCommand::renders_plain)
                .unwrap_or(true),
            Command::Mtp(mtp) => matches!(mtp.action, None | Some(MtpAction::Status)),
            Command::Storages(_) => true,
            // No-argument listing forms emit a FOX_LIST block to be tabulated.
            Command::Mount(p) | Command::Unmount(p) => p.path.is_empty(),
            Command::Backup(b) => b.part.is_empty(),
            Command::Wipe(w) => w.target.is_empty(),
            Command::Partition(p) => p.path.is_none(),
            Command::Addons(a) => a.action.is_none(),
            _ => false,
        }
    }

    pub fn page(&self) -> Option<CommandPage> {
        match self {
            Command::Mount(paths) if paths.path.is_empty() => Some(CommandPage::Mount),
            Command::Unmount(paths) if paths.path.is_empty() => Some(CommandPage::Unmount),
            Command::Flash(flash) if flash.zip.is_empty() => Some(CommandPage::Flash),
            Command::Backup(backup)
                if backup.part.is_empty()
                    && backup.name.is_none()
                    && backup.path.is_none()
                    && backup.storage.is_none() =>
            {
                Some(CommandPage::Backup)
            }
            Command::Decrypt(decrypt) if decrypt.password.is_none() => Some(CommandPage::Decrypt),
            Command::Sideload(sideload) if sideload.action.is_none() => Some(CommandPage::Sideload),
            Command::Restore(restore)
                if restore.part.is_empty() && restore.name.is_none() && restore.path.is_none() =>
            {
                Some(CommandPage::Restore)
            }
            Command::Reboot(reboot) if reboot.target.is_none() => Some(CommandPage::Reboot),
            Command::Wipe(wipe) if wipe.target.is_empty() => Some(CommandPage::Wipe),
            Command::FormatData(format_data) if !format_data.yes => Some(CommandPage::FormatData),
            Command::Partition(partition) if partition.path.is_none() => {
                Some(CommandPage::Partition)
            }
            Command::Addons(addons) if addons.action.is_none() => Some(CommandPage::Addons),
            Command::Wlan(wlan) if wlan.command.is_none() => Some(CommandPage::Wlan),
            Command::Mtp(mtp) if mtp.action.is_none() => Some(CommandPage::Mtp),
            Command::Log(log) if log.action.is_none() => Some(CommandPage::Log),
            Command::Reflash(reflash) if reflash.action.is_none() => Some(CommandPage::Reflash),
            Command::Storages(_) => Some(CommandPage::Storages),
            Command::Internal(internal) if internal.command.is_none() => {
                Some(CommandPage::Internal)
            }
            Command::Ors(script) if script.script_file.is_none() => Some(CommandPage::Ors),
            Command::OrsCmd(raw) if raw.raw.is_empty() => Some(CommandPage::OrsCmd),
            _ => None,
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
                if flash.reflash_orangefox {
                    out.push("--reflash-orangefox".to_string());
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
                if let Some(storage) = backup.storage {
                    out.extend(["--storage".to_string(), storage]);
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
            Command::Sideload(_) => vec!["sideload".to_string()],
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
            Command::Reflash(_) => vec!["reflash".to_string()],
            Command::Mtp(mtp) => flatten_mtp(mtp.action),
            Command::Log(_) => vec!["log".to_string()],
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
            Command::Storages(_) => vec!["storages".to_string()],
            Command::Ors(script) => prepend("ors", script.script_file.into_iter().collect()),
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
    zip: Vec<String>,
    #[arg(long)]
    no_verify: bool,
    #[arg(long)]
    verify: bool,
    /// Reflash OrangeFox after flashing (overrides the UI default).
    #[arg(long = "reflash-orangefox", alias = "reflash")]
    reflash_orangefox: bool,
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
    #[arg(long, hide = true)]
    path: Option<String>,
    /// Destination storage alias or path: internal, microsd, otg, usb, or /path.
    #[arg(long)]
    storage: Option<String>,
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
pub enum SideloadAction {
    Start,
}

#[derive(Args)]
pub struct Sideload {
    /// start.
    action: Option<SideloadAction>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ReflashAction {
    Run,
}

#[derive(Args)]
pub struct Reflash {
    /// run.
    action: Option<ReflashAction>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum LogAction {
    Save,
}

#[derive(Args)]
pub struct Log {
    /// save.
    action: Option<LogAction>,
}

#[derive(Args)]
pub struct Storages {}

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
    script_file: Option<String>,
}

#[derive(Args)]
pub struct Raw {
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
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
        matches!(
            self,
            WlanCommand::List | WlanCommand::Saved | WlanCommand::Info
        )
    }
}

#[derive(Args)]
pub struct Wlan {
    #[command(subcommand)]
    command: Option<WlanCommand>,
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
    command: Option<InternalCommand>,
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

fn flatten_wlan(command: Option<WlanCommand>) -> Vec<String> {
    match command {
        None => vec!["wlan".to_string(), "info".to_string()],
        Some(WlanCommand::Enable) => vec!["wlan".to_string(), "enable".to_string()],
        Some(WlanCommand::Disable) => vec!["wlan".to_string(), "disable".to_string()],
        Some(WlanCommand::List) => vec!["wlan".to_string(), "list".to_string()],
        Some(WlanCommand::Connect(connect)) => {
            let mut out = vec!["wlan".to_string(), "connect".to_string(), connect.ssid];
            if let Some(password) = connect.password {
                out.extend(["--password".to_string(), password]);
            }
            out
        }
        Some(WlanCommand::Saved) => vec!["wlan".to_string(), "saved".to_string()],
        Some(WlanCommand::ConnectSaved(ssid)) => {
            vec!["wlan".to_string(), "connect-saved".to_string(), ssid.ssid]
        }
        Some(WlanCommand::Forget(ssid)) => {
            vec!["wlan".to_string(), "forget".to_string(), ssid.ssid]
        }
        Some(WlanCommand::Info) => vec!["wlan".to_string(), "info".to_string()],
        Some(WlanCommand::Test) => vec!["wlan".to_string(), "test".to_string()],
    }
}

fn flatten_mtp(action: Option<MtpAction>) -> Vec<String> {
    let mut out = vec!["mtp".to_string()];
    if let Some(action) = action {
        out.push(action.to_string());
    }
    out
}

fn flatten_internal(command: Option<InternalCommand>) -> Vec<String> {
    match command {
        None => vec!["internal".to_string()],
        Some(InternalCommand::Get(get)) => vec!["internal".to_string(), "get".to_string(), get.var],
        Some(InternalCommand::Set(set)) => {
            let mut out = vec!["internal".to_string(), "set".to_string(), set.var];
            if let Some(value) = set.value {
                out.push(value);
            }
            out
        }
        Some(InternalCommand::Go(go)) => vec!["internal".to_string(), "go".to_string(), go.page],
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
                reflash_orangefox: false,
                no_reflash: false,
                unmount_system: false,
                unmount_vendor: false,
            }),
            Command::Backup(Backup {
                part: vec![],
                name: None,
                path: None,
                storage: None,
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
            Command::Sideload(Sideload {
                action: Some(SideloadAction::Start),
            }),
            Command::Wlan(Wlan {
                command: Some(WlanCommand::Info),
            }),
            Command::Reboot(Reboot { target: None }),
            Command::Internal(Internal {
                command: Some(InternalCommand::Go(InternalGo {
                    page: String::new(),
                })),
            }),
            Command::Wipe(Wipe { target: vec![] }),
            Command::FormatData(FormatData { yes: false }),
            Command::Reflash(Reflash {
                action: Some(ReflashAction::Run),
            }),
            Command::Mtp(Mtp { action: None }),
            Command::Log(Log {
                action: Some(LogAction::Save),
            }),
            Command::Partition(Partition {
                path: None,
                op: vec![],
            }),
            Command::Addons(Addons {
                action: None,
                names: vec![],
            }),
            Command::Storages(Storages {}),
            Command::Ors(Script {
                script_file: Some(String::new()),
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
                | Command::Sideload(_)
                | Command::Wlan(_)
                | Command::Reboot(_)
                | Command::Internal(_)
                | Command::Wipe(_)
                | Command::FormatData(_)
                | Command::Reflash(_)
                | Command::Mtp(_)
                | Command::Log(_)
                | Command::Partition(_)
                | Command::Addons(_)
                | Command::Storages(_)
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
