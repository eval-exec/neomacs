//! Deep stress: EIEIO method combinations + cl-call-next-method + undo + textprop.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_eieio_around_before_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-node nil\n\
         ((value :initarg :value :accessor node-value)\n\
         (children :initarg :children :initform nil :accessor node-children)))\n\
         (defclass test-leaf (test-node) nil)\n\
         (defclass test-branch (test-node) nil)\n\
         (cl-defgeneric node-render (n buf)\n\
         \"Render node into buffer.\")\n\
         (cl-defmethod node-render :before ((n test-node) buf)\n\
         (with-current-buffer buf\n\
         (insert \"[\")\n\
         (put-text-property (1- (point)) (point) 'bracket 'open)))\n\
         (cl-defmethod node-render :after ((n test-node) buf)\n\
         (with-current-buffer buf\n\
         (insert \"]\")\n\
         (put-text-property (1- (point)) (point) 'bracket 'close)))\n\
         (cl-defmethod node-render :around ((n test-node) buf)\n\
         (with-current-buffer buf\n\
         (let ((start (point)))\n\
         (cl-call-next-method)\n\
         (put-text-property start (point) 'rendered t))))\n\
         (cl-defmethod node-render ((n test-leaf) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"leaf:%d\" (node-value n)))))\n\
         (cl-defmethod node-render ((n test-branch) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"branch:%d\" (node-value n)))\n\
         (dolist (child (node-children n))\n\
         (insert \" \")\n\
         (node-render child buf))))\n\
         (let ((buf (generate-new-buffer \"eia\"))\n\
         (tree (test-branch :value 1\n\
         :children (list (test-leaf :value 2)\n\
         (test-branch :value 3\n\
         :children (list (test-leaf :value 4)\n\
         (test-leaf :value 5)))))))\n\
         (with-current-buffer buf\n\
         (node-render tree buf)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"MODIFIED\")\n\
         (put-text-property 10 18 'modified t)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (r1 (get-text-property 1 'rendered))\n\
         (m10 (get-text-property 10 'modified))\n\
         (b1 (get-text-property 1 'bracket)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s r1 m10 b1\n\
         (buffer-string)\n\
         (get-text-property 1 'rendered)\n\
         (get-text-property 10 'modified)\n\
         (get-text-property 1 'bracket)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_deep_inheritance_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass animal nil ((name :initarg :name :accessor animal-name)))\n\
         (defclass mammal (animal) ((fur :initarg :fur :accessor mammal-fur)))\n\
         (defclass dog (mammal) ((breed :initarg :breed :accessor dog-breed)))\n\
         (defclass cat (mammal) ((color :initarg :color :accessor cat-color)))\n\
         (cl-defgeneric describe-animal (a buf)\n\
         \"Describe animal.\")\n\
         (cl-defmethod describe-animal ((a animal) buf)\n\
         (with-current-buffer buf (insert (format \"%s\" (animal-name a)))))\n\
         (cl-defmethod describe-animal ((a mammal) buf)\n\
         (cl-call-next-method)\n\
         (with-current-buffer buf (insert (format \" fur=%s\" (mammal-fur a)))))\n\
         (cl-defmethod describe-animal ((a dog) buf)\n\
         (cl-call-next-method)\n\
         (with-current-buffer buf (insert (format \" breed=%s\" (dog-breed a)))))\n\
         (cl-defmethod describe-animal ((a cat) buf)\n\
         (cl-call-next-method)\n\
         (with-current-buffer buf (insert (format \" color=%s\" (cat-color a)))))\n\
         (let ((buf (generate-new-buffer \"eid\"))\n\
         (animals (list (dog :name 'buddy :fur 'golden :breed 'retriever)\n\
         (cat :name 'whiskers :fur 'gray :color 'tabby))))\n\
         (with-current-buffer buf\n\
         (dolist (a animals)\n\
         (describe-animal a buf)\n\
         (insert \"\\n\"))\n\
         (put-text-property 1 30 'type 'descriptions)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (length (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_slot_access_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-doc nil\n\
         ((title :initarg :title :accessor doc-title)\n\
         (body :initarg :body :accessor doc-body)\n\
         (version :initarg :version :accessor doc-version :initform 1)))\n\
         (let ((buf (generate-new-buffer \"esu\"))\n\
         (doc (test-doc :title \"Report\" :body \"Initial content\")))\n\
         (with-current-buffer buf\n\
         (insert (format \"Title: %s\\nVersion: %d\\nBody: %s\"\n\
         (doc-title doc) (doc-version doc) (doc-body doc)))\n\
         (put-text-property 1 7 'field 'title-label)\n\
         (put-text-property 8 14 'field 'title-value)\n\
         (put-text-property 15 23 'field 'version-label)\n\
         (put-text-property 24 25 'field 'version-value)\n\
         (put-text-property 26 31 'field 'body-label)\n\
         (put-text-property 32 47 'field 'body-value)\n\
         (undo-boundary)\n\
         (setf (doc-version doc) 2)\n\
         (setf (doc-body doc) \"Updated content\")\n\
         (erase-buffer)\n\
         (insert (format \"Title: %s\\nVersion: %d\\nBody: %s\"\n\
         (doc-title doc) (doc-version doc) (doc-body doc)))\n\
         (put-text-property 1 7 'field 'title-label)\n\
         (put-text-property 8 14 'field 'title-value)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'field))\n\
         (f8 (get-text-property 8 'field)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s f1 f8\n\
         (buffer-string)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 8 'field))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_polymorphic_sort_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass sortable-item nil\n\
         ((priority :initarg :priority :accessor item-priority)\n\
         (label :initarg :label :accessor item-label)))\n\
         (cl-defgeneric item-sort-key (item)\n\
         \"Return key for sorting.\")\n\
         (cl-defmethod item-sort-key ((item sortable-item))\n\
         (item-priority item))\n\
         (defclass named-item (sortable-item)\n\
         ((category :initarg :category :accessor item-category)))\n\
         (cl-defmethod item-sort-key ((item named-item))\n\
         (cons (item-category item) (item-priority item)))\n\
         (let* ((items (list (named-item :priority 3 :label 'c :category 'b)\n\
         (named-item :priority 1 :label 'a :category 'a)\n\
         (named-item :priority 2 :label 'b :category 'a)\n\
         (named-item :priority 5 :label 'e :category 'c)\n\
         (named-item :priority 4 :label 'd :category 'b))))\n\
         (let ((sorted (cl-sort (copy-sequence items)\n\
         (lambda (a b)\n\
         (let ((ka (item-sort-key a))\n\
         (kb (item-sort-key b)))\n\
         (if (and (consp ka) (consp kb))\n\
         (or (string< (symbol-name (car ka)) (symbol-name (car kb)))\n\
         (and (eq (car ka) (car kb))\n\
         (< (cdr ka) (cdr kb))))\n\
         (< ka kb)))))))\n\
         (list (mapcar #'item-label sorted)\n\
         (mapcar (lambda (i) (item-sort-key i)) sorted)\n\
         (= (length sorted) 5))))",
        expect,
    );
}

