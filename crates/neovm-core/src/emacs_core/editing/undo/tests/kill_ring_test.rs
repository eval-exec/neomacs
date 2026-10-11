use crate::emacs_core::{Context, format_eval_result};
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::test_utils::{runtime_startup_eval_all, runtime_startup_eval_one};

fn eval_one(src: &str) -> String {
    let mut ev = Context::new();
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

fn eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    // Root all parsed forms across the eval loop. The Vec<Value>
    // lives on the malloc heap and is invisible to conservative
    // stack scanning; without rooting, any intervening GC reclaims
    // the cons cells in the unrooted forms vec.
    let roots = ev.save_specpdl_roots();
    for form in &forms {
        ev.push_specpdl_root(*form);
    }
    let result = forms
        .iter()
        .map(|form| {
            let result = ev.eval_form(*form);
            format_eval_result(&result)
        })
        .collect();
    ev.restore_specpdl_roots(roots);
    result
}

fn bootstrap_eval_one(src: &str) -> String {
    runtime_startup_eval_one(src)
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

// -- kill/yank tests loaded from GNU simple.el --

#[test]
fn bootstrap_kill_ring_commands_are_not_rust_subrs() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(list kill-ring-max
                 (subrp (symbol-function 'kill-new))
                 (subrp (symbol-function 'kill-append))
                 (subrp (symbol-function 'current-kill))
                 (subrp (symbol-function 'kill-region))
                 (subrp (symbol-function 'kill-ring-save))
                 (subrp (symbol-function 'copy-region-as-kill))
                 (subrp (symbol-function 'kill-line))
                 (subrp (symbol-function 'kill-whole-line))
                 (subrp (symbol-function 'kill-word))
                 (subrp (symbol-function 'backward-kill-word))
                 (subrp (symbol-function 'yank))
                 (subrp (symbol-function 'yank-pop)))"#,
    );
    assert_eq!(
        result,
        "OK (120 nil nil nil nil nil nil nil nil nil nil nil nil)"
    );
}

#[test]
fn bootstrap_kill_ring_entry_ops_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "hello")
             (current-kill 0))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "first")
             (kill-new "second" t)
             (current-kill 0))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "hello")
             (kill-append " world" nil)
             (current-kill 0))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "world")
             (kill-append "hello " t)
             (current-kill 0))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "a")
             (kill-new "b")
             (kill-new "c")
             (list (current-kill 0)
                   (current-kill 1)
                   (current-kill 1)))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "a")
             (kill-new "b")
             (list (current-kill 0 t)
                   (current-kill 1 t)
                   (current-kill -1 t)
                   (current-kill 0 t)))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "x")
             (kill-new "")
             (current-kill 0 t))"#,
    );
    assert_eq!(results[0], r#"OK "hello""#);
    assert_eq!(results[1], r#"OK "second""#);
    assert_eq!(results[2], r#"OK "hello world""#);
    assert_eq!(results[3], r#"OK "hello world""#);
    assert_eq!(results[4], r#"OK ("c" "b" "a")"#);
    assert_eq!(results[5], r#"OK ("b" "a" "a" "b")"#);
    assert_eq!(results[6], r#"OK """#);
}

#[test]
fn bootstrap_current_kill_pointer_rules_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (condition-case err
                 (current-kill 0)
               (error (car err))))
           (progn
             (setq kill-ring (list "a" "b" "c"))
             (setq kill-ring-yank-pointer '("b" . "c"))
             (list
              (condition-case err (current-kill 0 t) (error (car err)))
              (condition-case err (current-kill 0 nil) (error (car err)))))
           (progn
             (setq kill-ring (list "a" "b" "c"))
             (setq kill-ring-yank-pointer '(1))
             (list
              (current-kill 0 t)
              (progn
                (setq kill-ring (list "a" "b" "c"))
                (setq kill-ring-yank-pointer '(1 2 3))
                (current-kill 0 t))
              (progn
                (setq kill-ring (list "a" "b" "c"))
                (setq kill-ring-yank-pointer '(1 2 3 4))
                (current-kill 0 t))))"#,
    );
    assert_eq!(results[0], "OK error");
    assert_eq!(
        results[1],
        r#"OK (wrong-type-argument wrong-type-argument)"#
    );
    assert_eq!(results[2], r#"OK ("c" "a" "c")"#);
}

// -- region copy/cut tests loaded from GNU simple.el --

#[test]
fn bootstrap_region_copy_cut_ops_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello world")
               (kill-region 1 6)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello world")
               (kill-ring-save 1 6)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello world")
               (copy-region-as-kill 1 6)
               (list (buffer-string)
                     (current-kill 0))))"#,
    );
    assert_eq!(results[0], r#"OK (" world" "hello")"#);
    assert_eq!(results[1], r#"OK ("hello world" "hello")"#);
    assert_eq!(results[2], r#"OK ("hello world" "hello")"#);
}

// -- line/word kill tests loaded from GNU simple.el --

#[test]
fn bootstrap_line_and_word_kill_ops_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello\nworld")
               (goto-char (point-min))
               (kill-line)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello\nworld")
               (goto-char 6)
               (kill-line)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "line1\nline2\nline3")
               (goto-char (point-min))
               (kill-line 2)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "line1\nline2\nline3")
               (goto-char 8)
               (kill-whole-line)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello world")
               (goto-char (point-min))
               (kill-word 1)
               (list (buffer-string)
                     (current-kill 0))))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (with-temp-buffer
               (insert "hello world")
               (backward-kill-word 1)
               (list (buffer-string)
                     (current-kill 0))))"#,
    );
    assert_eq!(results[0], "OK (\"\nworld\" \"hello\")");
    assert_eq!(results[1], "OK (\"helloworld\" \"\n\")");
    assert_eq!(results[2], "OK (\"line3\" \"line1\nline2\n\")");
    assert_eq!(results[3], "OK (\"line1\nline3\" \"line2\n\")");
    assert_eq!(results[4], r#"OK (" world" "hello")"#);
    assert_eq!(results[5], r#"OK ("hello " "world")"#);
}

#[test]
fn bootstrap_line_and_word_kill_arity_checks_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer (condition-case err (kill-line nil nil) (error err)))
           (with-temp-buffer (condition-case err (kill-line 1 nil) (error err)))
           (with-temp-buffer (condition-case err (kill-line 1 2 3) (error err)))
           (with-temp-buffer (condition-case err (kill-whole-line nil nil) (error err)))
           (with-temp-buffer (condition-case err (kill-whole-line 1 nil) (error err)))
           (with-temp-buffer (condition-case err (kill-whole-line 1 2 3) (error err)))
           (with-temp-buffer (condition-case err (kill-word) (error err)))"#,
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[2], "OK (wrong-number-of-arguments (0 . 1) 3)");
    assert_eq!(results[3], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[4], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[5], "OK (wrong-number-of-arguments (0 . 1) 3)");
    assert_eq!(results[6], "OK (wrong-number-of-arguments (1 . 1) 0)");
}

// -- yank tests --

#[test]
fn bootstrap_yank_ops_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "inserted")
             (with-temp-buffer
               (yank)
               (buffer-string)))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (kill-new "first")
             (kill-new "second")
             (with-temp-buffer
               (yank 2)
               (buffer-string)))
           (progn
             (setq kill-ring nil kill-ring-yank-pointer nil)
             (condition-case err
                 (with-temp-buffer (yank))
               (error (car err))))"#,
    );
    assert_eq!(results[0], r#"OK "inserted""#);
    assert_eq!(results[1], r#"OK "first""#);
    assert_eq!(results[2], "OK error");
}

#[test]
fn bootstrap_yank_and_yank_pop_arity_checks_match_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer (condition-case err (yank nil nil) (error err)))
           (with-temp-buffer (condition-case err (yank 1 2 3) (error err)))
           (with-temp-buffer (condition-case err (yank-pop nil nil) (error err)))
           (with-temp-buffer (condition-case err (yank-pop 1 2 3) (error err)))"#,
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments (0 . 1) 3)");
    assert_eq!(results[2], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[3], "OK (wrong-number-of-arguments (0 . 1) 3)");
}

// -- yank-pop tests --

#[test]
fn yank_pop_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(kill-new "first")
           (kill-new "second")
           (yank)
           (setq last-command 'yank)
           (yank-pop)
           (buffer-string)"#,
    );
    // After yank-pop, "second" should be replaced by "first".
    assert_eq!(results[5], r#"OK "first""#);
}

#[test]
fn yank_pop_without_yank_errors() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(kill-new "hello") (yank-pop)"#);
    assert!(results[1].contains("end-of-file"));
}

#[test]
fn yank_pop_with_last_command_yank_pop_errors() {
    crate::test_utils::init_test_tracing();
    let results =
        bootstrap_eval_all(r#"(kill-new "hello") (setq last-command 'yank-pop) (yank-pop)"#);
    assert!(results[2].contains("end-of-file"));
}

#[test]
fn yank_pop_empty_ring_errors() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one("(yank-pop)");
    assert!(result.contains("Kill ring is empty"));
}

#[test]
fn yank_pop_without_region_errors() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(kill-new "hello") (setq last-command 'yank) (yank-pop)"#);
    assert!(results[2].contains("wrong-type-argument"));
}

// -- downcase-region tests --

#[test]
fn downcase_region_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "HELLO WORLD")
           (downcase-region 1 12)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "hello world""#);
}

