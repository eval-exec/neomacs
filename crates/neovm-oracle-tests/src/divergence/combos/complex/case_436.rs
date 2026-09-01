//! Complex combo batch 436 — 16 more niche org-mode probes: org-bibtex,
//! org-eshell, org-feed, org-gnus, org-info, org-irc, org-mouse,
//! org-pcomplete, org-sudoku, org-w3m, org-wl, org-protocol,
//! org-choose, org-checklist, org-collector, org-toc.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// org-bibtex: BibTeX integration.
#[test]
fn div_cx436_org_bibtex_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-bibtex\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-bibtex)
  (list (fboundp 'org-bibtex-headline)
        (fboundp 'org-bibtex-fleshout)))
"##,
        expect,
    );
}

/// org-eshell: eshell integration.
#[test]
fn div_cx436_org_eshell_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-eshell\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-eshell)
  (list (boundp 'org-eshell-store-link-functions))))
"##,
        expect,
    );
}

/// org-feed: feed aggregation.
#[test]
fn div_cx436_org_feed_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-feed)
  (list (fboundp 'org-feed-update)
        (fboundp 'org-feed-goto-inbox)))
"##,
        expect,
    );
}

/// org-gnus: Gnus integration.
#[test]
fn div_cx436_org_gnus_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-gnus\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-gnus)
  (list (boundp 'org-gnus-store-link-functions))))
"##,
        expect,
    );
}

/// org-info: Info integration.
#[test]
fn div_cx436_org_info_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-info\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-info)
  (list (fboundp 'org-info-store-link)
        (fboundp 'org-info-open)))
"##,
        expect,
    );
}

/// org-irc: IRC integration.
#[test]
fn div_cx436_org_irc_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-irc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-irc)
  (list (boundp 'org-irc-store-link-functions))))
"##,
        expect,
    );
}

/// org-mouse: mouse-enabled org mode.
#[test]
fn div_cx436_org_mouse_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-mouse)
  (list (boundp 'org-mouse-features)
        (fboundp 'org-mouse-do-remotely)))
"##,
        expect,
    );
}

/// org-pcomplete: completion for org.
#[test]
fn div_cx436_org_pcomplete_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-pcomplete)
  (list (boundp 'pcomplete-org-setup)
        (fboundp 'org-pcomplete-initialize)))
"##,
        expect,
    );
}

/// org-sudoku: sudoku puzzle in org-mode.
#[test]
fn div_cx436_org_sudoku_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-sudoku\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-sudoku)
  (list (fboundp 'org-sudoku-create)
        (fboundp 'org-sudoku-solve)))
"##,
        expect,
    );
}

/// org-w3m: w3m browser integration.
#[test]
fn div_cx436_org_w3m_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-w3m\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-w3m)
  (list (boundp 'org-w3m-store-link-functions))))
"##,
        expect,
    );
}

/// org-wl: Wanderlust mail integration.
#[test]
fn div_cx436_org_wl_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-wl\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-wl)
  (list (boundp 'org-wl-store-link-functions)))
"##,
        expect,
    );
}

/// org-protocol: protocol handling.
#[test]
fn div_cx436_org_protocol_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-protocol)
  (list (fboundp 'org-protocol-check-filename-for-protocol)
        (fboundp 'org-protocol-create)))
"##,
        expect,
    );
}

/// org-choose: choose macro/tracking.
#[test]
fn div_cx436_org_choose_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-choose\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-choose)
  (list (fboundp 'org-choose-mark)
        (fboundp 'org-choose-reject)))
"##,
        expect,
    );
}

/// org-checklist: checklist handling.
#[test]
fn div_cx436_org_checklist_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-checklist\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-checklist)
  (list (fboundp 'org-checklist)))
"##,
        expect,
    );
}

/// org-collector: property collector.
#[test]
fn div_cx436_org_collector_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-collector\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-collector)
  (list (fboundp 'org-collect-todos)
        (boundp 'org-collect-allow-prop)))
"##,
        expect,
    );
}

/// org-toc: table of contents in org.
#[test]
fn div_cx436_org_toc_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-toc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-toc)
  (list (fboundp 'org-toc-show)
        (fboundp 'org-toc-recenter)))
"##,
        expect,
    );
}
