//! Complex OOP-like patterns implemented in Elisp:
//! plist-based objects, closure-based encapsulation, prototype chains,
//! method dispatch tables, mixin composition, and observer/event
//! emitter pattern.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Property-list-based objects with get/put for fields
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_plist_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // "Objects" are plists; "methods" are functions that take a plist as self.
    // Demonstrates constructor, field access, and immutable update.
    let form = r#"(let ((make-point
                         (lambda (x y)
                           (list :type 'point :x x :y y)))
                        (point-x (lambda (p) (plist-get p :x)))
                        (point-y (lambda (p) (plist-get p :y)))
                        (point-distance
                         (lambda (p1 p2)
                           (let ((dx (- (plist-get p1 :x) (plist-get p2 :x)))
                                 (dy (- (plist-get p1 :y) (plist-get p2 :y))))
                             (+ (* dx dx) (* dy dy)))))
                        (point-translate
                         (lambda (p dx dy)
                           (list :type 'point
                                 :x (+ (plist-get p :x) dx)
                                 :y (+ (plist-get p :y) dy)))))
                    (let ((p1 (funcall make-point 3 4))
                          (p2 (funcall make-point 0 0)))
                      (let ((p3 (funcall point-translate p1 10 20)))
                        (list
                         (funcall point-x p1)
                         (funcall point-y p1)
                         (funcall point-distance p1 p2)  ;; 3^2+4^2=25
                         (funcall point-x p3)             ;; 13
                         (funcall point-y p3)             ;; 24
                         ;; Original p1 unchanged
                         (funcall point-x p1)             ;; 3
                         (plist-get p1 :type)))))"#;
    let expect = expect_test::expect![[r#""OK (3 4 25 13 24 3 point)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure-based objects with private state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_closure_encapsulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A bank account: balance is truly private, only accessible via
    // deposit/withdraw/balance closures.
    let form = r#"(let ((make-account
                         (lambda (initial-balance)
                           (let ((balance initial-balance)
                                 (history nil))
                             (let ((deposit
                                    (lambda (amount)
                                      (setq balance (+ balance amount))
                                      (setq history (cons (list 'deposit amount balance) history))
                                      balance))
                                   (withdraw
                                    (lambda (amount)
                                      (if (> amount balance)
                                          (list 'error "insufficient funds" balance)
                                        (setq balance (- balance amount))
                                        (setq history (cons (list 'withdraw amount balance) history))
                                        balance)))
                                   (get-balance (lambda () balance))
                                   (get-history (lambda () (nreverse history))))
                               (list deposit withdraw get-balance get-history))))))
                    (let* ((acc (funcall make-account 100))
                           (deposit  (nth 0 acc))
                           (withdraw (nth 1 acc))
                           (balance  (nth 2 acc))
                           (history  (nth 3 acc)))
                      (funcall deposit 50)
                      (funcall withdraw 30)
                      (funcall deposit 200)
                      (let ((fail-result (funcall withdraw 500)))
                        (list (funcall balance)
                              fail-result
                              (funcall history)))))"#;
    let expect = expect_test::expect![[
        r#""OK (320 (error \"insufficient funds\" 320) ((deposit 50 150) (withdraw 30 120) (deposit 200 320)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prototype chain: object inherits from parent via alist lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_prototype_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Objects are alists with a special __proto__ key pointing to parent.
    // Lookup walks the chain. Demonstrates property shadowing.
    let form = r#"(let ((obj-get nil)
                        (obj-set nil)
                        (make-obj nil))
                    ;; Recursive prototype lookup
                    (setq obj-get
                          (lambda (obj key)
                            (let ((pair (assq key obj)))
                              (if pair
                                  (cdr pair)
                                (let ((proto (cdr (assq '__proto__ obj))))
                                  (if proto
                                      (funcall obj-get proto key)
                                    nil))))))
                    ;; Set (shadow) a property
                    (setq obj-set
                          (lambda (obj key val)
                            (cons (cons key val)
                                  (assq-delete-all key obj))))
                    ;; Constructor with prototype
                    (setq make-obj
                          (lambda (&optional proto)
                            (if proto
                                (list (cons '__proto__ proto))
                              nil)))
                    ;; Build prototype chain: grandparent -> parent -> child
                    (let* ((grandparent (list (cons 'species "animal")
                                             (cons 'legs 4)))
                           (parent (cons (cons '__proto__ grandparent)
                                        (list (cons 'species "mammal")
                                              (cons 'sound "generic"))))
                           (child (cons (cons '__proto__ parent)
                                       (list (cons 'name "Rex")
                                             (cons 'sound "woof")))))
                      (list
                       ;; Direct property
                       (funcall obj-get child 'name)        ;; "Rex"
                       ;; Shadowed: child overrides parent's sound
                       (funcall obj-get child 'sound)       ;; "woof"
                       ;; Inherited from parent
                       (funcall obj-get child 'species)     ;; "mammal"
                       ;; Inherited from grandparent
                       (funcall obj-get child 'legs)        ;; 4
                       ;; Not found
                       (funcall obj-get child 'color)       ;; nil
                       ;; Parent still has its own sound
                       (funcall obj-get parent 'sound)      ;; "generic"
                       ;; Grandparent species
                       (funcall obj-get grandparent 'species) ;; "animal"
                       ;; Set new property on child, parent unaffected
                       (let ((child2 (funcall obj-set child 'color "brown")))
                         (list (funcall obj-get child2 'color)
                               (funcall obj-get parent 'color))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Rex\" \"woof\" \"mammal\" 4 nil \"generic\" \"animal\" (\"brown\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Method dispatch table with hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_method_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A class-like system: methods stored in a hash table keyed by
    // (class . method-name). Dispatch finds method for object's class.
    let form = r#"(let ((methods (make-hash-table :test 'equal))
                        (dispatch nil))
                    (setq dispatch
                          (lambda (obj method-name &rest args)
                            (let* ((class (plist-get obj :class))
                                   (fn (gethash (cons class method-name) methods)))
                              (if fn
                                  (apply fn obj args)
                                (error "No method %s for class %s"
                                       method-name class)))))
                    ;; Define "dog" class methods
                    (puthash (cons 'dog 'speak) (lambda (self) "woof!") methods)
                    (puthash (cons 'dog 'describe)
                             (lambda (self)
                               (format "%s the dog" (plist-get self :name)))
                             methods)
                    ;; Define "cat" class methods
                    (puthash (cons 'cat 'speak) (lambda (self) "meow!") methods)
                    (puthash (cons 'cat 'describe)
                             (lambda (self)
                               (format "%s the cat" (plist-get self :name)))
                             methods)
                    ;; Shared method: both classes
                    (dolist (cls '(dog cat))
                      (puthash (cons cls 'greet)
                               (lambda (self other)
                                 (format "%s greets %s with %s"
                                         (plist-get self :name)
                                         (plist-get other :name)
                                         (funcall dispatch self 'speak)))
                               methods))
                    ;; Create instances and dispatch
                    (let ((rex '(:class dog :name "Rex"))
                          (whiskers '(:class cat :name "Whiskers")))
                      (list
                       (funcall dispatch rex 'speak)
                       (funcall dispatch whiskers 'speak)
                       (funcall dispatch rex 'describe)
                       (funcall dispatch whiskers 'describe)
                       (funcall dispatch rex 'greet whiskers)
                       (funcall dispatch whiskers 'greet rex))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"woof!\" \"meow!\" \"Rex the dog\" \"Whiskers the cat\" \"Rex greets Whiskers with woof!\" \"Whiskers greets Rex with meow!\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixin pattern: combining multiple behaviors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_mixin_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mixins as alists of (method-name . function).
    // Composing mixins merges their methods. Last mixin wins on conflict.
    let form = r#"(let ((make-mixin
                         (lambda (methods)
                           methods))
                        (compose-mixins
                         (lambda (&rest mixins)
                           (let ((result nil))
                             (dolist (mixin mixins)
                               (dolist (method mixin)
                                 (unless (assq (car method) result)
                                   (setq result (cons method result)))))
                             result)))
                        (mixin-call
                         (lambda (obj mixin method-name &rest args)
                           (let ((fn (cdr (assq method-name mixin))))
                             (if fn (apply fn obj args)
                               (error "no method: %s" method-name))))))
                    ;; Printable mixin
                    (let ((printable
                           (list (cons 'to-string
                                       (lambda (self)
                                         (format "[%s: %s]"
                                                 (plist-get self :type)
                                                 (plist-get self :name)))))))
                      ;; Comparable mixin (by :priority)
                      (let ((comparable
                             (list (cons 'less-than
                                         (lambda (self other)
                                           (< (plist-get self :priority)
                                              (plist-get other :priority))))
                                   (cons 'equal-to
                                         (lambda (self other)
                                           (= (plist-get self :priority)
                                              (plist-get other :priority)))))))
                        ;; Serializable mixin
                        (let ((serializable
                               (list (cons 'serialize
                                           (lambda (self)
                                             (let ((result nil))
                                               (let ((props self))
                                                 (while props
                                                   (setq result
                                                         (cons (format "%s=%S"
                                                                       (car props)
                                                                       (cadr props))
                                                               result))
                                                   (setq props (cddr props))))
                                               (mapconcat #'identity
                                                          (nreverse result) ",")))))))
                          ;; Compose all mixins
                          (let ((all-methods
                                 (funcall compose-mixins
                                          printable comparable serializable)))
                            (let ((task1 '(:type task :name "Build" :priority 2))
                                  (task2 '(:type task :name "Test" :priority 1)))
                              (list
                               (funcall mixin-call task1 all-methods 'to-string)
                               (funcall mixin-call task2 all-methods 'to-string)
                               (funcall mixin-call task1 all-methods 'less-than task2)
                               (funcall mixin-call task2 all-methods 'less-than task1)
                               (funcall mixin-call task1 all-methods 'equal-to task2)
                               (funcall mixin-call task1 all-methods 'serialize))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[task: Build]\" \"[task: Test]\" nil t nil \":type=task,:name=\\\"Build\\\",:priority=2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Observer / event emitter pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_oop_event_emitter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Event emitter: register listeners for named events, emit events
    // with data, listeners called in registration order.
    let form = r#"(let ((make-emitter
                         (lambda ()
                           (let ((listeners (make-hash-table :test 'eq)))
                             (list
                              ;; on: register listener
                              (lambda (event fn)
                                (puthash event
                                         (append (gethash event listeners nil)
                                                 (list fn))
                                         listeners))
                              ;; emit: fire event with data
                              (lambda (event &rest data)
                                (let ((fns (gethash event listeners nil))
                                      (results nil))
                                  (dolist (fn fns)
                                    (setq results
                                          (cons (apply fn data) results)))
                                  (nreverse results)))
                              ;; listener-count
                              (lambda (event)
                                (length (gethash event listeners nil)))
                              ;; off: remove all listeners for event
                              (lambda (event)
                                (remhash event listeners)))))))
                    (let* ((em (funcall make-emitter))
                           (on (nth 0 em))
                           (emit (nth 1 em))
                           (count (nth 2 em))
                           (off (nth 3 em))
                           (log nil))
                      ;; Register listeners
                      (funcall on 'data
                               (lambda (x)
                                 (setq log (cons (list 'listener1 x) log))
                                 (list 'got x)))
                      (funcall on 'data
                               (lambda (x)
                                 (setq log (cons (list 'listener2 (* x 2)) log))
                                 (list 'doubled (* x 2))))
                      (funcall on 'error
                               (lambda (msg)
                                 (setq log (cons (list 'error msg) log))
                                 'handled))
                      ;; Emit events
                      (let ((r1 (funcall emit 'data 42))
                            (r2 (funcall emit 'data 7))
                            (r3 (funcall emit 'error "boom"))
                            (r4 (funcall emit 'unknown 99)))
                        ;; Check counts and remove
                        (let ((c1 (funcall count 'data))
                              (c2 (funcall count 'error)))
                          (funcall off 'data)
                          (let ((c3 (funcall count 'data))
                                (r5 (funcall emit 'data 999)))
                            (list r1 r2 r3 r4
                                  c1 c2 c3 r5
                                  (nreverse log)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((got 42) (doubled 84)) ((got 7) (doubled 14)) (handled) nil 2 1 0 nil ((listener1 42) (listener2 84) (listener1 7) (listener2 14) (error \"boom\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
