# Issue #702 Long-Line Truncated Scroll Attribution and Baseline

Date: 2026-11-26
Issue: [#702](https://github.com/eval-exec/neomacs/issues/702)
Historical `main` (Section 2 treeroot 6-cell Magit matrix): `8471a2c0d179920675da4b43e04b80c1aca6fe47` (pre-77e)
Current `main` (Sections 3 & 5 attribution, LBR profiles, and permanent bench baseline): `9ca34ffd1c9450183825ab916cac313729bdf419` (post-77f; zero runtime code changes between `9ca34ffd1c` and this bench-only branch)
Oracle reference: GNU Emacs 31.1 (`a360712c9d272d950d8d8255ef74570f7e90b7d9`, `scripts/ci/check-gnu-oracle.sh` RC=0, profile `linux-gtk3`)

## 1. Executive Summary

Post-`cb8121c20b` (live overlay tree in redisplay), the original `#702` `O(N_commits)` overlay-rebuild regression is resolved:

- `C-n` (`next-line`) is completely flat `O(1)` across a 32× increase in commit history (`2,121 -> 71,459` overlays): `4.486 -> 4.426 -> 4.786 ms/key` (`1.00× -> 0.99× -> 1.07×`), tracking GNU Emacs 31.1's `3.193 -> 3.127 -> 3.467 ms/key` (`1.00× -> 0.98× -> 1.09×`) at a constant `1.38–1.42×` ratio.
- `C-v` (`scroll-up-command`) is flat from `2048 -> 16384` (`4.858 -> 4.916 ms/key`, `1.01×`, `1.45–1.48×` GNU), then steps to `15.563 ms/key` at `65536` (`3.20×` vs 2048; `3.02×` GNU's `5.152 ms/key`, down from `6.50×` in P1).
- Live overlay counters confirm `overlay_copies = 0`, `overlay_view_entries = 0`, `overlay_roots = 0`, and `overlay_rebuilds = 0` on all 6 cells.

Line-width analysis proves the `C-v 65536` jump is **line width and dense per-lane face runs in the traversed `--graph` band** (`lines 35729..44529`: mean `179.3` chars, `55% > 200` cols, max `319`), not commit/overlay count (`C-n 65536` in the same 71,459-overlay buffer stays flat at `1.38×` GNU).

Hardware LBR profiling on **`9ca34ffd1c`** (`cpu_core/cycles/u`, `cpu_core/cache-misses/u`, `cpu_core/instructions/u`) of `C-v 65536` alone (`25.98B` instructions / `30.06B` cycles / `110.99M` cache misses via `perf stat` over 200 keys) vs `C-v 16384` control (`7.79B` instructions / `9.95B` cycles / `26.51M` cache misses) attributes the `3.02×` gap to four concrete, measurable root causes:

1. **`RowsFontificationDemand::ensure` CoW clone & `IntervalTree` realloc (`31.95%` cycles, `57.15%` cache misses, `4.44%` instructions):** `Rc::make_mut` clones the multi-megabyte 71k-line `TextPropertyTable` (`13.08%` cycles / `26.84%` cache misses) and `IntervalTree::split_node_right_raw` reallocates it via `_rjem_je_large_ralloc` / `__memmove` (`16.00%` cycles / `27.69%` cache misses).
2. **Repeated full-window throwaway & retry on `truncate-lines = t` long lines (`~45–50%` of layout excess; drives `snapshots = 4` vs `2`, `overlay_live_records = 7,836` vs `1,900/frame`, `routed = 141` vs `36/frame` at line 40,000):** `WindowTextReadBudget::initial` caps the initial copy at `(rows + 2) * text_cols + 1 = 8,161` chars, which overflows mid-viewport on 46 truncated lines of 230–319 chars (`11–15 KB`), hitting `QueryHorizonExhausted`, discarding the window grid, and replaying from row 0.
3. **GNU visual bug + `O(margin * row_len)` quadratic loop in `install_text_window_right_edge_markers` (`5.35%` cycles, `15.26%` instructions; `0%` on `C-v 16384`):** `DisplayRowColumnCount::from_row` sums `LeftMargin + Text + RightMargin` (`169 + 30 = 199`) against the text-area `target_col = 169`, wrongly popping 30 visible text columns (leaving a 30-space gap before Magit's right margin, verified against GNU 31.1) while rescanning all ~200 glyphs + `char_width` on every `.pop()`.
4. **Secondary per-line layout overhead:** `plan_plain_row_classified` (`crates/neomacs-layout-engine/src/buffer_source/row_route/planning.rs:123-164`) runs a full 170-char `routed_line_scan` (`8.81%` cycles / `17.71%` instructions; `102,789` attempts / `79,815` `refuse_other` refusals) before rejecting col-0 margin-overlay hazards, and the unrouted pipeline calls `TtyGlyphlessCharDisplay::method_for` (`7.10%` instructions) per printable ASCII character.

---

## 2. Historical Treeroot 6-Cell Baseline (`8471a2c0d1` vs Attested GNU Emacs 31.1)

Measured on dedicated idle `TreeRoot` (`Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz`, Linux `7.2.2-1-cachyos`, `AC_ONLINE=1`, `powerprofilesctl=performance`, `scaling_governor=performance`, exclusive whole-host lock, interleaved GNU/Neomacs reps, fixed order `2048 -> 16384 -> 65536` with 5s cooldown between sizes). Retained strictly as the historical 6-cell Magit scaling citation; all LBR attribution (Section 3) and permanent bench baselines (Section 5) below were freshly built and measured on current `main` **`9ca34ffd1c`**.

- **Neomacs binary (historical Section 2 only):** `8471a2c0d179920675da4b43e04b80c1aca6fe47`, `cargo xtask fresh-build --release` built `2026-11-26T06:34:28Z`, SHA-256 `e9baac82a8a56d4eb38c9749f78376a1dff93c75fd6a3107c7e0598b79a440f5`, compiled `.elc` verified (`subr.elc`, `simple.elc`).
- **GNU Emacs oracle (`scripts/ci/check-gnu-oracle.sh` RC=0):**
  - `oracle profile: linux-gtk3`
  - `oracle features: CAIRO DBUS FREETYPE GIF GLIB GMP GNUTLS GSETTINGS HARFBUZZ JPEG LCMS2 LIBXML2 MODULES NOTIFY INOTIFY PDUMPER PNG RSVG SECCOMP THREADS TIFF TOOLKIT_SCROLL_BARS TREE_SITTER WEBP X11 XDBE XIM XINERAMA XINPUT2 XPM XRANDR GTK3 ZLIB`
  - `GNU Emacs 31.1` (pinned `emacs-31.1` commit `a360712c9d272d950d8d8255ef74570f7e90b7d9`, pinned `magit-20260724.2338`, `200×50` PTY `-nw -Q`, `hl-line-mode`, midpoint, `recenter 0`, 200 command-loop calls + `(redisplay t)`).

### Scaling Shape (relative to `n = 2048`, median `ms/key`, 3 reps)

| Cell | Lines | Overlays | GNU 31.1 (ms/key) | GNU vs 2048 | Neomacs `8471a2c0d1` (ms/key) | Neo vs 2048 | Neo/GNU | `overlay_live_records` (200 frames) | Copies / View / Roots / Rebuilds |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `C-n 2048` | 2,120 | 2,121 | 3.193 | 1.00× | 4.486 | 1.00× | 1.40× | 394,306 | 0 / 0 / 0 / 0 |
| `C-n 16384` | 18,044 | 18,045 | 3.127 | 0.98× | 4.426 | 0.99× | 1.42× | 384,330 | 0 / 0 / 0 / 0 |
| `C-n 65536` | 71,458 | 71,459 | 3.467 | 1.09× | 4.786 | 1.07× | 1.38× | 396,214 | 0 / 0 / 0 / 0 |
| `C-v 2048` | 2,120 | 2,121 | 3.342 | 1.00× | 4.858 | 1.00× | 1.45× | 372,164 | 0 / 0 / 0 / 0 |
| `C-v 16384` | 18,044 | 18,045 | 3.326 | 1.00× | 4.916 | 1.01× | 1.48× | 381,522 | 0 / 0 / 0 / 0 |
| `C-v 65536` | 71,458 | 71,459 | 5.152 | 1.54× | 15.563 | 3.20× | 3.02× | 961,030 | 0 / 0 / 0 / 0 |

Per-size loadavg (`1m 5m 15m`) and thermal telemetry (no throttling; package temp stayed `66–82°C`, spread `< 3%` across all 3 reps):

- `n = 2048`: start `0.26 1.65 2.43` (`66.0°C`, `2484 MHz`) -> end `0.55 1.58 2.38` (`74.0°C`, `2753 MHz`)
- `n = 16384`: start `0.51 1.56 2.37` (`66.0°C`, `2516 MHz`) -> end `0.75 1.49 2.31` (`82.0°C`, `4083 MHz`)
- `n = 65536`: start `0.69 1.46 2.30` (`68.0°C`, `2790 MHz`) -> end `1.00 1.30 2.12` (`78.0°C`, `2524 MHz`)

### Line-Width Evidence on the Traversed `emacs-31.1` Bands

Measuring `git log -n 65536 --graph --decorate --oneline` at `a360712c9d` over the exact line ranges traversed by the 200-key loops:

| Band | Lines | Mean chars/line | Lines `> 200` cols | Max line len |
|---|---|---:|---:|---:|
| `C-n 2048` (`1060..1260`) | 201 | 65.2 | 0 / 201 (0%) | <= 200 |
| `C-n 16384` (`9022..9222`) | 201 | 51.2 | 0 / 201 (0%) | <= 200 |
| `C-n 65536` (`35729..35929`) | 201 | 65.8 | 0 / 201 (0%) | <= 200 |
| `C-v 16384` (`9000..17800`) | 8,801 | 57.1 | 0 / 8,801 (0%) | <= 200 |
| `C-v 65536` (`35729..44529`) | 8,801 | 179.3 | 4,805 / 8,801 (54.6%) | 319 |

---

## 3. Per-Symbol Attribution on `9ca34ffd1c` (`C-v 65536` Alone vs `C-v 16384` Control)

### Provenance & Capture Commands (`provenance = local-UNTRUSTED` diagnostic profile)

`TreeRoot` does not have Linux `perf` installed, so hardware PMU/LBR attribution runs on the local `i9-14900K` host (`36 MiB` L3 vs `TreeRoot`'s `12 MiB` L3; absolute cycle/cache-miss counts reflect loaded local contention, while retired user-space instructions and per-symbol proportions identify the exact mechanisms). Every capture below used the freshly built **`9ca34ffd1c`** release binary (`cargo xtask fresh-build --release`, zero runtime diff from `9ca34ffd1c`, `subr.elc` verified):

- **Neomacs binary (`9ca34ffd1c` runtime):** SHA-256 `a9d86e7a3ba113416f42aa6c99b8424829e7738d2abefcd7fb4618171bb633be` (`778acd8e84d7ab1366472751b9f7efbb4993eab9`, bench-only commit on `9ca34ffd1c`, `lisp/subr.elc` loaded).
- **Host / governor / pinning / ambient loadavg:** `Intel(R) Core(TM) i9-14900K`, `scaling_governor = performance`, `taskset -c 0-15` (P-cores only so `cpu_core/*` counts 100% of user work), `perf_event_paranoid = 1`, ambient `/proc/loadavg` `37.84..43.66` (`2026-11-26T10:45:26Z..10:48:13Z`), `Total Lost Samples: 0` on all 4 LBR captures.
- **Exact commands (attached via READY/GO/DONE handshake strictly around the 200-key `scroll-up-command` loop, zero setup/teardown samples):**
  - Counter totals: `perf stat -p <pid> -e cpu_core/cycles/u,cpu_core/instructions/u,cpu_core/cache-misses/u`
  - Cycles LBR: `perf record -p <pid> -e cpu_core/cycles/u -c 2000003 --call-graph lbr`
  - Cache-misses LBR: `perf record -p <pid> -e cpu_core/cache-misses/u -c 20011 --call-graph lbr`
  - Instructions LBR (`65536` and `16384` control): `perf record -p <pid> -e cpu_core/instructions/u -c 4000003 --call-graph lbr`
  - Report extraction: `perf report --no-inline --stdio --percent-limit 0.5 --call-graph=none`

### Hardware Counter Totals (`9ca34ffd1c`, 200 `scroll-up-command` + `(redisplay t)` keys)

| Event (`perf stat`, user-space P-core) | `C-v 16384` (control) | `C-v 65536` (long-line band) | Ratio | Excess (`65536 - 16384`) |
|---|---:|---:|---:|---:|
| `cpu_core/instructions/u` | 7,788,434,047 (38.94M/key) | 25,980,648,005 (129.90M/key) | **3.34×** | +18.19B (+90.96M/key) |
| `cpu_core/cycles/u` | 9,948,485,889 (49.74M/key) | 30,061,421,081 (150.31M/key) | **3.02×** | +20.11B (+100.56M/key) |
| `cpu_core/cache-misses/u` | 26,511,556 (132.6k/key) | 110,989,721 (554.9k/key) | **4.19×** | +84.48M (+422.4k/key) |

### Top Symbols by Time / Cycles (`cpu_core/cycles/u`, `9ca34ffd1c`, `C-v 65536`, `-c 2000003`, `8K` samples / `16.69B` approx, 0 lost)

| Symbol | Inclusive % | Self % | Subsystem / Mechanism |
|---|---:|---:|---|
| `LayoutEngine::layout_window_rust` | 85.57% | 0.12% | Redisplay (`layout_frame_rust_with_face_services` `59.70%`) + synchronous `vertical-motion` (`~25.9%`) |
| `BufferSourceOutputSetup::render_body_attempt` | 50.95% | 0.08% | Viewport layout + horizon-exhaustion retries + right-edge markers |
| `RowsFontificationDemand::ensure` | 31.95% | 0.00% | `jit-lock` `put-text-property` (`29.39%`) triggering CoW clone & realloc |
| `TextPropertyTable::put_property_walk` -> `IntervalTree::split_node_right_raw` -> `_rjem_je_large_ralloc` | 16.00% | 0.01% | Multi-MB flat `IntervalTree` `Vec` reallocation (`__memmove` inside `do_rallocx` `18.88%`) |
| `Rc::make_mut` -> `<TextPropertyTable as Clone>::clone` | 13.08% | 10.81% | Full clone of 71k-line `TextPropertyTable` under shared `Rc` |
| `__memmove_avx_unaligned_erms` | 13.29% | 13.29% | Dominated by `IntervalTree` realloc + row/text copies |
| `BufferSourceLoopMutableState::render_next_source_item_for_context` | 20.78% | 0.17% | Unrouted per-run pipeline (`push_text_item`, `produce_step_into`, margin strings) |
| `plan_plain_row_classified` (`routed_line_scan`: `4.41%` incl / `1.38%` self) | 8.81% | 0.37% | Full-line classifier scan refused on col-0 margin overlay hazard |
| `BufferSourceRowAppendContext::render_display_item_source_to_text_row` | 7.49% | 0.04% | Routed plain-row text emission (`push_text_item`: `7.86%` incl / `3.42%` self) |
| `install_text_window_right_edge_markers` -> `trim_display_row_text_to_total_columns` | 5.35% | 0.02% | Quadratic pop loop + 30-col right-margin truncation bug (`from_glyphs`: `5.74%` incl / `4.23%` self) |
| `TtyRif::rasterize_presentations` (`2.18%`) + `TtyRif::diff_and_render` (`1.45%`) | 3.63% | 0.13% | TTY cell rasterization & diff |
| `neovm_value::emacs_char::char_width::char_width` | 2.03% | 2.03% | Width lookup (mostly inside `trim_display_row_text_to_total_columns -> from_glyphs`) |

### Top Symbols by Cache Misses (`cpu_core/cache-misses/u`, `9ca34ffd1c`, `C-v 65536`, `-c 20011`, `5K` samples / `110.58M` approx, 0 lost)

| Symbol | Inclusive % | Self % | Notes |
|---|---:|---:|---|
| `RowsFontificationDemand::ensure` -> `builtin_put_text_property_5` (`54.74%`) | 57.15% | 0.04% | Dominates LLC/DRAM traffic during scroll into 71k-line buffer |
| `IntervalTree::split_node_right_raw` -> `RawVec::grow_one` -> `_rjem_je_large_ralloc` | 27.69% | 0.02% | Multi-MB `Vec` realloc (`__memmove_avx_unaligned_erms`: `23.16%` self misses) |
| `Rc::make_mut` -> `<TextPropertyTable as Clone>::clone` | 26.84% | 21.99% | Deep clone of 71k-line `IntervalTree` |
| `BufferSourceOutputSetup::render_body_attempt` | 18.96% | 0.11% | Viewport layout + horizon-exhaustion retries (`render_visible_steps`: `11.78%`) |
| `TtyRif::rasterize_presentations` (`4.45%`) + `TtyRif::diff_and_render` (`1.03%`) | 5.48% | 0.05% | TTY grid updates |
| `drop_in_place<Vec<Glyph>>` | 2.66% | 2.64% | Dropping discarded retry / frame glyph rows |
| `install_text_window_right_edge_markers` (`from_glyphs`: `2.46%` incl / `2.19%` self) | 2.39% | 0.02% | Quadratic pop loop rescanning row glyphs |
| `GlyphRow::compute_hash` | 2.26% | 2.26% | Row hashing |
| `CollectionReads::unchanged` | 1.38% | 1.38% | Generation / read validation |

### Top Symbols by Retired Instructions (`cpu_core/instructions/u`, `9ca34ffd1c`, `C-v 65536`, `-c 4000003`, `6K` samples / `25.97B` approx vs `7.78B` on `C-v 16384`, 0 lost)

| Symbol | `C-v 65536` Inclusive % | `C-v 65536` Self % | `C-v 16384` control |
|---|---:|---:|---:|
| `BufferSourceOutputSetup::render_body_attempt` | 84.69% | 0.00% | 66.43% (5.17B vs 22.00B) |
| `render_next_source_item_for_context` | 30.37% | 0.23% | 31.62% (2.46B vs 7.89B) |
| `plan_plain_row_classified` (`routed_line_scan`: `11.98%` incl) | 17.71% | 1.16% | 13.98% (1.09B vs 4.60B) |
| `DisplayRowWriteMetrics::from_glyphs` | 16.79% | 11.89% | 1.34% incl / 0.93% self (0.10B vs 4.36B) |
| `install_text_window_right_edge_markers` (`trim_display_row_text_to_total_columns`: `13.74%`) | 15.26% | 0.03% | 0.00% (0.00B vs 3.96B) |
| `DisplayRowProgressWriter::push_text_item` | 12.60% | 6.44% | 9.61% incl / 3.96% self (0.75B vs 3.27B) |
| `BufferElementProducer::produce_step_into` | 11.24% | 0.12% | 9.36% (0.73B vs 2.92B) |
| `BufferOverlayStringTextRowRenderContext::render_produced_strings` | 8.35% | 0.00% | 12.44% (0.97B vs 2.17B) |
| `TtyGlyphlessCharDisplay::method_for` | 7.10% | 2.09% | 2.98% incl / 0.98% self (0.23B vs 1.84B) |
| `neovm_value::emacs_char::char_width::char_width` | 5.87% | 5.87% | 1.49% self (0.12B vs 1.52B) |
| `RowsFontificationDemand::ensure` | 4.44% | 0.00% | 5.96% (0.46B vs 1.15B) |

### Explicit Category Split (`9ca34ffd1c`, `C-v 65536`)

1. **Layout / wrapping / truncation (`render_body_attempt`):** `50.95%` cycles / `18.96%` cache misses / `84.69%` instructions.
   - **Horizon-exhaustion retries (`WindowTextReadBudget::initial`, `crates/neomacs-layout-engine/src/buffer_source/window_render.rs:62-76`):** On `truncate-lines = t` lines at line 40,000 (mean ~230 chars, 46 lines = 10.6–15 KB > initial 8,161-char budget `(46 + 2) * 170 + 1`), every frame hits `QueryHorizonExhausted`, discards the laid-out window grid (`drop_in_place<OutputWindowRowGrid>` `1.16%` cache misses), takes a new snapshot (`snapshots = 4` vs `2` at line 10,000), and replays from row 0 (`routed = 141` vs `36/frame`, `route_attempts = 778` vs `225/frame`).
   - **Right-edge truncation marker quadratic loop & GNU visual bug (`crates/neomacs-layout-engine/src/display_row/special_glyphs.rs:185,216,219`, `crates/neomacs-layout-engine/src/display_row/builder.rs:999-1035`):** `5.35%` cycles / `15.26%` instructions (`3.96B` instr; `0%` on `C-v 16384`).
   - **Plain-row route refusal storm (`crates/neomacs-layout-engine/src/buffer_source/row_route/planning.rs:123-164`):** `8.81%` cycles / `17.71%` instructions (`102,789` route attempts, `79,815` `refuse_other` refusals across 200 frames).
   - **Unrouted per-run text + margin string emission (`render_next_source_item_for_context` + `TtyGlyphlessCharDisplay::method_for`):** `20.78%` cycles / `30.37%` instructions.
2. **Fontification / `TextPropertyTable` CoW & realloc (`RowsFontificationDemand::ensure`):** `31.95%` cycles / `57.15%` cache misses / `4.44%` instructions — `Rc::make_mut` cloning (`13.08%` cycles / `26.84%` misses) + `IntervalTree::split_node_right_raw` reallocating (`16.00%` cycles / `27.69%` misses) the multi-megabyte 71k-line interval table.
3. **Glyph shaping:** `0%` HarfBuzz (TTY `-nw` mode); terminal cell width lookup is `char_width` (`2.03%` cycles / `5.87%` instructions, mostly inside `trim_display_row_text_to_total_columns -> from_glyphs`).
4. **Vertical motion (`scroll-up-command` / `vertical-motion`):** Calls `layout_window_rust` synchronously (`85.57%` total cycles in `layout_window_rust` minus `59.70%` inside `layout_frame_rust_with_face_services` = `~25.9%` cycles in synchronous `vertical-motion` layout queries, which also trigger `RowsFontificationDemand::ensure` and horizon retries).
5. **Overlay-record materialisation (`LiveOverlayView` / `OrderedShiftTree`):** `overlay_copies = 0`, `overlay_view_entries = 0`, `overlay_roots = 0`, `overlay_rebuilds = 0`. Direct live tree queries — whose inclusive call stacks overlap (`overlay_strings_at` `2.13%` and `overlay_faces_at` `2.05%` both call `OrderedShiftTree::for_each_match_in_node` `1.26%` and `LiveOverlayIndex::lock_intervals_if_fresh` `1.25%`) — account for `~4–5%` net cycles (`overlay_strings_at + overlay_faces_at` upper bound `4.18%`). The `2.52×` (`381,522 -> 961,030`) / `4.1×` (`1,900 -> 7,836/frame` at line 40,000) `overlay_live_records` jump is driven by the horizon-exhaustion full-window retries and ~80 colored `|` face runs per line, not an `O(N_overlays)` tree regression.
6. **TTY rasterization (`TtyRif`):** `3.63%` cycles (`rasterize_presentations` `2.18%` + `diff_and_render` `1.45%`) / `5.48%` cache misses.

---

## 4. Confirmed GNU Compatibility Visual Bug at Line 40,000

Captured side-by-side at `200×50` PTY on line 40,000 of `emacs-31.1` `magit-log -n65536 --graph --decorate`:

- **GNU Emacs 31.1 (`a360712c9d`):**
  ```text
  |c8d542fd593 | * | | | ... | | | | | | | | | | | |$Philipp Stephani    6 years|
  ```
  `$` is drawn flush at text column 169 right against the 30-column right margin (GNU `src/xdisp.c:26608-26650` operates strictly on `TEXT_AREA`).
- **Neomacs (`8471a2c0d1` / `9ca34ffd1c`):**
  ```text
  |c8d542fd593 | * | | | ... | |$                              Philipp Stephani    6 years|
  ```
  `DisplayRowColumnCount::from_row` (`crates/neomacs-layout-engine/src/display_row/builder.rs:999-1006`) sums `LeftMargin + Text + RightMargin` (`0 + 169 + 30 = 199 > 169`), causing `trim_display_row_text_to_total_columns` (`builder.rs:1021-1035`) to pop 30 real text columns and leave a 30-space blank gap before the right margin while rescanning the full row on every `.pop()`.

---

## 5. Permanent Benchmark Cell and Regression Gate (`9ca34ffd1c`)

To prevent silent regressions of the long-line band without depending on an external Git repository or MELPA cache:

- **Scenario:** `ScenarioId::ScrollingLongLines` (`scrolling-long-lines`), fixture `crates/neomacs-perf/fixtures/scrolling-long-lines.el`, `200×50` TUI, `truncate-lines = t`, `word-wrap = nil`, `hl-line-mode = 1`, 30-column right margin with one `before-string` `(display ((margin right-margin) ...))` overlay per line, and 2,400 deterministic lines built from a 20-line repeating vector with exact mean `179.30` chars (`3586 / 20`), `55.0%` (`11 / 20`) strictly `> 200` columns, max `319` chars, and dense alternating-face `| ` graph-lane prefixes. Both CPU time and `per-operation-wall-time` bracket strictly the cold + warm scroll phases (captured after buffer construction, initial `(redisplay t)`, `(garbage-collect)`, and sampling-gate `enable`). Kept outside `SuiteId::Standard` so it does not shift the standard board's geometric mean.
- **Scope boundary (`RowsFontificationDemand` gap explicitly bounded):** `scrolling-long-lines` runs in `fundamental-mode` with pre-propertized lines and `scrolling-long-lines-guard` gates on `MetricName::Instructions` (`CounterScope::EditLoop`). Therefore this cell isolates and guards the **layout / truncation / horizon-retry / right-edge-marker / overlay-route slice** (`84.69%` of instructions and `50.95%` of cycles on `C-v 65536`). It does **not** run `jit-lock` fontification during scrolling and does **not** guard the `RowsFontificationDemand::ensure` CoW-clone & `IntervalTree` reallocation hotspot (`57.15%` of cache misses / `31.95%` of cycles / `4.44%` of instructions on 71k-line Magit logs); that path must be verified against the 71k-line Magit matrix or a dedicated large-buffer lazy-fontification cache-miss check when Slice 3 lands.
- **Cohort high-load refusal & provenance stamping:** `scrolling-long-lines` and `scrolling-long-lines-guard` enforce the calibrated-cohort rule in `crates/neomacs-perf/src/host.rs`: `/proc/loadavg` (`1m 5m 15m`) is recorded in `HostProvenance`, `ComparisonArtifact`, and `SuiteArtifact`. When 1-minute loadavg is `>= 2.00` without `--allow-local`, `neomacs-perf` hard-stops with exit code `1` before creating any artifact directory or launching any editor (verified live on local host at `loadavg = 32.24 39.41 34.62` -> `RC=1`). Passing `--allow-local` permits execution and stamps `provenance = local-UNTRUSTED` on all emitted artifacts and CLI output so loaded local runs cannot be mistaken for calibrated idle-`TreeRoot` baselines. `SuiteId::ScrollingLongLinesGuard` also enables guard hardening (`resolve_guard_tools` canonical `perf`/`taskset` resolution with `PATH` shadowing rejection, canonical candidate resolution, narrowed TUI tool `PATH`, and stripping of all `NEOVM_*`/`NEOMACS_*` overrides).
- **Guard suite & threshold:** `SuiteId::ScrollingLongLinesGuard` (`scrolling-long-lines-guard`) gates `scrolling-long-lines` strictly on `MetricName::Instructions` (`CounterScope::EditLoop`, `--hardware-counters` required) with a **+2.0%** maximum regression budget (`0.001%` GNU / `0.008%` Neomacs spread and `+0.010%` self-vs-self delta even at `loadavg = 29.52..38.29`).

### Calibrated Wall/CPU Baseline on Dedicated Idle `TreeRoot` (`9ca34ffd1c` vs Attested GNU Emacs 31.1, `provenance = calibrated`)

Executed on dedicated idle `TreeRoot` (`Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz`, Linux `7.2.2-1-cachyos`, `AC_ONLINE=1`, `powerprofilesctl=performance`, `scaling_governor=performance`, exclusive whole-host flock across `.treeroot-lock.{gate,,2,3}`, `2026-11-26T11:21:39Z..11:23:05Z`, `loadavg_start = 1.24 3.16 4.56` (`1/665` runnable, `80.0°C`) -> `loadavg_end = 1.06 2.62 4.24` (`1/655` runnable, `87.0°C`)):

- **Neomacs binary (`9ca34ffd1c` runtime):** `cargo xtask fresh-build --release`, SHA-256 `6715f8f5922709c2bbca21268ac131fbcd4b55cac779a0d054c853e2dd2c2e28`, compiled `.elc` verified (`lisp/subr.elc`).
- **GNU Emacs oracle (`scripts/ci/check-gnu-oracle.sh` RC=0):** `oracle profile: linux-gtk3`, `GNU Emacs 31.1` (`a360712c9d272d950d8d8255ef74570f7e90b7d9`, `installed bindings.elc: 56106 bytes, 87 lazy docstring references`).
- **Command:** `neomacs-perf compare scrolling-long-lines --samples 3 --cpu 0 --require-governor performance` (no `--allow-local` needed; passed cohort load gate with `provenance = calibrated`, `verdict = valid`, `270` scroll ops = `54` cold + `216` warm, paired wall deltas `+171.07%, +174.02%, +173.86%`).

| Metric (`TreeRoot`, `provenance = calibrated`) | GNU Emacs 31.1 median (3 individual reps) | Neomacs `9ca34ffd1c` median (3 individual reps) | Neo / GNU | Spread (GNU / Neo) |
|---|---:|---:|---:|---:|
| `per-operation-wall-time` (µs/op, primary) | **24,360.9** (`[24607.1, 24360.9, 24304.4]`) | **66,702.2** (`[66702.2, 66752.9, 66559.9]`) | **2.738×** | **1.24% / 0.29%** |
| `per-operation-cpu-time` (µs/op) | **24,267.0** (`[24517.6, 24267.0, 24201.0]`) | **66,280.1** (`[66280.1, 66320.9, 66126.5]`) | **2.731×** | **1.30% / 0.29%** |
| `cold-scroll-phase-cpu-time` (µs, 54 ops) | 947,257 (`[960056, 947257, 944643]`) | 1,622,036 (`[1622036, 1629692, 1606021]`) | 1.712× | 1.63% / 1.46% |
| `warm-scroll-phase-cpu-time` (µs, 216 ops) | 5,604,841 (`[5659708, 5604841, 5589636]`) | 16,273,581 (`[16273581, 16276950, 16248123]`) | **2.903×** | 1.25% / 0.18% |
| `workload-wall-time` (µs) | 6,577,433 (`[6643909, 6577433, 6562199]`) | 18,009,587 (`[18009587, 18023279, 17971173]`) | 2.738× | 1.24% / 0.29% |
| `workload-cpu-time` (µs) | 6,552,098 (`[6619764, 6552098, 6534279]`) | 17,895,617 (`[17895617, 17906642, 17854144]`) | 2.731× | 1.30% / 0.29% |

Notice that on idle `TreeRoot` after moving `started-wall` strictly around the cold + warm scroll loops, `per-operation-wall-time` matches `per-operation-cpu-time` within `0.39%` (GNU) and `0.64%` (Neomacs) with `<= 1.30%` spread across all 3 reps (confirming zero setup leak into the primary metric).

### Hardware-Counter & Instruction-Guard Calibration on Local `i9-14900K` (`9ca34ffd1c` vs Attested GNU Emacs 31.1, `provenance = local-UNTRUSTED`)

Executed on **`9ca34ffd1c`** (`neomacs` SHA-256 `a9d86e7a3ba113416f42aa6c99b8424829e7738d2abefcd7fb4618171bb633be`, `lisp/subr.elc` loaded) vs attested local GNU Emacs 31.1 (`scripts/ci/check-gnu-oracle.sh` RC=0, `linux-gtk3`) with:
`neomacs-perf compare scrolling-long-lines --samples 3 --cpu 0 --require-governor performance --hardware-counters --counter-scope edit-loop --allow-local` (`provenance = local-UNTRUSTED`, `loadavg = 38.29 39.76 36.03`, `270` scroll ops = `54` cold + `216` warm, `verdict = valid`):

| Metric (`CounterScope::EditLoop`, `provenance = local-UNTRUSTED`) | GNU Emacs 31.1 median (3 individual reps) | Neomacs `9ca34ffd1c` median (3 individual reps) | Neo / GNU | Spread (GNU / Neo) |
|---|---:|---:|---:|---:|
| `instructions` (deterministic) | **66,105,604,720** (`[66105863650, 66105503295, 66105604720]`) | **137,527,230,409** (`[137521060105, 137527230409, 137532658384]`) | **2.080×** | **0.001% / 0.008%** |
| `cpu-cycles` (load-sensitive) | 40,562,419,116 (`[29048793710, 41750073037, 40562419116]`) | 126,812,675,480 (`[113386345920, 126812675480, 135828987937]`) | 3.126× | 31.3% / 17.7% |
| `cache-misses` (load-sensitive) | 40,532,810 (`[35134874, 43994648, 40532810]`) | 295,368,561 (`[260224981, 295368561, 295376861]`) | 7.287× | 21.9% / 11.9% |
| `per-operation-cpu-time` (µs/op) | 28,423.3 (`[20230.6, 29832.1, 28423.3]`) | 89,868.6 (`[79349.6, 89868.6, 96904.5]`) | 3.162× | 33.8% / 19.5% |
| `per-operation-wall-time` (µs/op) | 29,452.7 (`[20421.4, 34032.2, 29452.7]`) | 92,382.4 (`[81369.4, 92382.4, 100094.2]`) | 3.137× | 46.2% / 20.3% |

Self-vs-self guard stability check on **`9ca34ffd1c`** (`neomacs-perf suite scrolling-long-lines-guard --samples 3 --cpu 0 --require-governor performance --hardware-counters --counter-scope edit-loop --allow-local`):

- `provenance = local-UNTRUSTED`, `loadavg = 29.52 38.25 36.55`, `guard_tools = { perf: /nix/store/jj18bs67ix54xlcxqqw84cdqbdqp5qsr-perf-linux-7.3.5/bin/perf, taskset: /nix/store/kcihriicx768wp1g5mn06rnqjylidj46-util-linux-2.42.4-bin/bin/taskset }`
- `verdict = passed`, `gate_metric = instructions`, `percent_change = +0.010%` (190× below the `+2.0%` budget)
- Baseline 3 reps: `[137504302284, 137509977335, 137513126074]`
- Candidate 3 reps: `[137523046138, 137524382930, 137546421972]`

---

## 6. Runtime Fix Progression

Sections 1–5 above record the untouched pre-fix attribution (`8471a2c0d1` historical 6-cell matrix and `9ca34ffd1c` LBR / baseline). Each runtime fix slice below records its exact delta against its landing base.

### Slice 1 — Right-Edge Truncation Marker GNU Parity + `O(text_len)` Single-Pass Trim (on `da4a1d4e58`)

- **Behavioral fix (Section 4 resolved):** `DisplayRowColumnCount::from_text_area` and `trim_display_row_text_to_columns` (`crates/neomacs-layout-engine/src/display_row/builder.rs`, `crates/neomacs-layout-engine/src/display_row/special_glyphs.rs`) now measure strictly `GlyphArea::Text` (matching GNU Emacs 31.1 `src/xdisp.c:26608-26650`, which operates exclusively on `row->glyphs[TEXT_AREA]` / `row->used[TEXT_AREA]`), eliminating the blank gap before right display margins while keeping `$$` wide-char cut overwrite intact.
- **Complexity fix:** Replaced the `O(popped * row_len)` rescan-per-`.pop()` loop with a single `O(text_len)` pass plus an ASCII width fast path in `base_width_cols` (`crates/neomacs-layout-engine/src/composition.rs`).
- **Deterministic `EditLoop` `instructions` delta (`scrolling-long-lines-guard`, local `i9-14900K`, `provenance = local-UNTRUSTED`, `loadavg = 34.52 34.46 37.08`, `--samples 3 --cpu 0 --require-governor performance --hardware-counters --counter-scope edit-loop --allow-local`, both arms `portable_dump = loaded`):**
  - `verdict = passed`, `gate_metric = instructions`, **`percent_change = -3.798%`** (`-5,223,734,317` instructions, `0.9620×`, paired deltas `-3.811%, -3.784%, -3.796%`)
  - Baseline `da4a1d4e58` (`b62c36d7...`): **`137,532,973,298`** (`[137551418872, 137509757129, 137532973298]`, spread `0.030%`)
  - Candidate Slice 1 (`9fda678f...`): **`132,309,238,981`** (`[132309238981, 132306722713, 132312580895]`, spread `0.004%`)
  - Against attested GNU Emacs 31.1 (`compare scrolling-long-lines`, `provenance = local-UNTRUSTED`, `loadavg = 19.52 29.24 34.91`): GNU `66,112,992,103` (`spread 0.001%`) vs Slice 1 `132,322,028,476` (`spread 0.111%`), lowering Neo / GNU `instructions` from **`2.080×`** (`9ca34ffd1c`) to **`2.001×`** even while now rendering the 30 previously-truncated visible text columns per line.
  - Note: Local `cpu-cycles` / wall-time numbers are load-sensitive (`provenance = local-UNTRUSTED`); calibrated idle-`TreeRoot` wall/CPU re-baseline (`2.738×` in Section 5) is unchanged by this local-only measurement and will be re-measured on idle `TreeRoot` once the remaining layout/retry slices land.