#[test]
fn downcase_region_unicode_kelvin_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "K")
           (downcase-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "K""#);
}

// -- upcase-region tests --

#[test]
fn upcase_region_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hello world")
           (upcase-region 1 12)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "HELLO WORLD""#);
}

#[test]
fn upcase_region_unicode_dotless_i_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "ı")
           (upcase-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "ı""#);
}

#[test]
fn downcase_region_unicode_edge_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 42955))
           (downcase-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Ɤ""#);
}

#[test]
fn upcase_region_unicode_edge_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 42957))
           (upcase-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "ꟍ""#);
}

// -- capitalize-region tests --

#[test]
fn capitalize_region_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hello world")
           (capitalize-region 1 12)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Hello World""#);
}

#[test]
fn upcase_initials_region_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hELLo wORLD")
           (upcase-initials-region 1 12)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "HELLo WORLD""#);
}

#[test]
fn capitalize_region_unicode_sharp_s_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "ß")
           (capitalize-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Ss""#);
}

#[test]
fn upcase_initials_region_unicode_sharp_s_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "ß")
           (upcase-initials-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Ss""#);
}

#[test]
fn capitalize_region_unicode_ligature_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 64256))
           (capitalize-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Ff""#);
}

#[test]
fn capitalize_region_unicode_greek_precomposed_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 8064))
           (capitalize-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "ᾈ""#);
}

