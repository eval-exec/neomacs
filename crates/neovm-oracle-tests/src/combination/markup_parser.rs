//! Oracle parity tests for a markup language parser in Elisp.
//!
//! Implements a simplified Markdown-like parser that handles headers,
//! bold, italic, links, lists, and code blocks. Produces an AST as
//! nested lists, renders AST back to plain text, and implements a
//! visitor pattern over the AST.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Parse inline markup: bold, italic, code spans
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_parse_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse inline markup elements: **bold**, *italic*, `code`.
    // Returns an AST of (text ...) | (bold ...) | (italic ...) | (code ...) nodes.
    let form = r####"(progn
  (fset 'neovm--test-mp-parse-inline
    (lambda (text)
      "Parse inline markup from TEXT. Returns list of AST nodes."
      (let ((result nil)
            (i 0)
            (len (length text))
            (current ""))
        (while (< i len)
          (let ((ch (aref text i)))
            (cond
              ;; ** bold **
              ((and (= ch ?*) (< (1+ i) len) (= (aref text (1+ i)) ?*))
               (when (> (length current) 0)
                 (setq result (cons (list 'text current) result))
                 (setq current ""))
               (let ((end (let ((j (+ i 2)) (found nil))
                            (while (and (< (1+ j) len) (not found))
                              (when (and (= (aref text j) ?*)
                                         (= (aref text (1+ j)) ?*))
                                (setq found j))
                              (setq j (1+ j)))
                            found)))
                 (if end
                     (progn
                       (setq result (cons (list 'bold (substring text (+ i 2) end))
                                          result))
                       (setq i (+ end 2)))
                   ;; No closing **: treat as text
                   (setq current (concat current "**"))
                   (setq i (+ i 2)))))

              ;; * italic *
              ((and (= ch ?*) (or (>= (1+ i) len) (/= (aref text (1+ i)) ?*)))
               (when (> (length current) 0)
                 (setq result (cons (list 'text current) result))
                 (setq current ""))
               (let ((end (let ((j (1+ i)) (found nil))
                            (while (and (< j len) (not found))
                              (when (and (= (aref text j) ?*)
                                         (or (>= (1+ j) len)
                                             (/= (aref text (1+ j)) ?*)))
                                (setq found j))
                              (setq j (1+ j)))
                            found)))
                 (if end
                     (progn
                       (setq result (cons (list 'italic (substring text (1+ i) end))
                                          result))
                       (setq i (1+ end)))
                   (setq current (concat current "*"))
                   (setq i (1+ i)))))

              ;; ` code `
              ((= ch ?`)
               (when (> (length current) 0)
                 (setq result (cons (list 'text current) result))
                 (setq current ""))
               (let ((end (let ((j (1+ i)) (found nil))
                            (while (and (< j len) (not found))
                              (when (= (aref text j) ?`)
                                (setq found j))
                              (setq j (1+ j)))
                            found)))
                 (if end
                     (progn
                       (setq result (cons (list 'code (substring text (1+ i) end))
                                          result))
                       (setq i (1+ end)))
                   (setq current (concat current "`"))
                   (setq i (1+ i)))))

              ;; Regular character
              (t
               (setq current (concat current (char-to-string ch)))
               (setq i (1+ i))))))
        (when (> (length current) 0)
          (setq result (cons (list 'text current) result)))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Plain text
        (funcall 'neovm--test-mp-parse-inline "hello world")
        ;; Bold
        (funcall 'neovm--test-mp-parse-inline "hello **bold** world")
        ;; Italic
        (funcall 'neovm--test-mp-parse-inline "hello *italic* world")
        ;; Code
        (funcall 'neovm--test-mp-parse-inline "use `code` here")
        ;; Mixed
        (funcall 'neovm--test-mp-parse-inline "a **bold** and *italic* and `code` end")
        ;; Adjacent
        (funcall 'neovm--test-mp-parse-inline "**bold***italic*")
        ;; Unclosed bold treated as text
        (funcall 'neovm--test-mp-parse-inline "hello **unclosed")
        ;; Empty
        (funcall 'neovm--test-mp-parse-inline ""))
    (fmakunbound 'neovm--test-mp-parse-inline)))"####;
    let expect = expect_test::expect![[
        r#""OK (((text \"hello world\")) ((text \"hello \") (bold \"bold\") (text \" world\")) ((text \"hello \") (italic \"italic\") (text \" world\")) ((text \"use \") (code \"code\") (text \" here\")) ((text \"a \") (bold \"bold\") (text \" and \") (italic \"italic\") (text \" and \") (code \"code\") (text \" end\")) ((bold \"bold\") (italic \"italic\")) ((text \"hello \") (text \"**unclosed\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse block-level elements: headers, lists, paragraphs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_parse_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse block-level elements from a multi-line string:
    // # Header, - List item, blank-line-separated paragraphs.
    let form = r####"(progn
  (fset 'neovm--test-mp-split-lines
    (lambda (text)
      "Split TEXT into lines."
      (let ((lines nil) (current "") (i 0) (len (length text)))
        (while (< i len)
          (let ((ch (aref text i)))
            (if (= ch ?\n)
                (progn
                  (setq lines (cons current lines))
                  (setq current ""))
              (setq current (concat current (char-to-string ch)))))
          (setq i (1+ i)))
        (setq lines (cons current lines))
        (nreverse lines))))

  (fset 'neovm--test-mp-parse-blocks
    (lambda (text)
      "Parse block-level markup. Returns list of AST blocks."
      (let ((lines (funcall 'neovm--test-mp-split-lines text))
            (blocks nil)
            (para-lines nil))
        (dolist (line lines)
          (cond
            ;; Header: # ## ###
            ((string-match "^\\(#+\\) \\(.*\\)$" line)
             ;; Flush paragraph
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (mapconcat 'identity (nreverse para-lines) " "))
                                  blocks))
               (setq para-lines nil))
             (let ((level (length (match-string 1 line)))
                   (content (match-string 2 line)))
               (setq blocks (cons (list 'header level content) blocks))))

            ;; List item: - text
            ((string-match "^- \\(.*\\)$" line)
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (mapconcat 'identity (nreverse para-lines) " "))
                                  blocks))
               (setq para-lines nil))
             (setq blocks (cons (list 'list-item (match-string 1 line)) blocks)))

            ;; Blank line: end paragraph
            ((string-match "^[ \t]*$" line)
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (mapconcat 'identity (nreverse para-lines) " "))
                                  blocks))
               (setq para-lines nil)))

            ;; Regular text: accumulate paragraph
            (t
             (setq para-lines (cons line para-lines)))))

        ;; Flush final paragraph
        (when para-lines
          (setq blocks (cons (list 'paragraph
                                    (mapconcat 'identity (nreverse para-lines) " "))
                              blocks)))
        (nreverse blocks))))

  (unwind-protect
      (list
        ;; Headers
        (funcall 'neovm--test-mp-parse-blocks "# Title\n## Subtitle\n### Sub-sub")
        ;; List items
        (funcall 'neovm--test-mp-parse-blocks "- first\n- second\n- third")
        ;; Paragraphs separated by blank lines
        (funcall 'neovm--test-mp-parse-blocks "Hello world\nmore text\n\nNew paragraph")
        ;; Mixed
        (funcall 'neovm--test-mp-parse-blocks
          "# My Doc\n\nIntro paragraph\nwith two lines\n\n## Section\n\n- item one\n- item two\n\nConclusion")
        ;; Single line
        (funcall 'neovm--test-mp-parse-blocks "just text")
        ;; Empty
        (funcall 'neovm--test-mp-parse-blocks ""))
    (fmakunbound 'neovm--test-mp-split-lines)
    (fmakunbound 'neovm--test-mp-parse-blocks)))"####;
    let expect = expect_test::expect![[
        r#""OK (((header 1 \"Title\") (header 2 \"Subtitle\") (header 3 \"Sub-sub\")) ((list-item \"first\") (list-item \"second\") (list-item \"third\")) ((paragraph \"Hello world more text\") (paragraph \"New paragraph\")) ((header 1 \"My Doc\") (paragraph \"Intro paragraph with two lines\") (header 2 \"Section\") (list-item \"item one\") (list-item \"item two\") (paragraph \"Conclusion\")) ((paragraph \"just text\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse links: [text](url)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_parse_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse Markdown-style links [text](url) from inline text.
    let form = r####"(progn
  (fset 'neovm--test-mp-parse-links
    (lambda (text)
      "Parse text with [label](url) links into AST nodes."
      (let ((result nil)
            (i 0)
            (len (length text))
            (current ""))
        (while (< i len)
          (let ((ch (aref text i)))
            (if (= ch ?\[)
                ;; Try to parse [text](url)
                (let ((bracket-end
                       (let ((j (1+ i)) (found nil))
                         (while (and (< j len) (not found))
                           (when (= (aref text j) ?\])
                             (setq found j))
                           (setq j (1+ j)))
                         found)))
                  (if (and bracket-end
                           (< (1+ bracket-end) len)
                           (= (aref text (1+ bracket-end)) ?\())
                      ;; Found ](, now find closing )
                      (let ((paren-end
                             (let ((j (+ bracket-end 2)) (found nil))
                               (while (and (< j len) (not found))
                                 (when (= (aref text j) ?\))
                                   (setq found j))
                                 (setq j (1+ j)))
                               found)))
                        (if paren-end
                            (progn
                              (when (> (length current) 0)
                                (setq result (cons (list 'text current) result))
                                (setq current ""))
                              (let ((link-text (substring text (1+ i) bracket-end))
                                    (link-url (substring text (+ bracket-end 2) paren-end)))
                                (setq result (cons (list 'link link-text link-url) result))
                                (setq i (1+ paren-end))))
                          ;; No closing paren: treat [ as text
                          (setq current (concat current "["))
                          (setq i (1+ i))))
                    ;; No ]( pattern: treat [ as text
                    (setq current (concat current "["))
                    (setq i (1+ i))))
              ;; Regular character
              (setq current (concat current (char-to-string ch)))
              (setq i (1+ i)))))
        (when (> (length current) 0)
          (setq result (cons (list 'text current) result)))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Simple link
        (funcall 'neovm--test-mp-parse-links "[click here](http://example.com)")
        ;; Link with surrounding text
        (funcall 'neovm--test-mp-parse-links "Visit [Example](http://example.com) now")
        ;; Multiple links
        (funcall 'neovm--test-mp-parse-links "[a](http://a.com) and [b](http://b.com)")
        ;; No links
        (funcall 'neovm--test-mp-parse-links "just plain text")
        ;; Broken link syntax: no closing paren
        (funcall 'neovm--test-mp-parse-links "[text](no-close")
        ;; Bracket without paren
        (funcall 'neovm--test-mp-parse-links "[just bracket] text")
        ;; Adjacent links
        (funcall 'neovm--test-mp-parse-links "[a](x)[b](y)")
        ;; Link with path
        (funcall 'neovm--test-mp-parse-links "See [docs](/path/to/docs)"))
    (fmakunbound 'neovm--test-mp-parse-links)))"####;
    let expect = expect_test::expect![[
        r#""OK (((link \"click here\" \"http://example.com\")) ((text \"Visit \") (link \"Example\" \"http://example.com\") (text \" now\")) ((link \"a\" \"http://a.com\") (text \" and \") (link \"b\" \"http://b.com\")) ((text \"just plain text\")) ((text \"[text](no-close\")) ((text \"[just bracket] text\")) ((link \"a\" \"x\") (link \"b\" \"y\")) ((text \"See \") (link \"docs\" \"/path/to/docs\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full document parser combining all elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_full_document_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a complete document with headers, paragraphs, lists.
    // Each block's text content is further parsed for inline markup.
    let form = r####"(progn
  (fset 'neovm--test-mf-split-lines
    (lambda (text)
      (let ((lines nil) (current "") (i 0) (len (length text)))
        (while (< i len)
          (if (= (aref text i) ?\n)
              (progn (setq lines (cons current lines)) (setq current ""))
            (setq current (concat current (char-to-string (aref text i)))))
          (setq i (1+ i)))
        (setq lines (cons current lines))
        (nreverse lines))))

  (fset 'neovm--test-mf-parse-inline
    (lambda (text)
      "Simple inline parser: **bold**, *italic*, `code`."
      (let ((nodes nil) (current "") (i 0) (len (length text)))
        (while (< i len)
          (let ((ch (aref text i)))
            (cond
              ((and (= ch ?*) (< (1+ i) len) (= (aref text (1+ i)) ?*))
               (when (> (length current) 0)
                 (setq nodes (cons (list 'text current) nodes)
                       current ""))
               (let ((end (let ((j (+ i 2)) (f nil))
                            (while (and (< (1+ j) len) (not f))
                              (when (and (= (aref text j) ?*) (= (aref text (1+ j)) ?*))
                                (setq f j))
                              (setq j (1+ j))) f)))
                 (if end
                     (progn (setq nodes (cons (list 'bold (substring text (+ i 2) end)) nodes))
                            (setq i (+ end 2)))
                   (setq current (concat current "**") i (+ i 2)))))
              ((= ch ?*)
               (when (> (length current) 0)
                 (setq nodes (cons (list 'text current) nodes)
                       current ""))
               (let ((end (let ((j (1+ i)) (f nil))
                            (while (and (< j len) (not f))
                              (when (= (aref text j) ?*) (setq f j))
                              (setq j (1+ j))) f)))
                 (if end
                     (progn (setq nodes (cons (list 'italic (substring text (1+ i) end)) nodes))
                            (setq i (1+ end)))
                   (setq current (concat current "*") i (1+ i)))))
              ((= ch ?`)
               (when (> (length current) 0)
                 (setq nodes (cons (list 'text current) nodes)
                       current ""))
               (let ((end (let ((j (1+ i)) (f nil))
                            (while (and (< j len) (not f))
                              (when (= (aref text j) ?`) (setq f j))
                              (setq j (1+ j))) f)))
                 (if end
                     (progn (setq nodes (cons (list 'code (substring text (1+ i) end)) nodes))
                            (setq i (1+ end)))
                   (setq current (concat current "`") i (1+ i)))))
              (t (setq current (concat current (char-to-string ch)))
                 (setq i (1+ i))))))
        (when (> (length current) 0)
          (setq nodes (cons (list 'text current) nodes)))
        (nreverse nodes))))

  (fset 'neovm--test-mf-parse-document
    (lambda (text)
      "Parse full document into AST."
      (let ((lines (funcall 'neovm--test-mf-split-lines text))
            (blocks nil)
            (para-lines nil))
        (dolist (line lines)
          (cond
            ((string-match "^\\(#+\\) \\(.*\\)$" line)
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-mf-parse-inline
                                                 (mapconcat 'identity (nreverse para-lines) " ")))
                                  blocks)
                     para-lines nil))
             (setq blocks (cons (list 'header (length (match-string 1 line))
                                      (funcall 'neovm--test-mf-parse-inline
                                               (match-string 2 line)))
                                blocks)))
            ((string-match "^- \\(.*\\)$" line)
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-mf-parse-inline
                                                 (mapconcat 'identity (nreverse para-lines) " ")))
                                  blocks)
                     para-lines nil))
             (setq blocks (cons (list 'list-item
                                      (funcall 'neovm--test-mf-parse-inline
                                               (match-string 1 line)))
                                blocks)))
            ((string-match "^[ \t]*$" line)
             (when para-lines
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-mf-parse-inline
                                                 (mapconcat 'identity (nreverse para-lines) " ")))
                                  blocks)
                     para-lines nil)))
            (t (setq para-lines (cons line para-lines)))))
        (when para-lines
          (setq blocks (cons (list 'paragraph
                                    (funcall 'neovm--test-mf-parse-inline
                                             (mapconcat 'identity (nreverse para-lines) " ")))
                              blocks)))
        (nreverse blocks))))

  (unwind-protect
      (list
        ;; Simple doc with header and paragraph
        (funcall 'neovm--test-mf-parse-document
          "# Hello\n\nThis is a **bold** paragraph.")
        ;; Doc with multiple sections
        (funcall 'neovm--test-mf-parse-document
          "# Title\n\nIntro text.\n\n## Section 1\n\nSome *italic* text.\n\n## Section 2\n\n- item with `code`\n- plain item")
        ;; Header with inline markup
        (funcall 'neovm--test-mf-parse-document
          "# A **bold** title\n\nNormal text.")
        ;; Only list items
        (funcall 'neovm--test-mf-parse-document
          "- first\n- **second**\n- *third*")
        ;; Multi-line paragraph with inline
        (funcall 'neovm--test-mf-parse-document
          "Hello **bold** world\nand *italic* text\n\nNew paragraph"))
    (fmakunbound 'neovm--test-mf-split-lines)
    (fmakunbound 'neovm--test-mf-parse-inline)
    (fmakunbound 'neovm--test-mf-parse-document)))"####;
    let expect = expect_test::expect![[
        r#""OK (((header 1 ((text \"Hello\"))) (paragraph ((text \"This is a \") (bold \"bold\") (text \" paragraph.\")))) ((header 1 ((text \"Title\"))) (paragraph ((text \"Intro text.\"))) (header 2 ((text \"Section 1\"))) (paragraph ((text \"Some \") (italic \"italic\") (text \" text.\"))) (header 2 ((text \"Section 2\"))) (list-item ((text \"item with \") (code \"code\"))) (list-item ((text \"plain item\")))) ((header 1 ((text \"A \") (bold \"bold\") (text \" title\"))) (paragraph ((text \"Normal text.\")))) ((list-item ((text \"first\"))) (list-item ((bold \"second\"))) (list-item ((italic \"third\")))) ((paragraph ((text \"Hello \") (bold \"bold\") (text \" world and \") (italic \"italic\") (text \" text\"))) (paragraph ((text \"New paragraph\")))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Render AST back to plain text (strip all markup)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_render_plaintext() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given an AST, render it back to plain text: strip bold/italic/code
    // markers, print headers with indentation, format list items with bullets.
    let form = r####"(progn
  (fset 'neovm--test-mr-render-inline
    (lambda (nodes)
      "Render inline AST nodes to plain text string."
      (mapconcat
        (lambda (node)
          (cond
            ((eq (car node) 'text) (cadr node))
            ((eq (car node) 'bold) (cadr node))
            ((eq (car node) 'italic) (cadr node))
            ((eq (car node) 'code) (cadr node))
            ((eq (car node) 'link) (format "%s (%s)" (cadr node) (nth 2 node)))
            (t "")))
        nodes "")))

  (fset 'neovm--test-mr-render-block
    (lambda (block)
      "Render one block to plain text string."
      (let ((type (car block)))
        (cond
          ((eq type 'header)
           (let ((level (cadr block))
                 (content (funcall 'neovm--test-mr-render-inline (nth 2 block))))
             (format "%s %s" (make-string level ?#) content)))
          ((eq type 'paragraph)
           (funcall 'neovm--test-mr-render-inline (cadr block)))
          ((eq type 'list-item)
           (format "  * %s" (funcall 'neovm--test-mr-render-inline (cadr block))))
          (t "")))))

  (fset 'neovm--test-mr-render-document
    (lambda (ast)
      "Render full AST to plain text."
      (mapconcat (lambda (block)
                   (funcall 'neovm--test-mr-render-block block))
                 ast "\n")))

  (unwind-protect
      (let ((ast1 (list
                    (list 'header 1 (list (list 'text "My Title")))
                    (list 'paragraph (list (list 'text "Hello ")
                                           (list 'bold "world")
                                           (list 'text " and ")
                                           (list 'italic "more")))
                    (list 'header 2 (list (list 'text "Section")))
                    (list 'list-item (list (list 'text "item with ")
                                           (list 'code "code")))
                    (list 'list-item (list (list 'text "plain item")))))
            (ast2 (list
                    (list 'paragraph (list (list 'text "Just text")))
                    (list 'paragraph (list (list 'bold "All bold")))))
            (ast3 (list
                    (list 'header 1 (list (list 'bold "Bold") (list 'text " Header")))
                    (list 'list-item (list (list 'italic "italic") (list 'text " item"))))))
        (list
          (funcall 'neovm--test-mr-render-document ast1)
          (funcall 'neovm--test-mr-render-document ast2)
          (funcall 'neovm--test-mr-render-document ast3)
          ;; Empty AST
          (funcall 'neovm--test-mr-render-document nil)
          ;; Inline rendering
          (funcall 'neovm--test-mr-render-inline
                   (list (list 'text "a") (list 'bold "b") (list 'code "c")))))
    (fmakunbound 'neovm--test-mr-render-inline)
    (fmakunbound 'neovm--test-mr-render-block)
    (fmakunbound 'neovm--test-mr-render-document)))"####;
    let expect = expect_test::expect![[
        r##""OK (\"# My Title\\nHello world and more\\n## Section\\n  * item with code\\n  * plain item\" \"Just text\\nAll bold\" \"# Bold Header\\n  * italic item\" \"\" \"abc\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Visitor pattern over AST: collect statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_visitor_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a visitor that walks the AST and collects statistics:
    // count of each node type, total text length, list of headers,
    // all code snippets.
    let form = r####"(progn
  (defvar neovm--test-mv-stats nil)

  (fset 'neovm--test-mv-init-stats
    (lambda ()
      (setq neovm--test-mv-stats
            (list :headers nil :bold-count 0 :italic-count 0
                  :code-snippets nil :text-length 0
                  :block-count 0 :list-items 0))))

  (fset 'neovm--test-mv-visit-inline
    (lambda (nodes)
      "Visit inline nodes and update stats."
      (dolist (node nodes)
        (let ((type (car node)))
          (cond
            ((eq type 'text)
             (plist-put neovm--test-mv-stats :text-length
                        (+ (plist-get neovm--test-mv-stats :text-length)
                           (length (cadr node)))))
            ((eq type 'bold)
             (plist-put neovm--test-mv-stats :bold-count
                        (1+ (plist-get neovm--test-mv-stats :bold-count)))
             (plist-put neovm--test-mv-stats :text-length
                        (+ (plist-get neovm--test-mv-stats :text-length)
                           (length (cadr node)))))
            ((eq type 'italic)
             (plist-put neovm--test-mv-stats :italic-count
                        (1+ (plist-get neovm--test-mv-stats :italic-count)))
             (plist-put neovm--test-mv-stats :text-length
                        (+ (plist-get neovm--test-mv-stats :text-length)
                           (length (cadr node)))))
            ((eq type 'code)
             (plist-put neovm--test-mv-stats :code-snippets
                        (cons (cadr node)
                              (plist-get neovm--test-mv-stats :code-snippets))))
            )))))

  (fset 'neovm--test-mv-visit-document
    (lambda (ast)
      "Visit all blocks in AST, updating stats."
      (dolist (block ast)
        (plist-put neovm--test-mv-stats :block-count
                   (1+ (plist-get neovm--test-mv-stats :block-count)))
        (let ((type (car block)))
          (cond
            ((eq type 'header)
             (plist-put neovm--test-mv-stats :headers
                        (cons (list (cadr block)
                                    (mapconcat
                                      (lambda (n) (if (stringp (cadr n)) (cadr n) ""))
                                      (nth 2 block) ""))
                              (plist-get neovm--test-mv-stats :headers)))
             (funcall 'neovm--test-mv-visit-inline (nth 2 block)))
            ((eq type 'paragraph)
             (funcall 'neovm--test-mv-visit-inline (cadr block)))
            ((eq type 'list-item)
             (plist-put neovm--test-mv-stats :list-items
                        (1+ (plist-get neovm--test-mv-stats :list-items)))
             (funcall 'neovm--test-mv-visit-inline (cadr block))))))))

  (unwind-protect
      (progn
        ;; Test with a rich AST
        (funcall 'neovm--test-mv-init-stats)
        (let ((doc (list
                     (list 'header 1 (list (list 'text "Introduction")))
                     (list 'paragraph (list (list 'text "Hello ")
                                             (list 'bold "world")
                                             (list 'text ". Use ")
                                             (list 'code "emacs")
                                             (list 'text ".")))
                     (list 'header 2 (list (list 'text "Details")))
                     (list 'paragraph (list (list 'italic "Important")
                                             (list 'text " note about ")
                                             (list 'code "lisp")
                                             (list 'text " and ")
                                             (list 'bold "macros")))
                     (list 'list-item (list (list 'text "first item")))
                     (list 'list-item (list (list 'code "second")
                                             (list 'text " item")))
                     (list 'list-item (list (list 'text "third ") (list 'bold "bold") (list 'text " item"))))))
          (funcall 'neovm--test-mv-visit-document doc)
          (let ((s neovm--test-mv-stats))
            (list
              (plist-get s :block-count)      ;; 7
              (plist-get s :bold-count)        ;; 3 (world, macros, bold)
              (plist-get s :italic-count)      ;; 1 (Important)
              (nreverse (plist-get s :code-snippets))  ;; (emacs lisp second)
              (plist-get s :list-items)        ;; 3
              (nreverse (plist-get s :headers)) ;; ((1 "Introduction") (2 "Details"))
              (> (plist-get s :text-length) 0)))))
    (fmakunbound 'neovm--test-mv-init-stats)
    (fmakunbound 'neovm--test-mv-visit-inline)
    (fmakunbound 'neovm--test-mv-visit-document)
    (makunbound 'neovm--test-mv-stats)))"####;
    let expect = expect_test::expect![[
        r#""OK (7 3 1 (\"emacs\" \"lisp\" \"second\") 3 ((1 \"Introduction\") (2 \"Details\")) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AST transformer: convert headers to bold paragraphs, lists to numbered
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_ast_transformer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transform an AST: (1) convert all headers to bold paragraphs,
    // (2) convert unordered list items to numbered items,
    // (3) wrap all code nodes in brackets.
    let form = r####"(progn
  (fset 'neovm--test-mt-transform-inline
    (lambda (nodes)
      "Transform inline nodes: wrap code in brackets."
      (mapcar (lambda (node)
                (if (eq (car node) 'code)
                    (list 'code (concat "[" (cadr node) "]"))
                  node))
              nodes)))

  (fset 'neovm--test-mt-transform-document
    (lambda (ast)
      "Transform AST: headers->bold paragraphs, lists->numbered."
      (let ((result nil)
            (list-counter 0))
        (dolist (block ast)
          (let ((type (car block)))
            (cond
              ;; Header -> paragraph with bold text
              ((eq type 'header)
               (let ((inline-nodes (funcall 'neovm--test-mt-transform-inline
                                            (nth 2 block))))
                 (setq result
                       (cons (list 'paragraph
                                   (list (list 'bold
                                               (mapconcat
                                                 (lambda (n) (if (stringp (cadr n)) (cadr n) ""))
                                                 inline-nodes ""))))
                             result))))
              ;; List item -> numbered paragraph
              ((eq type 'list-item)
               (setq list-counter (1+ list-counter))
               (let ((inline-nodes (funcall 'neovm--test-mt-transform-inline
                                            (cadr block))))
                 (setq result
                       (cons (list 'paragraph
                                   (cons (list 'text (format "%d. " list-counter))
                                         inline-nodes))
                             result))))
              ;; Paragraph: just transform inline
              ((eq type 'paragraph)
               (setq result
                     (cons (list 'paragraph
                                 (funcall 'neovm--test-mt-transform-inline
                                          (cadr block)))
                           result)))
              ;; Unknown: pass through
              (t (setq result (cons block result))))))
        (nreverse result))))

  (unwind-protect
      (let ((doc (list
                   (list 'header 1 (list (list 'text "Title")))
                   (list 'paragraph (list (list 'text "Hello ")
                                           (list 'code "world")))
                   (list 'header 2 (list (list 'text "Items")))
                   (list 'list-item (list (list 'text "apple")))
                   (list 'list-item (list (list 'code "banana")
                                           (list 'text " fruit")))
                   (list 'list-item (list (list 'text "cherry"))))))
        (let ((transformed (funcall 'neovm--test-mt-transform-document doc)))
          (list
            ;; Should have 6 blocks (all paragraphs now)
            (length transformed)
            ;; All should be paragraphs
            (mapcar 'car transformed)
            ;; Full transformed AST
            transformed)))
    (fmakunbound 'neovm--test-mt-transform-inline)
    (fmakunbound 'neovm--test-mt-transform-document)))"####;
    let expect = expect_test::expect![[
        r#""OK (6 (paragraph paragraph paragraph paragraph paragraph paragraph) ((paragraph ((bold \"Title\"))) (paragraph ((text \"Hello \") (code \"[world]\"))) (paragraph ((bold \"Items\"))) (paragraph ((text \"1. \") (text \"apple\"))) (paragraph ((text \"2. \") (code \"[banana]\") (text \" fruit\"))) (paragraph ((text \"3. \") (text \"cherry\")))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// End-to-end: parse -> transform -> render pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markup_end_to_end_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full pipeline: raw markup -> parse to AST -> transform AST ->
    // render to plain text. Verify each stage produces expected results.
    let form = r####"(progn
  ;; Minimal inline parser
  (fset 'neovm--test-me-parse-inline
    (lambda (text)
      (let ((nodes nil) (current "") (i 0) (len (length text)))
        (while (< i len)
          (let ((ch (aref text i)))
            (cond
              ((and (= ch ?*) (< (1+ i) len) (= (aref text (1+ i)) ?*))
               (when (> (length current) 0)
                 (setq nodes (cons (list 'text current) nodes) current ""))
               (let ((end (let ((j (+ i 2)) (f nil))
                            (while (and (< (1+ j) len) (not f))
                              (when (and (= (aref text j) ?*) (= (aref text (1+ j)) ?*))
                                (setq f j))
                              (setq j (1+ j))) f)))
                 (if end
                     (setq nodes (cons (list 'bold (substring text (+ i 2) end)) nodes)
                           i (+ end 2))
                   (setq current (concat current "**") i (+ i 2)))))
              (t (setq current (concat current (char-to-string ch))) (setq i (1+ i))))))
        (when (> (length current) 0) (setq nodes (cons (list 'text current) nodes)))
        (nreverse nodes))))

  ;; Minimal block parser
  (fset 'neovm--test-me-parse
    (lambda (text)
      (let ((lines (split-string text "\n"))
            (blocks nil) (para nil))
        (dolist (line lines)
          (cond
            ((string-match "^\\(#+\\) \\(.*\\)" line)
             (when para
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-me-parse-inline
                                                 (mapconcat 'identity (nreverse para) " ")))
                                  blocks) para nil))
             (setq blocks (cons (list 'header (length (match-string 1 line))
                                      (funcall 'neovm--test-me-parse-inline
                                               (match-string 2 line))) blocks)))
            ((string-match "^- \\(.*\\)" line)
             (when para
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-me-parse-inline
                                                 (mapconcat 'identity (nreverse para) " ")))
                                  blocks) para nil))
             (setq blocks (cons (list 'list-item
                                      (funcall 'neovm--test-me-parse-inline
                                               (match-string 1 line))) blocks)))
            ((string= line "")
             (when para
               (setq blocks (cons (list 'paragraph
                                        (funcall 'neovm--test-me-parse-inline
                                                 (mapconcat 'identity (nreverse para) " ")))
                                  blocks) para nil)))
            (t (setq para (cons line para)))))
        (when para
          (setq blocks (cons (list 'paragraph
                                    (funcall 'neovm--test-me-parse-inline
                                             (mapconcat 'identity (nreverse para) " ")))
                              blocks)))
        (nreverse blocks))))

  ;; Transform: upcase all text nodes
  (fset 'neovm--test-me-upcase-transform
    (lambda (ast)
      (mapcar
        (lambda (block)
          (let ((type (car block)))
            (cond
              ((eq type 'header)
               (list 'header (cadr block)
                     (mapcar (lambda (n)
                               (if (stringp (cadr n))
                                   (list (car n) (upcase (cadr n)))
                                 n))
                             (nth 2 block))))
              ((or (eq type 'paragraph) (eq type 'list-item))
               (list type
                     (mapcar (lambda (n)
                               (if (stringp (cadr n))
                                   (list (car n) (upcase (cadr n)))
                                 n))
                             (cadr block))))
              (t block))))
        ast)))

  ;; Render to plain text
  (fset 'neovm--test-me-render
    (lambda (ast)
      (mapconcat
        (lambda (block)
          (let ((type (car block)))
            (cond
              ((eq type 'header)
               (concat (make-string (cadr block) ?#) " "
                       (mapconcat (lambda (n) (cadr n)) (nth 2 block) "")))
              ((eq type 'paragraph)
               (mapconcat (lambda (n) (cadr n)) (cadr block) ""))
              ((eq type 'list-item)
               (concat "- " (mapconcat (lambda (n) (cadr n)) (cadr block) "")))
              (t ""))))
        ast "\n")))

  (unwind-protect
      (let* ((input "# Welcome\n\nHello **world** today.\n\n## Items\n\n- first\n- **second**")
             (ast (funcall 'neovm--test-me-parse input))
             (transformed (funcall 'neovm--test-me-upcase-transform ast))
             (rendered-original (funcall 'neovm--test-me-render ast))
             (rendered-transformed (funcall 'neovm--test-me-render transformed)))
        (list
          ;; AST structure
          (length ast)
          (mapcar 'car ast)
          ;; Rendered original
          rendered-original
          ;; Rendered transformed (all uppercased)
          rendered-transformed
          ;; Verify transform actually changed text
          (not (string= rendered-original rendered-transformed))
          ;; Verify both have same number of blocks
          (= (length ast) (length transformed))))
    (fmakunbound 'neovm--test-me-parse-inline)
    (fmakunbound 'neovm--test-me-parse)
    (fmakunbound 'neovm--test-me-upcase-transform)
    (fmakunbound 'neovm--test-me-render)))"####;
    let expect = expect_test::expect![[
        r##""OK (5 (header paragraph header list-item list-item) \"# Welcome\\nHello world today.\\n## Items\\n- first\\n- second\" \"# WELCOME\\nHELLO WORLD TODAY.\\n## ITEMS\\n- FIRST\\n- SECOND\" t t)""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
