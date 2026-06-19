# foxcli (`fox`)

The modern OrangeFox command-line front-end. `fox` is a small Rust client that
runs inside recovery and talks to the running OrangeFox GUI engine, replacing
the legacy ORS-based `twrp` tool.

Unlike `twrp`, `fox` sends a NUL-delimited `argv` vector over a dedicated FIFO
pair, so spaces in passwords and paths survive intact and the engine can report
a real process exit code. It also offers a `--json` mode for scripting.

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
fox reboot [target]
fox internal <get|set|go> ...
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
 argv ──encode──▶ FOX_INPUT_FILE  ───▶  gui.cpp listener
                 (/system/bin/foxin)    parses argv, Fox_CLI::Set_Args()
                                        dispatches the "foxcmd" GUI action
 stdout ◀──parse── FOX_OUTPUT_FILE ◀──  Fox_CLI::Run_Command() (gui_print)
                 (/system/bin/foxout)   then writes "\x04EXIT <code>\n"
```

- **Request**: each argument is written as raw bytes followed by a `NUL`, with a
  trailing empty `NUL` terminator. Arguments may not contain `NUL`, and the
  whole encoded request must be ≤ 4096 bytes.
- **Response**: the engine streams command output, then emits the sentinel
  `\x04EXIT <code>\n`. The client mirrors output to its stdout live (unless it
  is capturing for `--json` or pretty-printing) and exits with `<code>`.

The engine side lives in `bootable/recovery/fox_cli.{cpp,hpp}`, wired up in
`gui/gui.cpp` and `gui/action.cpp`.

## Building

This crate is built by the Android build system (Soong), not Cargo — there is
no `Cargo.toml`. From the Android tree root:

```
m foxcli      # produces the `fox` binary (recovery variant)
```

`Android.bp` declares a `rust_binary` named `foxcli` (stem `fox`) that links
`libclap` for argument parsing and `libserde` / `libserde_json` (with the
`libserde_derive` proc-macro) for `--json` output. It is a `recovery: true`
binary, so every dependency must be `recovery_available`. `serde_json` and its
leaf deps (`itoa`, `ryu`, `memchr`) were not recovery-available upstream; that
flag was added to both their `Android.bp` and `cargo_embargo.json` under
`external/rust/android-crates-io/crates/`.

## License

GPL-3.0. See the headers in the engine sources and `Android.bp`.
