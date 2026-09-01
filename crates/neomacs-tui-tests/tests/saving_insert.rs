//! TUI comparison tests: saving insert.

mod support;
use neomacs_tui_tests::*;
use std::fs;
use std::time::Duration;
use support::*;

// ── Local helpers ───────────────────────────────────────────

fn normalize_hello_vc_row(row: &str) -> String {
    if row.contains("Egyptian Hieroglyphs") {
        return row.chars().filter(|ch| !ch.is_whitespace()).collect();
    }

    let Some(start) = row.find("Git-") else {
        return row.to_string();
    };
    let Some(rest) = row.get(start..) else {
        return row.to_string();
    };
    let end = rest
        .find("  (")
        .map(|offset| start + offset)
        .unwrap_or(row.len());
    let target_width = row.chars().count();
    let mut normalized = String::with_capacity(row.len());
    normalized.push_str(&row[..start]);
    normalized.push_str("Git-REV1234");
    normalized.push_str(&row[end..]);
    let normalized: String = normalized.chars().take(target_width).collect();
    // The mode line's trailing dash fill absorbs the VC segment's width, so a
    // branch-name length difference between the two checkouts (each session
    // opens its own repo's etc/HELLO) survives the token replacement as a
    // fill-length difference. Collapse the trailing dash run to a fixed token
    // so the comparison is branch-name-agnostic.
    let trimmed = normalized.trim_end();
    let without_fill = trimmed.trim_end_matches('-');
    if trimmed.len() - without_fill.len() >= 3 {
        return format!("{without_fill}---");
    }
    normalized
}

fn is_known_hello_scroll_diff(diff: &RowDiff) -> bool {
    let rows = [&diff.gnu, &diff.neo];
    rows.iter().any(|row| {
        row.contains("Javanese")
            || row.contains("Lepcha")
            || row.contains("Malayalam")
            || row.contains("Rejang")
            || row.contains("Git-")
            || row.contains("Git-main")
            || row.contains("view-hello-file")
    })
}

// ── Tests ──────────────────────────────────────────────────
#[test]
fn write_file_after_edit_via_cx_cw() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "write-file-source.txt",
        "alpha line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"omega line");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Write file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/write-file-dest.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("write-file-dest.txt"))
            && grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    let expected_dest = "alpha line\nomega line\n";
    let gnu_dest = gnu.home_dir().join("write-file-dest.txt");
    let neo_dest = neo.home_dir().join("write-file-dest.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        let gnu_saved = fs::read_to_string(&gnu_dest).ok().as_deref() == Some(expected_dest);
        let neo_saved = fs::read_to_string(&neo_dest).ok().as_deref() == Some(expected_dest);
        if gnu_saved && neo_saved {
            break;
        }
    }

    assert_eq!(
        fs::read_to_string(gnu.home_dir().join("write-file-source.txt"))
            .expect("read GNU source file"),
        "alpha line\n"
    );
    assert_eq!(
        fs::read_to_string(neo.home_dir().join("write-file-source.txt"))
            .expect("read Neo source file"),
        "alpha line\n"
    );
    assert_eq!(
        fs::read_to_string(&gnu_dest).expect("read GNU write-file dest"),
        expected_dest
    );
    assert_eq!(
        fs::read_to_string(&neo_dest).expect("read Neo write-file dest"),
        expected_dest
    );

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("Wrote ")),
            "{label} screen missing save completion message:\n{}",
            grid.join("\n")
        );
        assert!(
            grid.iter().any(|row| row.contains("write-file-dest.txt")),
            "{label} screen missing destination file name after write-file:\n{}",
            grid.join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "C-l");
    let recentered = |grid: &[String]| {
        grid.iter().any(|row| row.contains("write-file-dest.txt"))
            && grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), recentered);
    neo.read_until(Duration::from_secs(8), recentered);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches("write_file_after_edit_via_cx_cw", &gnu, &neo, 2);
}

