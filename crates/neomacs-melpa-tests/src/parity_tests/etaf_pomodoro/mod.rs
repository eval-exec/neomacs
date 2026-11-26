//! Behavioural parity for the pinned `etaf-pomodoro` countdown session.
//!
//! Every probe drives the public session API (`etaf-pomodoro-create`,
//! `-state`, `-toggle`, `-reset`, `-extend`, `-next-style` and
//! `-stop-session`) without mounting a view, so no timer, window, frame or
//! network is involved.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-pomodoro)
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-pomodoro", "0.2.0"), "etaf-pomodoro.el")
        .expect("prepare pinned etaf-pomodoro source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_pomodoro_behavior_batch() {
    let cases = vec![
        // A default session is a paused 25-minute focus with the minimal
        // style.
        ParityBatchCase::value(
            "default_session",
            r####"
(etaf-pomodoro-state (etaf-pomodoro-create))
"####,
            expect![[r#"OK (:remaining 1500 :total 1500 :running nil :style minimal)"#]],
        ),
        // The caller chooses the focus length and one of the published
        // styles.
        ParityBatchCase::value(
            "explicit_options",
            r####"
(list (etaf-pomodoro-state (etaf-pomodoro-create :minutes 1))
      (etaf-pomodoro-state (etaf-pomodoro-create :minutes 3 :style 'ring))
      (etaf-pomodoro-state (etaf-pomodoro-create :style 'card)))
"####,
            expect![[
                r#"OK ((:remaining 60 :total 60 :running nil :style minimal) (:remaining 180 :total 180 :running nil :style ring) (:remaining 1500 :total 1500 :running nil :style card))"#
            ]],
        ),
        // Invalid lengths and styles are rejected with the documented error
        // and leave no session behind.
        ParityBatchCase::value(
            "invalid_construction",
            r####"
(list
 (condition-case err
     (etaf-pomodoro-create :minutes 0)
   (etaf-pomodoro-error (list (car err) (cadr err))))
 (condition-case err
     (etaf-pomodoro-create :minutes -3)
   (etaf-pomodoro-error (list (car err) (cadr err))))
 (condition-case err
     (etaf-pomodoro-create :minutes 2.5)
   (etaf-pomodoro-error (list (car err) (cadr err))))
 (condition-case err
     (etaf-pomodoro-create :style 'fancy)
   (etaf-pomodoro-error (list (car err) (cadr err)))))
"####,
            expect![[
                r#"OK ((etaf-pomodoro-error "Focus length must be a positive number of minutes: 0") (etaf-pomodoro-error "Focus length must be a positive number of minutes: -3") (etaf-pomodoro-error "Focus length must be a positive number of minutes: 2.5") (etaf-pomodoro-error "Unknown Pomodoro style: fancy"))"#
            ]],
        ),
        // Toggling starts and pauses the countdown without changing its
        // length.
        ParityBatchCase::value(
            "toggle_running",
            r####"
(let ((session (etaf-pomodoro-create :minutes 2)))
  (let ((started (etaf-pomodoro-toggle session))
        (started-state (etaf-pomodoro-state session)))
    (let ((paused (etaf-pomodoro-toggle session)))
      (list started started-state paused (etaf-pomodoro-state session)))))
"####,
            expect![[
                r#"OK (t (:remaining 120 :total 120 :running t :style minimal) nil (:remaining 120 :total 120 :running nil :style minimal))"#
            ]],
        ),
        // Extending adds focus time to both the remaining and the full
        // length, and resetting returns to that extended length.
        ParityBatchCase::value(
            "extend_and_reset",
            r####"
(let ((session (etaf-pomodoro-create :minutes 1)))
  (etaf-pomodoro-toggle session)
  (let ((default (etaf-pomodoro-extend session))
        (custom (etaf-pomodoro-extend session 10)))
    (list default custom (etaf-pomodoro-reset session))))
"####,
            expect![[
                r#"OK ((:remaining 360 :total 360 :running t :style minimal) (:remaining 960 :total 960 :running t :style minimal) (:remaining 960 :total 960 :running nil :style minimal))"#
            ]],
        ),
        // A non-positive extension is rejected.
        ParityBatchCase::value(
            "invalid_extension",
            r####"
(let ((session (etaf-pomodoro-create)))
  (list (condition-case err
            (etaf-pomodoro-extend session 0)
          (etaf-pomodoro-error (list (car err) (cadr err))))
        (condition-case err
            (etaf-pomodoro-extend session -2)
          (etaf-pomodoro-error (list (car err) (cadr err))))
        (etaf-pomodoro-state session)))
"####,
            expect![[
                r#"OK ((etaf-pomodoro-error "Extension must be a positive number of minutes: 0") (etaf-pomodoro-error "Extension must be a positive number of minutes: -2") (:remaining 1500 :total 1500 :running nil :style minimal))"#
            ]],
        ),
        // Styles cycle in their published order and wrap around.
        ParityBatchCase::value(
            "style_cycle",
            r####"
(let ((session (etaf-pomodoro-create)))
  (list (etaf-pomodoro-next-style session)
        (etaf-pomodoro-next-style session)
        (etaf-pomodoro-next-style session)
        (etaf-pomodoro-state session)
        (let ((card (etaf-pomodoro-create :style 'card)))
          (list (etaf-pomodoro-next-style card)
                (plist-get (etaf-pomodoro-state card) :style)))))
"####,
            expect![[
                r#"OK (ring card minimal (:remaining 1500 :total 1500 :running nil :style minimal) (minimal minimal))"#
            ]],
        ),
        // Every state operation rejects a non-session with the same error.
        ParityBatchCase::value(
            "invalid_session",
            r####"
(mapcar
 (lambda (operation)
   (condition-case err
       (funcall operation 'not-a-session)
     (etaf-pomodoro-error (list (car err) (cadr err)))))
 (list #'etaf-pomodoro-state #'etaf-pomodoro-toggle
       #'etaf-pomodoro-reset #'etaf-pomodoro-extend
       #'etaf-pomodoro-next-style))
"####,
            expect![[
                r#"OK ((etaf-pomodoro-error "Not a Pomodoro session: not-a-session") (etaf-pomodoro-error "Not a Pomodoro session: not-a-session") (etaf-pomodoro-error "Not a Pomodoro session: not-a-session") (etaf-pomodoro-error "Not a Pomodoro session: not-a-session") (etaf-pomodoro-error "Not a Pomodoro session: not-a-session"))"#
            ]],
        ),
        // Stopping a session keeps its values and tolerates a non-session.
        ParityBatchCase::value(
            "stop_session",
            r####"
(let ((session (etaf-pomodoro-create :minutes 1)))
  (etaf-pomodoro-toggle session)
  (let ((stopped (etaf-pomodoro-stop-session session)))
    (list (eq stopped session)
          (etaf-pomodoro-state session)
          (eq (etaf-pomodoro-stop-session nil) nil)
          (eq (etaf-pomodoro-stop-session 'other) 'other))))
"####,
            expect![[r#"OK (t (:remaining 60 :total 60 :running t :style minimal) t t)"#]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-pomodoro-behavior", "etaf-pomodoro", &cases);
}
