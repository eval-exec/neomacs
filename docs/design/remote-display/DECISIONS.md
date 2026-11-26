# DECISIONS: remote-display campaign (issue #587)

Append-only decision log, written by the orchestrator and by synthesis agents (MISSION.md, "Where things
go"). Each entry is dated. Entries are never edited after the fact; a later entry may supersede an earlier
one by number. Every GNU behaviour cited is a source contract and is **UNATTESTED** unless an entry says
otherwise.

---

## 2026-11-24 — Design synthesis: DESIGN.md v1 and the milestone ledger

- **Author:** design-synthesis agent.
- **Outputs:** `DESIGN.md` (authoritative architecture) and
  `MILESTONES.md` (16 milestones, all `todo`).
- **Inputs:** proposals `p1`–`p4`, judgments `j1`–`j3`, `ARCHITECTURE-MAP.md`, `MISSION.md`,
  `AGENTS.md`, `RUNBOOK.md`, parts of `u8`–`u10`, and the source lines listed in DESIGN §18.
  Tree `72cba7f0`; GNU `a360712c9d272d950d8d8255ef74570f7e90b7d9`.
- **Nothing under `the repository` was edited; no cargo build, check or test was run.**

### How the winner was determined

The judges named three different winners: J1 P2 (8.0), J2 P1 (8), J3 P3 (8.1). Mean scores: P1 7.27, P2 7.17,
P4 7.13, P3 6.70. Pairwise majorities: P2 beats P1 and P3 and ties P4; P4 beats P1 and P3 and ties P2.
Borda: P2 5.5, P4 5.5, P1 4, P3 3.

**Decision S1:** the skeleton is **P2's protocol**, delivered in **P1's order**, with P3's frontier and feel
machinery and P4's layering, detach table and outward authority.
**Rationale:** P2 and P4 co-lead the pairwise and Borda counts; P2 leads P4 on the mean, and all three
judges found P4's protocol-core claim (frontend hit lookup) to be a misreading of the code (J1 V1, J2 §2
row 6, J3 A.2 item 7). J1's first required change to P2 is P1's order, which is J2's entire case. Each
judge's own synthesis recipe converges on this composite.

