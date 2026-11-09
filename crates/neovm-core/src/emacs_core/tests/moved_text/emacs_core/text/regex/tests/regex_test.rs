use super::*;
use std::mem;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use std::time::Instant;

use neovm_text::emacs_core::casefiddle::apply_replace_match_case;
use neovm_text::emacs_core::casefiddle::{
    apply_replace_match_case_lisp, apply_replace_match_case_lisp_with,
};
use neovm_text::emacs_core::regex_emacs::{
    self, BufferSyntaxLookup, CaseTranslation, CompiledPattern, DefaultSyntaxLookup,
    MatchRegisters, SyntaxCacheKey, SyntaxLookup,
};
use neovm_value::emacs_char::char_to_byte_pos;
use neovm_value::tagged::SymbolMode;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use neovm_edit::buffer::{Buffer, BufferId, BufferTextBackendKind, EmacsByteRange};
use neovm_eval::emacs_core::eval::Context;
use neovm_text::emacs_core::regex::replace_match_buffer;
use neovm_value::heap_types::byte_positions::EmacsBytePos;
use neovm_value::value::positions::CharRange;

use neovm_text::emacs_core::regex::*;

use neovm_value::heap_types::LispString;

use neovm_value::value::Value;

#[test]
fn replace_match_case_capitalizes_each_word_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(apply_match_case("[alice:5]", "Alice"), "[Alice:5]");
    assert_eq!(
        apply_match_case("h_hello w_world", "Hello World"),
        "H_Hello W_World"
    );
}

