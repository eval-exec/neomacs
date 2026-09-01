use expect_test::expect;

use super::ParityBatchCase;

fn all_language_keywords_receive_keyword_face_in_real_source_context() -> ParityBatchCase {
    ParityBatchCase::value(
        "all_language_keywords_receive_keyword_face_in_real_source_context",
        r##"
(with-temp-buffer
  (aiken-mode)
  (insert (mapconcat
           (lambda (keyword)
             (format "%s value\n" keyword))
           aiken-keywords ""))
  (font-lock-ensure)
  (goto-char (point-min))
  (mapcar
   (lambda (keyword)
     (search-forward keyword)
     (prog1
         (list keyword
               (get-text-property
                (- (point) (length keyword))
                'face))
       (forward-line 1)))
   aiken-keywords))
"##,
        expect![[
            r#"OK (("if" font-lock-keyword-face) ("else" font-lock-keyword-face) ("when" font-lock-keyword-face) ("is" font-lock-keyword-face) ("fn" font-lock-keyword-face) ("use" font-lock-keyword-face) ("let" font-lock-keyword-face) ("pub" font-lock-keyword-face) ("type" font-lock-keyword-face) ("opaque" font-lock-keyword-face) ("const" font-lock-keyword-face) ("todo" font-lock-keyword-face) ("error" font-lock-keyword-face) ("expect" font-lock-keyword-face) ("test" font-lock-keyword-face) ("trace" font-lock-keyword-face) ("fail" font-lock-keyword-face) ("validator" font-lock-keyword-face) ("and" font-lock-keyword-face) ("or" font-lock-keyword-face))"#
        ]],
    )
}

fn every_operator_uses_longest_token_matching_and_builtin_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_operator_uses_longest_token_matching_and_builtin_face",
        r##"
(with-temp-buffer
  (aiken-mode)
  (dolist (operator aiken-operators)
    (insert "left " operator " right\n"))
  (font-lock-ensure)
  (goto-char (point-min))
  (mapcar
   (lambda (operator)
     (search-forward (concat "left " operator))
     (let ((start (- (point) (length operator))))
       (prog1
           (list
            operator
            (buffer-substring-no-properties start (point))
            (get-text-property start 'face)
            (get-text-property (1- (point)) 'face))
         (forward-line 1))))
   aiken-operators))
"##,
        expect![[
            r#"OK (("=" "=" font-lock-builtin-face font-lock-builtin-face) ("->" "->" font-lock-builtin-face font-lock-builtin-face) (".." ".." font-lock-builtin-face font-lock-builtin-face) ("|>" "|>" font-lock-builtin-face font-lock-builtin-face) (">=" ">=" font-lock-builtin-face font-lock-builtin-face) ("<=" "<=" font-lock-builtin-face font-lock-builtin-face) (">" ">" font-lock-builtin-face font-lock-builtin-face) ("<" "<" font-lock-builtin-face font-lock-builtin-face) ("!=" "!=" font-lock-builtin-face font-lock-builtin-face) ("==" "==" font-lock-builtin-face font-lock-builtin-face) ("&&" "&&" font-lock-builtin-face font-lock-builtin-face) ("||" "||" font-lock-builtin-face font-lock-builtin-face) ("!" "!" font-lock-builtin-face font-lock-builtin-face) ("+" "+" font-lock-builtin-face font-lock-builtin-face) ("-" "-" font-lock-builtin-face font-lock-builtin-face) ("/" "/" font-lock-builtin-face font-lock-builtin-face) ("*" "*" font-lock-builtin-face font-lock-builtin-face) ("%" "%" font-lock-builtin-face font-lock-builtin-face) ("?" "?" font-lock-builtin-face font-lock-builtin-face))"#
        ]],
    )
}

fn declarations_highlight_names_types_constants_and_functions_differently() -> ParityBatchCase {
    ParityBatchCase::value(
        "declarations_highlight_names_types_constants_and_functions_differently",
        r##"
(with-temp-buffer
  (aiken-mode)
  (insert
   "pub const max_supply: Int = 42\n\
pub type Payment { Payment { amount: Int } }\n\
use cardano/transaction.{Transaction, Input}\n\
fn settle_payment(tx: Transaction) -> Bool { True }\n")
  (font-lock-ensure)
  (mapcar
   (lambda (token)
     (goto-char (point-min))
     (search-forward token)
     (list token
           (get-text-property
            (- (point) (length token))
            'face)))
   '("pub" "const" "max_supply" "Int" "type" "Payment"
     "use" "cardano" "Transaction" "fn" "settle_payment"
     "Bool" "True")))
"##,
        expect![[
            r#"OK (("pub" font-lock-keyword-face) ("const" font-lock-keyword-face) ("max_supply" font-lock-type-face) ("Int" font-lock-type-face) ("type" font-lock-keyword-face) ("Payment" font-lock-type-face) ("use" font-lock-keyword-face) ("cardano" font-lock-constant-face) ("Transaction" nil) ("fn" font-lock-keyword-face) ("settle_payment" font-lock-function-name-face) ("Bool" font-lock-type-face) ("True" font-lock-type-face))"#
        ]],
    )
}

fn keywords_and_types_inside_comments_and_strings_are_not_code_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "keywords_and_types_inside_comments_and_strings_are_not_code_faces",
        r##"
(with-temp-buffer
  (aiken-mode)
  (insert
   "// validator fn HiddenType let\n\
let message = \"validator fn StringType\"\n\
validator spend(datum: Option<Data>) { True }\n")
  (font-lock-ensure)
  (goto-char (point-min))
  (mapcar
   (lambda (token)
     (search-forward token)
     (let ((start (match-beginning 0)))
       (list
        token
        (nth 4 (syntax-ppss start))
        (nth 3 (syntax-ppss start))
        (get-text-property start 'face))))
   '("validator" "HiddenType" "validator"
     "StringType" "validator" "Option")))
"##,
        expect![[
            r#"OK (("validator" t nil font-lock-comment-face) ("HiddenType" t nil font-lock-comment-face) ("validator" nil 34 font-lock-string-face) ("StringType" nil 34 font-lock-string-face) ("validator" nil nil font-lock-keyword-face) ("Option" nil nil font-lock-type-face))"#
        ]],
    )
}

fn incremental_editing_refontifies_new_keywords_types_and_operators() -> ParityBatchCase {
    ParityBatchCase::value(
        "incremental_editing_refontifies_new_keywords_types_and_operators",
        r##"
(with-temp-buffer
  (aiken-mode)
  (insert "lett result = old_value => NewType\n")
  (font-lock-ensure)
  (let ((before
         (mapcar
          (lambda (token)
            (goto-char (point-min))
            (search-forward token)
            (get-text-property
             (- (point) (length token)) 'face))
          '("lett" "NewType" "=" ">"))))
    (goto-char (point-min))
    (delete-region (point-min) (+ (point-min) 4))
    (insert "let")
    (goto-char (point-max))
    (insert " |> next")
    (font-lock-flush)
    (font-lock-ensure)
    (list
     before
     (mapcar
      (lambda (token)
        (goto-char (point-min))
        (search-forward token)
        (get-text-property
         (- (point) (length token)) 'face))
      '("let" "NewType" "|>" "next"))
     (buffer-string))))
"##,
        expect![[
            r#"OK ((nil font-lock-type-face font-lock-builtin-face font-lock-builtin-face) (font-lock-keyword-face font-lock-type-face font-lock-builtin-face nil) #("let result = old_value => NewType\n |> next" 0 3 (face font-lock-keyword-face) 11 12 (face font-lock-builtin-face) 23 24 (face font-lock-builtin-face) 24 25 (face font-lock-builtin-face) 26 33 (face font-lock-type-face) 35 37 (face font-lock-builtin-face)))"#
        ]],
    )
}

fn practical_validator_source_has_expected_semantic_face_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "practical_validator_source_has_expected_semantic_face_map",
        r##"
(with-temp-buffer
  (aiken-mode)
  (insert
   "use aiken/collection/list\n\n\
pub type Datum {\n\
  Payment { owner: ByteArray, amount: Int }\n\
}\n\n\
validator payment {\n\
  spend(datum: Option<Datum>, redeemer: Data, _own_ref: OutputReference, tx: Transaction) {\n\
    when datum is {\n\
      Some(Payment { owner, amount }) -> amount >= 0 && owner != #\"\"\n\
      None -> fail\n\
    }\n\
  }\n\
}\n")
  (font-lock-ensure)
  (let ((tokens
         '("use" "list" "pub" "type" "Datum" "Payment"
           "ByteArray" "Int" "validator" "Option" "Data"
           "OutputReference" "Transaction" "when" "is" "Some"
           "->" ">=" "&&" "!=" "None" "fail")))
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (let ((start
              (if (string-match-p "\\`[[:word:]_]+\\'" token)
                  (progn
                    (re-search-forward
                     (concat
                      "\\(?:^\\|[^[:alnum:]_]\\)\\("
                      (regexp-quote token)
                      "\\)\\(?:$\\|[^[:alnum:]_]\\)"))
                    (match-beginning 1))
                (search-forward token)
                (- (point) (length token)))))
         (list token
               (get-text-property start 'face))))
     tokens)))
"##,
        expect![[
            r#"OK (("use" font-lock-keyword-face) ("list" nil) ("pub" font-lock-keyword-face) ("type" font-lock-keyword-face) ("Datum" font-lock-type-face) ("Payment" font-lock-type-face) ("ByteArray" font-lock-type-face) ("Int" font-lock-type-face) ("validator" font-lock-keyword-face) ("Option" font-lock-type-face) ("Data" font-lock-type-face) ("OutputReference" font-lock-type-face) ("Transaction" font-lock-type-face) ("when" font-lock-keyword-face) ("is" font-lock-keyword-face) ("Some" font-lock-type-face) ("->" font-lock-builtin-face) (">=" font-lock-builtin-face) ("&&" font-lock-builtin-face) ("!=" font-lock-builtin-face) ("None" font-lock-type-face) ("fail" font-lock-keyword-face))"#
        ]],
    )
}

pub(super) fn font_lock_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        all_language_keywords_receive_keyword_face_in_real_source_context(),
        every_operator_uses_longest_token_matching_and_builtin_face(),
        declarations_highlight_names_types_constants_and_functions_differently(),
        keywords_and_types_inside_comments_and_strings_are_not_code_faces(),
        incremental_editing_refontifies_new_keywords_types_and_operators(),
        practical_validator_source_has_expected_semantic_face_map(),
    ]
}
