//! Behavioural parity for the pinned `etaf-algo-lab` workspace models.
//!
//! The laboratories mount inside a batch buffer, so the probes drive the
//! public application API and published Hosts only: catalog and lifecycle,
//! player controls, pathfinding algorithms and unreachable goals, sorting
//! runs and comparisons, and switching laboratories.  Values are reduced to
//! facts (counts, matches, ordering) instead of animated frame text.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'etaf)
(require 'etaf-algo-lab)

(defun neomacs-al-open (lab)
  "Open LAB in a fresh buffer and return (BUFFER RUNTIME)."
  (let ((buffer (generate-new-buffer " *neomacs-algo-*")))
    (etaf-algo-lab-open lab (buffer-name buffer))
    (list buffer (etaf-runtime-for-buffer buffer))))

(defun neomacs-al-close (buffer)
  "Close BUFFER's laboratory and kill BUFFER."
  (ignore-errors (etaf-algo-lab-close (buffer-name buffer)))
  (when (buffer-live-p buffer) (kill-buffer buffer)))

(defun neomacs-al-text (runtime &optional ref)
  "Return RUNTIME's plain text, optionally only the Host REF."
  (let ((bounds (and ref (etaf-host-ref-bounds runtime ref))))
    (with-current-buffer (etaf-runtime-buffer runtime)
      (buffer-substring-no-properties (if bounds (car bounds) (point-min))
                                      (if bounds (cdr bounds) (point-max))))))

(defun neomacs-al-controls (runtime label &optional scope)
  "Return RUNTIME's controls labelled LABEL, optionally inside SCOPE."
  (let ((bounds (and scope (etaf-host-ref-bounds runtime scope))))
    (cl-remove-if-not
     (lambda (entry)
       (and (equal label (plist-get (cdr entry) :aria-label))
            (or (null bounds)
                (let ((position (etaf-host-ref-position runtime (car entry))))
                  (and position (<= (car bounds) position)
                       (< position (cdr bounds)))))))
     (etaf-runtime-host-props-entries runtime))))

(defun neomacs-al-press (runtime ref)
  "Press RUNTIME's public REF."
  (etaf-dispatch-event runtime ref 'press))

(defun neomacs-al-choose (runtime field label)
  "Choose the single visible LABEL control inside FIELD."
  (let ((controls (neomacs-al-controls runtime label field)))
    (unless (= 1 (length controls))
      (error "Expected one %S control, got %d" label (length controls)))
    (neomacs-al-press runtime (caar controls))))

(defun neomacs-al-finish (runtime)
  "Fast-forward RUNTIME to its terminal state."
  (neomacs-al-press runtime 'al-finish))

(defun neomacs-al-counter (runtime label)
  "Read RUNTIME's displayed integer named LABEL, or :none."
  (let ((text (neomacs-al-text runtime)))
    (if (string-match (concat label " +\\([0-9]+\\)") text)
        (string-to-number (match-string 1 text))
      :none)))

(defun neomacs-al-numbers (runtime ref)
  "Read RUNTIME's digit-only tokens inside REF."
  (delq nil
        (mapcar (lambda (token)
                  (and (string-match-p "\\`[0-9]+\\'" token)
                       (string-to-number token)))
                (split-string (neomacs-al-text runtime ref) "[ \t\n]+"))))

(defun neomacs-al-match (runtime ref pattern)
  "Return t when RUNTIME's REF text matches PATTERN."
  (and (string-match-p pattern (neomacs-al-text runtime ref)) t))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-algo-lab", "0.1.1"), "etaf-algo-lab.el")
        .expect("prepare pinned etaf-algo-lab source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_algo_lab_behavior_batch() {
    let cases = vec![
        // The catalog lists both laboratories, colors resolve, and closing
        // releases the mounted runtime and its buffer.
        ParityBatchCase::value(
            "catalog_and_lifecycle",
            r####"
(let* ((opened (neomacs-al-open 'pathfinding))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (let ((facts (list (etaf-algo-lab-labs)
                         (etaf-algo-lab-token :bg)
                         (and (etaf-runtime-mounted-p runtime) t)
                         (and (etaf-app-state buffer) t)
                         (and (string-match-p "寻路实验台"
                                              (neomacs-al-text runtime))
                              t))))
        (etaf-algo-lab-close (buffer-name buffer))
        (list facts
              (buffer-live-p buffer)
              (and (etaf-runtime-for-buffer buffer) t)))
    (neomacs-al-close buffer)))
"####,
            expect![[r##"OK (((pathfinding sorting) "#101925" t t t) nil nil)"##]],
        ),
        // Start, pause, one step and reset publish their status and counters.
        ParityBatchCase::value(
            "player_controls",
            r####"
(let* ((opened (neomacs-al-open 'pathfinding))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (let ((ready (neomacs-al-match runtime 'al-status "准备就绪")))
        (neomacs-al-press runtime 'al-start)
        (let ((running (neomacs-al-match runtime 'al-status "运行中")))
          (neomacs-al-press runtime 'al-pause)
          (let ((paused (neomacs-al-match runtime 'al-status "已暂停")))
            (neomacs-al-press runtime 'al-step)
            (let ((stepped (neomacs-al-counter runtime "探索")))
              (neomacs-al-press runtime 'al-reset)
              (list ready running paused stepped
                    (neomacs-al-counter runtime "探索")
                    (neomacs-al-match runtime 'al-status "准备就绪"))))))
    (neomacs-al-close buffer)))
"####,
            expect!["OK (t t t 1 0 t)"],
        ),
        // A*, Dijkstra and BFS all finish the same detour in five steps.
        ParityBatchCase::value(
            "pathfinding_algorithms",
            r####"
(let* ((opened (neomacs-al-open 'pathfinding))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (progn
        (neomacs-al-press runtime 'pf-clear)
        (neomacs-al-choose runtime 'pf-tool "终点")
        (neomacs-al-press runtime 'pf-cell-2-5)
        (neomacs-al-choose runtime 'pf-tool "障碍")
        (neomacs-al-press runtime 'pf-cell-2-3)
        (mapcar
         (lambda (algorithm)
           (neomacs-al-choose runtime 'pf-algorithm algorithm)
           (neomacs-al-finish runtime)
           (list algorithm
                 (neomacs-al-match runtime 'al-status "已完成")
                 (neomacs-al-match runtime 'al-narration "完成")
                 (neomacs-al-match runtime 'al-counters "路径 +5 步")))
         '("A*" "Dijkstra" "BFS")))
    (neomacs-al-close buffer)))
"####,
            expect![[r#"OK (("A*" t t t) ("Dijkstra" t t t) ("BFS" t t t))"#]],
        ),
        // An enclosed start terminates and explains the missing route.
        ParityBatchCase::value(
            "pathfinding_unreachable",
            r####"
(let* ((opened (neomacs-al-open 'pathfinding))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (progn
        (neomacs-al-press runtime 'pf-clear)
        (dolist (ref '(pf-cell-2-1 pf-cell-2-3 pf-cell-1-2 pf-cell-3-2))
          (neomacs-al-press runtime ref))
        (neomacs-al-finish runtime)
        (list (neomacs-al-match runtime 'al-status "已完成")
              (neomacs-al-match runtime 'al-narration "墙")
              (neomacs-al-match runtime 'al-counters "路径 +-- 步")))
    (neomacs-al-close buffer)))
"####,
            expect!["OK (t t t)"],
        ),
        // Every sorting algorithm ends with the same 14 values in ascending
        // order and a non-zero comparison count.
        ParityBatchCase::value(
            "sorting_all_algorithms",
            r####"
(let* ((opened (neomacs-al-open 'sorting))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (mapcar
       (lambda (algorithm)
         (neomacs-al-choose runtime 'sort-algorithm algorithm)
         (neomacs-al-finish runtime)
         (let ((numbers (neomacs-al-numbers runtime 'sort-values))
               (comparisons (neomacs-al-counter runtime "比较")))
           (prog1 (list algorithm (length numbers)
                        (equal numbers (sort (copy-sequence numbers) #'<))
                        (and (numberp comparisons) (> comparisons 0)))
             (neomacs-al-press runtime 'al-reset))))
       '("冒泡" "插入" "选择" "归并"))
    (neomacs-al-close buffer)))
"####,
            expect![[r#"OK (("冒泡" 14 t t) ("插入" 14 t t) ("选择" 14 t t) ("归并" 14 t t))"#]],
        ),
        // A headless comparison names every algorithm; merge compares fewer
        // times than bubble sort and the live run stays untouched.
        ParityBatchCase::value(
            "sorting_compare",
            r####"
(let* ((opened (neomacs-al-open 'sorting))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (progn
        (neomacs-al-press runtime 'al-compare)
        (let ((research (neomacs-al-text runtime 'al-research)))
          (let ((merge (progn (string-match "归并：比较 \\([0-9]+\\)" research)
                              (string-to-number (match-string 1 research))))
                (bubble (progn (string-match "冒泡：比较 \\([0-9]+\\)" research)
                               (string-to-number (match-string 1 research)))))
            (list (mapcar (lambda (name)
                            (and (string-match-p (regexp-quote name) research)
                                 t))
                          '("冒泡" "插入" "选择" "归并"))
                  (numberp merge) (numberp bubble) (< merge bubble)
                  (neomacs-al-counter runtime "比较")))))
    (neomacs-al-close buffer)))
"####,
            expect!["OK ((t t t t) t t t 0)"],
        ),
        // Switching laboratories disposes the running player and mounts the
        // sorting hosts instead.
        ParityBatchCase::value(
            "switching_lab",
            r####"
(let* ((opened (neomacs-al-open 'pathfinding))
       (buffer (car opened))
       (runtime (cadr opened)))
  (unwind-protect
      (progn
        (neomacs-al-press runtime 'al-start)
        (let ((running (neomacs-al-match runtime 'al-status "运行中")))
          (neomacs-al-choose runtime 'al-catalog "排序实验台")
          (let ((ready (neomacs-al-match runtime 'al-status "已完成\\|准备就绪"))
                (sort (and (etaf-host-ref-bounds runtime 'sort-values) t))
                (path-gone (not (etaf-runtime-host-props-for
                                 runtime 'pf-cell-0-0))))
            (neomacs-al-press runtime 'al-step)
            (list running ready sort path-gone
                  (> (neomacs-al-counter runtime "比较") 0)))))
    (neomacs-al-close buffer)))
"####,
            expect!["OK (t t t t t)"],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-algo-lab-behavior", "etaf-algo-lab", &cases);
}
