//! TUI comparison tests: replace sort.

mod support;
use std::time::Duration;
use support::*;

// ── Tests ──────────────────────────────────────────────────
#[test]
fn query_replace_via_mx_accepts_all_matches_and_saves() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "query-replace.txt",
        "foo one\nfoo two\nbar\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "query-replace");
    let from_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Query replace") && !row.contains("with:"))
    };
    gnu.read_until(Duration::from_secs(6), from_prompt);
    neo.read_until(Duration::from_secs(8), from_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"foo");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let to_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("with:"));
    gnu.read_until(Duration::from_secs(6), to_prompt);
    neo.read_until(Duration::from_secs(8), to_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"baz");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let replacement_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Query replacing") || row.contains("Replace"))
    };
    gnu.read_until(Duration::from_secs(6), replacement_prompt);
    neo.read_until(Duration::from_secs(8), replacement_prompt);
    send_both_raw(&mut gnu, &mut neo, b"!");

    let replaced = |grid: &[String]| {
        grid.iter().any(|row| row.contains("baz one"))
            && grid.iter().any(|row| row.contains("baz two"))
    };
    gnu.read_until(Duration::from_secs(6), replaced);
    neo.read_until(Duration::from_secs(8), replaced);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "query_replace_via_mx_accepts_all_matches_and_saves",
        &gnu,
        &neo,
        2,
    );

    save_current_file_and_assert_contents(
        "query_replace_via_mx_accepts_all_matches_and_saves",
        &mut gnu,
        &mut neo,
        "query-replace.txt",
        "baz one\nbaz two\nbar\n",
    );
    assert_home_file_contents(&gnu, &neo, "query-replace.txt", "baz one\nbaz two\nbar\n");
}

#[test]
fn query_replace_via_mpercent_bang() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "query-replace.txt",
        "alpha one\nalpha two\nalpha three\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-%");
    let from_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Query replace"));
    gnu.read_until(Duration::from_secs(6), from_ready);
    neo.read_until(Duration::from_secs(8), from_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let to_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("with:")) || grid.iter().any(|row| row.contains("with "))
    };
    gnu.read_until(Duration::from_secs(6), to_ready);
    neo.read_until(Duration::from_secs(8), to_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"omega");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let query_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Query replacing"))
            && grid.iter().any(|row| row.contains("(y/n/!/q/?)"))
    };
    gnu.read_until(Duration::from_secs(6), query_ready);
    neo.read_until(Duration::from_secs(8), query_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "!");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("omega one"))
            && grid.iter().any(|row| row.contains("omega two"))
            && grid.iter().any(|row| row.contains("omega three"))
            && !grid.iter().any(|row| row.contains("alpha one"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("query_replace_via_mpercent_bang", &gnu, &neo, 2);
}

#[test]
fn query_replace_empty_from_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "query-replace-empty-del.txt",
        "alpha one\nalpha two\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-%");
    let from_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Query replace"));
    gnu.read_until(Duration::from_secs(6), from_ready);
    neo.read_until(Duration::from_secs(8), from_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Query replace"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty query-replace prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "query_replace_empty_from_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn query_replace_question_mark_shows_query_help_without_replacing() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "query-replace-help.txt",
        "alpha one\nalpha two\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-%");
    let from_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Query replace"));
    gnu.read_until(Duration::from_secs(6), from_ready);
    neo.read_until(Duration::from_secs(8), from_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let to_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("with:")) || grid.iter().any(|row| row.contains("with "))
    };
    gnu.read_until(Duration::from_secs(6), to_ready);
    neo.read_until(Duration::from_secs(8), to_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"omega");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let query_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Query replacing"))
            && grid.iter().any(|row| row.contains("(y/n/!/q/?)"))
    };
    gnu.read_until(Duration::from_secs(6), query_ready);
    neo.read_until(Duration::from_secs(8), query_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "?");
    let help_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Type SPC or y to replace one match"))
            || grid
                .iter()
                .any(|row| row.contains("Delete or n to skip to next"))
    };
    gnu.read_until(Duration::from_secs(6), help_ready);
    neo.read_until(Duration::from_secs(8), help_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            help_ready(&grid),
            "{label} should show query-replace help after ?\n{}",
            grid.join("\n")
        );
        assert!(
            grid.iter().any(|row| row.contains("alpha one"))
                && !grid.iter().any(|row| row.contains("omega one")),
            "{label} should not replace text when only query-replace help was requested\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "query_replace_question_mark_shows_query_help_without_replacing",
        &gnu,
        &neo,
        4,
    );
}

