# Headless daemon and clients

The daemon implementation uses Unix process and socket APIs; its lifecycle
regressions currently verify Linux. Neomacs can keep its Lisp evaluator running
without opening a display or using the launching terminal:

```sh
neomacs -Q --daemon=development
neomacsclient -s development -e '(list (daemonp) noninteractive)'
neomacsclient -s development -e '(kill-emacs)'
```

`--daemon[=NAME]` and `--bg-daemon[=NAME]` start in the background. The launching
process succeeds only after initialization, command-line actions and the normal
Lisp `server-start` have completed. Startup failure returns a nonzero status;
background startup waits without a deadline, as in GNU Emacs.
`--fg-daemon[=NAME]` stays in the foreground and retains its standard streams,
which is useful under a supervisor or when debugging startup.

Daemon mode is **not batch mode**: `noninteractive` is nil, normal init files and
hooks run unless disabled with options such as `-Q`, and timers, subprocess
filters and repeated client evaluations continue to work. `(daemonp)` returns
the supplied name, or t for an unnamed daemon. `daemon-initialized` retains GNU's
one-shot, after-init contract; it is normally called by `startup.el`, not by user
configuration. `kill-emacs` runs the usual exit hooks and removes the server
socket. `(kill-emacs nil t)` closes owned processes and re-execs the daemon in
place: same PID, and the daemon stays attached to the process that started it.
Unlike GNU Emacs, a background daemon is restarted as a **foreground** one
rather than re-daemonizing, so the PID is preserved instead of changing.
SIGINT, SIGTERM and SIGHUP also run `kill-emacs` with
the signal number as the exit status (2, 15 and 1 respectively, matching GNU's
`maybe_fatal_sig` set), including when the daemon is idle or waiting in
synchronous `call-process` or `call-process-region`.
Exit hooks run on the evaluator thread; host cleanup restores attached client
terminals and closes owned listeners and subprocesses. Host shutdown removes a
Unix listener's filesystem node only if it still matches the identity captured
at bind; an exit hook's replacement endpoint is left alone. Optional
`NEOVM_AOT_PGO` exit-time cache persistence is skipped in daemon mode: its
external compiler must not delay mandatory cleanup or supervisor termination.
Ordinary GUI, TTY and batch persistence is unchanged.

On Unix, child creation retains exit status until the owning waiter reaps it.
Inherited explicit `SIGCHLD=SIG_IGN` and `SA_NOCLDWAIT` are normalized before
spawning (and before a daemon fork); compatible SIGCHLD handlers are preserved.
Native libraries must not enable auto-reap or reap editor-owned children during
their lifetime. Observed loss of ownership (`ECHILD` or auto-reap policy during
cleanup) denies numeric child signalling rather than guessing a new owner.

A shutdown request owns the exit hooks before invoking them. Further TERM/HUP
signals or nested `kill-emacs` calls during those hooks do not interrupt the
remaining hooks or replace that request: an explicit restart already in progress
still restarts; a signal which enters shutdown first exits with its signal status.
An unhandled batch error uses the same boundary: it reports its diagnostic,
runs hooks once and retains status 255 even if a hook requests another exit.
This first-entry policy is deliberate; GNU Emacs can recursively run exit hooks
when a signal arrives during its batch-error shutdown.
Batch startup preserves inherited ignored TERM/HUP dispositions, as GNU Emacs
and `nohup` require; interactive daemon startup captures them for orderly exit.

## Running the lifecycle regressions from source

With the normal Linux build prerequisites and Rust toolchain installed, run from
the repository root:

```sh
cargo nextest run -p neomacs --test daemon_lifecycle
```

Nothing has to be prepared first, and nothing outside the checkout is required.
The build generates the Lisp inputs a bootstrap loads from source — GNU's
`admin/charsets` awk recipes produce `lisp/international/cp51932.el` and
`eucjp-ms.el`, and `admin/unidata`'s produce `charscript.el` and `emoji-zwj.el` —
the way GNU's own `make` generates them. The suite's fixture then provisions the
bootstrap runtime image beside the debug editor binary on first use: it copies
the editor to its `neomacs-temacs` role name, dumps the image from Lisp sources,
smoke-tests the result, and reuses it for every later case, regenerating it only
when the editor binary is newer. The image is shared through an advisory lock,
so concurrent test processes serialize the one bootstrap instead of repeating
it. The whole target runs serially (a `.config/nextest.toml` override), because
every case carries wall-clock deadlines against real daemon and client
processes. Failures are errors, not skipped tests. This is source/bootstrap
verification, not the optimized, fully byte-compiled release runtime produced by
`cargo xtask fresh-build --release`.

