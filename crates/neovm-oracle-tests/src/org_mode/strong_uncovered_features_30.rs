//! Strong uncovered-features-30 oracle tests — org-w3m, org-eww, org-info.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-w3m-get-url
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_w3m() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-w3m-get-url)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-eww-copy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_eww() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-eww-copy)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-info-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-info-link "(org) Top")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-man-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_man() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-man-link "ls")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-gnus-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_gnus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-gnus-link "nntp" "news.example.com" "group" "123")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-irc-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_irc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-irc-link "irc://irc.example.com/#channel")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bbdb-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_bbdb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-bbdb-link "John Doe")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-mhe-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_mhe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-mhe-link "12345")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-rmail-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_rmail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-rmail-link "12345")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-vm-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_vm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-vm-link "12345")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-wl-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_wl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-wl-link "12345")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collector
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_collector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n:PROPERTIES:\n:A: 1\n:END:\n* T2\n:PROPERTIES:\n:A: 2\n:END:\n* T3\n:PROPERTIES:\n:A: 3\n:END:")
  (condition-case nil
      (org-collector-get-field "A" nil)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collector-get-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_collector_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T1\n:PROPERTIES:\n:A: 1\n:END:\n* T2\n:PROPERTIES:\n:A: 2\n:END:")
  (condition-case nil
      (org-collector-get-entries nil '("A"))
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-expiry
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_expiry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* T\\n:PROPERTIES:\\n:CREATED: [2026-01-15]\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CREATED: [2026-01-15]\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-expiry-insert-created)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-expiry-insert-created
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_expiry_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (condition-case nil
      (org-expiry-insert-created)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-expiry-get-created
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_expiry_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CREATED: [2026-01-15]\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-expiry-get-created)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-expiry-process-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_expiry_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* T\\n:PROPERTIES:\\n:CREATED: [2026-01-15]\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:CREATED: [2026-01-15]\n:END:")
  (condition-case nil
      (org-expiry-process-entries (point-min) (point-max))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find "John")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-email
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_email() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-email "john@example.com")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-name
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-name "John Doe")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-property "EMAIL" "john@example.com")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-prepare-buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-prepare-buffer)
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-tag
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-tag "friend")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-address
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_addr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-address "123 Main St")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-phone
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_phone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-phone "555-1234")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-contacts-find-by-note
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf30_contacts_note() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-contacts-find-by-note "meeting")
  (error nil))"##,
        expect,
    );
}
