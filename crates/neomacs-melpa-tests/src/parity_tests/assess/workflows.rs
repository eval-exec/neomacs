use expect_test::expect;

use super::ParityBatchCase;

/// The library's whole reason to exist: a test that compares a string against
/// a file, and what the author is shown when it does not match.
///
/// Two real ERT tests are defined and run, one that passes and one that does
/// not, so the boolean and the explanation are asserted together -- a
/// predicate that always failed would produce an explanation too.  The
/// explanation is a `diff -c` between the two sides, and it is asserted
/// exactly apart from the two header lines carrying generated file names and
/// modification times.
///
/// The third case is the short-input branch: when the strings are small enough
/// assess still shells out to diff, and the "No newline at end of file" marker
/// in the output is a real consequence of how it writes them.
fn a_failing_comparison_explains_itself_with_a_real_diff() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_failing_comparison_explains_itself_with_a_real_diff",
        r##"
(let ((path (assess-test-path "greeting.txt")))
  (with-temp-buffer
    (insert "Grüße\nzweite Zeile\n")
    (write-region (point-min) (point-max) path nil 'silent))
  (assess-test-deftest assess-probe-pass
    (should (assess= "Grüße\nzweite Zeile\n" (assess-file path))))
  (assess-test-deftest assess-probe-fail
    (should (assess= "Grüße\nandere Zeile\n" (assess-file path))))
  (list :passing (assess-test-run 'assess-probe-pass)
        :failing (assess-test-run 'assess-probe-fail)
        :explain-when-equal (assess-explain= "same" "same")
        :explain-short-strings (assess-test-scrub (assess-explain= "a" "b"))))
"##,
        expect![[
            r#"OK (:passing (:name assess-probe-pass :passed t) :failing (:name assess-probe-fail :passed nil :error ert-test-failed :explanation "Strings:\nGrüße\nandere Zeile\n\nand\nGrüße\nzweite Zeile\n\nDiffer at:*** <FILE> <TIME>\n--- <FILE> <TIME>\n***************\n*** 1,2 ****\n  Grüße\n! andere Zeile\n--- 1,2 ----\n  Grüße\n! zweite Zeile\n\n" :value nil) :explain-when-equal t :explain-short-strings "Strings:\na\nand\nb\nDiffer at:*** <FILE> <TIME>\n--- <FILE> <TIME>\n***************\n*** 1 ****\n! a\n\\ No newline at end of file\n--- 1 ----\n! b\n\\ No newline at end of file\n\n")"#
        ]],
    )
}

fn the_filesystem_macro_builds_the_described_tree_and_removes_it_afterwards() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_filesystem_macro_builds_the_described_tree_and_removes_it_afterwards",
        r##"
(let (inside root)
  (assess-with-filesystem
   '("leer.txt"
     "unter/verzeichnis/"
     ("notiz mit leerzeichen.txt" "Grüße aus München\n")
     ("unter/tief/inhalt.txt" "zweite Datei\n")
     ("paket" ("a.txt" ("b.txt" "b hat Inhalt\n") "c/d.txt")))
   (setq root default-directory)
   (setq inside
         (list :directory-name-prefix
               (string-prefix-p "temp-fs-"
                                (file-name-nondirectory
                                 (directory-file-name default-directory)))
               :tree (sort (mapcar (lambda (file)
                                     (file-relative-name file default-directory))
                                   (directory-files-recursively default-directory "" t))
                           #'string<)
               :file-with-a-space (assess-test-read "notiz mit leerzeichen.txt")
               :nested-file (assess-test-read "unter/tief/inhalt.txt")
               :file-in-recursive-spec (assess-test-read "paket/b.txt")
               :empty-file (assess-test-read "leer.txt"))))
  (list :inside inside
        :removed-afterwards (not (file-exists-p root))))
"##,
        expect![[
            r#"OK (:inside (:directory-name-prefix t :tree ("leer.txt" "notiz mit leerzeichen.txt" "paket" "paket/a.txt" "paket/b.txt" "paket/c" "paket/c/d.txt" "unter" "unter/tief" "unter/tief/inhalt.txt" "unter/verzeichnis") :file-with-a-space "Grüße aus München\n" :nested-file "zweite Datei\n" :file-in-recursive-spec "b hat Inhalt\n" :empty-file "") :removed-afterwards t)"#
        ]],
    )
}

fn temp_buffers_carry_their_own_contents_and_die_even_when_the_body_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "temp_buffers_carry_their_own_contents_and_die_even_when_the_body_signals",
        r##"
(let ((before (assess-test-buffers)) contents)
  (assess-with-temp-buffers
   ((one (insert "erste"))
    (two (insert "zweite")))
   (setq contents (list (with-current-buffer one (buffer-string))
                        (with-current-buffer two (buffer-string))
                        (buffer-name one))))
  (let ((after-normal (assess-test-buffers))
        (signalled (assess-test-outcome
                    (assess-with-temp-buffers ((one (insert "x")))
                      (error "boom")))))
    (list :contents contents
          :restored-after-normal-exit (equal before after-normal)
          :signal signalled
          :restored-after-signal (equal before (assess-test-buffers))
          :as-temp-buffer (assess-as-temp-buffer "Zeile eins\nZeile zwei\n"
                            (list (buffer-string) (point) (buffer-name))))))
