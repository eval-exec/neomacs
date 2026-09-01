use expect_test::expect;

use super::ParityBatchCase;

/// The workflow the package exists for: open a Rust file in a cargo project,
/// run `M-x ac-racer-setup', and complete a method against racer.  The
/// recorded invocation is the package's whole contract with the binary, so it
/// is pinned in full: the `racer complete <line> <column> <file> <tempfile>'
/// argument vector, the RUST_SRC_PATH racer.el contributes, the empty stdin
/// (the buffer travels through the temporary file, not the pipe), the working
/// directory and the exact bytes written to `ac-racer--tempfile'.  With two
/// candidates auto-complete first expands their common part inline, then the
/// user picks the second one and completes it.
fn sets_up_the_source_and_completes_a_string_method_through_racer() -> ParityBatchCase {
    ParityBatchCase::value(
        "sets_up_the_source_and_completes_a_string_method_through_racer",
        r##"
        (progn
          (ac-racer-test-project)
          (ac-racer-test-reply
           (concat "PREFIX 10,13,ins\n"
                   "MATCH insert,1749,11,"
                   "/rustlib/src/rust/library/alloc/src/string.rs,Function,"
                   "pub fn insert(&mut self, idx: usize, ch: char)\n"
                   "MATCH insert_str,1793,11,"
                   "/rustlib/src/rust/library/alloc/src/string.rs,Function,"
                   "pub fn insert_str(&mut self, idx: usize, string: &str)\n"
                   "END\n"))
          (ac-racer-test-open
           "src/main.rs"
           (concat "use std::collections::HashMap;\n"
                   "\n"
                   "fn main() {\n"
                   "    let mut scores: HashMap<String, i32> = HashMap::new();\n"
                   "    let mut label = String::new();\n"
                   "    label.ins\n"
                   "    scores.insert(label, 1);\n"
                   "    for (name, score) in &scores {\n"
                   "        println!(\"{}: {}\", name, score);\n"
                   "    }\n"
                   "}\n"))
          (list
           :setup (list :mode-before auto-complete-mode
                        :sources-before ac-sources
                        :returned (call-interactively 'ac-racer-setup)
                        :mode-after auto-complete-mode
                        :major-mode major-mode)
           :offered (progn
                      (goto-char (point-min))
                      (forward-line 5)
                      (end-of-line)
                      (auto-complete)
                      (list :line (ac-racer-test-line)
                            :point (point)
                            :ac-point ac-point
                            :ac-prefix (substring-no-properties ac-prefix)
                            :candidates
                            (ac-racer-test-candidate-details ac-candidates)
                            :first-properties
                            (text-properties-at 0 (car ac-candidates))
                            :menu-live (ac-menu-live-p)
                            :menu (mapcar #'substring-no-properties
                                          (popup-list ac-menu))
                            :selected (substring-no-properties
                                       (ac-selected-candidate))))
           :completed (progn
                        (ac-next)
                        (ac-complete)
                        (list :line (ac-racer-test-line)
                              :point (point)
                              :menu-live (ac-menu-live-p)
                              :last-completion (ac-racer-test-last-completion)
                              :buffer (buffer-substring-no-properties
                                       (point-min) (point-max))))
           :tempfile (list (file-exists-p ac-racer--tempfile)
                           (ac-racer-test-file-text ac-racer--tempfile))
           :recorded (ac-racer-test-recorded)))
    "##,
        expect![[
            r#"OK (:setup (:mode-before nil :sources-before #1=(ac-source-words-in-same-mode-buffers) :returned (ac-source-racer . #1#) :mode-after t :major-mode rust-mode) :offered (:line "    label.insert" :point 155 :ac-point 149 :ac-prefix "insert" :candidates (("insert" "Function" "pub fn insert(&mut self, idx: usize, ch: char)") ("insert_str" "Function" "pub fn insert_str(&mut self, idx: usize, string: &str)")) :first-properties (document "pub fn insert(&mut self, idx: usize, ch: char)" summary "Function") :menu-live t :menu ("insert" "insert_str") :selected "insert") :completed (:line "    label.insert_str" :point 159 :menu-live nil :last-completion ("insert_str" "Function" "pub fn insert_str(&mut self, idx: usize, string: &str)" 149) :buffer "use std::collections::HashMap;\n\nfn main() {\n    let mut scores: HashMap<String, i32> = HashMap::new();\n    let mut label = String::new();\n    label.insert_str\n    scores.insert(label, 1);\n    for (name, score) in &scores {\n        println!(\"{}: {}\", name, score);\n    }\n}\n") :tempfile (t "use std::collections::HashMap;\n\nfn main() {\n    let mut scores: HashMap<String, i32> = HashMap::new();\n    let mut label = String::new();\n    label.ins\n    scores.insert(label, 1);\n    for (name, score) in &scores {\n        println!(\"{}: {}\", name, score);\n    }\n}\n") :recorded (("01-request" . "argv: complete 6 13 [ORACLE-SANDBOX]/rust/src/main.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\nuse std::collections::HashMap;\n\nfn main() {\n    let mut scores: HashMap<String, i32> = HashMap::new();\n    let mut label = String::new();\n    label.ins\n    scores.insert(label, 1);\n    for (name, score) in &scores {\n        println!(\"{}: {}\", name, score);\n    }\n}\n")))"#
        ]],
    )
}

fn completes_a_non_ascii_identifier_and_reports_a_character_column() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_a_non_ascii_identifier_and_reports_a_character_column",
        r##"
        (progn
          (ac-racer-test-project)
          (ac-racer-test-reply
           (concat "PREFIX 26,29,erz\n"
                   "MATCH erzeuge,4,11,"
                   "/rustlib/src/rust/library/main.rs,Function,"
                   "pub fn erzeuge(höhe: i64) -> Betrag\n"
                   "END\n"))
          (ac-racer-test-open
           "src/main.rs"
           (concat "mod währung {\n"
                   "    pub struct Betrag { pub höhe: i64 }\n"
                   "\n"
                   "    pub fn erzeuge(höhe: i64) -> Betrag {\n"
                   "        Betrag { höhe }\n"
                   "    }\n"
                   "}\n"
                   "\n"
                   "fn main() {\n"
                   "    // Grüße an die Welt – 図形\n"
                   "    let betrag = währung::erz\n"
                   "    println!(\"{}\", betrag.höhe);\n"
                   "}\n"))
          (call-interactively 'ac-racer-setup)
          (goto-char (point-min))
          (forward-line 10)
          (end-of-line)
          (list
           :before (list :line (ac-racer-test-line)
                         :point (point)
                         :column (current-column)
                         :byte-column (ac-racer-test-byte-column))
           :completed (progn
                        (auto-complete)
                        (list :line (ac-racer-test-line)
                              :point (point)
                              :column (current-column)
                              :menu-live (ac-menu-live-p)
                              :last-completion (ac-racer-test-last-completion)))
           :tempfile (ac-racer-test-file-text ac-racer--tempfile)
           :recorded (ac-racer-test-recorded)))
    "##,
        expect![[
            r#"OK (:before (:line "    let betrag = währung::erz" :point 202 :column 29 :byte-column 30) :completed (:line "    let betrag = währung::erzeuge" :point 206 :column 33 :menu-live nil :last-completion ("erzeuge" "Function" "pub fn erzeuge(höhe: i64) -> Betrag" 199)) :tempfile "mod währung {\n    pub struct Betrag { pub höhe: i64 }\n\n    pub fn erzeuge(höhe: i64) -> Betrag {\n        Betrag { höhe }\n    }\n}\n\nfn main() {\n    // Grüße an die Welt – 図形\n    let betrag = währung::erz\n    println!(\"{}\", betrag.höhe);\n}\n" :recorded (("01-request" . "argv: complete 11 29 [ORACLE-SANDBOX]/rust/src/main.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\nmod währung {\n    pub struct Betrag { pub höhe: i64 }\n\n    pub fn erzeuge(höhe: i64) -> Betrag {\n        Betrag { höhe }\n    }\n}\n\nfn main() {\n    // Grüße an die Welt – 図形\n    let betrag = währung::erz\n    println!(\"{}\", betrag.höhe);\n}\n")))"#
        ]],
    )
    .fresh_process()
}

