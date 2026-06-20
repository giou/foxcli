# foxcli build-system patches

`foxcli` is a `recovery: true` binary, so every Rust crate it links must be
`recovery_available`. Most of these crates are **not** recovery-available in a
freshly cloned `external/rust/android-crates-io`, so these patches add the flag.
Without them, a clean-tree build fails to resolve the recovery variants of
`libclap` / `libserde_json` and their dependencies.

Each patch touches two files per crate:
- `Android.bp` — the generated Soong module (makes the current tree build).
- `cargo_embargo.json` — the generator config (so the flag survives the next
  `cargo_embargo` re-import).

## Crates patched

`clap` chain (argument parsing):

| Patch | Crate | Reason |
|-------|-------|--------|
| `clap-recovery-available.patch`         | `clap`         | argument parsing |
| `clap_builder-recovery-available.patch` | `clap_builder` | `clap` dependency |
| `clap_lex-recovery-available.patch`     | `clap_lex`     | `clap_builder` dependency |
| `anstyle-recovery-available.patch`      | `anstyle`      | `clap_builder` dependency |
| `strsim-recovery-available.patch`       | `strsim`       | `clap_builder` dependency (suggestions) |

`serde_json` chain (JSON output for `fox --json`):

| Patch | Crate | Reason |
|-------|-------|--------|
| `serde_json-recovery-available.patch` | `serde_json` | JSON output |
| `itoa-recovery-available.patch`       | `itoa`   | `serde_json` dependency |
| `ryu-recovery-available.patch`        | `ryu`    | `serde_json` dependency |
| `memchr-recovery-available.patch`     | `memchr` | `serde_json` dependency |

Not patched, and why:
`libc` (Ctrl+C / SIGINT handling for cancellable operations):

| Patch | Crate | Reason |
|-------|-------|--------|
| `libc-recovery-available.patch` | `libc` | SIGINT handler + cancel FIFO write |

Not patched, and why:
- `serde` — already ships `recovery_available: true` upstream (AOSP).
- `serde_derive`, `clap_derive` — `rust_proc_macro` crates run on the host at
  build time, so they don't need a recovery variant.

## Applying (in the sync-sources script)

```sh
ROOT=$PWD
CRATES="$ROOT/external/rust/android-crates-io"
for p in "$ROOT"/external/foxcli/patches/*-recovery-available.patch; do
    git -C "$CRATES" apply --reverse --check "$p" 2>/dev/null && continue  # already applied
    git -C "$CRATES" apply "$p"
done
```
