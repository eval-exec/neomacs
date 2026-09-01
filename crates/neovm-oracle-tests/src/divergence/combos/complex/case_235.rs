//! Complex combo batch 235 — `shr` / `eww` / `svg` / `dom` rendering hooks
//! and `url-retrieve` stubs availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx235_shr_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'shr)
      (list (fboundp 'shr-insert-document)
            (boundp 'shr-width)
            (boundp 'shr-use-fonts)
            (boundp 'shr-max-image-proportion)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_shr_render_simple_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t #(\"Hello\\nWorld\\n\" 0 1 (face shr-text shr-indentation nil) 1 5 (face shr-text) 5 6 (face nil) 6 11 (face (shr-text bold))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'shr)
      (require 'dom)
      (let ((dom (with-temp-buffer
                   (insert "<html><body><p>Hello <b>World</b></p></body></html>")
                   (libxml-parse-html-region (point-min) (point-max)))))
        (with-temp-buffer
          (shr-insert-document dom)
          (let ((result (buffer-string)))
            (list (stringp result)
                  (> (length result) 0)
                  result)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_eww_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eww)
      (list (fboundp 'eww)
            (fboundp 'eww-browse-url)
            (fboundp 'eww-back-url)
            (fboundp 'eww-forward-url)
            (boundp 'eww-search-prefix)
            (boundp 'eww-history-limit)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_dom_query_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dom)
      (let* ((xml "<root attr=\"val\"><child>text</child><child>more</child><empty/></root>")
             (parsed (with-temp-buffer
                       (insert xml)
                       (xml-parse-region (point-min) (point-max))))
             (dom (car parsed)))
        (list (dom-node-p dom)
              (car dom)
              (dom-attr dom 'attr)
              (length (dom-by-tag dom 'child))
              (dom-text (car (dom-by-tag dom 'child))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_url_retrieve_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'url)
      (list (fboundp 'url-retrieve)
            (fboundp 'url-retrieve-synchronously)
            (boundp 'url-user-agent)
            (boundp 'url-privacy-level)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_svg_create_basic_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t image svg)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'svg)
      (let ((svg (svg-create 200 100)))
        (svg-rectangle svg 10 10 80 60 :fill "red")
        (svg-circle svg 150 50 30 :fill "blue")
        (svg-line svg 0 0 200 100 :stroke "green")
        (svg-text svg "Hello" :x 100 :y 90)
        (let ((img (svg-image svg)))
          (list (imagep img)
                (car img)
                (plist-get (cdr img) :type)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_svg_path_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'svg)
      (let ((svg (svg-create 100 100)))
        (svg-path svg '((moveto ((10 . 10)))
                        (lineto ((90 . 90)))
                        (lineto ((10 . 90)))
                        (closepath))
                  :stroke "black" :fill "none")
        (let ((img (svg-image svg)))
          (list (imagep img)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_libxml_parse_html_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'libxml-parse-html-region)
          (fboundp 'libxml-parse-xml-region)
          (fboundp 'shr-insert-document))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_dom_child_text_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Hello World !\" (\"Hello\" \"World\") 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dom)
      (let* ((xml "<div><p>Hello</p><p>World</p><span>!</span></div>")
             (parsed (with-temp-buffer
                       (insert xml)
                       (xml-parse-region (point-min) (point-max))))
             (dom (car parsed)))
        (list (dom-texts dom)
              (mapcar #'dom-text (dom-by-tag dom 'p))
              (length (dom-by-tag dom 'p)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx235_shr_eww_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'shr)
      (require 'eww)
      (require 'dom)
      (let* ((xml "<root><child>text</child></root>")
             (parsed (with-temp-buffer
                       (insert xml)
                       (xml-parse-region (point-min) (point-max))))
             (dom (car parsed)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert (format "SHR/EWW mega: dom-tag=%s" (car dom)))
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 10))
                (ov (make-overlay 4 18)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 25)
            (let ((state (list (fboundp 'shr-insert-document)
                               (fboundp 'eww)
                               (car dom)
                               (dom-attr dom 'attr)
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