"##,
        expect![[
            r#"OK (:contents ("erste" "zweite" " *assess-with-temp-buffers*") :restored-after-normal-exit t :signal (:error error ("boom")) :restored-after-signal t :as-temp-buffer ("Zeile eins\nZeile zwei\n" 23 " *temp*"))"#
        ]],
    )
}

fn indentation_is_checked_against_the_mode_and_the_mismatch_shows_the_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "indentation_is_checked_against_the_mode_and_the_mismatch_shows_the_line",
        r##"
(let ((unindented "(defun greet (name)\n(message \"Hallo\"))\n")
      (indented "(defun greet (name)\n  (message \"Hallo\"))\n"))
  (list :matches (assess-indentation= 'emacs-lisp-mode unindented indented)
        :does-not-match (assess-indentation= 'emacs-lisp-mode unindented unindented)
        :explanation (assess-test-scrub
                      (assess-explain-indentation= 'emacs-lisp-mode unindented unindented))
        :roundtrip-of-indented (assess-roundtrip-indentation= 'emacs-lisp-mode indented)
        :roundtrip-of-unindented (assess-roundtrip-indentation= 'emacs-lisp-mode unindented)))
"##,
        expect![[
            r#"OK (:matches t :does-not-match nil :explanation "Strings:\n(defun greet (name)\n  (message \"Hallo\"))\n\nand\n(defun greet (name)\n(message \"Hallo\"))\n\nDiffer at:*** <FILE> <TIME>\n--- <FILE> <TIME>\n***************\n*** 1,2 ****\n  (defun greet (name)\n!   (message \"Hallo\"))\n--- 1,2 ----\n  (defun greet (name)\n! (message \"Hallo\"))\n\n" :roundtrip-of-indented t :roundtrip-of-unindented nil)"#
        ]],
    )
}

fn faces_are_checked_at_named_locations_and_a_mismatch_names_both_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "faces_are_checked_at_named_locations_and_a_mismatch_names_both_faces",
        r##"
(let ((source "(defun greet (name)\n  ;; a comment\n  (message \"Hallo %s\" name))\n"))
  (list :all-three-match
        (assess-face-at= source 'emacs-lisp-mode
                         '("greet" "a comment" "Hallo %s")
                         '(font-lock-function-name-face
                           font-lock-comment-face
                           font-lock-string-face))
        :wrong-face
        (assess-face-at= source 'emacs-lisp-mode '("greet") '(font-lock-string-face))
        :explanation
        (assess-test-plain
         (assess-explain-face-at= source 'emacs-lisp-mode
                                  '("greet") '(font-lock-string-face)))))
"##,
        expect![[
            r#"OK (:all-three-match t :wrong-face nil :explanation "Face does not match expected value\n\11Expected: font-lock-string-face\n\11Actual: font-lock-function-name-face\n\11Location: 8\n\11Line Context:   (message \"Hallo %s\" name))\n\n\11bol Position: 1\n")"#
        ]],
    )
}

fn a_related_file_is_edited_and_saved_without_touching_the_original() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_related_file_is_edited_and_saved_without_touching_the_original",
        r##"
(let ((original (assess-test-path "quelle.el")))
  (with-temp-buffer
    (insert "(defun greet (name)\n(message \"Hallo %s\" name))\n")
    (write-region (point-min) (point-max) original nil 'silent))
  (let* ((before (assess-test-buffers))
         (related (assess-make-related-file original))
         (in-buffer (assess-with-find-file related
                      (goto-char (point-max))
                      (insert ";; angehängt\n")
                      (basic-save-buffer)
                      (assess-test-plain (buffer-string)))))
    (list :keeps-the-extension (equal (file-name-extension original)
                                      (file-name-extension related))
          :keeps-the-base-name (equal (file-name-nondirectory original)
                                      (file-name-nondirectory related))
          :lives-elsewhere (not (equal (file-name-directory original)
                                       (file-name-directory related)))
          :buffer-contents in-buffer
          :copy-on-disk (assess-test-read related)
          :original-on-disk (assess-test-read original)
          :no-buffer-left-behind (equal before (assess-test-buffers)))))
"##,
        expect![[
            r#"OK (:keeps-the-extension t :keeps-the-base-name nil :lives-elsewhere t :buffer-contents "(defun greet (name)\n(message \"Hallo %s\" name))\n;; angehängt\n" :copy-on-disk "(defun greet (name)\n(message \"Hallo %s\" name))\n;; angehängt\n" :original-on-disk "(defun greet (name)\n(message \"Hallo %s\" name))\n" :no-buffer-left-behind t)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_failing_comparison_explains_itself_with_a_real_diff(),
        the_filesystem_macro_builds_the_described_tree_and_removes_it_afterwards(),
        temp_buffers_carry_their_own_contents_and_die_even_when_the_body_signals(),
        indentation_is_checked_against_the_mode_and_the_mismatch_shows_the_line(),
        faces_are_checked_at_named_locations_and_a_mismatch_names_both_faces(),
        a_related_file_is_edited_and_saved_without_touching_the_original(),
    ]
}
