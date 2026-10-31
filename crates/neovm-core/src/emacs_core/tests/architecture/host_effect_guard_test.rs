//! Option 1b fences native host effects, independently of Lisp dispatch shape.
//!
//! This is a source boundary guard, not Rust name resolution or a proof of
//! dataflow through inferred types. Rust-private backing fields and opaque
//! transport types supply the type boundary; this scan prevents new explicit
//! native imports, exports, TLS access and raw sender escape outside the small
//! host-owner family. Owned protocol data and projection traits remain public.
//! Concrete connected transports are also effect capabilities, even when their
//! raw backing is private. Their declared types and producer paths are fenced;
//! arbitrary inferred values or new capability-producing APIs require review.
//! No opcode, JIT intrinsic or canonical SubrObj is forbidden by this guard.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, GenericArgument, Item, Lit, Meta, PathArguments, Type, UseTree, Visibility,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum NativeOwner {
    Platform,
    Font,
    Image,
    Selection,
    Terminal,
    Layout,
    CommandTransport,
}

/// Unlike raw backing, an owned capability may cross a public factory boundary.
/// Its use still belongs to a concrete native adapter or lifecycle owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectCapability {
    RenderCommands,
    NativeEventLoop,
    NativeWake,
    ConnectedComms,
}

fn effect_capability_path(words: &[String]) -> Option<EffectCapability> {
    for word in words {
        match word.as_str() {
            "RenderCommandSender" => return Some(EffectCapability::RenderCommands),
            "RenderEventLoop"
            | "build_render_event_loop"
            | "build_render_event_loop_any_thread" => {
                return Some(EffectCapability::NativeEventLoop);
            }
            "RenderEventLoopProxy" => return Some(EffectCapability::NativeWake),
            // These public aggregates carry connected channels. Checking only
            // RenderCommandSender misses `comms.cmd_tx.send(...)` inference.
            "EmacsComms" | "RenderComms" | "ThreadComms" => {
                return Some(EffectCapability::ConnectedComms);
            }
            _ => {}
        }
    }
    None
}

fn capability_owner_allows(path: &str, capability: EffectCapability) -> bool {
    let display = "neomacs-display-runtime/src/";
    let render_kernel = path.starts_with(&format!("{display}render_thread/"));
    match capability {
        EffectCapability::RenderCommands => {
            render_kernel
                || path == format!("{display}thread_comm.rs")
                || path == format!("{display}thread_comm/command_projection.rs")
                || path == "neomacs/src/image_catalog.rs"
                || path == "neomacs/src/tty_frontend.rs"
        }
        EffectCapability::NativeEventLoop | EffectCapability::NativeWake => render_kernel,
        EffectCapability::ConnectedComms => {
            render_kernel
                || path == format!("{display}thread_comm.rs")
                || path == format!("{display}tty_input.rs")
                || path.starts_with(&format!("{display}tty_input/"))
        }
    }
}

/// Exact native owner modules, relative to `crates/`. This is deliberately not
/// an allowlist of whole crates. A new semantic caller is not a native owner.
fn owner_allows(path: &str, owner: NativeOwner) -> bool {
    let display = "neomacs-display-runtime/src/";
    let frontend = "neomacs/src/";
    let layout = "neomacs-layout-engine/src/";
    let render_kernel = path.starts_with(&format!("{display}render_thread/"));
    match owner {
        NativeOwner::Platform => {
            render_kernel
                || [
                    "menus/",
                    "presentation/",
                    "presentation_feedback/",
                    "window_chrome/",
                    "gui_resources/",
                    "font_defaults/",
                ]
                .iter()
                .any(|module| path.starts_with(&format!("{display}{module}")))
                || [
                    "tooltips.rs",
                    "window_icon.rs",
                    "window_identity.rs",
                    "display_identity.rs",
                    "display_scale.rs",
                    "wayland_toplevel_icon.rs",
                    "macos_bundle_runtime.rs",
                ]
                .iter()
                .any(|file| path == format!("{display}{file}"))
                || path.starts_with("neomacs-layout-engine/src/font_backend/")
                || path.starts_with("neomacs-webview/src/platform/")
                || path == "neomacs-webview/src/model.rs"
                || path.starts_with("neomacs-video/src/platform/")
                || path == "neomacs-renderer-wgpu/src/glyph_atlas/macos.rs"
        }
        NativeOwner::Font => {
            path.starts_with(&format!("{layout}font/"))
                || path.starts_with(&format!("{layout}font_backend/"))
                || path == format!("{layout}font_backend.rs")
                || [
                    "engine.rs",
                    "engine/position_chrome.rs",
                    "engine/prepared_viewports/export.rs",
                    "display_status_line.rs",
                    "text_shaper.rs",
                    "row_layout/font_measurement.rs",
                    "row_layout/program.rs",
                    "buffer_source/render_plan.rs",
                    "buffer_source/render_attempt.rs",
                    "buffer_source/body_render.rs",
                    "display_face_layout.rs",
                    "display_source_resolver.rs",
                    "neovm_bridge.rs",
                    "display_mock_frame.rs",
                    "gui_chrome.rs",
                    "display_source_append_plan.rs",
                    "display_row/mod.rs",
                    "display_row/source_render.rs",
                    "display_row/append_context.rs",
                    "display_row/face_state.rs",
                    "display_row/replacement.rs",
                    "display_row/render_policy.rs",
                ]
                .iter()
                .any(|file| path == format!("{layout}{file}"))
                || ["lib.rs", "fontdb.rs", "color_glyph.rs", "raster_sources.rs"]
                    .iter()
                    .any(|file| path == format!("neomacs-font-materializer/src/{file}"))
                || path.starts_with("neomacs-renderer-wgpu/src/glyph_atlas/")
                || path.starts_with("neomacs-renderer-wgpu/src/renderer/")
                || path == "neomacs-renderer-wgpu/src/svg.rs"
                || path == format!("{display}redisplay.rs")
                || path.starts_with(&format!("{display}menus/"))
                || path == format!("{frontend}startup_font.rs")
                || path == format!("{frontend}frame_layout.rs")
                || path == format!("{frontend}main.rs")
        }
        NativeOwner::Image => {
            path == format!("{frontend}image_catalog.rs")
                || path == format!("{frontend}main.rs")
                || render_kernel
        }
        NativeOwner::Selection => {
            path == format!("{display}clipboard.rs")
                || path.starts_with(&format!("{display}clipboard/"))
                || render_kernel
        }
        NativeOwner::Terminal => {
            // Both managers own terminal effects: core's manager retains native
            // TTY hosts, while the render kernel's manager retains PTY-backed
            // emulator views. Their shared name does not make either semantic
            // caller an owner. Only these exact owner families may borrow them.
            path == "neovm-core/src/emacs_core/display/terminal/pure.rs"
                || path == format!("{frontend}tty_frontend.rs")
                || path == format!("{frontend}secondary_tty.rs")
                || path == format!("{display}terminal/mod.rs")
                || path.starts_with(&format!("{display}terminal/"))
                || render_kernel
        }
        NativeOwner::Layout => {
            path == format!("{frontend}frame_layout.rs")
                || path == format!("{frontend}main.rs")
                || path == format!("{display}redisplay.rs")
                || path.starts_with(&format!("{display}redisplay/"))
        }
        NativeOwner::CommandTransport => {
            path == format!("{display}thread_comm/command_projection.rs")
        }
    }
}