#[test]
fn query_replace_keyboard_quit_leaves_buffer_unchanged() {
    let (mut gnu, mut neo) = boot_pair("");
    let name = "query-replace-quit.txt";
    let contents = "alpha one\nalpha two\n";

    open_home_file(&mut gnu, &mut neo, name, contents, "C-x C-f");

    send_both(&mut gnu, &mut neo, "M-%");
    let from_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Query replace"));
    gnu.read_until(Duration::from_secs(6), from_ready);
    neo.read_until(Duration::from_secs(8), from_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let to_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("with:")) || grid.iter().any(|row| row.contains("with "))
    };
    gnu.read_until(Duration::from_secs(6), to_ready);
    neo.read_until(Duration::from_secs(8), to_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"omega");
    }
    send_both(&mut gnu, &mut neo, "C-g");

    let unchanged = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha one"))
            && grid.iter().any(|row| row.contains("alpha two"))
            && !grid.iter().any(|row| row.contains("omega one"))
            && grid
                .iter()
                .all(|row| !row.contains("Query replace") && !row.contains("with:"))
    };
    gnu.read_until(Duration::from_secs(6), unchanged);
    neo.read_until(Duration::from_secs(8), unchanged);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "query_replace_keyboard_quit_leaves_buffer_unchanged",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "query_replace_keyboard_quit_leaves_buffer_unchanged",
        &mut gnu,
        &mut neo,
        name,
        contents,
    );
}

