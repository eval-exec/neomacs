;; -*- lexical-binding: t -*-
;; GNU 31.1 oracle for the A6 restricted subset. Refresh with
;; NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 (via NEOVM_LISP_SANDBOX on Linux).
(unless (string= emacs-version "31.1")
  (error "A6 oracle requires GNU Emacs 31.1, got %s" emacs-version))
(defun neovm-a6-out (value)
  (prin1 value)
  (terpri))
(neovm-a6-out (+ 40 2))
(neovm-a6-out (- 10 3))
(neovm-a6-out (< 1 2))
(neovm-a6-out (< 2 1))
(neovm-a6-out (= 3 3))
(neovm-a6-out (= 3 4))
(neovm-a6-out (if nil 1 2))
(neovm-a6-out (if t 1 2))
(neovm-a6-out
 (let ((n 5)
       (s 0))
   (while (> n 0)
     (setq s (+ s n))
     (setq n (- n 1)))
   s))
(dolist (i '(0 1 2 3))
  (let* ((base (* i 1000000))
         (head (cons base nil))
         (tail head)
         (k 1))
    (while (< k 8)
      (let ((cell (cons (+ base k) nil)))
        (setcdr tail cell)
        (setq tail cell)
        (setq k (1+ k))))
    (setcar head (cons 42 nil))
    (neovm-a6-out head)))

;; B3 twin: sealed single-owner hash-table ops (design §6). Mirrors
;; ht_main_setup table-for-table, then the P1-P8s programs in order,
;; interleaving the structural dumps exactly as the harness asserts.
(setq a6-tA (make-hash-table :test 'eq))
(setq a6-tB (make-hash-table :test 'eq))
(setq a6-tC (make-hash-table :test 'eql))
(setq a6-tD (make-hash-table :test 'equal))
(setq a6-tE (make-hash-table :test 'eq))
(setq a6-tG (make-hash-table :test 'eq))
(defun a6-user-hash (x) x)
(defun a6-user-cmp (a b) (eq a b))
(define-hash-table-test 'a6-user-test 'a6-user-cmp 'a6-user-hash)
(setq a6-tU (make-hash-table :test 'a6-user-test))
(defun a6-user-hash2 (x) 7)
(defun a6-user-cmp-other (a b) (puthash 5 55 a6-tO2) (eq a b))
(define-hash-table-test 'a6-user-test-other 'a6-user-cmp-other 'a6-user-hash2)
(setq a6-tO (make-hash-table :test 'a6-user-test-other))
(setq a6-tO2 (make-hash-table :test 'eq))
;; HtGet's (found value) pair as a two-element list prefix; the sentinel is
;; never a stored value in these programs.
(defun a6-gh (key table)
  (let ((value (gethash key table 'a6-miss)))
    (if (eq value 'a6-miss) '(nil nil) (list t value))))
;; P1: eq, fixnum keys -> (t 100 nil nil 2); then tA buckets.
(puthash 10 100 a6-tA)
(puthash 20 200 a6-tA)
(neovm-a6-out (append (a6-gh 10 a6-tA) (a6-gh 30 a6-tA)
                      (list (hash-table-count a6-tA))))
(neovm-a6-out (internal--hash-table-buckets a6-tA))
;; P2: eq, bare symbols -> (t 1 nil nil 2); then tB count and sorted contents.
;; Symbol EQ hashes use runtime identity, so bucket histograms are not portable.
(puthash 'a6-symA 1 a6-tB)
(puthash 'a6-symB 2 a6-tB)
(neovm-a6-out (append (a6-gh 'a6-symA a6-tB) (a6-gh 99 a6-tB)
                      (list (hash-table-count a6-tB))))
(neovm-a6-out
 (let ((entries nil))
   (maphash (lambda (key value)
              (setq entries (cons (cons (symbol-name key) value) entries))) a6-tB)
   (list (hash-table-count a6-tB)
         (sort entries (lambda (a b)
                         (string-lessp (car a) (car b)))))))
;; P3: eql -> (t 1000 1).
(puthash 10 1000 a6-tC)
(neovm-a6-out (append (a6-gh 10 a6-tC) (list (hash-table-count a6-tC))))
;; P4: equal, two fresh structurally-equal conses -> (t 5 1); then tD buckets.
(puthash (cons 1 2) 5 a6-tD)
(neovm-a6-out (append (a6-gh (cons 1 2) a6-tD)
                      (list (hash-table-count a6-tD))))
(neovm-a6-out (internal--hash-table-buckets a6-tD))
;; P5: rem removes once, refuses twice, one entry survives -> (t nil 1).
;; GNU's remhash returns nil unconditionally, so the found flags come from
;; presence checks around the removals, as the HtRem op reports them.
(puthash 1 11 a6-tE)
(puthash 2 22 a6-tE)
(neovm-a6-out
 (list (not (eq (gethash 1 a6-tE 'a6-miss) 'a6-miss))
       (progn (remhash 1 a6-tE)
              (not (eq (gethash 1 a6-tE 'a6-miss) 'a6-miss)))
       (progn (remhash 1 a6-tE)
              (hash-table-count a6-tE))))
(neovm-a6-out (internal--hash-table-buckets a6-tE))
;; P6: 65 puts grow the table through resizes -> (65 128); then tG histogram.
(let ((k 0))
  (while (< k 65)
    (puthash k (* k 2) a6-tG)
    (setq k (1+ k))))
(neovm-a6-out (list (hash-table-count a6-tG) (gethash 64 a6-tG)))
(neovm-a6-out (internal--hash-table-histogram a6-tG))
;; P7: pure user test -> (t 8 nil nil 2); then tU buckets.
(puthash 7 8 a6-tU)
(puthash 8 9 a6-tU)
(neovm-a6-out (append (a6-gh 7 a6-tU) (a6-gh 30 a6-tU)
                      (list (hash-table-count a6-tU))))
(neovm-a6-out (internal--hash-table-buckets a6-tU))
;; P8s: a non-EQ lookup with the constant hash forces the comparison, which
;; legally mutates a different table -> (nil nil 1).
(puthash 5 50 a6-tO)
(neovm-a6-out (append (a6-gh 9 a6-tO) (list (hash-table-count a6-tO2))))
;; P7b twin: a comparison that edits the table under test is refused with
;; GNU's message prefix (the full error text appends the printed table,
;; which no fixed oracle line could pin); the table keeps its one entry.
(defun a6-user-cmp-bad (a b) (puthash 99 9 a6-tM) (eq a b))
(define-hash-table-test 'a6-user-test-bad 'a6-user-cmp-bad 'a6-user-hash2)
(setq a6-tM (make-hash-table :test 'a6-user-test-bad))
(puthash 42 1 a6-tM)
(neovm-a6-out
 (condition-case error-info
     (progn (gethash 43 a6-tM) 'no-error)
   (error (let ((message (error-message-string error-info)))
            (substring message 0 (string-match ":" message))))))

;; B4 twin P9: string keys on an equal table (design §4 Rev R3) -> (t nil 1),
;; then presence checks and buckets. No string is printed; the found flags
;; come from sentinel presence checks (GNU's remhash says nil
;; unconditionally, as P5 established).
(setq a6-tS (make-hash-table :test 'equal))
(puthash "a6-alpha" 101 a6-tS)
(puthash "a6-beta" 102 a6-tS)
(gethash "a6-alpha" a6-tS)
(gethash "a6-gamma" a6-tS 'a6-miss)
(neovm-a6-out
 (list (not (eq (gethash "a6-beta" a6-tS 'a6-miss) 'a6-miss))
       (progn (remhash "a6-beta" a6-tS)
              (not (eq (gethash "a6-beta" a6-tS 'a6-miss) 'a6-miss)))
       (hash-table-count a6-tS)))
(neovm-a6-out (not (eq (gethash "a6-alpha" a6-tS 'a6-miss) 'a6-miss)))
(neovm-a6-out (not (eq (gethash "a6-beta" a6-tS 'a6-miss) 'a6-miss)))
(neovm-a6-out (internal--hash-table-buckets a6-tS))
;; B4 twin P10: both serial orders of the two-owner overlap program -- the
;; legal final values of the shared key are exactly these two lines.
(setq a6-tI (make-hash-table :test 'eq))
(puthash 77 11 a6-tI)
(puthash 77 12 a6-tI)
(puthash 77 21 a6-tI)
(puthash 77 22 a6-tI)
(neovm-a6-out (gethash 77 a6-tI))
(setq a6-tJ (make-hash-table :test 'eq))
(puthash 77 21 a6-tJ)
(puthash 77 22 a6-tJ)
(puthash 77 11 a6-tJ)
(puthash 77 12 a6-tJ)
(neovm-a6-out (gethash 77 a6-tJ))

;; B5 twin P11: 130 puts grow a fresh table through several resizes, then
;; count, get, two rems and the final count in one line (design §4 Rev R3).
;; The sealed row runs the identical program as one owner; growth charging
;; changes no answer, only the owner's byte counters.
(setq a6-tK (make-hash-table :test 'eq))
(let ((k 0))
  (while (< k 130)
    (puthash k (* k 3) a6-tK)
    (setq k (+ k 1))))
(neovm-a6-out
 (list (hash-table-count a6-tK)
       (gethash 129 a6-tK)
       (progn (remhash 5 a6-tK) (remhash 6 a6-tK) (hash-table-count a6-tK))))

;; B7 twins: tables made and copied by an owner, shallow values and independent
;; entry storage. The cons key is the SAME object for both table lookups.
(dolist (test '(eq eql equal))
  (let* ((source (make-hash-table :test test :size 4))
         (key (cons 7 nil))
         (value (cons 41 nil)))
    (puthash key value source)
    (let ((copy (copy-hash-table source)))
      (puthash key 99 source)
      (puthash 8 88 source)
      (setcar value 77)
      (let ((source-count (hash-table-count source))
            (copy-count (hash-table-count copy))
            (copy-value (gethash key copy 'a6-miss))
            (source-value (gethash key source 'a6-miss)))
        (clrhash source)
        (neovm-a6-out
         (list source-count copy-count
               (not (eq copy-value 'a6-miss)) copy-value
               (not (eq source-value 'a6-miss)) source-value
               (hash-table-count source) (hash-table-count copy)
               (not (eq (gethash key copy 'a6-miss) 'a6-miss))
               (gethash key copy)))))))

;; A copy made inside either kind of custom callback keeps all metadata and
;; callbacks, permits mutation, and leaves the source's test guard intact.
(neovm-a6-out
 (let ((answers nil) (callbacks '(compare hash)))
  (while callbacks
    (let ((callback (car callbacks))
          (source nil) (armed nil) (fired nil) (copy nil) (observed nil))
      (let ((action
             (lambda ()
               (if armed
                   (if fired nil
                     (progn
                       (setq fired t copy (copy-hash-table source))
                       (setq observed
                             (list (eq (hash-table-test source) (hash-table-test copy))
                                   (= (hash-table-size source) (hash-table-size copy))
                                   (= (hash-table-rehash-size source)
                                      (hash-table-rehash-size copy))
                                   (= (hash-table-rehash-threshold source)
                                      (hash-table-rehash-threshold copy))
                                   (eq (hash-table-weakness source)
                                       (hash-table-weakness copy))
                                   (puthash "copy-only" 2 copy)
                                   (condition-case err (puthash "source-only" 3 source)
                                     (error (list (car err) (car (cdr err))
                                                  (eq (nth 2 err) source))))))))))))
        (define-hash-table-test
         'a6-b7-copy-test
         (lambda (a b)
           (if (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (if (eq callback 'hash) (funcall action))
           0))
        (setq source (make-hash-table :test 'a6-b7-copy-test :size 32
                                      :rehash-size 2.0 :rehash-threshold 0.8))
        (puthash (copy-sequence "key") 1 source)
        (setq armed t)
        (let ((answer (gethash (copy-sequence "key") source)))
          (setq armed nil)
          (setq answers
                (cons (list callback answer observed
                            (hash-table-count source) (hash-table-count copy)
                            (gethash "key" copy) (gethash "copy-only" source 'absent))
                      answers)))))
    (setq callbacks (cdr callbacks)))
  answers))

;; B8 weak-table semantic twins: generated answers require GNU 31.1 refresh.

;; SETUP_FORM
(neovm-a6-out
 (list
  (hash-table-weakness (make-hash-table :weakness t))
  (hash-table-weakness (make-hash-table :weakness 'key))
  (hash-table-weakness (make-hash-table :weakness 'value))
  (hash-table-weakness (make-hash-table :weakness 'key-or-value))
  (hash-table-weakness (make-hash-table))
  (let ((h (make-hash-table :test 'equal :weakness t)))
    (puthash "key" 'val h)
    (list (hash-table-count h) (hash-table-weakness h)))
  (let ((h (make-hash-table :test 'eq :weakness t))
        (k (cons 1 2)))
    (puthash k 'weak-val h)
    (puthash 'stable-key 'stable-val h)
    (garbage-collect)
    (list (hash-table-count h) (gethash k h 'gone)
          (gethash 'stable-key h)))))

;; MATRIX_FORM
(neovm-a6-out
 (let ((weaknesses '(key value key-or-value key-and-value))
      (answers nil))
  (while weaknesses
    (let ((h (make-hash-table :test 'eq :weakness (car weaknesses)))
          (roots nil) (state 0))
      (while (< state 4)
        (let ((key (cons state nil)) (value (cons state nil)))
          (puthash key value h)
          (if (= state 0) (setq roots (cons key roots)))
          (if (= state 1) (setq roots (cons value roots)))
          (if (= state 2) (setq roots (cons key (cons value roots)))))
        (setq state (+ state 1)))
      (garbage-collect)
      (garbage-collect)
      (let ((kept (hash-table-count h)))
        (setq roots nil)
        (garbage-collect)
        (garbage-collect)
        (setq answers (cons (list kept (hash-table-count h)) answers))))
    (setq weaknesses (cdr weaknesses)))
  (nreverse answers)))

;; CHAIN_FORM
(neovm-a6-out
 (progn
  ;; The producer frame must end before GC: GNU conservatively protects
  ;; evaluated arguments and let initializers until their C frame returns.
  (fset 'a6-b8-oracle-produce-chain
    (lambda (h0 h1)
      (let ((a nil) (b nil) (c nil) (d nil) (e nil) (x nil) (y nil))
        (setq a (cons 1 nil) b (cons 2 nil) c (cons 3 nil)
              d (cons 4 nil) e (cons 5 nil) x (cons 6 nil) y (cons 7 nil))
        (puthash a b h0)
        (puthash c d h0)
        (puthash b c h1)
        (puthash d e h1)
        (puthash x y h0)
        (puthash y x h1)
        (setq a6-b8-oracle-root-a a)
        ;; Clear the lexical cells too: a suspended evaluator may retain the
        ;; completed call environment until its next serial root census.
        (setq a nil b nil c nil d nil e nil x nil y nil)
        nil)))
  (let ((h0 (make-hash-table :test 'eq :weakness 'key))
        (h1 (make-hash-table :test 'eq :weakness 'key)))
    (funcall 'a6-b8-oracle-produce-chain h0 h1)
    (garbage-collect)
    (garbage-collect)
    (let ((kept (list (hash-table-count h0) (hash-table-count h1))))
      (setq a6-b8-oracle-root-a nil)
      (garbage-collect)
      (garbage-collect)
      (list kept (hash-table-count h0) (hash-table-count h1))))))

;; MEMO_FORM
(neovm-a6-out
 (progn
  (define-hash-table-test 'a6-b8-oracle-collisions
    (lambda (a b) (eq a b)) (lambda (_key) 7))
  (let ((h (make-hash-table :test 'a6-b8-oracle-collisions
                            :weakness 'value :size 4))
        (roots nil) (batch 0) (answers nil))
    (while (< batch 2)
      (setq roots nil)
      (let ((index 0))
        (while (< index 24)
          (let ((key (+ (* batch 24) index)))
            (let ((value (cons key nil)))
              (puthash key value h)
              (if (= (% index 8) 0) (setq roots (cons value roots)))))
          (setq index (+ index 1))))
      (garbage-collect)
      (garbage-collect)
      (setq answers
            (cons (list (hash-table-count h)
                        (consp (gethash (* batch 24) h))
                        (gethash (+ (* batch 24) 1) h 'gone))
                  answers))
      (setq batch (+ batch 1)))
    (setq roots nil)
    (garbage-collect)
    (garbage-collect)
    (list (nreverse answers) (hash-table-count h)))))
