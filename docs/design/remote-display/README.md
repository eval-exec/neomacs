# Remote display: design, milestone ledger, decision log

Design for issue [#587](https://github.com/eval-exec/neomacs/issues/587), which asks for full remote editing
with a client/server split, LSP support and a near-native feel. Related: [#399](https://github.com/eval-exec/neomacs/issues/399)
and discussion [#380](https://github.com/eval-exec/neomacs/discussions/380). Status: **draft, under review**.

## In one paragraph

The design runs Lisp, buffers, files, processes, language servers, layout, shaping, font selection and image
decoding on the **evaluator host**. Every window-system display is a GNU terminal of type `neo`: the local
window, a second local display, or a laptop across ssh. Each such terminal is a display channel in a session
that speaks one versioned protocol, **NDP/1**. The protocol runs over an in-process transport for the local GUI,
a socketpair for a spawned local frontend, and ssh stdio for a remote one.

- **Downlink:** sealed post-layout presentations (`FrameDisplayState`), sent as row deltas.
- **Uplink:** the display input vocabulary, with serials and display time.
- **Frontend owns:** windows, GPU, compositor, animation, IME pre-edit, clipboard and input devices.
- **Disconnects:** a lost transport detaches the terminal, in the style of `suspend-tty`, and it resumes
  below Lisp.

## Files

| File | Contents |
|---|---|
| [`DESIGN.md`](DESIGN.md) | Architecture and protocol specification (revision 2): cut point, NDP/1 message families, codec, flow control, terminals, fonts and media, security, the GNU compatibility contract, the latency plan, testing, and the design horizon. §19 records how every red-team finding was disposed of. |
| [`MILESTONES.md`](MILESTONES.md) | The ordered milestone ledger (M0–M16 and M11b). Each entry gives scope, acceptance commands and status. M0–M4 are implemented on this branch. |
| [`DECISIONS.md`](DECISIONS.md) | Dated decision log, D1 onward, with the reason for each decision. |

## Reference conventions used in code comments and commit messages

- `DESIGN §6.9` refers to a section of [`DESIGN.md`](DESIGN.md).
- `D44` is a decision in [`DECISIONS.md`](DECISIONS.md) (D1–D40 are also summarized in DESIGN §0).
- `M7` and `M11b` are milestones in [`MILESTONES.md`](MILESTONES.md).
- `R1 F13` and `R2-31` are red-team review findings. Their disposition is in DESIGN §19.
- `U11` is an open maintainer decision, listed in DESIGN §17.
- `gnu:src/xterm.c:26907` is GNU Emacs source at the pinned oracle commit
  `a360712c9d272d950d8d8255ef74570f7e90b7d9` (`parity-reference.toml`).
- `campaign:` paths are the author's local tooling and verification logs, such as baselines and review logs.
  They are not part of the repository. The commands and results that matter are quoted in each milestone
  entry and in the pull request.

## Evidence grades

Every statement about GNU behaviour in these documents is a source reading at the pinned commit and is
**UNATTESTED**: no attested oracle binary was available where this was written.

"This host" in the documents means the Linux x86_64 development machine used for verification. It has no GPU
and no `libxkbcommon-x11`, so GUI pixel checks are stated as CI-only and never claimed from it.
