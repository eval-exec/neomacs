//! Evaluator-owned menu semantics. Presentation contains no Lisp values.

use neomacs_display_protocol::menu::MenuItemKind;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::PopupMenuEntry;
use neovm_eval::emacs_core::eval::keymap_lookup as keymap;
use neovm_value::error::{Flow, LispCondition};
use neovm_value::value::Value;
use resolve::parse;

mod resolve;

mod separators;
mod shortcuts;

#[derive(Debug, strum::Display)]
enum MenuBuildError {
    #[strum(serialize = "Cyclic menu keymap")]
    Cycle,
    #[strum(serialize = "Menu nesting limit exceeded")]
    TooDeep,
    #[strum(serialize = "Menu item limit exceeded")]
    TooManyItems,
}

impl MenuBuildError {
    fn signal(self) -> Flow {
        neovm_value::error::signal(LispCondition::Error, vec![Value::string(self.to_string())])
    }
}

#[derive(Default)]
struct MenuTraversal {
    path: Vec<Value>,
    ancestors: std::collections::HashSet<Value>,
}

/// Roots outlive preparation and remain live through the modal host call.
/// This thread-local scope must not move to a renderer or another evaluator.
struct MenuRoots {
    base: usize,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl MenuRoots {
    fn new() -> Self {
        Self {
            base: neovm_eval::emacs_core::eval::save_scratch_gc_roots(),
            _thread: std::marker::PhantomData,
        }
    }
    fn keep(&self, value: Value) -> Value {
        neovm_eval::emacs_core::eval::push_scratch_gc_root(value);
        value
    }
}

impl Drop for MenuRoots {
    fn drop(&mut self) {
        neovm_eval::emacs_core::eval::restore_scratch_gc_roots(self.base);
    }
}

pub(crate) struct ResolvedMenu {
    entries: Vec<PopupMenuEntry>,
    events: Vec<Value>,
    _roots: MenuRoots,
}

impl ResolvedMenu {
    pub(crate) fn entries(&self) -> &[PopupMenuEntry] {
        &self.entries
    }
    pub(crate) fn events(&self) -> &[Value] {
        &self.events
    }
}

pub(crate) fn resolve(ctx: &mut Context, menu: Value, is_tty: bool) -> Result<ResolvedMenu, Flow> {
    let mut result = ResolvedMenu {
        entries: Vec::new(),
        events: Vec::new(),
        _roots: MenuRoots::new(),
    };
    result._roots.keep(menu);
    let map = keymap::get_keymap_in_runtime(ctx, &menu, false, true)?;
    result._roots.keep(map);
    // GNU Fx_popup_menu (src/menu.c:1301-1320) turns the keymap's prompt
    // into the pane name, and the toolkit draws a pane name as the menu's
    // title row above the pane's items.
    let prompt = keymap_prompt(ctx, map);
    result._roots.keep(prompt);
    push_title_row(prompt, &mut result);
    append(ctx, map, is_tty, &mut MenuTraversal::default(), &mut result)?;
    Ok(result)
}

/// GNU Fkeymap_prompt (src/keymap.c:148-168): the first direct string
/// element of the keymap spine, looking through nested keymap elements.
fn keymap_prompt(ctx: &mut Context, map: Value) -> Value {
    let mut tail = map;
    while tail.is_cons() {
        let head = tail.cons_car();
        if head.is_string() {
            return head;
        }
        let child = keymap::get_keymap_in_runtime(ctx, &head, false, false)
            .unwrap_or(Value::NIL);
        if !child.is_nil() {
            let prompt = keymap_prompt(ctx, child);
            if !prompt.is_nil() {
                return prompt;
            }
        }
        tail = tail.cons_cdr();
    }
    Value::NIL
}

/// A pane name renders as the popup's title row: GNU sets it as the pane
/// title (push_menu_pane), which the toolkit draws above the pane's items.
fn push_title_row(prompt: Value, out: &mut ResolvedMenu) {
    if prompt.is_nil() {
        return;
    }
    if let Some(label) = prompt.as_runtime_string_owned() {
        push_list_entry(out, MenuItemKind::Label, label);
    }
}

/// Non-selectable rows still occupy an event slot with a nil value, so
/// entries and events keep one index space.
fn push_list_entry(out: &mut ResolvedMenu, kind: MenuItemKind, label: String) {
    out.entries.push(PopupMenuEntry {
        kind,
        label,
        help: None,
        shortcut: String::new(),
        depth: 0,
    });
    out.events.push(Value::NIL);
}

fn append(
    ctx: &mut Context,
    map: Value,
    is_tty: bool,
    traversal: &mut MenuTraversal,
    out: &mut ResolvedMenu,
) -> Result<(), Flow> {
    const MAX_DEPTH: usize = 64;
    const MAX_ITEMS: usize = 65_536;
    if !traversal.ancestors.insert(map) {
        return Err(MenuBuildError::Cycle.signal());
    }
    if traversal.path.len() >= MAX_DEPTH {
        return Err(MenuBuildError::TooDeep.signal());
    }
    let original_map = map;
    // GNU map_keymap_canonical delegates merging to the Lisp definition in
    // subr.el. In particular, duplicate submenu bindings compose their maps;
    // dropping the parent's duplicate would lose unrelated child commands.
    let map = if ctx
        .obarray()
        .symbol_function("keymap-canonicalize")
        .is_some()
    {
        out._roots
            .keep(ctx.safe_funcall(Value::symbol("keymap-canonicalize"), vec![map])?)
    } else {
        // Pre-bootstrap contexts can still inspect simple structural menus.
        map
    };
    let mut bindings = Vec::new();
    let mut seen = std::collections::HashSet::new();
    keymap::list_keymap_for_each_binding_recursive(
        &map,
        Some(ctx.obarray()),
        |key, def| {
            if seen.insert(key) {
                bindings.push((out._roots.keep(key), out._roots.keep(def)));
            }
        },
        ctx.symbol_mode(),
    );
    for (key, def) in bindings {
        let Some((entry, child)) =
            parse(ctx, def, traversal.path.len() as u32, is_tty, &out._roots)?
        else {
            continue;
        };
        if out.entries.len() >= MAX_ITEMS {
            return Err(MenuBuildError::TooManyItems.signal());
        }
        let child = child.filter(|_| entry.enabled());
        traversal.path.push(key);
        out.events
            .push(out._roots.keep(Value::list(traversal.path.clone())));
        out.entries.push(entry);
        if !is_tty && let Some(child) = child {
            append(ctx, child, is_tty, traversal, out)?;
        }
        traversal.path.pop();
    }
    traversal.ancestors.remove(&original_map);
    Ok(())
}
