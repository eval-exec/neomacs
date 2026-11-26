# DESIGN: remote display for Neomacs (issue #587)

**Status:** authoritative architecture for the remote-editing campaign, **version 2**, 2026-11-24.
**Supersedes** the four proposals (`p1`…`p4`) as the reference. They remain the detailed rationale for
the parts adopted from them.

**Version 2 is the red-team revision.** Every finding of `design/r1-code-truth.md` (R1 F1–F28 and its
open questions) and `design/r2-gaps.md` (R2-01–R2-35 and its open questions) was re-checked against the
code by the reviser and then folded in or rejected with a reason. §19 is the disposition table. §18.2
lists the revision's commands. New and amended decisions are D4–D6, D8, D10, D15–D18, D20, D27 and
D28–D40, and they are logged in `DECISIONS.md`. Version 1 text that a finding showed to be false was
replaced, not annotated.

- **Tree:** `the repository`, branch `feat/remote-display`, HEAD `72cba7f0ced20c8f751938ae0ff946dcee13e4ad`
  (`git rev-parse HEAD`, worktree clean). **GNU:** `the GNU Emacs checkout (gnu:)` at
  `a360712c9d272d950d8d8255ef74570f7e90b7d9` (`git rev-parse HEAD`, checked this session).
- **Inputs read in full:** `MISSION.md`, `AGENTS.md`, `RUNBOOK.md`, `env.sh`,
  `understanding/ARCHITECTURE-MAP.md`, proposals `p1`–`p4`, judgments `j1`–`j3`. Read in part:
  `u10-test-infra.md` (§2–§5), `u9-prior-art.md` (§2.3.1 and the recommendation table), `u8-gnu-contract.md`
  (Appendix A). Neomacs and GNU sources listed in §18.
- **Phase rules held.** Nothing under `the repository` was edited. No `cargo build`, `check`, `test` or
  `nextest` was run. Commands run are in §18.
- **Every GNU behaviour in this document is a source contract and is UNATTESTED.** No oracle binary
  exists on this host and no GNU form was run (AGENTS.md §1). Each Lisp-visible change carries an
  attestation form in §12.4.
- **GNU's own `AGENTS.md`** (`gnu:AGENTS.md`) states a policy against LLM-generated
  contributions and planning for GNU Emacs. This campaign reads GNU sources only as the behavioural
  reference for Neomacs. It never patches GNU Emacs or files anything upstream there.
- **The milestone ledger is `MILESTONES.md`.** The decision log entry is in
  `DECISIONS.md`.

## Evidence tags and path abbreviations

