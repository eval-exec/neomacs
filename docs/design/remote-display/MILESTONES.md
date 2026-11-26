# MILESTONES: remote display for Neomacs (issue #587)

This is the ordered ledger for implementing `DESIGN.md` (version 2). Each
milestone lands on its own and keeps every existing suite free of new failures against the baselines that
M0 records. Each has acceptance commands that run **on this host**: no GPU, no `libxkbcommon-x11`, no
sshd, and no logind session; debug binaries plus the bootstrap image (RUNBOOK §0). What only CI or a VM
can verify is listed under "Not runnable here" and is never claimed from this host.

- **Created:** 2026-11-24 by the design synthesis. **Revised:** 2026-11-24 (revision 2) after the red-team
  reports `design/r1-code-truth.md` and `design/r2-gaps.md`. DESIGN §19 maps every finding to the change
  it caused; finding ids (R1 F*n*, R2-*nn*) are cited inline below. **Tree:** `the repository`
  `feat/remote-display` at `72cba7f0`. **GNU:** `a360712c9d272d950d8d8255ef74570f7e90b7d9`.
- **Status values:** `todo` → `in-progress` → `landed` (with the PR link and evidence path) or `blocked`
  (with the reason). Only the agent that owns a milestone edits its Status line.
- Every GNU behaviour named here is a source contract and is **UNATTESTED** until an oracle run exists
  (AGENTS.md §1). PR bodies follow AGENTS.md §10, including a Design horizon section scoped to the
  milestone.
- **Revision 2 changed the plan as follows.**
  - New **M0** records the baselines and the TTY screen oracle (D38).
  - New **M11b** brings a GPU presenter over a link before the in-process conversion (D39).
  - Acceptance is now "no new failure against M0" instead of "passes" (R1 F5).
  - Every runtime command uses `$RT_FEATURES` (R1 F21).
  - The disk budget counts dev-feature copies (R1 F7).
  - M7, M9–M14 and M16 absorb the R2 design changes.

## Conventions for every milestone

**Command prefix.** Every command below assumes this, in one shell:

```sh
. campaign:env.sh      # PATH, CARGO_TARGET_DIR=target, CARGO_INCREMENTAL=0
cd the repository                   # cargo needs the repo's rust-toolchain.toml; it fails from other dirs
E=campaign:evidence; B=$E/baselines
NX='--no-fail-fast --status-level all --final-status-level none'
# The runtime needs the dbus flag only while it still depends on neovm-core (before M4). This selector
# was run on 72cba7f0 in revision 2 and printed the flag.
RT_FEATURES=$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; m=json.load(sys.stdin); p=[p for p in m["packages"] if p["name"]=="neomacs-display-runtime"][0]; print("--features neovm-core/dbus-vendored" if any(d["name"]=="neovm-core" and d["kind"] is None for d in p["dependencies"]) else "")')
BL=campaign:scripts/nextest-baseline.py   # created by M0
# Packages added to Cargo.lock since 72cba7f0 that are not workspace members. It prints nothing when no
# external package was added (run on 72cba7f0 in revision 2: empty, exit 0).
new_external_packages() { comm -23 <(git diff 72cba7f0 -- Cargo.lock | sed -n 's/^+name = "\(.*\)"/\1/p' | sort -u) <(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print("\n".join(p["name"] for p in json.load(sys.stdin)["packages"]))' | sort -u); }
```

**Baseline-relative acceptance (D38).** "Passes" means **no test that passed in M0's baseline fails, and
every test the milestone adds passes**. Each full-suite command is run as
`cargo nextest run -p PKG … $NX 2>&1 | tee $E/mNN-PKG.log` and then
`python3 -I $BL compare $B/PKG.json $E/mNN-PKG.log [--moved MAP]`, which exits 0 or lists the
regressions. A test that moved to another package is compared under its new id through `MAP`, and that
package runs in the same invocation (`-p PKG -p DEST …`): `compare` reads one log and counts a passing
baseline test absent from it as a regression (D46; generalized in M4 fix round 2). The
acceptance blocks below abbreviate this pair as `⟨compare PKG⟩`. A baseline failure stays allowed only
while the same test keeps failing for the same reason; a fix that turns it green is reported, not
required.

**Feature flags and disk (R1 F7).**

- *Flags.* Use `-p neovm-core --features dbus-vendored`, `-p neomacs --features dbus-vendored`,
  `--features neovm-core/dbus-vendored` for any other package that depends on `neovm-core` directly, and
  `$RT_FEATURES` for the display runtime. Packages that do not depend on `neovm-core` take no flag.
- *Never `--workspace`*: it builds `neovm-oracle-tests` and `neomacs-melpa-tests`.
- *Copies come from dev features.* Copies of the opt-2 `neovm-*` stack come from each package's
  *dev-dependency features*, not from these flags. Core, layout-engine, runtime and `neomacs` test builds
  are separate copies today (DESIGN §14.1). Revision 2 measured about 0.37 GB of `neovm-*` rlibs per copy
  plus 0.1–0.7 GB of test executables per package. That puts one copy at roughly 1–1.5 GB without
  incremental artifacts [E].
- *Budget.* At most five standing copies (dev bins, core, layout engine, `neomacs`, and display-host from
  M4; the runtime's copy disappears at M4). A source edit rebuilds a copy in place and does not add one.
- *Free space.* Free disk was 16 GB at revision time. Every milestone records `df -h /home` before and
  after its first build in its evidence log.

**Evidence.** Each milestone writes its command transcripts to `$E/mNN-<slug>.log` and names that file in
its Status line.

**Rebuild the dev binaries for manual runs.** After any `nextest` run of package `neomacs`,
`target/debug/neomacs` is the test-profile link (RUNBOOK gotcha 3). Before manual runs or the seam
oracle, run `cargo build -p neomacs --bins --features dbus-vendored` and
`campaign:bin/neomacs-provision-bootstrap-image`.

**Test placement rules** (u10 §3.1, §3.3):

- **End-to-end daemon tests** are **root-level** `#[test]`s in `crates/neomacs/tests/daemon_lifecycle.rs`,
  named `remote_*`, `multi_tty_*`, `display_names_*` or `kboard_*`.
  - Root-level cases in that target are serialized automatically (`.config/nextest.toml:287-303`). They
    run in CI's `neomacs-daemon-lifecycle` job (`.github/workflows/test-suite.yml:183`).
  - The fixture isolates `HOME`, `XDG_*` and `TMPDIR` (`daemon_lifecycle.rs:203-218`). Remote tests also
    set `NEOMACS_DISPLAY_RENDEZVOUS_DIR` to a per-test directory, identically for the daemon and the
    frontend.
  - Never put Linux remote tests in `neomacs_integration` or in `neomacs` lib or bin unit tests: no
    automatic Linux CI job runs them (R1 F16).
- **Evaluator tests** that need the full stack and `RecordingDisplayHost` go in neovm-core's existing
  `moved_display_host` suite (`crates/neovm-core/tests/moved_display_host/`), so
  `-p neovm-core --features dbus-vendored` runs them.
- **CI wiring.** Every new crate, and every crate whose tests start to matter, is added in the PR that
  creates it to the display job's `NEXTEST_FILTER` (`.github/workflows/test-suite.yml:1236`). It is also
  added to the macOS GUI suite's path filter (`.github/workflows/gui-tests-macos.yml:16-38`, R2-22).
  `neomacs-font-materializer` is also removed from the archive's exclusion list
  (`test-suite.yml:128`, R1 F15) in M10.
- **Test targets.** Every new test file is mounted (orphan guard). One integration-test binary per crate
  unless `check-test-targets.py` is amended with a justified row.

**"Byte-identical" means, operationally:**

1. every suite named in the milestone shows no new failure against M0 (`⟨compare⟩`), with no test
   rewritten to pass;
2. `seam_baseline` in `compare` mode passes. It compares the **parsed TTY screens** and the `text-faces`
   layout snapshots of both M0 scenarios (a daemon `-t` client and a non-daemon `-nw` session) with the M0
   baseline. The snapshot alone never reaches `TtyRif`, which is why the screens are part of it (R1 F2);
3. GUI pixel identity is **not runnable here**. It is CI L5 (`neomacs-gui-tests`), conditional on CI
   access (user decision U11), and stated as such.

**Compile fan-out reference** (approximate; transitive normal and build dependents in the Linux resolve
graph, revision 2):

| Package changed | Dependents | Rebuild time |
|---|---|---|
| `neomacs-display-protocol` | 15 | ≈2–2.5 min dev rebuild of the `neomacs` bins; measured 127 s window |
| `neovm-display-identity` | 18 | — |
| `neovm-eval` | 8 | 63 s rustc alone |
| `neovm-display` | 6 | — |
| `neovm-core` | 5 | ≈45 s |
| layout engine | 3 | ≈40 s |
| renderer | 2 | ≈26 s |
| display runtime | 1 | ≈18 s |
| a new leaf crate | — | seconds; its tests build without the `neovm-*` stack |

A test run that reaches a `neovm-*` stack copy after one of those changes rebuilds that copy. The first
`neovm-core` test build took 4 min 03 s (RUNBOOK §4.1). An edit to `host_effect_guard_test.rs` costs a
full `neovm-core` lib test build.

**Acceptance-command check (revision 2).**

- *Packages and test targets* used below exist (`cargo metadata --no-deps`): `neovm-core`
  (`dbus-vendored`), `neomacs` (`dbus-vendored`; tests `daemon_lifecycle`, `neomacs_integration`,
  `windows_launcher`), `neomacs-display-runtime` (`--lib`; `neo-term`, `jit`, `sqlite`),
  `neomacs-display-protocol` (`identity_impl_guard`), `neomacs-layout-engine` (`layout`),
  `neomacs-renderer-wgpu` (`renderer`), `neomacs-font-materializer` (`materializer_test`),
  `neomacs-tui-tests` (`tui`), `neovm-display-identity` (`dependencies`).
- *Ran in revision 2 on the untouched tree:* `check-test-targets.py` (`ok: checked 35 crates`),
  `cargo fmt --all --check` (exit 0) and the `RT_FEATURES` selector. `cargo nextest` 0.9.152 accepts
  `--no-fail-fast`, `--status-level all`, `--final-status-level none` and `--run-ignored only`.
- *Created by their own milestones:* every other command names a crate, test or script that does not
  exist yet. Nothing requires a GPU, sshd or logind except the items under "Not runnable here".

## Dependency graph and recommended order

```
M0 ──► M1 ──► M2 ───────────────────┐
M0 ──► M3 ──► M5 ───────────────────┤
       M1 ──► M4 (crate cut) ───────┴──► M6 ──► M7 ──┬──► M8 ─────────────────────────────────┐
                                                     ├──► M9 ───┐                             │
                                                     ├──► M10 ──┼──► M11 ──┐                  │
                                                     │          └──► M11b ─┴──► M12           ▼
                                                     │     (M4, M8, M9, M10, M11b) ─────────► M13 ──► M14
                                                     └──► M15 (any time after M7)
       M5 ──► M16 (any time after M5)
```

- **Parallel starts.** M0 comes first, with no source edit. M1 and M3 then start in parallel, and M4
  needs only M1. M3 and M4 touch `NM/frame_layout.rs` and `NM/secondary_tty.rs`, so whichever lands
  second rebases.
- **Joins.** M6 joins three lines (M2, M4, M5). M11b needs M4, M6 and M10. M12 needs M11 and M11b. M15
  and M16 can land in parallel with the M8–M14 line.
- **U11 (the CI route) is needed before M4.** It is the first milestone whose regression surface
  includes CI-only GUI suites.

**M1–M4 are byte-identical seam refactors.** M5 is the first Lisp-visible change (GNU-ward). **M7 is the
end-to-end milestone**: a daemon renders into a separate frontend process over a socket, verified
headlessly. **M11b shows the first remote window** (CI). **The decisions R2-01 required before M7**
(daemon lifetime and rendezvous, D8 and D31) are made in DESIGN v2.

| # | Title | Kind | Depends on | Status |
|---|---|---|---|---|
| M0 | Baselines and the TTY seam oracle | tooling and evidence (no product edit) | — | in-progress (uncommitted, acceptance green) |
| M1 | Boundary vocabulary and the exact input frontier in the protocol crate | byte-identical | M0 | in-progress (uncommitted, acceptance green) |
| M2 | Pure protocol crates: session layer and display wire codec | byte-identical (additive) | M1 | landed (5a7a7885, local branch) |
| M3 | Per-terminal display registry, one entry today | byte-identical | M0 | landed (267fe940, local branch) |
| M4 | Frontend runtime without the Lisp machine (crate cut) | byte-identical | M1 | landed (c4e7100d, local branch) |
| M5 | GNU multi-terminal parity in the evaluator | Lisp-visible, toward GNU | M3 | todo |
| M6 | Evaluator display-link host and headless frontend (L2) | additive | M2, M4, M5 | todo |
| M7 | End-to-end: a daemon renders into a separate frontend process | new feature | M6 | todo |
| M8 | Evaluator-side pointer resolution for every display | behaviour-preserving (differential) | M7 | todo |
| M9 | Synchronous services, replicas, typed capabilities and side-effect policy | Lisp-visible | M7 | todo |
| M10 | Assets: fonts by content, images decoded on the evaluator | Lisp-visible (`image-cache-size`) | M7 | todo |
| M11 | Relay and launcher: `neomacs --remote` over ssh (headless-verified) | new feature | M9, M10 | todo |
| M11b | GPU presenter over a link (first visible remote window, CI-verified) | new feature | M4, M6, M10 | todo |
| M12 | Durable sessions: detach and resume below Lisp | new feature | M11, M11b | todo |
| M13 | Local GUI convergence: one protocol for every display; headless daemons | behaviour-preserving + C95 fix | M4, M8, M9, M10, M11b | todo |
| M14 | Feel: pacing and presentation-only prediction | new feature | M13 | todo |
| M15 | neo-term on the evaluator | Lisp-visible | M7 | todo |
| M16 | KBOARD completion (multi-client parity) | Lisp-visible, toward GNU | M5 | todo |

---

## M0 — Baselines and the TTY seam oracle

- **Status:** landed (eae48104, local branch) — commits `817f0a6b` (screen dump) and `eae48104` (seam oracle)
  on `feat/remote-display`, no PR opened (CI route U11 open). PR body: `evidence/M0/PR.md`. Baselines:
  neovm-core 9474 run / 7498 pass / 1975 fail / 1 timeout (about 1759 of those fail only for missing `.elc`
  files, since `fresh-build` is unusable here, so roughly 21% of the suite is blind to regressions on this
  host); layout engine 3750 / 3743 / 7; runtime `--lib` 1332 run / 1307 / 25 / 1 skip; DP 942/942; identity
  1/1; `daemon_lifecycle` 35/35. Seam baseline `evidence/baselines/seam/` as recaptured in fix-r1/fix-r2.
  - The oracle refuses a stale binary or a binary a test build linked (cargo dep-info and inode rule) and an
    image older than the editor or dumped by another build. It does not detect a `cargo build` with other
    features; the rule rests on cargo 1.96.1's dep-info write order (`fix-r3.md`, Limits).
- **Evidence:** records `evidence/baselines/*.json` (from `evidence/m00-<suite>.log`); current seam baseline
  backed by `evidence/M0/fix-r1/acceptance-seam-{capture,compare}.log`, `evidence/M0/fix-r2/acceptance.log`,
  `evidence/M0/fix-r3/e2e/f5b-official-seam-compare.log` (the older `m00-seam-*.log` and
  `seam-compare-repeat-*` back the pre-fix-r1 capture); final independent verification
  `evidence/M0/verify-r4-driver.log` (all six suites 0 new failures, seam capture/compare/official 2 passed
  each); post-commit run at `eae48104` `evidence/M0/commit/driver.log` (fmt, check-test-targets, tui lib
  102/102, build + provision, official seam compare 2 passed); reviews `evidence/M0/review-r{1..4}-*.md`,
  fixes `evidence/M0/fix-r{1,2,3}.md`; CI state `evidence/m00-ci-state.log`; index `plan/M0-progress.md`.
- **Follow-ups (unresolved minor review findings, accepted at landing; owner: next agent that touches the
  oracle, M5 for the menu-bar item).**
  1. *Menu-bar premise.* `seam_baseline.rs` `Coverage::PrimaryTtyOnly` (≈:137/:186/:196) says a daemon client
     frame "has no menu bar to open". That is current Neomacs behaviour, not inherent: GNU gives tty frames a
     menu-bar row (`src/frame.c:1457-1458`, `:1602-1603`, `menu-bar-mode` t at `:7534`) and `menu-bar-open`
     falls back to `tmm-menubar` (`lisp/menu-bar.el:2856-2880`), all UNATTESTED. Reword as a divergence and
     extend the menu step to the client frame in M5.
  2. *Capture not atomic.* Capture runs `recreate(&expected)` (≈:822) before the new run succeeds, so a failed
     capture deletes the good baseline.
  3. *Shared `scenario.el`.* Both arrangements share one file, installed by either capture
     (`write_scenario_file` ≈:1520/:1663); a capture of one arrangement can let the other's stale baseline
     pass the scenario-changed guard. Make it per arrangement.
  4. *Environment not recorded.* `COLORTERM=truecolor` (≈:419) is not in `scenario.el`; the client binary's
     freshness against the editor is not checked (the image is).
  5. *Binary location.* Bins are taken from `$CARGO_TARGET_DIR`/`<workspace>/target/debug` (≈:409/:411), the
     test only checks they exist and nextest does not rebuild them; it ignores the harness's
     `NEOMACS_TUI_NEOMACS_BIN` convention, and `docs/building.md:61-62` (TUI harness uses the release binary)
     documents neither this nor `NEOMACS_SEAM_BASELINE{,_DIR}`.
  6. *Internal ids gated.* The text-faces files contain internal handles (`=== frame 4294967297`, window 3,
     `face:20`, ≈:642) that GNU does not show; normalise them before M3 changes allocation.
  7. *Exit not fully gated.* `leave_terminal` accepts any exit outcome and `stop_server` only prints the
     daemon's status (≈:1352). The doc (≈:33) claims the exit dump shows the terminal was restored, but
     `CanonicalScreenDump` omits the SGR pen and insert mode that GNU `tty_reset_terminal_modes` resets
     (`term.c:230-250`, UNATTESTED).
  8. *Uncited GNU claim.* `src/tests/screen_dump_test.rs:51-53` cites `tty_clear_end_of_line` without a
     location or UNATTESTED mark (the source agrees: `term.c:505`).
  9. *`#[ignore]` versus feature gate.* The crate's convention puts real-editor tools behind an opt-in feature
     (`doom-repro`; `Cargo.toml:24`, `build-gate.yml:136`). The ledger mandates `#[ignore]`; the PR body
     records the exception, which still needs a maintainer decision.
  10. *`.git/index` refusal.* The bootstrap dump rewrites `.git/index`, which `neomacs.d` names as a source
      (`crates/neomacs/build.rs:175`), so the first "build, provision, compare" after a tracked-file edit is
      refused (`fix-r3/e2e/f3d-compare-after-reprovision.log:106,125`); M2's acceptance (`git diff`, then seam
      compare, ≈:508) hits the same. Set `GIT_OPTIONAL_LOCKS=0` in the provision script and in infra's
      `isolated_environment`, and explain the refusal in `REBUILD`. Fails closed meanwhile.
  11. *Three freshness rules.* Image provenance lives in infra (`runtime_image.rs:291`, mtime only), the
      mission script (`cmp`) and the oracle (`image_dumper`). `ensure_role_binary` keeps the cross-build bug
      (fix-r3 Limits); `neomacs-infra` is a dev-dependency only, so the fix is test-infra scope.
  12. *Dispositions.* Rounds 1–3 compat/architecture minors (items 1, 3, 6, 8, 9) were never dispositioned in
      `fix-r{1,2,3}.md`; this list is their disposition (deferred, open).
- **Kind:** tooling and evidence. No product crate is edited; the product binaries stay those of
  `72cba7f0`.
- **Goal.** Define "green" before the first source edit (D38, R1 F2, R1 F5): full-suite baselines of
  every package the campaign touches, a TTY output oracle that observes what the terminal receives, and
  the CI state.
