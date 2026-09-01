//! GC-managed shader-surface handle tests (`shader_surface.rs`).
//!
//! `neomacs-surface-create` returns a `SurfaceObj` pseudovector handle; a
//! handle Lisp drops without `neomacs-surface-destroy` must drive
//! `DisplayHost::destroy_shader_surface` from the GC sweep's
//! pending-destroy drain instead of leaking the GPU texture until exit.

use super::eval::{Context, DisplayHost, GuiFrameHostRequest, ShaderSurfaceCreateRequest};
use super::value::Value;
use std::sync::{Arc, Mutex};

const STUB_SURFACE_ID: u32 = 42;

/// Stub host: creation always yields `STUB_SURFACE_ID`; uniform writes and
/// destroys are recorded so tests can assert which ids reached the host.
#[derive(Clone, Default)]
struct RecordingSurfaceDisplayHost {
    uniforms: Arc<Mutex<Vec<(u32, String)>>>,
    destroys: Arc<Mutex<Vec<u32>>>,
}

impl DisplayHost for RecordingSurfaceDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn create_shader_surface(&self, _request: ShaderSurfaceCreateRequest) -> Result<u32, String> {
        Ok(STUB_SURFACE_ID)
    }

    fn set_shader_surface_uniform(
        &self,
        id: u32,
        name: &str,
        _value: [f32; 4],
    ) -> Result<(), String> {
        self.uniforms
            .lock()
            .expect("surface host uniforms")
            .push((id, name.to_owned()));
        Ok(())
    }

    fn destroy_shader_surface(&self, id: u32) -> Result<(), String> {
        self.destroys
            .lock()
            .expect("surface host destroys")
            .push(id);
        Ok(())
    }
}

fn surface_context() -> (Context, RecordingSurfaceDisplayHost) {
    let host = RecordingSurfaceDisplayHost::default();
    let mut ctx = Context::new();
    ctx.set_display_host(Box::new(host.clone()));
    (ctx, host)
}

fn eval(ctx: &mut Context, source: &str) -> Value {
    ctx.eval_str(source).expect("surface form should evaluate")
}

#[test]
fn create_returns_gc_managed_handle() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, _host) = surface_context();

    let handle = eval(
        &mut ctx,
        r#"(neomacs-surface-create :width 8 :height 8 :shader "stub")"#,
    );
    assert!(handle.is_surface_handle());
    assert_eq!(handle.as_surface_handle(), Some(STUB_SURFACE_ID));
    // No longer a bare integer id.
    assert_eq!(handle.as_int(), None);
}

#[test]
fn set_uniform_and_destroy_accept_the_handle_and_a_fixnum() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, host) = surface_context();

    eval(
        &mut ctx,
        r#"
(progn
  (setq surface-test-handle
        (neomacs-surface-create :width 8 :height 8 :shader "stub"
                                :uniforms '((speed . 1.0))))
  (neomacs-surface-set-uniform surface-test-handle 'speed 3.5)
  (neomacs-surface-destroy surface-test-handle)
  ;; Plain integer ids stay accepted (backward compatibility).
  (neomacs-surface-destroy 7))
"#,
    );

    assert_eq!(
        *host.uniforms.lock().expect("surface host uniforms"),
        vec![(STUB_SURFACE_ID, "speed".to_owned())]
    );
    assert_eq!(
        *host.destroys.lock().expect("surface host destroys"),
        vec![STUB_SURFACE_ID, 7]
    );
}

#[test]
fn dropping_the_handle_and_collecting_garbage_destroys_the_surface() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, host) = surface_context();

    eval(
        &mut ctx,
        r#"(setq surface-test-handle
             (neomacs-surface-create :width 8 :height 8 :shader "stub"))"#,
    );

    // While the handle is reachable (global symbol value), GC must not
    // destroy the surface.
    eval(&mut ctx, "(garbage-collect)");
    assert!(
        host.destroys
            .lock()
            .expect("surface host destroys")
            .is_empty(),
        "GC destroyed a still-reachable surface handle"
    );

    // Drop the last reference: the next collection sweeps the dead handle
    // and the post-collection drain queues the host destroy.
    eval(&mut ctx, "(setq surface-test-handle nil)");
    eval(&mut ctx, "(garbage-collect)");
    assert_eq!(
        *host.destroys.lock().expect("surface host destroys"),
        vec![STUB_SURFACE_ID]
    );

    // The id was drained; a later collection must not destroy it again.
    eval(&mut ctx, "(garbage-collect)");
    assert_eq!(
        *host.destroys.lock().expect("surface host destroys"),
        vec![STUB_SURFACE_ID]
    );
}

#[test]
fn explicit_destroy_then_gc_double_free_is_delivered_but_harmless() {
    crate::test_utils::init_test_tracing();
    let (mut ctx, host) = surface_context();

    // Explicit destroy first (the render-thread free of a missing id is a
    // no-op, so the sweep's second destroy of the same id is harmless).
    eval(
        &mut ctx,
        r#"
(progn
  (setq surface-test-handle
        (neomacs-surface-create :width 8 :height 8 :shader "stub"))
  (neomacs-surface-destroy surface-test-handle)
  (setq surface-test-handle nil))
"#,
    );
    eval(&mut ctx, "(garbage-collect)");
    assert_eq!(
        *host.destroys.lock().expect("surface host destroys"),
        vec![STUB_SURFACE_ID, STUB_SURFACE_ID]
    );
}
