//! Evaluator-owned menu semantics. Presentation contains no Lisp values.

use neomacs_display_protocol::menu::{MenuAvailability, MenuIndicator, MenuItemKind};
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

/// Decode every MENU shape GNU's `x_popup_menu_1` accepts
/// (`src/menu.c:1294-1364`): a keymap, a list of keymaps
/// (`keymap_panes`), or an old-fashioned `(TITLE PANE...)` list
/// (`list_of_panes`).
///
/// The interactive `x-popup-menu` path used to resolve only a single
/// keymap and send the other two shapes to the batch validator, which
/// displayed nothing and returned nil.  GNU displays all three forms;
/// only the returned value differs: a list of event paths for keymaps,
/// the chosen item's VALUE for the old-fashioned form.
pub(crate) fn resolve_popup(
    ctx: &mut Context,
    menu: Value,
    is_tty: bool,
) -> Result<ResolvedMenu, Flow> {
    let mut result = ResolvedMenu {
        entries: Vec::new(),
        events: Vec::new(),
        _roots: MenuRoots::new(),
    };
    result._roots.keep(menu);
    // `keymap = get_keymap (menu, 0, 0); if (CONSP (keymap))`: the branch
    // needs a real keymap OBJECT.  `get_keymap` returns an unloaded
    // keymap autoload symbol itself, which GNU does not take here.
    let map = keymap::get_keymap_in_runtime(ctx, &menu, false, false)?;
    if keymap::is_list_keymap(&map, ctx.symbol_mode()) {
        result._roots.keep(map);
        return finish_keymap_pane(ctx, map, is_tty, result);
    }
    // `else if (CONSP (menu) && KEYMAPP (XCAR (menu)))`: the first
    // element must itself be a keymap object; the remaining elements are
    // resolved with the erroring `get_keymap` inside the branch.
    if menu.is_cons() && keymap::is_list_keymap(&menu.cons_car(), ctx.symbol_mode()) {
        return resolve_keymap_list(ctx, menu, is_tty, result);
    }
    // `else { title = Fcar (menu); CHECK_STRING (title); list_of_panes
    // (Fcdr (menu)); }`
    resolve_old_fashioned(menu, result)
}

/// GNU's single-keymap branch: `keymap_panes (&menu, 1)` makes one pane
/// whose name is `(Fkeymap_prompt keymap)`.
fn finish_keymap_pane(
    ctx: &mut Context,
    map: Value,
    is_tty: bool,
    mut out: ResolvedMenu,
) -> Result<ResolvedMenu, Flow> {
    let prompt = keymap_prompt(ctx, map);
    out._roots.keep(prompt);
    push_label_row(prompt, &mut out);
    append(ctx, map, is_tty, &mut MenuTraversal::default(), &mut out)?;
    Ok(out)
}

/// GNU's list-of-keymaps branch (`src/menu.c:1322-1350`): every element is
/// resolved with `get_keymap (XCAR (tem), 1, 0)`, so a non-keymap element
/// signals `keymapp`; the first prompt found is the menu title, and that
/// title becomes the FIRST pane's name (`ASET (menu_items,
/// MENU_ITEMS_PANE_NAME, title)`).
fn resolve_keymap_list(
    ctx: &mut Context,
    menu: Value,
    is_tty: bool,
    mut out: ResolvedMenu,
) -> Result<ResolvedMenu, Flow> {
    let mut maps = Vec::new();
    let mut title = Value::NIL;
    let mut rest = menu;
    while rest.is_cons() {
        let map = keymap::get_keymap_in_runtime(ctx, &rest.cons_car(), true, false)?;
        let map = out._roots.keep(map);
        if title.is_nil() {
            let prompt = keymap_prompt(ctx, map);
            if !prompt.is_nil() {
                title = out._roots.keep(prompt);
            }
        }
        maps.push(map);
        rest = rest.cons_cdr();
    }
    for (index, map) in maps.iter().enumerate() {
        let pane_name = if index == 0 && !title.is_nil() {
            title
        } else {
            keymap_prompt(ctx, *map)
        };
        out._roots.keep(pane_name);
        push_label_row(pane_name, &mut out);
        append(ctx, *map, is_tty, &mut MenuTraversal::default(), &mut out)?;
    }
    Ok(out)
}

