;; -*- lexical-binding: nil; -*-
(progn
  (setq auto-save-list-file-name nil create-lockfiles nil file-name-handler-alist nil
        write-region-annotate-functions nil
        cired2-property-weak (make-hash-table :test 'eq :weakness 'key)
        cired2-property-count nil cired2-property-format-calls 0
        cired2-property-selector-calls 0)
  (set-buffer (get-buffer-create "cired2-materialized-property-source"))
  (insert "seed\n")
  (setq buffer-file-name @TARGET@ buffer-auto-save-file-name @TARGET@
        buffer-file-format nil buffer-auto-save-file-format '(cired2-properties)
        coding-system-for-write 'utf-8-unix coding-system-require-warning t
        select-safe-coding-system-function 'cired2-property-selector)
  (set-buffer-modified-p t)
  (fset 'format-annotate-function
    (lambda (_name _start _end _original _index)
      (setq cired2-property-format-calls (1+ cired2-property-format-calls))
      (let* ((key (vector 'annotation-property 'must-survive))
             (text (propertize "A" 'cired2-property key)))
        (puthash key t cired2-property-weak)
        (list (cons 1 text)))))
  (fset 'cired2-property-selector
    (lambda (_start _end _coding _predicate _filename)
      (setq cired2-property-selector-calls (1+ cired2-property-selector-calls)
            write-region-annotations-so-far nil)
      (garbage-collect)
      (setq cired2-property-count (hash-table-count cired2-property-weak))
      (if (= cired2-property-count 1)
          'utf-8-unix
        ;; Stop before the encoder sees materialized text with stale property
        ;; references. This observer never dereferences a reclaimed key.
        (throw 'cired2-property-collected 'collected))))
  (setq cired2-property-result
    (catch 'cired2-property-collected
      (do-auto-save t t)
      'saved))
  (setq cired2-property-observation
    (list cired2-property-result cired2-property-count
          cired2-property-format-calls cired2-property-selector-calls))
  (setq buffer-auto-save-file-name nil)
  (set-buffer-modified-p nil)
  cired2-property-observation)