fn test_only(attributes: &[Attribute]) -> bool {
    fn requires_test(meta: &Meta) -> bool {
        match meta {
            Meta::Path(path) => path.is_ident("test"),
            Meta::List(list) => {
                let Ok(args) = list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                ) else {
                    return false;
                };
                if list.path.is_ident("all") {
                    args.iter().any(requires_test)
                } else if list.path.is_ident("any") {
                    !args.is_empty() && args.iter().all(requires_test)
                } else {
                    false
                }
            }
            Meta::NameValue(_) => false,
        }
    }
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| requires_test(&meta)))
    })
}

fn attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn path_words(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect()
}

fn native_path(words: &[String]) -> Option<NativeOwner> {
    let last = words.last()?.as_str();
    if words.first().is_some_and(|word| word == "winit")
        && ((words.iter().any(|word| word == "event_loop")
            && matches!(last, "EventLoop" | "EventLoopProxy" | "ActiveEventLoop"))
            || (words.iter().any(|word| word == "window") && last == "Window"))
    {
        return Some(NativeOwner::Platform);
    }
    if words.first().is_some_and(|word| {
        matches!(
            word.as_str(),
            "objc2_app_kit" | "app_kit" | "cocoa" | "core_text"
        )
    }) || matches!(last, "NSWindow" | "NSApplication" | "CTFont" | "CTFontRef")
    {
        return Some(NativeOwner::Platform);
    }
    // FontMetricsService and the platform backend structs are opaque facades,
    // not raw handles. Fence their actual database/FFI backing instead.
    if (words.first().is_some_and(|word| word == "fontdb") && last == "Database")
        || (words.first().is_some_and(|word| word == "cosmic_text") && last == "FontSystem")
        || (words.first().is_some_and(|word| word == "freetype")
            && matches!(last, "Library" | "Face" | "Glyph" | "Stroker"))
        || last.starts_with("FcConfig")
        || last.starts_with("FcPattern")
        || matches!(last, "FT_Face" | "FT_Library")
    {
        return Some(NativeOwner::Font);
    }
    match last {
        "AsyncImageCatalog" => Some(NativeOwner::Image),
        "ClipboardService" => Some(NativeOwner::Selection),
        "Clipboard"
            if words.iter().any(|word| {
                matches!(word.as_str(), "arboard" | "smithay_clipboard" | "copypasta")
            }) =>
        {
            Some(NativeOwner::Selection)
        }
        "TERMINAL_MANAGER" | "TerminalManager" | "TerminalRecord" | "TerminalView"
        | "NeomacsEventProxy" => Some(NativeOwner::Terminal),
        "REDISPLAY_RUNTIME" | "RedisplayRuntime" => Some(NativeOwner::Layout),
        _ => None,
    }
}

/// `RenderCommand` is owned protocol data. The channel carrying it is a native
/// capability and may only be held by the private opaque transport wrapper.
fn raw_command_sender(path: &syn::Path) -> bool {
    let Some(last) = path.segments.last() else {
        return false;
    };
    if last.ident != "Sender" {
        return false;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    args.args.iter().any(|arg| {
        matches!(arg, GenericArgument::Type(Type::Path(value))
            if value.path.segments.last().is_some_and(|part| part.ident == "RenderCommand"))
    })
}

#[derive(Default)]
struct TypeNatives {
    owners: BTreeSet<NativeOwner>,
    raw_sender: bool,
}
impl<'ast> Visit<'ast> for TypeNatives {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(owner) = native_path(&path_words(path)) {
            self.owners.insert(owner);
        }
        self.raw_sender |= raw_command_sender(path);
        visit::visit_path(self, path);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViolationKind {
    OutsideNativeOwner,
    OutsideEffectOwner,
    PublicNativeReexport,
    NativeGlobImport,
    RawCommandSender,
    PublicNativeField,
    PublicNativeAlias,
    PublicNativeStatic,
    PublicNativeResult,
    PublicNativeDeref,
    PublicNativeVariant,
    UnparsedNativeTls,
}
impl std::fmt::Display for ViolationKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::OutsideNativeOwner => "outside-native-owner",
            Self::OutsideEffectOwner => "outside-effect-owner",
            Self::PublicNativeReexport => "public-native-reexport",
            Self::NativeGlobImport => "native-glob-import",
            Self::RawCommandSender => "raw-command-sender",
            Self::PublicNativeField => "public-native-field",
            Self::PublicNativeAlias => "public-native-alias",
            Self::PublicNativeStatic => "public-native-static",
            Self::PublicNativeResult => "public-native-result",
            Self::PublicNativeDeref => "public-native-deref",
            Self::PublicNativeVariant => "public-native-variant",
            Self::UnparsedNativeTls => "unparsed-native-tls",
        })
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum ExportScope {
    Private,
    Public,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum ScanPosition {
    Item,
    Import,
}

#[derive(Debug, Eq, PartialEq)]
struct Violation {
    kind: ViolationKind,
    detail: String,
}

struct Guard<'a> {
    file: &'a str,
    imported: BTreeMap<String, Vec<String>>,
    context: Option<String>,
    inline_depth: usize,
    position: ScanPosition,
    public_types: BTreeSet<String>,
    impl_scope: ExportScope,
    violations: Vec<Violation>,
}
impl<'a> Guard<'a> {
    fn new(file: &'a str) -> Self {
        Self {
            file,
            imported: BTreeMap::new(),
            context: None,
            inline_depth: 0,
            position: ScanPosition::Item,
            public_types: BTreeSet::new(),
            impl_scope: ExportScope::Private,
            violations: Vec::new(),
        }
    }

