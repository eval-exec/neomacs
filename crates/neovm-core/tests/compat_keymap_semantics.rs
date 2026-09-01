mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct KeymapCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_keymap_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping keymap semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        KeymapCase {
            name: "keymap_text_property_precedes_minor_local_and_global_maps",
            form: r#"(progn
  (defvar demo-mode nil)
  (let ((g (make-sparse-keymap))
      (l (make-sparse-keymap))
      (m (make-sparse-keymap))
      (tp (make-sparse-keymap))
      (minor-mode-map-alist nil))
  (unwind-protect
      (with-temp-buffer
        (use-global-map g)
        (use-local-map l)
        (define-key g "a" 'global-a)
        (define-key l "a" 'local-a)
        (define-key m "a" 'minor-a)
        (define-key tp "a" 'textprop-a)
        (setq demo-mode t)
        (setq minor-mode-map-alist (list (cons 'demo-mode m)))
        (insert "ab")
        (put-text-property 1 2 'keymap tp)
        (goto-char 1)
        (list
         (mapcar (lambda (map) (lookup-key map "a" t))
                 (current-active-maps nil 1))
         (key-binding "a" t nil 1)
         (key-binding "a" t nil (copy-marker 1))))
    (setq minor-mode-map-alist nil))))"#,
        },
        KeymapCase {
            name: "local_map_text_property_replaces_buffer_local_map_only_at_position",
            form: r#"(progn
  (defvar demo-mode nil)
  (let ((g (make-sparse-keymap))
      (l (make-sparse-keymap))
      (m (make-sparse-keymap))
      (lp (make-sparse-keymap))
      (minor-mode-map-alist nil))
  (unwind-protect
      (with-temp-buffer
        (use-global-map g)
        (use-local-map l)
        (define-key g "a" 'global-a)
        (define-key l "a" 'local-a)
        (define-key m "a" 'minor-a)
        (define-key lp "a" 'property-local-a)
        (setq demo-mode t)
        (setq minor-mode-map-alist (list (cons 'demo-mode m)))
        (insert "ab")
        (put-text-property 1 2 'local-map lp)
        (goto-char 1)
        (list
         (mapcar (lambda (map) (lookup-key map "a" t))
                 (current-active-maps nil 1))
         (mapcar (lambda (map) (lookup-key map "a" t))
                 (current-active-maps nil 2))
         (key-binding "a" t nil 1)
         (key-binding "a" t nil 2)))
    (setq minor-mode-map-alist nil))))"#,
        },
        KeymapCase {
            name: "overriding_local_map_suppresses_text_property_minor_and_local_maps",
            form: r#"(progn
  (defvar demo-mode nil)
  (let ((g (make-sparse-keymap))
      (l (make-sparse-keymap))
      (m (make-sparse-keymap))
      (tp (make-sparse-keymap))
      (ov (make-sparse-keymap))
      (minor-mode-map-alist nil)
      (overriding-local-map nil))
  (with-temp-buffer
    (use-global-map g)
    (use-local-map l)
    (define-key g "a" 'global-a)
    (define-key l "a" 'local-a)
    (define-key m "a" 'minor-a)
    (define-key tp "a" 'textprop-a)
    (define-key ov "a" 'override-a)
    (setq demo-mode t)
    (setq minor-mode-map-alist (list (cons 'demo-mode m)))
    (insert "ab")
    (put-text-property 1 2 'keymap tp)
    (goto-char 1)
    (setq overriding-local-map ov)
    (list
     (mapcar (lambda (map) (lookup-key map "a" t))
             (current-active-maps t 1))
     (key-binding "a" t nil 1)))))"#,
        },
        KeymapCase {
            name: "overriding_terminal_local_map_precedes_all_other_active_maps",
            form: r#"(progn
  (defvar demo-mode nil)
  (let ((g (make-sparse-keymap))
      (l (make-sparse-keymap))
      (m (make-sparse-keymap))
      (tp (make-sparse-keymap))
      (term (make-sparse-keymap))
      (minor-mode-map-alist nil)
      (overriding-terminal-local-map nil))
  (with-temp-buffer
    (use-global-map g)
    (use-local-map l)
    (define-key g "a" 'global-a)
    (define-key l "a" 'local-a)
    (define-key m "a" 'minor-a)
    (define-key tp "a" 'textprop-a)
    (define-key term "a" 'terminal-override-a)
    (setq demo-mode t)
    (setq minor-mode-map-alist (list (cons 'demo-mode m)))
    (insert "ab")
    (put-text-property 1 2 'keymap tp)
    (goto-char 1)
    (setq overriding-terminal-local-map term)
    (list
     (mapcar (lambda (map) (lookup-key map "a" t))
             (current-active-maps t 1))
     (key-binding "a" t nil 1)))))"#,
        },
        KeymapCase {
            name: "set_keymap_parent_rejects_cyclic_inheritance",
            form: r#"(let ((a (make-sparse-keymap))
      (b (make-sparse-keymap)))
  (set-keymap-parent a b)
  (condition-case err
      (progn
        (set-keymap-parent b a)
        'no-error)
    (error (list (car err) (cadr err)))))"#,
        },
        KeymapCase {
            name: "keymap_parent_and_set_keymap_parent_follow_symbol_function_indirection",
            form: r#"(let ((parent (make-sparse-keymap))
      (child (make-sparse-keymap)))
  (fset 'compat-parent-map parent)
  (fset 'compat-child-map child)
  (list
   (eq (set-keymap-parent 'compat-child-map 'compat-parent-map) parent)
   (eq (keymap-parent 'compat-child-map) parent)))"#,
        },
        KeymapCase {
            name: "keymap_parent_autoloads_keymap_symbols",
            form: r#"(let* ((sym 'compat-autoload-keymap)
       (file (make-temp-file "compat-keymap-" nil ".el"))
       (base (file-name-sans-extension file)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert
           "(let ((parent (make-sparse-keymap)) (map (make-sparse-keymap)))"
           "  (define-key parent \"z\" 'autoload-parent-z)"
           "  (set-keymap-parent map parent)"
           "  (fset 'compat-autoload-keymap map))"))
        (fmakunbound sym)
        (autoload sym base nil nil 'keymap)
        (list
         (autoloadp (symbol-function sym))
         (keymapp (keymap-parent sym))
         (autoloadp (symbol-function sym))))
    (ignore-errors (delete-file file))
    (fmakunbound sym)))"#,
        },
        KeymapCase {
            name: "lookup_key_autoloads_keymap_symbols",
            form: r#"(let* ((sym 'compat-lookup-autoload-keymap)
       (file (make-temp-file "compat-keymap-" nil ".el"))
       (base (file-name-sans-extension file)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert
           "(let ((map (make-sparse-keymap)))"
           "  (define-key map \"z\" 'autoload-lookup-z)"
           "  (fset 'compat-lookup-autoload-keymap map))"))
        (fmakunbound sym)
        (autoload sym base nil nil 'keymap)
        (list
         (autoloadp (symbol-function sym))
         (lookup-key sym "z")
         (autoloadp (symbol-function sym))))
    (ignore-errors (delete-file file))
    (fmakunbound sym)))"#,
        },
        KeymapCase {
            name: "define_key_autoloads_keymap_symbols",
            form: r#"(let* ((sym 'compat-define-autoload-keymap)
       (file (make-temp-file "compat-keymap-" nil ".el"))
       (base (file-name-sans-extension file)))
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "(fset 'compat-define-autoload-keymap (make-sparse-keymap))"))
        (fmakunbound sym)
        (autoload sym base nil nil 'keymap)
        (list
         (autoloadp (symbol-function sym))
         (progn
           (define-key sym "z" 'autoload-define-z)
           (lookup-key sym "z"))
         (autoloadp (symbol-function sym))))
    (ignore-errors (delete-file file))
    (fmakunbound sym)))"#,
        },
        KeymapCase {
            name: "lookup_key_accepts_nil_keymap",
            form: r#"(list (lookup-key nil "a")
      (lookup-key nil ""))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "keymap semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

#[test]
fn command_dispatch_accepts_default_keymap_bindings_like_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping default keymap dispatch audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (defvar neovm-default-binding-events nil)
  (defun neovm-default-binding-local-fallback ()
    (interactive)
    (push 'local-fallback neovm-default-binding-events))
  (defun neovm-default-binding-local-specific ()
    (interactive)
    (push 'local-specific neovm-default-binding-events))
  (defun neovm-default-binding-override-fallback ()
    (interactive)
    (push 'override-fallback neovm-default-binding-events))
  (let ((buffer (generate-new-buffer " *neovm-default-binding*"))
        (local-map (make-sparse-keymap))
        (override-map (make-sparse-keymap)))
    (unwind-protect
        (save-window-excursion
          (set-window-buffer (selected-window) buffer)
          (with-current-buffer buffer
            (setq neovm-default-binding-events nil)
            (define-key local-map [t] #'neovm-default-binding-local-fallback)
            (define-key local-map (kbd "x") #'neovm-default-binding-local-specific)
            (define-key override-map [t] #'neovm-default-binding-override-fallback)
            (use-local-map local-map)
            (execute-kbd-macro (kbd "!"))
            (execute-kbd-macro (kbd "x"))
            (let ((overriding-local-map override-map))
              (execute-kbd-macro (kbd "x")))
            (list
             (nreverse neovm-default-binding-events)
             (buffer-string)
             (lookup-key local-map [t])
             (key-binding (kbd "!"))
             (let ((overriding-local-map override-map))
               (key-binding (kbd "x") t)))))
      (kill-buffer buffer))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "command dispatch default keymap binding semantics differ from GNU Emacs"
    );
}