| Tag | Meaning |
|---|---|
| **[V]** | I opened the cited lines in this session. |
| **[J1·V] [J2·V] [J3·V]** | Verified at the cited lines by that judge; I did not re-open them. |
| **[P1]…[P4]** | A proposal's claim with its own citation; not re-opened by me. |
| **[M]** | ARCHITECTURE-MAP (or a u-report through it), with its citation. |
| **[RB]** | Runbook evidence: commands the orchestrator ran (`campaign:evidence/runbook/`). |
| **[G·V]** | GNU source contract at the pinned commit; I opened those lines. UNATTESTED. |
| **[G]** | GNU source contract read by a proposal, judge or u-report. UNATTESTED. |
| **[I]** | My inference or design decision. Everything prescriptive here is [I] unless tagged otherwise. |
| **[E]** | Estimate (u1's size model or arithmetic). Not measured. |
| **[Q]** | Open question (§17). |

| Abbrev | Path (relative to `` unless absolute) |
|---|---|
| `DP/` | `crates/neomacs-display-protocol/src/` |
| `DR/` | `crates/neomacs-display-runtime/src/` |
| `RT/` | `crates/neomacs-display-runtime/src/render_thread/` |
| `RW/` | `crates/neomacs-renderer-wgpu/src/` |
| `LE/` | `crates/neomacs-layout-engine/src/` |
| `NM/` | `crates/neomacs/src/` |
| `EV/` | `crates/neovm-eval/src/emacs_core/runtime/eval/` |
| `ND/` | `crates/neovm-display/src/emacs_core/display/` |
| `NW/` | `crates/neovm-display/src/window/` |
| `NS/` | `crates/neovm-system/src/emacs_core/system/` |
| `gnu:` | `gnu:` |

---

## 0. Summary

**The architecture in one paragraph [I].** Lisp, buffers, files, processes, language servers, layout,
shaping, font selection and image decoding run on the **evaluator host**. Every window-system display is a
GNU terminal of type `neo`. That holds for the local window, a second local display and a laptop across
ssh. Each such terminal is a **display channel** inside a **session**, and every session speaks one
versioned, typed protocol: the **Neomacs Display Protocol, NDP/1**, carried by a service-agnostic
**session layer**. The local GUI uses an in-process transport with zero-copy `Arc` payloads. A spawned
local frontend uses an inherited socketpair. A remote frontend uses ssh stdio spliced into a per-daemon
**display listener**. Downlink, the unit is the sealed post-layout presentation (`FrameDisplayState`),
sent as row-slot deltas against what the receiver holds. Uplink, it is the display input vocabulary with
serials and display time, converted to Emacs events on the evaluator. **Text positions under the pointer
are resolved by the evaluator** against the presentation the user saw, as GNU builds posns in the Lisp
process. The frontend owns windows, GPU, compositor, animation, hover, menus once open, IME pre-edit,
clipboard, input devices and desktop facts. It may predict only within policy facts that the evaluator
certified for the presentation it shows. Remote displays dial in under a stable label `neo:LABEL`. They
reach a per-daemon display listener in an environment-independent rendezvous directory through a relay
that ssh starts with a fixed command line, and Lisp opens them by name. Frames are created through
byte-identical `server.el` with `-nowait`. Losing the transport **detaches** the terminal (a typed,
`suspend-tty`-shaped state). It is resumed below Lisp. Whether a daemon outlives the user's final logout
on its host is a fact the daemon reports, never an assumption (D31).

**Decisions** (each is argued in §1–§13 and logged in `DECISIONS.md`):

| # | Decision | Primary source |
|---|---|---|
| D1 | Lisp, layout, fonts, image decode, processes and LSP stay on the evaluator host; the view is local | all four; GNU X-forwarding model |
| D2 | One protocol (NDP/1) for **every** window-system display, the in-process GUI included; only payload representation differs by transport | P2 D2; J1 |
| D3 | The cut is the sealed presentation downlink plus the input vocabulary uplink, with eight corrections to today's seam (§5.3) | P2 D3 + P4 X1–X7 |
| D4 | Pointer → buffer-position resolution moves to the evaluator. No **hit-test** text positions cross the wire. Window-level facts (`window_start`, `buffer_size`, the cursor's `charpos`) still do (R1 F10). P3's projection descriptors cover predicted scroll | P2, P3; J1 V1 |
| D5 | Codec: bincode 2.0.1 declared with `default-features = false, features = ["std", "serde"]`, so `bincode_derive` is never pulled in and nothing new is locked or downloaded (R1 F8); one compact row representation; deflate (existing `flate2`) negotiated for bulk; no new codec or compression crate | P1, P4; J1 change 7; J2 §4.3 |
| D6 | Evolution: a frozen session layer (preface, hello, welcome, refusal, goodbye, relay records), negotiated new variants, append-only top-level fields, and golden fixtures whose digest is the `WireSchemaId`. **The default skew policy is `schema`**: same session major and same `WireSchemaId`. Nested DP vocabulary is positional and churns (R2-16). Cross-schema skew is horizon (§15) | P2 rules; J3 graft 7; R2-16 |
| D7 | Sessions carry N display channels; each channel is one Lisp terminal, so `x-open-connection` stays unconditional (GNU A12) | new [I] |
| D8 | Remote frontends dial in through `neomacsclient --display-relay` to a per-daemon display listener at `<rendezvous>/<server-name>.display`. `<rendezvous>` is `/tmp/neomacs-<uid>`, which is environment-independent, survives logout and is checked with GNU's safe-dir rules. The frontend uses a stable `neo:LABEL`; Lisp opens by name; resume runs in the listener, never in Lisp | P3, P4; J1 change 2; R2-01, R2-02 |
| D9 | Remote frames are created through `server.el` with `-nowait` | P2, P3, P4; J1 V5 |
| D10 | Transport loss on a resumable session detaches with P4's GNU-shaped table. The selected frame leaves the detached terminal, and its KBOARD is released as GNU's `delete_kboard` releases a dying terminal's. Grace expiry deletes `noelisp`, never with an injected `error`. The GTK abort is a recorded divergence | P4 §5.6; J3 graft 15; R2-09 |
| D11 | One last-frame rule for every `neo` terminal: pinned (C32). A standalone frontend process ends its session when it has no windows | P2; J3 graft 11 |
| D12 | The evaluator models every terminal in one registry enum: GNU ids from a monotonic counter (no tombstones, R2-28), record-only liveness, liveness-first teardown | P1 §5.1–5.3; J1 grafts 1–2 |
| D13 | Redisplay visits every terminal with per-terminal sinks and a per-frame display context | all four; C104 |
| D14 | The exact input frontier (`consumed_through` + `in_progress`) replaces the completed-prefix checkpoint before any prediction | P3 D6; J1 V7 |
| D15 | Selection ownership is published lazily as `Own{selection, generation}`. Data is inline only for small values; larger ones are converted on request, as GNU converts at request time. `gui-get-selection` answers locally only while the frontend's ownership signal for that selection is exact, and asks otherwise. Owner and existence are an ordered replica | P3 §5.6; J3 graft 14; R2-11, R2-12 |
| D16 | Every Lisp-visible wait is a correlated request inside `wait_reading_process_output` with a Lisp-variable deadline, in one of GNU's two disciplines. **Selection-style** waits run timers and process output (`gnu:src/xselect.c:1714-1745`). **Display-event-style** waits run no Lisp (`gnu:src/xterm.c:28432-28475`). Request cells are cancelled on every non-local exit. Outcomes converge with `HostResult` | P1 §5.8, P2 §4.8, P4 §4.9; R2-08 |
| D17 | Fonts travel by content (SHA-256) with an evaluator-computed raster policy and a typed pending state. The frontend answers each `Offer` with `Have` or `Want` from a local hash index and never sends an inventory. Renderer-owned text inside frames is laid out on the Lisp host | u5 option A; P1, P2, P4; R2-17 |
| D18 | Images are decoded on the evaluator (new CPU-only crate). Pixels are content-addressed by the hash of their bytes, cached persistently by the frontend and sent on the bulk lane. Encoded bytes come only later, behind sandboxed decoding | P2, P4; J3 graft 6; R2-18 |
| D19 | neo-term is re-homed to the evaluator; until then it is a typed same-host capability | all four |
| D20 | Same-host features are gated by a typed locality derived from how the evaluator obtained the transport. A relay-less dial-in is still `Dialed` in v1; a same-host proof is horizon. The lifecycle difference between spawned and dialed frontends is deliberate | P2 §4.3; R2-29 |
| D21 | Daemons are always headless (C95); a native display name spawns a frontend process. This switches only once that process exists | P2; J2 fatal flaw 3 |
| D22 | Startup "window before Lisp" is kept as a generalized provisional-window adoption available to any frontend | P3; J1 change 8 |
| D23 | Crates: `neomacs-session` (no workspace deps), `neomacs-display-wire` (DP only), `neomacs-display-host`, `neomacs-frontend`, `neomacs-tty-render`, `neomacs-media-cpu` | P4 layering + P3 endpoint inversion |
| D24 | Delivery: baselines first (M0, no product edit); then byte-identical seam refactors; GNU multi-terminal parity; the out-of-process link and an end-to-end slice; services and assets; the relay; a GPU presenter over links (M11b); durability; local-GUI convergence; then prediction | J1 change 1, J2 §5, the computed task; R1 F5, R2-21 |
| D25 | Presentation-only prediction generalizes the existing pixel-scroll grant; deterministic exactness gates, latency reported | P3; J2 §6 item 5 |
| D26 | Subprocess `DISPLAY` never carries a `neo:` name (pgtk precedent), and `browse-url` gets a `neo` branch plus an open-URI display service | P1–P4; J3 A.3 |
| D27 | Security: ssh stdio by default; no network listener; a 0700 rendezvous plus a peer-uid check, with the socket mode set by `chmod` on the path; 256-bit tokens never on argv; bounded decoding with re-seal; hash-verified assets. The per-host side-effect policy defaults clipboard and open-URI to **`on-input`** on `Dialed` links, and shaders are off on `Dialed` links unless the host policy allows them. Nothing ever executes on the display host | P2 §8; P4 §8.3; R2-15, R2-26 |
| D28 | In-flight input is replayed exactly once within a budget (5 s / 256 events), deduplicated by the highest *received* serial. Input typed while the link is only stalled is queued within that budget. Input typed after the frontend has abandoned the transport is dropped with a bell | P3 §4.12; P4 §4.11; J3 Part C item 4; R2-04, R2-05 |
| D29 | The registry, link host and listener code lives in `neomacs-display-host`, not in the `neomacs` binary crate | J2 §1.2, §4.6 |
| D30 | End-to-end acceptance: a separate headless frontend process records `render_text()` of every activated presentation. The latest must equal the text of the presentation the daemon last submitted for that frame. Equality with a fresh `neomacs--frame-snapshot` at quiescence is a separate test | P1 M6; P2 M5; u10 L3; R1 Q2 |
| D31 | Daemon lifetime belongs to the host's service manager; it is reported, not assumed. An auto-started daemon runs under the user's service manager when one is reachable. The daemon publishes a `durability` fact (`persistent`, `until-logout{reason}`, `unknown`). A relay never starts a second daemon while one answers on the rendezvous listener | R2-01, R2-02 |
| D32 | The control lane is **state synchronization**: keyed latest-wins records sent in dependency order, plus a bounded request table. There is no unbounded op log, so a stalled peer can never force a session closed. A request beyond the pending bound fails that one call with a typed error | R2-10 |
| D33 | Quit keys reach Lisp whatever the input bound. The evaluator ships its quit policy to the frontend as config, the frontend exempts quit edges from the bound, and the evaluator reports `consumed_through` on lane 0 independently of presentations | R2-07; `gnu:src/keyboard.c:3787-3850` |
| D34 | Resume survives loss and roaming: two-phase token rotation, implicit takeover by a valid token, one negotiated liveness timeout for both sides, a frontend incarnation epoch that forces a cold resume after a frontend restart, and detach closing the transport | R2-03, R2-04, R2-05 |
| D35 | The remote command is the fixed string `neomacsclient --display-relay`. Server name, label, files and directory travel in the frozen `RelayRequest` record. The relay answers with `RelayStatus` and forwards `server.el` errors on stderr | R2-13, R2-14, R2-31 |
| D36 | A relay-started daemon gets `SSH_AUTH_SOCK` pointing at a stable per-user link that every relay re-points on attach and resume. GNU's own `etc/emacs.service` comment names the problem. Other inherited variables keep GNU's daemon semantics | R2-06 |
| D37 | The primary TTY's `TtyPopupDisplayHost` becomes the popup and selection half of the termcap backing. "Is there a GUI" call sites keep today's answers in M3 and answer from the frame's terminal kind from M5 | R1 F1 |
| D38 | Acceptance baselines are recorded before any source edit: full suites of the touched packages (pass, fail, skip), a TTY **screen** oracle beside the layout snapshot, and the CI build-gate state. "Green" means no new failure against them | R1 F2, F5; R2-22 |
| D39 | A GPU presenter over a link lands before the in-process GUI converts (M11b). It goes through a frontend-side bridge onto today's `RenderComms`, which M13 deletes. The first remote window and its L5 tests therefore come before the riskiest milestone | R2-21 |
| D40 | The wire frame identity is `LiveDisplayFrameId(NonZeroU64)`, beside DP's `LiveDisplayWindowId`. It is not a new `FrameRef`, which would collide with the runtime's `FrameRef` enum until M13 | R1 F23 |

---

## 1. How this design was chosen

### 1.1 The judges did not agree on a winner

| Proposal | J1 architecture | J2 feasibility | J3 user and GNU | Mean | Borda (3/2/1/0) |
|---|---|---|---|---|---|
| P1 GNU multi-terminal first | 7.0 | **8** | 6.8 | **7.27** | 4 |
| P2 Protocol first (NDL/1) | **8.0** | 6 | 7.5 | 7.17 | **5.5** |
| P3 Feel first | 6.5 | 5.5 | **8.1** | 6.70 | 3 |
| P4 Contrarian | 7.5 | 6 | 7.9 | 7.13 | **5.5** |

- **Pairwise majorities [I].** P2 beats P1 (J1, J3) and P3 (J1, J2), and ties P4. P4 beats P1 (J1, J3)
  and P3 (J1, J2), and ties P2. P1 leads the mean by 0.10, almost entirely because of J2's two-point
  spread.
- **P2 rather than P4 as the skeleton [I].** P2 leads P4 on the mean. All three judges found P4's headline
  claim, that the frontend must keep hit lookup, to be a misreading of the code (J1 V1, J2 §2 row 6,
  J3 A.2 item 7). That claim sits inside P4's protocol core. J1: P2's defects "sit in migration order,
  resume layering and registry shape", and each "can be fixed without touching its protocol core" (J1 §6).
- **P1's order rather than P2's [I].** J1's first required change to P2 is P1's order (J1 §6 item 1). J2's
  whole verdict rests on it: P1 is the only proposal whose path to a remote frame avoids work this host
  cannot verify (J2 §1.4).
- **What every judge's own recipe converges on.** J1: P2 skeleton + P1 terminal model and order + P4
  layering + P3 frontier, resume and projection. J2: P1 spine + P2's sans-I/O link + P4's session crate +
  P3's frontier, `LinkEmulator` and one-coupling-per-PR conversion. J3: P3 + P1's terminal rigour + P2's
  codec evolution and security + P4's detach and authority. **The consensus is P2's protocol delivered in
  P1's order, with P3's frontier and feel machinery and P4's layering, detach table and outward
  authority.** That is this design.

### 1.2 Disagreements and how each was resolved

| Topic | Positions | Resolution | Reason |
|---|---|---|---|
| Hit resolution (DQ6) | evaluator (P2, P3); frontend with run-length positions (P1, P4) | **Evaluator**, with P3's `ScrollProjection` and `CurrentGeometry` targets. It lands as its own milestone, after the first remote frame and before the first user-facing remote (MILESTONES M8) | GNU builds posns in the Lisp process (`gnu:src/keyboard.c:5782-5795,6011` [G via P2]). The evaluator already retains presentations and accepts hits only on the active one (`EV/keyboard.rs:2086-2111` [V]; `EV/data/window_state.rs:3154-3170` [V]). The projected hit is a pure function of evaluator-exported coverage plus one offset (J1 V1, J2 §2 row 6). Shipping positions would give one fact two representations by transport (J1 V3). J2's point is honoured: it is not on the critical path of the first remote frame |
| Codec | postcard + columnar + JSON (P2); bincode 2 (P1, P4); bincode or postcard + lz4 (P3) | **bincode 2** for all bodies, without default features; one compact row type inside it; `VisualConfig` as one embedded JSON document; deflate | bincode 2.0.1 is in `Cargo.lock` (`:530-538` [V]), but only as an optional dependency of `typed-index-collections` reached through weak features, and it was never compiled. Its default `derive` feature would add `bincode_derive`, which is absent from the lock and the local registry; without default features only the cached `bincode` and `unty` are needed (R1 F8 [V]). postcard and lz4 are absent [V]; `flate2` is a direct dependency (`Cargo.toml:257` [V]). §6.9 justifies the one JSON document |
| Wire types (J1 Q5) | designed wire structs (P1, P3); in-memory derives (P2) | **DP derives for vocabulary** (guarded); **designed encodings only for the frame**, whose encoder destructures every field | No duplicated type universe, yet the frame payload, which is too large and pointer-shared to send as derived, has its own representation. Field additions become compile errors in the encoder (P2 §4.4) |
| Rendezvous (J1 Q1) | dial-out by name to a relay endpoint (P1, P2); dial-in offer and claim (P3, P4) | **Dial-in by stable label** to a listener in an environment-independent rendezvous directory; Lisp opens by name | Resume below Lisp needs a listener anyway (J1 change 2), so one listener serves offer and resume. Random per-session names break frameset and desktop restore in daemons, because `frameset-keep-original-display-p` is t under `(daemonp)` (`gnu:lisp/frameset.el:1141-1146` [J3·V]). The listener's directory must not depend on `XDG_RUNTIME_DIR` or `TMPDIR`: the runtime directory is removed at the final logout (pam_systemd(8)), and an ssh session's environment can differ from the daemon's (R2-01, R2-02) |
| Version skew | exact build (P1, P2, P4); not required (P3) | **Same `WireSchemaId` by default** (`schema` policy); `exact-build` stricter | v1 recommended same-major skew. R2-16 showed that unsafe: DP vocabulary travels by positional derive, and nested types churn (commit `023cbbe5` added a field to wire-reachable coverage types [V]). Requiring the same schema still lets most builds interoperate, because most commits change no wire-reachable type. Cross-schema skew needs length-framed nested encodings and is horizon (§15) |
| Last frame (C32) | pin (P1); pin while connected (P2); delete with last frame (P3, P4) | **Pin for every `neo` terminal**; a standalone frontend process closes its session when it has no windows | One Lisp rule matching the gtk3 oracle (`gnu:src/frame.c:2969-2983` [G·V]); process lifetime belongs to the frontend program (J1 Q2, J3 graft 11) |
| Detached visibility | nil (P2, P4); unchanged (P3) | **nil** | `suspend-tty` hides the top frame (`gnu:src/term.c:2491-2495` [G·V]); a visible but never-redisplayed frame has no GNU analogue (J3 A.2 item 4). A maintainer decision is still requested (§17) |
| Resume layering | `server.el -eval` (P2); broker or listener (P3, P4) | **Listener, no Lisp** | J1 change 2; `server-goto-toplevel` is irrelevant because resume never reaches `server.el` (`gnu:lisp/server.el:1467-1468` [G·V]) |
| Input during an outage | replay all (P2); budgeted replay (P4); not queued after "Reconnecting" (P3) | **In-flight input replayed exactly once within a budget, deduplicated by received serial. Input typed during a stall is queued within the budget. Input typed after the transport is abandoned is dropped with a bell** | J3 Part C item 4: no blind execution, and no loss of what the user already saw echoed. R2-04: a 3 s stall on a TCP link that recovers on its own must not drop keystrokes that plain ssh would have delivered |
| Clipboard set | correlated request (P2); FIFO (P1, P3, P4) | **Lazy keyed setter**: ownership and a generation; inline data only when small | GNU's `x_own_selection` does not wait (J3 A.2 item 3); the in-tree setter rule (`docs/design/host-effect-projections.md:87-92` [V]). GNU keeps selection data in the Lisp process and converts on request, so eager data on the wire is not GNU's model (R2-11) |
| Renderer-owned text | frontend fonts (P3 `ui-text/1`); Lisp-host layout (P1, P2, P4) | **Lisp-host layout** | C102; `ResolvedMenuLabel` already exists (F12 [M]); a second font authority is rejected (J1 §4.4) |
| Daemon with inherited `DISPLAY` | in-process (P1); always headless (P2, P3, P4) | **Always headless**, switched only in the convergence milestone, once spawned frontends exist | C95 (`gnu:src/dispnew.c:7222-7238` [G]); J2 fatal flaw 3 forbids the gap |
| Startup adoption | delete (P1, P2, P4); generalize (P3) | **Generalize** as `adopt_provisional` on `Create` | Keeps today's first-window feel for every frontend with no special case; not Lisp-visible (J1 change 8) |
| Prediction timing | late (P1, P2, P4); early with research gates (P3) | **After local convergence**, deterministic gates only | The frontier (D14) lands first; J2 §6 item 5 rejects research targets as landing gates |
| neo-term timing | late (P1, P3); early (P4) | **Own milestone after the e2e slice**, typed same-host until then | Not on the critical path; `term.el`, eat and vterm already run on the Lisp host (J3 B.5) |
| KBOARD completion timing | early (P1, J3); late (P3) | **Own milestone; may land any time after the GNU-parity milestone; gates every multi-client claim** | It is a pre-existing divergence that already affects two TTY clients today; the single-user remote does not need it (P1 §5.6) |

---

## 2. Goals and non-goals

**Goals.**

1. **#587 in full.** Remote editing with every package working: buffers, files, processes, eglot and
   lsp-mode (with file watching), magit, compile, shells, TRAMP and vterm run on the remote host. The
   native window, GPU, compositor, input devices, clipboard and IME are local.
2. **One mechanism.** The same protocol, session and frame-sync code path for every window-system
   display: in-process, spawned local or remote. No "if remote" branch anywhere. Transport differences
   are confined to payload representation (`Payload::Shared` versus `Payload::Encoded`) and to a typed
   locality fact used only for host-resource capabilities (§6.2).
3. **GNU 31.1 compatibility.** The multi-terminal contract (u8 C1–C105) holds. Every divergence is typed,
   documented, recorded in `DECISIONS.md`, and reachable only by using the new display kind (§12).
4. **Near-native feel.** Nothing in redisplay waits on the network. Everything the frontend can answer
   itself stays local. Speculation is exact and certified by the evaluator. Sessions survive sleep and
   network loss. Whether they also survive the user's final logout on the remote host depends on that
   host, and the daemon reports it as a typed durability fact (D31).
5. **Least authority, untrusted input.** ssh by default, no network listener, bounded decoding,
   hash-verified assets, and a local policy for every side effect the remote side initiates.
6. **Landable and verifiable.** Every milestone lands with no new failure against the baselines recorded
   before the first source edit (D38). It has acceptance commands that run on this GPU-less host. What
   only CI can verify (GUI pixels, ssh, systemd logout) is stated, never implied.

**Non-goals for this campaign** (all are design horizon, §15):

- "Lisp local, I/O remote" (TRAMP, tramp-rpc). It is a *compatibility target* that should simply work on
  Neomacs, not a second architecture (P4 §2.4; u9 R15).
- Layout on the frontend. Layout re-enters Lisp mid-walk (`LE/redisplay_fontification.rs:1-9`,
  `LE/layout_effect.rs:120-133`, `LE/display_status_line.rs:1758-1759` [J2·V]).
- Pixel or video streaming of whole frames.
- TCP, TLS, QUIC or UDP roaming transports; auto-deployment of the remote runtime.
- Two frontends viewing one frame (collaboration); moving frames between terminals.
- Video and webview on non-same-host displays (typed unavailable in v1).
- A Windows evaluator host (the frontend runs on Windows through ssh stdio).

---

## 3. User experience

### 3.1 Local use is unchanged

`neomacs` opens a window as today: one process, winit on the main thread, the evaluator on its worker
thread. Internally the window is a `neo` terminal whose session uses the in-process transport.
Nothing is serialized and no extra process starts. The first window still appears before Lisp runs,
through provisional-window adoption (D22).

### 3.2 Remote editing

```sh
laptop$ neomacs --remote me@devbox                     # attach to (or auto-start) devbox's daemon
laptop$ neomacs --remote me@devbox -- src/main.rs      # also visit files, emacsclient-style
laptop$ neomacs --remote me@devbox -s work --label desk  # daemon name; display label
laptop$ neomacs-frontend --remote me@devbox            # the same, from the lean frontend binary
```

What happens:

1. **Laptop.** The frontend role starts and never initializes Lisp. It opens the native display, shows a
   provisional "Connecting to devbox…" window, and measures monitors, DPI and desktop font settings. That
   window is frontend chrome (§8.2). The frontend then runs `ssh -T me@devbox -- neomacsclient
   --display-relay`. **That remote command is a fixed string.** No file name, label, server name or other
   user data ever reaches the remote shell, which would otherwise interpret it (ssh(1) appends arguments
   to the command line; R2-13). ssh brings the user's keys, agent, known_hosts and ControlMaster.
   Automatic retries add `-o BatchMode=yes -o ControlPath=none -o ServerAliveInterval=5
   -o ServerAliveCountMax=2`, so a retry never prompts and never multiplexes onto a stale master (R2-14,
   §8.2).
2. **Relay preamble.** The frontend writes the session preface (role `frontend`) and one frozen
   `RelayRequest{server_name, label, files, dir, resume}` record to ssh's stdin (§6.3). It scans at most
   64 KiB of ssh's stdout for the relay's answering preface and shows any bytes it skipped, such as
   shell rc-file noise.
3. **devbox, relay.** `neomacsclient --display-relay`:
   - reads and validates the preamble (bounded; label and server-name grammars, §7.3);
   - checks the rendezvous directory `/tmp/neomacs-<uid>` with `server-ensure-safe-dir` rules
     (`lisp/server.el:557-604` [M]; GNU's client-side check is missing in `neomacsclient` today,
     `NM/bin/neomacsclient.rs:440` against `gnu:lib-src/emacsclient.c:1233-1258` [M]), then probes the
     display listener `<rendezvous>/<server-name>.display`. `NEOMACS_DISPLAY_RENDEZVOUS_DIR` overrides
     the directory for tests and must be set identically on both sides. A live daemon answers the probe
     with its build, its `durability` fact and the server socket path it actually uses. The relay
     therefore never depends on its own `XDG_RUNTIME_DIR` or `TMPDIR` agreeing with the daemon's: today
     `neomacsclient` derives the path from `XDG_RUNTIME_DIR` when set (`NM/bin/neomacsclient.rs:664-675`
     [V]), and an ssh login often differs from a desktop session (R2-02);
   - starts a daemon only if nothing answers. It uses the user's service manager when one is reachable
     (`systemd-run --user`, with readiness by `sd_notify`, which GNU sends at `gnu:src/emacs.c:3434`
     [G·V] and Neomacs does not send yet). Otherwise it runs `neomacs --bg-daemon[=NAME]` with `setsid`,
     as today (`NM/client_daemon.rs:96-122` [V]). Either way `DISPLAY` and `WAYLAND_DISPLAY` are removed,
     so a daemon never grabs an ssh-forwarded X display (u7 §3.2 [M]), and `SSH_AUTH_SOCK` names the
     stable agent link of D36 (§11.5);
   - re-points the stable agent link at its own `SSH_AUTH_SOCK`;
   - writes `RelayStatus{Connected{durability, daemon_build, started} | Refused{reason}}` to the frontend
     and then splices bytes. It parses only frozen session-layer records (the preface, its own records,
     and the `SessionWelcome`/`SessionRefusal` header that tells it the attach was accepted). It never
     parses display-service bodies (R2-31).
4. **devbox, daemon.** The listener runs a per-connection handshake task with a deadline (R2-34). The
   task validates the frontend's `SessionHello` (preface, versions, `WireSchemaId`, peer uid, label) and
   registers the **offer** `neo:LABEL`. When the relay sees the `SessionWelcome` record pass, it sends an
   ordinary `server.el` request to the server socket path the probe returned. Every argument is quoted
   with `server.el`'s own quoting: `-nowait -display neo:LABEL -window-system -env … -dir … [-file …]`.
   If `server.el` replies `-error`, the relay writes the text to its stderr, which ssh carries to the
   frontend, and exits non-zero. The frontend then shows "Attach failed: TEXT" instead of waiting for
   the offer to expire (R2-31).
5. **`server.el`** (byte-identical to GNU, `cmp` [J3·V]) calls `make-frame` with
   `(display . "neo:LABEL")`. `window-system-for-display` maps it to `neo`. `x-create-frame` finds no live
   terminal of that name, so it opens one: the evaluator claims the offer, opens a display channel and
   creates terminal *T* with an id from `next_terminal_id`. The frame is created on *T* with
   `(client . nowait)` (`gnu:lisp/server.el:993-1000` [G·V]). The relay's `server.el` client ends at
   once. From then on it is only a byte splice. **Frame lifetime belongs to the display session, not to
   the ssh process.**
6. **Editing.** Buffers, files, subprocesses, eglot, compilation and shells run on devbox. Clipboard,
   menus once open, tooltip placement, hover, smooth scrolling over certified coverage, animation and IME
   pre-edit are local.
7. **Durability.** If the daemon reports `until-logout`, the frontend shows the reason once, for example
   "sessions on devbox end at your last logout there; `loginctl enable-linger` keeps them" (D31, §11.5).

### 3.3 Inside the session (GNU semantics hold)

| Action | Result |
|---|---|
| `C-x 5 2` on a remote frame | A new frame on the same terminal: a second native window on the laptop |
| `(x-display-list)` | `("neo:desk")` plus any other displays, oldest first |
| `(window-system)`, `(display-graphic-p)` | `neo`, `t`, the same as the local GUI |
| `(terminal-live-p (frame-terminal))` | `neo` |
| `neomacsclient -c -n` in a devbox shell started from a remote frame | Another frame on the laptop. Subprocesses of `neo` frames get `NEOMACS_DISPLAY=neo:desk`, and `neomacsclient -c` without `-d` prefers it, as `DISPLAY` works under X11 forwarding (R2-33; today `-c` reads only `WAYLAND_DISPLAY` and `DISPLAY`, `NM/bin/neomacsclient.rs:839-857` [V]). `-d neo:desk` works from any shell |
| `C-x C-c` on a remote frame | The `nowait` branch of `server-save-buffers-kill-terminal` deletes that frame because other frames exist (F1 always does in a daemon) (`gnu:lisp/server.el:1907-1916` [G·V]). The daemon survives |
| `M-x shell`, `M-x compile` | Run on devbox with the daemon's `process-environment`, plus `DISPLAY` per §11.4. GNU's frame `environment` parameter "has no effect now" on subprocesses (`gnu:lisp/server.el:1000-1011` [G·V]). Neomacs matches that (`NS/environment/mod.rs:238-290` [V]). v1 wrongly said the frame's `-env` applies. `SSH_AUTH_SOCK` follows D36 |
| `(make-frame-on-display ":0")` on devbox | A native display **on devbox** (GNU: the daemon dials display names): spawns a frontend process there |
| `M-x neo-term` before its milestone | `(neomacs-display-capability-unavailable terminal-emulator #<terminal 3 on neo:desk> requires-same-host)` |

### 3.4 Several frames and clients

- Any number of frames and child frames per terminal; each frame is one native window.
- A second machine (`--label laptop2`) is a second session and a second terminal with its own KBOARD.
  Concurrent typing on two terminals needs KBOARD completion (MILESTONES M16). The same limitation
  already affects two TTY clients today (E5 [M]), but it is **not** documented yet: the "Current
  limitations" list in `docs/daemon.md:183-207` [V] does not mention KBOARD sharing (R1 F17). M5 adds it.
- One daemon serves TTY clients, remote GUI frontends and spawned local frontends at once; each is a
  terminal.

### 3.5 Disconnect, sleep and resume

| Event | Laptop | devbox (Lisp-visible) |
|---|---|---|
| Wi-fi blip shorter than 3 s | Input keeps flowing; predictions switch to the marked style | nothing |
| Stall: no pong for 3 s and no EOF | A "Waiting for devbox…" band. Input is still queued within the replay budget (D28). The old transport is kept, and after 2 s more a replacement ssh starts in parallel. Whichever completes first wins: a pong on the old link cancels the attempt, and a replacement `Resume` takes over implicitly (D34, R2-04) | nothing until the liveness timeout |
| Network loss or sleep: EOF, an error, or no traffic for the agreed `keepalive_timeout` (10 s, the same on both sides) | Windows keep the last presentation, dimmed, with a "Reconnecting to devbox…" band; they are read-only, and input from then on rings the bell and is dropped. ssh is retried with backoff 0, 1, 2, 4, 8, then every 15 s; automatic retries never prompt | Terminal *T* becomes **detached**: frames live but `frame-visible-p` is nil, *T* is not redisplayed and not counted as active, pending requests fail with `neomacs-display-disconnected`, the selected frame moves off *T* (§12.2), and `neomacs-display-detach-functions` run. The evaluator closes its end of the transport, so the relay and the sshd child exit instead of lingering until TCP gives up |
| Reconnect within grace | Same windows, same placement; a cheap resync; input that was in flight is delivered exactly once | The listener validates the token (current or previous, D34) and rebinds the transport **without running Lisp**. At the next safe point *T* is connected again, frames regain the visibility Lisp last set, and `neomacs-display-resume-functions` run. `server.el` is not involved |
| Frontend restart within grace (crash, laptop reboot) | The stored token resumes the same frames as a **cold resume**: new input stream and time domain, a full keyframe, no input replay | as above, plus `StoreReset` |
| The user's final logout on devbox | — | With `durability = persistent` (lingering, or no logind) nothing changes. With `until-logout`, systemd may remove the runtime directory or kill the session scope. The rendezvous listener survives in `/tmp`, so resume still works while the daemon lives, but `server.el`'s socket may be gone. The frontend showed this fact at attach (D31) |
| Grace expires (`neomacs-display-detach-timeout`) | "Session expired"; offers a new session | *T* and its frames are deleted `noelisp`, with hooks deferred and run LIFO on dead objects (C39). The daemon keeps running |
| User closes the last window | The frontend exits once it has no windows (`Goodbye{NoWindows}`) | The WM close becomes GNU's `delete-frame` event; the terminal stays pinned until the session closes, then it is deleted |
| `kill-emacs` on devbox | "Session ended" | — |

### 3.6 Errors the user sees

Each failure is a typed Lisp condition (§10) and a precise frontend message:

- protocol or schema mismatch, showing both builds and both `WireSchemaId`s and naming which build to
  install where;
- build mismatch under the `exact-build` policy;
- label in use, for a token-less offer (`--takeover` offered);
- expired or unknown resume token;
- daemon unreachable, with the rendezvous path and the error;
- `server.el` socket unreachable while the listener answers (resume works; new frames need `server-start`
  from a resumed frame);
- "remote command not found", with the `--remote-command` remedy (§8.2);
- interactive authentication needed during an automatic retry;
- attach failed, with `server.el`'s error text;
- refused capability (feature, terminal, reason);
- the durability warning of §3.2 step 7.

---

## 4. Process and thread topology

### 4.1 Local GUI (non-daemon), in-process transport

```
┌──────────────────────────────── neomacs (one process) ──────────────────────────────────┐
│ OS main thread: winit EventLoop + RenderApp (frontend runtime, DR) ◄── FrontendSession   │
│   windows, compositor, menus, tooltips, IME, hover, prediction       (neomacs-frontend)  │
│                         ▲  Down envelopes (Arc payloads)  │ Up envelopes                 │
│                         │  in-process transport           ▼                              │
│ thread "neomacs-evaluator": Context ── DisplayTerminals                                  │
│    T0 initial_terminal ── T1 ":0" (neo) ── LinkConnection(channel 1) ── session outbox   │
│ thread "display-reader-S1": no Lisp; decode → convert → early C-g → HostInputPublisher   │
│ threads: neomacs-frame-prepare (materialize), GPU workers, clipboard worker,             │
│          desktop-settings observer (moved here from the evaluator's chain)               │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

The evaluator-side outbox and FrameSync encoder run exactly as for a remote session. The frontend-side
FrameSync decoder rebuilds the state from `Arc` rows, re-seals it and hands it to today's mailbox and
`neomacs-frame-prepare` worker. A debug switch, `NEOMACS_DISPLAY_TRANSPORT=wire`, runs the same GUI over a
socketpair through the real codec (P3 §3.1).

### 4.2 Daemon with a spawned local frontend

```
neomacs --daemon[=N]   (always headless: the evaluator owns the main thread)
  ├ evaluator: Context, redisplay of ALL terminals
  ├ display-listener (accepts dial-in sessions; validates resume tokens; runs no Lisp)
  ├ per session: link-reader / link-writer
  ║  socketpair, inherited fd, no filesystem node
neomacs --frontend=gpu --inherit-fd 3 --display :1    (spawned by `x-open-connection ":1"`)
  ├ main: winit + RenderApp; link-io; frame-prepare; GPU workers; clipboard; settings observer
```

The daemon spawns `/proc/self/exe` on Linux, so a package upgrade that replaced the installed binary
can never pair a daemon with a frontend of another build. On macOS it spawns the executable path captured
at startup, and the handshake's `BuildId` check refuses a mismatch (R2-16). winit allows one event loop
per process on every backend (E9 [M]), so **every display beyond the in-process one is its own frontend
process**. A headless daemon can then open GUI frames, which lifts
`docs/daemon.md:185-195` [M] by the same mechanism, not by a special case.

### 4.3 Remote

```
laptop                                         devbox
neomacs --remote devbox (frontend role)        sshd ── neomacsclient --display-relay --label L
  main: winit + RenderApp                    (splice: stdio ⇄ /tmp/neomacs-<uid>/<name>.display)
  link-io: codec, pacing, keepalive                                  ║ unix socket, same uid
  reconnect supervisor, asset store                  neomacs --daemon: display-listener,
  child: ssh -T devbox -- neomacsclient …                link-reader/writer per session, Lisp main
```

### 4.4 Thread inventory and rules

| Thread | Process | Runs Lisp? | Must never |
|---|---|---|---|
| evaluator / main | evaluator | yes | park on a transport; the only waits are correlated requests inside `wait_reading_process_output` |
| `display-listener` | evaluator | no | create frames (only Lisp does); block one connection behind another. It only accepts; each handshake or resume runs on its own task with a 10 s deadline, at most 16 at once (R2-34) |
| any thread that needs a Lisp variable's value (listener, link reader and writer) | evaluator | no | read a Lisp variable. It reads an evaluator-published snapshot of the variable's global value, refreshed when the variable is set, as `QuitPolicy` is (R2-27) |
| `link-reader-S` | evaluator | no | wait on Lisp (`docs/design/render-input-backpressure.md` invariant 1 [V]) |
| `link-writer-S` | evaluator | no | hold evaluator data structures (it receives owned messages) |
| winit main | frontend | no | block on the network |
| `link-io` | frontend | no | touch GPU or window state |
| `neomacs-frame-prepare` | frontend | no | panic on peer input (validation precedes sealing) |
| reconnect supervisor | frontend | no | queue input typed after "Reconnecting" is shown |

---

## 5. The cut point

### 5.1 The cut

- **Downlink:** the sealed post-layout presentation (`SealedFramePresentation{FrameDisplayState}`), sent
  as row-slot deltas; frame-addressed window, UI, config and selection operations; content-addressed
  assets; correlated requests.
- **Uplink:** input events with frame, `DisplayTime` and serial; presentation acknowledgements in the same
  ordered stream as pointer input; versioned display facts; replies.
- **Policy:** evaluator-certified interaction facts (pixel-scroll grant today; insert, delete and motion
  rules later) ride inside each presentation. The frontend may act alone only within them.

### 5.2 Evidence that this is the right cut

1. **It is today's publication boundary.** `SealedFramePresentation` is the "Immutable publication
   boundary for a completed display presentation"; `seal()` checks revision, placement, placeholder
   windows and spatial projections (`DP/sealed_frame_presentation.rs:1-6,47-72` [V]).
2. **The frame half is already serde and serialized in production.** `FrameDisplayState` derives serde
   (`DP/glyph_matrix.rs:2112-2113` [V]); `neomacs--frame-snapshot` serializes it to JSON
   (`NM/frame_layout.rs:174-180,203-240` [V]).
3. **Everything semantic runs before it and never waits for the renderer:** layout, synchronous layout
   queries, shaping, font selection, Lisp font queries (L1–L8 [M]).
4. **Materialization and drawing read draw facts only.** v1 claimed that no non-test runtime or
   renderer code reads glyph provenance or text positions. That was overstated (R1 F9, F10); the
   exceptions are exactly what M8 moves or keeps:
   - The render thread resolves text-position hits today (`RT/frame_windows.rs:886,923,1150`;
     `RT/frame_compositor/input_scroll.rs:368-383` [V]). It ships `hit: Option<PresentedHit>` to the
     evaluator (`DR/thread_comm.rs:73-76` [V]). M8 moves this to the evaluator.
   - The TTY rif logs `legacy_charpos()` in a tracing dump (`DR/backend/tty/rif.rs:3985` [V]). That is
     evaluator-side after M4.
   - The compositor reads row anchors (`start_charpos`, `end_charpos`: `input_scroll.rs:274-280`,
     `continuity/scroll.rs:150-168`, `continuity/reflow.rs:140-150` [V, J1·V]). It also reads window-level
     facts: `window_start`, `window_end`, `buffer_size` and `buffer_id`
     (`RT/frame_compositor/overlays.rs:54-58`, `continuity/scroll.rs:206-215`,
     `continuity/mod.rs:155-163,292-299` [V]). These travel in `WindowLayout.info`. The cursor's
     `charpos` is part of the frame too (`DP/frame_glyphs.rs:742-748` [V]).

   What never crosses is the **hit-test** text source and its evaluator GC roots (D4).
5. **The uplink seam is already transport-agnostic.** The input bridge converts without Lisp and raises
   quit before enqueueing (`NM/input_bridge/publication.rs:177-184` [V]).
6. **GNU's own remote mode cuts here.** With X forwarding, fonts, images and layout stay in the Emacs
   process and only drawing reaches the display (C102 [M]; `gnu:src/image.c:4231-4252,4411-4462` [G]).
7. **The alternatives lose.** Pixels discard the local compositor, animation and prediction. A
   materialized display list loses row identity. Layout on the frontend puts the network inside every
   fontification and synchronous query (P2 §2.1, P4 §2.1, P1 §2.4).

### 5.3 Eight corrections to today's seam

| # | Responsibility | Today | After | Why |
|---|---|---|---|---|
| X1 | neo-term PTY, shell, VT | render side (`DR/terminal/view.rs:240-270` [M]) | evaluator: a Lisp PTY process plus an evaluator VT | the shell must run where the files are; GNU `term.el` runs on the Lisp host [G] |
| X2 | image decode, probe, SVG, XPM, XBM, tool-bar icons | decoder pool moved to the render side (`NM/main.rs:4333-4336` [M]) | evaluator, in `neomacs-media-cpu`; pixels on the bulk lane | GNU decodes in the Lisp process [G]; removes the 1 s and 10 s waits (R2, R3, C14 [M]) |
| X3 | pointer → buffer position | frontend, via a GC-root-bearing lazy index (`DP/presented_pointer.rs:683-687` [M]) | evaluator, against its retained presentation | §1.2 row 1; no evaluator GC roots ever leave the evaluator thread |
| X4 | desktop font settings, render options, text DPI, resources, monitors | observed in the evaluator's process (`NM/main.rs:426-454` [M]) | frontend; sent in the handshake and as versioned facts | they are facts about the display host (DQ8 [M]) |
| X5 | `x-hide-tip`, `image-cache-size` | read from render memory (S4, S8 [M]) | evaluator-authoritative | GNU answers both in the Lisp process (`gnu:src/xfns.c:8984-9021`, `gnu:src/image.c:2484-2494` [G]) |
| X6 | renderer capabilities | shared atomics (S1, S2, D1 [M]) | handshake replica plus acknowledgements | shared memory has no wire form |
| X7 | latest presentation per frame | shared-lock mailbox (`DR/thread_comm/frame_mailbox.rs:1-3,76-98` [V]) | evaluator-side outbox; frontend-local mailbox kept | the "superseded" proof stays local to the producer; redisplay never parks |
| X8 | input bridge | one thread for one display | one link reader per session, tagged by terminal; uses the *actual* quit char | today it tests only the default quit character (`publication.rs:179` [V]) |

---

## 6. Protocol specification

### 6.1 Layers

```
Endpoints  evaluator: LinkConnection · Outbox · LinkReader/Writer   frontend: FrontendSession → presenter
Service    NDP display service: channels · FrameSync · AssetSync · replicas · input stream · requests
Session    neomacs-session: preface · records · lanes · scheduler · windows · keepalive · handshake ·
           resume · close (sans-I/O state machine, time injected)
Transport  in-process: typed envelopes, Arc payloads, waker │ byte stream: socketpair, Unix socket,
           child stdio (ssh), later TLS
```

- The **session layer** knows nothing about displays. It negotiates **services** by name and revision.
  NDP is the only service this campaign builds; a later `workspace` service (TRAMP-native) reuses
  framing, authentication, pacing and durability (P4 §2.4).
- The session core is **sans-I/O** (`receive(&[u8])` or `receive_typed(Envelope)`, `poll_transmit(now)`,
  `tick(now)`), in the style of `quinn-proto`. Time is a parameter (`display-crate-layout.md:110-115`
  rule 1 [V]). A generic `std::io` pump drives it over any `Read + Write`.
- **One payload type, two representations:**

  ```rust
  /// The session and sync layers never look inside.
  pub enum Payload<T: ?Sized> { Shared(Arc<T>), Encoded(Bytes) }
  ```

  Rows, asset bytes and the `VisualConfig` document use it. In process nothing is serialized; the `Arc`
  moves. Everything above the transport is the same code on every transport.

### 6.2 Transports and locality

| Transport | Obtained by | `LinkLocality` | Resumable | Compression |
|---|---|---|---|---|
| in-process | GUI startup in a non-daemon run | `InProcess` | no (the process is the display) | never |
| socketpair, inherited fd | the evaluator spawned the frontend | `SpawnedChild` | no (no rendezvous to return through) | never |
| Unix socket via the display listener | dial-in (relay or local tool) | `Dialed` | yes, within the grace | negotiated |
| TLS (horizon) | explicit opt-in listener | `Dialed` | yes | negotiated |

`LinkLocality` is **decided by the evaluator from how it obtained the transport, never from the peer's
claims** (P2 §4.3). It answers one question: can the frontend reach the evaluator host's files and
processes? Only `InProcess` and `SpawnedChild` may advertise same-host capabilities (video from files,
webview `file:`, neo-term before it is re-homed). The typed capability reason is `requires-same-host`,
which names where the resource lives. It is not a "remote" special case; each such capability disappears
once its feature is re-homed or streamed (P2 §7.3).

A frontend that dials the listener directly (`--connect PATH`, which the tests use) is `Dialed` too. A
Unix-socket peer cannot tell a local frontend from an ssh-forwarded socket without a proof of host-local
file access. That proof is horizon (§15, R2-29); until then a same-host `--connect` frontend loses video,
webview and pre-M15 neo-term. The lifecycle difference is deliberate and documented. A crashed spawned
frontend deletes its terminal `noelisp`, because there is no rendezvous to return through. A crashed
dialed frontend detaches it, because it can return.

### 6.3 Preface and records

**Preface** (16 bytes, sent first by both sides; frozen forever):

```
0..8    magic          "NEOSESS\0"
8..10   session_major  u16 LE = 1
10..12  session_minor  u16 LE = 0
12      role           u8: 1 evaluator, 2 frontend, 3 relay
13      flags          u8 = 0 (non-zero → refuse)
14..16  reserved       u16 = 0
```

A peer that sees bad magic writes one human-readable refusal line and closes, so a wrong ssh command or a
`server.el` socket gives a readable error (P4 §4.2).

**Record** (after the preface):

```
u32 LE  len       body bytes; ≤ 64 KiB before Welcome, then ≤ negotiated max_record (default 1 MiB);
                  checked BEFORE any allocation
u8      lane      0 control · 1 input · 2 present · 3 facts · 4 hint · 5 bulk
u8      flags     bit0 COMPRESSED · bit1 MORE (chunk follows) · bits 2–7 must be 0
u8      service   0 session · 1 display (assigned in Welcome)
u8      reserved  0
body              bincode-2 standard() encoding of the service's message, or one chunk of it;
                  compressed body = varint uncompressed_len (≤ max_message) ‖ raw deflate
```

- **Order.** The receiver processes records in arrival order; the stream is totally ordered. Lanes steer
  only the sender's scheduler and per-lane chunk reassembly. Cross-lane causality that matters is
  explicit (§6.4).
- **Sequence numbers are implicit:** each side counts complete messages per lane. Acks, resume cursors and
  replay refer to those counts. No bytes are spent on them.
- **Chunking.** A message larger than the chunk size is split on its lane so control records interleave.
  Chunk size is `clamp(rate × 5 ms, 1 KiB, 64 KiB)` (P3 §4.7).
- **Totality.** Decoding never panics. Truncation, oversize, unknown lane or tag, and non-zero reserved
  bits are a typed `ProtocolError`; the session closes with `Goodbye{ProtocolViolation}`.

**Relay records** (frozen like the handshake; R2-13, R2-31). They let user data travel inside the stream
instead of on a remote shell's command line, and they make the relay a typed participant:

```rust
/// Frontend → relay: the first record after the frontend's preface on the ssh stream (≤ 64 KiB).
pub struct RelayRequest {
    pub server_name: Option<ServerName>, // [A-Za-z0-9._-]{1,64} or an absolute path; None = "server"
    pub label: DisplayLabel,             // §7.3 grammar
    pub resume: bool,                    // true: the relay sends no server.el request
    pub dir: Option<Vec<u8>>,            // remote working directory for -dir, as bytes
    pub files: Vec<RelayFile>,           // { path: Vec<u8>, line: Option<u32>, column: Option<u32> }
    pub ext: Vec<(u16, Vec<u8>)>,
}
/// Relay → listener: the relay writes its own preface (role 3). The listener answers once and the same
/// connection then carries the frontend's spliced bytes, so the daemon knows a relay is in the path.
pub struct ProbeReply { pub build: BuildId, pub durability: Durability, pub server_socket: Vec<u8> }
/// Relay → frontend, before any daemon byte is spliced.
pub enum RelayStatus {
    Connected { durability: Durability, daemon_build: BuildId, started: bool },
    Refused { reason: RelayRefusal }, // DaemonUnreachable{path, errno} | ServerSocketUnreachable{path}
                                      // | UnsafeRendezvous{path} | DaemonTooOld{build} | StartFailed{detail}
}
pub enum Durability { Persistent, UntilLogout { reason: DurabilityReason }, Unknown }
// DurabilityReason: SessionScope | NoLinger | RuntimeDirRemovedAtLogout
```

The relay builds the `server.el` request from `RelayRequest` with `server.el`'s argument quoting, over
the local server socket. No shell is involved.

### 6.4 Lanes and ordering

| Lane | Direction | Sender priority | On resume |
|---|---|---|---|
| 0 control | both | 0 | keyed state re-synchronized (the records that differ from what the peer acknowledged, D32); requests pending at detach were failed |
| 1 input | up | 0 | replay serials above the evaluator's `received_through`, exactly once, within a budget (§6.19) |
| 2 present | down | 1 | **resync**, not replay: bounded journal or keyframe by reference |
| 3 facts | up | 1 | resync: latest value per kind |
| 4 hint | both | 2 | dropped |
| 5 bulk | down | 3 | reconciled per offer (`Have`/`Want`) |

Ordering invariants:

- **O1, per-frame order.** When the outbox admits `Window(frame, Destroy)` it drops and retires that
  frame's unsent presentation. A presentation whose chunks arrive after its window was destroyed is
  answered `Discarded`. This replaces today's unordered command and frame queues (T7 [M]).
- **O2, causal acks.** Presentation acknowledgements travel on the input lane with pointer input, so the
  evaluator's "hit only on the active presentation" rule stays causal (`EV/keyboard.rs:2086-2111` [V];
  I18 [M]).
- **O3, ordered facts.** Facts whose order relative to input matters travel on the input lane:
  `SelectionState` (D15), `Focus`, window state edges. A key pressed after a foreign copy therefore
  sees the foreign owner (R2-12).

### 6.5 Session handshake (frozen records)

```rust
/// Frontend → evaluator. First record on every transport (in-process included).
pub struct SessionHello {
    pub build: BuildId,                 // version, git commit, wire_schema: WireSchemaId (§6.6)
    pub incarnation: FrontendIncarnation, // random u64 per frontend process start (D34, R2-05)
    pub services: Vec<ServiceOffer>,    // [{ name: "display", revisions: (1,0)..=(1,m) }]
    pub intent: SessionIntent,
    pub limits: Limits,                 // max_record, max_message, store_rows, store_bytes,
                                        // asset_cache_bytes, input_edges
    pub ext: Vec<(u16, Vec<u8>)>,       // TLV extensions; unknown keys ignored
}
pub enum SessionIntent {
    InProcess,
    Inherited,                          // spawned child on an inherited fd
    Offer { label: DisplayLabel, takeover: bool },  // dial-in through the display listener
    Resume { session: SessionId, token: ResumeToken, cursors: ResumeCursors },
}
/// Evaluator → frontend.
pub struct SessionWelcome {
    pub session: SessionId,
    pub resumability: Resumability,     // Resumable { token, grace_secs: Option<u32> } | NotResumable
    pub accepted: Vec<ServiceAccept>,   // [{ name: "display", revision: (1, m) }]
    pub limits: Limits,                 // the minimum of both sides applies
    pub keepalive_timeout_ms: u32,      // one liveness timeout for both sides (D34)
    pub durability: Durability,         // D31
    pub build: BuildId,
    pub resume: Option<ResumeOutcome>,  // Warm { cursors, received_through } | Cold (new incarnation)
    pub ext: Vec<(u16, Vec<u8>)>,
}
pub enum SessionRefusal {
    ProtocolMajor { ours: u16, theirs: u16 },
    SchemaMismatch { ours: BuildId, theirs: BuildId },    // default `schema` policy (§6.6)
    BuildMismatch { ours: BuildId, theirs: BuildId },     // only under the exact-build policy
    ServiceUnavailable { name: ServiceName },
    LabelInUse { label: DisplayLabel, state: LabelState }, // token-less offer, no takeover
    ResumeUnknown, ResumeExpired, OfferExpired, Unauthorized,
    ResourceLimit { kind: LimitKind },
}
// Session-level messages on lane 0: Ping{nonce}, Pong{nonce}, Ack{lane, through},
// Credit{lane, bytes}, Goodbye{reason: CloseReason}.
```

The frontend speaks first on every transport, as the display server offering itself (P2, P3, P4).

- **Tokens.** `ResumeToken` is 256 bits from `getrandom` (`Cargo.toml:174` [V]). It is never on a command
  line and is stored 0600 in the platform state directory (§8.2). `server-generate-key` is never used: it
  draws 64 characters from a PRNG with a 32-bit seed (u7 §2.8 [M]).
- **Two-phase rotation (R2-03).** Every `Welcome` issues token *n+1*. The evaluator keeps accepting
  token *n* until the frontend's first record after that `Welcome` arrives, which proves it holds *n+1*.
  A `Welcome` lost in flight therefore never strands a session or squats its label.
- **Implicit takeover (R2-04).** A valid current or previous token on a session still marked connected
  closes the old transport with `Goodbye{TakenOver}` and resumes at once. Roaming therefore never meets
  `LabelInUse`. `--takeover` is needed only for a token-less `Offer` on a label another session holds;
  it then adopts that terminal as a cold resume.
- **Incarnation (R2-05).** A `Resume` whose `incarnation` differs from the one recorded at the last
  `Welcome` comes from a restarted frontend. It is a **cold resume**: a new `InputStreamId` and
  `DisplayTime` domain, `StoreReset` plus a keyframe, configuration replayed as at attach, and no input
  replay, because the old process's replay buffer is gone. A warm resume (same incarnation) follows
  §6.19.

### 6.6 Versioning, evolution and build policy

1. The preface, `SessionHello`, `SessionWelcome`, `SessionRefusal`, `Goodbye`, `RelayRequest`,
   `RelayStatus` and `ProbeReply` **never change shape**, so any two versions can refuse each other
   cleanly. The relay speaks only these records, so relay ↔ daemon skew is limited to them (R2-16).
2. A service revision is `major.minor`. A major mismatch is refused (`ProtocolMajor`).
3. A minor is **additive only**:
   - New message kinds and enum variants are sent only when the agreed minor is at least the one that
     introduced them, or the capability that names them was negotiated. Receiving one that was not
     negotiated is a protocol violation, never silently ignored.
   - New fields may only be **appended at the top level of a message body**. A decoder reading a record
     from a newer minor ignores trailing bytes inside that record's length. Nested vocabulary types are
     positional in bincode, so any change to one changes the `WireSchemaId` (item 5).
4. **Golden fixtures** (`expect-test`, workspace `Cargo.toml:399` [V]) pin every message's bytes per
   minor. CI decodes the fixture corpus of every earlier minor of the current major. That tests
   new-decodes-old only. Old-decodes-new cannot be tested without old decoders and is not claimed; the
   `schema` policy never needs it.
5. **`WireSchemaId`** is the SHA-256, truncated to 128 bits, of the canonical golden fixture corpus,
   compiled into `BuildId`. The exhaustiveness test (`every_variant_has_a_golden_fixture`) makes the
   corpus cover every wire-reachable type, so the id changes whenever any wire encoding changes, nested
   vocabulary included. Commit `023cbbe5`, which added `publication` to serde-derived coverage types,
   would have changed it [V].
6. **Build policy** (`neomacs-display-version-policy`): `schema` (the default: same session major and
   same `WireSchemaId`, capabilities intersected) or `exact-build`. Both `BuildId`s are always exchanged
   and shown. A mismatch is the typed `SchemaMismatch` refusal naming both builds, and the frontend says
   which build to install where.
   - **Why not v1's same-major `compatible` default (R2-16).** DP vocabulary travels by positional serde
     derive and churns: 3 of the 50 commits in the shallow history touched DP's sources [V]. A
     same-major policy would accept peers that silently misdecode.
   - **Why not exact build.** Most commits change no wire-reachable type, so `schema` keeps durable
     sessions working across most upgrades (J3 B.8 item 4).
   - **Horizon.** Length-framed nested structs with per-peer-minor encoding would make additive
     cross-schema skew safe (§15). VS Code and Zed use lockstep because they auto-deploy the server
     (u9 §2.3.1); Neomacs's daemon is long-lived and holds user state, and auto-deploy is horizon. [Q]
     for the user (§17 U6).

### 6.7 Display service: sessions, channels and terminals

- A **session** is one transport connection to one frontend process: one physical display and keyboard.
  It owns the lanes, credits, the row store, the asset offer state, the request table and the input
  stream.
- A **display channel** is one Lisp terminal of type `neo` on that session. `x-open-connection` opens a
  channel; `delete-terminal` and `x-close-connection` close one.
- **A second `x-open-connection` for the same name opens a second channel on the same session**, so
  `x-open-connection` stays unconditional as GNU's is (`gnu:src/xfns.c:7640-7660` [G·V]: `x_term_init`
  is called without a lookup; P1 attestation form A12). Because channels of one session share one
  physical keyboard, they **share one KBOARD**: the analogue of GNU's `same_x_server` (C45 [M]).
- Monitors, desktop facts and capabilities are per session and mirrored into each channel's terminal
  replica. Selections are per terminal (C100).
- A session with no open channel is an **offer**; an unclaimed offer expires after 60 s with
  `OfferExpired`.

### 6.8 Message families (display service, revision 1.0)

All ids are newtypes. Frame references are `LiveDisplayFrameId(NonZeroU64)` (D40), so frame 0 is
unrepresentable (`EV/context_api.rs:1946-1950` [V] shows today's frame-0 sentinel). It is the frame twin
of DP's existing `LiveDisplayWindowId` (`DP/types.rs:23-43` [V]). The name `FrameRef` stays with the
runtime's `FrameRef { Primary, Frame(u64) }` until M13 deletes it (`DR/thread_comm.rs:299` [V]; the
runtime glob-re-exports DP modules, so a second `FrameRef` would collide, R1 F23).

**Downlink (evaluator → frontend):**

| Family | Messages (field sketch) | Lane | Semantics | Replaces (map rows) |
|---|---|---|---|---|
| Channel | `OpenChannel{channel, terminal: TerminalNumber, name: DisplayName}`, `CloseChannel{channel, reason}` | 0 | the frontend answers `ChannelOpened` or `ChannelRefused` | T6, C1 |
| Window | `Window{frame, op}`; `op` ∈ `Create(WindowInit{channel, parent: Option<LiveDisplayFrameId>, size, position, visibility, decorations, title, hints, adopt_provisional: bool})`, `Destroy`, `SetTitle`, `SetGeometryHints`, `Resize{size, request: ResizeRequestId}`, `SetPosition`, `SetVisibility`, `SetFullscreen`, `SetDecorations`, `Iconify`, `Focus{activate}`, `RequestAttention`, `SetPointerShape`, `WarpPointer`, `ShowChild{…}`, `RemoveChild`, `Opacity(Accept │ SetLowerLimit │ SetRedirects │ Retire)`, `ImeSpot(rect)` | 0 | keyed latest-wins **state** per `(frame, kind)`, sent in dependency order (a frame before its children, a destroy after them); one-shot actions (`WarpPointer`, `RequestAttention`, `Focus{activate}`) coalesce per `(frame, kind)` (D32) | C2–C6, S5, S13 |
| Present | one transaction: `Begin{frame, presentation: PresentationId, update: UpdateSeq, base: Keyframe │ Delta, requires: Vec<AssetId>}` · `DefineRow{slot: RowSlot, key: RowKey, row: Payload<RowRuns>}`* · `Tables(TableDelta)` · `Windows(Vec<WindowLayout>)` · `FrameFacts{…}` · `Commit{presentation}`; also `StoreReset` | 2 | latest-wins per frame in the outbox before encoding; credit-gated (§6.12) | T2, F1–F14 |
| Asset | `Offer{id: AssetId, kind, len}`, `Forget{id}`, `ImageRealize{image, load, extent, pixels: AssetId}`, `ImageDrop{image}` on lane 0; `Put{id, offset, bytes: Payload<[u8]>, last}` on lane 5 | 0, 5 | content-addressed, offer before reference (§6.11) | C9–C13, F7, F11 |
| Ui | `ShowPopupMenu{token: MenuToken, frame, anchor, items: Vec<ResolvedMenuItem>}`, `HidePopupMenu{token}`, `PresentTooltip{ticket, cause: InputStamp, frame, anchor, runs}`, `DismissTooltip{ticket}`, `VisualBell{frame}` | 0 | tokens and tickets per session; labels laid out on the Lisp host | C22–C24, F14 |
| Config | `VisualConfig(Payload<VisualConfigDoc>)`, `ModifierPolicy`, `ScrollIndicators`, `TitlebarHeight`, `ShowFps`, `CornerRadius`, `ExtraSpacing`, `ChildFrameStyle`, `ConfirmQuit`, `SelectedRoot`, `CursorBlink`, `QuitKeys{quit_char, single_kboard_current: Option<ChannelId>}` | 0 | keyed latest-wins; replayed on attach and resume. `QuitKeys` lets the frontend exempt quit edges from its input bound (D33) | C25, C26, S14 |
| Selection | `Selection{channel, op: Own{selection, generation, content: Inline(Payload<[u8]>) │ Deferred{len_hint}} │ Disown{selection, generation}}` | 0 | keyed latest-wins per `(channel, selection)`. Data is inline only up to 64 KiB; larger values are converted on request (`Convert`, below), as GNU converts at request time, so `select-active-regions` never ships a large region after every command (D15, R2-11) | C27 (set half) |
| Selection data | `ConvertReply{convert, outcome}` on lane 0; `SelectionData{convert, offset, bytes: Payload<[u8]>, last}` on lane 5 | 0, 5 | answers an uplink `Convert`; chunked and credit-controlled | — |
| Input progress | `InputProgress{stream, consumed_through}` | 0 | sent whenever the evaluator's consumption advances, at most every 20 ms. The frontend's input bound tracks consumption even when no presentation is sent (D33, R2-07) | — |
| Request | `Request{id: RequestId, channel, body}`; `body` ∈ `SelectionGet{selection, target}`, `SelectionExists{selection}`, `OpenUri{uri}`, `VideoDiagnostics{video}` (same-host), `TerminalText{id}` (until neo-term is re-homed); `Cancel{id}` | 0 | correlated (§6.13) | C20, C27 |
| Input bookkeeping | `InputCancelled{ranges: Vec<SerialRange>}` | 0 | explicit cancellation, replacing `Drop` | S11, I21 |
| Hint | `ScrollPreview{frame, window, intent, inputs: Vec<InputSerial>}`, `CoverageRefill{…}` | 4 | lossy, latest-wins per key | C7 |
| Media | `SurfaceCreate/SetUniform/Free`, `FrameShaderSet/SetUniform`, `Video(…)`, `WebView(…)` | 0, 5 | capability-gated (§9) | C16–C19 |

`DefineRow` and the frame tables (field sketch, §6.10):

```rust
pub struct RowSlot(u32);                 // sender-managed, < store_rows
pub struct RowKey([u8; 16]);             // SHA-256 of the canonical RowRuns bytes, truncated
pub struct WindowLayout { window: DisplayWindowId, geometry: WindowGeometry,
                          rows: Vec<SlotRef>, info: Option<WindowInfo> /* only when its digest changed */ }
pub struct SlotRef { slot: RowSlot, y: f32 /* bit-exact */, anchors: RowAnchors, cursor: Option<RowCursor> }
pub struct RowAnchors { start_charpos: i64, end_charpos: i64, displays_text: bool, mode_line: bool }
pub struct FrameFacts {
    scalars, backgrounds, borders, cursors, scroll_bars, chrome, transition_hints,   // only when changed
    regions: NonTextHitIndex,            // window parts, mode/header lines, fringes, dividers,
                                         // resize handles, chrome interaction ids: NO text positions
    pointer_appearance: PointerAppearanceMap,   // hover and mouse-face stay local
    coverage: Vec<CoverageRef>,          // keyed (window, epoch, publication); rows are SlotRefs
    frontier: FrontierSnapshot,          // §6.14
    policies: Vec<WindowInteractionPolicy>,      // §13.3; pixel-scroll grant in v1
    layout_scale: LayoutScale,           // the device scale layout used (absent today, F10 [M])
}
```

**Uplink (frontend → evaluator):**

| Family | Messages (field sketch) | Lane | Semantics | Replaces |
|---|---|---|---|---|
| Channel | `ChannelOpened{channel, facts: DisplayFacts}`, `ChannelRefused{channel, reason}` | 0 | | T6 |
| Input edges | `Input{serial: InputSerial, time: DisplayTime, event}`; `event` ∈ `Key{frame, key: FrontendKey, mods: ModifierBits, text}`, `ImeCommit{frame, text}`, `Pointer{frame, target: PointerTarget, action, mods}`, `PresentedPointer{frame, presentation, interaction, pressed, button, at}`, `ToolBarClick{frame, presentation, item}`, `MenuBarClick{frame, presentation, request, index, key, anchor}`, `MenuSelection{token, index}`, `TooltipDismissed{ticket, reason}`, `FileDrop{frame, target, uris}`, `CloseRequested{frame}`, `Focus{frame, focused, tooltip_stamp}`, `Moved{frame, position}`, `Iconified{frame}`, `Deiconified{frame}`, `ResizeCompleted{request}`, `ApplicationQuit{channel}` | 1 | FIFO, lossless, exactly once by serial; no key releases | I1–I7, I14–I20, I26 |
| Input levels | `PointerMotion{frame, target, mods}`, `WindowGeometry{frame, size, scale}` | 1 | coalesced in place, latest per frame; never across an edge, and a `WindowGeometry` is never coalesced across a `ResizeCompleted` | I3, I4 |
| Presentation | `Received{frame, presentation}`, `Activated{frame, presentation, pending_assets}`, `Discarded{frame, presentation}`, `Retired{presentation}`, `Rejected{frame, presentation, reason}`, `StoreAck{through: UpdateSeq}` | 1 | causal with pointer input (O2); `Activated`/`Discarded` return credit | I18 |
| Ordered facts | `SelectionState{channel, selection, owner: Ours{generation} │ Foreign │ Empty │ Unknown, sequence: Option<u64>}` | 1 | ordered relative to input (O3). The source is the platform's ownership signal (§6.17 `selections/1`); `Unknown` when the platform has none (D15, R2-12) | — |
| Selection conversion | `Convert{convert: ConvertId, channel, selection, target, generation}` | 0 | a local application pasted Emacs-owned data; the evaluator converts it through the Lisp selection converters and answers with `ConvertReply`/`SelectionData` | — |
| Level facts | `Monitors{version, list}`, `Desktop{version, system_fonts, text_dpi, render_options}`, `Resources{version, db}`, `Capabilities{version, set}`, `DisplayReset{reason}` | 3 | versioned replicas, latest-wins | I8, I9, I25, S1, S9, S12, H1 |
| Asset | `Have{ids}`, `Want{ids, priority}`, `Stored{id}`, `Evicted{id}`, `ResidencyLost{image}` | 0 | only offered ids may be wanted | I11 |
| Reply | `Reply{id, outcome: ReplyOutcome}` | 0 | §6.13 | R1, C20, C27 |
| Media events | `SurfaceCreateFailed{id, error}`, `FrameShaderAck{request, execution}`, `FrameShaderFailed{request, error}`, `Video{id, event}`, `WebView{id, event}` | 0 | every reply names its request | I10, I12, S2 |
| Overflow | `InputOverflow{from: InputSerial, to: InputSerial}` | 0 | the frontend dropped edges at its bound and says so | T3 |

```rust
pub enum PointerTarget {                         // P3 §5.7; resolved on the evaluator (D4)
    Presented { presentation: PresentationId, at: LayoutPoint },
    ScrollProjection { presentation: PresentationId, window: DisplayWindowId, epoch: CoverageEpoch,
                       publication: CoveragePublicationId, offset: PixelOffset, at: LayoutPoint },
    CurrentGeometry { at: LayoutPoint },         // the pixels under the pointer were speculative
    Unpresented { at: LayoutPoint },
}
```

`at` is in layout space: the frontend applies its own visual transforms (pane-morph inverse projection)
before sending (u2 §1.4 [M]).

### 6.9 Codec

- **Message bodies:** serde with **bincode 2.0.1** `standard()` (varint, little endian), behind one
  `WireCodec` module in `neomacs-display-wire`.
  - **Lock status.** bincode 2.0.1 is in `Cargo.lock` (`:530-538` [V]) only as an optional dependency of
    `typed-index-collections`, reached through the weak features `bincode?/alloc` and `bincode?/std`. It
    has never been compiled here (R1 F8 [V]).
  - **Declaration.** Its default features include `derive`, which pulls in `bincode_derive`; that crate
    is absent from the lock and the local registry. The dependency is therefore declared
    `bincode = { version = "=2.0.1", default-features = false, features = ["std", "serde"] }`.
    `bincode-2.0.1.crate` and `unty-0.0.4.crate` are already in the local registry cache [V], so no
    download is needed and no package is added.
  - No postcard, lz4 or zstd exists in the tree [V]. Swapping the codec is one module, pinned by the
    golden fixtures.
- **bincode cannot honour seven serde attributes**: `flatten`, `skip`, `skip_deserializing`,
  `skip_serializing`, `skip_serializing_if`, `tag` and `untagged`. Its serde module says they "will result
  in lost data" (bincode-2.0.1 `src/features/serde/mod.rs:42-53` [V]; v1 listed only two, R1 F11).
  - **DP today.** DP has 13 `skip_serializing_if`/`flatten` sites (`rg` [V]) and 7 field-level
    `#[serde(skip)]` sites (`DP/glyph_matrix.rs:1265,2121`; `DP/geometry.rs:100`;
    `DP/presented_pointer.rs:816,818,2025,2027` [V]). It has no `tag` or `untagged`.
  - **Rule.** A DP type reachable from a wire message carries none of the seven. One exception: a
    symmetric field-level `skip` whose value is rebuilt after decode, as `PresentedPointerMap` rebuilds its
    caches (`DP/presented_pointer.rs:2721-2733` [V]), because both sides skip the same position.
  - **Enforcement.** A DP test enumerates wire-reachable types and checks the rule (P4 §4.3).
  - `#[serde(default)]` stays where JSON snapshots need it; it is harmless for bincode.
- **Vocabulary types** (ids, enums, `MonitorInfo`, `FrontendKey`, `Face`, `WindowInfo`, …) travel through
  their DP serde derives: no second type universe.
- **The frame** is not sent through `FrameDisplayState`'s derive. The FrameSync encoder (§6.10)
  destructures `Glyph{…}` and `GlyphRow{…}` **without `..`**, so a new field is a compile error in the
  codec (P2 §4.4). Rows travel as `RowRuns`: UTF-8 text, face runs, a per-row palette of distinct `f32`
  bit patterns for advances, run-length metrics, bidi and box flags, and provenance relative to the row's
  start (base plus steps plus exceptions). Floats are bit-exact. Row `hash` is never sent; the receiver
  recomputes it (`GlyphRow::compute_hash`, P2 §4.4) and never trusts a peer cache key.
- **`VisualConfig` is the one embedded self-describing document.** It crosses as `VisualConfigDoc`, the
  JSON text of its existing serde reflection (DP already depends on `serde_json`, `DP` `Cargo.toml` [V]).
  Reasons: it uses `#[serde(flatten)]` over the effect catalog (`DP/visual_config.rs:135-139` [V]); the
  effect registry *is* serde reflection over it (`display-crate-layout.md` rule 6 [V]); its leaves churn
  with every new effect knob; and it is latest-wins and rarely sent. A binary encoding would force a
  protocol minor for every effect knob. In process it moves as `Payload::Shared`.
- **Determinism.** Maps are encoded in key order (the in-memory `FxHashMap`s are not canonical, D8 of u1
  [M]); floats are bit-exact. Encodings are canonical, so digests and golden fixtures are stable.
- **Compression:** raw **deflate** through the existing `flate2` (`Cargo.toml:257` [V]) at level 1,
  negotiated (`compression/1{deflate}`), applied only to bulk and presentation records over 512 bytes on
  `Dialed` links. Never in process or on socketpairs. Already-compressed assets (woff2) are never
  recompressed. Decompression is bounded by the declared length. LZ4 or zstd become capabilities only if a
  measurement justifies a new dependency (AGENTS.md §5; u9 R13).

### 6.10 Frame transfer: FrameSync

**State per session (sender side).**

- A **row store** of sender-managed slots (`RowSlot`), bounded by the frontend's `store_rows` and
  `store_bytes`, LRU-replaced by the sender. The receiver never evicts on its own, so the sender always
  knows exactly what the receiver holds (the X Render GlyphSet model, u9 R4).
- **Row identity:** a fast path on `Arc` pointer identity of rows already sent (today's copy-on-write
  `MatrixRow(Arc<GlyphRow>)`, F3 [M]), then a 128-bit content key over the canonical `RowRuns` bytes
  (`sha2`, `Cargo.toml:170` [V]). Content keys replace the process-local `RowAppearance` counter (F4 [M])
  as the wire identity and deduplicate chrome rows that are always `New` today.
- **Per frame:** last-sent tables as digests (faces, font definitions, `char_fonts`, shaped clusters,
  fringe bitmaps, effect profiles, frame scalars, chrome, cursors) and the last-sent window layouts.
- **Font slots** are session-wide and interned by content, which removes per-service `ResolvedFontId`
  collisions (F8 [M]).

**Encoder, per presentation** (on the session writer):

1. Take the latest sealed presentation of a frame that has credit. Older pending ones were already
   superseded in the outbox and retired synchronously, exactly as `submit` returns a
   `SupersededPresentation` today (`DR/thread_comm/frame_mailbox.rs:76-98` [V]).
2. For each row (window matrices, chrome rows, scroll coverage): pointer memo → known slot; else content
   key → known slot; else take a slot not referenced by this update and emit `DefineRow`. In process the
   content-key step is skipped and the row is `Payload::Shared(Arc<GlyphRow>)`.
3. Diff the tables by digest; emit only changed definitions.
4. Emit each changed window as a `WindowLayout` of `SlotRef`s. **Damage is derived from slot identity
   against the last *sent* layout** (same slot at the same `y` → reused; same slot elsewhere → reused,
   shifted; otherwise new). The producer's own `RowDamage` is relative to its previous layout, not to what
   the receiver holds; it is not sent, and today's `skipped_predecessor` patch-up
   (`frame_mailbox.rs:11-14,92-96` [V]) disappears.
5. Emit `FrameFacts` fields whose digests changed. **The hit index crosses without its text source**: its
   non-text regions, resize handles and appearance map only (D4). The lazy, GC-root-bearing source
   (`DP/presented_pointer.rs:683-687` [M]) never leaves the evaluator thread; its absence is already
   representable (`PresentedTextPositionHit::Unsupported`, J1 V3).
6. `Begin` … `Commit`, chunked when needed.

**Decoder** (frontend `link-io` thread): apply every op in order, **including those of a presentation that
will be discarded**, because the store is a decode dictionary and not the display state. At `Commit`:
rebuild the `FrameDisplayState` (rows from slots plus anchors), recompute row hashes, call
`SealedFramePresentation::seal`, then `FrameDisplayState::validate_for_materialize()`, a new fallible check
covering the `expect`s in `materialize()` (`DP/glyph_matrix.rs:3068-3087` [M]). Only then does the
presentation enter today's frontend-local mailbox and `neomacs-frame-prepare` worker, unchanged. A failure
answers `Rejected{reason}` and closes the session with `Goodbye{ProtocolViolation}`; a peer frame can never
panic the frontend.

**Baseline and keyframes.**

- Deltas are against the **state the receiver holds after everything already sent**. On a reliable
  ordered stream that needs no acknowledgement for correctness (u1 §3.5 [M]). The receiver acknowledges
  applied updates with `StoreAck{through}` (piggybacked, coalesced).
- Keyframes are sent on attach, after `DisplayReset`, after `Rejected`, and on resume when the journal
  cannot cover the gap.
- **Resume** (MILESTONES M12): the sender keeps a **journal of store and table mutations after the
  acknowledged update**. Credits bound it to about two presentations per frame. The frontend reports its
  applied cursor in `SessionIntent::Resume`; the sender rolls its mirror back to it and continues with
  deltas. Without a usable journal it sends `StoreReset` and a keyframe whose rows are `SlotRef`s wherever
  the frontend's surviving slot keys still match (RDP cache import, u9 §2.3.8).

**Scroll coverage** goes through the same store, so a committed scroll into prefetched coverage costs only
slot references. Coverage identity on the wire is `(window, epoch, publication)`; `Arc::ptr_eq` admission
(F6 [M]) becomes key equality. The evaluator retains each exported coverage until its presentation is
retired, so `ScrollProjection` targets can be resolved (J1 V1).

**Size** [E], u1's 200×60 scenario (7,200 body glyphs), from P2 §4.5: keyframe about 25–30 KB raw
(8–12 KB deflated), against u1's per-glyph serde model of 416 KB; one typed character about 300 B raw; one
line of scroll about 850 B raw. MILESTONES M2 replaces these with measurements on real frames.

### 6.11 Assets

```rust
pub enum AssetId { Content([u8; 32]) /* SHA-256 of the canonical bytes: a font file, or a decoded
                                       pixel buffer with its format header (R2-18) */,
                   Session(NonZeroU64) /* shader-surface pixels and other per-session buffers */ }
pub enum AssetKind { FontFile { container: FontContainer, faces: u32 },
                     ImagePixels { format: PixelFormat, width: u32, height: u32, stride: u32 },
                     SurfacePixels { width: u32, height: u32 } }
```

| Step | Message | Rule |
|---|---|---|
| 1 | `Offer{id, kind, len}` | sent before the first presentation that `requires` the asset; the default face's fonts are prefetched at attach |
| 2 | `Have{ids}` or `Want{ids, priority}` | The frontend may want only offered ids. It answers from a local index of installed font files and cached assets, keyed by (dev, inode, mtime, size) → SHA-256 and built in the background, so identical files neither cross the link nor get parsed from remote bytes (J3 graft 23). **No inventory is ever sent.** v1's inventory in `SessionHello` had no field to carry it, could exceed the 64 KiB pre-`Welcome` cap on font-rich machines, and fingerprinted the laptop to every host. A host now learns only whether the frontend has the assets it offered (R2-17) |
| 3 | `Put{id, offset, bytes, last}` | bulk lane, chunked, credit-controlled; priority: fonts of visible glyphs of the newest presentation, then coverage, then the rest (P3 §4.6) |
| 4 | `Stored{id}` | the frontend verifies SHA-256 for `Content` ids **before use or persistence**, so a malicious peer cannot poison a cache that later sessions share (P2 §4.6) |
| 5 | `Evicted{id}`, `ResidencyLost{image}` | the evaluator re-offers on next reference |

- **Pending, never fallback.** A glyph run whose font has not arrived is drawn as a typed *asset-pending*
  run with exact advances, so positions are exact and nothing reflows, and `Activated` reports it. The
  renderer's silent emergency family fallback (`RW/glyph_atlas/mod.rs:2229-2379` [M]) is deleted for
  every display.
- **Persistence:** `<platform cache dir>/neomacs/assets/sha256/`, under a byte budget. That is
  `$XDG_CACHE_HOME` (0700) on Linux, `~/Library/Caches` on macOS, and `%LOCALAPPDATA%` with a user-only
  ACL on Windows (R2-35).
- **Decoded pixels are content assets too.** They are keyed by the SHA-256 of the pixel buffer, verified
  before use and persisted under the same budget, so a key can never be poisoned across hosts. Page
  turns, new sessions and GPU resets reuse them instead of re-shipping megabytes (R2-18; u6's arithmetic:
  one 1920×1080 RGBA image is 8.3 MB). The evaluator keeps a bounded pixel LRU and re-decodes on a miss.

### 6.12 Flow control

| Traffic | Mechanism | Bound |
|---|---|---|
| Presentations | latest-wins per frame in the evaluator outbox; at most **K = 2** sent but neither `Activated` nor `Discarded` per frame; the credit returns with the ack, so pacing follows RTT (mosh rule, u9 R3) | one pending presentation per frame; this also bounds retained `PreparedDisplayPresentation`s and interaction GC roots (u2 B8 [M]) |
| Whole stream | bytes written but not acknowledged ≤ `clamp(2 × BDP_est, 64 KiB, 8 MiB)` (P4 §4.8); BDP from ping RTT and acknowledged byte rate | keeps kernel and ssh buffers shallow, so a keystroke never waits behind more than one chunk |
| Bulk | sent only when the interactive lanes are empty and in-flight bytes are under half the window; frontend `Credit{bulk, bytes}` as it persists chunks | credit window |
| Control | **state synchronization** (D32): window, UI, config, selection ownership and media state are keyed latest-wins records, emitted in dependency order. Every record dirtied before a request is sent before that request, which keeps the in-tree setter-before-request rule (`host-effect-projections.md:87-92` [V]) | bounded by the number of live Lisp objects (frames, surfaces, selections), not by time, so a stalled peer accumulates nothing. A request beyond the pending bound (1024) fails **that call** with `neomacs-display-error` (`resource-limit`). **No session is ever closed or detached because a peer is slow.** v1 closed non-resumable sessions on `OutboxOverflow`, which would have torn down the in-process GUI on a render stall (R2-10). **Lisp never blocks** |
| Hints | latest-wins per key | one per key |
| Uplink input | the winit callback never blocks; edges FIFO and lossless; levels coalesced; the frontend bounds *unconsumed* edges at 4096, GNU's `kbd_buffer` size (C58 [M]), counted from `InputProgress{consumed_through}`, which the evaluator sends whether or not a presentation follows (D33) | at the bound: bell, overflow indicator, `InputOverflow{from, to}`; never a silent drop. **Quit edges are exempt** and are always sent, as GNU handles `quit_char` before its buffer-full check (`gnu:src/keyboard.c:3787-3836` then `:3845-3850` [G·V]; R2-07). The evaluator reader never stops reading, so pings, acks and a late C-g always flow (`render-input-backpressure.md` invariant 1 [V]) |

**Redisplay never parks.** Submitting returns the superseded presentation synchronously; every setter
enqueues; nothing on the evaluator thread waits on the transport. That removes the "Emacs over `ssh -X`
hangs in redisplay" failure (u3 B4, FLAG-L1 [M]) and today's blocking `send` from layout
(`NM/main.rs:1824-1836`, `LE/display_source_resolver.rs:1258-1287` [M]).

### 6.13 Request and response

```
evaluator thread                        link-reader thread                  frontend
LinkConnection::request(body)
 ├ id = next HostReplyId (evaluator); RequestId (session) ↔ HostReplyId; PendingTable.insert(id, cell)
 ├ every record dirtied earlier is sent first, then Request{id, channel, body} ──►  service it
 └ wait_reading_process_output(WaitRequest::host_reply(id, deadline, discipline))
     selection-style: timers, process output, special events and quit run
     display-event-style: only quit and the reply (no Lisp runs)
                                        ◄── Reply{id, outcome} ◄──────────── reply
                                        fill cell; publish HostReplyReady(id)
 ← Answered(v) │ TimedOut │ Cancelled(non-local exit) │ Detached │ Closed(reason) │ Refused(reason)
```

- **Mechanism.** This is GNU's selection pattern (a reply cell plus `wait_reading_process_output`,
  `gnu:src/xselect.c:1714-1745` [G·V]) on the existing `WaitRequest` machinery, which already has a
  special-input completion policy for the resize acknowledgement (`NS/wait/mod.rs:339-366` [V]).