    fn reject(&mut self, kind: ViolationKind, detail: impl Into<String>) {
        self.violations.push(Violation {
            kind,
            detail: detail.into(),
        });
    }

    fn check_native(&mut self, owner: NativeOwner, detail: String) {
        // main.rs is a mixed startup/semantic module, not a blanket native
        // owner. Only these existing native adapter/attach items may touch
        // backing; importing its opaque type does not grant a new helper access.
        let main_item = self.file == "neomacs/src/main.rs"
            && self.inline_depth == 0
            && (self.position == ScanPosition::Import
                || self.context.as_deref().is_some_and(|name| {
                    matches!(
                        name,
                        "PrimaryWindowDisplayHost"
                            | "run_gui_evaluator_worker"
                            | "GuiEventLoopWaker"
                            | "run_gui_main_thread"
                            | "spawn_gui_evaluator_worker"
                            | "publish_gui_frame"
                    )
                }));
        let allowed = if self.file == "neomacs/src/main.rs" {
            main_item
        } else {
            owner_allows(self.file, owner)
        };
        if !allowed {
            self.reject(ViolationKind::OutsideNativeOwner, detail);
        }
    }

    fn resolved_words(&self, words: &[String]) -> Vec<String> {
        resolve_words(&self.imported, words)
    }

    fn check_capability(&mut self, capability: EffectCapability, detail: String) {
        // Source ownership anchors: main.rs::run owns initial GUI loop creation,
        // TTY ThreadComms attachment and lifecycle shutdown (currently 4511,
        // 4670, 4794, 4977). The named GUI functions attach/publish native
        // adapters; PrimaryWindowDisplayHost and GuiEventLoopWaker retain their
        // connected transports. This grants no other main.rs item ownership and
        // does not widen the separate raw-backing/export permission.
        let allowed = if self.file == "neomacs/src/main.rs" {
            self.inline_depth == 0
                && (self.position == ScanPosition::Import
                    || self.context.as_deref().is_some_and(|name| {
                        matches!(
                            name,
                            "run"
                                | "PrimaryWindowDisplayHost"
                                | "GuiEventLoopWaker"
                                | "run_gui_main_thread"
                                | "spawn_gui_evaluator_worker"
                                | "run_gui_evaluator_worker"
                                | "publish_gui_frame"
                        )
                    }))
        } else {
            capability_owner_allows(self.file, capability)
        };
        if !allowed {
            self.reject(ViolationKind::OutsideEffectOwner, detail);
        }
    }