#[test]
fn write_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "write-file-empty-del.txt",
        "alpha line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-w");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Write file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "C-a C-k DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Write file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty write-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "write_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn read_only_mode_via_cx_cq_blocks_then_allows_insertion() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "read-only-toggle.txt",
        "alpha beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x C-q");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("alpha beta"))
                && !grid.iter().any(|row| row.contains("Xalpha beta")),
            "{label} should not insert into a read-only buffer:\n{}",
            grid.join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "C-g C-x C-q");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both_raw(&mut gnu, &mut neo, b"X");

    let inserted = |grid: &[String]| grid.iter().any(|row| row.contains("Xalpha beta"));
    gnu.read_until(Duration::from_secs(6), inserted);
    neo.read_until(Duration::from_secs(8), inserted);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("Xalpha beta"))
                && !grid.iter().any(|row| row.contains("XXalpha beta")),
            "{label} should insert exactly once after read-only-mode is disabled:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_nearly_matches(
        "read_only_mode_via_cx_cq_blocks_then_allows_insertion",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn set_visited_file_name_via_mx_saves_current_buffer_under_new_name() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "set-visited-source.txt",
        "visited file body\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "set-visited-file-name");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Set visited file name:"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/set-visited-dest.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let renamed_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("set-visited-dest.txt"))
            && grid.iter().any(|row| row.contains("visited file body"))
    };
    gnu.read_until(Duration::from_secs(6), renamed_ready);
    neo.read_until(Duration::from_secs(8), renamed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    save_current_file_and_assert_contents(
        "set_visited_file_name_via_mx_saves_current_buffer_under_new_name",
        &mut gnu,
        &mut neo,
        "set-visited-dest.txt",
        "visited file body\n",
    );
    assert_home_file_contents(&gnu, &neo, "set-visited-source.txt", "visited file body\n");

    assert_pair_nearly_matches(
        "set_visited_file_name_via_mx_saves_current_buffer_under_new_name",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn rename_visited_file_via_mx_moves_file_and_updates_buffer() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "rename-visited-source.txt",
        "rename visited body\n",
        "C-x C-f",
    );

    invoke_mx_command(&mut gnu, &mut neo, "rename-visited-file");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Rename visited file to:"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/rename-visited-dest.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let gnu_src = gnu.home_dir().join("rename-visited-source.txt");
    let neo_src = neo.home_dir().join("rename-visited-source.txt");
    let gnu_dest = gnu.home_dir().join("rename-visited-dest.txt");
    let neo_dest = neo.home_dir().join("rename-visited-dest.txt");
    let renamed_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("rename-visited-dest.txt"))
            && grid.iter().any(|row| row.contains("rename visited body"))
    };
    for _ in 0..10 {
        gnu.read_until(Duration::from_millis(300), renamed_ready);
        neo.read_until(Duration::from_millis(300), renamed_ready);
        if !gnu_src.exists() && !neo_src.exists() && gnu_dest.exists() && neo_dest.exists() {
            break;
        }
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert!(
        !gnu_src.exists(),
        "GNU rename-visited-file should remove source path"
    );
    assert!(
        !neo_src.exists(),
        "Neomacs rename-visited-file should remove source path"
    );
    assert_eq!(
        fs::read_to_string(&gnu_dest).expect("read GNU rename-visited destination"),
        "rename visited body\n"
    );
    assert_eq!(
        fs::read_to_string(&neo_dest).expect("read Neo rename-visited destination"),
        "rename visited body\n"
    );

    send_both(&mut gnu, &mut neo, "C-l");
    gnu.read_until(Duration::from_secs(6), renamed_ready);
    neo.read_until(Duration::from_secs(8), renamed_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "rename_visited_file_via_mx_moves_file_and_updates_buffer",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn save_buffer_after_edit_via_cx_cs() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "save-usage.txt",
        "alpha line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"omega line");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-s");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("save-usage.txt"))
            && grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_eq!(
        fs::read_to_string(gnu.home_dir().join("save-usage.txt")).expect("read GNU saved file"),
        "alpha line\nomega line\n"
    );
    assert_eq!(
        fs::read_to_string(neo.home_dir().join("save-usage.txt")).expect("read Neo saved file"),
        "alpha line\nomega line\n"
    );
    assert_pair_nearly_matches("save_buffer_after_edit_via_cx_cs", &gnu, &neo, 2);
}