- **Depends on:** —.
- **Scope.**
  - `campaign:scripts/nextest-baseline.py {record|compare}` (mission tooling). It parses
    nextest output produced with `$NX` into a JSON map from test id to status (`PASS`, `FAIL`, `SKIP`,
    `TIMEOUT`, `LEAK`). `compare` exits 1 when a test that passed at baseline did not pass now, or when a
    test absent from the baseline did not pass. It accepts `--moved MAP` (lines `old-id new-id`) for
    crate cuts and prints every status change.
  - **Baselines on the untouched tree**, recorded under `$B/`:
    - `neovm-core` full (9,474 tests listed by RUNBOOK §4.1, never run as a suite before);
    - `neomacs-layout-engine` full;
    - `neomacs-display-runtime --lib`;
    - `neomacs-display-protocol` full;
    - `neovm-display-identity` full;
    - `neomacs --test daemon_lifecycle` (35/35 in RB 09).
  - **`crates/neomacs-tui-tests/tests/seam_baseline.rs`**, a module of the one `tui` test binary,
    mounted in `tests/tui.rs`. It is test-harness code only, and its build closure contains no `neovm-*`
    crate (`cargo metadata`, revision 2). It has two `#[ignore]` scenarios, driven by
    `NEOMACS_SEAM_BASELINE=capture|compare` and `NEOMACS_SEAM_BASELINE_DIR`:
    - `seam_baseline_daemon_tty_client`. It starts `env -u DISPLAY -u WAYLAND_DISPLAY
      target/debug/neomacs -Q --fg-daemon=seam` with isolated `HOME`, `XDG_*` and `TMPDIR`, as the
      `daemon_lifecycle` fixture does (`daemon_lifecycle.rs:163-221`). It then attaches
      `target/debug/neomacsclient -s seam -t` in a 100×30 pty through `TuiSession` (`TuiLaunch`,
      `crates/neomacs-tui-tests/src/launch.rs:11-60`) with `TERM=xterm-256color`.
    - `seam_baseline_primary_tty`. It runs `target/debug/neomacs -Q -nw --eval '(server-start)'` with
      the server name `seam-nw` in a 100×30 pty. This is the non-daemon path whose
      `TtyPopupDisplayHost` the daemon tests never reach (R1 F1).
    - **Scenario** (driven through `neomacsclient -s NAME -e`, plus keystrokes for the menu):
      1. insert text in `*scratch*`;
      2. split the window;
      3. open a file from `etc/` and `scroll-up-command`;
      4. show an echo-area message;
      5. set a custom `mode-line-format`;
      6. open the menu bar with `menu-bar-open` and dismiss it with `C-g` (`-nw` only);
      7. delete the split.
    - **Per step**: wait for quiescence (screen unchanged for 500 ms). Record the **parsed screen** as a
      canonical per-cell dump of character and attributes from `vt100::Screen`
      (`crates/neomacs-tui-tests/src/lib.rs:980`). Record the `text-faces` snapshot of the client frame
      (`(neomacs--frame-snapshot F 'text-faces)`). Keep the raw pty bytes for diagnosis only; they are
      not gated, because damage timing may legitimately vary.
    - The fixed scenario forms are checked in under `$B/seam/scenario.el`.
  - **CI state** (U11): the route by which campaign PRs get CI. Where access exists, also the build
    gate's state for `72cba7f0`. It is documented as known-red on the `neovm-eval` lib-test target
    (`.github/workflows/build-gate.yml:87`).
- **Acceptance (run here).**

  ```sh
  df -h /home | tee $E/m00-disk-before.txt
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  for spec in "neovm-core|-p neovm-core --features dbus-vendored" \
              "neomacs-layout-engine|-p neomacs-layout-engine --features neovm-core/dbus-vendored" \
              "neomacs-display-runtime|-p neomacs-display-runtime $RT_FEATURES --lib" \
              "neomacs-display-protocol|-p neomacs-display-protocol" \
              "neovm-display-identity|-p neovm-display-identity" \
              "daemon_lifecycle|-p neomacs --features dbus-vendored --test daemon_lifecycle"; do
    name=${spec%%|*}; args=${spec#*|}
    cargo nextest run $args $NX 2>&1 | tee $E/m00-$name.log
    python3 -I $BL record $B/$name.json < $E/m00-$name.log     # writes pass/fail/skip counts too
  done
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=capture NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'
  #   → 2 passed: the second run reproduces the first exactly (self-consistency)
  df -h /home | tee $E/m00-disk-after.txt
  ```
- **Not runnable here:** the build gate's state without CI access (U11).
- **Risks.**
  - *Duration.* The full `neovm-core` suite's duration is unknown.
  - *Disk.* The layout-engine and runtime test profiles may add two stack copies, about 2–3 GB [E];
    `df` before and after bounds it.
  - *Determinism.* If two captures differ, the scenario is fixed first, before M1. A non-deterministic
    oracle is not an acceptance criterion.

---

## M1 — Boundary vocabulary and the exact input frontier in the protocol crate

- **Status:** landed (80c6c9f8, local branch) — one commit on `feat/remote-display` over `eae48104`, no PR
  opened (CI route U11 open). PR body: `evidence/M1/PR.md`. Results (independent verification round 3, tree
  identical to the commit, plain compares against M0, no `--moved`): DP 974/974 (+32 new), identity 1/1,
  neovm-core 7500 pass (+2 new, 0 new failures), runtime `--lib` 1310 pass (+3 new, 0 new failures), layout
  engine 3747 pass (+3 new, 0 new failures), `daemon_lifecycle` 35/35, bins + provision, seam compare 2 passed,
  check-test-targets ok, fmt clean, no new package; extras `neovm-value` 1403, `neovm-edit` 492, `neovm-eval`
  4678, `neovm-display` 1575, all passing.
- **Evidence:** verification `evidence/M1/verify-r3-driver.{sh,log}`, `verify-r3-summary.log`,
  `verify-r3-{dp,identity,core,rt,le,dl,seam}.log` and `verify-r3-compare-*.log`; post-commit
  `evidence/M1/commit/post-commit.log` (clean tree, fmt, check-test-targets); red runs `evidence/M1/red.log`,
  `fix-r1/red*.log`, `fix-r2/red-*.log`; fixes `fix-r1.md`, `fix-r2.md` (with `fix-r2/verify.log`); reviews
  `review-r{1,2}-*.md`; notes `plan/M1-progress.md`. Superseded: `driver.log`, `post.log`, `compare-m01-*.log`
  and `verify-r{1,2}-*` (pre-fix trees; the first DP compare passed only through `moved.map`).
- **Amendment (landing).** The neo-term vocabulary (`TerminalId`, `TerminalDisplayTarget`,
  `TerminalGridSize`, `TerminalFloatPlacement`) is in DP **unconditionally**, not behind DP's `neo-term`
  feature as Scope says: `neovm-eval` names the types in its display-host API and has no feature to forward,
  and turning DP's feature on from the evaluator would enable `FrameGlyph::Terminal` in every evaluator graph.
  M15 still removes them. `FrontierSnapshot::Indeterminate` carries `reflected_through` (D41).
- **Follow-ups (unresolved minor review findings, accepted at landing).** "Resolved" marks a finding that a
  source reading of `80c6c9f8` shows the fix rounds already addressed. Owner: M2 unless noted.
  1. *Materialize guard incomplete.* `DP/glyph_matrix.rs:3808` `validate_for_materialize` is documented as a
     decode guard, but `try_materialize` still reaches three `expect`s in `for_each_glyph` (xwidget origin,
     clip, width; ≈:3830-3847).
  2. *`MaterializeError::HitIndex` doc.* `glyph_matrix.rs:2220` says it covers a foreign-presentation hit
     index; that case is reported as `SpatialProjection(StalePresentation)`, and the test asserts it.
  3. *Frontier decode.* `DP/input_progress.rs:197`: in-progress serials are now bounded by `consumed_through`
     on decode (resolved; `frontier_decode_refuses_in_progress_beyond_consumed`); still open: a `checkpoint()`
     `Vec<FrontierSnapshot>` may repeat a stream.
  4. *`FrameFontSize::Points` decode range* (`DP/display_vocab.rs:208-209`). Resolved: `FontPoints` decodes
     through `TryFrom<f64>` (`frame_font_size_decode_applies_the_point_range`).
  5. *Cancellation and capacity.* `input_progress.rs:330,349`: cancelled serials never advance
     `observed_through`; only a later consumed tracked serial passes them, so a trailing run of 1024
     cancellations disables tracking permanently. Capacity returns only when a presented frame is observed,
     up to the completed prefix; the old `Drop` ledger freed it on completion.
  6. *Stream identity.* `DR/thread_comm.rs:1054`: the fixed `InputStreamId::FIRST` and a never-pruned
     `consumed_through` map can alias streams; the old code had unique per-process ids and pruned dead streams
     through `Weak`. With M2/M6 session stream ids.
  7. *Redisplay on cancellation.* `DR/thread_comm.rs:1283`, `EV/command_loop.rs:1436`: "presentations are
     triggered exactly as today" has an exception. A tracked input cancelled at a full or disconnected queue
     used to advance the shared `through` and change `RedisplaySignature`; now `completed_prefixes` moves only
     on consumption and the evaluator never sees the cancellation. The equivalence guard excludes cancelled
     inputs.
  8. *`settle` coarseness.* `input_progress.rs:255`: when layout consumed input the whole stream becomes
     `Indeterminate`, so serials above the completed prefix that the pre-layout snapshot had as `Reflected`
     become `Pending`; the old checkpoint acknowledged them. The engine's settle wiring has no test of this.
  9. *`TerminalDisplayTarget::Window{buffer: u64}`* (`DP/terminal_vocab.rs:17`): went from the `BufferId`
     newtype to a raw `u64`, losing the distinction from DP's other `u64` ids (AGENTS.md §5). Owner: M15 or a
     DP buffer-id newtype.
  10. *neo-term imports in `main.rs:192`.* Resolved: the imports are `cfg(feature = "neo-term")` at
      `80c6c9f8`. The unconditional DP vocabulary is the amendment above.
  11. *`layout_scale` source.* `LE/engine.rs:4649` stamps it from a re-read of the frame at seal time, not
      from the `device_scale` that `prepare_frame_font_inputs` measured with (`engine.rs:2052-2068`).
  12. *Second type universes.* `EV/runtime/eval/mod.rs:2992`: the evaluator's `ShaderSurfaceLanguage`,
      `ShaderSurfaceUniformInit` and `SurfaceChannelKind` are hand-mapped to DP types (`main.rs:1701-1730`);
      `WindowVisibility` and `FrameVisibility` are identical wire enums joined by a free function
      (`main.rs:1708`). DESIGN §6.9 says "no second type universe".
  13. *`ModifierBits` stops short of the seam.* `DR/thread_comm.rs:115`: `InputEvent::Key`, the pointer actions
      and the evaluator's `render_modifiers_to_modifiers` still carry `modifiers: u32`; producers call
      `.bits()`; no wire-reachable type contains `ModifierBits` although `WIRE_ROOTS` lists it. M2's wire `Key`.
  14. *`InputScroll` ledger.* `RT/frame_compositor/input_scroll.rs:156` takes its cancellation ledger from the
      first `InputDelivery` as `Option<InputStream>` and never updates it; `RenderComms` owns the ledger and
      should hand it over explicitly (per stream, M2/M6).
  15. *`DisplayTime` derives* (`DP/frontend_input.rs:19`). Resolved in fix r2: no `Default`/`Ord`, asserted.
  16. *`docs/design/input-keysyms.md:133`.* Resolved: names `frontend_input::native_key`.
  17. *`Indeterminate.reflected_through` vs DESIGN.* DESIGN §6.14 (`DESIGN.md:1075`) and §M14 (`:1896`) still
      say `Indeterminate { stream }`; D41 records the change. Owner: orchestrator (DESIGN text).
  18. *Stale evidence citations.* The Status line and `M1-progress.md` cited pre-fix-r1 evidence and numbers.
      Resolved: both cite the r2/r3 runs. `M1-progress`'s "0 clippy findings on changed lines" refers to the
      warn-level run; the verifier's all-targets run had 14 unused imports on rewritten test-import lines,
      also unused at HEAD, 13 removed in fix r1.
- **Kind:** byte-identical refactor (one batched DP change).
- **Goal.** Every type that crosses the evaluator↔frontend seam, or that NDP/1 will carry, is a
  `neomacs-display-protocol` (or `neovm-display-identity`) type with serde usable by a non-self-describing
  codec. The input frontier becomes value-based and exact (`FrontierSnapshot`), fixing the latent
  nested-reader stall before any prediction exists (DESIGN §6.14). Runtime behaviour does not change.
