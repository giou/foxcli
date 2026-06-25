# Fox RPC FIFO Protocol

`fox` talks to the OrangeFox recovery engine through the Fox RPC v1 protocol.
The FIFO pair is only a transport; the same request object is also accepted by
the WLAN webserver job API.

## Paths

- Input FIFO: `/system/bin/foxin`
- Output FIFO: `/system/bin/foxout`
- Cancel FIFO: `/system/bin/foxcancel`

## Request

Write one UTF-8 JSON object to `foxin`, then close it:

```json
{"v":1,"id":"optional-client-id","op":"status","args":{}}
```

Fields:

- `v`: protocol version, currently `1`.
- `id`: optional client id, echoed on events when present.
- `op`: operation name.
- `args`: operation-specific object; missing args are treated as `{}`.

## Response

Read newline-delimited JSON events from `foxout` until the final `result` event:

```json
{"event":"log","text":"Mounting /data\n"}
{"event":"progress","phase":"overall","percent":42,"label":"Data","current_bytes":104857600,"total_bytes":524288000,"bytes_per_second":20971520,"eta_seconds":20}
{"event":"result","code":0}
```

Events:

- `log`: ordinary recovery text.
- `data`: structured payload, such as a screencap base64 object.
- `progress`: progress update. v1 uses `phase:"overall"` for whole-command
  progress and `phase:"item"` for the current partition/file. Backup/restore
  may also include `label`, `current_bytes`, `total_bytes`,
  `bytes_per_second`, `eta_seconds`, `current_files`, `total_files`,
  `size_text`, and `file_text`.
- `error`: structured error.
- `result`: final result and process exit code.

`foxcli` renders `log` events like ordinary stdout for human commands. `fox rpc`
is the raw bridge mode: it reads a request JSON object from stdin and writes the
NDJSON event stream to stdout unchanged.

## Cancellation

`foxcancel` remains out-of-band. Write any byte to request cancellation of a
cancellable running operation. The command still completes by emitting a final
`result` event.