### Architecture decisions

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Lisp, buffers, processes, LSP, layout, shaping, font selection and image decoding stay on the evaluator host; the view is local | Layout re-enters Lisp mid-walk and serves synchronous queries; GNU's own remote mode (X forwarding) keeps fonts, images and layout in the Lisp process | all four; map L1–L8, C102 |
| D2 | One protocol (NDP/1) for every window-system display, the in-process GUI included; transports differ only in payload representation (`Payload::{Shared, Encoded}`) | One mechanism, no remote/local branch; removes shared memory, reply `Sender`s, `Instant`s and `Drop`-carried meaning from the seam | P2 D2; J1 L1 |
| D3 | The cut: sealed presentation down, display input vocabulary up, policy facts inside presentations; eight corrections (neo-term, image decode, pointer resolution, desktop facts, `x-hide-tip`/`image-cache-size`, capabilities, the mailbox, the input bridge) | The presentation is already the immutable publication boundary and serde; materialization reads draw facts only | P2 D3; P4 X1–X7 |
| D4 | Pointer → buffer-position resolution runs on the evaluator; frames carry no text positions and no evaluator GC roots; P3's `ScrollProjection`/`CurrentGeometry` targets handle predicted scroll | GNU builds posns in the Lisp process; the evaluator already retains presentations and accepts hits only on the active one; the projection is a pure function of evaluator-exported data plus an offset; avoids one fact with two representations | P2, P3; J1 V1–V4; J2 row 6 |
| D5 | bincode 2 (already in `Cargo.lock`) for every message body; one compact row representation; `VisualConfig` as one embedded JSON document; deflate through the existing `flate2`, negotiated, never in process | No new dependency to vet; one codec; the effect registry is serde reflection over `VisualConfig` and its leaves churn | P1, P4; J1 change 7; J2 §4.3 |
| D6 | Evolution: frozen handshake records, negotiated new variants, append-only top-level fields, golden fixtures per minor; default build policy accepts same-major skew, `exact-build` optional | Durable sessions plus lockstep would strand sessions at every client upgrade | P2 §4.3; J3 graft 7, B.8 item 4 |
| D7 | A session carries N display channels; each channel is one Lisp terminal; a second `x-open-connection` of the same name opens a second channel; channels of one session share a KBOARD | Keeps `x-open-connection` unconditional (GNU `x_term_init`, A12) without a documented divergence; mirrors `same_x_server` (C45) | new in this synthesis |
| D8 | Remote frontends dial in to a per-daemon display listener (`<server-socket-dir>/<server-name>.display`) under a stable `neo:LABEL`; Lisp opens displays by name; resume runs in the listener, never in Lisp or `server.el` | One listener serves offer and resume; stable names keep frameset/desktop restore working in daemons (`frameset-keep-original-display-p`) | P3, P4; J1 change 2; J3 A.2 item 10 |
| D9 | Remote frames are created by byte-identical `server.el` with `-nowait` | A waiting client's frames are deleted by `server-sentinel` → `server-delete-client`; nowait frames survive (`gnu:lisp/server.el:362-370,421-443,993-1000`) | P2, P3, P4; J1 V5 |
| D10 | Transport loss on a resumable session detaches the terminal (P4's table: `frame-visible-p` nil, redisplay skipped, inactive for the sole-active check, typed errors); grace expiry deletes `noelisp` without injecting an error | `suspend-tty` and `TERMINAL_ACTIVE_P` are the GNU anchors; the tty `-2` path is the deletion template | P4 §5.6; J3 graft 15 |
| D11 | One last-frame rule for every `neo` terminal: pinned (C32, gtk3 oracle); a standalone frontend process ends its session when it has no windows | One Lisp rule; process lifetime is the frontend program's business | P2; J1 Q2; J3 graft 11 |
| D12 | A main-owned registry with one exhaustive enum over terminal kinds; GNU ids, tombstones, record-only liveness, liveness-first teardown, `check_x_display_info` designators, `x-close-connection`; TTY native hosts leave World-carried state | Fixes divergences that exist today with no remote code; discharges `host-effect-projections.md:72-78` | P1 §5.1–5.3; J1 grafts 1–2; J3 A.2 item 8 |
| D13 | Redisplay visits every terminal with per-terminal sinks and a per-frame display context | GNU `redisplay_internal` (C104); today only the selected terminal repaints | all four |
| D14 | The exact input frontier (`consumed_through` + `in_progress`, `Indeterminate`) replaces the completed-prefix checkpoint, landing in M1 before any prediction | The completed prefix stalls below minibuffer keys once every key has a receipt (`DP/input_progress.rs:151`, `command_loop.rs:575-577`) | P3 D6; J1 V7; J2 row 4; J3 A.2 item 5 |
| D15 | Selection set is a FIFO setter; selection get answers locally while the terminal owns the selection, else a request bounded by `x-selection-timeout` | GNU's `x_own_selection` does not wait and `x-get-selection-internal` asks `x_get_local_selection` first; the in-tree setter rule | P3 §5.6; J3 graft 14 (rejects P2's blocking set) |
| D16 | Every Lisp-visible wait is a correlated request inside `wait_reading_process_output` with a Lisp-variable deadline; outcomes converge with `HostResult`; a closed connection makes teardown setters succeed as no-ops | GNU's selection wait; fixes raw `recv_timeout`s; `x_connection_closed` sets `display = 0` | P1 §5.7–5.8; P2 §4.8; P4 §4.9; J1 V10–V11 |
| D17 | Fonts travel by content (SHA-256) with an evaluator-computed raster policy and a typed pending state; the silent emergency fallback is deleted; renderer-owned text is laid out on the Lisp host; the frontend advertises installed fonts' hashes; `--sync-fonts` deployment for headless servers | C102; the renderer draws exact glyph ids from exact font bytes; no second font authority | u5 option A; P1, P2, P4; J1 §4.4; J3 grafts 17, 23 |
| D18 | Images are decoded on the evaluator (`neomacs-media-cpu`); pixels on the bulk lane; encoded bytes only after frontend decoding is sandboxed; `image-cache-size` from a nominal ledger | GNU decodes in the Lisp process; no remote-chosen bytes reach a decoder on the user's machine in v1 | P2, P4; J3 graft 6 |
| D19 | neo-term is re-homed to the evaluator (M15); until then it is a typed same-host capability | The shell must run where the files are; `term.el` precedent | all four |
| D20 | Same-host features are gated by a typed `LinkLocality` the evaluator derives from how it obtained the transport | Names where the resource lives; not a remote branch; disappears per feature when re-homed | P2 §4.3, §7.3 |
| D21 | Daemons are always headless; a native display name spawns `current_exe() --frontend=gpu` over a socketpair. The switch happens only in M13, once spawned frontends exist | GNU C95; also removes this host's startup panic with `DISPLAY` set; J2 forbids a window without a frontend | P2 §5.2; J2 fatal flaw 3 |
| D22 | Startup "window before Lisp" is kept as provisional-window adoption (`Create{adopt_provisional}`) available to any frontend | Keeps the first-window feel without a special case; not Lisp-visible | P3 §6.1; J1 change 8 |
| D23 | Crates: `neomacs-session` (no workspace dependencies, injected clock), `neomacs-display-wire` (DP only), `neomacs-display-host` (evaluator side, CI-run), `neomacs-frontend` (no `neovm-*` in its closure), `neomacs-tty-render`, `neomacs-media-cpu`; the runtime defines `FrontendEndpoint` and depends on neither wire nor session; `check-crate-graph.py` enforces it; `display-crate-layout.md` is amended | Cheapest inner loop; tests that CI actually runs; the layout doc's dependency rules | P4 §11; P3 §11.1; J1 grafts 11, 15, 16; J2 §4.6 |
| D24 | Delivery order: byte-identical seam refactors (M1–M4), GNU multi-terminal parity (M5), link host and end-to-end slice (M6–M7), evaluator hits (M8), services, assets, relay and durability (M9–M12), local-GUI convergence (M13), prediction (M14); neo-term (M15) and KBOARDs (M16) in parallel | Host-verifiable work first; the GPU-only local-GUI rewrite last; nothing CI-only on the path to the first remote frame | J1 change 1; J2 §5; the computed task |
| D25 | Presentation-only prediction generalizes the pixel-scroll grant, self-enables by measured latency, and lands with deterministic exactness gates; coverage and misprediction rates are reported, not gating | Research targets as landing gates are a stuck risk | P3 §5.5, §6.4; J2 §6 item 5 |
| D26 | Subprocess `DISPLAY` never carries a `neo:` name; `browse-url.el` gets a `neo` branch; `neomacs-display-open-uri` exists behind frontend policy | `browse-url` sets `DISPLAY` from the frame in Lisp, bypassing the callproc rule | P1–P4 (callproc); J3 A.3 (browse-url) |
| D27 | Security: ssh stdio by default, no network listener, 0700/0600 plus peer uid, 256-bit rotated tokens never on argv, never `server-generate-key`, bounded decoding with re-seal, hash-verified assets, per-host side-effect policy, never exec on the display host | Trust inversion: the remote side runs arbitrary user Lisp | P2 §8; P4 §8.3; J3 B.6 |
| D28 | Input in flight at a disconnect is replayed exactly once within a budget (5 s / 256 events, with a notice); input typed after the frontend shows "Reconnecting" is not queued | No blind execution and no loss of echoed input | P3 §4.12; P4 §4.11; J3 Part C item 4 |
| D29 | The registry, link host and listener code lives in `neomacs-display-host`, not in the `neomacs` binary crate | Tests in the `neomacs` package other than `daemon_lifecycle` run in no CI job | J2 §1.2, §4.6 |
| D30 | End-to-end acceptance: the daemon's `neomacs--frame-snapshot` text equals the `render_text()` recorded by a separate headless frontend process (`neomacs --frontend=headless`), in root-level `daemon_lifecycle` tests | GPU-free, deterministic, runs in the existing serialized CI job | P1 M6; P2 M5; u10 L3 |

