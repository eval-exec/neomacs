use super::*;
use crate::emacs_core::display_host::GuiResourceQuery;

struct ResourceHost(Arc<Mutex<Vec<GuiResourceQuery>>>);

#[test]
fn frame_font_selection_prefers_explicit_then_default_alist_then_resource() {
    let mut eval = Context::new();
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    let queries = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(ResourceHost(queries.clone())));
    for (arguments, expected) in [
        (
            r#"'((font . "Explicit-11") (font . "Older-10"))"#,
            "Explicit-11",
        ),
        ("nil", "Default-12"),
    ] {
        let expression = format!(
            r#"(let ((default-frame-alist '((font . "Default-12") (font . "Older-10")))) (frame-parameter (x-create-frame {arguments}) 'font))"#
        );
        assert_eq!(eval.eval_str(&expression).unwrap(), Value::string(expected));
    }
    assert!(queries.lock().unwrap().is_empty());
    assert_eq!(
        eval.eval_str("(frame-parameter (x-create-frame nil) 'font)")
            .unwrap(),
        Value::string("Mono-13")
    );
    assert_eq!(queries.lock().unwrap().last().unwrap().class, "Emacs.Font");
}

impl DisplayHost for ResourceHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn gui_resource(&self, query: &GuiResourceQuery) -> Option<String> {
        self.0.lock().unwrap().push(query.clone());
        Some(
            if query.name.ends_with(".empty") {
                ""
            } else {
                "Mono-13"
            }
            .into(),
        )
    }
}

#[test]
fn resource_lookup_uses_dynamic_keys_and_normalizes_empty_native_values() {
    let mut eval = Context::new();
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    let queries = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(ResourceHost(queries.clone())));
    assert_eq!(
        eval.eval_str(r#"(let ((x-resource-name "my.app") (x-resource-class "Custom") (inhibit-x-resources t)) (x-get-resource "font" "Face" "default" "Font"))"#).unwrap(),
        Value::string("Mono-13")
    );
    assert_eq!(
        &*queries.lock().unwrap(),
        &[GuiResourceQuery {
            name: "my_app.default.font".into(),
            class: "Custom.Face.Font".into(),
            inhibit_native: true,
        }]
    );
    assert!(
        eval.eval_str(r#"(x-get-resource "empty" "Empty")"#)
            .unwrap()
            .is_nil()
    );
}

#[test]
fn resource_lookup_validates_optional_pairs_before_querying_host() {
    let mut eval = Context::new();
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    let queries = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(ResourceHost(queries.clone())));
    for expression in [
        r#"(x-get-resource nil "Font")"#,
        r#"(x-get-resource "font" 4)"#,
        r#"(x-get-resource "font" "Font" 4 "Face")"#,
        r#"(x-get-resource "font" "Font" "default")"#,
        r#"(x-get-resource "font" "Font" nil "Face")"#,
    ] {
        assert!(eval.eval_str(expression).is_err(), "{expression}");
    }
    assert!(queries.lock().unwrap().is_empty());
    eval.eval_str(
        r#"(let ((x-resource-name ".?") (x-resource-class nil)) (x-get-resource "font" "Font"))"#,
    )
    .unwrap();
    let query = queries.lock().unwrap().last().unwrap().clone();
    assert_eq!(query.name, "emacs.font");
    assert_eq!(query.class, "Emacs.Font");
}

#[test]
fn resource_name_normalization_updates_the_visible_buffer_local_binding() {
    let mut eval = Context::new();
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    assert_eq!(
        eval.eval_str(
            r#"
        (progn
          (set-default 'x-resource-name "global")
          (set-buffer (get-buffer-create "*resource-local*"))
          (make-local-variable 'x-resource-name)
          (setq x-resource-name "local.name")
          (x-get-resource "font" "Font")
          (list x-resource-name (default-value 'x-resource-name)))
    "#
        )
        .unwrap()
        .to_string(),
        r#"("local_name" "global")"#
    );
}