#[test]
fn upcase_initials_region_unicode_armenian_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 1415))
           (upcase-initials-region 1 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "Եւ""#);
}

#[test]
fn upcase_region_noncontiguous_rejects_bad_region_extract_function() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "abc")
             (setq region-extract-function (lambda (_op) 42))
             (upcase-region 1 3 t))"#,
    );
    assert!(result.starts_with("ERR (wrong-type-argument"));
    assert!(result.contains("listp"));
}

#[test]
fn upcase_region_noncontiguous_uses_region_extract_bounds() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "abcde")
           (setq region-extract-function (lambda (_op) (list (cons 2 4))))
           (upcase-region 1 5 t)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "aBCde""#);
}

// -- downcase-word tests --

#[test]
fn downcase_word_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "HELLO WORLD")
           (goto-char 0)
           (downcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "hello WORLD""#);
}

#[test]
fn downcase_word_unicode_kelvin_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "K")
           (goto-char 0)
           (downcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "K""#);
}

#[test]
fn downcase_word_unicode_extended_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 68944))
           (goto-char 0)
           (downcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "𐵐""#);
}

// -- upcase-word tests --

#[test]
fn upcase_word_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hello world")
           (goto-char 0)
           (upcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "HELLO world""#);
}

#[test]
fn upcase_word_unicode_dotless_i_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "ı")
           (goto-char 0)
           (upcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "ı""#);
}

#[test]
fn upcase_word_unicode_extended_preserved() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 68976))
           (goto-char 0)
           (upcase-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "𐵰""#);
}

// -- capitalize-word tests --

