//! Re-homed bootstrap window-command tests (P1 B1): bodies are the display
//! originals. They need the full runtime boot, which only the core test tree
//! can compose, so the two local helpers move with them; the helper bodies
//! are unchanged.
use super::*;

use crate::emacs_core::window_cmds::FrameDomain;
use crate::test_utils::runtime_startup_context;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_eval::emacs_core::eval::data::window_state::WindowDomain;
use neovm_value::symbol::CanonicalAccess;

fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}

/// Evaluate FORMS in a cached runtime-startup evaluator and return formatted
/// results, matching the common bootstrap test pattern. This is the core
/// test-tree helper's implementation; core's cfg(test) test_utils is not
/// reachable from an integration-test crate, so the moved family carries the
/// same body on public paths.
fn runtime_startup_eval_all(src: &str) -> Vec<String> {
    let mut eval = crate::test_utils::runtime_startup_context();
    let forms = crate::emacs_core::value_reader::read_all(
        src,
        neovm_eval::emacs_core::eval::VariableView::canonical(CanonicalAccess::TEST, &test_ob()),
    )
    .expect("parse");
    // Root parsed forms across the eval loop. Heap literals like bignums,
    // strings, and cons cells are otherwise invisible to the GC until the
    // evaluator reaches them, which can corrupt bootstrap tests that call
    // into bytecode and trigger collection mid-eval.
    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    let results = forms
        .into_iter()
        .map(|form| {
            let result = eval.eval_form(form);
            crate::emacs_core::format_eval_result(&result)
        })
        .collect();
    eval.restore_specpdl_roots(roots);
    results
}

const GNU_WINDOW_ARGUMENT_DOMAINS: &[(&str, WindowDomain)] = &[
    ("delete-window-internal", WindowDomain::Any),
    ("internal-show-cursor-p", WindowDomain::Any),
    ("resize-mini-window-internal", WindowDomain::Live),
    ("run-window-scroll-functions", WindowDomain::Live),
    ("set-minibuffer-window", WindowDomain::Any),
    ("uncombine-window", WindowDomain::Valid),
    ("window-bottom-divider-width", WindowDomain::Live),
    ("window-buffer", WindowDomain::Any),
    ("window-bump-use-time", WindowDomain::Live),
    ("window-combination-limit", WindowDomain::Valid),
    ("window-cursor-info", WindowDomain::Live),
    ("window-cursor-type", WindowDomain::Live),
    ("window-dedicated-p", WindowDomain::Live),
    ("window-display-table", WindowDomain::Live),
    ("window-frame", WindowDomain::Valid),
    ("window-fringes", WindowDomain::Live),
    ("window-header-line-height", WindowDomain::Live),
    ("window-hscroll", WindowDomain::Live),
    ("window-left-child", WindowDomain::Valid),
    ("window-left-column", WindowDomain::Valid),
    ("window-margins", WindowDomain::Live),
    ("window-minibuffer-p", WindowDomain::Valid),
    ("window-mode-line-height", WindowDomain::Live),
    ("window-new-normal", WindowDomain::Valid),
    ("window-new-pixel", WindowDomain::Valid),
    ("window-new-total", WindowDomain::Valid),
    ("window-next-buffers", WindowDomain::Live),
    ("window-next-sibling", WindowDomain::Valid),
    ("window-old-body-pixel-height", WindowDomain::Live),
    ("window-old-body-pixel-width", WindowDomain::Live),
    ("window-old-buffer", WindowDomain::Any),
    ("window-old-pixel-height", WindowDomain::Valid),
    ("window-old-pixel-width", WindowDomain::Valid),
    ("window-old-point", WindowDomain::Live),
    ("window-parameters", WindowDomain::Valid),
    ("window-parent", WindowDomain::Valid),
    ("window-pixel-height", WindowDomain::Valid),
    ("window-pixel-left", WindowDomain::Valid),
    ("window-pixel-top", WindowDomain::Valid),
    ("window-pixel-width", WindowDomain::Valid),
    ("window-point", WindowDomain::Live),
    ("window-prev-buffers", WindowDomain::Live),
    ("window-prev-sibling", WindowDomain::Valid),
    ("window-right-divider-width", WindowDomain::Live),
    ("window-scroll-bar-height", WindowDomain::Live),
    ("window-scroll-bar-width", WindowDomain::Live),
    ("window-scroll-bars", WindowDomain::Live),
    ("window-start", WindowDomain::Live),
    ("window-tab-line-height", WindowDomain::Live),
    ("window-top-child", WindowDomain::Valid),
    ("window-top-line", WindowDomain::Valid),
    ("window-use-time", WindowDomain::Live),
];

const GNU_FRAME_ARGUMENT_DOMAINS: &[(&str, FrameDomain, &str)] = &[
    ("current-window-configuration", FrameDomain::Live, "{f}"),
    ("frame--set-was-invisible", FrameDomain::Live, "{f} nil"),
    ("frame-after-make-frame", FrameDomain::Live, "{f} nil"),
    ("frame-bottom-divider-width", FrameDomain::Any, "{f}"),
    ("frame-child-frame-border-width", FrameDomain::Any, "{f}"),
    ("frame-focus", FrameDomain::Live, "{f}"),
    ("frame-fringe-width", FrameDomain::Any, "{f}"),
    ("frame-id", FrameDomain::Live, "{f}"),
    ("frame-internal-border-width", FrameDomain::Any, "{f}"),
    ("frame-native-height", FrameDomain::Any, "{f}"),
    ("frame-native-width", FrameDomain::Any, "{f}"),
    ("frame-parameter", FrameDomain::Any, "{f} 'probe"),
    ("frame-parameters", FrameDomain::Any, "{f}"),
    ("frame-parent", FrameDomain::Live, "{f}"),
    ("frame-pointer-visible-p", FrameDomain::Any, "{f}"),
    ("frame-position", FrameDomain::Live, "{f}"),
    ("frame-right-divider-width", FrameDomain::Any, "{f}"),
    ("frame-root-frame", FrameDomain::Live, "{f}"),
    ("frame-scale-factor", FrameDomain::Live, "{f}"),
    ("frame-scroll-bar-height", FrameDomain::Any, "{f}"),
    ("frame-scroll-bar-width", FrameDomain::Any, "{f}"),
    ("frame-text-cols", FrameDomain::Any, "{f}"),
    ("frame-text-height", FrameDomain::Any, "{f}"),
    ("frame-text-lines", FrameDomain::Any, "{f}"),
    ("frame-text-width", FrameDomain::Any, "{f}"),
    ("frame-total-cols", FrameDomain::Any, "{f}"),
    ("frame-total-lines", FrameDomain::Any, "{f}"),
    ("frame-window-state-change", FrameDomain::Live, "{f}"),
    ("iconify-frame", FrameDomain::Live, "{f}"),
    ("lower-frame", FrameDomain::Live, "{f}"),
    ("make-frame-invisible", FrameDomain::Live, "{f}"),
    ("make-frame-visible", FrameDomain::Live, "{f}"),
    ("minibuffer-window", FrameDomain::Live, "{f}"),
    ("modify-frame-parameters", FrameDomain::Live, "{f} nil"),
    ("raise-frame", FrameDomain::Live, "{f}"),
    ("redirect-frame-focus", FrameDomain::Any, "{f}"),
    ("redraw-frame", FrameDomain::Live, "{f}"),
    (
        "run-window-configuration-change-hook",
        FrameDomain::Live,
        "{f}",
    ),
    ("set-frame-height", FrameDomain::Live, "{f} 10"),
    ("set-frame-position", FrameDomain::Live, "{f} 0 0"),
    ("set-frame-size", FrameDomain::Live, "{f} 10 10"),
    (
        "set-frame-size-and-position-pixelwise",
        FrameDomain::Live,
        "{f} 10 10 0 0",
    ),
    ("set-frame-width", FrameDomain::Live, "{f} 10"),
    ("set-frame-window-state-change", FrameDomain::Live, "{f}"),
    ("tab-bar-height", FrameDomain::Any, "{f}"),
    ("window-at", FrameDomain::Live, "0 0 {f}"),
    ("window-resize-apply", FrameDomain::Live, "{f}"),
    ("window-resize-apply-total", FrameDomain::Live, "{f}"),
];

const GNU_WINDOW_MULTIARG_DOMAINS: &[(&str, WindowDomain, &str)] = &[
    ("buffer-text-pixel-size", WindowDomain::Live, "nil {w}"),
    (
        "coordinates-in-window-p",
        WindowDomain::Live,
        "'(0 . 0) {w}",
    ),
    ("delete-other-windows-internal", WindowDomain::Valid, "{w}"),
    ("format-mode-line", WindowDomain::Any, "\"\" nil {w}"),
    ("fringe-bitmaps-at-pos", WindowDomain::Any, "nil {w}"),
    ("internal-show-cursor", WindowDomain::Any, "{w} nil"),
    ("pos-visible-in-window-p", WindowDomain::Live, "nil {w}"),
    (
        "set-window-buffer",
        WindowDomain::Live,
        "{w} (current-buffer)",
    ),
    ("set-window-cursor-type", WindowDomain::Live, "{w} nil"),
    ("set-window-dedicated-p", WindowDomain::Live, "{w} nil"),
    ("set-window-display-table", WindowDomain::Live, "{w} nil"),
    ("set-window-fringes", WindowDomain::Live, "{w} nil"),
    ("set-window-hscroll", WindowDomain::Live, "{w} 0"),
    ("set-window-margins", WindowDomain::Live, "{w} nil"),
    ("set-window-new-normal", WindowDomain::Valid, "{w}"),
    ("set-window-new-pixel", WindowDomain::Valid, "{w} 10"),
    ("set-window-new-total", WindowDomain::Valid, "{w} 10"),
    ("set-window-next-buffers", WindowDomain::Live, "{w} nil"),
    ("set-window-parameter", WindowDomain::Any, "{w} 'probe nil"),
    ("set-window-point", WindowDomain::Live, "{w} 1"),
    ("set-window-prev-buffers", WindowDomain::Live, "{w} nil"),
    ("set-window-scroll-bars", WindowDomain::Live, "{w}"),
    ("set-window-start", WindowDomain::Live, "{w} 1"),
    ("set-window-vscroll", WindowDomain::Live, "{w} 0"),
    ("window-body-height", WindowDomain::Live, "{w}"),
    ("window-body-width", WindowDomain::Live, "{w}"),
    ("window-end", WindowDomain::Live, "{w}"),
    ("window-line-height", WindowDomain::Live, "nil {w}"),
    ("window-lines-pixel-dimensions", WindowDomain::Live, "{w}"),
    ("window-normal-size", WindowDomain::Valid, "{w}"),
    ("window-parameter", WindowDomain::Any, "{w} 'probe"),
    ("window-text-height", WindowDomain::Live, "{w}"),
    ("window-text-pixel-size", WindowDomain::Live, "{w}"),
    ("window-text-width", WindowDomain::Live, "{w}"),
    ("window-total-height", WindowDomain::Valid, "{w}"),
    ("window-total-width", WindowDomain::Valid, "{w}"),
    ("window-vscroll", WindowDomain::Live, "{w}"),
];

/// Renders the frame's window tree as a structure of combination direction,
/// buffer names and column spans.  Column spans (not full edges) keep the
/// assertion comparable with GNU `--batch`, whose root window sits at a
/// different y origin than this harness's frame.
const WINDOW_TREE_RENDERER: &str = r#"
(defun neo--wt (node)
  (cond
   ((windowp node)
    (list (buffer-name (window-buffer node))
          (nth 0 (window-edges node))
          (nth 2 (window-edges node))))
   ((consp node)
    (cons (if (car node) 'v 'h) (mapcar #'neo--wt (cddr node))))))
(defun neo--tree () (neo--wt (car (window-tree))))
"#;

fn eval_window_tree(body: &str) -> String {
    crate::test_utils::init_test_tracing();
    let src = format!("{WINDOW_TREE_RENDERER}\n{body}");
    bootstrap_eval_with_frame(&src)
        .pop()
        .expect("at least one result")
}

/// The reported bug: with a `left` side window present, a new `right` side
/// window must land at the frame's far right as the LAST child of the root
/// combination -- not between the main windows.
///
/// GNU Emacs 31 `--batch`, 80-column frame:
///   (h ("*left-side*" 0 20) (h ("*scratch*" 20 40) ("*scratch*" 40 60))
///      ("*right-side*" 60 80))
#[test]
fn right_side_window_attaches_at_frame_far_right_when_left_side_window_exists() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*left-side\\*"  (display-buffer-in-side-window)
                      (side . left)  (window-width . 20))
                     ("\\*right-side\\*" (display-buffer-in-side-window)
                      (side . right) (window-width . 20))))
             (display-buffer (get-buffer-create "*left-side*"))
             (split-window (selected-window) nil 'right)
             (display-buffer (get-buffer-create "*right-side*"))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*left-side*\" 0 20) \
         (h (\"*scratch*\" 20 40) (\"*scratch*\" 40 60)) \
         (\"*right-side*\" 60 80))"
    );
}

/// The intermediate step of the same scenario: `split-window` binds
/// `window-combination-limit` to t when the split target has a side-window
/// sibling (lisp/window.el, "If `window-combination-resize' is 'side and
/// window has a side window sibling"), so the main area must be wrapped in
/// its own internal node rather than flattened into the root.
#[test]
fn splitting_next_to_a_side_window_wraps_the_main_area_in_a_new_parent() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*left-side\\*" (display-buffer-in-side-window)
                      (side . left) (window-width . 20))))
             (display-buffer (get-buffer-create "*left-side*"))
             (split-window (selected-window) nil 'right)
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*left-side*\" 0 20) \
         (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))"
    );
}

/// `window-combination-limit` is a DYNAMIC variable read by
/// `split-window-internal`; binding it to t must force a fresh parent even
/// though the target's parent is iso-combined.
#[test]
fn split_window_honors_dynamic_window_combination_limit() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit t))
               (split-window (selected-window) nil 'right)
               (split-window (selected-window) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40)) (\"*scratch*\" 40 80))"
    );
}

/// The control case: with the variable nil, an iso-combined parent is reused
/// and the tree stays flat.  (This already passed before the fix; it guards
/// against over-correcting into always making a new parent.)
#[test]
fn split_window_reuses_iso_combined_parent_when_limit_is_nil() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window (selected-window) nil 'right)
               (split-window (selected-window) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))"
    );
}

