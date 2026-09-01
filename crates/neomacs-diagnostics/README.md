# neomacs-diagnostics

A localhost HTTP performance-introspection server for neomacs. Off by default;
enable by setting `NEOMACS_DIAGNOSTICS_PORT` before launching neomacs:

```sh
NEOMACS_DIAGNOSTICS_PORT=9099 neomacs
```

It binds `127.0.0.1` only. The Lisp VM stays fully synchronous — the server runs
on its own thread with a tokio runtime, and reaches the VM only through channels.

## Endpoints

`GET /` returns this list as JSON (self-describing for agents).

### Metrics (always available)
- `GET /metrics` — frame + GC/heap snapshot (JSON). Frame timing includes
  `frame_p50_us` / `frame_p95_us` / `frame_p99_us` (commit-to-present latency
  percentiles).
- `GET /live` — server-sent-events stream of the metrics snapshot (~1 Hz).

### Lisp CPU capture (needs a running interactive editor)
A capture profiles whatever the editor executes during the window. In batch /
headless there is no event loop, so these return `503`.

- `GET /profile/lisp.folded?secs=N` — Brendan-Gregg folded stacks (feed
  [speedscope](https://speedscope.app)). Returns an `X-Capture-Id` header.
- `GET /profile/lisp.svg?secs=N` — the same capture as an SVG flamegraph.
- `GET /profile/lisp.pprof?secs=N` — pprof protobuf: `go tool pprof -top`,
  `-tree`, `-peek`, `-traces`, or `-http=:0`. Names are embedded, so no binary
  or symbols are needed.
- `GET /profile/lisp/callers?fn=NAME&secs=N` — callers/callees of `NAME`.
- `GET /report?secs=N&top=K&sort=self|total` — ranked top-K hotspots (JSON), the
  agent-friendly digest. Returns an `X-Capture-Id` header.

### Diff (before/after)
- `GET /captures` — list stored captures (`id`, samples, age).
- `GET /diff?before=A&after=B&top=K` — functions ranked by absolute self% change.
  Workflow: capture, change code, capture, diff — "did my change help?"

### Native (Rust) CPU capture (works even in batch)
Samples native Rust stacks (GC, layout, render, bytecode dispatch) — the code
the Lisp poll-sampler can't attribute. SIGPROF-based; no conflict with the
cooperative Lisp profiler.

- `GET /profile/native.svg?secs=N` — native CPU flamegraph.
- `GET /profile/native.pprof?secs=N` — native CPU as pprof (`go tool pprof`).

## How the layers fit together

| Question | Endpoint / tool |
|---|---|
| Which **elisp** function is hot? | `/profile/lisp.*`, `/report` |
| Which **native** code is hot (GC/layout/render)? | `/profile/native.*`, or external `samply record neomacs` → [Firefox Profiler](https://profiler.firefox.com) |
| Native **heap** allocation? | build with jemalloc profiling, or external `heaptrack neomacs` / `dhat` |
| Frame jank? | `/metrics` `frame_p*_us` |
| Did my change help? | `/diff` |
| Deep CLI analysis? | any `.pprof` → `go tool pprof` |

The Lisp `.pprof`, native `.pprof`, and (if enabled) jemalloc heap profiles all
consume with the same `go tool pprof` — one CLI across all layers.
