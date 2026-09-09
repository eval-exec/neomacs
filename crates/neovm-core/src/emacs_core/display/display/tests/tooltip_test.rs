use super::*;
use crate::emacs_core::eval::{DisplayHost, GuiFrameHostRequest};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct TooltipProbe {
    requests: Vec<TooltipRequest>,
    visible: bool,
    generation: u64,
    invalidate_on_hide: bool,
}
struct TooltipHost(Arc<Mutex<TooltipProbe>>);
impl DisplayHost for TooltipHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn show_tooltip(&mut self, _: FrameId, request: TooltipRequest) -> Result<(), String> {
        let mut probe = self.0.lock().unwrap();
        probe.requests.push(request);
        probe.visible = true;
        Ok(())
    }
    fn hide_tooltip(&mut self) -> Result<bool, String> {
        let mut probe = self.0.lock().unwrap();
        if probe.invalidate_on_hide {
            probe.generation += 1;
        }
        Ok(std::mem::take(&mut probe.visible))
    }
    fn tooltip_generation(&self) -> Option<neomacs_display_protocol::tooltip::TooltipGeneration> {
        Some(
            neomacs_display_protocol::tooltip::TooltipGeneration::from_raw(
                self.0.lock().unwrap().generation,
            ),
        )
    }
}

#[test]
fn help_callback_cannot_stamp_stale_help_with_a_new_generation() {
    let (mut eval, probe) = gui();
    probe.lock().unwrap().invalidate_on_hide = true;
    eval.eval_str("(setq show-help-function #'x-show-tip)")
        .unwrap();
    let callback = eval
        .eval_str("(lambda (&rest ignored) (x-hide-tip) \"old help\")")
        .unwrap();
    eval.show_help_echo(callback, Value::NIL, Value::NIL, Value::NIL)
        .unwrap();
    let probe = probe.lock().unwrap();
    assert_eq!(probe.generation, 1);
    assert_eq!(probe.requests[0].generation.unwrap().raw(), 0);
}

fn gui() -> (Context, Arc<Mutex<TooltipProbe>>) {
    let mut eval = Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    let probe = Arc::new(Mutex::new(TooltipProbe::default()));
    eval.set_display_host(Box::new(TooltipHost(probe.clone())));
    (eval, probe)
}

#[test]
fn gui_tooltip_primitive_does_not_report_a_terminal_frame() {
    let (mut eval, probe) = gui();
    assert!(
        eval.eval_str("(x-show-tip \"Help\" (selected-frame))")
            .unwrap()
            .is_nil()
    );
    assert_eq!(probe.lock().unwrap().requests[0].text, "Help");
    assert_eq!(probe.lock().unwrap().requests[0].offset, (5, -10));
    assert_eq!(eval.eval_str("(x-hide-tip)").unwrap(), Value::T);
    assert_eq!(eval.eval_str("(x-hide-tip)").unwrap(), Value::NIL);
}

#[test]
fn tooltip_preserves_unicode_face_runs_colors_and_explicit_parameters() {
    let (mut eval, probe) = gui();
    eval.eval_str(r##"(x-show-tip (propertize "αβ" 'face '(:foreground "#ff0000" :weight bold)) nil '((background-color . "#112233") (internal-border-width . 4)) 2 7 9)"##).unwrap();
    let probe = probe.lock().unwrap();
    let request = &probe.requests[0];
    assert_eq!(request.text, "αβ");
    assert_eq!(request.offset, (7, 9));
    assert_eq!(request.timeout, std::time::Duration::from_secs(2));
    assert_eq!(request.padding, 4);
    assert_eq!(request.background, Some(0x112233));
    assert_eq!(request.runs[0].range, 0..2);
    assert_eq!(
        request.runs[0].face.foreground,
        neomacs_display_protocol::Color::rgb(1.0, 0.0, 0.0)
    );
    assert_eq!(
        request.runs[0].face.background,
        neomacs_display_protocol::Color::from_pixel(0x112233)
    );
    assert_eq!(request.runs[0].face.font_weight, 700);
}

#[test]
fn unsupported_absolute_placement_and_invalid_timeout_do_not_reach_the_host() {
    let (mut eval, probe) = gui();
    assert!(
        eval.eval_str("(x-show-tip \"Help\" nil '((left . 20)))")
            .is_err()
    );
    assert!(eval.eval_str("(x-show-tip \"Help\" nil nil -1)").is_err());
    assert!(probe.lock().unwrap().requests.is_empty());
}

#[test]
fn terminal_tooltips_keep_the_window_system_error() {
    let mut eval = Context::new();
    let result = eval
        .eval_str("(condition-case err (x-show-tip \"Help\") (error (car (cdr err))))")
        .unwrap();
    assert_eq!(
        result.as_str_owned().as_deref(),
        Some("Window system frame should be used")
    );
}
