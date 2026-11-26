//! Real pinned grammar libraries through GNU 31.1's public Lisp API.
//!
//! The grammar cache is a pin, not a clone: 'cargo xtask treesit-grammars'
//! prefetches every revision below and verifies each generated parser.c
//! against its SHA-256 before publishing it. These rows read that cache
//! without the network -- a cold cache fails loudly naming the prefetch
//! command instead of flaking on a TLS hiccup mid-clone.
use neomacs_melpa_test_support::{
    EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS, MelpaSandbox, TreeSitterGrammarCase, elisp_string,
    output_with_timeout, output_with_timeout_pinned,
};
use std::{fs, path::Path, time::Duration};

fn copy_grammar(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn run_lisp(runtime: &EmacsRuntime, sandbox: &MelpaSandbox, form: &str) -> String {
    let mut command = sandbox.command(runtime);
    command.args(["--batch", "-Q", "--eval", form]);
    let output = output_with_timeout_pinned(&mut command, Duration::from_secs(60)).unwrap();
    assert!(
        output.status.success(),
        "{}: {}\n{}",
        runtime.name,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn grammar_case(language: &str) -> TreeSitterGrammarCase {
    *GNU31_TREE_SITTER_GRAMMARS
        .iter()
        .find(|case| case.language == language)
        .unwrap()
}

fn check_language(case: TreeSitterGrammarCase) {
    let language = case.language;
    let gnu = EmacsRuntime::gnu_emacs();
    let grammar = case.prepare().unwrap();
    let sandbox = MelpaSandbox::new(&format!("treesit-{language}")).unwrap();
    let destination = sandbox.home().join(".emacs.d/tree-sitter");
    copy_grammar(&grammar, &destination);
    let sample = elisp_string(case.sample);
    let root_type = elisp_string(case.root_type);
    let destination = elisp_string(&destination.to_string_lossy());
    let query = elisp_string(&format!("({}) @root", case.root_type));
    let form = format!(
        r#"(progn
      (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path nil)
      (with-temp-buffer
        (insert {sample})
        (let* ((available (treesit-language-available-p '{language} t))
               (parser (treesit-parser-create '{language}))
               (root (treesit-parser-root-node parser))
               (end (treesit-node-end root))
               (initial (list
                 (car available)
                 (file-in-directory-p (treesit-grammar-location '{language}) {destination})
                 (equal (treesit-node-type root) {root_type})
                 (not (treesit-node-check root 'has-error))
                 (= (length (treesit-query-capture root {query})) 1))))
          (goto-char (point-min))
          (insert "\n")
          (let ((updated (treesit-parser-root-node parser)))
            (prin1 (append initial
              (list (equal (treesit-node-type updated) {root_type})
                    (not (treesit-node-check updated 'has-error))
                    (= (treesit-node-end updated) (1+ end))))))))))"#
    );
    let expected = run_lisp(&gnu, &sandbox, &form);
    assert_eq!(expected, "(t t t t t t t t)", "GNU fixture for {language}");
    assert_eq!(
        run_lisp(&EmacsRuntime::neomacs(), &sandbox, &form),
        expected,
        "{language}"
    );
}

macro_rules! grammar_tests {
    ($($test:ident => $case:expr),* $(,)?) => {$ (
        #[test]
        fn $test() { check_language($case); }
    )*};
}
neomacs_melpa_test_support::gnu31_tree_sitter_grammar_cases!(grammar_tests);

#[test]
fn missing_grammar_parser_and_eager_query_signal_language_load_error() {
    let sandbox = MelpaSandbox::new("treesit-missing-language").unwrap();
    let form = r#"(progn (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path nil)
      (prin1 (list
        (condition-case err (treesit-parser-create 'issue543-missing-grammar)
          (error (list (car err) (cadr err))))
        (condition-case err
            (treesit-query-compile 'issue543-missing-grammar "(document) @root" t)
          (error (list (car err) (cadr err))))))))"#;
    let expected =
        "((treesit-load-language-error not-found) (treesit-load-language-error not-found))";
    assert_eq!(
        run_lisp(&EmacsRuntime::gnu_emacs(), &sandbox, form),
        expected
    );
    assert_eq!(run_lisp(&EmacsRuntime::neomacs(), &sandbox, form), expected);
}

#[test]
fn explicit_grammar_path_precedes_user_directory() {
    let gnu = EmacsRuntime::gnu_emacs();
    let grammar = grammar_case("go").prepare().unwrap();
    let sandbox = MelpaSandbox::new("treesit-precedence").unwrap();
    copy_grammar(&grammar, &sandbox.home().join(".emacs.d/tree-sitter"));
    let extra = sandbox.root().join("explicit");
    copy_grammar(&grammar, &extra);
    let form = format!(
        r#"(progn (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path '("explicit/"))
      (treesit-parser-create 'go)
      (prin1 (file-in-directory-p (treesit-grammar-location 'go) {})))"#,
        elisp_string(&extra.to_string_lossy())
    );
    assert_eq!(run_lisp(&gnu, &sandbox, &form), "t");
    assert_eq!(run_lisp(&EmacsRuntime::neomacs(), &sandbox, &form), "t");
}

#[test]
fn grammar_with_missing_entry_point_reports_symbol_error_without_fallback() {
    let gnu = EmacsRuntime::gnu_emacs();
    let go = grammar_case("go").prepare().unwrap();
    let json = grammar_case("json").prepare().unwrap();
    let sandbox = MelpaSandbox::new("treesit-symbol-error").unwrap();
    copy_grammar(&go, &sandbox.home().join(".emacs.d/tree-sitter"));
    let extra = sandbox.root().join("explicit");
    fs::create_dir_all(&extra).unwrap();
    // A real loadable library, deliberately exposing tree_sitter_json rather
    // than the entry point requested by the Go parser.
    for entry in fs::read_dir(json).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_str().unwrap().replace("json", "go");
        fs::copy(entry.path(), extra.join(name)).unwrap();
    }
    let form = r#"(progn (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path '("explicit/"))
      (let ((result (treesit-language-available-p 'go t)))
        (prin1 (list (car result) (cadr result)))))"#;
    assert_eq!(run_lisp(&gnu, &sandbox, form), "(nil symbol-error)");
    assert_eq!(
        run_lisp(&EmacsRuntime::neomacs(), &sandbox, form),
        "(nil symbol-error)"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn incompatible_grammar_reports_language_version_mismatch() {
    let gnu = EmacsRuntime::gnu_emacs();
    let case = grammar_case("json");
    let source = case.prepare_generated_source().unwrap();
    let sandbox = MelpaSandbox::new("treesit-abi-mismatch").unwrap();
    let parser = sandbox.root().join("parser.c");
    let generated = fs::read_to_string(source.join("parser.c")).unwrap();
    let generated: String = generated
        .lines()
        .map(|line| {
            if line.starts_with("#define LANGUAGE_VERSION ") {
                "#define LANGUAGE_VERSION 12\n".to_owned()
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    fs::write(&parser, generated).unwrap();
    let destination = sandbox.home().join(".emacs.d/tree-sitter");
    fs::create_dir_all(&destination).unwrap();
    let mut compiler = std::process::Command::new("cc");
    compiler
        .args(["-shared", "-fPIC", "-I"])
        .arg(&source)
        .arg(&parser)
        .arg("-o")
        .arg(destination.join("libtree-sitter-json.so"));
    let compiled = output_with_timeout(&mut compiler, Duration::from_secs(60)).unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let form = r#"(progn (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path nil)
      (let ((result (treesit-language-available-p 'json t)))
        (prin1 (list (car result) (cadr result)))))"#;
    assert_eq!(
        run_lisp(&gnu, &sandbox, form),
        "(nil language-grammar-version-mismatch)"
    );
    assert_eq!(
        run_lisp(&EmacsRuntime::neomacs(), &sandbox, form),
        "(nil language-grammar-version-mismatch)"
    );
}

#[test]
fn language_remap_alist_query_capture_matches_gnu_31() {
    let gnu = EmacsRuntime::gnu_emacs();
    let json = grammar_case("json").prepare().unwrap();
    let python = grammar_case("python").prepare().unwrap();
    let sandbox = MelpaSandbox::new("treesit-language-remap").unwrap();
    let destination = sandbox.home().join(".emacs.d/tree-sitter");
    copy_grammar(&json, &destination);
    copy_grammar(&python, &destination);
    let form = r#"(progn
      (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path nil)
      (with-temp-buffer
        (insert "{\"k\": 1}")
        (let* ((buf-local (local-variable-if-set-p 'treesit-language-remap-alist))
               (buf-isolation
                (let ((b1 (generate-new-buffer " *remap-b1*"))
                      (b2 (generate-new-buffer " *remap-b2*")))
                  (unwind-protect
                      (progn
                        (with-current-buffer b1
                          (setq treesit-language-remap-alist '((alias-json . json))))
                        (list (buffer-local-value 'treesit-language-remap-alist b1)
                              (buffer-local-value 'treesit-language-remap-alist b2)))
                    (kill-buffer b1)
                    (kill-buffer b2))))
               (unremapped-queries
                (let ((treesit-language-remap-alist '((alias-json . json))))
                  (list (treesit-language-available-p 'alias-json)
                        (treesit-language-abi-version 'alias-json)
                        (treesit-grammar-location 'alias-json))))
               (bad-parser
                (let ((treesit-language-remap-alist '((foo . 123))))
                  (condition-case err (treesit-parser-create 'foo) (error err))))
               (bad-query
                (let ((treesit-language-remap-alist '((foo . 123))))
                  (condition-case err (treesit-query-compile 'foo "(string) @s" t) (error err))))
               (circular-remap
                (let ((c (list (cons 'a 'b))))
                  (setcdr c c)
                  (let ((treesit-language-remap-alist c))
                    (list
                     (condition-case err (treesit-parser-create 'z) (error (car err)))
                     (condition-case err (treesit-query-compile 'z "(string) @s" t) (error (car err)))))))
               (improper-remap
                (let ((treesit-language-remap-alist '((a . b) . 42)))
                  (list
                   (condition-case err (treesit-parser-create 'c) (error err))
                   (condition-case err (treesit-query-compile 'c "(string) @s" t) (error err)))))
               (p (let ((treesit-language-remap-alist '((alias-json . json))))
                    (treesit-parser-create 'alias-json)))
               (plang (treesit-parser-language p))
               (cap-in-remap
                (let ((treesit-language-remap-alist '((alias-json . json))))
                  (mapcar (lambda (c) (cons (car c) (treesit-node-text (cdr c) t)))
                          (treesit-query-capture p "(string) @s"))))
               (cap-after-nil
                (let ((treesit-language-remap-alist nil))
                  (mapcar (lambda (c) (cons (car c) (treesit-node-text (cdr c) t)))
                          (treesit-query-capture p "(string) @s"))))
               (cap-after-python-remap
                (let ((treesit-language-remap-alist '((alias-json . python))))
                  (mapcar (lambda (c) (cons (car c) (treesit-node-text (cdr c) t)))
                          (treesit-query-capture p "(string) @s"))))
               (cap-sexp-after-python-remap
                (let ((treesit-language-remap-alist '((alias-json . python))))
                  (mapcar (lambda (c) (cons (car c) (treesit-node-text (cdr c) t)))
                          (treesit-query-capture p '((string) @s)))))
               (q-json (treesit-query-compile 'json "(string) @s" t))
               (cap-same-tslang-diff-sym
                (mapcar (lambda (c) (cons (car c) (treesit-node-text (cdr c) t)))
                        (treesit-query-capture p q-json))))
          (prin1 (list buf-local
                       buf-isolation
                       unremapped-queries
                       bad-parser
                       bad-query
                       circular-remap
                       improper-remap
                       plang
                       cap-in-remap
                       cap-after-nil
                       cap-after-python-remap
                       cap-sexp-after-python-remap
                       cap-same-tslang-diff-sym)))))"#;
    let expected = run_lisp(&gnu, &sandbox, form);
    assert_eq!(
        expected,
        r#"(t (((alias-json . json)) nil) (nil nil nil) (wrong-type-argument symbolp 123) (treesit-query-error "Invalid language symbol" 123) (circular-list circular-list) ((wrong-type-argument listp ((a . b) . 42)) (wrong-type-argument listp ((a . b) . 42))) alias-json ((s . "\"k\"")) ((s . "\"k\"")) ((s . "\"k\"")) ((s . "\"k\"")) ((s . "\"k\"")))"#
    );
    assert_eq!(run_lisp(&EmacsRuntime::neomacs(), &sandbox, form), expected);
}

#[test]
fn compiled_query_rejects_mismatched_loaded_tslanguage_across_remap() {
    // When `treesit-language-remap-alist` changes between `treesit-parser-create`
    // and `treesit-query-compile`, the parser and compiled query share the same
    // unremapped Lisp symbol (`alias-lang`) while holding different `TSLanguage`
    // grammars (`json` vs `python`).
    //
    // Note: GNU Emacs 31.1 (`src/treesit.c:3955-3964`) performs no `TSLanguage`
    // check on pre-compiled queries and silently executes `ts_query_cursor_exec`
    // across mismatched grammars (producing bogus symbol-ID collisions such as
    // Python `identifier` matching JSON `"{"`). Neomacs intentionally validates
    // loaded `TSLanguage` equality before `EmacsQuery::matches` to preserve its
    // language-compatibility safety invariant and reject cross-grammar queries
    // with `treesit-query-error`.
    let json = grammar_case("json").prepare().unwrap();
    let python = grammar_case("python").prepare().unwrap();
    let sandbox = MelpaSandbox::new("treesit-cross-tslanguage-remap").unwrap();
    let destination = sandbox.home().join(".emacs.d/tree-sitter");
    copy_grammar(&json, &destination);
    copy_grammar(&python, &destination);
    let form = r#"(progn
      (require 'treesit)
      (setq user-emacs-directory "~/.emacs.d/" treesit-extra-load-path nil)
      (with-temp-buffer
        (insert "{\"k\": 1}")
        (let* ((p (let ((treesit-language-remap-alist '((alias-lang . json))))
                    (treesit-parser-create 'alias-lang)))
               (q-eager-remapped
                (let ((treesit-language-remap-alist '((alias-lang . python))))
                  (treesit-query-compile 'alias-lang "(identifier) @x" t)))
               (q-lazy-remapped
                (treesit-query-compile 'alias-lang "(identifier) @x"))
               (q-direct-python
                (treesit-query-compile 'python "(identifier) @x" t))
               (res-eager
                (condition-case err
                    (list 'ok (treesit-query-capture p q-eager-remapped))
                  (error (car err))))
               (res-lazy
                (let ((treesit-language-remap-alist '((alias-lang . python))))
                  (condition-case err
                      (list 'ok (treesit-query-capture p q-lazy-remapped))
                    (error (car err)))))
               (res-direct
                (condition-case err
                    (list 'ok (treesit-query-capture p q-direct-python))
                  (error (car err)))))
          (prin1 (list res-eager res-lazy res-direct)))))"#;
    assert_eq!(
        run_lisp(&EmacsRuntime::neomacs(), &sandbox, form),
        "(treesit-query-error treesit-query-error treesit-query-error)"
    );
}
