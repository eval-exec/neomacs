//! GNU 31.1 printer settings fixtures, refreshed only from GNU with UPDATE_EXPECT=1.
use std::path::PathBuf;
fn oracle(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input = root
            .join("../../tmp")
            .join(format!("settings-{name}-{}.el", std::process::id()));
        std::fs::write(&input, format!("(prin1 {form})\n")).expect("GNU probe");
        let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".local/bin/emacs")
                .into_os_string()
        });
        let output = std::process::Command::new(emacs)
            .args(["-Q", "--batch", "-l"])
            .arg(input)
            .output()
            .expect("GNU oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).expect("UTF-8 oracle");
        std::fs::write(
            root.join("src/emacs_core/lisp/print/tests/settings_gnu")
                .join(format!("{name}.expect")),
            &expected,
        )
        .expect("GNU fixture");
        expected
    } else {
        fixture.to_owned()
    };
    let mut context = crate::test_utils::runtime_startup_context();
    let actual = crate::emacs_core::format_eval_result(&context.eval_str(form));
    assert_eq!(actual, format!("OK {}", expected.trim_end()), "{name}");
}

#[test]
fn printer_settings_gnu_quoted() {
    oracle(
        "quoted",
        r###"(list (let ((print-quoted nil)) (prin1-to-string '('a #'b `(c ,d)))) (prin1-to-string '(quote x) nil '((quoted . nil))))"###,
        include_str!("settings_gnu/quoted.expect"),
    );
}

#[test]
fn printer_settings_gnu_nbsp() {
    oracle(
        "nbsp",
        r###"(list (append (prin1-to-string (intern "a b")) nil) (eq (read (prin1-to-string (intern "a b"))) (intern "a b")))"###,
        include_str!("settings_gnu/nbsp.expect"),
    );
}

#[test]
fn printer_settings_gnu_hash_princ() {
    oracle(
        "hash_princ",
        r###"(let ((h (make-hash-table :test 'equal))) (puthash "k" "x" h) (list (format "%s" h) (format "%s" (list h)) (with-output-to-string (princ h)) (prin1-to-string h t)))"###,
        include_str!("settings_gnu/hash_princ.expect"),
    );
}

#[test]
fn printer_settings_gnu_unreadable() {
    oracle(
        "unreadable",
        r###"(list (let ((print-unreadable-function (lambda (_o _e) "Q"))) (prin1-to-string (list 1 (make-marker) (current-buffer) (obarray-make)))) (let ((print-unreadable-function (lambda (_o _e) t))) (prin1-to-string (make-marker))) (let ((print-unreadable-function (lambda (_o e) (if e "escaped" "bare")))) (list (prin1-to-string (make-marker)) (prin1-to-string (make-marker) t))) (prin1-to-string (make-marker) nil (list (cons 'unreadable-function (lambda (_o _e) "override")))))"###,
        include_str!("settings_gnu/unreadable.expect"),
    );
}

#[test]
fn printer_settings_gnu_charset() {
    oracle(
        "charset",
        r###"(progn (set-charset-priority 'unicode) (list print-charset-text-property (prin1-to-string (propertize "a" 'charset 'ascii)) (prin1-to-string (propertize "a" 'charset 'ascii 'foo 'bar)) (let ((print-charset-text-property nil)) (prin1-to-string (propertize "é" 'charset 'ascii))) (let ((print-charset-text-property t)) (prin1-to-string (propertize "a" 'charset 'ascii))) (let ((print-charset-text-property 'default)) (prin1-to-string (propertize "é" 'charset 'ascii))) (let ((print-charset-text-property 'default)) (prin1-to-string (propertize "é" 'charset 'unicode)))))"###,
        include_str!("settings_gnu/charset.expect"),
    );
}

#[test]
fn printer_settings_gnu_overrides() {
    oracle(
        "overrides",
        r###"(list (prin1-to-string ?a nil '((integers-as-characters . t))) (condition-case e (prin1-to-string 1 nil '((bogus . 1))) (error e)) (condition-case e (prin1-to-string 1 nil '((length . 1) . 3)) (error e)) (condition-case e (prin1-to-string 1 nil '((3 . 1))) (error e)) (condition-case e (prin1-to-string 1 nil (list (cons (make-symbol "length") 1))) (error e)))"###,
        include_str!("settings_gnu/overrides.expect"),
    );
}

