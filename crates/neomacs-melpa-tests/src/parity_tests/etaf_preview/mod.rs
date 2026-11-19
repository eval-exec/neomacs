//! Behavioural parity for the pinned `etaf-md` and `etaf-org-preview` readers.
//!
//! Both packages expose headless sessions, so the probes compare the parsed
//! state (title/keywords/outline/counts), the documented error contract and
//! the exact text mounted into the preview buffer.  Mermaid/KaTeX drawing and
//! live repaint need external tools or a GUI lane and stay out of this batch.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defconst neomacs-etaf-md-document
  "# Title

Intro with **bold**, *emphasis*, `code` and a [link](https://example.test).

## Section A

- one
- two
  - nested

## Section B

```elisp
(+ 1 2)
```

> quoted

| a | b |
|---|---|
| 1 | 2 |
")

(defconst neomacs-etaf-org-document
  "#+TITLE: Demo
#+AUTHOR: Ada
#+DATE: 2026-11-19
#+KEYWORDS: demo parity

* TODO First heading :work:
Some text with a [[https://example.test][link]].
** DONE Second heading :home:
- [X] done item
- [ ] open item
| a | b |
|---+---|
| 1 | 2 |
#+BEGIN_SRC elisp
(+ 1 2)
#+END_SRC
")

(defun neomacs-etaf-preview-buffer-text (session view)
  "Mount VIEW for SESSION in a fresh buffer and return its text."
  (let ((buffer (generate-new-buffer " *etaf-preview-parity*")))
    (unwind-protect
        (progn
          (etaf-mount buffer view)
          (with-current-buffer buffer
            (buffer-substring-no-properties (point-min) (point-max))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"####;

fn md_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-md", "0.2.0"), "etaf-md.el")
        .expect("prepare pinned etaf-md source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

fn org_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-org-preview", "0.3.1"), "etaf-org-preview.el")
        .expect("prepare pinned etaf-org-preview source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_md_preview_batch() {
    let cases = vec![
        // The headless session parses the document and exposes its outline.
        ParityBatchCase::value(
            "markdown_state_and_outline",
            r####"
(let* ((session (etaf-md-create :text neomacs-etaf-md-document))
       (state (etaf-md-state session)))
  (list (plist-get state :title)
        (plist-get state :theme)
        (plist-get state :flavor)
        (plist-get state :background)
        (plist-get state :outline)
        (plist-get state :error)
        (plist-get state :blocks)))
"####,
            expect![[
                r#"OK ("Title" dark nil nil ((1 "Title" "title") (2 "Section A" "section-a") (2 "Section B" "section-b")) nil 8)"#
            ]],
        ),
        // The mounted page's text is the user-visible contract.
        ParityBatchCase::value(
            "markdown_rendered_text",
            r####"
(let ((session (etaf-md-create :text neomacs-etaf-md-document)))
  (neomacs-etaf-preview-buffer-text session (etaf-md-view session)))
"####,
            expect![[
                r#"OK " \n Title  \n ──────────────────────────────────────────────────────────────────────────── \n    \n Intro with bold, emphasis, code and a link.  \n    \n Section A  \n ──────────────────────────────────────────────────────────────────────────── \n    \n • one  \n • two   \n  • nested  \n    \n Section B  \n ──────────────────────────────────────────────────────────────────────────── \n    \n │ elisp  \n │  \n │ (+ 1 2)  \n │  \n    \n │  quoted  \n    \n │ a  │ b  \n │ 1  │ 2  \n    \n ""#
            ]],
        ),
        // A fixed palette changes the theme facts, not the document.
        ParityBatchCase::value(
            "markdown_theme",
            r####"
(let* ((session (etaf-md-create :text neomacs-etaf-md-document :theme 'light))
       (state (etaf-md-state session)))
  (list (plist-get state :theme)
        (plist-get state :background)
        (plist-get state :width)
        (plist-get state :outline)))
"####,
            expect![[
                r#"OK (light palette 92 ((1 "Title" "title") (2 "Section A" "section-a") (2 "Section B" "section-b")))"#
            ]],
        ),
        // A missing file is the documented error, not a silent empty page.
        ParityBatchCase::value(
            "markdown_missing_file_error",
            r####"
(condition-case err
    (etaf-md-create :file "/nonexistent/etaf-md-parity.md")
  (error (list (car err) (cdr err))))
"####,
            expect![[r#"OK (etaf-md-error ("Cannot read file: /nonexistent/etaf-md-parity.md"))"#]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-md parity batch");
    assert_oracle_batch_cases(md_oracle(), test_name, "etaf-md parity", &cases);
}

#[test]
fn etaf_org_preview_batch() {
    let cases = vec![
        // Headless Org parsing: metadata, outline with TODO/tags, and counts.
        ParityBatchCase::value(
            "org_state_and_outline",
            r####"
(let* ((session (etaf-org-preview-create :text neomacs-etaf-org-document))
       (state (etaf-org-preview-state session)))
  (list (plist-get state :title)
        (plist-get state :author)
        (plist-get state :date)
        (plist-get state :keywords)
        (plist-get state :outline)
        (plist-get state :counts)
        (plist-get state :error)
        (plist-get state :blocks)))
"####,
            expect![[
                r#"OK ("Demo" "Ada" "2026-11-19" ("demo" "parity") ((1 "First heading" "s1" "TODO" ("work")) (2 "Second heading" "s1.1" "DONE" ("home"))) (:todo 1 :done 1 :deadline 0 :overdue 0) nil 10)"#
            ]],
        ),
        // The mounted page's text, including chips, lists and source blocks.
        ParityBatchCase::value(
            "org_rendered_text",
            r####"
(let ((session (etaf-org-preview-create :text neomacs-etaf-org-document)))
  (neomacs-etaf-preview-buffer-text
   session (etaf-org-preview-view session)))
"####,
            expect![[
                r#"OK " \n Demo  \n   \n Ada · 2026-11-19  \n   \n  demo   parity   \n   \n   \n │  目录  \n │  First heading  \n │   Second heading  \n   \n   \n  TODO  First heading    :work:  \n ──────────────────────────────────────────────────────────────────────────── \n   \n Some text with a link.  \n   \n  DONE  Second heading    :home:  \n ──────────────────────────────────────────────────────────────────────────── \n   \n 完 done item  \n 待 open item  \n   \n │ a    b │  \n │ 1    2 │  \n   \n │ elisp  │ \n │ (+ 1 2)  │ \n ""#
            ]],
        ),
        // Refresh re-parses text sessions deterministically.
        ParityBatchCase::value(
            "org_refresh",
            r####"
(let* ((session (etaf-org-preview-create :text neomacs-etaf-org-document))
       (refreshed (etaf-org-preview-refresh session))
       (state (etaf-org-preview-state session)))
  (list (eq refreshed session)
        (plist-get state :title)
        (plist-get state :outline)))
"####,
            expect![[
                r#"OK (t "Demo" ((1 "First heading" "s1" "TODO" ("work")) (2 "Second heading" "s1.1" "DONE" ("home"))))"#
            ]],
        ),
        // Capability facts are the same in both engines: they describe the
        // host, not the editor.
        ParityBatchCase::value(
            "org_doctor",
            r####"
(etaf-org-preview-doctor)
"####,
            expect![[
                r#"OK (:required nil :optional (("org" nil "Optional: load Org to resolve id: links and honour org-todo-keywords.") ("ffmpeg" t "Optional: install ffmpeg to preview video files and terminal images.")))"#
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("etaf-org-preview parity batch");
    assert_oracle_batch_cases(org_oracle(), test_name, "etaf-org-preview parity", &cases);
}