#[test]
fn capitalize_word_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hello world")
           (goto-char 0)
           (capitalize-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "Hello world""#);
}

#[test]
fn capitalize_word_mixed_case() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hELLO world")
           (goto-char 0)
           (capitalize-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "Hello world""#);
}

#[test]
fn capitalize_word_unicode_greek_iota_subscript_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 8114))
           (goto-char 0)
           (capitalize-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "Ὰͅ""#);
}

#[test]
fn capitalize_word_unicode_greek_small_alpha_ypogegrammeni_titlecase() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert (char-to-string 8064))
           (goto-char 0)
           (capitalize-word 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "ᾈ""#);
}

// -- transpose-chars tests --

#[test]
fn transpose_chars_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "abc")
           (goto-char 2)
           (transpose-chars 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "bac""#);
}

#[test]
fn transpose_chars_at_end() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "abc")
           (transpose-chars 1)
           (buffer-string)"#,
    );
    // Point is at end (3), should swap 'b' and 'c'.
    assert_eq!(results[2], r#"OK "acb""#);
}

// -- transpose-lines tests --

#[test]
fn transpose_lines_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "line1\nline2\nline3")
           (goto-char 8)
           (transpose-lines 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], "OK \"line2\nline1\nline3\"");
}

#[test]
fn transpose_lines_at_buffer_start() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "line1\nline2")
           (goto-char 1)
           (transpose-lines 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], "OK \"line2\nline1\n\"");
}

#[test]
fn transpose_lines_arg_two_at_buffer_start() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a\nb\nc\n")
           (goto-char 1)
           (transpose-lines 2)
           (buffer-string)"#,
    );
    assert_eq!(results[3], "OK \"b\nc\na\n\"");
}

#[test]
fn transpose_lines_last_line_without_trailing_newline() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "line1\nline2")
           (goto-char 7)
           (transpose-lines 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], "OK \"line2\nline1\n\"");
}

#[test]
fn transpose_lines_negative_errors() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "a\nb\n")
             (goto-char 3)
             (transpose-lines -1))"#,
    );
    assert!(result.starts_with("ERR (error"));
}

// -- transpose-words tests --

#[test]
fn transpose_words_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "aa bb")
           (goto-char 0)
           (transpose-words 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "bb aa""#);
}

#[test]
fn transpose_words_not_enough_words_errors() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "aa")
             (goto-char 0)
             (transpose-words 1))"#,
    );
    assert!(result.starts_with("ERR"));
}

// -- transpose-sexps tests --

#[test]
fn transpose_sexps_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "(aa) (bb)")
           (goto-char 5)
           (transpose-sexps 1)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "(bb) (aa)""#);
}

#[test]
fn transpose_sexps_at_bob_advances_without_swapping() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "(aa) (bb)")
           (goto-char 1)
           (transpose-sexps 1)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("(aa) (bb)" 5)"#);
}

// -- transpose-sentences tests --

#[test]
fn transpose_sentences_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "One.  Two.")
           (goto-char 1)
           (transpose-sentences 1)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("Two.  One." 11)"#);
}

#[test]
fn transpose_sentences_with_single_space_signals_end_of_buffer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "One. Two.")
             (goto-char 1)
             (transpose-sentences 1))"#,
    );
    assert!(result.starts_with("ERR (end-of-buffer"));
}

// -- transpose-paragraphs tests --

#[test]
fn transpose_paragraphs_basic() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "A\n\nB")
             (goto-char 1)
             (transpose-paragraphs 1)
             (list (buffer-string) (point)))"#,
    );
    assert_eq!(result, "OK (\"\nBA\n\" 5)");
}

#[test]
fn transpose_paragraphs_backward_from_eob() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (insert "A\n\nB\n\nC")
             (goto-char (point-max))
             (transpose-paragraphs -1)
             (list (buffer-string) (point)))"#,
    );
    assert_eq!(result, "OK (\"A\n\nC\nB\n\" 5)");
}

// -- indent-line-to tests --

#[test]
fn indent_line_to_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "hello")
           (indent-line-to 4)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "    hello""#);
}

#[test]
fn indent_line_to_replaces_existing() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "  hello")
           (indent-line-to 4)
           (buffer-string)"#,
    );
    assert_eq!(results[2], r#"OK "    hello""#);
}