- **Where the reply id lives (R1 Q1).** Add `SpecialInputWaitPolicy::CompleteOnHostReply(HostReplyId)`.
  `SpecialInputWaitPolicy` is a private `Copy` enum in `neovm-system` (`NS/wait/mod.rs:339-347` [V]), and
  `neovm-system` depends only on `neovm-{edit,eval,text,value}` (`cargo metadata` [V]). The id is
  therefore an evaluator-owned `HostReplyId(NonZeroU64)` defined in `neovm-eval` beside `HostResult`.
  `LinkConnection` (in `neomacs-display-host`) maps it to the session's `RequestId`, so no wire type
  reaches the wait code.
- **Two disciplines (D16, R2-08).**
  - *Selection-style* waits (selection get and exists, video diagnostics) run timers and process output,
    as GNU's call does (`wait_reading_process_output (secs, nsecs, 0, false, reading_selection_reply,
    NULL, 0)`, `gnu:src/xselect.c:1730-1736` [G·V]).
  - *Display-event-style* waits (resize acknowledgement, map, display open) run no Lisp. GNU's
    `x_wait_for_event` only `pselect`s the display connection (`gnu:src/xterm.c:28432-28475` [G·V]), and
    today's `WaitRequest::resize_ack` sets `TimerWaitPolicy::Suppress` and `ProcessWaitPolicy::None`
    (`NS/wait/mod.rs:572-582` [V]). v1 ran timers in every wait. A timer could then have re-opened, deleted
    or redisplayed a half-open terminal in the middle of `x-open-connection`; GNU never exposes that.
  - Quit is honoured in both. For display open that is an extension, because GNU's `XOpenDisplay`
    cannot be interrupted (§12.3).
- **Cell ownership.** The pending cell is owned by an unwind guard. Any non-local exit cancels it, sends
  `Cancel{id}` and drops a late reply: quit, a `throw` from a timer (`with-timeout`), or a signal from a
  process filter (R2-08). **Loss** fails every pending cell with a typed outcome. This generalizes
  `retire_render_command_obligations` (T8 [M]) and satisfies `render-input-backpressure.md` invariants
  2–4 and 7 [V] (never fabricate a value on timeout).
- **Outcomes converge with `HostResult::{Returned, LispError, Interrupted, Closed(HostClosure)}`**
  (`crates/neovm-eval/src/host_effect/contracts.rs:503-508` [V]) when that framework activates (P4 §4.9).
- **Teardown.** A `Closed` connection makes teardown-path host effects **successful no-ops** until
  `noelisp` deletion, as GNU's `x_connection_closed` sets `dpyinfo->display = 0`
  (`gnu:src/xterm.c:26921-26925` [G]). Otherwise `destroy_gui_frame` errors would abort teardown
  (`ND/window_cmds/mod.rs:8763-8776`, J1 V10).
- **The frontend's own bound.** The frontend answers every request within its own bound (5 s for
  selections) with a typed timeout. A stalled foreign selection owner on the laptop
  (`DR/clipboard.rs:137` [V]: "A foreign Wayland selection owner can stall a transfer forever") therefore
  cannot hang `C-y`. Only a dead link can, and the liveness timeout ends that (R2-32).

**Per-primitive table** (the classifier is the in-tree rule `host-effect-projections.md:87-92` [V]: a
setter with no GNU Lisp-visible result enqueues; operations that return a value or signal stay
synchronous):

| Primitive | Class | Deadline (variable, default) | Timeout | Quit | Detached or closed |
|---|---|---|---|---|---|
| `gui-set-selection`, `x-own-selection-internal`, `x-disown-selection-internal` | keyed latest-wins setter (`Own{generation}`, data inline ≤ 64 KiB, else deferred) | — | — | — | applied to evaluator ownership state; resynced on resume |
| `gui-get-selection`, `neomacs-clipboard-get` | **local answer while the `SelectionState` replica says `Ours{generation}` from an exact ownership signal** (`x_get_local_selection` first, `gnu:src/xselect.c:2637-2645` [G·V]); otherwise a selection-style request, which is today's local behaviour too: neo-win never trusts CLIPBOARD ownership and always asks (`lisp/term/neo-win.el:436-466` [V]; `DR/clipboard.rs:371-384` returns `Unknown` [V]; R2-12) | `x-selection-timeout` ms, 0 = unbounded (`gnu:src/xselect.c:3434-3439` [G·V]); bounded in practice by the frontend's 5 s reply bound (replacing today's fixed 5 s `CLIPBOARD_REPLY_TIMEOUT`, `NM/main.rs:1212,1846` [V], a deliberate change, R2-32) | `(error "Timed out waiting for reply from selection owner")` (`gnu:src/xselect.c:1744` [G·V]) | `quit` | `neomacs-display-disconnected` |
| `x-selection-owner-p` | the ordered `SelectionState` replica; `Unknown` answers nil, today's conservative rule (`lisp/term/neo-win.el:436-453` [V]) | — | — | — | nil |
| `x-selection-exists-p` | the replica when it is not `Unknown`; else a selection-style `SelectionExists` request. **It never fetches the data**: today neo-win fetches the whole selection to test existence (`lisp/term/neo-win.el:468-478` [V]), and `deactivate-mark` calls it after every region deactivation while owner-p is nil (`gnu:lisp/simple.el:7058-7070` [G·V]) | `x-selection-timeout` | GNU error | `quit` | typed |
| `x-popup-menu`, `x-popup-dialog` | async token; the selection returns as an input event (R4 template) | user-driven | — | `HidePopupMenu` | nil, as a cancelled menu |
| geometry getters after `set-frame-size` (eight callers of `sync_live_gui_resize_for_geometry_queries`, `ND/frame/mod.rs:2801-2809` [V]) | `Resize{request}` → `ResizeCompleted{request}`, **display-event-style** | today's 1 s bound, unchanged until DQ25 is attested (GNU: `x-wait-for-event-timeout` 0.1 s, `gnu:src/xterm.c:32785-32794` [G]) | current, possibly non-final size | `quit` | last known size |
| `x-display-*`, `display-monitor-attributes-list` | per-terminal replica (C99) | — | — | — | last known values |
| `mouse-position`, `mouse-pixel-position` | last-event replica (today's Neomacs semantics, L6 [M]) | — | — | — | last known; documented divergence (GNU queries the server) |
| `x-hide-tip`, `x-show-tip` | evaluator-authoritative tip state with an evaluator timer | — | — | — | hidden |
| `image-size`, `image-metadata`, `image-mask-p` | evaluator decode | — | — | — | unaffected |
| `image-cache-size`, `clear-image-cache`, `image-flush` | evaluator nominal ledger plus `ImageDrop` | — | — | — | unaffected |
| font queries, layout queries | evaluator only | — | — | — | unaffected |
| `neomacs-video-diagnostics` | request, same-host only, selection-style | `neomacs-video-diagnostics-timeout` (5 s) | `neomacs-display-timeout` | `quit` | typed |
| `x-open-connection` and display open | dial or claim, then `ChannelOpened`, **display-event-style** | `neomacs-display-open-timeout` (10 s) | `(error "Display %s can’t be opened")` | aborts the open (extension) | — |

### 6.14 Replicas, causal stamps, receipts and the exact frontier

- **Facts** (frontend → evaluator), versioned per session and mirrored per terminal: monitors (removing
  the 2 s startup condvar wait, R5 [M]), desktop fonts, text DPI, render options (observer moved to the
  frontend, I25), capabilities (replacing the shared `AtomicU8` and mutex, S1 and S2), the display's
  resource database (`x-get-resource` answers from it, as GNU on X reads the server's
  `RESOURCE_MANAGER`, H1 [M]). A monitor change runs `display-monitors-changed-functions` with the terminal
  (C81).
- **Config** (evaluator → frontend): keyed latest-wins, replayed on attach and resume. This generalizes
  what `set_display_host` replays today (`EV/gc_pacing.rs:1102-1122` [M]).
- **Causal stamps where a lagging replica would be wrong:** the tooltip generation rides on input events,
  `PresentTooltip` echoes the stamp of the input that caused it, and the frontend rejects stale stamps
  (S3 [M]).
- **Value receipts and the exact frontier** (D14). Today `InputCheckpoint{stream, through}` advances only
  while the front of the pending deque is complete (`DP/input_progress.rs:151` [V]); command scopes open
  *before* keys are read (`command_loop.rs:575-577` [V], "A mid-command redisplay must not acknowledge
  unfinished input"); scopes complete on `Drop` (`DP/input_progress.rs:232-246` [V]); stream ids come from
  a process static (`:14` [V]). Once every key carries a receipt, `through` stalls below every minibuffer
  key while the frames already show them (P3 §4.9; J1 V7). The replacement:

  ```rust
  pub struct InputStreamId(NonZeroU64);   // per session, allocated in Welcome
  pub struct InputSerial(NonZeroU64);     // dense per stream, assigned by the frontend
  pub enum FrontierSnapshot {
      Exact { stream: InputStreamId, consumed_through: u64, in_progress: SerialSet },
      Indeterminate { stream: InputStreamId },   // input consumed while this frame was laid out
  }
  pub enum Disposition { Reflected, Partial, Pending }
  ```

  The snapshot is captured before layout and re-checked after, because "Layout Lisp can enter a nested
  command reader" (`LE/engine.rs:2276-2278` [J1·V]). Cancellation is the explicit `InputCancelled`, not
  `Drop`. Today's completed prefix stays derivable, so the pixel-scroll reconciler migrates with identical
  results for its single-event inputs.

### 6.15 Time

- **No `Instant` on the wire, ever.** `EventTime` wraps an `Instant` and stays inside each process
  (`time_discipline_test`, u10 §2.3 [M]).
- **`DisplayTime(u64 ms)`:** the frontend's monotonic clock since session start, one domain per session
  *incarnation*, stamped at the platform adapter. A cold resume starts a new domain, so time never runs
  backwards inside one domain (R2-05). The evaluator only takes differences of display times from one
  session. Feeding it into posn timestamps and double-click detection (GNU uses the display's `Time`,
  `gnu:src/keyboard.c:6088-6093,6586-6594` [G]) changes values that are `0` today
  (`EV/keyboard.rs:6596-6606` [M]), so it lands only with an attested oracle comparison (§15).
- **Deadlines are relative;** the authoritative deadline is the waiter's Lisp variable.
- **RTT** is measured by each side on its own clock (`Ping{nonce}`/`Pong{nonce}`); clocks are never
  subtracted across hosts (S15 [M]). RTT drives chunk size, the byte window, coverage depth and prediction
  demand.
- **Layout scale** is stamped on every presentation. On a mismatch with the window scale the frontend
  draws at exact logical positions, marks bitmap-strike runs stale, and sends `WindowGeometry`, which
  triggers a relayout within one RTT (DQ9 option c [M]).
- The session crate takes an injected `Clock`, so heartbeat and pacing tests are deterministic.

### 6.16 Identity namespaces

| Id | Allocator | Scope | Replaces |
|---|---|---|---|
| `SessionId`, `ChannelId`, `RequestId`, `UpdateSeq`, `RowSlot`, `FontSlot` | evaluator | session | reply `Sender`s; `RowAppearance`; per-service `ResolvedFontId` |
| `TerminalNumber` | evaluator (`next_terminal_id`) | process | terminal 0 re-described in place (E2 [M]) |
| `LiveDisplayFrameId(NonZeroU64)` | evaluator (Lisp frame id) | process | the frame-0 sentinel (D40) |
| `HostReplyId` | evaluator | process | the reply id the wait machinery sees (§6.13) |
| `FrontendIncarnation` | frontend (random per process start) | frontend process | — (R2-05) |
| `ConvertId` | frontend | session | — (selection conversion, D15) |
| `PresentationId`, `CoveragePublicationId` | evaluator | process, carried by value | `Arc::ptr_eq` admission |
| `AssetId::Content` | SHA-256 | global | font paths, inline font bytes |
| `AssetId::Session`, `ImageId`, `VideoId`, `SurfaceId`, `WebViewId` | evaluator only | process | the frontend's `ImageId` allocator and numeric range partitions (D4 [M]) |
| `MenuToken`, `TooltipTicket` | evaluator | session | process counters and `Arc` atomics |
| `MenuBarRequestId`, `InputStreamId` stream serials | frontend (streams are allocated in `Welcome`, one per incarnation) | session | process counters (I15, S11) |
| `ResumeToken` | evaluator (CSPRNG) | session | — |

Font-entity handles stay process-global on the evaluator: the catalog is the Lisp host's one catalog
(C102), so they cannot alias across terminals (P1 §5.2).

### 6.17 Capabilities (v1 registry)

| Capability | Params | Notes |
|---|---|---|
| `frames/1` | — | mandatory |
| `images/1` | pixel formats (`rgba8`, `gray8`, `mask1`), max dimension | mandatory for graphical displays (`display-images-p` is t on graphic displays) |
| `fonts/1` | containers (sfnt, collection, pcf, bdf, woff, woff2), rasterizer and version | mandatory |
| `selections/1` | `clipboard`, `primary`; per selection `ownership: exact │ sequence │ none` | `exact`: X11 `SelectionClear` with XFixes owner notification, Wayland `data_device.selection` while focused. `sequence`: macOS `changeCount` or the Windows clipboard sequence number stamped on input edges. `none`: always ask (today's behaviour, R2-12) |
| `popup-menus/1`, `tooltips/1`, `child-frames/1` | — | |
| `open-uri/1` | schemes the frontend policy allows | §11.3 |
| `file-drop/1` | — | URIs `file://HOST/…` |
| `ime/1` | `preedit: local` | |
| `shader-surface/1`, `frame-shader/1` | `SurfaceContract` versions, naga level | absent on headless frontends; on `Dialed` links advertised only when the frontend's per-host policy allows GPU programs (default deny). The frontend validates every program with naga before compiling it, whatever the evaluator validated (R2-15) |
| `video/1`, `webview/1` | sources and engine | advertised only on `InProcess` and `SpawnedChild` links in v1 |
| `terminal-emulator/1` | — | same-host only until neo-term is re-homed |
| `prediction/1` | kinds: `pixel-scroll`; later `echo`, `motion` | |
| `compression/1` | `deflate` | `Dialed` links only |
| `resume/1` | max grace | `Dialed` links only |
| `headless/1` | — | test and agent frontends: no GPU, no native window |
| `diagnostics/1`, `debug/1` | — | |

