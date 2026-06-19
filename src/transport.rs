use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::thread;
use std::time::Duration;

const FOX_INPUT_FILE: &str = "/system/bin/foxin";
const FOX_OUTPUT_FILE: &str = "/system/bin/foxout";
const EXIT_SENTINEL: &[u8] = b"\x04EXIT ";

pub struct EngineResponse {
    pub exit_code: i32,
    pub stdout: String,
}

pub fn send_command(argv: &[String], quiet: bool) -> io::Result<EngineResponse> {
    let request = encode_argv(argv)?;
    let mut input = open_input_fifo()?;
    input.write_all(&request)?;
    drop(input);

    let mut output = OpenOptions::new().read(true).open(FOX_OUTPUT_FILE)?;
    read_response(&mut output, quiet)
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

fn read_response(output: &mut std::fs::File, quiet: bool) -> io::Result<EngineResponse> {
    let mut pending = Vec::new();
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 512];

    loop {
        let read = output.read(&mut chunk)?;
        if read == 0 {
            write_plain(&pending, quiet)?;
            captured.extend_from_slice(&pending);
            return Ok(EngineResponse {
                exit_code: 1,
                stdout: String::from_utf8_lossy(&captured).into_owned(),
            });
        }

        pending.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_sentinel(&pending) {
            write_plain(&pending[..pos], quiet)?;
            captured.extend_from_slice(&pending[..pos]);
            let mut exit_line = pending[pos + EXIT_SENTINEL.len()..].to_vec();
            output.read_to_end(&mut exit_line)?;
            return Ok(EngineResponse {
                exit_code: parse_exit_code(&exit_line),
                stdout: String::from_utf8_lossy(&captured).into_owned(),
            });
        }

        let keep = EXIT_SENTINEL.len().saturating_sub(1);
        if pending.len() > keep {
            let write_len = pending.len() - keep;
            write_plain(&pending[..write_len], quiet)?;
            captured.extend_from_slice(&pending[..write_len]);
            pending.drain(..write_len);
        }
    }
}

fn write_plain(bytes: &[u8], quiet: bool) -> io::Result<()> {
    if !quiet {
        io::stdout().write_all(bytes)?;
    }
    Ok(())
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