#[test]
fn indent_line_to_returns_column() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "  hi")
             (goto-char (point-min))
             (list (indent-line-to 4)
                   (current-indentation)
                   (buffer-string)))
           (with-temp-buffer
             (insert "    hi")
             (goto-char (point-min))
             (indent-line-to 4))"#,
    );
    assert_eq!(results[0], r#"OK (4 4 "    hi")"#);
    assert_eq!(results[1], "OK nil");
}

// -- indent-to tests --

#[test]
fn indent_to_basic() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(insert "hi")
           (indent-to 8)
           (buffer-string)"#,
    );
    // GNU Emacs uses a tab here because indent-tabs-mode defaults to non-nil.
    assert_eq!(results[2], "OK \"hi\t\"");
}

#[test]
fn indent_to_returns_reached_column() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef")
             (goto-char (point-max))
             (list (current-column)
                   (indent-to 2)
                   (current-column)))
           (with-temp-buffer
             (list (current-column)
                   (indent-to 2 5)
                   (current-column)))"#,
    );
    assert_eq!(results[0], "OK (6 6 6)");
    assert_eq!(results[1], "OK (0 5 5)");
}

#[test]
fn indent_to_minimum_requires_fixnump() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer (condition-case err (indent-to 4 nil) (error err)))
           (with-temp-buffer (condition-case err (indent-to 4 "x") (error err)))
           (with-temp-buffer (condition-case err (indent-to 4 t) (error err)))
           (with-temp-buffer (condition-case err (indent-to "x") (error err)))"#,
    );
    assert_eq!(results[0], "OK 4");
    assert_eq!(results[1], r#"OK (wrong-type-argument fixnump "x")"#);
    assert_eq!(results[2], "OK (wrong-type-argument fixnump t)");
    assert_eq!(results[3], r#"OK (wrong-type-argument fixnump "x")"#);
}

// -- newline tests --

#[test]
fn newline_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "ab")
           (goto-char 2)
           (newline)
           (buffer-string)"#,
    );
    assert_eq!(results[3], "OK \"a\nb\"");
}

#[test]
fn newline_multiple() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(newline 3) (buffer-string)"#);
    assert_eq!(results[1], "OK \"\n\n\n\"");
}

#[test]
fn newline_prefix_arg_coercion_contract() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (newline 1.5)
             (list (point) (append (buffer-string) nil)))
           (with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (newline t)
             (list (point) (append (buffer-string) nil)))
           (with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (newline "x")
             (list (point) (append (buffer-string) nil)))"#,
    );
    assert_eq!(results[0], "OK (3 (97 10 98))");
    assert_eq!(results[1], "OK (3 (97 10 98))");
    assert_eq!(results[2], "OK (3 (97 10 98))");
}

#[test]
fn newline_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (condition-case err (newline 1 t nil) (error err)))
           (with-temp-buffer
             (condition-case err (newline nil nil nil nil) (error err)))"#,
    );
    assert_eq!(results[0], "OK (wrong-number-of-arguments (0 . 2) 3)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments (0 . 2) 4)");
}

#[test]
fn newline_and_indent_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (condition-case err (newline-and-indent nil nil) (error err)))
           (with-temp-buffer
             (condition-case err (newline-and-indent nil nil nil) (error err)))"#,
    );
    // GNU 31.0.50: newline-and-indent has arity (0 . 1).
    assert_eq!(results[0], "OK (wrong-number-of-arguments (0 . 1) 2)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments (0 . 1) 3)");
}

// -- newline-and-indent tests --

#[test]
fn newline_and_indent_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (setq indent-line-function 'indent-relative)
             (insert "    hello")
             (newline-and-indent)
             (buffer-string))"#,
    );
    // Should add newline + 4 spaces of indentation (copying prev line).
    assert_eq!(results[0], "OK \"    hello\n    \"");
}

