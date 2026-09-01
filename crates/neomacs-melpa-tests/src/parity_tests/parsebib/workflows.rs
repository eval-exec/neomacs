use expect_test::expect;

use super::ParityBatchCase;

/// Drive `parsebib-read-entry' (the BibTeX parser) on an @article entry and
/// assert it extracts the type, key, and fields.
fn article_entry_extracts_type_key_and_fields() -> ParityBatchCase {
    ParityBatchCase::value(
        "article_entry_extracts_type_key_and_fields",
        r####"
(with-temp-buffer
  (insert "@article{foo2024,
  author = {Doe, Jane},
  title  = {A Title},
  year   = {2024},
}")
  (goto-char (point-min))
  (let ((e (parsebib-read-entry)))
    (list :type (cdr (assoc "=type=" e))
          :key (cdr (assoc "=key=" e))
          :author (cdr (assoc "author" e))
          :year (cdr (assoc "year" e)))))
"####,
        expect![[r#"OK (:type "article" :key "foo2024" :author "{Doe, Jane}" :year "{2024}")"#]],
    )
}

/// A different entry type (@book): assert the parser picks up its type/key/author.
fn book_entry_extracts_type_key_and_author() -> ParityBatchCase {
    ParityBatchCase::value(
        "book_entry_extracts_type_key_and_author",
        r####"
(with-temp-buffer
  (insert "@book{knuth1968,
  author = {Knuth, Donald},
  title  = {The Art of Computer Programming},
}")
  (goto-char (point-min))
  (let ((e (parsebib-read-entry)))
    (list :type (cdr (assoc "=type=" e))
          :key (cdr (assoc "=key=" e))
          :author (cdr (assoc "author" e)))))
"####,
        expect![[r#"OK (:type "book" :key "knuth1968" :author "{Knuth, Donald}")"#]],
    )
}

/// On non-entry text, `parsebib-read-entry' strictly signals `parsebib-error'
/// (character code 106 = the `j' of "just") rather than returning nil.
fn read_entry_signals_on_non_entry_text() -> ParityBatchCase {
    ParityBatchCase::signal(
        "read_entry_signals_on_non_entry_text",
        r####"
(with-temp-buffer
  (insert "just some prose, no entry here")
  (goto-char (point-min))
  (parsebib-read-entry))
"####,
        expect![[r#"ERR (parsebib-error 1 "Invalid character `%c'" 106)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        article_entry_extracts_type_key_and_fields(),
        book_entry_extracts_type_key_and_author(),
        read_entry_signals_on_non_entry_text(),
    ]
}