    fn type_is_native(&self, ty: &Type) -> bool {
        struct ImportedType<'a> {
            aliases: &'a BTreeMap<String, Vec<String>>,
            found: bool,
        }
        impl<'ast> Visit<'ast> for ImportedType<'_> {
            fn visit_path(&mut self, path: &'ast syn::Path) {
                self.found |=
                    native_path(&resolve_words(self.aliases, &path_words(path))).is_some();
                visit::visit_path(self, path);
            }
        }
        let mut direct = TypeNatives::default();
        direct.visit_type(ty);
        let mut imported = ImportedType {
            aliases: &self.imported,
            found: false,
        };
        imported.visit_type(ty);
        !direct.owners.is_empty() || direct.raw_sender || imported.found
    }

    fn imports(&mut self, prefix: &mut Vec<String>, tree: &UseTree, exported: bool) {
        match tree {
            UseTree::Path(item) => {
                prefix.push(item.ident.to_string());
                self.imports(prefix, &item.tree, exported);
                prefix.pop();
            }
            UseTree::Name(item) => {
                prefix.push(item.ident.to_string());
                self.imported.insert(item.ident.to_string(), prefix.clone());
                if let Some(capability) = effect_capability_path(&self.resolved_words(prefix)) {
                    self.check_capability(
                        capability,
                        format!("capability import {}", prefix.join("::")),
                    );
                }
                if exported
                    && prefix.first().is_some_and(|root| root == "winit")
                    && prefix
                        .iter()
                        .any(|part| matches!(part.as_str(), "window" | "event_loop"))
                {
                    self.reject(ViolationKind::PublicNativeReexport, prefix.join("::"));
                }
                if let Some(owner) = native_path(prefix) {
                    self.check_native(owner, format!("native import {}", prefix.join("::")));
                    if exported {
                        self.reject(ViolationKind::PublicNativeReexport, prefix.join("::"));
                    }
                }
                prefix.pop();
            }
            UseTree::Rename(item) => {
                prefix.push(item.ident.to_string());
                self.imported
                    .insert(item.rename.to_string(), prefix.clone());
                if let Some(capability) = effect_capability_path(&self.resolved_words(prefix)) {
                    self.check_capability(
                        capability,
                        format!("capability import {} as {}", prefix.join("::"), item.rename),
                    );
                }
                if exported
                    && prefix.first().is_some_and(|root| root == "winit")
                    && prefix
                        .iter()
                        .any(|part| matches!(part.as_str(), "window" | "event_loop"))
                {
                    self.reject(ViolationKind::PublicNativeReexport, prefix.join("::"));
                }
                if let Some(owner) = native_path(prefix) {
                    self.check_native(
                        owner,
                        format!("native import {} as {}", prefix.join("::"), item.rename),
                    );
                    if exported {
                        self.reject(ViolationKind::PublicNativeReexport, prefix.join("::"));
                    }
                }
                prefix.pop();
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.imports(prefix, item, exported);
                }
            }
            UseTree::Glob(_) => {
                // A native-platform glob erases the names this guard needs.
                // Require explicit imports even inside a native owner.
                if prefix.first().is_some_and(|word| {
                    matches!(
                        word.as_str(),
                        "winit" | "objc2_app_kit" | "core_text" | "arboard" | "smithay_clipboard"
                    )
                }) {
                    self.reject(ViolationKind::NativeGlobImport, prefix.join("::"));
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Guard<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(attributes(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if test_only(&item.attrs) {
            return;
        }
        if let Some((_, items)) = &item.content {
            let imports = self.imported.clone();
            let public_types = self.public_types.clone();
            self.inline_depth += 1;
            for child in items {
                match child {
                    Item::Use(child) if !test_only(&child.attrs) => self.visit_item_use(child),
                    Item::Struct(child) if matches!(child.vis, Visibility::Public(_)) => {
                        self.public_types.insert(child.ident.to_string());
                    }
                    Item::Enum(child) if matches!(child.vis, Visibility::Public(_)) => {
                        self.public_types.insert(child.ident.to_string());
                    }
                    _ => {}
                }
            }
            for child in items {
                self.visit_item(child);
            }
            self.inline_depth -= 1;
            self.imported = imports;
            self.public_types = public_types;
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let previous = self.position;
        self.position = ScanPosition::Import;
        self.imports(
            &mut Vec::new(),
            &item.tree,
            matches!(item.vis, Visibility::Public(_)),
        );
        self.position = previous;
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let words = self.resolved_words(&path_words(path));
        if let Some(capability) = effect_capability_path(&words) {
            self.check_capability(capability, words.join("::"));
        }
        if let Some(owner) = native_path(&words) {
            self.check_native(owner, words.join("::"));
        }
        if raw_command_sender(path) && !owner_allows(self.file, NativeOwner::CommandTransport) {
            self.reject(ViolationKind::RawCommandSender, words.join("::"));
        }
        visit::visit_path(self, path);
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        // Private native kernel aggregates may expose fields to sibling kernel
        // modules. An unrestricted public aggregate must keep backing private.
        if matches!(item.vis, Visibility::Public(_)) {
            for field in &item.fields {
                if matches!(field.vis, Visibility::Public(_)) && self.type_is_native(&field.ty) {
                    self.reject(
                        ViolationKind::PublicNativeField,
                        format!(
                            "{}::{}",
                            item.ident,
                            field
                                .ident
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_else(|| "tuple field".into())
                        ),
                    );
                }
            }
        }
        let previous = self.context.replace(item.ident.to_string());
        visit::visit_item_struct(self, item);
        self.context = previous;
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if matches!(item.vis, Visibility::Public(_)) && self.type_is_native(&item.ty) {
            self.reject(ViolationKind::PublicNativeAlias, item.ident.to_string());
        }
        visit::visit_item_type(self, item);
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if matches!(item.vis, Visibility::Public(_)) && self.type_is_native(&item.ty) {
            self.reject(ViolationKind::PublicNativeStatic, item.ident.to_string());
        }
        visit::visit_item_static(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if matches!(item.vis, Visibility::Public(_)) {
            if let syn::ReturnType::Type(_, ty) = &item.sig.output {
                if self.type_is_native(ty) {
                    self.reject(
                        ViolationKind::PublicNativeResult,
                        item.sig.ident.to_string(),
                    );
                }
            }
        }
        let previous = self.context.replace(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.context = previous;
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous_context = self.context.clone();
        if let Type::Path(ty) = item.self_ty.as_ref() {
            self.context = ty.path.segments.last().map(|part| part.ident.to_string());
        }
        let previous = self.impl_scope;
        self.impl_scope = if match item.self_ty.as_ref() {
            Type::Path(ty) => ty
                .path
                .segments
                .last()
                .is_some_and(|part| self.public_types.contains(&part.ident.to_string())),
            _ => false,
        } {
            ExportScope::Public
        } else {
            ExportScope::Private
        };
        // Opaque native transports must not grow Deref/AsRef escape hatches.
        if self.impl_scope == ExportScope::Public
            && item.trait_.as_ref().is_some_and(|(path, _)| {
                path.segments.last().is_some_and(|part| {
                    matches!(
                        part.ident.to_string().as_str(),
                        "Deref" | "AsRef" | "Borrow"
                    )
                })
            })
        {
            for member in &item.items {
                if let syn::ImplItem::Type(associated) = member {
                    if self.type_is_native(&associated.ty) {
                        self.reject(
                            ViolationKind::PublicNativeDeref,
                            associated.ident.to_string(),
                        );
                    }
                }
            }
        }
        visit::visit_item_impl(self, item);
        self.impl_scope = previous;
        self.context = previous_context;
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if matches!(item.vis, Visibility::Public(_)) {
            for variant in &item.variants {
                if variant
                    .fields
                    .iter()
                    .any(|field| self.type_is_native(&field.ty))
                {
                    self.reject(
                        ViolationKind::PublicNativeVariant,
                        format!("{}::{}", item.ident, variant.ident),
                    );
                }
            }
        }
        visit::visit_item_enum(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if self.impl_scope == ExportScope::Public
            && !test_only(&item.attrs)
            && matches!(item.vis, Visibility::Public(_))
        {
            if let syn::ReturnType::Type(_, ty) = &item.sig.output {
                if self.type_is_native(ty) {
                    self.reject(
                        ViolationKind::PublicNativeResult,
                        item.sig.ident.to_string(),
                    );
                }
            }
        }
        if !test_only(&item.attrs) {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        // thread_local! bodies are Rust static declarations; parse those so
        // `pub static REDISPLAY_RUNTIME` cannot hide behind a macro token tree.
        if item.path.is_ident("thread_local") {
            match syn::parse2::<syn::File>(item.tokens.clone()) {
                Ok(file) => {
                    let mut nested = Guard::new(self.file);
                    nested.imported = self.imported.clone();
                    nested.public_types = self.public_types.clone();
                    nested.visit_file(&file);
                    self.violations.extend(nested.violations);
                }
                Err(error) => self.reject(ViolationKind::UnparsedNativeTls, error.to_string()),
            }
        } else {
            let mut words = Vec::new();
            macro_words(item.tokens.clone(), &mut words);
            for start in 0..words.len() {
                let mut path = vec![words[start].clone()];
                let mut next = start + 1;
                while next + 2 < words.len() && words[next] == ":" && words[next + 1] == ":" {
                    path.push(words[next + 2].clone());
                    next += 3;
                }
                if let Some(owner) = native_path(&self.resolved_words(&path)) {
                    self.check_native(owner, format!("macro {}", path.join("::")));
                }
                if let Some(capability) = effect_capability_path(&self.resolved_words(&path)) {
                    self.check_capability(capability, format!("macro {}", path.join("::")));
                }
                if words[start] == "Sender"
                    && words.get(start + 1).is_some_and(|word| word == "<")
                    && words
                        .iter()
                        .skip(start + 2)
                        .take_while(|word| word.as_str() != ">")
                        .any(|word| word == "RenderCommand")
                    && !owner_allows(self.file, NativeOwner::CommandTransport)
                {
                    self.reject(
                        ViolationKind::RawCommandSender,
                        "macro Sender<RenderCommand>",
                    );
                }
            }
        }
    }
}

fn resolve_words(aliases: &BTreeMap<String, Vec<String>>, words: &[String]) -> Vec<String> {
    let mut resolved = words.to_vec();
    let mut visited = BTreeSet::new();
    while let Some(first) = resolved.first() {
        if !visited.insert(first.clone()) {
            break;
        }
        let Some(prefix) = aliases.get(first) else {
            break;
        };
        let mut next = prefix.clone();
        next.extend_from_slice(&resolved[1..]);
        resolved = next;
    }
    resolved
}

fn macro_words(tokens: proc_macro2::TokenStream, words: &mut Vec<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Group(group) => macro_words(group.stream(), words),
            proc_macro2::TokenTree::Ident(ident) => words.push(ident.to_string()),
            proc_macro2::TokenTree::Punct(punct) => words.push(punct.as_char().to_string()),
            proc_macro2::TokenTree::Literal(_) => words.push("<literal>".into()),
        }
    }
}

fn scan(path: &str, source: &str) -> Vec<Violation> {
    let file = syn::parse_file(source).unwrap_or_else(|error| panic!("parse {path}: {error}"));
    if test_only(&file.attrs) {
        return Vec::new();
    }
    let mut guard = Guard::new(path);
    for item in &file.items {
        match item {
            Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                guard.public_types.insert(item.ident.to_string());
            }
            Item::Enum(item) if matches!(item.vis, Visibility::Public(_)) => {
                guard.public_types.insert(item.ident.to_string());
            }
            _ => {}
        }
    }
    // Imports affect declarations even when the use item appears later.
    for item in &file.items {
        if let Item::Use(item) = item {
            if !test_only(&item.attrs) {
                guard.visit_item_use(item);
            }
        }
    }
    guard.visit_file(&file);
    guard.violations
}

fn explicit_module_path(module: &syn::ItemMod) -> Option<PathBuf> {
    module.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        let Expr::Lit(value) = &value.value else {
            return None;
        };
        let Lit::Str(value) = &value.lit else {
            return None;
        };
        Some(PathBuf::from(value.value()))
    })
}

fn include_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Lit(value) => match &value.lit {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        },
        Expr::Macro(value) if value.mac.path.is_ident("concat") => {
            let parts = value
                .mac
                .parse_body_with(
                    syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            parts.iter().map(include_name).collect::<Option<String>>()
        }
        Expr::Macro(value) if value.mac.path.is_ident("env") => {
            let name = value.mac.parse_body::<syn::LitStr>().ok()?;
            (name.value() == "OUT_DIR").then(|| "$OUT_DIR".into())
        }
        _ => None,
    }
}

/// Follow actual module edges in all production feature arms. A production
/// module called tests, or ending _test.rs, is deliberately NOT skipped.
struct ModuleEdges<'a, F> {
    source: &'a Path,
    module_directory: PathBuf,
    path_directory: PathBuf,
    exists: F,
    files: Vec<PathBuf>,
}
impl<'ast, F: Fn(&Path) -> bool> Visit<'ast> for ModuleEdges<'_, F> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(attributes(item)) {
            visit::visit_item(self, item);
        }
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if let Some((_, items)) = &module.content {
            let directory = self.module_directory.join(module.ident.to_string());
            let previous_module = std::mem::replace(&mut self.module_directory, directory.clone());
            let previous_path = std::mem::replace(&mut self.path_directory, directory);
            for item in items {
                self.visit_item(item);
            }
            self.module_directory = previous_module;
            self.path_directory = previous_path;
        } else {
            let path = explicit_module_path(module)
                .map(|path| self.path_directory.join(path))
                .unwrap_or_else(|| {
                    let base = self.module_directory.join(module.ident.to_string());
                    let file = base.with_extension("rs");
                    if (self.exists)(&file) {
                        file
                    } else {
                        base.join("mod.rs")
                    }
                });
            self.files.push(path);
        }
    }
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if !invocation.path.is_ident("include") {
            return;
        }
        let expression =
            syn::parse2::<Expr>(invocation.tokens.clone()).expect("include source expression");
        let name = include_name(&expression).expect("teach host guard this include expression");
        if name.starts_with("$OUT_DIR") {
            // These existing generated ingress points have fixed producers:
            // owned metadata, or bindings inside the private WPE native kernel.
            // Unknown generated includes fail closed; no OUT_DIR from another
            // crate/build configuration is guessed or scanned as current data.
            let source = self.source.to_string_lossy().replace('\\', "/");
            let declared = [
                (
                    "neomacs-display-protocol/src/x11_colors.rs",
                    "$OUT_DIR/x11_colors.rs",
                ),
                (
                    "neomacs-display-runtime/src/window_identity.rs",
                    "$OUT_DIR/application_identity.rs",
                ),
                (
                    "neomacs-webview/src/platform/linux/sys.rs",
                    "$OUT_DIR/wpe_platform_sys.rs",
                ),
                (
                    "neomacs-webview/src/platform/linux/sys.rs",
                    "$OUT_DIR/wpe_webkit_sys.rs",
                ),
            ]
            .iter()
            .any(|(file, generated)| source.ends_with(file) && name == *generated);
            assert!(
                declared,
                "unreviewed generated host boundary: {source}: {name}"
            );
        } else {
            self.files
                .push(self.source.parent().expect("include parent").join(name));
        }
    }
}

