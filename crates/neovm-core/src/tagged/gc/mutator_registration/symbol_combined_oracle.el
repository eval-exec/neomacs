;; -*- lexical-binding: t -*-
;; C6 serial language twins; expectations come only from GNU 31.1 refreshes.
;; A lambda's direct immutable string constant is the proposed NEW C6 closed
;; callable graph. C4's scalar-only validator does not already admit it.
;; GNU has no Plain/projected-Plain descriptor distinction: these ordinary
;; global operations are the same language twins for either Rust backing.
;; No specbind, BLV/alias/watchers, custom obarray or concurrent callback claim.
;; Explicit collections test GNU semantics, never Rust root lifetime or counts.
(unless (and (string= emacs-version "31.1")
             (equal (getenv "NEOVM_ORACLE_MODE") "refresh")
             (equal (getenv "UPDATE_EXPECT") "1"))
  (error "C6 oracle requires GNU 31.1 and explicit refresh mode"))
(defvar p3sym-c6-row-count 0)
(defun p3sym-c6-out (value)
  (setq p3sym-c6-row-count (1+ p3sym-c6-row-count))
  (prin1 value)
  (terpri))

;; Real global -> direct function -> returned exact name -> initial intern.
(setq p3sym-c6-holder
      (list 'lambda nil (copy-sequence "p3sym-c6-combined-fresh")))
(let (selected returned winner)
  ;; 01: the original global holds the function graph.
  (p3sym-c6-out (boundp 'p3sym-c6-holder))
  ;; 02: direct fset returns that exact graph.
  (p3sym-c6-out (eq (fset 'p3sym-c6-worker p3sym-c6-holder)
                    p3sym-c6-holder))
  ;; 03: reading the function cell selects the same graph as the global.
  (setq selected (symbol-function 'p3sym-c6-worker))
  (p3sym-c6-out (eq selected (symbol-value 'p3sym-c6-holder)))
  ;; 04-05: clear the actual original holder, not a copied local value.
  (p3sym-c6-out (eq (makunbound 'p3sym-c6-holder) 'p3sym-c6-holder))
  (p3sym-c6-out (boundp 'p3sym-c6-holder))
  (garbage-collect)
  ;; 06: the call returns the exact direct constant, not a copied string.
  (setq returned (funcall 'p3sym-c6-worker))
  (p3sym-c6-out (eq returned (funcall selected)))
  ;; 07-09: soft miss, first-name-object publication, then equal-name hit.
  (p3sym-c6-out (intern-soft returned))
  (setq winner (intern returned))
  (p3sym-c6-out (eq (symbol-name winner) returned))
  (p3sym-c6-out (eq (intern-soft (copy-sequence returned)) winner))
  ;; 10: a fresh ordinary symbol has no value/function/plist payload.
  (p3sym-c6-out (list (boundp winner) (fboundp winner) (symbol-plist winner)))
  ;; 11: reassigning the same definition preserves identity and call result.
  (p3sym-c6-out (list (eq (fset 'p3sym-c6-worker selected) selected)
                      (fboundp 'p3sym-c6-worker)
                      (eq (funcall 'p3sym-c6-worker) returned)))
  (fset 'p3sym-c6-worker '(lambda () 97))
  (garbage-collect)
  ;; 12: an already selected definition differs from a new named call.
  (p3sym-c6-out (list (eq (funcall selected) returned)
                      (funcall 'p3sym-c6-worker)))
  (fmakunbound 'p3sym-c6-worker)
  (garbage-collect)
  ;; 13: unbinding the cell does not unbind the already selected function.
  (p3sym-c6-out
   (list (eq (funcall selected) returned)
         (symbol-function 'p3sym-c6-worker)
         (fboundp 'p3sym-c6-worker)
         (condition-case condition (funcall 'p3sym-c6-worker)
           (void-function (car condition)))))
  ;; 14: repeated unbinding is an ordinary successful no-op.
  (p3sym-c6-out (list (eq (fmakunbound 'p3sym-c6-worker) 'p3sym-c6-worker)
                      (symbol-function 'p3sym-c6-worker)
                      (fboundp 'p3sym-c6-worker)))
  (setq selected nil returned nil)
  (garbage-collect)
  ;; 15: after clearing original holders, the published name still resolves.
  (p3sym-c6-out (eq (intern-soft (symbol-name winner)) winner))
  ;; 16: the newly interned plain global supports set/read/makunbound.
  (set winner 101)
  (p3sym-c6-out (list (symbol-value winner)
                      (eq (makunbound winner) winner)
                      (boundp winner))))

;; Independent equal-name objects: exactly the first published one wins.
(let* ((first (copy-sequence "p3sym-c6-equal-name-fresh"))
       (second (copy-sequence first)))
  ;; 17: neither equal spelling is initially present.
  (p3sym-c6-out (list (intern-soft first) (intern-soft second)))
  ;; 18: the inputs are equal strings with distinct identities.
  (p3sym-c6-out (list (equal first second) (eq first second)))
  (let ((winner (intern first)))
    ;; 19: publishing the equal spelling returns the existing symbol.
    (p3sym-c6-out (eq (intern second) winner))
    ;; 20: the losing string does not replace the selected name object.
    (p3sym-c6-out (list (eq (symbol-name winner) first)
                        (eq (symbol-name winner) second)
                        (eq (intern-soft second) winner)))))

;; Ordinary global cells only; lexical temporaries are not new binding ops.
(setq p3sym-c6-global 73)
;; 21: the existing global is bound and readable.
(p3sym-c6-out (list (boundp 'p3sym-c6-global)
                    (symbol-value 'p3sym-c6-global)))
;; 22: set's result and a subsequent value read agree.
(p3sym-c6-out (list (set 'p3sym-c6-global 97)
                    (symbol-value 'p3sym-c6-global)))
(let ((received (symbol-value 'p3sym-c6-global)))
  (makunbound 'p3sym-c6-global)
  ;; 23: the received value remains while the original global is unbound.
  (p3sym-c6-out (list received (boundp 'p3sym-c6-global))))
;; 24: a void-variable read does not overwrite the receiving destination.
(p3sym-c6-out
 (let ((answer 23))
   (list (condition-case condition
             (setq answer (symbol-value 'p3sym-c6-global))
           (void-variable (car condition)))
         answer)))
(unless (= p3sym-c6-row-count 24)
  (error "C6 oracle emitted %s rows, expected 24" p3sym-c6-row-count))
