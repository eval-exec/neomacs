//! TUI comparison tests: search.

mod support;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn file_name_shadow_overlay_does_not_leak_into_occur_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x C-f");
    let file_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Find file:"));
    gnu.read_until(Duration::from_secs(6), file_prompt);
    neo.read_until(Duration::from_secs(8), file_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"/tmp//shadow-probe");
    }
    let shadow_ready = |grid: &[String]| grid.iter().any(|row| row.contains("}"));
    gnu.read_until(Duration::from_secs(6), shadow_ready);
    neo.read_until(Duration::from_secs(8), shadow_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    abort_minibuffer_and_wait_for_scratch(&mut gnu, &mut neo);

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let prompt_row = session
            .text_grid()
            .into_iter()
            .find(|row| row.contains("List lines matching regexp"))
            .expect("occur prompt should be visible");
        assert!(
            !prompt_row.contains('{') && !prompt_row.contains('}'),
            "{label} occur prompt should not leak file-name shadow brackets\n{prompt_row}",
        );
    }

    assert_pair_nearly_matches(
        "file_name_shadow_overlay_does_not_leak_into_occur_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn occur_via_ms_o_lists_matching_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-usage.txt",
        "alpha needle one\nbeta plain\ngamma needle two\nneedle three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"needle");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("3 matches"))
            && grid.iter().any(|row| row.contains("alpha needle one"))
            && grid.iter().any(|row| row.contains("gamma needle two"))
            && grid.iter().any(|row| row.contains("needle three"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("occur_via_ms_o_lists_matching_lines", &gnu, &neo, 2);
}

#[test]
fn occur_prompt_ctrl_h_preserves_regexp_text() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-ctrl-h.txt",
        "alpha needle\nbeta plain\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"needleX");
    }
    send_both(&mut gnu, &mut neo, "BS");

    let regexp_preserved = |grid: &[String]| grid.iter().any(|row| row.contains("needleX"));
    gnu.read_until(Duration::from_secs(6), regexp_preserved);
    neo.read_until(Duration::from_secs(8), regexp_preserved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            regexp_preserved(&grid),
            "{label} should keep the previous occur prompt character after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches("occur_prompt_ctrl_h_preserves_regexp_text", &gnu, &neo, 2);
}

#[test]
fn occur_prompt_del_deletes_previous_regexp_character() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-del.txt",
        "alpha needle\nbeta plain\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"needleX");
    }
    send_both(&mut gnu, &mut neo, "DEL RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("1 match"))
            && grid.iter().any(|row| row.contains("alpha needle"))
            && !grid.iter().any(|row| row.contains("No matches"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            ready(&grid),
            "{label} should run occur for corrected regexp after terminal DEL\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "occur_prompt_del_deletes_previous_regexp_character",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn occur_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-empty-del.txt",
        "alpha needle\nbeta plain\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty occur prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "occur_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_forward_via_cs() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "search-usage.txt",
        "alpha line\nbeta target\nomega line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"target");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("search-usage.txt"))
            && grid.iter().any(|row| row.contains("beta target"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("isearch_forward_via_cs", &gnu, &neo, 2);
}

#[test]
fn isearch_repeat_forward_via_cs_cs_moves_to_next_match() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-repeat.txt",
        "needle first\nmiddle line\nneedle second\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-s");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"needle");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-s RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("needle first"))
            && grid.iter().any(|row| row.contains("needleX second"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_repeat_forward_via_cs_cs_moves_to_next_match",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_delete_char_recovers_from_failed_search() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-delete-char.txt",
        "alpha target\nomega line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-s");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"targetx");
    }
    let failing_search = |grid: &[String]| grid.iter().any(|row| row.contains("Failing I-search"));
    gnu.read_until(Duration::from_secs(6), failing_search);
    neo.read_until(Duration::from_secs(8), failing_search);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha target!"))
            && grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_delete_char_recovers_from_failed_search",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_ctrl_h_enters_isearch_help() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-ctrl-h.txt",
        "alpha target\nomega line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-s");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"targetx");
    }
    let failing_search = |grid: &[String]| grid.iter().any(|row| row.contains("Failing I-search"));
    gnu.read_until(Duration::from_secs(6), failing_search);
    neo.read_until(Duration::from_secs(8), failing_search);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "BS");
    let help_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Type ? for further options"))
    };
    gnu.read_until(Duration::from_secs(6), help_ready);
    neo.read_until(Duration::from_secs(8), help_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|row| row.contains("Type ? for further options")),
            "{label} should enter isearch help after terminal C-h\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches("isearch_ctrl_h_enters_isearch_help", &gnu, &neo, 2);
}

