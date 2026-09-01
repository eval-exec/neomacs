//! GUI menu-bar and tool-bar item collection.
//!
//! Mirrors the existing TTY menu-bar walk, but produces GUI overlay
//! payloads for the render thread from the active Lisp keymaps.

use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use neomacs_display_protocol::frame_chrome::{
    BandRect, ChromeAction, CompactBarContent, MenuBarContent, PositionedChromeItem, ToolBarContent,
};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::{MenuBarItem, ToolBarImageSource, ToolBarItem, ToolBarItemType};
use neovm_core::emacs_core::image::{ImageSpecKey, ImageType};
use neovm_core::emacs_core::intern::intern;
use neovm_core::emacs_core::keymap::{
    KeymapMarker, MenuButtonKind, MenuItemProperty, list_keymap_for_each_binding,
};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::window::FrameId;
use strum::{EnumString, IntoStaticStr};

use crate::tty_menu_bar::{collect_tty_menu_bar_items, collect_tty_menu_bar_items_for_frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ToolBarIconTheme {
    Gnu,
    Neomacs,
    VscodeLike,
    JetbrainsLike,
    AtomLike,
    Material,
}

impl ToolBarIconTheme {
    fn directory(self) -> Option<&'static str> {
        match self {
            Self::Gnu => None,
            theme => Some(theme.into()),
        }
    }
}

pub fn compact_bar_mode_enabled(eval: &Context) -> bool {
    eval.obarray()
        .symbol_value("compact-bar-mode")
        .copied()
        .unwrap_or(Value::NIL)
        .is_truthy()
}

pub fn collect_gui_menu_bar_items(eval: &Context) -> Vec<MenuBarItem> {
    collect_tty_menu_bar_items(eval)
        .into_iter()
        .enumerate()
        .map(|(index, item)| MenuBarItem {
            index: index as u32,
            label: item.label,
            key: item.key,
        })
        .collect()
}

pub fn collect_gui_menu_bar_items_for_frame(eval: &Context, frame_id: FrameId) -> Vec<MenuBarItem> {
    collect_tty_menu_bar_items_for_frame(eval, frame_id)
        .into_iter()
        .enumerate()
        .map(|(index, item)| MenuBarItem {
            index: index as u32,
            label: item.label,
            key: item.key,
        })
        .collect()
}

pub fn collect_gui_tool_bar_items(eval: &mut Context) -> Vec<ToolBarItem> {
    let raw_map = current_tool_bar_map(eval);
    let Some(keymap) = current_tool_bar_keymap(eval, &raw_map) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    // `eval` is borrowed mutably inside the closure, so the obarray cannot also
    // be borrowed across it; a tool-bar keymap is built by `define-key` and never
    // has a symbol spine tail for `get_keymap` to resolve.
    list_keymap_for_each_binding(&keymap, None, |key, def| {
        let key_name = key_symbol_name(&key);
        let def = normalize_binding_def(&def);
        let Some(item) = parse_tool_bar_item(eval, &key_name, &def, items.len() as u32) else {
            return;
        };
        items.push(item);
    });
    items
}

/// Collect the tool bar as `frame_id`'s selected window and buffer.
///
/// GNU `update_tool_bar` temporarily selects exactly this frame context before
/// evaluating buffer-local maps and menu-item forms. The shared evaluator
/// scope also guarantees that the caller's selection is restored afterward.
pub fn collect_gui_tool_bar_items_for_frame(
    eval: &mut Context,
    frame_id: FrameId,
) -> Vec<ToolBarItem> {
    eval.with_frame_display_context(frame_id, collect_gui_tool_bar_items)
        .unwrap_or_default()
}

pub(crate) const GUI_CHROME_HORIZONTAL_PADDING: f32 = 8.0;
const TOOL_BAR_SEPARATOR_WIDTH: f32 = 12.0;
const TOOL_BAR_ITEM_SPACING: f32 = 2.0;
const GNU_TOOL_BAR_BASE_HEIGHT: f32 = 34.0;
const GNU_TOOL_BAR_BASE_PADDING: f32 = 5.0;

pub(crate) fn toolbar_visual_config_for_height(height: f32) -> (u32, u32) {
    let height_px = if height.is_finite() && height > 0.0 {
        height.round().max(1.0) as u32
    } else {
        GNU_TOOL_BAR_BASE_HEIGHT as u32
    };
    let scale = (height_px as f32 / GNU_TOOL_BAR_BASE_HEIGHT).max(0.1);
    let max_padding = height_px.saturating_sub(1) / 2;
    let padding = ((GNU_TOOL_BAR_BASE_PADDING * scale).round() as u32).min(max_padding);
    let icon_size = height_px.saturating_sub(padding.saturating_mul(2)).max(1);
    (icon_size, padding)
}

