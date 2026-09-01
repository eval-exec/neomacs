//! Tests for image `:file` path resolution.
//!
//! These lock in GNU's `image_find_image_fd` -> `openp` contract (the
//! resolution backing every image `:file`, relative or absolute) and the
//! single-decode [`ImageFileRequest`] classification that the evaluator
//! thread and the off-thread submission worker share.

use super::*;
use crate::emacs_core::fileio::expand_file_name;
use std::fs;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// image_find_image_file -- GNU image_find_image_fd / openp fidelity
// ---------------------------------------------------------------------------

/// Helper: build the search-path candidate the way `expand_file_name` does, so
/// expected paths match the resolver byte-for-byte regardless of platform.
fn expected(dir: &str, file: &str) -> String {
    expand_file_name(file, Some(dir))
}

#[test]
fn relative_file_found_in_first_search_dir() {
    // The #242 repro: a bare relative `:file` must be searched against
    // `data-directory/images` (here, a stand-in temp dir).
    let images = tempdir().unwrap();
    let splash = images.path().join("splash.svg");
    fs::write(&splash, b"<svg/>").unwrap();
    let dir = images.path().to_string_lossy().into_owned();

    let got = image_find_image_file("splash.svg", &[dir.clone()]);
    assert_eq!(got.as_deref(), Some(expected(&dir, "splash.svg").as_str()));
}

#[test]
fn relative_file_found_in_later_search_dir() {
    // openp tries each search-path element in order; the match may be in a
    // later directory (mirrors x-bitmap-file-path fallback).
    let a = tempdir().unwrap();
    let b = tempdir().unwrap();
    fs::write(b.path().join("icon.png"), b"\x89PNG").unwrap();
    let search_path = [
        a.path().to_string_lossy().into_owned(),
        b.path().to_string_lossy().into_owned(),
    ];

    let got = image_find_image_file("icon.png", &search_path);
    assert_eq!(
        got.as_deref(),
        Some(expected(&search_path[1], "icon.png").as_str())
    );
}

#[test]
fn absolute_file_returned_cleaned() {
    // An absolute `:file` overrides the search path; openp still cleans it.
    let images = tempdir().unwrap();
    let abs = images.path().join("abs.png");
    fs::write(&abs, b"\x89PNG").unwrap();
    let abs = abs.to_string_lossy().into_owned();
    let stray = tempdir().unwrap();
    let search_path = [stray.path().to_string_lossy().into_owned()];

    let got = image_find_image_file(&abs, &search_path);
    assert_eq!(got.as_deref(), Some(expand_file_name(&abs, None).as_str()));
}

#[test]
fn not_found_returns_none() {
    let images = tempdir().unwrap();
    let dir = images.path().to_string_lossy().into_owned();
    assert_eq!(image_find_image_file("nope.svg", &[dir]), None);
}

#[test]
fn directory_match_is_skipped() {
    // openp treats a matching directory as EISDIR and keeps searching; a bare
    // directory name must not be reported as a found image.
    let images = tempdir().unwrap();
    fs::create_dir(images.path().join("sub")).unwrap();
    let dir = images.path().to_string_lossy().into_owned();
    assert_eq!(image_find_image_file("sub", &[dir]), None);
}

#[test]
fn quoted_name_prefix_is_stripped() {
    // openp strips a leading "/:" (the quoted-file-name prefix) from the
    // expanded candidate before probing.
    let images = tempdir().unwrap();
    let real = images.path().join("splash.svg");
    fs::write(&real, b"<svg/>").unwrap();
    let quoted = format!("/:{}", real.to_string_lossy());

    let got = image_find_image_file(&quoted, &[]);
    assert_eq!(
        got.as_deref(),
        Some(expand_file_name(&quoted[2..], None).as_str())
    );
}

// ---------------------------------------------------------------------------
// ImageFileRequest -- single-decode classification
// ---------------------------------------------------------------------------

#[test]
fn classify_absolute_is_direct() {
    assert!(matches!(
        ImageFileRequest::classify("/a/b.png", None, Vec::new()),
        ImageFileRequest::Direct(s) if s == "/a/b.png"
    ));
}