#[test]
fn save_unnamed_buffer_via_cx_cs_prompts_for_file() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x b");
    let switch_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("Switch to buffer:"));
    gnu.read_until(Duration::from_secs(6), switch_prompt);
    neo.read_until(Duration::from_secs(8), switch_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"unnamed-save-buffer");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let buffer_ready = |grid: &[String]| grid.iter().any(|row| row.contains("unnamed-save-buffer"));
    gnu.read_until(Duration::from_secs(6), buffer_ready);
    neo.read_until(Duration::from_secs(8), buffer_ready);

    for session in [&mut gnu, &mut neo] {
        session.send(b"unnamed save line\n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x C-s");
    let save_prompt = |grid: &[String]| grid.iter().any(|row| row.contains("File to save in:"));
    gnu.read_until(Duration::from_secs(6), save_prompt);
    neo.read_until(Duration::from_secs(8), save_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "save_unnamed_buffer_via_cx_cs_prompts_for_file/after-cx-cs",
        &gnu,
        &neo,
        2,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/unnamed-save-buffer.txt");
    }
    let typed_path = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("unnamed-save-buffer.txt"))
    };
    gnu.read_until(Duration::from_secs(6), typed_path);
    neo.read_until(Duration::from_secs(8), typed_path);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "save_unnamed_buffer_via_cx_cs_prompts_for_file/before-ret",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("unnamed-save-buffer.txt"))
            && grid.iter().any(|row| row.contains("unnamed save line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "save_unnamed_buffer_via_cx_cs_prompts_for_file/after-ret",
        &gnu,
        &neo,
        2,
    );

    let expected = "unnamed save line\n";
    let gnu_path = gnu.home_dir().join("unnamed-save-buffer.txt");
    let neo_path = neo.home_dir().join("unnamed-save-buffer.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        let gnu_saved = fs::read_to_string(&gnu_path).ok().as_deref() == Some(expected);
        let neo_saved = fs::read_to_string(&neo_path).ok().as_deref() == Some(expected);
        if gnu_saved && neo_saved {
            break;
        }
    }

    assert_eq!(
        fs::read_to_string(&gnu_path).expect("read GNU unnamed saved file"),
        expected
    );
    assert_eq!(
        fs::read_to_string(&neo_path).expect("read Neo unnamed saved file"),
        expected
    );
    assert_pair_nearly_matches(
        "save_unnamed_buffer_via_cx_cs_prompts_for_file",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn save_some_buffers_after_edit_via_cx_s() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "save-some-usage.txt",
        "alpha line\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"omega line");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x s");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Save file"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "SPC");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("save-some-usage.txt"))
            && grid.iter().any(|row| row.contains("omega line"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    let expected = "alpha line\nomega line\n";
    let gnu_path = gnu.home_dir().join("save-some-usage.txt");
    let neo_path = neo.home_dir().join("save-some-usage.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        let gnu_saved = fs::read_to_string(&gnu_path).ok().as_deref() == Some(expected);
        let neo_saved = fs::read_to_string(&neo_path).ok().as_deref() == Some(expected);
        if gnu_saved && neo_saved {
            break;
        }
    }

    let gnu_saved = fs::read_to_string(&gnu_path).expect("read GNU save-some file");
    let neo_saved = fs::read_to_string(&neo_path).expect("read Neo save-some file");
    if gnu_saved != expected || neo_saved != expected {
        eprintln!(
            "save_some_buffers_after_edit_via_cx_s: GNU file = {:?}",
            gnu_saved
        );
        eprintln!(
            "save_some_buffers_after_edit_via_cx_s: NEO file = {:?}",
            neo_saved
        );
        dump_pair_grids("save_some_buffers_after_edit_via_cx_s", &gnu, &neo);
    }
    assert_eq!(gnu_saved, expected);
    assert_eq!(neo_saved, expected);
    assert_pair_nearly_matches("save_some_buffers_after_edit_via_cx_s", &gnu, &neo, 2);
}

#[test]
fn insert_file_via_cx_i_inserts_contents_at_point() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "insert-source.txt", "inserted alpha\ninserted beta\n");
    write_home_file(&neo, "insert-source.txt", "inserted alpha\ninserted beta\n");
    open_home_file(
        &mut gnu,
        &mut neo,
        "insert-target.txt",
        "target header\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "C-x i");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Insert file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/insert-source.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("target header"))
            && grid.iter().any(|row| row.contains("inserted alpha"))
            && grid.iter().any(|row| row.contains("inserted beta"))
            && grid.iter().any(|row| row.contains("insert-target.txt"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "insert_file_via_cx_i_inserts_contents_at_point",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn insert_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "insert-file-empty-del.txt",
        "target header\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-x i");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Insert file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Insert file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty insert-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "insert_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn insert_file_literally_via_mx_inserts_contents_at_point() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "literal-source.txt", "literal alpha\nliteral beta\n");
    write_home_file(&neo, "literal-source.txt", "literal alpha\nliteral beta\n");
    open_home_file(
        &mut gnu,
        &mut neo,
        "literal-target.txt",
        "literal target\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M->");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "insert-file-literally");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Insert file literally:"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"~/literal-source.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("literal target"))
            && grid.iter().any(|row| row.contains("literal alpha"))
            && grid.iter().any(|row| row.contains("literal beta"))
            && grid.iter().any(|row| row.contains("literal-target.txt"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "insert_file_literally_via_mx_inserts_contents_at_point",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn insert_char_hex_via_cx8ret_inserts_named_character() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(&mut gnu, &mut neo, "insert-char.txt", "alpha\n", "C-x C-f");

    send_both(&mut gnu, &mut neo, "M-> C-x 8 RET");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Insert character (Unicode name or hex):"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"41");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("alphaA"));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "insert_char_hex_via_cx8ret_inserts_named_character",
        &gnu,
        &neo,
        2,
    );
    save_current_file_and_assert_contents(
        "insert_char_hex_via_cx8ret_inserts_named_character",
        &mut gnu,
        &mut neo,
        "insert-char.txt",
        "alpha\nA\n",
    );
}

#[test]
fn insert_char_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "insert-char-empty-del.txt",
        "alpha\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "M-> C-x 8 RET");
    let prompt_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Insert character (Unicode name or hex):"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Insert character (Unicode name or hex):"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty insert-char prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "insert_char_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn view_file_via_mx_opens_view_mode_and_q_quits() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(
        &gnu,
        "view-file.txt",
        "view mode first line\nview mode second line\n",
    );
    write_home_file(
        &neo,
        "view-file.txt",
        "view mode first line\nview mode second line\n",
    );

    invoke_mx_command(&mut gnu, &mut neo, "view-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("View file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "view_file_via_mx_opens_view_mode_and_q_quits/prompt",
        &gnu,
        &neo,
        2,
    );

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/view-file.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("view-file.txt"))
            && grid.iter().any(|row| row.contains("view mode first line"))
            && grid.iter().any(|row| row.contains("View"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "view_file_via_mx_opens_view_mode_and_q_quits/view",
        &gnu,
        &neo,
        2,
    );

    send_both(&mut gnu, &mut neo, "q");
    let scratch_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("view-file.txt"))
    };
    gnu.read_until(Duration::from_secs(6), scratch_ready);
    neo.read_until(Duration::from_secs(8), scratch_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "view_file_via_mx_opens_view_mode_and_q_quits/quit",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn view_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");

    invoke_mx_command(&mut gnu, &mut neo, "view-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("View file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("View file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty view-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "view_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn about_emacs_via_ch_ca_opens_about_buffer() {
    let (mut gnu, mut neo) = boot_pair("");

    send_help_sequence(&mut gnu, &mut neo, "C-a");
    let about_ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*About GNU Emacs*"))
            && grid.iter().any(|row| row.contains("This is GNU Emacs"))
            && grid
                .iter()
                .any(|row| row.contains("ABSOLUTELY NO WARRANTY"))
    };
    gnu.read_until(Duration::from_secs(8), about_ready);
    neo.read_until(Duration::from_secs(12), about_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("*About GNU Emacs*")),
            "{label} should show the About buffer"
        );
        assert!(
            grid.iter().any(|row| row.contains("This is GNU Emacs")),
            "{label} should show the About screen heading"
        );
        assert!(
            grid.iter()
                .any(|row| row.contains("ABSOLUTELY NO WARRANTY")),
            "{label} should show the About screen warranty link text"
        );
    }
    assert_pair_nearly_matches("about_emacs_via_ch_ca_opens_about_buffer", &gnu, &neo, 5);
}