fn fitted_local_bounds(x: f32, width: f32, band_width: f32, band_height: f32) -> Option<BandRect> {
    let remaining = (band_width - x).max(0.0);
    let width = width.min(remaining);
    (width > 0.0).then(|| {
        BandRect::new(x, 0.0, width, band_height)
            .expect("layout-owned chrome dimensions must be finite and nonnegative")
    })
}

fn position_menu_items(
    items: Vec<MenuBarItem>,
    band_width: f32,
    band_height: f32,
    char_width: f32,
    start_x: f32,
    horizontal_padding: f32,
) -> (Vec<PositionedChromeItem<MenuBarItem>>, f32) {
    let mut positioned = Vec::new();
    let mut x = start_x;
    for item in items {
        let width = item.label.chars().count() as f32 * char_width + horizontal_padding * 2.0;
        let Some(bounds) = fitted_local_bounds(x, width, band_width, band_height) else {
            break;
        };
        let action = ChromeAction::OpenMenu {
            index: item.index,
            key: item.key.clone(),
        };
        positioned.push(PositionedChromeItem::new(bounds, item, action));
        x += width;
    }
    (positioned, x)
}

fn position_tool_bar_items(
    items: Vec<ToolBarItem>,
    band_width: f32,
    band_height: f32,
    icon_size: u32,
    padding: u32,
    start_x: f32,
) -> Vec<PositionedChromeItem<ToolBarItem>> {
    let item_width = icon_size as f32 + padding as f32 * 2.0;
    let mut positioned = Vec::new();
    let mut x = start_x;
    for item in items {
        let width = if item.is_separator() {
            TOOL_BAR_SEPARATOR_WIDTH
        } else {
            item_width
        };
        let Some(bounds) = fitted_local_bounds(x, width, band_width, band_height) else {
            break;
        };
        let separator = item.is_separator();
        let index = item.index;
        if separator || !item.enabled {
            positioned.push(PositionedChromeItem::decorative(bounds, item));
            x += if separator {
                TOOL_BAR_SEPARATOR_WIDTH
            } else {
                item_width + TOOL_BAR_ITEM_SPACING
            };
        } else {
            positioned.push(PositionedChromeItem::new(
                bounds,
                item,
                ChromeAction::InvokeToolBarItem { index },
            ));
            x += item_width + TOOL_BAR_ITEM_SPACING;
        }
    }
    positioned
}

pub(crate) fn layout_gui_menu_bar_content(
    items: Vec<MenuBarItem>,
    band_width: f32,
    band_height: f32,
    char_width: f32,
    horizontal_padding: f32,
    foreground: Color,
    background: Color,
) -> MenuBarContent {
    let (items, _) = position_menu_items(
        items,
        band_width,
        band_height,
        char_width,
        horizontal_padding,
        horizontal_padding,
    );
    MenuBarContent::new(items, foreground, background)
}