/// Reusing the parent must not be gated on the split target being a LEAF.
/// GNU splices a new sibling next to an internal node just the same, which is
/// exactly how a side window is attached beside the main-window group.
#[test]
fn split_window_reuses_iso_combined_parent_when_target_is_internal() {
    let tree = eval_window_tree(
        r#"(progn
             (split-window (selected-window) nil 'right)
             (let ((window-combination-limit t))
               (split-window (selected-window) nil 'right))
             (let ((window-combination-limit nil)
                   (ignore-window-parameters t))
               (split-window (window-parent (selected-window)) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (h (\"*scratch*\" 0 10) (\"*scratch*\" 10 20)) \
         (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))"
    );
}

/// Deleting the MIDDLE of three siblings gives its columns to the window on
/// its left.  GNU Emacs 31 `--batch`, 80-column frame: `(h 0-40 40-80)`.
#[test]
fn deleting_a_middle_window_gives_its_space_to_the_previous_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 1 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))");
}

/// Deleting the LAST of three siblings gives its columns to the one before it;
/// the first window must not move.  GNU: `(h 0-20 20-80)`.
#[test]
fn deleting_the_last_window_gives_its_space_to_the_previous_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 2 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 80))");
}

/// Deleting the FIRST sibling has no previous sibling, so the NEXT one absorbs
/// the space and slides left; the third window must not move.  GNU:
/// `(h 0-40 40-80)`.
#[test]
fn deleting_the_first_window_gives_its_space_to_the_next_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 0 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))");
}

/// A side window keeps its width when an unrelated window is deleted: the
/// freed columns belong to the main group's sibling, not to every child of the
/// root.  GNU: `(h ("*L*" 0 20) (h 20-50 50-80))`.
#[test]
fn deleting_a_right_side_window_leaves_the_left_side_window_width_intact() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*L\\*" (display-buffer-in-side-window)
                      (side . left) (window-width . 20))
                     ("\\*R\\*" (display-buffer-in-side-window)
                      (side . right) (window-width . 20))))
             (display-buffer (get-buffer-create "*L*"))
             (split-window (window-main-window) nil 'right)
             (display-buffer (get-buffer-create "*R*"))
             (delete-window (get-buffer-window "*R*"))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))"
    );
}

