# Neomacs MCP

Neomacs can expose the running editor to a local agent through the
[Model Context Protocol](https://modelcontextprotocol.io) (MCP).  Agents
such as Claude Code, Codex or Hermes connect to it like any other stdio MCP
server and can then inspect buffers and evaluate Emacs Lisp in the editor
you are using.

Nothing starts by default.  Loading the library does not open a socket;
`neomacs-mcp-start` does.

```elisp
(require 'neomacs-mcp)
(neomacs-mcp-start (expand-file-name "neomacs/mcp" (or (getenv "XDG_RUNTIME_DIR") "/tmp")))
;; Later:
(neomacs-mcp-stop)
```

The socket's directory must be owned by you and not accessible to other
users; `neomacs-mcp-start` checks this with `server-ensure-safe-dir`, the
same check `server-start` uses, and creates the directory if needed.  An
existing file at the socket path is refused rather than replaced.  Keep the
path short: Unix socket paths are limited to about 100 bytes.

Build the relay with `cargo build --release -p neomacs-mcp` (release packages
do not include it yet) and configure the MCP client to launch it:

```sh
neomacs-mcp --socket /run/user/1000/neomacs/mcp
```

For example, in a client that uses the common `mcpServers` JSON format:

```json
{"mcpServers": {"neomacs": {"command": "neomacs-mcp",
                            "args": ["--socket", "/run/user/1000/neomacs/mcp"]}}}
```

The relay copies bytes between its stdin/stdout and the socket.  It does not
start an editor, guess a socket, parse MCP or retry.  When the client closes
the relay, the editor keeps running.

## Tools

| Tool | Arguments | Purpose |
| --- | --- | --- |
| `neomacs_identity` | none | `instance`, `pid`, `runtime`, `serverName`, `endpointGeneration` |
| `neomacs_eval` | `instance`, `code` | Evaluate Lisp; return the printed value |
| `neomacs_buffer_list` | `instance`, `offset`, `limit` (1-32) | Buffer names and metadata, one page at a time |
| `neomacs_buffer_read` | `instance`, `name`, `start`, `maxChars` (1-4096), optional `expectedTick` | Text from a buffer |

Every tool except `neomacs_identity` requires the `instance` string returned
by `neomacs_identity`.  It is different for every editor process, so a client
cannot silently act on a different editor after a restart.  It is not a
secret or a credential.

`neomacs_eval` reads all forms in `code` as one `progn`, evaluates them with
lexical binding and returns the result printed with `prin1`.  A printed value
over 64 KiB is reported as an error after evaluation.  Errors are returned as
tool results with `isError` set.  In every error case, including the response
errors described under limits, effects of the evaluation are not undone.

`neomacs_buffer_list` and `neomacs_buffer_read` only read.  They report
`name`, `tick` (`buffer-modified-tick`), `sizeChars`, `point`, `mode`,
`modified` and `readOnly`, never file names.  Reads use widened, 1-based,
end-exclusive character positions and preserve the current buffer, point and
narrowing.  Results are capped at 32 KiB of encoded JSON; a longer read is
shortened and reports `truncated` and `nextStart`.  Pagination offsets count
entries of `buffer-list`, which can change between calls.  Minibuffers and
buffers whose name or file name looks like `authinfo`, `netrc` or
`password-store` are skipped by these two tools.  That is a lexical filter,
not a sandbox.

## Full access and security

With the default settings, an MCP client connected to the socket has **full
access** to the editor: `neomacs_eval` runs arbitrary Lisp with the same
privileges as your user account.  It can read and write any buffer or file
you can, start processes, use the network, change your configuration or exit
the editor.  There is no sandbox, rollback or per-call confirmation.  This is
the same capability `emacsclient --eval` gives after `server-start`, and
what MCP integrations in other editors typically provide: an agent that can
drive the editor the way you do.

To turn it off:

```elisp
(setq neomacs-mcp-full-access nil)
```

When `neomacs-mcp-full-access` is nil, `neomacs_eval` is neither listed nor
callable: a call to it gets error `-32602` (Unknown tool).  The remaining
built-in tools only read buffers.  The option is
checked on every request, so changing it affects connected clients
immediately.  Tools registered by other packages are not affected.

The transport boundary:

- The endpoint listens only on a Unix domain socket.  There is no TCP or other
  network listener.
- The socket's directory must belong to you and be inaccessible to other users,
  so only processes running as you can connect.
- Nothing is started by loading the library, by opening a file or by
  directory-local or project settings; only an explicit `neomacs-mcp-start`
  call creates the socket.

Any process that can connect to the socket already runs as your user, and can
therefore run arbitrary code in your account without MCP (and in the editor
through `emacsclient` if the server is running).  The endpoint does not add a
capability for a malicious local process.  What it changes is who you hand
control to: an agent you connect can do anything you can.  Connect only agents
you would let type into your editor, and set `neomacs-mcp-full-access` to nil
when read-only access is enough.

## GNU Emacs

`neomacs-mcp.el` is plain Emacs Lisp: it uses `make-network-process`, the
native JSON functions and `server-ensure-safe-dir`, and does not depend on
Neomacs internals, so it also runs on GNU Emacs; the test suite below runs
on both.  The `neomacs-mcp` relay is an independent program and
works with any editor that serves the socket.  GNU Emacs users who want this
outside Neomacs currently need to load the file themselves.

## Protocol

Messages are UTF-8 JSON-RPC 2.0, one per line, as in the MCP stdio transport.
The endpoint supports:

- **2026-07-28:** no handshake; each request carries
  `params._meta["io.modelcontextprotocol/protocolVersion"]` and
  `["io.modelcontextprotocol/clientCapabilities"]`.  `server/discover` is
  implemented.  An unsupported version receives error `-32022` listing the
  supported versions.
- **2025-11-25 and 2025-06-18:** `initialize`, then
  `notifications/initialized`.  A supported version offer is echoed; any other
  offer receives 2025-11-25.

Only `ping`, `server/discover`, `tools/list` and `tools/call` are
implemented; resources, prompts and subscriptions are not advertised.
`notifications/cancelled` drops a queued request and suppresses the reply to a
running one; it cannot interrupt Lisp that is already running.

## Scheduling and limits

Process filters only split and queue messages; tools never run inside a
filter.  A timer runs at most one queued request at a time, and only while
`(input-pending-p)` is nil, so typing takes priority over agent requests.
Continuous input can therefore delay requests indefinitely.  A tool runs
synchronously in the editor's command loop: long-running Lisp blocks the
editor just as it would from `M-:`.

Limits: 8 connections, 64 queued requests (16 per connection), 128 KiB of
buffered input per connection (any incomplete message plus newly received
data) and 128 KiB per response line.  A request ID longer than 1024 bytes when
encoded as JSON is not echoed: the request gets one `-32600` error with a null
ID.  A response over the limit, or one that cannot be encoded as JSON (for
example text containing raw bytes), is replaced by a fixed-size `-32603` error
for the same request; the connection stays open.  Exceeding any other limit,
or reusing an outstanding request ID, closes that connection.  A response send
that does not complete within `neomacs-mcp-send-timeout` seconds closes the
connection.

## Adding tools

```elisp
(neomacs-mcp-register-tool
 "my_tool" "Describe what it does."
 (json-parse-string
  "{\"type\": \"object\", \"properties\": {\"name\": {\"type\": \"string\"}},
    \"required\": [\"name\"], \"additionalProperties\": false}")
 (lambda (arguments) (format "Hello %s" (gethash "name" arguments)))
 (json-parse-string "{\"readOnlyHint\": true}"))
```

The handler receives the arguments as a hash table and returns a JSON value;
a string becomes the text content.  Arguments are checked for required
fields, primitive types and unknown properties before the handler runs.
Annotations are hints for the client, not permissions.

## Tests

```sh
# Lisp endpoint, with Neomacs or GNU Emacs:
neomacs -Q --batch -L lisp -l test/neomacs/neomacs-mcp-test.el \
  -f ert-run-tests-batch-and-exit

# Relay:
cargo nextest run -p neomacs-mcp

# Both, through a real relay process:
cargo build -p neomacs-mcp
NEOMACS_MCP_RELAY=target/debug/neomacs-mcp neomacs -Q --batch -L lisp \
  -l test/neomacs/neomacs-mcp-test.el -f ert-run-tests-batch-and-exit
```
