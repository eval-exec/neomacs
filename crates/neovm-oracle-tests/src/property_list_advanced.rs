//! Oracle parity tests for advanced property list operations:
//! `symbol-plist`, `setplist`, property lists as metadata stores,
//! and complex plist manipulation patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// symbol-plist / setplist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (put 'neovm--test-plist-sym 'color 'red)
                  (put 'neovm--test-plist-sym 'size 42)
                  (put 'neovm--test-plist-sym 'active t)
                  (unwind-protect
                      (let ((plist (symbol-plist 'neovm--test-plist-sym)))
                        (list (plist-get plist 'color)
                              (plist-get plist 'size)
                              (plist-get plist 'active)))
                    (setplist 'neovm--test-plist-sym nil)))";
    let expect = expect_test::expect![[r#""OK (red 42 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setplist_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (put 'neovm--test-setplist 'old-prop 'old-val)
                  (setplist 'neovm--test-setplist '(new-prop new-val))
                  (unwind-protect
                      (list (get 'neovm--test-setplist 'old-prop)
                            (get 'neovm--test-setplist 'new-prop))
                    (setplist 'neovm--test-setplist nil)))";
    let expect = expect_test::expect![[r#""OK (nil new-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-get / plist-put / plist-member with various key types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_keyword_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((pl '(:name \"Alice\" :age 30 :active t)))
                  (list (plist-get pl :name)
                        (plist-get pl :age)
                        (plist-get pl :active)
                        (plist-get pl :missing)))";
    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_put_creates_and_updates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((pl nil))
                  (setq pl (plist-put pl :a 1))
                  (setq pl (plist-put pl :b 2))
                  (setq pl (plist-put pl :a 10))
                  (list (plist-get pl :a)
                        (plist-get pl :b)
                        (length pl)))";
    let expect = expect_test::expect![[r#""OK (10 2 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_member_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((pl '(:x 1 :y 2 :z 3)))
                  (list (plist-member pl :x)
                        (plist-member pl :y)
                        (plist-member pl :missing)))";
    let expect = expect_test::expect![[r#""OK ((:x 1 :y 2 :z 3) (:y 2 :z 3) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex plist patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two plists (second overrides first)
    let form = "(let ((plist-merge
                       (lambda (base override)
                         (let ((result (copy-sequence base))
                               (remaining override))
                           (while remaining
                             (setq result
                                   (plist-put result
                                              (car remaining)
                                              (cadr remaining)))
                             (setq remaining (cddr remaining)))
                           result))))
                  (let ((defaults '(:color blue :size 10 :weight normal))
                        (custom '(:color red :style bold)))
                    (funcall plist-merge defaults custom)))";
    let expect = expect_test::expect![[r#""OK (:color red :size 10 :weight normal :style bold)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_select_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Select specific keys from a plist
    let form = "(let ((plist-select
                       (lambda (pl keys)
                         (let ((result nil))
                           (dolist (k (reverse keys))
                             (let ((val (plist-get pl k)))
                               (when val
                                 (setq result
                                       (cons k (cons val result))))))
                           result))))
                  (funcall plist-select
                           '(:a 1 :b 2 :c 3 :d 4 :e 5)
                           '(:b :d :e)))";
    let expect = expect_test::expect![[r#""OK (:b 2 :d 4 :e 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_to_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert between plist and alist
    let form = "(let ((plist-to-alist
                       (lambda (pl)
                         (let ((result nil)
                               (remaining pl))
                           (while remaining
                             (setq result
                                   (cons (cons (car remaining)
                                               (cadr remaining))
                                         result))
                             (setq remaining (cddr remaining)))
                           (nreverse result))))
                      (alist-to-plist
                       (lambda (al)
                         (let ((result nil))
                           (dolist (pair (reverse al))
                             (setq result
                                   (cons (car pair)
                                         (cons (cdr pair) result))))
                           result))))
                  (let ((pl '(:name alice :age 30 :role engineer)))
                    (let ((al (funcall plist-to-alist pl)))
                      (let ((roundtrip (funcall alist-to-plist al)))
                        (list al roundtrip (equal pl roundtrip))))))";
    let expect = expect_test::expect![[
        r#""OK (((:name . alice) (:age . 30) (:role . engineer)) (:name alice :age 30 :role engineer) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find differences between two plists
    let form = "(let ((plist-diff
                       (lambda (old new)
                         (let ((added nil)
                               (changed nil)
                               (remaining new))
                           (while remaining
                             (let ((k (car remaining))
                                   (v (cadr remaining)))
                               (let ((old-v (plist-get old k)))
                                 (cond
                                   ((not (plist-member old k))
                                    (setq added
                                          (cons (list k v) added)))
                                   ((not (equal old-v v))
                                    (setq changed
                                          (cons (list k old-v v)
                                                changed))))))
                             (setq remaining (cddr remaining)))
                           (list (nreverse added)
                                 (nreverse changed))))))
                  (funcall plist-diff
                           '(:a 1 :b 2 :c 3)
                           '(:a 1 :b 20 :d 4)))";
    let expect = expect_test::expect![[r#""OK (((:d 4)) ((:b 2 20)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_as_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use plist as configuration with defaults and validation
    let form = r#"(let ((defaults '(:host "localhost" :port 8080
                                    :debug nil :timeout 30))
                        (user-config '(:port 3000 :debug t)))
                    ;; Merge with defaults
                    (let ((config (copy-sequence defaults))
                          (remaining user-config))
                      (while remaining
                        (setq config (plist-put config
                                               (car remaining)
                                               (cadr remaining)))
                        (setq remaining (cddr remaining)))
                      ;; Validate
                      (let ((valid t)
                            (errors nil))
                        (unless (stringp (plist-get config :host))
                          (setq valid nil
                                errors (cons "host must be string" errors)))
                        (unless (integerp (plist-get config :port))
                          (setq valid nil
                                errors (cons "port must be integer" errors)))
                        (list config valid errors))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:host \"localhost\" :port 3000 :debug t :timeout 30) t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
