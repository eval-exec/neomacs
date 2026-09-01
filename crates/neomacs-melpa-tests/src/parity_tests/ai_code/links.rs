use expect_test::expect;

use super::ParityBatchCase;

fn session_link_normalization_handles_terminal_wrapping_urls_and_spaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_link_normalization_handles_terminal_wrapping_urls_and_spaces",
        r##"
(progn
  (require 'ai-code-session-link)
  (list
   (mapcar
    #'ai-code-session-link--normalize-file
    '(" @src/Foo.java "
      "file:///workspace/My%20Image.png"
      "file://localhost/workspace/Foo.java"
      "file:/workspace/Foo.java"
      "file:///workspace/image-\nwrapped.png"
      "./screens/My\\ Image.png"
      "   "))
   (ai-code-session-link--normalize-link-text
    "/workspace/My \n Image.png")
   (ai-code-session-link--normalize-url-link-text
    "https://example.com/app-   \n  page=true")))
"##,
        expect![[
            r#"OK (("src/Foo.java" "/workspace/My Image.png" "/workspace/Foo.java" "/workspace/Foo.java" "/workspace/image-wrapped.png" "./screens/My Image.png" nil) "/workspace/My Image.png" "https://example.com/app-page=true")"#
        ]],
    )
}

fn session_link_parser_covers_editor_compiler_and_github_location_syntaxes() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_link_parser_covers_editor_compiler_and_github_location_syntaxes",
        r##"
(progn
  (require 'ai-code-session-link)
  (mapcar
   #'ai-code--parse-session-link
   '("https://example.com/review/42"
     "src/lib.rs:42:7"
     "src/lib.rs:42-60"
     "src/lib.rs:L42-L60"
     "src/lib.rs#L42-L60"
     "src/lib.rs(42,7)"
     "src/lib.rs(42)"
     "./README.md"
     "PaymentService")))
"##,
        expect![[
            r#"OK ((:url "https://example.com/review/42") (:file "src/lib.rs" :line-start 42 :column-start 7) (:file "src/lib.rs" :line-start 42) (:file "src/lib.rs:L42-L60") (:file "src/lib.rs" :line-start 42) (:file "src/lib.rs" :line-start 42 :column-start 7) (:file "src/lib.rs" :line-start 42) (:file "./README.md") nil)"#
        ]],
    )
}

fn session_linkification_marks_real_project_files_locations_and_urls() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_linkification_marks_real_project_files_locations_and_urls",
        r##"