#[test]
fn deficiency_eieio_static_methods_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-factory nil\n\
         ((products :initarg :products :initform nil :accessor factory-products)))\n\
         (cl-defmethod factory-create ((f test-factory) type name)\n\
         (let ((product (list :type type :name name :id (length (factory-products f)))))\n\
         (push product (factory-products f))\n\
         product))\n\
         (cl-defmethod factory-summary ((f test-factory) buf)\n\
         (with-current-buffer buf\n\
         (dolist (p (nreverse (factory-products f)))\n\
         (insert (format \"[%s] %s (id=%d)\\n\"\n\
         (plist-get p :type)\n\
         (plist-get p :name)\n\
         (plist-get p :id))))))\n\
         (let ((buf (generate-new-buffer \"esm\"))\n\
         (factory (test-factory)))\n\
         (factory-create factory 'widget \"button\")\n\
         (factory-create factory 'widget \"slider\")\n\
         (factory-create factory 'gadget \"clock\")\n\
         (factory-create factory 'gadget \"timer\")\n\
         (with-current-buffer buf\n\
         (factory-summary factory buf)\n\
         (put-text-property 1 20 'section 'widgets)\n\
         (put-text-property 21 45 'section 'gadgets)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"=== \")\n\
         (put-text-property 1 5 'section 'header)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (s1 (get-text-property 1 'section))\n\
         (s5 (get-text-property 5 'section))\n\
         (s25 (get-text-property 25 'section)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s s1 s5 s25\n\
         (buffer-string)\n\
         (get-text-property 1 'section)\n\
         (get-text-property 20 'section)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_cl_defmethod_multiple_specializers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-shape nil\n\
         ((name :initarg :name :accessor shape-name)))\n\
         (defclass test-canvas nil\n\
         ((width :initarg :width :accessor canvas-width)\n\
         (height :initarg :height :accessor canvas-height)))\n\
         (cl-defgeneric draw-shape (shape canvas buf)\n\
         \"Draw shape on canvas.\")\n\
         (cl-defmethod draw-shape ((s test-shape) (c test-canvas) buf)\n\
         (with-current-buffer buf\n\
         (insert (format \"%s on %dx%d\"\n\
         (shape-name s) (canvas-width c) (canvas-height c)))))\n\
         (let ((buf (generate-new-buffer \"ems\"))\n\
         (canvas (test-canvas :width 800 :height 600))\n\
         (shapes (list (test-shape :name 'circle)\n\
         (test-shape :name 'square)\n\
         (test-shape :name 'triangle))))\n\
         (with-current-buffer buf\n\
         (dolist (s shapes)\n\
         (draw-shape s canvas buf)\n\
         (insert \"\\n\"))\n\
         (put-text-property 1 20 'layer 'shapes)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"[MOD]\")\n\
         (put-text-property 10 15 'modified t)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'layer))\n\
         (m10 (get-text-property 10 'modified)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s l1 m10\n\
         (buffer-string)\n\
         (get-text-property 1 'layer)\n\
         (get-text-property 10 'modified)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_constructor_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-range nil\n\
         ((start :initarg :start :accessor range-start)\n\
         (end :initarg :end :accessor range-end)\n\
         (label :initarg :label :accessor range-label)))\n\
         (cl-defmethod initialize-instance :after ((r test-range) &rest _)\n\
         (when (> (range-start r) (range-end r))\n\
         (let ((tmp (range-start r)))\n\
         (setf (range-start r) (range-end r))\n\
         (setf (range-end r) tmp))))\n\
         (let ((buf (generate-new-buffer \"ecv\"))\n\
         (ranges (list (test-range :start 10 :end 5 :label 'swapped)\n\
         (test-range :start 1 :end 10 :label 'normal)\n\
         (test-range :start 100 :end 50 :label 'big-swap))))\n\
         (with-current-buffer buf\n\
         (dolist (r ranges)\n\
         (insert (format \"%s: %d-%d\\n\"\n\
         (range-label r) (range-start r) (range-end r))))\n\
         (put-text-property 1 20 'type 'ranges)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (= (range-start (car ranges)) 5)\n\
         (= (range-end (car ranges)) 10)\n\
         (= (range-start (caddr ranges)) 50)\n\
         (= (range-end (caddr ranges)) 100))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_object_assoc_list_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-entry nil\n\
         ((key :initarg :key :accessor entry-key)\n\
         (value :initarg :value :accessor entry-value)\n\
         (tags :initarg :tags :initform nil :accessor entry-tags)))\n\
         (let ((buf (generate-new-buffer \"eoa\"))\n\
         (entries (list (test-entry :key 'name :value \"Alice\" :tags '(person user))\n\
         (test-entry :key 'age :value 30 :tags '(person number))\n\
         (test-entry :key 'role :value 'admin :tags '(system access))\n\
         (test-entry :key 'status :value 'active :tags '(system state)))))\n\
         (with-current-buffer buf\n\
         (dolist (e entries)\n\
         (insert (format \"%s = %S  tags: %s\\n\"\n\
         (entry-key e) (entry-value e) (entry-tags e))))\n\
         (put-text-property 1 30 'section 'personal)\n\
         (put-text-property 31 70 'section 'system)\n\
         (undo-boundary)\n\
         (let ((all-tags (cl-reduce #'append (mapcar #'entry-tags entries))))\n\
         (goto-char 1)\n\
         (insert (format \"// %d total tags\\n\" (length all-tags))))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (sec1 (get-text-property 1 'section)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s sec1\n\
         (buffer-string)\n\
         (get-text-property 1 'section)\n\
         (get-text-property 30 'section)\n\
         (get-text-property 31 'section))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_with_cl_print_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-print)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-point nil\n\
         ((x :initarg :x :accessor point-x)\n\
         (y :initarg :y :accessor point-y))\n\
         (:default-initargs :x 0 :y 0))\n\
         (cl-defmethod cl-print-object ((p test-point) stream)\n\
         (princ (format \"<point %d,%d>\" (point-x p) (point-y p)) stream))\n\
         (let ((buf (generate-new-buffer \"epo\"))\n\
         (points (list (test-point :x 1 :y 2)\n\
         (test-point :x 3 :y 4)\n\
         (test-point :x 5 :y 6))))\n\
         (with-current-buffer buf\n\
         (dolist (p points)\n\
         (cl-print p buf)\n\
         (insert \" \"))\n\
         (put-text-property 1 12 'type 'points)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"CENTER\")\n\
         (put-text-property 5 11 'type 'center)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (get-text-property 1 'type))\n\
         (t5 (get-text-property 5 'type)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s t1 t5\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 5 'type)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_eieio_composition_deep_tree_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defclass test-component nil\n\
         ((name :initarg :name :accessor comp-name)\n\
         (children :initarg :children :initform nil :accessor comp-children)\n\
         (props :initarg :props :initform nil :accessor comp-props)))\n\
         (cl-defgeneric comp-render (c buf indent)\n\
         \"Render component tree.\")\n\
         (cl-defmethod comp-render ((c test-component) buf indent)\n\
         (with-current-buffer buf\n\
         (insert (make-string (* 2 indent) ? ))\n\
         (insert (format \"<%s\" (comp-name c)))\n\
         (dolist (p (comp-props c))\n\
         (insert (format \" %s=%s\" (car p) (cdr p))))\n\
         (insert \">\\n\")\n\
         (dolist (child (comp-children c))\n\
         (comp-render child buf (1+ indent)))\n\
         (insert (make-string (* 2 indent) ? ))\n\
         (insert (format \"</%s>\\n\" (comp-name c)))))\n\
         (let* ((buf (generate-new-buffer \"ecd\"))\n\
         (leaf1 (test-component :name 'span :props '((class . \"text\"))))\n\
         (leaf2 (test-component :name 'span :props '((class . \"bold\"))))\n\
         (inner (test-component :name 'div :props '((id . \"content\"))\n\
         :children (list leaf1 leaf2)))\n\
         (root (test-component :name 'div :props '((id . \"root\"))\n\
         :children (list inner))))\n\
         (with-current-buffer buf\n\
         (comp-render root buf 0)\n\
         (put-text-property 1 20 'layer 'root)\n\
         (put-text-property 20 60 'layer 'inner)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"STYLE\")\n\
         (put-text-property 5 10 'added 'style)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'layer))\n\
         (l20 (get-text-property 20 'layer))\n\
         (a5 (get-text-property 5 'added)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s l1 l20 a5\n\
         (buffer-string)\n\
         (get-text-property 1 'layer)\n\
         (get-text-property 20 'layer)\n\
         (get-text-property 5 'added)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