fn module_edges(source: &Path, syntax: &syn::File, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let parent = source.parent().expect("source parent");
    let stem = source.file_stem().expect("source stem");
    let module_directory = if stem == "lib" || stem == "main" || stem == "mod" {
        parent.to_path_buf()
    } else {
        parent.join(stem)
    };
    let mut walker = ModuleEdges {
        source,
        module_directory,
        path_directory: parent.to_path_buf(),
        exists,
        files: Vec::new(),
    };
    walker.visit_file(syntax);
    walker.files
}

fn crate_roots(directory: &Path) -> Vec<PathBuf> {
    let mut roots = ["src/lib.rs", "src/main.rs"]
        .iter()
        .map(|file| directory.join(file))
        .filter(|file| file.is_file())
        .collect::<Vec<_>>();
    // Honor explicit Cargo lib/bin paths, including nested bin main.rs. This
    // parser only reads literal target paths; other manifest sections are not
    // runtime module roots (tests/benches/examples/build scripts).
    let manifest = std::fs::read_to_string(directory.join("Cargo.toml")).expect("crate manifest");
    let mut runtime_target = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            runtime_target = line == "[lib]" || line == "[[bin]]";
        }
        if runtime_target {
            if let Some(value) = line
                .strip_prefix("path")
                .and_then(|rest| rest.trim_start().strip_prefix('='))
            {
                let value = value.trim().trim_matches('"');
                assert!(!value.contains('"'), "teach guard target path syntax");
                roots.push(directory.join(value));
            }
        }
    }
    let bins = directory.join("src/bin");
    if bins.is_dir() {
        for entry in std::fs::read_dir(bins).expect("bin roots") {
            let path = entry.expect("bin entry").path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                roots.push(path);
            } else if path.join("main.rs").is_file() {
                roots.push(path.join("main.rs"));
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[test]
fn native_host_backing_is_confined_to_declared_effect_owners() {
    let crates = neomacs_infra::crate_root!()
        .parent()
        .expect("workspace crates")
        .to_path_buf();
    let mut pending = Vec::new();
    for entry in std::fs::read_dir(&crates).expect("shipping crates") {
        let directory = entry.expect("crate entry").path();
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.contains("test") || name == "xtask" || !directory.join("Cargo.toml").is_file() {
            continue;
        }
        pending.extend(crate_roots(&directory));
    }
    let mut seen = BTreeSet::new();
    let mut violations = Vec::new();
    while let Some(file) = pending.pop() {
        let file = file
            .canonicalize()
            .unwrap_or_else(|error| panic!("module {}: {error}", file.display()));
        if !seen.insert(file.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read production module");
        let syntax = syn::parse_file(&text)
            .unwrap_or_else(|error| panic!("parse {}: {error}", file.display()));
        if test_only(&syntax.attrs) {
            continue;
        }
        let canonical_crates = crates.canonicalize().expect("workspace crates root");
        let relative = file
            .strip_prefix(&canonical_crates)
            .expect("production module under crates")
            .to_string_lossy()
            .replace('\\', "/");
        pending.extend(module_edges(&file, &syntax, Path::is_file));
        for violation in scan(&relative, &text) {
            violations.push(format!(
                "{relative}: {}: {}",
                violation.kind, violation.detail
            ));
        }
    }
    assert!(!seen.is_empty(), "empty production host survey");
    violations.sort();
    assert!(
        violations.is_empty(),
        "Native host effects must stay behind activation-selected owner projections. This is a source boundary guard, not a dispatch/opcode allowlist:\n{}",
        violations.join("\n")
    );
}

/// The published setter must use the alias obtained from this Context's
/// projection, not merely coexist with an unrelated projection read.
#[derive(Default)]
struct Publication {
    aliases: BTreeSet<String>,
    projected_calls: usize,
    unprojected_calls: usize,
}
fn projection_expression(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Field(field) => {
            matches!(&field.member, syn::Member::Named(name) if name == "display_host")
                && matches!(field.base.as_ref(), syn::Expr::Path(path) if path.path.is_ident("self"))
        }
        syn::Expr::MethodCall(call) => projection_expression(&call.receiver),
        syn::Expr::Paren(paren) => projection_expression(&paren.expr),
        syn::Expr::Reference(reference) => projection_expression(&reference.expr),
        _ => false,
    }
}
fn pattern_bindings(pattern: &syn::Pat) -> BTreeSet<String> {
    #[derive(Default)]
    struct Bindings(BTreeSet<String>);
    impl<'ast> Visit<'ast> for Bindings {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.0.insert(pattern.ident.to_string());
            visit::visit_pat_ident(self, pattern);
        }
    }
    let mut names = Bindings::default();
    names.visit_pat(pattern);
    names.0
}
impl<'ast> Visit<'ast> for Publication {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let aliases = self.aliases.clone();
        visit::visit_block(self, block);
        self.aliases = aliases;
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        let names = pattern_bindings(&local.pat);
        for name in &names {
            self.aliases.remove(name);
        }
        if local
            .init
            .as_ref()
            .is_some_and(|init| projection_expression(&init.expr))
        {
            self.aliases.extend(names);
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_if(&mut self, branch: &'ast syn::ExprIf) {
        self.visit_expr(&branch.cond);
        let aliases = self.aliases.clone();
        if let syn::Expr::Let(binding) = branch.cond.as_ref() {
            if projection_expression(&binding.expr) {
                self.aliases.extend(pattern_bindings(&binding.pat));
            }
        }
        self.visit_block(&branch.then_branch);
        self.aliases = aliases;
        if let Some((_, otherwise)) = &branch.else_branch {
            self.visit_expr(otherwise);
        }
    }
    fn visit_expr_assign(&mut self, assignment: &'ast syn::ExprAssign) {
        if let syn::Expr::Path(path) = assignment.left.as_ref() {
            if let Some(name) = path.path.get_ident() {
                self.aliases.remove(&name.to_string());
            }
        }
        visit::visit_expr_assign(self, assignment);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "set_gui_frame_alpha_lower_limit" {
            let projected = projection_expression(&call.receiver)
                || matches!(call.receiver.as_ref(), syn::Expr::Path(path)
                    if path.path.get_ident().is_some_and(|name| self.aliases.contains(&name.to_string())));
            if projected {
                self.projected_calls += 1;
            } else {
                self.unprojected_calls += 1;
            }
        }
        visit::visit_expr_method_call(self, call);
    }
}

#[test]
fn binding_publication_uses_existing_context_projection() {
    // Set/VarSet/VarBind/Unbind and their JIT shims remain permitted unchanged.
    // Only the actual native effect receiver is certified here.
    let source = include_str!("../../runtime/eval/mod.rs");
    let file = syn::parse_file(source).expect("parse evaluator");
    let method = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        })
        .flat_map(|item| &item.items)
        .find_map(|item| match item {
            syn::ImplItem::Fn(item) if item.sig.ident == "sync_cached_runtime_binding_by_id" => {
                Some(item)
            }
            _ => None,
        })
        .expect("binding publication owner");
    let mut publication = Publication::default();
    publication.visit_block(&method.block);
    assert!(
        publication.projected_calls > 0 && publication.unprojected_calls == 0,
        "binding publication must invoke the setter on the alias bound from self.display_host; a void setter may use FIFO one-way publication, result/error operations remain synchronous"
    );
    assert!(
        scan("neovm-core/src/emacs_core/runtime/eval/mod.rs", source).is_empty(),
        "binding publication acquired explicit native backing outside its projection"
    );
}

