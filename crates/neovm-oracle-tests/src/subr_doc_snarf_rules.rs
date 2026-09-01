//! Oracle guards for `Fsnarf_documentation`'s **function** arm.
//!
//! Ledger 168 and 173 did this work for variables.  The function arm is three
//! lines further down the same loop and has the same two clauses:
//!
//! ```c
//! /* Attach a docstring to a function?  */
//! else if (p[1] == 'F')
//!   {
//!     if (!NILP (Ffboundp (sym)) && strncmp (end, "\nSKIP", 5))
//!       store_function_docstring (sym, pos + end + 1 - buf);
//!   }
//! ```
//!
//! (`src/doc.c:617-621`.)  `neovm-core`'s `subr_docs::gnu_table` stands in for
//! `etc/DOC` and, until ledger 181, disagreed with it on 86 of 1733 rows --
//! not because the clauses were unported but because the *scanner* that built
//! the table was three regular expressions where GNU has one state machine.
//! The generator is now a port of `lib-src/make-docfile.c` whose DOC stream is
//! verified byte-identical against GNU's compiled binary; these pins are the
//! runtime half, one per mechanism, so a regression names its own cause.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// No built-in function's documentation is GNU's `SKIP` placeholder.
///
/// A function several window systems define carries the real text in exactly
/// one file and `doc: /* SKIP: real doc in xfns.c.  */` in the rest, so the
/// string is maintained once.  106 of GNU's `src/*.c` `DEFUN` blocks carry the
/// marker and 37 of them reached the generated table, because the DEFUN
/// extractor -- unlike the DEFVAR one after entry 168 -- had no `SKIP` filter
/// at all.  `(documentation 'x-display-list)` answered
/// "SKIP: real doc in xfns.c.".
///
/// Asked by prefix rather than by equality, so a future generator that invents
/// a *different* placeholder is caught by the same pin; GNU's own test is
/// `strncmp (end, "\nSKIP", 5)`, a prefix test, for the same reason.
#[test]
fn oracle_no_builtin_function_documentation_is_gnus_skip_placeholder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (bad)
  (dolist (s '(file-system-info font-get-system-font font-get-system-normal-font
               menu-or-popup-active-p x-change-window-property x-close-connection
               x-create-frame x-delete-window-property x-display-backing-store
               x-display-color-cells x-display-grayscale-p x-display-list
               x-display-mm-height x-display-mm-width x-display-pixel-height
               x-display-pixel-width x-display-planes x-display-save-under
               x-display-screens x-display-visual-class x-double-buffered-p
               x-export-frames x-hide-tip x-menu-bar-open-internal
               x-open-connection x-server-max-request-size x-server-vendor
               x-server-version x-show-tip x-synchronize x-window-property
               xw-color-defined-p xw-color-values xw-display-color-p))
    (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
      (when (and (stringp doc) (string-prefix-p "SKIP" doc))
        (push s bad))))
  (nreverse bad))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// And the text that appears instead is the canonical file's.
