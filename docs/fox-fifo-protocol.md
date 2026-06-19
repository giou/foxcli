# Fox FIFO protocol

This document specifies the local IPC protocol between the `fox` CLI client and
the running OrangeFox recovery engine.

The protocol is intentionally small: one request is sent as an argv vector over
`foxin`, and one response stream is read from `foxout` until an exit-code
sentinel is received.

## Endpoints

The recovery GUI owns both FIFOs:

- Input FIFO: `/system/bin/foxin`
- Output FIFO: `/system/bin/foxout`

The symbolic constants are defined engine-side in `orscmd/orscmd.h`:

- `FOX_INPUT_FILE`
- `FOX_OUTPUT_FILE`
- `FOX_EXIT_SENTINEL`

The current sentinel byte sequence is:

```text
\x04EXIT 
```

That is ASCII EOT (`0x04`), then `EXIT`, then one space.

## Lifecycle

1. The recovery GUI starts and calls `setup_fox_command()`.
2. The GUI removes stale `foxin`/`foxout` FIFO nodes.
3. The GUI creates:
   - `foxin` with mode `0660`
   - `foxout` with mode `0666`
4. The GUI opens `foxin` as non-blocking read-only and watches it from the GUI
   event loop.
5. A client opens `foxin` for writing, writes exactly one encoded request, then
   closes it.
6. The GUI reads the request, opens `foxout` for writing, and routes `gui_print`
   output to `foxout`.
7. The GUI dispatches the `foxcmd` action, which calls `Fox_CLI::Run_Command()`
   on the recovery action thread.
8. The engine writes normal command output to `foxout`, then writes the exit
   sentinel plus numeric exit code.
9. The GUI closes `foxout`, clears the mirrored output target, and recreates the
   FIFO pair for the next request.

The protocol is single-command-at-a-time. It is not a multiplexed transport.

## Request format

Requests are NUL-delimited argv vectors.

Each argument is written as raw bytes followed by `NUL` (`0x00`). A final empty
field terminates the vector.

For argv:

```text
["backup", "boot", "data", "--storage", "microsd"]
```

the request bytes are:

```text
backup\0boot\0data\0--storage\0microsd\0\0
```

Rules:

- Arguments are byte strings interpreted as UTF-8 by the current Rust client.
- Arguments must not contain `NUL`.
- Spaces are preserved because they are not delimiters.
- The final empty `NUL` field is required.
- The current engine reads at most 4096 bytes for a request.
- The current Rust client rejects encoded requests larger than 4096 bytes.

If the GUI reads an empty argv vector, it closes and recreates the FIFO pair
without dispatching a command.

## Response format

The response is a byte stream:

```text
<command stdout/stderr-like text>\x04EXIT <code>\n
```

The command output is whatever the engine emits through `gui_print()` while the
command runs. It may contain arbitrary text blocks, including structured blocks
such as `FOX_LIST_BEGIN`/`FOX_LIST_END` and `FOX_STATUS_BEGIN`/`FOX_STATUS_END`.

The end of response is marked by:

```text
\x04EXIT <code>\n
```

where `<code>` is a decimal shell-style exit code:

- `0` means success.
- non-zero means failure.
- `2` is commonly used for command-line usage errors.

The current client scans the stream for the sentinel byte sequence `\x04EXIT `,
captures all preceding bytes as command output, parses the following line as an
integer exit code, and exits with that code.

## Busy behavior

If recovery is already busy (`tw_busy != 0`) when a request arrives, the engine
does not dispatch the command. It writes:

```text
Failed, operation in progress
\x04EXIT 1
```

then closes and recreates the FIFO pair.

Clients should treat this like any other command failure.

## Client responsibilities

A conforming client should:

1. Encode argv with NUL delimiters and a trailing empty NUL field.
2. Reject arguments containing NUL.
3. Keep requests within the 4096-byte engine read limit.
4. Open `/system/bin/foxin` for writing.
5. Write the full request and close `foxin`.
6. Open `/system/bin/foxout` for reading.
7. Stream or capture bytes until the sentinel `\x04EXIT ` is found.
8. Parse the following line as the exit code.
9. Exit with that code.

The Rust client retries opening `foxin` if it does not exist yet, printing a
message that OrangeFox is not yet running.

## Engine responsibilities

A conforming engine should:

1. Own FIFO creation and cleanup.
2. Read exactly one request per FIFO lifecycle.
3. Parse NUL-delimited argv until the first empty field.
4. Reject or ignore empty argv.
5. Open `foxout` before dispatching the command.
6. Route command output to `foxout`.
7. Dispatch the command on the recovery action thread.
8. Always write `\x04EXIT <code>\n` before closing `foxout`.
9. Recreate the FIFO pair after command completion, unless recovery is exiting
   or the GUI has moved into a finished state.

## Concurrency model

Only one command may be active. There is no request ID, session ID, or framing
inside `foxout`; therefore multiple clients writing concurrently are undefined.

External callers should serialize invocations.

## Structured output blocks

The FIFO protocol only frames request and response transport. Higher-level
command output can define its own text blocks inside the response stream.

Current examples include:

- `FOX_STATUS_BEGIN` / `FOX_STATUS_END` for `fox status`
- `FOX_LIST_BEGIN` / `FOX_LIST_END` for list-like command output

Those blocks are command-level formats, not transport-level frames.

## Compatibility notes

- The FIFO paths are part of the protocol contract for the current recovery
  image.
- The sentinel must remain unlikely to appear in normal command output. Changing
  it requires updating all clients.
- The 4096-byte request limit comes from the current engine buffer and should be
  treated as a hard compatibility limit.
- The protocol preserves spaces in paths and passwords, unlike the legacy
  ORS-style string command interface.
