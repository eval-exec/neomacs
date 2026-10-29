//! Tests for the evaluator-to-renderer chrome controls.

use super::*;
use crate::emacs_core::display_host::DisplayHost;
use crate::emacs_core::eval::{Context, GuiFrameHostRequest};
use crate::emacs_core::value::Value;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
struct CapturingHost {
    scroll: Arc<Mutex<Vec<bool>>>,
    spacing: Arc<Mutex<Vec<(f32, f32)>>>,
}

impl DisplayHost for CapturingHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_scroll_indicators(&mut self, enabled: bool) -> Result<(), String> {
        self.scroll.lock().expect("scroll log").push(enabled);
        Ok(())
    }

    fn set_extra_spacing(&mut self, line_spacing: f32, letter_spacing: f32) -> Result<(), String> {
        self.spacing
            .lock()
            .expect("spacing log")
            .push((line_spacing, letter_spacing));
        Ok(())
    }
}

#[test]
fn chrome_subrs_are_registered() {
    let mut eval = Context::new();
    let found = eval
        .eval_str(
            "(list (fboundp 'neomacs-set-scroll-indicators)
                   (fboundp 'neomacs-set-extra-spacing))",
        )
        .expect("evaluation");
    assert_eq!(found, Value::list(vec![Value::T, Value::T]));
}

#[test]
fn chrome_subrs_reach_the_display_host() {
    let host = CapturingHost::default();
    let mut eval = Context::new();
    eval.set_display_host(Box::new(host.clone()));
    eval.eval_str(
        "(progn
           (neomacs-set-scroll-indicators t)
           (neomacs-set-scroll-indicators nil)
           (neomacs-set-extra-spacing 4 2))",
    )
    .expect("chrome subrs should run");

    assert_eq!(*host.scroll.lock().expect("scroll log"), vec![true, false]);
    assert_eq!(*host.spacing.lock().expect("spacing log"), vec![(4.0, 2.0)]);
}

#[test]
fn extra_spacing_rejects_non_numbers() {
    let mut eval = Context::new();
    let error = eval
        .eval_str("(neomacs-set-extra-spacing 'x 2)")
        .expect_err("a non-number spacing must signal");
    assert!(
        format!("{error:?}").contains("wrong-type-argument"),
        "unexpected error: {error:?}"
    );
}
