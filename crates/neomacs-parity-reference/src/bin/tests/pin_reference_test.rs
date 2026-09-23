use super::*;

fn manifest() -> ReferenceManifest {
    ReferenceManifest {
        schema: "1".to_string(),
        emacs_version: "31.0.90".to_string(),
        mirror_commit: "a".repeat(40),
        build_time: "2026-06-10T02:39:56-04:00".to_string(),
        fingerprint: "b".repeat(64),
        executable_sha256: "c".repeat(64),
        executable_size: 10,
        pdmp_sha256: "d".repeat(64),
        pdmp_size: 20,
    }
}

#[test]
fn a_reason_is_required_and_may_not_be_blank() {
    let args = |values: &[&str]| values.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(Options::parse(args(&["--emacs", "/bin/emacs"])).is_err());
    assert!(Options::parse(args(&["--emacs", "/bin/emacs", "--reason", "   "])).is_err());
    assert!(Options::parse(args(&["--reason", "because"])).is_err());
    let parsed = Options::parse(args(&["--emacs", "/bin/emacs", "--reason", "because"]))
        .expect("a complete invocation parses");
    assert_eq!(parsed.reason.as_str(), "because");
    assert!(!parsed.dry_run);
}

#[test]
fn the_reason_requirement_lives_in_the_type_not_in_one_call_site() {
    // The point of the newtype: a caller added later cannot re-pin without
    // a reason, and cannot pass an empty one either, because there is no
    // way to build a `Reason` that is blank.
    for blank in ["", "   ", "\t\n", " \r\n "] {
        assert!(
            Reason::new(blank).is_none(),
            "a blank reason must be unrepresentable: {blank:?}"
        );
    }
    assert_eq!(
        Reason::new("  the mirror was rebuilt  ")
            .expect("a real reason")
            .as_str(),
        "the mirror was rebuilt",
        "surrounding whitespace is trimmed so the log entry reads cleanly"
    );
}

#[test]
fn rewriting_keeps_the_comments_and_records_the_reason() {
    let original = format!(
        "# a header\n#\n{LOG_HEADER}\n#   2026-01-01  an older entry\n\n{}",
        render_manifest_keys(&manifest())
    );
    let mut proposed = manifest();
    proposed.fingerprint = "e".repeat(64);
    let rewritten = rewrite(
        &original,
        &proposed,
        &Reason::new("the mirror was rebuilt for profiling").expect("reason"),
    )
    .expect("rewrite");

    assert!(rewritten.contains("# a header"), "comments must survive");
    assert!(
        rewritten.contains("#   2026-01-01  an older entry"),
        "the log is append-only in effect: earlier entries stay"
    );
    assert!(
        rewritten.contains("the mirror was rebuilt for profiling"),
        "the reason must be recorded: {rewritten}"
    );
    assert_eq!(
        parse_manifest(&rewritten).expect("the rewrite must parse"),
        proposed,
        "the rewritten pin must read back as what was intended"
    );
}

#[test]
fn a_manifest_without_a_log_is_refused_rather_than_silently_re_pinned() {
    let original = render_manifest_keys(&manifest());
    let error = rewrite(
        &original,
        &manifest(),
        &Reason::new("because").expect("reason"),
    )
    .expect_err("a pin with nowhere to record the change must not be written");
    assert!(error.contains("RE-BASELINING LOG"), "{error}");
}

#[test]
fn differences_names_every_changed_field_and_nothing_else() {
    let current = manifest();
    assert!(differences(&current, &current).is_empty());
    let mut proposed = current.clone();
    proposed.pdmp_size = 21;
    proposed.fingerprint = "f".repeat(64);
    let changed: Vec<&str> = differences(&current, &proposed)
        .into_iter()
        .map(|(field, _, _)| field)
        .collect();
    assert_eq!(changed, vec!["fingerprint", "pdmp_size"]);
}

#[test]
fn a_zone_offset_is_written_the_way_the_pin_records_it() {
    assert_eq!(
        insert_offset_colon("2026-06-10T02:39:56-0400"),
        "2026-06-10T02:39:56-04:00"
    );
    assert_eq!(
        insert_offset_colon("2026-06-10T02:39:56+0000"),
        "2026-06-10T02:39:56+00:00"
    );
    assert_eq!(insert_offset_colon("no offset"), "no offset");
}