pub(crate) fn layout_gui_tool_bar_content(
    items: Vec<ToolBarItem>,
    band_width: f32,
    band_height: f32,
    foreground: Color,
    background: Color,
) -> ToolBarContent {
    let (icon_size, padding) = toolbar_visual_config_for_height(band_height);
    let items = position_tool_bar_items(
        items,
        band_width,
        band_height,
        icon_size,
        padding,
        padding as f32,
    );
    ToolBarContent::new(items, foreground, background, icon_size, padding)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_gui_compact_bar_content(
    menu_items: Vec<MenuBarItem>,
    tool_items: Vec<ToolBarItem>,
    band_width: f32,
    band_height: f32,
    char_width: f32,
    menu_foreground: Color,
    menu_background: Color,
    tool_foreground: Color,
    tool_background: Color,
) -> CompactBarContent {
    // The compact bar is a window-system-only affordance; keep the pixel gutter.
    let (menu_items, menu_end_x) = position_menu_items(
        menu_items,
        band_width,
        band_height,
        char_width,
        GUI_CHROME_HORIZONTAL_PADDING,
        GUI_CHROME_HORIZONTAL_PADDING,
    );
    let (icon_size, padding) = toolbar_visual_config_for_height(band_height);
    let tool_items = position_tool_bar_items(
        tool_items,
        band_width,
        band_height,
        icon_size,
        padding,
        menu_end_x + GUI_CHROME_HORIZONTAL_PADDING + padding as f32,
    );
    CompactBarContent::new(
        menu_items,
        tool_items,
        menu_foreground,
        menu_background,
        tool_foreground,
        tool_background,
        icon_size,
        padding,
    )
}

fn normalize_binding_def(def: &Value) -> Value {
    if def.is_cons() && def.cons_cdr().is_nil() {
        return def.cons_car();
    }
    *def
}

fn current_tool_bar_map(eval: &Context) -> Value {
    if let Some(buffer) = eval.buffer_manager().current_buffer()
        && let Some(local) = buffer.buffer_local_value("tool-bar-map")
    {
        return local;
    }
    eval.obarray()
        .default_value_id(intern("tool-bar-map"))
        .copied()
        .unwrap_or(Value::NIL)
}

fn current_tool_bar_keymap(eval: &mut Context, raw_map: &Value) -> Option<Value> {
    if display_images_p(eval)
        && eval.obarray().fboundp("tool-bar-make-keymap")
        && let Ok(value) = eval.eval_form(Value::list(vec![Value::symbol("tool-bar-make-keymap")]))
        && let Some(keymap) = resolve_keymap(eval, &value)
    {
        return Some(keymap);
    }
    resolve_keymap(eval, raw_map)
}

fn display_images_p(eval: &mut Context) -> bool {
    eval.eval_form(Value::list(vec![Value::symbol("display-images-p")]))
        .map(|value| value.is_truthy())
        .unwrap_or(false)
}

fn parse_tool_bar_item(
    eval: &mut Context,
    key_name: &str,
    def: &Value,
    index: u32,
) -> Option<ToolBarItem> {
    if key_name.starts_with("separator") || def.as_symbol_name() == Some("menu-bar-separator") {
        return Some(ToolBarItem {
            index,
            key: key_name.to_string(),
            image: None,
            label: String::new(),
            help: String::new(),
            enabled: false,
            selected: false,
            item_type: ToolBarItemType::Separator,
            wrap: false,
        });
    }

    let (mut label, plist) = extract_menu_item_label_and_plist(eval, def)?;
    if let Some(visible) = plist_lookup(&plist, MenuItemProperty::Visible)
        && !eval_menu_property(eval, visible).is_truthy()
    {
        return None;
    }
    if label.is_empty() {
        label = plist_lookup(&plist, MenuItemProperty::Label)
            .and_then(|value| value.as_runtime_string_owned())
            .unwrap_or_default();
    }
    let image = plist_lookup(&plist, MenuItemProperty::Image)
        .and_then(|image| tool_bar_image_source(eval, &image));
    let help = plist_lookup(&plist, MenuItemProperty::Help)
        .and_then(|value| value.as_runtime_string_owned())
        .unwrap_or_default();
    let enabled = plist_lookup(&plist, MenuItemProperty::Enable)
        .map(|value| eval_menu_property(eval, value).is_truthy())
        .unwrap_or(true);
    let (item_type, selected) = plist_lookup(&plist, MenuItemProperty::Button)
        .and_then(|value| button_state(eval, value))
        .unwrap_or((ToolBarItemType::Button, false));
    let wrap = plist_lookup(&plist, MenuItemProperty::Wrap)
        .map(|value| eval_menu_property(eval, value).is_truthy())
        .unwrap_or(false);

    Some(ToolBarItem {
        index,
        key: key_name.to_string(),
        image,
        label,
        help,
        enabled: if wrap { false } else { enabled },
        selected,
        item_type,
        wrap,
    })
}

fn extract_menu_item_label_and_plist(eval: &mut Context, def: &Value) -> Option<(String, Value)> {
    if !def.is_cons() {
        return None;
    }
    let car = def.cons_car();
    let cdr = def.cons_cdr();

    if KeymapMarker::MenuItem.is_value(car) && cdr.is_cons() {
        let label = menu_caption_string(eval, cdr.cons_car())?;
        let mut rest = cdr.cons_cdr();
        if !rest.is_cons() {
            return None;
        }
        rest = rest.cons_cdr();
        return Some((label, rest));
    }

    let label = menu_caption_string(eval, car)?;
    Some((label, cdr))
}

fn menu_caption_string(eval: &mut Context, value: Value) -> Option<String> {
    if let Some(string) = value.as_runtime_string_owned() {
        return Some(string);
    }
    eval.eval_form(value).ok()?.as_runtime_string_owned()
}

fn eval_menu_property(eval: &mut Context, value: Value) -> Value {
    eval.eval_form(value).unwrap_or(Value::NIL)
}

fn button_state(eval: &mut Context, value: Value) -> Option<(ToolBarItemType, bool)> {
    if !value.is_cons() {
        return None;
    }
    let name = value.cons_car().as_symbol_name()?;
    let item_type = match MenuButtonKind::from_keyword(name)? {
        MenuButtonKind::Radio => ToolBarItemType::Radio,
        MenuButtonKind::Toggle => ToolBarItemType::Toggle,
    };
    let selected = eval_menu_property(eval, value.cons_cdr()).is_truthy();
    Some((item_type, selected))
}

fn plist_lookup(plist: &Value, wanted: MenuItemProperty) -> Option<Value> {
    plist_lookup_by_symbol(plist, |key| wanted.is_value(key))
}

fn plist_lookup_by_symbol(
    plist: &Value,
    mut matches_key: impl FnMut(Value) -> bool,
) -> Option<Value> {
    let mut tail = *plist;
    while tail.is_cons() {
        let key = tail.cons_car();
        tail = tail.cons_cdr();
        if !tail.is_cons() {
            break;
        }
        let value = tail.cons_car();
        if matches_key(key) {
            return Some(value);
        }
        tail = tail.cons_cdr();
    }
    None
}

fn tool_bar_image_source(eval: &Context, value: &Value) -> Option<ToolBarImageSource> {
    image_file_from_spec(value)
        .or_else(|| best_image_file_from_expression(value))
        .map(|path| ToolBarImageSource::File {
            path: themed_tool_bar_image_path(eval, &path),
        })
}

fn image_file_from_spec(value: &Value) -> Option<String> {
    let plist = if value.is_cons() && value.cons_car().as_symbol_name() == Some("image") {
        value.cons_cdr()
    } else {
        *value
    };
    plist_lookup_by_symbol(&plist, |key| ImageSpecKey::File.is_value(key))
        .and_then(|file| file.as_runtime_string_owned())
}

fn best_image_file_from_expression(value: &Value) -> Option<String> {
    let mut files = Vec::new();
    collect_image_file_candidates(value, &mut files);
    files
        .into_iter()
        .min_by_key(|file| toolbar_image_score(file))
}

fn collect_image_file_candidates(value: &Value, files: &mut Vec<String>) {
    if let Some(path) = value.as_runtime_string_owned() {
        if is_supported_toolbar_image_file(&path) {
            files.push(path);
        }
        return;
    }
    if value.is_cons() {
        collect_image_file_candidates(&value.cons_car(), files);
        collect_image_file_candidates(&value.cons_cdr(), files);
    } else if let Some(values) = value.as_vector_data() {
        for item in values.iter() {
            collect_image_file_candidates(item, files);
        }
    }
}

fn is_supported_toolbar_image_file(path: &str) -> bool {
    toolbar_image_type(Path::new(path)).is_some()
}

fn toolbar_image_score(path: &str) -> (u8, u8) {
    let path = Path::new(path);
    let low_color_penalty = path
        .components()
        .any(|component| component.as_os_str() == "low-color") as u8;
    let extension_rank = toolbar_image_type(path)
        .map(toolbar_image_type_rank)
        .unwrap_or(9);
    (low_color_penalty, extension_rank)
}

fn toolbar_image_type(path: &Path) -> Option<ImageType> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(ImageType::from_file_extension)
}

