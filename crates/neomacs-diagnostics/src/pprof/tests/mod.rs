use crate::pprof::folded_to_pprof;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn pprof_embeds_names_and_sample_types() {
    let pb = folded_to_pprof("main;foo;bar 10\nmain;baz 5");
    assert!(!pb.is_empty());
    for name in [
        b"main".as_ref(),
        b"foo",
        b"bar",
        b"baz",
        b"samples",
        b"count",
    ] {
        assert!(
            contains(&pb, name),
            "pprof missing {:?}",
            std::str::from_utf8(name)
        );
    }
}

#[test]
fn pprof_empty_input_is_still_a_minimal_profile() {
    let pb = folded_to_pprof("   \n");
    assert!(!pb.is_empty());
    assert!(contains(&pb, b"samples"));
}

#[test]
fn pprof_string_table_has_empty_first_entry() {
    // pprof requires string_table[0] == "". It is emitted as field 6 (tag 0x32)
    // with length 0.
    let pb = folded_to_pprof("a 1");
    assert!(contains(&pb, &[0x32, 0x00]), "no empty string_table[0]");
}

/// Validation-only: when NEOMACS_PPROF_OUT is set, write a golden pprof file so
/// an external `go tool pprof` run can confirm the hand-rolled wire format.
/// A no-op in normal test runs.
#[test]
fn pprof_golden_for_go_validation() {
    if let Ok(path) = std::env::var("NEOMACS_PPROF_OUT") {
        let folded = "main;font_lock_fontify_region;re_search_forward 40\nmain;jit_lock_function 12\nmain;redisplay 8";
        std::fs::write(path, folded_to_pprof(folded)).unwrap();
    }
}