### Documented GNU divergences recorded by this entry

All are source contracts, UNATTESTED; each is reachable only through the new display kind.

1. **Display loss.** The pinned linux-gtk3 oracle aborts the process on any X disconnect
   (`gnu:src/xterm.c:26907-26919`). Neomacs detaches resumable sessions and otherwise follows the
   no-toolkit and tty-hangup path (`gnu:src/xterm.c:26965-26992`, `gnu:src/keyboard.c:8109-8130`). Attestation
   form A9 can never show "matching" on the gtk3 profile.
2. **Subprocess `DISPLAY`** excludes `neo:` names (pgtk precedent, `gnu:src/callproc.c:1879-1895`).
3. **`browse-url.el`** gains a `neo` branch beside GNU's pgtk and Android branches
   (`gnu:lisp/net/browse-url.el:927-951,1073-1087`); the vendored file stops being byte-identical.
4. **Dial-in rendezvous** (u8 N3): Lisp still opens displays by name.
5. **The detached state** (u8 N1), with `frame-visible-p` nil while detached.
6. **`mouse-position`** answered from the last pointer event (existing; GNU queries the server).
7. **The resize-acknowledgement wait** keeps today's 1 s bound until DQ25 is attested (GNU waits inside
   `set-frame-size` up to `x-wait-for-event-timeout`, 0.1 s).

### Decisions deferred to the user (DESIGN §17)

U1 detach-grace default (recommend 86 400 s); U2 `frame-visible-p` while detached (recommend nil);
U3 clipboard-read policy for dialed frontends (recommend `allow`); U4 default display label (recommend short
hostname); U5 `browse-url` on non-local displays opens on the user's machine by default (recommend yes);
U6 version-skew default (recommend `compatible`); U7 input typed while reconnecting (recommend drop with a
bell); U8 shipping the user's own font bytes (`fsType` licensing); U9 approval to install user-space
lavapipe and `libxkbcommon-x11` on this host for GUI tests, or CI-only GUI verification; U10 whether a
detached terminal holding the single-kboard lock releases it (recommend release).

---

## 2026-11-24 — Design revision 2: red-team disposition

- **Author:** design-reviser agent.
- **Outputs:** `DESIGN.md` version 2, whose disposition table is §19 and
  whose revision evidence is §18.2, and `MILESTONES.md` revision 2 (18
  milestones: M0–M16 plus M11b, all `todo`).
- **Inputs:** DESIGN v1, MILESTONES v1, `design/r1-code-truth.md` (R1 F1–F28, Q1–Q5) and
  `design/r2-gaps.md` (R2-01–R2-35, Q1–Q6). Every finding was re-checked against the code at
  `72cba7f0` and the GNU source at `a360712c9d272d950d8d8255ef74570f7e90b7d9` before disposition.
