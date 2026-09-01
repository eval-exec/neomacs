use expect_test::expect;

use super::ParityBatchCase;

/// Opening a `.dart' file activates the autoloaded `dart-mode' (a
/// prog-mode derivative) with Dart's syntax-table entries, electric-indent
/// configuration, comment variables, font-lock defaults, two-space tab
/// policy, and the raw/multiline-string syntax propertizer.
fn opening_a_dart_file_activates_the_mode_and_its_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_a_dart_file_activates_the_mode_and_its_surface",
        r####"(unwind-protect
    (progn
      (dart793-test-reset)
      (let* ((file (dart793-test-fixture "surface.dart"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (list
           :file (file-relative-name file default-directory)
           :mode major-mode
           :parent (derived-mode-p 'prog-mode)
           :source (dart793-test-source-state)
           :auto-mode (and (assoc "\\.dart\\'" auto-mode-alist) t)
           :syntax
           (list :slash (char-syntax ?/)
                 :star (char-syntax ?*)
                 :newline (char-syntax ?\n)
                 :single-quote (char-syntax ?')
                 :angle (char-syntax ?<))
           :electric
           (list :indent-chars electric-indent-chars
                 :inhibit electric-indent-inhibit)
           :comments (list :start comment-start
                           :end comment-end)
           :indent (list :tabs indent-tabs-mode
                         :tab-width tab-width
                         :line-fn indent-line-function)
           :fill-column fill-column
           :font-lock font-lock-defaults
           :syntax-propertize syntax-propertize-function
           :keymap (lookup-key dart-mode-map (kbd "C-c C-i"))))))
  (dart793-test-reset))"####,
        expect![[
            r#"OK (:file "surface.dart" :mode dart-mode :parent prog-mode :source (:upstream-tree "cf2e800047a5a23401538241424b34c68335cd30" :feature t :version "20260529.1840") :auto-mode t :syntax (:slash 95 :star 46 :newline 62 :single-quote 34 :angle 46) :electric (:indent-chars (10 41 93 125) :inhibit t) :comments (:start "//" :end "") :indent (:tabs nil :tab-width 8 :line-fn dart-indent-line-function) :fill-column 72 :font-lock ((dart-font-lock-keywords-1 dart-font-lock-keywords-1 dart-font-lock-keywords-2 dart-font-lock-keywords-3)) :syntax-propertize dart-syntax-propertize-function :keymap indent-according-to-mode)"#
        ]],
    )
}

/// Fontifying a real Dart program paints the documented constructs:
/// keywords, types, the class name, a method, a string, a library
/// annotation, a doc comment, line comments, and a block comment.
fn fontifying_a_real_dart_program_paints_each_construct() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifying_a_real_dart_program_paints_each_construct",
        r####"(unwind-protect
    (progn
      (dart793-test-reset)
      (let* ((file (dart793-test-fixture "program.dart"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (insert
           "/// Doc comment for the class.\n"
           "library shapes;\n\n"
           "@Deprecated('use Shape2')\n"
           "abstract class Shape {\n"
           "  final String name;\n\n"
           "  /// Doc comment for the method.\n"
           "  double area() => 0.0;\n\n"
           "  void describe() {\n"
           "    // line comment\n"
           "    print('$name has area ${area()}');\n"
           "    /* block\n"
           "       comment */\n"
           "  }\n"
           "}\n")
          (save-buffer)
          (font-lock-ensure)
          (list
           :mode major-mode
           :lines
           (mapcar (lambda (needle) (dart793-test-line-runs needle))
                   '("/// Doc comment for the class."
                     "abstract class Shape"
                     "final String name"
                     "double area()"
                     "print('$name"
                     "// line comment"
                     "/* block"))))))
  (dart793-test-reset))"####,
        expect![[
            r#"OK (:mode dart-mode :lines ((:needle "/// Doc comment for the class." :line "/// Doc comment for the class." :runs (("/// " (font-lock-comment-delimiter-face)) ("Doc comment for the class." (font-lock-comment-face)))) (:needle "abstract class Shape" :line "abstract class Shape {" :runs (("abstract" (font-lock-builtin-face)) (" " nil) ("class" (font-lock-keyword-face)) (" " nil) ("Shape" (font-lock-type-face)) (" {" nil))) (:needle "final String name" :line "  final String name;" :runs (("  " nil) ("final" (font-lock-keyword-face)) (" " nil) ("String" (font-lock-type-face)) (" " nil) ("name" (font-lock-variable-name-face)) (";" nil))) (:needle "double area()" :line "  double area() => 0.0;" :runs (("  " nil) ("double" (font-lock-type-face)) (" " nil) ("area" (font-lock-function-name-face)) ("() => " nil) ("0.0" (font-lock-constant-face)) (";" nil))) (:needle "print('$name" :line "    print('$name has area ${area()}');" :runs (("    " nil) ("print" (font-lock-function-name-face)) ("(" nil) ("'" (font-lock-string-face)) ("$name" (font-lock-variable-name-face)) (" has area " (font-lock-string-face)) ("${area()}" (font-lock-variable-name-face)) ("'" (font-lock-string-face)) (");" nil))) (:needle "// line comment" :line "    // line comment" :runs (("    " nil) ("// " (font-lock-comment-delimiter-face)) ("line comment" (font-lock-comment-face)))) (:needle "/* block" :line "    /* block" :runs (("    " nil) ("/* block" (font-lock-comment-face))))))"#
        ]],
    )
}

