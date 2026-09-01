use expect_test::expect;

use super::ParityBatchCase;

fn activation_and_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "activates_unicode_scala_files_with_real_syntax_and_prettify",
        r###"
(scala365-test-run
 "activates_unicode_scala_files_with_real_syntax_and_prettify"
 :space-unicode
 (lambda (_world)
   (let* ((source
           (concat
            "#!/usr/bin/env scala\npackage demo.界\n\n"
            "/** Service doc with @param value and [[Link]]. */\n"
            "sealed trait Event\n"
            "final case class Added(value: Int) extends Event\n"
            "object Runner {\n"
            "  @deprecated(\"old\", \"1\") val total = 0x2aL\n"
            "  var mutable = s\"value=$total\"\n"
            "  def render(xs: List[Int]): String = xs match {\n"
            "    case head :: tail => s\"${head}: ${tail.sum}\\n\"\n"
            "    case Nil => \"empty sealed trait\"\n"
            "  }\n"
            "  val raw = \"\"\"triple string\nline two\"\"\"\n"
            "  /* outer /* nested */ done */\n"
            "  val rune: Char = 'λ'\n"
            "  def ++(other: Int): Int = total + other\n"
            "  val `match`: Boolean = true && false || total != 0\n"
            "  val arrow = ((x: Int) => x + 1)\n"
            "  val yielded = for (x <- List(1)) yield x\n"
            "  given ordering: Ordering[Int] = Ordering.Int\n"
            "  extension (x: Int) def twice(using y: Int) = x + y\n"
            "}\n"))
          (scala (scala365-test-visit "src space/Unicode界.scala" source))
          scala-state scala-runs hooks pretty sbt-mode worksheet-mode near-mode)
     (with-current-buffer scala
       (setq-local prettify-symbols-alist scala-prettify-symbols-alist)
       (call-interactively #'prettify-symbols-mode)
       (font-lock-ensure)
       (setq scala-state
             (list :buffer (scala365-test-buffer-state)
                   :coding buffer-file-coding-system :comment comment-start
                   :indent-tabs indent-tabs-mode
                   :syntax-function syntax-propertize-function
                   :forward forward-sexp-function :indent indent-line-function
                   :fill fill-paragraph-function :imenu imenu-create-index-function))
       (setq hooks (scala365-test-local-hooks)
             scala-runs (scala365-test-property-runs
                         '(face syntax-table fontified))
             pretty (scala365-test-property-runs '(composition))))
     (let ((buffer (scala365-test-visit "build space/demo.sbt"
                                        "ThisBuild / scalaVersion := \"2.13.16\"\n")))
       (setq sbt-mode (with-current-buffer buffer
                        (list major-mode buffer-file-coding-system))))
     (let ((buffer (scala365-test-visit "notes/demo.worksheet.sc"
                                        "val worksheet = 1\n")))
       (setq worksheet-mode (with-current-buffer buffer
                              (list major-mode buffer-file-coding-system))))
     (let ((buffer (scala365-test-visit "notes/demo.scalax" "val near = 1\n")))
       (setq near-mode (with-current-buffer buffer major-mode)))
     (list :provenance (scala365-test-provenance)
           :scala scala-state :hooks hooks :semantic-runs scala-runs
           :prettify pretty :sbt sbt-mode :worksheet worksheet-mode
           :near-miss near-mode))))
"###,
        expect![[
            r##"OK (:result (:provenance (:version "20260118.942" :commit "50bcafa181baec7054e27f4bca55d5f9277c6350" :source-files (("scala-mode.el" . "b6b36c2cc87e9d5fd947c4b47f364f9860c49419dba38575488ab7fc742521a2") ("scala-mode-syntax.el" . "ef8a3fa3da75e62262d03914ac5eaa577131158170b69f50121d0b0d3b40b711") ("scala-mode-indent.el" . "176ad15a4d8631a7dd7e2c01e150a4bdfcd51dd7cfb93a46f472b5345f267fc9") ("scala-mode-fontlock.el" . "06d0da90d49f31e4465748dcd51241b4b8cea5abf58836e368bd59741639e90b") ("scala-mode-map.el" . "9bf772541ef638a6da184249517f7bf17cf91a4574defa3b64a714d996cbba67") ("scala-mode-prettify-symbols.el" . "897d4debe8966224ab58c7f3bdb332ad65d211e9e75fdc82ea17e1b8ad86e7dc") ("scala-mode-lib.el" . "801b3c8c3f9c0ba247d3c60c75575c6babfd4ae73dc6be5ee7884f06d8a3a5b9") ("scala-mode-imenu.el" . "55a601e03f24399e14f4ba99b0aed50c7ac0f82ce7c0b91af130840c5a3ae2c6") ("scala-mode-paragraph.el" . "5472721bce109c062e93cc4782b59a317b744d652473609d522c3d15d10f6522") ("scala-compile.el" . "6275cdc73daec5f42209683cdab39ff5335efe74ce2a7cc888bcb36849ed2c29") ("scala-organise.el" . "71df577bed8259d4384f66be4544f97f7e832cc16a18eeec83e0bb0401457be5")) :installed-root "5ac029f2921d4b72df2e24501213c734aa05b95fd2928b1f07d39cef421dda9d" :dependency-closure nil :global-post-command-count 1 :default-syntax-hooks (syntax-propertize-wholelines) :default-post-self-hooks (electric-indent-post-self-insert-function blink-paren-post-self-insert-function) :tool (:version "1.12.14" :java "21.0.10" :scala "2.13.16" :archive-sha256 "cd17daae220ff264faa4251334522444518584f0eb2ee82da01523a9b9002b7e" :script-sha256 "0479b7d305132e216bdd0c8aa376f916dc062c4cae010f21625c033b08435715" :launcher-sha256 "1750c8fb61c2d2f82da40b2cd9014f4d8a1bd49a361402ea5b1cec061ed66578" :sbtn-version "2.0.0-b4d628dd" :sbtn-sha256 "4527047664bce3f473f3bc960be888199947c74346a1ea9f717809e8dcefbcc6") :failure-stream "ea0b785b102c1ae2348042b4b7815bcb6594e0601354b7ed2fc0f9a90fe0f005" :success-stream "cf44689cc6c5a78f3ba28bc053a8a3002e7d9062984182095a36d0264a70959b") :scala (:buffer (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo.界\n\n/** Service doc with @param value and [[Link]]. */\nsealed trait Event\nfinal case class Added(value: Int) extends Event\nobject Runner {\n  @deprecated(\"old\", \"1\") val total = 0x2aL\n  var mutable = s\"value=$total\"\n  def render(xs: List[Int]): String = xs match {\n    case head :: tail => s\"${head}: ${tail.sum}\\n\"\n    case Nil => \"empty sealed trait\"\n  }\n  val raw = \"\"\"triple string\nline two\"\"\"\n  /* outer /* nested */ done */\n  val rune: Char = 'λ'\n  def ++(other: Int): Int = total + other\n  val `match`: Boolean = true && false || total != 0\n  val arrow = ((x: Int) => x + 1)\n  val yielded = for (x <- List(1)) yield x\n  given ordering: Ordering[Int] = Ordering.Int\n  extension (x: Int) def twice(using y: Int) = x + y\n}\n" :point 1 :line 1 :column 0 :mark nil :active nil :modified nil :undo nil :narrowed nil) :coding utf-8-unix :comment "// " :indent-tabs nil :syntax-function scala-syntax:propertize :forward scala-mode:forward-sexp-function :indent scala-indent:indent-line :fill scala-paragraph:fill-paragraph :imenu scala-imenu:create-imenu-index) :hooks (:syntax-local t :syntax (scala-syntax:propertize-extend-region syntax-propertize-wholelines) :post-self-local t :post-self (scala-indent:fix-scaladoc-close scala-indent:indent-on-scaladoc-asterisk scala-indent:indent-on-special-words scala-indent:indent-on-parentheses t) :post-command-local t :post-command (eldoc-schedule-timer t)) :semantic-runs ((1 3 "#!" (font-lock-comment-face (11) nil)) (3 21 "/usr/bin/env scala" (font-lock-comment-face nil nil)) (21 22 "\n" (font-lock-comment-face (12) nil)) (22 29 "package" (font-lock-keyword-face nil nil)) (30 35 "demo." (font-lock-string-face nil nil)) (38 88 "/** Service doc with @param value and [[Link]]. */" (font-lock-doc-face nil nil)) (89 95 "sealed" (scala-font-lock:sealed-face nil nil)) (96 101 "trait" (font-lock-keyword-face nil nil)) (102 107 "Event" (font-lock-type-face nil nil)) (108 113 "final" (scala-font-lock:final-face nil nil)) (114 118 "case" (font-lock-keyword-face nil nil)) (119 124 "class" (font-lock-keyword-face nil nil)) (125 130 "Added" (font-lock-type-face nil nil)) (136 137 ":" (font-lock-keyword-face nil nil)) (138 141 "Int" (font-lock-type-face nil nil)) (143 150 "extends" (font-lock-keyword-face nil nil)) (151 156 "Event" (font-lock-type-face nil nil)) (157 163 "object" (font-lock-keyword-face nil nil)) (164 170 "Runner" (font-lock-constant-face nil nil)) (175 186 "@deprecated" (font-lock-preprocessor-face nil nil)) (187 188 "\"" (font-lock-string-face (7) nil)) (188 191 "old" (font-lock-string-face nil nil)) (191 192 "\"" (font-lock-string-face (7) nil)) (194 195 "\"" (font-lock-string-face (7) nil)) (195 196 "1" (font-lock-string-face nil nil)) (196 197 "\"" (font-lock-string-face (7) nil)) (199 202 "val" (font-lock-keyword-face nil nil)) (203 208 "total" (font-lock-variable-name-face nil nil)) (209 210 "=" (font-lock-keyword-face nil nil)) (211 216 "0x2aL" (font-lock-constant-face nil nil)) (219 222 "var" (font-lock-keyword-face nil nil)) (223 230 "mutable" (scala-font-lock:var-face nil nil)) (231 232 "=" (font-lock-keyword-face nil nil)) (234 235 "\"" (font-lock-string-face (7) nil)) (235 241 "value=" (font-lock-string-face nil nil)) (241 242 "$" (font-lock-variable-name-face (1) nil)) (242 247 "total" (font-lock-variable-name-face nil nil)) (247 248 "\"" (font-lock-string-face (7) nil)) (251 254 "def" (font-lock-keyword-face nil nil)) (255 261 "render" (font-lock-function-name-face nil nil)) (264 265 ":" (font-lock-keyword-face nil nil)) (266 270 "List" (font-lock-type-face nil nil)) (271 274 "Int" (font-lock-constant-face nil nil)) (276 277 ":" (font-lock-keyword-face nil nil)) (278 284 "String" (font-lock-type-face nil nil)) (285 286 "=" (font-lock-keyword-face nil nil)) (290 295 "match" (font-lock-keyword-face nil nil)) (302 306 "case" (font-lock-keyword-face nil nil)) (307 311 "head" (font-lock-variable-name-face nil nil)) (312 314 "::" (font-lock-type-face (3) nil)) (315 319 "tail" (font-lock-variable-name-face nil nil)) (320 322 "=>" (font-lock-keyword-face (3) nil)) (324 325 "\"" (font-lock-string-face (7) nil)) (325 326 "$" (font-lock-variable-name-face (1) nil)) (326 332 "{head}" (font-lock-variable-name-face nil nil)) (332 334 ": " (font-lock-string-face nil nil)) (334 335 "$" (font-lock-variable-name-face (1) nil)) (335 345 "{tail.sum}" (font-lock-variable-name-face nil nil)) (345 347 "\\n" ((font-lock-constant-face font-lock-string-face) nil nil)) (347 348 "\"" (font-lock-string-face (7) nil)) (353 357 "case" (font-lock-keyword-face nil nil)) (358 361 "Nil" (font-lock-constant-face nil nil)) (362 364 "=>" (font-lock-keyword-face (3) nil)) (365 366 "\"" (font-lock-string-face (7) nil)) (366 384 "empty sealed trait" (font-lock-string-face nil nil)) (384 385 "\"" (font-lock-string-face (7) nil)) (392 395 "val" (font-lock-keyword-face nil nil)) (396 399 "raw" (font-lock-variable-name-face nil nil)) (400 401 "=" (font-lock-keyword-face nil nil)) (402 403 "\"" (font-lock-string-face (15) nil)) (403 429 "\"\"triple string\nline two\"\"" (font-lock-string-face nil nil)) (429 430 "\"" (font-lock-string-face (15) nil)) (433 436 "/* " (font-lock-comment-delimiter-face nil nil)) (436 462 "outer /* nested */ done */" (font-lock-comment-face nil nil)) (465 468 "val" (font-lock-keyword-face nil nil)) (469 473 "rune" (font-lock-variable-name-face nil nil)) (473 474 ":" (font-lock-keyword-face nil nil)) (475 479 "Char" (font-lock-type-face nil nil)) (480 481 "=" (font-lock-keyword-face nil nil)) (482 483 "'" (font-lock-string-face (7) nil)) (483 484 "λ" (font-lock-string-face nil nil)) (484 485 "'" (font-lock-string-face (7) nil)) (488 491 "def" (font-lock-keyword-face nil nil)) (492 494 "++" (font-lock-function-name-face (3) nil)) (500 501 ":" (font-lock-keyword-face nil nil)) (502 505 "Int" (font-lock-type-face nil nil)) (506 507 ":" (font-lock-keyword-face nil nil)) (508 511 "Int" (font-lock-type-face nil nil)) (512 513 "=" (font-lock-keyword-face nil nil)) (530 533 "val" (font-lock-keyword-face nil nil)) (534 541 "`match`" (font-lock-variable-name-face (3) nil)) (541 542 ":" (font-lock-keyword-face nil nil)) (543 550 "Boolean" (font-lock-type-face nil nil)) (551 552 "=" (font-lock-keyword-face nil nil)) (553 557 "true" (font-lock-constant-face nil nil)) (558 560 "&&" (nil (3) nil)) (561 566 "false" (font-lock-constant-face nil nil)) (567 569 "||" (nil (3) nil)) (576 578 "!=" (nil (3) nil)) (579 580 "0" (font-lock-constant-face nil nil)) (583 586 "val" (font-lock-keyword-face nil nil)) (587 592 "arrow" (font-lock-variable-name-face nil nil)) (593 594 "=" (font-lock-keyword-face nil nil)) (598 599 ":" (font-lock-keyword-face nil nil)) (600 603 "Int" (font-lock-type-face nil nil)) (605 607 "=>" (font-lock-keyword-face (3) nil)) (612 613 "1" (font-lock-constant-face nil nil)) (617 620 "val" (font-lock-keyword-face nil nil)) (621 628 "yielded" (font-lock-variable-name-face nil nil)) (629 630 "=" (font-lock-keyword-face nil nil)) (631 634 "for" (font-lock-keyword-face nil nil)) (638 640 "<-" (font-lock-keyword-face (3) nil)) (641 645 "List" (font-lock-constant-face nil nil)) (646 647 "1" (font-lock-constant-face nil nil)) (650 655 "yield" (font-lock-keyword-face nil nil)) (660 665 "given" (font-lock-keyword-face nil nil)) (674 675 ":" (font-lock-keyword-face nil nil)) (676 684 "Ordering" (font-lock-type-face nil nil)) (685 688 "Int" (font-lock-constant-face nil nil)) (690 691 "=" (font-lock-keyword-face nil nil)) (692 700 "Ordering" (font-lock-constant-face nil nil)) (701 704 "Int" (font-lock-constant-face nil nil)) (707 716 "extension" (font-lock-keyword-face nil nil)) (719 720 ":" (font-lock-keyword-face nil nil)) (721 724 "Int" (font-lock-type-face nil nil)) (726 729 "def" (font-lock-keyword-face nil nil)) (730 735 "twice" (font-lock-function-name-face nil nil)) (736 741 "using" (font-lock-keyword-face nil nil)) (743 744 ":" (font-lock-keyword-face nil nil)) (745 748 "Int" (font-lock-type-face nil nil)) (750 751 "=" (font-lock-keyword-face nil nil))) :prettify ((138 141 "Int" (((3 . 8484)))) (271 274 "Int" (((3 . 8484)))) (312 314 "::" (((2 . 11820)))) (320 322 "=>" (((2 . 8658)))) (362 364 "=>" (((2 . 8658)))) (492 494 "++" (((2 . 10746)))) (502 505 "Int" (((3 . 8484)))) (508 511 "Int" (((3 . 8484)))) (543 550 "Boolean" (((7 . 120121)))) (553 557 "true" (((4 . 8868)))) (558 560 "&&" (((2 . 8743)))) (561 566 "false" (((5 . 8869)))) (567 569 "||" (((2 . 8744)))) (576 578 "!=" (((2 . 8802)))) (600 603 "Int" (((3 . 8484)))) (605 607 "=>" (((2 . 8658)))) (638 640 "<-" (((2 . 8592)))) (685 688 "Int" (((3 . 8484)))) (701 704 "Int" (((3 . 8484)))) (721 724 "Int" (((3 . 8484)))) (745 748 "Int" (((3 . 8484))))) :sbt (scala-mode utf-8-unix) :worksheet (scala-mode utf-8-unix) :near-miss fundamental-mode) :cleanup clean)"##
        ]],
    )
}

fn electric_command_loop() -> ParityBatchCase {
    ParityBatchCase::value(
        "types_and_reindents_scala_through_one_real_command_loop",
        r###"
(scala365-test-run
 "types_and_reindents_scala_through_one_real_command_loop"
 :space-unicode
 (lambda (_world)
   (let* ((buffer
           (scala365-test-visit
            "edit/Command界.scala"
            (concat "object T {\n  val x = foo(1)\nbar(2)\n"
                    "  val chain = service\n    .foo(1)\n    .bar(2)\n}\n")))
          object-close tab-states tab-observer typed abandoned unmatched-close recovery)
     (with-current-buffer buffer
       (goto-char (point-max))
       (skip-chars-backward " \t\n")
       (backward-char)
       (unless (eq (char-after) ?})
         (error "Scala Mode fixture has no final object close: %S"
                (char-after)))
       (setq object-close (copy-marker (point) t))
       (push object-close scala365-test-owned-markers)
       (goto-char (point-min)) (forward-line 2)
       (setq tab-observer
             (lambda ()
               (when (eq this-command 'indent-for-tab-command)
                 (push (list :indent (current-indentation)
                             :effective scala-indent:effective-run-on-strategy
                             :point (point) :last last-command)
                       tab-states))))
       (add-hook 'post-command-hook tab-observer nil t)
       (scala365-test-run-keys '(:keys "TAB TAB TAB"))
       (remove-hook 'post-command-hook tab-observer t)
       (setq tab-states (nreverse tab-states))
       (goto-char object-close)
       (scala365-test-run-contiguous
        '(:keys "RET")
        '(:text "  def choose(界: Int) = 界 match {")
        '(:keys "RET TAB") '(:text "case 1 => foo(") '(:text "2")
        '(:text ")") '(:keys "RET") '(:text "}")
        '(:keys "RET") '(:text "if (界 > 0) {")
        '(:keys "RET") '(:text "foo()")
        '(:keys "RET") '(:text "}")
        '(:keys "RET") '(:text "else ") '(:text "fallback()")
        '(:keys "RET") '(:text "/** typed") '(:keys "RET")
        '(:text "*") '(:text "/") '(:keys "RET"))
       (font-lock-ensure)
       (setq typed (scala365-test-buffer-state))
       (goto-char object-close)
       (scala365-test-run-contiguous '(:keys "RET TAB") '(:text " ")
                                     '(:keys "RET") '(:text "val cleanup = 1")
                                     '(:keys "RET"))
       (setq abandoned (scala365-test-buffer-state))
       (goto-char object-close)
       (setq unmatched-close
             (condition-case condition
                 (progn
                   (scala365-test-run-contiguous '(:text ")")
                                                 '(:keys "C-M-b"))
                   (list :returned (point)
                         :state (scala365-test-buffer-state)))
               (t (list :condition (scala365-test-condition-state condition)
                        :state (scala365-test-buffer-state)))))
       (goto-char object-close)
       (scala365-test-run-contiguous '(:keys "C-_") '(:keys "C-M-b"))
       (setq recovery
             (list :returned (point) :state (scala365-test-buffer-state))))
     (list :tabs tab-states :default-run-on scala-indent:default-run-on-strategy
           :typed typed :abandoned-indent abandoned
           :unmatched-close-motion unmatched-close :undo-recovery recovery))))
"###,
        expect![[
            r#"OK (:result (:tabs ((:indent 2 :effective nil :point 31 :last nil) (:indent 4 :effective 0 :point 33 :last indent-for-tab-command) (:indent 2 :effective nil :point 31 :last indent-for-tab-command)) :default-run-on 2 :typed (:mode scala-mode :text "object T {\n  val x = foo(1)\n  bar(2)\n  val chain = service\n    .foo(1)\n    .bar(2)\n\n  def choose(界: Int) = 界 match {\n    case 1 => foo(2)\n  }\n  if (界 > 0) {\n    foo()\n  }\n  else fallback()\n  /** typed\n    */\n}\n" :point 209 :line 17 :column 0 :mark nil :active nil :modified t :undo t :narrowed nil) :abandoned-indent (:mode scala-mode :text "object T {\n  val x = foo(1)\n  bar(2)\n  val chain = service\n    .foo(1)\n    .bar(2)\n\n  def choose(界: Int) = 界 match {\n    case 1 => foo(2)\n  }\n  if (界 > 0) {\n    foo()\n  }\n  else fallback()\n  /** typed\n    */\n\n\n  val cleanup = 1\n}\n" :point 229 :line 20 :column 0 :mark nil :active nil :modified t :undo t :narrowed nil) :unmatched-close-motion (:returned 10 :state (:mode scala-mode :text "object T {\n  val x = foo(1)\n  bar(2)\n  val chain = service\n    .foo(1)\n    .bar(2)\n\n  def choose(界: Int) = 界 match {\n    case 1 => foo(2)\n  }\n  if (界 > 0) {\n    foo()\n  }\n  else fallback()\n  /** typed\n    */\n\n\n  val cleanup = 1\n)}\n" :point 10 :line 1 :column 9 :mark nil :active nil :modified t :undo t :narrowed nil)) :undo-recovery (:returned 227 :state (:mode scala-mode :text "object T {\n  val x = foo(1)\n  bar(2)\n  val chain = service\n    .foo(1)\n    .bar(2)\n\n  def choose(界: Int) = 界 match {\n    case 1 => foo(2)\n  }\n  if (界 > 0) {\n    foo()\n  }\n  else fallback()\n  /** typed\n    */\n\n\n  val cleanup = 1\n}\n" :point 227 :line 19 :column 16 :mark nil :active nil :modified t :undo t :narrowed nil))) :cleanup clean)"#
        ]],
    )
}

fn indentation_customization() -> ParityBatchCase {
    ParityBatchCase::value(
        "applies_each_documented_indentation_style_to_real_source",
        r###"
(scala365-test-run
 "applies_each_documented_indentation_style_to_real_source"
 :space-unicode
 (lambda (_world)
   (let* ((source
           (concat "object Matrix {\ndef apply(\nfirst: Int,\n"
                   "longerName: String\n): Int = {\n"
                   "val names = List(\"Alpha\", \"Bravo\",\n\"Charlie\")\n"
                   "val chosen = if (first > 0)\nfirst\nelse\n0\n"
                   "val total = first +\nlongerName.length\n"
                   "names.size + chosen + total\n}\n}\n"))
          (buffer (scala365-test-visit "indent/Matrix.scala" source))
          results)
     (with-current-buffer buffer
       (dolist
           (entry
            '((:default)
              (:parameters (scala-indent:align-parameters t))
              (:forms (scala-indent:align-forms t))
              (:value (scala-indent:indent-value-expression t))
              (:operators (scala-indent:default-run-on-strategy 1))
              (:step (scala-indent:step 4))
              (:eager (scala-indent:default-run-on-strategy 0))
              (:composed (scala-indent:align-parameters t)
                         (scala-indent:align-forms t)
                         (scala-indent:indent-value-expression t))))
         (let ((inhibit-read-only t))
           (erase-buffer) (insert source) (set-buffer-modified-p nil))
         (kill-all-local-variables)
         (scala-mode)
         (dolist (setting (cdr entry))
           (set (make-local-variable (car setting)) (cadr setting)))
         (goto-char (point-min))
         (push-mark (point-max) nil t)
         (call-interactively #'indent-region)
         (deactivate-mark)
         (push (list (car entry) (scala365-test-buffer-state)) results)))
     (list :outputs (nreverse results)))))
"###,
        expect![[
            r#"OK (:result (:outputs ((:default (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n      \"Charlie\")\n    val chosen = if (first > 0)\n      first\n    else\n      0\n    val total = first +\n      longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 285 :active t :modified t :undo t :narrowed nil)) (:parameters (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n                     \"Charlie\")\n    val chosen = if (first > 0)\n      first\n    else\n      0\n    val total = first +\n      longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 300 :active t :modified t :undo t :narrowed nil)) (:forms (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n      \"Charlie\")\n    val chosen = if (first > 0)\n                   first\n                 else\n                   0\n    val total = first +\n      longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 324 :active t :modified t :undo t :narrowed nil)) (:value (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n        \"Charlie\")\n    val chosen = if (first > 0)\n        first\n      else\n        0\n    val total = first +\n      longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 293 :active t :modified t :undo t :narrowed nil)) (:operators (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n      \"Charlie\")\n    val chosen = if (first > 0)\n      first\n    else\n      0\n    val total = first +\n    longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 283 :active t :modified t :undo t :narrowed nil)) (:step (:mode scala-mode :text "object Matrix {\n    def apply(\n        first: Int,\n        longerName: String\n    ): Int = {\n        val names = List(\"Alpha\", \"Bravo\",\n            \"Charlie\")\n        val chosen = if (first > 0)\n            first\n        else\n            0\n        val total = first +\n            longerName.length\n        names.size + chosen + total\n    }\n}\n" :point 1 :line 1 :column 0 :mark 343 :active t :modified t :undo t :narrowed nil)) (:eager (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n      \"Charlie\")\n    val chosen = if (first > 0)\n      first\n    else\n      0\n    val total = first +\n    longerName.length\n      names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 285 :active t :modified t :undo t :narrowed nil)) (:composed (:mode scala-mode :text "object Matrix {\n  def apply(\n    first: Int,\n    longerName: String\n  ): Int = {\n    val names = List(\"Alpha\", \"Bravo\",\n                     \"Charlie\")\n    val chosen = if (first > 0)\n                   first\n                 else\n                   0\n    val total = first +\n      longerName.length\n    names.size + chosen + total\n  }\n}\n" :point 1 :line 1 :column 0 :mark 339 :active t :modified t :undo t :narrowed nil)))) :cleanup clean)"#
        ]],
    )
}

fn motion_fill_join_fixup() -> ParityBatchCase {
    ParityBatchCase::value(
        "moves_fills_joins_and_repairs_real_scala_text",
        r###"
(scala365-test-run
 "moves_fills_joins_and_repairs_real_scala_text"
 :space-unicode
 (lambda (_world)
   (let* ((source
           (concat "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\n"
                   "object Motion {\n"
                   "  /** A long scaladoc paragraph that should wrap across practical narrow columns while retaining its comment prefix.\n"
                   "    * - first list item remains structurally distinct\n"
                   "    * @param value a separate semantic paragraph\n    */\n"
                   "  def compute: String = {\n"
                   "    /* outer comment /* nested comment */ tail */\n"
                   "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n"
                   "    val raw = \"\"\"|A long multiline string paragraph that should wrap across practical narrow columns while staying inside the string.\n"
                   "                 |and preserve its margin.\"\"\".stripMargin\n"
                   "    val punct = foo ( 1 ,  2 )\n"
                   "      . bar\n"
                   "    val fixes = foo ( 3 ,  4 ) . baz\n"
                   "    val block = {  1 + 2  }\n"
                   "    nested\n  }\n}\n"))
          (buffer (scala365-test-visit "motion/Motion.scala" source))
          motions code-start fill-config filled joined code-join-before code-joined
          code-join-undo code-join-rejoined doc-default doc-javadoc fixed
          broken broken-recovery)
     (with-current-buffer buffer
       (goto-char (point-min))
       (dolist (keys '("C-M-f" "C-M-b" "C-M-e" "C-M-a"))
         (scala365-test-run-keys (list :keys keys))
         (push (list keys (point) (line-number-at-pos)
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))
               motions))
       (goto-char (point-min)) (search-forward "outer comment")
       (dolist (keys '("C-M-f" "C-M-b"))
         (scala365-test-run-keys (list :keys keys))
         (push (list (concat "nested-" keys) (point) (line-number-at-pos)
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))
               motions))
       (dolist
           (entry
            '(("interpolation-forward" "${s\"inner" "C-M-f")
              ("interpolation-backward" "${2 + 3}" "C-M-b")
              ("triple-forward" "|A long multiline" "C-M-f")
              ("triple-backward" "preserve its margin" "C-M-b")
              ("defun-end" "def compute" "C-M-e")
              ("defun-beginning" "val nested" "C-M-a")))
         (goto-char (point-min)) (search-forward (cadr entry))
         (let ((before (scala365-test-point-state)))
           (scala365-test-run-keys (list :keys (caddr entry)))
           (push (list :route (car entry) :before before
                       :after (scala365-test-point-state))
                 motions)))
       (goto-char (point-min))
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-mode:goto-start-of-code")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq code-start
             (list (point) (line-number-at-pos)
                   (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))))
       (setq-local fill-column 52)
       (goto-char (point-min)) (search-forward "long scaladoc")
       (scala365-test-run-keys '(:keys "M-q"))
       (goto-char (point-min)) (search-forward "long multiline")
       (scala365-test-run-keys '(:keys "M-q"))
       (setq fill-config (list :column fill-column :prefix fill-prefix)
             filled (scala365-test-buffer-state))
       (goto-char (point-min)) (search-forward "@param") (beginning-of-line)
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-indent:join-line")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq joined (scala365-test-buffer-state))
       (goto-char (point-min)) (search-forward ". bar") (beginning-of-line)
       (setq code-join-before (scala365-test-buffer-state)
             scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-indent:join-line")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq code-joined (scala365-test-buffer-state))
       (scala365-test-run-keys '(:keys "C-_"))
       (setq code-join-undo (scala365-test-buffer-state)
             scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-indent:join-line")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq code-join-rejoined (scala365-test-buffer-state))
       (goto-char (point-max))
       (insert "\n  /** typed default\n\n")
       (search-backward "\n\n") (forward-char)
       (scala365-test-run-contiguous '(:text "*") '(:text "/"))
       (setq doc-default (scala365-test-buffer-state))
       (setq-local scala-indent:use-javadoc-style t)
       (goto-char (point-max))
       (insert "\n  /** typed javadoc\n\n")
       (search-backward "\n\n") (forward-char)
       (scala365-test-run-contiguous '(:text "*") '(:text "/"))
       (setq doc-javadoc (scala365-test-buffer-state))
       (goto-char (point-min)) (search-forward "val fixes")
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-indent:fixup-whitespace")))
       (search-forward "foo ")
       (scala365-test-run-keys '(:keys "M-x"))
       (dolist (needle '("3 " ",  " ") " ". "))
         (goto-char (point-min)) (search-forward "val fixes")
         (search-forward needle (line-end-position))
         (when (equal needle "  }") (backward-char))
         (call-interactively #'scala-indent:fixup-whitespace))
       (dolist (needle '("{  " "  }"))
         (goto-char (point-min)) (search-forward "val block")
         (search-forward needle (line-end-position))
         (when (equal needle "  }") (backward-char))
         (call-interactively #'scala-indent:fixup-whitespace))
       (setq fixed (scala365-test-buffer-state))
       (goto-char (point-max))
       (insert "  val broken = s\"${1 + 2}\n")
       (let ((origin (point))
             (before (scala365-test-buffer-state))
             outcome)
         (setq outcome
               (condition-case condition
                   (progn (scala365-test-run-keys '(:keys "C-M-f"))
                          (list :returned (point)))
                 (t (scala365-test-condition-state condition))))
         (setq broken
               (list :before before :outcome outcome
                     :after (scala365-test-buffer-state)
                     :no-progress (= origin (point)))))
       (goto-char (point-max))
       (scala365-test-run-contiguous '(:keys "C-b") '(:text "\""))
       (let ((origin (point)))
       (condition-case condition
           (progn (scala365-test-run-keys '(:keys "C-M-b"))
                  (setq broken-recovery
                        (list :returned (point) :progress (/= origin (point))
                              :state (scala365-test-buffer-state))))
         (t (setq broken-recovery
                  (list :condition (scala365-test-condition-state condition)
                        :progress (/= origin (point))
                        :state (scala365-test-buffer-state))))))
     (list :motions (nreverse motions) :code-start code-start
           :fill-config fill-config
           :filled filled :scaladoc-joined joined
           :code-join-before code-join-before :code-joined code-joined
           :code-join-undo code-join-undo
           :code-join-rejoined code-join-rejoined :doc-default doc-default
           :doc-javadoc doc-javadoc :fixed fixed :broken broken
           :broken-recovery broken-recovery
           :completions (nreverse scala365-test-completion-events))))))
"###,
        expect![[
            r##"OK (:result (:motions (("C-M-f" 29 2 "package demo") ("C-M-b" 22 2 "package demo") ("C-M-e" 737 22 "") ("C-M-a" 54 5 "object Motion {") ("nested-C-M-f" 369 11 "    /* outer comment /* nested comment */ tail */") ("nested-C-M-b" 365 11 "    /* outer comment /* nested comment */ tail */") (:route "interpolation-forward" :before (:point 407 :line 12 :column 34 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"") :after (:point 409 :line 12 :column 36 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"")) (:route "interpolation-backward" :before (:point 416 :line 12 :column 43 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"") :after (:point 409 :line 12 :column 36 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"")) (:route "triple-forward" :before (:point 454 :line 13 :column 34 :text "    val raw = \"\"\"|A long multiline string paragraph that should wrap across practical narrow columns while staying inside the string.") :after (:point 461 :line 13 :column 41 :text "    val raw = \"\"\"|A long multiline string paragraph that should wrap across practical narrow columns while staying inside the string.")) (:route "triple-backward" :before (:point 595 :line 14 :column 41 :text "                 |and preserve its margin.\"\"\".stripMargin") :after (:point 589 :line 14 :column 35 :text "                 |and preserve its margin.\"\"\".stripMargin")) (:route "defun-end" :before (:point 310 :line 10 :column 13 :text "  def compute: String = {") :after (:point 735 :line 21 :column 0 :text "}")) (:route "defun-beginning" :before (:point 387 :line 12 :column 14 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"") :after (:point 377 :line 12 :column 4 :text "    val nested = s\"outer ${s\"inner ${2 + 3}\"}\""))) :code-start (54 5 "object Motion {") :fill-config (:column 52 :prefix nil) :filled (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct\n    * @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 )\n      . bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 466 :line 15 :column 34 :mark 399 :active t :modified t :undo t :narrowed nil) :scaladoc-joined (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 )\n      . bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 252 :line 9 :column 53 :mark 393 :active t :modified t :undo t :narrowed nil) :code-join-before (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 )\n      . bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 704 :line 20 :column 0 :mark 393 :active t :modified t :undo t :narrowed nil) :code-joined (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 703 :line 19 :column 30 :mark 393 :active t :modified t :undo t :narrowed nil) :code-join-undo (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 )\n      . bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 704 :line 20 :column 0 :mark 393 :active t :modified t :undo t :narrowed nil) :code-join-rejoined (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n" :point 703 :line 19 :column 30 :mark 393 :active t :modified t :undo t :narrowed nil) :doc-default (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n\n  /** typed default\n    */\n" :point 818 :line 27 :column 6 :mark 393 :active t :modified t :undo t :narrowed nil) :doc-javadoc (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 ,  4 ) . baz\n    val block = {  1 + 2  }\n    nested\n  }\n}\n\n  /** typed default\n    */\n\n  /** typed javadoc\n   */\n" :point 845 :line 30 :column 5 :mark 393 :active t :modified t :undo t :narrowed nil) :fixed (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 , 4 ).baz\n    val block = { 1 + 2 }\n    nested\n  }\n}\n\n  /** typed default\n    */\n\n  /** typed javadoc\n   */\n" :point 766 :line 21 :column 23 :mark 393 :active t :modified t :undo t :narrowed nil) :broken (:before (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 , 4 ).baz\n    val block = { 1 + 2 }\n    nested\n  }\n}\n\n  /** typed default\n    */\n\n  /** typed javadoc\n   */\n  val broken = s\"${1 + 2}\n" :point 867 :line 32 :column 0 :mark 393 :active t :modified t :undo t :narrowed nil) :outcome (:returned 867) :after (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 , 4 ).baz\n    val block = { 1 + 2 }\n    nested\n  }\n}\n\n  /** typed default\n    */\n\n  /** typed javadoc\n   */\n  val broken = s\"${1 + 2}\n" :point 867 :line 32 :column 0 :mark 393 :active t :modified t :undo t :narrowed nil) :no-progress t) :broken-recovery (:returned 857 :progress t :state (:mode scala-mode :text "#!/usr/bin/env scala\npackage demo\nimport demo.Tools\n\nobject Motion {\n  /** A long scaladoc paragraph that should wrap\n    * across practical narrow columns while\n    * retaining its comment prefix.\n    * - first list item remains structurally distinct @param value a separate semantic paragraph\n    */\n  def compute: String = {\n    /* outer comment /* nested comment */ tail */\n    val nested = s\"outer ${s\"inner ${2 + 3}\"}\"\n    val raw = \"\"\"|A long multiline string paragraph that should\n                 |wrap across practical narrow\n                 |columns while staying inside the\n                 |string.  and preserve its\n                 |margin.\"\"\".stripMargin\n    val punct = foo ( 1 ,  2 ). bar\n    val fixes = foo ( 3 , 4 ).baz\n    val block = { 1 + 2 }\n    nested\n  }\n}\n\n  /** typed default\n    */\n\n  /** typed javadoc\n   */\n  val broken = s\"${1 + 2}\"\n" :point 857 :line 31 :column 16 :mark 393 :active t :modified t :undo t :narrowed nil)) :completions ((:prompt "M-x " :input "scala-mode:goto-start-of-code" :require-match t :initial "" :final "scala-mode:goto-start-of-code" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-mode:goto-start-of-code")) (:prompt "M-x " :input "scala-indent:join-line" :require-match t :initial "" :final "scala-indent:join-line" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-indent:join-line" "scala-mode:goto-start-of-code")) (:prompt "M-x " :input "scala-indent:join-line" :require-match t :initial "" :final "scala-indent:join-line" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-indent:join-line" "scala-mode:goto-start-of-code")) (:prompt "M-x " :input "scala-indent:join-line" :require-match t :initial "" :final "scala-indent:join-line" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-indent:join-line" "scala-mode:goto-start-of-code")) (:prompt "M-x " :input "scala-indent:fixup-whitespace" :require-match t :initial "" :final "scala-indent:fixup-whitespace" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-indent:fixup-whitespace" "scala-indent:join-line" "scala-mode:goto-start-of-code")))) :cleanup clean)"##
        ]],
    )
}

fn public_imenu() -> ParityBatchCase {
    ParityBatchCase::value(
        "selects_nested_definitions_through_public_imenu",
        r###"
(scala365-test-run
 "selects_nested_definitions_through_public_imenu"
 :space-unicode
 (lambda (_world)
   (let* ((source
           (concat "package demo\nobject Inventory {\n  val version = 1\n"
                   "  class Ledger {\n    def credit(x: Int) = x\n"
                   "    def debit(x: Int) = -x\n  }\n"
                   "  object Helpers {\n    type Amount = BigDecimal\n"
                   "    def parse(s: String) = s\n  }\n}\n"))
          (buffer (scala365-test-visit "imenu/Inventory.scala" source))
          flat-jump flat-index tree-jump tree-index cleanup-count)
     (with-current-buffer buffer
       (setq-local scala-imenu:cleanup-hooks
                   (list (lambda () (setq cleanup-count (1+ (or cleanup-count 0))))))
       (setq-local scala-imenu:should-flatten-index t)
       (goto-char (point-min))
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "imenu")
                   '(:prompt "Index item: "
                     :input "(def)Inventory.Ledger.debit")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq flat-jump (list :state (scala365-test-buffer-state)
                             :mark-ring (mapcar #'marker-position mark-ring))
             flat-index (scala365-test-stable-imenu-index imenu--index-alist))
       (setq-local scala-imenu:should-flatten-index nil)
       (setq imenu--index-alist nil)
       (goto-char (point-min)) (search-forward "version") (beginning-of-line)
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "imenu")
                   '(:prompt "Index item: "
                     :input "(object)Inventory")
                   '(:prompt "Index item: "
                     :input "(class)Inventory.Ledger")
                   '(:prompt "Index item: "
                     :input "(def)Inventory.Ledger.debit")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq tree-jump (list :state (scala365-test-buffer-state)
                             :mark-ring (mapcar #'marker-position mark-ring))
             tree-index (scala365-test-stable-imenu-index imenu--index-alist)))
     (list :flat-index flat-index :flat-jump flat-jump
           :tree-index tree-index :tree-jump tree-jump
           :cleanup-count cleanup-count
           :reads (nreverse scala365-test-completion-events)))))
"###,
        expect![[
            r#"OK (:result (:flat-index (("(object)Inventory" 14) ("(val)Inventory.version" 35) ("(class)Inventory.Ledger" 53) ("(def)Inventory.Ledger.credit" 72) ("(def)Inventory.Ledger.debit" 99) ("(object)Inventory.Helpers" 128) ("(type)Inventory.Helpers.Amount" 149) ("(def)Inventory.Helpers.parse" 178)) :flat-jump (:state (:mode scala-mode :text "package demo\nobject Inventory {\n  val version = 1\n  class Ledger {\n    def credit(x: Int) = x\n    def debit(x: Int) = -x\n  }\n  object Helpers {\n    type Amount = BigDecimal\n    def parse(s: String) = s\n  }\n}\n" :point 99 :line 6 :column 4 :mark 1 :active t :modified nil :undo nil :narrowed nil) :mark-ring nil) :tree-index (("(object)Inventory" ("(object)Inventory" 14) ("(val)Inventory.version" 35) ("(class)Inventory.Ledger" ("(class)Inventory.Ledger" 53) ("(def)Inventory.Ledger.credit" 72) ("(def)Inventory.Ledger.debit" 99)) ("(object)Inventory.Helpers" ("(object)Inventory.Helpers" 128) ("(type)Inventory.Helpers.Amount" 149) ("(def)Inventory.Helpers.parse" 178)))) :tree-jump (:state (:mode scala-mode :text "package demo\nobject Inventory {\n  val version = 1\n  class Ledger {\n    def credit(x: Int) = x\n    def debit(x: Int) = -x\n  }\n  object Helpers {\n    type Amount = BigDecimal\n    def parse(s: String) = s\n  }\n}\n" :point 99 :line 6 :column 4 :mark 33 :active t :modified nil :undo nil :narrowed nil) :mark-ring (1)) :cleanup-count 2 :reads ((:prompt "M-x " :input "imenu" :require-match t :initial "" :final "imenu" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("imenu")) (:prompt "Index item: " :input "(def)Inventory.Ledger.debit" :require-match t :initial "" :final "(def)Inventory.Ledger.debit" :candidates ("*Rescan*" "(object)Inventory" "(val)Inventory.version" "(class)Inventory.Ledger" "(def)Inventory.Ledger.credit" "(def)Inventory.Ledger.debit" "(object)Inventory.Helpers" "(type)Inventory.Helpers.Amount" "(def)Inventory.Helpers.parse") :history-argument imenu--history-list :history-after ("(def)Inventory.Ledger.debit")) (:prompt "M-x " :input "imenu" :require-match t :initial "" :final "imenu" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("imenu")) (:prompt "Index item: " :input "(object)Inventory" :require-match t :initial "" :final "(object)Inventory" :candidates ("*Rescan*" "(object)Inventory") :history-argument imenu--history-list :history-after ("(object)Inventory" "(def)Inventory.Ledger.debit")) (:prompt "Index item: " :input "(class)Inventory.Ledger" :require-match t :initial "" :final "(class)Inventory.Ledger" :candidates ("(object)Inventory" "(val)Inventory.version" "(class)Inventory.Ledger" "(object)Inventory.Helpers") :history-argument imenu--history-list :history-after ("(class)Inventory.Ledger" "(object)Inventory" "(def)Inventory.Ledger.debit")) (:prompt "Index item: " :input "(def)Inventory.Ledger.debit" :require-match t :initial "" :final "(def)Inventory.Ledger.debit" :candidates ("(class)Inventory.Ledger" "(def)Inventory.Ledger.credit" "(def)Inventory.Ledger.debit") :history-argument imenu--history-list :history-after ("(def)Inventory.Ledger.debit" "(class)Inventory.Ledger" "(object)Inventory" "(def)Inventory.Ledger.debit")))) :cleanup clean)"#
        ]],
    )
}

fn public_import_organisation() -> ParityBatchCase {
    ParityBatchCase::value(
        "organises_imports_warns_and_recovers_through_public_commands",
        r###"
(scala365-test-run
 "organises_imports_warns_and_recovers_through_public_commands"
 :space-unicode
 (lambda (_world)
   (let* ((source
           (concat "package demo\n\nimport demo.z.Zed\nimport java.util.List\n"
                   "import scala.collection.mutable.Map\n"
                   "import java.util.{ Set, List, Map => JMap }\n"
                   "import demo.z._\nimport demo.z.{ Other, Named=>Renamed }\n"
                   "import javax.time.Clock\nimport scala.collection.mutable.Map\n"
                   "import demo.界.Name\n\n"
                   "object Main {\n  def run = {\n    import local.Hidden\n    ()\n  }\n}\n"))
          (buffer (scala365-test-visit "imports/Imports.scala" source))
          once once-text undone twice twice-text read-only recovery messages-start)
     (with-current-buffer buffer
       (setq-local scala-organise-first
                   '(("java." "javax.") "scala." "demo."))
       (setq messages-start (length scala365-test-message-events)
             scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-organise")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq once (scala365-test-buffer-state)
             once-text (plist-get once :text))
       (scala365-test-run-keys '(:keys "C-_"))
       (setq undone (scala365-test-buffer-state))
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-organise")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-organise")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq twice (scala365-test-buffer-state)
             twice-text (plist-get twice :text))
       (let ((inhibit-read-only t))
         (erase-buffer) (insert "import z.B\nimport a.A\n"))
       (goto-char 23)
       (set-buffer-modified-p nil)
       (setq buffer-undo-list nil buffer-read-only t)
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-organise")))
       (setq read-only
             (condition-case condition
                 (progn (scala365-test-run-keys '(:keys "M-x")) :returned)
               (t (list :condition (scala365-test-condition-state condition)
                        :state (scala365-test-buffer-state)))))
       (setq buffer-read-only nil
             scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-organise")))
       (scala365-test-run-keys '(:keys "M-x"))
       (setq recovery (scala365-test-buffer-state)))
     (list :once once :undone undone :twice twice
           :idempotent (equal once-text twice-text)
           :messages (scala365-test-message-delta messages-start)
           :read-only read-only :recovery recovery
           :commands (nreverse scala365-test-completion-events)))))
"###,
        expect![[
            r#"OK (:result (:once (:mode scala-mode :text "package demo\n\nimport java.util.{ List, Map => JMap, Set }\nimport javax.time.Clock\n\nimport scala.collection.mutable.Map\n\nimport demo.z.{ _, Named => Renamed }\nimport demo.界.Name\n\nobject Main {\n  def run = {\n    import local.Hidden\n    ()\n  }\n}\n" :point 1 :line 1 :column 0 :mark nil :active nil :modified t :undo t :narrowed nil) :undone (:mode scala-mode :text "package demo\n\nimport demo.z.Zed\nimport java.util.List\nimport scala.collection.mutable.Map\nimport java.util.{ Set, List, Map => JMap }\nimport demo.z._\nimport demo.z.{ Other, Named=>Renamed }\nimport javax.time.Clock\nimport scala.collection.mutable.Map\nimport demo.界.Name\n\nobject Main {\n  def run = {\n    import local.Hidden\n    ()\n  }\n}\n" :point 271 :line 13 :column 0 :mark nil :active nil :modified nil :undo t :narrowed nil) :twice (:mode scala-mode :text "package demo\n\nimport java.util.{ List, Map => JMap, Set }\nimport javax.time.Clock\n\nimport scala.collection.mutable.Map\n\nimport demo.z.{ _, Named => Renamed }\nimport demo.界.Name\n\nobject Main {\n  def run = {\n    import local.Hidden\n    ()\n  }\n}\n" :point 15 :line 3 :column 0 :mark nil :active nil :modified t :undo t :narrowed nil) :idempotent t :messages ("Inline imports, starting at line 13, have not been organised." "Undo" "Inline imports, starting at line 13, have not been organised." "Inline imports, starting at line 13, have not been organised.") :read-only (:condition (:symbol buffer-read-only :data ((:buffer "Imports.scala")) :message "Buffer is read-only: [BUFFER:Imports.scala]") :state (:mode scala-mode :text "import z.B\nimport a.A\n" :point 23 :line 3 :column 0 :mark nil :active nil :modified nil :undo nil :narrowed nil)) :recovery (:mode scala-mode :text "import a.A\nimport z.B\n\n" :point 1 :line 1 :column 0 :mark nil :active nil :modified t :undo t :narrowed nil) :commands ((:prompt "M-x " :input "scala-organise" :require-match t :initial "" :final "scala-organise" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-organise")) (:prompt "M-x " :input "scala-organise" :require-match t :initial "" :final "scala-organise" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-organise")) (:prompt "M-x " :input "scala-organise" :require-match t :initial "" :final "scala-organise" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-organise")) (:prompt "M-x " :input "scala-organise" :require-match t :initial "" :final "scala-organise" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-organise")) (:prompt "M-x " :input "scala-organise" :require-match t :initial "" :final "scala-organise" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-organise")))) :cleanup clean)"#
        ]],
    )
}

fn public_compilation() -> ParityBatchCase {
    ParityBatchCase::value(
        "compiles_navigates_and_recovers_with_recorded_real_sbt",
        r###"
(scala365-test-run
 "compiles_navigates_and_recovers_with_recorded_real_sbt"
 :compile-no-space-unicode
 (lambda (world)
   (scala365-test-write "build.sbt" scala365-test-build-sbt)
   (scala365-test-write "project/build.properties"
                        scala365-test-build-properties)
   (scala365-test-write "src/main/scala/Warnings.scala"
                        scala365-test-warnings-source)
   (let* ((inventory
           (scala365-test-visit "src/main/scala/Inventory.scala"
                                scala365-test-failure-inventory))
          failure-buffer failure-wait failure-before failure-selected failure-after
          failure-materialized failure-materialized-state failure-end
          success-buffer success-wait success-before success-selected success-after
          success-materialized success-materialized-state success-end
          missing-buffer missing-wait missing-output recovery-wait recovery-state
          recovery-output)
     (with-current-buffer inventory
       (setq-local scala-compile-suggestion "sbt --batch compile")
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-compile"))
             scala365-test-expected-reads
             (list '(:prompt "Compile command: "
                     :answer "sbt --batch compile"
                     :events ((:keys "RET"))))
             scala365-test-compile-phase :failure
             scala365-test-body-stage :failure-start)
       (scala365-test-run-keys '(:keys "M-x"))
       (setq failure-buffer compilation-last-buffer))
     (setq scala365-test-body-stage :failure-wait)
     (setq failure-wait
           (scala365-test-wait-process (get-buffer-process failure-buffer))
           failure-before (scala365-test-compilation-state failure-buffer)
           failure-selected
           (scala365-test-selected-window-point-state inventory))
     (setq scala365-test-body-stage :failure-navigation)
     (set-window-buffer (selected-window) failure-buffer)
     (with-current-buffer failure-buffer (goto-char (point-min)))
     (setq scala365-test-watch-commands '(next-error))
     (scala365-test-run-keys '(:keys "M-g n"))
     (setq failure-after
           (with-selected-window (selected-window)
             (scala365-test-buffer-state (window-buffer))))
     (setq failure-materialized-state
           (scala365-test-compilation-state failure-buffer t)
           failure-materialized
           (list
            :text-unchanged
            (equal (plist-get failure-materialized-state :text)
                   (plist-get failure-before :text))
            :messages (plist-get failure-materialized-state :messages)
            :overlays-unchanged
            (equal (plist-get failure-materialized-state :overlays)
                   (plist-get failure-before :overlays))))
     (setq failure-end
           (condition-case condition
               (progn (scala365-test-run-keys '(:keys "M-g n")) :returned)
             (t (scala365-test-condition-state condition))))
     (with-current-buffer inventory
       (setq scala365-test-body-stage :public-edit-and-cached-success)
       (set-window-buffer (selected-window) inventory)
       (goto-char (point-min))
       (scala365-test-run-keys '(:keys "C-x h C-w")
                               (list :text scala365-test-success-inventory)
                               '(:keys "C-x C-s"))
       (setq scala365-test-expected-completions
             (list '(:prompt "M-x " :input "scala-compile"))
             scala365-test-expected-reads nil
             scala365-test-compile-phase :success)
       (scala365-test-run-keys '(:keys "M-x"))
       (setq success-buffer compilation-last-buffer))
     (setq scala365-test-body-stage :success-wait)
     (setq success-wait
           (scala365-test-wait-process (get-buffer-process success-buffer))
           success-before (scala365-test-compilation-state success-buffer)
           success-selected
           (scala365-test-selected-window-point-state inventory))
     (set-window-buffer (selected-window) success-buffer)
     (with-current-buffer success-buffer (goto-char (point-min)))
     (scala365-test-run-keys '(:keys "M-g n"))
     (setq success-after
           (with-selected-window (selected-window)
             (scala365-test-buffer-state (window-buffer))))
     (setq success-materialized-state
           (scala365-test-compilation-state success-buffer t)
           success-materialized
           (list
            :text-unchanged
            (equal (plist-get success-materialized-state :text)
                   (plist-get success-before :text))
            :messages (plist-get success-materialized-state :messages)
            :overlays-unchanged
            (equal (plist-get success-materialized-state :overlays)
                   (plist-get success-before :overlays))))
     (setq success-end
           (condition-case condition
               (progn (scala365-test-run-keys '(:keys "M-g n")) :returned)
             (t (scala365-test-condition-state condition))))
     (with-current-buffer inventory
       (setq scala365-test-body-stage :missing-command)
       (set-window-buffer (selected-window) inventory)
       (setq scala365-test-expected-completions
             (list '(:prompt "C-u M-x " :input "scala-compile"))
             scala365-test-expected-reads
             (list '(:prompt "Compile command: "
                     :answer "missing-sbt365 --batch compile"
                     :events ((:keys "C-a C-k")
                              (:text "missing-sbt365 --batch compile")
                              (:keys "RET"))))
             scala365-test-compile-phase :missing)
       (scala365-test-run-keys '(:keys "C-u M-x"))
       (setq missing-buffer compilation-last-buffer))
     (setq missing-wait
           (scala365-test-wait-process (get-buffer-process missing-buffer))
           missing-output (scala365-test-compilation-state missing-buffer))
     (with-current-buffer inventory
       (setq scala365-test-body-stage :missing-recovery)
       (set-window-buffer (selected-window) inventory)
       (setq scala365-test-expected-completions
             (list '(:prompt "C-u M-x " :input "scala-compile"))
             scala365-test-expected-reads
             (list '(:prompt "Compile command: "
                     :answer "sbt --batch compile"
                     :events ((:keys "C-a C-k")
                              (:text "sbt --batch compile")
                              (:keys "RET"))))
             scala365-test-compile-phase :success)
       (scala365-test-run-keys '(:keys "C-u M-x"))
       (setq recovery-state compilation-last-buffer))
     (setq recovery-wait
           (scala365-test-wait-process (get-buffer-process recovery-state))
           recovery-output (scala365-test-compilation-state recovery-state))
     (list
      :tool scala365-test-sbt-tool
      :recordings (list :failure scala365-test-failure-stream-sha256
                        :success scala365-test-success-stream-sha256
                        :empty-stderr scala365-test-empty-stream-sha256)
      :replay-rejections (plist-get world :replay-rejections)
      :failure (list :wait failure-wait
                     :selected-before-navigation failure-selected
                     :before failure-before
                     :destination failure-after
                     :materialized failure-materialized
                     :terminal failure-end)
      :success (list :wait success-wait
                     :selected-before-navigation success-selected
                     :before success-before
                     :destination success-after
                     :materialized success-materialized
                     :terminal success-end)
      :missing (list :wait missing-wait
                     :state missing-output)
      :recovery (list :wait recovery-wait
                      :semantic-equals-success
                      (and (equal (plist-get recovery-output :mode)
                                  (plist-get success-before :mode))
                           (equal (plist-get recovery-output :process)
                                  (plist-get success-before :process))
                           (equal (plist-get recovery-output :text)
                                  (plist-get success-before :text))
                           (equal (plist-get recovery-output :overlays)
                                  (plist-get success-before :overlays))))
      :source (scala365-test-buffer-state inventory)
      :compile-state
      (with-current-buffer inventory
        (list :cached scala--compile-command
              :suggestion scala-compile-suggestion
              :history (copy-tree scala--compile-history)))
      :navigation (nreverse scala365-test-command-events)
      :reads (nreverse scala365-test-read-events)
      :completions (nreverse scala365-test-completion-events)
      :processes
      (mapcar (lambda (entry)
                (list :phase (plist-get entry :phase)
                      :program (plist-get entry :program)
                      :argv (plist-get entry :argv)
                      :cwd (plist-get entry :cwd)))
              (nreverse scala365-test-process-records))
      :invocations (scala365-test-invocation-state)
      :replay (list :path (equal (plist-get world :replay)
                                 (scala365-test-path "bin/sbt"))
                    :digest-match
                    (equal (scala365-test-file-sha256
                            (plist-get world :replay))
                           (plist-get world :replay-sha256)))))))
"###,
        expect![[
            r#"OK (:result (:tool (:version "1.12.14" :java "21.0.10" :scala "2.13.16" :archive-sha256 "cd17daae220ff264faa4251334522444518584f0eb2ee82da01523a9b9002b7e" :script-sha256 "0479b7d305132e216bdd0c8aa376f916dc062c4cae010f21625c033b08435715" :launcher-sha256 "1750c8fb61c2d2f82da40b2cd9014f4d8a1bd49a361402ea5b1cec061ed66578" :sbtn-version "2.0.0-b4d628dd" :sbtn-sha256 "4527047664bce3f473f3bc960be888199947c74346a1ea9f717809e8dcefbcc6") :recordings (:failure "ea0b785b102c1ae2348042b4b7815bcb6594e0601354b7ed2fc0f9a90fe0f005" :success "cf44689cc6c5a78f3ba28bc053a8a3002e7d9062984182095a36d0264a70959b" :empty-stderr "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") :replay-rejections ((:argv 86 "") (:cwd 86 "") (:env 86 "") (:manifest 86 "")) :failure (:wait (:status exit :exit 1 :detached t :stable-polls 2) :selected-before-navigation (:buffer "Inventory.scala" :owned-source t :point 1 :line 1 :column 0) :before (:mode scala-compilation-mode :process nil :text "-*- mode: scala-compilation; default-directory: \"[ROOT]/\" -*-\nscala-compilation started at [TIME]\n\nsbt --batch compile\n[info] welcome to sbt 1.12.14 (N/A Java 21.0.10)\n[info] loading project definition from [ROOT]/project\n[info] loading settings for project project-space- from build.sbt...\n[info] set current project to project-space- (in build file:[ROOT]/)\n[info] Executing in batch mode. For better performance use sbt's shell\n[info] compiling 2 Scala sources to [ROOT]/target/scala-2.13/classes ...\n[error] [ROOT]/src/main/scala/Inventory.scala:3:16: not found: value missing\n[error]   val broken = missing\n[error]                ^\n[error] one error found\n[error] (Compile / compileIncremental) Compilation failed\n[error] Total time: 8 s, completed Aug 11, 2026, 4:21:08 PM\n\nscala-compilation exited abnormally with code 1 at [TIME]\n" :overlays ((:start (11 1) :end (11 6) :text "error" :face (:foreground "red3")) (:start (12 1) :end (12 6) :text "error" :face (:foreground "red3")) (:start (13 1) :end (13 6) :text "error" :face (:foreground "red3")) (:start (14 1) :end (14 6) :text "error" :face (:foreground "red3")) (:start (15 1) :end (15 6) :text "error" :face (:foreground "red3")) (:start (15 19) :end (15 37) :text "compileIncremental" :face (:foreground "red3")) (:start (16 1) :end (16 6) :text "error" :face (:foreground "red3")))) :destination (:mode scala-mode :text "object Inventory {\n  val okay = 1\n  val broken = missing\n}\n\n" :point 50 :line 3 :column 15 :mark nil :active nil :modified nil :undo nil :narrowed nil) :materialized (:text-unchanged t :messages ((:start (11 8) :end (11 45) :text "[ROOT]/src/main/scala/Inventory.scala" :type 2 :rule nil :line 3 :column 16 :file ("[ROOT]/src/main/scala/Inventory.scala" nil))) :overlays-unchanged t) :terminal (:symbol user-error :data ("Past last error") :message "Past last error")) :success (:wait (:status exit :exit 0 :detached t :stable-polls 2) :selected-before-navigation (:buffer "Inventory.scala" :owned-source t :point 64 :line 5 :column 0) :before (:mode scala-compilation-mode :process nil :text "-*- mode: scala-compilation; default-directory: \"[ROOT]/\" -*-\nscala-compilation started at [TIME]\n\nsbt --batch compile\n[info] welcome to sbt 1.12.14 (N/A Java 21.0.10)\n[info] loading project definition from [ROOT]/project\n[info] loading settings for project project-space- from build.sbt...\n[info] set current project to project-space- (in build file:[ROOT]/)\n[info] Executing in batch mode. For better performance use sbt's shell\n[info] compiling 2 Scala sources to [ROOT]/target/scala-2.13/classes ...\n[warn] [ROOT]/src/main/scala/Warnings.scala:4:17: method legacy in object Warnings is deprecated (since 1): use current\n[warn]   val warning = legacy\n[warn]                 ^\n[warn] one warning found\n[info] done compiling\n[success] Total time: 3 s, completed Aug 11, 2026, 4:21:46 PM\n\nscala-compilation finished at [TIME]\n" :overlays ((:start (11 1) :end (11 5) :text "warn" :face (:foreground "yellow3")) (:start (12 1) :end (12 5) :text "warn" :face (:foreground "yellow3")) (:start (13 1) :end (13 5) :text "warn" :face (:foreground "yellow3")) (:start (14 1) :end (14 5) :text "warn" :face (:foreground "yellow3")) (:start (16 1) :end (16 8) :text "success" :face (:foreground "green3")))) :destination (:mode scala-mode :text "object Warnings {\n  @deprecated(\"use current\", \"1\") def legacy = 1\n  val okay = 2\n  val warning = legacy\n}\n\n" :point 85 :line 4 :column 2 :mark nil :active nil :modified nil :undo nil :narrowed nil) :materialized (:text-unchanged t :messages ((:start (11 7) :end (11 43) :text "[ROOT]/src/main/scala/Warnings.scala" :type 1 :rule nil :line 4 :column nil :file ("[ROOT]/src/main/scala/Warnings.scala" nil))) :overlays-unchanged t) :terminal (:symbol user-error :data ("Past last error") :message "Past last error")) :missing (:wait (:status exit :exit 127 :detached t :stable-polls 2) :state (:mode scala-compilation-mode :process nil :text "-*- mode: scala-compilation; default-directory: \"[ROOT]/\" -*-\nscala-compilation started at [TIME]\n\nmissing-sbt365 --batch compile\n[SHELL]: line 1: missing-sbt365: command not found\n\nscala-compilation exited abnormally with code 127 at [TIME]\n" :overlays nil)) :recovery (:wait (:status exit :exit 0 :detached t :stable-polls 2) :semantic-equals-success t) :source (:mode scala-mode :text "object Inventory {\n  val okay = 1\n  val recovered = okay + 1\n}\n" :point 64 :line 5 :column 0 :mark 1 :active t :modified nil :undo t :narrowed nil) :compile-state (:cached "sbt --batch compile" :suggestion "sbt --batch compile" :history ("sbt --batch compile" "missing-sbt365 --batch compile" "sbt --batch compile" "sbtn compile" "sbtn test" "sbtn testOnly ")) :navigation ((:command next-error :file "src/main/scala/Inventory.scala" :point 50 :line 3 :column 15) (:command next-error :file "src/main/scala/Inventory.scala" :point 1 :line 1 :column 0) (:command next-error :file "src/main/scala/Warnings.scala" :point 85 :line 4 :column 2) (:command next-error :file "src/main/scala/Inventory.scala" :point 64 :line 5 :column 0) (:command next-error :file "src/main/scala/Inventory.scala" :point 64 :line 5 :column 0)) :reads ((:prompt "Compile command: " :arguments ("sbt --batch compile" (scala--compile-history . 1)) :answer "sbt --batch compile" :initial "sbt --batch compile" :final "sbt --batch compile" :history-argument (scala--compile-history . 1) :history-after ("sbt --batch compile" "sbtn compile" "sbtn test" "sbtn testOnly ")) (:prompt "Compile command: " :arguments ("sbt --batch compile" (scala--compile-history . 1)) :answer "missing-sbt365 --batch compile" :initial "sbt --batch compile" :final "missing-sbt365 --batch compile" :history-argument (scala--compile-history . 1) :history-after ("missing-sbt365 --batch compile" "sbt --batch compile" "sbtn compile" "sbtn test" "sbtn testOnly ")) (:prompt "Compile command: " :arguments ("missing-sbt365 --batch compile" (scala--compile-history . 1)) :answer "sbt --batch compile" :initial "missing-sbt365 --batch compile" :final "sbt --batch compile" :history-argument (scala--compile-history . 1) :history-after ("sbt --batch compile" "missing-sbt365 --batch compile" "sbt --batch compile" "sbtn compile" "sbtn test" "sbtn testOnly "))) :completions ((:prompt "M-x " :input "scala-compile" :require-match t :initial "" :final "scala-compile" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-compile")) (:prompt "M-x " :input "scala-compile" :require-match t :initial "" :final "scala-compile" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-compile")) (:prompt "C-u M-x " :input "scala-compile" :require-match t :initial "" :final "scala-compile" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-compile")) (:prompt "C-u M-x " :input "scala-compile" :require-match t :initial "" :final "scala-compile" :candidates (:selected-present t) :history-argument extended-command-history :history-after ("scala-compile"))) :processes ((:phase :failure :program :shell :argv ("-c" "sbt --batch compile") :cwd "[ROOT]/") (:phase :success :program :shell :argv ("-c" "sbt --batch compile") :cwd "[ROOT]/") (:phase :missing :program :shell :argv ("-c" "missing-sbt365 --batch compile") :cwd "[ROOT]/") (:phase :success :program :shell :argv ("-c" "sbt --batch compile") :cwd "[ROOT]/")) :invocations (:fields ("[ROOT]" "--batch" "compile" "[ROOT]" "--batch" "compile" "[ROOT]" "--batch" "compile") :misses "") :replay (:path t :digest-match t)) :cleanup clean)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        activation_and_rendering(),
        electric_command_loop(),
        indentation_customization(),
        motion_fill_join_fixup(),
        public_imenu(),
        public_import_organisation(),
        public_compilation(),
    ]
}