Unknown names are ignored; the agreed set is the intersection at the lower version and is announced in
`Welcome`. Later changes arrive as `Capabilities{version}` facts (for example after device loss).

### 6.18 Limits (checked before allocation)

| Limit | Default | Hard cap |
|---|---|---|
| record before `Welcome` | 64 KiB | 64 KiB |
| `max_record` | 1 MiB | 16 MiB |
| `max_message` (reassembled) | 8 MiB | 64 MiB |
| glyphs per row area | 4096 | 65535 |
| windows per frame | 1024 | — |
| `store_rows` / `store_bytes` | 16384 / 64 MiB | frontend-chosen |
| pending requests per session | 1024 | — |
| unconsumed input edges (frontend) | 4096 (quit edges exempt) | — |
| relay preamble (`RelayRequest`) | 64 KiB | 64 KiB |
| ssh stdout scanned for the relay's preface | 64 KiB | 64 KiB |
| concurrent handshakes per listener | 16 | — |
| handshake or resume deadline | 10 s | — |
| inline selection data | 64 KiB | 1 MiB |
| presentations retained per frame on the evaluator | 4: two in flight, the active one, one kept for `ScrollProjection`; beyond that the oldest is retired unilaterally (R2-28) | — |

### 6.19 Liveness, detach and resume

- **One liveness timeout for both sides (R2-04).** `keepalive_timeout` (default 10 s, from
  `neomacs-display-keepalive-timeout` through an evaluator-published snapshot) is announced in `Welcome`.
  Both sides use it. Each side pings after 1 s of send silence. I/O threads answer pings, so a busy
  evaluator never looks dead. v1's frontend gave up after 3 missed pongs while the evaluator waited 10 s,
  so a roaming frontend's first `Resume` met a session still marked connected.
- **Frontend states.**

  | State | Entered when | Input | Transport |
  |---|---|---|---|
  | `Connected` | handshake or resume done | sent | — |
  | `Stalled` | 3 s without a pong, no EOF | **queued** in the replay buffer within the budget (5 s / 256 events), with a "Waiting for devbox…" band | the old transport is kept; after 2 s more a replacement is started in parallel. A pong on the old link returns to `Connected` and cancels the replacement; a replacement whose `Resume` completes wins |
  | `Reconnecting` | EOF, a transport error, or `keepalive_timeout` with no traffic | rings the bell and is dropped (U7) | retries with backoff 0, 1, 2, 4, 8, then every 15 s, never prompting (§8.2) |
  | `Expired` | the evaluator answers `ResumeExpired` or `ResumeUnknown` | — | "Session expired"; a new session is offered |

- **Detach** (evaluator, at a safe point, never inside an unrelated command):
  1. The connection state becomes `Detached{since}`.
  2. **The evaluator closes its end of the transport**, so the relay sees EOF and exits, and so does the
     sshd child. v1 left them lingering until TCP gave up: `tcp_retries2` is 15 and
     `tcp_keepalive_time` is 7200 s on this host (R2-04).
  3. In-flight presentations get a synthesized `Retired` (u2 §3.8 [M]), but the last *activated*
     presentation stays resolvable until the first activation after resume. A click on the dimmed frame
     right after resume therefore resolves (R2-23).
  4. Pending requests fail with `neomacs-display-disconnected`.
  5. The terminal is skipped in redisplay.
  6. **The selected frame moves off the terminal and its KBOARD is released** (§12.2, R2-09).
  7. The detached-state table (§12.2) applies.
  8. `neomacs-display-detach-functions` run.
- **Resume** (a listener task; no Lisp): validate `Resume{session, token}` against the current or the
  previous token (two-phase, §6.5). A session still marked connected is taken over implicitly. A changed
  `incarnation` makes it a cold resume (§6.5). Then rebind the byte stream and, for a warm resume, apply
  per lane:

  | Lane | What happens |
  |---|---|
  | control | the evaluator re-sends every keyed state record that differs from what the frontend acknowledged (state synchronization makes this a diff, not a log replay; D32). Correlated requests still pending were failed at detach |
  | input | `Welcome` reports `received_through`, the highest serial the evaluator has received on the stream. The frontend replays only serials above it, within the budget. The evaluator drops any serial at or below it, so delivery is exactly once even when a replay races a late original (R2-05). Input received before the detach stays queued and is consumed normally; GNU never discards stored input for a live terminal. Older input than the budget is discarded with a visible notice and `InputOverflow` |
  | present | journal-based delta, or `StoreReset` plus a keyframe by reference (§6.10) |
  | facts | the frontend resends its latest values; the evaluator replays config |
  | hint | dropped |
  | bulk | reconciled per offer (`Have`/`Want`) |

  At the next safe point the evaluator marks the terminal connected and restores each frame's
  **desired** visibility. That is the value Lisp last set, kept separately from the detached flag, so a
  `make-frame-visible` or `make-frame-invisible` during detach is honoured rather than overwritten
  (R2-23). It then runs `neomacs-display-resume-functions`.
- **Hook pairing (R2-23).** Detach and resume transitions are queued in order and processed at safe
  points. A resume that arrives before the detach hooks have run still runs the detach hooks first, so
  hooks always alternate and Lisp never sees two resumes in a row.
- **Grace expiry** deletes the terminal exactly like a tty hangup: `delete_input_terminal_noelisp`
  (`EV/keyboard.rs:3711-3736` [J3·V]), hooks deferred and run LIFO on dead objects (C39). No error is
  injected into a running command; GNU's X `error` exists only because Xlib needs a non-local exit
  (u8 R7 [M]).
- **Takeover.** A valid token never needs `--takeover` (§6.5). A token-less `Offer` for a label another
  session holds is refused `LabelInUse{state}`. With `takeover` set, it adopts that terminal as a cold
  resume, and the old link, if any, gets `Goodbye{TakenOver}`.
- **What this host cannot exercise** (R2-20): a real sshd, half-open TCP below the application layer, and
  systemd's logout behaviour. `LinkEmulator` stalls without EOF and cuts after a chosen record, so the
  state machine above is testable here; the rest is CI or VM work (MILESTONES M12).

---

## 7. Evaluator-side architecture

### 7.1 The terminal registry

Replace the single slot `Context.display_host: Option<Box<dyn DisplayHost>>` (`EV/mod.rs:3775-3776` [V])
with a **main-owned registry** that occupies the same slot, outside World-carried state:

```rust
pub struct TerminalNumber(u64);          // the Emacs terminal id; NOT protocol::TerminalId (neo-term's)
pub enum TerminalBacking {               // one exhaustive enum over every terminal kind (D12)
    Initial,                             // F1 on initial_terminal
    Termcap(Box<dyn TtyHost>),           // today's TerminalHost + TtyRif sink, plus, on the primary
                                         // TTY, the popup and selection half (TtyPopupDisplayHost)
    WindowSystem(Box<dyn DisplayConnection>),   // LinkConnection for every window-system display
}
pub trait DisplayConnection {            // per connection: G1, G3, G4, G5, G8, G10, G11 of u4 §2.1
    fn identity(&self) -> &GraphicalDisplayIdentity;
    fn facts(&self) -> &DisplayFacts;    // replica: monitors, desktop, resources, capabilities
    fn state(&self) -> ConnectionState;  // Connected | Detached { since } | Closed { reason }
    fn window_op(&mut self, frame: LiveDisplayFrameId, op: WindowOp) -> Result<(), HostError>;
    fn submit(&mut self, p: SealedFramePresentation) -> SubmitOutcome;   // never parks
    fn request(&mut self, body: RequestBody) -> PendingReply;            // PendingReply holds a HostReplyId
    fn selection_op(&mut self, op: SelectionOp) -> Result<(), HostError>;
    fn media_op(&mut self, op: MediaOp) -> Result<MediaOutcome, CapabilityRefusal>;
    fn config(&mut self, op: ConfigOp);
    fn close(&mut self, reason: CloseReason);
}
pub struct DisplayServices { /* per process: font catalog and entities (G6), image catalog and
                                decoder (G7), shader validation; never crosses a link */ }
```

- **Routing.** Frame-addressed calls route `frame → Frame.terminal_id → registry` (frames already carry
  `terminal_id`, u4 [M]). Frame-less calls follow `check_x_display_info(nil)`: the selected frame's
  terminal if it is `neo`, else the most recently opened `neo` terminal, else GNU's error
  (`gnu:src/xfns.c:168-205` [G]).
- **Scale.** The 145 non-test `.display_host` accesses in 37 files (J2 §1.3 W1; re-counted [V]) migrate
  by u4's five call-site classes. 23 of them are in 6 layout-engine files, not "6 accesses" as v1 said
  (R1 F20 [V]). The 62 test-only implementors migrate through a mechanical adapter (P1 §11 item 8).
- **Two production hosts exist today, not one (R1 F1).**
  - `PrimaryWindowDisplayHost`: `NM/main.rs:1862`, installed at `:4717` [V].
  - `TtyPopupDisplayHost`: `NM/tty_frontend.rs:30,40` [V]. It is installed for every non-daemon
    interactive TTY (`NM/main.rs:5477-5483`; `tty_init::should_enable_live_tty_io`,
    `NM/tty_init.rs:566-568` [V]). It draws popup menus to stdout (`:69`) and returns Lisp-visible
    selection errors ("system clipboard is unavailable in TTY mode", `:50-66`).

  The TTY host becomes part of the primary termcap terminal's `TtyHost`. Popup menus are a termcap
  terminal operation (GNU `tty_menu_show`), and its selection refusals stay typed per terminal. Daemon
  `-t` clients never install it, so `daemon_lifecycle` cannot catch a regression there; the `-nw` TUI
  seam oracle does (MILESTONES M0, M3).
- **"Is there a GUI" sites.** Because this host occupies `Context.display_host` under `-nw`, the sites
  that test `display_host.is_some()` answer as if a GUI existed. For example,
  `neomacs-surface-available-p` returns t (`ND/shader_surface/mod.rs:350-353` [V]), and the image entry
  points skip their no-display error (`ND/image/mod.rs:1224-1229,1413` [V]). M3 keeps those answers
  exactly, through a registry predicate equivalent to today's `is_some()`. M5 (Lisp-visible) switches
  class 3 to the frame's terminal kind and lists each changed answer against GNU (D37).
- **The in-tree host-effect rule (R1 F19).** `host-effect-projections.md:72-78` [V] requires native
  backing to stay on main, selected "using existing storage; there is no new TLS word, inline
  Context/World field or hot owner check in this range". It does not require a registry; v1 paraphrased
  it as if it did. `DisplayTerminals` occupies the existing `Context.display_host` slot (its type
  changes; no field is added), and the TTY host boxes leave the World-carried `TerminalManager` TLS
  (`ND/terminal/pure.rs:249,1809-1839` [M]) for it. Whether a changed slot type counts as "existing
  storage" is for that document's owner to rule. M3's PR quotes the sentence and asks.

### 7.2 GNU terminal model

| Today | Change | GNU contract |
|---|---|---|
| `ensure_initial_terminal` revives deleted record 0 (`ND/terminal/pure.rs:330-348` [V]); `ensure_terminal` revives any id (`:378-391` [J3·V]) | ids come from a process-wide monotonic counter, as GNU's `next_terminal_id`; deleted records are dropped, so nothing accumulates over a daemon's life (today the next id is the maximum over retained records, `ND/terminal/pure.rs:594-605` [V], which is why v1 kept tombstones; R2-28); window-system terminals get their id when a channel opens; initial terminal 0, first display 1 | C1; `gnu:src/terminal.c:292-323` [G] |
| the GUI terminal is id 0 re-described in place (`:412-426` [M]); every `x-create-frame` frame goes to terminal 0 (`NW/display_ext.rs:2190-2198` [V]) | frames are created on the resolved terminal (`create_frame_value_on_terminal`) | C16, C20 |
| `terminal-live-p` consults frames first (`:1372-1411` [J3·V]); X11 terminals answer `x` while their frames answer `neo` | answers from the record only; `neo` for every window-system terminal on every platform; the platform stays observable through `neomacs-display-backend` | C4; `gnu:src/terminal.c:443-480` [G] |
| `delete_terminal_owned` runs hooks, host delete, frames, then the liveness flip (`:1700-1760` [J3·V]) | liveness flips first, then frames `noelisp`, unlink, release the KBOARD | C3/C37; `gnu:src/terminal.c:330-357` [G] |
| `x-close-connection` always errors (`ND/display/mod.rs:3447-3483` [M]) | "Display still has frames on it" while frames remain, otherwise close | C44; `gnu:src/xfns.c:7674-7690` [G] |
| `x-display-*` accept only nil or a frame (`ND/display/mod.rs:445-473` [M]); monitors ignore DISPLAY | `check_x_display_info` designators (nil, terminal, name, frame) everywhere | C43, C46 |
| a headless daemon has no `neo` entry in `display-format-alist` [RB 04d] | **Preload `term/neo-win` in every mode and provide `neomacs` at dump time**, as GNU X builds preload x-win (`gnu:lisp/loadup.el:304-309` [G·V]). `neo-win` errors unless `(featurep 'neomacs)` (`lisp/term/neo-win.el:30-32` [V]), and `neomacs` is provided only when the GUI layer loads (`NM/main.rs:4300` [V]), so the provide moves into the dump first. This **reverses an in-tree decision**: "The full `term/neo-win` layer remains a GUI-runtime concern and is therefore intentionally not loaded into the portable dump" (`lisp/term/neo-preload.el:9-11` [V]; also `lisp/loadup.el:264-267` [V]). The reason is C19 and C105: a headless daemon must know `neo` displays. The audit covers every top-level effect (`neo-win.el:30-32, 40, 200-201, 233, 584, 586, 589, 603, 642-646, 756` [V]; v1 listed five, R1 F18). Keep what GNU's window-system files also do at load; the `(".*" . neo)` entry of `display-format-alist` has GNU's pgtk precedent (`gnu:lisp/term/pgtk-win.el:112` [G·V]). Move the rest into `window-system-initialization`: the `s-=`/`s-+`/`s--`/`s-0` bindings, `(require 'dnd)`, `(require 'neomacs-effects)` and the darwin modifier setup. Run `neomacs--window-setup` on the first `neo` open in a daemon (P3 §5.1; J3 A.2 item 13). M5's Design horizon cites and justifies the reversal | C19, C105 |

