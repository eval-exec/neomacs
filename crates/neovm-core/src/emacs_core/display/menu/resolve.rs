//! GNU menu-item interpretation and dynamic property evaluation.
use super::{MenuRoots, separators, shortcuts};
use crate::emacs_core::error::{EvalResult, Flow};
use crate::emacs_core::keymap::{self, KeymapMarker, MenuButtonKind, MenuItemProperty};
use crate::emacs_core::{Context, PopupMenuEntry, Value};
use neomacs_display_protocol::menu::{
    MenuAvailability, MenuCheckState, MenuIndicator, MenuItemKind,
};

pub(super) fn parse(
    ctx: &mut Context,
    def: Value,
    depth: u32,
    is_tty: bool,
    roots: &MenuRoots,
) -> Result<Option<(PopupMenuEntry, Option<Value>)>, Flow> {
    if !def.is_cons() {
        return Ok(None);
    }
    let name;
    let mut command;
    let mut help = None;
    let mut button = None;
    let mut keys = shortcuts::Hints::default();
    let mut enable = Value::T;
    let mut filter = None;
    let car = def.cons_car();
    let tail = def.cons_cdr();
    if KeymapMarker::MenuItem.is_value(car) && tail.is_cons() {
        name = roots.keep(tail.cons_car());
        let tail = tail.cons_cdr();
        command = roots.keep(if tail.is_cons() {
            tail.cons_car()
        } else {
            Value::NIL
        });
        let mut props = if tail.is_cons() {
            tail.cons_cdr()
        } else {
            Value::NIL
        };
        if props.is_cons() && props.cons_car().is_cons() {
            props = props.cons_cdr();
        }
        while props.is_cons() {
            let keyword = props.cons_car();
            props = roots.keep(props.cons_cdr());
            if !props.is_cons() {
                break;
            }
            let value = roots.keep(props.cons_car());
            props = roots.keep(props.cons_cdr());
            match MenuItemProperty::from_value(keyword) {
                Some(MenuItemProperty::Visible) if property(ctx, value)?.is_nil() => {
                    return Ok(None);
                }
                Some(MenuItemProperty::Help) => help = help_text(ctx, value)?,
                Some(MenuItemProperty::Keys) => {
                    let callable = ctx.funcall_general(Value::symbol("functionp"), vec![value])?;
                    keys.explicit = if !callable.is_nil() {
                        Some(roots.keep(ctx.funcall_general(value, vec![])?))
                    } else if value.is_string() || value.is_cons() {
                        Some(value)
                    } else {
                        None
                    };
                }
                Some(MenuItemProperty::KeySequence)
                    if value.is_symbol() || value.is_string() || value.is_vector() =>
                {
                    keys.sequence = Some(value)
                }
                Some(MenuItemProperty::Enable) => {
                    enable = if ctx
                        .eval_symbol("enable-disabled-menus-and-buttons")?
                        .is_nil()
                    {
                        value
                    } else {
                        Value::T
                    };
                }
                Some(MenuItemProperty::Filter) => filter = Some(value),
                Some(MenuItemProperty::Button) if value.is_cons() => {
                    button = MenuButtonKind::from_value(value.cons_car())
                        .map(|kind| (kind, roots.keep(value.cons_cdr())));
                }
                _ => {}
            }
        }
    } else {
        if !car.is_string() {
            return Ok(None);
        }
        name = roots.keep(car);
        command = roots.keep(tail);
        if command.is_cons() && command.cons_car().is_string() {
            help = help_text(ctx, roots.keep(command.cons_car()))?;
            command = command.cons_cdr();
        }
        // GNU's obsolete cache occupies one cons before the actual dotted
        // definition. It is neither a command nor a submenu.
        if command.is_cons() && command.cons_car().is_cons() {
            let cached_sequence = command.cons_car().cons_car();
            if cached_sequence.is_nil() || cached_sequence.is_vector() {
                command = command.cons_cdr();
            }
        }
        roots.keep(command);
        if ctx
            .eval_symbol("enable-disabled-menus-and-buttons")?
            .is_nil()
            && let Some(symbol) = command.as_symbol_name()
            && let Some(form) = ctx.obarray().get_property(symbol, "menu-enable")
            && !form.is_nil()
        {
            enable = roots.keep(form);
        }
    }
    let name = if name.is_string() {
        name
    } else {
        property(ctx, name)?
    };
    let Some(mut label) = name.as_runtime_string_owned() else {
        return Ok(None);
    };
    let separator = separators::is_separator(&label);
    if let Some(filter) = filter {
        let call = Value::list(vec![
            Value::symbol("funcall"),
            Value::list(vec![Value::symbol("quote"), filter]),
            Value::list(vec![Value::symbol("quote"), command]),
        ]);
        command = roots.keep(property(ctx, call)?);
    }
    let enabled = enable == Value::T || !property(ctx, enable)?.is_nil();
    let map = keymap::get_keymap_in_runtime(ctx, &command, false, true)?;
    let child = (!map.is_nil()).then(|| roots.keep(map));
    let shortcut = if child.is_none() && !command.is_nil() {
        shortcuts::resolve(ctx, command, keys, roots)?
    } else {
        String::new()
    };
    let indicator = if !command.is_nil()
        && !separator
        && child.is_none()
        && let Some((kind, form)) = button
    {
        let selected = property(ctx, form)?;
        let state = if selected.is_nil() {
            MenuCheckState::Off
        } else {
            MenuCheckState::On
        };
        match kind {
            MenuButtonKind::Toggle => MenuIndicator::Toggle(state),
            MenuButtonKind::Radio => MenuIndicator::Radio(state),
        }
    } else {
        MenuIndicator::None
    };
    if is_tty && child.is_some() {
        label.push_str(" >");
    }
    let kind = if separator {
        MenuItemKind::Separator
    } else if child.is_some() {
        MenuItemKind::Submenu {
            availability: MenuAvailability::from(enabled),
        }
    } else if command.is_nil() {
        MenuItemKind::Label
    } else {
        MenuItemKind::Command {
            availability: MenuAvailability::from(enabled),
            indicator,
        }
    };
    Ok(Some((
        PopupMenuEntry {
            kind,
            label,
            help,
            shortcut,
            depth,
        },
        child.filter(|_| !separator),
    )))
}

