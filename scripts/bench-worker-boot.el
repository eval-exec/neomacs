;;; bench-worker-boot.el --- Measure worker pool boot latency -*- lexical-binding: t; -*-

;; Run once per process for each pool width; the pool is a process singleton:
;;
;;   NEOMACS_WORKER_THREADS=1 NEOMACS_WORKER_BOOT_TRACE=1 \
;;     target/release/neomacs --batch -Q -l scripts/bench-worker-boot.el
;;   NEOMACS_WORKER_THREADS=16 NEOMACS_WORKER_BOOT_TRACE=1 \
;;     target/release/neomacs --batch -Q -l scripts/bench-worker-boot.el
;;
;; Set NEOMACS_RUNTIME_ROOT to this checkout when running outside its root.
;; Rust phase timings are printed to stderr. This script reports the owner
;; count, first-call return, full first task wait, pool owner boot mean/max,
;; and the residual pool-start/coordination time. It never asserts on time.

(let* ((started (float-time))
       (task (neomacs-worker-run #'identity 42))
       (returned (float-time))
       (value (neomacs-worker-wait task))
       (task-finished (float-time))
       (threads (plist-get (neomacs-worker-stats) :threads)))
  ;; A task may finish on the first ready owner while the rest of the pool is
  ;; still booting. Wait for every owner before reading mean/max boot times.
  (while (< (plist-get (neomacs-worker-stats) :ready) threads)
    (sleep-for 0.01))
  (let* ((finished (float-time))
       (stats (neomacs-worker-stats))
       (owner-mean (plist-get stats :boot-mean-ms))
       (owner-max (plist-get stats :boot-max-ms))
       (total-ms (* 1000.0 (- finished started)))
       (run-ms (* 1000.0 (- returned started)))
       (wait-ms (* 1000.0 (- task-finished returned)))
       (coordination-ms (max 0.0 (- total-ms owner-max))))
  (unless (= value 42)
    (error "worker identity returned %S, expected 42" value))
  (princ
   (format
    (concat "worker-boot threads=%d first_run_return_ms=%.3f "
            "first_task_wait_ms=%.3f pool_ready_total_ms=%.3f "
            "owner_boot_mean_ms=%.3f owner_boot_max_ms=%.3f "
            "spawn_startup_coordination_estimate_ms=%.3f\n")
    threads run-ms wait-ms total-ms owner-mean owner-max coordination-ms))))

(kill-emacs 0)