### 7.3 Opening displays

`DisplayConnectionFactory` generalizes `TtyFrameHostFactory` (`EV/protocol/tty_host.rs:157-174` [M]) and
is installed in every interactive mode, headless daemons included.

| Display name | Grammar | Transport |
|---|---|---|
| `neo:LABEL` | `LABEL` matches `[A-Za-z][A-Za-z0-9._@+-]{0,62}`. It starts with a letter and contains no colon, so the name never ends in `:DIGITS` or `:DIGITS.DIGITS` and can never match X's display regexp (`gnu:lisp/term/x-win.el:1347` [G]) | claim the listener's offer; open a channel |
| the startup native name in a non-daemon GUI run | `:0`, `wayland-0` | in process; the terminal is created when `window-system-initialization` first opens it (GNU order) |
| any other native name, and every native name in a daemon | X11 or Wayland names | spawn `/proc/self/exe` (Linux; the startup path on macOS) as `--frontend=gpu --inherit-fd N --display NAME` over a socketpair (MILESTONES M13; before that, GNU's open error) |
| anything else | — | `(error "Display %s can’t be opened")` |

- **Lisp-visible name = the exact string Lisp used** (C5). `neo+…`-style URIs are not used.
- **Opening is a display-event-style wait** (§6.13): no timer or process filter runs while a display
  opens, as with GNU's synchronous `x_term_init`. A timer therefore cannot open the same name again,
  delete the terminal being opened, or redisplay it half-open (R2-08).
- **`x-create-frame`** resolves `terminal`, then `display`, then the default, as `check_x_display_info`
  does; errors are GNU's ("Terminal is not live, can't create new frames on it", `gnu:src/xfns.c:4960`
  [G]). The silent fallback in which `make-frame-on-display ":1"` lands on the existing connection (E4,
  u4 S16 [M]) is removed.
- **Neomacs extension APIs** (§10) give typed reasons; GNU APIs keep GNU's exact errors.

### 7.4 Redisplay over every terminal

- `redisplay_fn` walks the registry in terminal order, as GNU's `redisplay_internal` does
  (`gnu:src/xdisp.c:17849-17907` [G]): termcap terminals render their top frame through their `TtyRif`;
  window-system terminals lay out every visible native-window root **on that terminal**
  (`RenderFrameScope::NativeWindowTreesOn(terminal)` beside `AllNativeWindowTrees`, `NW/display_ext.rs:2607-2633`
  [M]) and submit to that terminal's outbox. Detached terminals are skipped. This fixes today's
  "only the selected terminal repaints" for TTY clients (`NM/frame_layout.rs:481-485` [V];
  `NM/secondary_tty.rs:54-71` [J2·V]).
- **Layout runs even without credit**: layout has Lisp-visible effects (window start and end,
  fontification); only transmission coalesces (P4 §5.3).
- **One redisplay runtime with a per-frame `DisplayContext`** (identity, `FontSizing`, render options,
  scale, image-catalog view), switched at most once per terminal per pass; the process-global applied
  render-option memo (`LE/font/metrics.rs:1018-1058` [M]) becomes per context. The font catalog stays
  process-global, as GNU's Fontconfig driver is (DQ34 option b).
- A terminal or frame may vanish mid-pass (C52); liveness is re-checked after every Lisp-running step.
- `neomacs--frame-snapshot` works for every frame on every terminal (it is already installed in headless
  daemons, `NM/frame_layout.rs:486` [V]); it is the GPU-free observation point for tests.

### 7.5 Input routing

- **One link reader per session**, generalizing the input bridge: decode; route replies to the pending
  table, acks to presentation lifecycle and credits, facts to replicas, input to conversion
  (`convert_display_event`, unchanged) and `HostInputPublisher`.
- **Attribution is explicit:** frames are named (non-zero), display-scoped events carry their terminal
  (E6 [M]), and frame 0 is never produced. `handle_display_terminal_disconnect`'s hard-wired terminal and
  the render loop's `WindowClose{0}` (`RT/bootstrap.rs:753-757` [M]) are replaced by
  `DisplayHangup{terminal, reason}`, which reaches the existing `delete_input_terminal_noelisp`.
- **Early quit on the reader** uses an evaluator-published `QuitPolicy{quit_char, current_kboard,
  throw_on_input}` snapshot (an atomic swap inside the evaluator process; nothing crosses the seam). It
  uses the *actual* `quit-char` (today: `requests_default_quit`, `publication.rs:179` [V]).
  - **Non-current KBOARD.** A C-g from a non-current KBOARD fails to quit **only in single-kboard
    state**: GNU's condition is `if (single_kboard && kb != current_kboard)`
    (`gnu:src/keyboard.c:3794` [G·V]). Outside that state any terminal's C-g quits. Before M16 adds
    `single_kboard`, every C-g therefore quits; v1 dropped the condition (R2-24).
  - **`throw-on-input`** raises `quit-flag` when the event is **stored** (`gnu:src/keyboard.c:3867-3871`
    [G·V]; Lisp-visible, UNATTESTED).
  - **Frontend side.** The same policy goes to the frontend as `QuitKeys` config, so quit edges bypass
    the frontend's input bound (D33).
  - **Consumption.** The evaluator publishes its consumption counter; the link writer sends
    `InputProgress` from it (§6.8).
- **Pointer events are resolved here** (D4): the reader publishes `PresentedRegion{presentation, target}`
  without a hit; when the evaluator services it at the head of the queue, where it already checks that the
  presentation is the frame's active one (`EV/keyboard.rs:2096-2111` [V]), it resolves the hit with the
  same `PresentedTextPositionSource` logic over its retained snapshots. `ScrollProjection` resolves against
  the retained coverage; `CurrentGeometry` against the latest geometry. The f32 bit-equality coupling
  between observation and action (`EV/keyboard.rs:7077-7109` [M]) disappears.

### 7.6 Evaluator-side changes by crate

| Crate | Change | Milestone |
|---|---|---|
| `neovm-display-identity` | receives `GuiResizeRequestId` and `FrameVisibility` (from `crates/neovm-value/src/display_vocab.rs:15-20,92-101` [V]), because `neovm-value` cannot see DP; both gain serde derives only. `neovm-value` already depends on the leaf (`crates/neovm-value/Cargo.toml:25` [V]). The existing guards stay green: `crates/neovm-display-identity/tests/dependencies.rs` (the leaf depends only on serde) and `crates/neomacs-display-protocol/tests/identity_impl_guard.rs` (DP never implements a type listed in `MOVED`) (R1 F24 [V]) | M1 |
| `neomacs-display-protocol` | the M1 boundary vocabulary, including what v1 missed (R1 F4 [V]): `FrameFontSize` (`EV/protocol.rs:156`, used at `DR/font_defaults/policy.rs:4-6,84-91`), `ResolvedImageMetadata` (`EV/protocol/image_catalog.rs:221`, used at `RT/state.rs:20`, `RT/media.rs:87`), and, behind DP's existing `neo-term` feature, neo-term's `TerminalId`, `TerminalDisplayTarget`, `TerminalGridSize` and `TerminalFloatPlacement` (`EV/protocol.rs:54-110`, re-exported at `DR/terminal/mod.rs:11-13`); `LiveDisplayFrameId` (D40); then the hit-index split (M8), asset refs (M10), policy types (M14), and the removal of neo-term's types and `FrameGlyph::Terminal` (`DP/frame_glyphs.rs:380-388`, M15) | M1, M8, M10, M14, M15 |
| `neovm-value` | keyboard forwarders for all 14 `DEFVAR_KBOARD` variables (`crates/neovm-value/src/forward/mod.rs:974-990` [M]) | M16 |
| `neovm-eval` | `DisplayTerminals` replacing `display_host`; `HostReplyId` beside `HostResult`; `DisplayConnection` and `DisplayServices`; `DisplayHangup`; terminal-tagged events; `QuitPolicy`; evaluator-side hit resolution; conditions and Lisp API of §10; detached state; KBOARD single-kboard discipline | M3, M5, M6, M8, M9, M12, M16 |
| `neovm-display` | terminal model (§7.2); `x-open-connection`, `x-close-connection`, `x-create-frame`, `x-display-*`, monitor and selection builtins per `check_x_display_info`; `NativeWindowTreesOn`; evaluator-authoritative tip state; `neo/prediction.rs` generalizing `neo/scroll.rs` | M5, M9, M14 |
| `neovm-system` | `CompleteOnHostReply(HostReplyId)` wait policy with the two disciplines of §6.13; the child `DISPLAY` rule and `NEOMACS_DISPLAY` (§11.4); image ledger plumbing | M6, M7, M10 |
| `neovm-core` | command-loop frontier capture; KBOARD scopes; guards (`host_effect_guard_test`) | M1, M6, M16 |
| `neomacs-layout-engine` | stamps `layout_scale`; frontier capture before layout with an after-layout re-check; per-frame `DisplayContext`; `FontRasterPolicy` where metric policy is computed (`LE/font/metrics.rs:2081-2098` [P1]); interaction-policy facts | M1, M5, M10, M14 |
| **`neomacs-display-host`** (new; evaluator side) | registry glue; `LinkConnection` over sessions; outbox; link reader and writer; in-process, socketpair and Unix transports (`cfg(unix)` arms); display listener; factory; the redisplay runtime moved from the display runtime (`DR/redisplay.rs`, `redisplay_step.rs`, `redisplay/maintenance.rs` [V rg]). `tty_input.rs` stays in the runtime until M13: it publishes through `RenderComms`, and only its tests name `neovm_*` (D43). Also the Linux `desktop-font-settings` forward (D45), and the contract tests that pin the evaluator's half of split tests; a dev-only runtime edge serves its integration tests (D44) | M4, M6, M7, M13 |
| **`neomacs-tty-render`** (new) | `DR/backend/tty/` with the production `neovm_core` uses passed in one `TtyRasterPolicy`: `TerminalCharWidths` for `char_width` (`rif.rs:2732,3826-3831`), and its own `TextFillEdge` for `posn_object_extent_mode` (`:2797` [V], D47). The tests that consult evaluator oracles (`char_width`, `posn_object_extent_mode`, `automatic_composition_for_terminal` at `rif/tests/packed_raster_test.rs:372,429,766`, `rif/tests/rif_test.rs:2847,3033`, `damage/tests/damage_test.rs:218` [V]) cannot leave the crate: the reference rasterizer is an inherent `impl TtyRif`, and the tests read private grids. They stay and run on `testing` fixtures (a GNU width table that panics on unlisted scalars; literal terminal compositions). Display-host contract tests pin those fixtures to the evaluator (D44). No case is weakened (R1 F12) | M4 |
| **`neomacs-media-cpu`** (new) | CPU image decode, probe, SVG, XPM, XBM, sequences and scaling, plus naga validation with `SurfaceContract`. The sources are `RW/image_cache/decoder.rs`, and `RW/image_probe.rs`, `RW/svg.rs`, `RW/xpm.rs`, `RW/xbm.rs` at the renderer root (2,076 lines, no `wgpu::` [V]), plus `RW/image_sequence.rs` (656 lines, called from `image_probe.rs:118,127` [V]). `decoder.rs` starts with `use super::*;` over `RW/image_cache.rs` (2,806 lines, 64 `wgpu::` references [V]), so the CPU items it uses are split out of that module first (R1 F28) | M10 |
| `neomacs-font-materializer` | receives the renderer's layout-engine helpers in M4 (R1 F13 [V]): `font::subpixel` with its Fontconfig query (`LE/font/subpixel.rs:27` delegates to `LE/font/fontconfig.rs:1857-1880`, whose module imports `neovm_core` for unrelated items); `composition::representative_char_for_cluster` (`LE/composition.rs:116`); and `font_match::{select_cosmic_family, CosmicFamilySelection, resolve_weight_in_family}` with `probe::named_instance_wght_values` (`LE/font/font_match.rs:202`, `LE/font/probe.rs:399,692`). It already links `yeslogic-fontconfig-sys`. Byte-based loading follows in M10. It enters CI in M10: today the workspace test archive excludes it (`.github/workflows/test-suite.yml:128` [V]) (R1 F15) | M4, M10 |
| `neomacs` (bins) | `PrimaryWindowDisplayHost` dissolves into the in-process `LinkConnection` plus `DisplayServices`; `TtyPopupDisplayHost` folds into the primary termcap backing (M3); `--frontend=…` role dispatched before any Lisp initialization (precedent: `runtime_mode_from_program_name`, `NM/main.rs:285-296` [P1]); `--remote`; daemon starts the listener; `neomacsclient --display-relay` and the socket-directory check | M7, M11, M13 |
| Lisp | `lisp/loadup.el` preloads `term/neo-win`; `neo-win.el` selection methods pass their terminal (`gui-backend-get-selection` already takes `_terminal`, `lisp/term/neo-win.el:456-466` [V]); `browse-url.el` `neo` branch (§11.4) | M5, M9 |

---

## 8. Frontend-side architecture

### 8.1 Crates and the dependency graph

```
neovm-display-identity                    (serde-only leaf)
        ▲
neomacs-display-protocol (DP)             vocabulary; no I/O, no threads; depends only on the leaf
   ▲        ▲          ▲            ▲
   │  neomacs-display-wire  neomacs-tty-render   neomacs-media-cpu     (pure; DP only)
   │        ▲                    ▲                   ▲
   │        │      neomacs-session (NO workspace dependencies; Clock injected)
   │        │        ▲     ▲
neomacs-layout-engine  neomacs-renderer-wgpu (DP + font-materializer; NO layout engine)
   ▲                        ▲
   │     neomacs-display-runtime (frontend runtime: RenderApp, compositor; DP + renderer;
   │                          NO neovm-*, NO layout engine, NO wire, NO session;
   │                          defines the FrontendEndpoint trait)
   │                        ▲
   │     neomacs-frontend (lib + bin: FrontendSession over session + wire; presenters;
   │                       in-process endpoint; launcher; reconnect)        [no neovm-* in closure]
   │
neomacs-display-host (evaluator side: neovm-core, layout engine, DP, wire, session, media-cpu, tty-render)
   ▲
neomacs (bins: neomacs = evaluator + in-process GUI (display-host + frontend lib) + --frontend role;
         neomacsclient = server.el client + --display-relay)
```

The layout doc's rules hold: DP depends only on the identity leaf; a type shared by runtime and renderer
lives in DP; the renderer never calls the runtime and never sees the link (`display-crate-layout.md:11-31`
[V]). **Amendments** to that document (MILESTONES M2): line 14 says DP "depends on nothing in the
workspace", but it depends on `neovm-display-identity` (`DP` `Cargo.toml:15` [V]); add charters for the
new crates; state that stream I/O never enters DP; record that crate boundaries changed by design decision
(`:142-147` [V]). `scripts/ci/check-crate-graph.py` enforces the graph.

**Dev-only edge (M4, D44).** `neomacs-display-host`'s tests may use the runtime's public API and its
`testing` feature (the headless entry M11b extends). Its normal edges stay as drawn, and
`check-crate-graph.py` pins them.

**Endpoint inversion (P3 §11.1).** The runtime defines `FrontendEndpoint`: typed inbound DP messages
(sealed presentations, window ops, config, UI ops, replies) and typed outbound input. It depends on neither
the wire nor the session crate. `neomacs-frontend` implements it over a session; the in-process GUI uses
the same `FrontendSession` with the in-process transport. `RenderApp`'s command handlers stay; they take
decoded messages instead of `RenderCommand`.

### 8.2 The frontend process

- `neomacs-frontend` (lean binary; a `cargo metadata` guard asserts its closure has no `neovm-*`) and
  `neomacs --frontend=…` (the same library entry, `neomacs_frontend::run`, inside the full binary) are the
  two ways to run the frontend role. The daemon spawns its own executable, so spawning is deterministic.
- Options: `--remote [USER@]HOST [--label L] [-s NAME] [--ssh-program P] [--remote-command CMD]
  [--sync-fonts FAMILY…] [-- FILES]`, `--connect PATH` (local dial-in), `--inherit-fd N --display NAME`
  (spawned), `--presenter gpu|headless`, `--record DIR` (headless snapshots), `--clipboard rw|w|r|none`,
  `--takeover`, `--askpass PROMPT` (internal, §8.2 ssh integration), and `--link-emulator SEED:PROFILE`
  (testing builds only).
- **Presenters.** `GpuPresenter` is today's `RenderApp` (winit, wgpu, compositor, menus, IME, clipboard
  worker), driven by `FrontendSession` instead of `RenderComms`. It reaches links first through M11b's
  frontend-side bridge, which plays the evaluator's half of `ThreadComms` for an unmodified `RenderApp`
  (D39). M13 replaces the bridge with the typed `FrontendEndpoint` and deletes it. M11b also adds a
  public headless `RenderApp` constructor (behind a `testing` feature) that spawns `FramePreparation`
  without winit. None exists today: `RenderApp` and `RenderApp::new` are `pub(super)` (`RT/state.rs:978,1202`
  [V]), and the materializing worker is spawned only in the winit startup closure (`RT/bootstrap.rs:713-736`
  [V]). M13's parity test and M14's feel tests run on it (R1 F6). `HeadlessPresenter` has no winit and no
  GPU: virtual windows sized by `Create` and `Resize`; decode, apply, seal, validate and `materialize()` on
  the CPU; `Activated` acks; records `render_text()` (`DP/snapshot_text.rs:47` [V]) and JSON; scripted input
  through a control channel. It is the frontend this host can run, a product feature for agents, and the
  vehicle for every GPU-free acceptance test.
- **Startup.** The native loop builds, observes monitors, scale and desktop settings, sends
  `SessionHello`, and may show one provisional window; the first `Create{adopt_provisional: true}` binds
  it (D22). This replaces `AdoptPrimaryFrame`, `FrameRef::Primary`, `primary_window_size` and frame-0
  routing (C5, S13, I24 [M]).
- **Fonts from bytes**: outline fonts through fontdb's binary source, bitmap and WOFF fonts through
  byte-based loaders (u5 §3.2 A [M]); `FontRasterPolicy` from the evaluator; only surface facts (device
  scale, subpixel geometry) are local. A catalog generation no longer rescans the local catalog. IME
  pre-edit uses the frame's default-face font asset instead of the cosmic `Monospace` generic.
- **Images from pixel assets**: the frontend never opens a path (`ImageLoadFile`, tool-bar icons by path
  and `Face.font_file_path` priming all go, C9, F11 [M]).
- **Input outbound queue**: non-blocking; serials and `DisplayTime` on every user input; a replay buffer of
  unacknowledged input for resume.
- **Disconnected UI**: the status band, the state machine of §6.19, backoff, ssh relaunch, token storage
  in the platform state directory (`$XDG_STATE_HOME/neomacs/sessions/`, 0600, on Linux;
  `~/Library/Application Support/Neomacs/sessions` on macOS; `%LOCALAPPDATA%\Neomacs\sessions` with a
  user-only ACL on Windows; the locked `dirs` crate, `Cargo.lock:1643` [V]; R2-35).
- **ssh integration (R2-14).**
  - *Fixed command.* The remote command is fixed (§3.2). `--remote-command CMD` replaces only the
    program path, which is validated locally and never comes from file names.
  - *Retries.* Automatic retries pass `-o BatchMode=yes -o ControlPath=none -o ServerAliveInterval=5
    -o ServerAliveCountMax=2`. They never prompt, and never multiplex onto a `ControlPersist` master that
    outlived a network change.
  - *Prompts.* The first attempt inherits the frontend's terminal when it has one. Otherwise it sets
    `SSH_ASKPASS_REQUIRE=force` and `SSH_ASKPASS` to the frontend's own `--askpass` mode, which shows
    the prompt in the provisional window. Interactive authentication is never retried automatically.
  - *rc-file noise.* The frontend scans at most 64 KiB of ssh's stdout for the relay's preface and
    shows any skipped bytes.
  - *Missing command.* Exit status 127 reports `remote-command-not-found` with the remedy. The frontend
    makes one visible retry through the fixed `sh -lc 'exec neomacsclient --display-relay'`, because a
    user-installed `neomacsclient` in `~/.local/bin` is on `PATH` only for login shells on stock Ubuntu
    (`/etc/skel/.profile:19-26`; `.bashrc` returns early when non-interactive, `/etc/skel/.bashrc:5-9`
    [V]).
- **Default label (R2-35).** The short hostname, lowercased. Every character outside the label grammar
  becomes `-`; an `h` is prepended when it does not start with a letter; it is truncated to 63
  characters.
- **Frontend chrome (R2-30).** The connection band and the provisional window's text are frontend chrome.
  The frontend draws them with its own font stack, outside every Emacs frame's content. They are never
  Lisp-visible, so the rule that text inside frames is laid out on the Lisp host does not apply to them.
- **Robustness**: nothing from the peer can panic the frontend: bounded decoding, fallible re-seal, no
  `expect` on peer data; fuzz-style corpus tests in L1.

### 8.3 What stays frontend-local (and therefore instant)

| Behaviour | Today | Remote treatment |
|---|---|---|
| cursor blink | render-owned (`lisp/term/neo-win.el:332-367` [P3]) | unchanged; zero traffic |
| cursor motion and size transitions, click halo, smooth scroll, reflow, pane and theme continuity | compositor, sampled motion (`RT/frame_compositor/*` [P3]) | unchanged; robust to jitter because motion is sampled |
| pixel-scroll prediction over certified coverage | `RT/frame_compositor/input_scroll.rs:42-127` [P3] | generalized; coverage depth sized by RTT |
| hover, mouse-face, pointer shapes | presented pointer map | unchanged; the appearance map travels with rows |
| chrome hit tests (menu, tool and tab bars) | frontend | unchanged; tool-bar clicks become presentation-qualified (I16) |
| popup-menu navigation once open; tooltip placement | frontend | opening costs one RTT; navigation local |
| IME pre-edit | frontend | unchanged; commits are input |
| native resize, move, fullscreen | frontend | provisional crop while relayout is in flight (§13.4) |
| GPU device-loss recovery | frontend, then `DisplayReset` | local from the mirror and the asset cache |
| desktop settings observer | evaluator's process today | frontend |

---

## 9. Fonts, images, media, webview, neo-term and shaders

**General rules [I]:** the evaluator is authoritative for every Lisp-visible media fact; no host path
crosses a link to be opened; anything that spawns processes or touches the filesystem or network on Lisp's
behalf runs on the evaluator host; capabilities are per display; an unavailable feature is a typed,
observable error, never `Ok(None)` or a silent no-op. The silent defaults in `EV/protocol.rs:676-749` [M]
are deleted for every display, local included.

| Feature | `InProcess` | `SpawnedChild` | `Dialed` (remote) | Lisp observes | Milestone |
|---|---|---|---|---|---|
| Text, faces, fringes, cursors, chrome | ✔ | ✔ | ✔ | — | M7 |
| Fonts | bytes by `Arc` | ✔ | ✔ (first use transfers bytes; typed pending until they land) | font queries unchanged (Lisp-host catalog, C102); `neomacs-display-info` lists pending assets | M10 |
| Images (PNG, JPEG, GIF, WebP, TIFF, BMP, PBM; SVG, XPM, XBM always on the evaluator) | ✔ | ✔ | ✔ (evaluator-decoded pixels, deflated) | `display-images-p` t; `image-size` local | M10 |
| Tool-bar icons (through the catalog, evaluator `ImageId`) | ✔ | ✔ | ✔ | — | M10 |
| Popup menus, dialogs, tooltips (labels laid out on the Lisp host) | ✔ | ✔ | ✔ | — | M9 |
| Clipboard and primary | ✔ | ✔ | ✔ subject to frontend policy | GNU errors and timeout text; `neomacs-display-policy-refused` | M9 |
| Shader surfaces, frame shader | ✔ if a GPU | ✔ if a GPU | ✔ only if the frontend's per-host policy allows GPU programs (default deny) and it advertises `shader-surface/1` with a matching contract; the frontend validates with naga itself (R2-15) | `neomacs-surface-available-p` takes an optional display; typed error otherwise | M9 |
| Video | ✔ (file source) | ✔ | ✘ in v1 | `(neomacs-display-capability-unavailable video T requires-same-host)` | horizon |
| Webview / xwidget | ✔ | ✔ | ✘ in v1 | same, `webview` | horizon |
| neo-term | ✔ (PTY on the display host, which is the same host) | ✔ | ✘ until re-homed | same, `terminal-emulator` | M15 re-homes it for every display |
| IME | pre-edit local | same | same | committed text as input; `preedit-text` later (DQ27) | — |
| File drop | URIs | URIs | `file://LAPTOP/…` | GNU's `dnd.el` rejects foreign hosts (`gnu:lisp/dnd.el:279-299` [G]) | M9 |
| `play-sound` | evaluator host | evaluator host | evaluator host (GNU X-forwarding semantics) | documented | — |
| Desktop integration: open URI | — | — | `neomacs-display-open-uri` through frontend policy | §11.4 | M9 |

**Fonts in more detail.** u5 option A: content-addressed bytes plus a shipped raster policy. The path
stays Lisp-side metadata in `ResolvedFontIdentity`, for `font-info`, so it still names a Lisp-host path
as GNU's would. For headless servers without the user's fonts, `neomacs --remote HOST --sync-fonts
FAMILY…` copies the selected local font files into `$XDG_DATA_HOME/neomacs/fonts/synced/` on the remote.
That is a Neomacs-private directory, which the daemon adds to **its own** Fontconfig configuration as an
application font directory (`FcConfigAppFontAddDir`), so the Lisp-host catalog sees the fonts
(GNU-faithful C102, P4 §7.1) while other applications on the remote do not. v1 wrote into
`~/.local/share/fonts`, which changes every application's fonts (R2-29). Content addressing and per-offer
`Have` answers mean those bytes never travel back. Licensing of shipping a user's own
font bytes to the user's own frontend is a user decision (§17).

**Images in more detail.** Pixels by default, content-addressed by the hash of the pixel buffer and
persistently cached by the frontend (§6.11). M14's report measures page turns, zoom and animation in
bytes (R2-18). Encoded bytes (`image-codec-parity`) are horizon: they would
put a decoder on the user's machine in front of bytes a compromised remote chose, so they wait for
sandboxed frontend decoding (J3 graft 6). `image-cache-size` becomes the evaluator's nominal ledger, as in
GNU "to avoid having to query the server" (`gnu:src/image.c:2484-2494` [G]); the value change is
Lisp-visible and UNATTESTED.

**neo-term.** Re-homed (X1): the shell is a Lisp process with a PTY (`ProcessKind::Real`,
`EV/data/process_state.rs:70-75` [P4]); the VT state lives in the evaluator process; cells become ordinary
glyphs. `TerminalCommand`, the `Terminal*` input events and `SharedTerminals` leave the protocol. It then
honours `default-directory` and `process-environment` as `term.el` does, works on every display including
TTY frames, and its typed same-host capability disappears.

---

## 10. The Lisp-visible surface

**GNU APIs keep GNU's exact values and errors:** "Don't know how to interpret display %S"
(`gnu:lisp/frame.el:1084` [G]), "Display %s can't be opened" (`gnu:src/xterm.c:30669` [G]), "Display still
has frames on it" (`gnu:src/xfns.c:7685` [G]), "Terminal is not live, can't create new frames on it"
(`:4960` [G]), the selection timeout text, `display-graphic-p` t, `display-images-p` t, `window-system`
`neo`.

