//! Behavioural parity for the pinned `etaf-gamekit` adapter contract.
//!
//! These probes exercise the public engine registry and the validation that
//! runs before a game process starts.  They register only local launch
//! functions and never spawn an engine, so no Godot, OpenSpiel, boardgame.io,
//! Node, Python or graphical frame is involved.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-gamekit)

(defun neomacs-gamekit-backend (name)
  "Return NAME's public backend description, or nil."
  (cl-find name (etaf-gamekit-backends)
           :key (lambda (entry) (plist-get entry :name))))

(defun neomacs-gamekit-error (thunk)
  "Return THUNK's error as (TYPE MESSAGE)."
  (condition-case err
      (funcall thunk)
    (error (list (car err) (cadr err)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-gamekit", "0.2.0"), "etaf-gamekit.el")
        .expect("prepare pinned etaf-gamekit source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_gamekit_behavior_batch() {
    let cases = vec![
        // The package ships three built-in adapters with their public
        // capability descriptions, sorted by name.
        ParityBatchCase::value(
            "builtin_backends",
            r####"
(etaf-gamekit-backends)
"####,
            expect![[
                r#"OK ((:name boardgame-io :title "boardgame.io" :features (turns phases player-views replay-snapshots)) (:name godot :title "Godot" :features (real-time game-project snapshots)) (:name openspiel :title "OpenSpiel" :features (rule-catalog legal-actions chance simultaneous-actions snapshots undo-redo)))"#
            ]],
        ),
        // A custom adapter registers, rejects a duplicate, replaces in place,
        // validates its description, and unregisters idempotently.
        ParityBatchCase::value(
            "backend_registration",
            r####"
(unwind-protect
    (progn
      (etaf-gamekit-register-backend
       'neomacs-probe-backend :title "Probe Engine" :features '(alpha beta)
       :launch (lambda (options) (list "true" (plist-get options :mode))))
      (let ((added (neomacs-gamekit-backend 'neomacs-probe-backend))
            (duplicate
             (neomacs-gamekit-error
              (lambda ()
                (etaf-gamekit-register-backend
                 'neomacs-probe-backend :title "Again" :features nil
                 :launch #'ignore))))
            (bad
             (neomacs-gamekit-error
              (lambda ()
                (etaf-gamekit-register-backend
                 'neomacs-bad :title "" :features nil :launch #'ignore)))))
        (etaf-gamekit-register-backend
         'neomacs-probe-backend :title "Probe Engine 2" :features '(gamma)
         :launch #'ignore :replace t)
        (list added duplicate bad
              (neomacs-gamekit-backend 'neomacs-probe-backend)
              (let ((count (length (etaf-gamekit-backends))))
                (etaf-gamekit-unregister-backend 'neomacs-probe-backend)
                (list count
                      (neomacs-gamekit-backend 'neomacs-probe-backend)
                      (etaf-gamekit-unregister-backend 'neomacs-probe-backend))))))
  (etaf-gamekit-unregister-backend 'neomacs-probe-backend))
"####,
            expect![[
                r#"OK ((:name neomacs-probe-backend :title "Probe Engine" :features (alpha beta)) (etaf-gamekit-error "Engine already registered: neomacs-probe-backend") (etaf-gamekit-error "Engine registration needs a name, title, launch function and feature symbols") (:name neomacs-probe-backend :title "Probe Engine 2" :features (gamma)) (4 nil nil))"#
            ]],
        ),
        // Game definitions validate their title, backend and keyword options;
        // start rejects unknown engines and non-game values before a process
        // exists.
        ParityBatchCase::value(
            "definition_and_start_validation",
            r####"
(list
 (neomacs-gamekit-error
  (lambda ()
    (eval '(etaf-gamekit-define-game neomacs-bad-title
             :title "" :backend 'godot) t)))
 (neomacs-gamekit-error
  (lambda ()
    (eval '(etaf-gamekit-define-game neomacs-bad-backend
             :title "Bad" :backend :godot) t)))
 (neomacs-gamekit-error
  (lambda ()
    (eval '(etaf-gamekit-define-game neomacs-bad-options
             :title "Bad" :backend 'godot :options '(mode "split")) t)))
 (progn
  (etaf-gamekit-define-game neomacs-probe-game
    :title "Probe Game" :backend 'neomacs-probe-missing-backend
    :options '(:mode "split") :view #'ignore)
  (neomacs-gamekit-error (lambda () (etaf-gamekit-start neomacs-probe-game))))
 (neomacs-gamekit-error (lambda () (etaf-gamekit-start 'godot))))
"####,
            expect![[
                r#"OK ((etaf-gamekit-error "A game needs a symbolic backend, title and callable view/validator") (etaf-gamekit-error "A game needs a symbolic backend, title and callable view/validator") (etaf-gamekit-error "Game options must be a keyword plist") (etaf-gamekit-error "Unknown engine: neomacs-probe-missing-backend") (etaf-gamekit-error "Start requires a game defined with etaf-gamekit-define-game"))"#
            ]],
        ),
        // A registered adapter that returns no command, or whose launch
        // function signals, fails the start with the adapter's own diagnostic.
        ParityBatchCase::value(
            "launch_failures",
            r####"
(unwind-protect
    (progn
      (etaf-gamekit-register-backend
       'neomacs-empty-backend :title "Empty" :features nil
       :launch (lambda (_options) nil))
      (etaf-gamekit-register-backend
       'neomacs-failing-backend :title "Failing" :features nil
       :launch (lambda (_options) (error "boom")))
      (etaf-gamekit-define-game neomacs-empty-game
        :title "Empty Game" :backend 'neomacs-empty-backend :view #'ignore)
      (etaf-gamekit-define-game neomacs-failing-game
        :title "Failing Game" :backend 'neomacs-failing-backend :view #'ignore)
      (list (neomacs-gamekit-error
             (lambda () (etaf-gamekit-start neomacs-empty-game)))
            (neomacs-gamekit-error
             (lambda () (etaf-gamekit-start neomacs-failing-game)))))
  (etaf-gamekit-unregister-backend 'neomacs-empty-backend)
  (etaf-gamekit-unregister-backend 'neomacs-failing-backend))
"####,
            expect![[
                r#"OK ((etaf-gamekit-error "Command must be a nonempty program/argument list") (etaf-gamekit-error "Backend neomacs-failing-backend configuration failed: boom"))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-gamekit-behavior", "etaf-gamekit", &cases);
}