#[test]
fn classify_tilde_slash_expands_inline_with_cached_home() {
    // `~/...` is resolvable without I/O once $HOME is cached: inline Direct.
    let req = ImageFileRequest::classify("~/Pictures/icon.png", Some("/home/u"), Vec::new());
    assert!(matches!(
        req,
        ImageFileRequest::Direct(ref s) if s == "/home/u/Pictures/icon.png"
    ));
    assert!(!req.needs_off_thread());
}

#[test]
fn classify_bare_tilde_expands_inline_with_cached_home() {
    let req = ImageFileRequest::classify("~", Some("/home/u"), Vec::new());
    assert!(matches!(req, ImageFileRequest::Direct(s) if s == "/home/u"));
}

#[test]
fn classify_named_user_deferred_off_thread() {
    // `~user/...` may consult NSS/LDAP; defer to the submission worker.
    let req = ImageFileRequest::classify("~some-user/x.png", Some("/home/u"), Vec::new());
    assert!(matches!(
        req,
        ImageFileRequest::ExpandHome(ref s) if s == "~some-user/x.png"
    ));
    assert!(req.needs_off_thread());
}

#[test]
fn classify_tilde_without_cached_home_is_deferred() {
    // No cached home: cannot expand inline, so defer (the worker uses $HOME).
    let req = ImageFileRequest::classify("~/x.png", None, Vec::new());
    assert!(matches!(
        req,
        ImageFileRequest::ExpandHome(ref s) if s == "~/x.png"
    ));
    assert!(req.needs_off_thread());
}

#[test]
fn classify_relative_searches_data_images_path() {
    // The bug: a bare relative name must be searched, not opened verbatim.
    let req = ImageFileRequest::classify(
        "splash.svg",
        Some("/home/u"),
        vec!["<data>/images".to_owned()],
    );
    match req {
        ImageFileRequest::Search {
            ref name,
            ref search_path,
        } => {
            assert_eq!(name, "splash.svg");
            assert_eq!(search_path, &vec!["<data>/images".to_owned()]);
        }
        other => panic!("expected Search, got {other:?}"),
    }
    assert!(req.needs_off_thread());
}

#[test]
fn cache_key_is_stable_per_variant() {
    // The catalog dedups entries on cache_key; it must be the expanded path for
    // inline-resolved Direct, and the raw name for deferred resolutions.
    let direct = ImageFileRequest::classify("~/p.png", Some("/h"), Vec::new());
    assert_eq!(direct.cache_key(), "/h/p.png");

    let abs = ImageFileRequest::classify("/a/b.png", None, Vec::new());
    assert_eq!(abs.cache_key(), "/a/b.png");

    let home = ImageFileRequest::classify("~u/p.png", Some("/h"), Vec::new());
    assert_eq!(home.cache_key(), "~u/p.png");

    let search = ImageFileRequest::classify("splash.svg", Some("/h"), vec!["/s".to_owned()]);
    assert_eq!(search.cache_key(), "splash.svg");
}

#[test]
fn resolve_direct_is_identity() {
    let req = ImageFileRequest::Direct("/abs.png".to_owned());
    assert_eq!(req.resolve().as_deref(), Some("/abs.png"));
}

#[test]
fn resolve_search_finds_relative_file() {
    let images = tempdir().unwrap();
    fs::write(images.path().join("splash.svg"), b"<svg/>").unwrap();
    let dir = images.path().to_string_lossy().into_owned();
    let req = ImageFileRequest::Search {
        name: "splash.svg".to_owned(),
        search_path: vec![dir.clone()],
    };
    assert_eq!(
        req.resolve().as_deref(),
        Some(expected(&dir, "splash.svg").as_str())
    );
}

#[test]
fn resolve_search_missing_returns_none() {
    let req = ImageFileRequest::Search {
        name: "nope.svg".to_owned(),
        search_path: vec!["/nonexistent-dir".to_owned()],
    };
    assert_eq!(req.resolve(), None);
}

#[test]
fn resolve_expand_home_uses_expand_file_name() {
    // The worker resolves deferred `~`-expansion lexically via expand-file-name.
    let req = ImageFileRequest::ExpandHome("~/x.png".to_owned());
    // Without $HOME override at the enum layer this mirrors the existing
    // worker: expand_file_name(name, None) against the process $HOME. Assert
    // the shape, not a specific $HOME value.
    let got = req
        .resolve()
        .expect("expand_file_name always yields a path");
    assert!(
        got.ends_with("/x.png"),
        "expected expanded home path ending in /x.png, got {got}"
    );
}