#[test]
fn printer_settings_gnu_unreadable_scopes() {
    oracle(
        "unreadable_scopes",
        r###"(let* ((v (vector (make-marker) (make-marker))) (print-unreadable-function (lambda (o _e) (aset v 1 'changed) (concat (if print-unreadable-function "wrong" "nil") (let (chars) (prin1 o (lambda (c) (push c chars))) (apply #'string (nreverse chars))))))) (list (prin1-to-string v) (condition-case e (let ((print-unreadable-function (lambda (_o _e) (error "callback")))) (prin1-to-string (make-marker))) (error e)) (functionp print-unreadable-function)))"###,
        include_str!("settings_gnu/unreadable_scopes.expect"),
    );
}

#[test]
fn printer_settings_gnu_unreadable_fallback() {
    oracle(
        "unreadable_fallback",
        r###"(list (let ((print-unreadable-function (lambda (_o _e) nil))) (prin1-to-string (make-marker))) (let ((print-unreadable-function 'unbound-printer)) (prin1-to-string (make-marker))) (prin1-to-string (make-marker) nil (list (cons 'unreadeable-function (lambda (_o _e) "alias")))) (let ((print-unreadable-function (lambda (_o e) (if e "yes" "no")))) (list (format "%S" (list (make-marker))) (format "%s" (list (make-marker))) (with-output-to-string (princ (make-marker))))))"###,
        include_str!("settings_gnu/unreadable_fallback.expect"),
    );
}

#[test]
fn printer_settings_gnu_override_scopes() {
    oracle(
        "override_scopes",
        r###"(let ((print-length 99) (print-quoted nil) (print-escape-nonascii nil)) (list (prin1-to-string (make-marker) nil (list (cons 'length 7) (cons 'quoted t) (cons 'unreadable-function (lambda (_o _e) (concat (number-to-string print-length) (if print-quoted "q" "-") (if print-escape-nonascii "e" "-") (if print-unreadable-function "u" "-")))))) (list print-length print-quoted print-escape-nonascii) (condition-case e (prin1-to-string (make-marker) nil (list (cons 'length 7) (cons 'unreadable-function (lambda (_o _e) (error "scope"))))) (error e)) print-length (prin1-to-string (make-marker) nil (list t (cons 'unreadable-function (lambda (_o _e) (concat (if print-length "l" "-") (if print-quoted "q" "-") (if print-charset-text-property "c" "-"))))))))"###,
        include_str!("settings_gnu/override_scopes.expect"),
    );
}

#[test]
fn printer_settings_gnu_callback_buffer() {
    oracle(
        "callback_buffer",
        r###"(with-temp-buffer (let ((origin (current-buffer)) (print-unreadable-function (lambda (_o _e) (set-buffer (get-buffer-create "printer-callback-target")) "Q"))) (list (prin1-to-string (vector (make-marker) (make-marker))) (eq origin (current-buffer)) (condition-case e (let ((print-unreadable-function (lambda (_o _e) (set-buffer (get-buffer-create "printer-callback-target")) (error "changed")))) (prin1-to-string (make-marker))) (error e)) (eq origin (current-buffer)))))"###,
        include_str!("settings_gnu/callback_buffer.expect"),
    );
}

#[test]
fn printer_settings_gnu_buffer_princ() {
    oracle(
        "buffer_princ",
        r###"(with-temp-buffer (rename-buffer "printer buffer") (let ((print-unreadable-function (lambda (_o _e) nil))) (list (prin1-to-string (current-buffer) t) (format "%s" (list (current-buffer))) (format "%S" (list (current-buffer))))))"###,
        include_str!("settings_gnu/buffer_princ.expect"),
    );
}

#[test]
fn printer_settings_gnu_callback_gc_plists() {
    oracle(
        "callback_gc_plists",
        r###"(let* ((tail (list (make-marker) 'after)) (object (propertize "ab" 'charset 'ascii 'payload tail)) (print-unreadable-function (lambda (_o _e) (set-text-properties 0 2 nil object) (garbage-collect) "Q"))) (list (prin1-to-string object) (text-properties-at 0 object)))"###,
        include_str!("settings_gnu/callback_gc_plists.expect"),
    );
}