#[test]
fn compile_search_pattern_uses_backref_engine_for_supported_captures() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\([a-z]+\\)-\\([0-9]+\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_uses_backref_engine_for_noncapturing_groups() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?:foo\\|bar\\)+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_syntax_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(defun\\|defvar\\)\\s-+\\(\\w+\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_category_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[ \t]\\|\\c|.\\|.\\c|"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_digit_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\d+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_char_class_escapes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[\\w-]+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[\\s-]+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_lazy_quantifiers_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a.*?b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a\\{2,4\\}?b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_open_interval_quantifiers_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a\\{,2\\}b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_explicit_numbered_groups_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?1:[^}]*\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?9:.*?\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_symbol_boundaries_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\_<foo\\_>"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_bracket_section_anchor_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\`\\[\\([^]]+\\)\\]\\'"), true),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn posix_word_class_extends_via_buffer_syntax_table_override() {
    crate::test_utils::init_test_tracing();
    use neovm_text::emacs_core::syntax::{SyntaxClass, SyntaxEntry};

    let mut buf = make_test_buffer("foo_bar baz");
    // GNU-parity isolation: give this buffer its own copy of the
    // standard chartable so the mutation doesn't leak into other
    // buffers / tests.
    neovm_text::emacs_core::syntax::SyntaxTable::isolate_for_buffer(&mut buf)
        .modify_syntax_entry('_', SyntaxEntry::simple(SyntaxClass::Word));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));

    let mut md = None;
    let matched = looking_at(&buf, &lisp_pat("[[:word:]]+"), false, &mut md).expect("compile ok");
    assert!(matched, "[[:word:]]+ should match `foo_bar`");
    let md = md.unwrap();
    assert_eq!(
        match_group(md.group(0)),
        Some(MatchGroup::new(1, 8)),
        "match should cover the whole `foo_bar`"
    );

    // Without the override, `_` is Symbol (not Word) in the
    // standard syntax table, so the match stops at index 3.
    let mut buf2 = make_test_buffer("foo_bar baz");
    buf2.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let matched = looking_at(&buf2, &lisp_pat("[[:word:]]+"), false, &mut md).expect("compile ok");
    assert!(matched);
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(1, 4)),
        "without override, match stops at `_`"
    );
}

#[test]
fn string_match_multibyte_charset_range_matches_interior_character() {
    crate::test_utils::init_test_tracing();
    let pattern = lisp_pat("[À-Å]");
    let compiled = regex_emacs::regex_compile_lisp_with_translation(&pattern, false, None).unwrap();
    assert_eq!(compiled.multibyte_charsets.get(&0), Some(&vec![('À', 'Å')]));
    let folded = regex_emacs::regex_compile_lisp_with_translation(
        &pattern,
        false,
        Some(regex_emacs::CaseTranslation::standard()),
    )
    .unwrap();
    assert!(
        folded
            .multibyte_charsets
            .get(&0)
            .is_some_and(|ranges| ranges.contains(&('à', 'å'))),
        "case-folded range should include GNU's translated image"
    );
    assert!(
        compiled.fastmap[0xC3],
        "UTF-8 lead byte should be searchable"
    );
    let haystack = LispString::from_utf8("Ä");
    assert!(
        regex_emacs::re_match(
            &compiled,
            haystack.as_bytes(),
            0,
            haystack.byte_len(),
            &DefaultSyntaxLookup,
            STRING_MATCH_AT_DOT_UNREACHABLE,
        )
        .is_some(),
        "anchored charset match should accept the interior codepoint"
    );

    let mut md = None;
    let result = string_match_full("[À-Å]", "Ä", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 1)));
}

#[test]
fn re_search_forward_backreference_word_boundary() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("the the cat");
    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\b\\(\\w+\\) \\1\\b"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(7));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 8)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 4)));
}

#[test]
fn bounded_search_view_agrees_across_fragmented_backends() {
    crate::test_utils::init_test_tracing();
    let text = "aaé中\naa TAIL";
    let positions: Vec<_> = text
        .char_indices()
        .map(|(pos, _)| pos)
        .chain(std::iter::once(text.len()))
        .collect();
    for pattern in ["a", "a$", "a\\b", "\\(é中\\)", "^\\(a*\\)"] {
        for &start in &positions {
            for &bound in positions.iter().filter(|&&pos| pos >= start) {
                let search = |buf: &mut Buffer| {
                    buf.goto_emacs_byte_pos(EmacsBytePos::new(start));
                    let mut md = None;
                    let result = re_search_forward(
                        buf,
                        &lisp_pat(pattern),
                        Some(bound),
                        true,
                        false,
                        &mut md,
                    );
                    buffer_search_snapshot(result, buf, &md)
                };
                let expected = search(&mut make_test_buffer(text));
                for kind in implemented_text_backends() {
                    for &gap in &positions {
                        let mut buf = make_test_buffer_with_backend(text, kind);
                        buf.goto_emacs_byte_pos(EmacsBytePos::new(gap));
                        buf.insert("x");
                        buf.delete_emacs_byte_range(EmacsByteRange::from_usize(gap, gap + 1));
                        assert_eq!(
                            search(&mut buf),
                            expected,
                            "{kind:?}, {pattern:?}, gap={gap}, {start}..={bound}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn implemented_text_backends_match_literal_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = literal_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(literal_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_regex_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = regex_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(regex_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_looking_at_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = looking_at_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(looking_at_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_replace_match_after_regex_search() {
    crate::test_utils::init_test_tracing();
    let baseline = replace_match_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(replace_match_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_unibyte_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = unibyte_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(unibyte_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn search_forward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("world"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(11)); // end of "world"
    assert_eq!(buf.point_char_pos().get(), 0);
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(7, 12)));
}

#[test]
fn search_forward_not_found_noerror() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("xyz"), None, true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
    assert_eq!(buf.point_char_pos().get(), 0); // point unchanged
}

#[test]
fn search_forward_not_found_error() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("xyz"), None, false, false, &mut md);
    assert!(result.is_err());
}

#[test]
fn search_forward_case_fold_true() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("a"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
}

#[test]
fn search_forward_case_fold_true_unicode_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("Äx");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("ä"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some('Ä'.len_utf8()));
}

#[test]
fn search_forward_case_fold_true_ascii_literal_in_non_ascii_buffer() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("α GENERATED-AUTOLOAD-FILE");
    let mut md = None;
    let result = search_forward(
        &mut buf,
        &lisp_pat("generated-autoload-file"),
        None,
        false,
        true,
        &mut md,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some("α GENERATED-AUTOLOAD-FILE".len()));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(
            "α ".len(),
            "α GENERATED-AUTOLOAD-FILE".len(),
        ))
    );
}

#[test]
fn search_forward_case_fold_true_ascii_literal_does_not_unicode_fold_kelvin() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("xxKelvin");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("kelvin"), None, true, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
    assert!(md.is_none());
}

#[test]
fn re_search_forward_trivial_regexp_follows_literal_case_fold_path() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A.b");
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("a\\."), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(2));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 3)));
}

#[test]
fn search_forward_with_bound() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    // Search only within first 5 bytes — "world" starts at 6 so should not be found
    let result = search_forward(&mut buf, &lisp_pat("world"), Some(5), true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[test]
fn search_forward_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("aaa bbb aaa");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(4)); // after "aaa "
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("aaa"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(11)); // second "aaa" at end
}

#[test]
fn search_backward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11)); // end of buffer
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("hello"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0)); // beginning of "hello"
    assert_eq!(buf.point_char_pos().get(), 11);
}

#[test]
fn search_backward_not_found() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11));
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("xyz"), None, true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[test]
fn search_backward_finds_last_occurrence() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("aaa bbb aaa");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11)); // end
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("aaa"), None, false, false, &mut md);
    assert!(result.is_ok());
    // Should find the LAST "aaa" (at position 8)
    assert_eq!(result.unwrap(), Some(8));
    assert_eq!(buf.point_char_pos().get(), 11);
}

#[test]
fn search_backward_case_fold_true_unicode_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("Ää");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new("Ää".len()));
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("ä"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some('Ä'.len_utf8()));
    assert_eq!(buf.point_emacs_byte_pos().get(), "Ää".len());
    assert_eq!(buf.point_char_pos().get(), 2);
}

#[test]
fn re_search_forward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("foo 123 bar");
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("[0-9]+"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(7)); // end of "123"
    assert_eq!(buf.point_char_pos().get(), 0);
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(5, 8)));
}

#[test]
fn re_search_forward_with_groups() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("name: John");
    let mut md = None;
    // Emacs regex: \(\w+\): \(\w+\)
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\(\\w+\\): \\(\\w+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(result.is_ok());
    let md = md.unwrap();
    assert_eq!(md.group_count(), GNU_SEARCH_REGS_BASE_CAPACITY);
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 5))); // "name"
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(7, 11))); // "John"
}

#[test]
fn re_search_forward_multiline_anchor_respects_real_line_start() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("alpha=1\nbeta=2\ngamma=3\n");
    let mut md = None;

    let first = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^=]+\\)=\\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    )
    .expect("first search should succeed");
    assert_eq!(first, Some("alpha=1".len()));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(first.unwrap()));
    let first_md = md.as_ref().expect("match data for first search");
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            first_md.group(1).unwrap(),
        )),
        "alpha"
    );

    let second = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^=]+\\)=\\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    )
    .expect("second search should succeed");
    assert_eq!(second, Some("alpha=1\nbeta=2".len()));
    let second_md = md.as_ref().expect("match data for second search");
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            second_md.group(1).unwrap(),
        )),
        "beta"
    );
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            second_md.group(2).unwrap(),
        )),
        "2"
    );
}

#[test]
fn re_search_forward_bound_is_not_artificial_line_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("* TODO Alpha\n:LOGBOOK:\nCLOCK: \n:END:\n* TODO Beta\n");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(30));
    let mut md = None;

    // GNU `search_buffer_re` passes the full visible buffer to
    // `re_search_2` and passes the search bound separately as STOP.
    // Therefore `$` does not match merely because the bound is just after
    // point; it only matches before a real newline or at the real string end.
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("^[ \t]*$"),
        Some(31),
        true,
        false,
        &mut md,
    );

    assert_eq!(result, Ok(None));
    assert_eq!(buf.point_emacs_byte_pos().get(), 30);
    assert!(md.is_none());
}

#[test]
fn re_search_backward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc 123 def 456");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(15)); // end
    let mut md = None;
    let result = re_search_backward(&mut buf, &lisp_pat("[0-9]+"), None, false, false, &mut md);
    assert!(result.is_ok());
    // GNU re-search-backward scans positions backward and matches at the
    // first position where the regex succeeds.  From point-max (15/0-indexed=14),
    // position 14 is '6' which matches [0-9]+.  So match-beginning is 14.
    assert_eq!(result.unwrap(), Some(14));
    assert_eq!(buf.point_char_pos().get(), 15);
}

#[test]
fn re_search_backward_rejects_match_extending_past_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("ab12cd");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(2)); // point at the start of the current match
    let mut md = None;
    let result = re_search_backward(&mut buf, &lisp_pat("[0-9]+"), Some(0), true, false, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
    assert_eq!(buf.point_char_pos().get(), 2);
}

#[test]
fn re_search_backward_finds_nullable_match_at_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc\n");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3)); // point before trailing newline
    let mut md = None;
    let result = re_search_backward(
        &mut buf,
        &lisp_pat("\\(?:$\\)\\="),
        Some(0),
        true,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(3)));
    assert_eq!(buf.point_char_pos().get(), 3);
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(4, 4)));
}

#[test]
fn re_search_backward_word_begin_respects_search_origin_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(progn (erase-buffer) (insert \"one two three\") (goto-char 9))")
        .expect("prepare public Lisp search fixture");

    let word_begin = eval
        .eval_str("(re-search-backward \" \\\\<\" nil t)")
        .expect("search for a word beginning");
    assert_eq!(word_begin.as_int(), Some(4));

    eval.eval_str("(goto-char 9)")
        .expect("restore search origin");
    let word_boundary = eval
        .eval_str("(re-search-backward \" \\\\b\" nil t)")
        .expect("search for a word boundary");
    assert_eq!(word_boundary.as_int(), Some(8));

    eval.eval_str("(goto-char 9)")
        .expect("restore search origin");
    let symbol_begin = eval
        .eval_str("(re-search-backward \" \\\\_<\" nil t)")
        .expect("search for a symbol beginning");
    assert_eq!(symbol_begin.as_int(), Some(4));
}

#[test]
fn re_search_backward_log_line_loop_progresses() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer(
        "[09:01:00] INFO: Server started\n[09:02:15] INFO: Connection from 10.0.0.1\n[09:03:30] WARN: High memory usage detected",
    );
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let mut md = None;
    let pattern = "\\[\\([0-9:]+\\)\\] \\(INFO\\|WARN\\|ERROR\\): \\(.*\\)$";
    let mut positions = Vec::new();

    for _ in 0..4 {
        let Some(pos) =
            re_search_backward(&mut buf, &lisp_pat(pattern), None, true, false, &mut md).unwrap()
        else {
            break;
        };
        positions.push((pos, md.as_ref().and_then(|data| match_group(data.group(0)))));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(pos));
    }

    assert_eq!(
        positions,
        vec![
            (74, Some(MatchGroup::new(75, 118))),
            (32, Some(MatchGroup::new(33, 74))),
            (0, Some(MatchGroup::new(1, 32)))
        ]
    );
}

#[test]
fn re_search_forward_finds_nullable_match_at_buffer_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3));
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("\\="), None, true, false, &mut md);
    assert_eq!(result, Ok(Some(3)));
    assert_eq!(buf.point_char_pos().get(), 3);
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(4, 4)));
}

#[test]
fn looking_at_matches() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("hello"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
    assert!(md.is_some());
}

#[test]
fn looking_at_no_match() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("world"), true, &mut md);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn looking_at_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(6)); // "world"
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("world"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn looking_at_defaults_to_case_fold() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("a"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn looking_at_respects_case_fold_false() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("a"), false, &mut md);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn looking_at_case_fold_preserves_unibyte_raw_bytes_in_multibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let magic = LispString::from_unibyte(vec![0xed, 0xab, 0xee, 0xdb, 0x03, 0x00]);
    let mut buf = make_test_buffer("");
    buf.insert_lisp_string(&magic);
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::ZERO);

    let mut md = None;
    assert_eq!(looking_at(&buf, &magic, true, &mut md), Ok(true));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(1, 7))
    );
}

#[test]
fn looking_at_with_groups() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("foo123bar");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    // Emacs: \(\w+\)\([0-9]+\)
    let result = looking_at(&buf, &lisp_pat("\\(\\w+\\)\\([0-9]+\\)"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
    let md = md.unwrap();
    // \w+ is greedy, matches "foo123bar" leaving nothing for [0-9]+
    // Actually \w includes digits, so \w+ matches everything
    // Let's check what actually happens
    assert!(md.group(0).is_some());
}

#[test]
fn looking_at_character_class_backslash_range_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let buf = make_test_buffer("/");
    let result = looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md);
    assert_eq!(result, Ok(true));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 2)));

    let mut md = None;
    let buf = make_test_buffer("*");
    assert_eq!(
        looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md),
        Ok(false)
    );

    let mut md = None;
    let buf = make_test_buffer("-");
    assert_eq!(
        looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md),
        Ok(false)
    );
}

#[test]
fn replace_match_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let _ = re_search_forward(&mut buf, &lisp_pat("world"), None, false, false, &mut md);
    let result = replace_match_buffer(&mut buf, "rust", false, true, 0, &md);
    assert!(result.is_ok());
    let content = buf.buffer_substring_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        0,
        buf.total_emacs_byte_len().get(),
    ));
    assert_eq!(content, "hello rust");
}

#[test]
fn replace_match_with_backref() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    // Match "hello" with a group
    let _ = re_search_forward(
        &mut buf,
        &lisp_pat("\\(hello\\)"),
        None,
        false,
        false,
        &mut md,
    );
    let result = replace_match_buffer(&mut buf, "\\1 there", false, false, 0, &md);
    assert!(result.is_ok());
    let content = buf.buffer_substring_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        0,
        buf.total_emacs_byte_len().get(),
    ));
    assert_eq!(content, "hello there world");
}

#[test]
fn replace_match_buffer_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("raw"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&neovm_value::heap_types::LispString::from_unibyte(vec![
        0xFF,
    ]));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));

    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("."), None, false, false, &mut md);
    assert_eq!(result, Ok(Some(1)));

    let result = replace_match_buffer(&mut buf, "\\&", false, false, 0, &md);
    assert!(result.is_ok());

    let content = buf.buffer_substring_lisp_string_range(buf.full_emacs_byte_range());
    assert!(!content.is_multibyte());
    assert_eq!(content.as_bytes(), &[0xFF]);
    assert_eq!(buf.total_emacs_byte_len().get(), 1);
}

#[test]
fn replace_match_applies_case_pattern() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("FOO", "FOO", 0, &mut md);
    let replaced = replace_match_string_str("FOO", "bar", false, false, 0, &md).unwrap();
    assert_eq!(replaced, b"BAR");

    let _ = string_match_full("Foo", "Foo", 0, &mut md);
    let replaced = replace_match_string_str("Foo", "bar", false, false, 0, &md).unwrap();
    assert_eq!(replaced, b"Bar");
}

#[test]
fn replace_match_subexp_replaces_requested_group() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\([a-z]+\\)\\([0-9]+\\)", "abc123", 0, &mut md);
    let replaced = replace_match_string_str("abc123", "X", false, false, 2, &md).unwrap();
    assert_eq!(replaced, b"abcX");
}

#[test]
fn replace_match_subexp_errors_when_missing() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\([a-z]+\\)?\\([0-9]+\\)", "123", 0, &mut md);
    let err = replace_match_string_str("123", "X", false, false, 1, &md).unwrap_err();
    assert_eq!(err, REPLACE_MATCH_SUBEXP_MISSING);
}

#[test]
fn replace_match_preserves_multibyte_replacement_literals() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("x", "x", 0, &mut md);
    let replaced = replace_match_string_str("x", "éz", false, false, 0, &md).unwrap();
    assert_eq!(replaced, "éz".as_bytes());
}

#[test]
fn replace_match_preserves_multibyte_replacement_with_backref() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\(x\\)", "x", 0, &mut md);
    let replaced = replace_match_string_str("x", "\\1é", false, false, 0, &md).unwrap();
    assert_eq!(replaced, "xé".as_bytes());
}

#[test]
fn replace_match_rejects_backslash_zero_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);
    let err = replace_match_string_str("foo", "\\0", false, false, 0, &md)
        .expect_err("\\0 must be rejected by replace-match");
    assert_eq!(err, "Invalid use of `\\' in replacement text");
}

