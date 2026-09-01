//! Oracle parity tests for garbage collector simulation in pure Elisp.
//!
//! Simulates mark-sweep, reference counting with cycle detection,
//! copying/semi-space collector, generational GC, root set management,
//! heap allocation with compaction, weak references, and finalization queues.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Mark-sweep garbage collector simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_mark_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate mark-sweep: allocate objects on a heap (vector), maintain
    // a root set, mark reachable objects, sweep unreachable ones.
    let form = r#"(progn
  ;; Heap: vector of (id . refs) pairs, or nil for free slots
  ;; refs = list of heap indices this object references
  (fset 'neovm--gc-ms-alloc
    (lambda (heap id refs)
      "Allocate object ID with REFS in first free slot. Return index or nil."
      (let ((i 0) (len (length heap)) (found nil))
        (while (and (< i len) (not found))
          (if (null (aref heap i))
              (progn (aset heap i (cons id refs)) (setq found i))
            (setq i (1+ i))))
        found)))

  (fset 'neovm--gc-ms-mark
    (lambda (heap roots)
      "Mark reachable objects starting from ROOTS (list of heap indices).
       Return a bool-vector of marked slots."
      (let ((marked (make-vector (length heap) nil))
            (worklist (copy-sequence roots)))
        (dolist (r worklist)
          (aset marked r t))
        (while worklist
          (let* ((idx (pop worklist))
                 (obj (aref heap idx)))
            (when obj
              (dolist (ref (cdr obj))
                (when (and (< ref (length heap))
                           (aref heap ref)
                           (not (aref marked ref)))
                  (aset marked ref t)
                  (setq worklist (cons ref worklist)))))))
        marked)))

  (fset 'neovm--gc-ms-sweep
    (lambda (heap marked)
      "Free unmarked slots. Return count of freed objects."
      (let ((freed 0) (i 0) (len (length heap)))
        (while (< i len)
          (when (and (aref heap i) (not (aref marked i)))
            (aset heap i nil)
            (setq freed (1+ freed)))
          (setq i (1+ i)))
        freed)))

  (unwind-protect
      (let ((heap (make-vector 10 nil)))
        ;; Allocate objects with references:
        ;; 0: A -> [1, 2]
        ;; 1: B -> [3]
        ;; 2: C -> []
        ;; 3: D -> [2]   (D refs C, creating shared reference)
        ;; 4: E -> [5]
        ;; 5: F -> []    (E->F, both unreachable from root {0})
        ;; 6: G -> []    (isolated, unreachable)
        (funcall 'neovm--gc-ms-alloc heap 'A '(1 2))
        (funcall 'neovm--gc-ms-alloc heap 'B '(3))
        (funcall 'neovm--gc-ms-alloc heap 'C '())
        (funcall 'neovm--gc-ms-alloc heap 'D '(2))
        (funcall 'neovm--gc-ms-alloc heap 'E '(5))
        (funcall 'neovm--gc-ms-alloc heap 'F '())
        (funcall 'neovm--gc-ms-alloc heap 'G '())
        (let* ((roots '(0))  ;; root set = {A}
               (marked (funcall 'neovm--gc-ms-mark heap roots))
               (pre-snapshot (let ((s nil) (i 0))
                               (while (< i (length heap))
                                 (when (aref heap i)
                                   (push (car (aref heap i)) s))
                                 (setq i (1+ i)))
                               (nreverse s)))
               (freed (funcall 'neovm--gc-ms-sweep heap marked))
               (post-snapshot (let ((s nil) (i 0))
                                (while (< i (length heap))
                                  (when (aref heap i)
                                    (push (car (aref heap i)) s))
                                  (setq i (1+ i)))
                                (nreverse s))))
          (list
            pre-snapshot      ;; (A B C D E F G)
            freed             ;; 3 (E, F, G freed)
            post-snapshot     ;; (A B C D)
            ;; marked vector state
            (let ((m nil) (i 0))
              (while (< i (length marked))
                (push (aref marked i) m)
                (setq i (1+ i)))
              (nreverse m)))))
    (fmakunbound 'neovm--gc-ms-alloc)
    (fmakunbound 'neovm--gc-ms-mark)
    (fmakunbound 'neovm--gc-ms-sweep)))"#;
    let expect = expect_test::expect![[
        r#""OK ((A B C D E F G) 3 (A B C D) (t t t t nil nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reference counting with cycle detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_refcount_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate reference counting: each object has a refcount.
    // Decrement on unlink, free when 0. Then detect cycles via
    // trial-deletion (decrement counts for internal refs, check for zeros).
    let form = r#"(progn
  ;; Object table: hash of id -> (refcount . list-of-refs)
  (fset 'neovm--gc-rc-new
    (lambda (table id)
      (puthash id (cons 0 nil) table)))

  (fset 'neovm--gc-rc-addref
    (lambda (table from to)
      "FROM references TO: increment TO's refcount, add to FROM's refs."
      (let ((to-obj (gethash to table))
            (from-obj (gethash from table)))
        (when (and to-obj from-obj)
          (setcar to-obj (1+ (car to-obj)))
          (setcdr from-obj (cons to (cdr from-obj)))))))

  (fset 'neovm--gc-rc-release
    (lambda (table from to)
      "FROM drops reference to TO: decrement TO's refcount."
      (let ((to-obj (gethash to table)))
        (when to-obj
          (setcar to-obj (max 0 (1- (car to-obj))))))))

  (fset 'neovm--gc-rc-detect-cycles
    (lambda (table ids)
      "Trial deletion: temporarily decrement refcounts for internal refs.
       Objects still at 0 after are part of a garbage cycle."
      (let ((trial (make-hash-table :test 'eq)))
        ;; Copy refcounts
        (dolist (id ids)
          (let ((obj (gethash id table)))
            (when obj (puthash id (car obj) trial))))
        ;; Trial decrement: for each internal ref, decrement target's trial count
        (dolist (id ids)
          (let ((obj (gethash id table)))
            (when obj
              (dolist (ref (cdr obj))
                (when (gethash ref trial)
                  (puthash ref (1- (gethash ref trial)) trial))))))
        ;; Collect objects whose trial count dropped to 0 = garbage cycle
        (let ((garbage nil))
          (dolist (id ids)
            (when (and (gethash id trial) (<= (gethash id trial) 0))
              (push id garbage)))
          (nreverse garbage)))))

  (unwind-protect
      (let ((table (make-hash-table :test 'eq)))
        ;; Create objects
        (dolist (id '(a b c d e))
          (funcall 'neovm--gc-rc-new table id))
        ;; External root holds 'a
        (let ((a-obj (gethash 'a table)))
          (setcar a-obj (1+ (car a-obj))))  ;; external ref
        ;; Build references: a->b, b->c, c->b (cycle!), a->d, d->e
        (funcall 'neovm--gc-rc-addref table 'a 'b)
        (funcall 'neovm--gc-rc-addref table 'b 'c)
        (funcall 'neovm--gc-rc-addref table 'c 'b)  ;; creates cycle b<->c
        (funcall 'neovm--gc-rc-addref table 'a 'd)
        (funcall 'neovm--gc-rc-addref table 'd 'e)
        ;; Snapshot refcounts
        (let ((refcounts (mapcar (lambda (id) (cons id (car (gethash id table))))
                                  '(a b c d e))))
          ;; Drop a's reference to b (simulating a->b unlink)
          (funcall 'neovm--gc-rc-release table 'a 'b)
          ;; Now b has refcount 1 (from c), c has refcount 1 (from b)
          ;; Neither can be freed by simple refcounting -- it's a cycle!
          ;; Detect cycles among b, c (the suspected garbage)
          (let ((cycles (funcall 'neovm--gc-rc-detect-cycles
                                  table '(b c d e))))
            (list refcounts
                  (car (gethash 'b table))  ;; b's current refcount
                  (car (gethash 'c table))  ;; c's current refcount
                  cycles))))
    (fmakunbound 'neovm--gc-rc-new)
    (fmakunbound 'neovm--gc-rc-addref)
    (fmakunbound 'neovm--gc-rc-release)
    (fmakunbound 'neovm--gc-rc-detect-cycles)))"#;
    let expect =
        expect_test::expect![[r#""OK (((a . 1) (b . 2) (c . 1) (d . 1) (e . 1)) 1 1 (b c e))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copying / semi-space collector simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_semispace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Semi-space collector: two halves (from-space, to-space).
    // Copy reachable objects from from-space to to-space, updating refs.
    let form = r#"(progn
  (fset 'neovm--gc-ss-collect
    (lambda (from-space roots)
      "Copy reachable objects to a new space. Return (to-space . forwarding-table)."
      (let ((to-space (make-vector (length from-space) nil))
            (forwarding (make-hash-table :test 'eql))
            (to-ptr 0)
            (scan-ptr 0))
        ;; Copy root objects first
        (dolist (r roots)
          (when (and (aref from-space r) (not (gethash r forwarding)))
            (aset to-space to-ptr (copy-sequence (aref from-space r)))
            (puthash r to-ptr forwarding)
            (setq to-ptr (1+ to-ptr))))
        ;; Cheney's scan: process to-space objects, copy their refs
        (while (< scan-ptr to-ptr)
          (let* ((obj (aref to-space scan-ptr))
                 (refs (cdr obj))
                 (new-refs nil))
            (dolist (ref refs)
              (if (gethash ref forwarding)
                  (push (gethash ref forwarding) new-refs)
                ;; Copy referenced object
                (when (aref from-space ref)
                  (aset to-space to-ptr (copy-sequence (aref from-space ref)))
                  (puthash ref to-ptr forwarding)
                  (push to-ptr new-refs)
                  (setq to-ptr (1+ to-ptr)))))
            ;; Update refs in to-space object
            (setcdr (aref to-space scan-ptr) (nreverse new-refs)))
          (setq scan-ptr (1+ scan-ptr)))
        (cons to-space forwarding))))

  (unwind-protect
      (let ((from (make-vector 8 nil)))
        ;; Allocate: slot -> (id . refs-as-indices)
        (aset from 0 (cons 'root '(1 2)))
        (aset from 1 (cons 'child-a '(3)))
        (aset from 2 (cons 'child-b '()))
        (aset from 3 (cons 'grandchild '(2)))
        (aset from 4 (cons 'garbage-1 '(5)))
        (aset from 5 (cons 'garbage-2 '()))
        (aset from 6 (cons 'garbage-3 '()))
        (let* ((result (funcall 'neovm--gc-ss-collect from '(0)))
               (to (car result))
               (fwd (cdr result)))
          ;; Collect live objects from to-space
          (let ((live nil) (i 0))
            (while (< i (length to))
              (when (aref to i)
                (push (car (aref to i)) live))
              (setq i (1+ i)))
            (list
              ;; Live objects (should be root, child-a, child-b, grandchild)
              (sort (nreverse live)
                    (lambda (a b) (string< (symbol-name a) (symbol-name b))))
              ;; Forwarding table size = number of copied objects
              (hash-table-count fwd)
              ;; Garbage was NOT copied
              (let ((has-garbage nil) (j 0))
                (while (< j (length to))
                  (when (and (aref to j)
                             (memq (car (aref to j)) '(garbage-1 garbage-2 garbage-3)))
                    (setq has-garbage t))
                  (setq j (1+ j)))
                has-garbage)))))
    (fmakunbound 'neovm--gc-ss-collect)))"#;
    let expect = expect_test::expect![[r#""OK ((child-a child-b grandchild root) 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generational GC simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_generational() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-generation GC: young gen collected frequently, old gen rarely.
    // Objects surviving N young collections get promoted.
    let form = r#"(progn
  (fset 'neovm--gc-gen-create
    (lambda ()
      "Create GC state: (young-gen old-gen remembered-set promote-threshold)"
      (list nil nil nil 2)))  ;; promote after 2 survivals

  (fset 'neovm--gc-gen-alloc
    (lambda (state id refs)
      "Allocate in young generation."
      (let ((young (nth 0 state)))
        (setcar state (cons (list id refs 0) young)))))  ;; age = 0

  (fset 'neovm--gc-gen-minor-gc
    (lambda (state roots)
      "Collect young generation. ROOTS = list of live object ids."
      (let ((young (nth 0 state))
            (old (nth 1 state))
            (threshold (nth 3 state))
            (survivors nil)
            (promoted nil)
            (freed nil))
        (dolist (obj young)
          (let ((id (nth 0 obj))
                (refs (nth 1 obj))
                (age (nth 2 obj)))
            (if (memq id roots)
                (let ((new-age (1+ age)))
                  (if (>= new-age threshold)
                      ;; Promote to old gen
                      (progn
                        (setcar (cdr state) (cons (list id refs new-age) old))
                        (setq old (cons (list id refs new-age) old))
                        (push id promoted))
                    ;; Survive in young gen
                    (push (list id refs new-age) survivors)))
              (push id freed))))
        (setcar state survivors)
        (list 'freed freed 'promoted promoted))))

  (unwind-protect
      (let ((gc (funcall 'neovm--gc-gen-create)))
        ;; Allocate objects
        (funcall 'neovm--gc-gen-alloc gc 'obj-a nil)
        (funcall 'neovm--gc-gen-alloc gc 'obj-b nil)
        (funcall 'neovm--gc-gen-alloc gc 'obj-c nil)
        (funcall 'neovm--gc-gen-alloc gc 'temp-1 nil)
        (funcall 'neovm--gc-gen-alloc gc 'temp-2 nil)
        ;; Round 1: a, b, c survive; temp-1, temp-2 freed
        (let ((r1 (funcall 'neovm--gc-gen-minor-gc gc '(obj-a obj-b obj-c))))
          ;; Round 2: only a, b survive
          (let ((r2 (funcall 'neovm--gc-gen-minor-gc gc '(obj-a obj-b))))
            ;; a and b should now be promoted (age >= 2)
            ;; Allocate more in young gen
            (funcall 'neovm--gc-gen-alloc gc 'new-1 nil)
            ;; Round 3: only new-1 survives in young
            (let ((r3 (funcall 'neovm--gc-gen-minor-gc gc '(new-1 obj-a obj-b))))
              (let ((young-ids (mapcar #'car (nth 0 gc)))
                    (old-ids (mapcar #'car (nth 1 gc))))
                (list r1 r2 r3
                      (sort young-ids (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                      (sort old-ids (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))))
    (fmakunbound 'neovm--gc-gen-create)
    (fmakunbound 'neovm--gc-gen-alloc)
    (fmakunbound 'neovm--gc-gen-minor-gc)))"#;
    let expect = expect_test::expect![[
        r#""OK ((freed (temp-1 temp-2) promoted nil) (freed (obj-c) promoted (obj-b obj-a)) (freed nil promoted nil) (new-1) (obj-a obj-b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Root set management
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_root_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manage a root set with push/pop frames (like stack frames in a real GC).
    // Each frame owns some roots; popping a frame removes those roots.
    let form = r#"(progn
  (fset 'neovm--gc-root-create
    (lambda () (list nil)))  ;; stack of frames, each frame = list of root ids

  (fset 'neovm--gc-root-push-frame
    (lambda (rs) (setcar rs (cons nil (car rs)))))

  (fset 'neovm--gc-root-pop-frame
    (lambda (rs)
      (let ((removed (caar rs)))
        (setcar rs (cdar rs))
        removed)))

  (fset 'neovm--gc-root-add
    (lambda (rs id)
      (setcar (car rs) (cons id (caar rs)))))

  (fset 'neovm--gc-root-all
    (lambda (rs)
      "Collect all roots across all frames."
      (let ((result nil))
        (dolist (frame (car rs))
          (dolist (id frame)
            (unless (memq id result)
              (push id result))))
        (nreverse result))))

  (unwind-protect
      (let ((rs (funcall 'neovm--gc-root-create)))
        ;; Frame 0 (global)
        (funcall 'neovm--gc-root-push-frame rs)
        (funcall 'neovm--gc-root-add rs 'global-a)
        (funcall 'neovm--gc-root-add rs 'global-b)
        (let ((s0 (funcall 'neovm--gc-root-all rs)))
          ;; Frame 1 (function call)
          (funcall 'neovm--gc-root-push-frame rs)
          (funcall 'neovm--gc-root-add rs 'local-x)
          (funcall 'neovm--gc-root-add rs 'local-y)
          (let ((s1 (funcall 'neovm--gc-root-all rs)))
            ;; Frame 2 (nested call)
            (funcall 'neovm--gc-root-push-frame rs)
            (funcall 'neovm--gc-root-add rs 'inner-z)
            (let ((s2 (funcall 'neovm--gc-root-all rs)))
              ;; Pop frame 2
              (let ((removed2 (funcall 'neovm--gc-root-pop-frame rs))
                    (s3 (funcall 'neovm--gc-root-all rs)))
                ;; Pop frame 1
                (let ((removed1 (funcall 'neovm--gc-root-pop-frame rs))
                      (s4 (funcall 'neovm--gc-root-all rs)))
                  (list
                    (sort s0 (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                    (sort s1 (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                    (sort s2 (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                    removed2
                    (sort s3 (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                    removed1
                    (sort s4 (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))))))
    (fmakunbound 'neovm--gc-root-create)
    (fmakunbound 'neovm--gc-root-push-frame)
    (fmakunbound 'neovm--gc-root-pop-frame)
    (fmakunbound 'neovm--gc-root-add)
    (fmakunbound 'neovm--gc-root-all)))"#;
    let expect = expect_test::expect![[
        r#""OK ((global-a global-b) (global-a global-b local-x local-y) (global-a global-b inner-z local-x local-y) (inner-z) (global-a global-b local-x local-y) (local-y local-x) (global-a global-b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Heap allocation with compaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_compaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After mark-sweep leaves holes, compact the heap by sliding live objects
    // to the front, then update all references via a forwarding table.
    let form = r#"(progn
  (fset 'neovm--gc-compact
    (lambda (heap)
      "Compact heap: slide live objects to front. Return (compacted . forwarding-map)."
      (let ((fwd (make-hash-table :test 'eql))
            (dest 0)
            (len (length heap))
            (compacted (make-vector (length heap) nil))
            (i 0))
        ;; First pass: compute forwarding addresses
        (while (< i len)
          (when (aref heap i)
            (puthash i dest fwd)
            (setq dest (1+ dest)))
          (setq i (1+ i)))
        ;; Second pass: copy and update references
        (maphash
          (lambda (old-idx new-idx)
            (let ((obj (aref heap old-idx)))
              (aset compacted new-idx
                    (cons (car obj)
                          (mapcar (lambda (ref)
                                    (or (gethash ref fwd) ref))
                                  (cdr obj))))))
          fwd)
        (cons compacted fwd))))

  (unwind-protect
      (let ((heap (make-vector 10 nil)))
        ;; Scattered live objects with gaps
        (aset heap 0 (cons 'A '(3 7)))
        ;; 1 = free
        ;; 2 = free
        (aset heap 3 (cons 'B '(7)))
        ;; 4,5,6 = free
        (aset heap 7 (cons 'C '(0)))
        ;; 8,9 = free
        (let* ((result (funcall 'neovm--gc-compact heap))
               (compacted (car result))
               (fwd (cdr result)))
          ;; After compaction: A, B, C should be at indices 0, 1, 2
          ;; with updated refs
          (list
            ;; Compacted objects
            (aref compacted 0)  ;; A with updated refs
            (aref compacted 1)  ;; B with updated refs
            (aref compacted 2)  ;; C with updated refs
            (aref compacted 3)  ;; nil (free)
            ;; Forwarding: 0->0, 3->1, 7->2
            (gethash 0 fwd) (gethash 3 fwd) (gethash 7 fwd)
            ;; Verify A's refs are updated: old (3,7) -> new (1,2)
            (cdr (aref compacted 0))
            ;; Verify C's refs updated: old (0) -> new (0)
            (cdr (aref compacted 2)))))
    (fmakunbound 'neovm--gc-compact)))"#;
    let expect = expect_test::expect![[r#""OK ((A 1 2) (B 2) (C 0) nil 0 1 2 (1 2) (0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Weak references and finalization queue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_gc_sim_weak_refs_finalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate weak references: they don't prevent collection.
    // When the referent is collected, the weak ref becomes nil
    // and the object's finalizer is enqueued.
    let form = r#"(progn
  ;; Weak ref: (weak-ref id . alive?)
  ;; Finalization queue: list of finalized object ids
  (fset 'neovm--gc-weak-collect
    (lambda (objects strong-roots weak-refs)
      "Collect dead objects, clear their weak refs, enqueue finalizers.
       OBJECTS: alist of (id . has-finalizer?)
       STRONG-ROOTS: list of live ids
       WEAK-REFS: alist of (ref-name . target-id)
       Returns (cleared-weak-refs finalization-queue surviving-ids)."
      (let ((live (make-hash-table :test 'eq))
            (finalize-queue nil)
            (cleared nil)
            (surviving nil))
        ;; Mark live objects
        (dolist (root strong-roots)
          (puthash root t live))
        ;; Process each object
        (dolist (obj objects)
          (let ((id (car obj))
                (has-finalizer (cdr obj)))
            (if (gethash id live)
                (push id surviving)
              ;; Dead: check for finalizer
              (when has-finalizer
                (push id finalize-queue)))))
        ;; Clear weak refs to dead objects
        (dolist (wr weak-refs)
          (let ((ref-name (car wr))
                (target (cdr wr)))
            (if (gethash target live)
                (push (cons ref-name target) cleared)
              (push (cons ref-name nil) cleared))))
        (list
          (nreverse cleared)
          (nreverse finalize-queue)
          (sort surviving (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))

  (unwind-protect
      (let ((objects '((obj-a . t)     ;; has finalizer
                       (obj-b . nil)   ;; no finalizer
                       (obj-c . t)     ;; has finalizer
                       (obj-d . t)     ;; has finalizer
                       (obj-e . nil))) ;; no finalizer
            (strong-roots '(obj-a obj-c))
            (weak-refs '((wr-1 . obj-a)
                         (wr-2 . obj-b)
                         (wr-3 . obj-d)
                         (wr-4 . obj-e)
                         (wr-5 . obj-c))))
        (let ((result (funcall 'neovm--gc-weak-collect
                                objects strong-roots weak-refs)))
          (list
            ;; Cleared weak refs: wr-1 and wr-5 still point to live objs,
            ;; wr-2, wr-3, wr-4 cleared to nil
            (nth 0 result)
            ;; Finalization queue: obj-d (dead with finalizer)
            ;; obj-b and obj-e are dead but obj-b has no finalizer, obj-e has no finalizer
            (nth 1 result)
            ;; Surviving objects
            (nth 2 result)
            ;; Verify weak ref cleared count
            (let ((cleared-count 0))
              (dolist (wr (nth 0 result))
                (when (null (cdr wr))
                  (setq cleared-count (1+ cleared-count))))
              cleared-count))))
    (fmakunbound 'neovm--gc-weak-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK (((wr-1 . obj-a) (wr-2) (wr-3) (wr-4) (wr-5 . obj-c)) (obj-d) (obj-a obj-c) 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
