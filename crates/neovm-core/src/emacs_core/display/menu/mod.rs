//! Evaluator-owned menu semantics. Presentation contains no Lisp values.

use super::error::{Flow, LispCondition};
use super::keymap;
use super::value::Value;
use super::{Context, PopupMenuEntry};
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
        super::error::signal(LispCondition::Error, vec![Value::string(self.to_string())])
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
            base: super::eval::save_scratch_gc_roots(),
            _thread: std::marker::PhantomData,
        }
    }
    fn keep(&self, value: Value) -> Value {
        super::eval::push_scratch_gc_root(value);
        value
    }
}

impl Drop for MenuRoots {
    fn drop(&mut self) {
        super::eval::restore_scratch_gc_roots(self.base);
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
    append(ctx, map, is_tty, &mut MenuTraversal::default(), &mut result)?;
    Ok(result)
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
    keymap::list_keymap_for_each_binding_recursive(&map, Some(ctx.obarray()), |key, def| {
        if seen.insert(key) {
            bindings.push((out._roots.keep(key), out._roots.keep(def)));
        }
    });
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