#[test]
fn append_to_file_via_mx_appends_region_to_existing_file() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, "append-to-file-dest.txt", "existing header\n");
    write_home_file(&neo, "append-to-file-dest.txt", "existing header\n");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-to-file-source.txt",
        "region alpha\nregion beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "append-to-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Append to file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"~/append-to-file-dest.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let expected = "existing header\nregion alpha\nregion beta\n";
    let gnu_dest = gnu.home_dir().join("append-to-file-dest.txt");
    let neo_dest = neo.home_dir().join("append-to-file-dest.txt");
    for _ in 0..10 {
        read_both(&mut gnu, &mut neo, Duration::from_millis(300));
        let gnu_saved = fs::read_to_string(&gnu_dest).ok().as_deref() == Some(expected);
        let neo_saved = fs::read_to_string(&neo_dest).ok().as_deref() == Some(expected);
        if gnu_saved && neo_saved {
            break;
        }
    }

    assert_eq!(
        fs::read_to_string(&gnu_dest).expect("read GNU append destination"),
        expected
    );
    assert_eq!(
        fs::read_to_string(&neo_dest).expect("read Neo append destination"),
        expected
    );
    assert_pair_nearly_matches(
        "append_to_file_via_mx_appends_region_to_existing_file",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn append_to_file_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-to-file-empty-del.txt",
        "region alpha\nregion beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "append-to-file");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Append to file:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Append to file:"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty append-to-file prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "append_to_file_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn copy_to_buffer_via_mx_replaces_target_buffer_contents() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "copy-to-buffer-source.txt",
        "copy alpha\ncopy beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "copy-to-buffer");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Copy to buffer"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"copy-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"copy-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("copy-to-buffer-target"))
            && grid.iter().any(|row| row.contains("copy alpha"))
            && grid.iter().any(|row| row.contains("copy beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "copy_to_buffer_via_mx_replaces_target_buffer_contents",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn copy_to_buffer_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "copy-to-buffer-empty-del.txt",
        "copy alpha\ncopy beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "copy-to-buffer");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Copy to buffer"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Copy to buffer"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty copy-to-buffer prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "copy_to_buffer_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn append_to_buffer_via_mx_inserts_region_at_target_point() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-to-buffer-source.txt",
        "append alpha\nappend beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"append-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let target_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("append-to-buffer-target"))
    };
    gnu.read_until(Duration::from_secs(6), target_ready);
    neo.read_until(Duration::from_secs(8), target_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"target header\n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"append-to-buffer-source.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");
    gnu.read_until(Duration::from_secs(6), |grid| {
        grid.iter().any(|row| row.contains("append alpha"))
    });
    neo.read_until(Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("append alpha"))
    });

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "append-to-buffer");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Append to buffer:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"append-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"append-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("target header"))
            && grid.iter().any(|row| row.contains("append alpha"))
            && grid.iter().any(|row| row.contains("append beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "append_to_buffer_via_mx_inserts_region_at_target_point",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn append_to_buffer_empty_prompt_multiple_del_keeps_prompt() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "append-to-buffer-empty-del.txt",
        "append alpha\nappend beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "append-to-buffer");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Append to buffer"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    send_both(&mut gnu, &mut neo, "DEL DEL DEL");

    let prompt_intact = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Append to buffer"))
            && !grid.iter().any(|row| row.contains("*Help*"))
    };
    gnu.read_until(Duration::from_secs(6), prompt_intact);
    neo.read_until(Duration::from_secs(8), prompt_intact);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            prompt_intact(&grid),
            "{label} should keep the empty append-to-buffer prompt intact after repeated terminal DEL keyhits\n{}",
            grid.join("\n")
        );
    }

    assert_pair_nearly_matches(
        "append_to_buffer_empty_prompt_multiple_del_keeps_prompt",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn prepend_to_buffer_via_mx_inserts_region_before_target_text() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "prepend-to-buffer-source.txt",
        "prepend alpha\nprepend beta\n",
        "C-x C-f",
    );

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"prepend-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let target_ready = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("prepend-to-buffer-target"))
    };
    gnu.read_until(Duration::from_secs(6), target_ready);
    neo.read_until(Duration::from_secs(8), target_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"target footer\n");
    }
    send_both(&mut gnu, &mut neo, "M-<");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"prepend-to-buffer-source.txt");
    }
    send_both(&mut gnu, &mut neo, "RET");
    gnu.read_until(Duration::from_secs(6), |grid| {
        grid.iter().any(|row| row.contains("prepend alpha"))
    });
    neo.read_until(Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("prepend alpha"))
    });

    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    invoke_mx_command(&mut gnu, &mut neo, "prepend-to-buffer");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Prepend to buffer:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"prepend-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"prepend-to-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("prepend alpha"))
            && grid.iter().any(|row| row.contains("prepend beta"))
            && grid.iter().any(|row| row.contains("target footer"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "prepend_to_buffer_via_mx_inserts_region_before_target_text",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn insert_buffer_via_cx_x_i_inserts_named_buffer_contents() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"insert-buffer-source");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let source_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("insert-buffer-source"));
    gnu.read_until(Duration::from_secs(6), source_ready);
    neo.read_until(Duration::from_secs(8), source_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"source alpha\nsource beta\n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for session in [&mut gnu, &mut neo] {
        session.send(b"insert-buffer-target");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let target_ready =
        |grid: &[String]| grid.iter().any(|row| row.contains("insert-buffer-target"));
    gnu.read_until(Duration::from_secs(6), target_ready);
    neo.read_until(Duration::from_secs(8), target_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"target header\n");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-x x i");
    let prompt_ready = |grid: &[String]| grid.iter().any(|row| row.contains("Insert buffer:"));
    gnu.read_until(Duration::from_secs(6), prompt_ready);
    neo.read_until(Duration::from_secs(8), prompt_ready);
    for session in [&mut gnu, &mut neo] {
        session.send(b"insert-buffer-source");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("insert-buffer-target"))
            && grid.iter().any(|row| row.contains("target header"))
            && grid.iter().any(|row| row.contains("source alpha"))
            && grid.iter().any(|row| row.contains("source beta"))
    };
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    assert_pair_nearly_matches(
        "insert_buffer_via_cx_x_i_inserts_named_buffer_contents",
        &gnu,
        &neo,
        2,
    );
}