#[test]
fn replace_match_rejects_unknown_backslash_escape_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);

    // `\n` must error, not emit literal `\n`.
    let err = replace_match_string_str("foo", "a\\nb", false, false, 0, &md)
        .expect_err("\\n in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");

    // An arbitrary ASCII letter must error too.
    let err = replace_match_string_str("foo", "\\x", false, false, 0, &md)
        .expect_err("\\x in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");

    // A non-ASCII character must error too.
    let err = replace_match_string_str("foo", "\\é", false, false, 0, &md)
        .expect_err("\\<non-ascii> in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");
}

#[test]
fn replace_match_passes_backslash_question_literally() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);
    let replaced = replace_match_string_str("foo", "\\?", false, true, 0, &md)
        .expect("\\? must be accepted in non-literal replacement");
    // With `literal=true` the template is copied verbatim, matching
    // GNU's pass-through semantics from the other path.
    assert_eq!(replaced, b"\\?");

    let replaced = replace_match_string_str("foo", "a\\?b", false, false, 0, &md)
        .expect("\\? must be accepted in non-literal replacement");
    assert_eq!(replaced, b"a\\?b");
}

#[test]
fn search_forward_then_match_string() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("The quick brown fox");
    let mut md = None;
    let _ = re_search_forward(
        &mut buf,
        &lisp_pat("\\(quick\\) \\(brown\\)"),
        None,
        false,
        false,
        &mut md,
    );
    let md = md.as_ref().unwrap();

    // match-string 0 = "quick brown"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(0).unwrap(),)),
        "quick brown"
    );

    // match-string 1 = "quick"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(1).unwrap(),)),
        "quick"
    );

    // match-string 2 = "brown"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(2).unwrap(),)),
        "brown"
    );
}

/// The `SyntaxSpecSet` fusion peephole must (a) actually fire on the
/// font-lock symbol patterns it targets, and (b) preserve match semantics
/// byte-for-byte.  `SyntaxSpecSet` is opcode 33.
#[test]
fn regex_syntax_class_fusion() {
    const SYNTAX_SPEC_SET: u8 = 33;
    let syn = DefaultSyntaxLookup;

    // (a) Fusion fires on the real font-lock alternations.
    for pat in [
        r"\(?:\w\|\s_\)",               // FinalBare: two positive branches
        r"\(?:\w\|\s_\)+",              // ...inside a `+` loop
        r"\(?:\w\|\s_\|\\.\)+",         // Chain: run of 2 + a non-syntax branch
        r"(\(\(?:\w\|\s_\|\\.\)+\)\_>", // the sexp-head-kw bench pattern
    ] {
        let cp = regex_bench_compile(pat, false);
        assert!(
            cp.bytecode().contains(&SYNTAX_SPEC_SET),
            "fusion must emit SyntaxSpecSet for {pat:?}"
        );
    }

    // Negated alternations must NOT fuse (set algebra differs): `\Sw\|\S_`
    // matches every char, so a "not in {word,symbol}" set would be wrong.
    let neg = regex_bench_compile(r"\(?:\Sw\|\S_\)+", false);
    assert!(
        !neg.bytecode().contains(&SYNTAX_SPEC_SET),
        "negated syntax alternations must not fuse"
    );

    // (b) Match equivalence: the fused pattern behaves exactly like the
    // alternation it replaced across representative inputs.  `-` is symbol
    // syntax, space is whitespace, `\` is escape.
    let sym = regex_bench_compile(r"\_<\(?:\w\|\s_\|\\.\)+\_>", false);
    for (text, want) in [
        ("foo-bar", Some((0usize, 7usize))), // word + symbol run
        ("a b", Some((0, 1))),               // stops at whitespace
        ("x_y1", Some((0, 4))),              // word/symbol mix
        (" 123", Some((1, 4))),              // leading space skipped
    ] {
        let bytes = text.as_bytes();
        let got = regex_emacs::re_search(&sym, bytes, 0, bytes.len() as isize, &syn, 0)
            .map(|(_p, r)| (r.start[0] as usize, r.end[0] as usize));
        assert_eq!(got, want, "fused match mismatch for {text:?}");
    }

    // A single positive syntax branch stays a plain SyntaxSpec (nothing to
    // fuse) and still matches.
    let single = regex_bench_compile(r"\w+", false);
    assert!(!single.bytecode().contains(&SYNTAX_SPEC_SET));
    let got = regex_emacs::re_search(&single, b"abc!", 0, 4, &syn, 0)
        .map(|(_p, r)| (r.start[0], r.end[0]));
    assert_eq!(got, Some((0, 3)));
}

/// Interleaved A/B benchmark of the `SyntaxSpecSet` fusion, immune to
/// whole-machine timing noise: for each fused font-lock pattern it times the
/// SAME pattern compiled fused vs unfused, alternating iterations in one
/// process so both experience identical contention, and reports the min for
/// each plus the speedup.  Fused must be at least as fast; match counts must
/// be identical (proving semantics-preserving).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_syntax_fusion_ab() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 15u32;

    // The font-lock patterns that actually carry a `\w\|\s_\|\\.` alternation.
    let fused_patterns = ["el-defs", "sexp-head-kw", "catch-throw", "quoted-symbol"];

    let mut report = String::from("BENCH syntax-fusion A/B (fused vs unfused, interleaved):\n");
    let mut tot_f = std::time::Duration::ZERO;
    let mut tot_u = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        if !fused_patterns.contains(name) {
            continue;
        }
        let cp_f = regex_bench_compile(pat, false);
        let cp_u =
            neovm_text::test_support::authority::with_syntax_fusion_disabled_for_test(|| {
                regex_bench_compile(pat, false)
            });
        assert!(
            cp_f.bytecode().contains(&33u8) && !cp_u.bytecode().contains(&33u8),
            "A/B setup: fused must contain SyntaxSpecSet, unfused must not ({name})"
        );
        let mc_f = regex_bench_engine_scan(&cp_f, bytes);
        let mc_u = regex_bench_engine_scan(&cp_u, bytes);
        assert_eq!(mc_f, mc_u, "fused/unfused match count differs for {name}");

        // Warm.
        regex_bench_engine_scan(&cp_f, bytes);
        regex_bench_engine_scan(&cp_u, bytes);
        let mut best_f = std::time::Duration::MAX;
        let mut best_u = std::time::Duration::MAX;
        for _ in 0..iters {
            let t = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_f, bytes), mc_f);
            best_f = best_f.min(t.elapsed());
            let t = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_u, bytes), mc_u);
            best_u = best_u.min(t.elapsed());
        }
        tot_f += best_f;
        tot_u += best_u;
        report.push_str(&format!(
            "  {name:<14} fused {best_f:>9.1?}  unfused {best_u:>9.1?}  speedup {:.2}x  ({mc_f} matches)\n",
            best_u.as_secs_f64() / best_f.as_secs_f64(),
        ));
    }
    report.push_str(&format!(
        "  TOTAL          fused {tot_f:>9.1?}  unfused {tot_u:>9.1?}  speedup {:.2}x",
        tot_u.as_secs_f64() / tot_f.as_secs_f64(),
    ));
    panic!("{report}");
}

/// (a) Literal-heavy search over ~256 KiB of real elisp:
///   - the `Literal` fast path (`str::find` / naive case-fold window scan),
///   - the same literal forced through the GNU-bytecode engine (`Exactn`),
///   - both case-fold variants.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_literal_100kb() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let needle = "unread-command-events"; // real symbol, occurs in subr.el
    let iters = 15;

    let t_find = regex_bench_min(iters, || {
        assert!(neovm_text::emacs_core::regex::literal_find(&hay, needle, false).is_some());
    });
    let t_find_fold = regex_bench_min(iters, || {
        assert!(
            neovm_text::emacs_core::regex::literal_find(&hay, "UNREAD-COMMAND-EVENTS", true)
                .is_some()
        );
    });

    let cp = regex_bench_compile(needle, false);
    assert!(cp.fastmap_accurate && !cp.uses_syntax);
    let t_engine = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(regex_emacs::re_search(&cp, bytes, 0, bytes.len() as isize, &syn, 0).is_some());
    });
    let cp_fold = regex_bench_compile("UNREAD-COMMAND-EVENTS", true);
    let t_engine_fold = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(
            regex_emacs::re_search(&cp_fold, bytes, 0, bytes.len() as isize, &syn, 0).is_some()
        );
    });

    // Count full-scan throughput too (needle near start would flatter us):
    // scan for a needle that never occurs, so every byte is visited.
    let cp_miss = regex_bench_compile("neverxyzzyneverxyzzy", false);
    let t_engine_miss = regex_bench_min(iters, || {
        let syn = DefaultSyntaxLookup;
        assert!(
            regex_emacs::re_search(&cp_miss, bytes, 0, bytes.len() as isize, &syn, 0).is_none()
        );
    });
    let t_find_miss = regex_bench_min(iters, || {
        assert!(
            neovm_text::emacs_core::regex::literal_find(&hay, "neverxyzzyneverxyzzy", false)
                .is_none()
        );
    });

    panic!(
        "BENCH regex literal ({kib:.0} KiB subr.el): \
         hit: str::find {t_find:?} | engine-Exactn {t_engine:?} | \
         fold: window-scan {t_find_fold:?} | engine-translate {t_engine_fold:?} || \
         full-scan miss: str::find {t_find_miss:?} ({:.0} MiB/s) | engine {t_engine_miss:?} ({:.0} MiB/s)",
        kib / 1024.0 / t_find_miss.as_secs_f64(),
        kib / 1024.0 / t_engine_miss.as_secs_f64(),
    );
}