///
/// The pin that keeps the one above honest: a list of names that are all
/// unbound would satisfy "none of them says SKIP" too, which is exactly the
/// shape ledger 173 warned about.  This asks for the text.
#[test]
fn oracle_platform_duplicated_functions_carry_the_canonical_doc_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
            (cons s (and (stringp doc) (car (split-string doc "\n"))))))
        '(file-system-info font-get-system-font font-get-system-normal-font
          menu-or-popup-active-p x-display-list x-display-mm-width
          x-display-pixel-width x-display-planes x-display-screens
          x-double-buffered-p x-export-frames x-hide-tip x-open-connection
          x-server-vendor x-server-version x-show-tip x-synchronize
          x-window-property xw-color-defined-p xw-color-values
          xw-display-color-p))"#;
    let expect = expect_test::expect![[
        r#""OK ((file-system-info . \"Return storage information about the file system FILENAME is on.\") (font-get-system-font . \"Get the system default fixed width font.\") (font-get-system-normal-font . \"Get the system default application font.\") (menu-or-popup-active-p . \"Return t if a menu or popup dialog is active.\") (x-display-list . \"Return the list of display names that Emacs has connections to.\") (x-display-mm-width . \"Return the width in millimeters of the X display TERMINAL.\") (x-display-pixel-width . \"Return the width in pixels of the X display TERMINAL.\") (x-display-planes . \"Return the number of bitplanes of the X display TERMINAL.\") (x-display-screens . \"Return the number of screens on the X server of display TERMINAL.\") (x-double-buffered-p . \"Return t if FRAME is being double buffered.\") (x-export-frames . \"Return image data of FRAMES in TYPE format.\") (x-hide-tip . \"Hide the current tooltip window, if there is any.\") (x-open-connection . \"Open a connection to a display server.\") (x-server-vendor . \"Return the \\\"vendor ID\\\" string of the GUI software on TERMINAL.\") (x-server-version . \"Return the version numbers of the GUI software on TERMINAL.\") (x-show-tip . \"Show STRING in a \\\"tooltip\\\" window on frame FRAME.\") (x-synchronize . \"If ON is non-nil, report X errors as soon as the erring request is made.\") (x-window-property . \"Value is the value of window property PROP on FRAME.\") (xw-color-defined-p . \"Internal function called by `color-defined-p'.\") (xw-color-values . \"Internal function called by `color-values'.\") (xw-display-color-p . \"Internal function called by `display-color-p'.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The heads GNU's `make-docfile` reads and a regular expression does not.
///
/// `scan_c_stream` reaches the `doc` keyword by counting five commas and then
/// reading forward; it has no opinion about line breaks and no opinion about
/// whether MIN and MAX are literals.  The head regex had both: it anchored on
/// `\s*$` after the interactive spec, which `data.c:1067`, `callint.c:239`,
/// `dispnew.c:3442`, `xfaces.c:7335` and 26 others do not provide, and it
/// spelled MIN `[A-Z0-9_]+`, which `charset.c:845` (`charset_arg_max`) and
/// `coding.c:10988` (`coding_arg_max`) do not match.  Thirty names had no row
/// at all and `(documentation 'funcall-interactively)` answered nil.
#[test]
fn oracle_defun_heads_a_regex_cannot_parse_still_have_gnus_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
            (cons s (and (stringp doc) (car (split-string doc "\n"))))))
        '(char-charset define-charset-internal define-coding-system-internal
          font-get-glyphs font-match-p frame--z-order-lessp
          funcall-interactively internal-set-lisp-face-attribute-from-resource
          native-comp-function-p set-window-next-buffers set-window-parameter
          set-window-prev-buffers split-char treesit-parser-p
          tty--output-buffer-size tty--set-output-buffer-size
          x-translate-coordinates))"#;
    let expect = expect_test::expect![[
        r#""OK ((char-charset . \"Return the charset of highest priority that contains CH.\") (define-charset-internal . \"For internal use only.\") (define-coding-system-internal . \"For internal use only.\") (font-get-glyphs . \"Return a vector of FONT-OBJECT's glyphs for the specified characters.\") (font-match-p . \"Return t if and only if font-spec SPEC matches with FONT.\") (frame--z-order-lessp . \"Internal frame sorting function A < B.\") (funcall-interactively . \"Like `funcall' but marks the call as interactive.\") (internal-set-lisp-face-attribute-from-resource . \"\") (native-comp-function-p . \"Return t if the object is native-compiled Lisp function, nil otherwise.\") (set-window-next-buffers . \"Set WINDOW's next buffers to NEXT-BUFFERS.\") (set-window-parameter . \"Set WINDOW's value of PARAMETER to VALUE.\") (set-window-prev-buffers . \"Set WINDOW's previous buffers to PREV-BUFFERS.\") (split-char . \"Return list of charset and one to four position-codes of CH.\") (treesit-parser-p . \"Return t if OBJECT is a tree-sitter parser.\") (tty--output-buffer-size . \"Return the output buffer size of TTY.\") (tty--set-output-buffer-size . \"Set the output buffer size for a TTY.\") (x-translate-coordinates . \"Translate coordinates from FRAME.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The seven `doc:` markers whose spelling sent the old literal search into a
/// LATER function's comment.
///
/// `"doc: /*"` as a literal misses `doc:  /*` (two spaces, `window.c:2324`,
/// `2351`, `2390`, `xwidget.c:3374`), `doc:` with the `/*` on the next line
/// (`charset.c:1885`, `font.c:5336`) and `doc :` (`treesit.c:1203`).  The
/// search was also unbounded, so each miss took the *next* function's doc
/// string -- `window-parameter` answered `set-window-parameter`'s text -- and
/// the scan then resumed past it, dropping twelve more heads on the way.
///
/// Each pair below is a thief and its victim, which is what makes this a pin
/// on the mechanism rather than on twelve strings.
#[test]
fn oracle_odd_spaced_doc_markers_do_not_take_the_next_functions_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (s)
          (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
            (cons s (and (stringp doc) (car (split-string doc "\n"))))))
        '(window-prev-buffers set-window-prev-buffers
          window-next-buffers set-window-next-buffers
          window-parameter set-window-parameter
          make-char split-char char-charset
          font-has-char-p font-get-glyphs font-match-p))"#;
    let expect = expect_test::expect![[
        r#""OK ((window-prev-buffers . \"Return buffers previously shown in WINDOW.\") (set-window-prev-buffers . \"Set WINDOW's previous buffers to PREV-BUFFERS.\") (window-next-buffers . \"Return list of buffers recently re-shown in WINDOW.\") (set-window-next-buffers . \"Set WINDOW's next buffers to NEXT-BUFFERS.\") (window-parameter . \"Return WINDOW's value for PARAMETER.\") (set-window-parameter . \"Set WINDOW's value of PARAMETER to VALUE.\") (make-char . \"Return a character of CHARSET whose position codes are CODEn.\") (split-char . \"Return list of charset and one to four position-codes of CH.\") (char-charset . \"Return the charset of highest priority that contains CH.\") (font-has-char-p . \"Return non-nil if FONT on FRAME has a glyph for character CH.\") (font-get-glyphs . \"Return a vector of FONT-OBJECT's glyphs for the specified characters.\") (font-match-p . \"Return t if and only if font-spec SPEC matches with FONT.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// No function's documentation opens with whitespace, and GNU's own two
/// unreadable heads answer nil.
///
/// Two rules of `make-docfile` in one pin, because both are about what a DOC
/// record may look like at its edges:
///
/// * `read_c_string_or_comment` discards **all** leading whitespace inside a
///   `doc:` comment (`lib-src/make-docfile.c:416-419`), so `doc: /*` followed
///   by a newline (`charset.c:2109`) or by two spaces (`font.c:5493`) stores
///   text that starts at the first real character.  Ten rows opened with a
///   newline or a space, so `C-h f string` began with a blank line.
/// * `scan_c_stream` reads the doc keyword with
///   `while (c_isalpha (c)) c = getc (infile); if (c == ':')`, so `doc :` with
///   a space before the colon never sets `doc_keyword` and the comment two
///   characters later is never read.  `src/treesit.c:1203` and `1221` spell it
///   that way, and **GNU Emacs itself answers nil for both names** -- this is
///   a GNU typo the port reproduces rather than a divergence to fix.
#[test]
fn oracle_function_docs_never_open_with_whitespace_and_gnus_two_typos_answer_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; No leading whitespace on any of the ten rows that had it.
 (let (bad)
   (dolist (s '(char-table-subtype charset-after clear-charset-maps draw-string
                font-xlfd-name forward-comment get-unused-iso-final-char
                iso-charset set-fontset-font string))
     (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
       (when (and (stringp doc) (string-match-p "\\`[ \t\n]" doc))
         (push s bad))))
   (nreverse bad))
 ;; ... and the text really is there, so an all-unbound list cannot pass.
 (mapcar (lambda (s)
           (let ((doc (and (fboundp s) (ignore-errors (documentation s t)))))
             (cons s (and (stringp doc) (substring doc 0 (min 28 (length doc)))))))
         '(char-table-subtype forward-comment string))
 ;; GNU's own `doc :' typo: nil in both editors.
 (mapcar (lambda (s)
           (list s (and (fboundp s) t)
                 (and (fboundp s) (ignore-errors (documentation s t)))))
         '(treesit-tracking-line-column-p
           treesit-parser-tracking-line-column-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil ((char-table-subtype . \"Return the subtype of char-t\") (forward-comment . \"Move forward across up to CO\") (string . \"Concatenate all the argument\")) ((treesit-tracking-line-column-p t nil) (treesit-parser-tracking-line-column-p t nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