fn toolbar_image_type_rank(image_type: ImageType) -> u8 {
    match image_type {
        ImageType::Xpm => 0,
        ImageType::Pbm => 1,
        ImageType::Xbm => 2,
        ImageType::Png => 3,
        ImageType::Svg => 4,
        ImageType::Gif => 5,
        ImageType::Jpeg => 6,
        ImageType::Tiff => 7,
        ImageType::Webp => 8,
    }
}

fn themed_tool_bar_image_path(eval: &Context, path: &str) -> String {
    let original_path = resolve_tool_bar_image_path(path);
    let Some(icon_name) =
        tool_bar_icon_name_from_path(path).or_else(|| tool_bar_icon_name_from_path(&original_path))
    else {
        return original_path;
    };

    if let Some(path) = custom_tool_bar_icon_path(eval, &icon_name) {
        return path;
    }

    let theme = current_tool_bar_icon_theme(eval);
    if theme == ToolBarIconTheme::Gnu {
        return original_path;
    }

    themed_tool_bar_icon_path(theme, &icon_name)
        .or_else(|| themed_tool_bar_icon_path(ToolBarIconTheme::Neomacs, &icon_name))
        .unwrap_or(original_path)
}

fn current_tool_bar_icon_theme(eval: &Context) -> ToolBarIconTheme {
    eval.obarray()
        .symbol_value("neomacs-toolbar-icon-theme")
        .and_then(|value| value.as_symbol_name())
        .and_then(|name| ToolBarIconTheme::from_str(name).ok())
        .unwrap_or(ToolBarIconTheme::VscodeLike)
}

