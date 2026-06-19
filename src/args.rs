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
            Command::Ors(_) => "ors",
            Command::OrsCmd(_) => "ors-cmd",
        }
    }

    pub fn into_engine_argv(self) -> Vec<String> {
        match self {
            Command::Status => vec!["status".to_string()],
            Command::Mount(paths) => prepend("mount", paths.path),
            Command::Unmount(paths) => prepend("unmount", paths.path),
            Command::Flash(flash) => {
                let mut out = vec!["flash".to_string(), flash.zip];
                if flash.no_verify {
                    out.push("--no-verify".to_string());
                }
                out
            }
            Command::Backup(backup) => {
                let mut out = vec!["backup".to_string()];
                out.extend(backup.part);
                if let Some(name) = backup.name {
                    out.extend(["--name".to_string(), name]);
                }
                if backup.compress {
                    out.push("--compress".to_string());
                }
                if backup.no_digest {
                    out.push("--no-digest".to_string());
                }
                out
            }
            Command::Restore(restore) => {
                let mut out = vec!["restore".to_string()];
                out.extend(restore.part);
                out.extend(["--name".to_string(), restore.name]);
                if restore.no_digest_check {
                    out.push("--no-digest-check".to_string());
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
            Command::Ors(script) => vec!["ors".to_string(), script.script_file],
            Command::OrsCmd(raw) => prepend("ors-cmd", raw.raw),
        }
    }
}

#[derive(Args)]
pub struct Paths {
    #[arg(required = true)]
    path: Vec<String>,
}

#[derive(Args)]
pub struct Flash {
    zip: String,
    #[arg(long)]
    no_verify: bool,
}

#[derive(Args)]
pub struct Backup {
    #[arg(required = true)]
    part: Vec<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    compress: bool,
    #[arg(long)]
    no_digest: bool,
}

#[derive(Args)]
pub struct Restore {
    part: Vec<String>,
    #[arg(long)]
    name: String,
    #[arg(long)]
    no_digest_check: bool,
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
