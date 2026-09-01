//! Strict combo oracle probes, batch 280: core subr fboundp sweep. Verifies
//! standard built-in functions exist. Any nil-in-Neomacs/t-in-GNU is a missing-
//! function bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_list_sequence_subr_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'mapc)
      (fboundp 'mapcan)
      (fboundp 'mapconcat)
      (fboundp 'mapcar)
      (fboundp 'delq)
      (fboundp 'nconc)
      (fboundp 'copy-tree)
      (fboundp 'copy-sequence)
      (fboundp 'number-sequence)
      (fboundp 'assoc-delete-all)
      (fboundp 'rassq-delete-all)
      (fboundp 'assq-delete-all)
      (fboundp 'member)
      (fboundp 'memql)
      (fboundp 'remq)
      (fboundp 'delete-consecutive-dups)
      (fboundp 'split-string)
      (fboundp 'combine-and-quote-strings)
      (fboundp 'shell-quote-argument)
      (fboundp 'string-match-p)
      (fboundp 'looking-at-p)
      (fboundp 'lookup-key)
      (fboundp 'where-is-internal))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_position_subr_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'buffer-match-p)
      (fboundp 'match-buffers-p)
      (fboundp 'position-bytes)
      (fboundp 'byte-to-position)
      (fboundp 'line-beginning-position)
      (fboundp 'line-end-position)
      (fboundp 'line-number-at-pos)
      (fboundp 'count-lines)
      (fboundp 'count-screen-lines)
      (fboundp 'pos-visible-in-window-p)
      (fboundp 'window-text-pixel-size)
      (fboundp 'posn-at-point)
      (fboundp 'posn-at-x-y)
      (fboundp 'format-mode-line)
      (fboundp 'buffer-local-value)
      (fboundp 'default-value)
      (fboundp 'local-variable-p)
      (fboundp 'local-variable-if-set-p)
      (fboundp 'kill-local-variable)
      (fboundp 'make-variable-buffer-local)
      (fboundp 'make-local-variable))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_io_process_subr_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'call-process)
      (fboundp 'call-process-region)
      (fboundp 'process-lines)
      (fboundp 'make-process)
      (fboundp 'make-pipe-process)
      (fboundp 'make-network-process)
      (fboundp 'make-serial-process)
      (fboundp 'accept-process-output)
      (fboundp 'process-live-p)
      (fboundp 'process-running-child-p)
      (fboundp 'set-process-filter)
      (fboundp 'set-process-sentinel)
      (fboundp 'process-put)
      (fboundp 'process-get)
      (fboundp 'process-attributes)
      (fboundp 'format-network-address)
      (fboundp 'network-interface-list)
      (fboundp 'network-interface-info)
      (fboundp 'write-region)
      (fboundp 'insert-file-contents)
      (fboundp 'file-attributes))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