#[test]
fn replace_string_via_mx_replaces_from_point_to_end() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "replace-string.txt",
        "alpha one\nbeta one\none tail\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "replace-string");

    let from_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Replace string"));
    gnu.read_until(Duration::from_secs(6), from_prompt);
    neo.read_until(Duration::from_secs(8), from_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"one");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let to_prompt = |grid: &[String]| {
        grid.iter().any(|row| row.contains("with:")) || grid.iter().any(|row| row.contains("with "))
    };
    gnu.read_until(Duration::from_secs(6), to_prompt);
    neo.read_until(Duration::from_secs(8), to_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"uno");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha uno"))
            && grid.iter().any(|row| row.contains("beta uno"))
            && grid.iter().any(|row| row.contains("uno tail"))
            && !grid.iter().any(|row| row.contains("alpha one"))
            && !grid.iter().any(|row| row.contains("beta one"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "replace_string_via_mx_replaces_from_point_to_end",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn replace_string_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "replace-string-empty-del.txt",
        "alpha one\nbeta one\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "replace-string");
    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Replace string"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Replace string"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty replace-string prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "replace_string_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn replace_regexp_via_mx_replaces_numbers_from_point_to_end() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "replace-regexp.txt",
        "item-101\nitem-202\nplain\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "replace-regexp");

    let from_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Replace regexp"));
    gnu.read_until(Duration::from_secs(6), from_prompt);
    neo.read_until(Duration::from_secs(8), from_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"[0-9][0-9]*");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let to_prompt = |grid: &[String]| {
        grid.iter().any(|row| row.contains("with:")) || grid.iter().any(|row| row.contains("with "))
    };
    gnu.read_until(Duration::from_secs(6), to_prompt);
    neo.read_until(Duration::from_secs(8), to_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"NUM");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("item-NUM"))
            && grid.iter().any(|row| row.contains("plain"))
            && !grid.iter().any(|row| row.contains("item-101"))
            && !grid.iter().any(|row| row.contains("item-202"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "replace_regexp_via_mx_replaces_numbers_from_point_to_end",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn replace_regexp_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "replace-regexp-empty-del.txt",
        "item-101\nitem-202\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "replace-regexp");
    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Replace regexp"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Replace regexp"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty replace-regexp prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "replace_regexp_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn align_regexp_via_mx_aligns_equals_in_region() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "align-regexp.txt",
        "aa=1\nlong=2\nb=3\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "align-regexp");

    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Align regexp:"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"=");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("aa   =1"))
            && grid.iter().any(|row| row.contains("long =2"))
            && grid.iter().any(|row| row.contains("b    =3"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("align_regexp_via_mx_aligns_equals_in_region", &gnu, &neo, 2);
    save_current_file_and_assert_contents(
        "align_regexp_via_mx_aligns_equals_in_region",
        &mut gnu,
        &mut neo,
        "align-regexp.txt",
        "aa\t=1\nlong\t=2\nb\t=3\n",
    );
}

#[test]
fn align_regexp_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "align-regexp-empty-del.txt",
        "aa=1\nlong=2\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "align-regexp");

    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Align regexp:"));
    gnu.read_until(Duration::from_secs(6), prompt);
    neo.read_until(Duration::from_secs(8), prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Align regexp:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty align-regexp prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "align_regexp_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn sort_lines_region_via_mx_orders_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-lines.txt",
        "delta\nalpha\ncharlie\nbravo\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-lines");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(alpha) = text.find("alpha") else {
            return false;
        };
        let Some(bravo) = text.find("bravo") else {
            return false;
        };
        let Some(charlie) = text.find("charlie") else {
            return false;
        };
        let Some(delta) = text.find("delta") else {
            return false;
        };
        alpha < bravo && bravo < charlie && charlie < delta
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("sort_lines_region_via_mx_orders_lines", &gnu, &neo, 2);
}

#[test]
fn sort_pages_region_via_mx_orders_pages() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-pages.txt",
        "zeta page\nz body\n\x0calpha page\na body\n\x0cmango page\nm body\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-pages");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(alpha) = text.find("alpha page") else {
            return false;
        };
        let Some(mango) = text.find("mango page") else {
            return false;
        };
        let Some(zeta) = text.find("zeta page") else {
            return false;
        };
        alpha < mango && mango < zeta
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("sort_pages_region_via_mx_orders_pages", &gnu, &neo, 2);
    save_current_file_and_assert_contents(
        "sort_pages_region_via_mx_orders_pages",
        &mut gnu,
        &mut neo,
        "sort-pages.txt",
        "alpha page\na body\n\x0cmango page\nm body\nzeta page\nz body\n\x0c\n",
    );
}

#[test]
fn sort_regexp_fields_via_mx_orders_lines_by_numeric_text_key() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-regexp-fields.txt",
        "id 20 pear\nid 03 apple\nid 11 banana\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-regexp-fields");

    let record_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Regexp specifying records to sort:"))
    };
    gnu.read_until(Duration::from_secs(6), record_prompt);
    neo.read_until(Duration::from_secs(8), record_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"^.*$");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let key_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Regexp specifying key within record:"))
    };
    gnu.read_until(Duration::from_secs(6), key_prompt);
    neo.read_until(Duration::from_secs(8), key_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(br#"\([0-9][0-9]\)"#);
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(three) = text.find("id 03 apple") else {
            return false;
        };
        let Some(eleven) = text.find("id 11 banana") else {
            return false;
        };
        let Some(twenty) = text.find("id 20 pear") else {
            return false;
        };
        three < eleven && eleven < twenty
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "sort_regexp_fields_via_mx_orders_lines_by_numeric_text_key",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "sort_regexp_fields_via_mx_orders_lines_by_numeric_text_key",
        &mut gnu,
        &mut neo,
        "sort-regexp-fields.txt",
        "id 03 apple\nid 11 banana\nid 20 pear\n",
    );
}

#[test]
fn reverse_region_via_mx_reverses_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "reverse-region.txt",
        "one\ntwo\nthree\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "reverse-region");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(three) = text.find("three") else {
            return false;
        };
        let Some(two) = text.find("two") else {
            return false;
        };
        let Some(one) = text.find("one") else {
            return false;
        };
        three < two && two < one
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("reverse_region_via_mx_reverses_lines", &gnu, &neo, 2);
    save_current_file_and_assert_contents(
        "reverse_region_via_mx_reverses_lines",
        &mut gnu,
        &mut neo,
        "reverse-region.txt",
        "three\ntwo\none\n",
    );
}