fn offers_nothing_when_racer_finds_no_matches_or_exits_non_zero() -> ParityBatchCase {
    ParityBatchCase::value(
        "offers_nothing_when_racer_finds_no_matches_or_exits_non_zero",
        r##"
        (progn
          (ac-racer-test-project)
          (ac-racer-test-reply "PREFIX 10,13,zzz\nEND\n" 1)
          (ac-racer-test-reply "" 2 1
                               "error: no matching crate root found\n")
          (list
           :no-matches
           (progn
             (ac-racer-test-open
              "src/main.rs"
              (concat "fn main() {\n"
                      "    let label = String::new();\n"
                      "    label.zzz\n"
                      "}\n"))
             (call-interactively 'ac-racer-setup)
             (goto-char (point-min))
             (forward-line 2)
             (end-of-line)
             (auto-complete)
             (list :line (ac-racer-test-line)
                   :point (point)
                   :candidates ac-candidates
                   :menu-live (ac-menu-live-p)
                   :modified (buffer-modified-p)
                   :invocations (ac-racer-test-invocations)))
           :racer-failed
           (progn
             (ac-racer-test-open
              "src/lib.rs"
              (concat "pub fn helper() {\n"
                      "    let label = String::new();\n"
                      "    label.ins\n"
                      "}\n"))
             (call-interactively 'ac-racer-setup)
             (goto-char (point-min))
             (forward-line 2)
             (end-of-line)
             (auto-complete)
             (list :line (ac-racer-test-line)
                   :point (point)
                   :candidates ac-candidates
                   :menu-live (ac-menu-live-p)
                   :modified (buffer-modified-p)
                   :invocations (ac-racer-test-invocations)))
           :recorded (ac-racer-test-recorded)))
    "##,
        expect![[
            r#"OK (:no-matches (:line "    label.zzz" :point 57 :candidates nil :menu-live nil :modified nil :invocations 1) :racer-failed (:line "    label.ins" :point 63 :candidates nil :menu-live nil :modified nil :invocations 2) :recorded (("01-request" . "argv: complete 3 13 [ORACLE-SANDBOX]/rust/src/main.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\nfn main() {\n    let label = String::new();\n    label.zzz\n}\n") ("02-request" . "argv: complete 3 13 [ORACLE-SANDBOX]/rust/src/lib.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\npub fn helper() {\n    let label = String::new();\n    label.ins\n}\n")))"#
        ]],
    )
    .fresh_process()
}

fn signals_file_missing_when_the_racer_binary_is_not_installed() -> ParityBatchCase {
    ParityBatchCase::value(
        "signals_file_missing_when_the_racer_binary_is_not_installed",
        r##"
        (progn
          (ac-racer-test-project)
          (ac-racer-test-reply
           (concat "PREFIX 10,13,ins\n"
                   "MATCH insert,1749,11,"
                   "/rustlib/src/rust/library/alloc/src/string.rs,Function,"
                   "pub fn insert(&mut self, idx: usize, ch: char)\n"
                   "END\n"))
          (ac-racer-test-open
           "src/main.rs"
           (concat "fn main() {\n"
                   "    let mut label = String::new();\n"
                   "    label.ins\n"
                   "}\n"))
          (call-interactively 'ac-racer-setup)
          (goto-char (point-min))
          (forward-line 2)
          (end-of-line)
          (delete-file racer-cmd)
          (list
           :uninstalled
           (list :racer-cmd (file-name-nondirectory racer-cmd)
                 :exists (file-exists-p racer-cmd)
                 :error (condition-case error
                            (progn (auto-complete) 'completed-without-racer)
                          (error (list (car error) (cdr error))))
                 :line (ac-racer-test-line)
                 :point (point)
                 :candidates ac-candidates
                 :modified (buffer-modified-p)
                 :invocations (ac-racer-test-invocations))
           :reinstalled
           (progn
             (ac-racer-test-install-racer)
             (auto-complete)
             (list :line (ac-racer-test-line)
                   :point (point)
                   :last-completion (ac-racer-test-last-completion)
                   :invocations (ac-racer-test-invocations)))
           :recorded (ac-racer-test-recorded)))
    "##,
        expect![[
            r#"OK (:uninstalled (:racer-cmd "racer" :exists nil :error (file-missing ("Searching for program" "No such file or directory" "[ORACLE-SANDBOX]/rust/bin/racer")) :line "    label.ins" :point 61 :candidates nil :modified nil :invocations 0) :reinstalled (:line "    label.insert" :point 64 :last-completion ("insert" "Function" "pub fn insert(&mut self, idx: usize, ch: char)" 58) :invocations 1) :recorded (("01-request" . "argv: complete 3 13 [ORACLE-SANDBOX]/rust/src/main.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\nfn main() {\n    let mut label = String::new();\n    label.ins\n}\n")))"#
        ]],
    )
    .fresh_process()
}

fn setup_adds_the_source_once_and_only_to_the_buffer_that_ran_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_adds_the_source_once_and_only_to_the_buffer_that_ran_it",
        r##"
        (progn
          (ac-racer-test-project)
          (ac-racer-test-reply
           (concat "PREFIX 10,13,ins\n"
                   "MATCH insert,1749,11,"
                   "/rustlib/src/rust/library/alloc/src/string.rs,Function,"
                   "pub fn insert(&mut self, idx: usize, ch: char)\n"
                   "MATCH insert_str,1793,11,"
                   "/rustlib/src/rust/library/alloc/src/string.rs,Function,"
                   "pub fn insert_str(&mut self, idx: usize, string: &str)\n"
                   "END\n"))
          (let ((configured
                 (ac-racer-test-open
                  "src/main.rs"
                  (concat "fn main() {\n"
                          "    let mut label = String::new();\n"
                          "    label.ins\n"
                          "}\n")))
                (untouched
                 (ac-racer-test-open
                  "src/helper.rs"
                  (concat "pub fn helper() {\n"
                          "    let mut label = String::new();\n"
                          "    label.ins\n"
                          "}\n"))))
            (list
             :configured
             (with-current-buffer configured
               (list :first (call-interactively 'ac-racer-setup)
                     :second (call-interactively 'ac-racer-setup)
                     :mode auto-complete-mode
                     :buffer-local (local-variable-p 'ac-sources)))
             :untouched
             (with-current-buffer untouched
               (auto-complete-mode 1)
               (list :major-mode major-mode
                     :mode auto-complete-mode
                     :sources ac-sources
                     :buffer-local (local-variable-p 'ac-sources)))
             :global-default (default-value 'ac-sources)
             :completes-in-configured
             (with-current-buffer configured
               (set-window-buffer (selected-window) configured)
               (goto-char (point-min))
               (forward-line 2)
               (end-of-line)
               (auto-complete)
               (ac-next)
               (ac-complete)
               (list :line (ac-racer-test-line)
                     :last-completion (ac-racer-test-last-completion)
                     :invocations (ac-racer-test-invocations)))
             :silent-in-untouched
             (with-current-buffer untouched
               (set-window-buffer (selected-window) untouched)
               (goto-char (point-min))
               (forward-line 2)
               (end-of-line)
               (auto-complete)
               (list :line (ac-racer-test-line)
                     :point (point)
                     :sources ac-sources
                     :candidates (mapcar #'substring-no-properties ac-candidates)
                     :modified (buffer-modified-p)
                     :invocations (ac-racer-test-invocations)))
             :recorded (ac-racer-test-recorded))))
    "##,
        expect![[
            r#"OK (:configured (:first #1=(ac-source-racer . #2=(ac-source-words-in-same-mode-buffers)) :second #1# :mode t :buffer-local t) :untouched (:major-mode rust-mode :mode t :sources #2# :buffer-local nil) :global-default #2# :completes-in-configured (:line "    label.insert" :last-completion ("insert" "Function" "pub fn insert(&mut self, idx: usize, ch: char)" 58) :invocations 1) :silent-in-untouched (:line "    label.ins" :point 67 :sources #2# :candidates ("ins" "insert") :modified nil :invocations 1) :recorded (("01-request" . "argv: complete 3 13 [ORACLE-SANDBOX]/rust/src/main.rs [ORACLE-TMPDIR]/ac-racer-complete\nRUST_SRC_PATH: [ORACLE-SANDBOX]/rust/rust-src\nstdin: \ncwd: [ORACLE-SANDBOX]/rust/src\ntempfile([ORACLE-TMPDIR]/ac-racer-complete):\nfn main() {\n    let mut label = String::new();\n    label.ins\n}\n")))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        sets_up_the_source_and_completes_a_string_method_through_racer(),
        completes_a_non_ascii_identifier_and_reports_a_character_column(),
        offers_nothing_when_racer_finds_no_matches_or_exits_non_zero(),
        signals_file_missing_when_the_racer_binary_is_not_installed(),
        setup_adds_the_source_once_and_only_to_the_buffer_that_ran_it(),
    ]
}