#[test]
fn rejects_outside_native_borrow_and_renamed_import() {
    let source =
        "use winit::window::Window as NativeWindow; fn wrong(window: &dyn NativeWindow) {}";
    assert!(
        scan("neovm-core/src/emacs_core/editing/new_effect.rs", source)
            .iter()
            .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
    );
}

#[test]
fn terminal_backing_stays_inside_both_terminal_owner_families() {
    let registry = "fn native(manager: &TerminalManager) {}";
    let emulator = "use crate::terminal::TerminalManager; fn native(manager: &TerminalManager) {}";
    assert!(
        scan(
            "neovm-core/src/emacs_core/display/terminal/pure.rs",
            registry
        )
        .is_empty()
    );
    assert!(
        scan(
            "neomacs-display-runtime/src/render_thread/state.rs",
            emulator
        )
        .is_empty()
    );
    assert!(
        scan(
            "neomacs-display-runtime/src/terminal/mod.rs",
            "pub(crate) use view::{TerminalManager, TerminalView};"
        )
        .is_empty()
    );
    for source in [
        registry,
        emulator,
        "use crate::terminal::view::TerminalView as NativeView; fn semantic(view: &NativeView) {}",
        "fn semantic(proxy: &crate::terminal::view::NeomacsEventProxy) {}",
    ] {
        assert!(
            scan("neomacs-display-runtime/src/semantic_command.rs", source)
                .iter()
                .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
        );
    }
    assert!(
        scan(
            "neomacs-display-runtime/src/terminal/mod.rs",
            "pub use view::{TerminalManager, TerminalView};"
        )
        .iter()
        .any(|v| v.kind == ViolationKind::PublicNativeReexport)
    );
}

