use super::*;
use crate::emacs_core::intern::{intern_calls, reset_intern_calls};
use strum::IntoEnumIterator;

fn context() -> Context {
    let mut ctx = Context::new();
    let buffer = ctx.buffers.create_buffer("*ttykey-k4-command-symbols*");
    ctx.buffers.set_current(buffer);
    let frame = ctx.frames.create_frame("ttykey-k4", 80, 24, buffer);
    assert!(ctx.frames.select_frame(frame));
    ctx
}

fn printed(ctx: &mut Context, source: &str) -> String {
    crate::emacs_core::print::print_value(&ctx.eval_str(source).expect("command-symbol test form"))
}

#[test]
fn fixed_command_symbol_warm_lookups_avoid_named_intern() {
    let mut ctx = context();
    ctx.assign("this-command", Value::fixnum(1));
    for symbol in CommandLoopSymbol::iter() {
        symbol.id();
    }
    reset_intern_calls();
    for symbol in CommandLoopSymbol::iter() {
        symbol.id();
    }
    for _ in 0..20 {
        ctx.assign_by_id(CommandLoopSymbol::ThisCommand.id(), Value::fixnum(2));
        assert_eq!(
            ctx.eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
                .expect("bound command"),
            Value::fixnum(2)
        );
    }
    assert_eq!(
        intern_calls(),
        0,
        "warm command sites must skip named interning"
    );
}

#[test]
fn fixed_command_prologue_keeps_local_reset_and_hook_order() {
    let mut ctx = context();
    printed(
        &mut ctx,
        r#"(progn
             (setq prefix-arg 7 last-prefix-arg 8 last-command-event 97)
             (set-default 'deactivate-mark t)
             (set (make-local-variable 'deactivate-mark) t)
             (setq post-command-hook
                   (list (lambda ()
                           (setq ttykey-k4-prologue-observation
                                 (list prefix-arg last-prefix-arg deactivate-mark)
                                 this-command 'ttykey-k4-hook-command
                                 real-this-command 'ttykey-k4-hook-real)))))"#,
    );
    ctx.command_loop_1_entry_prologue().expect("entry prologue");
    assert_eq!(
        printed(
            &mut ctx,
            "(list prefix-arg last-prefix-arg deactivate-mark ttykey-k4-prologue-observation last-command real-last-command last-repeatable-command (default-value 'deactivate-mark))"
        ),
        "(nil nil nil (nil nil nil) ttykey-k4-hook-command ttykey-k4-hook-real ttykey-k4-hook-real t)"
    );
}

#[test]
fn fixed_command_assignment_stays_in_dynamic_binding() {
    let mut ctx = context();
    printed(
        &mut ctx,
        "(defvaralias 'ttykey-k4-command-binding-alias 'this-command)",
    );
    let symbol = CommandLoopSymbol::ThisCommand.id();
    ctx.assign_by_id(symbol, Value::fixnum(1));
    let depth = ctx.specpdl.len();
    ctx.try_specbind(symbol, Value::fixnum(2))
        .expect("dynamic command binding");
    ctx.assign_by_id(CommandLoopSymbol::ThisCommand.id(), Value::fixnum(3));
    let bound = ctx.eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id());
    let alias_bound = printed(&mut ctx, "ttykey-k4-command-binding-alias");
    ctx.unbind_to(depth);
    assert_eq!(bound.expect("bound command"), Value::fixnum(3));
    assert_eq!(alias_bound, "3");
    assert_eq!(
        ctx.eval_symbol_by_id(symbol).expect("restored command"),
        Value::fixnum(1)
    );
    assert_eq!(printed(&mut ctx, "ttykey-k4-command-binding-alias"), "1");
}

#[test]
fn fixed_command_assignment_preserves_existing_watcher_behavior() {
    let mut ctx = context();
    printed(
        &mut ctx,
        "(progn (setq ttykey-k4-watch-log nil) (add-variable-watcher 'last-command (lambda (&rest args) (setq ttykey-k4-watch-log (cons args ttykey-k4-watch-log)))))",
    );
    ctx.assign("last-command", Value::fixnum(7));
    let baseline = printed(&mut ctx, "ttykey-k4-watch-log");
    printed(&mut ctx, "(setq ttykey-k4-watch-log nil)");
    ctx.assign_by_id(CommandLoopSymbol::LastCommand.id(), Value::fixnum(7));
    assert_eq!(printed(&mut ctx, "ttykey-k4-watch-log"), baseline);
    assert_eq!(printed(&mut ctx, "last-command"), "7");
}

#[test]
fn fixed_command_identity_keeps_redefined_function_cell_live() {
    let mut ctx = context();
    printed(
        &mut ctx,
        "(progn (setq this-command 'ttykey-k4-live-command) (fset 'command-execute (lambda (cmd) (list 'old cmd))))",
    );
    let old = ctx
        .dispatch_command_in_loop(Value::NIL)
        .expect("first dispatch");
    assert_eq!(
        crate::emacs_core::print::print_value(&old),
        "(old ttykey-k4-live-command)"
    );
    printed(
        &mut ctx,
        "(fset 'command-execute (lambda (cmd) (list 'new cmd)))",
    );
    let new = ctx
        .dispatch_command_in_loop(Value::NIL)
        .expect("redefined dispatch");
    assert_eq!(
        crate::emacs_core::print::print_value(&new),
        "(new ttykey-k4-live-command)"
    );
}
