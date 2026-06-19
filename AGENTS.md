# AGENTS.md — foxcli

Guidance for AI agents and contributors working in this crate. Read this before
editing. See `README.md` for user-facing usage and the wire protocol.

## What this is

`foxcli` is the Rust client binary `fox` that runs inside OrangeFox recovery and
talks to the GUI engine over a FIFO pair. It only *formats* requests/responses —
all real work (mount, flash, decrypt, …) happens engine-side in
`bootable/recovery/fox_cli.{cpp,hpp}`. Keep the two ends consistent.

## Module map (`src/`)

- `main.rs` — entry point and dispatch. Decides JSON vs plain, whether to stream
  or capture output, and routes each command through `run_engine_command`.
- `args.rs` — the clap CLI definition (`Cli`, `Command`, subcommand structs) and
  the `Command::into_engine_argv` mapping that produces the engine argv.
- `transport.rs` — the FIFO protocol: NUL-delimited argv out, streamed output
  back, terminated by the `\x04EXIT <code>\n` sentinel.
- `status.rs` — parses the `FOX_STATUS_BEGIN/END` key=value block from
  `fox status` into a typed `StatusSnapshot`.
- `output.rs` — all rendering (plain + colored, and the hand-rolled JSON), plus
  the `COMMANDS` help catalog.
- `patterns.rs` — the `fox decrypt` pattern help text.

## Build & test

- Built by Soong, **not Cargo** — there is no `Cargo.toml`. Build with `m foxcli`
  from the Android tree root. You cannot `cargo build` here.
- `Android.bp` is the source of truth for dependencies. Adding a crate means
  adding it to `rustlibs` *and* it must exist in the tree as a **recovery
  variant** rlib. Most of `external/rust/crates` is absent — verify before
  reaching for a dependency.
- There is `#[cfg(test)]` test code (e.g. the help-catalog drift guard in
  `args.rs`). It runs under Cargo/`rust_test` but is not currently wired into a
  Soong `rust_test` module.

## Conventions & invariants — don't break these

1. **New dependencies must be recovery-capable.** This is a `recovery: true`
   binary, so every `rustlib` it links must have `recovery_available: true`.
   JSON uses `libserde` + `libserde_json` (via `libserde_derive`); enabling
   `serde_json` required adding `recovery_available` to it and its leaf deps
   (`itoa`, `ryu`, `memchr`) — done in both their generated `Android.bp` *and*
   their `cargo_embargo.json` (the latter so the flag survives crate re-import).
   Before adding another crate, check/patch its recovery availability the same
   way, and keep the binary lean.

2. **The command list has two halves that must agree.** `args.rs`'s `Command`
   enum (parsing) and `output.rs`'s `COMMANDS` catalog (help/JSON listing). The
   test `every_command_is_in_help_catalog` in `args.rs` enforces this — if you
   add a `Command` variant, the exhaustive `match` there will fail to compile
   until you update both the test list and `COMMANDS`.

3. **Plain-vs-streamed output is derived from the typed command.** Add the rule
   to `Command::renders_plain` (and the relevant subcommand helper), not to
   string matching on argv. Read-only queries that should be captured and
   pretty-printed return `true`.

4. **The pattern encoding is owned by the engine.** `src/patterns.rs` only
   documents it; the real algorithm is `GeneratePassphrase` in
   `gui/patternpassword.cpp` (`char = dot_index + '1'`). If that changes, update
   the help and the worked examples together.

5. **Engine protocol is shared.** `transport.rs` constants
   (`FOX_INPUT_FILE`, `FOX_OUTPUT_FILE`, the `\x04EXIT ` sentinel, the 4096-byte
   request cap) mirror `fox_cli.cpp` / `gui/gui.cpp`. Change one side, change the
   other.

6. **JSON is a contract.** `--json` emits exactly one object on stdout, produced
   by `#[derive(Serialize)]` wrapper structs in `output.rs` (`ErrorJson`,
   `CommandJson`, `DecryptHelpJson`, `StatusJson`). If you add fields, prefer
   additive changes; don't reorder or rename existing keys without a reason
   (note `CommandInfo.usage` is renamed to `command` via `#[serde(rename)]`). The
   status object is just `StatusSnapshot` serialized directly, so its typed
   fields *are* the schema — bools serialize as real booleans automatically.

## Gotchas

- stdout is dual-purpose: in streaming mode the engine's bytes go straight to
  the terminal; in JSON/plain-render mode they are captured first. Don't `print!`
  stray text that would corrupt `--json` output.
- Colors auto-disable when not a TTY, when `NO_COLOR` is set, or under `--json`.
  Route all colored text through `Renderer::paint`.
- The status start page tolerates a non-zero engine exit (it warns rather than
  failing); keep that resilience if you touch `start_page`.