/// The indentation rules: `dart-indent-line-relative' indents the body of
/// a brace/paren block relative to its opener, dedents a closing brace to
/// the opener's level, and the two-space tab policy holds through.
fn the_indentation_rules_indent_dart_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_indentation_rules_indent_dart_forms",
        r####"(unwind-protect
    (progn
      (dart793-test-reset)
      (let* ((file (dart793-test-fixture "indent.dart"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (let ((probe
                 (lambda (text)
                   (erase-buffer)
                   (insert text)
                   (goto-char (point-max))
                   (insert "\nbody")
                   (dart-indent-line-relative)
                   (current-column))))
            (list
             :mode major-mode
             :class-open (funcall probe "class A {")
             :method-open (funcall probe "  void m() {")
             :arg-open (funcall probe "  m(a,")
             :nested (funcall probe "  if (x) {")
             :plain-stmt (funcall probe "  int x;")
             :tab-width tab-width
             :tabs-mode indent-tabs-mode)))))
  (dart793-test-reset))"####,
        expect![
            "OK (:mode dart-mode :class-open 12 :method-open 14 :arg-open 14 :nested 14 :plain-stmt 6 :tab-width 8 :tabs-mode nil)"
        ],
    )
}

/// The syntax propertizer fences raw and multiline strings: content inside
/// r"..." and triple-quoted strings parses as a string even when it
/// contains quotes or newlines, and the fence syntax-table properties are
/// placed at the string boundaries.
fn the_syntax_propertizer_fences_raw_and_multiline_strings() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_syntax_propertizer_fences_raw_and_multiline_strings",
        r####"(unwind-protect
    (progn
      (dart793-test-reset)
      (let* ((file (dart793-test-fixture "strings.dart"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (insert
           "final raw = r\"a \\\"quoted\\\" value\";\n"
           "final multi = '''\n"
           "line with \" and ' quotes\n"
           "''';\n"
           "final plain = \"normal\";\n")
          (save-buffer)
          (font-lock-ensure)
          (goto-char (point-min))
          (let ((raw-pos (progn (search-forward "quoted") (point)))
                (multi-pos (progn (search-forward "and ' quotes") (point)))
                (plain-pos (progn (search-forward "normal") (point))))
            (list
             :mode major-mode
             :raw-in-string (nth 3 (syntax-ppss raw-pos))
             :raw-string-start (nth 8 (syntax-ppss raw-pos))
             :multi-in-string (nth 3 (syntax-ppss multi-pos))
             :multi-string-start (nth 8 (syntax-ppss multi-pos))
             :plain-in-string (nth 3 (syntax-ppss plain-pos))
             :lines
             (mapcar (lambda (needle) (dart793-test-line-runs needle))
                     '("final raw"
                       "final multi"
                       "line with"
                       "final plain")))))))
  (dart793-test-reset))"####,
        expect![[
            r#"OK (:mode dart-mode :raw-in-string nil :raw-string-start nil :multi-in-string 34 :multi-string-start 64 :plain-in-string 34 :lines ((:needle "final raw" :line "final raw = r\"a \\\"quoted\\\" value\";" :runs (("final" (font-lock-keyword-face)) (" " nil) ("raw" (font-lock-variable-name-face)) (" = r" nil) ("\"a \\\"" (font-lock-string-face)) ("quoted\\\" value" nil) ("\";" (font-lock-string-face)))) (:needle "final multi" :line "final multi = '''" :runs (("final multi = '''" (font-lock-string-face)))) (:needle "line with" :line "line with \" and ' quotes" :runs (("line " nil) ("with" (font-lock-keyword-face)) (" " nil) ("\" and ' quotes" (font-lock-string-face)))) (:needle "final plain" :line "final plain = \"normal\";" :runs (("final plain = \"normal\";" (font-lock-string-face))))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_a_dart_file_activates_the_mode_and_its_surface(),
        fontifying_a_real_dart_program_paints_each_construct(),
        the_indentation_rules_indent_dart_forms(),
        the_syntax_propertizer_fences_raw_and_multiline_strings(),
    ]
}