A *final* runtime image sitting beside the debug binaries wins over the
bootstrap image at load time; if one is older than the editor binary the fixture
fails with that path in the message rather than letting the daemon run stale
code. Remove it or use a clean `CARGO_TARGET_DIR`.

The native gate does not require a GNU installation. To additionally exercise a
GNU client, explicitly select it:

```sh
NEOMACS_GNU_EMACSCLIENT=/path/to/emacsclient \
  cargo nextest run -p neomacs --test daemon_lifecycle
```

A selected oracle must pass; it is not silently skipped if missing or failing.
The selected integration case checks GNU protocol evaluation against the matching
Neomacs daemon: persistent state, error survival, same-PID restart with fresh Lisp
state, exit status 13 and socket removal. It also applies the alternate-editor
argv and fallback-before-TTY/stdin assertions to GNU. CI installs the pinned GNU
reference, verifies its `emacsclient` executable and selects that exact path before
running the lifecycle command. Without the selector, no GNU client is looked up or
spawned.

A nonempty `-a EDITOR` or `ALTERNATE_EDITOR` runs that executable with its fixed
arguments followed by the original filename argv; explicit `-a` overrides the
environment. Like GNU's client, the editor string uses ASCII-space-delimited
tokens with double quotes recognized only at the start of a token. It is not
shell code: single quotes, backslashes, variable expansions and operators are
literal. Double-quote an executable path or fixed argument containing spaces;
filenames retain their existing argv boundaries without shell interpolation.

`CARGO_TARGET_DIR` selects both the binaries and the image. To exercise clean
build and runtime state, use a fresh checkout and an empty target directory, for
example:

```sh
CARGO_TARGET_DIR=target/daemon-lifecycle \
  cargo nextest run -p neomacs --test daemon_lifecycle
```

The build generates only the ordinary bootstrap inputs in the checkout and build
artifacts in the selected target; it does not install Neomacs or use your init
files. `CARGO_BUILD_JOBS` can bound compilation concurrency without changing test
scheduling.

## Automatic local startup

An empty alternate editor enables automatic startup when no server is available:

```sh
neomacsclient -s development -a '' -e '(emacs-pid)'
```

`ALTERNATE_EDITOR=''` provides the same default when `-a` is omitted.
`EMACS_SOCKET_NAME` supplies the socket selection when `-s` is omitted. Socket
names and absolute socket paths are passed to the matching `neomacs` executable
next to the client. Automatic startup loads normal init files; it does not
silently add `-Q`.

Concurrent clients serialize startup for the selected local endpoint and then
submit their original requests over the ordinary Emacs server protocol. Startup
waits without a deadline by default. `-w SECONDS` / `--timeout SECONDS` bounds
only server replies, after startup and request submission; zero means unlimited.

The Neomacs-specific `--startup-timeout SECONDS` optionally bounds automatic
startup waiting, including the endpoint lock and a full listener backlog. Zero
(or omission) means unlimited. Expiry fails only that client, submits no request
and never replays it later. It does not kill an initializing daemon or undo init
side effects. A later client can wait for and use the same daemon.

Automatic startup transfers the endpoint lock to the matching background
launcher before abandoning its own copy. The launcher owns readiness and exact
child failure/reaping until `daemon-initialized`, even if the initiating client
times out or dies. An early-bound socket is not readiness. Neither the lock
capability nor readiness descriptors are inherited by Lisp subprocesses.
A mode-0600 `.startup-lock` file remains next to the socket to avoid lock
unlink/recreation races. It is not a readiness marker and need not be removed
when stopping or restarting a daemon. The socket directory must be owned by the
current user and inaccessible to other users; symlinked or otherwise unsafe
startup locks are rejected.

## Current limitations

- Headless daemons support ordinary evaluation and client TTY frames. Attaching
  a native graphical frame is not implemented: it requires a separately hosted
  GUI event loop. Graphical frame requests fail rather than creating an invisible
  stand-in window. Start ordinary GUI Neomacs for graphical editing.
- Automatic startup is for local Unix sockets. TCP clients continue to use the
  existing server-file/authentication path, but do not automatically start a
  local daemon for a missing or unreachable TCP endpoint.
- A client startup-wait budget does not constrain init execution. An init which
  never finishes keeps its background launcher and startup lock alive; explicitly
  stop the daemon when cancellation of init is
  intended. Normal `kill-emacs` performs orderly cleanup.
- Interrupted synchronous calls kill and reap their immediate child; shell
  descendants are not tracked independently. Integer/no-wait destinations
  deliberately detach their child and do not wait for its completion at exit.
- Daemon mode is not available on Windows. Ordinary GUI, TTY and batch startup
  remain separate paths.
