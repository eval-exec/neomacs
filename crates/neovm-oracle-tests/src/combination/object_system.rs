//! Oracle parity tests for a simple object system built in Elisp.
//!
//! Tests objects as closures with method dispatch, inheritance via prototype
//! chain, mixins/multiple inheritance, encapsulation (private state),
//! polymorphism (virtual method dispatch), introspection, and factory pattern.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Closure-based objects with method dispatch via symbol messages
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_closure_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Objects are closures that accept a message symbol as first arg.
    // Internal state is truly private (lexical scope).
    let form = r#"(progn
  (fset 'neovm--os-make-counter
    (lambda (initial step)
      "Create a counter object as a closure. Messages: get, inc, dec, reset, step-size."
      (let ((value initial)
            (init-val initial)
            (s step)
            (history nil))
        (lambda (msg &rest args)
          (cond
            ((eq msg 'get) value)
            ((eq msg 'inc)
             (setq history (cons value history))
             (setq value (+ value s))
             value)
            ((eq msg 'dec)
             (setq history (cons value history))
             (setq value (- value s))
             value)
            ((eq msg 'reset)
             (setq history (cons value history))
             (setq value init-val)
             value)
            ((eq msg 'set)
             (setq history (cons value history))
             (setq value (car args))
             value)
            ((eq msg 'step-size) s)
            ((eq msg 'history) (nreverse (copy-sequence history)))
            ((eq msg 'undo)
             (when history
               (setq value (car history))
               (setq history (cdr history)))
             value)
            (t (error "Unknown message: %s" msg)))))))

  (unwind-protect
      (let ((c1 (funcall 'neovm--os-make-counter 0 1))
            (c2 (funcall 'neovm--os-make-counter 100 5)))
        ;; c1: increment a few times
        (funcall c1 'inc)
        (funcall c1 'inc)
        (funcall c1 'inc)
        (let ((v1 (funcall c1 'get)))
          ;; c2: decrement
          (funcall c2 'dec)
          (funcall c2 'dec)
          (let ((v2 (funcall c2 'get)))
            ;; c1: set and undo
            (funcall c1 'set 42)
            (let ((v3 (funcall c1 'get)))
              (funcall c1 'undo)
              (let ((v4 (funcall c1 'get)))
                ;; c1: reset
                (funcall c1 'reset)
                (let ((v5 (funcall c1 'get))
                      (h1 (funcall c1 'history)))
                  ;; c2 is independent
                  (list
                    v1           ;; 3
                    v2           ;; 90
                    v3           ;; 42
                    v4           ;; 3 (undo)
                    v5           ;; 0 (reset)
                    h1           ;; history trace
                    (funcall c1 'step-size)  ;; 1
                    (funcall c2 'step-size)  ;; 5
                    ;; Prove isolation: c1 operations didn't affect c2
                    (funcall c2 'get))))))))
    (fmakunbound 'neovm--os-make-counter)))"#;
    let expect = expect_test::expect![[r#""OK (3 90 42 3 0 (0 1 2 3) 1 5 90)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prototype chain inheritance with method resolution order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_prototype_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Objects are hash-tables with a __proto__ key for parent lookup.
    // Method resolution walks the prototype chain.
    let form = r#"(progn
  (fset 'neovm--os-obj-new
    (lambda (&optional proto)
      (let ((obj (make-hash-table :test 'eq)))
        (when proto (puthash '__proto__ proto obj))
        obj)))

  (fset 'neovm--os-obj-set
    (lambda (obj key val)
      (puthash key val obj)))

  (fset 'neovm--os-obj-get
    (lambda (obj key)
      "Lookup KEY in OBJ, walking prototype chain."
      (let ((current obj)
            (found nil)
            (result nil))
        (while (and current (not found))
          (let ((val (gethash key current 'neovm--os-not-found)))
            (if (not (eq val 'neovm--os-not-found))
                (progn (setq found t) (setq result val))
              (setq current (gethash '__proto__ current nil)))))
        result)))

  (fset 'neovm--os-obj-has-own
    (lambda (obj key)
      (not (eq (gethash key obj 'neovm--os-not-found) 'neovm--os-not-found))))

  (fset 'neovm--os-obj-send
    (lambda (obj method &rest args)
      "Call METHOD on OBJ, resolving through prototype chain."
      (let ((fn (funcall 'neovm--os-obj-get obj method)))
        (if fn
            (apply fn obj args)
          (error "No method %s" method)))))

  (unwind-protect
      (progn
        ;; Base: Animal
        (let ((animal (funcall 'neovm--os-obj-new)))
          (funcall 'neovm--os-obj-set animal 'type "animal")
          (funcall 'neovm--os-obj-set animal 'speak
                   (lambda (self) "..."))
          (funcall 'neovm--os-obj-set animal 'describe
                   (lambda (self)
                     (format "%s says %s"
                             (funcall 'neovm--os-obj-get self 'name)
                             (funcall 'neovm--os-obj-send self 'speak))))
          ;; Dog inherits from Animal
          (let ((dog (funcall 'neovm--os-obj-new animal)))
            (funcall 'neovm--os-obj-set dog 'type "dog")
            (funcall 'neovm--os-obj-set dog 'speak
                     (lambda (self) "woof!"))
            (funcall 'neovm--os-obj-set dog 'fetch
                     (lambda (self item)
                       (format "%s fetches the %s"
                               (funcall 'neovm--os-obj-get self 'name) item)))
            ;; Cat inherits from Animal
            (let ((cat (funcall 'neovm--os-obj-new animal)))
              (funcall 'neovm--os-obj-set cat 'type "cat")
              (funcall 'neovm--os-obj-set cat 'speak
                       (lambda (self) "meow!"))
              ;; Kitten inherits from Cat (3 levels)
              (let ((kitten (funcall 'neovm--os-obj-new cat)))
                (funcall 'neovm--os-obj-set kitten 'speak
                         (lambda (self) "mew!"))
                ;; Create instances
                (let ((rex (funcall 'neovm--os-obj-new dog))
                      (whiskers (funcall 'neovm--os-obj-new cat))
                      (tiny (funcall 'neovm--os-obj-new kitten)))
                  (funcall 'neovm--os-obj-set rex 'name "Rex")
                  (funcall 'neovm--os-obj-set whiskers 'name "Whiskers")
                  (funcall 'neovm--os-obj-set tiny 'name "Tiny")
                  (list
                    ;; Polymorphic speak
                    (funcall 'neovm--os-obj-send rex 'speak)
                    (funcall 'neovm--os-obj-send whiskers 'speak)
                    (funcall 'neovm--os-obj-send tiny 'speak)
                    ;; Describe (inherited from Animal, calls speak polymorphically)
                    (funcall 'neovm--os-obj-send rex 'describe)
                    (funcall 'neovm--os-obj-send whiskers 'describe)
                    (funcall 'neovm--os-obj-send tiny 'describe)
                    ;; Dog-specific method
                    (funcall 'neovm--os-obj-send rex 'fetch "ball")
                    ;; Type resolution through chain
                    (funcall 'neovm--os-obj-get rex 'type)
                    (funcall 'neovm--os-obj-get tiny 'type)
                    ;; has-own checks
                    (funcall 'neovm--os-obj-has-own rex 'name)
                    (funcall 'neovm--os-obj-has-own rex 'speak))))))))
    (fmakunbound 'neovm--os-obj-new)
    (fmakunbound 'neovm--os-obj-set)
    (fmakunbound 'neovm--os-obj-get)
    (fmakunbound 'neovm--os-obj-has-own)
    (fmakunbound 'neovm--os-obj-send)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"woof!\" \"meow!\" \"mew!\" \"Rex says woof!\" \"Whiskers says meow!\" \"Tiny says mew!\" \"Rex fetches the ball\" \"dog\" \"cat\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixin-based multiple inheritance with conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_mixins_multiple_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mixins are alists of (method-name . function).
    // Composing mixins: last-one-wins for conflicts.
    // super-call by explicitly referencing parent mixin.
    let form = r#"(progn
  (fset 'neovm--os-mixin-create
    (lambda (name methods)
      "Create a mixin: (name . methods-alist)."
      (cons name methods)))

  (fset 'neovm--os-mixin-compose
    (lambda (&rest mixins)
      "Compose mixins. Later mixins override earlier ones."
      (let ((result nil)
            (origin nil))
        (dolist (mixin mixins)
          (dolist (method (cdr mixin))
            (let ((existing (assq (car method) result)))
              (if existing
                  (setcdr existing (cdr method))
                (setq result (cons (cons (car method) (cdr method)) result))))
            (let ((orig-key (cons (car mixin) (car method))))
              (setq origin (cons (cons orig-key (cdr method)) origin)))))
        (list result origin))))

  (fset 'neovm--os-mixin-call
    (lambda (composed method self &rest args)
      (let ((fn (cdr (assq method (car composed)))))
        (if fn
            (apply fn self args)
          (error "No method: %s" method)))))

  (fset 'neovm--os-mixin-list-methods
    (lambda (composed)
      (sort (mapcar #'car (car composed))
            (lambda (a b) (string< (symbol-name a) (symbol-name b))))))

  (unwind-protect
      (let ((printable
             (funcall 'neovm--os-mixin-create 'printable
                      (list (cons 'to-string
                                  (lambda (self)
                                    (format "[%s]" (plist-get self :name)))))))
            (comparable
             (funcall 'neovm--os-mixin-create 'comparable
                      (list (cons 'less-than
                                  (lambda (self other)
                                    (< (plist-get self :priority)
                                       (plist-get other :priority))))
                            (cons 'greater-than
                                  (lambda (self other)
                                    (> (plist-get self :priority)
                                       (plist-get other :priority))))
                            (cons 'equal-to
                                  (lambda (self other)
                                    (= (plist-get self :priority)
                                       (plist-get other :priority)))))))
            (taggable
             (funcall 'neovm--os-mixin-create 'taggable
                      (list (cons 'tags
                                  (lambda (self) (plist-get self :tags)))
                            (cons 'has-tag
                                  (lambda (self tag)
                                    (memq tag (plist-get self :tags))))
                            ;; Override to-string to include tags
                            (cons 'to-string
                                  (lambda (self)
                                    (format "[%s tags:%s]"
                                            (plist-get self :name)
                                            (mapconcat #'symbol-name
                                                       (plist-get self :tags) ","))))))))
        (let ((composed (funcall 'neovm--os-mixin-compose
                                 printable comparable taggable)))
          (let ((task1 '(:name "Build" :priority 2 :tags (dev ci)))
                (task2 '(:name "Test" :priority 1 :tags (qa)))
                (task3 '(:name "Deploy" :priority 2 :tags (ops ci))))
            (list
              ;; to-string uses taggable's override (last wins)
              (funcall 'neovm--os-mixin-call composed 'to-string task1)
              (funcall 'neovm--os-mixin-call composed 'to-string task2)
              ;; Comparison
              (funcall 'neovm--os-mixin-call composed 'less-than task2 task1)
              (funcall 'neovm--os-mixin-call composed 'greater-than task1 task2)
              (funcall 'neovm--os-mixin-call composed 'equal-to task1 task3)
              ;; Tags
              (funcall 'neovm--os-mixin-call composed 'tags task1)
              (funcall 'neovm--os-mixin-call composed 'has-tag task1 'ci)
              (funcall 'neovm--os-mixin-call composed 'has-tag task2 'dev)
              ;; Introspection
              (funcall 'neovm--os-mixin-list-methods composed)))))
    (fmakunbound 'neovm--os-mixin-create)
    (fmakunbound 'neovm--os-mixin-compose)
    (fmakunbound 'neovm--os-mixin-call)
    (fmakunbound 'neovm--os-mixin-list-methods)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[Build tags:dev,ci]\" \"[Test tags:qa]\" t t t (dev ci) (ci) nil (equal-to greater-than has-tag less-than tags to-string))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Encapsulation with accessor generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_encapsulation_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate getter/setter closures for named fields.
    // Private fields have no public setter. Validation on set.
    let form = r#"(progn
  (fset 'neovm--os-make-class
    (lambda (fields)
      "Create a class from field specs: ((name :read :write) (age :read :write :validate integerp) ...).
       Returns a constructor function."
      (lambda (&rest init-values)
        (let ((state (make-hash-table :test 'eq))
              (validators (make-hash-table :test 'eq))
              (readable (make-hash-table :test 'eq))
              (writable (make-hash-table :test 'eq)))
          ;; Set up field metadata
          (dolist (field fields)
            (let ((fname (car field))
                  (opts (cdr field)))
              (when (memq :read opts) (puthash fname t readable))
              (when (memq :write opts) (puthash fname t writable))
              (let ((validate-fn (plist-get opts :validate)))
                (when validate-fn (puthash fname validate-fn validators)))))
          ;; Initialize with given values
          (let ((vals init-values))
            (dolist (field fields)
              (when vals
                (puthash (car field) (car vals) state)
                (setq vals (cdr vals)))))
          ;; Return dispatch closure
          (lambda (msg &rest args)
            (cond
              ((eq msg 'get)
               (let ((fname (car args)))
                 (if (gethash fname readable)
                     (gethash fname state)
                   (error "Field %s is not readable" fname))))
              ((eq msg 'set)
               (let ((fname (car args))
                     (val (cadr args)))
                 (unless (gethash fname writable)
                   (error "Field %s is not writable" fname))
                 (let ((vfn (gethash fname validators)))
                   (when (and vfn (not (funcall vfn val)))
                     (error "Validation failed for %s" fname)))
                 (puthash fname val state)
                 val))
              ((eq msg 'fields)
               (let ((result nil))
                 (dolist (field fields)
                   (setq result (cons (car field) result)))
                 (nreverse result)))
              (t (error "Unknown message: %s" msg))))))))

  (unwind-protect
      (let ((Person (funcall 'neovm--os-make-class
                             '((name :read :write)
                               (age :read :write :validate integerp)
                               (id :read)))))     ;; id: read-only
        (let ((p1 (funcall Person "Alice" 30 1001))
              (p2 (funcall Person "Bob" 25 1002)))
          (list
            ;; Read fields
            (funcall p1 'get 'name)
            (funcall p1 'get 'age)
            (funcall p1 'get 'id)
            ;; Write name
            (funcall p1 'set 'name "Alicia")
            (funcall p1 'get 'name)
            ;; Write age with validation
            (funcall p1 'set 'age 31)
            (funcall p1 'get 'age)
            ;; Try writing id (read-only) -> error
            (condition-case err
                (funcall p1 'set 'id 9999)
              (error (error-message-string err)))
            ;; Try setting age to non-integer -> validation error
            (condition-case err
                (funcall p1 'set 'age "thirty")
              (error (error-message-string err)))
            ;; p2 is independent
            (funcall p2 'get 'name)
            (funcall p2 'get 'age)
            ;; Introspection
            (funcall p1 'fields))))
    (fmakunbound 'neovm--os-make-class)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 1001 \"Alicia\" \"Alicia\" 31 31 \"Field id is not writable\" \"Validation failed for age\" \"Bob\" 25 (name age id))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic virtual method dispatch with class hierarchy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_polymorphism_virtual_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Class registry with virtual method tables. Subclass methods override
    // parent methods. Dispatch looks up vtable for instance's class.
    let form = r#"(progn
  (defvar neovm--os-vtables (make-hash-table :test 'eq))

  (fset 'neovm--os-defclass
    (lambda (class-name parent-name methods)
      "Define a class with optional parent. Methods override parent's."
      (let ((vtable (make-hash-table :test 'eq)))
        ;; Copy parent methods first
        (when parent-name
          (let ((parent-vt (gethash parent-name neovm--os-vtables)))
            (when parent-vt
              (maphash (lambda (k v) (puthash k v vtable)) parent-vt))))
        ;; Add/override with own methods
        (dolist (m methods)
          (puthash (car m) (cdr m) vtable))
        (puthash class-name vtable neovm--os-vtables))))

  (fset 'neovm--os-make-instance
    (lambda (class-name &rest props)
      (cons class-name props)))

  (fset 'neovm--os-class-of
    (lambda (inst) (car inst)))

  (fset 'neovm--os-prop
    (lambda (inst key) (plist-get (cdr inst) key)))

  (fset 'neovm--os-dispatch
    (lambda (inst method &rest args)
      (let* ((cls (funcall 'neovm--os-class-of inst))
             (vtable (gethash cls neovm--os-vtables))
             (fn (and vtable (gethash method vtable))))
        (if fn
            (apply fn inst args)
          (error "No method %s for class %s" method cls)))))

  (unwind-protect
      (progn
        ;; Define Shape (base)
        (funcall 'neovm--os-defclass 'shape nil
                 (list (cons 'area (lambda (self) 0))
                       (cons 'describe
                             (lambda (self)
                               (format "%s with area %s"
                                       (symbol-name (funcall 'neovm--os-class-of self))
                                       (funcall 'neovm--os-dispatch self 'area))))))
        ;; Circle overrides area
        (funcall 'neovm--os-defclass 'circle 'shape
                 (list (cons 'area
                             (lambda (self)
                               (let ((r (funcall 'neovm--os-prop self :radius)))
                                 ;; Use integer approximation: pi ~ 314/100
                                 (/ (* 314 r r) 100))))))
        ;; Rectangle overrides area
        (funcall 'neovm--os-defclass 'rect 'shape
                 (list (cons 'area
                             (lambda (self)
                               (* (funcall 'neovm--os-prop self :width)
                                  (funcall 'neovm--os-prop self :height))))))
        ;; Square inherits from Rectangle, overrides nothing
        (funcall 'neovm--os-defclass 'square 'rect nil)

        (let ((c (funcall 'neovm--os-make-instance 'circle :radius 10))
              (r (funcall 'neovm--os-make-instance 'rect :width 5 :height 8))
              (s (funcall 'neovm--os-make-instance 'square :width 6 :height 6)))
          ;; Polymorphic dispatch
          (let ((shapes (list c r s)))
            (list
              ;; Areas
              (mapcar (lambda (sh) (funcall 'neovm--os-dispatch sh 'area)) shapes)
              ;; Descriptions (inherited from shape, calls area polymorphically)
              (mapcar (lambda (sh) (funcall 'neovm--os-dispatch sh 'describe)) shapes)
              ;; Class identification
              (mapcar (lambda (sh) (funcall 'neovm--os-class-of sh)) shapes)
              ;; Total area
              (let ((total 0))
                (dolist (sh shapes)
                  (setq total (+ total (funcall 'neovm--os-dispatch sh 'area))))
                total)))))
    (fmakunbound 'neovm--os-defclass)
    (fmakunbound 'neovm--os-make-instance)
    (fmakunbound 'neovm--os-class-of)
    (fmakunbound 'neovm--os-prop)
    (fmakunbound 'neovm--os-dispatch)
    (makunbound 'neovm--os-vtables)))"#;
    let expect = expect_test::expect![[
        r#""OK ((314 40 36) (\"circle with area 314\" \"rect with area 40\" \"square with area 36\") (circle rect square) 390)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Introspection: list methods, check responds-to, class hierarchy query
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an introspectable class system: each class knows its parent,
    // can list its own and inherited methods, and test responds-to.
    let form = r#"(progn
  (defvar neovm--os-class-registry (make-hash-table :test 'eq))

  (fset 'neovm--os-register-class
    (lambda (name parent method-names)
      (puthash name (list parent method-names) neovm--os-class-registry)))

  (fset 'neovm--os-class-parent
    (lambda (name)
      (car (gethash name neovm--os-class-registry))))

  (fset 'neovm--os-class-own-methods
    (lambda (name)
      (cadr (gethash name neovm--os-class-registry))))

  (fset 'neovm--os-class-all-methods
    (lambda (name)
      "Return all methods including inherited, with overrides resolved."
      (let ((methods nil)
            (seen (make-hash-table :test 'eq))
            (current name))
        (while current
          (let ((own (funcall 'neovm--os-class-own-methods current)))
            (dolist (m own)
              (unless (gethash m seen)
                (puthash m t seen)
                (setq methods (cons m methods)))))
          (setq current (funcall 'neovm--os-class-parent current)))
        (sort methods (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (fset 'neovm--os-class-responds-to
    (lambda (name method)
      (memq method (funcall 'neovm--os-class-all-methods name))))

  (fset 'neovm--os-class-ancestors
    (lambda (name)
      (let ((chain nil)
            (current name))
        (while current
          (setq chain (cons current chain))
          (setq current (funcall 'neovm--os-class-parent current)))
        (nreverse chain))))

  (fset 'neovm--os-class-is-a
    (lambda (name ancestor)
      (memq ancestor (funcall 'neovm--os-class-ancestors name))))

  (unwind-protect
      (progn
        (funcall 'neovm--os-register-class 'vehicle nil '(start stop describe))
        (funcall 'neovm--os-register-class 'car 'vehicle '(honk describe))
        (funcall 'neovm--os-register-class 'electric-car 'car '(charge battery-level))
        (funcall 'neovm--os-register-class 'truck 'vehicle '(load-cargo capacity))

        (list
          ;; Ancestor chains
          (funcall 'neovm--os-class-ancestors 'electric-car)
          (funcall 'neovm--os-class-ancestors 'truck)
          (funcall 'neovm--os-class-ancestors 'vehicle)
          ;; Own methods
          (funcall 'neovm--os-class-own-methods 'car)
          (funcall 'neovm--os-class-own-methods 'electric-car)
          ;; All methods (inherited + own)
          (funcall 'neovm--os-class-all-methods 'electric-car)
          (funcall 'neovm--os-class-all-methods 'truck)
          ;; Responds-to
          (if (funcall 'neovm--os-class-responds-to 'electric-car 'charge) t nil)
          (if (funcall 'neovm--os-class-responds-to 'electric-car 'start) t nil)
          (if (funcall 'neovm--os-class-responds-to 'electric-car 'honk) t nil)
          (if (funcall 'neovm--os-class-responds-to 'truck 'charge) t nil)
          (if (funcall 'neovm--os-class-responds-to 'truck 'load-cargo) t nil)
          ;; Is-a (subtype check)
          (if (funcall 'neovm--os-class-is-a 'electric-car 'vehicle) t nil)
          (if (funcall 'neovm--os-class-is-a 'electric-car 'car) t nil)
          (if (funcall 'neovm--os-class-is-a 'truck 'car) t nil)
          (if (funcall 'neovm--os-class-is-a 'vehicle 'electric-car) t nil)))
    (fmakunbound 'neovm--os-register-class)
    (fmakunbound 'neovm--os-class-parent)
    (fmakunbound 'neovm--os-class-own-methods)
    (fmakunbound 'neovm--os-class-all-methods)
    (fmakunbound 'neovm--os-class-responds-to)
    (fmakunbound 'neovm--os-class-ancestors)
    (fmakunbound 'neovm--os-class-is-a)
    (makunbound 'neovm--os-class-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK ((electric-car car vehicle) (truck vehicle) (vehicle) (honk describe) (charge battery-level) (battery-level charge describe honk start stop) (capacity describe load-cargo start stop) t t t nil t t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Factory pattern: create objects by type name with configuration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obj_sys_factory_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Factory registry maps type names to constructor functions.
    // Supports default configs, config merging, and singleton caching.
    let form = r#"(progn
  (defvar neovm--os-factory-registry (make-hash-table :test 'eq))
  (defvar neovm--os-factory-defaults (make-hash-table :test 'eq))
  (defvar neovm--os-factory-cache (make-hash-table :test 'equal))

  (fset 'neovm--os-factory-register
    (lambda (type-name constructor defaults)
      (puthash type-name constructor neovm--os-factory-registry)
      (puthash type-name defaults neovm--os-factory-defaults)))

  (fset 'neovm--os-factory-merge-config
    (lambda (defaults overrides)
      "Merge two plists, overrides win."
      (let ((result (copy-sequence defaults))
            (rest overrides))
        (while rest
          (let ((key (car rest))
                (val (cadr rest)))
            (setq result (plist-put result key val)))
          (setq rest (cddr rest)))
        result)))

  (fset 'neovm--os-factory-create
    (lambda (type-name &rest config)
      "Create instance of TYPE-NAME with CONFIG merged over defaults."
      (let ((constructor (gethash type-name neovm--os-factory-registry))
            (defaults (gethash type-name neovm--os-factory-defaults '())))
        (unless constructor
          (error "Unknown type: %s" type-name))
        (let ((merged (funcall 'neovm--os-factory-merge-config defaults config)))
          (funcall constructor merged)))))

  (fset 'neovm--os-factory-singleton
    (lambda (type-name &rest config)
      "Get or create a singleton for TYPE-NAME + CONFIG combination."
      (let ((key (cons type-name config)))
        (or (gethash key neovm--os-factory-cache)
            (let ((instance (apply 'neovm--os-factory-create type-name config)))
              (puthash key instance neovm--os-factory-cache)
              instance)))))

  (unwind-protect
      (progn
        ;; Register types
        (funcall 'neovm--os-factory-register 'logger
                 (lambda (config)
                   (let ((level (plist-get config :level))
                         (prefix (plist-get config :prefix))
                         (messages nil))
                     (lambda (msg &rest args)
                       (cond
                         ((eq msg 'log)
                          (let ((text (format "%s[%s] %s" prefix level (car args))))
                            (setq messages (cons text messages))
                            text))
                         ((eq msg 'messages) (nreverse messages))
                         ((eq msg 'level) level)))))
                 '(:level "INFO" :prefix ""))

        (funcall 'neovm--os-factory-register 'formatter
                 (lambda (config)
                   (let ((style (plist-get config :style))
                         (width (plist-get config :width)))
                     (lambda (msg &rest args)
                       (cond
                         ((eq msg 'format-text)
                          (let ((text (car args)))
                            (cond
                              ((eq style 'upper) (upcase text))
                              ((eq style 'lower) (downcase text))
                              ((eq style 'title)
                               (concat (upcase (substring text 0 1))
                                       (downcase (substring text 1))))
                              (t text))))
                         ((eq msg 'style) style)
                         ((eq msg 'width) width)))))
                 '(:style upper :width 80))

        ;; Create with defaults
        (let ((log1 (funcall 'neovm--os-factory-create 'logger))
              ;; Create with overrides
              (log2 (funcall 'neovm--os-factory-create 'logger :level "DEBUG" :prefix "APP:"))
              (fmt1 (funcall 'neovm--os-factory-create 'formatter))
              (fmt2 (funcall 'neovm--os-factory-create 'formatter :style 'title)))
          ;; Use loggers
          (funcall log1 'log "system started")
          (funcall log1 'log "processing")
          (funcall log2 'log "variable dump")
          (list
            (funcall log1 'level)
            (funcall log2 'level)
            (funcall log1 'messages)
            (funcall log2 'messages)
            ;; Formatters
            (funcall fmt1 'format-text "hello world")
            (funcall fmt2 'format-text "hello world")
            (funcall fmt1 'style)
            (funcall fmt2 'width)
            ;; Singleton test
            (let ((s1 (funcall 'neovm--os-factory-singleton 'logger :level "WARN"))
                  (s2 (funcall 'neovm--os-factory-singleton 'logger :level "WARN")))
              (eq s1 s2)))))
    (fmakunbound 'neovm--os-factory-register)
    (fmakunbound 'neovm--os-factory-merge-config)
    (fmakunbound 'neovm--os-factory-create)
    (fmakunbound 'neovm--os-factory-singleton)
    (makunbound 'neovm--os-factory-registry)
    (makunbound 'neovm--os-factory-defaults)
    (makunbound 'neovm--os-factory-cache)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"INFO\" \"DEBUG\" (\"[INFO] system started\" \"[INFO] processing\") (\"APP:[DEBUG] variable dump\") \"HELLO WORLD\" \"Hello world\" upper 80 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