fn custom_tool_bar_icon_path(eval: &Context, icon_name: &str) -> Option<String> {
    let directory = eval
        .obarray()
        .symbol_value("neomacs-toolbar-icon-directory")
        .and_then(|value| value.as_runtime_string_owned())?;
    let directory = directory.trim();
    if directory.is_empty() {
        return None;
    }
    let path = Path::new(directory).join(format!("{icon_name}.svg"));
    if path.exists() {
        return Some(path.to_string_lossy().into_owned());
    }
    None
}

fn themed_tool_bar_icon_path(theme: ToolBarIconTheme, icon_name: &str) -> Option<String> {
    let directory = theme.directory()?;
    let relative = Path::new(directory).join(format!("{icon_name}.svg"));
    resolve_tool_bar_icon_theme_path(&relative)
}

fn tool_bar_icon_name_from_path(path: &str) -> Option<String> {
    let mut parts = normal_path_components(path);
    if parts.is_empty() {
        return None;
    }

    if let Some(pos) = parts
        .windows(2)
        .position(|window| window[0] == "etc" && window[1] == "images")
    {
        parts.drain(..pos + 2);
    }
    if parts.first().is_some_and(|part| part == "low-color") {
        parts.remove(0);
    }

    let last = parts.pop()?;
    let stem = Path::new(&last).file_stem()?.to_str()?.to_string();
    if stem.is_empty() {
        return None;
    }
    parts.push(stem);
    Some(parts.join("/"))
}

fn normal_path_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string),
            _ => None,
        })
        .collect()
}

fn resolve_tool_bar_image_path(path: &str) -> String {
    let original = Path::new(path);
    if original.is_absolute() && original.exists() {
        return original.to_string_lossy().into_owned();
    }

    for candidate in tool_bar_image_load_candidates(original) {
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }

    path.to_string()
}

fn resolve_tool_bar_icon_theme_path(relative_path: &Path) -> Option<String> {
    tool_bar_icon_theme_load_candidates(relative_path)
        .into_iter()
        .find(|candidate| candidate.exists())
        .map(|path| path.to_string_lossy().into_owned())
}

fn tool_bar_icon_theme_load_candidates(relative_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("etc/toolbar-icons").join(relative_path));
    }
    candidates.push(
        Path::new(env!("CARGO_WORKSPACE_DIR"))
            .join("etc/toolbar-icons")
            .join(relative_path),
    );
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        candidates.push(exe_dir.join("etc/toolbar-icons").join(relative_path));
        candidates.push(
            exe_dir
                .join("../share/neomacs/etc/toolbar-icons")
                .join(relative_path),
        );
        candidates.push(
            exe_dir
                .join("../Resources/etc/toolbar-icons")
                .join(relative_path),
        );
    }
    candidates
}

fn tool_bar_image_load_candidates(path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if path.is_absolute() {
        candidates.push(path.to_path_buf());
        return candidates;
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(path));
        candidates.push(cwd.join("etc/images").join(path));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        candidates.push(exe_dir.join(path));
        candidates.push(exe_dir.join("etc/images").join(path));
        candidates.push(exe_dir.join("../share/neomacs/etc/images").join(path));
        candidates.push(exe_dir.join("../Resources/etc/images").join(path));
    }
    candidates
}

fn resolve_keymap(eval: &Context, value: &Value) -> Option<Value> {
    if is_keymap(value) {
        return Some(*value);
    }
    if let Some(name) = value.as_symbol_name()
        && let Some(symbol_value) = eval.obarray().symbol_value(name)
        && is_keymap(symbol_value)
    {
        return Some(*symbol_value);
    }
    None
}

fn is_keymap(value: &Value) -> bool {
    value.is_cons() && KeymapMarker::Keymap.is_value(value.cons_car())
}

fn key_symbol_name(key: &Value) -> String {
    key.as_symbol_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{key:?}"))
}

#[cfg(test)]
#[path = "gui_chrome_test.rs"]
mod tests;