#[test]
fn isearch_backward_via_cr() {
    let (mut gnu, mut neo) = boot_pair("");
    let mut contents = String::new();
    for line in 1..=40 {
        if line == 5 {
            contents.push_str("needle target\n");
        } else {
            contents.push_str(&format!("filler line {line:02}\n"));
        }
    }
    open_home_file(
        &mut gnu,
        &mut neo,
        "reverse-search.txt",
        &contents,
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-r");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"needle");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("needle target"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("isearch_backward_via_cr", &gnu, &neo, 2);
}

#[test]
fn isearch_forward_word_via_ms_w_matches_words_across_whitespace() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-word.txt",
        "intro line\nalpha\n   beta target\nalpha-x beta miss\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("word I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha beta");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("beta! target"))
            && grid.iter().any(|row| row.contains("alpha-x beta miss"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_forward_word_via_ms_w_matches_words_across_whitespace",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_forward_symbol_via_ms_underscore_respects_symbol_boundaries() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-symbol.el",
        "foo_bar\nfoo done\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s _");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("symbol I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"foo");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("foo_bar"))
            && grid.iter().any(|row| row.contains("foo! done"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_forward_symbol_via_ms_underscore_respects_symbol_boundaries",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_forward_regexp_via_cmeta_s_matches_pattern() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-regexp-forward.txt",
        "alpha 123 target\nalpha abc target\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-M-s");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Regexp I-search"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(br"alpha [0-9]+ target");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha 123! target"))
            && grid.iter().any(|row| row.contains("alpha abc target"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_forward_regexp_via_cmeta_s_matches_pattern",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn isearch_backward_regexp_via_cmeta_r_matches_previous_pattern() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-regexp-backward.txt",
        "item alpha\nplain middle\nitem beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-M-r");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Regexp I-search backward"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(br"item .+");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("item alpha"))
            && grid.iter().any(|row| row.contains("item beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "isearch_backward_regexp_via_cmeta_r_matches_previous_pattern",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn occur_next_error_via_mg_n_visits_match() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-next-error.txt",
        "alpha needle one\nbeta plain\ngamma needle two\nneedle three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"needle");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let occur_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("3 matches"))
            && grid.iter().any(|row| row.contains("alpha needle one"))
            && grid.iter().any(|row| row.contains("gamma needle two"))
            && grid.iter().any(|row| row.contains("needle three"))
    };
    gnu.read_until(Duration::from_secs(8), occur_ready);
    neo.read_until(Duration::from_secs(12), occur_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "M-g n");
    let jumped_to_source = |grid: &[String]| {
        !grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("alpha needle one"))
            && grid.iter().any(|row| row.contains("beta plain"))
    };
    gnu.read_until(Duration::from_secs(6), jumped_to_source);
    neo.read_until(Duration::from_secs(8), jumped_to_source);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both_raw(&mut gnu, &mut neo, b"X");
    let edited_ready = |grid: &[String]| {
        !grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("beta plain"))
            && grid.iter().any(|row| row.contains("Xneedle"))
    };
    gnu.read_until(Duration::from_secs(6), edited_ready);
    neo.read_until(Duration::from_secs(8), edited_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !edited_ready(&gnu.text_grid()) || !edited_ready(&neo.text_grid()) {
        dump_pair_grids("occur_next_error_via_mg_n_visits_match", &gnu, &neo);
    }

    assert_pair_nearly_matches("occur_next_error_via_mg_n_visits_match", &gnu, &neo, 3);
}

#[test]
fn occur_next_and_previous_error_via_cx_backtick_mg_p() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "occur-prev-error.txt",
        "needle first\nplain middle\nneedle second\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-s o");
    let occur_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("List lines matching regexp"))
    };
    gnu.read_until(Duration::from_secs(6), occur_prompt);
    neo.read_until(Duration::from_secs(8), occur_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"needle");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let occur_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("2 matches"))
            && grid.iter().any(|row| row.contains("needle first"))
            && grid.iter().any(|row| row.contains("needle second"))
    };
    gnu.read_until(Duration::from_secs(8), occur_ready);
    neo.read_until(Duration::from_secs(12), occur_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x `");
    let first_match = |grid: &[String]| {
        !grid.iter().any(|row| row.contains("*Occur*"))
            && grid.iter().any(|row| row.contains("needle first"))
            && grid.iter().any(|row| row.contains("plain middle"))
    };
    gnu.read_until(Duration::from_secs(6), first_match);
    neo.read_until(Duration::from_secs(8), first_match);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-g n");
    let second_match = |grid: &[String]| {
        grid.iter().any(|row| row.contains("plain middle"))
            && grid.iter().any(|row| row.contains("needle second"))
    };
    gnu.read_until(Duration::from_secs(6), second_match);
    neo.read_until(Duration::from_secs(8), second_match);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    send_both(&mut gnu, &mut neo, "M-g p");
    gnu.read_until(Duration::from_secs(6), first_match);
    neo.read_until(Duration::from_secs(8), first_match);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, b"!");

    let edited_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("!needle first"))
            && grid.iter().any(|row| row.contains("needle second"))
    };
    gnu.read_until(Duration::from_secs(6), edited_ready);
    neo.read_until(Duration::from_secs(8), edited_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "occur_next_and_previous_error_via_cx_backtick_mg_p",
        &gnu,
        &neo,
        3,
    );
}

#[test]
fn isearch_forward() {
    let (mut gnu, mut neo) = boot_pair("");
    send_both(&mut gnu, &mut neo, "C-s");
    let prompt_ready = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("search") || row.contains("I-search"))
    };
    gnu.read_until(Duration::from_secs(4), prompt_ready);
    neo.read_until(Duration::from_secs(6), prompt_ready);
    for s in [&mut gnu, &mut neo] {
        s.send(b"buffer");
    }
    let query_ready = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("search") || row.contains("buffer"))
    };
    gnu.read_until(Duration::from_secs(4), query_ready);
    neo.read_until(Duration::from_secs(6), query_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let gl = gnu.text_grid();
    let nl = neo.text_grid();

    // Echo area should show "I-search: buffer" or similar
    let gnu_echo = gl.last().unwrap();
    let neo_echo = nl.last().unwrap();
    assert!(
        gnu_echo.contains("search") || gnu_echo.contains("buffer"),
        "GNU should show isearch: {gnu_echo:?}"
    );
    assert!(
        neo_echo.contains("search") || neo_echo.contains("buffer"),
        "NEO should show isearch: {neo_echo:?}"
    );

    send_both(&mut gnu, &mut neo, "C-g");
}

#[test]
fn isearch_yank_word_via_cs_cw_appends_word_at_point_to_search() {
    let (mut gnu, mut neo) = boot_pair("");

    open_home_file(
        &mut gnu,
        &mut neo,
        "isearch-yank-word.txt",
        "alpha beta gamma delta\nepsilon zeta eta theta\n",
        "C-x C-f",
    );

    // Search for "alpha" to put isearch in a known state
    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    send_both_raw(&mut gnu, &mut neo, b"alpha");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // RET to exit isearch at the match, then go to next word
    send_both(&mut gnu, &mut neo, "RET M-f");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    // C-s C-w should yank "beta" into search
    send_both(&mut gnu, &mut neo, "C-s C-w");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("beta") && row.contains("I-search"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("I-search")),
            "{label}: should show I-search prompt after C-s C-w"
        );
    }

    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
}

#[test]
fn isearch_forward_for_word_then_repeat_via_cs_confirm() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "isearch-repeat.txt";
    let content = "alpha beta alpha gamma alpha\n";

    open_home_file(&mut gnu, &mut neo, name, content, "C-x C-f");
    // C-s alpha
    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for s in [&mut gnu, &mut neo] {
        s.send(b"alpha");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .any(|r| r.contains("I-search: alpha") || r.contains("alpha")),
            "{label}: isearch should find alpha"
        );
    }
}

#[test]
fn highlight_regexp_via_mx_marks_matches() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "highlight-regexp.txt";
    let initial = "apple banana apple\n";

    open_home_file(&mut gnu, &mut neo, name, initial, "C-x C-f");
    invoke_mx_command(&mut gnu, &mut neo, "highlight-regexp");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(b"apple\r");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    // Accept default face
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("apple")),
            "{label}: buffer should still show apple after highlight-regexp"
        );
    }
}