/// GNU's old-fashioned branch (`src/menu.c:1354-1364` -> `list_of_panes`
/// :496-515 -> `list_of_items` :459-484):
///
/// * TITLE is `(Fcar menu)` and must be a string.
/// * Each PANE is `(PANE-TITLE . PANE-DATA)`; PANE-TITLE must be a string
///   and PANE-DATA a cons (an empty pane is `wrong-type-argument consp`).
/// * An item that is a string is a non-selectable line.
/// * An item that is a cons is `(LABEL . VALUE)`; LABEL must be a string
///   and the popup returns VALUE.
/// * Any other item is a left/right boundary, drawn as a separator.
fn resolve_old_fashioned(menu: Value, mut out: ResolvedMenu) -> Result<ResolvedMenu, Flow> {
    let title = car_or_nil(menu);
    if !title.is_string() {
        return Err(wrong_type_string(title));
    }
    out._roots.keep(title);
    push_label_row(title, &mut out);
    let mut panes = cdr_or_nil(menu);
    while panes.is_cons() {
        let pane = panes.cons_car();
        let pane_name = car_or_nil(pane);
        if !pane_name.is_string() {
            return Err(wrong_type_string(pane_name));
        }
        out._roots.keep(pane_name);
        push_label_row(pane_name, &mut out);
        let pane_data = cdr_or_nil(pane);
        if !pane_data.is_cons() {
            return Err(neovm_value::error::signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("consp"), pane_data],
            ));
        }
        out._roots.keep(pane_data);
        let mut items = pane_data;
        while items.is_cons() {
            let item = items.cons_car();
            if item.is_string() {
                push_label_row(item, &mut out);
            } else if item.is_cons() {
                let label = item.cons_car();
                if !label.is_string() {
                    return Err(wrong_type_string(label));
                }
                let value = out._roots.keep(item.cons_cdr());
                if let Some(label) = label.as_runtime_string_owned() {
                    push_event_entry(
                        &mut out,
                        MenuItemKind::Command {
                            availability: MenuAvailability::Enabled,
                            indicator: MenuIndicator::None,
                        },
                        label,
                        value,
                    );
                }
            } else {
                push_event_entry(&mut out, MenuItemKind::Separator, String::new(), Value::NIL);
            }
            items = items.cons_cdr();
        }
        panes = panes.cons_cdr();
    }
    Ok(out)
}

/// GNU `Fcar`/`Fcdr` return nil for a non-cons argument rather than
/// signalling; the old-fashioned decode depends on that to put the
/// `stringp`/`consp` error on the actual offending value.
fn car_or_nil(value: Value) -> Value {
    if value.is_cons() {
        value.cons_car()
    } else {
        Value::NIL
    }
}

fn cdr_or_nil(value: Value) -> Value {
    if value.is_cons() {
        value.cons_cdr()
    } else {
        Value::NIL
    }
}

fn wrong_type_string(value: Value) -> Flow {
    neovm_value::error::signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), value],
    )
}

/// GNU Fkeymap_prompt (src/keymap.c:148-168): `get_keymap` first, then
/// the first direct string element of the keymap spine, looking through
/// nested keymap elements (only a real keymap OBJECT recurses).
fn keymap_prompt(ctx: &mut Context, map: Value) -> Value {
    let mut map = keymap::get_keymap_in_runtime(ctx, &map, false, false).unwrap_or(Value::NIL);
    while map.is_cons() {
        let head = map.cons_car();
        if head.is_string() {
            return head;
        }
        if keymap::is_list_keymap(&head, ctx.symbol_mode()) {
            let prompt = keymap_prompt(ctx, head);
            if !prompt.is_nil() {
                return prompt;
            }
        }
        map = map.cons_cdr();
    }
    Value::NIL
}

/// A pane name or non-selectable item renders as a label row.  GNU sets a
/// pane name as the pane title (`push_menu_pane`), which the toolkit draws
/// above the pane's items; a string item in an old-fashioned menu becomes
/// a disabled menu item line (`list_of_items`).
fn push_label_row(value: Value, out: &mut ResolvedMenu) {
    if value.is_nil() {
        return;
    }
    if let Some(label) = value.as_runtime_string_owned() {
        push_list_entry(out, MenuItemKind::Label, label);
    }
}

/// Non-selectable rows still occupy an event slot with a nil value, so
/// entries and events keep one index space.
fn push_list_entry(out: &mut ResolvedMenu, kind: MenuItemKind, label: String) {
    push_event_entry(out, kind, label, Value::NIL);
}

/// Selectable rows carry the value the popup returns when that row is
/// chosen: an event path for keymap panes, the item's VALUE for the
/// old-fashioned list form (`list_of_items` pushes `XCDR (item)` as the
/// item key).
fn push_event_entry(out: &mut ResolvedMenu, kind: MenuItemKind, label: String, event: Value) {
    out.entries.push(PopupMenuEntry {
        kind,
        label,
        help: None,
        shortcut: String::new(),
        depth: 0,
    });
    out.events.push(event);
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