#[test]
fn revert_buffer_via_mx_rereads_file_from_disk() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "revert-usage.txt",
        "original disk line\n",
        "C-x C-f",
    );
    fs::write(
        gnu.home_dir().join("revert-usage.txt"),
        "updated disk line\n",
    )
    .expect("update GNU revert fixture");
    fs::write(
        neo.home_dir().join("revert-usage.txt"),
        "updated disk line\n",
    )
    .expect("update Neo revert fixture");

    send_both(&mut gnu, &mut neo, "M-x");
    let mx_prompt = |grid: &[String]| grid.last().is_some_and(|row| row.contains("M-x"));
    gnu.read_until(Duration::from_secs(6), mx_prompt);
    neo.read_until(Duration::from_secs(8), mx_prompt);
    for session in [&mut gnu, &mut neo] {
        session.send(b"revert-buffer");
    }
    send_both(&mut gnu, &mut neo, "RET");
    let revert_prompt = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("Revert buffer from file"))
    };
    gnu.read_until(Duration::from_secs(6), revert_prompt);
    neo.read_until(Duration::from_secs(8), revert_prompt);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for session in [&mut gnu, &mut neo] {
        session.send(b"yes");
    }
    send_both(&mut gnu, &mut neo, "RET");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("revert-usage.txt"))
            && grid.iter().any(|row| row.contains("updated disk line"))
            && !grid.iter().any(|row| row.contains("original disk line"))
    };
    gnu.read_until(Duration::from_secs(8), ready);
    neo.read_until(Duration::from_secs(12), ready);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches("revert_buffer_via_mx_rereads_file_from_disk", &gnu, &neo, 2);
}