- **Depends on:** M0.
- **Scope.**
  - `crates/neovm-display-identity/src/lib.rs`: receive `GuiResizeRequestId` and `FrameVisibility` from
    `crates/neovm-value/src/display_vocab.rs:15-20,92-101` with serde derives only. `neovm-value`
    re-exports them; it already depends on the leaf (`crates/neovm-value/Cargo.toml:25`). Both existing
    guards must stay green (R1 F24):
    - `crates/neovm-display-identity/tests/dependencies.rs`: the leaf depends only on serde.
    - `crates/neomacs-display-protocol/tests/identity_impl_guard.rs`: DP never implements a type in
      `MOVED`.
  - `crates/neomacs-display-protocol/src/`:
    - **input vocabulary**:
      - `FrontendKey` (from `EV/keyboard.rs:1182-1190`);
      - one `ModifierBits` replacing the three copies (`DR/backend/wgpu/events.rs:32-37`,
        `DP/modifier_policy.rs:254`, the evaluator copy);
      - `DisplayTime`, `InputStreamId`, `InputSerial`, `PointerTarget`;
      - the native-key mapping functions now called from `RT/input.rs:273-277`;
    - **window and display vocabulary**:
      - `LiveDisplayFrameId(NonZeroU64)` beside DP's `LiveDisplayWindowId` (`DP/types.rs:23-43`). It is
        **not** a `FrameRef`, which would collide with the runtime's `FrameRef` enum
        (`DR/thread_comm.rs:299`) until M13 (D40, R1 F23);
      - `GuiFrameGeometryHints` (from `crates/neovm-edit/src/emacs_core/editing/display_metrics.rs:540-548`);
      - `WindowVisibility`, `WindowFullscreenMode`, `MonitorInfo`;
      - `SystemFonts`/`SystemFontName`/`SystemFontRole`, `GuiResourceQuery`;
      - **`FrameFontSize`** (`EV/protocol.rs:156`, used at `DR/font_defaults/policy.rs:4-6,84-91`, R1 F4);
    - **media vocabulary**:
      - `SurfaceShaderLanguage`, `SurfaceUniformInit` and `SurfaceChannelSource{Video(VideoId)}` (from
        `RW/shader_surface.rs`);
      - `ImageDataSource`;
      - **`ResolvedImageMetadata`** (`EV/protocol/image_catalog.rs:221`, used at `RT/state.rs:20` and
        `RT/media.rs:87`, R1 F4);
    - **neo-term vocabulary**, behind DP's existing `neo-term` feature: `TerminalId`,
      `TerminalDisplayTarget`, `TerminalGridSize` and `TerminalFloatPlacement` (`EV/protocol.rs:54-110`,
      re-exported at `DR/terminal/mod.rs:11-13`, R1 F4). M15 removes them;
    - serde for `PopupMenuItem`, `MenuToken`, `MenuBarRequestId` (`DP/menu.rs`) and `PresentedHit`;
    - `FrameDisplayState.layout_scale` (F10), stamped by `LE/engine.rs` from the frame's device scale;
    - `FrameDisplayState::validate_for_materialize() -> Result<(), MaterializeError>`, the fallible form
      of the `expect`s at `DP/glyph_matrix.rs:3068-3087`;
    - **the bincode attribute rule** (DESIGN §6.9, R1 F11). Wire-reachable types carry none of
      `flatten`, `skip`, `skip_deserializing`, `skip_serializing`, `skip_serializing_if`, `tag` or
      `untagged`. The one exception is a symmetric field-level `skip` whose value is rebuilt after decode,
      as `PresentedPointerMap` does (`DP/presented_pointer.rs:2721-2733`). `#[serde(default)]` stays for
      JSON snapshots;
    - `bincode = { version = "=2.0.1", default-features = false, features = ["std", "serde"] }` as a DP
      dev-dependency. `bincode` and `unty` are in the local registry cache, `bincode_derive` is not, so
      no download is needed and no package is added (R1 F8).
  - `DP/input_progress.rs`: value receipts per serial. `checkpoint()` returns
    `FrontierSnapshot::{Exact{stream, consumed_through, in_progress}, Indeterminate{stream}}` with a
    derivable completed prefix. `InputCancelled` replaces cancellation by `Drop` (`:94-98`). Per-session
    stream ids replace `static NEXT_STREAM` (`:14`).
  - **Use sites**:
    - `DR/thread_comm.rs`: receipts are still issued only for PageUp, PageDown and scroll (`:1295-1309`);
    - `RT/frame_compositor/input_scroll.rs`: reconcile on `Disposition::Reflected`;
    - the evaluator keyboard and command loop: `command_loop.rs:575-577` semantics unchanged;
    - `LE/engine.rs`: snapshot before layout, re-checked after; `Indeterminate` if consumption moved.
  - **The redisplay signature keeps keying on the derived completed prefix**, so presentations are
    triggered exactly as today (`EV/command_loop.rs:1186-1196`). The full snapshot is carried for later
    consumers.
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `frontier::minibuffer_inner_keys_reflected_while_outer_in_progress` (outer `M-x` key open, inner keys complete in a nested scope: inner keys `Reflected`, outer `Partial`) | DP unit | **yes**: on today's completed-prefix semantics `through` stays below the inner keys (`DP/input_progress.rs:151`) |
  | `command_loop_nested_reader_frontier` (a real nested minibuffer read through the command loop) | neovm-core `core` binary | **yes**, same reason |
  | `frontier_single_event_inputs_match_completed_prefix` (scroll and PageUp/PageDown dispositions equal today's `through` comparison) | DP unit | no (equivalence guard) |
  | `wire_reachable_types_respect_the_bincode_attribute_rule` (enumerates types reachable from the wire vocabulary; seven attributes; symmetric-skip exception only with a rebuild) | DP unit | **yes** today: 13 `skip_serializing_if`/`flatten` sites on wire-reachable paint spans and hit index |
  | `boundary_vocabulary_bincode_roundtrip::*` (one per moved type) | DP unit | n/a (new API) |
  | `validate_for_materialize_rejects_*` (each `expect` condition becomes a typed error) | DP unit | n/a |
  | `layout_scale_is_stamped` | layout engine | n/a |
  | re-derived `DP/tests/input_progress_test.rs:67-78` (cancellation now explicit) | DP unit | rewritten from the new contract, never deleted (AGENTS.md §3) |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-display-protocol $NX 2>&1 | tee $E/m01-dp.log          # ⟨compare neomacs-display-protocol⟩
  cargo nextest run -p neovm-display-identity $NX 2>&1 | tee $E/m01-identity.log      # ⟨compare neovm-display-identity⟩
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m01-core.log      # ⟨compare neovm-core⟩
  cargo nextest run -p neomacs-display-runtime $RT_FEATURES --lib $NX 2>&1 | tee $E/m01-rt.log  # ⟨compare neomacs-display-runtime⟩
  #   → includes the 77 thread_comm tests and the input_scroll tests, unchanged
  cargo nextest run -p neomacs-layout-engine --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/m01-le.log  # ⟨compare neomacs-layout-engine⟩
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m01-dl.log   # ⟨compare daemon_lifecycle⟩
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-test-targets.py   # exit 0
  cargo fmt --all --check                                                                     # exit 0
  test -z "$(new_external_packages)"          # no new external package (bincode resolves from the lock)
  ```
- **Not runnable here:** GUI L5 pixel identity (CI `neomacs-gui-tests`).
- **Compile fan-out:** `neovm-display-identity` (≈18 dependents) and DP (≈15) once, plus the layout
  engine and evaluator use sites: ≈3 min dev rebuild of the `neomacs` bins, and a rebuild of each
  standing stack copy. Batch all M1 moves into this milestone. Later DP changes are limited to M8, M10,
  M14 and M15 (R1 F4).
- **Risks.** Mechanical breadth (three copies of modifier bits; `FrontendKey` users). No compatibility
  re-exports at old paths: update every use site in the same PR series.

---

## M2 — Pure protocol crates: session layer and display wire codec

- **Status:** landed (5a7a7885, local branch) — one commit on `feat/remote-display` over `267fe940`, no PR
  opened (CI route U11 open). PR body: `evidence/M2/PR.md`. Results (final tree after review fix round 2, run
  by `evidence/M2/fix-r2/driver.log`; tree hashes equal at its start and end, at `fix-r2/final-check.log`, at
  review r3 and immediately before staging): session 39/39, wire 41/41, layout-engine `wire_parity` 13/13
  (sizes unchanged); against M0, DP 982/982, identity 1/1, layout engine 3761 pass (0 new failures), runtime
  `--lib` 1310 pass (0 new failures); crate graph ok (injected violations fail, fix r1), check-test-targets ok,
  fmt clean, no external package added; clippy `-D warnings` clean for both new crates (all targets); bins +
  provision, seam compare 2 passed.
- **Evidence:** landing `evidence/M2/commit/commit.log` (hash check, staging, trailer check) and
  `commit/post-commit.log` (clean tree, fmt, check-test-targets, crate graph, bins + provision in
  `commit/bins.log`); final acceptance `fix-r2/driver.{sh,log}`, `fix-r2/final-check.log`, red/green
  `fix-r2/{red,green}-session.log`, `fix-r2/mutation-proof.log`; fix round 1 `fix-r1.md`,
  `fix-r1/driver-{1,2,3}.{sh,log}`, `fix-r1/{red,green}-*.log`, `fix-r1/schema-mutation-*.log`; red runs
  `red-dp.log` (a real failure), `red-session.log`, `red-wire.log`; fixes `fix-r{1,2}.md`; reviews
  `review-r{1,2}-*.md`, `review-r3-correctness.md`; notes `plan/M2-progress.md`. Superseded (pre-fix trees):
  `verify-driver-*`, `verify-r{1,2}-*`, the top-level `m02-*.log` and `fix-r1/m02-*.log`.
- **Amendments (landing).** Recorded in `plan/M2-progress.md` "Amendments" 1-10. Most visible: DP gains
  `GlyphRowView`/`Parts`, `FrameDisplayStateView`/`Parts` and a fallible glyph walk (1); revision 1.0 has no
  video or web-view messages (2); `DefineRow` carries `Payload<RowRuns>`, so DESIGN §6.10 step 2's in-process
  `Payload::Shared(Arc<GlyphRow>)` cannot be expressed until M13 (9); `WireSchemaId` also hashes the traced read
  shapes (8); the token-proof rule of DESIGN §6.5 is made precise and `Pump::run` is removed (10).
  M1 follow-ups 5-8, 11, 12, 14 and the in-process half of 13 (owner "M2 unless noted") are product changes,
  outside this additive milestone; they move to M6. The M1 list and the M6 section do not record this yet
  (follow-up 24).
- **Follow-ups (unresolved minor review findings, accepted at landing).** "Resolved" marks a finding that the
  fix rounds addressed (source reading of `5a7a7885`, with the test or log named in `fix-r1.md`/`fix-r2.md`).
  Owner: M6 unless noted.
  1. *Window edit validation.* `wire/present/decoder.rs:336`. Resolved (fix r1): the layout is checked before
     the frame changes (`red-decoder-minors.log`, `green-decoder-minors.log`).
  2. *Asset bound.* `wire/assets.rs:240`: `AssetReceiver` bounds each offer but not the total across partially
     assembled offers. Owner: M10.
  3. *Welcome values unclamped.* `session/session.rs:1031`. Limits resolved (fix r1: `LimitsBeyondOffer`).
     Still open: `keepalive_timeout_ms` has no lower bound; 0 ends in a typed `KeepaliveTimeout`
     (`a_zero_keepalive_timeout_ends_in_a_typed_timeout`). DESIGN §6.19 gives no bound. Owner: orchestrator.
  4. *`DefineRow` payload.* `wire/present/mod.rs:93`: amendment 9 above. Owner: M13.
  5. *Encoder memo cost.* `wire/present/mirror.rs:197`: the memo holds `MatrixRow` clones (up to 4 per slot ×
     `store_rows`), keeping evaluator rows shared and uncounted in `store_bytes`.
  6. *Refusal order and fallback service.* `session/handshake.rs:603`: `negotiate` checks the schema before the
     service major, so `ProtocolMajor` is unreachable between real builds; the session crate hard-codes a
     fallback `ServiceName("display")`.
  7. *Hit-index oracle not independent.* `wire/testing.rs:360`: the parity oracle's expected side strips the hit
     index with the wire crate's own `NonTextHitIndex::of/rebuild`. Owner: M6 or M8.
  8. *Hand-listed `WIRE_ROOTS`.* `DP/tests/wire_attribute_rule_test.rs:29`. Partly resolved (fix r1:
     `PopupPlacement`, `CoveragePublicationId`, `InputSerial` and others added; the shape trace refuses
     `flatten`/`tag`/`untagged`). Open: derive the list from `wire_shapes.txt`; `SurfaceId` is macro-defined.
  9. *Two `Lane` enums.* `wire/msg/mod.rs`. Agreement resolved (fix r1: `wire_lanes_are_the_session_lanes`).
     Open: the session layer does not check lane direction.
  10. *View API churn.* `DP/glyph_matrix.rs:2485`: `FrameDisplayStateView` exposes `&Vec<T>`, `&usize`, `&bool`,
      unlike `GlyphRowView`; about 400 lines of View/Parts mirror a struct whose fields are all pub but one memo.
      Owner: DP owner.
  11. *Budget on a synthetic frame.* `wire/testing.rs`: `size_budget_200x60` meets 1 KiB on a `TestFrame` with no
      `window_infos` or `scroll_bars`; in `m02-wire-sizes.log` 7 of 13 real one-character deltas exceed 1 KiB
      (1099-1524 B). Disclosed in PR.md and `M2-progress.md`. Owner: M7/M14 (per-window keyed deltas).
  12. *`ServerName` narrower than GNU.* `session/relay.rs:23`: `[A-Za-z0-9._-]{1,64}` or an absolute path; GNU
      `server-name` is any file name (`server.el:255-267`, **UNATTESTED**). Follows DESIGN:581; now in PR.md
      Limitations (fix r1). Owner: M11.
  13. *`MaterializeError` order doc.* `DP/glyph_matrix.rs:2883`. Resolved (fix r1): `Glyph` is declared first,
      where `materialize` checks it.
  14. *Encoder `expect`.* `wire/rows.rs:552`. Not real (fix r1): `encode_area` refuses more than 65,535 glyphs
      per area first, so the palette index is at most 65,534.
  15. *Clippy gate.* `.github/workflows/linux.yml:156`. Resolved (fix r1): both crates are in the `-D warnings`
      gate.
  16. *M1 follow-up 3, repeated frontier stream.* `wire/present/decoder.rs`. Resolved (fix r1):
      `RejectReason::FrontierRepeatsStream`.
  17. *`Pump::run` busy loop, public.* `session/pump.rs:216` and `:212`. Resolved (fix r2): `run` removed; hosts
      call `Pump::step` on readiness or a timer (M6).
  18. *Frontend `replaying` on a warm Welcome.* `session/session.rs:1045`. Resolved (fix r2): the replay position
      is per-`Link` state.
  19. *Constructors hard-code `display` 1.0.* `session/session.rs:625`, `:1197`: contradicts DESIGN §6.1 and the
      charter; the revision duplicates `schema::REVISION` with nothing tying the two. M6's binder takes the
      service list (with item 6's fallback).
  20. *Link state not typed.* `session/session.rs:977`: state spread over `Handshake`, `Option`s and bools;
      "established implies a session" is held by production `expect`s (:960, :973, :977, :1015, :1031, :1059),
      not by the type (AGENTS.md §5). Partly addressed in fix r2 (`awaiting_proof` removed).
  21. *No raw-clock guard for the new crates.* `session/keepalive.rs:61`: `time_discipline_test` covers only the
      runtime; rule 1 of `display-crate-layout.md` covers all production code. Owner: M6 or a workspace guard.
  22. *Tracer-only `visit_seq`.* `wire/payload.rs:136`. Resolved (fix r2): `#[cfg(feature = "testing")]`.
  23. *Selection vocabulary narrower than GNU.* `wire/msg/mod.rs:142`, **UNATTESTED**: closed
      `SelectionName{Primary, Secondary, Clipboard}` vs any symbol (`xselect.c:2586`, `:132-170`);
      `ConvertOutcome::Converted` has no format vs 8/16/32 from the data (`xselect.c:2318-2400`). Not a
      regression, but pinned in `WIRE_SCHEMA_ID`. In PR.md Limitations. Owner: M9.
  24. *M1 follow-up move unrecorded.* `plan/MILESTONES.md:352`: the move of M1 follow-ups 5-8, 11, 12, 14 to M6
      is recorded only here and in `M2-progress.md`, not in the M1 list or the M6 section. Owner: orchestrator.
  25. *PR keepalive-0 claim untested.* Resolved (fix r2): `a_zero_keepalive_timeout_ends_in_a_typed_timeout`.
  26. *PR Verification lacks commands.* Resolved (fix r2): each command is listed with its result.
  27. *Token-proof coverage.* `session/session.rs:183`: only `Ping` and service messages have a mutation-checked
      test (`fix-r2/mutation-proof.log`); `Ack`, `Credit`, `Pong` and `ReplayFrom` have none. Moving one to the
      non-proof arm only lengthens the window (review r3 m1).
- **Kind:** byte-identical (additive: nothing in the product uses the crates yet).
- **Goal.** The protocol of DESIGN §6 exists as two leaf crates whose tests build without the `neovm-*`
  stack. Evaluator-driven layout-engine frames are proven to survive encode → decode → seal →
  materialize exactly.
- **Depends on:** M1.
- **Scope.**
  - New `crates/neomacs-session/` (**no workspace dependencies**):
    - `preface.rs`: roles evaluator, frontend and relay;
    - `record.rs`: header, chunking, limits checked before allocation;
    - `lane.rs`: implicit per-lane counters, priority scheduler, byte window, chunk sizing;
    - `handshake.rs`: `SessionHello` with `incarnation`; `SessionWelcome` with `keepalive_timeout_ms`,
      `durability` and `ResumeOutcome`; `SessionRefusal` with `SchemaMismatch` and
      `LabelInUse{state}`; also `BuildId` (with `WireSchemaId`), `ServiceOffer`, `Limits`, `SessionId`,
      `ResumeToken` and `Durability`;
    - `relay.rs`: the frozen `RelayRequest`, `RelayStatus` and `ProbeReply` (DESIGN §6.3; R2-13, R2-31);
    - `keepalive.rs`: an injected `Clock` trait and one agreed timeout;
    - `session.rs`: the sans-I/O state machine (`receive`, `poll_transmit(now)`, `tick(now)`). It covers
      two-phase token rotation, implicit takeover by a valid token, token-less takeover, incarnation and
      cold resume, and `received_through` deduplication (D34);
    - `pump.rs`: a generic `Read + Write` pump; `cfg(unix)` socketpair helpers;
    - `testing.rs` behind feature `testing`: fault adapters (1-byte I/O, random splits, half-close, EOF
      mid-record, stalls without EOF, cut after the Nth record) and a fake clock.
  - New `crates/neomacs-display-wire/`.
    - *Dependencies.* Workspace: DP only. External: serde; `bincode = { version = "=2.0.1",
      default-features = false, features = ["std", "serde"] }`; sha2; flate2; thiserror; tracing.
    - `msg/{down,up}.rs` (DESIGN §6.8, including `QuitKeys`, `InputProgress`,
      `Own{generation, content}`, `Convert`/`ConvertReply`/`SelectionData` and `SelectionState`).
    - `codec.rs`: `WireCodec` with bincode `standard()`, deflate and bounded decode.
    - `rows.rs`: `RowRuns`; destructures `Glyph{…}` and `GlyphRow{…}` without `..`.
    - `present/{encoder,decoder,mirror}.rs`: FrameSync (DESIGN §6.10).
    - `assets.rs`: `AssetId::Content` for fonts and pixels; offer, have and want state; SHA-256
      verification.
    - `payload.rs`: `Payload<T>`.
    - `schema.rs`: `WireSchemaId`, the SHA-256 of the golden corpus, compiled in.
    - Golden fixtures (`expect-test`).
  - `scripts/ci/check-crate-graph.py` (new). It reads `cargo metadata --no-deps` JSON on stdin and
    asserts:
    - DP's workspace normal dependencies ⊆ {`neovm-display-identity`};
    - `neomacs-session` has none;
    - `neomacs-display-wire` ⊆ {DP, identity};
    - no normal-dependency path leads from {display runtime, renderer, `neomacs-session`,
      `neomacs-display-wire`} to {`neovm-core`, `neovm-eval`, `neomacs-layout-engine`}, except the edges
      in `KNOWN_EDGES_TO_REMOVE` = {runtime→`neovm-core`, runtime→layout engine, renderer→layout engine}.
      Revision 2 confirmed that exactly these three exist.

    It fails if a listed edge no longer exists, so the allowlist cannot go stale (as
    `time_discipline_test` requires of its own list).
  - `docs/design/display-crate-layout.md`:
    - correct line 14 (DP depends only on the serde-only identity leaf);
    - add charters for `neomacs-session`, `neomacs-display-wire`, `neomacs-display-host`,
      `neomacs-frontend`, `neomacs-tty-render` and `neomacs-media-cpu`;
    - state "stream I/O never enters DP" and "the renderer never sees the link";
    - record that these boundaries are a design decision (`:142-147`).
  - `crates/neomacs-layout-engine/Cargo.toml`: a dev-dependency on `neomacs-display-wire` for the
    parity test.
  - **CI.** The display job filter (`test-suite.yml:1236`) gains
    `package(neomacs-session)|package(neomacs-display-wire)`. A small `protocol-crates` job runs
    `cargo nextest run -p neomacs-session -p neomacs-display-wire` on the Linux, macOS and Windows legs
    (R2-35). The macOS path filter gains both crates (R2-22).
- **New tests** (all new API; red-before n/a).
  - **session**:
    - `preface_rejects_wrong_magic_with_readable_line`, `oversized_record_rejected_before_allocation`;
    - `handshake_matrix`: same minor; older and newer minor; a major mismatch gives `ProtocolMajor`; a
      schema mismatch gives `SchemaMismatch` naming both builds; an `exact-build` mismatch gives
      `BuildMismatch`; the `schema` policy intersects services;
    - `refusal_decodable_by_both_ends`;
    - `priority_control_and_input_overtake_bulk_within_one_chunk`: with 10 MB of bulk queued on a
      simulated 1 MB/s link, control and input records enqueued later arrive within one chunk plus one
      record (P4 M1);
    - `keepalive_timeout_with_fake_clock`, `pump_adversarial_*` (each ends in a typed outcome within a
      bounded time);
    - `relay_request_roundtrip_and_bounds`, `relay_status_decodable_by_both_ends`;
    - `two_phase_token_accepts_previous_until_proven`, `implicit_takeover_by_valid_token`,
      `tokenless_offer_on_held_label_needs_takeover`;
    - `incarnation_change_is_a_cold_resume`, `replay_dedupes_by_received_through`.
  - **wire**:
    - `every_message_variant_roundtrips` (proptest), `golden_bytes_*` per variant,
      `every_variant_has_a_golden_fixture` (exhaustiveness);
    - `wire_schema_id_changes_when_a_nested_field_changes`, using a test-only nested type;
    - `decode_is_total`: random, truncated and oversized input gives a typed `WireError`, never a panic,
      with bounded allocation;
    - `framesync_sequence_reconstructs_canonically` (random edit, scroll, resize and split sequences),
      `discarded_updates_still_update_the_mirror`, `bad_slot_ref_rejected`, `missing_face_rejected`,
      `materialize_never_panics_on_accepted_state`;
    - `size_budget_200x60`: a one-character edit ≤ 1 KiB raw; a keyframe ≤ 64 KiB raw (targets from
      DESIGN §6.10 [E]).
  - **layout engine**: `wire_parity_real_frames` over a **named corpus of evaluator-driven frames**
    (R1 F3). Each scenario is a function in `LE/engine/tests/wire_parity_corpus.rs`, built with the
    engine's own fixtures: an evaluator `Context`, a real frame, `engine.layout_frame_rust`, then
    `last_frame_display_state`. That is the pattern of `LE/engine/tests/engine_test.rs:5251-5296` and
    `LE/engine/tests/lazy_text_hit_test.rs:18-32`. The mock frames are not used; they bypass the
    evaluator. The scenarios:
    1. plain text;
    2. faces and overlays;
    3. `display` strings and properties;
    4. a bidi paragraph;
    5. wide and composed characters;
    6. mode, header and tab lines;
    7. margins and fringes;
    8. line numbers;
    9. truncation and continuation;
    10. split windows;
    11. a child frame;
    12. scroll coverage with offsets;
    13. the lazy, GC-root-bearing text hit source.

    For each scenario, the canonical encoding of `materialize(seal(decode(encode(s))))` equals that of
    `materialize(seal(s))`, with the hit index's text source stripped on the encoded side (DESIGN D4).
    The test writes measured keyframe and delta sizes.
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-session --features testing   # all pass; builds no workspace crate
  cargo nextest run -p neomacs-display-wire                 # all pass; builds only DP and the identity leaf
  cargo nextest run -p neomacs-layout-engine --features neovm-core/dbus-vendored -E 'test(~wire_parity)'
  #   → 13 scenarios pass; sizes appended to $E/m02-wire-sizes.log
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-crate-graph.py    # exit 0
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-test-targets.py   # exit 0
  cargo fmt --all --check
  test -z "$(new_external_packages)"          # only workspace crates were added to Cargo.lock
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:** the macOS and Windows legs of `protocol-crates` (CI, U11).
- **Compile fan-out:** two new leaf crates (seconds); the layout engine's test target only. No product
  crate changes behaviour.
- **Risks.** Enabling the `serde` feature of bincode adds an already-locked edge (`bincode → serde`,
  `Cargo.lock:530-538`). If `git diff Cargo.lock` shows any new external package, stop and record a
  cargo-deny review.
  `new_external_packages` checks this mechanically.

---

## M3 — Per-terminal display registry, one entry today

- **Status:** landed (267fe940, local branch) — one commit on `feat/remote-display` over `80c6c9f8`, no PR
  opened (CI route U11 open). PR body: `evidence/M3/PR.md`. Results (independent verification round 2, tree
  identical to the commit, plain compares against M0): neovm-core 7509 pass (+11 new, 0 new failures), runtime
  `--lib` 1310 (0 new failures), layout engine 3748 (0 new failures), `daemon_lifecycle` 35/35, bins +
  provision, seam compare 2 passed, check-test-targets ok, fmt clean, no new package; extras `neovm-value`
  1403, `neovm-eval` 4678, `neovm-display` 1575, all passing; clippy: one pre-existing size text changed.
- **Evidence:** verification `evidence/M3/verify-r2-driver.{sh,log}`, `verify-r2-{core,rt,le,dl,seam,bins}.log`
  and `verify-r2-compare-*.log`; post-commit `evidence/M3/commit/post-commit.log` (tree-hash match, clean
  tree, fmt, check-test-targets, bins + provision in `commit/bins.log`); implementer runs
  `fix-r1/acceptance.{sh,log}`, `r2/acceptance.{sh,log}`; red `evidence/M3/red.log`, `red-r2.log`,
  `fix-r1/item1-red.log`, `fix-r1/item2-mutation-*.log`; fixes `fix-r1.md`; reviews `review-r{1,2}-*.md`;
  notes `plan/M3-progress.md`. Superseded: `verify-r1-*` and the top-level `m03-*.log` (pre-fix-r1 trees).
- **Amendment (landing).** The test-only `DisplayHost` implementors were split into a `DisplayConnection` impl
  and a `DisplayServices` impl each (about 100 files, by script), not migrated through the planned adapter over
  `RecordingDisplayHost`. M3a–M3e landed as one commit, not five PRs.
- **Follow-ups (unresolved minor review findings, accepted at landing).** Owner: M5 unless noted.
  1. *Rows audit covers services only.* `EV/rows_observation.rs:181`: `InertRowsHostAuthority` validates only
     the services `TypeId`. Layout in the scope also calls the frame's connection
     (`FrameDisplay::request_video/webkit/surface`), which is unaudited when services and connection differ.
  2. *Replay of pre-display settings.* `EV/display_terminals.rs:75,81`: settings broadcast before any display
     existed are recorded and replayed into the GUI host when `set_display_host` runs. Those calls were no-ops
     before. This contradicts the progress note "Nothing is recorded before the production install".
  3. *Weak structural tests.* `NC/tests/moved_display_host/display_suite/terminal_registry_test.rs:259`: the
     parked-World assertion is true by construction (the registry is a `Context` field). The popup-half test
     uses `install_display` (split), not the production `install_display_host(INITIAL, Termcap, ..)`.
  4. *Production `-nw` install untested.* `terminal_registry_test.rs:330`: the `-nw` tests build split `Own`
     services with `install_display`. Production uses `install_display_host(Termcap)` with `OfTerminal`
     services, and that path has no unit test.
  5. *Backing slots collapse.* `EV/protocol/terminals.rs`: `set_backing`/`remove` silently replace a backing of
     another kind on the same `TerminalNumber`. `set_tty_device` on a `WindowSystem` terminal drops the GUI
     host and clears the process services. `set_display_host` after `set_terminal_host` drops the TTY device.
     These were independent slots before M3.
  6. *`uninstall_display` and `install_split`.* `EV/protocol/terminals.rs`: `pub uninstall_display` turns a
     non-initial `WindowSystem` terminal into `TerminalBacking::Initial`. `install_split` after a previous
     split leaves the old connection registered. M5 replaces both with a typed close.
  7. *Unbound `FaceResolver`.* `LE/neovm_bridge.rs:5228`: the frame is an `Option<FrameId>` set by a separate
     `bind_frame`. An unbound resolver answers `Ok(None)` for video, webkit and surface. MISSION rule 2 forbids
     this silent fallback. Prefer a constructor argument or a bound type. Owner: M8.
  8. *Class 4 is partly ad hoc.* `ND/neo/effects/mod.rs`: visual config, alpha limit, `ns-confirm-quit` and
     focus redirects each loop over `connected_terminals()` by hand (a `Vec` allocation and duplicated
     first-error logic). Broadcast records a setting before applying it, so a rejected setting is replayed to
     later registrations.
  9. *PR limitations, class 5.* `evidence/M3/PR.md` first named only the class 1 divergence. Resolved at
     landing: class 5 (a TTY frame in a GUI session no longer asks the GUI host for media) is listed too.
  10. *Test-implementor split.* `ND/test_utils.rs`: the plan's adapter was not built (see the amendment).
  11. *Deleted-terminal gate.* `ND/terminal/pure.rs:1180`: `with_terminal_host_for_id` and
      `delete_terminal_owned` gate on `get(id).is_some()`, which is also true for a deleted terminal's record.
      Before M3 `mark_deleted` cleared the host. Now only `delete_terminal_owned` releases the registry device.
  12. *`TerminalNumber::INITIAL` doc.* `neovm-value/src/display_vocab.rs:24` calls it GNU's `initial_terminal`
      (F1's terminal). In GUI mode F1 is on terminal 1 and terminal 0 is the window-system terminal.
  13. *Stale design doc.* `docs/design/host-effect-projections.md`: the DisplayHost and native-terminal-backing
      table rows are stale. Lines 9-11 ("no production activation in the contracts range") and 74-76 ("no hot
      owner check") no longer hold: `Context::tty_device_request` checks `is_green_worker_owner()` on every
      call, and the main pump serves `TerminalDeviceRequest` under unhosted batch Green. PR.md discloses this
      and asks the doc owner. Owner: the doc's owner and the orchestrator.
  14. *Delete after side effects.* `ND/terminal/pure.rs:1839`: a worker's `delete-terminal` can fail at a routed
      hop after side effects. In Noelisp mode the `?` on the Delete hop bypasses `ignore_host_delete_errors`.
  15. *Unhosted pump aborts.* `EV/green/host_service/pump.rs:129`: the terminal-device family calls
      `fatal_pump()` (process abort) when `host` is `None`. The host is still an `Option`, and the
      `MainNativeHandoff` doc says unhosted waits hold no display.
  16. *Routed error paths untested.* `.../green/host_service/tests/terminal_device_test.rs:108`: no test of
      `HostError` crossing as a `LispError` reply, `NoDevice` through the hop, or the Noelisp ignore path.
  17. *Backend enum incomplete.* `pump.rs:127`: `NativeBackend::start` handles `TerminalDeviceRequest` before
      it matches the backend. The backend enum therefore no longer describes everything main serves, and
      fixture backends cannot intercept this family. No test covers its `Interrupted` or `HostClosed` decode.
  18. *Device effect refusals.* `EV/display_terminals.rs:82`: a worker's device effect can now be refused
      wherever `green_prepare_host_call` refuses (GC inhibited, collection driver active, module call), with a
      generic error string. Before M3 the effect ran inline. PR.md discloses it. No test pins the refusal or its
      Lisp-visible form. Owner: M9 (typed refusals).
  19. *`TtyHost::device_mut` is `pub`.* `EV/protocol/terminals.rs:121`, although `perform_tty_device` is
      documented as the only path to a device. Its only caller is private.
  20. *Optional main host.* `pump.rs:129`: the main-only service path takes `host: Option<&mut
      DisplayTerminals>`, but main always passes `Some`. The `fatal_pump` arms for `None` cannot run (a
      non-main owner aborts earlier). A non-optional parameter would make the invalid state unrepresentable.
- **Kind:** byte-identical refactor (XL; lands as five PRs).
- **Goal.** Replace `Context.display_host: Option<Box<dyn DisplayHost>>` (`EV/mod.rs:3775-3776`) with the
  main-owned `DisplayTerminals` registry (DESIGN §7.1) and route every access through frame, then
  terminal, then backing. Production keeps exactly today's hosts, answers and terminal numbering.
- **Depends on:** M0.
- **Scope and PRs.**
  - **M3a.**
    - Add `TerminalNumber`, `TerminalBacking::{Initial, Termcap, WindowSystem}` and `DisplayTerminals`,
      which occupy the existing `display_host` slot.
    - Split the 90-method `DisplayHost` (`EV/protocol.rs:300`) into `DisplayConnection` (per
      connection) and `DisplayServices` (per process).
    - Account for **both** production hosts (R1 F1). `PrimaryWindowDisplayHost` (`NM/main.rs:1862`)
      implements both halves for the GUI. `TtyPopupDisplayHost` (`NM/tty_frontend.rs:30,40`, installed at
      `NM/main.rs:5477-5483` for non-daemon interactive TTYs) becomes the popup and selection half of the
      primary termcap terminal's `TtyHost`.
    - Route class 1 call sites (frame-addressed) by frame.
  - **M3b.** Class 2: selected-frame queries gain a terminal argument.
  - **M3c.** Class 3 ("is there a GUI"). These sites test `display_host.is_some()` today, which is true
    under `-nw` because of the TTY popup host; for example `neomacs-surface-available-p` answers t
    (`ND/shader_surface/mod.rs:350-353`). M3c routes them through a registry predicate **equivalent to
    today's `is_some()`**, so every answer is unchanged. M5 changes them deliberately (D37).
  - **M3d.** Class 4: broadcast settings apply to every window-system terminal and replay on
    registration, generalizing `EV/gc_pacing.rs:1102-1122`.
  - **M3e.** Class 5: layout reads through `DisplayServices` plus the frame's terminal. That is 23
    accesses in 6 layout-engine files (R1 F20). TTY native hosts move out of the World-carried
    `TerminalManager` (`ND/terminal/pure.rs:249,1809-1839`) into the registry. The Rows authority and
    Green host-service steps carry the frame's terminal (u4 S5, S6). The PR quotes
    `docs/design/host-effect-projections.md:72-78` ("using existing storage; there is no new TLS word,
    inline Context/World field") and asks the document's owner whether a changed slot type satisfies it
    (R1 F19).
  - The 62 test-only `DisplayHost` implementors migrate through a mechanical adapter over
    `RecordingDisplayHost` (`crates/neovm-display/src/test_utils.rs:29`).
- **New tests** (in `crates/neovm-core/tests/moved_display_host/`; red-before means the API does not
  exist):
  - `two_window_system_terminals_route_frame_setters_to_their_own_host`: two `RecordingDisplayHost`s on
    terminals A and B; `(set-frame-parameter FB 'title "x")` reaches only B;
  - `selection_routes_to_the_selected_frames_terminal`;
  - `broadcast_config_reaches_every_window_system_terminal_and_replays_on_registration`;
  - `tty_host_is_owned_by_the_registry_not_the_carried_terminal_manager` (structural);
  - `primary_tty_popup_half_is_owned_by_the_termcap_backing` (structural; a recording popup host);
  - `class3_answers_are_unchanged`: the registry predicate equals today's `is_some()` for a GUI host, a
    TTY with a popup host, and batch.

  The `-nw` M0 scenario (popup menu included) is the behavioural regression test for the TTY popup path.
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m03-core.log     # ⟨compare neovm-core⟩ + new pass
  cargo nextest run -p neomacs-display-runtime $RT_FEATURES --lib $NX 2>&1 | tee $E/m03-rt.log # ⟨compare⟩
  cargo nextest run -p neomacs-layout-engine --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/m03-le.log  # ⟨compare⟩
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m03-dl.log  # ⟨compare⟩
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:** GUI L5 (frame routing of the in-process GUI host).
- **Compile fan-out:** `neovm-eval` (8 dependents), `neovm-display`, `neovm-core`, the layout engine and
  `neomacs`: ≈2–3 min dev, plus a rebuild of the standing stack copies per PR. Measure one edit-test
  cycle first (J2 Q3) and size the PR split by it.

---

## M4 — Frontend runtime without the Lisp machine (crate cut)

- **Status:** landed (c4e7100d, local branch). One commit on `feat/remote-display` over `5a7a7885`, no PR
  opened (U11). PR body: `evidence/M4/PR.md`. The committed diff against `5a7a7885` hashes to `47ffa05cd493`, the
  fix-r3 tree below. Pre-landing state: Fix-r3 tree: porcelain 102, `git diff HEAD | sha256sum`
  `47ffa05cd493`. That is the fix-r2 tree `a32cfd982359`, on which the acceptance block ran green, plus one edit in
  `neomacs-tty-render/src/damage.rs`. Evidence `evidence/M4/`:
  - index `fix-r2/summary.log` (driver `fix-r2/driver.sh`, 13:53–14:03 UTC, the acceptance block as amended);
  - `fix-r3/summary.log` (14:37–14:46 UTC), the suites the fix-r3 edit reaches: tty-render 198/198, combined
    compare 0 failing, dl 35/35, bins + provision, seam 2 passed;
  - dispositions `fix-r1.md`, `fix-r2.md`, `fix-r3.md`;
  - red `red.log`, `fix-r2/closed-table-red-green.log`, `fix-r3/red-tty-render-gate.log`;
  - notes `plan/M4-progress.md`.

  Superseded (earlier trees):
  `acceptance-summary.log`, `post-acceptance-summary.log`, `final-neomacs-summary.log` and
  `fix-r1/r2-summary.log` (`581a8bff24d0`).
  Combined runtime/tty-render/display-host compare 0 new failures (`m04-moved.map`, 205 moves); runtime-only
  run fails exactly its 25 M0-baseline failures and its closure holds no Lisp-machine crate (only the identity
  leaf); tty-render 198/198; core architecture 180/2 (`--lib` subset of M0) plus the obarray scan; materializer
  131/131 vs `m04-fm-base`; LE + materializer in one invocation with `m04-le-moved.map` 0 new failures (the LE
  line as amended in fix-r2: the 5 `font_match` private-item tests moved with the module, D44/D46); dl 35/35;
  seam 2 passed. Clippy, scoped `--no-deps` as in `linux.yml`: every gate crate was linted at HEAD and on M4, and M4
  has no diagnostic that HEAD lacks. tty-render and display-host pass the gate's flags. The gate stays red only on
  HEAD's own diagnostics in DP, LE, the renderer, the runtime and `neomacs` (`fix-r3.md`). Deviations:
  `test-suite.yml` takes R1 F15's materializer CI fix from M10 (its tests now matter);
  `buffer_local_global_read_test.rs` scan roots name display-host.
- **Evidence:** landing `evidence/M4/commit/message.txt` (commit body) and `commit/post-commit.log` (HEAD, clean
  tree, diff hash `47ffa05cd493`, crate graph, test targets, fmt: exit 0); final acceptance `fix-r2/summary.log`
  (driver `fix-r2/driver.sh`) and `fix-r3/summary.log` (driver `fix-r3/driver.sh`); dispositions `fix-r{1,2,3}.md`;
  red `red.log`, `red-le-full.log`, `fix-r2/closed-table-red-green.log`, `fix-r3/red-tty-render-gate.log`;
  pre-edit record `m04-fm-base.{log,json}`; move maps `m04-moved.map`, `m04-le-moved.map`.
- **Follow-ups (unresolved minor review findings, accepted at landing).** Owner: M11b for the runtime/headless
  items, M13 for the reclaim hook, otherwise the next agent that touches the file. `DH` = `neomacs-display-host`,
  `TR` = `neomacs-tty-render`, `DR` = `neomacs-display-runtime`, `LE` = `neomacs-layout-engine`.
  1. `LE/font/fontconfig.rs:133`: In LE test builds `invalidate_catalog_caches` no longer resets the subpixel order
     (the call is under `cfg(not(all(test, unix)))`); at HEAD `QueryOwner::System` reset it with the rest of
     `FontconfigCaches`.
  2. `DH/redisplay.rs:46`: `PreparedPresentationTicket::for_unacquired(&SealedFramePresentation)` replaces
     `SupersededPresentation::discard` and accepts any sealed presentation, dropping the newtype proof (private
     constructor in `frame_mailbox.rs`) that the renderer never acquired the revision. `SupersededPresentation`'s
     `#[must_use]` text still names the removed discard method. (Three review findings.)
  3. `DH/tests/real_layout_fixture_test.rs:108,120`: In bless mode both contract tests overwrite the fixtures and pass
     without comparing. (Two findings.)
  4. `TR/rif.rs:2810`: No tty-render test constructs a `TtyRif` with `TextFillEdge::PosnObjectExtent`; every split
     test uses `Structural`, and the display-host contract test only checks `policy.text_fill ==
     text_fill_edge(posn_object_extent_mode())`, which restates the implementation. (Two findings.)
  5. `neovm-value/Cargo.toml:68-70`: Stale comment: it names the display runtime as the `desktop-font-settings`
     enabler, display-host since D45. The moved `desktop_font_settings_test` cites `lisp/loadup.el:305`; at the pinned
     GNU commit the `featurep` check is at `:301` (source contract, UNATTESTED). (Two findings.)
  6. `DR/render_thread/command_processing.rs:7`: `RenderApp::process_commands` is `pub` unconditionally though
     `RenderApp` is re-exported only under `testing`; the forward belongs in `headless.rs`, like
     `send_input_with_receipt`.
  7. `DR/testing/key_transport.rs:11`: `TransportedKey.modifiers` is a raw `u32`; DP's `ModifierBits` newtype exists.
  8. `DR/render_thread/webview_child_signal.rs`: `WebViewChildSignal` wraps `fn() -> bool`, and its only call site
     (`bootstrap.rs`) discards the bool.
  9. `DR/render_thread/bootstrap.rs:376`: The WebView SIGCHLD re-claim, unconditional before M4, runs only when the
     `Option<WebViewChildSignal>` is `Some`. `RenderApp::new` and `RenderApp::headless` leave it `None`, and the
     `None` arm skips the re-claim silently, which MISSION rule 2 forbids.
  10. `scripts/ci/check-crate-graph.py`: No red or injected-violation proof that a normal display-host → runtime edge
      fails the DISPLAY_HOST charter rule; only the dev-edge walk is shown red (`red.log` §1).
  11. `docs/design/display-crate-layout.md`: The diagram draws no edge from display-host to `neomacs-tty-render` or to
      the protocol, though both are normal dependencies. The charter text is correct.
  12. `evidence/M4/acceptance-summary.log`: The superseded index contradicts its logs: step 7's core-arch compare
      (exit 1, 8870 failing) was overwritten 26 s later by an unscripted rerun against a base json with no saved
      derivation; `post-acceptance-summary` says clippy exit 101 but `m04-clippy-evaluator.log`, overwritten by
      `clippy-evaluator.sh`, shows 0. The claims are supported by the post-acceptance compare (181/2, 0 new) and
      `fix-r2/summary.log`.
  13. `TR/damage.rs:118`: Default tracing targets of the moved TTY and redisplay code changed from
      `neomacs_display_runtime::backend::tty::*` to `neomacs_tty_render::*`.
  14. `plan/M4-progress.md`: Called the pre-fix-round indices (12:19 UTC) the final tree after fix round 1 had edited
      files. *Resolved at landing* for MILESTONES (Status names `fix-r2`/`fix-r3`); `M4-progress.md` keeps its
      history.
  15. `TR/testing.rs:64`: No `should_panic` case pinned the closed-table panics. *Resolved* in fix round 2:
      `testing/tests/testing_test.rs:14,26`, red then green in `fix-r2/closed-table-red-green.log`.
- **Amendment (2026-11-25, design steward; D43–D47).** The implementer stopped before any product edit
  (`plan/M4-plan.md` B1–B6). Each claim was re-checked against `5a7a7885` (`evidence/M4/steward-verify.log`)
  and all hold. Scope, tests and acceptance below carry A1, A2 with rulings (a) and (b), A4, A5 and A6. A3 is
  changed: the `desktop-font-settings` forward goes to display-host, not `neomacs` (D45). Four items the
  block missed are added: the reclaim hook, the runtime's `default`, the guard arms plus an architecture-test
  run, and a materializer pre-edit record. `plan/M4-plan.md` steps 1–6 apply as amended here.
- **Kind:** byte-identical refactor (mechanical, wide).
- **Goal.** `neomacs-display-runtime` and `neomacs-renderer-wgpu` no longer link the Lisp machine or the
  layout engine. A frontend binary can then exist without `neovm-*`, and the runtime's tests stop paying
  for a stack copy (J2 §3 P4 M2).
- **Depends on:** M1 (which moved all the vocabulary the runtime uses, R1 F4). U11 should be settled
  first (R2-22).
- **Scope.**
  - New `crates/neomacs-tty-render/` (DP, `rustc-hash`, `tracing`; pure). `DR/backend/tty/` and all its
    tests move here. The production `neovm_core` uses become one construction parameter, `TtyRasterPolicy
    { char_widths: TerminalCharWidths, text_fill: TextFillEdge }` (`rif.rs:2732,3826-3831`; `:2797`).
    `TextFillEdge { Structural, PosnObjectExtent }` is tty-render's own enum, matched exhaustively (D47).
    - Every TTY test stays in this crate. That includes the reference rasterizer, an inherent
      `impl TtyRif` (`rif/tests/packed_raster_test.rs:4-37`), and the cases that consulted evaluator
      oracles (`packed_raster_test.rs:372,429,766`, `rif_test.rs:2847,3033`, `damage_test.rs:218`).
    - They run on a `testing` module (`cfg(any(test, feature = "testing"))`) with two parts:
      `gnu_char_width` (GNU's ASCII rules plus each non-ASCII scalar the tests use; it panics on any other
      scalar), and literal `TerminalComposition`s for the strings the tests compose.
    - Display-host contract tests pin both parts to the evaluator (D44). No case is weakened, and the
      `--moved` map records each move (R1 F12).
  - New `crates/neomacs-display-host/` (evaluator side; neovm-core, layout engine, DP, tty-render).
    - It receives `DR/redisplay.rs`, `DR/redisplay_step.rs`, `DR/redisplay/maintenance.rs` and their tests.
    - The foreign inherent `impl SupersededPresentation` (`DR/redisplay.rs:43`) becomes a display-host
      constructor. Its one caller (`NM/main.rs:6868`) feeds it the mailbox's superseded value.
    - `NM/frame_layout.rs` (`REDISPLAY_RUNTIME`) and `NM/secondary_tty.rs` import from it.
    - It supplies the production `TtyRasterPolicy`: the evaluator's `char_width`, and
      `posn_object_extent_mode()` mapped to `TextFillEdge`.
    - Its only runtime edge is a dev-dependency (`testing`), for the integration tests below. All its
      tests are lib tests, which the combined `--lib` run in the acceptance below covers.
  - **`DR/tty_input` stays in the runtime** (D43). Its production code publishes through the runtime's
    `RenderComms` (`DR/tty_input.rs:24-27`, `unix_reader.rs:20`, `publication.rs:25`) and names no
    `neovm_*`. M13 moves it when it deletes `RenderComms`.
  - **The runtime's remaining production users** at `5a7a7885` (M1 already moved the vocabulary):
    - the TTY backend and `redisplay*` (they move);
    - the unused layout-engine re-exports `lib.rs:43-51` and `core/mod.rs:9` (deleted; `rg` finds no user);
    - `RT/bootstrap.rs:375`, a `reclaim_child_signal()` call during WebView init (`cfg(feature = "webview")`).
      It becomes a hook that the embedder injects: `neomacs` passes
      `neovm_core::emacs_core::reclaim_child_signal`. The handler it reinstalls, and that handler's
      latches, live in `neovm-system` (`os_signal/mod.rs:628`), so the runtime must not install a SIGCHLD
      handler of its own. `webview` cannot build here (no WPE), so CI is the only place this arm compiles.
      The hook's plumbing must leave no dead code in builds without `webview`.
  - **Feature forwards** (R1 F14).
    - The runtime's `jit` and `sqlite` forwards go. Its `default` becomes `["neo-term"]`, because the M0
      runtime baseline holds 63 `terminal::` tests (`Cargo.toml:89-94`).
    - Its `neo-term` stops forwarding to `neomacs-layout-engine/neo-term` (`:110-117`). `neomacs` enables
      that feature directly.
    - **`desktop-font-settings`** (D45). The runtime is the only crate that enables
      `neovm-core/desktop-font-settings` (Linux, `Cargo.toml:144`). That feature makes the GNU C-feature rows
      `dynamic-setting` and `system-font-setting` Implemented (`neovm-value` `c_features.rs:330,344`).
      `neomacs-display-host` takes over the same Linux forward.
      `font_defaults::tests::the_linked_desktop_font_subscription_is_advertised_to_lisp` moves to
      display-host's lib tests.
  - **Renderer helpers** (R1 F13). The renderer's layout-engine uses (`RW/glyph_atlas/mod.rs:37,2268,2328-2351`)
    move to `neomacs-font-materializer`, which depends only on DP among workspace crates and already links
    `yeslogic-fontconfig-sys`. Three pieces move:
    - `font::subpixel` with the Fontconfig subpixel query (`LE/font/subpixel.rs:27` →
      `LE/font/fontconfig.rs:1857-1880` and the cache and handle it uses);
    - `composition::representative_char_for_cluster` (`LE/composition.rs:116`);
    - `font_match::{select_cosmic_family, CosmicFamilySelection, resolve_weight_in_family}` with
      `probe::named_instance_wght_values` (`LE/font/font_match.rs:202`, `LE/font/probe.rs:399,692`).

    The layout engine re-exports them from the materializer, so no code is duplicated. The renderer's
    `jit` and `sqlite` forwards to the layout engine (`crates/neomacs-renderer-wgpu/Cargo.toml:16-17`) go
    with the edge.
  - **Renderer contract tests** split at the DP boundary. Two files use the layout engine:
    `RW/glyph_atlas/tests/glyph_atlas_test.rs:775-906`, and `tests/offscreen_frame/menu_test.rs:289,374,440,496`
    (`FontMetricsService`).
    - Layout-engine tests assert the `ResolvedFont`, metric and `ResolvedMenuLabel` values the engine
      publishes.
    - Renderer tests consume equivalent DP values built from `neomacs-test-fonts` fixtures, already a
      renderer dev-dependency (`crates/neomacs-renderer-wgpu/Cargo.toml:60`).
    - They need a GPU, so they are only compiled here; CI runs them.
  - **Runtime tests that used the evaluator** (D44). The dev-dependency on `neovm-eval[test-support]`
    (`Cargo.toml:72-77`) is removed. At `5a7a7885`, 11 runtime test files name `neovm_*`. They follow one
    rule: no test reaches another crate's private items.
    - *Halves that meet at a DP value split there.* Each crate keeps the half that reads its own private
      items. A display-host contract test pins the evaluator's half to the same values, exported once
      under a `testing` feature (precedent: `neomacs-display-wire`). This covers:
      - the TTY cases above;
      - the four `input::tests` key-transport cases and `tty_input`'s fullwidth case, split at
        `FrontendKey` (`Character(c)`; "C-M-n" `0x800000e`; "C-M-#" `0xc000023`);
      - the raw byte `0xa4` case (`tty_relay_test.rs:535`, which uses literal bytes).

      The image tests import `ImageLoadAttempt` and `ImageLoadToken` from DP.
    - *Runtime behaviour over live evaluator output moves to display-host.* It reaches the runtime only
      through public API or the runtime's `testing` feature.
      - The `frame_opacity` evaluator case uses public API.
      - Ruling (a): `input_backpressure_test` (3 tests) uses the public headless entry that M11b names
        (`RenderApp::headless` under `testing`). M4 gives it only what these tests need: construction over
        a `ThreadComms` render half, `send_input_with_receipt` and `process_commands`. M11b extends that
        entry rather than adding a second headless type.
    - *Ruling (b).* `input::tests::real_layout_publication_*` (2 tests) stay in the runtime.
      - They load checked-in `FrameDisplayState` JSON, which the runtime exports under `testing`.
      - A display-host contract test lays out the same scenarios and asserts
        `fixture.materialize() == live.materialize()`. That compares the `FrameGlyphBuffer` the runtime
        consumes, so a `#[serde(skip)]` field matters only through what materialization rebuilds.
      - A documented command regenerates the fixtures.
      - If that equality cannot hold, both tests move to display-host over the `testing` entry. That entry
        then exposes exactly the queries the tests assert: the pointer owner and semantic hit at a point,
        and a child frame's absolute origin.
      - No hand-built frame replaces real layout output.
  - `check-crate-graph.py`:
    - `KNOWN_EDGES_TO_REMOVE` becomes empty;
    - the runtime check covers dev edges too;
    - `neomacs-tty-render` may depend only on DP and the identity leaf;
    - display-host's normal workspace dependencies are pinned to its charter, so its runtime edge stays
      dev-only.
  - `host_effect_guard_test.rs`: owner declarations follow the moved files.
    - `DR/redisplay.rs` is named in two arms: Font (`:168`) and Layout (`:200-204`).
    - A new materializer file that makes Fontconfig calls needs a Font entry.
    - `tty_input` keeps its ConnectedComms owner (`:80-84`).
  - CI: the display filter and the macOS path filter gain
    `package(neomacs-tty-render)|package(neomacs-display-host)`.
- **New tests.**
  - `check-crate-graph.py` with an empty known-edge list. **Red before**: the three edges exist today.
  - Display-host contract tests, each naming the test it backs. They cover TTY widths (the table and all
    ASCII), compositions, the `TextFillEdge` mapping, key transport, the raw-byte encoding and the
    real-layout fixtures. They are red only by absence (new API).
  - The moved tests pass in their new crates under their old names (`--moved` map).
  - `materializer_subpixel_order_matches_layout_engine`: the extracted query returns what the old path
    returned on this host.
- **Acceptance (run here).**

  ```sh
  # Before the first edit: M0 has no materializer baseline.
  cargo nextest run -p neomacs-font-materializer $NX 2>&1 | tee $E/M4/m04-fm-base.log
  python3 -I $BL record $E/M4/m04-fm-base.json < $E/M4/m04-fm-base.log
  # After:
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-crate-graph.py    # exit 0, no known edges left
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-test-targets.py && cargo fmt --all --check
  echo "[$RT_FEATURES]"                                    # → [] : the runtime no longer depends on neovm-core
  cargo nextest run -p neomacs-display-runtime --lib $NX 2>&1 | tee $E/M4/m04-rt.log
  #   closure proof: compiles no neovm-* crate and no layout engine; every FAIL line is a baseline failure
  cargo nextest run -p neomacs-display-runtime -p neomacs-tty-render -p neomacs-display-host --lib \
    --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/M4/m04-split.log
  #   ⟨compare neomacs-display-runtime --moved $E/M4/m04-moved.map⟩: one log, every moved test under its new id (D46)
  cargo nextest run -p neomacs-tty-render $NX                # alone: compiles no neovm-* crate
  cargo nextest run -p neovm-core --features dbus-vendored --lib -E 'test(~architecture_)' $NX 2>&1 | tee $E/M4/m04-core-arch.log
  #   host_effect_guard and the other workspace scans: every architecture test that passed at M0 passes (M0: 181/2)
  cargo nextest run -p neomacs-font-materializer $NX 2>&1 | tee $E/M4/m04-fm.log
  python3 -I $BL compare $E/M4/m04-fm-base.json $E/M4/m04-fm.log
  cargo build -p neomacs-renderer-wgpu && cargo nextest run -p neomacs-renderer-wgpu --no-run
  #   → builds with no feature flag and no neovm-* crate; its tests compile (GPU runs: CI only)
  cargo nextest run -p neomacs --features dbus-vendored --lib --no-run && cargo build -p neomacs --features dbus-vendored --example mock-display
  #   → its TTY-using lib tests and the example compile; no automatic CI job runs them (R1 F16)
  cargo nextest run -p neomacs-layout-engine -p neomacs-font-materializer --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/M4/m04-le.log
  #   ⟨compare neomacs-layout-engine --moved $E/M4/m04-le-moved.map⟩: five private-item font_match tests moved
  #   with their module (D44), so the materializer runs in the same invocation (D46; fix-r2 erratum: the
  #   LE-only line could not pass, evidence/M4/fix-r2.md item 1)
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/M4/m04-dl.log  # ⟨compare⟩
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'
  #   → 2 passed: identical screens now that TTY output flows through neomacs-tty-render
  ```
- **Not runnable here:** GUI L5; GPU renderer tests; the `webview` arm of the reclaim hook (no WPE here).
- **Compile fan-out:** runtime (1 dependent), renderer (2), font-materializer, two new crates and
  `neomacs`: ≈1–2 min dev. Afterwards a runtime test build compiles no `neovm-*` crate, which retires one
  stack copy. Display-host's test build is the new copy, and it also compiles the runtime (dev-only). The
  guard edit costs a neovm-core lib test build (≈4 min).
- **Risks.**
  - The width fixture panics on an unlisted scalar; its contract test catches drift from the evaluator.
  - Ruling (b)'s equality may not hold. Its fallback is fixed above, so that outcome is not a new block.
  - The work list is the 11 test files above plus the 7 production files that `rg` finds at `5a7a7885`
    (`evidence/M4/steward-verify.log`).

---

## M5 — GNU multi-terminal parity in the evaluator

- **Status:** todo
- **Kind:** Lisp-visible, toward GNU. It fixes divergences that exist without any remote code. Three PRs.
- **Goal.** The evaluator's terminal model, display-name resolution, "is there a GUI" answers and
  redisplay follow GNU's multi-terminal contract (DESIGN §7.2–§7.5). A new kind of display connection
  then needs no special case.
- **Depends on:** M3.
- **Scope and PRs.**
  - **M5a, terminal records** (`ND/terminal/pure.rs`):
    - GNU ids: initial terminal 0; window-system terminals from `next_terminal_id`; first display 1;
    - ids from a **process-wide monotonic counter**. Deleted records are dropped, with no tombstones and
      no revival (`:330-348`, `:378-391`, `:594-605`; R2-28);
    - `terminal-live-p` answered from the record, `neo` for every window-system terminal (`:1372-1411`);
    - liveness flips before frames are deleted (`:1700-1776`);
    - `x-close-connection` per `gnu:src/xfns.c:7674-7690`.
  - **M5b, names and class 3**:
    - `DisplayConnectionFactory` installed in every interactive mode;
    - the `DisplayName` grammar (DESIGN §7.3);
    - `x-open-connection` and `x-create-frame` resolution with GNU errors and no silent fallback
      (`ND/display/mod.rs:3293-3441`, `NW/display_ext.rs:2190-2198`);
    - `check_x_display_info` designators for every `x-display-*` and monitor query
      (`ND/display/mod.rs:445-473`; `crates/neovm-core/src/emacs_core/commands/display_entries/mod.rs:561-602`).
    - **Class 3 answers from the frame's terminal kind** (D37). Each changed answer is listed with its GNU
      source in the PR. For example, on a TTY frame `neomacs-surface-available-p` becomes nil, and
      `image-flush` with a TTY frame signals "Window system frame should be used"
      (`ND/image/mod.rs:1224-1229,1413`).
    - **Preload `term/neo-win`** in `lisp/loadup.el` and provide `neomacs` at dump time
      (`neo-win.el:30-32` errors otherwise; today `NM/main.rs:4300` provides it only when the GUI layer
      loads). This is GNU parity (`gnu:lisp/loadup.el:304-309`).
      - **The reversal.** The Design horizon cites and justifies reversing
        `lisp/term/neo-preload.el:9-11` and `lisp/loadup.el:264-267` ("intentionally not loaded into the
        portable dump", R1 F18).
      - **The audit** covers every top-level effect: `neo-win.el:30-32, 40, 200-201, 233, 584, 586,
        589, 603, 642-646, 756`. Keep what GNU's window-system files also do at load; pgtk-win uses the
        same `".*"` entry, `gnu:lisp/term/pgtk-win.el:112`. Move the rest into
        `window-system-initialization`: the `s-` bindings, `dnd`, `neomacs-effects` and the darwin
        modifier setup.
      - Run `neomacs--window-setup` on the first `neo` open in a daemon.
    - Native names in a headless daemon fail with GNU's open error until M13.
    - **Docs** (R1 F17): add the KBOARD-sharing limitation to "Current limitations" in `docs/daemon.md`
      (`:183-207`). It lasts until M16.
  - **M5c, redisplay and loss**:
    - `redisplay_fn` visits every terminal (`NM/frame_layout.rs:474-490`, `NM/secondary_tty.rs:41-117`,
      `NM/main.rs:6807-6867`), with per-terminal sinks and `NativeWindowTreesOn(terminal)`;
    - one runtime with a per-frame `DisplayContext` and no process-global render-option memo
      (`LE/font/metrics.rs:1018-1058`);
    - terminal-tagged display events (E6);
    - `DisplayHangup{terminal, reason}` reaching `delete_input_terminal_noelisp` (`EV/keyboard.rs:3711-3736`).
      It replaces the hard-wired `TERMINAL_ID` (`:4646-4651`) and the render loop's `WindowClose{0}`
      (`RT/bootstrap.rs:753-757`);
    - a typed `ConnectionState::Closed` makes teardown setters succeed as no-ops
      (`ND/window_cmds/mod.rs:8763-8776`).
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `multi_tty_redisplay_repaints_unselected_client`: two `neomacsclient -t` clients in two ptys (template `daemon_lifecycle.rs:1828`); a third `neomacsclient -e '(with-current-buffer "*scratch*" (insert "MARK-1"))'`; both pty streams contain `MARK-1` within 5 s | `daemon_lifecycle` | **yes**: only the selected terminal is repainted today (`NM/frame_layout.rs:481-485`) |
  | `display_names_neo_label_without_offer_signals_gnu_open_error`: `(condition-case e (make-frame-on-display "neo:nobody") (error e))` → `(error "Display neo:nobody can’t be opened")`; `(length (frame-list))` → `1`; `(x-display-list)` → `nil` | `daemon_lifecycle` | **yes**: today "Don't know how to interpret display" [RB 04d] |
  | `display_names_headless_daemon_knows_neo`: `(list (featurep 'neomacs) (window-system-for-display "neo:x") (window-system-for-display ":987"))` → `(t neo neo)` | `daemon_lifecycle` | **yes** (no `neo` entry in a headless daemon today) |
  | `terminal_ids_follow_gnu_numbering`, `terminal_ids_are_monotonic_without_tombstones`, `deleted_terminal_is_never_revived`, `terminal_live_p_answers_from_record`, `delete_terminal_flips_liveness_before_deleting_frames` (a `delete-frame-functions` hook sees `(terminal-live-p (frame-terminal f))` nil), `x_close_connection_refuses_with_frames_then_closes`, `x_display_queries_accept_designators`, `x_create_frame_resolves_terminal_then_display`, `display_hangup_deletes_only_that_terminal_noelisp`, `closed_connection_setters_are_noops_during_teardown`, `class3_answers_follow_the_frame_terminal_kind` | neovm-core `moved_display_host` | **yes** for each divergence |
  | `per_frame_display_context_switches_scale_and_options`: two recording hosts with different device scales and render options in one pass; each frame stamps its own `layout_scale` | layout engine | **yes** (one global context today) |
  | re-derived: `ND/display/tests/display_test.rs:1220-1262`; `daemon_graphical_request_fails_without_creating_a_phantom_frame` stays green | — | re-derived from the GNU contract, never deleted |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m05-core.log     # ⟨compare⟩ + new pass
  cargo nextest run -p neomacs-layout-engine --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/m05-le.log  # ⟨compare⟩
  cargo nextest run -p neomacs-display-runtime $RT_FEATURES --lib $NX 2>&1 | tee $E/m05-rt.log  # ⟨compare⟩ (flag empty once M4 landed)
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m05-dl.log
  #   ⟨compare daemon_lifecycle⟩: the 35 existing plus the new multi_tty_ and display_names_ cases pass
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:** GUI L5, which pins today's GUI terminal numbering in some tests. Those tests are
  re-derived from the GNU contract (AGENTS.md §3).
- **GNU claims:** UNATTESTED. Forms A1–A7, A10 and A12–A14 (DESIGN §12.4).
- **Compile fan-out:** `neovm-display`, `neovm-eval`, the layout engine, `neomacs` and the dump: ≈2–3 min
  dev, plus the stack copies; the `daemon_lifecycle` build takes ≈2 min plus about 4 min of tests.
- **Risks.** Preloading neo-win in batch and TTY dumps may churn test expectations (J2 Q2). List the
  affected tests in the PR before changing any of them.

---

## M6 — Evaluator display-link host and headless frontend (L2)

- **Status:** todo
- **Kind:** additive (no production display uses it yet), plus GNU's wait disciplines.
- **Goal.** The evaluator side of a display link (`LinkConnection`, outbox, reader, writer, request
  table) and the frontend side (`FrontendSession`, `HeadlessPresenter`) exist. They are proven equivalent
  over the in-process transport and a socketpair, without a daemon. Waits follow GNU's two disciplines.
  Quit is never lost.
- **Depends on:** M2, M4 (it extends the `neomacs-display-host` crate M4 creates), M5.
- **Scope.**
  - `neomacs-display-host/src/link/`:
    - `connection.rs`: `LinkConnection: DisplayConnection` for one channel of a session.
    - `outbox.rs` (D32, R2-10):
      - **keyed latest-wins state records** for window, UI, config, selection and media, emitted in
        dependency order;
      - latest-wins presentations per frame with a synchronous `SupersededPresentation` proof;
      - K = 2 credits on `Activated` or `Discarded`;
      - a cap of 4 retained presentations per frame;
      - requests bounded at 1024, where the 1025th fails **that call** with `neomacs-display-error`
        (`resource-limit`);
      - nothing ever closes a session because a peer is slow.
    - `reader.rs`: decode; route; convert with the input bridge's logic from
      `NM/input_bridge/publication.rs:146-195`; early quit from an evaluator-published `QuitPolicy` using
      the **actual** `quit-char`. A C-g from another terminal quits unless single-kboard state holds,
      which cannot happen before M16 (R2-24).
    - `writer.rs`: sends `InputProgress{consumed_through}` whenever the evaluator's published consumption
      advances, at most every 20 ms, and `QuitKeys` config (D33, R2-07).
    - `requests.rs`: the `RequestTable`, mapping `HostReplyId` to `RequestId`.
    - `inprocess.rs`: typed envelopes and `Payload::Shared`.
  - `NS/wait/mod.rs` (D16, R2-08, R1 Q1):
    - `SpecialInputWaitPolicy::CompleteOnHostReply(HostReplyId)` beside `CompleteOnResize`
      (`:339-366`);
    - `WaitRequest::host_reply(id, deadline, discipline)` with two disciplines: `Selection` (timers and
      process output run) and `DisplayEvent` (`TimerWaitPolicy::Suppress`, `ProcessWaitPolicy::None`, as
      `resize_ack` at `:572-582`);
    - a `display_wait` helper with the per-primitive deadline table (DESIGN §6.13).
  - `neovm-eval`:
    - `HostReplyId` beside `HostResult` (`crates/neovm-eval/src/host_effect/contracts.rs:503-508`);
    - an unwind guard that cancels the cell, sends `Cancel`, and drops the late reply on **any** non-local
      exit;
    - the condition family `neomacs-display-error` (parent `error`, DESIGN §10);
    - a `DisplayRequestOutcome` shaped to converge with `HostResult`;
    - evaluator-published snapshots of the link-relevant Lisp variables (`QuitPolicy`,
      `neomacs-display-keepalive-timeout`, `neomacs-display-version-policy`), refreshed when the
      variable is set (R2-27).
  - New `crates/neomacs-frontend/` (lib + bin; workspace dependencies: DP, `neomacs-display-wire`,
    `neomacs-session` and `neomacs-font-materializer` only):
    - `session.rs`: `FrontendSession`;
    - `headless.rs`: virtual windows; decode, apply, seal, `validate_for_materialize` and
      `materialize()`; `Activated` acks; records `render_text()` and JSON per presentation under
      `--record DIR`;
    - `input.rs`: the 4096 unconsumed-edge bound counted from `InputProgress`, with **quit edges
      exempt**;
    - `control.rs`: a test control channel on stdin (`key`, `text`, `wheel N`, `wait`, `dump`, `quit`);
    - `main.rs`.
  - `host_effect_guard_test.rs`: declare the transport and endpoint owners.
  - `check-crate-graph.py`: `neomacs-frontend`'s closure contains no `neovm-*` and no layout engine.
  - CI: the display filter and the macOS path filter gain `package(neomacs-frontend)`.
- **New tests** (new API; red-before n/a unless marked).
  - **display-host**:
    - `scripted_session_is_equivalent_over_inprocess_and_socketpair` (identical delivered state,
      presentations after coalescing, exactly-once retirement set, reply outcomes);
    - `stalled_peer_with_100k_window_ops_keeps_bounded_state` (memory bounded by live objects; no
      `Goodbye`; on drain the final state arrives; superseded presentations retired exactly once);
    - `request_beyond_pending_bound_fails_that_call`, `connection_loss_fails_every_pending_request`;
    - `reader_raises_quit_for_the_actual_quit_char` (**red before**: the bridge tests only the default
      character, `publication.rs:179`);
    - `quit_edge_reaches_lisp_after_5000_unconsumed_wheel_events` and
      `input_progress_is_sent_without_presentations` (R2-07).
  - **neovm-core**:
    - `selection_style_wait_runs_timers`;
    - `display_event_wait_runs_no_timers_or_filters` (**red before** for a generalized wait; today's
      resize wait already suppresses them, and the new one must keep that);
    - `host_reply_wait_is_quit_interruptible`, `host_reply_wait_honours_its_deadline_variable`;
    - `throw_from_timer_cancels_the_request_cell` (R2-08);
    - `cg_from_another_terminal_quits_outside_single_kboard` (R2-24).
  - **frontend**:
    - `headless_presenter_reconstructs_a_golden_session` (replays a wire fixture; `render_text` matches);
    - `frontend_rejects_invalid_presentation_without_panicking`;
    - `frontend_exempts_quit_edges_from_the_bound`.
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-display-host --features neovm-core/dbus-vendored -E 'test(~link)'
  cargo nextest run -p neomacs-frontend                       # builds no neovm-* crate
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m06-core.log     # ⟨compare⟩ + new pass
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-crate-graph.py    # exit 0
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m06-dl.log  # ⟨compare⟩
  ```
- **Compile fan-out:** `neovm-system` and `neovm-eval` (conditions, wait policy): ≈2 min dev plus the
  stack copies; new crates otherwise.

---

## M7 — End-to-end: a daemon renders into a separate frontend process

- **Status:** todo
- **Kind:** new feature (the first remote frame).
- **Goal.** A headless daemon accepts a dial-in session through its display listener in the
  environment-independent rendezvous directory, and Lisp opens the display by name. The daemon's frame is
  decoded, validated and materialized **in a separate frontend process**, whose recorded text equals the
  presentation the daemon submitted. Verified headlessly.
- **Depends on:** M6.
- **Scope.**
  - `neomacs-display-host/src/listener.rs` (D8, D31; R2-01, R2-02, R2-26, R2-27, R2-34):
    - **Path.** The listener lives at `<rendezvous>/<server-name>.display`. `<rendezvous>` is
      `/tmp/neomacs-<uid>`; `NEOMACS_DISPLAY_RENDEZVOUS_DIR` overrides it, and an absolute `server-name`
      puts the socket beside that path. The directory is created 0700 and checked with
      `server-ensure-safe-dir` rules.
    - **Permissions.** The socket's mode is set by `chmod(2)` on the path after `bind`, never `fchmod`.
      The peer uid is checked with `SO_PEERCRED` / `getpeereid`.
    - **Handshakes.** Each runs on its own task with a 10 s deadline, at most 16 at once.
    - **Offers.** An offers table; label grammar `[A-Za-z][A-Za-z0-9._@+-]{0,62}`;
      `LabelInUse{state}` and token-less `takeover`; a 60 s offer expiry.
    - **Relays.** A connection that starts with a relay preface gets a `ProbeReply{build, durability,
      server_socket}` and then carries the spliced frontend stream.
    - **Durability.** `durability` is computed from `/proc/self/cgroup` (user-manager unit or session
      scope) and `/var/lib/systemd/linger/USER`.
    - **Policy.** It reads policy variables only through the published snapshots.
    - **Start.** It starts at daemon readiness (`NM/daemon.rs`) and on `(neomacs-display-listen)`.
  - `neomacs-display-host/src/factory.rs`: `neo:LABEL` → claim the offer → `OpenChannel` →
    `ChannelOpened` within `neomacs-display-open-timeout`, as a **display-event-style** wait. A second
    `x-open-connection` of the same name opens a second channel (DESIGN D7).
  - `neomacs` binary:
    - the `--frontend=headless|gpu` role, dispatched before any Lisp initialization (the
      `runtime_mode_from_program_name` precedent, `NM/main.rs:285-296`), running `neomacs_frontend::run`;
    - `neomacs--frame-snapshot` works for frames on link terminals and gains the optional SOURCE argument
      `'submitted`: the text of the last presentation the daemon submitted for that frame (D30).
  - `NS/environment/mod.rs:257-279`: export the frame's display as `DISPLAY` only for a host-local X11
    terminal (DESIGN §11.4). Export `NEOMACS_DISPLAY=<terminal name>` for frames on `neo` terminals
    (R2-33).
  - `NM/bin/neomacsclient.rs:839-857`: `-c` without `-d` prefers `NEOMACS_DISPLAY` over
    `WAYLAND_DISPLAY` and `DISPLAY`.
- **New tests.** These are root-level in `crates/neomacs/tests/daemon_lifecycle.rs`. The fixture sets
  `NEOMACS_DISPLAY_RENDEZVOUS_DIR=<root>/rendezvous` for the daemon and the frontend alike. The frontend is
  spawned as `env!("CARGO_BIN_EXE_neomacs") --frontend=headless --connect <root>/rendezvous/rd.display
  --label t1 --record <dir>`. All are **red before**: there is no listener and no frontend role.

  | Test | Assertions |
  |---|---|
  | `remote_display_headless_frontend_renders_daemon_frame` | `neomacsclient -s rd -c -n -d neo:t1` succeeds; `(terminal-live-p (frame-terminal F))` → `neo`; `(frame-parameter F 'display)` → `"neo:t1"`; `(frame-parameter F 'client)` → `nowait`; `(x-display-list)` → `("neo:t1")`; within 10 s the frontend's latest recorded `render_text()` equals `(neomacs--frame-snapshot F 'text 'submitted)` |
  | `remote_display_snapshot_equals_last_submitted_at_quiescence` | after 2 s without input, `(equal (neomacs--frame-snapshot F 'text) (neomacs--frame-snapshot F 'text 'submitted))` → `t`. This answers R1 Q2; a failure is reported as a finding about the snapshot path, not hidden |
  | `remote_display_keys_reach_the_buffer_and_return` | control `key a`, `key b`, `key c` → the selected window's buffer line ends in `abc`; the next recorded presentation contains `abc` |
  | `remote_display_close_connection_and_delete_terminal` | `(condition-case e (x-close-connection "neo:t1") (error e))` → `(error "Display still has frames on it")`; `(delete-terminal (frame-terminal F))` → the frontend records `CloseChannel`; `(+ 20 22)` → `42` |
  | `remote_display_loss_deletes_the_terminal_noelisp` | `kill -9` the frontend → within 5 s `(frame-live-p F)` → `nil`, `(length (frame-list))` → `1`, a `delete-frame-functions` recorder saw F (deferred), the daemon is alive |
  | `remote_display_protocol_major_mismatch_is_a_typed_refusal` | a frontend with `--test-protocol-major 99` exits non-zero naming both majors; no terminal is created; `(+ 20 22)` → `42` |
  | `remote_display_schema_mismatch_is_a_typed_refusal` | a frontend with `--test-wire-schema 0` gets `SchemaMismatch` naming both builds (R2-16) |
  | `remote_display_garbage_closes_only_that_session` | raw bytes after the preface → that session is closed with a typed reason; a second frontend still attaches |
  | `remote_display_rendezvous_ignores_runtime_dir_and_tmpdir` | the daemon and the frontend run with different `XDG_RUNTIME_DIR` and `TMPDIR` and still meet at the rendezvous (R2-02) |
  | `remote_display_subprocess_display_excludes_neo_names` | with F selected, `(with-output-to-string (call-process "sh" nil standard-output nil "-c" "printf %s \"$DISPLAY\""))` → `""` |
  | `remote_display_subprocess_env_has_neomacs_display` | the same with `$NEOMACS_DISPLAY` → `"neo:t1"` (R2-33) |
  | `remote_display_neomacsclient_c_prefers_neomacs_display` | `neomacsclient -s rd -c -n` with `NEOMACS_DISPLAY=neo:t1` set and `DISPLAY` unset creates a frame on `neo:t1` |
  | `remote_display_second_open_is_a_second_terminal` | `(progn (x-open-connection "neo:t1") (length (x-display-list)))` → `2` (GNU A12 analogue) |
  | existing `daemon_graphical_request_fails_without_creating_a_phantom_frame` | still green |

  Plus `neomacs-display-host` unit tests:
  - `listener_refuses_unsafe_directory`;
  - `listener_socket_mode_is_0600_by_stat`, which stats the path (R2-26);
  - `listener_refuses_foreign_uid` (simulated credentials);
  - `listener_handshakes_do_not_block_each_other`: a silent peer holds no other connection (R2-34);
  - `listener_reads_policy_from_the_published_snapshot` (R2-27);
  - `durability_from_cgroup_and_linger`, over injected `/proc/self/cgroup` and linger inputs (D31);
  - `label_grammar_never_matches_x_display_regexp`;
  - `offer_expires_with_fake_clock`.

  And a `neovm-system` unit test, `child_display_never_exports_neo_names` (**red before**: the injector
  copies any display string).
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_)'
  #   → 13 passed
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m07-dl.log
  #   ⟨compare daemon_lifecycle⟩: baseline + M5 + M7 cases pass
  cargo nextest run -p neomacs-display-host --features neovm-core/dbus-vendored -E 'test(~listener) | test(~label) | test(~durability)'
  cargo nextest run -p neovm-core --features dbus-vendored -E 'test(~child_display)'
  ```
  Record a sample of the frontend's `render_text()` and the daemon's submitted text in `$E/m07-e2e.log`.
- **Compile fan-out:** `neomacs-display-host`, `neomacs-frontend`, `neovm-system` and the `neomacs` bins:
  ≈1–2 min dev; the `daemon_lifecycle` build takes ≈2 min.
- **Notes.** Pointer input is not supported on link displays until M8, and the headless frontend does not
  send it. Until M12, losing a dial-in session deletes the terminal at once. M12 re-derives
  `remote_display_loss_deletes_the_terminal_noelisp` with an explicit `neomacs-display-detach-timeout`
  of 0.

---

## M8 — Evaluator-side pointer resolution for every display

- **Status:** todo
- **Kind:** behaviour-preserving for local users (proved by a differential test); a new capability for
  links.
- **Goal.** Text positions under the pointer are resolved by the evaluator, against the presentation the
  user saw (DESIGN D4, §7.5), for the in-process GUI and for links alike. Frames crossing the seam carry no
  hit-test text source and no evaluator GC roots.
- **Depends on:** M7.
- **Scope.**
  - DP: the presented hit index splits explicitly into the non-text index (regions, resize handles,
    appearance regions, chrome interaction ids) and the evaluator-only text source. Submission strips the
    source (`DP/presented_pointer.rs:683-687`).
  - Evaluator: `InternalFrontendEvent::PresentedRegion` (`EV/keyboard.rs:2086-2111`) carries a
    `PointerTarget` instead of a `hit`. Resolution runs against the frame's active
    `PreparedDisplayPresentation` (`EV/data/window_state.rs:3154-3170`), with the same text-source logic
    (`LE/presentation/spatial.rs:594-662`). `ScrollProjection` resolves against retained exported
    coverage, kept until its presentation is retired (within the per-frame cap). `CurrentGeometry`
    resolves against the latest geometry.
  - Runtime: `RT/pointer_events.rs`; `RT/frame_windows.rs:1136-1158` (hover appearance stays local);
    `RT/frame_compositor/input_scroll.rs:367-383` (local appearance; the semantic target becomes a
    `ScrollProjection`).
  - Headless frontend control: `click X Y`, `drag …`.
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `pointer_resolution_differential`: a corpus of layout-engine presentations (bidi, wide characters, images, mode and header lines, margins, fringes, child frames, coverage with offsets) × a grid of points; today's frontend resolution equals the evaluator's, bit for bit on every `PresentedHit` field | layout engine | n/a (proves equality) |
  | `pane_morph_pointer_resolution_differential`: the same with pane-morph inverse projection (`RT/frame_windows.rs:825,868,883,908,1148`), in the runtime, which owns that transform (R1 F27) | runtime | n/a |
  | `submitted_presentation_carries_no_evaluator_gc_roots` | layout engine | **yes** (the lazy source is the default today) |
  | `remote_display_click_moves_point`: control `click X Y` on a text cell → `(point)` equals `(posn-point (posn-at-x-y X Y F))` evaluated in the daemon | `daemon_lifecycle` | **yes** (links cannot click before M8) |
  | `remote_display_click_in_mode_line_is_a_mode_line_posn`: `(posn-area (event-start last-input-event))` → `mode-line` | `daemon_lifecycle` | **yes** |
  | existing input-scroll and frame-window tests | runtime | unchanged |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-display-protocol $NX 2>&1 | tee $E/m08-dp.log                   # ⟨compare⟩
  cargo nextest run -p neomacs-layout-engine --features neovm-core/dbus-vendored $NX 2>&1 | tee $E/m08-le.log  # ⟨compare⟩ + new pass
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m08-core.log      # ⟨compare⟩
  cargo nextest run -p neomacs-display-runtime --lib $NX 2>&1 | tee $E/m08-rt.log               # ⟨compare⟩ + pane-morph differential
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_)'
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:** GUI L5 clicks in the in-process GUI.
- **Compile fan-out:** DP (≈15 dependents), the evaluator, the runtime and the layout engine: ≈2.5 min
  dev plus the stack copies.

---

## M9 — Synchronous services, replicas, typed capabilities and side-effect policy

- **Status:** todo
- **Kind:** Lisp-visible (GNU-ward fixes plus typed extensions).
- **Goal.** Selections, menus, tooltips, monitors, resources and capabilities work on link displays with
  GNU's wait discipline and typed failures (DESIGN §6.13, §9, §10, §11.3–11.4). Ownership is lazy and
  trusted only when exact, and the remote side gains no default authority over the laptop.
- **Depends on:** M7.
- **Scope.**
  - **Selections per terminal** (D15; R2-11, R2-12, R2-32).
    - *Set.* `Own{selection, generation, content}` is a keyed latest-wins setter. Data is inline up to
      64 KiB; larger values are `Deferred`. A laptop paste of deferred data sends `Convert`, and the
      evaluator converts through the Lisp selection converters and answers on the bulk lane.
      `select-active-regions` (`command_loop.rs:1262-1280`) therefore ships ownership, not regions.
    - *Ownership replica.* `SelectionState` is an ordered replica. The `selections/1` capability
      carries `ownership: exact | sequence | none`. The headless presenter scripts both `exact` and
      `none`. Platform backends report `none` until their signal lands, each with its CI test: X11
      `SelectionClear` with XFixes, focused Wayland `data_device`, macOS `changeCount`, and the Windows
      sequence number. Today `DR/clipboard.rs:371-384` knows no owner.
    - *Get.* It is answered locally only under exact ownership, else by a selection-style request
      bounded by `x-selection-timeout`. The frontend answers within its own 5 s bound.
    - *Owner and existence.* `x-selection-owner-p` and `x-selection-exists-p` come from the replica.
      Existence never fetches data: neo-win's fetch-to-test (`lisp/term/neo-win.el:468-478`) is replaced
      with `SelectionExists`.
    - *neo-win.* The `gui-backend-*-selection` methods pass their terminal (`lisp/term/neo-win.el:456-466`).
  - Popup menus and dialogs over links, with per-session tokens and Lisp-host-laid-out labels
    (`ResolvedMenuLabel` shape, F12, F14). Evaluator-authoritative tip state for `x-hide-tip`
    (`gnu:src/xfns.c:8984-9021`). Causal tooltip stamps on input.
  - Per-terminal replicas: monitors (`display-monitor-attributes-list` by designator;
    `display-monitors-changed-functions` receives the terminal), desktop facts, and the display's
    resource database for `x-get-resource`.
  - **Capabilities.**
    - The Lisp API and conditions of DESIGN §10.
    - The silent defaults of `EV/protocol.rs:676-749` (`Ok(None)` from `request_video/webkit/surface`,
      and the webkit no-ops) become `neomacs-display-capability-unavailable` on **every** display.
    - `neomacs-surface-available-p` takes an optional display.
  - **Side-effect policy** (R2-15).
    - The per-host frontend policy defaults `Dialed` links to `on-input` for clipboard read, clipboard
      write and open-URI.
    - Loopback, link-local and private-network URI targets are refused unless the host policy names
      them, and open-URI is rate-limited.
    - GPU programs are denied on `Dialed` links unless allowed. The frontend validates every shader with
      naga before compiling it.
  - `neomacs-display-open-uri`. `lisp/net/browse-url.el` gets a `neo` branch next to its pgtk and Android
    branches (`:927-951`, `:1073-1087`), recorded as a documented divergence. Every Lisp writer of
    `DISPLAY` is audited.
  - File drop: `FileDrop{uris}` becomes GNU's `(drag-n-drop POSN FILES)` with `file://HOST/` URIs
    (today discarded, `NM/input_bridge.rs:428-431`).
  - The resize acknowledgement goes through the request table as a **display-event-style** wait, keeping
    today's 1 s bound.
  - Headless frontend control: `set-clipboard TEXT`, `foreign-copy TEXT`, `paste SELECTION`,
    `reply-delay MS`, `ownership exact|none`, `menu-pick N`, `stop`/`cont`.
- **New tests** (`daemon_lifecycle`, root-level). Each is **red before**, because links lack the service,
  except where noted.

  | Test | Assertion |
  |---|---|
  | `remote_selection_owned_yank_makes_no_request` | with `ownership exact`, after `(gui-set-selection 'CLIPBOARD "abc")`, `(gui-get-selection 'CLIPBOARD)` → `"abc"` and `(plist-get (neomacs-display-info T) :requests)` is unchanged |
  | `remote_selection_unknown_ownership_always_asks` | with `ownership none`, the same get makes one request (today's local behaviour, R2-12) |
  | `remote_selection_foreign_copy_between_keys_is_seen` | `ownership exact`; control `foreign-copy xyz` between two keys → `C-y` inserts `xyz` (R2-12) |
  | `remote_selection_large_region_is_deferred` | `(gui-set-selection 'PRIMARY (make-string 200000 ?x))` → the frontend records `Own{Deferred}` with no data; control `paste PRIMARY` receives 200,000 bytes over the bulk lane (R2-11) |
  | `remote_selection_active_region_publishes_ownership_only` | 50 commands extending an active region send no region data over lane 0 (R2-11) |
  | `remote_selection_exists_never_fetches_data` | `(x-selection-exists-p 'PRIMARY)` makes no data transfer |
  | `remote_selection_foreign_owner_roundtrip` | control `set-clipboard xyz` → `(gui-get-selection 'CLIPBOARD)` → `"xyz"` |
  | `remote_selection_timeout_uses_x_selection_timeout` | with `reply-delay 5000`, `(let ((x-selection-timeout 200)) (condition-case e (gui-get-selection 'CLIPBOARD) (error e)))` → `(error "Timed out waiting for reply from selection owner")` within 2 s |
  | `remote_selection_frontend_reply_bound` | with `reply-delay 60000` and `x-selection-timeout` 0, the get ends with the typed timeout after about 5 s instead of hanging (R2-32) |
  | `remote_selection_wait_runs_timers` and `remote_selection_wait_is_quit_interruptible` | a timer fires during the wait; a C-g from the frontend ends it with `quit` |
  | `remote_selection_loss_during_wait_is_typed` | killing the frontend during the wait → `neomacs-display-disconnected` |
  | `remote_policy_clipboard_read_on_input` | a timer reading the clipboard 5 s after the last input → `neomacs-display-policy-refused`; `C-y` within 2 s of input succeeds (R2-15) |
  | `remote_open_uri_refuses_loopback` | `(neomacs-display-open-uri "http://127.0.0.1:8080/" T)` → `neomacs-display-policy-refused` (R2-15) |
  | `remote_shader_surface_is_off_by_default_on_dialed` | `neomacs-surface-create` on F → `(neomacs-display-capability-unavailable shader-surface #<terminal …> policy-refused)` (R2-15) |
  | `remote_popup_menu_returns_the_selection` | `menu-pick 2` → `x-popup-menu` returns the second item's value |
  | `remote_x_hide_tip_answers_while_the_frontend_is_stopped` | SIGSTOP the frontend; `(x-hide-tip)` returns within 1 s |
  | `remote_monitors_are_per_terminal` | `(display-monitor-attributes-list "neo:t1")` reflects the frontend's advertised monitor; the changed-functions hook receives the terminal |
  | `remote_capability_unavailable_is_typed` | `neomacs-video-load` on F → `(neomacs-display-capability-unavailable video #<terminal …> requires-same-host)`; caught by `(condition-case nil … (error 'caught))` |
  | `remote_open_uri_reaches_frontend_policy` | `(neomacs-display-open-uri "https://example.org" T)` within 2 s of input → recorded by the frontend |
  | `remote_browse_url_does_not_export_neo_display` | with F selected and a `browse-url-browser-function` that records `(getenv "DISPLAY")` → not `"neo:t1"` |
  | `remote_file_drop_becomes_a_drag_n_drop_event` | the event arrives; `dnd.el` rejects the foreign host, as GNU does |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_)'
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m09-core.log     # ⟨compare⟩ + new pass
  cargo nextest run -p neomacs-frontend
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m09-dl.log  # ⟨compare⟩
  ```
- **Not runnable here:** each platform's ownership signal (X11, Wayland, macOS, Windows; CI per platform).
- **GNU claims:** UNATTESTED; forms A15, A17 and A18. The move from the fixed 5 s clipboard wait
  (`NM/main.rs:1212,1846`) to `x-selection-timeout` plus the frontend's bound is a deliberate local change
  (DESIGN §12.3 item 12).
- **Compile fan-out:** `neovm-display`, `neovm-eval`, `neovm-system`, display-host, frontend and Lisp:
  ≈2–3 min dev plus the stack copies.

---

## M10 — Assets: fonts by content, images decoded on the evaluator

- **Status:** todo
- **Kind:** Lisp-visible (`image-cache-size` becomes nominal). It changes how every display loads fonts
  and images.
- **Goal.** The frontend never opens an evaluator-host path. Fonts cross by SHA-256 with an
  evaluator-computed raster policy and a typed pending state. Images are decoded on the evaluator; their
  pixels are content-addressed and cached persistently (DESIGN §6.11, §9).
- **Depends on:** M7.
- **Scope.**
  - DP: `FontReplay` path variants become `FontAssetRef{asset, face_index, container}` on the wire, with
    the path kept as Lisp-side metadata for `font-info`. Also `FontRasterPolicy` and a producer-scoped
    `ResolvedFontId` (F8).
  - Layout engine: `FontRasterPolicy` computed where metric policy is (`LE/font/metrics.rs:2081-2098`); a
    content-hash cache keyed by (dev, inode, mtime, size). The daemon adds a private application font
    directory (`$XDG_DATA_HOME/neomacs/fonts/synced/`, through `FcConfigAppFontAddDir`) to its own
    Fontconfig configuration only; M11's `--sync-fonts` fills it (R2-29).
  - `neomacs-font-materializer`: byte-based loading for outline, bitmap (pcf, bdf) and WOFF containers.
    **CI** (R1 F15): remove `--exclude neomacs-font-materializer` from the archive
    (`test-suite.yml:128`) and add `package(neomacs-font-materializer)` to the display filter and the
    macOS path filter.
  - Renderer: the glyph atlas takes a `FontBytesSource` parameter. The emergency family fallback
    (`RW/glyph_atlas/mod.rs:2229-2379`) is deleted for every display and replaced by typed pending and
    unresolved glyph states with exact advances.
  - **New `crates/neomacs-media-cpu/`** (R1 F28).
    - *Sources.* `RW/image_cache/decoder.rs`; `RW/image_probe.rs`, `RW/svg.rs`, `RW/xpm.rs` and
      `RW/xbm.rs` at the renderer root (2,076 lines, no `wgpu::`); `RW/image_sequence.rs` (656 lines,
      used by `image_probe.rs:118,127`); scaling; naga validation and `SurfaceContract`.
    - *Coupling.* `decoder.rs` begins with `use super::*;` over `RW/image_cache.rs` (2,806 lines, 64
      `wgpu::` references). The CPU items it uses are split out of that module first, in its own
      commit.
    - *Ownership.* The evaluator owns the decoder (`NM/image_catalog.rs`). `image-cache-size` is a
      nominal ledger (`gnu:src/image.c:2484-2494`). Tool-bar icons go through the catalog (F11).
    - *Transfer.* Pixels travel as `ImageRealize` plus `AssetId::Content` (the SHA-256 of the pixel
      buffer and its format header, R2-18) on the bulk lane, deflated on `Dialed` links, with
      per-terminal residency.
  - **Frontend.**
    - A persistent cache in the platform cache directory (`$XDG_CACHE_HOME/neomacs/assets/sha256/`,
      0700, on Linux). Every `Content` asset, fonts and pixels alike, is verified before use and
      persistence.
    - A background hash index of installed fonts and cached assets keyed by (dev, inode, mtime, size).
      `Offer` is answered with `Have` or `Want` from it, and **no inventory is ever sent** (R2-17).
    - Prefetch by visibility.
    - A `--verify-fonts` mode for the headless presenter.
  - The in-process transport passes font and pixel bytes as `Payload::Shared`.
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `remote_fonts_requested_by_content_hash`: the frontend's wanted font ids equal the `sha256sum` of the file named by `(font-info (face-font 'default))` | `daemon_lifecycle` | **yes** |
  | `remote_fonts_verified_from_bytes`: with `--verify-fonts`, every glyph id in `char_fonts` exists in the received font and its advance matches within 1/64 px | `daemon_lifecycle` | **yes** |
  | `remote_second_attach_transfers_no_font_bytes` (answered `Have` from the persistent cache) | `daemon_lifecycle` | **yes** |
  | `remote_frontend_never_sends_an_inventory`: `SessionHello` and every uplink asset message carry only offered ids | `daemon_lifecycle` | **yes** (R2-17) |
  | `remote_withheld_font_is_typed_pending_then_resolves`: `(plist-get (neomacs-display-info T) :pending-assets)` → 1, then 0 | `daemon_lifecycle` | **yes** |
  | `remote_second_session_reuses_cached_pixels`: a second session showing the same image transfers no pixel bytes | `daemon_lifecycle` | **yes** (R2-18) |
  | `poisoned_asset_put_is_rejected_and_not_persisted` (fonts and pixels) | frontend unit | n/a |
  | `image_size_answers_with_the_frontend_stopped`: SIGSTOP the frontend; `(image-size (create-image "etc/images/splash.png"))` returns promptly | `daemon_lifecycle` | **yes** |
  | `image_cache_size_is_the_nominal_ledger` (value change UNATTESTED) | neovm-core | **yes** |
  | `remote_image_pixels_match_the_evaluator_decode` (hash equality) | `daemon_lifecycle` | **yes** |
  | `clear_image_cache_emits_image_drop` | display-host | n/a |
  | `synced_font_dir_is_private_to_the_daemon`: a family in the app font directory appears in `(font-family-list)` but not in `fc-list` run as a subprocess | `daemon_lifecycle` | n/a (R2-29) |
  | `atlas_never_opens_font_paths_for_asset_fonts` (mock `FontBytesSource`, no filesystem access) | renderer, CPU-only | **yes** |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-media-cpu
  cargo nextest run -p neomacs-font-materializer
  cargo nextest run -p neomacs-renderer-wgpu --lib -E 'test(~font_bytes_source) | test(~asset_pending)'
  cargo nextest run -p neomacs-frontend
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m10-core.log     # ⟨compare⟩ + new pass
  cargo nextest run -p neomacs-display-protocol $NX 2>&1 | tee $E/m10-dp.log                  # ⟨compare⟩
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m10-dl.log  # ⟨compare⟩
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:** GPU rasterization from asset bytes (CI L5, M11b).
- **GNU claims:** the `image-cache-size` value change is UNATTESTED.
- **Compile fan-out:** DP (≈15 dependents), the layout engine, the font-materializer, the renderer, the new
  crate and the evaluator: ≈3 min dev plus the stack copies. Lands as M10a (fonts) and M10b (images).
- **Risks.** First-use font bandwidth (0.34–10.8 MB per file, u5 §2.14); licensing decision U8.

---

## M11 — Relay and launcher: `neomacs --remote` over ssh (headless-verified)

- **Status:** todo
- **Kind:** new feature. It is the first remote session a user can start, verified with the headless
  presenter. The first *visible* remote window is M11b (CI); v1 overstated M11 as "the first user-facing
  remote" (R2-21).
- **Goal.** `neomacs --remote HOST` and `neomacs-frontend --remote HOST` attach a laptop to a remote
  daemon through ssh stdio. User data never reaches a remote shell, and a second daemon is never started.
  Frames are created by `server.el` as `-nowait` frames (DESIGN §3.2).
- **Depends on:** M9, M10 (fonts come before the relay: without them a real remote user sees no text,
  P3 §12).
- **Scope.**
  - **Relay** (D31, D35, D36; R2-01, R2-02, R2-06, R2-13, R2-31). It lives in `NM/bin/neomacsclient.rs`
    as `neomacsclient --display-relay`, with **no other arguments ever used by the frontend**.
    - Read the frontend preface and `RelayRequest` (≤ 64 KiB, validated).
    - Check the rendezvous with `server-ensure-safe-dir` rules. Also add GNU's socket-directory
      ownership and mode check to ordinary client connects (missing at `:440`; GNU
      `lib-src/emacsclient.c:1233-1258`).
    - Probe the display listener.
    - If nothing answers, auto-start the daemon. Under a reachable service manager that is
      `systemd-run --user --collect --unit=neomacs-NAME -p Type=notify … --fg-daemon=NAME`. Otherwise it
      is `--bg-daemon` with `setsid`, as today (`NM/client_daemon.rs:96-122`). Either way `DISPLAY` and
      `WAYLAND_DISPLAY` are removed and `SSH_AUTH_SOCK=/tmp/neomacs-<uid>/agent.sock` is set.
    - Re-point the agent link atomically.
    - Write `RelayStatus`, then splice.
    - After the `SessionWelcome` record header, send `-nowait -display neo:L -window-system -env … -dir …
      [-file …]` with `server.el` quoting to the server socket path the probe returned, unless
      `resume`. A `server.el` `-error` reply goes to stderr, followed by a non-zero exit.
  - **`--fg-daemon` readiness.** Send `sd_notify(READY=1)` when `NOTIFY_SOCKET` is set. This is GNU
    parity for libsystemd builds (`#ifdef HAVE_LIBSYSTEMD`, `gnu:src/emacs.c:3433-3435`; UNATTESTED).
    Neomacs sends nothing today, and the datagram protocol needs no libsystemd.
  - **`neomacs-frontend`** (R2-14, R2-35):
    - `--remote [USER@]HOST [--label L] [-s NAME] [--ssh-program P] [--remote-command CMD]
      [--sync-fonts FAMILY…] [-- FILES]`; `neomacs --remote` runs the same entry.
    - The ssh argv is `ssh -T HOST -- neomacsclient --display-relay`, a constant. Automatic retries add
      `-o BatchMode=yes -o ControlPath=none -o ServerAliveInterval=5 -o ServerAliveCountMax=2`.
    - The first attempt inherits the terminal, or else uses `SSH_ASKPASS_REQUIRE=force` with
      `--askpass`.
    - It scans 64 KiB of stdout for the preface.
    - Exit 127 gives a typed `remote-command-not-found` and one visible `sh -lc` retry.
    - The label is sanitized; tokens and caches live in platform directories.
    - `--sync-fonts` copies into the daemon's private application font directory (M10).
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `remote_relay_attach_through_ssh_shim`: a test-created shim that asserts its remote command equals the constant, drops the host argument and execs the command locally under the fixture's environment; `neomacs --remote fake --ssh-program SHIM --frontend=headless --record D` auto-starts a daemon whose `(getenv "DISPLAY")` → `nil`; `(frame-parameter F 'client)` → `nowait`; `(length server-clients)` → `0` once the frame exists; recorded text equals the submitted text | `daemon_lifecycle` | **yes** |
  | `remote_relay_files_travel_in_the_preamble`: opening a file named `$(touch PWNED);x` creates no `PWNED` anywhere and visits the literal name (R2-13) | `daemon_lifecycle` | **yes** |
  | `remote_relay_finds_daemon_across_runtime_dirs`: a daemon started with `XDG_RUNTIME_DIR=A`; the relay runs with it unset and with `TMPDIR=B`; it reaches the same daemon and starts no second one (R2-02) | `daemon_lifecycle` | **yes** |
  | `remote_relay_never_starts_a_second_daemon`: the listener answers but the server socket is removed; a new attach gets `ServerSocketUnreachable`, a resume still works, and one daemon exists (R2-01) | `daemon_lifecycle` | **yes** |
  | `remote_relay_attach_error_is_reported`: a `server-after-make-frame-hook` that signals → the frontend exits naming the error text (R2-31) | `daemon_lifecycle` | **yes** |
  | `remote_relay_skips_rc_noise_before_the_preface`: the shim prints 300 bytes before exec; the attach succeeds and reports the skipped bytes (R2-14) | `daemon_lifecycle` | **yes** |
  | `remote_relay_command_not_found_is_typed`: the shim exits 127 → typed error, one `sh -lc` retry visible in the log (R2-14) | `daemon_lifecycle` | **yes** |
  | `remote_relay_repoints_the_agent_link`: `(getenv "SSH_AUTH_SOCK")` in the daemon names `/tmp/neomacs-<uid>/agent.sock` (under the fixture's rendezvous override), which resolves to the relay's socket (R2-06) | `daemon_lifecycle` | **yes** |
  | `remote_relay_ctrl_x_ctrl_c_deletes_only_the_remote_frame`: the `nowait` branch (`gnu:lisp/server.el:1907-1916`) deletes F; the frontend exits 0; the daemon is alive | `daemon_lifecycle` | **yes** |
  | `neomacsclient_rejects_an_unsafe_socket_directory` (GNU parity) | `daemon_lifecycle` | **yes** (u7 §1.9) |
  | `relay_refuses_a_non_session_peer_with_a_readable_line` | `daemon_lifecycle` | **yes** |
  | `remote_fg_daemon_notifies_readiness`: `--fg-daemon` with `NOTIFY_SOCKET` set to a test datagram socket sends `READY=1` (UNATTESTED GNU parity) | `daemon_lifecycle` | **yes** |
  | `ssh_argv_carries_no_user_data`, `retry_argv_is_batch_mode_without_control_path`, `default_label_is_sanitized`, `relay_autostart_prefers_the_service_manager` (injected probe) | frontend and relay unit | n/a |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_relay_) | test(remote_fg_daemon_) | test(neomacsclient_rejects_) | test(relay_refuses_)'
  cargo nextest run -p neomacs-frontend -E 'test(~ssh_argv) | test(~retry_argv) | test(~default_label)'
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m11-dl.log  # ⟨compare⟩
  ```
- **Not runnable here:**
  - a real `sshd`: remote-shell parsing, rc files, ControlPersist and half-open TCP (none on this host;
    U12 for a fixture);
  - `systemd-run --user`, linger and `KillUserProcesses` (CI or VM; this host has no logind session);
  - the askpass prompt in a window (CI);
  - a native window (M11b).
- **Compile fan-out:** `neomacs` bins and `neomacs-frontend`: ≈1 min dev.

---

## M11b — GPU presenter over a link (first visible remote window)

- **Status:** todo
- **Kind:** new feature; CI-verified pixels, host-verified build and logic (D39, R2-21, R1 F6).
- **Goal.** `neomacs-frontend --presenter gpu` draws a remote session's frames in a native window, through
  today's `RenderApp`, before the in-process GUI converts. A public headless `RenderApp` exists for M13 and
  M14.
- **Depends on:** M4 (the runtime links no `neovm-*`), M6 (`FrontendSession`), M10 (fonts and pixels
  over the link).
- **Scope.**
  - `neomacs-frontend` feature `gpu`, with `src/bridge.rs`. This is a **temporary** frontend-side adapter
    that plays the evaluator's half of `ThreadComms` for an unmodified `RenderApp`:
    - window and UI state records → `RenderCommand`s;
    - sealed presentations → the frame mailbox;
    - `RenderComms` input → NDP input edges with serials and `DisplayTime`;
    - the shared-state couplings (S1–S15) owned inside the frontend process.

    M13 deletes it and adds a guard test asserting that it is gone.
  - `RenderApp::headless(…)`, public behind a `testing` feature. It spawns `FramePreparation` without
    winit; today `RenderApp`/`new` are `pub(super)` (`RT/state.rs:978,1202`), and the worker spawns only
    in the winit closure (`RT/bootstrap.rs:713-736`). M4 creates the `testing` feature and this entry,
    with only the input and command surface its moved backpressure tests use (D44). M11b adds
    `FramePreparation` to that same entry.
  - `neomacs-gui-tests` gains the first `remote_gpu_*` L5 tests (below).
  - CI: the macOS path filter covers `neomacs-frontend` (R2-22).
- **New tests.**

  | Test | Where | Runs here? |
  |---|---|---|
  | `headless_render_app_materializes_presentations` | runtime (`testing`) | **yes** |
  | `bridge_translates_a_golden_session_into_render_commands` (golden `RenderCommand` sequence for a recorded NDP session) | frontend (`gpu`) | **yes** |
  | `bridge_input_carries_serials_and_display_time` | frontend (`gpu`) | **yes** |
  | `remote_gpu_frontend_png_matches_inprocess`: a daemon plus a GPU frontend over `--connect`; PNG readback equals the in-process GUI's for the same scenario | `neomacs-gui-tests` | **no** (CI L5) |
  | `remote_gpu_asset_fonts_reach_the_atlas`, `remote_gpu_pending_glyphs_draw_exact_advances`, `remote_gpu_provisional_window_is_adopted` | `neomacs-gui-tests` | **no** (CI L5) |
- **Acceptance (run here).**

  ```sh
  cargo build -p neomacs-frontend --features gpu                   # compiles winit and wgpu; no GPU needed to build
  cargo nextest run -p neomacs-frontend --features gpu -E 'test(~bridge)'
  cargo nextest run -p neomacs-display-runtime --features testing --lib -E 'test(~headless_render_app)'
  cargo nextest run -p neomacs-display-runtime --lib $NX 2>&1 | tee $E/m11b-rt.log   # ⟨compare⟩
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-crate-graph.py    # frontend closure: no neovm-*
  ```
- **Not runnable here:** the four `remote_gpu_*` L5 tests (CI, U11).
- **Risks.** The bridge is the only deliberate adapter in the plan. Its deletion is an M13 acceptance
  item, so it cannot become permanent.

---

## M12 — Durable sessions: detach and resume below Lisp

- **Status:** todo
- **Kind:** new feature (Lisp-visible extension N1).
- **Goal.** Losing the transport detaches the terminal with the GNU-shaped table of DESIGN §12.2. The
  frontend resumes the same terminal and frames without running Lisp. Input is delivered exactly once,
  roaming takes over implicitly, and a restarted frontend resumes cold (DESIGN §6.19, D34).
- **Depends on:** M11, M11b (for the reconnect-band L5 test).
- **Scope.**
  - **Session.**
    - One agreed `keepalive_timeout`;
    - resume cursors and `received_through`;
    - the per-lane resume rules;
    - `LinkEmulator` behind the `testing` feature (seeded delay, jitter, bandwidth, stall without EOF,
      cut, heal, cut after the Nth record).
  - **Display host.**
    - *Detach* (at a safe point): `ConnectionState::Detached{since}`.
      - **Close the transport** (R2-04).
      - Synthesize `Retired` for in-flight presentations; keep the last activated one resolvable
        (R2-23).
      - Fail pending requests; skip the terminal in redisplay.
      - Set `frame-visible-p` nil, with the **desired** visibility kept separately (R2-23).
      - Treat the terminal as inactive for the sole-active rule.
      - **Move the selected frame off the terminal** (R2-09). The **KBOARD release** by
        `delete_kboard`'s rule is implemented here if M16 has landed; otherwise M16 adds it.
    - `x-create-frame` on a detached terminal **creates the frame** (R2-09).
    - Detach and resume hooks alternate (R2-23).
    - *Resume* in a listener task: token validation (current or previous), implicit takeover, cold
      resume on a new incarnation, transport rebind, a journal-based present resync or a keyframe by
      reference, keyed-state resync, and config replay.
    - Grace expiry → `delete_input_terminal_noelisp`.
    - Retained memory per detached session is reported in `neomacs-display-info` (R2-28).
  - **Lisp.** `neomacs-display-state`, `neomacs-display-detach-timeout` (default per U1),
    `neomacs-display-detach-functions`, `neomacs-display-resume-functions`.
  - **Frontend.**
    - The `Connected`/`Stalled`/`Reconnecting`/`Expired` states of DESIGN §6.19, including a parallel
      replacement transport.
    - The replay buffer: 5 s / 256 events, deduplicated by `received_through`, with a visible notice.
    - The incarnation value, tokens in the platform state directory, and re-pointing the agent link on
      every resume.
    - `--link-emulator SEED:PROFILE` in testing builds (R2-20).
  - **Headless frontend control.** `drop-transport`, `stall MS`, `cut-after-welcome`, `reconnect`,
    `restart` (a new process, the same token), `type-while-disconnected`.
  - Re-derive M7's `remote_display_loss_deletes_the_terminal_noelisp` with an explicit timeout of 0.
- **New tests** (`daemon_lifecycle` unless noted; all **red before**).

  | Test | Assertion |
  |---|---|
  | `remote_resume_detaches_on_transport_loss` | after `drop-transport`, within 12 s `(neomacs-display-state T)` → `detached`, `(frame-live-p F)` → `t`, `(frame-visible-p F)` → `nil`, the detach hook ran once, `(+ 20 22)` → `42` |
  | `remote_resume_detaches_on_silent_stall` | `stall 30000` (no EOF) → detached within `keepalive_timeout` + 2 s (R2-20) |
  | `remote_resume_closes_the_dead_transport` | after detach, the relay process has exited (R2-04) |
  | `remote_resume_restores_the_same_frames` | `reconnect` → `connected`, F is the same frame object (`eq` through its id), `(frame-visible-p F)` → `t`, the resume hook ran, the recorded text equals the submitted text |
  | `remote_resume_survives_lost_welcome` | `cut-after-welcome` → the next attempt with the previous token succeeds (R2-03) |
  | `remote_resume_stall_then_recover_keeps_input` | `stall 4000` with keys typed during it → each key reaches the buffer exactly once; no detach (R2-04) |
  | `remote_resume_roam_takes_over_implicitly` | a replacement transport resumes while the old one is stalled → no `LabelInUse`; the old link gets `Goodbye{TakenOver}` (R2-04) |
  | `remote_resume_cold_after_frontend_restart` | `restart` → the same frames; a new input stream; no input duplicated or replayed (R2-05) |
  | `remote_resume_dedupes_by_received_serial` | keys sent while a 3 s command runs, then a cut and a resume → each key exactly once (R2-05) |
  | `remote_resume_delivers_in_flight_input_exactly_once` | keys sent just before the drop appear once in the buffer |
  | `remote_resume_drops_input_typed_after_reconnecting` | keys typed after the transport is abandoned never reach the buffer |
  | `remote_resume_moves_selection_off_the_detached_terminal` | after detach, `(frame-terminal (selected-frame))` is not T (R2-09) |
  | `remote_resume_create_frame_while_detached` | `(make-frame '((display . "neo:t1")))` while detached succeeds; the frame appears after resume (R2-09) |
  | `remote_resume_visibility_set_while_detached_is_honoured` | `(make-frame-invisible F2)` while detached → F2 stays invisible after resume (R2-23) |
  | `remote_resume_hooks_alternate` | a quick detach and resume runs the hooks in order: detach, then resume (R2-23) |
  | `remote_resume_does_not_touch_server_el` | an open minibuffer survives resume (`(minibuffer-depth)` unchanged) |
  | `remote_resume_wrong_token_is_refused` and `remote_offer_on_held_label_needs_takeover` | `ResumeUnknown`; `LabelInUse{state}` unless `takeover` |
  | `remote_resume_grace_expiry_deletes_noelisp` | with a 1 s timeout the terminal is deleted and `delete-frame-functions` ran deferred |
  | `remote_delete_terminal_in_daemon_is_not_refused` | deleting the only connected `neo` terminal while another is detached succeeds, because the daemon's initial terminal is active per GNU's `TERMINAL_ACTIVE_P` (R1 F22; UNATTESTED) |
  | `remote_resume_through_the_relay` | killing the ssh shim mid-session → the frontend reconnects automatically and resumes; the agent link is re-pointed to the new relay's socket (R2-06) |
  | neovm-core `sole_active_terminal_rule_counts_detached_as_inactive` | in a registry without an initial terminal (the non-daemon model, form A19), `(delete-terminal A)` with B detached → `(error "Attempt to delete the sole active display terminal")`; with FORCE it deletes |
  | session unit `link_emulator_is_deterministic_and_never_reorders` | — |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-session --features testing
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_resume_) | test(remote_offer_) | test(remote_delete_terminal_) | test(remote_display_loss_)'
  cargo nextest run -p neovm-core --features dbus-vendored -E 'test(~sole_active_terminal)'
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m12-dl.log  # ⟨compare⟩
  ```
- **Not runnable here:**
  - logind, linger and `KillUserProcesses` (CI or VM);
  - a real sshd with half-open TCP below the application layer (U12);
  - `remote_gpu_reconnect_band_dims_and_recovers` in `neomacs-gui-tests` (CI L5).
- **GNU claims:** the detached state is a Neomacs extension. The display-loss divergence from the gtk3
  abort is recorded (DESIGN §12.3) and cannot be attested as matching (A9). Form A19 attests the
  sole-active rule's reachability.
- **Compile fan-out:** the session, display host, frontend and `neovm-eval` (state and hooks): ≈2 min dev
  plus the stack copies.

---

## M13 — Local GUI convergence: one protocol for every display; headless daemons

- **Status:** todo
- **Kind:** behaviour-preserving for the local GUI (transport-parity tested), plus the C95 fix.
- **Goal.** The in-process GUI speaks NDP/1 through `FrontendSession` over the in-process transport. No
  shared memory, reply `Sender`, `Instant` or `Drop`-carried meaning crosses the seam. Daemons never open
  a display in process; they spawn frontend processes for native display names (DESIGN D2, D21, D22).
- **Depends on:** M4, M8, M9, M10, M11b.
- **Scope** (one coupling per PR, P3 M3a–h):
  - S1/S2 frame-shader capability and acks → handshake facts and `FrameShaderAck`;
  - S3/S4 tooltips → causal stamps and evaluator tip state;
  - S5 opacity → keyed state;
  - S9 and R5 monitors → handshake facts;
  - S12 and I25 desktop observer → the frontend;
  - C27 clipboard → the request path;
  - T1/T2, the blocking 64-slot queue and the shared mailbox → keyed state plus credits;
  - C5, S13 and I24 adoption → `Create{adopt_provisional}`.

  Then:
  - delete `ThreadComms`, `EmacsComms` and `RenderComms` (`DR/thread_comm.rs:1049-1148`),
    `PrimaryWindowDisplayHost`, `FrameRef::Primary`, frame 0 and `WindowClose{0}`;
  - move `DR/tty_input` to the evaluator side. M4 kept it in the runtime because it publishes through
    `RenderComms` (D43). Its ConnectedComms owner entry (`host_effect_guard_test.rs:80-84`) moves with it;
  - **delete M11b's bridge**;
  - the runtime defines `FrontendEndpoint`;
  - `NEOMACS_DISPLAY_TRANSPORT=wire` runs the local GUI over a socketpair;
  - daemons are always headless: a native name spawns `/proc/self/exe --frontend=gpu --inherit-fd N
    --display NAME` on Linux, and the startup path on macOS (R2-16);
  - the startup display becomes a terminal when `window-system-initialization` first opens it (GNU
    order).
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | the 77 `thread_comm` invariants re-expressed as generic functions over the in-process and socketpair transports | runtime + frontend | rewritten from the new contract, never weakened |
  | `transport_parity_scripted_session`: a scripted session through `RenderApp::headless` (M11b) over the in-process transport and over `wire`; identical materialized `FrameGlyphBuffer`s by canonical encoding for every presentation | frontend | n/a |
  | `inprocess_framesync_copies_no_row_payload` (**gate**: no row payload is cloned or encoded in process; counted) | display host | n/a (R2-19) |
  | `inprocess_present_ops_and_bytes_per_typed_char_within_budget` (**gate**, deterministic counts against the M2 size budgets) | display host | n/a (R2-19) |
  | `redisplay_never_parks_on_a_stalled_in_process_frontend` | display host | **yes** (the 64-slot `send` blocks today, `NM/main.rs:1824-1836`) |
  | `bridge_module_is_gone` (guard: M11b's bridge no longer exists) | frontend | **yes** (it exists until M13) |
  | `spawned_frontend_uses_proc_self_exe` (injected spawner) | display host | n/a |
  | `daemon_with_display_env_starts_headless`: `DISPLAY=:0 neomacs -Q --fg-daemon=hd` reaches readiness and `(list (x-display-list) (length (frame-list)))` → `(nil 1)` | `daemon_lifecycle` | **yes on this host**: today it panics with exit 101 [RB 04] |
  | `local_display_spawn_unreachable_signals_gnu_error`: in a headless daemon `(condition-case e (make-frame-on-display ":987") (error e))` → `(error "Display :987 can’t be opened")`; no phantom frame; the child is reaped; `(+ 20 22)` → `42` | `daemon_lifecycle` | **yes** |
  | `inprocess_framesync_overhead` (p99 per presentation; reported here, gated in CI against a 200 µs budget with a tolerance) | frontend bench | n/a |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-display-runtime --lib $NX 2>&1 | tee $E/m13-rt.log   # ⟨compare⟩, no neovm-* in the build
  cargo nextest run -p neomacs-frontend --features gpu,testing -E 'test(~thread_comm) | test(~transport_parity) | test(~endpoint) | test(~bridge_module_is_gone)'
  cargo nextest run -p neomacs-display-host --features neovm-core/dbus-vendored
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m13-core.log    # ⟨compare⟩ (incl. host_effect_guard)
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m13-dl.log  # ⟨compare⟩
  cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-crate-graph.py
  cargo build -p neomacs --bins --features dbus-vendored && campaign:bin/neomacs-provision-bootstrap-image
  NEOMACS_SEAM_BASELINE=compare NEOMACS_SEAM_BASELINE_DIR=$B/seam \
    cargo nextest run -p neomacs-tui-tests --run-ignored only -E 'test(seam_baseline)'   # → 2 passed
  ```
- **Not runnable here:**
  - **the GUI L5 suite (CI, lavapipe plus Xvfb) must stay green.** This is the campaign's main
    regression risk;
  - the CI timing gates (p99 overhead, first-window time within a tolerance of the M0 CI baseline,
    R2-19);
  - macOS occlusion and `Activated` credits (R2 Q2).

  All of them require U11.
- **Compile fan-out:** runtime, frontend, display host and `neomacs`: ≈2 min dev per PR. Runtime tests
  no longer build a stack copy (after M4).

---

## M14 — Feel: pacing and presentation-only prediction

- **Status:** todo
- **Kind:** new feature (frontend presentation only; Lisp-invisible except `throw-on-input`).
- **Goal.** At 50–150 ms RTT, typing, deleting, motion within a row and scrolling stay at frame rate,
  exactly, under evaluator certification (DESIGN §13).
- **Depends on:** M13 (the GPU and headless `RenderApp` are driven by `FrontendSession`). It uses M1's
  frontier.
- **Scope.**
  - *Pacing:* Interactive and Ambient presentation classes; RTT-sized coverage depth; provisional resize.
  - *Policy:* `ND/neo/prediction.rs` generalizes `ND/neo/scroll.rs` into a `WindowInteractionPolicy` in
    the redisplay signature. Serials go on every user input, and the signature now keys on the full
    frontier snapshot; measure the extra presentations.
  - *Engine:* `RT/frame_compositor/input_echo.rs` (P3 §6.4); `PredictionDemand` by measured latency;
    `neomacs-remote-prediction`.
  - *Quit:* store-time `throw-on-input` in the reader's `QuitPolicy`.
  - *Feel report:* driven by `LinkEmulator` profiles (`local`, `lan`, `wan-20`, `wan-100`, `far-150`,
    `lossy-wifi`) as an ignored `daemon_lifecycle` case, `remote_feel_report`. It runs a headless
    frontend with `RenderApp::headless` and `--link-emulator`. It is not added to `neomacs-perf`, whose
    dependencies include `neomacs-melpa-test-support` (R1 F25).
  - *Measured workloads:* PDF/doc-view page turns, `text-scale` zoom and a 10 fps animation, in bytes
    (R2-18); child-frame resize-then-query popups such as posframe and `fit-frame-to-buffer` (R2-25).
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `input_echo_exactness_model` (proptest over interleavings of keys, frames, `Partial`, `Indeterminate` and cancellations: never shown by both frame and overlay; the screen equals the authoritative frame when idle) | runtime | n/a |
  | `minibuffer_typing_predicted_while_outer_command_partial` | runtime | n/a (needs M1) |
  | `prediction_policy_*` (Org-style remap excluded; read-only excluded; `electric-pair` openers excluded; prefix argument excluded; unknown hook → advisory) | neovm-core | n/a |
  | `remote_feel_echo_is_exact_under_wan_100` (daemon plus a headless frontend running `RenderApp::headless` behind `LinkEmulator`: typed characters appear before the authoritative presentation; the final state equals it; zero double-applied effects) | `daemon_lifecycle` | n/a |
  | `remote_feel_cg_reaches_lisp_under_a_compilation_flood` (gate: within RTT/2 + 20 ms of emulated time) | `daemon_lifecycle` | n/a |
  | `throw_on_input_is_set_when_input_is_stored` (UNATTESTED, `gnu:src/keyboard.c:3867-3871`) | neovm-core | **yes** if the read-time check is confirmed by the test |
- **Acceptance (run here).** Deterministic properties gate; latency is reported.

  ```sh
  cargo nextest run -p neomacs-display-runtime --lib -E 'test(~input_echo) | test(~provisional_resize)'
  cargo nextest run -p neovm-core --features dbus-vendored -E 'test(~prediction_policy) | test(~throw_on_input)'
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(remote_feel_)'
  NEOMACS_FEEL_REPORT=$E/m14-feel.json \
    cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored --run-ignored only -E 'test(remote_feel_report)'
  #   → the report is written: coverage %, mispredict %, K2P and K2A per profile, plus bytes per image
  #     workload and RTTs per child-frame popup. Report only, not a gate
  ```
- **Compile fan-out:** DP (policy types: ≈15 dependents), `neovm-display`, the layout engine and the
  runtime: ≈3 min dev plus the stack copies.

---

## M15 — neo-term on the evaluator

- **Status:** todo
- **Kind:** Lisp-visible (neo-term honours `default-directory` and `process-environment`).
- **Goal.** neo-term's shell runs where the files are, on every display (DESIGN §5.3 X1, §9).
- **Depends on:** M7.
- **Scope.**
  - The shell is a Lisp process with a PTY (`ProcessKind::Real`), and the VT state (rio-vt) lives in the
    evaluator process (display host). Cells become frame glyphs through layout.
  - `TerminalCommand`, the `Terminal*` input events and `SharedTerminals` (`DR/terminal/`) leave the
    protocol and the runtime.
  - **A DP change** (R1 F4): neo-term's vocabulary moved in M1 (`TerminalId`, `TerminalDisplayTarget`,
    `TerminalGridSize`, `TerminalFloatPlacement`) and `FrameGlyph::Terminal`
    (`DP/frame_glyphs.rs:380-388`) are removed, with the `neo-term` feature forwards that only served
    them.
  - The same-host `terminal-emulator` capability disappears.
- **New tests.**

  | Test | Where | Red before? |
  |---|---|---|
  | `neo_term_shell_runs_on_the_evaluator_host`: in a headless daemon with no frontend, `neomacs-terminal-create` adds the shell to `(process-list)`; `pwd` output read through `neomacs-terminal-get-text` equals `default-directory` | `daemon_lifecycle` | **yes** (the PTY lives on the render side today, `DR/terminal/view.rs:240-270`) |
  | `neo_term_honours_process_environment` | `daemon_lifecycle` | **yes** |
  | `remote_neo_term_renders_on_a_link_display` | `daemon_lifecycle` | **yes** |
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neomacs-display-host --features neovm-core/dbus-vendored -E 'test(~vt)'
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(neo_term_) | test(remote_neo_term_)'
  cargo nextest run -p neomacs-display-runtime --lib $NX 2>&1 | tee $E/m15-rt.log   # ⟨compare --moved⟩: passes without the terminal module
  cargo nextest run -p neomacs-display-protocol $NX 2>&1 | tee $E/m15-dp.log         # ⟨compare⟩
  ```
- **Compile fan-out:** DP (≈15 dependents), the runtime, the display host and evaluator process code:
  ≈2–3 min dev plus the stack copies.

---

## M16 — KBOARD completion (multi-client parity)

- **Status:** todo
- **Kind:** Lisp-visible, toward GNU. **It gates any claim of concurrent multi-terminal typing.**
- **Goal.** GNU's per-terminal input contract (C54–C66): 14 terminal-local variables, single-kboard
  locking, the 31.1 "Frame is locked" message, C-g isolation **in single-kboard state**, and KBOARD
  sharing within a session (C45).
- **Depends on:** M5.
- **Scope.**
  - `select_keyboard` and `forget_keyboard` iterate every keyboard forwarder
    (`crates/neovm-value/src/forward/mod.rs:974-990`). New KBOARDs get GNU's initial values.
  - `single_kboard` and `temporarily_switch_to_single_kboard`: funcall-interactively, recursive edit above
    level 0, minibuffer reads and tty menus.
  - ``Frame is locked; see `multiple-terminals-merge-keyboards' variable`` (`gnu:src/keyboard.c:2362-2386`)
    and `multiple-terminals-merge-keyboards` (`:14472-14479`).
  - Partial key sequence replay on a KBOARD switch; "Terminal %d is locked, cannot read from it".
  - `QuitPolicy`'s current KBOARD and single-kboard flag, shipped to the frontend in `QuitKeys`. A C-g
    from a non-current KBOARD fails to quit only in single-kboard state (`gnu:src/keyboard.c:3794`;
    R2-24).
  - Channels of one session share a KBOARD.
  - **Detached terminals** (U10, R2-09): if M12 has landed, M16 adds the release at detach, using GNU's
    `delete_kboard` rule (`gnu:src/keyboard.c:13174-13183`): `current_kboard` becomes the selected
    frame's, and `single_kboard` becomes false.
- **New tests** (two pty clients, template `daemon_lifecycle.rs:1828`; all **red before**):
  - `kboard_minibuffer_on_a_locks_b_with_the_gnu_message`;
  - `kboard_last_command_is_per_terminal`;
  - `kboard_merge_option_unlocks`;
  - `kboard_cg_from_a_non_current_terminal_does_not_quit_in_single_kboard_state`, scoped as R2-24
    requires;
  - `kboard_cg_from_another_terminal_quits_outside_single_kboard`;
  - `remote_detach_releases_the_single_kboard_lock` (when M12 has landed);
  - neovm-core unit tests per variable.
- **Acceptance (run here).**

  ```sh
  cargo nextest run -p neovm-core --features dbus-vendored $NX 2>&1 | tee $E/m16-core.log    # ⟨compare⟩ + new pass
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored -E 'test(kboard_) | test(remote_detach_releases_)'
  cargo nextest run -p neomacs --test daemon_lifecycle --features dbus-vendored $NX 2>&1 | tee $E/m16-dl.log  # ⟨compare⟩
  ```
- **GNU claims:** UNATTESTED; form A8 needs an oracle and two X servers (u8 Q11).
- **Compile fan-out:** `neovm-value` (≈19–25 s rustc, 11 dependents), `neovm-eval` and `neovm-core`:
  ≈3 min dev plus the stack copies.