**Neomacs extensions** (outside GNU's namespace; GNU-compatible Lisp never reaches them unless it asks):

| Symbol | Kind | Meaning |
|---|---|---|
| `neomacs-display-capabilities` (&optional TERMINAL) | function | plist, e.g. `(:protocol (1 . 0) :locality dialed :images t :shader-surface (:contract (1)) :video nil :resume t)` |
| `neomacs-display-capability-p` (CAP &optional TERMINAL) | function | boolean |
| `neomacs-display-state` (&optional TERMINAL) | function | `connected`, `detached`, `closing`, or nil for non-display terminals |
| `neomacs-display-info` (&optional TERMINAL) | function | plist: locality, protocol, both builds and `WireSchemaId`s, `:durability` (D31), RTT, pending assets, request counters, retained bytes of a detached session (R2-28), prediction statistics |
| `neomacs-display-backend` (&optional TERMINAL) | function | `x11`, `wayland`, `ns`, `w32`, `headless` |
| `neomacs-display-open-uri` (URI &optional TERMINAL) | function | asynchronous request through the frontend's policy |
| `neomacs-display-listen` (&optional NAME) | function | start the display listener in a non-daemon Emacs (a daemon starts it at readiness) |
| `neomacs-display-detach-functions`, `neomacs-display-resume-functions` | hooks (TERMINAL) | modelled on `suspend-tty-functions` and `resume-tty-functions`; GNU's tty hooks are deliberately not run for GUI terminals, because `suspend-tty` refuses non-text terminals (`gnu:src/term.c:2465-2466` [P4]) |
| `neomacs-display-detach-timeout` | variable | seconds, or nil for never (default: §17) |
| `neomacs-display-keepalive-timeout`, `neomacs-display-open-timeout`, `neomacs-video-diagnostics-timeout` | variables | 10, 10, 5 seconds. Threads that run no Lisp read the **global** values through evaluator-published snapshots refreshed when the variable is set; a `let`-binding does not reach them (R2-27) |
| `neomacs-display-version-policy` | variable | `schema` (default, same `WireSchemaId`) or `exact-build` (§6.6); read by the listener through a snapshot |
| `neomacs-remote-prediction` | defcustom | `adaptive`, `marked`, `off` |

**Conditions** (all with parent `error`, so `(condition-case … (error …))` catches them):
`neomacs-display-error` ⊃ `neomacs-display-capability-unavailable` `(CAPABILITY TERMINAL REASON)`,
`neomacs-display-disconnected` `(TERMINAL STATE)`, `neomacs-display-timeout` `(TERMINAL OPERATION SECONDS)`,
`neomacs-display-open-error` `(DISPLAY REASON DETAIL)`, `neomacs-display-protocol-error`,
`neomacs-display-policy-refused` `(TERMINAL EFFECT)`. `REASON` is one of `not-advertised`,
`requires-same-host`, `policy-refused`, `not-implemented`, `protocol-mismatch`, `build-mismatch`,
`auth-failed`, `label-in-use`, `timeout`, `peer-closed`.

---

## 11. Security and deployment

### 11.1 Transports and authentication

| Transport | Authentication | Exposure |
|---|---|---|
| in-process | — | none |
| socketpair to a spawned frontend | inherited fd; no filesystem node | none |
| display listener (Unix socket) | in the 0700 rendezvous directory `/tmp/neomacs-<uid>` (or `NEOMACS_DISPLAY_RENDEZVOUS_DIR`; an absolute `server-name` puts it beside that path), checked with `server-ensure-safe-dir` rules (owned by the user, not a symlink, `mode & 077 == 0`, `lisp/server.el:557-604` [M]) on bind and by the relay before connect. The socket's mode is set by `chmod(2)` on the path right after `bind`, never by the umask (Lisp's `with-file-modes` changes it) and never by `fchmod`: unix(7) on this host says permissions come from the umask and change with chmod(2), and its source notes "fchown() and fchmod() do not seem to have an effect" (R2-26 [V]). The real controls are the private directory and the peer check: the `SO_PEERCRED` / `getpeereid` uid equals the daemon's | same user only; confers no new authority, because any same-uid process can already `-eval` through the server socket (P3 §8.1) |
| ssh stdio (remote default) | ssh keys and host keys; the relay runs as the user | the ssh server |
| TLS (horizon) | `rustls` (workspace dependency, `Cargo.toml:390` [V]) with a pinned certificate fingerprint plus a 256-bit token | explicit opt-in listener only |

There is **no network listener anywhere** in this campaign.

### 11.2 Trust boundaries

- **The frontend treats the evaluator's stream as untrusted** (trust inversion: the remote runs arbitrary
  user Lisp, u9 R10): every limit in §6.18 is checked before allocation; re-seal and validation are
  fallible; asset hashes are verified before use and persistence; pixels arrive decoded, so no image
  decoder parses remote-chosen bytes in v1.
- **Font parsing is a real attack surface.** `ttf-parser` carries a waived advisory
  (`deny.toml:8` [V]: `RUSTSEC-2026-0192`). Fonts from a compromised remote are parsed on the user's
  machine. The frontend's local hash index avoids it for files it already has (`Have`). Sandboxing font
  parsing is horizon.
- **The evaluator treats the frontend as untrusted for host access**: assets are wanted by offered id only;
  the frontend can never make the evaluator read a path; file drops are URIs that Lisp's `dnd.el` judges;
  **hits are resolved by the evaluator, so peer-supplied buffer positions no longer exist** (D4).
- **Keystroke timing leaks through traffic shape** (u9 R10); padding is horizon.

### 11.3 Least authority for side effects the remote side initiates

| Effect | Mechanism | Default frontend policy |
|---|---|---|
| clipboard write (`gui-set-selection`) | keyed latest-wins ownership | `Dialed`: **`on-input`**: allowed within 2 s after user input on that display, so kills and copies work while a timer cannot overwrite the laptop's clipboard ("pastejacking"); local links: allow (R2-15) |
| clipboard read (`gui-get-selection` when not owned) | request | `Dialed`: **`on-input`** (`C-y` works; a background reader does not); `allow` and `never` available (P4 §8.3). "GNU-over-X parity" holds only for trusted X11 forwarding, and ssh's upstream default is untrusted (`ForwardX11Trusted no`, ssh_config(5)). The default is user decision U3 (§17) |
| open URI | `OpenUri` request | `Dialed`: `on-input` for `http`, `https` and `mailto`. Loopback, link-local and private-network targets are refused unless the per-host policy names them, which blocks CSRF and SSRF against services on the laptop. Rate-limited; other schemes ask |
| GPU programs (shader surfaces, frame shaders) | media ops | `Dialed`: deny unless the per-host policy allows; the frontend validates every program with naga before compiling it (R2-15) |
| attention requests, pointer warps, focus stealing | window ops | per-host `allow │ ask │ deny` (P2 §8.2) |
| process execution on the display host | — | **never** |

### 11.4 Subprocess `DISPLAY` and `browse-url`

- **C101.** GNU copies the selected frame's `display` parameter into children, and pgtk refuses non-X11
  displays (`gnu:src/callproc.c:1879-1895` [G·V]). Neomacs's injector copies any display string
  (`NS/environment/mod.rs:257-279` [V]). Rule: export the frame's display only when its terminal has a
  **host-local X11 display name** (an `InProcess` or `SpawnedChild` link with backend X11); otherwise fall
  back to `initial-environment`. A `neo:` name never reaches `DISPLAY`. Documented divergence on the pgtk
  precedent.
- **`browse-url` bypasses that rule from Lisp** (J3 A.3): it binds `process-environment` and calls
  `(setenv "DISPLAY" dpy)` from `(frame-parameter nil 'display)` for every build except pgtk-on-Wayland
  and Android (`gnu:lisp/net/browse-url.el:927-951` [G·V]); `browse-url-process-environment` does the same
  (`:1073-1087` [G·V]); Neomacs's copy is byte-identical (`cmp` [V]). Fix, following GNU's own
  per-display-kind branches: add a `neo` branch that sets `DISPLAY` only from
  `(neomacs-display-local-x11-name)` (nil for `Dialed` terminals). This is a documented divergence of one
  vendored GNU Lisp file (Neomacs already tracks such divergences, `lisp/term/neo-win.el:231` cites
  `DIVERGENCES.md`). Whether `browse-url` on a non-local display routes to `neomacs-display-open-uri` by
  default is a user decision (§17); the mechanism exists either way.
- Audit every Lisp writer of `DISPLAY` in the same milestone (`common-win.el:157` runs only for
  `-display` at startup [J3]).
- **`NEOMACS_DISPLAY` (R2-33).** Subprocesses of a frame on a `neo` terminal get
  `NEOMACS_DISPLAY=<terminal name>`. `neomacsclient -c` without `-d` prefers it over `WAYLAND_DISPLAY` and
  `DISPLAY` (`NM/bin/neomacsclient.rs:839-857` [V]), so `EDITOR="neomacsclient -c"` in a remote shell
  opens a frame on the laptop, as `DISPLAY` does under X11 forwarding. This is a documented divergence:
  GNU exports no such variable, and only `neo` frames can produce it.

### 11.5 Deployment

- **Remote host.** Neomacs is installed (binary, Lisp tree and dump image) with the same `WireSchemaId`
  as the frontend, under the default `schema` policy (§6.6). A mismatch is a typed refusal that names both
  builds. Auto-install over ssh (VS Code and Zed, u9 R9) needs a packaged runtime tarball keyed by
  `BuildId`, because Neomacs needs its Lisp tree on the remote (u9 B7): horizon.
- **The remote daemon reads the remote user's init** (GNU daemon semantics).
- **An auto-started daemon never inherits an ssh-forwarded `DISPLAY`.** The relay removes `DISPLAY` and
  `WAYLAND_DISPLAY`.
- **Daemon lifetime (D31, R2-01).** It is a host fact, not something the relay decides.
  - *Why.* `pam_systemd(8)` on this host documents that `$XDG_RUNTIME_DIR` "is automatically created
    the first time a user logs in and removed on the user's final logout". `logind.conf(5)` documents
    that `KillUserProcesses=yes` terminates the session scope. `loginctl(1)` documents that only
    lingering keeps the user manager "around after logouts" [V]. `setsid` does not leave the session's
    cgroup scope. On a dev box where the remote frontend is the user's only login, the overnight
    disconnect that U1 targets is therefore exactly the final logout.
  - *Starting.* When a service manager is reachable, the relay starts the daemon in it as a transient
    user service (`systemd-run --user --collect --unit=neomacs-NAME -p Type=notify neomacs --fg-daemon=NAME`).
    That needs `sd_notify(READY=1)` in `--fg-daemon`, which GNU sends when built with libsystemd
    (`#ifdef HAVE_LIBSYSTEMD`, `gnu:src/emacs.c:3433-3435` [G·V]; whether the pinned oracle profile has it is
    unattested) and Neomacs does not yet (no `NOTIFY_SOCKET` use in `crates/neomacs/src` [V]); M11 adds it as a
    GNU-ward fix. GNU's own `etc/emacs.service` is `Type=notify` (`etc/emacs.service` [V]).
  - *Reporting.* The daemon computes `durability` at startup from its own cgroup (a user-manager unit or
    a session scope, `/proc/self/cgroup`) and the user's linger flag (`/var/lib/systemd/linger/USER`).
    It reports `persistent`, `until-logout{reason}` or `unknown` in `ProbeReply`, `RelayStatus`,
    `Welcome` and `neomacs-display-info`.
  - *Surviving logout.* The rendezvous listener lives in `/tmp`, so it survives the removal of the
    runtime directory. While the daemon lives, resume works. `server.el`'s socket may vanish with the
    runtime directory; a new attach then gets `ServerSocketUnreachable` and the remedy (resume, then
    `M-x server-start`). Neomacs never rebinds `server.el`'s socket behind its back.
  - *What this host cannot test.* This host has no logind session for the shell (`XDG_RUNTIME_DIR`
    unset), so the logout behaviour is CI or VM work.
- **Agent forwarding (D36, R2-06).** With agent forwarding, `SSH_AUTH_SOCK` names a per-connection
  socket that disappears with that connection. Every resume is a new ssh connection, so a daemon that
  kept the first one would fail `git push`, magit fetch and TRAMP hops for the rest of its life. GNU has
  the same problem and its unit file says so: "The location of the SSH auth socket varies…"
  (`etc/emacs.service`). Rules:
  - every relay re-points the per-user link `/tmp/neomacs-<uid>/agent.sock` at its own
    `SSH_AUTH_SOCK`, atomically, by symlink and rename, on attach and on resume;
  - a relay-started daemon gets `SSH_AUTH_SOCK=/tmp/neomacs-<uid>/agent.sock`;
  - a user-started daemon can opt in by setting that path in its unit;
  - while no frontend is connected the link dangles and agent use fails, which is honest;
  - with two frontends attached, the last attach wins.

  `(getenv "SSH_AUTH_SOCK")` then differs from what a GNU daemon started by `emacsclient -a ''` would
  show. That is a documented divergence (§12.3), reachable only through `--display-relay`. Other
  inherited variables (`SSH_CONNECTION`, `SSH_TTY`) keep GNU's daemon semantics and may go stale, as in
  GNU.
- **Platforms.** The evaluator host is Unix (Linux, macOS). The frontend runs wherever winit runs,
  Windows included, with ssh as a child process. All socket code is `cfg(unix)` behind the transport
  trait, for the six-target build gate (u10 §2.3). The pure crates (`neomacs-session`,
  `neomacs-display-wire`, and the headless parts of `neomacs-frontend`) also run their tests on the
  macOS and Windows CI legs (R2-35). Caches and tokens use platform directories (§8.2). On macOS, the
  frontend holds an activity assertion while a session is connected, so App Nap cannot stretch the 1 s
  ping past the liveness timeout (R2 Q3; CI-only).

---

## 12. GNU compatibility contract mapping

All GNU entries are **source contracts, UNATTESTED**. C-numbers are u8's. Milestones refer to
`MILESTONES.md`.

### 12.1 Contract table