/// (b) Font-lock-ish workload, engine level: run each real emacs-lisp-mode
/// font-lock matcher over the haystack in the `(while (re-search-forward))`
/// shape. This is the per-fontification cost with zero elisp overhead.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fontlock_engine() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let iters = 7;

    let mut report = format!(
        "BENCH regex fontlock engine ({kib:.0} KiB subr.el):\n  \
         {:<16} {:>12} {:>12}  {:>5}  prefilter\n",
        "pattern", "fastmap-only", "prefilter", "matches"
    );
    // TOTAL over both configs so the net payoff is visible in one number.
    let mut total_off = std::time::Duration::ZERO; // fastmap-only (baseline)
    let mut total_on = std::time::Duration::ZERO; //  prefilter-enabled
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        // Prefilter-enabled compile (this branch's default).
        let cp = regex_bench_compile(pat, false);
        // Same pattern with the prefilter cleared → the fastmap-only baseline
        // (identical match engine + fastmap; the ONLY difference is the SIMD
        // multi-literal skip), so this is an apples-to-apples before/after.
        let mut cp_off = cp.clone();
        cp_off.prefilter = std::cell::OnceCell::from(None);

        let matches = regex_bench_engine_scan(&cp, bytes);
        // Semantics unchanged: the prefilter must never alter the match count.
        assert_eq!(
            regex_bench_engine_scan(&cp_off, bytes),
            matches,
            "prefilter changed match count for {name}"
        );

        // Interleave the two measurements in ONE loop (and warm both first)
        // so cache/paging state is identical for each — otherwise the second
        // `regex_bench_min` benefits from the first's warm-up, which would
        // fabricate a speedup for the no-prefilter patterns (whose two configs
        // run byte-identical code).  Take the min per config to reject noise.
        regex_bench_engine_scan(&cp_off, bytes);
        regex_bench_engine_scan(&cp, bytes);
        let mut t_off = std::time::Duration::MAX;
        let mut t_on = std::time::Duration::MAX;
        for _ in 0..iters {
            let a = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp_off, bytes), matches);
            t_off = t_off.min(a.elapsed());
            let b = std::time::Instant::now();
            assert_eq!(regex_bench_engine_scan(&cp, bytes), matches);
            t_on = t_on.min(b.elapsed());
        }
        total_off += t_off;
        total_on += t_on;
        report.push_str(&format!(
            "  {name:<16} {t_off:>12.1?} {t_on:>12.1?}  {matches:>5}  {}\n",
            if cp.literal_prefilter().is_some() {
                "YES"
            } else {
                "none"
            },
        ));
    }
    report.push_str(&format!(
        "  {:<16} {total_off:>12.1?} {total_on:>12.1?}   TOTAL one fontify pass  ({:+.1}%)",
        "TOTAL",
        (total_on.as_secs_f64() / total_off.as_secs_f64() - 1.0) * 100.0,
    ));
    panic!("{report}");
}

/// A/B: the same font-lock fontify pass with the non-backtracking Pike VM
/// (default for eligible patterns) vs the backtracker forced on.  Reports
/// per-pattern + TOTAL time for both and asserts the MATCH COUNT is
/// identical (semantics unchanged).  Run explicitly in release.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_pike_vs_backtracker() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let kib = bytes.len() as f64 / 1024.0;
    let iters = 7;

    let mut report = format!("BENCH pike-vs-backtracker fontlock ({kib:.0} KiB subr.el):\n");
    let mut tot_pike = std::time::Duration::ZERO;
    let mut tot_bt = std::time::Duration::ZERO;
    let mut tot_def = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let cp = regex_bench_compile(pat, false);
        let eligible = cp.pike_eligible;

        // Match counts MUST agree between all three routings.
        let mc_pike = regex_emacs::with_regex_engine_override(
            regex_emacs::RegexEngineOverride::PikeVm,
            || regex_bench_engine_scan(&cp, bytes),
        );
        let mc_bt = regex_emacs::with_regex_engine_override(
            regex_emacs::RegexEngineOverride::Backtracker,
            || regex_bench_engine_scan(&cp, bytes),
        );
        let mc_def = regex_bench_engine_scan(&cp, bytes); // production routing
        assert_eq!(
            mc_pike, mc_bt,
            "pike/backtrack match count differs for {name}"
        );
        assert_eq!(
            mc_def, mc_bt,
            "default/backtrack match count differs for {name}"
        );

        let t_pike = regex_bench_min(iters, || {
            assert_eq!(
                regex_emacs::with_regex_engine_override(
                    regex_emacs::RegexEngineOverride::PikeVm,
                    || regex_bench_engine_scan(&cp, bytes),
                ),
                mc_pike
            );
        });
        let t_bt = regex_bench_min(iters, || {
            assert_eq!(
                regex_emacs::with_regex_engine_override(
                    regex_emacs::RegexEngineOverride::Backtracker,
                    || regex_bench_engine_scan(&cp, bytes),
                ),
                mc_bt
            );
        });
        // Production default routing (backtracker + catastrophe budget).
        let t_def = regex_bench_min(iters, || {
            assert_eq!(regex_bench_engine_scan(&cp, bytes), mc_def);
        });
        tot_pike += t_pike;
        tot_bt += t_bt;
        tot_def += t_def;
        report.push_str(&format!(
            "  {name:<16} default {t_def:>9.1?}  backtrack {t_bt:>9.1?}  pike {t_pike:>9.1?}  eligible={eligible} ({mc_pike} matches)\n",
        ));
    }
    report.push_str(&format!(
        "  TOTAL default {tot_def:.1?}  backtrack {tot_bt:.1?}  pike {tot_pike:.1?}  (default/backtrack {:.2}x, pike/backtrack {:.2}x)",
        tot_def.as_secs_f64() / tot_bt.as_secs_f64(),
        tot_pike.as_secs_f64() / tot_bt.as_secs_f64(),
    ));
    panic!("{report}");
}

/// Fastmap A/B: the same effective search (find `(defun` heads) with a
/// syntax-free pattern (fastmap ON — skip loop over the first-byte table)
/// vs a `\_>`-terminated pattern (uses_syntax → fastmap disabled →
/// `re_match` attempted at every char position).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fastmap_on_vs_off() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 7;

    let cp_on = regex_bench_compile("(defun[ \t]", false);
    assert!(!cp_on.uses_syntax, "charset-only pattern must keep fastmap");
    let cp_off = regex_bench_compile("(defun\\_>", false);
    assert!(cp_off.uses_syntax, "\\_> must mark uses_syntax");

    let n_on = regex_bench_engine_scan(&cp_on, bytes);
    let n_off = regex_bench_engine_scan(&cp_off, bytes);
    let t_on = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_on, bytes), n_on);
    });
    let t_off = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_off, bytes), n_off);
    });

    panic!(
        "BENCH regex fastmap A/B ((defun heads, {} KiB): \
         fastmap-on \"(defun[ \\t]\" {t_on:?} ({n_on} matches) | \
         fastmap-off \"(defun\\\\_>\" {t_off:?} ({n_off} matches) -> {:.1}x",
        bytes.len() / 1024,
        t_off.as_secs_f64() / t_on.as_secs_f64(),
    );
}

/// (d) Case-fold + multibyte: the `quoted-symbol` matcher (multibyte
/// pattern chars U+2018/U+2019) and a case-folded keyword alternation,
/// both over the multibyte haystack.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_casefold_multibyte() {
    crate::test_utils::init_test_tracing();
    let hay = regex_bench_haystack();
    let bytes = hay.as_bytes();
    let iters = 7;

    let alt = "(\\(?:defun\\|defmacro\\|defvar\\|defconst\\|defsubst\\)[ \t]";
    let cp_nofold = regex_bench_compile(alt, false);
    let cp_fold = regex_bench_compile(alt, true);
    let n_nofold = regex_bench_engine_scan(&cp_nofold, bytes);
    let n_fold = regex_bench_engine_scan(&cp_fold, bytes);
    let t_nofold = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_nofold, bytes), n_nofold);
    });
    let t_fold = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_fold, bytes), n_fold);
    });

    let (mb_name, mb_pat) = REGEX_BENCH_FONTLOCK_PATTERNS
        .iter()
        .find(|(name, _)| *name == "quoted-symbol")
        .expect("quoted-symbol pattern present");
    let cp_mb = regex_bench_compile(mb_pat, false);
    let n_mb = regex_bench_engine_scan(&cp_mb, bytes);
    let t_mb = regex_bench_min(iters, || {
        assert_eq!(regex_bench_engine_scan(&cp_mb, bytes), n_mb);
    });

    panic!(
        "BENCH regex case-fold/multibyte ({} KiB): def-alternation nofold {t_nofold:?} \
         ({n_nofold}) vs fold {t_fold:?} ({n_fold}) -> {:.1}x | \
         {mb_name} (multibyte chars) {t_mb:?} ({n_mb} matches)",
        bytes.len() / 1024,
        t_fold.as_secs_f64() / t_nofold.as_secs_f64(),
    );
}

/// (2) Pattern-compilation cost: fresh `regex_compile_lisp` per pattern
/// (a cache miss) vs the LRU cache hit path (`compile_lisp_pattern_with_posix`),
/// vs one engine scan — how hot is compilation relative to matching?
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_compile_cost() {
    crate::test_utils::init_test_tracing();
    let mut report = String::from("BENCH regex compile cost:\n");

    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let lisp = LispString::from_utf8(pat);
        let compiles = 200u32;
        let t_miss = regex_bench_min(5, || {
            for _ in 0..compiles {
                let cp =
                    regex_emacs::regex_compile_lisp(&lisp, false, false).expect("pattern compiles");
                std::hint::black_box(&cp);
            }
        });
        // Cached path: same key each call — a hit after the first.
        let t_hit = regex_bench_min(5, || {
            for _ in 0..compiles {
                let cp = neovm_text::emacs_core::regex::compile_lisp_pattern_with_posix(
                    &lisp, false, false, true,
                )
                .expect("pattern compiles");
                std::hint::black_box(&cp);
            }
        });
        report.push_str(&format!(
            "  {name:<16} compile-miss {:>8.2?}/ea  cache-hit {:>8.2?}/ea\n",
            t_miss / compiles,
            t_hit / compiles,
        ));
    }
    panic!("{report}");
}

