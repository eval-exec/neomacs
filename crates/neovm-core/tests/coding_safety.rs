//! Coding selection preserves the repertoire of decoded Emacs characters.
use neovm_core::emacs_core::load::create_bootstrap_evaluator_cached;

#[test]
fn shift_jis_internal_character_is_safe_without_becoming_unicode() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval.eval_str(r#"(let* ((text (decode-coding-string (unibyte-string #x87 #x40) 'japanese-shift-jis))
                                      (safe (find-coding-systems-region text nil)))
                               (list (= (aref text 0) #x140468)
                                     (null (unencodable-char-position 0 1 'japanese-shift-jis nil text))
                                     (and (memq 'japanese-shift-jis safe) t)
                                     (and (memq 'utf-8 safe) t)
                                     (and (memq 'utf-8-emacs safe) t)))"#).unwrap();
    assert_eq!(format!("{result}"), "(t t t nil t)");
}

#[test]
fn safe_coding_lists_distinguish_raw_bytes_from_private_use_and_replacement_characters() {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str(
            r#"(let (results)
           (dolist (code '(#x3fff80 #xe080 #xfffd))
             (let* ((text (string code)) (safe (find-coding-systems-region text nil)))
               (push (list (and (memq 'utf-8 safe) t)
                           (and (memq 'utf-8-emacs safe) t)
                           (unencodable-char-position 0 1 'utf-8 nil text)
                           (unencodable-char-position 0 1 'utf-8-emacs nil text)) results)))
           (nreverse results))"#,
        )
        .unwrap();
    assert_eq!(
        format!("{result}"),
        "((nil nil 0 0) (t t nil nil) (t t nil nil))"
    );
}
