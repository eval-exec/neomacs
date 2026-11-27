//! GNU Emacs 31.1 parity for coding repertoire expansion (`:charset-list
//! 'iso-2022` / `'emacs-mule`), `find-coding-systems-region-internal`
//! candidate ordering (`Vcoding_system_list`), and fixed-EOL base bucket
//! resolution.
//!
//! GNU Emacs 31.1 (`a360712c9d27`) source contract:
//! - `Fdefine_coding_system_internal` (`src/coding.c:11017-11031`):
//!   - `:charset-list 'iso-2022` expands `CODING_ATTR_CHARSET_LIST` to
//!     `Viso_2022_charset_list`; `:charset-list 'emacs-mule` expands it to
//!     `Vemacs_mule_charset_list`.

use crate::common::assert_oracle_parity_expect;
use expect_test::expect;

#[test]
fn coding_repertoire_expands_iso2022_and_emacs_mule_markers() {
    let form = r#"(list
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #xe9))
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #x3042))
  (unencodable-char-position 0 2 'emacs-mule nil (string ?a #x1f600))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #xe9))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #x3042))
  (unencodable-char-position 0 2 'iso-2022-7bit nil (string ?a #x1f600))
  (unencodable-char-position 0 2 'compound-text nil (string ?a #xe9))
  (unencodable-char-position 0 2 'compound-text nil (string ?a #x1f600))
  (check-coding-systems-region (string ?a #xe9 #x3042 #x1f600) nil '(emacs-mule iso-2022-7bit compound-text utf-8)))"#;

    assert_oracle_parity_expect(
        form,
        expect![[
            r#""OK (nil nil 1 nil nil 1 nil 1 ((emacs-mule 3) (iso-2022-7bit 3) (compound-text 3)))""#
        ]],
    );
}
