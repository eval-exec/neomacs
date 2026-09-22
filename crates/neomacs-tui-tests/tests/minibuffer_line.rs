#![cfg(unix)]
//! TUI regression for the `minibuffer-line' GNU ELPA package (issue #414).
//!
//! `minibuffer-line' displays status information in the minibuffer window
//! when no minibuffer is active.  Its whole mechanism rests on one engine
//! behaviour: the miniwindow displays the permanently-inactive minibuffer
//! buffer `" *Minibuf-0*"' (GNU: `init_minibuf_once' creates it at startup,
//! `minibuffer_unwind' points the miniwindow back at it after every
//! minibuffer exit), so writing formatted text into that buffer — which is
//! all `minibuffer-line--update' does — must become visible in the
//! miniwindow's grid.
//!
//! The package source is provisioned through `neomacs-infra`'s shared
//! content-addressed cache (`packages::source_file`), so both engines load
//! the same pinned bytes and no suite embeds its own copy.

use crate::support;
use neomacs_infra::packages;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

/// The package source, verbatim from GNU ELPA (minibuffer-line 0.1).
const MINIBUFFER_LINE_SOURCE: &str = include_str!("minibuffer_line_package.el");

/// Provision the package once and mount it into both sessions through the
/// shared cache path (`-l` with an activation file).
fn mount_minibuffer_line() -> neomacs_infra::packages::ProvisionedSourceFile {
    let provisioned = packages::source_file("minibuffer-line", MINIBUFFER_LINE_SOURCE)
        .expect("provision minibuffer-line source");
    provisioned
}

#[test]
fn minibuffer_line_mode_displays_the_format_in_the_miniwindow() {
    let provisioned = mount_minibuffer_line();
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", provisioned.path().display()));
    // Deterministic format: the package default embeds the hostname and
    // wall clock, which cannot match across machines or runs.  `(:eval …)`
    // is the construct class the default format uses.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(setq minibuffer-line-format '(\"\" (:eval (concat \"MBL-STATUS\" \"-MARKER\"))))",
    );
    eval_expression(&mut gnu, &mut neo, "(minibuffer-line-mode 1)");

    // `minibuffer-line-mode' calls `minibuffer-line--update' once at enable
    // time (no waiting for the 60s refresh timer), so the marker must appear
    // without any further input.
    let marker_ready = |grid: &[String]| grid.iter().any(|row| row.contains("MBL-STATUS-MARKER"));
    gnu.read_until(Duration::from_secs(6), marker_ready);
    neo.read_until(Duration::from_secs(8), marker_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_exact_display(
        "minibuffer_line_mode_displays_the_format_in_the_miniwindow",
        &gnu,
        &neo,
    );
}

#[test]
fn minibuffer_line_updates_on_its_refresh_timer() {
    let provisioned = mount_minibuffer_line();
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", provisioned.path().display()));
    // A 1s refresh interval with a counting marker proves the refresh timer
    // re-renders the miniwindow (erase + re-insert), not just the first draw.
    // The counter lives in a single well-formed `(:eval ...)' form.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (defvar mbl-tick 0) \
 (setq minibuffer-line-refresh-interval 1 \
 minibuffer-line-format '((:eval (concat \"TICK\" (number-to-string (setq mbl-tick (1+ mbl-tick))))))))",
    );
    eval_expression(&mut gnu, &mut neo, "(minibuffer-line-mode 1)");

    // The first update runs at enable time (TICK1); the refresh timer must
    // push the count upward without any user input.
    let tick_ready = |grid: &[String]| grid.iter().any(|row| row.contains("TICK1"));
    gnu.read_until(Duration::from_secs(6), tick_ready);
    neo.read_until(Duration::from_secs(8), tick_ready);
    let tick_bumped = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("TICK2") || row.contains("TICK3"))
    };
    gnu.read_until(Duration::from_secs(8), tick_bumped);
    neo.read_until(Duration::from_secs(10), tick_bumped);

    // Live counters make an exact cross-engine grid comparison racy, so
    // settle both engines into the same static state: stop the refresh
    // timer and force one final update with a fixed marker.  This still
    // exercises the same `minibuffer-line--update' re-render path the
    // timer used, and gives the parity assertion a deterministic screen.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (cancel-timer minibuffer-line--timer) \
 (setq minibuffer-line--timer nil \
 minibuffer-line-format '(\"MBL-SETTLED-MARKER\")) \
 (minibuffer-line--update))",
    );
    let settled_ready = |grid: &[String]| grid.iter().any(|row| row.contains("MBL-SETTLED-MARKER"));
    gnu.read_until(Duration::from_secs(6), settled_ready);
    neo.read_until(Duration::from_secs(8), settled_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_pair_exact_display("minibuffer_line_updates_on_its_refresh_timer", &gnu, &neo);
}
