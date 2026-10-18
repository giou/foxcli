use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

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
    #[command(alias = "wifi")]
    Wlan(Wlan),
    /// Networking umbrella: web server, ttyd, and network ADB.
    Web(Web),
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
    Ota(Ota),
    Rpc(Rpc),
    Fifo(Fifo),
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
    Fifo,
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
            Command::Web(_) => "web",
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
            Command::Ota(_) => "ota",
            Command::Rpc(_) => "rpc",
            Command::Fifo(_) => "fifo",
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
            Command::Web(web) => web.renders_plain(),
            Command::Mtp(mtp) => matches!(mtp.action, None | Some(MtpAction::Status)),
            Command::Storages(_) => true,
            Command::Ota(ota) => matches!(ota.action, None | Some(OtaAction::Status)),
            Command::Rpc(_) => false,
            // No-argument listing forms emit a `list` data event to be tabulated.
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
            Command::Fifo(fifo) if fifo.command.is_none() => Some(CommandPage::Fifo),
            Command::Internal(internal) if internal.command.is_none() => {
                Some(CommandPage::Internal)
            }
            Command::Ors(script) if script.script_file.is_none() => Some(CommandPage::Ors),
            Command::OrsCmd(raw) if raw.raw.is_empty() => Some(CommandPage::OrsCmd),
            _ => None,
        }
    }

    /// Build the typed Fox RPC request (op + args JSON) for this command
    /// directly — no argv round-trip. The op/args field names are the wire
    /// contract in `bootable/recovery/docs/fox-rpc-schema.md`; the engine's
    /// `Cmd_*` handlers read the same fields. The raw `fifo send` bridge and the
    /// internal status/page probes still go through `argv_to_rpc` in
    /// `transport.rs` (token input, no typed struct to drift from).
    pub fn to_request(self) -> (&'static str, Value) {
        match self {
            Command::Status => ("status", json!({})),
            Command::Mount(p) => ("mount", json!({ "paths": p.path })),
            Command::Unmount(p) => ("unmount", json!({ "paths": p.path })),
            Command::Flash(f) => (
                "flash",
                json!({
                    "zips": f.zip,
                    "verify": tristate(f.verify, f.no_verify),
                    "reflash": tristate(f.reflash_orangefox, f.no_reflash),
                    "unmount_system": f.unmount_system,
                    "unmount_vendor": f.unmount_vendor,
                }),
            ),
            Command::Backup(b) => (
                "backup",
                json!({
                    "parts": b.part,
                    "name": b.name,
                    "path": b.path,
                    "storage": b.storage,
                    "compress": b.compression,
                    "digest": !b.disable_digest_generation,
                }),
            ),
            Command::Restore(r) => (
                "restore",
                json!({
                    "parts": r.part,
                    "name": r.name,
                    "path": r.path,
                    "digest_check": !r.disable_digest_check,
                }),
            ),
            Command::Decrypt(d) => ("decrypt", json!({ "password": d.password, "user": d.user })),
            Command::Sideload(_) => ("sideload", json!({})),
            Command::Wlan(wlan) => ("wlan", wlan_request(wlan.command)),
            Command::Web(web) => web_request(web.command),
            Command::Reboot(r) => ("reboot", json!({ "target": r.target })),
            Command::Internal(i) => ("internal", internal_request(i.command)),
            Command::Wipe(w) => ("wipe", json!({ "parts": w.target })),
            Command::FormatData(fd) => ("format_data", json!({ "confirm": fd.yes })),
            Command::Reflash(_) => ("reflash", json!({})),
            Command::Mtp(mtp) => ("mtp", json!({ "action": mtp.action.map(MtpAction::to_string) })),
            Command::Log(_) => ("log", json!({})),
            Command::Partition(p) => (
                "partition",
                json!({ "path": p.path, "action": p.op.first(), "fs": p.op.get(1) }),
            ),
            Command::Addons(a) => ("addons", json!({ "action": a.action, "names": a.names })),
            Command::Storages(_) => ("storages", json!({})),
            Command::Ota(o) => (
                "ota",
                json!({
                    "action": o.action.map(OtaAction::to_string),
                    "fox_id": o.fox_id,
                    "type": o.release_type,
                }),
            ),
            Command::Ors(s) => ("ors", json!({ "path": s.script_file })),
            Command::OrsCmd(r) => ("ors-cmd", json!({ "raw": r.raw })),
            // Handled before this point in main(): raw token bridge / stdin JSON.
            Command::Fifo(_) | Command::Rpc(_) => unreachable!("fifo/rpc handled in main"),
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

#[derive(Args)]
pub struct Rpc {}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OtaAction {
    Status,
    Init,
    Check,
    Download,
    Install,
    Refresh,
}

impl OtaAction {
    fn to_string(self) -> String {
        match self {
            OtaAction::Status => "status",
            OtaAction::Init => "init",
            OtaAction::Check => "check",
            OtaAction::Download => "download",
            OtaAction::Install => "install",
            OtaAction::Refresh => "refresh",
        }
        .to_string()
    }
}

#[derive(Args)]
pub struct Ota {
    /// status (default), init, check, download, install, or refresh.
    action: Option<OtaAction>,
    /// Override the current fox_id/build_id for OTA API testing.
    #[arg(long = "fox-id")]
    fox_id: Option<String>,
    /// Comma-separated release types to query: stable,beta,alpha.
    #[arg(long = "type")]
    release_type: Option<String>,
}

#[derive(Args)]
pub struct Fifo {
    #[command(subcommand)]
    command: Option<FifoCommand>,
}

impl Fifo {
    /// The verbatim tokens for the raw `fifo send` bridge (`argv_to_rpc` then
    /// builds the request from these), or empty for a bare `fifo`.
    pub fn into_tokens(self) -> Vec<String> {
        flatten_fifo(self.command)
    }
}

#[derive(Subcommand)]
enum FifoCommand {
    /// Debug command flattening through the Fox RPC FIFO transport.
    Send(FifoSend),
}

#[derive(Args)]
struct FifoSend {
    #[arg(required = true, allow_hyphen_values = true, trailing_var_arg = true)]
    raw: Vec<String>,
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
    /// WiFi hotspot / access point control.
    ///
    /// Verbs: enable | disable | status | ssid <name> | password <pw> | list.
    Ap(WlanAp),
}

impl WlanCommand {
    /// Read-only queries whose output is captured and pretty-printed.
    fn renders_plain(&self) -> bool {
        match self {
            WlanCommand::List | WlanCommand::Saved | WlanCommand::Info => true,
            // `ap` with no verb, `ap status`, and `ap list` are read-only.
            WlanCommand::Ap(group) => {
                matches!(group.verb.as_deref(), None | Some("status") | Some("list"))
            }
            _ => false,
        }
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

/// `fox wlan ap <verb> [tokens...]`. The verb (enable/disable/status/ssid/
/// password/list) and its trailing tokens flatten into the subaction/values the
/// engine's hotspot handler consumes.
#[derive(Args)]
struct WlanAp {
    verb: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    rest: Vec<String>,
}

#[derive(Args)]
pub struct Web {
    #[command(subcommand)]
    command: Option<WebCommand>,
}

impl Web {
    /// Read-only forms whose output is captured and pretty-printed. Anything
    /// that mutates state is streamed through verbatim.
    fn renders_plain(&self) -> bool {
        match &self.command {
            None | Some(WebCommand::Status) | Some(WebCommand::Password(_)) => true,
            Some(WebCommand::Webserver(g)) | Some(WebCommand::Adb(g)) => {
                // Read-only when the verb is a query, or when it's a transport
                // group (`http`/`https`/…) whose nested verb is a query.
                let query = |v: Option<&str>| matches!(v, Some("status") | Some("list"));
                g.verb.is_none() || query(g.verb.as_deref()) || query(g.rest.first().map(String::as_str))
            }
            _ => false,
        }
    }
}

#[derive(Subcommand)]
enum WebCommand {
    /// Enable the whole networking group: web server + ttyd + network ADB.
    Enable,
    /// Disable the whole networking group.
    Disable,
    /// Show networking status.
    Status,
    /// Show, set, generate, or clear the shared remote password.
    Password(WebPassword),
    /// HTTP/HTTPS server and its transports/policy.
    ///
    /// Verbs: enable | disable | status | http <v> | https <v> | redirect <v>
    /// | cert {regen|status} | cors {set <origin>|reset|status} | terminal <v>.
    Webserver(WebGroup),
    /// Network ADB.
    ///
    /// Verbs: enable | disable | status | pair | list | forget <id|name>.
    Adb(WebGroup),
}

#[derive(Args)]
struct WebPassword {
    /// New password. Omit to show the current one; "clear" removes it.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    value: Vec<String>,
    /// Generate a strong random password and set it.
    #[arg(long)]
    generate: bool,
}

/// A "<verb> [tokens...]" group shared by `web webserver` and `web adb`. The
/// verb and its trailing tokens flatten straight into the positional argv the
/// engine's action handler consumes (action/subaction/values).
#[derive(Args)]
struct WebGroup {
    verb: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    rest: Vec<String>,
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
    /// Dismiss the screen-off lock overlay and restore the display.
    /// Engine-side `internal unlock` (bootable/recovery fox_fifo).
    Unlock,
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

// `Some(true)`/`Some(false)` for an explicit yes/no flag pair, else `None`.
fn tristate(yes: bool, no: bool) -> Option<bool> {
    if yes {
        Some(true)
    } else if no {
        Some(false)
    } else {
        None
    }
}

// Remove `--name` / `--name=value` from a raw token list, returning its value.
fn take_opt(rest: &mut Vec<String>, name: &str) -> Option<String> {
    if let Some(i) = rest.iter().position(|a| a == name) {
        if i + 1 < rest.len() {
            let value = rest.remove(i + 1);
            rest.remove(i);
            return Some(value);
        }
        rest.remove(i);
        return None;
    }
    let prefix = format!("{name}=");
    if let Some(i) = rest.iter().position(|a| a.starts_with(&prefix)) {
        return Some(rest.remove(i)[prefix.len()..].to_string());
    }
    None
}

fn wlan_request(command: Option<WlanCommand>) -> Value {
    // The hotspot subtree carries verb + tokens rather than the ssid/password
    // pair, so it builds its own request shape.
    if let Some(WlanCommand::Ap(group)) = command {
        return json!({
            "action": "ap",
            "subaction": group.verb,
            "values": group.rest,
        });
    }

    let (action, ssid, password): (&str, Option<String>, Option<String>) = match command {
        None | Some(WlanCommand::Info) => ("info", None, None),
        Some(WlanCommand::Enable) => ("enable", None, None),
        Some(WlanCommand::Disable) => ("disable", None, None),
        Some(WlanCommand::List) => ("list", None, None),
        Some(WlanCommand::Connect(c)) => ("connect", Some(c.ssid), c.password),
        Some(WlanCommand::Saved) => ("saved", None, None),
        Some(WlanCommand::ConnectSaved(s)) => ("connect-saved", Some(s.ssid), None),
        Some(WlanCommand::Forget(s)) => ("forget", Some(s.ssid), None),
        Some(WlanCommand::Test) => ("test", None, None),
        Some(WlanCommand::Ap(_)) => unreachable!("handled above"),
    };
    json!({ "action": action, "ssid": ssid, "password": password })
}

// `web webserver ...` maps to the `webserver` op, `web adb ...` to `adbd`.
fn web_request(command: Option<WebCommand>) -> (&'static str, Value) {
    match command {
        None => ("web", json!({ "action": "status" })),
        Some(WebCommand::Enable) => ("web", json!({ "action": "enable" })),
        Some(WebCommand::Disable) => ("web", json!({ "action": "disable" })),
        Some(WebCommand::Status) => ("web", json!({ "action": "status" })),
        Some(WebCommand::Password(p)) => {
            // The password value rides in subaction + values (the engine joins
            // them); --generate is a typed bool.
            let mut values = p.value;
            let subaction = if values.is_empty() {
                None
            } else {
                Some(values.remove(0))
            };
            (
                "web",
                json!({
                    "action": "password",
                    "subaction": subaction,
                    "values": values,
                    "generate": p.generate,
                }),
            )
        }
        Some(WebCommand::Webserver(group)) => ("webserver", group_request(group)),
        Some(WebCommand::Adb(group)) => ("adbd", group_request(group)),
    }
}

// The generic group shape: action = verb, subaction = the first remaining
// token, values = the rest, with --port/--timeout pulled into typed fields.
fn group_request(group: WebGroup) -> Value {
    let mut rest = group.rest;
    let port = take_opt(&mut rest, "--port").and_then(|v| v.parse::<u16>().ok());
    let timeout = take_opt(&mut rest, "--timeout").and_then(|v| v.parse::<u32>().ok());
    let subaction = if rest.is_empty() {
        None
    } else {
        Some(rest.remove(0))
    };
    json!({
        "action": group.verb,
        "subaction": subaction,
        "values": rest,
        "port": port,
        "timeout": timeout,
    })
}

fn flatten_fifo(command: Option<FifoCommand>) -> Vec<String> {
    match command {
        None => vec![],
        Some(FifoCommand::Send(send)) => send.raw,
    }
}

fn internal_request(command: Option<InternalCommand>) -> Value {
    match command {
        None => json!({}),
        Some(InternalCommand::Get(g)) => json!({ "action": "get", "name": g.var }),
        Some(InternalCommand::Set(s)) => {
            json!({ "action": "set", "name": s.var, "value": s.value })
        }
        Some(InternalCommand::Go(g)) => json!({ "action": "go", "page": g.page }),
        Some(InternalCommand::Unlock) => json!({ "action": "unlock" }),
    }
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
            Command::Web(Web {
                command: Some(WebCommand::Status),
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
            Command::Ota(Ota {
                action: Some(OtaAction::Status),
                fox_id: None,
                release_type: None,
            }),
            Command::Rpc(Rpc {}),
            Command::Fifo(Fifo {
                command: Some(FifoCommand::Send(FifoSend {
                    raw: vec!["status".to_string()],
                })),
            }),
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
                | Command::Web(_)
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
                | Command::Ota(_)
                | Command::Rpc(_)
                | Command::Fifo(_)
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