#[test]
fn newline_and_indent_normalizes_surrounding_whitespace() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "  x")
             (goto-char 3)
             (list (condition-case err (newline-and-indent) (error err))
                   (point)
                   (buffer-string)))
           (with-temp-buffer
             (insert "a b")
             (goto-char 3)
             (list (condition-case err (newline-and-indent) (error err))
                   (point)
                   (buffer-string)))"#,
    );
    assert_eq!(results[0], "OK (nil 2 \"\nx\")");
    assert_eq!(results[1], "OK (nil 3 \"a\nb\")");
}

// -- open-line tests --

#[test]
fn open_line_keeps_point_before_inserted_newlines() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "ab")
           (goto-char 2)
           (open-line 2)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], "OK (\"a\n\nb\" 2)");
}

#[test]
fn open_line_accepts_float_and_rejects_non_number_marker() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (open-line 1.5)
             (list (point) (append (buffer-string) nil)))
           (with-temp-buffer (condition-case err (open-line "x") (error err)))
           (with-temp-buffer (condition-case err (open-line t) (error err)))"#,
    );
    assert_eq!(results[0], "OK (2 (97 10 98))");
    assert_eq!(
        results[1],
        r#"OK (wrong-type-argument number-or-marker-p "x")"#
    );
    assert_eq!(results[2], "OK (wrong-type-argument number-or-marker-p t)");
}

#[test]
fn open_line_count_coercion_contract() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (condition-case err (open-line -1) (error err)))
           (with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (open-line 2)
             (list (point) (append (buffer-string) nil)))
           (with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (open-line 2.0)
             (list (point) (append (buffer-string) nil)))
           (with-temp-buffer
             (insert "ab")
             (goto-char 2)
             (open-line -2.5)
             (list (point) (append (buffer-string) nil)))"#,
    );
    assert_eq!(
        results[0],
        r#"OK (error "Repetition argument has to be non-negative")"#
    );
    assert_eq!(results[1], "OK (2 (97 10 10 98))");
    assert_eq!(results[2], "OK (2 (97 10 98))");
    assert_eq!(results[3], "OK (2 (97 10 98))");
}

// -- delete-horizontal-space tests --

#[test]
fn delete_horizontal_space_deletes_both_sides() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a \t  b")
           (goto-char 4)
           (delete-horizontal-space)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("ab" 2)"#);
}

#[test]
fn delete_horizontal_space_backward_only() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a \t  b")
           (goto-char 4)
           (delete-horizontal-space t)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("a  b" 2)"#);
}

// -- just-one-space tests --

#[test]
fn just_one_space_default() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a \t  b")
           (goto-char 4)
           (just-one-space)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("a b" 3)"#);
}

#[test]
fn just_one_space_zero() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a \t  b")
           (goto-char 4)
           (just-one-space 0)
           (list (buffer-string) (point))"#,
    );
    assert_eq!(results[3], r#"OK ("ab" 2)"#);
}

#[test]
fn just_one_space_argument_contract_subset() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (condition-case err (just-one-space "x") (error (list (car err) (nth 1 err)))))
           (with-temp-buffer
             (condition-case err (just-one-space t) (error (list (car err) (nth 1 err)))))
           (with-temp-buffer
             (condition-case err (just-one-space 1.5) (error (list (car err) (nth 1 err) (nth 2 err)))))
           (with-temp-buffer
             (condition-case err (just-one-space -1.5) (error (list (car err) (nth 1 err) (nth 2 err)))))
           (with-temp-buffer
             (let ((m (make-marker)))
               (set-marker m 1)
               (condition-case err (just-one-space m) (error (list (car err) (nth 1 err))))))
           (with-temp-buffer
             (insert "a \t  b")
             (goto-char 4)
             (just-one-space -2)
             (list (point) (append (buffer-string) nil)))"#,
    );
    assert_eq!(results[0], "OK (wrong-type-argument number-or-marker-p)");
    assert_eq!(results[1], "OK (wrong-type-argument number-or-marker-p)");
    assert_eq!(
        results[2],
        "OK (wrong-type-argument integer-or-marker-p 2.5)"
    );
    assert_eq!(
        results[3],
        "OK (wrong-type-argument integer-or-marker-p 2.5)"
    );
    assert_eq!(results[4], "OK (wrong-type-argument numberp)");
    assert_eq!(results[5], "OK (4 (97 32 32 98))");
}

// -- delete-indentation tests --