#[test]
fn sort_fields_second_field_via_prefix_orders_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-fields.txt",
        "3 banana\n2 apple\n1 cherry\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h C-u 2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-fields");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(apple) = text.find("2 apple") else {
            return false;
        };
        let Some(banana) = text.find("3 banana") else {
            return false;
        };
        let Some(cherry) = text.find("1 cherry") else {
            return false;
        };
        apple < banana && banana < cherry
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "sort_fields_second_field_via_prefix_orders_lines",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "sort_fields_second_field_via_prefix_orders_lines",
        &mut gnu,
        &mut neo,
        "sort-fields.txt",
        "2 apple\n3 banana\n1 cherry\n",
    );
}

#[test]
fn sort_numeric_fields_second_field_via_prefix_orders_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-numeric-fields.txt",
        "alpha 10\nbravo 2\ncharlie 7\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h C-u 2");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-numeric-fields");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(two) = text.find("bravo 2") else {
            return false;
        };
        let Some(seven) = text.find("charlie 7") else {
            return false;
        };
        let Some(ten) = text.find("alpha 10") else {
            return false;
        };
        two < seven && seven < ten
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "sort_numeric_fields_second_field_via_prefix_orders_lines",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "sort_numeric_fields_second_field_via_prefix_orders_lines",
        &mut gnu,
        &mut neo,
        "sort-numeric-fields.txt",
        "bravo 2\ncharlie 7\nalpha 10\n",
    );
}

#[test]
fn sort_columns_via_mx_orders_lines_by_marked_columns() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-columns.txt",
        "id 20 pear\nid 03 apple\nid 11 banana\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-< C-f C-f C-f C-SPC C-n C-n C-f C-f");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-columns");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(three) = text.find("id 03 apple") else {
            return false;
        };
        let Some(eleven) = text.find("id 11 banana") else {
            return false;
        };
        let Some(twenty) = text.find("id 20 pear") else {
            return false;
        };
        three < eleven && eleven < twenty
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "sort_columns_via_mx_orders_lines_by_marked_columns",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "sort_columns_via_mx_orders_lines_by_marked_columns",
        &mut gnu,
        &mut neo,
        "sort-columns.txt",
        "id 03 apple\nid 11 banana\nid 20 pear\n",
    );
}

#[test]
fn sort_paragraphs_via_mx_orders_paragraph_blocks() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "sort-paragraphs.txt",
        "zeta paragraph\ncontinues here\n\nalpha paragraph\ncontinues here\n\ngamma paragraph\ncontinues here\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "sort-paragraphs");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        let Some(alpha) = text.find("alpha paragraph") else {
            return false;
        };
        let Some(gamma) = text.find("gamma paragraph") else {
            return false;
        };
        let Some(zeta) = text.find("zeta paragraph") else {
            return false;
        };
        alpha < gamma && gamma < zeta
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "sort_paragraphs_via_mx_orders_paragraph_blocks",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "sort_paragraphs_via_mx_orders_paragraph_blocks",
        &mut gnu,
        &mut neo,
        "sort-paragraphs.txt",
        "alpha paragraph\ncontinues here\n\ngamma paragraph\ncontinues here\n\nzeta paragraph\ncontinues here\n",
    );
}

#[test]
fn delete_duplicate_lines_via_mx_keeps_first_occurrences() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-duplicate-lines.txt",
        "alpha\nbeta\nalpha\ngamma\nbeta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "delete-duplicate-lines");

    let ready = |grid: &[String]| {
        let text = grid.join("\n");
        text.contains("alpha")
            && text.contains("beta")
            && text.contains("gamma")
            && grid
                .iter()
                .any(|row| row.contains("Deleted 2 duplicate lines"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_duplicate_lines_via_mx_keeps_first_occurrences",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "delete_duplicate_lines_via_mx_keeps_first_occurrences",
        &mut gnu,
        &mut neo,
        "delete-duplicate-lines.txt",
        "alpha\nbeta\ngamma\n",
    );
}

#[test]
fn flush_lines_via_mx_deletes_matching_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "flush-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "flush-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Flush lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"drop");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("flush_lines_via_mx_deletes_matching_lines", &gnu, &neo, 2);
}