- **Phase rules held.** Nothing under `the repository` was edited. No `cargo build`, `check`, `test` or
  `nextest` was run. The commands run were read-only:
  - `cargo metadata` (no-deps and the Linux resolve graph);
  - `cargo fmt --all --check`, which exited 0;
  - `check-test-targets.py`, which printed `ok: checked 35 crates`;
  - `git`, `rg`, `sed`, `du`, `df` and man pages.

  GNU behaviour cited is a source contract, UNATTESTED. GNU's own `AGENTS.md` prohibits LLM-generated
  contributions to GNU Emacs; the campaign only reads GNU sources as a reference.

### Disposition summary

- **R1.** All 28 findings were confirmed at their cited lines and accepted. F7's per-copy disk cost was
  measured from existing artifacts only. Q1, Q2 and Q4 are answered; Q3 is moot for bincode; Q5 is
  answered in part.
- **R2.** 31 findings were accepted. Four were accepted in part, each with the rejected part named:
  - **R2-16**: a second, designed type universe for nested vocabulary is rejected for v1 (AGENTS.md §8,
    duplicated abstractions). The honest `schema` policy is adopted instead.
  - **R2-19**: shared-memory pixels are deferred until measured.
  - **R2-25**: answering geometry getters from the requested size is rejected, because the window
    manager may change the size.
  - **R2-29**: the same-host proof is deferred to the horizon.

  No finding was rejected outright. R2-01 was treated as a blocker and decided now (D8, D31), before
  M7 fixes the listener path.

### v1 factual claims corrected

1. Production has two `DisplayHost`s, not one. `TtyPopupDisplayHost` is installed for non-daemon `-nw`
   (R1 F1).
2. The seam baseline's snapshot re-lays out and never observes TTY output (R1 F2).
3. No "real frames" scenario corpus existed; mock frames bypass the evaluator (R1 F3).
4. Six runtime-used types were missing from M1 (R1 F4).
5. 9,474 was a listed count, not a pass result (R1 F5).
6. No headless `RenderApp` fixture exists (R1 F6).
7. "One feature set" does not prevent stack copies, and ≈3 GB per copy included incremental artifacts
   (R1 F7).
8. bincode 2.0.1 is locked only through weak features, and its default features add `bincode_derive`
   (R1 F8).
9. Runtime code does read text positions (hit resolution) and window-level facts (R1 F9, F10).
10. bincode has seven serde hazards, not two (R1 F11).
11. The layout engine has 23 `.display_host` accesses in 6 files, not 6 (R1 F20).
12. The KBOARD limitation is not documented anywhere (R1 F17).
13. `host-effect-projections.md:72-78` does not require a registry (R1 F19).
14. DESIGN §3.3 wrongly said subprocesses get the frame's `-env`. GNU's frame `environment` parameter has
    no effect (`gnu:lisp/server.el:1000-1011`), and Neomacs matches that (found while verifying R2-06).
15. The renderer helpers carry Fontconfig and probe code; the image decoder files sit at the renderer root
    and are coupled to `image_cache.rs` (R1 F13, F28).
16. Fan-out counts are 15 and 18, not 16 and 19 (R1 F26).
17. The feel report is not in `neomacs-perf` (R1 F25).

### Amended decisions

| # | Amendment | Finding |
|---|---|---|
| D4 | "no **hit-test** text positions"; window-level facts still cross | R1 F10 |
| D5 | `bincode = { version = "=2.0.1", default-features = false, features = ["std","serde"] }` | R1 F8 |
| D6 | the default skew policy becomes `schema` (same `WireSchemaId`, the digest of the golden corpus); the relay records join the frozen layer | R2-16 |
| D8 | the listener lives at `/tmp/neomacs-<uid>/<server-name>.display`, independent of `XDG_RUNTIME_DIR` and `TMPDIR`, survives logout, and is reached through a probe | R2-01, R2-02 |
| D10 | at detach, the selected frame moves off the terminal and the KBOARD is released by GNU's `delete_kboard` rule | R2-09 |
| D12 | ids come from a monotonic counter; no tombstones | R2-28 |
| D15 | lazy `Own{generation}` ownership (inline ≤ 64 KiB, else converted on request); a local answer only with an exact ownership signal; an ordered `SelectionState` replica | R2-11, R2-12 |
| D16 | two GNU wait disciplines (selection-style runs timers; display-event-style runs no Lisp); request cells cancelled on any non-local exit; `HostReplyId` in `neovm-eval` | R2-08, R1 Q1 |
| D17 | no inventory; per-offer `Have` from a local hash index | R2-17 |
| D18 | pixels content-addressed and persistently cached | R2-18 |
| D24 | delivery order gains M0 (baselines first) and M11b (a GPU presenter over links before durability and convergence) | R1 F5, R2-21 |
| D20 | a relay-less dial-in is `Dialed`; the lifecycle difference is documented | R2-29 |
| D27 | `on-input` defaults for clipboard and open-URI on `Dialed` links; private targets refused; shaders off by default on `Dialed` links; the socket mode set by `chmod` on the path | R2-15, R2-26 |
| D28 (supersedes D28) | replay is deduplicated by the highest *received* serial; input during a stall is queued within the budget; input after the transport is abandoned is dropped | R2-04, R2-05 |
| D30 (supersedes D30) | the e2e criterion compares against the daemon's last **submitted** presentation; snapshot equality at quiescence is a separate test | R1 Q2 |

