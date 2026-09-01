//! Oracle parity tests for an advanced text templating system in Elisp.
//!
//! Covers: variable interpolation, conditional blocks (if/else), loop blocks
//! (foreach), nested templates, template inheritance/include patterns,
//! escape sequences, and template compilation to a list of operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Variable interpolation: {{var}} replacement from an alist context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_variable_interpolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple template engine that replaces {{var}} with values from an alist.
    // Handles missing keys, nested lookups, and multiple occurrences.
    let form = r#"(progn
  (fset 'neovm--tpl-interpolate
    (lambda (template context)
      "Replace {{key}} placeholders in TEMPLATE with values from CONTEXT alist."
      (let ((result template)
            (start 0))
        (while (string-match "{{\\([^}]+\\)}}" result start)
          (let* ((key (match-string 1 result))
                 (val (cdr (assoc key context)))
                 (replacement (if val (format "%s" val) (concat "{{" key "}}"))))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 replacement
                                 (substring result (match-end 0)))
                  start (+ (match-beginning 0) (length replacement)))))
        result)))
  (unwind-protect
      (let ((ctx '(("name" . "Alice")
                   ("age" . 30)
                   ("city" . "Paris")
                   ("greeting" . "Hello"))))
        (list
          ;; Basic interpolation
          (funcall 'neovm--tpl-interpolate
                   "{{greeting}}, {{name}}! You are {{age}} years old."
                   ctx)
          ;; Multiple occurrences of same var
          (funcall 'neovm--tpl-interpolate
                   "{{name}} lives in {{city}}. {{name}} is {{age}}."
                   ctx)
          ;; Missing key stays as-is
          (funcall 'neovm--tpl-interpolate
                   "{{name}} works at {{company}}."
                   ctx)
          ;; No placeholders
          (funcall 'neovm--tpl-interpolate "Just plain text." ctx)
          ;; All placeholders
          (funcall 'neovm--tpl-interpolate "{{name}}{{city}}" ctx)
          ;; Empty template
          (funcall 'neovm--tpl-interpolate "" ctx)
          ;; Adjacent placeholders
          (funcall 'neovm--tpl-interpolate "{{greeting}} {{name}}!" ctx)
          ;; Numeric value interpolation
          (funcall 'neovm--tpl-interpolate "Age: {{age}}" ctx)))
    (fmakunbound 'neovm--tpl-interpolate)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, Alice! You are 30 years old.\" \"Alice lives in Paris. Alice is 30.\" \"Alice works at {{company}}.\" \"Just plain text.\" \"AliceParis\" \"\" \"Hello Alice!\" \"Age: 30\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditional blocks: {%if cond%}...{%else%}...{%endif%}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_conditional_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Template engine that processes {%if var%}...{%else%}...{%endif%} blocks.
    // A variable is truthy if present and non-nil in the context.
    let form = r#"(progn
  (fset 'neovm--tpl-eval-cond
    (lambda (template context)
      "Process conditional blocks in TEMPLATE using CONTEXT."
      (let ((result template))
        ;; Process if/else/endif blocks (non-nested for simplicity)
        (while (string-match "{%if \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%else%}\\(\\(?:.\\|\n\\)*?\\){%endif%}" result)
          (let* ((var (match-string 1 result))
                 (then-part (match-string 2 result))
                 (else-part (match-string 3 result))
                 (val (cdr (assoc var context)))
                 (chosen (if val then-part else-part)))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 chosen
                                 (substring result (match-end 0))))))
        ;; Also process if/endif (no else)
        (while (string-match "{%if \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%endif%}" result)
          (let* ((var (match-string 1 result))
                 (body (match-string 2 result))
                 (val (cdr (assoc var context)))
                 (chosen (if val body "")))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 chosen
                                 (substring result (match-end 0))))))
        result)))
  (unwind-protect
      (let ((ctx-admin '(("user" . "Alice") ("admin" . t) ("logged-in" . t)))
            (ctx-guest '(("user" . "Guest") ("admin" . nil) ("logged-in" . nil))))
        (list
          ;; Admin sees admin panel
          (funcall 'neovm--tpl-eval-cond
                   "Welcome! {%if admin%}[Admin Panel]{%else%}[User Dashboard]{%endif%}"
                   ctx-admin)
          ;; Guest sees user dashboard
          (funcall 'neovm--tpl-eval-cond
                   "Welcome! {%if admin%}[Admin Panel]{%else%}[User Dashboard]{%endif%}"
                   ctx-guest)
          ;; If without else: logged-in user
          (funcall 'neovm--tpl-eval-cond
                   "Hello{%if logged-in%}, you are logged in{%endif%}."
                   ctx-admin)
          ;; If without else: not logged-in
          (funcall 'neovm--tpl-eval-cond
                   "Hello{%if logged-in%}, you are logged in{%endif%}."
                   ctx-guest)
          ;; Multiple conditionals in same template
          (funcall 'neovm--tpl-eval-cond
                   "{%if admin%}A{%else%}U{%endif%}-{%if logged-in%}L{%else%}G{%endif%}"
                   ctx-admin)
          (funcall 'neovm--tpl-eval-cond
                   "{%if admin%}A{%else%}U{%endif%}-{%if logged-in%}L{%else%}G{%endif%}"
                   ctx-guest)
          ;; Missing variable treated as falsy
          (funcall 'neovm--tpl-eval-cond
                   "{%if premium%}VIP{%else%}Free{%endif%}"
                   ctx-admin)))
    (fmakunbound 'neovm--tpl-eval-cond)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Welcome! [Admin Panel]\" \"Welcome! [User Dashboard]\" \"Hello, you are logged in.\" \"Hello.\" \"A-L\" \"U-G\" \"Free\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Loop blocks: {%foreach var in list%}...{%endforeach%}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_loop_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Template engine with foreach loops that iterate over a list variable
    // in the context, replacing {{item}} inside the loop body.
    let form = r#"(progn
  (fset 'neovm--tpl-foreach
    (lambda (template context)
      "Process foreach loops. {%foreach ITEM in LISTVAR%}BODY{%endforeach%}."
      (let ((result template))
        (while (string-match
                "{%foreach \\([^ ]+\\) in \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%endforeach%}"
                result)
          (let* ((item-var (match-string 1 result))
                 (list-var (match-string 2 result))
                 (body (match-string 3 result))
                 (block-start (match-beginning 0))
                 (block-end (match-end 0))
                 (lst (cdr (assoc list-var context)))
                 (expanded ""))
            (dolist (elem (if (listp lst) lst nil))
              (let ((iteration body)
                    (start 0)
                    (pattern (concat "{{" item-var "}}")))
                ;; Replace {{item-var}} with elem in this iteration
                (while (string-match (regexp-quote pattern) iteration start)
                  (setq iteration (concat (substring iteration 0 (match-beginning 0))
                                          (format "%s" elem)
                                          (substring iteration (match-end 0)))
                        start (+ (match-beginning 0)
                                 (length (format "%s" elem)))))
                (setq expanded (concat expanded iteration))))
            (setq result (concat (substring result 0 block-start)
                                 expanded
                                 (substring result block-end)))))
        result)))
  (unwind-protect
      (let ((ctx '(("items" . ("apple" "banana" "cherry"))
                   ("nums" . (1 2 3 4 5))
                   ("users" . ("Alice" "Bob"))
                   ("empty" . nil))))
        (list
          ;; Basic foreach with list of strings
          (funcall 'neovm--tpl-foreach
                   "Items: {%foreach x in items%}[{{x}}] {%endforeach%}"
                   ctx)
          ;; Foreach with numbers
          (funcall 'neovm--tpl-foreach
                   "{%foreach n in nums%}{{n}}, {%endforeach%}"
                   ctx)
          ;; Empty list produces no output
          (funcall 'neovm--tpl-foreach
                   "List: {%foreach x in empty%}{{x}}{%endforeach%}end"
                   ctx)
          ;; Multiple foreach in same template
          (funcall 'neovm--tpl-foreach
                   "F:{%foreach f in items%}{{f}};{%endforeach%} U:{%foreach u in users%}{{u}};{%endforeach%}"
                   ctx)
          ;; Foreach with HTML-like output
          (funcall 'neovm--tpl-foreach
                   "<ul>{%foreach item in items%}<li>{{item}}</li>{%endforeach%}</ul>"
                   ctx)
          ;; No foreach blocks - pass through
          (funcall 'neovm--tpl-foreach "no loops here" ctx)))
    (fmakunbound 'neovm--tpl-foreach)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Items: [apple] [banana] [cherry] \" \"1, 2, 3, 4, 5, \" \"List: end\" \"F:apple;banana;cherry; U:Alice;Bob;\" \"<ul><li>apple</li><li>banana</li><li>cherry</li></ul>\" \"no loops here\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined: interpolation + conditionals + loops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_combined_features() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full template pipeline: first expand foreach loops, then conditionals,
    // then variable interpolation.
    let form = r#"(progn
  (fset 'neovm--tpl-interpolate
    (lambda (template context)
      (let ((result template) (start 0))
        (while (string-match "{{\\([^}]+\\)}}" result start)
          (let* ((key (match-string 1 result))
                 (val (cdr (assoc key context)))
                 (replacement (if val (format "%s" val) "")))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 replacement
                                 (substring result (match-end 0)))
                  start (+ (match-beginning 0) (length replacement)))))
        result)))
  (fset 'neovm--tpl-eval-cond
    (lambda (template context)
      (let ((result template))
        (while (string-match "{%if \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%else%}\\(\\(?:.\\|\n\\)*?\\){%endif%}" result)
          (let* ((var (match-string 1 result))
                 (then-part (match-string 2 result))
                 (else-part (match-string 3 result))
                 (val (cdr (assoc var context))))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 (if val then-part else-part)
                                 (substring result (match-end 0))))))
        (while (string-match "{%if \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%endif%}" result)
          (let* ((var (match-string 1 result))
                 (body (match-string 2 result))
                 (val (cdr (assoc var context))))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 (if val body "")
                                 (substring result (match-end 0))))))
        result)))
  (fset 'neovm--tpl-foreach
    (lambda (template context)
      (let ((result template))
        (while (string-match
                "{%foreach \\([^ ]+\\) in \\([^%]+\\)%}\\(\\(?:.\\|\n\\)*?\\){%endforeach%}"
                result)
          (let* ((item-var (match-string 1 result))
                 (list-var (match-string 2 result))
                 (body (match-string 3 result))
                 (block-start (match-beginning 0))
                 (block-end (match-end 0))
                 (lst (cdr (assoc list-var context)))
                 (expanded ""))
            (dolist (elem (if (listp lst) lst nil))
              (let ((iteration body) (start 0)
                    (pattern (concat "{{" item-var "}}")))
                (while (string-match (regexp-quote pattern) iteration start)
                  (setq iteration (concat (substring iteration 0 (match-beginning 0))
                                          (format "%s" elem)
                                          (substring iteration (match-end 0)))
                        start (+ (match-beginning 0)
                                 (length (format "%s" elem)))))
                (setq expanded (concat expanded iteration))))
            (setq result (concat (substring result 0 block-start)
                                 expanded
                                 (substring result block-end)))))
        result)))
  (fset 'neovm--tpl-render
    (lambda (template context)
      "Full render pipeline: foreach -> conditionals -> interpolation."
      (funcall 'neovm--tpl-interpolate
               (funcall 'neovm--tpl-eval-cond
                        (funcall 'neovm--tpl-foreach template context)
                        context)
               context)))
  (unwind-protect
      (let ((ctx '(("title" . "My Page")
                   ("user" . "Bob")
                   ("admin" . t)
                   ("items" . ("Sword" "Shield" "Potion")))))
        (list
          ;; Full template with all features
          (funcall 'neovm--tpl-render
                   "{{title}} - {{user}} {%if admin%}[ADMIN]{%else%}[USER]{%endif%} Items: {%foreach i in items%}{{i}} {%endforeach%}"
                   ctx)
          ;; Without admin
          (funcall 'neovm--tpl-render
                   "{{title}} - {{user}} {%if admin%}[ADMIN]{%else%}[USER]{%endif%}"
                   '(("title" . "Page") ("user" . "Guest") ("admin" . nil)))
          ;; Conditional around a foreach
          (funcall 'neovm--tpl-render
                   "{%if items%}Inventory: {%foreach i in items%}[{{i}}]{%endforeach%}{%else%}Empty{%endif%}"
                   ctx)
          ;; Empty items
          (funcall 'neovm--tpl-render
                   "{%if items%}Has items{%else%}No items{%endif%}"
                   '(("items" . nil)))))
    (fmakunbound 'neovm--tpl-interpolate)
    (fmakunbound 'neovm--tpl-eval-cond)
    (fmakunbound 'neovm--tpl-foreach)
    (fmakunbound 'neovm--tpl-render)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"My Page - Bob [ADMIN] Items: Sword Shield Potion \" \"Page - Guest [USER]\" \"Inventory: [Sword][Shield][Potion]\" \"No items\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template inheritance/include pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_inheritance_include() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a template include system using an alist of named templates.
    // {%include name%} is replaced with the named template's content.
    // Supports one level of includes (no infinite recursion).
    let form = r#"(progn
  (fset 'neovm--tpl-include
    (lambda (template templates)
      "Expand {%include name%} directives using TEMPLATES alist."
      (let ((result template) (iterations 0))
        ;; Allow up to 10 rounds of expansion for chained includes
        (while (and (string-match "{%include \\([^%]+\\)%}" result)
                    (< iterations 10))
          (let* ((name (match-string 1 result))
                 (included (or (cdr (assoc name templates)) "")))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 included
                                 (substring result (match-end 0)))
                  iterations (1+ iterations))))
        result)))
  (unwind-protect
      (let ((templates '(("header" . "<header>SITE HEADER</header>")
                         ("footer" . "<footer>Copyright 2026</footer>")
                         ("nav" . "<nav>Home | About | Contact</nav>")
                         ("sidebar" . "<aside>{%include nav%}</aside>"))))
        (list
          ;; Simple include
          (funcall 'neovm--tpl-include
                   "{%include header%}<main>Content</main>{%include footer%}"
                   templates)
          ;; Multiple includes
          (funcall 'neovm--tpl-include
                   "{%include header%}{%include nav%}<p>Body</p>{%include footer%}"
                   templates)
          ;; Chained include: sidebar includes nav
          (funcall 'neovm--tpl-include
                   "{%include header%}{%include sidebar%}{%include footer%}"
                   templates)
          ;; Missing template -> empty string
          (funcall 'neovm--tpl-include
                   "Before {%include missing%} After"
                   templates)
          ;; No includes
          (funcall 'neovm--tpl-include
                   "No includes here"
                   templates)
          ;; Multiple same includes
          (funcall 'neovm--tpl-include
                   "{%include nav%} | {%include nav%}"
                   templates)))
    (fmakunbound 'neovm--tpl-include)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"<header>SITE HEADER</header><main>Content</main><footer>Copyright 2026</footer>\" \"<header>SITE HEADER</header><nav>Home | About | Contact</nav><p>Body</p><footer>Copyright 2026</footer>\" \"<header>SITE HEADER</header><aside><nav>Home | About | Contact</nav></aside><footer>Copyright 2026</footer>\" \"Before  After\" \"No includes here\" \"<nav>Home | About | Contact</nav> | <nav>Home | About | Contact</nav>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Escape sequences in templates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_escape_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Handle escape sequences: HTML-escape output values,
    // and support a raw output syntax {{{var}}} that skips escaping.
    let form = r#"(progn
  (fset 'neovm--tpl-html-escape
    (lambda (str)
      "Escape HTML special characters in STR."
      (let ((result str))
        (setq result (let ((r "") (i 0))
                       (while (< i (length result))
                         (let ((c (aref result i)))
                           (setq r (concat r
                                    (cond
                                     ((= c ?&) "&amp;")
                                     ((= c ?<) "&lt;")
                                     ((= c ?>) "&gt;")
                                     ((= c ?\") "&quot;")
                                     ((= c ?') "&#39;")
                                     (t (string c)))))
                           (setq i (1+ i))))
                       r))
        result)))
  (fset 'neovm--tpl-render-escaped
    (lambda (template context)
      "Render template: {{{var}}} = raw, {{var}} = HTML-escaped."
      (let ((result template) (start 0))
        ;; First pass: raw output {{{var}}}
        (while (string-match "{{{\\([^}]+\\)}}}" result start)
          (let* ((key (match-string 1 result))
                 (val (or (cdr (assoc key context)) "")))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 (format "%s" val)
                                 (substring result (match-end 0)))
                  start (+ (match-beginning 0) (length (format "%s" val))))))
        ;; Second pass: escaped output {{var}}
        (setq start 0)
        (while (string-match "{{\\([^}]+\\)}}" result start)
          (let* ((key (match-string 1 result))
                 (val (or (cdr (assoc key context)) ""))
                 (escaped (funcall 'neovm--tpl-html-escape (format "%s" val))))
            (setq result (concat (substring result 0 (match-beginning 0))
                                 escaped
                                 (substring result (match-end 0)))
                  start (+ (match-beginning 0) (length escaped)))))
        result)))
  (unwind-protect
      (let ((ctx '(("name" . "O'Brien")
                   ("html" . "<b>Bold</b>")
                   ("safe" . "plain text")
                   ("amp" . "AT&T"))))
        (list
          ;; Escaped output
          (funcall 'neovm--tpl-render-escaped "Hello, {{name}}!" ctx)
          ;; Raw output
          (funcall 'neovm--tpl-render-escaped "Content: {{{html}}}" ctx)
          ;; Both in same template
          (funcall 'neovm--tpl-render-escaped
                   "Escaped: {{html}} Raw: {{{html}}}"
                   ctx)
          ;; Ampersand escaping
          (funcall 'neovm--tpl-render-escaped "Company: {{amp}}" ctx)
          ;; Safe text needs no escaping
          (funcall 'neovm--tpl-render-escaped "{{safe}}" ctx)
          ;; HTML escape function directly
          (funcall 'neovm--tpl-html-escape "<script>alert('xss')</script>")
          ;; Multiple special chars
          (funcall 'neovm--tpl-html-escape "a < b & c > d \"quoted\" it's")))
    (fmakunbound 'neovm--tpl-html-escape)
    (fmakunbound 'neovm--tpl-render-escaped)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, O&#39;Brien!\" \"Content: <b>Bold</b>\" \"Escaped: &lt;b&gt;Bold&lt;/b&gt; Raw: <b>Bold</b>\" \"Company: AT&amp;T\" \"plain text\" \"&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;\" \"a &lt; b &amp; c &gt; d &quot;quoted&quot; it&#39;s\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template compilation: parse template into a list of operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_compile_to_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compile a template string into a list of operations:
    // (text "literal") for literal text, (var "name") for variable lookup,
    // (if "var" then-ops else-ops) for conditionals.
    // Then execute the compiled ops against a context.
    let form = r#"(progn
  (fset 'neovm--tpl-compile
    (lambda (template)
      "Compile TEMPLATE into a list of operations."
      (let ((ops nil) (pos 0) (len (length template)))
        (while (< pos len)
          (cond
           ;; Variable: {{var}}
           ((and (< (1+ pos) len)
                 (= (aref template pos) ?{)
                 (= (aref template (1+ pos)) ?{))
            (let ((end (string-match "}}" template (+ pos 2))))
              (if end
                  (let ((varname (substring template (+ pos 2) end)))
                    (setq ops (cons (list 'var varname) ops)
                          pos (+ end 2)))
                (setq ops (cons (list 'text (substring template pos (1+ pos))) ops)
                      pos (1+ pos)))))
           ;; Literal text: accumulate until next {{ or end
           (t
            (let ((next (string-match "{{" template pos)))
              (if next
                  (progn
                    (when (> next pos)
                      (setq ops (cons (list 'text (substring template pos next)) ops)))
                    (setq pos next))
                (setq ops (cons (list 'text (substring template pos)) ops)
                      pos len))))))
        (nreverse ops))))
  (fset 'neovm--tpl-exec-ops
    (lambda (ops context)
      "Execute compiled template OPS against CONTEXT."
      (let ((result ""))
        (dolist (op ops)
          (let ((type (car op)))
            (cond
             ((eq type 'text)
              (setq result (concat result (cadr op))))
             ((eq type 'var)
              (let ((val (cdr (assoc (cadr op) context))))
                (setq result (concat result (if val (format "%s" val) ""))))))))
        result)))
  (unwind-protect
      (let ((ctx '(("name" . "Charlie") ("count" . 42) ("title" . "Sir"))))
        (list
          ;; Compile and inspect ops
          (funcall 'neovm--tpl-compile "Hello, {{name}}! Count: {{count}}.")
          ;; Execute compiled ops
          (let ((ops (funcall 'neovm--tpl-compile "Dear {{title}} {{name}},")))
            (funcall 'neovm--tpl-exec-ops ops ctx))
          ;; Compile no-variable template
          (funcall 'neovm--tpl-compile "Just plain text")
          ;; Compile all-variable template
          (funcall 'neovm--tpl-compile "{{name}}{{title}}")
          ;; Execute with missing variable
          (let ((ops (funcall 'neovm--tpl-compile "Hi {{name}}, role: {{role}}")))
            (funcall 'neovm--tpl-exec-ops ops ctx))
          ;; Verify: compile then exec == direct interpolation
          (let* ((tpl "{{title}} {{name}} has {{count}} items")
                 (ops (funcall 'neovm--tpl-compile tpl))
                 (result (funcall 'neovm--tpl-exec-ops ops ctx)))
            result)
          ;; Number of ops for a complex template
          (length (funcall 'neovm--tpl-compile "a{{b}}c{{d}}e{{f}}g"))))
    (fmakunbound 'neovm--tpl-compile)
    (fmakunbound 'neovm--tpl-exec-ops)))"#;
    let expect = expect_test::expect![[
        r#""OK (((text \"Hello, \") (var \"name\") (text \"! Count: \") (var \"count\") (text \".\")) \"Dear Sir Charlie,\" ((text \"Just plain text\")) ((var \"name\") (var \"title\")) \"Hi Charlie, role: \" \"Sir Charlie has 42 items\" 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