(progn
  (require 'ai-code-session-link)
  (let* ((root (make-temp-file "ai-code-links-" t))
         (source (expand-file-name "src/payment.rs" root)))
    (unwind-protect
        (progn
          (make-directory (file-name-directory source) t)
          (with-temp-file source
            (insert "fn settle() {}\n"))
          (with-temp-buffer
            (setq-local ai-code-backends-infra--session-directory root)
            (insert
             "Inspect src/payment.rs and src/payment.rs:42:7.\nReview https://example.com/pull/77, then continue.\n")
            (ai-code-session-link--linkify-session-region
             (point-min) (point-max))
            (let (result)
              (dolist (needle
                       '("src/payment.rs"
                         "src/payment.rs:42:7"
                         "https://example.com/pull/77"))
                (goto-char (point-min))
                (search-forward needle)
                (let ((position (- (point) (length needle))))
                  (push
                   (list needle
                         (get-text-property
                          position 'ai-code-session-link)
                         (get-text-property position 'face)
                         (get-text-property position 'mouse-face))
                   result)))
              (nreverse result))))
      (delete-directory root t))))
"##,
        expect![[
            r#"OK (("src/payment.rs" "src/payment.rs" link highlight) ("src/payment.rs:42:7" "src/payment.rs:42:7" link highlight) ("https://example.com/pull/77" "https://example.com/pull/77" link highlight))"#
        ]],
    )
}

fn session_linkification_reconstructs_wrapped_urls_without_claiming_prose() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_linkification_reconstructs_wrapped_urls_without_claiming_prose",
        r##"
(progn
  (require 'ai-code-session-link)
  (with-temp-buffer
    (insert
     "https://example.com/repo/project-int\n")
    (insert "erface.el\n")
    (insert "https://example.com/repo/pkg\n")
    (insert "-interface.el trailing prose\n")
    (ai-code-session-link--linkify-session-region (point-min) (point-max))
    (let (result)
      (dolist (needle
               '("https://example.com/repo/project-int"
                 "erface.el"
                 "https://example.com/repo/pkg"
                 "-interface.el"))
        (goto-char (point-min))
        (search-forward needle)
        (push
         (list needle
               (get-text-property
                (- (point) (length needle))
                'ai-code-session-link))
         result))
      (nreverse result))))
"##,
        expect![[
            r#"OK (("https://example.com/repo/project-int" "https://example.com/repo/project-interface.el") ("erface.el" "https://example.com/repo/project-interface.el") ("https://example.com/repo/pkg" "https://example.com/repo/pkg") ("-interface.el" "-interface.el"))"#
        ]],
    )
}

fn session_symbol_filters_are_language_aware_near_file_references() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_symbol_filters_are_language_aware_near_file_references",
        r##"
(progn
  (require 'ai-code-session-link)
  (let ((cases
         '((emacs-lisp-mode
            "ai-code-session-register" "make-ai-code-session" "42" "*scratch*")
           (python-mode
            "retry_payment" "PaymentService" "simple" "UPPER_CASE")
           (java-mode
            "PaymentService" "retryPayment" "Simple" "ALLCAPS"))))
    (mapcar
     (lambda (case)
       (let ((major-mode (car case)))
         (cons major-mode
               (mapcar
                (lambda (candidate)
                  (list candidate
                        (and
                         (ai-code-session-link--symbol-candidate-p candidate)
                         t)))
                (cdr case)))))
     cases)))
"##,
        expect![[
            r#"OK ((emacs-lisp-mode ("ai-code-session-register" t) ("make-ai-code-session" t) ("42" nil) ("*scratch*" nil)) (python-mode ("retry_payment" t) ("PaymentService" t) ("simple" nil) ("UPPER_CASE" nil)) (java-mode ("PaymentService" t) ("retryPayment" nil) ("Simple" nil) ("ALLCAPS" nil)))"#
        ]],
    )
}

fn session_image_safety_enforces_extension_locality_and_size_budget() -> ParityBatchCase {
    ParityBatchCase::value(
        "session_image_safety_enforces_extension_locality_and_size_budget",
        r##"
(progn
  (require 'ai-code-session-link)
  (let* ((root (make-temp-file "ai-code-image-link-" t))
         (small (expand-file-name "diagram.png" root))
         (large (expand-file-name "large.png" root))
         (text (expand-file-name "notes.txt" root))
         (ai-code-session-link-ghostel-image-preview-max-bytes 8))
    (unwind-protect
        (progn
          (with-temp-file small (insert "PNG"))
          (with-temp-file large (insert "0123456789abcdef"))
          (with-temp-file text (insert "PNG"))
          (list
           (ai-code-session-link--image-extension-p small)
           (ai-code-session-link--safe-local-image-file-p small)
           (ai-code-session-link--safe-local-image-file-p large)
           (ai-code-session-link--safe-local-image-file-p text)
           (car (ai-code-session-link--image-preview-file-signature small))))
      (delete-directory root t))))
"##,
        expect![[r#"OK (("png" "ppm" "svg" "tif" "tiff" "webp" "xbm" "xpm") t nil nil 3)"#]],
    )
}

pub(super) fn links_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        session_link_normalization_handles_terminal_wrapping_urls_and_spaces(),
        session_link_parser_covers_editor_compiler_and_github_location_syntaxes(),
        session_linkification_marks_real_project_files_locations_and_urls(),
        session_linkification_reconstructs_wrapped_urls_without_claiming_prose(),
        session_symbol_filters_are_language_aware_near_file_references(),
        session_image_safety_enforces_extension_locality_and_size_budget(),
    ]
}