#[test]
fn delete_indentation_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "hello\n    world")
           (goto-char 14)
           (delete-indentation)
           (buffer-string)"#,
    );
    assert_eq!(results[3], r#"OK "hello world""#);
}

#[test]
fn delete_indentation_keeps_point_before_join_space() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "a\n  b")
             (goto-char 3)
             (list (delete-indentation) (point) (append (buffer-string) nil)))
           (with-temp-buffer
             (insert "a\n  b")
             (goto-char 2)
             (list (delete-indentation t) (point) (append (buffer-string) nil)))"#,
    );
    assert_eq!(results[0], "OK (nil 2 (97 32 98))");
    assert_eq!(results[1], "OK (nil 2 (97 32 98))");
}

#[test]
fn delete_indentation_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (condition-case err (delete-indentation nil nil nil nil) (error err)))
           (with-temp-buffer
             (condition-case err (delete-indentation t nil nil nil) (error err)))"#,
    );
    // GNU emacs 31.0.50 verified: delete-indentation is a Lisp
    // bytecode function with arity (0 . 3); arity errors carry
    // the (MIN . MAX) tuple from funcall_lambda.
    assert_eq!(results[0], "OK (wrong-number-of-arguments (0 . 3) 4)");
    assert_eq!(results[1], "OK (wrong-number-of-arguments (0 . 3) 4)");
}

// -- tab-to-tab-stop tests --

#[test]
fn tab_to_tab_stop_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "hi")
           (tab-to-tab-stop)
           (buffer-string)"#,
    );
    assert_eq!(results[2], "OK \"hi\t\"");
}

#[test]
fn tab_to_tab_stop_returns_reached_column() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (list (current-column)
                   (tab-to-tab-stop)
                   (current-column)
                   (buffer-string)))
           (with-temp-buffer
             (insert "abc")
             (goto-char (point-max))
             (list (current-column)
                   (tab-to-tab-stop)
                   (current-column)
                   (buffer-string)))"#,
    );
    assert_eq!(results[0], "OK (0 8 8 \"\t\")");
    assert_eq!(results[1], "OK (3 8 8 \"abc\t\")");
}

// -- indent-rigidly tests --

#[test]
fn indent_rigidly_forward() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "a\nb\nc")
           (indent-rigidly 1 6 2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], "OK \"  a\n  b\n  c\"");
}

#[test]
fn indent_rigidly_backward() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(insert "  a\n  b\n  c")
           (indent-rigidly 1 12 -2)
           (buffer-string)"#,
    );
    assert_eq!(results[2], "OK \"a\nb\nc\"");
}

#[test]
fn indent_rigidly_argument_contract_subset() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer (condition-case err (indent-rigidly 1 2 "x") (error err)))
           (with-temp-buffer (condition-case err (indent-rigidly 1 2 t) (error err)))
           (with-temp-buffer (condition-case err (indent-rigidly 1 2 [1]) (error err)))
           (with-temp-buffer (condition-case err (indent-rigidly 1 2 nil) (error err)))
           (with-temp-buffer (condition-case err (indent-rigidly "x" 2 1) (error err)))
           (with-temp-buffer (condition-case err (indent-rigidly 1 "x" 1) (error err)))
           (with-temp-buffer
             (insert "a")
             (condition-case err (indent-rigidly 1 2 1.5) (error err))
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK nil");
    assert_eq!(
        results[4],
        r#"OK (wrong-type-argument integer-or-marker-p "x")"#
    );
    assert_eq!(
        results[5],
        r#"OK (wrong-type-argument integer-or-marker-p "x")"#
    );
    assert_eq!(results[6], r#"OK " a""#);
}

// -- wrong args tests --

#[test]
fn kill_new_wrong_type() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(kill-new 42)");
    assert!(result.starts_with("ERR"));
}

#[test]
fn kill_word_wrong_args() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one("(kill-word)");
    assert!(result.starts_with("ERR"));
}

#[test]
fn downcase_region_wrong_args() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(downcase-region 0)");
    assert!(result.starts_with("ERR"));
}

#[test]
fn transpose_chars_wrong_args() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(transpose-chars)");
    assert!(result.starts_with("ERR"));
}