/// GNU `recombine_windows` (src/window.c:2606-2650), called on the window
/// promoted into its parent's slot (src/window.c:5801): if that window is
/// itself a combination along the SAME axis as its new parent, and its stored
/// `combination_limit` slot is nil, its children are spliced into the parent
/// and it disappears.
///
/// The combination must be UNSEALED for this to fire, so the nesting here is
/// built with *orthogonal* splits — a `window-combination-limit t` split seals
/// the parent it creates, and GNU skips sealed nodes.
///
/// GNU Emacs 31 `--batch`, 80-column frame: `(h 0-40 40-60 60-80)`.
#[test]
fn deleting_a_window_recombines_the_promoted_child_into_an_iso_parent() {
    let tree = eval_window_tree(
        r#"(progn
             (let* ((w (selected-window))
                    (n1 (split-window w nil 'right)))
               (select-window n1)
               (let ((n2 (split-window n1 nil 'below)))
                 (select-window n2)
                 (split-window n2 nil 'right)
                 (delete-window n1)))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 60) (\"*scratch*\" 60 80))"
    );
}

/// A SEALED combination must survive promotion intact — `set-window-combination-limit`
/// exists precisely to stop the merge (and `window--make-major-side-window`
/// relies on it, Bug#80665).
#[test]
fn deleting_a_window_does_not_recombine_a_sealed_promoted_child() {
    let tree = eval_window_tree(
        r#"(progn
             (let* ((w (selected-window))
                    (n1 (split-window w nil 'right)))
               (select-window n1)
               (let ((n2 (split-window n1 nil 'below)))
                 (select-window n2)
                 (split-window n2 nil 'right)
                 (set-window-combination-limit (window-parent (selected-window)) t)
                 (delete-window n1)))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 40) (h (\"*scratch*\" 40 60) (\"*scratch*\" 60 80)))"
    );
}

fn bootstrap_eval_with_frame(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn bootstrap_eval_one_with_frame(src: &str) -> String {
    bootstrap_eval_with_frame(src)
        .into_iter()
        .next()
        .expect("result")
}

#[test]
fn bootstrap_window_command_boundary_matches_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(list (subrp (symbol-function 'select-window))
                 (subrp (symbol-function 'split-window-internal))
                 (subrp (symbol-function 'delete-window-internal))
                 (subrp (symbol-function 'delete-other-windows-internal))
                 (subrp (symbol-function 'other-window-for-scrolling))
                 (subrp (symbol-function 'display-buffer))
                 (subrp (symbol-function 'switch-to-buffer))
                 (subrp (symbol-function 'pop-to-buffer))
                 (subrp (symbol-function 'other-window))
                 (subrp (symbol-function 'delete-window))
                 (subrp (symbol-function 'delete-other-windows))
                 (subrp (symbol-function 'split-window))
                 (subrp (symbol-function 'split-window-below))
                 (subrp (symbol-function 'split-window-right)))"#,
    );
    assert_eq!(result, "OK (t t t t t nil nil nil nil nil nil nil nil nil)");
}

#[test]
fn frame_root_window_window_valid_and_minibuffer_activity_semantics() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(window-valid-p (selected-window))
         (window-valid-p (minibuffer-window))
         (window-valid-p nil)
         (window-valid-p 999999)
         (window-valid-p 'foo)
         (eq (frame-root-window) (selected-window))
         (eq (frame-root-window (selected-frame)) (selected-window))
         (eq (frame-root-window (selected-window)) (selected-window))
         (eq (frame-root-window (minibuffer-window)) (selected-window))
         (eq (window-next-sibling (frame-root-window)) (minibuffer-window))
         (eq (window-prev-sibling (minibuffer-window)) (frame-root-window))
         (minibuffer-selected-window)
         (active-minibuffer-window)
         (minibuffer-window-active-p (minibuffer-window))
         (minibuffer-window-active-p (selected-window))
         (minibuffer-window-active-p nil)
         (minibuffer-window-active-p 999999)
         (minibuffer-window-active-p 'foo)
         (condition-case err (window-valid-p) (error err))
         (condition-case err (window-valid-p nil nil) (error err))
         (condition-case err (frame-root-window 999999) (error err))
         (condition-case err (frame-root-window 'foo) (error err))
         (condition-case err (frame-root-window nil nil) (error err))
         (condition-case err (minibuffer-selected-window nil) (error err))
         (condition-case err (active-minibuffer-window nil) (error err))
         (condition-case err (minibuffer-window-active-p) (error err))
         (condition-case err (minibuffer-window-active-p nil nil) (error err))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK nil");
    assert_eq!(out[3], "OK nil");
    assert_eq!(out[4], "OK nil");
    assert_eq!(out[5], "OK t");
    assert_eq!(out[6], "OK t");
    assert_eq!(out[7], "OK t");
    assert_eq!(out[8], "OK t");
    assert_eq!(out[9], "OK t");
    assert_eq!(out[10], "OK t");
    assert_eq!(out[11], "OK nil");
    assert_eq!(out[12], "OK nil");
    assert_eq!(out[13], "OK nil");
    assert_eq!(out[14], "OK nil");
    assert_eq!(out[15], "OK nil");
    assert_eq!(out[16], "OK nil");
    assert_eq!(out[17], "OK nil");
    assert_eq!(out[18], "OK (wrong-number-of-arguments window-valid-p 0)");
    assert_eq!(out[19], "OK (wrong-number-of-arguments window-valid-p 2)");
    assert_eq!(out[20], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[21], "OK (wrong-type-argument frame-live-p foo)");
    assert_eq!(
        out[22],
        "OK (wrong-number-of-arguments frame-root-window 2)"
    );
    assert_eq!(
        out[23],
        "OK (wrong-number-of-arguments minibuffer-selected-window 1)"
    );
    assert_eq!(
        out[24],
        "OK (wrong-number-of-arguments active-minibuffer-window 1)"
    );
    // GNU `minibuffer-window-active-p` is a Lisp defun (window.el),
    // so its arity errors carry the (MIN . MAX) tuple, not the symbol.
    assert_eq!(out[25], "OK (wrong-number-of-arguments (1 . 1) 0)");
    assert_eq!(out[26], "OK (wrong-number-of-arguments (1 . 1) 2)");
}

#[test]
fn frame_root_window_p_semantics_and_errors() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(frame-root-window-p (selected-window))
         (frame-root-window-p (minibuffer-window))
         (condition-case err (frame-root-window-p 999999) (error err))
         (condition-case err (frame-root-window-p 'foo) (error err))
         (condition-case err (frame-root-window-p) (error (car err)))
         (condition-case err (frame-root-window-p nil nil) (error (car err)))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK nil");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p foo)");
    assert_eq!(out[4], "OK wrong-number-of-arguments");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn get_buffer_window_list_returns_matching_windows() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_with_frame("(length (get-buffer-window-list (window-buffer)))");
    assert_eq!(result[0], "OK 1");
}

#[test]
fn get_buffer_window_and_list_match_optional_and_missing_buffer_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((vm-gbwl-live (generate-new-buffer \"gbwl-live\"))
               (vm-gbwl-dead (generate-new-buffer \"gbwl-dead\")))
           (list
            (windowp (condition-case err (get-buffer-window) (error err)))
            (windowp (condition-case err (get-buffer-window nil) (error err)))
            (condition-case err (get-buffer-window \"missing\") (error err))
            (windowp (get-buffer-window \"*scratch*\"))
            (length (get-buffer-window-list))
            (length (get-buffer-window-list nil))
            (length (get-buffer-window-list \"*scratch*\"))
            (condition-case err (get-buffer-window-list \"missing\") (error err))
            (condition-case err (get-buffer-window-list 1) (error err))
            (prog1 (condition-case err (get-buffer-window-list vm-gbwl-live) (error err))
              (kill-buffer vm-gbwl-live))
            (progn
              (kill-buffer vm-gbwl-dead)
              (condition-case err (get-buffer-window-list vm-gbwl-dead) (error err)))))",
    );
    assert_eq!(
        results[0],
        "OK (t t nil t 1 1 1 (error \"No such live buffer missing\") (error \"No such buffer 1\") nil (error \"No such live buffer #<killed buffer>\"))"
    );
}

#[test]
fn fit_window_to_buffer_returns_nil_in_batch_mode() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_with_frame("(fit-window-to-buffer)");
    assert_eq!(result[0], "OK nil");
}

#[test]
fn fit_window_to_buffer_invalid_window_designators_signal_error() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (fit-window-to-buffer 999999) (error (car err)))
         (condition-case err (fit-window-to-buffer 'foo) (error (car err)))",
    );
    // GNU's Lisp `fit-window-to-buffer` normalizes WINDOW via
    // `window-normalize-window`, which signals plain `error` for invalid
    // designators.
    assert_eq!(results[0], "OK error");
    assert_eq!(results[1], "OK error");
}

#[test]
fn window_resize_apply_preserves_lisp_computed_vertical_sizes() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((w1 (selected-window))
                  (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) w1 (/ (window-pixel-height w1) 2) nil nil))
                  (root (frame-root-window))
                  (root-pixels (window-pixel-height root))
                  (char-height (frame-char-height)))
             (let* ((frame (window-frame root))
                    (small-pixels (min (* 5 char-height)
                                       (max char-height (- root-pixels char-height))))
                    (large-pixels (- root-pixels small-pixels)))
               (set-window-new-pixel root root-pixels)
               (set-window-new-pixel w1 large-pixels)
               (set-window-new-pixel w2 small-pixels)
             (list (window-resize-apply frame nil)
                     (= (window-pixel-height w1) large-pixels)
                     (= (window-pixel-height w2) small-pixels)
                     (= (+ (window-pixel-height w1)
                           (window-pixel-height w2))
                        root-pixels))))"#,
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn resize_mini_window_internal_applies_staged_pixel_heights() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini))
                  (delta (frame-char-height frame)))
             (set-window-new-pixel root (- root-height delta))
             (set-window-new-pixel mini (+ mini-height delta))
             (list (resize-mini-window-internal mini)
                   (= (window-pixel-height root) (- root-height delta))
                   (= (window-pixel-height mini) (+ mini-height delta))
                   (= (cadr (window-pixel-edges mini))
                      (nth 3 (window-pixel-edges root)))
                   (= (window-new-pixel mini) 0)))"#,
    );
    assert_eq!(result, "OK (t t t t t)");
}

#[test]
fn resize_mini_window_internal_rejects_nonconserving_geometry() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini))
                  (delta (frame-char-height frame)))
             (set-window-new-pixel root (- root-height delta))
             (set-window-new-pixel mini (+ mini-height delta 2))
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn resize_mini_window_internal_rejects_missing_root_staging() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini)))
             (set-window-new-pixel mini mini-height)
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn resize_mini_window_internal_rejects_non_positive_minibuffer_staging() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini)))
             (set-window-new-pixel root (+ root-height mini-height))
             (set-window-new-pixel mini 0)
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn window_resize_minibuffer_grows_and_conserves_pixel_height() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (old-root-height (window-pixel-height root))
                  (old-mini-height (window-pixel-height mini))
                  (old-total (+ old-root-height old-mini-height)))
             (window-resize mini 1)
             (let ((new-root-height (window-pixel-height root))
                   (new-mini-height (window-pixel-height mini)))
               (list (> new-mini-height old-mini-height)
                     (= (+ new-root-height new-mini-height) old-total))))"#,
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn display_buffer_fit_window_to_buffer_shrinks_new_window() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((buf (get-buffer-create "*fit-window-probe*")))
             (with-current-buffer buf
               (erase-buffer)
               (insert "a\nb\nc\n"))
             (let ((window
                    (display-buffer
                     buf
                     '((display-buffer-below-selected)
                       . ((window-height . fit-window-to-buffer))))))
               (list (eq window-combination-limit 'window-size)
                     (window-live-p window)
                     (not (null (window-combined-p window)))
                     (= (window-total-height window) window-min-height))))"#,
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn buffer_text_pixel_size_requires_a_live_window_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's first statement is `struct window *w = decode_live_window (window);`
    // (`src/xdisp.c`), so WINDOW is decoded before BUFFER-OR-NAME is resolved,
    // and an internal or deleted window signals `window-live-p`.
    //
    // neomacs tag-tested it -- `!window.is_nil() && !window.is_window()` --
    // while still reporting `window-live-p`, so it accepted every window object
    // and measured against it.  That is the fourth instance this session of a
    // check that names one decoder's contract and enforces another's.
    //
    // Measured on GNU Emacs 31.1:
    //   (buffer-text-pixel-size nil INTERNAL)  => (wrong-type-argument window-live-p W)
    //   (buffer-text-pixel-size nil DEAD)      => (wrong-type-argument window-live-p W)
    //   (buffer-text-pixel-size "nope" INTERNAL) => window-live-p, NOT the buffer
    //     error -- which is the ordering proof.
    //
    // This subr was invisible to the differential audit: the harness only ever
    // put its probe value in argument position 0, and WINDOW is argument 1.
    //
    // GNU segfaults on `(buffer-text-pixel-size "no-such-buffer" nil)` -- a
    // nonexistent buffer name with a live window.  neomacs raises a clean
    // `error "No buffer named ..."` there and that is deliberately NOT changed
    // to match.
    let results = bootstrap_eval_with_frame(
        "(let* ((live (selected-window))
                (parent (window-parent (progn (split-window-below) (selected-window))))
                (dead (let ((w (split-window-below))) (delete-window w) w))
                (probe (lambda (th) (condition-case e (funcall th)
                                      (wrong-type-argument (car (cdr e)))
                                      (error 'plain-error)))))
           (list (funcall probe (lambda () (buffer-text-pixel-size nil parent)))
                 (funcall probe (lambda () (buffer-text-pixel-size nil dead)))
                 ;; WINDOW is decoded before BUFFER-OR-NAME
                 (funcall probe (lambda () (buffer-text-pixel-size \"no-such-buffer-xyz\" parent)))
                 ;; a live window still measures
                 (funcall probe (lambda () (buffer-text-pixel-size nil live)))
                 (funcall probe (lambda () (buffer-text-pixel-size nil nil)))))",
    );
    assert_eq!(
        results[0],
        "OK (window-live-p window-live-p window-live-p (0 . 0) (0 . 0))"
    );
}

#[test]
fn window_new_normal_starts_at_zero_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's `make_window` initializes the three pending-size slots to 0, right
    // beside the `normal_lines`/`normal_cols` 1.0 this code already mirrors
    // (`src/window.c:4607-4611`):
    //
    //     wset_normal_lines (w, make_float (1.0));
    //     wset_normal_cols  (w, make_float (1.0));
    //     wset_new_total    (w, make_fixnum (0));
    //     wset_new_normal   (w, make_fixnum (0));
    //     wset_new_pixel    (w, make_fixnum (0));
    //
    // so `window-new-normal` answers a NUMBER for every valid window, never
    // nil.  `new_pixel`/`new_total` are `Option<i64>` here and already read as
    // 0 through `unwrap_or(0)`; `new_normal` is a raw `Value` and leaked the
    // `nil` it was constructed with.
    //
    // The minibuffer window is where this shows: it never takes part in a
    // resize, so nothing ever overwrites the initial value.  Measured on GNU
    // Emacs 31.1: `(window-new-normal (minibuffer-window))` => 0, neomacs nil.
    //
    // Initializing the slot is NOT separable from staging it in the split.
    // `window_resize_apply` copies `new_normal` into `normal_lines`/
    // `normal_cols` behind a `NUMBERP` guard (`src/window.c:4829,4838`), so
    // while the slot was nil the copy was simply skipped.  Making it a number
    // without also staging the new parent's value -- GNU's
    // `wset_new_normal (p, horflag ? o->normal_cols : o->normal_lines)` --
    // makes that copy overwrite the parent's inherited fraction with 0, which
    // is what `window_tree_navigation_and_normal_size_match_gnu_runtime` and
    // the `compat_window_semantics` oracle both caught.
    let results = bootstrap_eval_with_frame(
        "(let* ((mini (minibuffer-window))
                (l (selected-window))
                (r (split-window-right)))
           (list (window-new-normal mini)
                 (window-new-total mini)
                 (window-new-pixel mini)
                 ;; windows the split touched keep their staged fractions
                 (window-new-normal l)
                 (window-new-normal r)
                 ;; and the new parent is staged from the OLD window's
                 ;; pre-split fraction, not left at 0
                 (window-new-normal (window-parent l))
                 (window-normal-size (window-parent l))))",
    );
    assert_eq!(results[0], "OK (0 80 0 0.5 0.5 1.0 1.0)");
}

#[test]
fn coordinates_in_window_p_takes_frame_relative_coordinates_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's COORDINATES are FRAME-relative -- the docstring says "distances
    // measured in characters from the upper-left corner of the frame" -- and
    // the answer is window-relative (`src/window.c`):
    //
    //     case ON_TEXT:
    //       x -= window_box_left (w, TEXT_AREA);
    //       y -= WINDOW_TOP_EDGE_Y (w);
    //       return Fcons (...);
    //
    // with the non-text regions answering a SYMBOL instead: `mode-line',
    // `tab-line', `header-line', `vertical-line'.
    //
    // neomacs tested the input against the window's SIZE -- i.e. read it as
    // window-relative -- and returned the input cons unchanged, so every
    // window whose origin is not (0,0) answered for the wrong region:
    //
    //   (coordinates-in-window-p '(5 . 24) MINI)  GNU (5 . 0)  neomacs nil
    //   (coordinates-in-window-p '(0 . 0)  MINI)  GNU nil      neomacs (0 . 0)
    //   (coordinates-in-window-p '(5 . 23) ROOT)  GNU mode-line neomacs (5 . 23)
    //
    // It only agreed for a window at the frame origin, where the two readings
    // coincide -- which is why a probe that only tried (0 . 0) saw one row.
    //
    // Oracle measured on GNU Emacs 31.1 with a side-by-side split, whose
    // geometry neomacs reproduces exactly: L=(0 0 40 24) R=(40 0 80 24)
    // mini=(0 24 80 25).
    let results = bootstrap_eval_with_frame(
        "(let* ((l (selected-window)) (r (split-window-right)) (mini (minibuffer-window)))
           (list (window-edges l) (window-edges r) (window-edges mini)
                 (coordinates-in-window-p '(0 . 0) l)
                 (coordinates-in-window-p '(5 . 3) l)
                 (coordinates-in-window-p '(39 . 3) l)
                 (coordinates-in-window-p '(40 . 3) l)
                 (coordinates-in-window-p '(5 . 23) l)
                 (coordinates-in-window-p '(5 . 24) l)
                 (coordinates-in-window-p '(5 . 3) r)
                 (coordinates-in-window-p '(40 . 3) r)
                 (coordinates-in-window-p '(45 . 3) r)
                 (coordinates-in-window-p '(0 . 0) mini)
                 (coordinates-in-window-p '(5 . 24) mini)
                 ;; float coordinates answer in char units, as GNU does
                 (coordinates-in-window-p '(5.5 . 3.0) l)))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 40 24) (40 0 80 24) (0 24 80 25) (0 . 0) (5 . 3) vertical-line nil mode-line nil nil (0 . 3) (5 . 3) nil (5 . 0) (5 . 3))"
    );
}

#[test]
fn internal_show_cursor_p_accepts_any_window_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's whole body is one line (`src/dispnew.c`):
    //
    //     return decode_any_window (window)->cursor_off_p ? Qnil : Qt;
    //
    // `decode_any_window` is the widest decoder there is -- an INTERNAL window
    // and a DELETED one are both windows, so both are accepted and answer `t`.
    //
    // neomacs guarded it with a helper that required a LIVE window while
    // reporting `windowp`: the predicate named `decode_any_window`'s contract
    // but the check enforced `decode_live_window`'s.  That is the same
    // name-divorced-from-check shape as `expect_frame_live_or_nil` and
    // `expect_window_live_or_nil`, in a third place.
    let results = bootstrap_eval_with_frame(
        "(let* ((live (selected-window))
                (parent (window-parent (progn (split-window-below) (selected-window))))
                (dead (let ((w (split-window-below))) (delete-window w) w))
                (probe (lambda (arg) (condition-case e (internal-show-cursor-p arg)
                                       (wrong-type-argument (car (cdr e)))))))
           (list (funcall probe live) (funcall probe parent) (funcall probe dead)
                 (condition-case e (internal-show-cursor-p 'foo)
                   (wrong-type-argument (car (cdr e))))))",
    );
    assert_eq!(results[0], "OK (t t t windowp)");
}

#[test]
fn set_minibuffer_window_returns_the_window_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU ends with `return window;` (`src/minibuf.c`):
    //
    //     CHECK_WINDOW (window);
    //     if (! MINI_WINDOW_P (XWINDOW (window)))
    //       error ("Window is not a minibuffer window");
    //     minibuf_window = window;
    //     return window;
    //
    // neomacs returned nil from the success path.
    //
    // NOTE: the `minibuf_window = window` assignment is still NOT implemented
    // -- there is no global minibuffer-window slot to write, only a per-frame
    // one -- so this fixes the return value and not the effect.  In practice
    // the effect is unobservable here because the guard only admits a window
    // that already IS some frame's minibuffer window, but it is a real gap and
    // is called out rather than papered over.
    let results = bootstrap_eval_with_frame(
        "(list (eq (set-minibuffer-window (minibuffer-window)) (minibuffer-window))
               (condition-case e (set-minibuffer-window (selected-window))
                 (error (car (cdr e))))
               (condition-case e (set-minibuffer-window 'foo)
                 (wrong-type-argument (car (cdr e)))))",
    );
    assert_eq!(
        results[0],
        "OK (t \"Window is not a minibuffer window\" windowp)"
    );
}

#[test]
fn window_line_height_returns_nil_in_batch_after_decoding_window() {
    crate::test_utils::init_test_tracing();
    // GNU bails out of `Fwindow_line_height' before it ever looks at LINE
    // (`src/window.c'):
    //
    //     w = decode_live_window (window);
    //     if (noninteractive || w->pseudo_window_p)
    //       return Qnil;
    //     ...
    //     CHECK_FIXNUM (line);      /* only reached with a display matrix */
    //
    // so in batch EVERY call returns nil, whatever LINE is -- and the WINDOW
    // decode still happens FIRST, so a bad WINDOW signals even though the
    // result would have been nil.  neomacs decoded WINDOW correctly but then
    // fell through to a `CHECK_FIXNUM'-equivalent on LINE, signalling
    // `integerp' where GNU returns nil:
    //
    //   (window-line-height SOME-WINDOW)   GNU nil   neomacs (... integerp W)
    //
    // This is a runtime contract (`noninteractive'), not an `#ifdef', so it is
    // a real divergence rather than a build-configuration artifact.
    let results = bootstrap_eval_with_frame(
        "(let* ((live (selected-window))
                (parent (window-parent (progn (split-window-below) (selected-window))))
                (dead (let ((w (split-window-below))) (delete-window w) w))
                (probe (lambda (th) (condition-case e (funcall th)
                                      (wrong-type-argument (car (cdr e)))))))
           (list (funcall probe (lambda () (window-line-height 0)))
                 ;; a non-integer LINE is NOT an error in batch -- nil wins
                 (funcall probe (lambda () (window-line-height live)))
                 ;; but WINDOW is still decoded before the bail-out
                 (funcall probe (lambda () (window-line-height 0 parent)))
                 (funcall probe (lambda () (window-line-height 0 dead)))))",
    );
    assert_eq!(results[0], "OK (nil nil window-live-p window-live-p)");
}

#[test]
fn resize_mini_window_internal_requires_a_live_window_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU spells it `CHECK_LIVE_WINDOW (window)' (`src/window.c') -- a
    // REQUIRED argument, so nil is rejected rather than defaulted, and an
    // internal or deleted window signals `window-live-p'.
    //
    // neomacs accepted any window OBJECT (`as_window_id' succeeds for an
    // internal or dead one) and only failed later, when the live-only frame
    // lookup missed, with a plain `error "Window not found"' -- reporting a
    // condition GNU never reaches.
    let results = bootstrap_eval_with_frame(
        "(let* ((parent (window-parent (progn (split-window-below) (selected-window))))
                (dead (let ((w (split-window-below))) (delete-window w) w))
                (probe (lambda (arg) (condition-case e
                                         (progn (resize-mini-window-internal arg) 'no-error)
                                       (wrong-type-argument (car (cdr e)))
                                       (error 'plain-error)))))
           (list (funcall probe parent) (funcall probe dead) (funcall probe nil)))",
    );
    assert_eq!(results[0], "OK (window-live-p window-live-p window-live-p)");
}

#[test]
fn window_old_size_subrs_reject_the_windows_gnu_rejects() {
    crate::test_utils::init_test_tracing();
    // Verified against `src/window.c`, every decode unguarded:
    //
    //   window-old-body-pixel-width   decode_live_window  -> window-live-p
    //   window-old-body-pixel-height  decode_live_window  -> window-live-p
    //   window-lines-pixel-dimensions decode_live_window  -> window-live-p
    //   window-old-pixel-width        decode_valid_window -> window-valid-p
    //   window-old-pixel-height       decode_valid_window -> window-valid-p
    //
    // These went through `expect_window_live_or_nil` /
    // `expect_window_valid_or_nil`, which differ from each other ONLY in the
    // predicate string: both just tag-test `is_window()`.  A helper with no
    // `FrameManager` in hand cannot tell a live window from an internal or a
    // deleted one, so all five accepted every window object and returned a
    // value where GNU signals.
    //
    // `window-combination-limit` is a different shape: GNU spells it
    // `CHECK_VALID_WINDOW (window)` with NO nil defaulting, so nil is simply
    // not a valid window.  neomacs resolved nil to the selected window and
    // then failed further in with a plain `error`.
    let results = bootstrap_eval_with_frame(
        "(let* ((live (selected-window))
                (parent (window-parent (progn (split-window-below) (selected-window))))
                (dead (let ((w (split-window-below))) (delete-window w) w))
                (probe (lambda (fn arg)
                         (condition-case e (progn (funcall fn arg) 'no-error)
                           (wrong-type-argument (car (cdr e)))
                           (error 'plain-error)))))
           (list (funcall probe 'window-old-body-pixel-width parent)
                 (funcall probe 'window-old-body-pixel-height dead)
                 (funcall probe 'window-lines-pixel-dimensions parent)
                 (funcall probe 'window-old-pixel-width dead)
                 (funcall probe 'window-old-pixel-height dead)
                 (funcall probe 'window-combination-limit nil)
                 ;; a LIVE window must still work everywhere
                 (funcall probe 'window-old-body-pixel-width live)))",
    );
    assert_eq!(
        results[0],
        "OK (window-live-p window-live-p window-live-p window-valid-p window-valid-p window-valid-p no-error)"
    );
}

#[test]
fn every_frame_subr_decodes_in_gnus_domain() {
    crate::test_utils::init_test_tracing();
    // A non-frame must signal the domain's predicate.  That is the half
    // testable from Lisp: `make-frame` fails in batch (in GNU too), so no
    // DELETED frame can be built here -- the `Live`-rejects-a-dead-frame half
    // is covered from Rust by
    // `live_frame_subrs_reject_a_deleted_frame_like_gnu`.
    //
    // Rejection happens before any mutation, so the `set-frame-*` and
    // `iconify-frame` members are safe to call.
    let mut cases = String::new();
    for (name, domain, args) in GNU_FRAME_ARGUMENT_DOMAINS {
        let call = args.replace("{f}", "'not-a-frame");
        cases.push_str(&format!(
            "(probe \"{name}\" '{pred} (lambda () ({name} {call})))\n",
            pred = domain.predicate()
        ));
    }

    let src = format!(
        "(let ((bad nil))
           (fset 'probe
             (lambda (fn want thunk)
               (let ((got (condition-case e (progn (funcall thunk) 'no-error)
                            (wrong-type-argument (car (cdr e)))
                            (error 'other-error))))
                 (unless (eq got want)
                   (setq bad (cons (list fn :want want :got got) bad))))))
           {cases}
           (nreverse bad))"
    );

    let results = bootstrap_eval_with_frame(&src);
    assert_eq!(
        results[0], "OK nil",
        "each entry is (SUBR :want GNU-PREDICATE :got OURS)"
    );
}

#[test]
fn every_window_subr_decodes_in_gnus_domain() {
    crate::test_utils::init_test_tracing();
    // What each domain must REJECT, which is the half that never mutates --
    // the subr signals before doing anything, so mutating members of the table
    // (`delete-window-internal`, `uncombine-window`, ...) are safe to call:
    //
    //   every domain : a non-window signals that domain's predicate
    //   Live         : also rejects an INTERNAL and a DELETED window
    //   Valid        : also rejects a DELETED window (an internal one is fine)
    //   Any          : rejects neither -- a deleted window is still a window
    let mut cases = String::new();
    for (name, domain) in GNU_WINDOW_ARGUMENT_DOMAINS {
        let pred = domain.predicate();
        cases.push_str(&format!("(probe '{name} 'sym '{pred})\n"));
        match domain {
            WindowDomain::Live => {
                cases.push_str(&format!("(probe '{name} 'internal '{pred})\n"));
                cases.push_str(&format!("(probe '{name} 'dead '{pred})\n"));
            }
            WindowDomain::Valid => {
                cases.push_str(&format!("(probe '{name} 'dead '{pred})\n"));
            }
            WindowDomain::Any => {}
        }
    }

    let src = format!(
        "(progn (split-window-below)
           (let* ((internal (window-parent (selected-window)))
                  (dead (let ((w (split-window-below))) (delete-window w) w))
                  (bad nil))
             (fset 'probe
               (lambda (fn shape want)
                 (let* ((arg (cond ((eq shape 'internal) internal)
                                   ((eq shape 'dead) dead)
                                   (t 'not-a-window)))
                        (got (condition-case e (progn (funcall fn arg) 'no-error)
                               (wrong-type-argument (car (cdr e)))
                               (error 'other-error))))
                   (unless (eq got want)
                     (setq bad (cons (list fn shape :want want :got got) bad))))))
             {cases}
             (nreverse bad)))"
    );

    let results = bootstrap_eval_with_frame(&src);
    assert_eq!(
        results[0], "OK nil",
        "each entry is (SUBR SHAPE :want GNU-PREDICATE :got OURS)"
    );
}

#[test]
fn every_multiarg_window_subr_decodes_in_gnus_domain() {
    crate::test_utils::init_test_tracing();
    // Same rejection-only contract as the single-argument table, and safe for
    // the same reason: a subr signals before it mutates, so the `set-window-*`
    // members never take effect.
    let mut cases = String::new();
    for (name, domain, args) in GNU_WINDOW_MULTIARG_DOMAINS {
        let pred = domain.predicate();
        let mut shapes = vec!["'not-a-window"];
        match domain {
            WindowDomain::Live => {
                shapes.push("internal");
                shapes.push("dead");
            }
            WindowDomain::Valid => shapes.push("dead"),
            WindowDomain::Any => {}
        }
        for shape in shapes {
            let call = args.replace("{w}", shape);
            cases.push_str(&format!(
                "(probe \"{name}\" \"{shape}\" '{pred} (lambda () ({name} {call})))\n"
            ));
        }
    }

    let src = format!(
        "(progn (split-window-below)
           (let* ((internal (window-parent (selected-window)))
                  (dead (let ((w (split-window-below))) (delete-window w) w))
                  (bad nil))
             (fset 'probe
               (lambda (fn shape want thunk)
                 (let ((got (condition-case e (progn (funcall thunk) 'no-error)
                              (wrong-type-argument (car (cdr e)))
                              (error 'other-error))))
                   (unless (eq got want)
                     (setq bad (cons (list fn shape :want want :got got) bad))))))
             {cases}
             (nreverse bad)))"
    );

    let results = bootstrap_eval_with_frame(&src);
    assert_eq!(
        results[0], "OK nil",
        "each entry is (SUBR SHAPE :want GNU-PREDICATE :got OURS)"
    );
}

#[test]
fn delete_window_internal_refuses_a_parentless_window_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's guard is about the window having NO PARENT, not about how many
    // leaves the frame has (`src/window.c`):
    //
    //     parent = w->parent;
    //     if (NILP (parent))
    //       error ("Attempt to delete minibuffer or sole ordinary window");
    //     else if (NILP (w->prev) && NILP (w->next))
    //       error ("Attempt to delete sole window of parent");
    //
    // Exactly two windows on a frame are parentless: the minibuffer and the
    // root.  neomacs tested `minibuffer || window_list().len() <= 1`, and
    // `window_list` counts LEAVES -- so after a split the root is an internal
    // window, the frame has two leaves, and the root slipped through to the
    // resize step:
    //
    //   (delete-window-internal ROOT)  GNU "Attempt to delete minibuffer or
    //                                      sole ordinary window"
    //                                  neomacs "Deletion failed"
    //
    // The minibuffer already matched, because it was special-cased by identity
    // rather than by the property GNU actually tests.
    let results = bootstrap_eval_with_frame(
        "(progn (split-window-below)
           (let* ((live (selected-window)) (root (frame-root-window)))
             (list (eq (window-parent live) root)
                   (window-parent root)
                   (condition-case e (delete-window-internal root) (error (car (cdr e))))
                   (condition-case e (delete-window-internal (minibuffer-window))
                     (error (car (cdr e)))))))",
    );
    assert_eq!(
        results[0],
        "OK (t nil \"Attempt to delete minibuffer or sole ordinary window\" \"Attempt to delete minibuffer or sole ordinary window\")"
    );
}

#[test]
fn frame_geometry_subrs_name_gnus_decoder_predicate() {
    crate::test_utils::init_test_tracing();
    // Which predicate a frame subr names is fixed by which decoder GNU's C
    // opens with: `decode_any_frame' signals `framep', `decode_live_frame'
    // signals `frame-live-p'.  Verified against `src/frame.c', with the decode
    // unguarded by any `#ifdef' in each case:
    //
    //   frame-fringe-width       decode_any_frame   -> framep
    //   frame-pointer-visible-p  decode_any_frame   -> framep
    //   frame-scroll-bar-height  decode_any_frame   -> framep
    //   frame-scroll-bar-width   decode_any_frame   -> framep
    //   frame-scale-factor       decode_live_frame  -> frame-live-p
    //
    // The first four said `frame-live-p' because they went through a helper
    // named `expect_frame_live_or_nil' that only tag-checks the value and then
    // names `frame-live-p' regardless -- the predicate was a constant in the
    // helper rather than a consequence of the check.  `frame-scale-factor'
    // diverged the other way, decoding with `FrameDomain::Any'.
    //
    // Checking the `#ifdef' guards matters here: `tool-bar-height' and
    // `tool-bar-pixel-width' look like divergences in exactly this shape and
    // are NOT -- they are artifacts of the GTK reference binary's
    // `HAVE_EXT_TOOL_BAR'.  These five have no such guard.
    let results = bootstrap_eval_with_frame(
        "(mapcar (lambda (fn)
                   (condition-case e (progn (funcall fn (selected-window)) 'no-error)
                     (wrong-type-argument (car (cdr e)))))
                 '(frame-fringe-width frame-pointer-visible-p
                   frame-scroll-bar-height frame-scroll-bar-width
                   frame-scale-factor))",
    );
    assert_eq!(results[0], "OK (framep framep framep framep frame-live-p)");
}

#[test]
fn an_integer_is_not_a_window_or_frame_designator_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU has no notion of an integer window or frame.  A window is a
    // `Lisp_Window' pseudovector and nothing else -- `WINDOW_VALID_P' is
    // `WINDOWP (w) && ...' (`src/window.h:1188'), and `WINDOWP' is a
    // pseudovector tag test.  There is no path by which the fixnum 1 names
    // window 1.
    //
    // neomacs had one: `window_id_from_designator' mapped any non-negative
    // fixnum to `WindowId(n)', so every predicate and every decoder built on
    // it accepted a raw integer -- and, because the ids really do start at 1,
    // small integers silently named REAL windows.  Measured on GNU Emacs 31.1
    // versus neomacs before this fix:
    //
    //   (windowp 1)           nil                              => t
    //   (window-live-p 1)     nil                              => t
    //   (window-valid-p 1)    nil                              => t
    //   (window-buffer 1)     (wrong-type-argument windowp 1)  => #<buffer *scratch*>
    //   (window-point 1)      (wrong-type-argument window-live-p 1) => 1
    //   (frame-root-window 1) (wrong-type-argument frame-live-p 1)  => #<window 1 ...>
    //
    // `(window-buffer 1)` handing back a live buffer is the sharp end of it:
    // a type confusion that reads as success.
    let results = bootstrap_eval_with_frame(
        "(list (windowp 1) (window-live-p 1) (window-valid-p 1)
               (condition-case e (progn (window-buffer 1) 'no-error)
                 (wrong-type-argument (car (cdr e))))
               (condition-case e (progn (window-point 1) 'no-error)
                 (wrong-type-argument (car (cdr e))))
               (condition-case e (progn (frame-root-window 1) 'no-error)
                 (wrong-type-argument (car (cdr e)))))",
    );
    assert_eq!(
        results[0],
        "OK (nil nil nil windowp window-live-p frame-live-p)"
    );
}

#[test]
fn split_window_internal_bad_old_names_window_valid_p_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU decodes OLD with `decode_valid_window' (`src/window.c'), and does so
    // BEFORE `CHECK_FIXNUM (pixel_size)'.  Measured on GNU Emacs 31.1:
    //
    //   (split-window-internal 'foo 10   nil 0.5) => (wrong-type-argument window-valid-p foo)
    //   (split-window-internal 'foo 'zz  9   0.5) => (wrong-type-argument window-valid-p foo)
    //
    // The second case is the ordering proof: with BOTH arguments wrong it is
    // still the window that gets named, never `fixnump'.
    let results = bootstrap_eval_with_frame(
        "(list (condition-case err (split-window-internal 'foo 10 nil 0.5)
                 (error (car (cdr err))))
               (condition-case err (split-window-internal 'foo 'zz 9 0.5)
                 (error (car (cdr err)))))",
    );
    assert_eq!(results[0], "OK (window-valid-p window-valid-p)");
}

#[test]
fn split_window_internal_never_type_checks_side_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `Fsplit_window_internal' reduces SIDE to two `EQ' tests over a closed
    // set and validates nothing (`src/window.c'):
    //
    //     bool horflag = EQ (side, Qt) || EQ (side, Qleft) || EQ (side, Qright);
    //     ...
    //     if (EQ (side, Qabove) || EQ (side, Qleft))   /* insert before */
    //
    // so a fixnum SIDE is not an error -- it is simply neither horizontal nor
    // before, i.e. exactly `below'.  Measured on GNU Emacs 31.1: no SIDE value
    // produces a `wrong-type-argument'; 9, "x", nil, `below' and `right' all
    // reach the resize step and fail there identically.
    //
    // That last part is why this asserts a RELATIVE result.  GNU rejects a
    // direct `split-window-internal' call outright -- its docstring requires
    // the caller to have staged the new pixel/normal sizes first ("See the
    // code of `split-window' for how this is done"), and an unstaged call
    // signals (error "Resizing old window failed") for EVERY side, valid ones
    // included.  So GNU's geometry is not an available oracle here; what is
    // comparable is the decode that runs before any resizing.  Pinning that a
    // fixnum SIDE lands on the same side as `below' and NOT on `right' says
    // exactly what GNU's two `EQ' tests say, without depending on either
    // implementation's sizing.
    let results = bootstrap_eval_with_frame(
        "(let* ((half (/ (window-pixel-height (selected-window)) 2))
                (a ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) half 9 nil))
                (ga (list (window-total-height a) (window-total-width a))))
           (delete-window a)
           (let* ((b ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) half 'below nil))
                  (gb (list (window-total-height b) (window-total-width b))))
             (delete-window b)
             (let* ((c ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) half 'right nil))
                    (gc (list (window-total-height c) (window-total-width c))))
               (delete-window c)
               (list (equal ga gb) (equal ga gc)))))",
    );
    assert_eq!(results[0], "OK (t nil)");
}

#[test]
fn split_window_side_t_splits_horizontally_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((old (selected-window))
                (new (split-window old nil t)))
           (list (> (window-left-column new) (window-left-column old))
                 (= (window-top-line new) (window-top-line old))))",
    );
    assert_eq!(results[0], "OK (t t)");
}

#[test]
fn split_window_preserves_requested_normal_size_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((old (selected-window))
                (new (split-window old 20 'right)))
           (list (window-total-width old)
                 (window-total-width new)
                 (window-normal-size old t)
                 (window-normal-size new t)
                 (window-normal-size old)
                 (window-normal-size new)))",
    );
    assert_eq!(results[0], "OK (20 60 0.25 0.75 1.0 1.0)");
}

#[test]
fn display_buffer_in_left_side_window_places_window_on_left_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((side-window
                (display-buffer-in-side-window
                 (get-buffer-create \"*side*\")
                 '((side . left)))))
           (list (window-edges side-window)
                 (window-total-width side-window)
                 (window-parameter side-window 'window-side)
                 (mapcar (lambda (w)
                           (list (buffer-name (window-buffer w))
                                 (window-edges w)))
                         (window-list nil 'no-minibuf nil))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 20 24) 20 left ((\"*scratch*\" (20 0 80 24)) (\"*side*\" (0 0 20 24))))"
    );
}

#[test]
fn display_buffer_respects_requested_width_when_buffer_width_is_fixed() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((buffer (get-buffer-create \"*fixed-side*\")))
           (with-current-buffer buffer
             (setq-local window-size-fixed 'width))
           (let ((side-window
                  (display-buffer-in-side-window
                   buffer
                   '((side . left) (window-width . 20)))))
             (list (window-edges side-window)
                   (window-total-width side-window)
                   (window-size-fixed-p side-window t)
                   (mapcar (lambda (w)
                             (list (buffer-name (window-buffer w))
                                   (window-edges w)))
                           (window-list nil 'no-minibuf nil)))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 20 24) 20 (width t) ((\"*scratch*\" (20 0 80 24)) (\"*fixed-side*\" (0 0 20 24))))"
    );
}

#[test]
fn display_buffer_side_window_splits_internal_root_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(progn
           (display-buffer (get-buffer-create \"*Warnings*\"))
           (let ((buffer (get-buffer-create \"*fixed-side*\")))
             (with-current-buffer buffer
               (setq-local window-size-fixed 'width))
             (let ((side-window
                    (display-buffer-in-side-window
                     buffer
                     '((side . left) (window-width . 20)))))
               (list (window-live-p side-window)
                     (window-edges side-window)
                     (window-total-width side-window)
                     (window-size-fixed-p side-window t)
                     (mapcar (lambda (w)
                               (list (buffer-name (window-buffer w))
                                     (window-edges w)
                                     (window-width w)
                                     (window-parameter w 'window-side)))
                             (window-list nil 'no-minibuf nil))))))",
    );
    assert_eq!(
        results[0],
        "OK (t (0 0 20 24) 20 (width t) ((\"*scratch*\" (20 0 80 12) 60 nil) (\"*Warnings*\" (20 12 80 24) 60 nil) (\"*fixed-side*\" (0 0 20 24) 19 left)))"
    );
}

#[test]
fn split_window_accepts_internal_root_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(progn
           (split-window (selected-window) nil 'below)
           (let ((new (split-window (frame-root-window) nil 'left)))
             (list (window-live-p new)
                   (window-valid-p (frame-root-window))
                   (window-edges new)
                   (mapcar (lambda (w)
                             (list (buffer-name (window-buffer w))
                                   (window-edges w)))
                           (window-list nil 'no-minibuf nil)))))",
    );
    assert_eq!(
        results[0],
        "OK (t t (0 0 40 24) ((\"*scratch*\" (40 0 80 12)) (\"*scratch*\" (40 12 80 24)) (\"*scratch*\" (0 0 40 24))))"
    );
}

#[test]
fn display_buffer_in_top_side_window_places_window_on_top_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((side-window
                (display-buffer-in-side-window
                 (get-buffer-create \"*side*\")
                 '((side . top)))))
           (list (window-edges side-window)
                 (window-total-height side-window)
                 (window-parameter side-window 'window-side)
                 (mapcar (lambda (w)
                           (list (buffer-name (window-buffer w))
                                 (window-edges w)))
                         (window-list nil 'no-minibuf nil))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 80 6) 6 top ((\"*scratch*\" (0 6 80 24)) (\"*side*\" (0 0 80 6))))"
    );
}

#[test]
fn delete_window_after_split() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((new-win ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window new-win)
           (length (window-list)))",
    );
    assert_eq!(results[0], "OK 1");
}

#[test]
fn delete_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"dw-curbuf-a\"))
                  (b2 (get-buffer-create \"dw-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (delete-window w2)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"dw-curbuf-a\"");
}

#[test]
fn delete_sole_window_errors() {
    crate::test_utils::init_test_tracing();
    let r = bootstrap_eval_one_with_frame("(delete-window)");
    assert!(r.contains("ERR"), "deleting sole window should error: {r}");
}

#[test]
fn delete_window_and_delete_other_windows_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (delete-window nil nil) (error (car err)))
         (condition-case err (delete-other-windows nil nil nil) (error (car err)))
         (condition-case err
             (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
               (delete-other-windows w2 nil))
           (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK wrong-number-of-arguments");
    assert_eq!(out[2], "OK nil");
}

#[test]
fn delete_other_windows_keeps_one() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
         ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
         (delete-other-windows)
         (length (window-list))",
    );
    assert_eq!(results[3], "OK 1");
}

#[test]
fn delete_other_windows_relocates_kept_window_across_frame_width_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(let ((right (split-window nil nil 'right)))
           (select-window right)
           (delete-other-windows)
           (let ((edges (window-edges)))
             (list (car edges) (nth 2 edges) (frame-width))))",
    );
    assert_eq!(result, "OK (0 80 80)");
}

#[test]
fn delete_other_windows_internal_only_replaces_its_root_subtree_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(let* ((left (selected-window))
                (right (split-window nil nil 'right))
                (bottom-right (split-window right nil 'below))
                (right-root (window-parent right)))
           (select-window left)
           (delete-other-windows-internal bottom-right right-root)
           (let ((left-edges (window-edges left))
                 (kept-edges (window-edges bottom-right)))
             (list (length (window-list))
                   (car left-edges) (nth 2 left-edges)
                   (car kept-edges) (nth 2 kept-edges)
                   (eq (selected-window) bottom-right))))",
    );
    assert_eq!(result, "OK (2 0 40 40 80 t)");
}

#[test]
fn delete_other_windows_updates_current_buffer_when_kept_window_differs() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"dow-curbuf-a\"))
                  (b2 (get-buffer-create \"dow-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil))
                   (w1 (selected-window)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (delete-other-windows w1)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"dow-curbuf-a\"");
}

#[test]
fn other_window_cycles() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((w1 (selected-window)))
           ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
           (other-window 1)
           (not (eq (selected-window) w1)))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn other_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"ow-curbuf-a\"))
                  (b2 (get-buffer-create \"ow-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
               (set-window-buffer w2 b2)
               (other-window 1)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"ow-curbuf-b\"");
}

#[test]
fn other_window_requires_count_and_enforces_number_or_marker_p() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (other-window) (error (car err)))
         (condition-case err (other-window nil) (error err))
         (condition-case err (other-window \"x\") (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument number-or-marker-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument number-or-marker-p \"x\")");
}

#[test]
fn other_window_accepts_float_counts_with_floor_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((w1 (progn (delete-other-windows) (selected-window)))
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (list
             (progn (other-window 1.5) (eq (selected-window) w2))
             (progn (select-window w1) (other-window 0.4) (eq (selected-window) w1))
             (progn (select-window w1) (other-window -0.4) (eq (selected-window) w2))
             (progn (select-window w1) (other-window -1.2) (eq (selected-window) w1))))",
    );
    assert_eq!(results[0], "OK (t t t t)");
}

#[test]
fn other_window_enforces_max_arity() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (other-window 1 nil nil nil) (error (car err)))
         (condition-case err
             (let ((w1 (selected-window)))
               ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
               (other-window 1 nil nil)
               (not (eq (selected-window) w1)))
           (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK t");
}

/// `set-frame-size` resizes a top-level TTY frame's WINDOW TREE.
///
/// `builtin_set_frame_size` had a GUI branch and a child-frame branch and
/// nothing else: a top-level terminal frame fell through to `Ok(Value::NIL)`,
/// so the call did nothing at all.
///
/// GNU's `Fset_frame_size` calls `adjust_frame_size` for every frame.  On a
/// terminal frame that does not resize the terminal -- `frame-width` and
/// `frame-height` keep reporting it -- but the windows are laid out to the
/// requested TEXT size, which includes the minibuffer and excludes the top
/// margin.  Measured on GNU Emacs 31.0.50, `emacs -Q --batch`, from the 80x25
/// startup frame with `menu-bar-lines` 1:
///
///     request  frame     root top/h/w   mini top
///     100x40   80x25     1 / 39 / 100   40
///     60x10    80x25     1 /  9 /  60   10
///     80x24    80x25     1 / 23 /  80   24
///
/// neomacs answered `0 / 24 / 80`, mini 24, for all three -- the startup
/// layout, untouched.
///
/// The root's character row is the frame's top margin while its pixel origin
/// stays at the text area's top, which is how GNU reports `window-pixel-top` 0
/// alongside `window-top-line` 1, and the minibuffer follows by the sum rule
/// `m->top_line = r->top_line + r->total_lines` (`src/window.c:5030`).
/// `tool-bar-pixel-width` decodes FRAME before it answers.
///
/// It was a bare stub returning 0 without looking at its argument, so
/// `(tool-bar-pixel-width SOME-WINDOW)` answered 0.  GNU decodes
/// (`src/frame.c:4439`):
///
///     #ifdef FRAME_TOOLBAR_WIDTH
///       struct frame *f = decode_any_frame (frame);
///       if (FRAME_WINDOW_P (f)) return make_fixnum (FRAME_TOOLBAR_WIDTH (f));
///     #endif
///       return make_fixnum (0);
///
/// so a non-frame signals `framep` before the constant is ever reached.
///
/// This sat in the audit beside `tool-bar-height`, whose lax GNU answers ARE a
/// `#ifndef HAVE_EXT_TOOL_BAR` artifact of the GTK reference binary -- there
/// the decode is compiled OUT, which is why GNU returns a number for a window
/// and neomacs correctly signals.  The two guards are not the same: GTK
/// DEFINES `FRAME_TOOLBAR_WIDTH`, so this decode is compiled IN and GNU
/// signals.  The pair diverging in OPPOSITE directions is what separates the
/// artifact from the bug, and neomacs was internally inconsistent too --
/// `tool-bar-height` decoded, this did not.
///
/// Measured on GNU Emacs 31.0.50, `emacs -Q --batch`, five probe rows each:
/// live, internal, minibuffer and deleted windows and a plain symbol all
/// signal `wrong-type-argument framep` in GNU and answered 0 here.
#[test]
fn tool_bar_pixel_width_decodes_any_frame_like_gnu() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(list (tool-bar-pixel-width)
               (tool-bar-pixel-width (selected-frame))
               (condition-case e (tool-bar-pixel-width (selected-window))
                 (wrong-type-argument (car (cdr e))))
               (condition-case e (tool-bar-pixel-width 'foo)
                 (wrong-type-argument (car (cdr e)))))",
    );
    assert_eq!(out[0], "OK (0 0 framep framep)");
}

#[test]
fn set_frame_size_lays_out_a_tty_frames_windows_like_gnu() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(progn
           (set-frame-parameter nil 'menu-bar-lines 1)
           (set-frame-size (selected-frame) 60 10)
           (list (window-top-line (frame-root-window))
                 (window-total-height (frame-root-window))
                 (window-total-width (frame-root-window))
                 (window-top-line (minibuffer-window))))",
    );
    assert_eq!(out[0], "OK (1 9 60 10)");
}

#[test]
fn window_preserve_size_fixed_and_resizable_helpers_match_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(let ((w (selected-window)))
           (list (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (let ((r (window-preserve-size w nil t)))
                   (list (bufferp (car r))
                         (nth 1 r)
                         (integerp (nth 2 r))))
                 (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (let ((r (window-preserve-size w t t)))
                   (list (bufferp (car r))
                         (integerp (nth 1 r))
                         (integerp (nth 2 r))))
                 (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (window-size-fixed-p w nil t)
                 (window-size-fixed-p w t t)
                 (progn
                   (window-preserve-size w nil nil)
                   (window-preserve-size w t nil)
                   (list (window-size-fixed-p w)
                         (window-size-fixed-p w t)))))
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-width (selected-window)) 2) 'right nil)))
           ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) w (/ (window-pixel-height w) 2) 'below nil)
           (window-preserve-size w t t)
           (let ((before (list (window-resizable w 100 t)
                               (window-resizable w -100 t)
                               (window-resizable w 100 nil)
                               (window-resizable w -100 nil)
                               (window-size-fixed-p w)
                               (window-size-fixed-p w t)
                               (window-resizable w 1 t)
                               (window-resizable w 1 t 'preserved)
                               (window-resizable w 1.5 t)
                               (window-resizable w -1.5 t))))
             (window-preserve-size w t nil)
             (list before
                   (window-size-fixed-p w t)
                   (window-resizable w 1 t)
                   (window-resizable w 1.5 t)
                   (window-resizable w -1.5 t))))
         (list (condition-case err (window-size-fixed-p 999999) (error (car err)))
               (condition-case err (window-preserve-size 999999 nil t) (error (car err)))
               (condition-case err (window-resizable 999999 1) (error (car err)))
               (condition-case err (window-resizable nil 'foo) (error (car err)))
               (condition-case err (window-size-fixed-p nil nil nil nil) (error err))
               (condition-case err (window-preserve-size nil nil nil nil) (error err))
               (condition-case err (window-resizable nil 1 nil nil nil nil) (error err)))",
    );
    assert_eq!(
        out[0],
        "OK (nil nil (t nil t) t nil (t t t) t t nil nil (nil nil))"
    );
    assert_eq!(out[1], "OK ((0 0 8 -8 nil t 0 1 0 0) nil 1 1.5 -1.5)");
    // window-size-fixed-p, window-preserve-size, window-resizable
    // are Lisp defuns (window.el), so arity errors carry (MIN . MAX)
    // tuples instead of the function symbol.
    assert_eq!(
        out[2],
        "OK (error error error wrong-type-argument (wrong-number-of-arguments (0 . 3) 4) (wrong-number-of-arguments (0 . 3) 4) (wrong-number-of-arguments (2 . 5) 6))"
    );
}

#[test]
fn window_tree_navigation_and_normal_size_match_gnu_runtime() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(let* ((left (selected-window))
                (right (split-window nil nil 'right))
                (bottom (split-window right nil 'below))
                (root (frame-root-window))
                (vparent (window-parent right)))
           (list (window-valid-p root)
                 (window-live-p root)
                 (eq (window-parent left) root)
                 (eq (window-next-sibling left) vparent)
                 (eq (window-left-child root) left)
                 (window-top-child root)
                 (eq (window-parent right) vparent)
                 (eq (window-parent bottom) vparent)
                 (eq (window-top-child vparent) right)
                 (window-left-child vparent)
                 (eq (window-next-sibling right) bottom)
                 (eq (window-prev-sibling bottom) right)
                 (window-normal-size left)
                 (window-normal-size left t)
                 (window-normal-size right)
                 (window-normal-size right t)
                 (window-normal-size vparent)
                 (window-normal-size vparent t)))",
    );
    assert_eq!(
        out[0],
        "OK (t nil t t t nil t t t nil t t 1.0 0.5 0.5 1.0 1.0 0.5)"
    );
}

/// A split that splices into an existing combination commits only the staged
/// `new_normal`s: the untouched siblings keep their normal sizes, and the
/// following `delete-window` under `window-combination-resize` distributes the
/// freed columns by them.  Values are GNU 31.1 `--batch` on its 80-column frame
/// (window-oracle-nightly 2026-10-29: neomacs restated the siblings from pixels,
/// 27/80 as an f32 quotient, and deleted to 39/20/21 instead of 38/21/21).
#[test]
fn splice_split_keeps_sibling_normal_sizes_for_combination_resize_delete_like_gnu() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(let ((nth-window
                (lambda (n)
                  (let ((ws (window-list nil 'no-minibuf nil)))
                    (nth (mod n (length ws)) ws))))
               (state
                (lambda ()
                  (mapcar (lambda (w)
                            (list (window-left-column w)
                                  (window-total-width w)
                                  (window-normal-size w t)))
                          (window-list nil 'no-minibuf (frame-first-window))))))
           (list (frame-width)
                 (progn
                   (let ((window-combination-limit nil) (window-combination-resize nil))
                     (split-window (funcall nth-window 0) nil 'left))
                   (let ((window-combination-limit nil) (window-combination-resize t))
                     (split-window (funcall nth-window 0) nil 'left))
                   (let ((window-combination-limit nil) (window-combination-resize nil))
                     (split-window (funcall nth-window 2) nil 'left))
                   (funcall state))
                 (progn
                   (let ((window-combination-resize t))
                     (delete-window (funcall nth-window 4)))
                   (funcall state))))",
    );
    assert_eq!(
        out[0],
        "OK (80 ((0 27 0.33333333333333337) (27 13 0.16666666666666666) (40 13 0.16666666666666666) (53 27 0.33333333333333337)) ((0 38 0.5000000000000001) (38 21 0.25) (59 21 0.25)))"
    );
}

#[test]
fn vertical_motion_truncates_partial_width_split_windows_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-truncate-narrow*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert (make-string 200 ?x))
                   (let ((w2 (split-window nil 40 'right)))
                     (select-window w2)
                     (goto-char (point-min))
                     (list (window-body-width)
                           (truncated-partial-width-window-p)
                           (vertical-motion 1)
                           (point)
                           (count-screen-lines (point-min) (point-max)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (40 t 0 201 0)");
}

/// A buffer-local `truncate-partial-width-windows' turns partial-width
/// truncation OFF for that buffer, and screen-line motion must see it.
///
/// GNU's `Vtruncate_partial_width_windows' is a `DEFVAR_LISP', so `setq-local'
/// localizes the symbol and the C global always holds the value swapped in for
/// `current_buffer'; `init_iterator' (src/xdisp.c:3416-3426) therefore picks
/// WINDOW_WRAP/WORD_WRAP over TRUNCATE from the BUFFER's value, not from the
/// global default.  GNU's own Lisp predicate spells the same rule out:
/// `truncated-partial-width-window-p' reads
/// `(buffer-local-value 'truncate-partial-width-windows (window-buffer window))'
/// (lisp/window.el:11285-11298).
///
/// `visual-line-mode' depends on exactly this: it does
/// `(setq-local truncate-partial-width-windows nil)' (lisp/simple.el:8716) so
/// that a window narrower than the 50-column default still wraps.  Reading the
/// global instead collapses `beginning-of-visual-line' -- which is just
/// `(vertical-motion 0)' (lisp/simple.el:8573) -- onto the LOGICAL line start.
///
/// Ground truth measured under GNU Emacs 31.0.90 in a TTY frame, on a
/// 40-column partial-width window over 200 `x' characters:
///   buffer-local nil -> tpwwp nil, 6 screen lines, (vertical-motion 0) from
///                       ZV lands on 196, (vertical-motion 1) from BOB is 1.
#[test]
fn buffer_local_truncate_partial_width_windows_keeps_motion_wrapping_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-tpww-buffer-local*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert (make-string 200 ?x))
                   (let ((w2 (split-window nil 40 'right)))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-partial-width-windows nil)
                     (list (window-body-width)
                           (truncated-partial-width-window-p)
                           (count-screen-lines (point-min) (point-max))
                           (progn (goto-char (point-max))
                                  (vertical-motion 0)
                                  (point))
                           (progn (goto-char (point-min))
                                  (vertical-motion 1)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (40 nil 6 196 1)");
}

/// `word-wrap` breaks a screen line at the last WORD boundary that fits, not
/// at the window edge -- the difference between GNU's `WORD_WRAP` and
/// `WINDOW_WRAP` (`enum line_wrap_method`, src/dispextern.h), chosen by
/// `init_iterator` from the buffer's `word-wrap` (src/xdisp.c:3425-3426).
///
/// GNU records the break with `wrap_it` inside `move_it_in_display_line_to`
/// (src/xdisp.c:10280-10300): a candidate is saved at each glyph that both
/// follows a wrappable glyph and can be wrapped before -- with the default
/// `word-wrap-by-category` nil that is "the first non-whitespace after
/// whitespace" (`char_can_wrap_after` / `char_can_wrap_before`,
/// src/xdisp.c:577-617).  When the row overflows, GNU restores the saved
/// candidate; with none saved it breaks at the edge like `WINDOW_WRAP`
/// (src/xdisp.c:10612).
///
/// Ground truth measured under GNU Emacs 31.0.90 in a TTY frame, on a
/// 24-column `visual-line-mode` window over
/// "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi
/// omicron": the screen lines start at 1, 20, 43 and 60, so from position 48
/// `(vertical-motion 0)` answers 43 and `(vertical-motion 1)` answers 60 --
/// where a character wrap would answer 47 and 70.
///
/// **In a TTY frame** is load-bearing, and this test says so with
/// `(noninteractive nil)`.  `word-wrap` is an input to `init_iterator`
/// (src/xdisp.c:3425-3426), and `Fvertical_motion` reaches `init_iterator`
/// only from its non-`noninteractive` arm (src/indent.c:2287); the batch arm
/// is `vmotion` -> `compute_motion`, which has no word-wrap concept at all.
/// The batch answers to this very probe are pinned by
/// [`word_wrap_is_inert_under_the_batch_motion_engine_like_gnu`] below, and
/// they are different numbers.
#[test]
fn word_wrap_screen_line_motion_breaks_at_word_boundaries_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-word-wrap-motion*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                   (let ((w2 (split-window nil -24 'right))
                         (noninteractive nil))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (list (window-body-width)
                           (progn (goto-char 48) (vertical-motion 0) (point))
                           (progn (goto-char 48) (vertical-motion 1) (point))
                           (progn (goto-char 48) (vertical-motion -1) (point))
                           (progn (goto-char 1) (vertical-motion 1) (point)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (24 43 60 20 20)");
}

/// The same window, the same buffer, the same `word-wrap t` -- under the BATCH
/// engine, where GNU ignores it.
///
/// `Fvertical_motion` under `noninteractive` is `vmotion` -> `compute_motion`
/// (src/indent.c:2280-2286, :1963, :1253).  `compute_motion` decides a line
/// end with exactly one test, `if (hpos > width)`, and then either truncates
/// or continues (src/indent.c:1474-1527); the identifier `word_wrap` does not
/// occur anywhere in src/indent.c.  So a 24-column window character-wraps at
/// column 23 whatever the buffer asks for, and the rows start at 1, 24, 47 and
/// 70 instead of 1, 20, 43 and 60.
///
/// Ground truth, GNU Emacs 31.0.90, the probe above run under `emacs --batch`
/// on the same 80-column frame this harness has:
///
/// ```text
///   (24 47 70 24 24)      -- batch,    this test
///   (24 43 60 20 20)      -- terminal, the test above
/// ```
///
/// Ledger 191 pinned the terminal numbers here, in a harness that is batch,
/// and made the port word-wrap in both engines to satisfy them.  That is the
/// regression this pair exists to keep out.
#[test]
fn word_wrap_is_inert_under_the_batch_motion_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-word-wrap-batch*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                   (let ((w2 (split-window nil -24 'right)))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (list (window-body-width)
                           (progn (goto-char 48) (vertical-motion 0) (point))
                           (progn (goto-char 48) (vertical-motion 1) (point))
                           (progn (goto-char 48) (vertical-motion -1) (point))
                           (progn (goto-char 1) (vertical-motion 1) (point)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (24 47 70 24 24)");
}

/// `word-wrap-by-category` non-nil lets a row break inside a CJK run.
///
/// GNU's `char_can_wrap_after` / `char_can_wrap_before` (src/xdisp.c:577-617)
/// keep their whitespace-only answer while `word-wrap-by-category` is nil, but
/// under non-nil they also consult the `|` (`LINE_BREAKABLE`) category and the
/// kinsoku `'<'`/`'>'` categories set up in `characters.el`/`kinsoku.el`.  A
/// row can therefore break between ordinary CJK characters, which contain no
/// spaces.
///
/// Ground truth, GNU Emacs 31.1 behavior in a 24-column TTY window over
/// "supertag v0.4 " followed by 60 `提`: a TTY window reserves one column for
/// the continuation glyph, so the row edge is column 23.  The 14 ASCII columns
/// plus four two-column CJK characters use 22 columns, and the fifth CJK
/// character would need columns 23-24, so `(vertical-motion 1)` from
/// `point-min` answers 19.  With `word-wrap-by-category` nil the only saved
/// candidate is the position before the first CJK character, and the answer is
/// 15.  Neomacs currently answers 15 in both cases.
#[test]
fn word_wrap_by_category_allows_a_cjk_break_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-word-wrap-category*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "supertag v0.4 " (make-string 60 ?提))
                   (let ((w2 (split-window nil -24 'right))
                         (noninteractive nil))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (list (window-body-width)
                           (char-width ?提)
                           (let ((word-wrap-by-category t))
                             (goto-char (point-min))
                             (vertical-motion 1)
                             (point))
                           (let ((word-wrap-by-category nil))
                             (goto-char (point-min))
                             (vertical-motion 1)
                             (point)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (24 2 19 15)");
}

/// The oracle's own probe (`div_l0_word_wrap_at_spaces`), as a unit pin on
/// both engines: one 201-character line whose only space sits at column 100,
/// in an 80-column window.
///
/// The line matters because no wrap boundary is reachable on the first row --
/// 100 unbroken `x` characters exceed the width -- so the two engines part
/// company on the SECOND row: the display iterator restores the wrap point it
/// saved after the space (src/xdisp.c:10289-10300, :10601) and starts a fourth
/// row, while `compute_motion` continues at the edge and there are three.
///
/// Ground truth, GNU Emacs 31.0.90, 80-column terminal:
///
/// ```text
///   emacs --batch        rows 1 80 159       count-screen-lines 3
///   emacs -nw in a pty   rows 1 80 102 181   count-screen-lines 4
/// ```
#[test]
fn count_screen_lines_ignores_word_wrap_under_the_batch_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(with-temp-buffer
             (insert (make-string 100 ?x) " " (make-string 100 ?x))
             (let ((word-wrap t))
               (count-screen-lines (point-min) (point-max))))"#,
    );
    assert_eq!(result, "OK 3");
}

/// The display-engine half of the pair above.
#[test]
fn count_screen_lines_honors_word_wrap_under_the_display_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(with-temp-buffer
             (insert (make-string 100 ?x) " " (make-string 100 ?x))
             (let ((word-wrap t)
                   (noninteractive nil))
               (count-screen-lines (point-min) (point-max))))"#,
    );
    assert_eq!(result, "OK 4");
}

/// `move-to-window-line' counts SCREEN lines and answers how many it actually
/// moved over -- it is `vertical-motion' with the window's line count folded
/// into ARG, not a logical-line walk from `window-start'.
///
/// GNU `Fmove_to_window_line' (src/window.c:7498-7573):
///
/// ```c
///   Fgoto_char (w->start);
///   lines = displayed_window_lines (w);
///   if (NILP (arg)) XSETFASTINT (arg, lines / 2);
///   else { EMACS_INT iarg = XFIXNUM (Fprefix_numeric_value (arg));
///          if (iarg < 0) iarg = iarg + lines; arg = make_fixnum (iarg); }
///   if (w->vscroll) XSETINT (arg, XFIXNUM (arg) + 1);
///   return Fvertical_motion (arg, window, Qnil);
/// ```
///
/// Three consequences the port used to get wrong, all measured under GNU
/// Emacs 31.0.90 in a TTY window whose body is 47 lines:
///
///   * The value is `vertical-motion''s -- the lines actually moved over.  On a
///     three-line buffer `(move-to-window-line 5)` answers 3 and stops at ZV,
///     and `(move-to-window-line -1)` answers 3 as well.
///   * A positive ARG is NOT clamped to the window: over 200 logical lines
///     `(move-to-window-line 100)` answers 100 and lands on line 101.
///   * The lines counted are SCREEN lines.  On one 500-character logical line
///     wrapped in a 160-column window the rows start at 1, 160 and 319, so
///     `(move-to-window-line 2)` lands on 319; truncated, the same buffer has
///     one screen line and `(move-to-window-line 2)` answers 1 at ZV.
#[test]
fn move_to_window_line_counts_screen_lines_and_returns_lines_moved_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-move-to-window-line*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "aaa\nbbb\nccc\n")
                   (let ((walk
                          (lambda ()
                            (mapcar (lambda (n)
                                      (goto-char (point-min))
                                      (set-window-start (selected-window)
                                                        (point-min) t)
                                      (list n (move-to-window-line n) (point)))
                                    '(0 1 2 5 -1 100)))))
                     (let ((short (funcall walk)))
                       (erase-buffer)
                       (insert (make-string 500 ?x) "\n")
                       (setq-local truncate-lines nil)
                       (setq-local truncate-partial-width-windows nil)
                       (let ((wrapped (funcall walk)))
                         (setq-local truncate-lines t)
                         (let ((truncated (funcall walk)))
                           (erase-buffer)
                           (setq-local truncate-lines nil)
                           (insert (mapconcat (lambda (n) (format "line%03d" n))
                                              (number-sequence 1 200) "\n")
                                   "\n")
                           (goto-char (point-min))
                           (set-window-start (selected-window) (point-min) t)
                           (list (window-body-width)
                                 short
                                 wrapped
                                 truncated
                                 ;; A buffer taller than the window: GNU's
                                 ;; negative ARG counts from the window's own
                                 ;; line count, so -1 is the LAST window line.
                                 (list (= (move-to-window-line -1)
                                          (1- (window-body-height)))
                                       (progn
                                         (goto-char (point-min))
                                         (set-window-start (selected-window)
                                                           (point-min) t)
                                         (move-to-window-line 100)))))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    // GNU Emacs 31.0.90 on an 80-column TTY frame:
    //   short-3-logical    ((0 0 1) (1 1 5) (2 2 9) (5 3 13) (-1 3 13) (100 3 13))
    //   one-long-wrapped   ((0 0 1) (1 1 80) (2 2 159) (5 5 396) (-1 7 502) (100 7 502))
    //   one-long-truncated ((0 0 1) (1 1 502) (2 1 502) (5 1 502) (-1 1 502) (100 1 502))
    //   tall-200-logical   (move-to-window-line -1)  = body height - 1
    //                      (move-to-window-line 100) = 100  (ARG is not clamped)
    // The window body height differs between that frame and this bootstrap
    // one, so the last pair is pinned as GNU's RELATION to the body height
    // rather than as GNU's number.
    assert_eq!(
        result,
        "OK (80 \
         ((0 0 1) (1 1 5) (2 2 9) (5 3 13) (-1 3 13) (100 3 13)) \
         ((0 0 1) (1 1 80) (2 2 159) (5 5 396) (-1 7 502) (100 7 502)) \
         ((0 0 1) (1 1 502) (2 1 502) (5 1 502) (-1 1 502) (100 1 502)) \
         (t 100))"
    );
}

/// `(vertical-motion (COLS . 0))` stops at the LAST column that does not pass
/// COLS, and never leaves the screen row.
///
/// GNU reaches the goal with
/// `move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)`
/// (src/indent.c:2540).  `move_it_in_display_line_to` places a glyph only
/// while it still fits before the goal and backs up to `x_before_this_char`
/// as soon as one would pass it (src/xdisp.c:10385-10400), and it also stops
/// where the display line itself ends (`it->last_visible_x`).
///
/// This is what `end-of-visual-line` -- `(vertical-motion (cons (window-width)
/// 0))`, lisp/simple.el:8558 -- rides on, and it is why `move-to-column`
/// cannot stand in for the walk: `move-to-column` moves PAST a TAB to the
/// column where it ends, while GNU stops before it.
///
/// Measured under GNU Emacs 31.0.90 on a 24-column TTY window:
///
///   row "\tabcdef..."  goals 0..7 -> point 1 (the TAB, column 0)
///                      goal  8    -> point 2 (column 8)
///                      goal  23   -> point 17 (column 23)
///                      goal  30   -> point 17  (saturates at the row's edge)
///   row "xxxx..."      goal  23   -> point 24 (column 23)
///                      goal  30   -> point 24
///
/// -- identical answers whether the window truncates or wraps, because both
/// display lines end at the same right edge.
#[test]
fn vertical_motion_goal_column_stops_before_overshooting_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-goal-column*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((w2 (split-window nil -24 'right))
                         ;; The goal-column walk is display motion: GNU leaves
                         ;; point at the line start under `noninteractive'
                         ;; (src/indent.c:2280-2286 takes the batch `vmotion'
                         ;; path), so the probe has to be an interactive one.
                         (noninteractive nil)
                         (walk
                          (lambda ()
                            (mapcar (lambda (n)
                                      (goto-char (point-min))
                                      (list n (vertical-motion (cons n 0))
                                            (point)))
                                    '(0 3 7 8 22 23 24 30)))))
                     (select-window w2)
                     (switch-to-buffer b)
                     (erase-buffer)
                     (insert "\tabcdefghijklmnopqrstuvwxyz0123456789\n")
                     (setq-local truncate-lines t)
                     (let ((tab-truncated (funcall walk)))
                       (setq-local truncate-lines nil)
                       (setq-local truncate-partial-width-windows nil)
                       (let ((tab-wrapped (funcall walk)))
                         (erase-buffer)
                         (insert (make-string 200 ?x) "\n")
                         (list (window-body-width)
                               tab-truncated
                               tab-wrapped
                               (funcall walk))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (24 \
         ((0 0 1) (3 0 1) (7 0 1) (8 0 2) (22 0 16) (23 0 17) (24 0 17) (30 0 17)) \
         ((0 0 1) (3 0 1) (7 0 1) (8 0 2) (22 0 16) (23 0 17) (24 0 17) (30 0 17)) \
         ((0 0 1) (3 0 4) (7 0 8) (8 0 9) (22 0 23) (23 0 24) (24 0 24) (30 0 24)))"
    );
}

/// A goal column past the end of the row rests ON the row -- and where the row
/// ends depends on WHY it ended.
///
/// This is `end-of-visual-line`, which is `(vertical-motion (cons
/// (window-width) 0))` (lisp/simple.el:8558): the goal is always past the row,
/// so the answer is always the row's end boundary and nothing else.
///
/// GNU's `move_it_in_display_line_to` stops where the DISPLAY LINE ends
/// (`it->last_visible_x`), and a `WORD_WRAP` row does not reach that edge: it
/// broke at a saved wrap point (src/xdisp.c:10280-10300) whose position is
/// drawn on the NEXT row.  So a word-wrapped row's last stop is its last
/// GLYPH, while a row that filled the width has one more stop past its last
/// glyph -- the next row's first position, at the edge column.
///
/// Measured under GNU Emacs 31.0.90 on a 24-column TTY window over
/// "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi
/// omicron\n", walking the goal from 20 to 40 from each row start:
///
/// ```text
///   word wrap  row 1..19   goals 20+ -> 19   the row's LAST GLYPH
///   word wrap  row 20..42  goals 23+ -> 42   likewise (22 -> 42, 21 -> 41, 20 -> 40)
///   word wrap  row 43..59  goals 20+ -> 59   likewise
///   word wrap  row 60..83  goals 23+ -> 83   the NEWLINE, which draws nothing
///   char wrap  row 1..23   goals 23+ -> 24   the NEXT ROW's first position
///   char wrap  row 24..46  goals 23+ -> 47   likewise
/// ```
#[test]
fn goal_column_past_a_word_wrapped_row_rests_on_its_last_glyph_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-row-end*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((w2 (split-window nil -24 'right))
                         (noninteractive nil)
                         (walk
                          (lambda ()
                            (mapcar
                             (lambda (start)
                               (cons start
                                     (mapcar (lambda (n)
                                               (goto-char start)
                                               (vertical-motion (cons n 0))
                                               (point))
                                             '(20 21 22 23 24 40))))
                             '(1 20 43 60)))))
                     (select-window w2)
                     (switch-to-buffer b)
                     (erase-buffer)
                     (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (let ((wrapped (funcall walk)))
                       (setq-local word-wrap nil)
                       (list (window-body-width) wrapped (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (24 \
         ((1 19 19 19 19 19 19) \
          (20 40 41 42 42 42 42) \
          (43 59 59 59 59 59 59) \
          (60 80 81 82 83 83 83)) \
         ((1 21 22 23 24 24 24) \
          (20 21 22 23 24 24 24) \
          (43 44 45 46 47 47 47) \
          (60 67 68 69 70 70 70)))"
    );
}

#[test]
fn tty_window_body_width_reserves_the_non_rightmost_separator_column() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(progn
             (delete-other-windows)
             (let* ((left (selected-window))
                    (right (split-window-horizontally 50)))
               (list (list (window-body-width left)
                           (window-total-width left)
                           (window-right-divider-width left))
                     (list (window-body-width right)
                           (window-total-width right)
                           (window-right-divider-width right)))))"#,
    );
    assert_eq!(result, "OK ((49 50 0) (30 30 0))");
}

#[test]
fn window_geometry_queries_match_batch_alias_and_edge_shapes() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(list (symbol-function 'window-inside-pixel-edges)
               (symbol-function 'window-inside-edges))
         (let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-mode-line-height w)
                 (window-mode-line-height m)
                 (window-header-line-height w)
                 (window-header-line-height m)
                 (window-pixel-height w)
                 (window-pixel-height m)
                 (window-pixel-width w)
                 (window-pixel-width m)
                 (window-text-height w)
                 (window-text-height m)
                 (window-text-height w t)
                 (window-text-height m t)
                 (window-text-width w)
                 (window-text-width m)
                 (window-text-width w t)
                 (window-text-width m t)
                 (window-body-pixel-edges w)
                 (window-body-pixel-edges m)
                 (window-pixel-edges w)
                 (window-pixel-edges m)
                 (window-body-edges w)
                 (window-body-edges m)
                 (window-edges w)
                 (window-edges m)
                 (window-edges w t)
                 (window-edges m t)))
         (list (condition-case err (window-mode-line-height 999999) (error err))
               (condition-case err (window-header-line-height 999999) (error err))
               (condition-case err (window-pixel-height 999999) (error err))
               (condition-case err (window-pixel-width 999999) (error err))
               (condition-case err (window-text-height 999999) (error err))
               (condition-case err (window-text-width 999999) (error err))
               (condition-case err (window-body-pixel-edges 999999) (error err))
               (condition-case err (window-pixel-edges 999999) (error err))
               (condition-case err (window-body-edges 999999) (error err))
               (condition-case err (window-edges 999999) (error err))
               (condition-case err (window-text-height nil nil nil) (error err))
               (condition-case err (window-mode-line-height nil nil) (error err))
               (condition-case err (window-inside-pixel-edges nil nil) (error (car err)))
               (condition-case err (window-edges nil nil nil nil) (error err))
               (condition-case err (window-edges nil nil nil nil nil) (error err)))",
    );
    assert_eq!(out[0], "OK (window-body-pixel-edges window-body-edges)");
    assert_eq!(
        out[1],
        "OK (1 0 0 0 24 1 80 80 23 1 23 1 80 80 80 80 (0 0 80 23) (0 24 80 25) (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25) (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25))"
    );
    assert_eq!(
        out[2],
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-valid-p 999999) (wrong-type-argument window-valid-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (error \"999999 is not a live window\") (error \"999999 is not a valid window\") (error \"999999 is not a live window\") (error \"999999 is not a valid window\") (wrong-number-of-arguments window-text-height 3) (wrong-number-of-arguments window-mode-line-height 2) wrong-number-of-arguments (0 0 80 24) (wrong-number-of-arguments (0 . 4) 5))"
    );
}

#[test]
fn one_window_p_tracks_current_window_count() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(list (one-window-p)
               (progn
                 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
                 (one-window-p)))",
    );
    assert_eq!(results[0], "OK (t nil)");
}

#[test]
fn one_window_p_enforces_max_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (one-window-p nil nil nil) (error (car err)))",
    );
    assert_eq!(results[0], "OK wrong-number-of-arguments");
}

#[test]
fn gnu_lisp_window_edges_use_logical_outer_and_presented_body_regions() {
    crate::test_utils::init_test_tracing();
    for (presentation, scrollbar_left, body_left, body_right) in
        [(1, true, 173.0, 756.0), (2, false, 160.0, 743.0)]
    {
        let mut ev = runtime_startup_context();
        let fid = ev
            .frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id;
        let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
        let outer = neomacs_display_protocol::types::Rect::new(144.0, 32.0, 640.0, 480.0);
        let body = neomacs_display_protocol::types::Rect::new(
            body_left,
            54.0,
            body_right - body_left,
            438.0,
        );
        let scrollbar = neomacs_display_protocol::types::Rect::new(
            if scrollbar_left { 144.0 } else { 771.0 },
            54.0,
            13.0,
            438.0,
        );
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        neovm_display::test_support::authority::prepare_and_activate_display_presentation_for_test(
            frame,
            crate::window::geometry::PresentationId::new(presentation),
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id: wid,
                cell_origin: crate::window::geometry::CellOrigin::new(18, 2),
                regions: crate::window::PresentedWindowRegions {
                    outer,
                    text_body: body,
                    left_margin_columns: 1,
                    right_margin_columns: 2,
                    left_margin: Some(neomacs_display_protocol::types::Rect::new(
                        165.0, 54.0, 8.0, 438.0,
                    )),
                    right_margin: Some(neomacs_display_protocol::types::Rect::new(
                        body_right, 54.0, 16.0, 438.0,
                    )),
                    left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        if scrollbar_left { 157.0 } else { 144.0 },
                        54.0,
                        8.0,
                        438.0,
                    )),
                    right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        body_right + 16.0,
                        54.0,
                        12.0,
                        438.0,
                    )),
                    left_scroll_bar: scrollbar_left.then_some(scrollbar),
                    right_scroll_bar: (!scrollbar_left).then_some(scrollbar),
                    tab_line: Some(neomacs_display_protocol::types::Rect::new(
                        144.0, 32.0, 640.0, 17.0,
                    )),
                    header_line: Some(neomacs_display_protocol::types::Rect::new(
                        144.0, 49.0, 640.0, 5.0,
                    )),
                    mode_line: Some(neomacs_display_protocol::types::Rect::new(
                        144.0, 492.0, 640.0, 20.0,
                    )),
                    right_divider: Some(neomacs_display_protocol::types::Rect::new(
                        782.0, 32.0, 2.0, 480.0,
                    )),
                    bottom_divider: Some(neomacs_display_protocol::types::Rect::new(
                        144.0, 510.0, 640.0, 2.0,
                    )),
                    ..Default::default()
                },
                regions_materialized: true,
                header_line_height: 99,
                tab_line_height: 99,
                mode_line_height: 99,
                text_area_left_offset: 999,
                points: vec![crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: neovm_edit::buffer::LispCharPos1::ONE,
                    x: 72,
                    y: 999,
                    width: 7,
                    height: 17,
                    row: 99,
                    col: 9,
                }],
                body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                    output_row: 99,
                    body_row: 2,
                    body_y: 34,
                }],
                ..Default::default()
            }],
        )
        .expect("presented GUI geometry");
        let logical_window = frame.find_window_mut(wid).expect("live window");
        logical_window.set_bounds(crate::window::Rect::new(0.0, 0.0, 80.0, 24.0));
        logical_window.set_left_col(18);
        logical_window.set_top_line(2);
        let display = frame
            .find_window_mut(wid)
            .expect("live window")
            .display_mut()
            .expect("leaf display state");
        display.scroll_bar_width = 13;
        // Redisplay publishes these widths from the window's own state, and
        // `window-fringes'/`window-margins' read that state as GNU does.
        display.left_fringe_width = 8;
        display.right_fringe_width = 12;
        display.vertical_scroll_bar_type =
            Value::symbol(if scrollbar_left { "left" } else { "right" });
        display.horizontal_scroll_bar_type = Value::NIL;
        let crate::window::Window::Leaf { margins, .. } =
            frame.find_window_mut(wid).expect("live window")
        else {
            panic!("the selected window should be a leaf");
        };
        *margins = crate::window::WindowMargins::new(1, 2);

        let result = ev
            .eval_str(
                "(list (window-pixel-left)
                       (window-pixel-top)
                       (window-left-column)
                       (window-top-line)
                       (window-pixel-edges)
                       (window-inside-pixel-edges)
                       (window-inside-edges)
                       (window-body-width)
                       (window-text-width)
                       (window-body-height)
                       (window-text-height)
                       (window-text-width nil t)
                       (window-text-height nil t)
                       (posn-at-point 1))",
            )
            .expect("GNU window.el geometry query");
        assert_eq!(
            neovm_eval::emacs_core::print::print_value(&result),
            format!(
                "(0 0 18 2 (0 0 80 24) ({} 22 {} 460) ({} 1 {} 29) 72 72 27 27 {} 438 (#<window {}> 1 (72 . 34) 0 nil 1 (9 . 2) nil (0 . 0) (7 . 17)))",
                (body_left - outer.x) as i64,
                (body_right - outer.x) as i64,
                ((body_left - outer.x) / 8.0).floor() as i64,
                ((body_right - outer.x) / 8.0).ceil() as i64,
                (body_right - body_left) as i64,
                wid.0,
            )
        );
    }
}

#[test]
fn display_buffer_returns_window() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(get-buffer-create \"disp-buf\")
         (windowp (display-buffer \"disp-buf\"))",
    );
    assert_eq!(results[1], "OK t");
}

#[test]
fn pop_to_buffer_returns_buffer() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(get-buffer-create \"pop-buf\")
         (bufferp (pop-to-buffer \"pop-buf\"))",
    );
    assert_eq!(results[1], "OK t");
}

#[test]
fn switch_display_pop_bootstrap_initial_frame() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(save-current-buffer (bufferp (switch-to-buffer \"*scratch*\")))
         (save-current-buffer (windowp (display-buffer \"*scratch*\")))
         (save-current-buffer (bufferp (pop-to-buffer \"*scratch*\")))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
}

#[test]
fn switch_display_pop_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (switch-to-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (display-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (pop-to-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (set-window-buffer (selected-window) \"*scratch*\" nil nil) (error (car err)))",
    );
    assert_eq!(results[0], "OK wrong-number-of-arguments");
    assert_eq!(results[1], "OK wrong-number-of-arguments");
    assert_eq!(results[2], "OK wrong-number-of-arguments");
    assert_eq!(results[3], "OK wrong-number-of-arguments");
}

#[test]
fn switch_display_pop_reject_non_buffer_designators() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (switch-to-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (display-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (pop-to-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (set-window-buffer (selected-window) 1) (error (list (car err) (nth 1 err) (nth 2 err))))",
    );
    assert_eq!(results[0], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[1], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[2], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[3], "OK (wrong-type-argument stringp 1)");
}

#[test]
fn switch_and_pop_create_missing_named_buffers() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(save-current-buffer (bufferp (switch-to-buffer \"sw-auto-create\")))
         (buffer-live-p (get-buffer \"sw-auto-create\"))
         (kill-buffer \"sw-auto-create\")
         (save-current-buffer (bufferp (pop-to-buffer \"pop-auto-create\")))
         (buffer-live-p (get-buffer \"pop-auto-create\"))
         (kill-buffer \"pop-auto-create\")",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK t");
    assert_eq!(results[4], "OK t");
    assert_eq!(results[5], "OK t");
}

#[test]
fn display_buffer_missing_or_dead_signals_invalid_buffer() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (display-buffer \"db-missing\") (error err))
         (let ((b (get-buffer-create \"db-dead\")))
           (kill-buffer b)
           (condition-case err (display-buffer b) (error err)))",
    );
    assert_eq!(results[0], "OK (error \"Invalid buffer\")");
    assert_eq!(results[1], "OK (error \"Invalid buffer\")");
}

/// A wrap that lands exactly at end of buffer starts no screen line, so
/// `vertical-motion' must report that it moved over none.
///
/// GNU returns the number of screen lines actually moved over, "closer to zero
/// if beginning or end of buffer was reached" (src/indent.c, Fvertical_motion).
/// In batch GNU answers from `vmotion' -> `compute_motion', which stops at ZV
/// and counts only the lines it genuinely crossed. Filling the body width
/// exactly puts point at ZV with nothing on a following line, so the count is
/// zero even though point moves.
///
/// The one-past case is pinned alongside it deliberately: with a single extra
/// character there IS an occupied continuation line, and the answer is 1. That
/// keeps the end-of-buffer rule from degenerating into "always report zero at
/// ZV", which would break every ordinary wrap.
#[test]
fn vertical_motion_counts_only_screen_lines_that_are_actually_occupied() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-vertical-motion-eob*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((width (window-body-width)))
                     (mapcar
                      (lambda (n)
                        (erase-buffer)
                        (insert (make-string n ?x))
                        (goto-char (point-min))
                        (list n (vertical-motion 1) (point)))
                      (list (- width 1) width (+ width 1)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    // Ground truth taken from GNU 31.0.90 on the same probe, body width 80:
    //   ((79 0 80) (80 1 80) (81 1 80))
    assert_eq!(
        result, "OK ((79 0 80) (80 1 80) (81 1 80))",
        "a wrap landing exactly at end of buffer must report no screen line \
         moved, while one character more must still report the continuation line"
    );
}

/// A TRUNCATE row CLIPPED at the window's right edge is one screen line to
/// GNU's display iterator and none at all to `compute_motion`, and this port
/// has to answer both.
///
/// GNU's two `vertical-motion` engines part company exactly here, and their
/// own sources say so in four lines:
///
/// * `compute_motion`'s truncating branch skips to the next newline and
///   leaves `vpos` alone (`src/indent.c:1494-1502`), where its CONTINUING
///   branch increments it (`src/indent.c:1523`).  A clipped row that runs to
///   the end of the buffer without a newline therefore crosses no screen line
///   at all -- and `count-screen-lines`, which is `1+` that count unless the
///   end is off-screen (`lisp/window.el:9886-9889`), then answers 0 for a
///   buffer with text in it.  **That is GNU's own answer under `--batch`**,
///   measured: `((78 0 79 1) (79 0 80 1) (80 0 81 1) (81 0 82 0) (160 0 161 0))`.
/// * The display iterator's `MOVE_LINE_TRUNCATED` arm reseats to the next
///   visible line start and falls through to `++it->vpos`
///   (`src/xdisp.c:11118-11143` and `:11200`); only the "Stop when ZV reached"
///   exit above it (`src/xdisp.c:10250-10257`, which runs BEFORE the row is
///   found to overflow) returns without counting.  So a row that the buffer
///   merely ran out on counts none, and a row that was CLIPPED counts one.
///
/// The two answers cannot both come out of one row label, which is why
/// `ScreenLineEnd::ClippedAtBufferEnd` is read through `MotionEngine`.
///
/// `(vertical-motion 0)` from ZV is pinned beside it, and it is the reason the
/// engine-dependence lives on the FORWARD count and not on "is `next` a row
/// start": GNU answers the LOGICAL line start under BOTH engines, at every one
/// of these lengths, because its backward walk
/// (`move_it_vertically_backward`, `src/xdisp.c:11473-11492`) puts the clipped
/// remainder back on the row it was clipped from.  A fix that made the clipped
/// boundary a row START would answer 81 here where GNU answers 1.
///
/// Ground truth, GNU Emacs 31.0.90, body width 80, `truncate-lines' t, one
/// line of `x' with no trailing newline, fields
/// `(LEN MOVED POINT COUNT-SCREEN-LINES VERTICAL-MOTION-0-FROM-ZV)':
///
/// ```text
///   --batch          (79 0 80 1 1) (80 0 81 1 1) (81 0 82 0 1)
///   -nw in a pty     (79 0 80 1 1) (80 1 81 2 1) (81 1 82 1 1)
/// ```
#[test]
fn a_clipped_truncate_row_counts_as_a_screen_line_only_under_the_display_iterator() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-clipped-truncate-row*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (walk
                           (lambda ()
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x))
                                (setq-local truncate-lines t)
                                (setq-local word-wrap nil)
                                (goto-char (point-min))
                                (let* ((moved (vertical-motion (buffer-size)))
                                       (landed (point))
                                       (csl (count-screen-lines (point-min)
                                                                (point-max)))
                                       (bovl (progn (goto-char (point-max))
                                                    (vertical-motion 0)
                                                    (point))))
                                  (list n moved landed csl bovl)))
                              (list (- width 1) width (+ width 1))))))
                     (list width
                           (funcall walk)
                           (let ((noninteractive nil)) (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((79 0 80 1 1) (80 0 81 1 1) (81 0 82 0 1)) \
         ((79 0 80 1 1) (80 1 81 2 1) (81 1 82 1 1)))",
        "a clipped TRUNCATE row must count as one screen line under the display \
         iterator and as none under compute_motion, while `vertical-motion 0' \
         from ZV answers the logical line start under both"
    );
}

/// The same clipped row, but NOT the last one: a TRUNCATE row that ends at a
/// NEWLINE counts under BOTH engines, and must keep doing so.
///
/// This is the control that keeps the engine split from being applied to every
/// truncated row.  `compute_motion` reaches the newline it skipped to and
/// increments `vpos` there (`src/indent.c:1494-1502` leaves `pos` just before
/// the newline, and the main loop then consumes it), while the display
/// iterator counted the row itself; the two arrive at the same place with the
/// same count.  Only the row whose clipped remainder ends at ZV -- with no
/// newline for `compute_motion` to reach -- divides them.
///
/// Ground truth, GNU Emacs 31.0.90, body width 80, `truncate-lines' t, over
/// LEN `x', a newline, LEN `y' and a newline, fields
/// `(LEN MOVED POINT COUNT-SCREEN-LINES)':
///
/// ```text
///   --batch        (79 2 161 2) (80 2 163 2) (160 2 323 2)
///   -nw in a pty   (79 2 161 2) (80 2 163 3) (160 2 323 3)
/// ```
///
/// The MOVED and POINT columns are identical in the two engines -- it is only
/// `count-screen-lines`, which narrows away the final newline and so ends the
/// buffer in the middle of a clipped row, that can tell them apart.
#[test]
fn a_clipped_truncate_row_that_ends_at_a_newline_counts_under_both_engines() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-clipped-truncate-newline*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (walk
                           (lambda ()
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x) "\n"
                                        (make-string n ?y) "\n")
                                (setq-local truncate-lines t)
                                (setq-local word-wrap nil)
                                (goto-char (point-min))
                                (let* ((moved (vertical-motion (buffer-size)))
                                       (landed (point))
                                       (csl (count-screen-lines (point-min)
                                                                (point-max))))
                                  (list n moved landed csl)))
                              (list (- width 1) width (+ width 80))))))
                     (list width
                           (funcall walk)
                           (let ((noninteractive nil)) (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((79 2 161 2) (80 2 163 2) (160 2 323 2)) \
         ((79 2 161 2) (80 2 163 3) (160 2 323 3)))",
        "a truncated row terminated by a newline must count under both engines; \
         only the one whose clipped remainder ends at ZV divides them"
    );
}

/// The end of the accessible buffer is a goal-column STOP on the row that
/// reached it -- one more stop than that row has glyphs.
///
/// GNU's goal walk is `move_it_in_display_line (&it, ZV, first_x + to_x,
/// MOVE_TO_X)` (`src/indent.c:2540`), and `move_it_in_display_line_to` tests
/// `get_next_display_element` -- "Stop when ZV reached" -- BEFORE it tests the
/// row's right edge (`src/xdisp.c:10250-10257`).  So a row the buffer ran out
/// on returns `MOVE_POS_MATCH_OR_ZV` with the iterator standing ON ZV, which
/// draws nothing and therefore sits one column past the last glyph, exactly
/// like the newline on a terminated row.
///
/// A CLIPPED row is the case that keeps this from being "always stop at ZV":
/// its ZV is past the window edge, and the walk stops at the edge first.
///
/// Measured under GNU Emacs 31.0.90, body width 80, one line of `x' with no
/// trailing newline, `(vertical-motion (cons GOAL 0))' from `point-min',
/// identical whether the window truncates or wraps:
///
/// ```text
///   len 78   goal 77 -> 78    goal 78, 79, 80, 200 -> 79   (79 is ZV)
///   len 79   goal 77 -> 78    goal 78 -> 79   goal 79, 80, 200 -> 80  (80 is ZV)
///   len 80   goal 77 -> 78    goal 78 -> 79   goal 79, 80, 200 -> 80  (the row
///                                     was CLIPPED, so its ZV -- 81 -- is not
///                                     on it and 80 is the last stop)
/// ```
#[test]
fn the_end_of_the_buffer_is_a_goal_column_stop_on_the_row_that_reached_it() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-goal-at-buffer-end*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (noninteractive nil)
                          (walk
                           (lambda (trunc)
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x))
                                (setq-local truncate-lines trunc)
                                (setq-local word-wrap nil)
                                (cons n
                                      (mapcar
                                       (lambda (goal)
                                         (goto-char (point-min))
                                         (vertical-motion (cons goal 0))
                                         (point))
                                       (list (- width 3) (- width 2)
                                             (- width 1) width 200))))
                              (list (- width 2) (- width 1) width)))))
                     (list width
                           (funcall walk t)
                           (funcall walk nil))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((78 78 79 79 79 79) (79 78 79 80 80 80) (80 78 79 80 80 80)) \
         ((78 78 79 79 79 79) (79 78 79 80 80 80) (80 78 79 80 80 80)))",
        "the goal-column walk must be able to come to rest at the end of the \
         accessible buffer, which is a stop on the row that reached it"
    );
}

/// A hscrolled window's right edge is `hscroll` columns further into the LINE,
/// so a hscrolled `TRUNCATE` row is clipped later -- or not at all.
///
/// GNU puts the hscroll into the iterator's coordinates, not into the row's
/// content: `it->first_visible_x = window_hscroll_limited (w, f) *
/// FRAME_COLUMN_WIDTH (it->f)` (`src/xdisp.c:3500-3501`) and
/// `it->last_visible_x = it->first_visible_x + body_width`
/// (`src/xdisp.c:3507`), less the truncation glyph on a `TRUNCATE` row
/// (`src/xdisp.c:3512-3518`).  `it->current_x` still starts at 0 at the line
/// start, so the column at which the row is cut off is
/// `hscroll + body-width - 1` and not `body-width - 1`.
///
/// Measured under GNU Emacs 31.0.90 at body width 80, `truncate-lines' t, one
/// line of `x' with no trailing newline, `set-window-hscroll' with NO
/// redisplay, `(vertical-motion (buffer-size))' from `point-min':
///
/// ```text
///   len  80   hscroll 0                 -> (1 81)   clipped at column 79
///   len  80   hscroll 1, 5, 40, 100     -> (0 81)   80 columns now fit
///   len 160   hscroll 0, 1, 5, 40       -> (1 161)  still clipped
///   len 160   hscroll 100, 200          -> (0 161)  no longer clipped
/// ```
///
/// The boundary is exactly `hscroll + 79`: 160 columns are clipped while that
/// is 119 and not while it is 179.
#[test]
fn a_hscrolled_truncate_row_is_clipped_hscroll_columns_further_along() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-hscrolled-clip*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (noninteractive nil))
                     (cons width
                           (mapcar
                            (lambda (n)
                              (erase-buffer)
                              (insert (make-string n ?x))
                              (setq-local truncate-lines t)
                              (setq-local word-wrap nil)
                              (cons n
                                    (mapcar
                                     (lambda (hs)
                                       (set-window-hscroll (selected-window) hs)
                                       (goto-char (point-min))
                                       (list hs (vertical-motion (buffer-size))
                                             (point)))
                                     '(0 1 5 40 100))))
                            (list width (* 2 width))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (set-window-hscroll (selected-window) 0)
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         (80 (0 1 81) (1 0 81) (5 0 81) (40 0 81) (100 0 81)) \
         (160 (0 1 161) (1 1 161) (5 1 161) (40 1 161) (100 0 161)))",
        "the column at which a TRUNCATE row is cut off is hscroll + body-width \
         - 1, so hscrolling a window makes a row that was clipped stop being \
         clipped"
    );
}

#[test]
fn category_word_wrap_respects_custom_ascii_categories_and_kinsoku() {
    let result = bootstrap_eval_one_with_frame(
        r#"(progn
      (delete-other-windows)
      (select-window (split-window nil -24 'right))
      (mapcar
       (lambda (kind)
         (switch-to-buffer (get-buffer-create (format " *category-wrap-%s*" kind)))
         (set-category-table (copy-category-table))
         (setq-local word-wrap t truncate-lines nil truncate-partial-width-windows nil word-wrap-by-category t)
         (modify-category-entry ?x ?|)
         (cond ((eq kind 'plain) (insert "supertag v0.4 " (make-string 60 ?x)))
               ((eq kind 'closing) (insert (make-string 23 ?x) "!" (make-string 20 ?x)) (modify-category-entry ?! ?|) (modify-category-entry ?! ?>))
               ((eq kind 'opening) (insert (make-string 22 ?x) "?" (make-string 20 ?x)) (modify-category-entry ?? ?|) (modify-category-entry ?? ?<)))
         (goto-char 1)
         (let ((noninteractive nil)) (vertical-motion 1))
         (point))
       '(plain closing opening)))"#,
    );
    assert_eq!(result, "OK (24 23 23)");
}

/// Word wrapping may add policy reads at display stops, not one obarray walk
/// per ordinary character. Count real symbol reads around the public command
/// instead of using a timing threshold that depends on the test machine.
#[test]
#[cfg(not(feature = "display-stop-validate"))]
fn category_word_wrap_symbol_reads_are_bounded_by_display_stops() {
    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(progn
        (delete-other-windows)
        (set-frame-size nil 1200 24)
        (switch-to-buffer (get-buffer-create " *wrap-policy-reads*"))
        (insert (make-string 1500 ?x))
        (set-category-table (copy-category-table))
        (modify-category-entry ?x ?|)
        (setq-local truncate-lines nil truncate-partial-width-windows nil)
        (setq noninteractive nil))"#,
    )
    .expect("prepare a long terminal row");

    for category in ["nil", "t"] {
        let mut reads = Vec::new();
        for wrap in ["nil", "t"] {
            ev.eval_str(&format!(
                "(progn (setq-local word-wrap {wrap} word-wrap-by-category {category}) (goto-char 1))"
            ))
            .expect("select wrap mode");
            ev.obarray().reset_symbol_slot_read_count();
            ev.eval_str("(vertical-motion 1)").expect("advance one row");
            reads.push(ev.obarray().symbol_slot_read_count());
            assert!(
                ev.edit
                    .buffers
                    .current_buffer()
                    .unwrap()
                    .point_char_pos()
                    .get()
                    > 1000
            );
        }
        assert!(
            reads[1] <= reads[0] + 128,
            "category={category}: word wrapping added {} symbol reads to one ordinary row (character wrap={}, word wrap={})",
            reads[1].saturating_sub(reads[0]),
            reads[0],
            reads[1]
        );
    }
}
