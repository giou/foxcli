# foxcli (`fox`)

The modern OrangeFox command-line front-end. `fox` is a small Rust client that
runs inside recovery and talks to the running OrangeFox GUI engine, replacing
the legacy ORS-based `twrp` tool.

Unlike `twrp`, `fox` speaks Fox RPC v1 over a dedicated FIFO pair: one JSON
request to `foxin`, newline-delimited JSON events from `foxout`. It also offers
a `--json` mode for scripting.

## Usage

Run `fox` with no arguments (or `fox status`) for a device status page and the
command list:

```
fox                       # device status + command list
fox status                # device status only
fox mount /system /data   # mount one or more partitions
fox unmount /system       # (alias: umount)
fox flash rom.zip [--no-verify]
fox backup System Data --name mybackup [--compress] [--no-digest]
fox restore --name mybackup [System Data] [--no-digest-check]
fox decrypt                # show pattern help
fox decrypt <password> [--user <id>]
fox sideload               # start `adb sideload` mode
fox wlan <enable|disable|list|connect|saved|connect-saved|forget|info|test>
fox ota check [--fox-id=<id>] [--type=alpha,beta,stable]
fox reboot [target]
fox internal <get|set|go> ...
fox rpc < request.json      # bridge raw Fox RPC JSON to NDJSON events
fox ors <file>             # run an OpenRecoveryScript file
fox ors-cmd <command...>   # run one raw OpenRecoveryScript command
...
```

Global flags (valid before or after the subcommand):

- `--json` — emit a single machine-readable JSON object instead of plain text.
- `--no-color` — disable ANSI colors in plain output.

## Architecture

```
 fox (this crate)                       OrangeFox GUI engine
 ─────────────────                      ────────────────────
 command ──JSON──▶ FOX_INPUT_FILE ───▶  gui.cpp listener
                  (/system/bin/foxin)   parses Fox RPC, Fox_Fifo::Set_Args()
                                        dispatches the "foxcmd" GUI action
 stdout ◀─NDJSON── FOX_OUTPUT_FILE ◀──  Fox_Fifo::Run_Command() (gui_print)
                  (/system/bin/foxout)  then writes a final result event
```

- **Request**: a UTF-8 JSON object such as
  `{"v":1,"op":"status","args":{}}`.
- **Response**: newline-delimited JSON events. `log` events are mirrored to
  stdout for normal commands; the final `result` event supplies the process exit
  code.

The engine side lives in `bootable/recovery/fox_fifo/fox_fifo.{cpp,hpp}`, wired up in
`gui/gui.cpp` and `gui/action.cpp`.

See [`docs/fox-fifo-protocol.md`](docs/fox-fifo-protocol.md) for the full
`foxin`/`foxout` transport specification.

## Building

This crate is built by the Android build system (Soong), not Cargo — there is
no `Cargo.toml`. From the Android tree root:

```
m foxcli      # produces the `fox` binary (recovery variant)
```

`Android.bp` declares a `rust_binary` named `foxcli` (stem `fox`) that links
`libclap` for argument parsing and `libserde` / `libserde_json` (with the
`libserde_derive` proc-macro) for `--json` output. It is a `recovery: true`
binary, so every dependency must be `recovery_available`. In a freshly cloned
`external/rust/android-crates-io`, the `clap` chain (`clap`, `clap_builder`,
`clap_lex`, `anstyle`, `strsim`) and the `serde_json` chain (`serde_json`,
`itoa`, `ryu`, `memchr`) are **not** recovery-available — only `serde` is. The
[`patches/`](patches/) directory holds the patches that add the flag (to both
`Android.bp` and `cargo_embargo.json`); wire them into the source-sync script.

## License

GPL-3.0. See the headers in the engine sources and `Android.bp`.