/// (b, full stack) Font-lock-ish workload through the real elisp builtins:
/// insert the haystack into a buffer, then per pattern run the exact
/// font-lock loop `(goto-char (point-min)) (while (re-search-forward P nil t))`.
/// Also re-runs one pattern with the gap parked mid-buffer (as after a
/// keystroke at the middle) — `with_buffer_emacs_bytes` then copies the
/// whole accessible region on every `re-search-forward` call (audit #17).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_fontlock_elisp() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let hay = regex_bench_haystack();
    let kib = hay.len() as f64 / 1024.0;
    ev.eval_str(&format!(
        "(progn (insert \"{}\") (goto-char (point-max)) (insert \"\\n\") nil)",
        regex_bench_elisp_escape(&hay)
    ))
    .expect("insert haystack");
    // Gap now sits at point-max: the whole accessible region is contiguous,
    // so buffer searches borrow zero-copy.

    let iters = 5;
    let mut report = format!("BENCH regex fontlock elisp ({kib:.0} KiB buffer):\n");
    let mut total = std::time::Duration::ZERO;
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let form = format!(
            "(progn (goto-char (point-min)) (let ((n 0)) (while (re-search-forward \"{}\" nil t) (setq n (1+ n))) n))",
            regex_bench_elisp_escape(pat)
        );
        let matches = ev
            .eval_str(&form)
            .expect("fontlock pass evaluates")
            .as_int()
            .expect("match count");
        let t = regex_bench_min(iters, || {
            let got = ev.eval_str(&form).expect("fontlock pass evaluates");
            assert_eq!(got.as_int(), Some(matches));
        });
        total += t;
        report.push_str(&format!("  {name:<16} {t:>10.1?}  {matches:>5} matches\n"));
    }
    report.push_str(&format!(
        "  TOTAL one fontify pass over all patterns: {total:.1?}\n"
    ));

    // Park the gap mid-buffer: insert+delete a char at the middle. Every
    // subsequent whole-buffer search must copy the accessible region.
    // (Core builtins only — a bare Context has no subr.el, so no delete-char.)
    ev.eval_str(
        "(progn (goto-char (/ (point-max) 2)) (insert \"z\") (delete-region (- (point) 1) (point)) nil)",
    )
    .expect("park gap mid-buffer");
    let (name, pat) = REGEX_BENCH_FONTLOCK_PATTERNS[1]; // sexp-head-kw
    let form = format!(
        "(progn (goto-char (point-min)) (let ((n 0)) (while (re-search-forward \"{}\" nil t) (setq n (1+ n))) n))",
        regex_bench_elisp_escape(pat)
    );
    let matches = ev
        .eval_str(&form)
        .expect("gap-split pass evaluates")
        .as_int()
        .expect("match count");
    let t_split = regex_bench_min(iters, || {
        let got = ev.eval_str(&form).expect("gap-split pass evaluates");
        assert_eq!(got.as_int(), Some(matches));
    });
    report.push_str(&format!(
        "  {name} with gap mid-buffer (copy per call): {t_split:.1?}  {matches} matches"
    ));
    panic!("{report}");
}

/// `string-match` per-call overhead: a short hot-cache `string-match` on a
/// small string (per-call fixed costs: cache probe, pattern copy, match-data
/// conversion) vs a full-scan miss over the big string (throughput).
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_string_match_elisp() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let hay = regex_bench_haystack();
    ev.eval_str(&format!(
        "(progn (defvar regex-bench-big \"{}\") (defvar regex-bench-small (substring regex-bench-big 0 512)) nil)",
        regex_bench_elisp_escape(&hay)
    ))
    .expect("define bench strings");

    let iters = 7;
    let calls = 1000u32;
    // Core builtins only — a bare Context has no subr.el (no dotimes/when).
    let t_small = regex_bench_min(iters, || {
        ev.eval_str(
            "(let ((n 0) (i 0)) (while (< i 1000) (if (string-match \"(\\\\(\\\\(?:\\\\w\\\\|\\\\s_\\\\)+\\\\)\\\\_>\" regex-bench-small) (setq n (1+ n))) (setq i (1+ i))) n)",
        )
        .expect("small string-match loop");
    });
    let t_miss = regex_bench_min(iters, || {
        ev.eval_str("(string-match \"neverxyzzyneverxyzzy\" regex-bench-big)")
            .expect("big string-match miss");
    });
    panic!(
        "BENCH regex string-match elisp: hot small (512B, first match) {:?}/call | \
         literal full-scan miss over {} KiB: {t_miss:?}",
        t_small / calls,
        hay.len() / 1024,
    );
}

/// Fastmap-on/off equivalence over the real font-lock patterns plus
/// syntax/class/boundary/charset patterns, across ASCII, multibyte,
/// case-fold, and match-at-start/middle/none haystacks.
#[test]
fn regex_fastmap_equivalence_fuzz() {
    let mut patterns: Vec<(&str, bool)> = REGEX_BENCH_FONTLOCK_PATTERNS
        .iter()
        .map(|(_, pat)| (*pat, false))
        .collect();
    patterns.extend_from_slice(&[
        ("[[:digit:]]+", false),
        ("[[:word:]]+", false),
        ("[[:space:]]+", false),
        ("x[[:alpha:]]*!", false),
        ("[-[:alnum:]]+\\.el", false),
        ("[^[:word:]]+", false),
        ("[[:word:]]+", true),
        ("\\bdefun", false),
        ("provide\\b", false),
        ("\\<forward\\>", false),
        ("\\_<point-min\\_>", false),
        ("\\w+", false),
        ("\\W\\w", false),
        ("\\s-;;", false),
        ("\\Sw+", false),
        ("[a-f\u{e9}-\u{ef}]+", false),
        ("(defun\\_>", false),
        ("(DEFUN\\_>", true),
        ("^;;;###autoload", false),
        ("\\(setq\\|setf\\)[ \t]", false),
        ("[A-F]+x", true),
    ]);

    let haystacks: [&str; 10] = [
        "",
        "(defun foo-bar (x) \"doc \u{2018}quoted\u{2019}\" nil)",
        ";;;###autoload\n(defun x ())",
        "no-match-here 12345 end",
        "===defun=== \u{2018}sym\u{2019} `q' \\e",
        "\u{e9}\u{e9} (defun \u{e0}ccent-fn ()) ;; \u{2018}x\u{2019}",
        "a b\tc\nd-e_f",
        "defun at start",
        "ends with (defun\t",
        "(SETQ CASE-FOLD T) (setf x)",
    ];

    for &(pat, case_fold) in &patterns {
        let cp = regex_bench_compile(pat, case_fold);
        for hay in &haystacks {
            assert_fastmap_equivalence(
                &format!("{pat:?} (fold={case_fold}) vs {hay:?}"),
                &cp,
                &DefaultSyntaxLookup,
                hay.as_bytes(),
            );
        }
        // Same corpus under a modified syntax table, mirroring the
        // front-end pipeline: `used_syntax` fastmaps are rebaked
        // against the active table before use.
        let mut cp_dash = cp.clone();
        regex_emacs::recompute_fastmap(&mut cp_dash, &DashWordSyntaxLookup);
        for hay in &haystacks {
            assert_fastmap_equivalence(
                &format!("{pat:?} (fold={case_fold}, dash-word table) vs {hay:?}"),
                &cp_dash,
                &DashWordSyntaxLookup,
                hay.as_bytes(),
            );
        }
    }

    // The real font-lock patterns over real elisp text (2 KiB of
    // subr.el, cut at a char boundary).
    let big = regex_bench_haystack();
    let mut end = big.len().min(2 * 1024);
    while !big.is_char_boundary(end) {
        end -= 1;
    }
    let big = &big[..end];
    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let cp = regex_bench_compile(pat, false);
        assert_fastmap_equivalence(
            &format!("{name} vs subr.el slice"),
            &cp,
            &DefaultSyntaxLookup,
            big.as_bytes(),
        );
    }
}

/// Per-opcode `analyze_first` gates (GNU regex-emacs.c:3062-3234).
#[test]
fn regex_fastmap_syntax_pattern_gates() {
    // `\_>` after a literal: match-time syntax use must NOT disable the
    // fastmap; the leading exactn provides the single candidate byte.
    let cp = regex_bench_compile("(defun\\_>", false);
    assert!(cp.uses_syntax, "\\_> consults syntax at match time");
    assert!(!cp.used_syntax, "no table content is baked for \\_>");
    assert!(cp.fastmap_accurate && !cp.can_be_null, "fastmap stays live");
    assert!(cp.fastmap[b'(' as usize]);
    assert!(!cp.fastmap[b'd' as usize]);

    // Zero-width `\<` contributes the following atom (GNU: "not
    // succeeded yet", keep walking).
    let cp = regex_bench_compile("\\<defun", false);
    assert!(!cp.can_be_null);
    assert!(cp.fastmap[b'd' as usize]);
    assert!(!cp.fastmap[b'e' as usize]);

    // Leading `\w` (syntaxspec): GNU analyze_first aborts ("This match
    // depends on text properties") -> can_be_null -> no skip.
    let cp = regex_bench_compile("\\w+", false);
    assert!(cp.can_be_null, "leading syntaxspec disables the skip");

    // Fixed POSIX class: fastmap = ASCII members + multibyte leads; no
    // syntax-table axis.
    let cp = regex_bench_compile("[[:digit:]]x", false);
    assert!(!cp.can_be_null && !cp.used_syntax);
    assert!(cp.fastmap[b'5' as usize]);
    assert!(!cp.fastmap[b'a' as usize]);
    assert!(cp.fastmap[0xC3], "class charsets admit multibyte leads");

    // Syntax-dependent POSIX class: `used_syntax` (GNU
    // regex-emacs.c:2096-2101) + standard-table ASCII word members.
    let cp = regex_bench_compile("[[:word:]]+z", false);
    assert!(cp.used_syntax && !cp.can_be_null);
    assert!(cp.fastmap[b'a' as usize] && cp.fastmap[b'0' as usize]);
    assert!(
        !cp.fastmap[b'-' as usize],
        "standard table: '-' is not a word constituent"
    );

    // Rebake against a table where '-' IS a word constituent.
    let mut cp_dash = cp.clone();
    regex_emacs::recompute_fastmap(&mut cp_dash, &DashWordSyntaxLookup);
    assert!(cp_dash.fastmap[b'-' as usize]);

    // keyword-colon: `\_<` passes through to the exactn `:`.
    let cp = regex_bench_compile("\\_<:\\(?:\\w\\|\\s_\\|\\\\.\\)+\\_>", false);
    assert!(!cp.can_be_null);
    assert!(cp.fastmap[b':' as usize]);
    assert!(!cp.fastmap[b'a' as usize]);
}