#[test]
fn rejects_outside_native_tls_even_inside_same_crate() {
    let source = "fn wrong() { crate::frame_layout::REDISPLAY_RUNTIME.with(|runtime| {}); }";
    assert!(
        scan("neomacs/src/semantic_command.rs", source)
            .iter()
            .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
    );
}

#[test]
fn rejects_connected_capabilities_and_inferred_aggregate_fields() {
    for source in [
        "fn wrong(tx: &RenderCommandSender, command: RenderCommand) { tx.send(command); }",
        "fn wrong(c: &EmacsComms, command: RenderCommand) { c.cmd_tx.send(command); }",
        "fn wrong(c: &RenderComms, event: InputEvent) { c.input_tx.send(event); }",
        "fn wrong(command: RenderCommand) { ThreadComms::new().split().0.cmd_tx.send(command); }",
        "fn wrong(proxy: &RenderEventLoopProxy) { proxy.wake_up(); }",
        "fn wrong() { build_render_event_loop().unwrap().create_proxy().wake_up(); }",
        "fn wrong() { build_render_event_loop_any_thread().unwrap().create_proxy().wake_up(); }",
    ] {
        assert!(
            scan("neovm-core/src/emacs_core/editing/new_effect.rs", source)
                .iter()
                .any(|v| v.kind == ViolationKind::OutsideEffectOwner),
            "unfenced capability: {source}"
        );
    }
}

#[test]
fn rejects_capability_aliases_macros_and_unowned_main_items() {
    // Main imports are permitted for its named native items, so these controls
    // must reject the use in the unrelated helper, not merely the import.
    for source in [
        "use neomacs_display_runtime::thread_comm::EmacsComms as Connected; fn semantic(c: &Connected, command: RenderCommand) { c.cmd_tx.send(command); }",
        "use neomacs_display_runtime::thread_comm as transport; fn semantic(c: &transport::RenderComms) {}",
        "use neomacs_display_runtime::render_thread::build_render_event_loop as make_loop; fn semantic() { make_loop().unwrap().create_proxy().wake_up(); }",
        "use neomacs_display_runtime::thread_comm::RenderCommandSender as Connected; fn semantic() { native_operation!(Connected); }",
        "fn semantic() { native_operation!(neomacs_display_runtime::render_thread::RenderEventLoopProxy); }",
    ] {
        assert!(
            scan("neomacs/src/main.rs", source)
                .iter()
                .any(|v| v.kind == ViolationKind::OutsideEffectOwner),
            "unfenced capability alias or macro: {source}"
        );
    }
}

#[test]
fn accepts_owned_capability_factories_and_named_lifecycle_owners() {
    // Public owned factory results are intentionally distinct from raw native
    // results. Their explicit use remains confined to capability owners.
    for (path, source) in [
        (
            "neomacs-display-runtime/src/thread_comm/command_projection.rs",
            "pub fn channel() -> RenderCommandSender { todo!() } pub type Commands = RenderCommandSender;",
        ),
        (
            "neomacs-display-runtime/src/thread_comm.rs",
            "pub use command_projection::RenderCommandSender; pub struct EmacsComms { pub cmd_tx: RenderCommandSender } pub fn channel() -> (EmacsComms, RenderComms) { todo!() }",
        ),
        (
            "neomacs-display-runtime/src/render_thread/bootstrap.rs",
            "pub fn build_render_event_loop() -> Result<RenderEventLoop, String> { todo!() }",
        ),
        (
            "neomacs/src/main.rs",
            "use neomacs_display_runtime::thread_comm::ThreadComms; fn run() { let (emacs, _) = ThreadComms::new().split(); emacs.cmd_tx.send(command); let proxy = build_render_event_loop().unwrap().create_proxy(); proxy.wake_up(); }",
        ),
        (
            "neomacs-display-runtime/src/tty_input/unix_reader.rs",
            "fn deliver(comms: &RenderComms, event: InputEvent) { comms.input_tx.send(event); }",
        ),
    ] {
        assert!(
            scan(path, source).is_empty(),
            "rejected owned factory: {path}"
        );
    }
}