### New decisions

| # | Decision | Finding |
|---|---|---|
| D31 | Daemon lifetime belongs to the service manager. Auto-start goes through `systemd-run --user` when one is reachable, with `sd_notify` readiness (GNU `src/emacs.c:3434`). A `durability` fact is reported; lingering is the supported configuration on systemd hosts; a relay never starts a second daemon while one answers on the rendezvous | R2-01, R2-02 |
| D32 | The control lane is state synchronization plus a bounded request table. No session is ever closed because a peer is slow; an over-bound request fails that call | R2-10 |
| D33 | Quit edges bypass the frontend's input bound (`QuitKeys` config); `InputProgress{consumed_through}` travels on lane 0 independently of presentations | R2-07 |
| D34 | Two-phase token rotation, implicit takeover by a valid token, one agreed liveness timeout, a frontend incarnation epoch (cold resume after restart), and detach closing the transport | R2-03, R2-04, R2-05 |
| D35 | The remote command is the fixed `neomacsclient --display-relay`; user data travels in the frozen `RelayRequest`; `RelayStatus`, `ProbeReply`, `server.el` errors on stderr and a non-zero exit. ssh retries use BatchMode, `ControlPath=none` and ServerAlive; askpass; a 64 KiB preface scan; one visible `sh -lc` retry on exit 127 | R2-13, R2-14, R2-31 |
| D36 | A relay-started daemon gets `SSH_AUTH_SOCK=/tmp/neomacs-<uid>/agent.sock`, a link every relay re-points on attach and resume | R2-06 |
| D37 | `TtyPopupDisplayHost` becomes the popup and selection half of the primary termcap backing. Class-3 answers are unchanged in M3 and follow the frame's terminal kind from M5 | R1 F1 |
| D38 | Baselines (full suites, TTY screens plus layout snapshots, CI state) are recorded in M0 before any source edit; "passes" means no new failure against them | R1 F2, F5; R2-22 |
| D39 | M11b brings a GPU presenter over links through a temporary frontend-side bridge onto `RenderComms`, deleted in M13 (enforced by a guard test), plus a public headless `RenderApp` and the first `remote_gpu_*` L5 tests | R2-21, R1 F6 |
| D40 | The wire frame id is `LiveDisplayFrameId(NonZeroU64)`, not a colliding `FrameRef` | R1 F23 |

### Plan changes (MILESTONES revision 2)

- **New milestones.**
  - M0, baselines and the TTY seam oracle. It adds a `seam_baseline` module to the `neomacs-tui-tests`
    `tui` binary, whose closure has no `neovm-*` crate, plus `scripts/nextest-baseline.py`. M1 and M3
    now depend on M0.
  - M11b, the GPU presenter over a link. It depends on M4, M6 and M10. M12 and M13 now also depend on
    M11b.
- **Acceptance semantics.** "Passes" means `⟨compare⟩` against the M0 baselines.
- **Conventions.**
  - `RT_FEATURES` derives the runtime's dbus flag from `cargo metadata`, which removes M5's broken
    command (R1 F21).
  - `new_external_packages` checks the lock.
  - The disk budget is at most five standing stack copies (R1 F7).
  - The command prefix must run from `the repository`: cargo fails elsewhere for lack of
    `rust-toolchain.toml`. Observed in this revision.
