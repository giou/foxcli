use clap::{Args, Parser, Subcommand};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::process;
use std::thread;
use std::time::Duration;

const FOX_INPUT_FILE: &str = "/system/bin/foxin";
const FOX_OUTPUT_FILE: &str = "/system/bin/foxout";
const EXIT_SENTINEL: &[u8] = b"\x04EXIT ";

#[derive(Parser)]
#[command(name = "fox", about = "FoxCLI (modern OrangeFox cli)", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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

#[derive(Args)]
struct Paths {
    #[arg(required = true)]
    path: Vec<String>,
}

#[derive(Args)]
struct Flash {
    zip: String,
    #[arg(long)]
    no_verify: bool,
}

#[derive(Args)]
struct Backup {
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
struct Restore {
    part: Vec<String>,
    #[arg(long)]
    name: String,
    #[arg(long)]
    no_digest_check: bool,
}

#[derive(Args)]
struct Decrypt {
    password: String,
    #[arg(long)]
    user: Option<u32>,
}

#[derive(Args)]
struct Reboot {
    target: Option<String>,
}

#[derive(Args)]
struct Script {
    script_file: String,
}

#[derive(Args)]
struct Raw {
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
struct Wlan {
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
struct Internal {
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

fn main() {
    let cli = Cli::parse();
    let argv = flatten(cli.command);
    match send_and_stream(&argv) {
        Ok(code) => process::exit(code),
        Err(err) => {
            eprintln!("fox: {err}");
            process::exit(1);
        }
    }
}

fn flatten(command: Command) -> Vec<String> {
    match command {
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
            let mut out = vec!["decrypt".to_string(), decrypt.password];
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

fn send_and_stream(argv: &[String]) -> io::Result<i32> {
    let request = encode_argv(argv)?;
    let mut input = open_input_fifo()?;
    input.write_all(&request)?;
    drop(input);

    let mut output = OpenOptions::new().read(true).open(FOX_OUTPUT_FILE)?;
    stream_response_and_extract_exit(&mut output)
}

fn encode_argv(argv: &[String]) -> io::Result<Vec<u8>> {
    let mut request = Vec::new();
    for arg in argv {
        if arg.as_bytes().contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "arguments cannot contain NUL bytes",
            ));
        }
        request.extend_from_slice(arg.as_bytes());
        request.push(0);
    }
    request.push(0);
    if request.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "encoded command is too long",
        ));
    }
    Ok(request)
}

fn open_input_fifo() -> io::Result<std::fs::File> {
    loop {
        match OpenOptions::new().write(true).open(FOX_INPUT_FILE) {
            Ok(file) => return Ok(file),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                eprintln!(
                    "OrangeFox does not appear to be running. Waiting for OrangeFox to start . . ."
                );
                eprintln!("Press CTRL + C to quit.");
                break;
            }
            Err(err) => return Err(err),
        }
    }

    loop {
        match OpenOptions::new().write(true).open(FOX_INPUT_FILE) {
            Ok(file) => return Ok(file),
            Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    }
}

fn stream_response_and_extract_exit(output: &mut std::fs::File) -> io::Result<i32> {
    let mut pending = Vec::new();
    let mut chunk = [0_u8; 512];
    loop {
        let read = output.read(&mut chunk)?;
        if read == 0 {
            io::stdout().write_all(&pending)?;
            return Ok(1);
        }

        pending.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_sentinel(&pending) {
            io::stdout().write_all(&pending[..pos])?;
            let mut exit_line = pending[pos + EXIT_SENTINEL.len()..].to_vec();
            output.read_to_end(&mut exit_line)?;
            return Ok(parse_exit_code(&exit_line));
        }

        let keep = EXIT_SENTINEL.len().saturating_sub(1);
        if pending.len() > keep {
            let write_len = pending.len() - keep;
            io::stdout().write_all(&pending[..write_len])?;
            pending.drain(..write_len);
        }
    }
}

fn parse_exit_code(line: &[u8]) -> i32 {
    let code_end = line.iter().position(|&b| b == b'\n').unwrap_or(line.len());
    let code_text = std::str::from_utf8(&line[..code_end]).unwrap_or("1");
    code_text.trim().parse().unwrap_or(1)
}

fn find_sentinel(buf: &[u8]) -> Option<usize> {
    buf.windows(EXIT_SENTINEL.len())
        .position(|window| window == EXIT_SENTINEL)
}
