# Upstream reports of the Wayland popup lifetime panic

Checked 2026-09-09. Scope: existing public upstream discussions, not filing a new issue or modifying dependencies. Sources were read through GitHub's issue, PR, comment, and commit APIs.

## Exact matching failure mechanism

**Yes: this was reported during review of winit's popup implementation.** On June 12, 2026, CryZe reported that closing popups could leave compositor updates which the event loop then processed after their window no longer existed. The result was an `Option::unwrap()` panic in `winit-wayland/src/event_loop/mod.rs:361:58`. This matches Neomacs's destroyed-window/compositor-update failure mechanism; the line number differs in the newer pinned source. [Original report, second numbered finding](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4692260604).

The report links a June 12 fork commit, `3d03cabc3ae8e6391a9697c0107bf53bd29a5fe9`, titled “Skip compositor updates for dead windows.” Its patch checks that both the window state and window request entry still exist before processing a compositor update, skipping the update otherwise. This is a proposed fix in the reporter's fork, not by itself evidence of an upstream fix. [Proposed patch](https://github.com/CryZe/winit/commit/3d03cabc3ae8e6391a9697c0107bf53bd29a5fe9).

PR #4543 was merged July 27, 2026 as `9674d8ceef6976326fe9583a81f2e684daac05d6`. The merge comment asked the tester to open issues for remaining bugs. Merging popup support does **not** establish that every issue mentioned during its review was resolved. [PR and merge status](https://github.com/rust-windowing/winit/pull/4543), [maintainer's merge comment](https://github.com/rust-windowing/winit/pull/4543#issuecomment-5092119066).

Local inspection of Neomacs's pinned `a98b2b2` checkout confirms that its compositor-update loop still unconditionally obtains the window with `windows.get(&window_id).unwrap()` in the scale-change branch, without the proposed live-window guard. This supports treating the upstream report as directly relevant to the observed panic. [Pinned source](https://github.com/rust-windowing/winit/blob/a98b2b2/winit-wayland/src/event_loop/mod.rs#L379-L390).

## Related design discussion in the same PR

The same June 12 report separately identified Wayland's child-before-parent popup destruction requirement. Maintainer kchibisov advocated parent ownership and a separate popup/surface API to encode restrictions; a follow-up explicitly advocated using the type system to make invalid Wayland usage less likely. These are directly relevant architectural discussions, but the destruction-order protocol error is a **different failure** from the missing-window panic. [Report](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4692260604), [ownership/API discussion](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4693907661), [type-system rationale](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4694946009).

On June 18, the author described implementing ordered destruction and using a weak window-state pointer, then discussed whether users should hold weak handles while the event loop owns popup state. This confirms ongoing discussion of lifetime modeling, but it is not a statement that compositor-update retirement was fixed. [Author's update](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4742209482), [maintainer's response](https://github.com/rust-windowing/winit/pull/4543#issuecomment-4743923050).

## Related but not duplicate reports

- [#415: events after window destruction](https://github.com/rust-windowing/winit/issues/415), opened March 4, 2018 and closed April 24, 2019: an old Windows event-dispatch lifetime race. Conceptually related, not the current Wayland backend failure.
- [#4169: make windows owned by Winit](https://github.com/rust-windowing/winit/issues/4169), opened March 17, 2025, currently open: broader ownership/lifecycle API proposal, primarily motivated by iOS and Android; not a report of this panic.
- [#2972: panic in `handle_scale_changed`](https://github.com/rust-windowing/winit/issues/2972), opened July 22, 2023 and closed August 4, 2023: a Web/Firefox reentrant borrow panic, **not** this Wayland bug despite its similar title.

Searches included upstream issues/PRs for Wayland with panic, destroyed windows, scale, queue destruction, and the phrase `compositor updates`; the exact report was found in PR comments rather than a separately titled bug issue. This search does not prove that no additional report or later fix exists.
