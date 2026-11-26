//! Behavioural parity for the pinned `ecss` cascade engine.
//!
//! `ecss` owns selectors, specificity, origins, importance, cascade layers,
//! CSS-wide values, custom properties and computed styles as pure data.  Every
//! probe is therefore deterministic in batch mode and needs no display; the
//! probes mirror the package's own public acceptance scenarios so a divergence
//! names a documented behaviour rather than an internal detail.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-ecss-schemas ()
  "The schema set used by ECSS's own public acceptance suite."
  (ecss-schema-set-compose
   (ecss-schema-package-create
    'app
    (append
     (list
      (list :id 'app/color :initial "black" :inherits t
            :validator #'stringp :impacts '(paint))
      (list :id 'app/width :initial 0 :validator #'integerp
            :impacts '(geometry))
      (list :id 'app/payload :initial nil :inherits t :impacts '(data)))
     (mapcar (lambda (property)
               (list :id property :initial 0 :validator #'integerp
                     :impacts '(geometry)))
             '(app/padding-top app/padding-right
               app/padding-bottom app/padding-left))))))
"####;

fn ecss_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("ecss", "0.1.0"), "ecss.el")
        .expect("prepare pinned ecss source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn ecss_cascade_batch() {
    let cases = vec![
        // The README's independent-use example, expanded with the metadata
        // accessors the package documents alongside it.
        ParityBatchCase::value(
            "readme_core_cascade",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (sheet (ecss-stylesheet-create))
       (button (ecss-subject-create :type "button" :classes '("primary"))))
  (ecss-stylesheet-add-rule
   sheet schemas "button.primary" '(app/color "royalblue"))
  (let ((style (ecss-compute-style schemas button :stylesheet sheet)))
    (list (ecss-computed-style-value style 'app/color)
          (ecss-computed-style-value style 'app/width)
          (ecss-computed-style-value style 'app/missing :absent)
          (ecss-computed-style-active-properties style)
          (ecss-computed-style-specified-properties style)
          (ecss-computed-style-present-p style 'app/color)
          (ecss-computed-style-specified-p style 'app/width)
          (ecss-computed-style-p style))))
"####,
            expect![[r#"OK ("royalblue" 0 :absent (app/color) (app/color) (app/color) nil t)"#]],
        ),
        // Specificity beats source order; the later of two equal-specificity
        // rules wins.
        ParityBatchCase::value(
            "selector_specificity_order",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (sheet (ecss-stylesheet-create))
       (subject (ecss-subject-create
                 :type "button" :id "go" :classes '("primary"))))
  (ecss-stylesheet-add-rule sheet schemas "button" '(app/color "type"))
  (ecss-stylesheet-add-rule sheet schemas ".primary" '(app/color "class"))
  (ecss-stylesheet-add-rule sheet schemas "#go" '(app/color "id"))
  (ecss-stylesheet-add-rule sheet schemas ".primary" '(app/color "class-late"))
  (list (ecss-computed-style-value
         (ecss-compute-style schemas subject :stylesheet sheet) 'app/color)
        (ecss-selector-specificity "button.primary")
        (ecss-selector-specificity "#go")
        (ecss-selector-specificity "ul > li.item:first-child")))
"####,
            expect![[r#"OK ("id" (0 1 1) (1 0 0) (0 2 2))"#]],
        ),
        // Origin order, then importance: a user !important declaration beats
        // an author !important declaration.
        ParityBatchCase::value(
            "origin_and_importance",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (sheet (ecss-stylesheet-create))
       (subject (ecss-subject-create :type "button" :classes '("target"))))
  (ecss-stylesheet-add-rule sheet schemas ".target" '(app/color "ua") :origin 'ua)
  (ecss-stylesheet-add-rule sheet schemas ".target" '(app/color "user") :origin 'user)
  (ecss-stylesheet-add-rule sheet schemas ".target" '(app/color "author") :origin 'author)
  (let ((before (ecss-computed-style-value
                 (ecss-compute-style schemas subject :stylesheet sheet)
                 'app/color)))
    (ecss-stylesheet-add-rule
     sheet schemas ".target" (list 'app/color (ecss-important "author-important"))
     :origin 'author)
    (ecss-stylesheet-add-rule
     sheet schemas ".target" (list 'app/color (ecss-important "user-important"))
     :origin 'user)
    (list before
          (ecss-computed-style-value
           (ecss-compute-style schemas subject :stylesheet sheet)
           'app/color))))
"####,
            expect![[r#"OK ("author" "user-important")"#]],
        ),
        // A later-declared cascade layer wins over an earlier one regardless of
        // rule insertion order.
        ParityBatchCase::value(
            "cascade_layers",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (sheet (ecss-stylesheet-create))
       (subject (ecss-subject-create :type "button" :classes '("target"))))
  (ecss-stylesheet-declare-layers sheet '(base components))
  (ecss-stylesheet-add-rule
   sheet schemas ".target" '(app/color "component") :layer 'components)
  (ecss-stylesheet-add-rule
   sheet schemas ".target" '(app/color "base") :layer 'base)
  (list (ecss-stylesheet-layers sheet)
        (ecss-stylesheet-layers sheet 'author)
        (ecss-computed-style-value
         (ecss-compute-style schemas subject :stylesheet sheet) 'app/color)))
"####,
            expect![[r#"OK ((base components) (base components) "component")"#]],
        ),
        // Inherited properties follow the parent style; CSS-wide initial and
        // unset behave differently for an inheriting property.
        ParityBatchCase::value(
            "inheritance_and_css_wide",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (parent (ecss-subject-create :type "div"))
       (child (ecss-subject-create :type "span" :parent parent))
       (parent-style (ecss-compute-style
                      schemas parent :declarations '(app/color "red")))
       (inherited (ecss-compute-style
                   schemas child :parent-style parent-style))
       (initial (ecss-compute-style
                 schemas child :parent-style parent-style
                 :declarations (list 'app/color (ecss-wide-value 'initial))))
       (unset (ecss-compute-style
               schemas child :parent-style parent-style
               :declarations (list 'app/color (ecss-wide-value 'unset)))))
  (list (ecss-computed-style-value inherited 'app/color)
        (ecss-computed-style-value initial 'app/color)
        (ecss-computed-style-value unset 'app/color)
        (ecss-computed-style-value
         (ecss-compute-style schemas child :parent-style parent-style
                             :declarations
                             (list 'app/width (ecss-wide-value 'unset)))
         'app/width)))
"####,
            expect![[r#"OK ("red" "black" "red" 0)"#]],
        ),
        // Custom properties resolve nested var() references and inherit by
        // default.
        ParityBatchCase::value(
            "custom_properties_and_var",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (subject (ecss-subject-create :type "button"))
       (style (ecss-compute-style
               schemas subject
               :declarations
               (list '--base 12 '--alias (ecss-var '--base)
                     'app/width (ecss-var '--alias)
                     'app/payload (list 'calc (ecss-var '--base)))))
       (parent (ecss-compute-style
                schemas subject :declarations '(--theme 17)))
       (child (ecss-compute-style
               schemas subject :parent-style parent
               :declarations (list 'app/width (ecss-var '--theme)))))
  (list (ecss-computed-style-value style 'app/width)
        (ecss-computed-style-value style 'app/payload)
        (ecss-computed-style-custom-properties style)
        (ecss-computed-style-value child 'app/width)
        (ecss-computed-style-custom-properties child)))
"####,
            expect!["OK (12 (calc 12) (--alias 12 --base 12) 17 (--theme 17))"],
        ),
        // Computed styles and their projected plists are defensive copies.
        ParityBatchCase::value(
            "defensive_values",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (subject (ecss-subject-create :type "button"))
       (style (ecss-compute-style schemas subject :declarations '(app/color "red")))
       (values (ecss-computed-style-values style))
       (copy (ecss-computed-style-copy-with-values
              style '(app/color "blue")
              :active-properties '(app/color)
              :specified-properties '(app/color))))
  (setf (plist-get values 'app/color) "mutated")
  (list (plist-get values 'app/color)
        (ecss-computed-style-value style 'app/color)
        (ecss-computed-style-value copy 'app/color)
        (eq style copy)
        (ecss-computed-style-active-properties copy)
        (ecss-computed-style-specified-properties copy)))
"####,
            expect![[r#"OK ("mutated" "red" "blue" nil (app/color) (app/color))"#]],
        ),
        // An invalid declaration is atomic: the signal is an ECSS condition and
        // a failed rule addition leaves the stylesheet unchanged.
        ParityBatchCase::value(
            "invalid_declaration_signal",
            r####"
(let* ((schemas (neomacs-ecss-schemas))
       (sheet (ecss-stylesheet-create)))
  (ecss-stylesheet-add-rule sheet schemas ".valid" '(app/color "blue") :layer 'base)
  (list (condition-case err
            (progn
              (ecss-stylesheet-add-rule
               sheet schemas ".broken" '(unknown/value 1) :layer 'broken)
              :no-error)
          (error (list (car err) (cdr err))))
        (ecss-stylesheet-layers sheet)
        (length (ecss-stylesheet-rules sheet))))
"####,
            expect!["OK ((ecss-invalid-declaration (:unknown-property unknown/value)) (base) 1)"],
        ),
        // Selector matching and parsing are pure and cover the documented
        // combinators, attribute selectors and states.
        ParityBatchCase::value(
            "selector_parse_and_match",
            r####"
(let ((subject (ecss-subject-create
                :type "button" :id "go" :classes '("primary")))
      (selectors '("button" ".primary" "#go" "button.primary"
                   "div button" "button[data-x]" "button:disabled")))
  (list (ecss-selector-specificity "button.primary")
        (ecss-selector-specificity "#go")
        (ecss-selector-specificity "ul > li.item:first-child")
        (mapcar (lambda (selector)
                  (list selector
                        (condition-case err
                            (ecss-selector-match-p selector subject)
                          (error (cons 'error (car err))))))
                selectors)
        (ecss-selector-parse "ul > li.item:first-child")))
"####,
            expect![[
                r##"OK ((0 1 1) (1 0 0) (0 2 2) (("button" t) (".primary" ("primary")) ("#go" t) ("button.primary" t) ("div button" nil) ("button[data-x]" nil) ("button:disabled" nil)) (:child (:type "ul") (:and (:type "li") (:class "item") (:state "first-child"))))"##
            ]],
        ),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("ecss parity batch");
    assert_oracle_batch_cases(ecss_oracle(), test_name, "ecss parity", &cases);
}
