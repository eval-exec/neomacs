;;; bench-worker-isolates.el --- Benchmark worker isolate calls -*- lexical-binding: t; -*-

;;; Commentary:

;; Run from the Neomacs source root with:
;;   target/release/neomacs -Q --batch -l scripts/bench-worker-isolates.el

;;; Code:

(require 'json)
(require 'neomacs-worker)

(defun bench-worker--numeric-loop (count)
  (let ((number 0)
        (total 0))
    (while (< number count)
      (setq total (+ total number)
            number (1+ number)))
    total))

(defun bench-worker--measure (function inputs)
  "Return elapsed seconds and results for serial and worker calls."
  (let* ((serial-start (float-time))
         (serial (mapcar function inputs))
         (serial-time (- (float-time) serial-start))
         (parallel-start (float-time))
         (parallel (neomacs-worker-pmap function inputs))
         (parallel-time (- (float-time) parallel-start)))
    (unless (equal serial parallel)
      (error "Worker benchmark result differs from serial result"))
    (list serial-time parallel-time)))

(defun bench-worker--json-document ()
  (concat "{\"items\":["
          (mapconcat (lambda (number) (number-to-string number))
                     (number-sequence 0 35000) ",")
          "]}"))

(defun bench-worker--run ()
  (unless (neomacs-worker-available-p)
    (error "Worker isolate capability is unavailable"))
  (let* ((owners (neomacs-worker-pool-size))
         (processors (num-processors))
         (interpreted (symbol-function 'bench-worker--numeric-loop))
         ;; Byte-code function literals currently fail to cross the isolate
         ;; reader, so each worker compiles the same loop immediately before
         ;; running it; the serial comparison includes the same compile cost.
         (compiled
          (lambda (count)
            (funcall
             (byte-compile
              '(lambda (limit)
                 (let ((number 0)
                       (total 0))
                   (while (< number limit)
                     (setq total (+ total number)
                           number (1+ number)))
                   total)))
             count)))
         (document (bench-worker--json-document))
         (workloads `(("interpreted numeric loop" ,interpreted 100000)
                      ("byte-compiled numeric loop (per call)" ,compiled 100000)
                      ("json-parse-string (~200 KB)"
                       ,(lambda (json)
                          (json-parse-string json :object-type 'alist))
                       ,document)))
         rows)
    ;; Start the pool before measuring and wait for each owner so boot metrics
    ;; describe the complete configured pool.
    (neomacs-worker-wait* (neomacs-worker-call #'identity :warm-up))
    (while (< (plist-get (neomacs-worker-stats) :ready) owners)
      (sleep-for 0.01))
    (let ((stats (neomacs-worker-stats)))
      (dolist (workload workloads)
        (let ((name (nth 0 workload))
              (function (nth 1 workload))
              (input (nth 2 workload)))
          (dolist (task-count (list owners (* 4 owners)))
            (let* ((timings
                    (bench-worker--measure function
                                           (make-list task-count input)))
                   (serial-time (nth 0 timings))
                   (parallel-time (nth 1 timings)))
              (push (list name task-count serial-time parallel-time
                          (if (= parallel-time 0.0)
                              "n/a"
                            (format "%.2fx" (/ serial-time parallel-time))))
                    rows)))))
      (setq rows (nreverse rows))
      (princ "| Processors | Pool owners | Boot mean per owner (ms) | Boot max per owner (ms) | Workload | Tasks | Serial main thread (ms) | Parallel (ms) | Speedup |\n")
      (princ "| ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: |\n")
      (dolist (row rows)
        (princ (format "| %d | %d | %.3f | %.3f | %s | %d | %.3f | %.3f | %s |\n"
                       processors owners
                       (plist-get stats :boot-mean-ms)
                       (plist-get stats :boot-max-ms)
                       (nth 0 row) (nth 1 row)
                       (* 1000 (nth 2 row)) (* 1000 (nth 3 row))
                       (nth 4 row)))))))

(bench-worker--run)

;;; bench-worker-isolates.el ends here