#[test]
fn not_modified_via_mtilde_prevents_next_save_from_writing_edit() {
    let (mut gnu, mut neo) = boot_pair("");
    open_home_file(
        &mut gnu,
        &mut neo,
        "not-modified.txt",
        "original text\n",
        "C-x C-f",
    );

    send_both_raw(&mut gnu, &mut neo, b"changed ");
    let edited = |grid: &[String]| grid.iter().any(|row| row.contains("changed original text"));
    gnu.read_until(Duration::from_secs(6), edited);
    neo.read_until(Duration::from_secs(8), edited);

    send_both(&mut gnu, &mut neo, "M-~ C-x C-s");
    let saved = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("No changes need to be saved"))
    };
    gnu.read_until(Duration::from_secs(6), saved);
    neo.read_until(Duration::from_secs(8), saved);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    assert_pair_nearly_matches(
        "not_modified_via_mtilde_prevents_next_save_from_writing_edit",
        &gnu,
        &neo,
        2,
    );
    assert_home_file_contents(&gnu, &neo, "not-modified.txt", "original text\n");
}

#[test]
fn mx_view_hello_file() {
    // M-x view-hello-file opens the built-in etc/HELLO file (the multilingual
    // "hello" demo). Content includes "Hello, world!" (English row) plus many
    // other-language greetings.
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for s in [&mut gnu, &mut neo] {
        s.send(b"view-hello-file");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");
    // `view-hello-file` runs format-decode → enriched-decode → view-mode
    // setup with pauses between stages, which can exceed the idle-detect
    // threshold. Wait explicitly for the buffer switch (mode-line shows
    // "HELLO") instead of relying on a plain timeout.
    let wants_hello = |rows: &[String]| rows.iter().any(|r| r.contains("HELLO"));
    gnu.read_until(Duration::from_secs(5), wants_hello);
    neo.read_until(Duration::from_secs(5), wants_hello);

    let gl = gnu.text_grid();
    let nl = neo.text_grid();

    // HELLO opens in view-mode and its content varies (many languages). Both
    // editors should at least have "hello" (case-insensitive) somewhere in
    // the visible rows — that covers the explanatory "…write a 'hello' …"
    // opening, the English "Hola" / "Hello" rows, etc.
    let lower_has_hello = |rows: &[String]| rows.iter().any(|r| r.to_lowercase().contains("hello"));
    let gnu_has_hello = lower_has_hello(&gl);
    let neo_has_hello = lower_has_hello(&nl);

    let dump = |label: &str, rows: &[String]| {
        eprintln!("{label} screen:");
        for (i, r) in rows.iter().enumerate() {
            let t = r.trim();
            if !t.is_empty() {
                eprintln!("  {i:2}: |{t}|");
            }
        }
    };
    if !gnu_has_hello {
        dump("GNU", &gl);
    }
    if !neo_has_hello {
        dump("NEO", &nl);
    }
    assert!(
        gnu_has_hello,
        "GNU should show some 'hello' text after M-x view-hello-file"
    );
    assert!(
        neo_has_hello,
        "NEO should show some 'hello' text after M-x view-hello-file"
    );

    // Mode line should surface the buffer name HELLO.
    let gnu_has_name = gl.iter().any(|r| r.contains("HELLO"));
    let neo_has_name = nl.iter().any(|r| r.contains("HELLO"));
    assert!(gnu_has_name, "GNU should show HELLO in the mode line");
    assert!(neo_has_name, "NEO should show HELLO in the mode line");
}

#[test]
fn mx_view_hello_file_page_scroll_repaints_cleanly() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for session in [&mut gnu, &mut neo] {
        session.send(b"view-hello-file");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");

    let wants_hello = |rows: &[String]| rows.iter().any(|row| row.contains("HELLO"));
    gnu.read_until(Duration::from_secs(5), wants_hello);
    neo.read_until(Duration::from_secs(5), wants_hello);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    send_both(&mut gnu, &mut neo, "C-v");
    let paged_down = |rows: &[String]| {
        rows.iter()
            .any(|row| row.contains("Greek") || row.contains("Cyrillic") || row.contains("Hebrew"))
    };
    gnu.read_until(Duration::from_secs(6), paged_down);
    neo.read_until(Duration::from_secs(8), paged_down);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let down_diffs: Vec<_> = meaningful_diffs(diff_text_grids(&gnu.text_grid(), &neo.text_grid()))
        .into_iter()
        .filter(|diff| !is_known_hello_scroll_diff(diff))
        .collect();
    if !down_diffs.is_empty() {
        eprintln!(
            "mx_view_hello_file_page_scroll_repaints_cleanly/down: {} rows differ",
            down_diffs.len()
        );
        print_row_diffs(&down_diffs);
    }
    assert!(
        down_diffs.is_empty(),
        "HELLO after C-v should not leave stale row text; {} rows differ",
        down_diffs.len()
    );

    send_both(&mut gnu, &mut neo, "M-v");
    let paged_up = |rows: &[String]| {
        rows.iter().any(|row| row.contains("HELLO"))
            && rows.iter().any(|row| row.to_lowercase().contains("hello"))
    };
    gnu.read_until(Duration::from_secs(6), paged_up);
    neo.read_until(Duration::from_secs(8), paged_up);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let neo_rows = neo.text_grid();
    for (row_idx, row) in neo_rows.iter().enumerate() {
        assert!(
            !row.contains("ꦭꦺꦴ\");") || row.contains("Javanese"),
            "HELLO after M-v left Javanese suffix on unrelated row {row_idx}: |{}|",
            row.trim_end()
        );
    }
}

#[test]
fn mx_view_hello_file_strict_match() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x");
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    for s in [&mut gnu, &mut neo] {
        s.send(b"view-hello-file");
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    send_both(&mut gnu, &mut neo, "RET");
    // Wait for the actual HELLO mode line.  GNU may first emit an autosave
    // warning mentioning "HELLO" while the selected window is still
    // *scratch*, so a plain substring check can sample too early.
    let wants_hello_vc_mode_line = |rows: &[String]| {
        rows.iter()
            .any(|r| r.contains("HELLO") && r.contains("Git-"))
    };
    gnu.read_until(Duration::from_secs(8), wants_hello_vc_mode_line);
    neo.read_until(Duration::from_secs(8), wants_hello_vc_mode_line);

    let gl = gnu.text_grid();
    let nl = neo.text_grid();
    assert!(
        gl.iter().any(|row| row.contains("Git-")),
        "GNU HELLO mode line should show VC status"
    );
    assert!(
        nl.iter().any(|row| row.contains("Git-")),
        "NEO HELLO mode line should show VC status"
    );
    let gl_normalized: Vec<String> = gl.iter().map(|row| normalize_hello_vc_row(row)).collect();
    let nl_normalized: Vec<String> = nl.iter().map(|row| normalize_hello_vc_row(row)).collect();
    let diffs = diff_text_grids(&gl_normalized, &nl_normalized);

    if !diffs.is_empty() {
        eprintln!(
            "mx_view_hello_file_strict_match: {} of {} rows differ",
            diffs.len(),
            gl_normalized.len().min(nl_normalized.len())
        );
        print_row_diffs(&diffs);
    }
    assert_eq!(
        diffs.len(),
        0,
        "NEO and GNU HELLO buffers should be byte-for-byte identical (differ in {} rows)",
        diffs.len()
    );
}

#[test]
fn save_file_visit_another_switch_back_round_trip() {
    let (mut gnu, mut neo) = boot_pair("");
    let name_a = "roundtrip-A.txt";
    let content_a = "file A content line\n";
    let name_b = "roundtrip-B.txt";
    let content_b = "file B content line\n";

    open_home_file(&mut gnu, &mut neo, name_a, content_a, "C-x C-f");
    // Save with C-x C-s
    send_both(&mut gnu, &mut neo, "C-x C-s");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Visit file B
    open_home_file(&mut gnu, &mut neo, name_b, content_b, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-x C-s");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    // Switch back to file A via C-x b
    send_both(&mut gnu, &mut neo, "C-x");
    send_both(&mut gnu, &mut neo, "b");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(format!("{}\r", name_a).as_bytes());
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains(name_a)),
            "{label} should switch back to file A"
        );
        assert!(
            grid.iter().any(|r| r.contains("file A content line")),
            "{label} file A should still have its content after round trip"
        );
    }
}

