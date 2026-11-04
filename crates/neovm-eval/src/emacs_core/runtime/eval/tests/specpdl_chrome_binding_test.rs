//! A local chrome binding must reach an ordinary redisplay within a command.
//! The interactive TTY reproducer also verifies actual terminal output. This
//! renderer spy exercises the redisplay skip gate and records its format.

use super::*;
use std::cell::RefCell;
use std::rc::Rc;

fn chrome_context(name: &'static str) -> (Context, Rc<RefCell<Vec<String>>>) {
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames
        .create_frame("specpdl-chrome", 80 * 8, 24 * 16, buffer);
    eval.buffers.get_mut(buffer).unwrap().insert("abc\ndef\n");
    eval.eval_str(&format!("(set (make-local-variable '{name}) \"OLD\")"))
        .expect("initial local format");
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let observed = rendered.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        // Chrome formats are BUFFER_OBJFWD slots. The bufferless variable
        // view deliberately has no value for these forwarded descriptors.
        let format = eval
            .variable_value_in_buffer(eval.buffers.current_buffer(), intern(name))
            .expect("buffer-local chrome format");
        observed
            .borrow_mut()
            .push(crate::emacs_core::print::print_value(&format));
        // Match the renderer's acknowledgement so unchanged content cannot
        // mask the direct binding's responsibility to request redisplay.
        eval.buffers
            .current_buffer()
            .unwrap()
            .reset_unchanged_region();
    }));
    (eval, rendered)
}

#[test]
fn local_chrome_binding_repaints_the_bound_format_without_a_force() {
    // This spy acknowledges text, but does not seal a GNU accepted frame.
    let _policy = RedisplayHookPolicyGuard::legacy();
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        let (mut eval, rendered) = chrome_context(name);
        eval.redisplay().expect("initial presentation");
        eval.eval_str(&format!("(let (({name} \"BOUND\")) (redisplay))"))
            .expect("bound presentation");
        assert_eq!(
            &*rendered.borrow(),
            &["\"OLD\"", "\"BOUND\""],
            "the renderer must receive the bound {name} within the command"
        );
    }
}

#[test]
fn local_chrome_unwind_repaints_the_restored_format_without_a_force() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        let (mut eval, rendered) = chrome_context(name);
        let scope = eval.specpdl.len();
        eval.try_specbind(intern(name), Value::string("BOUND"))
            .expect("local binding");
        // Present the bound format even before the bind publication fix, so
        // this regression independently requires restoration publication.
        eval.redisplay_with_force(true).expect("bound presentation");
        eval.unbind_to(scope);
        eval.redisplay().expect("restored presentation");
        assert_eq!(
            &*rendered.borrow(),
            &["\"BOUND\"", "\"OLD\""],
            "the renderer must receive restored {name} within the command"
        );
    }
}

#[test]
fn local_chrome_alias_binding_repaints_bound_and_restored_formats() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        let (mut eval, rendered) = chrome_context(name);
        eval.eval_str(&format!("(defvaralias 'ttykey-chrome-alias '{name})"))
            .expect("alias to the local chrome variable");
        eval.redisplay().expect("initial presentation");
        eval.eval_str("(let ((ttykey-chrome-alias \"BOUND\")) (redisplay))")
            .expect("aliased bound presentation");
        eval.redisplay().expect("restored presentation");
        assert_eq!(
            &*rendered.borrow(),
            &["\"OLD\"", "\"BOUND\"", "\"OLD\""],
            "alias binding and restoration must publish resolved {name}"
        );
    }
}

#[test]
fn non_chrome_local_bindings_do_not_publish_chrome() {
    for (name, projected) in [
        ("tab-width", true),
        ("fill-column", false),
        ("ttykey-local-control", false),
    ] {
        let mut eval = Context::new();
        eval.eval_str(&format!("(set (make-local-variable '{name}) 8)"))
            .expect("initial local value");
        let sym = intern(name);
        assert_eq!(eval.runtime_binding_has_projection(sym), projected);
        let buffer = eval.buffers.current_buffer_id().expect("current buffer");
        let old = eval
            .variable_value_in_buffer(eval.buffers.get(buffer), sym)
            .expect("old local value");
        let bound = eval.eval_str("3").expect("bound value");
        let before = (eval.redisplay_generation(), eval.display_var_change_count);
        let scope = eval.specpdl.len();
        eval.specbind_uncached(sym, bound).expect("local binding");
        assert_eq!(
            eval.variable_value_in_buffer(eval.buffers.get(buffer), sym),
            Some(bound),
            "the actual {name} binding must be installed"
        );
        assert_eq!(
            (eval.redisplay_generation(), eval.display_var_change_count),
            before,
            "a conservative mask positive must still reject non-chrome {name}"
        );
        eval.unbind_to(scope);
        assert_eq!(
            eval.variable_value_in_buffer(eval.buffers.get(buffer), sym),
            Some(old),
            "the actual {name} binding must be restored"
        );
        assert_eq!(
            (eval.redisplay_generation(), eval.display_var_change_count),
            before,
            "restoring non-chrome {name} must not publish chrome"
        );
    }
}
