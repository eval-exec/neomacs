//! Oracle parity tests for memory allocator simulations in Elisp.
//!
//! Implements buddy allocator (split/merge), slab allocator with size classes,
//! arena allocator (bump pointer), free list management (first-fit, best-fit,
//! next-fit), memory fragmentation analysis, coalescing adjacent free blocks,
//! and allocation statistics tracking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Buddy allocator: power-of-2 split and merge
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_buddy_allocator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Buddy allocator state: (total-size free-blocks allocated)
  ;; free-blocks: alist of (size . list-of-offsets)
  ;; allocated: alist of (id . (offset . size))
  (defvar neovm--buddy-state nil)

  (fset 'neovm--buddy-create
    (lambda (total-size)
      "Create a buddy allocator with TOTAL-SIZE (must be power of 2)."
      (list total-size
            (list (cons total-size (list 0)))
            nil
            0)))

  (fset 'neovm--buddy-next-pow2
    (lambda (n)
      "Return smallest power of 2 >= N."
      (let ((p 1))
        (while (< p n)
          (setq p (* p 2)))
        p)))

  (fset 'neovm--buddy-alloc
    (lambda (state req-size id)
      "Allocate REQ-SIZE bytes from buddy allocator STATE. Returns updated state."
      (let* ((total (nth 0 state))
             (free-blocks (nth 1 state))
             (allocated (nth 2 state))
             (alloc-count (nth 3 state))
             (size (funcall 'neovm--buddy-next-pow2 (max req-size 1)))
             (found nil)
             (found-size nil))
        ;; Find smallest available block >= size
        (let ((try-size size))
          (while (and (<= try-size total) (not found))
            (let ((entry (assq try-size free-blocks)))
              (if (and entry (cdr entry))
                  (progn
                    (setq found (car (cdr entry)))
                    (setq found-size try-size)
                    ;; Remove block from free list
                    (setcdr entry (cdr (cdr entry))))
                (setq try-size (* try-size 2)))))
        ;; Split down to requested size
        (when found
          (while (> found-size size)
            (setq found-size (/ found-size 2))
            (let ((buddy-offset (+ found found-size))
                  (entry (assq found-size free-blocks)))
              (if entry
                  (setcdr entry (cons buddy-offset (cdr entry)))
                (setq free-blocks (cons (cons found-size (list buddy-offset))
                                        free-blocks)))))
          (setq allocated (cons (cons id (cons found size)) allocated))
          (setq alloc-count (1+ alloc-count)))
        (list total free-blocks allocated alloc-count)))))

  (fset 'neovm--buddy-free
    (lambda (state id)
      "Free allocation ID from buddy allocator STATE."
      (let* ((total (nth 0 state))
             (free-blocks (nth 1 state))
             (allocated (nth 2 state))
             (alloc-count (nth 3 state))
             (alloc-entry (assq id allocated)))
        (when alloc-entry
          (let ((offset (cadr alloc-entry))
                (size (cddr alloc-entry)))
            ;; Remove from allocated
            (setq allocated (assq-delete-all id allocated))
            ;; Add back to free list
            (let ((entry (assq size free-blocks)))
              (if entry
                  (setcdr entry (cons offset (cdr entry)))
                (setq free-blocks (cons (cons size (list offset)) free-blocks))))))
        (list total free-blocks allocated alloc-count))))

  (unwind-protect
      (let* ((state (funcall 'neovm--buddy-create 256))
             ;; Allocate several blocks
             (state (funcall 'neovm--buddy-alloc state 30 'a))   ;; -> 32 bytes
             (state (funcall 'neovm--buddy-alloc state 50 'b))   ;; -> 64 bytes
             (state (funcall 'neovm--buddy-alloc state 10 'c))   ;; -> 16 bytes
             (state (funcall 'neovm--buddy-alloc state 1 'd))    ;; -> 1 byte
             ;; Check allocations
             (allocs (nth 2 state))
             (a-info (cdr (assq 'a allocs)))
             (b-info (cdr (assq 'b allocs)))
             (c-info (cdr (assq 'c allocs)))
             (d-info (cdr (assq 'd allocs)))
             ;; Free block b and c
             (state (funcall 'neovm--buddy-free state 'b))
             (state (funcall 'neovm--buddy-free state 'c))
             (after-free-allocs (nth 2 state))
             ;; Re-allocate
             (state (funcall 'neovm--buddy-alloc state 20 'e))   ;; -> 32 bytes
             (final-allocs (nth 2 state)))
        (list
         :a-size (cdr a-info)
         :b-size (cdr b-info)
         :c-size (cdr c-info)
         :d-size (cdr d-info)
         :alloc-count (nth 3 state)
         :after-free-count (length after-free-allocs)
         :final-count (length final-allocs)
         :e-present (if (assq 'e final-allocs) t nil)
         :b-freed (if (assq 'b after-free-allocs) nil t)))
    (fmakunbound 'neovm--buddy-create)
    (fmakunbound 'neovm--buddy-next-pow2)
    (fmakunbound 'neovm--buddy-alloc)
    (fmakunbound 'neovm--buddy-free)
    (makunbound 'neovm--buddy-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:a-size 32 :b-size 64 :c-size 16 :d-size 1 :alloc-count 5 :after-free-count 2 :final-count 3 :e-present t :b-freed t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Slab allocator with size classes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_slab_allocator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Slab allocator: size-classes, each class has a pool of fixed-size objects
  ;; State: ((class-size . (free-list allocated-count total-count)) ...)
  (defvar neovm--slab-state nil)

  (fset 'neovm--slab-create
    (lambda (classes slab-size)
      "Create slab allocator with given size CLASSES, SLAB-SIZE objects per class."
      (mapcar (lambda (cls)
                (let ((free-list nil)
                      (i 0))
                  (while (< i slab-size)
                    (setq free-list (cons (* cls i) free-list))
                    (setq i (1+ i)))
                  (cons cls (list (nreverse free-list) 0 slab-size))))
              classes)))

  (fset 'neovm--slab-find-class
    (lambda (state size)
      "Find smallest class >= SIZE."
      (let ((found nil))
        (dolist (entry state)
          (when (and (>= (car entry) size)
                     (or (null found) (< (car entry) (car found))))
            (setq found entry)))
        found)))

  (fset 'neovm--slab-alloc
    (lambda (state size)
      "Allocate from slab allocator. Returns (offset . updated-state) or (nil . state)."
      (let* ((entry (funcall 'neovm--slab-find-class state size))
             (cls (car entry))
             (info (cdr entry))
             (free-list (nth 0 info))
             (alloc-count (nth 1 info))
             (total (nth 2 info)))
        (if (null free-list)
            (cons nil state)
          (let ((offset (car free-list)))
            (setcdr entry (list (cdr free-list) (1+ alloc-count) total))
            (cons offset state))))))

  (fset 'neovm--slab-free
    (lambda (state cls-size offset)
      "Return OFFSET back to class CLS-SIZE."
      (let ((entry (assq cls-size state)))
        (when entry
          (let ((info (cdr entry)))
            (setcdr entry (list (cons offset (nth 0 info))
                                (1- (nth 1 info))
                                (nth 2 info))))))
      state))

  (fset 'neovm--slab-stats
    (lambda (state)
      "Return allocation statistics per size class."
      (mapcar (lambda (entry)
                (let ((info (cdr entry)))
                  (list :class (car entry)
                        :free (length (nth 0 info))
                        :allocated (nth 1 info)
                        :total (nth 2 info)
                        :utilization (if (> (nth 2 info) 0)
                                         (/ (* 100 (nth 1 info)) (nth 2 info))
                                       0))))
              state)))

  (unwind-protect
      (let* ((state (funcall 'neovm--slab-create '(8 16 32 64 128) 10))
             ;; Allocate various sizes
             (r1 (funcall 'neovm--slab-alloc state 5))
             (state (cdr r1))
             (r2 (funcall 'neovm--slab-alloc state 12))
             (state (cdr r2))
             (r3 (funcall 'neovm--slab-alloc state 30))
             (state (cdr r3))
             (r4 (funcall 'neovm--slab-alloc state 50))
             (state (cdr r4))
             (r5 (funcall 'neovm--slab-alloc state 7))
             (state (cdr r5))
             ;; Stats after allocation
             (stats-after-alloc (funcall 'neovm--slab-stats state))
             ;; Free some
             (state (funcall 'neovm--slab-free state 8 (car r1)))
             (state (funcall 'neovm--slab-free state 32 (car r3)))
             ;; Stats after free
             (stats-after-free (funcall 'neovm--slab-stats state)))
        (list
         :offsets (list (car r1) (car r2) (car r3) (car r4) (car r5))
         :stats-after-alloc stats-after-alloc
         :stats-after-free stats-after-free))
    (fmakunbound 'neovm--slab-create)
    (fmakunbound 'neovm--slab-find-class)
    (fmakunbound 'neovm--slab-alloc)
    (fmakunbound 'neovm--slab-free)
    (fmakunbound 'neovm--slab-stats)
    (makunbound 'neovm--slab-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:offsets (0 0 0 0 8) :stats-after-alloc ((:class 8 :free 8 :allocated 2 :total 10 :utilization 20) (:class 16 :free 9 :allocated 1 :total 10 :utilization 10) (:class 32 :free 9 :allocated 1 :total 10 :utilization 10) (:class 64 :free 9 :allocated 1 :total 10 :utilization 10) (:class 128 :free 10 :allocated 0 :total 10 :utilization 0)) :stats-after-free ((:class 8 :free 9 :allocated 1 :total 10 :utilization 10) (:class 16 :free 9 :allocated 1 :total 10 :utilization 10) (:class 32 :free 10 :allocated 0 :total 10 :utilization 0) (:class 64 :free 9 :allocated 1 :total 10 :utilization 10) (:class 128 :free 10 :allocated 0 :total 10 :utilization 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Arena allocator (bump pointer)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_arena_allocator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Arena: (capacity next-offset allocations)
  (defvar neovm--arena-state nil)

  (fset 'neovm--arena-create
    (lambda (capacity)
      "Create an arena allocator with CAPACITY bytes."
      (list capacity 0 nil)))

  (fset 'neovm--arena-alloc
    (lambda (arena size alignment)
      "Allocate SIZE bytes with ALIGNMENT from ARENA.
Returns (offset . new-arena) or (nil . arena) if full."
      (let* ((capacity (nth 0 arena))
             (next (nth 1 arena))
             (allocs (nth 2 arena))
             ;; Align up: (next + align - 1) & ~(align - 1)
             ;; Since we lack bitwise in standard Elisp, use modular arithmetic
             (remainder (% next alignment))
             (aligned (if (= remainder 0) next (+ next (- alignment remainder)))))
        (if (> (+ aligned size) capacity)
            (cons nil arena)
          (cons aligned
                (list capacity (+ aligned size)
                      (cons (list aligned size) allocs)))))))

  (fset 'neovm--arena-reset
    (lambda (arena)
      "Reset arena, freeing all allocations at once."
      (list (nth 0 arena) 0 nil)))

  (fset 'neovm--arena-stats
    (lambda (arena)
      (let ((cap (nth 0 arena))
            (used (nth 1 arena))
            (count (length (nth 2 arena))))
        (list :capacity cap :used used :free (- cap used)
              :alloc-count count
              :utilization (if (> cap 0) (/ (* 100 used) cap) 0)))))

  (unwind-protect
      (let* ((arena (funcall 'neovm--arena-create 1024))
             ;; Allocate with various alignments
             (r1 (funcall 'neovm--arena-alloc arena 100 8))
             (arena (cdr r1))
             (r2 (funcall 'neovm--arena-alloc arena 200 16))
             (arena (cdr r2))
             (r3 (funcall 'neovm--arena-alloc arena 50 4))
             (arena (cdr r3))
             (r4 (funcall 'neovm--arena-alloc arena 300 32))
             (arena (cdr r4))
             (r5 (funcall 'neovm--arena-alloc arena 150 8))
             (arena (cdr r5))
             ;; Stats before overflow
             (stats-before (funcall 'neovm--arena-stats arena))
             ;; Try to allocate too much
             (r6 (funcall 'neovm--arena-alloc arena 500 8))
             (overflow-failed (null (car r6)))
             ;; Reset and reallocate
             (arena (funcall 'neovm--arena-reset (cdr r6)))
             (stats-after-reset (funcall 'neovm--arena-stats arena))
             (r7 (funcall 'neovm--arena-alloc arena 512 16))
             (arena (cdr r7))
             (stats-final (funcall 'neovm--arena-stats arena)))
        (list
         :offsets (list (car r1) (car r2) (car r3) (car r4) (car r5))
         :aligned-check (list (= (% (car r1) 8) 0)
                              (= (% (car r2) 16) 0)
                              (= (% (car r3) 4) 0)
                              (= (% (car r4) 32) 0)
                              (= (% (car r5) 8) 0))
         :stats-before stats-before
         :overflow-failed overflow-failed
         :stats-after-reset stats-after-reset
         :r7-offset (car r7)
         :stats-final stats-final))
    (fmakunbound 'neovm--arena-create)
    (fmakunbound 'neovm--arena-alloc)
    (fmakunbound 'neovm--arena-reset)
    (fmakunbound 'neovm--arena-stats)
    (makunbound 'neovm--arena-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:offsets (0 112 312 384 688) :aligned-check (t t t t t) :stats-before (:capacity 1024 :used 838 :free 186 :alloc-count 5 :utilization 81) :overflow-failed t :stats-after-reset (:capacity 1024 :used 0 :free 1024 :alloc-count 0 :utilization 0) :r7-offset 0 :stats-final (:capacity 1024 :used 512 :free 512 :alloc-count 1 :utilization 50))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Free list management: first-fit, best-fit, next-fit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_free_list_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Free list: sorted list of (offset . size) entries
  (defvar neovm--fl-state nil)

  (fset 'neovm--fl-create
    (lambda (total-size)
      "Create free list with single block of TOTAL-SIZE."
      (list (list (cons 0 total-size)) nil)))

  (fset 'neovm--fl-first-fit
    (lambda (free-list size)
      "First-fit: find first block >= SIZE. Returns (offset . remaining-list) or nil."
      (let ((prev nil)
            (curr free-list)
            (found nil))
        (while (and curr (not found))
          (if (>= (cdar curr) size)
              (setq found (car curr))
            (setq prev curr)
            (setq curr (cdr curr))))
        (when found
          (let ((offset (car found))
                (blk-size (cdr found)))
            ;; Remove or shrink block
            (if (= blk-size size)
                (if prev
                    (setcdr prev (cdr curr))
                  (setq free-list (cdr free-list)))
              ;; Shrink: allocate from front
              (setcar (car curr) (+ offset size))
              (setcdr (car curr) (- blk-size size)))
            (cons offset free-list))))))

  (fset 'neovm--fl-best-fit
    (lambda (free-list size)
      "Best-fit: find smallest block >= SIZE."
      (let ((best nil)
            (best-waste most-positive-fixnum))
        (dolist (block free-list)
          (let ((waste (- (cdr block) size)))
            (when (and (>= waste 0) (< waste best-waste))
              (setq best block)
              (setq best-waste waste))))
        (when best
          (let ((offset (car best))
                (blk-size (cdr best)))
            (if (= blk-size size)
                (setq free-list (delq best free-list))
              (setcar best (+ offset size))
              (setcdr best (- blk-size size)))
            (cons offset free-list))))))

  (unwind-protect
      (let* (;; Set up fragmented free list
             (fl (list (cons 0 100) (cons 200 50) (cons 400 200) (cons 700 80) (cons 900 100)))
             ;; First-fit for 60: should pick first block >= 60, which is (0 . 100)
             (ff1 (funcall 'neovm--fl-first-fit (copy-sequence fl) 60))
             ;; Best-fit for 60: should pick (700 . 80) as smallest fit
             (bf1 (funcall 'neovm--fl-best-fit (copy-sequence fl) 60))
             ;; First-fit for 150: should pick (400 . 200)
             (ff2 (funcall 'neovm--fl-first-fit (copy-sequence fl) 150))
             ;; Best-fit for 150: should pick (400 . 200)
             (bf2 (funcall 'neovm--fl-best-fit (copy-sequence fl) 150))
             ;; First-fit for exactly 50: should pick (200 . 50) exact match
             (ff3 (funcall 'neovm--fl-first-fit (copy-sequence fl) 50))
             ;; Allocation too large
             (ff-fail (funcall 'neovm--fl-first-fit (copy-sequence fl) 500))
             (bf-fail (funcall 'neovm--fl-best-fit (copy-sequence fl) 500)))
        (list
         :ff1-offset (car ff1)
         :bf1-offset (car bf1)
         :ff2-offset (car ff2)
         :bf2-offset (car bf2)
         :ff3-offset (car ff3)
         :ff-fail ff-fail
         :bf-fail bf-fail
         ;; Remaining free list sizes after first-fit 60
         :ff1-remaining (mapcar #'cdr (cdr ff1))
         ;; Remaining free list sizes after best-fit 60
         :bf1-remaining (mapcar #'cdr (cdr bf1))))
    (fmakunbound 'neovm--fl-create)
    (fmakunbound 'neovm--fl-first-fit)
    (fmakunbound 'neovm--fl-best-fit)
    (makunbound 'neovm--fl-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:ff1-offset 0 :bf1-offset 700 :ff2-offset 400 :bf2-offset nil :ff3-offset 200 :ff-fail nil :bf-fail nil :ff1-remaining (40 50 50 20 100) :bf1-remaining (40 50 50 20 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memory fragmentation analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_fragmentation_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--frag-state nil)

  (fset 'neovm--frag-analyze
    (lambda (free-blocks total-size)
      "Analyze fragmentation of FREE-BLOCKS (list of (offset . size)) in TOTAL-SIZE.
Returns fragmentation metrics."
      (let ((total-free 0)
            (max-block 0)
            (min-block most-positive-fixnum)
            (count 0)
            (sorted (sort (copy-sequence free-blocks)
                          (lambda (a b) (< (car a) (car b))))))
        ;; Basic stats
        (dolist (blk sorted)
          (let ((sz (cdr blk)))
            (setq total-free (+ total-free sz))
            (when (> sz max-block) (setq max-block sz))
            (when (< sz min-block) (setq min-block sz))
            (setq count (1+ count))))
        (when (= count 0) (setq min-block 0))
        (let* ((total-alloc (- total-size total-free))
               (external-frag (if (> total-free 0)
                                  (- 100 (/ (* 100 max-block) total-free))
                                0))
               (avg-block (if (> count 0) (/ total-free count) 0))
               ;; Count gaps between allocated regions
               (gaps 0))
          ;; Count gaps: check spaces between consecutive free blocks
          (let ((prev nil))
            (dolist (blk sorted)
              (when (and prev (> (car blk) (+ (car prev) (cdr prev))))
                (setq gaps (1+ gaps)))
              (setq prev blk)))
          (list :total-size total-size
                :total-free total-free
                :total-alloc total-alloc
                :free-block-count count
                :max-block max-block
                :min-block min-block
                :avg-block avg-block
                :external-frag-pct external-frag
                :allocated-gaps gaps)))))

  (unwind-protect
      (let* (;; No fragmentation: single large free block
             (no-frag (funcall 'neovm--frag-analyze
                               '((0 . 1000)) 1000))
             ;; High fragmentation: many small scattered blocks
             (high-frag (funcall 'neovm--frag-analyze
                                 '((0 . 10) (50 . 10) (100 . 10)
                                   (150 . 10) (200 . 10) (250 . 10)
                                   (300 . 10) (350 . 10) (400 . 10)
                                   (450 . 10))
                                 500))
             ;; Medium fragmentation: few medium blocks
             (med-frag (funcall 'neovm--frag-analyze
                                '((0 . 200) (400 . 150) (700 . 100))
                                1000))
             ;; Extreme: all free but split into 1-byte blocks
             (extreme (funcall 'neovm--frag-analyze
                               (let ((r nil) (i 0))
                                 (while (< i 50)
                                   (setq r (cons (cons (* i 4) 2) r))
                                   (setq i (1+ i)))
                                 (nreverse r))
                               200)))
        (list
         :no-frag no-frag
         :high-frag high-frag
         :med-frag med-frag
         :extreme extreme))
    (fmakunbound 'neovm--frag-analyze)
    (makunbound 'neovm--frag-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:no-frag (:total-size 1000 :total-free 1000 :total-alloc 0 :free-block-count 1 :max-block 1000 :min-block 1000 :avg-block 1000 :external-frag-pct 0 :allocated-gaps 0) :high-frag (:total-size 500 :total-free 100 :total-alloc 400 :free-block-count 10 :max-block 10 :min-block 10 :avg-block 10 :external-frag-pct 90 :allocated-gaps 9) :med-frag (:total-size 1000 :total-free 450 :total-alloc 550 :free-block-count 3 :max-block 200 :min-block 100 :avg-block 150 :external-frag-pct 56 :allocated-gaps 2) :extreme (:total-size 200 :total-free 100 :total-alloc 100 :free-block-count 50 :max-block 2 :min-block 2 :avg-block 2 :external-frag-pct 98 :allocated-gaps 49))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coalescing adjacent free blocks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_coalescing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--coal-state nil)

  (fset 'neovm--coal-coalesce
    (lambda (free-blocks)
      "Coalesce adjacent free blocks. FREE-BLOCKS is a list of (offset . size).
Returns sorted, merged list."
      (let* ((sorted (sort (copy-sequence free-blocks)
                           (lambda (a b) (< (car a) (car b)))))
             (result nil)
             (current nil))
        (dolist (blk sorted)
          (if (null current)
              (setq current (cons (car blk) (cdr blk)))
            ;; Check if adjacent: current-end == blk-start
            (if (= (+ (car current) (cdr current)) (car blk))
                ;; Merge
                (setcdr current (+ (cdr current) (cdr blk)))
              ;; Not adjacent: flush current, start new
              (setq result (cons current result))
              (setq current (cons (car blk) (cdr blk))))))
        (when current
          (setq result (cons current result)))
        (nreverse result))))

  (fset 'neovm--coal-free-and-coalesce
    (lambda (free-blocks offset size)
      "Add a freed block and coalesce."
      (funcall 'neovm--coal-coalesce
               (cons (cons offset size) free-blocks))))

  (unwind-protect
      (let* (;; Already coalesced: no change
             (already '((0 . 100) (200 . 100) (400 . 100)))
             (c1 (funcall 'neovm--coal-coalesce already))
             ;; Two adjacent blocks
             (adj '((0 . 100) (100 . 100)))
             (c2 (funcall 'neovm--coal-coalesce adj))
             ;; Three adjacent blocks
             (tri '((0 . 50) (50 . 50) (100 . 50)))
             (c3 (funcall 'neovm--coal-coalesce tri))
             ;; Mixed: some adjacent, some not
             (mixed '((0 . 30) (30 . 20) (100 . 40) (140 . 60) (300 . 50)))
             (c4 (funcall 'neovm--coal-coalesce mixed))
             ;; Unsorted input
             (unsorted '((200 . 50) (0 . 100) (100 . 100) (250 . 50)))
             (c5 (funcall 'neovm--coal-coalesce unsorted))
             ;; Simulate alloc/free/coalesce cycle
             ;; Start: [0..1000] allocated. Free blocks: none
             ;; Free [100..200], [200..400], [500..600]: should coalesce [100..400]
             (fl nil)
             (fl (funcall 'neovm--coal-free-and-coalesce fl 100 100))
             (fl (funcall 'neovm--coal-free-and-coalesce fl 200 200))
             (fl (funcall 'neovm--coal-free-and-coalesce fl 500 100))
             ;; Single element
             (single (funcall 'neovm--coal-coalesce '((42 . 10)))))
        (list
         :already-no-change (equal c1 already)
         :c1 c1
         :two-merged c2
         :three-merged c3
         :mixed-merged c4
         :unsorted-merged c5
         :alloc-free-cycle fl
         :single single))
    (fmakunbound 'neovm--coal-coalesce)
    (fmakunbound 'neovm--coal-free-and-coalesce)
    (makunbound 'neovm--coal-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:already-no-change t :c1 ((0 . 100) (200 . 100) (400 . 100)) :two-merged ((0 . 200)) :three-merged ((0 . 150)) :mixed-merged ((0 . 50) (100 . 100) (300 . 50)) :unsorted-merged ((0 . 300)) :alloc-free-cycle ((100 . 300) (500 . 100)) :single ((42 . 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Allocation statistics tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_memalloc_statistics_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Stats tracker: records allocation/free events and computes metrics
  (defvar neovm--astats-state nil)

  (fset 'neovm--astats-create
    (lambda ()
      "Create allocation stats tracker.
State: (alloc-count free-count total-alloc-bytes total-free-bytes
        peak-usage current-usage size-histogram)"
      (list 0 0 0 0 0 0
            (make-hash-table :test 'equal))))

  (fset 'neovm--astats-record-alloc
    (lambda (stats size)
      "Record an allocation of SIZE bytes."
      (let ((ac (1+ (nth 0 stats)))
            (fc (nth 1 stats))
            (tab (+ (nth 2 stats) size))
            (tfb (nth 3 stats))
            (peak (nth 4 stats))
            (current (+ (nth 5 stats) size))
            (hist (nth 6 stats)))
        ;; Update histogram: bucket by power of 2
        (let* ((bucket (let ((p 1)) (while (< p size) (setq p (* p 2))) p))
               (key (format "%d" bucket))
               (old (gethash key hist 0)))
          (puthash key (1+ old) hist))
        (list ac fc tab tfb (max peak current) current hist))))

  (fset 'neovm--astats-record-free
    (lambda (stats size)
      "Record a free of SIZE bytes."
      (let ((ac (nth 0 stats))
            (fc (1+ (nth 1 stats)))
            (tab (nth 2 stats))
            (tfb (+ (nth 3 stats) size))
            (peak (nth 4 stats))
            (current (- (nth 5 stats) size))
            (hist (nth 6 stats)))
        (list ac fc tab tfb peak current hist))))

  (fset 'neovm--astats-summary
    (lambda (stats)
      "Generate summary from stats."
      (let ((hist (nth 6 stats))
            (hist-list nil))
        (maphash (lambda (k v) (push (cons k v) hist-list)) hist)
        (setq hist-list (sort hist-list (lambda (a b) (< (string-to-number (car a))
                                                          (string-to-number (car b))))))
        (list :alloc-count (nth 0 stats)
              :free-count (nth 1 stats)
              :total-alloc-bytes (nth 2 stats)
              :total-free-bytes (nth 3 stats)
              :peak-usage (nth 4 stats)
              :current-usage (nth 5 stats)
              :leak-bytes (- (nth 2 stats) (nth 3 stats))
              :avg-alloc-size (if (> (nth 0 stats) 0)
                                  (/ (nth 2 stats) (nth 0 stats))
                                0)
              :histogram hist-list))))

  (unwind-protect
      (let* ((stats (funcall 'neovm--astats-create))
             ;; Simulate allocation pattern
             (stats (funcall 'neovm--astats-record-alloc stats 32))
             (stats (funcall 'neovm--astats-record-alloc stats 64))
             (stats (funcall 'neovm--astats-record-alloc stats 128))
             (stats (funcall 'neovm--astats-record-alloc stats 32))
             (stats (funcall 'neovm--astats-record-alloc stats 16))
             ;; Free some
             (stats (funcall 'neovm--astats-record-free stats 32))
             (stats (funcall 'neovm--astats-record-free stats 64))
             ;; More allocations
             (stats (funcall 'neovm--astats-record-alloc stats 256))
             (stats (funcall 'neovm--astats-record-alloc stats 32))
             (stats (funcall 'neovm--astats-record-alloc stats 16))
             ;; Free all remaining
             (stats (funcall 'neovm--astats-record-free stats 128))
             (stats (funcall 'neovm--astats-record-free stats 32))
             (stats (funcall 'neovm--astats-record-free stats 16))
             (stats (funcall 'neovm--astats-record-free stats 256))
             (stats (funcall 'neovm--astats-record-free stats 32))
             (stats (funcall 'neovm--astats-record-free stats 16))
             (summary (funcall 'neovm--astats-summary stats)))
        (list
         :summary summary
         :balanced (= (nth 5 stats) 0)
         :no-leaks (= (plist-get summary :leak-bytes) 0)))
    (fmakunbound 'neovm--astats-create)
    (fmakunbound 'neovm--astats-record-alloc)
    (fmakunbound 'neovm--astats-record-free)
    (fmakunbound 'neovm--astats-summary)
    (makunbound 'neovm--astats-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:summary (:alloc-count 8 :free-count 8 :total-alloc-bytes 576 :total-free-bytes 576 :peak-usage 480 :current-usage 0 :leak-bytes 0 :avg-alloc-size 72 :histogram ((\"16\" . 2) (\"32\" . 3) (\"64\" . 1) (\"128\" . 1) (\"256\" . 1))) :balanced t :no-leaks t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