#[test]
fn copy_whole_buffer_and_yank_to_new_file_via_mw_cy() {
    let (mut gnu, mut neo) = boot_pair("");
    let src_name = "copy-source.txt";
    let src = "source content line\n";
    let dst_name = "copy-dest.txt";
    let empty = "";

    // Open source file
    open_home_file(&mut gnu, &mut neo, src_name, src, "C-x C-f");
    // Select all and copy
    send_both(&mut gnu, &mut neo, "C-x h");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "M-w");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));

    // Open destination file
    open_home_file(&mut gnu, &mut neo, dst_name, empty, "C-x C-f");
    // Yank
    send_both(&mut gnu, &mut neo, "C-y");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|r| r.contains("source content line")),
            "{label}: should yank source content into new file"
        );
    }
}

#[test]
fn write_file_via_cx_cw_saves_buffer_to_new_name() {
    let (mut gnu, mut neo) = boot_pair("");
    let src_name = "write-file-src.txt";
    let src = "write this file\n";
    let new_name = "write-file-dst.txt";

    open_home_file(&mut gnu, &mut neo, src_name, src, "C-x C-f");
    send_both(&mut gnu, &mut neo, "C-x C-w");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    for s in [&mut gnu, &mut neo] {
        s.send(format!("{}\r", new_name).as_bytes());
    }
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));
    // Overwrite confirmation
    send_both(&mut gnu, &mut neo, "y");
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    let gnu_dst = std::fs::read_to_string(gnu.home_dir().join(new_name)).unwrap_or_default();
    let neo_dst = std::fs::read_to_string(neo.home_dir().join(new_name)).unwrap_or_default();
    assert!(
        gnu_dst.contains("write this file"),
        "GNU write-file should save to new name"
    );
    assert!(
        neo_dst.contains("write this file"),
        "NEO write-file should save to new name"
    );
}
