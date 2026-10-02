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
which is useful under a supervisor
or when debugging startup.

Daemon mode is **not batch mode**: `noninteractive` is nil, normal init files and
hooks run unless disabled with options such as `-Q`, and timers, subprocess
filters and repeated client evaluations continue to work. `(daemonp)` returns
the supplied name, or t for an unnamed daemon. `daemon-initialized` retains GNU's
one-shot, after-init contract; it is normally called by `startup.el`, not by user
configuration. `kill-emacs` runs the usual exit hooks and removes the server
socket. `(kill-emacs nil t)` closes owned processes and re-execs the daemon with
the original arguments and PID. SIGTERM and SIGHUP also run `kill-emacs` with
the signal number as the exit status (15 and 1 respectively), including when the
daemon is idle or waiting in synchronous `call-process` or `call-process-region`.
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
cargo xtask test-daemon-lifecycle
```

This command prepares early generated Lisp inputs using the existing bootstrap
pipeline, compiles the default-feature debug test target and matching editor/client,
copies that editor to the native `neomacs-temacs` role, and creates and smoke-tests
its bootstrap image. Using that matching native image, it also regenerates
`lisp/term/neo-win.elc`: deferred GUI registration loads this layer even for a
display-free daemon, and repeatedly expanding its source macros would delay
startup before the user init file. The smoke loads this
compiled terminal layer too. It then runs the entire `daemon_lifecycle` target serially,
with the tests' existing deadlines and disposable HOME/XDG data. Failures are
errors, not skipped tests. The existing test-suite CI runs this same Linux command.
This is source/bootstrap verification, not the optimized, fully byte-compiled
release runtime produced by `cargo xtask fresh-build --release`.

The native gate does not require a GNU installation. To additionally exercise a
GNU client, explicitly select it:

```sh
NEOMACS_GNU_EMACSCLIENT=/path/to/emacsclient cargo xtask test-daemon-lifecycle
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

`CARGO_TARGET_DIR` selects both the binaries and image. To exercise clean build
and runtime state, use a fresh checkout (without generated Lisp/bytecode) and an
empty target directory, for example:

```sh
CARGO_TARGET_DIR=target/daemon-lifecycle cargo xtask test-daemon-lifecycle
```

The command generates the ordinary bootstrap inputs and GUI terminal bytecode in the checkout and
build artifacts in the selected target; it does not install Neomacs or use your
init files. A conflicting final image in that debug directory is rejected rather
than silently used; choose a clean target directory. `CARGO_BUILD_JOBS` can bound
compilation concurrency without changing test scheduling.

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

- On Linux a display-free daemon can later attach native frames to one explicitly
  selected Wayland socket. Lisp `make-frame` and native/GNU client `-c -d SOCKET`
  use the original evaluator and an OS-main-owned native loop. Deleting the last
  graphical frame retains that connection and evaluator for recreation. Unlike
  GNU Emacs, explicit `delete-terminal` on this retained graphical connection is
  rejected, even with FORCE, before hooks or frame/terminal changes: independent
  connection retirement/reconnection is not supported. Ordinary frame deletion,
  TTY terminal deletion and daemon shutdown remain available. Explicit
  X11 and multiple independent display connections are rejected. Native attach
  and frame readiness have a 15-second budget. If cancellation interrupts a
  synchronous Wayland registry/configure wait, the exact owned connection is
  closed and cannot be reused after a successful native-loop construction;
  the daemon evaluator remains available, and restart establishes a fresh loop.
  Ordinary GPU-start and evaluator/font/admission errors do not discard a healthy
  native loop. Foreign driver, font and loader calls are not claimed to finish
  within a deadline; cancelled foreign workers require bounded process exit
  without library finalizers.
- Automatic startup is for local Unix sockets. TCP clients continue to use the
  existing server-file/authentication path, but do not automatically start a
  local daemon for a missing or unreachable TCP endpoint.
- A client startup-wait budget does not constrain init execution or native GUI
  attachment. An init which never finishes keeps its background launcher and
  startup lock alive; explicitly stop the daemon when cancellation of init is
  intended. Normal `kill-emacs` performs orderly cleanup.
- Interrupted synchronous calls kill and reap their immediate child; shell
  descendants are not tracked independently. Integer/no-wait destinations
  deliberately detach their child and do not wait for its completion at exit.
- Daemon mode is not available on Windows. Ordinary GUI, TTY and batch startup
  remain separate paths.

## Strict in-process compositor and graphical-client acceptance

This separate, opt-in Linux command prepares the same matching debug/bootstrap
runtime as the native lifecycle command, then runs real mapped-frame and native/
GNU graphical-client assertions. The ordinary lifecycle command never downloads
or requires EWM. Missing selected fixtures or GNU oracles are errors.

Prepare the GPL-3.0-or-later EWM fixture in an **absent disposable directory**:

```sh
CARGO_BUILD_JOBS=2 python3 scripts/test-daemon-gui.py --prepare-ewm "$TMPDIR/ewm-acceptance"
NEOMACS_EWM_MODULE="$TMPDIR/ewm-acceptance/target/debug/libewm_core.so" \
NEOMACS_GNU_EMACSCLIENT=/usr/bin/emacsclient \
CARGO_TARGET_DIR=target/daemon-gui cargo xtask test-daemon-gui
```

EWM is fetched from `https://codeberg.org/ezemtsov/ewm.git`, exactly revision
`d5bf1e0e8c6b3e7423b88c64db5c199442b7fdb3`, with its locked dependencies. The
checked-in `test/daemon-gui/ewm-headless.patch` is a disclosed test adapter, not
a vendored production dependency: it exposes upstream's HeadlessBackend fixture,
production State/socket/event queues, and native committed-buffer instrumentation.
Stock `ewm-start` opens DRM/libseat and is deliberately never called. This is not
full EWM desktop Lisp compatibility. `--prepare-ewm DIR --reuse` verifies the
complete tracked source against the exact pin plus adapter and reports the
existing module hash without rebuilding; preserve its original build provenance.

Prerequisites: Bubblewrap with user/PID/network namespaces, GNU emacsclient, a
DRM **render node** (default `/dev/dri/renderD128`, selected by
`NEOMACS_TEST_RENDER_NODE`), Vulkan/Wayland runtime libraries, and EWM's native
build libraries (pkg-config: libinput, libseat, gbm, egl, libudev, libdisplay-info,
wayland-server/client, xkbcommon, gio-2.0). Rust must meet both projects' MSRV.
`VK_DRIVER_FILES` may select an ICD. The sandbox hides personal HOME and `/run`,
uses a short private socket path, and binds only the selected render node, never
a DRM card, input device, VT, seat or live display socket.

Assertions start with DISPLAY/WAYLAND_DISPLAY/WAYLAND_SOCKET absent, then Lisp
loads and starts actual in-process EWM. They require native mapped counts
1→2→1→0→1, failed-display recovery, retained module debug/capture state through GC,
evaluator/buffer identity, server selection/hooks, native and GNU `-c -n` and
waiting `-c -d` ownership, and a client staying connected after its first frame
is deleted while its second frame remains owned. The final frame is recreated
after all client frames are deleted. Exit-zero or Lisp visibility alone is never
graphical success. The runner prints exact module/editor hashes and assertions,
reaps every owned client and sandbox wrapper, and removes its disposable fixture.
This gate does not replace the ordinary native lifecycle gate, cross-platform
tests, full GUI/VM/JIT/release matrices or independent code review.
