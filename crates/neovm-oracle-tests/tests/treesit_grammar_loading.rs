//! Real pinned grammar libraries through GNU 31.1's public Lisp API.
use neomacs_melpa_test_support::{
    EmacsRuntime, GNU31_TREE_SITTER_GRAMMARS, MelpaSandbox, TreeSitterGrammarCase, elisp_string,
    output_with_timeout,
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
    let mut command = runtime.command();
    sandbox.configure(&mut command);
    command.args(["--batch", "-Q", "--eval", form]);
    let output = output_with_timeout(&mut command, Duration::from_secs(60)).unwrap();
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
    let grammar = case.prepare(&gnu).unwrap();
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
    let grammar = grammar_case("go").prepare(&gnu).unwrap();
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
    let go = grammar_case("go").prepare(&gnu).unwrap();
    let json = grammar_case("json").prepare(&gnu).unwrap();
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
    let source = case.prepare_generated_source(&gnu).unwrap();
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