/// A syntax-table content mutation must not serve a stale fastmap:
/// GNU `Fmodify_syntax_entry` ends with `clear_regexp_cache ()`; our
/// analog is the syntax mutation epoch in the cache key.
#[test]
fn regex_syntax_table_mutation_invalidates_cached_fastmap() {
    let mut ev = crate::test_utils::new_context();
    ev.eval_str("(progn (insert \"===abc===\") nil)")
        .expect("insert");
    let probe = "(progn (goto-char (point-min)) \
                 (if (re-search-forward \"[[:word:]]+\" nil t) (match-beginning 0) -1))";
    // Standard table: '=' is Ssymbol, so the first word chars are "abc".
    let first = ev.eval_str(probe).expect("first search");
    assert_eq!(first.as_int(), Some(4), "standard table matches at 'abc'");
    // Make '=' a word constituent.  The cached pattern's fastmap (baked
    // without '=') must not be reused, or the search would skip the
    // '===' prefix and still report position 4.
    ev.eval_str("(modify-syntax-entry ?= \"w\")")
        .expect("modify-syntax-entry");
    let second = ev.eval_str(probe).expect("second search");
    assert_eq!(
        second.as_int(),
        Some(1),
        "mutated table must re-bake the fastmap (no stale cache entry)"
    );
}

/// Cache entries baked for one syntax table must not be served under
/// another (GNU keys the regexp cache by the syntax-table object,
/// search.c:222-224), and table-independent entries survive table
/// switches.
#[test]
fn regex_syntax_table_identity_keys_cached_fastmap() {
    let mut ev = crate::test_utils::new_context();
    // Build the modified table BEFORE any compile so the mutation epoch
    // is constant across all three probes — this isolates the identity
    // axis of the cache key.
    ev.eval_str(
        "(progn (defvar regex-fastmap-t2 (copy-syntax-table)) \
                (modify-syntax-entry ?= \"w\" regex-fastmap-t2) \
                (insert \"===abc===\") nil)",
    )
    .expect("setup");
    let probe = "(progn (goto-char (point-min)) \
                 (if (re-search-forward \"[[:word:]]+\" nil t) (match-beginning 0) -1))";

    let under_standard = ev.eval_str(probe).expect("standard search");
    assert_eq!(under_standard.as_int(), Some(4));

    ev.eval_str("(set-syntax-table regex-fastmap-t2)")
        .expect("switch to t2");
    let under_t2 = ev.eval_str(probe).expect("t2 search");
    assert_eq!(
        under_t2.as_int(),
        Some(1),
        "the standard-table cache entry must not satisfy table t2"
    );

    ev.eval_str("(set-syntax-table (standard-syntax-table))")
        .expect("switch back");
    let back = ev.eval_str(probe).expect("standard search again");
    assert_eq!(
        back.as_int(),
        Some(4),
        "the t2 cache entry must not satisfy the standard table"
    );
}

/// The same overflow at the elisp level signals GNU's error
/// (`search.c:matcher_overflow`), even under `noerror`.
#[test]
fn regex_fail_stack_overflow_signals_elisp_error() {
    let mut ev = crate::test_utils::new_context();
    ev.eval_str("(progn (insert (make-string 400000 ?a)) (goto-char (point-min)) nil)")
        .expect("insert haystack");
    // `noerror = t` must not swallow the overflow: GNU's
    // matcher_overflow is an `error`, not `search-failed`.
    let caught = ev
        .eval_str(
            "(condition-case err \
                 (progn (re-search-forward \"\\\\(a*\\\\)*b\" nil t) \"no-error\") \
               (error (format \"%S\" err)))",
        )
        .expect("condition-case evaluates");
    let msg = caught
        .as_lisp_string()
        .and_then(|s| s.as_utf8_str())
        .unwrap_or_default()
        .to_string();
    assert!(
        msg.contains("Stack overflow in regexp matcher"),
        "expected GNU matcher_overflow error, got: {msg}"
    );
}

/// Audit #17: a regexp search across a mid-buffer gap must produce the
/// same result as over contiguous text (the search path now moves the
/// gap out of the accessible range instead of copying it per call).
#[test]
fn regex_search_across_mid_buffer_gap() {
    let mut ev = crate::test_utils::new_context();
    // Insert text, then park the gap in the middle of "defun" via an
    // insert+delete at position 7.
    ev.eval_str(
        "(progn (insert \"aaa defun bbb defun ccc\") \
                (goto-char 7) (insert \"z\") (delete-region 7 8) \
                (goto-char (point-min)) nil)",
    )
    .expect("build gap-split buffer");
    let hit = ev
        .eval_str("(if (re-search-forward \"defun\\\\_>\" nil t) (match-beginning 0) -1)")
        .expect("first search");
    assert_eq!(hit.as_int(), Some(5), "match spanning the parked gap");
    let hit2 = ev
        .eval_str("(if (re-search-forward \"defun\\\\_>\" nil t) (match-beginning 0) -1)")
        .expect("second search");
    assert_eq!(hit2.as_int(), Some(15));
    // Backward from the end re-finds the second occurrence.
    let back = ev
        .eval_str(
            "(progn (goto-char (point-max)) \
                    (if (re-search-backward \"defun\\\\_>\" nil t) (point) -1))",
        )
        .expect("backward search");
    assert_eq!(back.as_int(), Some(15));
}

/// Measurement probe for the lazy-match-data (Tier-2) sizing question: of
/// the group endpoints eagerly byte->char converted at publish time, how
/// many are ever read back by Lisp? Run explicitly:
///   cargo nextest run -p neovm-core -E 'test(match_publish_read_stats_probe)' --run-ignored all --no-capture
#[test]
#[cfg(debug_assertions)]
#[cfg(feature = "perf-probes")]
fn match_publish_read_stats_probe() {
    use std::sync::atomic::Ordering;
    let mut eval = crate::test_utils::runtime_startup_context();
    neovm_text::test_support::authority::reset_regex_match_stats_for_test();
    eval.eval_str(
        r#"(with-temp-buffer
             (insert-file-contents (locate-library "bytecomp.el" t))
             (emacs-lisp-mode)
             (font-lock-ensure (point-min) (point-max))
             t)"#,
    )
    .expect("fontify bytecomp.el");
    let s = |c: &std::sync::atomic::AtomicUsize| c.load(Ordering::Relaxed);
    println!(
        "MATCH-STATS publishes={} published_g0={} published_sub={} \
         read_g0={} read_sub={} full_exports={} \
         distinct_some_g0={} distinct_some_sub={}",
        s(&neovm_text::emacs_core::regex::match_stats::PUBLISHES),
        s(&neovm_text::emacs_core::regex::match_stats::PUBLISHED_GROUP0),
        s(&neovm_text::emacs_core::regex::match_stats::PUBLISHED_SUB),
        s(&neovm_text::emacs_core::regex::match_stats::READ_GROUP0),
        s(&neovm_text::emacs_core::regex::match_stats::READ_SUB),
        s(&neovm_text::emacs_core::regex::match_stats::FULL_EXPORTS),
        s(&neovm_text::emacs_core::regex::match_stats::FIRST_SOME_G0),
        s(&neovm_text::emacs_core::regex::match_stats::FIRST_SOME_SUB),
    );
}

/// GNU 31.1: a buffer search publishes its registers as character positions
/// (`search_buffer_re`'s `BYTE_TO_CHAR` per register) -- in a buffer whose
/// characters are all one byte and in one with multibyte characters, with
/// unmatched groups, a match at the end of the text, a narrowing and a
/// backward search.
#[test]
fn buffer_search_match_data_is_in_characters_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (if (bufferp x) 'buf x)) (match-data t)))))
          (list
           (with-temp-buffer
             (insert "alpha beta* gamma\nzeta: end")
             (list
              (progn (goto-char (point-min)) (re-search-forward "\\(b\\(e\\)\\)\\|\\(x\\)ta\\(\\*\\)?" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(en\\)\\(d\\)\\'" nil t) (funcall md))
              (progn (goto-char (point-min)) (looking-at "\\(al\\)\\(ph\\)?\\(q\\)?") (funcall md))
              (save-restriction (narrow-to-region 7 12) (goto-char (point-min)) (re-search-forward "\\(ta\\)\\(\\*\\)" nil t) (funcall md))
              (progn (goto-char (point-max)) (re-search-backward "\\(z\\)\\(e\\)" nil t) (funcall md))
              (progn (goto-char (point-min)) (search-forward "gamma" nil t) (funcall md))))
           (with-temp-buffer
             (insert "αλφα beta* γάμμα\nζήτα: end")
             (list
              (progn (goto-char (point-min)) (re-search-forward "\\(b\\(e\\)\\)\\|\\(x\\)ta\\(\\*\\)?" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(γά\\)\\(μ+\\)" nil t) (funcall md))
              (progn (goto-char (point-min)) (re-search-forward "\\(en\\)\\(d\\)\\'" nil t) (funcall md))
              (progn (goto-char (point-min)) (looking-at "\\(αλ\\)\\(φ\\)?\\(q\\)?") (funcall md))
              (save-restriction (narrow-to-region 7 12) (goto-char (point-min)) (re-search-forward "\\(ta\\)\\(\\*\\)" nil t) (funcall md))))))
        "#,
    );
    assert_eq!(
        result,
        "OK (((7 9 7 9 8 9 buf) (25 28 25 27 27 28 buf) (1 5 1 3 3 5 buf) (9 12 9 11 11 12 buf) (19 21 19 20 20 21 buf) (13 18 buf)) ((6 8 6 8 7 8 buf) (12 16 12 14 14 16 buf) (24 27 24 26 26 27 buf) (1 4 1 3 3 4 buf) (8 11 8 10 10 11 buf)))"
    );
}

/// GNU 31.1: a successful buffer search replaces the match data whatever it
/// held (string match data, another buffer's), `COUNT` publishes the last
/// match, and a failed search or `inhibit-changing-match-data` leaves it as
/// it was -- the in-place publication must reuse nothing stale.
#[test]
fn buffer_search_replaces_match_data_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (cond ((bufferp x) (buffer-name x)) ((markerp x) (list 'm (marker-position x) (buffer-name (marker-buffer x)))) (t x))) (match-data)))))
          (let ((a (get-buffer-create "md-a")) (b (get-buffer-create "md-b")) out)
            (with-current-buffer a (erase-buffer) (insert "one two three two one"))
            (with-current-buffer b (erase-buffer) (insert "αβ two γ two"))
            (string-match "\\(t\\)wo" "xx two")
            (push (match-data) out)
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "\\(t\\)\\(w\\)o" nil t) (push (funcall md) out))
            (with-current-buffer b (goto-char (point-min)) (search-forward "two" nil t 2) (push (funcall md) out) (push (point) out))
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "zzz" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-max)) (re-search-backward "\\(o\\)ne" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min))
              (let ((inhibit-changing-match-data t)) (re-search-forward "three" nil t)) (push (point) out) (push (funcall md) out))
            (with-current-buffer b (goto-char (point-max)) (search-backward "two" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min)) (re-search-forward "\\(two\\)\\|\\(xyz\\)" nil t) (push (funcall md) out))
            (with-current-buffer a (goto-char (point-min)) (condition-case e (re-search-forward "nope") (error (push (car e) out))) (push (funcall md) out))
            (kill-buffer a) (kill-buffer b)
            (nreverse out)))
        "#,
    );
    assert_eq!(
        result,
        r#"OK ((3 6 3 4) ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 6 "md-a") (m 6 "md-a") (m 7 "md-a")) ((m 10 "md-b") (m 13 "md-b")) 13 ((m 10 "md-b") (m 13 "md-b")) ((m 19 "md-a") (m 22 "md-a") (m 19 "md-a") (m 20 "md-a")) 14 ((m 19 "md-a") (m 22 "md-a") (m 19 "md-a") (m 20 "md-a")) ((m 10 "md-b") (m 13 "md-b")) ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 8 "md-a")) search-failed ((m 5 "md-a") (m 8 "md-a") (m 5 "md-a") (m 8 "md-a")))"#
    );
}