| GNU contract | Source | This design | Status |
|---|---|---|---|
| C1–C2 one terminal per connection; monotonic ids never reused; `terminal-list` oldest first | `gnu:src/terminal.c:292-323` [G] | one terminal per display channel; `next_terminal_id` from a monotonic counter | conforms after M5 |
| C3/C37 liveness flips before frames are deleted | `gnu:src/terminal.c:330-357` [G] | reordered teardown (§7.2) | fixed in M5 |
| C4 `terminal-live-p` = window-system symbol | `gnu:src/terminal.c:443-480` [G] | record-only; `neo` | fixed in M5 |
| C5 `terminal-name` = the name used | `gnu:src/terminal.c:517-528` [G] | exact string, stable across resume | conforms |
| C10/C11 `read_socket_hook` N/0/−1/−2 | `gnu:src/termhooks.h:822-833` [G] | reader outcomes; hangup → `DisplayHangup` | equivalent |
| C12/C36 `delete-terminal` hooks; "Attempt to delete the sole active display terminal" | `gnu:src/terminal.c:381-421` [G·V] | unchanged in shape; detached terminals are inactive | conforms; extension documented. The refusal is reachable only once the initial terminal is gone, as in a non-daemon session after `frame-initialize` deletes `terminal-frame` (`gnu:lisp/frame.el:446` [G·V]), because `TERMINAL_ACTIVE_P` is true for the initial terminal (`gnu:src/termhooks.h:935-937` [G·V]). In a daemon it cannot fire (R1 F22) |
| C15 `TERMINAL_ACTIVE_P` | `gnu:src/termhooks.h:935-938` [G·V] | `neo` terminals active unless detached | extension (N1) |
| C16 a frame's terminal is fixed | `gnu:src/xfns.c:5042` [G] | yes; resume rebinds a *transport*, never a frame | conforms |
| C18–C22 `make-frame` / `x-create-frame` resolution and errors; `display` parameter = terminal name | `gnu:lisp/frame.el:1065-1086`, `gnu:src/xfns.c:4949-4960`, `gnu:src/frame.c:4205-4208` [G] | §7.3 | fixed in M5 (today `display` is ignored) |
| C19 `window-system-initialization` once per type | `gnu:lisp/frame.el:1090-1094` [G] | neo-win preloaded; per-display opening inside `x-open-connection` | conforms after M5 |
| C28–C35 `delete-frame` rules | `gnu:src/frame.c:2495-2866` [G] | unchanged; remote frames count as other frames | conforms |
| C32 toolkit pin of the last frame's terminal | `gnu:src/frame.c:2969-2983` [G·V] | pinned for every `neo` terminal (D11) | conforms (gtk3 profile) |
| C40/C41 displays dialed by name; `x-open-connection` unconditional | `gnu:src/xfns.c:7640-7660` [G·V] | open by name; second open → second channel (D7) | conforms; dial-in rendezvous is extension N3 |
| C42 `x-display-list` oldest first, frameless included | `gnu:src/xfns.c:7692-7703` [G] | yes, detached included | conforms |
| C43 `check_x_display_info` designators and errors | `gnu:src/xfns.c:168-205` [G] | all `x-display-*` and monitor queries | fixed in M5 |
| C44 `x-close-connection` refuses with frames | `gnu:src/xfns.c:7674-7690` [G] | yes | fixed in M5 |
| C45 KBOARD shared on the same X server | `gnu:src/xterm.c:30875-30916` [G] | channels of one session share a KBOARD | conforms after M16 |
| C46, C99 `x-display-*` per display; avoid round trips | `gnu:src/xfns.c:5613-5990` [G] | per-terminal replicas | conforms |
| C49 linux-gtk3 aborts on display loss | `gnu:src/xterm.c:26907-26919` [G] | **not reproduced** | **documented divergence** (cannot be attested as matching on linux-gtk3) |
| C50/C51 non-GTK and tty loss: `noelisp` deletion, `kill-emacs` only when nothing remains, no error from the tty path | `gnu:src/xterm.c:26965-26992`, `gnu:src/keyboard.c:8109-8130` [G] | non-resumable loss and grace expiry follow the tty path | equivalent |
| C52 frames vanishing during redisplay | `gnu:src/xdisp.c:17883-17890` [G] | liveness re-checked | equivalent |
| C53 `server-handle-delete-frame` ignores dead frames | `gnu:lisp/server.el:500-513` [G] | frames are `nowait`; the relay's client ends at once | conforms |
| C54–C66 KBOARDs, 14 variables, single-kboard, "Frame is locked", C-g isolation | `gnu:src/keyboard.c:2362-2386,14472-14479` [G] | M16 | documented limitation until M16 |
| C60 `gobble_input` makes C-g reach a busy Emacs | `gnu:src/keyboard.c` [G] | early quit on the link reader with the actual quit char | conforms |
| C72–C76 `window-system` per KBOARD; `initial-window-system` nil in a daemon | `gnu:src/dispnew.c:7193-7313` [G] | unchanged | conforms |
| C77–C84 `make-frame-on-display`; `make-frame-on-monitor` quirk; `framep-on-display`; WM close → `delete-frame` event; C-z iconifies | `gnu:lisp/frame.el` [G] | Lisp unchanged; quirks kept | conforms |
| C85–C94 `server.el` byte-identical; `-window-system`; `server-goto-toplevel` on attach | `gnu:lisp/server.el` [G·V parts] | attach is an ordinary `-nowait` request; resume bypasses `server.el` | conforms; `server-goto-toplevel` applies to initial attach only, as GNU's (`:1467-1468` [G·V]) |
| `nowait` frames survive their client; the sentinel deletes a waiting client's frames | `gnu:lisp/server.el:362-370,421-443,993-1000` [G·V] | attach uses `-nowait` (D9) | conforms; precondition for detach |
| C95 a daemon initializes no display | `gnu:src/dispnew.c:7222-7238` [G] | daemons always headless (M13) | **fixes** today's divergence |
| C100 selections per terminal; selection wait quit-able, `x-selection-timeout` 0 unbounded | `gnu:src/termhooks.h:489-503`; `gnu:src/xselect.c:1714-1745,3434-3439` [G, G·V] | §6.13; ownership published lazily; local answer only with an exact ownership signal | conforms after M9; replaces the raw 5 s wait (`NM/main.rs:1212,1838-1848` [V]) with GNU's variable plus the frontend's own reply bound |
| PRIMARY set from the active region after every command | `gnu:src/keyboard.c:1612-1641` [G·V]; Neomacs `crates/neovm-core/src/emacs_core/commands/keyboard/command_loop.rs:1262-1280` [V] | the data stays on the evaluator; only ownership and a generation cross, and data is converted on request | conforms (GNU's data likewise stays in the Lisp process until a client asks) |
| server-created frames: the `environment` parameter has no effect on subprocesses | `gnu:lisp/server.el:1000-1011` [G·V] | unchanged | conforms; the `SSH_AUTH_SOCK` link (D36) is a divergence (§12.3) |
| `--fg-daemon` notifies systemd readiness | `gnu:src/emacs.c:3433-3435` [G·V], under `HAVE_LIBSYSTEMD` | added in M11 (the datagram protocol needs no libsystemd) | **fixes** a divergence for libsystemd builds (Neomacs sends nothing today) |
| local selection answer | `gnu:src/xselect.c:2637-2645` [G·V] | local answer while owned | conforms after M9 |
| C101 subprocess `DISPLAY` from the frame | `gnu:src/callproc.c:1879-1895` [G·V] | X11 host-local names only | **documented divergence** (pgtk precedent) |
| `browse-url` sets `DISPLAY` from the frame | `gnu:lisp/net/browse-url.el:927-951,1073-1087` [G·V] | `neo` branch | **documented divergence** (GNU's per-kind branches) |
| C102 fonts on the Lisp host; cache per terminal | `gnu:src/termhooks.h:940-960` [G] | catalog stays on the evaluator; bytes shipped | conforms |
| C104 redisplay visits every terminal | `gnu:src/xdisp.c:17849-17907` [G] | §7.4 | fixed in M5 |
| images decoded in the Lisp process; `image-cache-size` nominal; caches per terminal | `gnu:src/image.c:2484-2494,4231-4252` [G] | evaluator decode, ledger, per-terminal residency | conforms after M10 (value change UNATTESTED) |
| `x-hide-tip` from Emacs-side state | `gnu:src/xfns.c:8984-9021` [G] | evaluator-authoritative | conforms after M9 |
| `x-wait-for-event-timeout` 0.1 s | `gnu:src/xterm.c:32785-32794` [G] | resize wait keeps today's 1 s until DQ25 is attested | open (§15) |
| posn timestamp = display `Time`; double-click uses it | `gnu:src/keyboard.c:6088-6093,6586-6594` [G] | `DisplayTime` carried; adoption oracle-gated | horizon |
| `throw-on-input` sets `quit-flag` at store time | `gnu:src/keyboard.c:3867-3871` [G] | reader-side store-time check | M14 |
| posns built in the Lisp process | `gnu:src/keyboard.c:5782-5795,6011` [G] | evaluator-side resolution | conforms after M8 |
| drag and drop with `file://HOST`, foreign hosts rejected | `gnu:lisp/dnd.el:279-299` [G] | URIs carry the laptop's host | conforms after M9 (today the drop is discarded, `NM/input_bridge.rs:428-431` [M]) |
| `suspend-tty` keeps frames, hides the top frame, stops display | `gnu:src/term.c:2440-2447,2486-2497` [G·V] | model for the detached state | extension (N1) |

### 12.2 The detached state (extension N1, modelled on `suspend-tty`)

| Lisp surface | Connected | Detached | GNU anchor |
|---|---|---|---|
| `terminal-live-p` | `neo` | `neo` (the terminal was not deleted) | C4 |
| `frame-live-p` on its frames | t | t | `suspend-tty` keeps frames (`gnu:src/term.c:2443-2446` [G·V]) |
| `frame-visible-p` | as set | **nil**. Lisp's own setting is kept apart from the detached flag and restored on resume, including changes made while detached (R2-23) | `SET_FRAME_VISIBLE (root_frame (top), false)` (`gnu:src/term.c:2491-2495` [G·V]) |
| selected frame | — | moves at detach to a frame on an active terminal, else to the initial frame, by the same choice `delete-frame` makes, without running deletion hooks. Input on *T*'s frames after resume selects them again through the usual `switch-frame` event. Timer and sentinel prompts, and idle child frames that follow the selected frame (posframe, eldoc-box), therefore stop targeting the hidden terminal (R2-09) | the frame-deletion path |
| KBOARD | — | if *T*'s KBOARD is current in single-kboard state (from M16), it is released exactly as GNU's `delete_kboard` releases a dying terminal's: `current_kboard` becomes the selected frame's, and `single_kboard` becomes false (`gnu:src/keyboard.c:13174-13183` [G·V]). A read pending on *T* stays on the stack and can be answered from another terminal or after resume. Before M16 there is no lock to release | `delete_kboard` (gives U10 a mechanism) |
| presentations | — | in-flight ones are retired; the last activated one stays resolvable until the first activation after resume (R2-23) | [I] |
| redisplay | yes | skipped | "if they change, their display is not updated" (`:2445-2446` [G·V]) |
| `TERMINAL_ACTIVE_P` analogue | true | **false**: `delete-terminal` refuses the sole *active* terminal unless FORCE | `gnu:src/termhooks.h:935-938` [G·V]; `gnu:src/terminal.c:395-403` [G] |
| `x-display-list` | includes it | includes it | the terminal exists |
| `x-create-frame` on it | creates a frame | creates the frame, invisible while detached; its window state is sent at resume like any other state record (D32). v1 signalled `neomacs-display-disconnected`, which made idle child-frame packages fail repeatedly (R2-09) | [I] |
| requests | served | fail with `neomacs-display-disconnected` | [I] |
| setters | sent | applied to evaluator state; the differing records are sent on resume (state synchronization, D32) | [I] |
| hooks | — | `neomacs-display-detach-functions` (TERMINAL); on resume `neomacs-display-resume-functions` | analogues of `suspend-tty-functions` / `resume-tty-functions` |
| timeout | — | `neomacs-display-detach-timeout` → `noelisp` deletion | C51 path |

### 12.3 Documented divergences (each recorded in `DECISIONS.md`)

1. Display loss: no GTK abort (C49); detach, then the tty/no-toolkit path.
2. Subprocess `DISPLAY` and `browse-url`: `neo:` names never exported (pgtk precedent).
3. Dial-in rendezvous (N3): Lisp still opens by name; only the transport rendezvous is reversed, as sshd
   provides the listener under X11 forwarding.
4. The detached state (N1) with `frame-visible-p` nil.
5. `mouse-position` from the last event (existing; GNU queries the server).
6. The resize-ack wait bound stays 1 s until DQ25 is attested.
7. One vendored GNU Lisp file changes (`browse-url.el`).
8. A relay-started daemon's `SSH_AUTH_SOCK` names the stable per-user agent link (D36).
9. Subprocesses of `neo` frames get `NEOMACS_DISPLAY`, and `neomacsclient -c` prefers it (§11.4).
10. Opening a display can be interrupted by `quit` (GNU's `XOpenDisplay` cannot be; §6.13).
11. From M5, class-3 "is there a GUI" answers follow the frame's terminal kind. For example,
    `neomacs-surface-available-p` becomes nil under `-nw`, where today the primary TTY's popup host
    makes it t (D37). This is a correction toward GNU's tty behaviour and is recorded with its tests.
12. `x-selection-timeout` 0 (unbounded) now also governs the local GUI, which had a fixed 5 s bound
    (`NM/main.rs:1212,1846` [V]). The frontend's own 5 s reply bound keeps a stalled foreign owner from
    hanging `C-y` (R2-32).

### 12.4 Attestation forms (to run once an attested oracle exists; none has been run)

u8 Appendix A forms A1–A11 apply ([M]; e.g. A6: daemon plus `emacsclient -c -n` on `:0`,
`(list (x-display-list) …)` → `((":0") (("initial_terminal" t) (":0" x)) ":0" x nil)`; A7: the toolkit pin;
A8: "Frame is locked"; A9: GTK abort on X loss; A11: tty hangup path). Added by P1 (A12–A15) and here:

| Form | Setup | Expression | Expected (source reading) |
|---|---|---|---|
| A12 | daemon, two Xvfb displays | `(progn (x-open-connection ":1") (x-open-connection ":1") (length (x-display-list)))` | `2` (`gnu:src/xfns.c:7655`) |
| A13 | frame on `:1` | `(condition-case e (x-close-connection ":1") (error e))` | `(error "Display still has frames on it")` |
| A14 | after opening `:1` | `(terminal-live-p (car (last (terminal-list))))` | `x` (Neomacs analogue `neo`) |
| A15 | frame on `:1` selected | `(getenv "DISPLAY")` in a subprocess | `:1` on linux-gtk3 (documents C101) |
| A16 | daemon; `emacsclient -c -n -d :1`; then kill the client process | `(length (frame-list))` | the nowait frame survives (`gnu:lisp/server.el:993-1000`) |
| A17 | frame on `:1`; `(gui-set-selection 'CLIPBOARD "x")` | `(gui-get-selection 'CLIPBOARD)` with the X server stopped (SIGSTOP Xvfb) | `"x"` without blocking (`gnu:src/xselect.c:2637-2645`) |
| A18 | as A6 | `(with-selected-frame F (let ((process-environment (copy-sequence process-environment))) (browse-url "x") ...))` instrumented | `DISPLAY` becomes `":1"` on GNU X |
| A19 | non-daemon `emacs -nw`; `(make-frame-on-display ":1")` = X; then `(suspend-tty)` on the tty | `(condition-case e (delete-terminal (frame-terminal X)) (error e))` | `(error "Attempt to delete the sole active display terminal")`: no initial terminal remains, and the suspended tty is inactive (`gnu:src/terminal.c:395-403`, `gnu:src/termhooks.h:935-937`). It grounds M12's registry-level test (R1 F22) |
| A20 | over ssh with agent forwarding: `emacsclient -a '' -t` auto-starts a GNU daemon; reconnect ssh | `(getenv "SSH_AUTH_SOCK")` in the daemon | the first connection's socket path, now dead. It documents the problem D36 diverges from |

---

## 13. Latency and feel plan

### 13.1 The physics

Lisp is remote, so a command's effect costs one round trip: frontend → input lane (flushed at once;
control and input first; never behind more than one bulk chunk) → reader → command loop → redisplay →
outbox → a delta of a few hundred bytes [E] → frontend. The design controls everything around that:

1. **Never block on the link** (§6.12): redisplay coalesces; Lisp waits only where GNU itself waits;
   the frontend UI loop never waits on the network.
2. **Keep local everything that is local** (§8.3).
3. **Bytes:** row-slot deltas; scrolling is slot re-listing; interned tables; deflate on WAN; a persistent
   content-addressed asset cache (fonts and decoded pixels) answered per offer with `Have`; bulk windowed
   behind control.
4. **Pacing:** two presentations in flight per frame, so a slow link lowers the frame rate instead of
   building a queue (mosh rule, u9 R3). Presentations are classed **Interactive** (the frame's frontier
   advanced since the last sent presentation) or **Ambient** (timers, process output); ambient ones are
   paced to at most one per `max(SRTT/2, one frame)` per frame, so a compilation flood cannot delay the
   echo of the next key, and C-g keeps working because input is the other direction (P3 §4.7). Locally
   SRTT ≈ 0, so nothing changes: there is no remote/local branch.
5. **Zero-RTT paths that GNU already allows:** the local selection answer while ownership is exactly known
   (D15); `x-hide-tip`, `image-size` and `image-cache-size` on the evaluator.

### 13.2 Feel by interaction (targets [I], gated or reported in MILESTONES M14)

| Interaction | In process | 20 ms RTT | 100–150 ms RTT, lossy |
|---|---|---|---|
| typing in a certified window (code, prose, minibuffer) | ≤ 1 frame, authoritative | ≤ 1 frame predicted; confirmed after RTT | ≤ 1 frame predicted; marked style during stalls |
| `RET` then a typed line | 1 frame | the new line after 1 RTT; the queued keys appear with it (deferred epoch) | same |
| `C-f`, `C-b`, `DEL` within a row | ≤ 1 frame | ≤ 1 frame predicted | same |
| wheel or touchpad scroll inside coverage | ≤ 1 frame | ≤ 1 frame; coverage sized by RTT | same |
| hover, pointer shapes, menu navigation | local | local | local |
| `C-y` while Emacs owns the clipboard | local | **0 RTT** with an exact ownership signal; 1 RTT otherwise (`selections/1` `ownership`) | same |
| region deactivation (`deactivate-mark`, after every command that drops the mark) | local | 0 RTT with an exact ownership signal; else one `SelectionExists` round trip, never a data fetch | same |
| `C-y` from another application | 1 IPC | 1 RTT | 1 RTT |
| resize drag | ≤ 1 frame | provisional crop at once; relaid after 1 RTT | same |
| completion popups (corfu, company) | — | 1 RTT after the key (they are Lisp) | same; the typing itself stays predicted |
| child-frame popups that resize then query (posframe, `fit-frame-to-buffer`, `gnu:lisp/window.el:10153,10305-10316` [G]) | — | one display-event-style RTT per resize-then-query, serialized on the evaluator (R2-25); measured in M14, not hidden | same |

### 13.3 Presentation-only prediction (MILESTONES M14)

- **Authority.** The evaluator already computes a read-only pixel-scroll proof over keymaps, hooks and
  verified definitions (`ND/neo/scroll.rs:1-3,163` [J2·V]) and republishes it through the redisplay
  signature "so the renderer cannot keep a stale grant" (`EV/command_loop.rs:1426-1434` [J2·V]). Prediction
  generalizes it into `ND/neo/prediction.rs`: one `WindowInteractionPolicy{scroll, insert, delete, motion}`
  per window, valid only for the presentation that carries it, computed only while the frontend's
  `PredictionDemand` is on, never executing keymap filters, autoloads or Lisp (P3 §5.5 certification rules:
  identity translation, `self-insert-command` unremapped, no prefix argument or overriding map, no partial
  key sequence, writable point, fill limit, verified hooks with character classes, advisories for unknown
  hooks).
- **Engine.** `RT/frame_compositor/input_echo.rs`, beside `input_scroll.rs`, under the compositor charter
  ("retained presentation state and every decision derived from it", `display-crate-layout.md:78-83` [V]).
  It paints into the materialized `FrameGlyphBuffer`; the renderer only draws. P3 §6.4's algorithm: an
  input's effect is shown either by the frame (`Reflected`) or by the overlay (`Pending`), never both;
  deferred epochs after an unpredictable key; `Indeterminate` frames drop predictions; adaptive marking by
  per-fingerprint confidence.
- **Self-regulation, no branch.** `PredictionDemand` turns on when the p50 of measured
  key→authoritative-frame latency over the last 32 inputs exceeds 1.5 frame intervals, off below 1.0
  (P3 §6.4). J1's caution that a slow local command could trip it locally stands: when it does, predicted
  characters are drawn before the authoritative frame and reconciled exactly, so nothing Lisp-visible
  changes, but the local screen does change visually. M14 measures how often that happens in process and
  reports it; if it is not rare, the threshold is revisited rather than special-cased.
- **Never touches Lisp state.** A misprediction is a one-RTT visual correction (mosh measured 0.9 %,
  u9 §2.3.4).

### 13.4 Resize, startup and recovery

- **Provisional resize** (DQ29): when expected relayout latency exceeds one frame (the same measured rule),
  draw the previous presentation anchored and clipped, never stretched, background-filled, then hand
  authority back. Purely visual; no Lisp-visible effect (J3 A.2 item 11). Today's rule never to draw a
  pre-resize *layout* as final (`RT/surface_resize.rs:105-112` [M]) is kept.
- **Startup**: the provisional "Connecting…" window appears before any network traffic.
- **Back from sleep**: first frame within one RTT plus a frame after the transport returns; resync of a few
  KiB with valid caches [E].

### 13.5 Measurement

`LinkEmulator` (in `neomacs-session` behind a `testing` feature): a seeded, deterministic in-process byte
proxy between two socketpairs modelling delay plus jitter without reordering, a token-bucket bandwidth cap,
stall injection for TCP retransmission under loss, cut/heal, and cut-after-record (P3 §10.4); tc/netem need
root, which this host does not have. A testing build of the frontend can insert it into its own transport
(`--link-emulator`), so daemon tests can stall a link without EOF or lose a `Welcome` in flight (R2-20). Metrics are measured on the frontend's own clock (K2P first response, K2A
authoritative), plus bytes and frames per key (deterministic). **Deterministic properties gate; latency is
reported** (u10 L6; J2 §6 item 5).

---

## 14. Testing strategy on this machine

### 14.1 Constraints (verified facts)

- **No GUI here.** No winit process can start: `libxkbcommon-x11.so` is missing, so `DISPLAY=:0
  neomacs -Q` panics before wgpu [RB 06]. There is no Vulkan ICD and no EGL (u10 §2.1). **GUI pixels and
  any native frontend window are CI-only (L5).**
- **The runnable editor** is the debug binary plus a bootstrap image. Daemons work with `DISPLAY` unset;
  `daemon_lifecycle` passed 35/35 in 318 s [RB 09]. With `DISPLAY=:0` the daemon panics at startup
  [RB 04].
- **No sshd, no logind session.** `command -v sshd` is empty, and `XDG_RUNTIME_DIR` is unset in the
  shell [V]. ssh-specific behaviour (remote-shell parsing, rc output, ControlPersist, half-open TCP) and
  systemd's logout behaviour cannot be exercised here (R2-20). The shim-based tests prove the fixed
  command line and the relay protocol. `LinkEmulator` proves the state machine. The rest is CI or VM work,
  or uses a user-mode sshd fixture if U12 approves one.
- **Disk.** 16 GB free; target 8.7 GB (`df`, `du`, re-measured in this revision [V]).
  - *Where copies come from.* Copies of the opt-2 `neovm-*` stack come from **distinct dev-dependency
    feature sets, not from feature flags** (R1 F7). Each `-p` selection unifies its own dev features onto
    the stack: `neovm-core` (`crates/neovm-core/Cargo.toml:211-232`), the layout engine
    (`crates/neomacs-layout-engine/Cargo.toml:65-83`), the runtime
    (`crates/neomacs-display-runtime/Cargo.toml:72-77`) and `neomacs` (`crates/neomacs/Cargo.toml:135-142`)
    [V]. Each of their test builds is therefore a separate copy.
  - *Measured.* The target holds 3–4 copies of each `neovm-*` rlib, about 0.37 GB of `neovm-*` rlibs
    per copy, plus 0.1–0.7 GB of test executables per package (`du` of `target/debug/deps` [V]).
    RUNBOOK §4.1's +5.77 GB for the first `neovm-core` test build included 3.21 GB of incremental
    artifacts, which `CARGO_INCREMENTAL=0` now avoids; its `deps` grew 2.10 GB.
  - *Budget.* At most five standing copies: dev bins, core tests, layout-engine tests, `neomacs` tests,
    and display-host tests from M4. The runtime's copy disappears at M4. That is about 1–1.5 GB per copy
    without incremental artifacts [E]. A rebuild after a source edit overwrites a copy in place; it does
    not add one. Never `--workspace`.
- **CI.**
  - *Jobs.* Tests in the `neomacs` package other than `--test daemon_lifecycle` run in no **automatic
    Linux** job. `windows_launcher` runs on Windows pull requests, and manual workflows run others (R1 F16
    [V]). Root-level `daemon_lifecycle` cases are serialized automatically (u10 §2.4–2.5). A new crate
    runs in CI only when added to the display job filter (`.github/workflows/test-suite.yml:1236` [V])
    and, for the macOS GUI suite, to the path filter (`.github/workflows/gui-tests-macos.yml:16-38` [V],
    R2-22).
  - *Access.* CI triggers only on pushes and pull requests to `main` (`test-suite.yml:3-10`,
    `build-gate.yml:3-10` [V]). The campaign branch has no upstream (`git branch -vv` [V]). The build
    gate is already known-red on the `neovm-eval` lib-test target (`build-gate.yml:87` [V]). How campaign
    PRs get CI is user decision U11. Every "CI-only" claim depends on it.
- **Baselines (R1 F5).** No full-suite pass result exists. RUNBOOK §4.1 compiled and *listed* 9,474
  `neovm-core` tests and states "I did not run the whole suite" [V]. Nor do the layout engine and the
  runtime `--lib` have one. MILESTONES M0 records all three, plus DP and `daemon_lifecycle`, on the
  untouched tree. "Passes" in any milestone means **no test that passed at baseline fails, and every new
  test passes** (D38). Run in this revision on the untouched tree [V]: `cargo fmt --all --check` (exit 0)
  and `check-test-targets.py` (`ok: checked 35 crates`).

### 14.2 Layers

| Layer | Content | Where | Runs here |
|---|---|---|---|
| L0 | `check-test-targets.py`, the new `check-crate-graph.py`, `cargo fmt --all --check`, the baseline diff script | scripts | yes |
| L1 | codec: round trips for every message, golden bytes and the `WireSchemaId`, exhaustiveness, totality, handshake matrix (incl. two-phase tokens, implicit takeover, incarnations, schema refusal), relay records, FrameSync sequence properties, size budgets | `neomacs-session`, `neomacs-display-wire` (no `neovm-*` in their build) | yes, about a minute |
| L1′ | real-frame parity: `materialize(seal(decode(encode(s))))` equals `materialize(seal(s))` on a **named corpus of evaluator-driven frames**. The engine's own fixtures lay them out (`engine.layout_frame_rust` then `last_frame_display_state`: `LE/engine.rs:1977-1987`, `LE/engine/tests/lazy_text_hit_test.rs:18-32` [V]). The mock frames bypass the evaluator and exist only for the `mock-display` example (`LE/mock_frame.rs:1-6`; `crates/neomacs/examples/mock-display.rs` [V]; R1 F3) | `neomacs-layout-engine` tests | yes |
| L2 | one scripted session over the in-process transport and `UnixStream::pair()`: identical delivered state, coalesced presentations, retirement sets, reply outcomes. Adversarial adapters: 1-byte I/O, splits, half-close, EOF mid-record, slow reader, peer drop, a stalled peer with 100k ops. The 77 `thread_comm` invariants are re-expressed generically | `neomacs-display-host`, `neomacs-frontend`, display runtime | yes |
| L3 | daemon plus a **separate headless frontend process** over the display listener, inside `crates/neomacs/tests/daemon_lifecycle.rs` (`remote_*` root-level tests); `LinkEmulator` inside the frontend for stalls and cuts | `-p neomacs --test daemon_lifecycle` | yes |
| L3′ | TTY output oracle for seam refactors: per step of a scripted scenario, the client pty's **parsed screen** (`vt100::Screen` through the `neomacs-tui-tests` harness, `crates/neomacs-tui-tests/src/lib.rs:980` [V]; its closure has no `neovm-*` crate [V]) beside the `text-faces` layout snapshot, for a daemon `-t` client **and** a non-daemon `-nw` session. The snapshot alone re-lays the frame out and never reaches `TtyRif` or `tty_output` (`NM/frame_layout.rs:111-171` versus `:437-449` [V], R1 F2) | `neomacs-tui-tests` `seam_baseline` module (MILESTONES M0) | yes |
| L4 | self-differential TUI (`compare_displays`) | `neomacs-tui-tests` | maybe (needs a pdumped runtime) |
| L5 | GUI e2e on Xvfb with lavapipe. The existing suite, plus `remote_gpu_*`: daemon plus GPU frontend PNG readback against the in-process GUI; asset fonts in the atlas; pending glyphs; provisional-window adoption; the reconnect band. Created in M11b and M12–M13 (R2-21) | `neomacs-gui-tests` | **no** (CI, U11) |
| L6 | latency and bandwidth through `LinkEmulator`, including image workloads (page turns, zoom, animation) and child-frame popups | an ignored `daemon_lifecycle` case, `remote_feel_report`; not `neomacs-perf`, whose dependencies include `neomacs-melpa-test-support` (`crates/neomacs-perf/Cargo.toml:21` [V], R1 F25) | report only |

### 14.3 Oracles that need no GPU

- `neomacs--frame-snapshot FRAME FORMAT` with `text`, `text-faces`, `json` and `json-geometry`
  (`ND/xdisp/mod.rs:10948-10994` [V]). It shows what the daemon lays out **now**: it re-runs layout with
  `FrameLayoutPurpose::Snapshot` (`NM/frame_layout.rs:140,158` [V]).
- The last presentation the daemon **submitted** for a frame, exposed for tests as
  `(neomacs--frame-snapshot F 'text 'submitted)`. That is a new optional SOURCE argument in the internal
  `neomacs--` namespace.
- The headless frontend's recorded `render_text()` per activated presentation: what the frontend
  decoded.
- **End-to-end criterion (D30).** The frontend's latest record equals the daemon's last submitted
  presentation. Whether a fresh snapshot equals the last submitted presentation at quiescence is
  untested today (R1 Q2), so it is its own test in M7. If it fails, that is a finding about the snapshot
  path to report, not a reason to weaken the criterion.
- The parsed TTY screens of L3′.
- **`TtyRif` dumps.** There is **no headless `RenderApp` today**:
  - `RenderApp` and its constructor are `pub(super)` (`RT/state.rs:978,1202` [V]).
  - The fixture v1 cited is an ignored subprocess test that floods input
    (`RT/tests/input_backpressure_test.rs:219-221,330-350` [V]).
  - The materializing worker is spawned only inside the winit startup closure (`RT/bootstrap.rs:713-736`
    [V]).

  M11b adds a public headless constructor that spawns `FramePreparation`, tested to materialize
  presentations. Until then, parity tests use DP `materialize()` and do not cover the compositor
  (R1 F6).
- The differential pointer test: old frontend-side resolution against new evaluator-side resolution
  over a corpus (MILESTONES M8; the pane-morph case runs in the runtime, R1 F27).
- The transport-parity test: identical materialized `FrameGlyphBuffer`s over the in-process and `wire`
  transports, on the headless `RenderApp` (MILESTONES M13).

### 14.4 What is never claimed

No GNU behaviour is claimed as attested; no GUI pixel behaviour is claimed from this host; no latency
number is claimed from an unmeasured estimate. Every acceptance command in `MILESTONES.md` records its
output under `campaign:evidence/`.

---

## 15. Design horizon

This section is the AGENTS.md §10 design horizon for the campaign as a whole; each milestone's PR body
narrows it to its own scope.

**The broader problem.** Neomacs's display boundary was a thread seam: three queues plus 15 shared-memory
couplings and 6 reply channels (T1–T3, S1–S15 [M]). The general problem is that a display is a *terminal
connection* with Lisp-visible identity, lifecycle, input, selections and facts, and that every such
connection should speak one typed, versioned, transport-independent protocol. Other entry points that
share the problem:

- TTY terminals still render on the evaluator (`TtyRif`); a cell-grid NDP frontend would give mosh-like
  durability to terminal users (`TtyRif` already consumes `SealedFramePresentation`, P3 §13.2).
- A crash-isolated local GUI (frontend out of process by default), deciding on measurement (P4 gate G1).
- A web or WASM frontend (raster shipping; `GraphicalBackend::Web` exists, u5 §3.2 D).
- Collaboration: two frontends viewing one frame, and moving frames between terminals (no GNU analogue,
  u8 N2).
- A `workspace` service on the same session layer: a first-party TRAMP-native method for "Lisp local, I/O
  remote" (u9 R15; P4 §2.4).

**GNU files opened for this synthesis** (source contracts, UNATTESTED): `lisp/server.el`
(`:360-372,421-445,988-1002,1464-1470,1905-1918`), `src/frame.c:2962-3000`, `src/callproc.c:1874-1900`,
`lisp/net/browse-url.el:915-955,1070-1090`, `src/term.c:2440-2447,2486-2497`, `src/termhooks.h:933-939`,
`src/xselect.c:2634-2648,3432-3440`, `src/xfns.c:7640-7660`, `lisp/loadup.el:300-312`.

**Practice compared.** Observable behaviour follows GNU Emacs 31.1 at the pinned commit. For the internal
design, the practice compared is the prior art researched in u9 (Neovim's UI protocol and `--embed`, mosh's
state synchronization, VS Code's `PersistentProtocol` with a 3 h reconnection grace and exact-commit
servers, Zed, xpra, RDP glyph caches, X Render glyph sets, SUPDUP local editing, emacs-tramp-rpc, and the
2026 emacs-devel display-server threads). **This synthesis did not open those external pages again; it
relies on u9's citations** (AGENTS.md §11: do not cite a page you did not open). The existing Neomacs seams
this design preserves and generalizes: `SealedFramePresentation`, `InputCheckpoint`, the pixel-scroll
grant, `TtyFrameHostFactory`, `HostResult`/`HostClosure`, and the invariants of
`docs/design/render-input-backpressure.md`.

**The long-term design** is this document. **What the ledger lands:** M0–M16 plus M11b (baselines,
vocabulary, protocol crates, registry, crate cut, GNU multi-terminal parity, link host, end-to-end,
evaluator-side hits, services, assets, relay, the GPU presenter over links, durability, local
convergence, prediction, neo-term, KBOARDs).

**Deliberately left for later:**

| Item | Why later | Prerequisite |
|---|---|---|
| `DisplayTime` in posn timestamps and double-click | Lisp-visible value change (from `0`) | an attested oracle comparison |
| Resize wait inside `set-frame-size` bounded by `x-wait-for-event-timeout` (DQ25) | changes local behaviour | oracle attestation |
| `preedit-text` events (DQ27) | Lisp input methods; GNU-visible | oracle |
| Encoded image bytes to the frontend | untrusted decoder input | frontend parser sandbox |
| Font-parser sandboxing; keystroke-timing padding | security hardening | measurement of cost |
| Video (byte-range streaming, evaluator-owned timeline) and webview (Lisp-host engine with pixel streaming) on remote displays | large; typed unavailable is honest | — |
| TLS, QUIC or UDP roaming transports | ssh covers v1 | — |
| Auto-deploy of the remote runtime | packaging pipeline | runtime tarball keyed by `BuildId` |
| Splitting `Glyph` into appearance and source | the frontend type would carry no source fields | after prediction's row checks are settled |
| Partial-row patches; LZ4/zstd; dictionary compression | only if measurements show row resends dominate | L6 data |
| SUPDUP-style delegated editing with faithful hook replay | presentation-only prediction first | M14 data |
| Respawn-and-resume for crashed spawned local frontends | crash isolation | M13 |
| KBOARD sharing by keyboard identity across sessions | rare | M16 |
| Windows evaluator host (named pipes) | no daemon on Windows today | — |
| Cross-schema skew: length-framed nested structs with per-peer-minor encoding, and old-decodes-new tested against an archive of frozen decoders | DP vocabulary is positional today; v1's same-major claim was unsafe (R2-16) | a wire-evolution design that does not duplicate DP's type universe |
| A same-host proof for relay-less dial-in: a per-daemon nonce readable only on the host, never forwarded by a relay | `--connect` frontends are `Dialed` in v1 (R2-29) | — |
| Shared-memory pixels (memfd) for spawned local frontends | measure first (R2-19) | M13 deterministic counts and CI p99 |
| A user-mode sshd fixture (nix OpenSSH on a high port, user-owned host key) | disk cost unmeasured on this host (R2-20) | U12 |
| Durable sessions across the final logout without lingering on systemd hosts | the host removes the runtime directory and may kill the session scope; reported as `durability` (R2-01) | none: lingering is the supported configuration |
| Answering geometry getters from a requested size instead of waiting | the window manager may adjust the size, so the answer could be wrong (rejected part of R2-25) | — |

---

## 16. Risks

| # | Risk | Mitigation |
|---|---|---|
| K1 | Size of the evaluator refactor: 90 trait methods, 145 accesses in 37 files, 64 implementors, 28 `TERMINAL_ID` references (J2 §1.3) | registry with one entry first (M3); u4's five call-site classes as five PRs; `daemon_lifecycle` and the seam baseline at every step |
| K2 | The local GUI cannot be exercised here | GUI conversion last (M13), one coupling per PR; the GPU path over links lands first (M11b) with its L5 tests; a headless `RenderApp` constructor added in M11b (none exists today, R1 F6); transport-parity test; L5 in CI once U11 grants access (R2-22) |
| K3 | Compile and disk budget (16 GB free; about 1–1.5 GB per stack copy without incremental artifacts) | protocol logic in leaf crates whose tests skip `neovm-*`; DP changes in M1, M8, M10, M14 and M15, because neo-term's types leave DP with it (R1 F4); at most five standing `-p` copies, budgeted in §14.1 (R1 F7) |
| K4 | Fonts: first-use bandwidth (0.34–10.8 MB per file, u5 §2.14), rasterizer version skew, licensing, untrusted parsing | prefetch by visibility, persistent cache, per-offer `Have` from a local hash index, typed pending, versions in the handshake, `--sync-fonts`, user decision on licensing |
| K5 | GNU divergences | each typed, recorded, UNATTESTED until the oracle runs; none reachable without the new display kind |
| K6 | Codec rigidity (bincode has no field tags) | frozen handshake, variant gating, append-only top-level fields, golden fixtures, the codec isolated in one module |
| K7 | In-process overhead from FrameSync bookkeeping | pointer-identity fast path; report p99 per presentation before switching (M13 bench) |
| K8 | Prediction mispredictions with electric modes, input methods, ligatures | conservative certification, advisories, marking, confidence cool-down, presentation-only |
| K9 | Head-of-line blocking and bufferbloat inside ssh | application-level byte window clamped to BDP; chunked, credit-gated bulk |
| K10 | Concurrent agents and the shared target directory | builds serialize on the target lock; acceptance commands are scoped per package |
| K11 | systemd hosts end user sessions at the final logout (runtime directory removed, session scope killed) | service-manager start, a logout-surviving rendezvous, a typed `durability` fact and the linger remedy (D31); CI or VM verification only (R2-01) |
| K12 | No established CI route for campaign PRs; the build gate is already red | U11 before M4; record the build-gate state per milestone (D38, R2-22) |
| K13 | Upgrades that change the `WireSchemaId` refuse resume to an older daemon | a typed refusal naming both builds; most commits keep the schema; cross-schema skew is horizon (R2-16) |
| K14 | The remote shell and ssh configuration vary (PATH, rc noise, prompts, stale masters) | a fixed command; the relay preamble; BatchMode retries; askpass; a scanned prefix; one visible login-shell retry (R2-13, R2-14) |

---

## 17. Open questions and decisions for the user

**Product decisions** (not engineering; each has a recommendation; version 2 changed U3, U5, U6, U7 and U10
and added U11–U13):

| # | Decision | Options | Recommendation |
|---|---|---|---|
| U1 | Default detach grace `neomacs-display-detach-timeout` | 3600 s; 10 800 s (VS Code's 3 h, u9 §2.3.1); 86 400 s; nil (never) | **86 400 s**: survives an overnight sleep; detached frames cost little (memory is bounded and reported, R2-28); nil leaves frames forever. On systemd hosts this holds only with lingering (D31) |
| U2 | `frame-visible-p` while detached | nil (`suspend-tty` analogue); unchanged | **nil** (J3 Part C item 1); Lisp's own setting is kept and restored (§12.2); no oracle form can attest either |
| U3 | Clipboard policy for `Dialed` frontends | `allow` (GNU-over-trusted-X parity); `on-input`; `never` | **`on-input`** for read and write (changed from `allow`, R2-15): `C-y` and kills work, background readers and pastejacking do not; `allow` is one flag away |
| U4 | Default display label | short hostname; `user@host` | **short hostname**, sanitized to the label grammar (R2-35) |
| U5 | `browse-url` on a non-local display | open on the user's machine via `neomacs-display-open-uri` by default; explicit opt-in only | **default on, under the `on-input` scheme policy** with loopback and private targets refused (R2-15) |
| U6 | Version-skew default | `schema` (same `WireSchemaId`); `exact-build`; same-major `compatible` | **`schema`** (changed; same-major is unsafe while nested vocabulary is positional, R2-16) |
| U7 | Input typed while the link is down | queue during a stall and drop after the transport is abandoned; drop as soon as "Reconnecting" shows; queue behind a confirmation | **queue during a stall (within the replay budget), drop with a bell once the transport is abandoned** (changed, R2-04) |
| U8 | Shipping the user's own font bytes to the user's own frontend (`fsType`) | accept; restrict to embeddable fonts | needs a maintainer or legal call |
| U9 | Host test infrastructure | approve user-space lavapipe plus `libxkbcommon-x11` (about 20 MB) so GUI tests can run here; accept CI-only GUI verification | approve if disk allows; otherwise CI-only is stated, not hidden |
| U10 | A detached terminal holding the single-kboard lock (once M16 lands) | release; keep the lock | **release, by GNU's `delete_kboard` mechanism** (§12.2; R2-09 supplied the mechanism) |
| U11 | How campaign PRs get CI (R2-22) | PRs to `main` from a fork with maintainer workflow approval; push access to a branch with `workflow_dispatch`; no CI (every CI-only claim stays unverified) | **fork PRs to `main` with workflow approval**, settled **before M4**. L5, macOS and Windows coverage depend on it |
| U12 | A user-mode OpenSSH sshd from nix as a test fixture (R2-20) | approve (disk cost to be measured first); CI-only ssh coverage | approve only if `nix path-info -S` shows a closure that fits the disk budget |
| U13 | The `SSH_AUTH_SOCK` link for relay-started daemons (D36) | on by default; opt-in | **on by default**: without it, agent-based `git push` breaks after the first reconnect |

**Engineering questions left to their milestones** [Q]:

- the real cost of one `neovm-eval` edit-test cycle under `CARGO_INCREMENTAL=0` (J2 Q3);
- whether `-p A -p B --features neovm-core/dbus-vendored` resolves for every package (u10 Q11). One
  combined invocation would also unify dev features into one stack copy (§14.1);
- the FrameSync in-process p99 budget;
- whether the M0 seam scenario is deterministic enough to gate (M0 requires two identical captures);
- whether a fresh snapshot layout equals the last submitted presentation at quiescence (R1 Q2; M7
  tests it);
- whether a GPU presenter blocked by occlusion stalls `Activated` credits (R2 Q2). This is harmless
  under D32: the frame's presentations are latest-wins. M13 measures it on macOS CI;
- macOS App Nap stretching the ping cadence (R2 Q3; the activity assertion of §11.5; CI);
- whether ssh sessions on macOS lack the launchd per-user `TMPDIR` (R2 Q4). This is moot for the
  rendezvous, which never reads `TMPDIR`, and the probe returns the server socket path;
- crates.io reachability (R1 Q3). This is moot for bincode, whose crates are cached, and still open for
  any new dependency.

---

## 18. Evidence log

### 18.1 Version 1 (design synthesis)

**Commands run in this session (read-only):**

- `git -C the repository rev-parse HEAD; git status --porcelain` → `72cba7f0…`, clean.
  `git -C the GNU Emacs checkout (gnu:) rev-parse HEAD` → `a360712c9d272d950d8d8255ef74570f7e90b7d9`.
- `cmp gnu:lisp/net/browse-url.el lisp/net/browse-url.el` and the same for
  `lisp/server.el`: identical.
- `grep` of `Cargo.lock` (`bincode` 2.0.1 at `:530-532`; postcard, lz4_flex, lz4 and zstd: 0 matches),
  `Cargo.toml` (`:149-150,170,174,215,257,386-387,390,392,398-400`), `deny.toml:7-11`.
- `rg -l neovm_core` and `rg -l neomacs_layout_engine` over non-test `DR/` sources (23 and 5 files);
  `grep` of `RW/glyph_atlas/mod.rs` for layout-engine uses (`:37,2268,2328-2351`); `rg` count of
  `skip_serializing_if|serde(flatten)` in non-test DP sources (13).
- `df -h /home` (16 GB free) and `du -sh target` (8.7 GB).

**Neomacs lines opened:** `AGENTS.md` (all); `docs/design/display-crate-layout.md` (all),
`host-effect-projections.md` (all), `render-input-backpressure.md:250-330`; `DR/thread_comm.rs:826-840,1045-1100`;
`DR/thread_comm/frame_mailbox.rs:1-30,60-100`; `DP/sealed_frame_presentation.rs:1-25,44-75`;
`NM/frame_layout.rs:170-245,470-492`; `DP/input_progress.rs:10-30,130-160,170-250`;
`crates/neovm-core/src/emacs_core/commands/keyboard/command_loop.rs:568-582`; `EV/mod.rs:3770-3780`;
`EV/protocol.rs:300` (grep); `EV/data/window_state.rs:3150-3175`; `EV/keyboard.rs:2085-2125`;
`EV/context_api.rs:1939-1953`; `NM/input_bridge/publication.rs:172-192`; `NS/wait/mod.rs:338-368`;
`crates/neovm-eval/src/host_effect/contracts.rs:494-512`; `ND/terminal/pure.rs:328-350`;
`NW/display_ext.rs:2186-2200`; `NS/environment/mod.rs:236-290`; `lisp/term/neo-win.el:25-36,225-240,248-312,450-470`;
`DP/visual_config.rs:125-180`; `DP/glyph_matrix.rs:2108-2120`; `DP/snapshot_text.rs:47,53` (grep);
`ND/xdisp/mod.rs:10930-10990`; `crates/neomacs-display-protocol/Cargo.toml` (all);
`crates/neomacs-display-runtime/Cargo.toml:40-100`; `crates/neovm-display/Cargo.toml:1-63`;
`crates/neovm-core/Cargo.toml:245-275`; `crates/neomacs/Cargo.toml:72-83` (grep);
`crates/neomacs/tests/daemon_lifecycle.rs:156-320,1827-1990` and its function list;
`.github/workflows/test-suite.yml:175-183,1225-1245`; `.config/nextest.toml` (grep);
`scripts/ci/check-test-targets.py:1-60`; `lisp/loadup.el` (grep).

**GNU lines opened:** listed in §15.

**Not run:** any `cargo build`, `check`, `test` or `nextest`; any Neomacs or GNU binary; any oracle form;
any latency or bandwidth measurement. Every size or latency figure is tagged [E] and comes from u1's model
or a proposal's arithmetic.

### 18.2 Version 2 (red-team revision)

**Phase rules held.** Nothing under `the repository` was edited. No `cargo build`, `check`, `test` or
`nextest` was run. No Neomacs or GNU binary was run. No oracle form was run.

**Commands run (read-only):**

- **Trees.** `git -C the repository rev-parse HEAD` returned `72cba7f0ced20c8f751938ae0ff946dcee13e4ad`,
  and `git status --porcelain` was empty. `git -C the GNU Emacs checkout (gnu:) rev-parse HEAD` returned
  `a360712c9d272d950d8d8255ef74570f7e90b7d9`.
- **Metadata.** `cargo metadata --no-deps --format-version 1` and `cargo metadata --format-version 1
  --offline --filter-platform x86_64-unknown-linux-gnu`, both saved to `/tmp/revise/`. A small query
  script over the resolve graph gave closures and reverse dependents:
  - transitive dependents: DP 15, `neovm-display-identity` 18, `neovm-eval` 8, `neovm-display` 6,
    `neovm-core` 5, layout engine 3, renderer 2, runtime 1;
  - `neomacs-tui-tests` with dev edges: 112 packages, 7 workspace crates, no `neovm-*`;
  - `neovm-system`'s workspace dependencies: `neovm-{edit,eval,text,value}` only.
- **Runtime feature selector.** It printed `--features neovm-core/dbus-vendored` on today's tree
  (MILESTONES conventions).
- **Baseline checks.** `python3 -I scripts/ci/check-test-targets.py < metadata` printed
  `ok: checked 35 crates` (exit 0). `cargo fmt --all --check` exited 0 with no output.
- **Disk.** `df -h /home` showed 16 GB free; `du -sh target` showed 8.7 GB. `ls -la` and `du -cb` of the
  `target/debug/deps` rlibs found 3–4 copies of each `neovm-*` crate, about 0.37 GB of `neovm-*` rlibs per
  copy, and test executables of 68–325 MB.
- **Host tools.** `command -v tmux screen sshd` found none. `python3 -c 'import pyte'` failed. `ssh -V`
  printed `OpenSSH_9.6p1`. `XDG_RUNTIME_DIR` is unset.
- **Manual pages read** with `zcat … | grep/sed`: pam_systemd(8), logind.conf(5), loginctl(1), unix(7),
  ssh(1) and ssh_config(5). Also `/etc/pam.d/common-session:29`, `/etc/skel/.bashrc:5-9` and
  `/etc/skel/.profile:19-26`.
- **History.** `git log --oneline -- crates/neomacs-display-protocol/src | wc -l` returned 3 of 50
  commits. `git show 023cbbe5 -- …/scroll_coverage.rs` showed the added `publication` field.
- **Registry.** `ls ~/.cargo/registry/{src,cache}/*` found `bincode-2.0.1` and `unty-0.0.4`, and no
  `bincode_derive`.

**Neomacs lines opened in this pass** (beyond §18.1), by area:

- *Display hosts and the TTY path.* `NM/tty_frontend.rs:25-80`; `NM/main.rs:4296-4302,5470-5490`;
  `NM/tty_init.rs:566-576`; `NM/frame_layout.rs:108-242,425-495`; `NM/secondary_tty.rs:95-115`.
- *Client, daemon and environment.* `NM/bin/neomacsclient.rs:428-442,660-680,836-860`;
  `NM/client_daemon.rs:96-122`; `NS/environment/mod.rs:236-292`.
- *Wait machinery and builtins.* `NS/wait/mod.rs:335-370,565-590`;
  `ND/window_cmds/mod.rs:6010-6020`; `ND/frame/mod.rs:2798-2812` and its eight callers;
  `ND/display/mod.rs:905-925`; `ND/shader_surface/mod.rs:340-356`; `ND/image/mod.rs:1220-1232,1410-1416`;
  `ND/terminal/pure.rs:60-70,280-298,590-610`; `crates/neovm-core/src/emacs_core/commands/keyboard/command_loop.rs:1262-1280`.
- *Runtime.* `RT/state.rs:975-980,1198-1206`; `RT/bootstrap.rs:700-740`;
  `RT/tests/input_backpressure_test.rs:214-226,330-350`; `RT/frame_windows.rs:886,923,1150`;
  `RT/frame_compositor/input_scroll.rs:274-280,368-384`; `RT/frame_compositor/overlays.rs:54-58`;
  `RT/frame_compositor/continuity/scroll.rs:206-215`; `RT/frame_compositor/continuity/mod.rs:155-163,292-299`;
  `DR/thread_comm.rs:70-78,296-302,833-836`; `DR/thread_comm/command_projection.rs:36-86`;
  `DR/clipboard.rs:135-139,365-388`; `DR/backend/tty/rif.rs:3983-3987`, plus its tests' `neovm_core` uses;
  `DR/font_defaults/policy.rs:1-12,80-92`; `DR/terminal/mod.rs:1-20`; `DR/lib.rs:120-140`;
  `crates/neomacs-display-runtime/Cargo.toml:70-120`.
- *Protocol crate.* `DP/types.rs:1-45`; `DP/frame_glyphs.rs:376-392,742-748`;
  `DP/glyph_matrix.rs:1263-1266,2119-2122`; `DP/geometry.rs:98-102`;
  `DP/presented_pointer.rs:812-820,2021-2029,2718-2735`; `DP/snapshot_text.rs:40-60`;
  `crates/neomacs-display-protocol/tests/identity_impl_guard.rs:16-90`;
  `crates/neovm-display-identity/tests/dependencies.rs`; `crates/neovm-value/src/display_vocab.rs:15-22,92-105`.
- *Renderer and layout engine.* `RW/glyph_atlas/mod.rs:37,2266-2270,2326-2352`;
  `RW/glyph_atlas/tests/glyph_atlas_test.rs:772-800`; `RW/image_cache/decoder.rs:6`;
  `RW/image_probe.rs:116-128`; line counts of the image files; `LE/mock_frame.rs:1-12`;
  `LE/engine.rs:1970-2000`; `LE/engine/tests/engine_test.rs:5251-5300`;
  `LE/engine/tests/lazy_text_hit_test.rs:15-35`; `LE/font/subpixel.rs:20-32`;
  `LE/font/fontconfig.rs:20-30,686-692,1855-1880`.
- *Manifests.* `crates/neovm-core/Cargo.toml:210-232`; `crates/neomacs-layout-engine/Cargo.toml:64-84`;
  `crates/neomacs/Cargo.toml` `[features]` and `:132-143`; `crates/neomacs-renderer-wgpu/Cargo.toml:14-20`;
  `crates/neomacs-perf/Cargo.toml:19-23`; `crates/neomacs-tui-tests/Cargo.toml`;
  `crates/neomacs-tui-tests/src/{lib.rs:2590-2640,launch.rs:1-60}`;
  `crates/neomacs/tests/daemon_lifecycle.rs:160-225`.
- *Lisp, docs and CI.* `lisp/term/neo-win.el:26-42,198-202,231-235,436-480,583-605,640-648,754-758`;
  `lisp/term/neo-preload.el:1-16`; `lisp/loadup.el:258-270`; `etc/emacs.service`;
  `docs/design/host-effect-projections.md:60-95`; `docs/daemon.md:180-210`;
  `.github/workflows/{test-suite.yml:1-12,120-135,175-186,1230-1240,1369,1468;build-gate.yml:1-12,84-90;gui-tests-macos.yml:16-40;windows-installer.yml:1-30}`
  and the trigger lines of `linux.yml`, `native-contract.yml` and `native-display-contract.yml`.

**GNU lines opened in this pass** (source contracts, UNATTESTED): `src/keyboard.c:1612-1642,3785-3872,13164-13203`;
`src/xterm.c:28425-28476`; `src/xselect.c:1712-1748`; `src/terminal.c:381-421,663-690`;
`src/termhooks.h:930-940`; `lisp/server.el:905-935,996-1012,1027-1036`; `lisp/simple.el:7058-7072`;
`lisp/loadup.el:298-312`; `lisp/term/x-win.el:1347` and `lisp/term/pgtk-win.el:112` (grep);
`lib-src/emacsclient.c:1626-1650`; `lisp/frame.el:425-447,499` (grep); `src/emacs.c:3009,3434` (grep).

---

## 19. Red-team disposition

"Accepted" means the finding was re-verified at the cited lines and the design or ledger now says
otherwise. "In part" names the part rejected and why. No finding was rejected outright. Two suggested
remedies were rejected: R2-16's separate wire type universe and R2-25's answer-from-request.
"MILES" is `plan/MILESTONES.md`.

### 19.1 R1 (code truth)

| Finding | Re-verified | Disposition | Change made |
|---|---|---|---|
| F1 second production host `TtyPopupDisplayHost` | `NM/tty_frontend.rs:30,40`; installed only for non-daemon interactive TTY (`NM/main.rs:5477-5483`, `NM/tty_init.rs:566-568`). It also makes `neomacs-surface-available-p` t under `-nw` (`ND/shader_surface/mod.rs:350-353`) | accepted | D37; §7.1 and §7.6 name it. MILES M3 folds it into the termcap backing with today's answers; M5 switches class 3 to the terminal kind; M0's `-nw` screen scenario includes a popup menu |
| F2 seam baseline does not observe TTY output | snapshot re-lays out (`NM/frame_layout.rs:111-171`, `:140,158`); painting only in `run_tty_rif_redisplay_to` (`:437-449`) | accepted | D38; L3′ is now parsed `vt100` screens (the `neomacs-tui-tests` harness has no `neovm-*` in its closure) plus the layout snapshot, for a daemon `-t` client and a `-nw` session; MILES M0 |
| F3 no "real frames" corpus; mock frames bypass the evaluator | `LE/mock_frame.rs:1-6`; the example target `crates/neomacs/examples/mock-display.rs`; one use at `engine_test.rs:31731` | accepted | L1′ uses evaluator-driven engine fixtures (`LE/engine.rs:1977-1987`). MILES M2 names the scenario list |
| F4 six runtime-used types missing from M1 | `EV/protocol.rs:54,63,83,107,156`; `EV/protocol/image_catalog.rs:221`; `DP/frame_glyphs.rs:380-388` is `cfg(neo-term)` | accepted | §7.6 DP row; MILES M1 moves all six (neo-term's behind DP's `neo-term` feature); K3 lists M15 as a DP change |
| F5 no full-suite pass baseline | RUNBOOK §4.1: "I did not run the whole suite" | accepted | D38; §14.1; MILES M0 records the baselines; every "passes" is now "no new failure against M0" |
| F6 no headless `RenderApp` | `RT/state.rs:978,1202` `pub(super)`; ignored fixture `RT/tests/input_backpressure_test.rs:219-221`; worker spawned in `RT/bootstrap.rs:713-736` | accepted | §8.2, §14.3; MILES M11b adds a public headless constructor with `FramePreparation`; M13 parity and M14 feel tests name it |
| F7 one feature set does not prevent stack copies | dev features differ (`neovm-core/Cargo.toml:211-232`, LE `:65-83`, runtime `:72-77`, `neomacs` `:135-142`); 3–4 copies on disk; about 0.37 GB of `neovm-*` rlibs per copy | accepted (measured in part) | §14.1 budget of at most five standing copies; MILES conventions; K3 |
| F8 bincode only weakly locked; defaults add `bincode_derive` | `Cargo.lock:530-538`; manifest `default = ["std","derive"]`; `bincode_derive` absent; `bincode` and `unty` crates cached | accepted | D5, §6.9 and MILES M2 declare `default-features = false, features = ["std","serde"]`; R1 Q3 is moot for it |
| F9 "no non-test code reads text positions" overstated | `RT/frame_windows.rs:886,923,1150`; `input_scroll.rs:368-383`; `DR/thread_comm.rs:73-76`; `rif.rs:3985` | accepted | §5.2 item 4 rewritten |
| F10 the compositor reads more than row anchors | `overlays.rs:54-58`; `continuity/scroll.rs:206-215`; `continuity/mod.rs:155-163,292-299`; `DP/frame_glyphs.rs:742-748` | accepted | D4 says "no hit-test text positions"; §5.2 lists the window-level facts |
| F11 seven bincode serde hazards, not two | bincode `src/features/serde/mod.rs:42-53`; 7 DP `skip` sites, 13 `skip_serializing_if`/`flatten` | accepted | §6.9 rule with the symmetric-skip exception; MILES M1 guard covers the seven |
| F12 moved TTY tests need `neovm_core` | `packed_raster_test.rs:372,429,766`; `rif_test.rs:2847,3033`; `damage_test.rs:218` | accepted | those cases move to `neomacs-display-host` (no new stack copy); the rest stay pure (§7.6; MILES M4) |
| F13 renderer helpers drag Fontconfig and probe code; renderer tests use the layout engine | `RW/glyph_atlas/mod.rs:37,2268,2328-2351`; `LE/font/subpixel.rs:27` → `fontconfig.rs:1857-1880`; `font_match.rs:202` → `probe.rs:399,692`; `glyph_atlas_test.rs:775-906` | accepted | §7.6 font-materializer row lists the code. MILES M4 splits the three contract tests at the DP boundary: the engine asserts the published DP values, and the renderer consumes equivalent values built from `neomacs-test-fonts` fixtures |
| F14 runtime feature forwards | `crates/neomacs-display-runtime/Cargo.toml:88-117` | accepted | MILES M4 drops `jit`/`sqlite` and re-points `neo-term` (`neomacs` enables `neomacs-layout-engine/neo-term`) |
| F15 font-materializer tests run in no CI job | `test-suite.yml:128` excludes it; display filter `:1236` | accepted | §7.6; MILES M10 adds it to the filter and removes the exclusion |
| F16 "no CI job runs other `neomacs` tests" too strong | `windows-installer.yml` (PR, `windows_launcher`); manual `linux.yml:91`, `native-contract.yml:114` | accepted | §14.1 says "no automatic Linux job" |
| F17 the KBOARD limitation is not documented | `docs/daemon.md:183-207` lists none; E5 recommended documenting it | accepted | §3.4; MILES M5 documents it |
| F18 neo-win audit incomplete; in-tree decision says the opposite | `neo-win.el:30-32,40,200-201,233,584,586,589,603,642-646,756`; `neo-preload.el:9-11`; `loadup.el:264-267`; `NM/main.rs:4300` | accepted | §7.2 row: full audit list, provide `neomacs` first, reversal cited and justified; pgtk's `".*"` precedent |
| F19 host-effect doc paraphrased as requiring the registry | `host-effect-projections.md:72-78`: "using existing storage; there is no new TLS word, inline Context/World field" | accepted | §7.1 reconciles (the same slot, a changed type) and M3's PR asks the doc owner |
| F20 23 accesses in 6 layout-engine files, not 6 | `rg -c` per file: 2+6+2+10+2+1 | accepted | §7.1; MILES M3 |
| F21 M5's runtime command lacks the dbus flag | M5 depends only on M3 | accepted | MILES conventions define `RT_FEATURES` from `cargo metadata` (tested on today's tree); every runtime command uses it |
| F22 the sole-active rule is unreachable in a daemon | `ND/terminal/pure.rs:286-295`; `gnu:src/termhooks.h:935-937`; `gnu:src/terminal.c:395-403`; `gnu:lisp/frame.el:446` | accepted | §12.1 row; attestation form A19. MILES M12 replaces the test with a daemon "not refused" test and a registry-level refusal test without an initial terminal |
| F23 `FrameRef` collides | `DR/thread_comm.rs:299`; runtime glob re-exports; DP has `DisplayFrameId` and `LiveDisplayWindowId` (`DP/types.rs:10,23-43`) | accepted | D40: `LiveDisplayFrameId(NonZeroU64)`; §6.8, §6.16, §7.1; MILES M1 |
| F24 identity guards unmentioned | `identity_impl_guard.rs:16-24` (`MOVED`); `neovm-display-identity/tests/dependencies.rs` (serde only); `neovm-value` already depends on the leaf | accepted | §7.6 identity row; MILES M1 scope and acceptance |
| F25 feel report location inconsistent | `crates/neomacs-perf/Cargo.toml:21` → melpa-test-support | accepted | §14.2 L6 is the ignored `daemon_lifecycle` case |
| F26 fan-out counts off by one | resolve graph: DP 15, identity 18 | accepted | MILES conventions: approximate counts, corrected |
| F27 pane-morph case in a layout-engine test | `RT/frame_windows.rs:825,868,883,908,1148` (`inverse_map`) | accepted | §14.3; MILES M8 runs that case in the runtime |
| F28 image decoder paths and coupling | `decoder.rs:6` `use super::*` over `image_cache.rs` (2,806 lines, 64 `wgpu::`); `image_probe.rs:118,127` → `image_sequence.rs` (656 lines) | accepted | §7.6 media-cpu row; MILES M10 |
| Q1 where `RequestId` lives | `NS/wait/mod.rs:339-347` private; `neovm-system` deps | answered | `HostReplyId` in `neovm-eval`, mapped by `LinkConnection` (§6.13) |
| Q2 snapshot equals last submitted at quiescence? | nothing asserts it | answered | D30 compares against the last *submitted* presentation; MILES M7 tests snapshot equality separately |
| Q3 crates.io reachable? | not probed; `bincode` and `unty` are cached | moot for bincode | §17 keeps it open for new dependencies |
| Q4 where neo-term's types go | — | answered | DP behind `neo-term` in M1; removed in M15 |
| Q5 per-copy disk cost | measured from existing artifacts | answered in part | §14.1 (no build was run to measure a fresh copy) |

### 19.2 R2 (gaps and failure modes)

| Finding | Re-verified | Disposition | Change made |
|---|---|---|---|
| R2-01 durable sessions die at the final logout on systemd hosts | pam_systemd(8) line 221; logind.conf(5) `KillUserProcesses`; loginctl(1) linger; `NM/client_daemon.rs:96-122` (`setsid`, inherited env) | accepted (blocker; decided before M7) | D31; D8's rendezvous moves to `/tmp/neomacs-<uid>`; §11.5 service-manager start, `sd_notify`, `durability` fact, linger remedy; MILES M7 (rendezvous, durability), M11 (auto-start, `sd_notify`), M12 (CI or VM only for logind) |
| R2-02 socket-directory mismatch starts a second daemon | `NM/bin/neomacsclient.rs:432-438,664-675`; `gnu:lib-src/emacsclient.c:1630-1648` only warns | accepted | environment-independent rendezvous plus the listener probe returning the server socket path; never auto-start while a daemon answers (§3.2); MILES M11 two-environment test |
| R2-03 lost `Welcome` strands the session | design text (v1 §6.5, §6.19) | accepted | two-phase rotation (§6.5); MILES M12 `remote_resume_survives_lost_welcome` |
| R2-04 roaming refused; stalls drop input; transports linger | design text; this host's `tcp_retries2`/`tcp_keepalive_time` per R2 | accepted | D34; one liveness timeout; `Stalled` state with queued input and a parallel replacement; implicit takeover; detach closes the transport (§6.19); U7 changed |
| R2-05 on-disk tokens versus per-process serials, time and replay | design text | accepted (incarnation epoch, not memory-only tokens) | `SessionHello.incarnation`; cold resume; dedupe by `received_through`; received input is kept (§6.5, §6.19); MILES M12 tests |
| R2-06 first ssh session's environment baked into the daemon | `NM/client_daemon.rs:100-110`; `gnu:lisp/server.el:1000-1011` (env has no effect) | accepted | D36 stable agent link; §11.5; divergence 8; U13; §3.3 row corrected (v1 claimed the frame's `-env` applies) |
| R2-07 C-g dropped at the input bound | `gnu:src/keyboard.c:3787-3836` (quit before the full check `:3845-3850`) | accepted | D33: `QuitKeys` config, quit edges exempt, `InputProgress` on lane 0 (§6.8, §6.12, §7.5); MILES M6 test with 5,000 unconsumed wheel events |
| R2-08 timers and filters inside waits GNU runs without Lisp | `NS/wait/mod.rs:572-582` (resize suppresses timers and processes); `gnu:src/xterm.c:28432-28475`; `gnu:src/xselect.c:1714-1745` | accepted | D16: two disciplines; display open is display-event-style; cells cancelled on any non-local exit (§6.13, §7.3); MILES M6 tests |
| R2-09 selected frame and pending prompts on a detached terminal | `gnu:src/keyboard.c:13164-13202` (`delete_kboard`); `gnu:lisp/server.el:1029-1035` | accepted | D10; §12.2: the selected frame moves, the KBOARD is released by `delete_kboard`'s rule, and `x-create-frame` works while detached; U10 has a mechanism |
| R2-10 overflow closes non-resumable sessions | `DR/thread_comm/command_projection.rs:36-86` (today blocks or times out, never tears down) | accepted | D32: the control lane is state synchronization; requests fail per call; no teardown (§6.12); MILES M6 stalled-peer test |
| R2-11 eager selection data | `gnu:src/keyboard.c:1612-1641`; Neomacs `command_loop.rs:1262-1280` | accepted | D15 lazy `Own{generation}`; inline ≤ 64 KiB; `Convert` plus bulk-lane data (§6.8); MILES M9 tests |
| R2-12 no ownership-loss signals | `DR/clipboard.rs:371-384` returns `Unknown`; `neo-win.el:436-478` | accepted | `SelectionState` replica; `selections/1` `ownership` quality; local answer only when exact; exists never fetches (§6.13, §6.17); MILES M9 |
| R2-13 user data on the remote command line | ssh(1) line 103 | accepted | D35 fixed command plus `RelayRequest`; `server.el` quoting (§3.2, §6.3); MILES M11 hostile-filename test |
| R2-14 PATH, rc output, interactive auth, stale masters | `/etc/skel/.bashrc:5-9`; `/etc/skel/.profile:19-26`; ssh(1) `SSH_ASKPASS_REQUIRE` | accepted | §8.2 ssh integration (BatchMode retries, `ControlPath=none`, askpass, a 64 KiB scan, one visible `sh -lc` retry, `--remote-command`) |
| R2-15 default policies grant laptop authority | ssh_config(5) `ForwardX11Trusted` | accepted | D27; §11.3 `on-input` defaults, private-target refusal, rate limit, GPU-program row; §6.17 frontend naga validation; U3 and U5 changed |
| R2-16 vocabulary is the wire format; old-decodes-new untested | 3 of 50 commits touch DP; `023cbbe5` added a coverage field | accepted in part | D6: `WireSchemaId`; default `schema` policy; honest limits (§6.6); `/proc/self/exe` spawn (§4.2); frozen relay records. **Rejected:** a second, designed type universe for nested vocabulary in v1 (it duplicates DP's types; AGENTS.md §8). Cross-schema evolution is horizon (§15) |
| R2-17 the hello cannot carry the inventory | v1 §6.5 had no field; 64 KiB cap | accepted | D17: no inventory; per-offer `Have` from a (dev, inode, mtime, size) hash index (§6.11) |
| R2-18 image cost unquantified; pixels never cached | u6 arithmetic; v1 `AssetId::Session` | accepted | D18: pixels content-addressed and persisted (§6.11); MILES M14 reports image workloads in bytes |
| R2-19 local performance reported, not gated | design text | accepted in part | MILES M13 gates deterministic counts here (zero row-payload copies in process, ops and bytes per typed character) and CI timing with a tolerance; shared-memory pixels are horizon pending measurement |
| R2-20 M11 and M12 cannot exercise ssh, half-open TCP or logind here | `command -v sshd` empty; `XDG_RUNTIME_DIR` unset | accepted | §14.1, §6.19; `LinkEmulator` inside the frontend for stall-without-EOF and lost-`Welcome` cases; logind marked CI or VM only; U12 for an sshd fixture |
| R2-21 no visible remote window before the CI-only M13 | no milestone created the GPU-frontend readback test | accepted | D39; MILES M11b (GPU presenter over links through a bridge M13 deletes, plus `remote_gpu_*` L5 tests); M11 relabelled as headless-verified |
| R2-22 CI coverage not established | `test-suite.yml:3-10`; `build-gate.yml:87`; `gui-tests-macos.yml:16-38`; no upstream | accepted | U11 before M4; the macOS path filter updated in the PRs that add crates; build-gate state recorded per milestone (D38) |
| R2-23 detached-state edge cases | design text | accepted | §6.19 and §12.2: desired visibility kept apart; hooks alternate; the last activated presentation stays resolvable |
| R2-24 the single-kboard condition dropped | `gnu:src/keyboard.c:3794` | accepted | §7.5; MILES M6 test (every C-g quits before M16); M16's test scoped to single-kboard state |
| R2-25 child-frame popups pay an RTT per resize-then-query | eight callers of `sync_live_gui_resize_for_geometry_queries` (`ND/frame/mod.rs:2801-2809`) | accepted in part | §13.2 row; MILES M14 measures it. **Rejected:** answering getters from the requested size (the window manager may adjust it, so Lisp would read a wrong size; GNU waits too) |
| R2-26 `fchmod` on the socket is not dependable | unix(7) lines 230-252 | accepted | §11.1 `chmod(2)` on the path; MILES M7 test stats the path |
| R2-27 Lisp variables read on non-Lisp threads | design text | accepted | evaluator-published snapshots (§4.4, §10) |
| R2-28 retention depends on peer `Retired`; tombstones; detached memory | `DP/presented_pointer.rs:683-687`; `ND/terminal/pure.rs:594-605` | accepted | §6.18 cap of 4 retained presentations per frame; §7.2 monotonic counter, no tombstones; `neomacs-display-info` reports retained bytes |
| R2-29 transport-dependent semantics; `--sync-fonts` mutates the remote | design text | accepted in part | lifecycle difference documented (§6.2); `--sync-fonts` goes to a private app-font directory (§9). Same-host proof is horizon (§15) |
| R2-30 frontend chrome text versus the Lisp-host layout rule | design text | accepted | §8.2 scoped "frontend chrome" rule |
| R2-31 attach failures invisible; the relay parses more than claimed | design text | accepted | `RelayStatus`; `server.el` errors on stderr and a non-zero exit; the parsing claim amended (§3.2, §6.3) |
| R2-32 selection wait 5 s → unbounded | `NM/main.rs:1212,1846`; `DR/clipboard.rs:137` | accepted | §6.13: the frontend's own 5 s reply bound; divergence 12 records the local change |
| R2-33 `emacsclient -c` parity needs `-d` | `NM/bin/neomacsclient.rs:839-857` | accepted | `NEOMACS_DISPLAY` (§3.3, §11.4); divergence 9; MILES M7 |
| R2-34 one listener thread does handshakes | design text | accepted | per-connection tasks with a 10 s deadline, at most 16 (§4.4, §6.18); MILES M7 test |
| R2-35 platform defaults | label grammar versus hostnames; `build-gate.yml` targets | accepted | §8.2 label sanitizing and platform directories; §11.5 pure-crate tests on macOS and Windows; MILES M2 CI matrix |
| Q1 cross-process resume intended? | — | yes | incarnation epoch (R2-05) |
| Q2 occluded presenter stalls credits? | `pacing.rs` per R2 | open, harmless | §17; measured in M13 on macOS CI |
| Q3 macOS App Nap | — | open | activity assertion (§11.5); CI |
| Q4 macOS `TMPDIR` | — | moot | the rendezvous never reads `TMPDIR`; the probe returns the server path |
| Q5 CI access | — | user decision | U11 |
| Q6 user-mode sshd disk | — | user decision | U12 |
