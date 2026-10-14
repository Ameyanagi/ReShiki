# Limits and errors

## Budgets

Default operation budgets, as reported by `info`.

| Budget | Value | Unit |
| --- | --- | --- |
| `concurrency` | 2 | count |
| `export_png_pixels` | 16000000 | pixels |
| `history_depth` | 20 | count |
| `idempotency_receipts` | 32 | count |
| `idle_ttl_seconds` | 3600 | seconds |
| `max_cdx_base64` | 22369624 | bytes |
| `max_documents` | 16 | count |
| `max_idempotency_key_bytes` | 128 | bytes |
| `max_ids` | 5000 | count |
| `max_inspect_objects` | 5000 | count |
| `max_objects` | 100000 | count |
| `max_output_bytes` | 16777216 | bytes |
| `max_request_bytes` | 25165824 | bytes |
| `max_session_picture_bytes` | 134217728 | bytes |
| `max_session_weight` | 2000000 | count |
| `max_text_bytes` | 16777216 | bytes |
| `op_deadline_seconds` | 120 | seconds |
| `queue` | 8 | count |
| `render.default_height` | 1000 | pixels |
| `render.default_width` | 1600 | pixels |
| `render.max_pixels` | 16000000 | pixels |
| `render.max_side` | 8192 | pixels |
| `render.min_side` | 1 | pixels |

## Error codes

| Code | Meaning |
| --- | --- |
| `unknown_tool` | No tool has this name; MCP reports it as a JSON-RPC error. |
| `invalid_arguments` | The arguments break the tool's schema or one of its rules. |
| `unknown_document` | The document handle is unknown, closed, expired or not the caller's. |
| `unknown_object` | An object ID is not in the document. |
| `stale` | The document's revision is not the one the call was based on; nothing changed. |
| `busy` | The server is stopping, the request ID is in flight or the queue is full. |
| `budget` | The call would exceed a budget. |
| `timeout` | The call passed its deadline before it committed any change. |
| `rejected` | Validation or the document's edit policy refused the change. |
| `unsupported` | Reserved; no current tool returns it. |
| `access_denied` | The file is outside the granted folders, has an extension the tool does not accept, or the operating system denied access. |
| `failed` | The operation failed, for example on unreadable chemistry, a missing or existing file, an I/O error or an internal error. |
| `cancelled` | The caller cancelled the request; no response is sent. |

## File-error reasons

A file error's message starts with one of these reasons, followed by a colon.

| Reason | Error code | Meaning |
| --- | --- | --- |
| `path_invalid` | `invalid_arguments` | The path is not a plain absolute path to an allowed file name: for example a relative path, '..', a file: URI, or Windows UNC or device syntax. |
| `path_not_granted` | `access_denied` | The path is not inside a folder granted for this access; the message lists the granted folders. |
| `path_escapes_root` | `access_denied` | Resolving the path left the granted folder, or the operating system refused it. |
| `not_a_regular_file` | `failed` | The path names a folder, FIFO, device or other special file. |
| `file_too_large` | `budget` | The file is larger than the size limit. |
| `file_exists` | `failed` | The file already exists and overwrite is not true. |
| `file_not_found` | `failed` | The file or a parent folder does not exist, or a parent is not a folder. |
| `extension_not_allowed` | `access_denied` | The extension is not one the tool reads or writes; the message lists the allowed ones. |
| `os_denied` | `access_denied` | The operating system refused access. |
| `no_clobber_unsupported` | `failed` | The folder's file system has no hard links, so a new file cannot be created without risking an overwrite; overwrite: true replaces instead. |
| `io_error` | `failed` | Any other I/O error. |

## File formats

Extensions accepted by `file_open` and `file_save`; a dash means unsupported.

| Extension | Import format | Export format |
| --- | --- | --- |
| `.rsk` | `reshiki` | `reshiki` |
| `.reshiki` | `reshiki` | — |
| `.moruno` | `reshiki` | — |
| `.mol` | `mol` | `mol` |
| `.rxn` | `rxn` | — |
| `.rsmi` | `rsmi` | — |
| `.cdxml` | `cdxml` | `cdxml` |
| `.cdx` | `cdx` | — |
| `.smi` | `smiles` | `smiles` |
| `.smiles` | `smiles` | `smiles` |
| `.inchi` | `inchi` | `inchi` |
| `.svg` | — | `svg` |
| `.pdf` | — | `pdf` |
| `.png` | — | `png` |

## Command-line exit codes

Reported by `reshiki --cli help --json`. See setup.md for MCP usage and exit status.

| Exit | Meaning |
| --- | --- |
| 0 | success |
| 1 | the operation failed, or reading the input or writing the output failed |
| 2 | usage error |
| 75 | the memory ceiling was reached (see RESHIKI_AGENT_HEAP_MB) |

## Heap ceiling

The same environment variable controls the `--cli` and `--mcp` processes.

| Variable | Meaning | Unit | Minimum | Maximum | Default | Invalid exit |
| --- | --- | --- | --- | --- | --- | --- |
| `RESHIKI_AGENT_HEAP_MB` | memory ceiling | MiB | 256 | 16384 | 2048 | 2 |
