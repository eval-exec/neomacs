//! Guards the injected-time discipline.
//!
//! The render thread used to call `Instant::now()` in 98 places. Anything that
//! samples a visual now takes a `FrameSample`, anything that records an
//! observation takes an `EventTime`, and anything that schedules takes a
//! `FutureDeadline` — but nothing in the type system stops the next change from
//! reaching for the wall clock again, and a clock read is easy to add and hard
//! to notice in review.
//!
//! So this test enumerates every remaining raw clock read and demands a reason
//! for each. Adding one fails the build until it is listed here with a
//! justification, which is the point: the cost of a new clock read should be
//! writing down why it is not a frame sample.
//!
//! `.elapsed()` is checked alongside `Instant::now()` because it is the same
//! read wearing a different name, and it is the form that hid most of the
//! original sites.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Directories whose production sources must not read the wall clock.
const GUARDED_DIRS: [&str; 2] = ["src/render_thread", "src/core"];

/// Every sanctioned raw clock read, as `file` -> why it is allowed.
///
/// Keep the justifications specific. "It's fine" is not a justification; the
/// entry should say what the read measures and why a frame sample would be
/// wrong for it.
fn allowlist() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (
            "src/render_thread/state.rs",
            "ImageRenderState::wait_for_terminal is a Condvar::wait_timeout run on the \
             Lisp thread, not the render loop; FpsCounter::cpu_span_start seeds the \
             CPU work stopwatch.",
        ),
        (
            "src/render_thread/frame_state.rs",
            "begin_fps_cpu_span starts a stopwatch over CPU render work; a constant \
             frame sample would make it read zero.",
        ),
        (
            "src/render_thread/render_pass/mod.rs",
            "Closes the CPU render-work span begun by begin_fps_cpu_span.",
        ),
        (
            "src/render_thread/bootstrap.rs",
            "Runs before RenderApp exists: no coordinator, no tick, no frame.",
        ),
    ])
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Production `.rs` files under the guarded directories, relative to the crate
/// root, with `*_test.rs` and `tests.rs` excluded.
fn guarded_files() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = crate_root();
    let mut found = Vec::new();
    for dir in GUARDED_DIRS {
        walk(&root.join(dir), &mut found);
    }
    let mut rel: Vec<String> = found
        .iter()
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            !name.ends_with("_test.rs") && name != "tests.rs"
        })
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    rel.sort();
    rel
}

/// Lines that read a clock, ignoring comments and doc comments.
fn clock_reads(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
                && (line.contains("Instant::now()") || line.contains(".elapsed()"))
        })
        .map(|(i, line)| (i + 1, line.trim().to_string()))
        .collect()
}

#[test]
fn every_raw_clock_read_in_the_render_thread_is_justified() {
    let allowed = allowlist();
    let root = crate_root();
    let mut unjustified = Vec::new();

    for file in guarded_files() {
        let Ok(source) = std::fs::read_to_string(root.join(&file)) else {
            continue;
        };
        let reads = clock_reads(&source);
        if reads.is_empty() || allowed.contains_key(file.as_str()) {
            continue;
        }
        for (line, text) in reads {
            unjustified.push(format!("  {file}:{line}  {text}"));
        }
    }

    assert!(
        unjustified.is_empty(),
        "these read the wall clock but are not justified in time_discipline_test.rs.\n\
         Temporal code should take a FrameSample (visual sampling), an EventTime \
         (an observation) or a FutureDeadline (a wakeup). If this read genuinely \
         cannot be one of those, add its file to the allowlist with a reason.\n{}",
        unjustified.join("\n")
    );
}

#[test]
fn the_allowlist_has_no_stale_entries() {
    // An allowlist that outlives its reason is worse than none: it silently
    // re-permits the thing it was meant to bound.
    let root = crate_root();
    let mut stale = Vec::new();
    for (file, _) in allowlist() {
        let Ok(source) = std::fs::read_to_string(root.join(file)) else {
            stale.push(format!("  {file} (no longer exists)"));
            continue;
        };
        if clock_reads(&source).is_empty() {
            stale.push(format!("  {file} (no longer reads a clock)"));
        }
    }
    assert!(
        stale.is_empty(),
        "these allowlist entries are no longer needed and should be deleted:\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_guarded_directories_are_actually_being_scanned() {
    // Guards against the scan silently matching nothing — a path typo would
    // otherwise make both tests above pass vacuously forever.
    let files = guarded_files();
    assert!(
        files.len() > 20,
        "expected to scan the whole render thread, found only {}: {files:?}",
        files.len()
    );
    assert!(
        files.iter().any(|f| f.ends_with("frame_sched.rs")),
        "the scheduler should be in scope"
    );
    assert!(
        files.iter().all(|f| !f.ends_with("_test.rs")),
        "test files must be excluded"
    );
}