#[test]
fn delete_matching_lines_via_mx_alias_deletes_matching_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-matching-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "delete-matching-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Flush lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"drop");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
            && grid
                .iter()
                .rev()
                .take(4)
                .any(|row| row.contains("Deleted 2 matching lines"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_matching_lines_via_mx_alias_deletes_matching_lines",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn keep_lines_via_mx_preserves_matching_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "keep-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "keep-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Keep lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"keep");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("keep_lines_via_mx_preserves_matching_lines", &gnu, &neo, 2);
}

#[test]
fn delete_non_matching_lines_via_mx_alias_preserves_matching_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "delete-non-matching-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "delete-non-matching-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Keep lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"keep");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "delete_non_matching_lines_via_mx_alias_preserves_matching_lines",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn kill_matching_lines_via_mx_deletes_and_accumulates_for_yank() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "kill-matching-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "kill-matching-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Kill lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"drop");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let source_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
            && grid
                .iter()
                .rev()
                .take(4)
                .any(|row| row.contains("Killed 2 matching lines"))
    };
    gnu.read_until(Duration::from_secs(6), source_ready);
    neo.read_until(Duration::from_secs(8), source_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    let switch_prompt = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("Switch to buffer:"))
    };
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"kill-matching-yank");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let yank_buffer = |grid: &[String]| grid.iter().any(|row| row.contains("kill-matching-yank"));
    gnu.read_until(Duration::from_secs(6), yank_buffer);
    neo.read_until(Duration::from_secs(8), yank_buffer);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-y");
    let yanked = |grid: &[String]| {
        grid.iter().any(|row| row.contains("drop beta"))
            && grid.iter().any(|row| row.contains("drop delta"))
            && !grid.iter().any(|row| row.contains("keep alpha"))
            && !grid.iter().any(|row| row.contains("keep gamma"))
    };
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "kill_matching_lines_via_mx_deletes_and_accumulates_for_yank",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn copy_matching_lines_via_mx_accumulates_matches_for_yank() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "copy-matching-lines.txt",
        "keep alpha\ndrop beta\nkeep gamma\ndrop delta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "copy-matching-lines");

    let regexp_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Copy lines containing match"))
    };
    gnu.read_until(Duration::from_secs(6), regexp_prompt);
    neo.read_until(Duration::from_secs(8), regexp_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    for session in [&mut gnu, &mut neo] {
        session.send(b"keep");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let copied = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("Copied 2 matching lines"))
    };
    gnu.read_until(Duration::from_secs(6), copied);
    neo.read_until(Duration::from_secs(8), copied);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    let switch_prompt = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("Switch to buffer:"))
    };
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for session in [&mut gnu, &mut neo] {
        session.send(b"copy-matching-yank");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let yank_buffer = |grid: &[String]| grid.iter().any(|row| row.contains("copy-matching-yank"));
    gnu.read_until(Duration::from_secs(6), yank_buffer);
    neo.read_until(Duration::from_secs(8), yank_buffer);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-y");
    let yanked = |grid: &[String]| {
        grid.iter().any(|row| row.contains("keep alpha"))
            && grid.iter().any(|row| row.contains("keep gamma"))
            && !grid.iter().any(|row| row.contains("drop beta"))
            && !grid.iter().any(|row| row.contains("drop delta"))
    };
    gnu.read_until(Duration::from_secs(6), yanked);
    neo.read_until(Duration::from_secs(8), yanked);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "copy_matching_lines_via_mx_accumulates_matches_for_yank",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn how_many_via_mx_reports_regexp_match_count() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "how-many.txt",
        "foo\nbar foo\nfoo\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "how-many");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("How many matches for regexp"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"foo");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("3 occurrences"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .rev()
                .take(4)
                .any(|row| row.contains("3 occurrences")),
            "{label} should report three regexp matches in the echo area"
        );
    }
    assert_pair_nearly_matches("how_many_via_mx_reports_regexp_match_count", &gnu, &neo, 2);
}

#[test]
fn count_matches_via_mx_alias_reports_regexp_match_count() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "count-matches.txt",
        "alpha\nalpha beta\nbeta\nalpha\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "count-matches");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("How many matches for regexp"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"alpha");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .rev()
            .take(4)
            .any(|row| row.contains("3 occurrences"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter()
                .rev()
                .take(4)
                .any(|row| row.contains("3 occurrences")),
            "{label} should report three regexp matches through count-matches"
        );
    }
    assert_pair_nearly_matches(
        "count_matches_via_mx_alias_reports_regexp_match_count",
        &gnu,
        &neo,
        2,
    );
}
