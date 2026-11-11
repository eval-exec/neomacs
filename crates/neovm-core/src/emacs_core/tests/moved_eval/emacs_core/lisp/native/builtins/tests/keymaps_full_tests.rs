//! BOOT-FULL keymap regressions, including key-description oracles split out of eval's
//! builtins/tests/keymaps_test.rs (EVAL-UNIT-RUNTIME: FULL tests re-home; the
//! other two tests in that file are CONTEXT-FREE and stay).
//!
//! Evaluations use runtime_startup_eval_all().

/// Kill and reap the isolated probe even if polling or an assertion panics.
struct KeymapOverrideProbe(std::process::Child);

impl Drop for KeymapOverrideProbe {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Internal compiler overrides may supply keymaps or aliases, but repeated
/// override targets must terminate as non-keymaps instead of retrying forever.
#[test]
fn keymap_autoload_compiler_overrides_terminate() {
    const PROBE_ENV: &str = "NEOVM_KEYMAP_OVERRIDE_TERMINATION_PROBE";
    const PROBE_OK: &str = "KEYMAP-OVERRIDE-PROBE-OK";
    if std::env::var_os(PROBE_ENV).is_none() {
        // The baseline loops without checking Lisp quit. Keep it outside the
        // test runner and bound its lifetime independently of runtime signals.
        let output = tempfile::NamedTempFile::new().expect("probe output file");
        let module = module_path!();
        let module = module.split_once("::").map_or(module, |(_, rest)| rest);
        let child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .arg("--exact")
            .arg(format!(
                "{module}::keymap_autoload_compiler_overrides_terminate"
            ))
            .arg("--nocapture")
            .env(PROBE_ENV, "1")
            .stdout(output.as_file().try_clone().expect("probe stdout"))
            .stderr(output.as_file().try_clone().expect("probe stderr"))
            .spawn()
            .expect("spawn keymap override probe");
        let mut child = KeymapOverrideProbe(child);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(status) = child.0.try_wait().expect("poll keymap override probe") {
                let log = std::fs::read_to_string(output.path()).expect("read probe output");
                assert!(
                    status.success(),
                    "keymap override probe failed: {status}\n{log}"
                );
                assert!(
                    log.lines().any(|line| line == PROBE_OK),
                    "keymap override probe did not run its assertions:\n{log}"
                );
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "keymap override resolution did not terminate within 30 seconds"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(progn
             (autoload 'neovm--override-self "neovm--missing-override-self" nil t 'keymap)
             (autoload 'neovm--override-cycle-a "neovm--missing-override-a" nil t 'keymap)
             (autoload 'neovm--override-cycle-b "neovm--missing-override-b" nil t 'keymap)
             (let ((map (make-sparse-keymap))
                   (prefixes (make-sparse-keymap))
                   (results nil))
               (define-key map [?x] 'ignore)
               (define-key prefixes [f2] 'neovm--override-self)
               (define-key prefixes [f3] 'neovm--override-cycle-a)
               (fset 'neovm--override-loaded-alias map)
               (unwind-protect
                   (progn
                     (setq internal--compiler-function-overrides
                           '((neovm--override-self . neovm--override-self)))
                     (push (list (condition-case err
                                     (lookup-key 'neovm--override-self [?x])
                                   (error (car err)))
                                 (condition-case err
                                     (define-key 'neovm--override-self [?y] 'forward-char)
                                   (error (car err)))
                                 (lookup-key prefixes [f2 ?x])
                                 (autoloadp (symbol-function 'neovm--override-self)))
                           results)
                     (setq internal--compiler-function-overrides
                           '((neovm--override-cycle-a . neovm--override-cycle-b)
                             (neovm--override-cycle-b . neovm--override-cycle-a)))
                     (push (list (condition-case err
                                     (lookup-key 'neovm--override-cycle-a [?x])
                                   (error (car err)))
                                 (condition-case err
                                     (define-key 'neovm--override-cycle-a [?y] 'forward-char)
                                   (error (car err)))
                                 (lookup-key prefixes [f3 ?x])
                                 (autoloadp (symbol-function 'neovm--override-cycle-a))
                                 (autoloadp (symbol-function 'neovm--override-cycle-b)))
                           results)
                     (setq internal--compiler-function-overrides
                           (list (cons 'neovm--override-self map)))
                     (push (list (lookup-key 'neovm--override-self [?x])
                                 (define-key 'neovm--override-self [?y] 'forward-char)
                                 (lookup-key 'neovm--override-self [?y])
                                 (autoloadp (symbol-function 'neovm--override-self)))
                           results)
                     (setq internal--compiler-function-overrides
                           '((neovm--override-self . neovm--override-loaded-alias)))
                     (push (list (lookup-key 'neovm--override-self [?x])
                                 (define-key 'neovm--override-self [?z] 'backward-char)
                                 (lookup-key 'neovm--override-self [?z])
                                 (autoloadp (symbol-function 'neovm--override-self)))
                           results))
                 (setq internal--compiler-function-overrides nil))
               (nreverse results)))"#,
    );
    assert_eq!(
        result,
        [
            "OK ((wrong-type-argument wrong-type-argument 1 t) (wrong-type-argument wrong-type-argument 1 t t) (ignore forward-char forward-char t) (ignore backward-char backward-char t))"
        ]
    );
    println!("\n{PROBE_OK}");
}

#[test]
fn key_description_accepts_position_bearing_mouse_events() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(let ((event '(C-mouse-movement
                          (nil 332 (801 . 219) 0 nil 332 (0 . 12)
                           nil (0 . 0) (1679 . 18)))))
             (list (key-description (list event))
                   (key-description (vector event))
                   (single-key-description event)
                   (single-key-description event t)))"#,
    );
    assert_eq!(
        result,
        [
            r#"OK ("C-<mouse-movement>" "C-<mouse-movement>" "C-<mouse-movement>" "C-mouse-movement")"#
        ]
    );
}

#[test]
fn single_key_description_distinguishes_event_payloads_ranges_and_lucid_lists() {
    // Literal results measured with GNU Emacs's public builtin.
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(mapcar (lambda (event)
                     (condition-case err (single-key-description event)
                       (error (car err))))
                   (list '(mouse-1 (nil 1 (2 . 3)))
                         '(drag-mouse-1 (nil 1 (2 . 3)) (nil 2 (4 . 5)))
                         (cons 'mouse-movement 42)
                         '((mouse-1) (nil))
                         '(meta shift up)
                         (cons ?a ?z)))"#,
    );
    assert_eq!(
        result,
        [r#"OK ("<mouse-1>" "<drag-mouse-1>" "<mouse-movement>" error "M-S-<up>" "a..z")"#]
    );
}

#[test]
fn string_vector_keys_go_through_key_parse_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU `possibly_translate_key_sequence' (src/keymap.c), used by
    // `define-key' and `lookup-key': a vector holding one string is parsed
    // with `key-parse' when `key-valid-p' accepts the string.
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(let ((map (make-sparse-keymap)))
             (list
              ;; GNU's keymap-test-lookups.
              (lookup-key (current-global-map) ["C-x C-f"])
              (define-key map ["C-c a"] 'cyc-command)
              map
              (lookup-key map ["C-c a"])
              (lookup-key map "\C-ca")
              (lookup-key map ["C-c"])
              (let ((valid (symbol-function 'key-valid-p)))
                (fmakunbound 'key-valid-p)
                (unwind-protect
                    (condition-case err (lookup-key map ["C-c a"]) (error err))
                  (fset 'key-valid-p valid)))))"#,
    );
    assert_eq!(
        result,
        [concat!(
            r#"OK (find-file cyc-command (keymap (3 keymap (97 . cyc-command)))"#,
            r#" cyc-command cyc-command (keymap (97 . cyc-command))"#,
            r#" (error "`key-valid-p' is not defined, so this syntax can't be used: %s" ["C-c a"]))"#,
        )]
    );
}

#[test]
fn define_key_keeps_a_symbol_keymap_alive_while_parsing_the_key() {
    crate::test_utils::init_test_tracing();
    // The map below is only reachable through the symbol's function cell,
    // which `key-valid-p' clears before collecting garbage; the weak table
    // only observes it. `define-key' resolved the map first, so it must keep
    // it alive and still update it.
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(let ((valid (symbol-function 'key-valid-p))
                 (weak (make-hash-table :test 'eq :weakness 'value)))
             (fset 'cyc-prefix-map (make-sparse-keymap))
             (puthash 'map (symbol-function 'cyc-prefix-map) weak)
             (unwind-protect
                 (progn
                   (fset 'key-valid-p
                         (lambda (key)
                           (fset 'cyc-prefix-map nil)
                           (garbage-collect)
                           (funcall valid key)))
                   (list (define-key 'cyc-prefix-map ["C-c b"] 'cyc-command)
                         (gethash 'map weak)))
               (fset 'key-valid-p valid)))"#,
    );
    assert_eq!(
        result,
        [r#"OK (cyc-command (keymap (3 keymap (98 . cyc-command))))"#]
    );
}