- **Milestone-level fixes.**
  - M1 and M15 now both change DP (neo-term's types).
  - M2 gains a named 13-scenario evaluator-driven corpus and a 3-OS `protocol-crates` CI job.
  - M4 lists the extra extraction, test-split and feature-forward work (R1 F12–F14).
  - M7's `remote_` count is 13.
  - M8 runs the pane-morph differential in the runtime (R1 F27).
  - M10 adds the materializer to CI (R1 F15).
  - M12's sole-active test is split into a daemon "not refused" test and a registry-level refusal test
    grounded by attestation form A19 (R1 F22).
  - M16's C-g test is scoped to single-kboard state (R2-24).
- **Re-checked here.** The prefix block, the M0 loop (dry run with commands echoed), the selector, the
  lock helper, the nextest flags (0.9.152), the package, feature and test-target names, `fmt`, and
  `check-test-targets`.

### Documented GNU divergences added (UNATTESTED source contracts)

8. A relay-started daemon's `SSH_AUTH_SOCK` names the stable link (D36). Attestation form A20
   documents GNU's stale-socket behaviour.
9. `neo` frames export `NEOMACS_DISPLAY`, and `neomacsclient -c` prefers it (R2-33).
10. Display open is quit-able. GNU's `XOpenDisplay` is not.
11. From M5, class-3 answers follow the terminal kind. For example, `neomacs-surface-available-p` is nil
    under `-nw` (a correction toward GNU's tty behaviour).
12. `x-selection-timeout` now governs the local GUI too (it was a fixed 5 s), bounded in practice by the
    frontend's own 5 s reply bound (R2-32).

### Decisions deferred to the user (changed or new; DESIGN §17)

- U3 recommendation changes from `allow` to **`on-input`**.
- U5 becomes **default on under the `on-input` policy**, with private targets refused.
- U6 changes from `compatible` to **`schema`**.
- U7 becomes **queue during a stall, drop after the transport is abandoned**.
- U10 now has a mechanism: GNU's `delete_kboard`.
- New: **U11**, the CI route for campaign PRs, needed before M4. **U12**, approval of a user-mode sshd
  fixture from nix once its disk cost is measured. **U13**, the `SSH_AUTH_SOCK` link on by default.

## M0 landing (2026-11-24)

- **Two commits, not one.** `817f0a6b` adds `CanonicalScreenDump` (a reusable harness type with its own unit
  tests); `eae48104` adds the seam oracle that uses it. Each is reviewable alone; the first was not built in
  isolation (its files do not reference the second).
- **No PR opened, no push.** The CI route (U11) is undecided, so M0 is `landed` on the local branch only; the
  PR body is staged at `evidence/M0/PR.md`.
- **Unresolved review minors are deferred, not fixed, at landing.** They are listed as M0 follow-ups 1–12 in
  `plan/MILESTONES.md`, which serves as their disposition. The menu-bar premise (follow-up 1) belongs to M5;
  the `.git/index` refusal (follow-up 10) fails closed and is worked around by rebuild + re-provision.
- **Post-commit check uses `GIT_OPTIONAL_LOCKS=0`.** The verification driver
  (`evidence/M0/commit/post-commit.sh`) sets it so its own `git status` does not stale the bins before the
  seam compare (fix-r2 note).

## M1 landing (2026-11-25)

- **D41. `FrontierSnapshot::Indeterminate` carries `reflected_through`** (M1 fix r1, item q; proposed in
  `evidence/M1/fix-r2.md`). It is the completed prefix of the snapshot captured before layout. Serials at or
  below it are `Reflected`; serials above it are `Pending` ("not known to be shown"), so the scroll reconciler
  keeps their offset as before M1, and M14's echo engine drops their predictions. Without the field, a frame
  laid out while Lisp read input would stop acknowledging inputs it already shows, which breaks §6.14's own
  requirement that today's completed prefix stays derivable. DESIGN §6.14 (`:1075`) and §M14 (`:1896`) still
  show `Indeterminate { stream }`; updating that text is the orchestrator's (M1 follow-up 17). Optional before
  M2 freezes NDP/1: a fourth `Disposition` (for example `Unknown`).
- **D42. The neo-term vocabulary is unconditional in DP** until M15 removes it, not behind DP's `neo-term`
  feature. `neovm-eval` names the types in its display-host API and has no feature to forward, and turning
  DP's feature on from the evaluator would enable `FrameGlyph::Terminal` in every evaluator graph. Recorded as
  an M1 amendment in `plan/MILESTONES.md`.
- **One commit, not several.** `80c6c9f8` holds the vocabulary moves, the attribute rule and the frontier
  redesign together: they share use sites (`thread_comm`, the evaluator keyboard, `main.rs`), and no
  intermediate split was built or tested. No PR opened or pushed (U11); the body is staged at
  `evidence/M1/PR.md`.
- **Landing on the verified tree, not a re-run.** The commit's tree is the tree verification round 3 ran on:
  its `git status --porcelain` hash `ea9532d2…` was recorded at the start of `verify-r3-driver.log` and
  re-checked immediately before staging, and no tracked source was newer than the driver. After the commit
  only fmt and check-test-targets were re-run (`evidence/M1/commit/post-commit.log`).
- **Review minors deferred at landing.** They are M1 follow-ups 1–18 in `plan/MILESTONES.md`. Six of them
  (3 in part, 4, 10, 15, 16, 18) were already addressed in fix rounds 1–2; this is a source reading of
  `80c6c9f8`, with the tests named there.

## M3 landing (2026-11-25)

- **One commit, not five PRs.** `267fe940` holds M3a–M3e together. The ledger planned five PRs, but they share
  the slot type, the trait split and the test hosts, and no intermediate split was built or tested. No PR
  opened or pushed (U11). The body is staged at `evidence/M3/PR.md`.
- **Landing on the verified tree, not a re-run.** Just before staging, the working tree's porcelain and diff
  hashes (`73eac676…`, `0bf23356…`) equalled the start and end hashes of independent verification round 2
  (`evidence/M3/verify-r2-driver.log`). Only `crates/` was staged (105 modified, 7 new files). After the commit,
  only check-test-targets, `fmt --all --check` and the bins rebuild plus provision were re-run, with
  `GIT_OPTIONAL_LOCKS=0` (`evidence/M3/commit/post-commit.log`).
- **Scope deviation recorded as an amendment.** The test hosts were split into two impls each instead of going
  through an adapter over `RecordingDisplayHost`. The M3 section of `plan/MILESTONES.md` carries this as
  "Amendment (landing)".
- **Review minors deferred at landing.** They are M3 follow-ups 1–20 in `plan/MILESTONES.md`. Duplicate
  findings were merged (pre-display replay; `pump.rs` optional host). Follow-up 9 (class 5 missing from PR
  Limitations) was fixed in PR.md at landing. The PR now also lists the pre-display replay, the stale
  host-effect doc and the unbound `FaceResolver` as limitations. Most go to M5 by default. The unbound resolver
  goes to M8, device-effect refusals to M9, and the stale design doc to its owner.
- The M0 and M1 rows of the summary table still say "in-progress". This was not changed, because only a
  milestone's owner edits its row. The orchestrator should update them.

## M2 landing (2026-11-25)

- **One commit.** `5a7a7885` holds both new crates, the DP additions (View/Parts, the fallible glyph walk, serde
  for the NS modifier policy), the layout-engine parity corpus, `check-crate-graph.py`, the design-doc charters
  and CI. They share `Cargo.toml`/`Cargo.lock` and the parity test, and no intermediate split was built or
  tested. No PR opened or pushed (U11). The body is staged at `evidence/M2/PR.md`.
- **Landing on the verified tree, not a re-run.** Just before staging, the tracked-diff, porcelain and
  untracked-content hashes (`28b88be0…`, `125bc49d…`, `f4db34b1…`) equalled `fix-r2/final-check.log` and review
  r3 (`evidence/M2/commit/commit.log`). All 52 changed or new paths were staged; nothing was left untracked.
  After the commit, fmt, check-test-targets, the crate graph and the bins build plus provision were re-run with
  `GIT_OPTIONAL_LOCKS=0` (`commit/post-commit.log`). The proptest regression seeds file is committed, as the
  repo does for 14 others.
- **Review minors at landing.** They are M2 follow-ups 1-27 in `plan/MILESTONES.md`, one per finding in the
  landing brief. Ten were already resolved or found not real in fix rounds 1-2 (1, 13-18, 22, 25, 26) and three
  in part (3, 8, 9); this is a source reading of `5a7a7885` against `fix-r{1,2}.md`. PR.md
  Limitations gained the open items it did not list (encoder memo, hit-index oracle, `WIRE_ROOTS`, refusal
  order, untyped link state, proof coverage), and its Verification gained the landing check.
- **Orchestrator items.** M2 follow-up 24 (record the move of M1 follow-ups 5-8, 11, 12, 14 to M6 in the M1 list
  and the M6 section) and follow-up 3 (a keepalive lower bound, which DESIGN §6.19 does not give). The M0 and
  M1 rows of the summary table still say "in-progress"; only the M2 row was changed.

## M4 amendment (2026-11-25, design steward)

The M4 implementer stopped before any product edit (`plan/M4-plan.md`: B1–B6, proposed A1–A6). Each claim was
re-checked by source reading on `5a7a7885`, with no product file edited (`evidence/M4/steward-verify.log`). All six
hold. A1, A2 (with both rulings), A4, A5 and A6 are accepted; A3 is changed; four missed items are added. MILESTONES
M4, M11b and M13 and DESIGN §7.6 and §8.1 carry the result.

- **D43. `tty_input` stays in the runtime until M13 deletes `RenderComms`.** Its production code publishes through
  the runtime's `thread_comm` (`DR/tty_input.rs:24-27`, `unix_reader.rs:20`, `publication.rs:25`) and names no
  `neovm_*`. The guard already classes it as runtime ConnectedComms code (`host_effect_guard_test.rs:80-84`). In
  display-host it would need a normal edge to the runtime, which the charter forbids
  (`display-crate-layout.md:148-151`). DESIGN §7.6 had listed it from a test-only `rg` hit.
- **D44. A test that spans the seam either splits at the protocol vocabulary or moves whole. No test reads another
  crate's private items.**
  - *Split.* Where the two halves meet at a DP value (TTY widths, compositions and fill edge; `FrontendKey`; raw
    bytes), each crate keeps its half. Display-host pins the evaluator's half against one table exported under
    `testing`. M11b and M13 already test the frontend this way, on recorded protocol data.
  - *Move whole.* A test of runtime behaviour over live evaluator output moves to display-host. It uses the
    runtime's public API or its `testing` feature, over a dev-only edge.
  - *Ruling (a).* The backpressure tests use `RenderApp::headless`, which M11b already planned. M4 creates it with
    the minimal surface, and M11b extends it. This keeps one headless mechanism instead of an M4
    `HeadlessRenderApp` beside M11b's entry.
  - *Ruling (b).* The two real-layout tests stay in the runtime on checked-in `FrameDisplayState` JSON. The
    contract checks the value the runtime consumes: `fixture.materialize() == live.materialize()`. If that cannot
    hold, the two tests move whole instead.
  - *Rejected.* Moving `char_width` (a pure static table in `neovm-value`) into a new leaf crate. That would touch
    the bottom crate of the stack, rebuild every copy and need a full core run, all for a test-only need. The
    width fixture panics on any unlisted scalar, and its contract test catches drift.
- **D45. Display-host, not `neomacs`, forwards `neovm-core/desktop-font-settings` on Linux (A3 changed).**
  - *GNU (source contract, UNATTESTED).* GNU provides `dynamic-setting` from `syms_of_xsettings`
    (`src/xsettings.c:1417`). It provides `system-font-setting` only under its build guards (`:1409`). `xsettings.o`
    is window-system terminal code: it links beside `xterm.o` (`configure.ac:7227`) and in the PGTK objects
    (`:3911`). `lisp/loadup.el:301-302` loads `dynamic-setting.el` only when the feature is present.
  - *Here.* The evaluator side of a `neo` terminal is display-host, and the desktop observer stays a frontend
    producer. `neomacs` links display-host, so `features` in the shipped binary are unchanged.
  - *The guard test* moves to display-host's lib tests. Those run in M4's combined compare and in CI's display
    job. `neomacs` lib tests run in neither (R1 F16), and a test moved there would be absent from the compare.
- **D46. M4 compares the runtime baseline against one combined nextest invocation (A4).**
  - `nextest-baseline.py` parses one log with one Summary, and it counts a passing baseline test that is absent
    from that log as a regression.
  - The combined run (`-p runtime -p tty-render -p display-host --lib --features neovm-core/dbus-vendored`)
    puts every moved test into one log under its new id. Cargo accepts the flag when a selected package depends
    on neovm-core (`evidence/M4/probe.log`).
  - The runtime-only run stays as the proof that the runtime's build closure has no `neovm-*` crate. No tooling
    change is needed.
- **D47. `TextFillEdge` is tty-render's own enum (A5), which display-host maps from `PosnObjectExtentMode`
  exhaustively.**
  - Putting the evaluator's enum into DP would rebuild every stack copy, all for one rasterizer parameter.
  - The mode is an immutable process `OnceLock` over `NEOMACS_POSN_OBJECT_EXTENT`. No production code sets that
    variable, and only `neovm-display` tests force the override. Reading the mode when `TtyRif` is constructed
    is therefore byte-identical.
- **Added to the M4 scope (the block missed these).**
  - `reclaim_child_signal()` (`RT/bootstrap.rs:375`, in the `webview` arm) becomes a hook that the embedder
    injects. The handler it reinstalls, and that handler's latches, live in `neovm-system`
    (`os_signal/mod.rs:628`). Without WPE on this host, that arm compiles only in CI.
  - The runtime keeps `default = ["neo-term"]`: the M0 baseline holds 63 `terminal::` tests.
  - The guard's Font arm (`:168`) and Layout arm (`:200-204`) both name `DR/redisplay.rs`. M4 therefore runs
    neovm-core's `architecture_` tests (M0: 181 pass, 2 fail).
  - The font materializer gets a record before its first edit, because M0 has no baseline for it.
  - `menu_test.rs` splits the same way as `glyph_atlas_test.rs` (A6).

## M4 landing (2026-11-25)

- **One commit.** `c4e7100d` (`refactor(display): cut the frontend runtime off the Lisp machine`) holds both new
  crates, the runtime and renderer edits, the materializer extraction, the guard, CI and design-doc changes. They
  share `Cargo.toml`/`Cargo.lock`, the moved-test maps and the crate-graph rules, and no intermediate split was
  built or tested (as for M2). No PR opened or pushed (U11); the body is `evidence/M4/PR.md`. No AI trailer.
- **Landing on the verified tree, not a re-run.** Before staging, porcelain was 102 and `git diff HEAD | sha256sum`
  was `47ffa05cd493`, the fix-r3 tree. There were no untracked files; `git add -A` staged all 103 paths and the
  staged diff hashed the same. After the commit, `git diff 5a7a7885 HEAD` hashes to `47ffa05cd493`, the tree is
  clean, and graph, test targets and fmt exit 0 (`evidence/M4/commit/post-commit.log`). Suites were not re-run:
  the code is byte-identical to what `fix-r2/summary.log` and `fix-r3/summary.log` verified.
- **Review minors at landing.** They are M4 follow-ups 1-15 in `plan/MILESTONES.md`. Duplicate findings were merged
  (the `for_unacquired` proof x3, bless mode x2, `PosnObjectExtent` coverage x2, `neovm-value` comment x2).
  Two are marked resolved by source reading: the closed-table `should_panic` pins exist since fix round 2
  (`testing_test.rs:14,26`), and MILESTONES' Status already named the fix-r2/fix-r3 indices. The reclaim-hook
  `None` arm (follow-up 9) is the one finding that cites a MISSION rule; it is assigned to M13 with the hook,
  and PR.md Limitations lists it.