#[test]
fn printer_settings_gnu_override_watchers() {
    oracle(
        "override_watchers",
        r###"(let* ((print-length 99)
        (print-quoted nil)
        (events nil)
        (overrides (list (cons 'length 3) (cons 'quoted t)
                         (cons 'unreadable-function (lambda (_object _escaping) "kept"))))
        (watcher (lambda (_symbol value operation _where)
                   (when (memq operation '(let unlet)) (push (list operation value) events))
                   (when (eq operation 'let)
                     (setcdr overrides nil)
                     (garbage-collect)))))
   (unwind-protect
       (progn
         (add-variable-watcher 'print-quoted watcher)
         (let ((text (prin1-to-string (make-marker) nil overrides)))
           (list text print-length print-quoted (nreverse events))))
     (remove-variable-watcher 'print-quoted watcher)))"###,
        include_str!("settings_gnu/override_watchers.expect"),
    );
}

#[test]
fn printer_settings_gnu_live_options() {
    oracle(
        "live_options",
        r###"(list
  (let ((print-quoted t)
        (print-unreadable-function (lambda (_o _e) (setq print-quoted nil) "Q")))
    (prin1-to-string (vector (make-marker) '(quote a))))
  (let ((print-length nil)
        (print-unreadable-function (lambda (_o _e) (setq print-length 1) "Q")))
    (prin1-to-string (vector (make-marker) '(1 2) '(3 4))))
  (let ((print-length nil)
        (print-unreadable-function (lambda (_o _e) (setq print-length 1) "Q")))
    (prin1-to-string (list (make-marker) '(1 2) '(3 4))))
  (let ((print-level nil)
        (print-unreadable-function (lambda (_o _e) (setq print-level 1) "Q")))
    (prin1-to-string (vector (make-marker) '(1 2) (vector 3 4))))
  (let ((print-circle nil)
        (print-unreadable-function (lambda (_o _e) (setq print-circle t) "Q")))
    (condition-case e (prin1-to-string (vector (make-marker) '(a))) (error e)))
  (let ((print-circle t)
        (print-unreadable-function (lambda (_o _e) (let ((print-circle nil)) (prin1 1 #'ignore)) "Q")))
    (condition-case e (let ((shared '(a))) (prin1-to-string (vector (make-marker) shared shared))) (error e)))
  (let ((print-length nil)
        (print-unreadable-function (lambda (_o _e) (setq print-length 1) "Q"))
        (h (make-hash-table :test 'eq)))
    (puthash (make-marker) '(1 2) h)
    (puthash 'next '(3 4) h)
    (prin1-to-string h)))"###,
        include_str!("settings_gnu/live_options.expect"),
    );
}

#[test]
fn printer_settings_gnu_callback_string_flags() {
    oracle(
        "callback_string_flags",
        r###"(list
  (let ((print-unreadable-function (lambda (_o _e) (setq print-escape-nonascii nil) (unibyte-string 200))))
    (append (prin1-to-string (make-marker)) nil))
  (let ((print-unreadable-function (lambda (_o _e) (setq print-escape-nonascii t) (unibyte-string 200))))
    (append (prin1-to-string (make-marker)) nil))
  (let ((print-circle nil)
        (print-unreadable-function (lambda (_o _e) (setq print-circle t) "Q")))
    (condition-case e (prin1-to-string (vector (make-marker) 3)) (error e)))
  (let ((print-escape-newlines nil)
        (print-unreadable-function (lambda (_o _e) (setq print-escape-newlines t) "Q")))
    (prin1-to-string (vector (make-marker) "a\nb"))))"###,
        include_str!("settings_gnu/callback_string_flags.expect"),
    );
}

#[test]
fn printer_settings_gnu_shorthand_refresh() {
    oracle(
        "shorthand_refresh",
        r###"(list
  (let ((print-quoted t)
        (print-unreadable-function (lambda (_o _e) (setq print-quoted nil) "Q")))
    (prin1-to-string (vector (list 'quote (make-marker)) '(quote a))))
  (let ((print-quoted t)
        (print-unreadable-function (lambda (_o _e) (setq print-quoted nil) "Q")))
    (prin1-to-string (vector (list '\` (make-marker)) '(quote a))))
  (let ((print-quoted t)
        (print-unreadable-function (lambda (_o _e) (setq print-quoted nil) "Q")))
    (prin1-to-string (vector (list 'quote (vector (make-marker) '(quote a))) '(quote b)))))"###,
        include_str!("settings_gnu/shorthand_refresh.expect"),
    );
}

#[test]
fn printer_settings_gnu_format_source_ownership() {
    oracle(
        "format_source_ownership",
        r###"(list
 (let* ((fmt (copy-sequence "%s:%s:%s")) (calls 0)
        (print-unreadable-function (lambda (_o _e) (setq calls (1+ calls)) (aset fmt (if (= calls 1) 4 7) ?d) "Q")))
   (list (format fmt (make-marker) (make-marker) "tail") fmt calls))
 (let* ((fmt (string-to-unibyte (copy-sequence "%s:%s")))
        (print-unreadable-function (lambda (_o _e) (aset fmt 2 ?X) (aset fmt 4 ?d) "Q")))
   (list (format fmt (make-marker) "tail") fmt))
 (let* ((fmt (copy-sequence "%sé%s"))
        (print-unreadable-function (lambda (_o _e) (aset fmt 4 ?d) (garbage-collect) "Q")))
   (list (format fmt (make-marker) "tail") fmt))
 (let* ((fmt (copy-sequence "%Sé%s"))
        (print-unreadable-function (lambda (_o _e) (aset fmt 4 ?d) (garbage-collect) "Q")))
   (list (format-message fmt (make-marker) "tail") fmt))
 (let* ((fmt (unibyte-string 37 115 128 37 115))
        (print-unreadable-function (lambda (_o _e) (aset fmt 2 255) (aset fmt 4 ?d) "Q")))
   (list (append (format fmt (make-marker) "tail") nil) (append fmt nil)))
 (let* ((fmt (propertize "%S|%s" 'tag 'before))
        (print-unreadable-function (lambda (_o _e) (aset fmt 4 ?d) (set-text-properties 0 5 '(tag after) fmt) "Q")))
   (let ((result (format fmt (make-marker) "tail")))
     (list (substring-no-properties result) (text-properties-at 0 result) (text-properties-at 3 result))))
 (let* ((fmt (copy-sequence "%S:%s"))
        (print-escape-nonascii nil)
        (print-unreadable-function nil)
        (watcher (lambda (_symbol _value operation _where)
                   (when (eq operation 'let)
                     (aset fmt 4 ?d)
                     (setq print-unreadable-function (lambda (_o _e) "W"))))))
   (unwind-protect
       (progn (add-variable-watcher 'print-escape-nonascii watcher)
              (list (format fmt (make-marker) "tail") fmt))
     (remove-variable-watcher 'print-escape-nonascii watcher))))"###,
        include_str!("settings_gnu/format_source_ownership.expect"),
    );
}

#[test]
fn printer_settings_gnu_charset_merge_identity() {
    oracle(
        "charset_merge_identity",
        r###"(let* ((print-charset-text-property nil)
                 (left (copy-sequence "x"))
                 (right (copy-sequence "x"))
                 (distinct (copy-sequence "ab"))
                 (reordered (copy-sequence "ab"))
                 (same_order (copy-sequence "ab")))
            (set-text-properties 0 1 (list 'charset 'ascii 'foo left) distinct)
            (set-text-properties 1 2 (list 'charset 'unicode 'foo right) distinct)
            (set-text-properties 0 1 '(foo bar baz quux charset ascii) reordered)
            (set-text-properties 1 2 '(baz quux charset unicode foo bar) reordered)
            (set-text-properties 0 1 '(charset ascii foo bar) same_order)
            (set-text-properties 1 2 '(charset unicode foo bar) same_order)
            (list (eq left right) (prin1-to-string distinct)
                  (prin1-to-string reordered) (prin1-to-string same_order)))"###,
        include_str!("settings_gnu/charset_merge_identity.expect"),
    );
}

#[test]
fn printer_settings_gnu_cons_tortoise_callback_gc() {
    oracle(
        "cons_tortoise_callback_gc",
        r###"(let* ((object (list (make-marker) (make-marker) (make-marker)
                              (make-marker) (make-marker) (make-marker)
                              (make-marker) 'end))
                 (calls 0)
                 (print-circle nil)
                 (print-unreadable-function
                  (lambda (_object _escaping)
                    (setq calls (1+ calls))
                    (when (= calls 5)
                      ;; The third cons holds the printer's Brent tortoise,
                      ;; but this surgery leaves only first/fifth reachable.
                      (let ((current (nthcdr 4 object)))
                        (setcdr object current)
                        (garbage-collect)
                        (dotimes (index 1024) (cons index nil))
                        (setcdr current
                                (list (make-marker) (make-marker) 'end))))
                    "Q")))
            (list (prin1-to-string object) calls (length object)))"###,
        include_str!("settings_gnu/cons_tortoise_callback_gc.expect"),
    );
}

#[test]
fn printer_settings_gnu_destination_local_callback() {
    oracle(
        "destination_local_callback",
        r###"(with-temp-buffer
  (let* ((origin (current-buffer))
         (destination (generate-new-buffer "printer-local-destination"))
         (origin-function (lambda (_o _e) "O"))
         (calls 0) (events nil) (nested nil)
         (destination-function
          (lambda (_o _e)
            (setq calls (1+ calls))
            (when (> calls 2) (error "recursive callback"))
            (push (list (eq (current-buffer) origin)
                        (eq print-unreadable-function origin-function)
                        (with-current-buffer destination
                          (null print-unreadable-function))) events)
            (with-current-buffer destination
              (prin1 (make-marker) (lambda (character) (push character nested))))
            (garbage-collect)
            "D")))
    (setq-local print-unreadable-function origin-function)
    (with-current-buffer destination
      (setq-local print-unreadable-function destination-function))
    (unwind-protect
        (let ((result (condition-case e
                          (progn (prin1 (make-marker) destination) 'ok)
                        (error e))))
          (list result calls (nreverse events)
                (with-current-buffer destination (buffer-string))
                (apply #'string (nreverse nested))
                (eq (current-buffer) origin)
                (eq print-unreadable-function origin-function)
                (with-current-buffer destination
                  (eq print-unreadable-function destination-function))))
      (kill-buffer destination))))"###,
        include_str!("settings_gnu/destination_local_callback.expect"),
    );
}

#[test]
fn printer_settings_gnu_default_local_callback() {
    oracle(
        "default_local_callback",
        r###"(with-temp-buffer
  (let* ((origin (current-buffer))
         (origin-function (lambda (_o _e) "O"))
         (old-default (default-value 'print-unreadable-function))
         (calls 0) (events nil) (bindings nil)
         (watcher (lambda (_symbol _value operation where)
                    (push (list operation (null where)) bindings)))
         (default-function
          (lambda (_o _e)
            (setq calls (1+ calls))
            (when (> calls 2) (error "recursive default callback"))
            (push (list (eq (current-buffer) origin)
                        (eq print-unreadable-function origin-function)
                        (null (default-value 'print-unreadable-function))) events)
            (garbage-collect)
            "D")))
    (setq-local print-unreadable-function origin-function)
    (unwind-protect
        (progn
          (set-default 'print-unreadable-function default-function)
          (add-variable-watcher 'print-unreadable-function watcher)
          (let ((text (condition-case e (prin1-to-string (make-marker)) (error e))))
            (list text calls (nreverse events)
                  (nreverse bindings)
                  (eq print-unreadable-function origin-function)
                  (eq (default-value 'print-unreadable-function) default-function))))
      (remove-variable-watcher 'print-unreadable-function watcher)
      (set-default 'print-unreadable-function old-default))))"###,
        include_str!("settings_gnu/default_local_callback.expect"),
    );
}

/// GNU editfns.c:3670-3685 calls Fprin1_to_string for escaped %S and object %s.
/// print.c:143-151 selects its scratch destination before the implicit binding
/// at :175-177; caller-local NIL therefore must not suppress raw-byte escaping.
/// Full witnesses cover default/local cells, all output domains, and cleanup.
const FORMAT_LOCALIZED_SCRATCH_ESCAPE: &str = r###"(progn
(require 'cl-lib)
(defvar gdk-format-policy-watch-log nil)
(defun gdk-format-policy-watcher (symbol newval operation _where)
  ;; GNU invokes watchers before its final store. Observe buffer and cells;
  ;; do not mutate the watched value, redirects, or the selected buffer.
  (push (list :operation operation :new-value newval
              :buffer (buffer-name)
              :default (default-value symbol)
              :visible (symbol-value symbol))
        gdk-format-policy-watch-log))
(defun gdk-format-policy-witness (text)
  ;; Complete character list + explicit encoding/byte length preserve byte8 vs
  ;; unibyte-high domains, with no string-as-unibyte conversion/normalization.
  (list :length (length text) :bytes (string-bytes text)
        :multibyte (multibyte-string-p text) :codes (string-to-list text)))
(defun gdk-format-policy-row (local operation)
  (with-temp-buffer
    (rename-buffer "gdk-format-policy-caller" t)
    (when local
      (make-local-variable 'print-escape-nonascii)
      (setq print-escape-nonascii nil))
    (setq gdk-format-policy-watch-log nil)
    (let* ((input (unibyte-string 233))
           (before-default (default-value 'print-escape-nonascii))
           (before-visible print-escape-nonascii)
           (result
            (pcase operation
              ('format-S (format "%S" input))
              ('format-s-aggregate (format "%s" (list input)))
              ('prin1-string (prin1-to-string input)))))
      (list :local local :operation operation
            :input (gdk-format-policy-witness input)
            :before-default before-default :before-visible before-visible
            :result (gdk-format-policy-witness result)
            :after-default (default-value 'print-escape-nonascii)
            :after-visible print-escape-nonascii
            :watch-log (nreverse gdk-format-policy-watch-log)
            :buffer-after (buffer-name)))))
(let ((original-default (default-value 'print-escape-nonascii))
      (watched nil)
      rows)
  (unwind-protect
      (progn
        ;; Exactly the disputed default=nil/local=nil shape. No dynamic let of
        ;; this printer flag surrounds format, since that would mask the cells.
        (set-default 'print-escape-nonascii nil)
        (when watched
          (add-variable-watcher 'print-escape-nonascii #'gdk-format-policy-watcher))
        (dolist (local '(nil t))
          (dolist (operation '(format-S format-s-aggregate prin1-string))
            (push (gdk-format-policy-row local operation) rows)))
        ;; Rows contain numeric complete result domains and ASCII buffer names.
        ;; Capture them before serialization adds any unrelated printer work.
        (list :watched watched :rows (nreverse rows)))
    (when watched
      (remove-variable-watcher 'print-escape-nonascii #'gdk-format-policy-watcher))
    (set-default 'print-escape-nonascii original-default)))
)"###;

#[test]
fn printer_settings_gnu_format_localized_scratch_escape() {
    oracle(
        "format_localized_scratch_escape",
        FORMAT_LOCALIZED_SCRATCH_ESCAPE,
        include_str!("settings_gnu/format_localized_scratch_escape.expect"),
    );
}

/// Native scope control complements the per-row local/default/caller-buffer
/// cleanup witnesses. No GNU private scratch-buffer selection is claimed.
#[test]
fn printer_format_localized_scratch_escape_restores_native_scope() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let depth = context.specpdl.len();
    let origin = context.buffers.current_buffer_id();
    let escape_before = context.obarray.symbol_value_copied("print-escape-nonascii");
    context
        .eval_str(FORMAT_LOCALIZED_SCRATCH_ESCAPE)
        .expect("localized formatter scratch-escape scope");
    assert_eq!(context.specpdl.len(), depth);
    assert_eq!(context.buffers.current_buffer_id(), origin);
    assert_eq!(
        context.obarray.symbol_value_copied("print-escape-nonascii"),
        escape_before,
    );
}

#[cfg(test)]
#[path = "output_domains.rs"]
mod output_domains;

/// GNU editfns.c:3680-3688,3720-3727 returns a resolved exact %S string.
/// The expected fixture must be refreshed from sandboxed GNU, not handwritten.
#[test]
fn printer_settings_gnu_format_exact_prin1_result_encoding() {
    oracle(
        "format_exact_prin1_result_encoding",
        r###"(let ((print-charset-text-property t))
(let* ((shape
        (lambda (text)
          (list :chars (length text)
                :bytes (string-bytes text)
                :multibyte (multibyte-string-p text)
                :codes (string-to-list text)
                :properties
                (mapcar (lambda (position)
                          (list position (text-properties-at position text)))
                        (number-sequence 0 (1- (length text)))))))
       (sources
        (list
         (list 'ascii-multibyte (string-to-multibyte (make-string 2 97)))
         (list 'byte8 (make-string 2 #x3fff80))
         (list 'ascii-properties
               (propertize (string-to-multibyte (make-string 2 97))
                           'gdk-tag '(x y) 'charset 'ascii))
         (list 'unicode-properties
               (propertize (make-string 2 233)
                           'gdk-tag '(x y) 'charset 'unicode))))
       (controls
        (list
         (list 'exact-unibyte (string-to-unibyte (copy-sequence "%S")))
         (list 'exact-multibyte (string-to-multibyte (copy-sequence "%S")))
         (list 'padded (copy-sequence "%12S"))
         (list 'prefixed (copy-sequence "x:%S"))
         (list 'propertized (propertize (copy-sequence "%S") 'gdk-format 'control)))))
  (mapcar
   (lambda (builtin)
     (list builtin
           (mapcar
            (lambda (source-row)
              (let ((source (cadr source-row)))
                (list
                 (car source-row)
                 (mapcar
                  (lambda (control-row)
                    (let* ((control (cadr control-row))
                           (source-before (funcall shape source))
                           (control-before (funcall shape control))
                           ;; Even unused multibyte inputs must not override
                           ;; GNU's resolved-string shortcut for exact %S.
                           (result (funcall builtin control source
                                            (string-to-multibyte "unused"))))
                      (list (car control-row)
                            :source-before source-before
                            :control-before control-before
                            :result (funcall shape result)
                            :source-after (funcall shape source)
                            :control-after (funcall shape control))))
                  controls))))
            sources)))
   '(format format-message))))"###,
        include_str!("settings_gnu/format_exact_prin1_result_encoding.expect"),
    );
}

/// GNU editfns.c:3680-3688,3720-3727 returns a resolved exact %S string.
/// The expected fixture must be refreshed from sandboxed GNU, not handwritten.
#[test]
fn printer_settings_gnu_format_exact_prin1_callback_control() {
    oracle(
        "format_exact_prin1_callback_control",
        r###"(let ((shape
       (lambda (text)
         (list :chars (length text)
               :bytes (string-bytes text)
               :multibyte (multibyte-string-p text)
               :codes (string-to-list text)
               :properties
               (mapcar (lambda (position)
                         (list position (text-properties-at position text)))
                       (number-sequence 0 (1- (length text))))))))
  (mapcar
   (lambda (builtin)
     (list
      builtin
      (mapcar
       (lambda (mode)
         (let* ((control (string-to-multibyte (copy-sequence "%S")))
                (calls 0)
                (print-unreadable-function
                 (lambda (_object _escaping)
                   (setq calls (1+ calls))
                   (when (memq mode '(mutate-byte add-and-mutate remove-and-mutate))
                     (aset control 1 ?d))
                   (when (memq mode '(add-properties add-and-mutate))
                     (put-text-property 0 2 'gdk-format 'after control))
                   (when (memq mode '(remove-properties remove-and-mutate))
                     (set-text-properties 0 2 nil control))
                   (garbage-collect)
                   "Q")))
           (when (memq mode '(remove-properties remove-and-mutate))
             (put-text-property 0 2 'gdk-format 'before control))
           (let* ((control-before (funcall shape control))
                  (result (funcall builtin control (make-marker)
                                   (string-to-multibyte "unused"))))
             (list mode :calls calls
                   :control-before control-before
                   :result (funcall shape result)
                   :control-after (funcall shape control)))))
       '(unchanged mutate-byte add-properties remove-properties
                   add-and-mutate remove-and-mutate))))
   '(format format-message)))"###,
        include_str!("settings_gnu/format_exact_prin1_callback_control.expect"),
    );
}