#[test]
fn rejects_public_native_field_in_legitimate_owner() {
    let source = "pub struct NativeState { pub window: std::sync::Arc<dyn winit::window::Window> }";
    assert!(
        scan(
            "neomacs-display-runtime/src/render_thread/new_native.rs",
            source
        )
        .iter()
        .any(|v| v.kind == ViolationKind::PublicNativeField)
    );
}

#[test]
fn rejects_public_raw_alias_and_raw_sender_export() {
    let alias = "pub type NativeLoop = winit::event_loop::EventLoop;";
    assert!(
        scan("neomacs-display-runtime/src/render_thread/mod.rs", alias)
            .iter()
            .any(|v| v.kind == ViolationKind::PublicNativeAlias)
    );
    let sender = "pub struct Commands { pub raw: crossbeam_channel::Sender<RenderCommand> }";
    assert!(
        scan(
            "neomacs-display-runtime/src/thread_comm/command_projection.rs",
            sender
        )
        .iter()
        .any(|v| v.kind == ViolationKind::PublicNativeField)
    );
    let getter = "pub fn raw() -> crossbeam_channel::Sender<RenderCommand> { todo!() }";
    assert!(
        scan(
            "neomacs-display-runtime/src/thread_comm/command_projection.rs",
            getter
        )
        .iter()
        .any(|v| v.kind == ViolationKind::PublicNativeResult)
    );
}

#[test]
fn rejects_public_native_tls_hidden_in_macro() {
    let source = "thread_local! { pub static REDISPLAY_RUNTIME: Box<RedisplayRuntime> = todo!(); }";
    assert!(
        scan("neomacs/src/frame_layout.rs", source)
            .iter()
            .any(|v| v.kind == ViolationKind::PublicNativeStatic)
    );
}

#[test]
fn accepts_private_native_backing_and_owned_protocol() {
    let native = "pub struct NativeLoop(winit::event_loop::EventLoop); struct Local { window: std::sync::Arc<dyn winit::window::Window> }";
    assert!(scan("neomacs-display-runtime/src/render_thread/mod.rs", native).is_empty());
    let protocol = "pub enum RenderCommand { SetTitle { title: String }, Resize { width: u32, height: u32 } } pub trait Projection { fn send(&mut self, command: RenderCommand) -> Result<(), String>; } pub struct Context { pub callback: Option<Box<dyn FnMut(&mut Self)>> }";
    assert!(
        scan(
            "neovm-core/src/emacs_core/runtime/host_service/protocol.rs",
            protocol
        )
        .is_empty()
    );
    let command =
        "pub struct RenderCommandSender { sender: crossbeam_channel::Sender<RenderCommand> }";
    assert!(
        scan(
            "neomacs-display-runtime/src/thread_comm/command_projection.rs",
            command
        )
        .is_empty()
    );
}

#[test]
fn ignores_test_only_native_controls_but_not_other_feature_arms() {
    let source = "#[cfg(test)] fn fixture(_: winit::event_loop::EventLoop) {} #[cfg(target_os = \"macos\")] fn production(_: winit::event_loop::EventLoop) {}";
    let violations = scan("neovm-core/src/emacs_core/editing/new_effect.rs", source);
    assert_eq!(
        violations
            .iter()
            .filter(|v| v.kind == ViolationKind::OutsideNativeOwner)
            .count(),
        1
    );
}

#[test]
fn rejects_renamed_module_and_macro_hidden_native_path() {
    let module_alias = "use winit::window as native; fn wrong(window: &dyn native::Window) {}";
    assert!(
        scan(
            "neovm-core/src/emacs_core/editing/new_effect.rs",
            module_alias
        )
        .iter()
        .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
    );
    let macro_call = "native_operation!(winit::event_loop::EventLoop);";
    assert!(
        scan(
            "neovm-core/src/emacs_core/editing/new_effect.rs",
            macro_call
        )
        .iter()
        .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
    );
    let main_bypass =
        "fn unrelated_semantic_helper() { let value: winit::event_loop::EventLoop = todo!(); }";
    assert!(
        scan("neomacs/src/main.rs", main_bypass)
            .iter()
            .any(|v| v.kind == ViolationKind::OutsideNativeOwner)
    );
}

#[test]
fn module_edges_follow_production_test_shaped_paths() {
    let file = syn::parse_file(
        "#[path = \"tests/new_test.rs\"] mod production; #[cfg(test)] mod fixture;",
    )
    .expect("module fixture");
    let edges = module_edges(Path::new("/crate/src/lib.rs"), &file, |_| false);
    assert_eq!(edges, vec![PathBuf::from("/crate/src/tests/new_test.rs")]);
}

#[test]
fn binding_receiver_control_rejects_unrelated_read_and_shadow() {
    let unrelated: syn::ImplItemFn = syn::parse_str("fn publish(&mut self) { if let Some(host) = self.display_host.as_mut() { observe(host); } other.set_gui_frame_alpha_lower_limit(1.0); }").expect("control");
    let mut survey = Publication::default();
    survey.visit_block(&unrelated.block);
    assert_eq!((survey.projected_calls, survey.unprojected_calls), (0, 1));
    let shadow: syn::ImplItemFn = syn::parse_str("fn publish(&mut self) { if let Some(host) = self.display_host.as_mut() { let host = other; host.set_gui_frame_alpha_lower_limit(1.0); } }").expect("shadow control");
    let mut survey = Publication::default();
    survey.visit_block(&shadow.block);
    assert_eq!((survey.projected_calls, survey.unprojected_calls), (0, 1));
    let legitimate: syn::ImplItemFn = syn::parse_str("fn publish(&mut self) { if let Some(host) = self.display_host.as_mut() { host.set_gui_frame_alpha_lower_limit(1.0); } }").expect("positive");
    let mut survey = Publication::default();
    survey.visit_block(&legitimate.block);
    assert_eq!((survey.projected_calls, survey.unprojected_calls), (1, 0));
}