fn help_text(ctx: &mut Context, value: Value) -> Result<Option<String>, Flow> {
    let Some(text) = value.as_runtime_string_owned() else {
        return Ok(None);
    };
    // The Lisp helper is absent in a pre-bootstrap evaluator, as in the
    // documentation-property path. Real runtime startup loads help.el.
    if text.is_empty()
        || ctx
            .obarray()
            .symbol_function("substitute-command-keys")
            .is_none()
    {
        return Ok(Some(text));
    }
    let inhibited = ctx.funcall_general(
        Value::symbol("get-text-property"),
        vec![
            Value::fixnum(0),
            Value::symbol("help-echo-inhibit-substitution"),
            value,
        ],
    )?;
    if !inhibited.is_nil() {
        return Ok(Some(text));
    }
    let substituted = ctx.funcall_general(Value::symbol("substitute-command-keys"), vec![value])?;
    Ok(substituted.as_runtime_string_owned())
}

/// GNU evaluates menu forms dynamically with ordinary redisplay inhibited.
/// Its ordinary error/quit distinction is applied at this single seam.
fn property(ctx: &mut Context, form: Value) -> EvalResult {
    let count = ctx.specpdl.len();
    ctx.try_specbind(
        crate::emacs_core::intern::intern("inhibit-redisplay"),
        Value::T,
    )?;
    let guarded = Value::list(vec![
        Value::symbol("condition-case"),
        Value::NIL,
        form,
        Value::list(vec![Value::symbol("error"), Value::NIL]),
    ]);
    let result = ctx.eval_value_with_lexical_arg(guarded, Some(Value::NIL));
    ctx.unbind_to_with_result(count, result)
}