/// GNU 31.1: a search made zero times (`COUNT` 0) does not move point and
/// makes the match data an empty match at point -- `search_buffer`'s
/// `set_search_regs (pos_byte, 0)` -- whatever it held before, unless
/// `inhibit-changing-match-data' is set.
#[test]
fn a_zero_count_search_matches_the_empty_string_at_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"
        (let ((md (lambda () (mapcar (lambda (x) (if (bufferp x) 'buf x)) (match-data t)))))
          (with-temp-buffer
            (insert "one two three αβ two")
            (list
             (progn (goto-char 5) (string-match "\\(x\\)" "zzx") (list (re-search-forward "t\\(w\\)o" nil nil 0) (point) (funcall md)))
             (progn (goto-char 5) (re-search-forward "\\(o\\)" nil t) (list (search-forward "two" nil nil 0) (point) (funcall md)))
             (progn (goto-char 18) (list (search-backward "one" nil nil 0) (point) (funcall md)))
             (progn (goto-char 18) (string-match "\\(x\\)" "zzx") (list (posix-search-backward "o+" nil nil 0) (point) (funcall md)))
             (progn (goto-char 7) (string-match "\\(x\\)" "zzx")
                    (let ((inhibit-changing-match-data t)) (list (re-search-forward "x" nil nil 0) (point) (funcall md)))))))
        "#,
    );
    assert_eq!(
        result,
        "OK ((5 5 (5 5 buf)) (8 8 (8 8 buf)) (18 18 (18 18 buf)) (18 18 (18 18 buf)) (7 7 (2 3 2 3)))"
    );
}

#[test]
fn literal_register_reuse_discards_captures_and_publishes_character_positions() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        for text in ["xab ab", "éab 中ab"] {
            for backward in [false, true] {
                let mut buf = make_test_buffer_with_backend(text, kind);
                // A prior result with enough captures to spill inline storage.
                let mut regs = SearchRegisters(SearchRegistersKind::Groups(EngineMatchData::new(
                    std::iter::repeat_n(Some(MatchGroup::new(0, 1)), 12).collect(),
                )));
                let mut published = None;
                regs.publish_buffer_into(&buf, &mut published);
                for pattern in ["ab", "b", "", "absent"] {
                    let start = if backward { text.len() } else { 0 };
                    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(start));
                    let old = match_data_snapshot(&published);
                    let result = if backward {
                        super::search_backward_into(
                            &mut buf,
                            &lisp_pat(pattern),
                            None,
                            true,
                            false,
                            &mut regs,
                        )
                    } else {
                        super::search_forward_into(
                            &mut buf,
                            &lisp_pat(pattern),
                            None,
                            true,
                            false,
                            &mut regs,
                        )
                    }
                    .unwrap();
                    let expected_start = if backward {
                        text.rfind(pattern)
                    } else {
                        text.find(pattern)
                    };
                    match (result, expected_start) {
                        (Some(point), Some(byte_start)) => {
                            let byte_end = byte_start + pattern.len();
                            assert_eq!(point.get(), if backward { byte_start } else { byte_end });
                            regs.publish_buffer_into(&buf, &mut published);
                            let data = published.as_ref().unwrap();
                            assert_eq!(data.source(), MatchDataSource::Buffer(buf.id()));
                            assert_eq!(
                                data.group(0),
                                Some(MatchGroup::new(
                                    text[..byte_start].chars().count() + 1,
                                    text[..byte_end].chars().count() + 1,
                                ))
                            );
                            let groups = data.groups_snapshot();
                            assert_eq!(groups.len(), GNU_SEARCH_REGS_BASE_CAPACITY);
                            assert!(groups[1..].iter().all(Option::is_none));
                        }
                        (None, None) => assert_eq!(match_data_snapshot(&published), old),
                        other => panic!("literal result disagrees: {other:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn retained_match_registers_preserve_existing_layout() {
    // This is the representation before failure could retag Lisp registers.
    // Its size/alignment (including Option's niche) must remain unchanged.
    #[allow(dead_code)]
    enum PreviousKind {
        StringChars {
            groups: smallvec::SmallVec<[Option<CharRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
            searched: Option<SearchedString>,
        },
        Buffer {
            id: BufferId,
            groups: smallvec::SmallVec<[Option<LispCharMatchRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
        },
    }
    #[allow(dead_code)]
    struct PreviousMatchData {
        kind: PreviousKind,
        #[cfg(debug_assertions)]
        read_mask: std::cell::Cell<u64>,
    }
    assert_eq!(
        std::mem::size_of::<MatchData>(),
        std::mem::size_of::<PreviousMatchData>()
    );
    assert_eq!(
        std::mem::align_of::<MatchData>(),
        std::mem::align_of::<PreviousMatchData>()
    );
    assert_eq!(
        std::mem::size_of::<Option<MatchData>>(),
        std::mem::size_of::<Option<PreviousMatchData>>()
    );
}

fn match_group(group: Option<MatchGroup>) -> Option<MatchGroup> {
    group
}

fn search_forward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::search_forward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn search_backward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::search_backward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn re_search_forward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::re_search_forward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn re_search_backward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::re_search_backward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn looking_at(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<bool, String> {
    super::looking_at(buf, pattern, case_fold).map(|published_match_data| {
        if let Some(published_match_data) = published_match_data {
            *match_data = Some(published_match_data);
            true
        } else {
            false
        }
    })
}

/// `replace_match_string` is byte-native (issue #131). These string tests pass
/// Rust `&str` sources mirroring `string_match_full`, which searches
/// `LispString::from_utf8(source)`; this helper threads the matching Emacs-bytes
/// and `STRING_MULTIBYTE` flag so the existing assertions stay byte-faithful.
fn replace_match_string_str(
    source: &str,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
) -> Result<Vec<u8>, String> {
    let source_lisp = LispString::from_utf8(source);
    replace_match_string(
        source_lisp.as_bytes(),
        source_lisp.is_multibyte(),
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
    )
}

/// Build a multibyte `LispString` search pattern from a Rust literal (issue #131:
/// `search_forward`/`search_backward` now take the faithful pattern, not a storage
/// `&str`).
fn lisp_pat(s: &str) -> LispString {
    LispString::from_utf8(s)
}

fn buffer_match_group_byte_range(buf: &Buffer, group: MatchGroup) -> EmacsByteRange {
    EmacsByteRange::new(
        buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::from_one_based_usize(
            group.start(),
        )),
        buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::from_one_based_usize(
            group.end(),
        )),
    )
}

fn make_test_buffer(text: &str) -> Buffer {
    make_test_buffer_with_backend(text, BufferTextBackendKind::GapBuffer)
}

fn make_test_buffer_with_backend(text: &str, kind: BufferTextBackendKind) -> Buffer {
    let implemented_kind = kind.implemented().expect("test backend is implemented");
    let mut buf = Buffer::new_with_text_backend_kind(
        BufferId(1),
        Value::string("test"),
        implemented_kind,
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    // Reset point to beginning
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    // zv was updated by insert
    buf
}

fn implemented_text_backends() -> impl Iterator<Item = BufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MatchDataSnapshot {
    groups: Vec<Option<MatchGroup>>,
    searched_buffer: Option<BufferId>,
    searched_string_is_some: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferSearchSnapshot<T> {
    result: Result<T, String>,
    pt_byte: usize,
    pt: usize,
    full_bytes: Vec<u8>,
    multibyte: bool,
    match_data: Option<MatchDataSnapshot>,
}

fn match_data_snapshot(match_data: &Option<MatchData>) -> Option<MatchDataSnapshot> {
    match_data.as_ref().map(|data| {
        let source = data.source();
        MatchDataSnapshot {
            groups: data.groups_snapshot(),
            searched_buffer: match source {
                MatchDataSource::String => None,
                MatchDataSource::Buffer(buffer_id) => Some(buffer_id),
            },
            searched_string_is_some: source.is_string(),
        }
    })
}

fn buffer_search_snapshot<T>(
    result: Result<T, String>,
    buf: &Buffer,
    match_data: &Option<MatchData>,
) -> BufferSearchSnapshot<T> {
    let mut full_bytes = Vec::new();
    buf.copy_emacs_byte_range_to(buf.full_emacs_byte_range(), &mut full_bytes);
    BufferSearchSnapshot {
        result,
        pt_byte: buf.point_emacs_byte_pos().get(),
        pt: buf.point_char_pos().get(),
        full_bytes,
        multibyte: buf.get_multibyte(),
        match_data: match_data_snapshot(match_data),
    }
}

fn literal_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("foo"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = search_backward(&mut buf, &lisp_pat("foo"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let result = search_forward(&mut buf, &lisp_pat("beta"), None, false, true, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let narrow_start = "α foo\n".len();
    let narrow_end = "α foo\nBeta 123".len();
    buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        narrow_start,
        narrow_end,
    ));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = search_forward(&mut buf, &lisp_pat("foo"), None, true, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = search_forward(&mut buf, &lisp_pat("Beta"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn regex_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\([^ \n]+\\) \\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = re_search_backward(
        &mut buf,
        &lisp_pat("\\(foo\\)\\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let narrow_start = "α foo\n".len();
    let narrow_end = "α foo\nBeta 123".len();
    buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        narrow_start,
        narrow_end,
    ));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^ ]+\\) \\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn looking_at_backend_trace(kind: BufferTextBackendKind) -> Vec<BufferSearchSnapshot<bool>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let gamma_line = "α foo\nBeta 123\n".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(gamma_line));
    let result = looking_at(&buf, &lisp_pat("\\(.\\) foo\\([0-9]+\\)"), false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new("α ".len()));
    let result = looking_at(&buf, &lisp_pat("foo"), false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn replace_match_backend_trace(kind: BufferTextBackendKind) -> BufferSearchSnapshot<()> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\(foo\\)\\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(
        result.is_ok(),
        "setup search failed for {kind:?}: {result:?}"
    );

    let result = replace_match_buffer(&mut buf, "\\1-\\2", false, false, 0, &md);
    buffer_search_snapshot(result.map(|_| ()), &buf, &md)
}

fn unibyte_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_unibyte_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("."), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let result = search_forward(&mut buf, &lisp_pat("a"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = search_backward(&mut buf, &lisp_pat("b"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

/// Real GNU Emacs 31 emacs-lisp-mode font-lock matcher regexps.
const REGEX_BENCH_FONTLOCK_PATTERNS: &[(&str, &str)] = &[
    (
        "el-defs",
        "(\\(cl-def\\(?:generic\\|m\\(?:acro\\|ethod\\)\\|s\\(?:\\(?:truc\\|ubs\\)t\\)\\|type\\|un\\)\\|def\\(?:a\\(?:dvice\\|lias\\)\\|c\\(?:lass\\|onst\\|ustom\\)\\|face\\|g\\(?:eneric\\|roup\\)\\|ine-\\(?:advice\\|derived-mode\\|error\\|g\\(?:\\(?:eneric\\|lobalized-minor\\)-mode\\)\\|inline\\|minor-mode\\|skeleton\\|widget\\)\\|m\\(?:acro\\|ethod\\)\\|subst\\|theme\\|un\\|var\\(?:-local\\|alias\\)?\\)\\|ert-deftest\\)\\_>[ \t']*\\(([ \t']*\\)?\\(\\(setf\\)[ \t]+\\(?:\\w\\|\\s_\\|\\\\.\\)+\\|\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    ("sexp-head-kw", "(\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)\\_>"),
    (
        "autoload-cookie",
        "^;;;###\\(\\([-[:alnum:]]+?\\)-\\)?\\(autoload\\)",
    ),
    (
        "el-errors",
        "(\\(cl-\\(?:assert\\|check-type\\)\\|error\\|signal\\|user-error\\|warn\\)\\_>",
    ),
    (
        "catch-throw",
        "(\\(catch\\|throw\\|featurep\\|provide\\|require\\)\\_>[ \t']*\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    (
        "quoted-symbol",
        "[`\u{2018}']\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)['\u{2019}]",
    ),
    ("keyword-colon", "\\_<:\\(?:\\w\\|\\s_\\|\\\\.\\)+\\_>"),
    ("backslash-esc", "\\(\\\\\\)\\([^\"\\]\\)"),
];

/// First ~256 KiB of `lisp/subr.el`, cut at a char boundary.
fn regex_bench_haystack() -> String {
    let path = crate::test_utils::workspace_root()
        .join("lisp/subr.el")
        .to_string_lossy()
        .into_owned();
    let text = std::fs::read_to_string(path).expect("read lisp/subr.el haystack");
    let mut end = text.len().min(256 * 1024);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn regex_bench_min(iters: u32, mut f: impl FnMut()) -> std::time::Duration {
    // Warm once (compile caches, page in the haystack).
    f();
    let mut best = std::time::Duration::MAX;
    for _ in 0..iters {
        let t = std::time::Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

fn regex_bench_compile(pattern: &str, case_fold: bool) -> regex_emacs::CompiledPattern {
    regex_emacs::regex_compile_lisp(&LispString::from_utf8(pattern), false, case_fold)
        .expect("bench pattern should compile")
}

/// Emulate the font-lock `(while (re-search-forward P nil t))` loop over
/// `text` with the already-compiled pattern; returns the match count.
fn regex_bench_engine_scan(cp: &regex_emacs::CompiledPattern, text: &[u8]) -> usize {
    let syn = DefaultSyntaxLookup;
    let mut n = 0usize;
    let mut at = 0usize;
    while at <= text.len() {
        let Some((_pos, regs)) =
            regex_emacs::re_search(cp, text, at, (text.len() - at) as isize, &syn, at)
        else {
            break;
        };
        n += 1;
        let end = regs.end[0].max(0) as usize;
        if end > at {
            at = end;
        } else {
            // Zero-width match: advance one char like `re-search-forward`.
            match next_search_char_boundary(text, end) {
                Some(next) if next > at => at = next,
                _ => break,
            }
        }
    }
    n
}

fn regex_bench_elisp_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1024);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

/// Syntax lookup with `-` promoted to a word constituent (as lisp modes
/// do via `modify-syntax-entry`) — exercises the buffer-table rebake
/// path of the fastmap without needing a full evaluator.
struct DashWordSyntaxLookup;

impl regex_emacs::SyntaxLookup for DashWordSyntaxLookup {
    fn char_syntax(&self, c: char) -> neovm_text::emacs_core::syntax::SyntaxClass {
        if c == '-' {
            neovm_text::emacs_core::syntax::SyntaxClass::Word
        } else {
            neovm_text::emacs_core::syntax::standard_syntax_class_for_char(c)
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> regex_emacs::SyntaxCacheKey {
        // Test-only: distinct from `Standard` so it can never satisfy a
        // standard-baked cache entry.
        regex_emacs::SyntaxCacheKey::External {
            id: usize::MAX - 1,
            epoch: 0,
        }
    }
}

/// Run `re_search` twice — fastmap enabled vs force-disabled — and
/// assert identical results (match position + full match-data
/// registers) over forward-from-0, forward-from-middle, and backward
/// spans.
fn assert_fastmap_equivalence(
    label: &str,
    cp: &regex_emacs::CompiledPattern,
    syn: &dyn regex_emacs::SyntaxLookup,
    text: &[u8],
) {
    let mid = text.len() / 2;
    let spans: [(usize, isize); 3] = [
        (0, text.len() as isize),
        (mid, (text.len() - mid) as isize),
        (text.len(), -(text.len() as isize)),
    ];
    for &(start, range) in &spans {
        let normal = regex_emacs::re_search(cp, text, start, range, syn, start);
        let forced = regex_emacs::with_fastmap_disabled(|| {
            regex_emacs::re_search(cp, text, start, range, syn, start)
        });
        match (&normal, &forced) {
            (None, None) => {}
            (Some((p1, r1)), Some((p2, r2))) => {
                assert_eq!(
                    p1, p2,
                    "{label}: match position diverged at start={start} range={range}"
                );
                assert_eq!(
                    r1.start, r2.start,
                    "{label}: register starts diverged at start={start} range={range}"
                );
                assert_eq!(
                    r1.end, r2.end,
                    "{label}: register ends diverged at start={start} range={range}"
                );
            }
            (n, f) => panic!(
                "{label}: fastmap changed match existence at start={start} range={range}: \
                 with-fastmap={:?} without-fastmap={:?}",
                n.as_ref().map(|(p, _)| *p),
                f.as_ref().map(|(p, _)| *p),
            ),
        }
    }
}

fn commit_test_search_success(
    success: Option<BufferSearchSuccess>,
    match_data: &mut Option<MatchData>,
) -> Option<usize> {
    success.map(|success| {
        let (_buffer_id, point, published_match_data) = success.into_parts();
        *match_data = Some(published_match_data);
        point.get()
    })
}

fn make_fragmented_search_buffer(kind: BufferTextBackendKind) -> Buffer {
    let mut buf = make_test_buffer_with_backend("α foo\nBeta 123\nγ foo42\nomega", kind);

    let first_fragment = "α ".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(first_fragment));
    buf.insert("tmp");
    buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        first_fragment,
        first_fragment + "tmp".len(),
    ));

    let second_fragment = "α foo\nBeta ".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(second_fragment));
    buf.insert("xx");
    buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        second_fragment,
        second_fragment + "xx".len(),
    ));

    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    assert_eq!(buf.buffer_string(), "α foo\nBeta 123\nγ foo42\nomega");
    assert_eq!(buf.text_backend_kind(), kind);
    buf
}

fn make_unibyte_search_buffer(kind: BufferTextBackendKind) -> Buffer {
    let implemented_kind = kind.implemented().expect("test backend is implemented");
    let mut buf = Buffer::new_with_text_backend_kind(
        BufferId(1),
        Value::string("raw"),
        implemented_kind,
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&LispString::from_unibyte(vec![0xFF, b'a', b'b', 0x80]));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    assert_eq!(buf.text_backend_kind(), kind);
    buf
}
