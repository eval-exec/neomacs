//! Value printing (Lisp representation).

mod float_output;
pub(crate) use float_output::format_float_with_output_format;

mod princ_text;
pub(crate) use princ_text::{PrincStringDestination, PrincStringOutput, princ_string_bytes};

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;

use super::chartable::{bool_vector_length, char_table_external_slots};
use super::intern::{
    SymId, intern, lookup_interned_lisp_string, resolve_sym, resolve_sym_lisp_string,
    symbol_name_confusing,
};
use super::string_escape::format_lisp_string_bytes_emacs;
use super::value::{
    HashKey, HashTableTest, LispHashTable, StringTextPropertyRun, Value,
    get_string_text_properties_for_value, list_to_vec,
};
use crate::buffer::EmacsBytePos;
use crate::emacs_core::value::{ValueKind, VecLikeType};

/// Canonical output buffer for the stateful printer.
///
/// GNU's `print_object` always emits Emacs characters to a sink; rendering to
/// a Lisp string is only one possible sink.  Keeping bytes here is essential:
/// Rust `String` cannot represent Emacs byte8/non-Unicode characters, so using
/// it as the graph-aware printer's intermediate representation made
/// `print-circle`, `print-level`, and `print-length` silently lossy.
#[derive(Default)]
struct StatefulPrintOutput {
    bytes: Vec<u8>,
}

impl StatefulPrintOutput {
    fn from_str(value: &str) -> Self {
        Self {
            bytes: value.as_bytes().to_vec(),
        }
    }

    fn push(&mut self, character: char) {
        let mut encoded = [0; 4];
        self.bytes
            .extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    }

    fn push_str(&mut self, value: &str) {
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn append(&mut self, other: Self) {
        self.bytes.extend(other.bytes);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl std::fmt::Write for StatefulPrintOutput {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.push_str(value);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrintOptions {
    pub print_gensym: bool,
    pub print_circle: bool,
    pub print_quoted: bool,
    pub print_symbols_bare: bool,
    pub print_escape_newlines: bool,
    pub print_escape_nonascii: bool,
    pub print_escape_multibyte: bool,
    pub print_escape_control_characters: bool,
    pub print_integers_as_characters: bool,
    pub print_level: Option<i64>,
    pub print_length: Option<i64>,
    pub print_continuous_numbering: bool,
    pub print_number_table: Option<Value>,
    pub float_output_format: Option<Value>,
    pub print_noescape: bool,
    pub charset_text_property: CharsetTextProperty,
    pub unreadable_function: Option<Value>,
    pub(crate) print_string_destination: PrincStringDestination,
    backquote_output_level: usize,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self::new(false, false, None, None)
    }
}

/// Policy for charset annotations (GNU print.c:1586-1610).
/// This is a per-call value; it holds no mutator-local global state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharsetTextProperty {
    Never,
    Guessable,
    Always,
}

impl From<Value> for CharsetTextProperty {
    fn from(value: Value) -> Self {
        if value.is_nil() {
            Self::Never
        } else if value == Value::T {
            Self::Always
        } else {
            Self::Guessable
        }
    }
}

/// GNU preserves every charset property if ANY interval has an unsafe one.
/// Return pruned, nonempty intervals without changing the original string.
pub(crate) fn printed_string_property_runs(
    value: Value,
    policy: CharsetTextProperty,
) -> Option<Vec<StringTextPropertyRun>> {
    let runs = get_string_text_properties_for_value(value)?;
    match policy {
        CharsetTextProperty::Always => return Some(runs),
        CharsetTextProperty::Never | CharsetTextProperty::Guessable => {}
    }
    let has_charset = runs.iter().any(|run| {
        let mut plist = run.plist;
        while plist.is_cons() {
            if plist.cons_car() == Value::symbol("charset") {
                return true;
            }
            let tail = plist.cons_cdr();
            if !tail.is_cons() {
                break;
            }
            plist = tail.cons_cdr();
        }
        false
    });
    if !has_charset {
        return Some(runs);
    }
    if policy == CharsetTextProperty::Guessable {
        let ls = value.as_lisp_string()?;
        let chars = crate::emacs_core::builtins::lisp_string_char_codes(ls);
        let unsafe_charset = runs.iter().any(|run| {
            let mut plist = run.plist;
            while plist.is_cons() {
                let key = plist.cons_car();
                plist = plist.cons_cdr();
                if !plist.is_cons() {
                    break;
                }
                let charset = plist.cons_car();
                plist = plist.cons_cdr();
                if key == Value::symbol("charset") {
                    return chars.iter().take(run.end).skip(run.start).any(|&ch| {
                        ch >= 128
                            && charset
                                != Value::symbol(crate::emacs_core::charset::char_charset_name(
                                    ch as i64,
                                ))
                    });
                }
            }
            false
        });
        if unsafe_charset {
            return Some(runs);
        }
    }
    let pruned: Vec<_> = runs
        .into_iter()
        .filter_map(|run| {
            let mut plist = run.plist;
            let mut props = Vec::new();
            while plist.is_cons() {
                let key = plist.cons_car();
                plist = plist.cons_cdr();
                if !plist.is_cons() {
                    break;
                }
                let value = plist.cons_car();
                plist = plist.cons_cdr();
                if key != Value::symbol("charset") {
                    props.extend([key, value]);
                }
            }
            (!props.is_empty()).then(|| StringTextPropertyRun {
                plist: Value::list(props),
                ..run
            })
        })
        .collect();
    // GNU print.c:1595-1610 removes charset via Fremove_text_properties.
    // Its walk (textprop.c:1636-1680) preserves physical interval boundaries
    // and plist order; semantic property-change equality does not merge them.
    (!pruned.is_empty()).then_some(pruned)
}

/// GNU prints the test symbol through `print_object`, preserving name bytes,
/// the caller's escape mode, and EQ identity when omitting the default eql test
/// (src/print.c:2636-2640). No UTF-8 conversion is valid for a Lisp symbol name.
/// The borrowed table is confined to its mutator; this helper stores no state.
fn hash_table_printed_test_name(table: &LispHashTable) -> Option<Value> {
    let name = match table.test_name {
        Some(id) => Value::symbol(id),
        None => Value::symbol(table.test.name()),
    };
    (name != Value::symbol("eql")).then_some(name)
}

fn append_hash_table_test_bytes(table: &LispHashTable, out: &mut Vec<u8>, options: PrintOptions) {
    if let Some(name) = hash_table_printed_test_name(table) {
        out.extend_from_slice(b" test ");
        append_print_value_bytes(&name, out, options);
    }
}

/// Mirrors GNU `named_escape` (print.c): the single-letter escape used to
/// print a character in `?\X` form, or `None` if there is no named escape.
///
/// `\a`, `\v`, `\e` and `\d` are intentionally excluded (matching GNU): they
/// are rare as characters and more likely meant as plain integers.
fn named_escape(i: u32) -> Option<char> {
    match i {
        0x08 => Some('b'), // \b
        0x09 => Some('t'), // \t
        0x0A => Some('n'), // \n
        0x0C => Some('f'), // \f
        0x0D => Some('r'), // \r
        0x20 => Some('s'), // space
        _ => None,
    }
}

/// If `print-integers-as-characters` is active and `i` is a character that
/// GNU prints in `?X` syntax, format it that way and return `true`. Mirrors
/// the `Lisp_Int0/Int1` case of GNU `print_object` (print.c).
///
/// `escapeflag` is true for `prin1`-style output (adds the extra backslash
/// before the self-delimiting characters), false for `princ`-style output.
fn try_format_integer_as_character(
    i: i64,
    escapeflag: bool,
    push_char: &mut dyn FnMut(char),
) -> bool {
    use crate::emacs_core::emacs_char::{MAX_UNICODE_CHAR, char_general_category, graphic_base_p};

    if !(0..=i64::from(MAX_UNICODE_CHAR)).contains(&i) {
        return false;
    }
    let code = i as u32;
    let escaped_name = named_escape(code);
    let is_graphic_base = char_general_category(code).is_some_and(graphic_base_p);
    if escaped_name.is_none() && !is_graphic_base {
        return false;
    }

    push_char('?');
    if let Some(name) = escaped_name {
        push_char('\\');
        push_char(name);
    } else {
        // `code` is graphic_base, so it is a valid scalar value.
        let ch = char::from_u32(code).unwrap();
        if escapeflag
            && matches!(
                ch,
                ';' | '"' | '\'' | '\\' | '(' | ')' | '{' | '}' | '[' | ']'
            )
        {
            push_char('\\');
        }
        push_char(ch);
    }
    true
}

impl PrintOptions {
    /// Source encoding and the stream jointly decide the no-escape character
    /// protocol. Callable streams ignore print-escape-nonascii (print.c:525).
    #[inline]
    pub(crate) fn princ_string_output(&self) -> PrincStringOutput {
        match self.print_string_destination {
            PrincStringDestination::Callable => PrincStringOutput::CallbackCharacters,
            PrincStringDestination::Text => {
                if self.print_escape_nonascii {
                    PrincStringOutput::OctalByte8
                } else {
                    PrincStringOutput::PreserveByte8
                }
            }
        }
    }

    /// `princ`'s options: strings inside an aggregate print their own bytes
    /// instead of a quoted form.
    ///
    /// GNU's `format` prints `%s` with `princ` (as opposed to `%S`, which is
    /// `prin1`), and native code reaches for it when it has an object and no
    /// string to put in its place — `image_error ("Not a PNG image: `%s'",
    /// img->spec)`, `src/image.c:8323`. Callers outside this module cannot
    /// build `PrintOptions` themselves, because one field is private.
    #[must_use]
    pub fn princ() -> Self {
        Self {
            print_noescape: true,
            ..Self::default()
        }
    }

    pub const fn with_print_gensym(print_gensym: bool) -> Self {
        Self {
            print_gensym,
            print_circle: false,
            print_quoted: true,
            print_symbols_bare: false,
            print_escape_newlines: false,
            print_escape_nonascii: false,
            print_escape_multibyte: false,
            print_escape_control_characters: false,
            print_integers_as_characters: false,
            print_level: None,
            print_length: None,
            print_continuous_numbering: false,
            print_number_table: None,
            float_output_format: None,
            print_noescape: false,
            charset_text_property: CharsetTextProperty::Guessable,
            unreadable_function: None,
            print_string_destination: PrincStringDestination::Text,
            backquote_output_level: 0,
        }
    }

    /// Full constructor for all print options.
    pub fn new(
        print_gensym: bool,
        print_circle: bool,
        print_level: Option<i64>,
        print_length: Option<i64>,
    ) -> Self {
        Self {
            print_gensym,
            print_circle,
            print_quoted: true,
            print_symbols_bare: false,
            print_escape_newlines: false,
            print_escape_nonascii: false,
            print_escape_multibyte: false,
            print_escape_control_characters: false,
            print_integers_as_characters: false,
            print_level,
            print_length,
            print_continuous_numbering: false,
            print_number_table: None,
            float_output_format: None,
            print_noescape: false,
            charset_text_property: CharsetTextProperty::Guessable,
            unreadable_function: None,
            print_string_destination: PrincStringDestination::Text,
            backquote_output_level: 0,
        }
    }

    pub(crate) fn enter_backquote(self) -> Self {
        Self {
            backquote_output_level: self.backquote_output_level + 1,
            ..self
        }
    }

    pub(crate) fn exit_backquote(self) -> Self {
        Self {
            backquote_output_level: self.backquote_output_level.saturating_sub(1),
            ..self
        }
    }

    pub(crate) fn allow_unquote_shorthand(self) -> bool {
        self.backquote_output_level > 0
    }
}

// ---------------------------------------------------------------------------
// Print-circle state (two-pass algorithm)
// ---------------------------------------------------------------------------

/// State for the print-circle two-pass algorithm.
/// Keys are object identity values (SymId).
#[derive(Debug)]
pub struct PrintCircleState {
    /// Maps object identity -> label status:
    /// 0 = seen once (removed after pass 1)
    /// negative = assigned label, not yet printed
    /// positive = already printed with this label
    number_table: HashMap<u64, i64>,
    next_label: i64,
}

impl PrintCircleState {
    fn new() -> Self {
        Self {
            number_table: HashMap::new(),
            next_label: 0,
        }
    }
}

thread_local! {
    static PRINT_NUMBER_INDEX: Cell<i64> = const { Cell::new(0) };
    static PRINT_CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn reset_print_number_index() {
    PRINT_NUMBER_INDEX.with(|index| index.set(0));
}

fn next_print_number_index() -> i64 {
    PRINT_NUMBER_INDEX.with(|index| {
        let next = index.get() + 1;
        index.set(next);
        next
    })
}

/// Combined print state used by the stateful print path.
#[derive(Debug)]
pub(crate) struct PrintState<'a> {
    pub options: PrintOptions,
    pub circle: Option<&'a mut PrintCircleState>,
    context: PrinterContext<'a>,
    failure: Option<crate::emacs_core::error::Flow>,
    settings_source: PrintSettingsSource,
    callback_number_table: CallbackNumberTable,
    pub depth: i64,
    _mutator: std::marker::PhantomData<*const ()>,
    default_print_stack: Vec<Option<u64>>,
    object_stack: Vec<u64>,
}

/// Select the bindings used by the print destination. Scratch/format targets
/// use defaults; an actual stream uses its saved target/caller buffer identity.
/// This scalar metadata holds no heap pointers and may cross threads. The
/// mutable printer Context and its guards remain confined to their mutator.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PrintSettingsSource {
    Globals,
    Buffer(crate::buffer::BufferId),
}
static_assertions::assert_impl_all!(PrintSettingsSource: Send, Sync);
impl PrintSettingsSource {
    fn buffer<'context>(
        self,
        context: &'context crate::emacs_core::eval::Context,
    ) -> Option<&'context crate::buffer::Buffer> {
        match self {
            Self::Globals => None,
            Self::Buffer(id) => context.buffers.get(id),
        }
    }
}

/// A temporary destination selection for internal print-variable writes.
/// Explicit finish restores runtime state on ordinary and Lisp-signaled exits;
/// Drop restores only native storage during Rust unwinding. This scope owns no
/// Lisp bindings and remains on its creating mutator thread.
#[must_use = "finish the guard to restore the originating runtime state"]
struct PrintSettingsBufferGuard<'context> {
    context: &'context mut crate::emacs_core::eval::Context,
    origin: Option<crate::buffer::BufferId>,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(PrintSettingsBufferGuard<'static>: Send, Sync);
impl std::fmt::Debug for PrintSettingsBufferGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintSettingsBufferGuard")
            .field("origin", &self.origin)
            .finish()
    }
}
impl<'context> PrintSettingsBufferGuard<'context> {
    fn new(context: &'context mut crate::emacs_core::eval::Context) -> Self {
        let origin = context.buffers.current_buffer_id();
        Self {
            context,
            origin,
            _mutator: std::marker::PhantomData,
        }
    }
    fn select(
        &mut self,
        buffer: crate::buffer::BufferId,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        if self.context.buffers.current_buffer_id() == Some(buffer) {
            Ok(())
        } else {
            self.context.set_current_buffer_unrecorded(buffer)
        }
    }
    fn finish<T>(
        mut self,
        result: Result<T, crate::emacs_core::error::Flow>,
    ) -> Result<T, crate::emacs_core::error::Flow> {
        if let Some(origin) = self.origin {
            self.context.restore_current_buffer_if_live(origin);
        }
        self.origin = None;
        result
    }
}
impl Drop for PrintSettingsBufferGuard<'_> {
    fn drop(&mut self) {
        if let Some(origin) = self.origin.take() {
            self.context.restore_current_buffer_storage_if_live(origin);
        }
    }
}

impl PrintSettingsSource {
    /// GNU print.c:144-151 selects the stream buffer before its direct C
    /// variable writes at 1299-1320. Use raw binding storage, without Lisp
    /// watcher notifications; a default-cell write must ignore caller locals.
    fn write_number_table(
        self,
        context: &mut crate::emacs_core::eval::Context,
        table: Value,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        let _roots = PrintScratchRootsGuard::new();
        crate::emacs_core::eval::push_scratch_gc_root(table);
        match self {
            Self::Globals => {
                context.set_variable("print-number-table", table);
                Ok(())
            }
            Self::Buffer(buffer) => {
                let mut scope = PrintSettingsBufferGuard::new(context);
                let result = (|| {
                    scope.select(buffer)?;
                    // Context::set_variable writes the DEFAULT cell of localized
                    // symbols; this raw store honors the selected local cell.
                    scope
                        .context
                        .try_set_runtime_binding_by_id(intern("print-number-table"), table)
                        .map(|_| ())
                })();
                scope.finish(result)
            }
        }
    }
}

/// Ordinary calls retain their captured table; successful callbacks expose
/// GNU's live table value, including invalid values signaled at the NEXT
/// candidate visit. No table is regenerated or preprocessed after callback.
#[derive(Clone, Copy, Debug)]
enum CallbackNumberTable {
    Captured,
    Live(Value),
}

/// Evaluator-backed printing holds the exclusive mutator borrow across callbacks.
/// Standalone printing never evaluates Lisp. No per-thread Lisp state is cached.
enum PrinterContext<'a> {
    Standalone(Option<&'a crate::buffer::BufferManager>),
    Evaluator(&'a mut crate::emacs_core::eval::Context),
}
impl std::fmt::Debug for PrinterContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standalone(buffers) => f
                .debug_struct("StandalonePrinter")
                .field("has_buffers", &buffers.is_some())
                .finish(),
            Self::Evaluator(_) => f.debug_struct("EvaluatorPrinter").finish_non_exhaustive(),
        }
    }
}
impl PrinterContext<'_> {
    fn buffers(&self) -> Option<&crate::buffer::BufferManager> {
        match self {
            Self::Standalone(buffers) => *buffers,
            Self::Evaluator(ctx) => Some(&ctx.buffers),
        }
    }
}

/// Root and dynamic-binding scope owned by one mutator. Explicit finish
/// propagates Lisp cleanup errors; Drop abandons Lisp cleanup payloads and
/// restores native bindings and roots without evaluating Lisp.
#[must_use = "finish the guard to propagate printer cleanup signals"]
pub(crate) struct PrintEvaluationGuard<'a> {
    context: &'a mut crate::emacs_core::eval::Context,
    depth: Option<usize>,
    _mutator: std::marker::PhantomData<*const ()>,
}
impl std::fmt::Debug for PrintEvaluationGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintEvaluationGuard")
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}
impl<'a> PrintEvaluationGuard<'a> {
    pub(crate) fn context(&mut self) -> &mut crate::emacs_core::eval::Context {
        self.context
    }
    pub(crate) fn new(context: &'a mut crate::emacs_core::eval::Context) -> Self {
        let depth = context.specpdl.len();
        Self {
            context,
            depth: Some(depth),
            _mutator: std::marker::PhantomData,
        }
    }
    pub(crate) fn finish_value(
        mut self,
        result: crate::emacs_core::error::EvalResult,
    ) -> crate::emacs_core::error::EvalResult {
        let result = match self.depth {
            Some(depth) => self.context.unbind_to_with_result(depth, result),
            None => result,
        };
        self.depth = None;
        result
    }
    pub(crate) fn finish<T>(
        mut self,
        result: Result<T, crate::emacs_core::error::Flow>,
    ) -> Result<T, crate::emacs_core::error::Flow> {
        if let Some(depth) = self.depth {
            let pending = result.as_ref().map(|_| Value::NIL).map_err(Clone::clone);
            let cleanup = self.context.unbind_to_with_result(depth, pending);
            self.depth = None;
            cleanup?;
        }
        result
    }
}
impl Drop for PrintEvaluationGuard<'_> {
    fn drop(&mut self) {
        if let Some(depth) = self.depth.take() {
            self.context.discard_specpdl_to(depth);
        }
    }
}

/// Owns roots while exclusively borrowing the caller's per-print state.
/// The context cannot change until Drop restores the saved scope. Rooting is
/// needed only on the evaluator path capable of invoking Lisp callbacks.
#[must_use = "dropping the guard releases temporary printer roots"]
struct PrintValueRootsGuard<'s, 'a> {
    state: &'s mut PrintState<'a>,
    scope: Option<crate::emacs_core::eval::SpecpdlRootScopeState>,
}
impl std::fmt::Debug for PrintValueRootsGuard<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintValueRootsGuard")
            .field("scope", &self.scope)
            .field("depth", &self.state.depth)
            .finish()
    }
}
impl<'s, 'a> PrintValueRootsGuard<'s, 'a> {
    fn new(state: &'s mut PrintState<'a>, values: impl IntoIterator<Item = Value>) -> Self {
        let scope = if state.options.unreadable_function.is_some() {
            match &mut state.context {
                PrinterContext::Standalone(_) => None,
                PrinterContext::Evaluator(context) => {
                    let scope = context.save_specpdl_roots();
                    for value in values {
                        context.push_specpdl_root(value);
                    }
                    Some(scope)
                }
            }
        } else {
            None
        };
        Self { state, scope }
    }
}
impl Drop for PrintValueRootsGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take()
            && let PrinterContext::Evaluator(context) = &mut self.state.context
        {
            context.restore_specpdl_roots(scope);
        }
    }
}

static_assertions::assert_not_impl_any!(PrintEvaluationGuard<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(PrintValueRootsGuard<'static, 'static>: Send, Sync);
static_assertions::assert_not_impl_any!(PrintState<'static>: Send, Sync);

/// GNU print.c:1700-1733: nil delegates; a string emits its characters;
/// any other nonnil result consumes the object without emitting characters.
#[derive(Debug)]
enum UnreadableResult {
    NotCalled,
    Delegated,
    HandledString(crate::heap_types::LispString),
    HandledEmpty,
}

#[cold]
#[inline(never)]
fn refresh_print_options_after_callback(state: &mut PrintState) {
    let PrinterContext::Evaluator(context) = &state.context else {
        return;
    };
    let buffer = state.settings_source.buffer(context);
    let dynamic = super::error::print_options_from_state(context.variables(), buffer);
    let table = context
        .variables()
        .value_in_buffer_by_name(buffer, "print-number-table")
        .unwrap_or(Value::NIL);
    // Syntax nesting and destination belong to this invocation, whereas all
    // Lisp-visible print variables are read live after the callback returns.
    state.options = PrintOptions {
        print_noescape: state.options.print_noescape,
        print_string_destination: state.options.print_string_destination,
        backquote_output_level: state.options.backquote_output_level,
        ..dynamic
    };
    state.callback_number_table = CallbackNumberTable::Live(table);
    state.circle = None;
}

fn options_buffer_princ(options: PrintOptions, value: Value) -> bool {
    options.print_noescape && value.is_buffer()
}

fn unreadable_object(value: Value) -> bool {
    match value.kind() {
        ValueKind::Subr(_) => true,
        ValueKind::Veclike(kind) => match kind {
            VecLikeType::Vector
            | VecLikeType::Bignum
            | VecLikeType::SymbolWithPos
            | VecLikeType::BoolVector
            | VecLikeType::HashTable
            | VecLikeType::Lambda
            | VecLikeType::CharTable
            | VecLikeType::SubCharTable
            | VecLikeType::Record
            | VecLikeType::Macro
            | VecLikeType::ByteCode => false,
            VecLikeType::Marker
            | VecLikeType::Overlay
            | VecLikeType::Finalizer
            | VecLikeType::UserPtr
            | VecLikeType::Process
            | VecLikeType::Frame
            | VecLikeType::Window
            | VecLikeType::Buffer
            | VecLikeType::Obarray
            | VecLikeType::Terminal
            | VecLikeType::WindowConfiguration
            | VecLikeType::Subr
            | VecLikeType::Xwidget
            | VecLikeType::XwidgetView
            | VecLikeType::Thread
            | VecLikeType::Mutex
            | VecLikeType::CondVar
            | VecLikeType::ModuleFunction
            | VecLikeType::Sqlite
            | VecLikeType::Font
            | VecLikeType::Timer
            | VecLikeType::SurfaceHandle
            | VecLikeType::VideoHandle => true,
        },
        ValueKind::Nil
        | ValueKind::T
        | ValueKind::Fixnum(_)
        | ValueKind::Float
        | ValueKind::Symbol(_)
        | ValueKind::String
        | ValueKind::Cons
        | ValueKind::Unbound
        | ValueKind::Unknown => false,
    }
}

/// The GNU secret printer buffer observes the default cell even if the
/// originating caller has its own local printer function. Keep this narrow
/// printer-owned binding on the existing specpdl; ordinary unwind restores it.
/// No new Context API or temporary Lisp buffer is required.
fn bind_unreadable_printer_default(
    context: &mut crate::emacs_core::eval::Context,
) -> Result<(), crate::emacs_core::error::Flow> {
    use crate::emacs_core::eval::{SavedBindingValue, SavedBufferId, SpecBinding};
    let symbol = crate::emacs_core::builtins::resolve_variable_alias_id(
        context,
        intern("print-unreadable-function"),
    )?;
    let old = context.variable_value(symbol).unwrap_or(Value::UNBOUND);
    context.push_specpdl_with(|| SpecBinding::LetDefault {
        sym_id: symbol,
        old_value: SavedBindingValue::from_plain(old),
        buffer_id: SavedBufferId::from_option(None),
    });
    // Notify exactly once with the binding operation and default-cell WHERE.
    // ThreadSwitch selects the existing checked default storage without
    // duplicating a watcher notification as a set-default operation.
    context.run_variable_watchers_by_id(symbol, &Value::NIL, &Value::NIL, "let")?;
    crate::emacs_core::data::set_default_internal_resolved(
        context,
        symbol,
        Value::NIL,
        crate::emacs_core::symbol::SetInternalBind::ThreadSwitch,
    )?;
    Ok(())
}

fn call_unreadable_printer(
    context: &mut crate::emacs_core::eval::Context,
    value: Value,
    options: PrintOptions,
    settings_source: PrintSettingsSource,
) -> Result<UnreadableResult, crate::emacs_core::error::Flow> {
    let Some(function) = options.unreadable_function else {
        return Ok(UnreadableResult::NotCalled);
    };
    if !crate::emacs_core::builtins::value_is_function(context, function) {
        return Ok(UnreadableResult::NotCalled);
    }
    let caller_buffer = context.buffers.current_buffer_id();
    let caller_scope = PrintSettingsBufferGuard::new(context);
    let mut scope = PrintEvaluationGuard::new(caller_scope.context);
    // A successful earlier callback can replace the dynamic printer function.
    // Root this invocation's captured function and object before a binding
    // watcher can remove them from their original heap owners and collect.
    scope.context.push_specpdl_root(function);
    scope.context.push_specpdl_root(value);
    // GNU print.c:1710-1726: failed binding also unwinds this scope; a
    // cleanup Flow replaces the pending bind/apply Flow, as for normal exit.
    let result = (|| {
        // GNU print.c:1710 binds in the prepared destination; only afterward
        // does 1720-1723 select the original caller for callback execution.
        // This guard ends before apply, while the outer evaluator scope owns
        // the destination-local Let binding through callback and unlet.
        match settings_source {
            PrintSettingsSource::Globals => {
                let binding_result = bind_unreadable_printer_default(scope.context());
                if let Some(buffer_id) = scope.context.buffers.current_buffer_id() {
                    scope.context.specpdl.push(
                        crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id },
                    );
                }
                binding_result?;
            }
            PrintSettingsSource::Buffer(buffer) => {
                let mut destination = PrintSettingsBufferGuard::new(scope.context());
                let binding_result = (|| {
                    destination.select(buffer)?;
                    let binding_result = destination
                        .context
                        .try_specbind(intern("print-unreadable-function"), Value::NIL);
                    if let Some(buffer_id) = destination.context.buffers.current_buffer_id() {
                        destination.context.specpdl.push(
                            crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id },
                        );
                    }
                    binding_result
                })();
                destination.finish(binding_result)?;
            }
        }
        if let Some(caller_buffer) = caller_buffer {
            if scope.context.buffers.current_buffer_id() != Some(caller_buffer) {
                scope.context.set_current_buffer_unrecorded(caller_buffer)?;
            }
        }
        scope.context.apply(
            function,
            vec![value, Value::bool_val(!options.print_noescape)],
        )
    })();
    let result = scope.finish_value(result).map(|result| {
        if result.is_nil() {
            UnreadableResult::Delegated
        } else if let Some(string) = result.as_lisp_string() {
            let bytes = string.as_bytes().to_vec();
            // GNU print_string ignores result text properties; copy only payload
            // and source encoding, so no unrooted property Values survive here.
            let owned = if string.is_multibyte() {
                crate::heap_types::LispString::from_emacs_bytes(bytes)
            } else {
                crate::heap_types::LispString::from_unibyte(bytes)
            };
            UnreadableResult::HandledString(owned)
        } else {
            UnreadableResult::HandledEmpty
        }
    });
    caller_scope.finish(result)
}

/// GNU print.c:1299-1320 and 1441-1445: reset the visible numbering
/// table at print entry unless continuous numbering retains it; materialize an
/// eq table only when preprocessing can visit a circle candidate. The table is
/// owned and rooted by this evaluator's dynamic variable, never by a new TLS
/// cache. Override bindings must already be active before this helper runs.
fn prepare_print_number_table(
    context: &mut crate::emacs_core::eval::Context,
    value: &Value,
    options: &mut PrintOptions,
    source: PrintSettingsSource,
) -> Result<(), crate::emacs_core::error::Flow> {
    let mut table = context
        .variables()
        .value_in_buffer_by_name(source.buffer(context), "print-number-table")
        .unwrap_or(Value::NIL);
    if !options.print_continuous_numbering || table.is_nil() {
        reset_print_number_index();
        if !table.is_nil() {
            source.write_number_table(context, Value::NIL)?;
        }
        table = Value::NIL;
    }
    if options.print_circle
        && is_print_circle_candidate(value, options.print_gensym)
        && !table.is_hash_table()
    {
        table = Value::hash_table(HashTableTest::Eq);
        source.write_number_table(context, table)?;
    }
    options.print_number_table = table.is_hash_table().then_some(table);
    Ok(())
}

/// Fallible synchronous printer for Lisp entrypoints. A callback can mutate
/// the evaluator before the following object is rendered, as in GNU print.c.
pub(crate) fn print_value_bytes_in_context(
    context: &mut crate::emacs_core::eval::Context,
    value: &Value,
    mut options: PrintOptions,
    settings_source: PrintSettingsSource,
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    prepare_print_number_table(context, value, &mut options, settings_source)?;
    // Immediate values and property-free strings cannot invoke Lisp or reach
    // the depth limit. Retain their existing direct printer path.
    if !options.print_circle
        && match value.kind() {
            ValueKind::Nil
            | ValueKind::T
            | ValueKind::Fixnum(_)
            | ValueKind::Float
            | ValueKind::Symbol(_) => true,
            ValueKind::String => get_string_text_properties_for_value(*value).is_none(),
            ValueKind::Subr(_)
            | ValueKind::Cons
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => false,
        }
    {
        return Ok(print_value_bytes_with_options(value, options));
    }
    let scope = PrintEvaluationGuard::new(context);
    if let Some(buffer_id) = scope.context.buffers.current_buffer_id() {
        scope
            .context
            .specpdl
            .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
    }
    scope.context.push_specpdl_root(*value);
    for value in [
        options.unreadable_function,
        options.print_number_table,
        options.float_output_format,
    ]
    .into_iter()
    .flatten()
    {
        scope.context.push_specpdl_root(value);
    }
    let _guard = enter_print_call(&options);
    let mut circle = PrintCircleState::new();
    let number_table = active_print_number_table(&options);
    if options.print_circle {
        if let Some(table) = number_table {
            print_preprocess_external(value, table, options);
        } else {
            print_preprocess(value, &mut circle, options);
        }
    }
    let mut state = PrintState {
        options,
        circle: (options.print_circle && number_table.is_none()).then_some(&mut circle),
        context: PrinterContext::Evaluator(&mut *scope.context),
        failure: None,
        settings_source,
        callback_number_table: CallbackNumberTable::Captured,
        depth: 0,
        _mutator: std::marker::PhantomData,
        default_print_stack: Vec::new(),
        object_stack: Vec::new(),
    };
    let mut output = StatefulPrintOutput::default();
    write_value_stateful(value, &mut output, &mut state);
    let failure = state.failure.take();
    drop(state);
    scope.finish(match failure {
        Some(error) => Err(error),
        None => Ok(output.into_bytes()),
    })
}

/// Check if a value is a candidate for circle detection.
/// Matches GNU Emacs's `print_circle_candidate_p`.
fn is_print_circle_candidate(value: &Value, print_gensym: bool) -> bool {
    match value.kind() {
        ValueKind::Cons => true,
        ValueKind::Veclike(VecLikeType::Vector) => {
            // Non-empty vectors only (bool-vectors are their own type).
            value.as_vector_data().is_some_and(|v| !v.is_empty())
        }
        ValueKind::Veclike(VecLikeType::Record) => true,
        ValueKind::Veclike(VecLikeType::HashTable) => true,
        ValueKind::Veclike(VecLikeType::Obarray) => true,
        ValueKind::Veclike(VecLikeType::CharTable) => true,
        ValueKind::Veclike(VecLikeType::SubCharTable) => true,
        ValueKind::Veclike(VecLikeType::Lambda) => true,
        ValueKind::Veclike(VecLikeType::Macro) => true,
        ValueKind::Veclike(VecLikeType::ByteCode) => true,
        ValueKind::String => string_print_circle_candidate(value),
        ValueKind::Symbol(id) if print_gensym => {
            // Uninterned symbols only
            let name = resolve_sym_lisp_string(id);
            lookup_interned_lisp_string(name) != Some(id)
        }
        ValueKind::Unbound => print_gensym,
        _ => false,
    }
}

fn string_print_circle_candidate(value: &Value) -> bool {
    value.as_lisp_string().is_some_and(|s| !s.is_empty())
}

fn is_uninterned_symbol(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Unbound => true,
        ValueKind::Symbol(id) => {
            let name = resolve_sym_lisp_string(id);
            lookup_interned_lisp_string(name) != Some(id)
        }
        _ => false,
    }
}

/// Return a unique identity key for a circle-candidate value.
fn object_identity_key(value: &Value) -> Option<u64> {
    match value.kind() {
        ValueKind::Cons
        | ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::HashTable)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::SubCharTable)
        | ValueKind::String
        | ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::Macro)
        | ValueKind::Veclike(VecLikeType::ByteCode) => Some(value.0 as u64),
        ValueKind::Symbol(id) => {
            // Use a distinct namespace to avoid collisions with heap pointer keys.
            // Set the high bit to separate from pointer keys.
            Some((1u64 << 63) | (id.0 as u64))
        }
        ValueKind::Unbound => Some((1u64 << 63) | (super::intern::UNBOUND_SYM_ID.0 as u64)),
        _ => None,
    }
}

fn active_print_number_table(options: &PrintOptions) -> Option<Value> {
    if !options.print_circle {
        return None;
    }
    options
        .print_number_table
        .filter(|table| table.is_hash_table())
}

#[must_use = "the thread-local extent ends when this guard drops"]
#[derive(Debug)]
pub(crate) struct PrintNumberingGuard {
    _scope: crate::tls_scope::TlsScope<usize, std::cell::Cell<usize>>,
}
static_assertions::assert_not_impl_any!(PrintNumberingGuard: Send, Sync);

pub(crate) fn enter_print_call(options: &PrintOptions) -> PrintNumberingGuard {
    let previous = PRINT_CALL_DEPTH.with(|depth| {
        let current = depth.get();
        if current == 0
            && (!options.print_continuous_numbering || active_print_number_table(options).is_none())
        {
            reset_print_number_index();
        }
        depth.replace(current.saturating_add(1))
    });
    PrintNumberingGuard {
        _scope: crate::tls_scope::TlsScope::restore(&PRINT_CALL_DEPTH, previous),
    }
}

fn print_number_table_key(table_value: Value, value: &Value) -> Option<HashKey> {
    let table = table_value.as_hash_table()?;
    Some(value.to_hash_key(&table.test))
}

fn get_print_number_table_entry(table_value: Value, value: &Value) -> Option<(HashKey, Value)> {
    let key = print_number_table_key(table_value, value)?;
    let entry = {
        let table = table_value.as_hash_table()?;
        table.data.get(&key).copied().unwrap_or(Value::NIL)
    };
    Some((key, entry))
}

fn put_print_number_table_entry(
    table_value: Value,
    key: HashKey,
    key_value: Value,
    entry_value: Value,
) {
    let _ = table_value.with_hash_table_mut(|table| {
        table.insert(key, key_value, entry_value);
    });
}

fn remove_print_number_table_t_entries(table_value: Value) {
    let _ = table_value.with_hash_table_mut(|table| {
        // GNU print.c removes all `t` status entries after preprocessing.
        table.data.retain(|_, value| *value != Value::T);
    });
}

fn print_number_entry_is_symbol(entry: Value) -> bool {
    matches!(
        entry.kind(),
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_)
    )
}

fn print_number_entry_is_cdr_label(entry: Value) -> bool {
    !entry.is_nil() && entry != Value::T
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrintNumberTableAction {
    PrintedReference,
    PrintedPrefix,
}

fn write_print_number_table_entry(
    value: &Value,
    out: &mut StatefulPrintOutput,
    options: &PrintOptions,
) -> Option<PrintNumberTableAction> {
    if !options.print_circle {
        return None;
    }

    let table_value = active_print_number_table(options)?;
    let (key, entry) = get_print_number_table_entry(table_value, value)?;

    match entry.kind() {
        ValueKind::String => {
            let s = entry.as_lisp_string().unwrap();
            out.extend_from_slice(s.as_bytes());
            Some(PrintNumberTableAction::PrintedReference)
        }
        ValueKind::Fixnum(n) if n < 0 => {
            write!(out, "#{}=", -n).unwrap();
            put_print_number_table_entry(table_value, key, *value, Value::fixnum(-n));
            Some(PrintNumberTableAction::PrintedPrefix)
        }
        ValueKind::Fixnum(n) if n > 0 => {
            write!(out, "#{n}#").unwrap();
            Some(PrintNumberTableAction::PrintedReference)
        }
        _ => None,
    }
}

/// Preprocess pass: walk the value tree to find shared/circular structures.
/// Uses an explicit stack (not recursive) matching GNU Emacs.
fn print_preprocess(value: &Value, state: &mut PrintCircleState, options: PrintOptions) {
    let mut stack: Vec<Value> = vec![*value];
    while let Some(obj) = stack.pop() {
        if !is_print_circle_candidate(&obj, options.print_gensym) {
            continue;
        }
        let key = match object_identity_key(&obj) {
            Some(k) => k,
            None => continue,
        };
        if let Some(status) = state.number_table.get(&key) {
            if *status == 0 {
                // Seen second time -- assign label
                let label = if options.print_continuous_numbering {
                    next_print_number_index()
                } else {
                    state.next_label += 1;
                    state.next_label
                };
                state.number_table.insert(key, -label);
            }
            // Already labeled or already seen multiple times -- skip children
            continue;
        }
        if options.print_continuous_numbering && is_uninterned_symbol(&obj) {
            state.number_table.insert(key, -next_print_number_index());
            continue;
        }
        // First time seen -- mark and process children
        state.number_table.insert(key, 0);
        match obj.kind() {
            ValueKind::Cons => {
                let pair_car = obj.cons_car();
                let pair_cdr = obj.cons_cdr();
                // Push cdr first so car is processed first (stack is LIFO)
                stack.push(pair_cdr);
                stack.push(pair_car);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = obj.as_vector_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let items = obj.as_record_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                if let Some(slots) = char_table_external_slots(&obj) {
                    stack.extend(slots.into_iter().rev());
                }
            }
            ValueKind::Veclike(VecLikeType::SubCharTable) => {
                if let Some((_, _, slots)) = super::chartable::sub_char_table_external_slots(&obj) {
                    stack.extend(slots.into_iter().rev());
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let table = obj.as_hash_table().unwrap().clone();
                for (_, entry) in table.data.keyed_entries_in_slot_order().into_iter().rev() {
                    stack.push(entry.value);
                    stack.push(entry.key);
                }
            }
            ValueKind::Veclike(VecLikeType::Obarray) => {
                if let Some(obarray) = obj.as_obarray_obj() {
                    for item in obarray.buckets.iter().rev() {
                        stack.push(*item);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Lambda) => {
                if let Some(slots) = obj.closure_slots() {
                    stack.extend(slots.iter().rev().copied());
                }
            }
            ValueKind::Veclike(VecLikeType::Macro) => {
                if let Some(doc) = obj.closure_doc_value() {
                    stack.push(doc);
                }
                if let Some(env) = obj.closure_env().flatten() {
                    stack.push(env);
                }
                if let Some(body) = obj.closure_body_value() {
                    stack.push(body);
                }
                if let Some(params) = obj.closure_params() {
                    stack.push(crate::emacs_core::builtins::lambda_params_to_value(params));
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let _ = with_bytecode_literal_slots(&obj, |slots| {
                    for item in slots.iter().rev() {
                        stack.push(*item);
                    }
                });
            }
            ValueKind::String => push_string_text_property_plists(obj, &mut stack),
            _ => {}
        }
    }
    // Remove entries seen only once
    state.number_table.retain(|_, v| *v != 0);
}

fn print_preprocess_external(value: &Value, table_value: Value, options: PrintOptions) {
    print_preprocess_external_with_t_removal(value, table_value, options, true);
}

/// Core of GNU `print_preprocess`. When `remove_t_entries` is true the
/// neomacs printer path strips the transient `t` status entries afterwards
/// (they are cosmetic — the printer only acts on fixnum/string labels). The
/// `print--preprocess` primitive passes `false` so the user-visible
/// `print-number-table` keeps every traversed candidate, exactly like GNU.
fn print_preprocess_external_with_t_removal(
    value: &Value,
    table_value: Value,
    options: PrintOptions,
    remove_t_entries: bool,
) {
    let mut stack: Vec<Value> = vec![*value];
    while let Some(obj) = stack.pop() {
        if !is_print_circle_candidate(&obj, options.print_gensym) {
            continue;
        }
        let Some((key, entry)) = get_print_number_table_entry(table_value, &obj) else {
            continue;
        };
        if !entry.is_nil() || (options.print_continuous_numbering && is_uninterned_symbol(&obj)) {
            if print_number_entry_is_symbol(entry) {
                let label = next_print_number_index();
                put_print_number_table_entry(table_value, key, obj, Value::fixnum(-label));
            }
            continue;
        }

        put_print_number_table_entry(table_value, key, obj, Value::T);
        match obj.kind() {
            ValueKind::Cons => {
                let pair_car = obj.cons_car();
                let pair_cdr = obj.cons_cdr();
                if !pair_cdr.is_nil() {
                    stack.push(pair_cdr);
                }
                stack.push(pair_car);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = obj.as_vector_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let items = obj.as_record_data().unwrap().clone();
                for item in items.iter().rev() {
                    stack.push(*item);
                }
            }
            ValueKind::Veclike(VecLikeType::CharTable) => {
                if let Some(slots) = char_table_external_slots(&obj) {
                    stack.extend(slots.into_iter().rev());
                }
            }
            ValueKind::Veclike(VecLikeType::SubCharTable) => {
                if let Some((_, _, slots)) = super::chartable::sub_char_table_external_slots(&obj) {
                    stack.extend(slots.into_iter().rev());
                }
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let table = obj.as_hash_table().unwrap().clone();
                for (_, entry) in table.data.keyed_entries_in_slot_order().into_iter().rev() {
                    stack.push(entry.value);
                    stack.push(entry.key);
                }
            }
            ValueKind::Veclike(VecLikeType::Obarray) => {
                if let Some(obarray) = obj.as_obarray_obj() {
                    for item in obarray.buckets.iter().rev() {
                        stack.push(*item);
                    }
                }
            }
            ValueKind::Veclike(VecLikeType::Lambda) => {
                if let Some(slots) = obj.closure_slots() {
                    stack.extend(slots.iter().rev().copied());
                }
            }
            ValueKind::Veclike(VecLikeType::Macro) => {
                if let Some(doc) = obj.closure_doc_value() {
                    stack.push(doc);
                }
                if let Some(env) = obj.closure_env().flatten() {
                    stack.push(env);
                }
                if let Some(body) = obj.closure_body_value() {
                    stack.push(body);
                }
                if let Some(params) = obj.closure_params() {
                    stack.push(crate::emacs_core::builtins::lambda_params_to_value(params));
                }
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let _ = with_bytecode_literal_slots(&obj, |slots| {
                    for item in slots.iter().rev() {
                        stack.push(*item);
                    }
                });
            }
            ValueKind::String => push_string_text_property_plists(obj, &mut stack),
            _ => {}
        }
    }

    if remove_t_entries {
        remove_print_number_table_t_entries(table_value);
    }
}

fn push_string_text_property_plists(value: Value, stack: &mut Vec<Value>) {
    if let Some(runs) = get_string_text_properties_for_value(value) {
        for run in runs.into_iter().rev() {
            stack.push(run.plist);
        }
    }
}

/// Public preprocessing entry point used by the `print--preprocess` builtin.
///
/// Mirrors GNU `print_preprocess` (print.c): resets the shared-structure
/// number index and traverses `value`, filling `table_value` (the Lisp
/// `print-number-table` hash) with sharing info so that circular and shared
/// structures can be printed with `#N=` / `#N#` labels.
///
/// `print_gensym` and `print_continuous_numbering` come from the like-named
/// dynamic variables; they affect which symbols are treated as candidates and
/// the gensym special case, exactly as in GNU.
pub(crate) fn preprocess_print_number_table(
    value: &Value,
    table_value: Value,
    print_gensym: bool,
    print_continuous_numbering: bool,
) {
    reset_print_number_index();
    let options = PrintOptions {
        print_circle: true,
        print_gensym,
        print_continuous_numbering,
        ..PrintOptions::default()
    };
    // GNU's `print--preprocess` leaves the transient `t` status entries in the
    // table (the printer simply ignores non-fixnum entries), so don't strip
    // them here.
    print_preprocess_external_with_t_removal(value, table_value, options, false);
}

/// Entry point for stateful printing (circle/level/length aware).
/// Returns the printed representation as a String.
pub(crate) fn print_value_stateful(value: &Value, options: PrintOptions) -> String {
    print_value_stateful_with_buffers(value, None, options)
}

pub(crate) fn print_value_stateful_with_buffers(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
    options: PrintOptions,
) -> String {
    String::from_utf8_lossy(&print_value_stateful_bytes_with_buffers(
        value, buffers, options,
    ))
    .into_owned()
}

pub(crate) fn print_value_stateful_bytes_with_buffers(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
    options: PrintOptions,
) -> Vec<u8> {
    let _print_guard = enter_print_call(&options);
    let mut out = StatefulPrintOutput::default();
    let number_table = active_print_number_table(&options);
    if options.print_circle {
        let mut circle = PrintCircleState::new();
        if let Some(table_value) = number_table {
            print_preprocess_external(value, table_value, options);
        } else {
            print_preprocess(value, &mut circle, options);
        }
        let mut state = PrintState {
            options,
            circle: if number_table.is_some() {
                None
            } else {
                Some(&mut circle)
            },
            context: PrinterContext::Standalone(buffers),
            failure: None,
            settings_source: PrintSettingsSource::Globals,
            callback_number_table: CallbackNumberTable::Captured,
            depth: 0,
            _mutator: std::marker::PhantomData,
            default_print_stack: Vec::new(),
            object_stack: Vec::new(),
        };
        write_value_stateful(value, &mut out, &mut state);
    } else {
        let mut state = PrintState {
            options,
            circle: None,
            context: PrinterContext::Standalone(buffers),
            failure: None,
            settings_source: PrintSettingsSource::Globals,
            callback_number_table: CallbackNumberTable::Captured,
            depth: 0,
            _mutator: std::marker::PhantomData,
            default_print_stack: Vec::new(),
            object_stack: Vec::new(),
        };
        write_value_stateful(value, &mut out, &mut state);
    }
    out.into_bytes()
}

pub(crate) fn default_cycle_candidate_key(value: &Value) -> Option<u64> {
    match value.kind() {
        ValueKind::Cons => object_identity_key(value),
        ValueKind::Veclike(VecLikeType::Vector) => value
            .as_vector_data()
            .is_some_and(|items| !items.is_empty())
            .then(|| object_identity_key(value))
            .flatten(),
        ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::SubCharTable)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::HashTable)
        | ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::Macro)
        | ValueKind::Veclike(VecLikeType::ByteCode) => object_identity_key(value),
        ValueKind::String if string_print_circle_candidate(value) => object_identity_key(value),
        _ => None,
    }
}

fn default_cycle_stack_index(value: &Value, state: &PrintState) -> Option<usize> {
    if state.circle.is_some() {
        return None;
    }
    let key = default_cycle_candidate_key(value)?;
    state.object_stack.iter().position(|entry| *entry == key)
}

fn push_default_cycle_object(value: &Value, state: &mut PrintState) -> bool {
    if state.circle.is_some() {
        return false;
    }
    let Some(key) = default_cycle_candidate_key(value) else {
        return false;
    };
    state.object_stack.push(key);
    true
}

fn with_default_cycle_guard(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
    render: impl FnOnce(&mut StatefulPrintOutput, &mut PrintState),
) {
    if !state.options.print_circle {
        render(out, state);
        return;
    }

    if let Some(index) = default_cycle_stack_index(value, state) {
        write!(out, "#{index}").unwrap();
        return;
    }
    let mut stack_guard = PrintTraversalStackGuard::new(state);
    push_default_cycle_object(value, stack_guard.state());
    render(out, stack_guard.state());
}

/// Scoped per-invocation traversal prefixes. No callback can access the
/// private guard or retain its exclusive state borrow. Drop only truncates
/// Rust vectors of identity integers and cannot run Lisp or allocate.
#[must_use = "dropping the guard restores the traversal prefixes"]
struct PrintTraversalStackGuard<'scope, 'context> {
    state: &'scope mut PrintState<'context>,
    default_len: usize,
    object_len: usize,
}
static_assertions::assert_not_impl_any!(PrintTraversalStackGuard<'static, 'static>: Send, Sync);
impl std::fmt::Debug for PrintTraversalStackGuard<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintTraversalStackGuard")
            .field("default_len", &self.default_len)
            .field("object_len", &self.object_len)
            .finish()
    }
}
impl<'scope, 'context> PrintTraversalStackGuard<'scope, 'context> {
    fn new(state: &'scope mut PrintState<'context>) -> Self {
        let default_len = state.default_print_stack.len();
        let object_len = state.object_stack.len();
        Self {
            state,
            default_len,
            object_len,
        }
    }
    fn state(&mut self) -> &mut PrintState<'context> {
        self.state
    }
}
impl Drop for PrintTraversalStackGuard<'_, '_> {
    fn drop(&mut self) {
        self.state.default_print_stack.truncate(self.default_len);
        self.state.object_stack.truncate(self.object_len);
    }
}

/// Core stateful print routine. Writes the printed representation of `value`
/// into `out`, respecting print-circle, print-level, and print-length.
fn write_value_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    let roots = PrintValueRootsGuard::new(state, std::iter::once(*value));
    write_value_stateful_rooted(value, out, roots.state);
}

fn validate_live_number_table(
    value: &Value,
    state: &PrintState,
) -> Result<(), crate::emacs_core::error::Flow> {
    if state.options.print_circle {
        match state.callback_number_table {
            CallbackNumberTable::Captured => {}
            CallbackNumberTable::Live(table) => {
                if !table.is_hash_table()
                    && is_print_circle_candidate(value, state.options.print_gensym)
                {
                    return Err(crate::emacs_core::error::signal(
                        crate::emacs_core::error::LispCondition::WrongTypeArgument,
                        vec![Value::symbol("hash-table-p"), table],
                    ));
                }
            }
        }
    }
    Ok(())
}

fn write_value_stateful_rooted(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    if state.failure.is_some() {
        return;
    }
    if let Err(error) = validate_live_number_table(value, state) {
        state.failure = Some(error);
        return;
    }
    if !state.options.print_circle {
        // GNU print.c:2285-2289 checks depth before its cycle-reference scan.
        if state.depth >= 200 {
            state.failure = Some(crate::emacs_core::error::signal(
                crate::emacs_core::error::LispCondition::Error,
                vec![Value::string("Apparently circular structure being printed")],
            ));
            return;
        }
        let key = default_cycle_candidate_key(value);
        if let Some(key) = key
            && let Some(index) = state
                .default_print_stack
                .iter()
                .position(|entry| *entry == Some(key))
        {
            write!(out, "#{index}").unwrap();
            return;
        }

        let mut stack_guard = PrintTraversalStackGuard::new(state);
        stack_guard.state().default_print_stack.push(key);
        write_value_stateful_inner(value, out, stack_guard.state());
        return;
    }

    write_value_stateful_inner(value, out, state);
}

fn write_value_stateful_inner(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    if unreadable_object(*value) {
        let outcome = match &mut state.context {
            PrinterContext::Standalone(_) => Ok(UnreadableResult::NotCalled),
            PrinterContext::Evaluator(context) => {
                call_unreadable_printer(context, *value, state.options, state.settings_source)
            }
        };
        match outcome {
            Err(error) => {
                state.failure = Some(error);
                return;
            }
            Ok(UnreadableResult::NotCalled) => {}
            Ok(UnreadableResult::Delegated) => refresh_print_options_after_callback(state),
            Ok(UnreadableResult::HandledString(string)) => {
                refresh_print_options_after_callback(state);
                out.extend_from_slice(&princ_string_bytes(
                    &string,
                    state.options.princ_string_output(),
                ));
                return;
            }
            Ok(UnreadableResult::HandledEmpty) => {
                refresh_print_options_after_callback(state);
                return;
            }
        }
        if let PrinterContext::Evaluator(context) = &state.context {
            if !options_buffer_princ(state.options, *value)
                && let Some(handle) = super::error::format_opaque_handle_in_state(
                    &context.buffers,
                    &context.frames,
                    &context.threads,
                    value,
                )
            {
                out.push_str(&handle);
                return;
            }
        }
    }
    if !options_buffer_princ(state.options, *value)
        && let Some(handle) = print_special_handle(value, state.context.buffers())
    {
        out.push_str(&handle);
        return;
    }

    // Circle check: if this object is a circle candidate, handle #N= / #N#
    if is_print_circle_candidate(value, state.options.print_gensym) {
        let mut printed_number_table_prefix = false;
        match write_print_number_table_entry(value, out, &state.options) {
            Some(PrintNumberTableAction::PrintedReference) => return,
            Some(PrintNumberTableAction::PrintedPrefix) => {
                printed_number_table_prefix = true;
            }
            None => {}
        }

        if !printed_number_table_prefix
            && let Some(ref mut circle) = state.circle
            && let Some(key) = object_identity_key(value)
            && let Some(label) = circle.number_table.get_mut(&key)
        {
            if *label < 0 {
                // First occurrence: emit #N= prefix
                write!(out, "#{}=", -(*label)).unwrap();
                *label = -(*label); // flip to positive
            } else if *label > 0 {
                // Subsequent: emit #N# and return
                write!(out, "#{}#", *label).unwrap();
                return;
            }
            // label == 0: not shared, fall through to normal print
        }
    }

    match value.kind() {
        ValueKind::Nil => out.push_str("nil"),
        ValueKind::T => out.push('t'),
        ValueKind::Fixnum(v) => {
            if !(state.options.print_integers_as_characters
                && try_format_integer_as_character(v, !state.options.print_noescape, &mut |c| {
                    out.push(c)
                }))
            {
                write!(out, "{}", v).unwrap();
            }
        }
        ValueKind::Float => out.push_str(&format_float_with_options(value.xfloat(), state.options)),
        ValueKind::Symbol(id) => out.extend_from_slice(&symbol_bytes(id, state.options)),
        ValueKind::String => {
            with_default_cycle_guard(value, out, state, |out, state| {
                let ls = value.as_lisp_string().unwrap();
                if state.options.print_noescape {
                    out.extend_from_slice(&princ_string_bytes(
                        ls,
                        state.options.princ_string_output(),
                    ));
                } else {
                    match printed_string_property_runs(*value, state.options.charset_text_property)
                    {
                        Some(runs) => {
                            let roots =
                                PrintValueRootsGuard::new(state, runs.iter().map(|run| run.plist));
                            let bytes = format_lisp_string_bytes_emacs(ls, &roots.state.options);
                            write_lisp_propertized_string_stateful(
                                &bytes,
                                &runs,
                                out,
                                &mut *roots.state,
                            );
                        }
                        None => out
                            .extend_from_slice(&format_lisp_string_bytes_emacs(ls, &state.options)),
                    }
                }
            });
        }
        ValueKind::Cons => {
            with_default_cycle_guard(value, out, state, |out, state| {
                // Level check for containers
                if let Some(level) = state.options.print_level
                    && state.depth >= level
                {
                    out.push_str("...");
                    return;
                }
                // Try shorthand (quote, function, backquote, etc.)
                if let Some(shorthand) = write_list_shorthand_stateful(value, state) {
                    out.append(shorthand);
                    return;
                }
                let mut depth_guard = PrintDepthGuard::enter(state);
                let state = depth_guard.state();
                out.push('(');
                write_cons_stateful(value, out, state);
                out.push(')');
            });
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            if let Some(slots) = LiteralSlots::new(value) {
                with_default_cycle_guard(value, out, state, |out, state| {
                    out.push_str("#^[");
                    write_vector_slots_stateful(slots, out, state);
                    out.push(']');
                });
            }
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            if let Some((depth, min_char)) = value
                .as_sub_char_table_obj()
                .map(|table| (table.depth, table.min_char))
            {
                with_default_cycle_guard(value, out, state, |out, state| {
                    out.push_str("#^^[");
                    out.push_str(&depth.to_string());
                    out.push(' ');
                    out.push_str(&min_char.to_string());
                    if let Some(slots) = LiteralSlots::new(value) {
                        write_vector_slots_stateful(slots, out, state);
                    }
                    out.push(']');
                });
            }
        }
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            let nbits = bool_vector_length(value).unwrap_or(0);
            out.push_str(&format_bool_vector(value, nbits as usize, state.options));
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            with_default_cycle_guard(value, out, state, |out, state| {
                let mut depth_guard = PrintDepthGuard::enter(state);
                let state = depth_guard.state();
                out.push('[');
                let print_length = state.options.print_length;
                let count = value.as_vector_data().unwrap().len();
                for idx in 0..count {
                    let item = value.as_vector_data().unwrap()[idx];
                    if let Some(length) = print_length
                        && idx as i64 >= length
                    {
                        if idx > 0 {
                            out.push(' ');
                        }
                        out.push_str("...");
                        break;
                    }
                    if idx > 0 {
                        out.push(' ');
                    }
                    write_value_stateful(&item, out, state);
                    if state.failure.is_some() {
                        break;
                    }
                }
                out.push(']');
            });
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            with_default_cycle_guard(value, out, state, |out, state| {
                let mut depth_guard = PrintDepthGuard::enter(state);
                let state = depth_guard.state();
                out.push_str("#s(");
                let print_length = state.options.print_length;
                let count = value.as_record_data().unwrap().len();
                for idx in 0..count {
                    let item = value.as_record_data().unwrap()[idx];
                    if let Some(length) = print_length
                        && idx as i64 >= length
                    {
                        if idx > 0 {
                            out.push(' ');
                        }
                        out.push_str("...");
                        break;
                    }
                    if idx > 0 {
                        out.push(' ');
                    }
                    write_value_stateful(&item, out, state);
                    if state.failure.is_some() {
                        break;
                    }
                }
                out.push(')');
            });
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            with_default_cycle_guard(value, out, state, |out, state| {
                let mut depth_guard = PrintDepthGuard::enter(state);
                let state = depth_guard.state();
                write_hash_table_stateful(value, out, state);
            });
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            let count = value.as_obarray_obj().map_or(0, |obj| obj.count);
            out.push_str(&format!("#<obarray n={count}>"));
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            write_lambda_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            write_macro_stateful(value, out, state);
        }
        ValueKind::Subr(id) => write!(out, "#<subr {}>", resolve_sym(id)).unwrap(),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = value.as_subr_id().unwrap();
            write!(out, "#<subr {}>", resolve_sym(id)).unwrap()
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            write_bytecode_literal_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Marker) => out.push_str(
            &print_special_handle(value, state.context.buffers())
                .unwrap_or_else(|| "#<marker>".to_string()),
        ),
        ValueKind::Veclike(VecLikeType::Overlay) => out.push_str(
            &print_special_handle(value, state.context.buffers())
                .unwrap_or_else(|| "#<overlay>".to_string()),
        ),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let bid = value.as_buffer_id().unwrap();
            if let Some(buffers) = state.context.buffers() {
                if let Some(buf) = buffers.get(bid) {
                    if state.options.print_noescape {
                        if let Some(name) = buf.name_value().as_lisp_string() {
                            out.extend_from_slice(&princ_string_bytes(
                                name,
                                state.options.princ_string_output(),
                            ));
                        }
                    } else {
                        write!(out, "#<buffer {}>", buf.name_runtime_string_owned()).unwrap();
                    }
                } else if buffers.dead_buffer_last_name_value(bid).is_some() {
                    out.push_str("#<killed buffer>");
                } else {
                    write!(out, "#<buffer {}>", bid.0).unwrap();
                }
            } else {
                write!(out, "#<buffer {}>", bid.0).unwrap();
            }
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            let wid = value.as_window_id().unwrap();
            write!(out, "#<window {}>", wid).unwrap();
        }
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => {
            out.push_str("#<window-configuration>");
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            let fid = value.as_frame_id().unwrap();
            out.push_str(&format_frame_handle(fid));
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            let tid = value.as_timer_id().unwrap();
            write!(out, "#<timer {}>", tid).unwrap();
        }
        ValueKind::Veclike(VecLikeType::Process) => {
            out.push_str(
                &super::process::print_process_handle(value)
                    .unwrap_or_else(|| "#<process>".to_string()),
            );
        }
        ValueKind::Veclike(VecLikeType::Terminal) => {
            out.push_str(
                &print_special_handle(value, state.context.buffers())
                    .unwrap_or_else(|| "#<terminal>".to_string()),
            );
        }
        ValueKind::Veclike(VecLikeType::Xwidget) => {
            let xw = value.as_xwidget().unwrap();
            write!(out, "#<xwidget {}>", xw.xwidget_id).unwrap();
        }
        ValueKind::Veclike(VecLikeType::XwidgetView) => {
            out.push_str("#<xwidget-view>");
        }
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
            let id = value.as_surface_handle().unwrap();
            write!(out, "#<neomacs-surface {id}>").unwrap();
        }
        ValueKind::Veclike(VecLikeType::VideoHandle) => {
            let id = value.as_video_handle().unwrap();
            write!(out, "#<neomacs-video {id}>").unwrap();
        }
        ValueKind::Veclike(VecLikeType::Bignum) => {
            // GNU `print_object` formats bignums via `mpz_get_str`
            // (`src/print.c` PRINT_INTEGER branch). `malachite::Integer`'s
            // Display implements the same formatting.
            write!(out, "{}", value.as_bignum().unwrap()).unwrap();
        }
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
            write_symbol_with_pos_stateful(value, out, state);
        }
        ValueKind::Veclike(VecLikeType::Finalizer) => {
            // GNU `print_vectorlike_unreadable` prints finalizers opaquely.
            out.push_str("#<finalizer>");
        }
        ValueKind::Veclike(
            kind @ (VecLikeType::Thread | VecLikeType::Mutex | VecLikeType::CondVar),
        ) => {
            write!(
                out,
                "#<{} {}>",
                threading_handle_print_name(kind),
                value.threading_handle_id().unwrap_or(0)
            )
            .unwrap();
        }
        ValueKind::Veclike(VecLikeType::Sqlite) => {
            let obj = value.as_sqlite().unwrap();
            if obj.is_statement {
                write!(out, "#<sqlite statement {}>", obj.id).unwrap();
            } else {
                write!(out, "#<sqlite db {}>", obj.id).unwrap();
            }
        }
        ValueKind::Veclike(VecLikeType::UserPtr) => {
            out.push_str("#<user-ptr>");
        }
        ValueKind::Veclike(VecLikeType::ModuleFunction) => {
            out.push_str("#<module-function>");
        }
        ValueKind::Veclike(VecLikeType::Font) => out.push_str("#<font-object>"),
        // backtrace--locals can expose GNU's actual noncanonical Qunbound.
        ValueKind::Unbound => {
            out.extend_from_slice(&symbol_bytes(super::intern::UNBOUND_SYM_ID, state.options))
        }
        ValueKind::Unknown => write!(out, "#<unknown {:#x}>", value.0).unwrap(),
    }
}

/// Scratch roots are part of the current mutator's existing root stack.
/// This scope releases them on both normal return and Rust unwinding.
#[derive(Debug)]
#[must_use = "dropping the guard releases temporary bytecode slot roots"]
struct PrintScratchRootsGuard {
    saved: usize,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(PrintScratchRootsGuard: Send, Sync);
impl PrintScratchRootsGuard {
    fn new() -> Self {
        Self {
            saved: crate::emacs_core::eval::save_scratch_gc_roots(),
            _mutator: std::marker::PhantomData,
        }
    }
}
impl Drop for PrintScratchRootsGuard {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.saved);
    }
}

fn with_bytecode_literal_slots<R>(value: &Value, f: impl FnOnce(&[Value]) -> R) -> Option<R> {
    let bc = value.get_bytecode_data()?.clone();
    let _roots = PrintScratchRootsGuard::new();

    let arglist = bc.arglist;
    crate::emacs_core::eval::push_scratch_gc_root(arglist);

    // Slots 1 and 2: the objects Lisp already holds when `aref` created
    // them (so `print-circle` sees the sharing, as in GNU), else a temporary
    // equal copy: with no object created, no Lisp reference can share it.
    use crate::tagged::header::ByteCodeSlotObject;
    let code = if let Some(code) = value.bytecode_slot_object_if_created(ByteCodeSlotObject::Code) {
        code
    } else if let Some(bytes) = &bc.gnu_bytecode_bytes {
        Value::heap_string(crate::heap_types::LispString::from_unibyte(
            bytes.as_slice().to_vec(),
        ))
    } else {
        Value::NIL
    };
    crate::emacs_core::eval::push_scratch_gc_root(code);

    let constants = if let Some(env) = bc.env {
        env
    } else if let Some(constants) =
        value.bytecode_slot_object_if_created(ByteCodeSlotObject::Constants)
    {
        constants
    } else {
        Value::vector(bc.constants.as_slice().to_vec())
    };
    crate::emacs_core::eval::push_scratch_gc_root(constants);

    let depth = bc.max_stack.value();
    let doc = bc
        .doc_form
        .or_else(|| bc.docstring.as_ref().map(|d| Value::heap_string(d.clone())))
        .unwrap_or(Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(doc);

    let interactive = bc.interactive.unwrap_or(Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(interactive);

    let slot_count = bc.observable_closure_slot_count();
    let mut slots = vec![arglist, code, constants, depth];
    if slot_count > 4 {
        slots.push(doc);
    }
    if slot_count > 5 {
        slots.push(interactive);
    }
    if slot_count > 6 {
        let extra_count = slot_count - 6;
        for idx in 0..extra_count {
            let extra = bc.extra_slots.get(idx).copied().unwrap_or(Value::NIL);
            crate::emacs_core::eval::push_scratch_gc_root(extra);
            slots.push(extra);
        }
    }
    let result = f(&slots);
    Some(result)
}

fn write_bytecode_literal_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    with_default_cycle_guard(value, out, state, |out, state| {
        let _ = with_bytecode_literal_slots(value, |slots| {
            let mut depth_guard = PrintDepthGuard::enter(state);
            let state = depth_guard.state();
            out.push_str("#[");
            let print_length = state.options.print_length;
            for (idx, item) in slots.iter().enumerate() {
                if let Some(length) = print_length
                    && idx as i64 >= length
                {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str("...");
                    break;
                }
                if idx > 0 {
                    out.push(' ');
                }
                write_value_stateful(item, out, state);
                if state.failure.is_some() {
                    break;
                }
            }
            out.push(']');
        });
    });
}

fn write_closure_body_forms_stateful(
    body: Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let Some(forms) = list_to_vec(&body) else {
        write_value_stateful(&body, out, state);
        return;
    };
    let roots = PrintValueRootsGuard::new(state, forms.iter().copied());
    let state = &mut *roots.state;
    if forms.is_empty() {
        out.push_str("nil");
    } else {
        for (idx, form) in forms.iter().enumerate() {
            if idx > 0 {
                out.push(' ');
            }
            write_value_stateful(form, out, state);
            if state.failure.is_some() {
                return;
            }
        }
    }
}

/// GNU lisp.h:2098-2120, 2248-2259: depth and min_char are C ints,
/// whose combined storage consumes one Lisp_Object slot on 64-bit targets
/// and two on 32-bit targets. Printed scalar count is not the slot offset.
const SUB_CHAR_TABLE_HEADER_SLOTS: usize =
    (2 * std::mem::size_of::<libc::c_int>()).div_ceil(std::mem::size_of::<usize>());
const _: () = {
    assert!((2 * std::mem::size_of::<libc::c_int>()) % std::mem::size_of::<usize>() == 0);
    assert!(std::mem::size_of::<Value>() == std::mem::size_of::<usize>());
    assert!(SUB_CHAR_TABLE_HEADER_SLOTS == 1 || SUB_CHAR_TABLE_HEADER_SLOTS == 2);
};

/// Exclusive access to one invocation's printer state while a child object is
/// printed. Restores depth on early return, Lisp callback failure, or unwind.
/// Threading: the borrow remains on the owning mutator; no state is shared.
#[must_use = "printer state is restored when this scope ends"]
struct PrintDepthGuard<'scope, 'context> {
    state: &'scope mut PrintState<'context>,
    saved_depth: i64,
    _mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(PrintDepthGuard<'static, 'static>: Send, Sync);

impl<'scope, 'context> PrintDepthGuard<'scope, 'context> {
    fn enter(state: &'scope mut PrintState<'context>) -> Self {
        let saved_depth = state.depth;
        state.depth += 1;
        Self {
            state,
            saved_depth,
            _mutator: std::marker::PhantomData,
        }
    }

    fn state(&mut self) -> &mut PrintState<'context> {
        self.state
    }
}

impl std::fmt::Debug for PrintDepthGuard<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintDepthGuard")
            .field("saved_depth", &self.saved_depth)
            .finish_non_exhaustive()
    }
}

impl Drop for PrintDepthGuard<'_, '_> {
    fn drop(&mut self) {
        self.state.depth = self.saved_depth;
    }
}

/// GNU-readable pseudovector slot source. The constructor validates its owner,
/// and the source selects its GNU storage offset, so sub-char-table scalar
/// metadata consumes one pointer-sized slot on 64-bit targets, two on 32-bit
/// targets, even though the metadata prints as two numbers (lisp.h:2248-2259).
/// Threading: this borrowed descriptor is confined to the calling mutator.
#[derive(Clone, Copy, Debug)]
enum LiteralSlotSource<'value> {
    CharTable(&'value Value),
    SubCharTable(&'value Value),
    Closure(&'value Value),
}

#[derive(Debug)]
struct LiteralSlots<'value> {
    source: LiteralSlotSource<'value>,
    _mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(LiteralSlots<'static>: Send, Sync);

impl<'value> LiteralSlots<'value> {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn new(value: &'value Value) -> Option<Self> {
        let source = match value.kind() {
            ValueKind::Veclike(kind) => match kind {
                VecLikeType::CharTable => LiteralSlotSource::CharTable(value),
                VecLikeType::SubCharTable => LiteralSlotSource::SubCharTable(value),
                VecLikeType::Lambda => LiteralSlotSource::Closure(value),
                VecLikeType::Vector
                | VecLikeType::Bignum
                | VecLikeType::SymbolWithPos
                | VecLikeType::BoolVector
                | VecLikeType::HashTable
                | VecLikeType::Record
                | VecLikeType::Macro
                | VecLikeType::ByteCode
                | VecLikeType::Marker
                | VecLikeType::Overlay
                | VecLikeType::Finalizer
                | VecLikeType::UserPtr
                | VecLikeType::Process
                | VecLikeType::Frame
                | VecLikeType::Window
                | VecLikeType::Buffer
                | VecLikeType::Obarray
                | VecLikeType::Terminal
                | VecLikeType::WindowConfiguration
                | VecLikeType::Subr
                | VecLikeType::Xwidget
                | VecLikeType::XwidgetView
                | VecLikeType::Thread
                | VecLikeType::Mutex
                | VecLikeType::CondVar
                | VecLikeType::ModuleFunction
                | VecLikeType::Sqlite
                | VecLikeType::Font
                | VecLikeType::Timer
                | VecLikeType::SurfaceHandle
                | VecLikeType::VideoHandle => return None,
            },
            ValueKind::Nil
            | ValueKind::T
            | ValueKind::Fixnum(_)
            | ValueKind::Float
            | ValueKind::Symbol(_)
            | ValueKind::String
            | ValueKind::Cons
            | ValueKind::Subr(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => return None,
        };
        Some(Self {
            source,
            _mutator: std::marker::PhantomData,
        })
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn header_slots(&self) -> usize {
        match self.source {
            LiteralSlotSource::CharTable(_) | LiteralSlotSource::Closure(_) => 0,
            LiteralSlotSource::SubCharTable(_) => SUB_CHAR_TABLE_HEADER_SLOTS,
        }
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn len(&self) -> usize {
        match self.source {
            LiteralSlotSource::CharTable(owner) => owner.as_char_table_obj().map_or(0, |table| {
                [
                    table.defalt.get(),
                    table.parent.get(),
                    table.purpose.get(),
                    table.ascii.get(),
                ]
                .len()
                    + table.contents.len()
                    + table.extras.len()
            }),
            LiteralSlotSource::SubCharTable(owner) => owner
                .as_sub_char_table_obj()
                .map_or(0, |table| table.contents.len()),
            LiteralSlotSource::Closure(owner) => {
                owner.closure_slots().map_or(0, |slots| slots.len())
            }
        }
    }

    /// Copy one live slot before calling Lisp. No vector/slice borrow survives
    /// a callback, which may change slots and collect the displaced children.
    #[deny(clippy::wildcard_enum_match_arm)]
    fn slot(&self, index: usize) -> Option<Value> {
        match self.source {
            LiteralSlotSource::CharTable(owner) => {
                let table = owner.as_char_table_obj()?;
                let header = [
                    table.defalt.get(),
                    table.parent.get(),
                    table.purpose.get(),
                    table.ascii.get(),
                ];
                if let Some(value) = header.get(index) {
                    return Some(*value);
                }
                let index = index.checked_sub(header.len())?;
                if let Some(value) = table.contents.get(index) {
                    return Some(value.get());
                }
                table
                    .extras
                    .get(index.checked_sub(table.contents.len())?)
                    .copied()
            }
            LiteralSlotSource::SubCharTable(owner) => {
                owner.as_sub_char_table_obj()?.contents.get(index).copied()
            }
            LiteralSlotSource::Closure(owner) => owner.closure_slots()?.get(index).copied(),
        }
    }
}

/// GNU print.c:2250-2265 and 2611-2628: readable pseudovectors share slot
/// traversal, print-length, and child depth. Slots are fetched afresh after
/// callbacks instead of borrowing heap vectors across evaluator mutation.
fn write_vector_slots_stateful(
    slots: LiteralSlots<'_>,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let mut depth_guard = PrintDepthGuard::enter(state);
    let state = depth_guard.state();
    let offset = slots.header_slots();
    let limit = state
        .options
        .print_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or(usize::MAX);
    let slot_count = slots.len();
    let count = slot_count.min(limit.saturating_sub(offset));
    for index in 0..count {
        let Some(slot) = slots.slot(index) else {
            break;
        };
        if index + offset > 0 {
            out.push(' ');
        }
        write_value_stateful(&slot, out, state);
        if state.failure.is_some() {
            return;
        }
    }
    if count < slot_count {
        if count + offset > 0 {
            out.push(' ');
        }
        out.push_str("...");
    }
}

fn write_interpreted_closure_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    out.push_str("#[");
    if let Some(slots) = LiteralSlots::new(value) {
        write_vector_slots_stateful(slots, out, state);
    }
    out.push(']');
}

fn write_lambda_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    // GNU print.c:1324, 2611-2613: closure recursion belongs to this
    // invocation's graph/depth state, not another printer activation's TLS.
    with_default_cycle_guard(value, out, state, |out, state| {
        write_interpreted_closure_stateful(value, out, state);
    });
}

fn write_macro_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    with_default_cycle_guard(value, out, state, |out, state| {
        out.push_str("(macro ");
        write_params_stateful(value.closure_params(), out, state);
        if state.failure.is_some() {
            return;
        }
        out.push(' ');
        if let Some(body) = value.closure_body_value() {
            write_closure_body_forms_stateful(body, out, state);
        } else {
            out.push_str("nil");
        }
        out.push(')');
    });
}

fn append_bytecode_literal_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    if with_bytecode_literal_slots(value, |slots| {
        out.extend_from_slice(b"#[");
        for (idx, item) in slots.iter().enumerate() {
            if idx > 0 {
                out.push(b' ');
            }
            append_print_value_bytes(item, out, options);
        }
        out.push(b']');
    })
    .is_none()
    {
        out.extend_from_slice(b"#<bytecode>");
    }
}

#[inline]
fn symbol_id_is(id: SymId, name: &str) -> bool {
    id == intern(name)
}

#[inline]
fn value_is_symbol_named(value: &Value, name: &str) -> bool {
    matches!(value.kind(), ValueKind::Symbol(id) if symbol_id_is(id, name))
}

#[derive(Clone, Copy, Debug)]
enum SymbolShorthand {
    Quote,
    Function,
    Backquote,
    Comma,
    CommaAt,
}

fn symbol_shorthand(id: SymId) -> Option<SymbolShorthand> {
    if symbol_id_is(id, "quote") {
        Some(SymbolShorthand::Quote)
    } else if symbol_id_is(id, "function") {
        Some(SymbolShorthand::Function)
    } else if symbol_id_is(id, "`") {
        Some(SymbolShorthand::Backquote)
    } else if symbol_id_is(id, ",") {
        Some(SymbolShorthand::Comma)
    } else if symbol_id_is(id, ",@") {
        Some(SymbolShorthand::CommaAt)
    } else {
        None
    }
}

/// Scoped shorthand syntax and nesting on an invocation's exclusive printer
/// state. GNU print.c:2533-2568 retains depth for quote/function and increases
/// it for backquote/comma; restoration is guaranteed on every exit.
/// Threading: all references remain on the calling mutator.
#[must_use = "printer state is restored when this scope ends"]
struct PrintShorthandGuard<'scope, 'context> {
    state: &'scope mut PrintState<'context>,
    saved_backquote_output_level: usize,
    saved_depth: i64,
    _mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(PrintShorthandGuard<'static, 'static>: Send, Sync);

impl<'scope, 'context> PrintShorthandGuard<'scope, 'context> {
    fn enter(
        state: &'scope mut PrintState<'context>,
        options: PrintOptions,
        shorthand: SymbolShorthand,
    ) -> Self {
        let saved_backquote_output_level = state.options.backquote_output_level;
        let saved_depth = state.depth;
        state.options = options;
        state.depth += match shorthand {
            SymbolShorthand::Quote | SymbolShorthand::Function => 0,
            SymbolShorthand::Backquote | SymbolShorthand::Comma | SymbolShorthand::CommaAt => 1,
        };
        Self {
            state,
            saved_backquote_output_level,
            saved_depth,
            _mutator: std::marker::PhantomData,
        }
    }

    fn state(&mut self) -> &mut PrintState<'context> {
        self.state
    }
}

impl std::fmt::Debug for PrintShorthandGuard<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintShorthandGuard")
            .field("saved_depth", &self.saved_depth)
            .field(
                "saved_backquote_output_level",
                &self.saved_backquote_output_level,
            )
            .finish_non_exhaustive()
    }
}

impl Drop for PrintShorthandGuard<'_, '_> {
    fn drop(&mut self) {
        self.state.options.backquote_output_level = self.saved_backquote_output_level;
        self.state.depth = self.saved_depth;
    }
}

/// Try to produce a shorthand form (quote, function, backquote, etc.) using
/// stateful printing. Returns `Some(string)` on success.
fn write_list_shorthand_stateful(
    value: &Value,
    state: &mut PrintState,
) -> Option<StatefulPrintOutput> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    let head = match items[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => return None,
    };

    if symbol_id_is(head, "make-hash-table-from-literal") {
        if let Some(payload) = quote_payload_stateful(&items[1]) {
            let mut out = StatefulPrintOutput::from_str("#s");
            write_value_stateful(&payload, &mut out, state);
            return Some(out);
        }
        return None;
    }

    // GNU print.c:2533-2568 gates every quote/function/backquote/comma
    // abbreviation on the live flag, including callback option refresh.
    if !state.options.print_quoted {
        return None;
    }
    let shorthand = symbol_shorthand(head)?;
    let (prefix, nested_options) = match shorthand {
        SymbolShorthand::Quote => ("'", state.options),
        SymbolShorthand::Function => ("#'", state.options),
        SymbolShorthand::Backquote => ("`", state.options.enter_backquote()),
        SymbolShorthand::Comma => {
            if !state.options.allow_unquote_shorthand() {
                return None;
            }
            (",", state.options.exit_backquote())
        }
        SymbolShorthand::CommaAt => {
            if !state.options.allow_unquote_shorthand() {
                return None;
            }
            (",@", state.options.exit_backquote())
        }
    };

    let mut scope = PrintShorthandGuard::enter(state, nested_options, shorthand);
    let mut out = StatefulPrintOutput::from_str(prefix);
    write_value_stateful(&items[1], &mut out, scope.state());
    Some(out)
}

fn quote_payload_stateful(value: &Value) -> Option<Value> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    value_is_symbol_named(&items[0], "quote").then_some(items[1])
}

/// Print a cons cell (list elements) with stateful print support.
fn write_cons_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = state.options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let mut stack_guard = PrintTraversalStackGuard::new(state);
    let state = stack_guard.state();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                let roots = PrintValueRootsGuard::new(state, [cursor, tortoise]);
                let state = &mut *roots.state;
                if first {
                    if maxlen == 0 {
                        out.push_str("...");

                        return;
                    }
                } else {
                    out.push(' ');

                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.push_str("...");

                        return;
                    }

                    if state.circle.is_none() {
                        n -= 1;
                        if n == 0 {
                            tortoise_idx = tortoise_idx.saturating_add(m);
                            m = m.saturating_mul(2);
                            n = m;
                            tortoise = cursor;
                        } else if super::value::eq_value(&cursor, &tortoise) {
                            write!(out, ". #{tortoise_idx}").unwrap();

                            return;
                        }
                    }
                }
                let pair_car = cursor.cons_car();

                // Circle check on the cdr (for detecting shared tails)
                // But first, print the car
                write_value_stateful(&pair_car, out, state);
                if state.failure.is_some() {
                    return;
                }
                cursor = cursor.cons_cdr();
                first = false;

                // Check if cdr is a cons that has a circle label
                if cursor.is_cons() {
                    if let Err(error) = validate_live_number_table(&cursor, state) {
                        state.failure = Some(error);

                        return;
                    }
                    if let Some(table_value) = active_print_number_table(&state.options) {
                        if let Some((_key, entry)) =
                            get_print_number_table_entry(table_value, &cursor)
                            && print_number_entry_is_cdr_label(entry)
                        {
                            out.push_str(" . ");
                            write_value_stateful(&cursor, out, state);
                            return;
                        }
                    } else if let Some(ref circle) = state.circle
                        && let Some(key) = object_identity_key(&cursor)
                        && let Some(label) = circle.number_table.get(&key)
                        && *label != 0
                    {
                        // This cons is shared/circular -- print as dotted pair
                        out.push_str(" . ");
                        write_value_stateful(&cursor, out, state);
                        return;
                    }
                }
            }
            ValueKind::Nil => {
                return;
            }
            _ => {
                if !first {
                    out.push_str(" . ");
                }
                write_value_stateful(&cursor, out, state);

                return;
            }
        }
    }
}

/// A checked physical hash entry slot, selected by a forward cursor. Its
/// position remains paired with the table that validated it, so a slot from
/// one table cannot accidentally fetch a value from another table.
/// Threading: this borrowed owner descriptor remains on the calling mutator.
#[derive(Clone, Copy, Debug)]
struct HashPrintSlot<'owner> {
    owner: &'owner Value,
    index: usize,
    _mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(HashPrintSlot<'static>: Send, Sync);

impl HashPrintSlot<'_> {
    /// GNU print.c:2826-2836 fetches the value AFTER the key's printer returns.
    /// A callback can replace/reuse this slot; removal clears its value to nil
    /// (GNU fns.c:5185), which is represented by an absent storage entry here.
    fn value(self) -> Value {
        self.owner
            .as_hash_table()
            .and_then(|table| table.entry_at(self.index))
            .map_or(Value::NIL, |entry| entry.value)
    }
}

/// GNU print.c:2820-2836: skip vacant physical slots, emit the live key and
/// then refetch its value from the same slot. Never retain a storage borrow
/// across a Lisp callback or perform a key lookup that could call user code.
/// Threading: only a scalar cursor is retained; each owner borrow is local.
#[derive(Debug, Default)]
struct HashPrintCursor {
    next_slot: usize,
}

impl HashPrintCursor {
    fn next_key<'owner>(&mut self, owner: &'owner Value) -> Option<(HashPrintSlot<'owner>, Value)> {
        let table = owner.as_hash_table()?;
        while self.next_slot < table.entry_slot_count() {
            let slot = HashPrintSlot {
                owner,
                index: self.next_slot,
                _mutator: std::marker::PhantomData,
            };
            self.next_slot += 1;
            if let Some(entry) = table.entry_at(slot.index) {
                return Some((slot, entry.key));
            }
        }
        None
    }
}

/// GNU print.c:2630-2672 captures the initial number of pairs and truncation;
/// PE_hash (2803-2836) fetches keys and values live after earlier callbacks.
fn write_hash_table_stateful(value: &Value, out: &mut StatefulPrintOutput, state: &mut PrintState) {
    let Some((test_name, weakness, initial_count)) = value.as_hash_table().map(|table| {
        (
            hash_table_printed_test_name(table),
            table.weakness,
            table.data.len(),
        )
    }) else {
        return;
    };
    out.push_str("#s(hash-table");
    if let Some(name) = test_name {
        out.push_str(" test ");
        write_value_stateful(&name, out, state);
        if state.failure.is_some() {
            return;
        }
    }
    if let Some(weakness) = weakness {
        out.push_str(" weakness ");
        out.push_str(weakness.name());
    }
    if initial_count != 0 {
        out.push_str(" data (");
        let count = state
            .options
            .print_length
            .and_then(|length| usize::try_from(length).ok())
            .map_or(initial_count, |limit| initial_count.min(limit));
        let mut cursor = HashPrintCursor::default();
        let mut printed = 0;
        for _ in 0..count {
            let Some((slot, key)) = cursor.next_key(value) else {
                break;
            };
            if printed != 0 {
                out.push(' ');
            }
            // The callback may remove the key from the table and collect it;
            // the copied current key remains rooted for the whole pair.
            let roots = PrintValueRootsGuard::new(state, std::iter::once(key));
            let state = &mut *roots.state;
            write_value_stateful(&key, out, state);
            if state.failure.is_some() {
                return;
            }
            out.push(' ');
            let element = slot.value();
            write_value_stateful(&element, out, state);
            if state.failure.is_some() {
                return;
            }
            printed += 1;
        }
        if count < initial_count {
            if printed != 0 {
                out.push(' ');
            }
            out.push_str("...");
        }
        out.push(')');
    }
    out.push(')');
}

thread_local! {
    static PRINT_OBJECT_STACK: RefCell<Vec<PrintObjectRef>> = const { RefCell::new(Vec::new()) };
    static PRINT_BYTES_OBJECT_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrintObjectRef {
    Lambda(usize),
}

/// Owns one prefix of the existing thread-local closure recursion stack.
/// The private constructor releases its RefCell borrow before returning;
/// rendering and guard Drop never overlap a borrow of that RefCell. Drop only
/// truncates copyable identity entries, with no allocation or Lisp callbacks.
#[derive(Debug)]
#[must_use = "dropping the guard restores the closure recursion stack"]
struct PrintObjectStackGuard {
    saved: usize,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(PrintObjectStackGuard: Send, Sync);
impl PrintObjectStackGuard {
    fn enter(object: PrintObjectRef) -> Result<Self, usize> {
        PRINT_OBJECT_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(index) = stack.iter().position(|entry| *entry == object) {
                return Err(index);
            }
            let saved = stack.len();
            stack.push(object);
            Ok(Self {
                saved,
                _mutator: std::marker::PhantomData,
            })
        })
    }
}
impl Drop for PrintObjectStackGuard {
    fn drop(&mut self) {
        let _ = PRINT_OBJECT_STACK.try_with(|stack| {
            if let Ok(mut stack) = stack.try_borrow_mut() {
                stack.truncate(self.saved);
            }
        });
    }
}

fn with_print_object_guard<R>(
    object: PrintObjectRef,
    on_cycle: impl FnOnce(usize) -> R,
    render: impl FnOnce() -> R,
) -> R {
    match PrintObjectStackGuard::enter(object) {
        Ok(_guard) => render(),
        Err(index) => on_cycle(index),
    }
}

fn append_bytes_cycle_ref_if_any(value: &Value, out: &mut Vec<u8>) -> bool {
    let Some(key) = default_cycle_candidate_key(value) else {
        return false;
    };
    PRINT_BYTES_OBJECT_STACK.with(|stack| {
        if let Some(index) = stack.borrow().iter().position(|entry| *entry == key) {
            out.extend_from_slice(format!("#{index}").as_bytes());
            true
        } else {
            false
        }
    })
}

/// Owns one traversal prefix of the existing synchronous byte-printer stack.
/// Its constructor releases the RefCell borrow before rendering, and Drop only
/// truncates copyable identities after all child borrows have ended. The guard
/// is confined to its owning mutator thread and introduces no Lisp-state cache.
#[derive(Debug)]
#[must_use = "dropping the guard restores the byte-printer traversal prefix"]
struct BytePrintObjectStackGuard {
    saved: usize,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(BytePrintObjectStackGuard: Send, Sync);

impl BytePrintObjectStackGuard {
    fn enter(value: &Value) -> Option<Self> {
        let key = default_cycle_candidate_key(value)?;
        let saved = PRINT_BYTES_OBJECT_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let saved = stack.len();
            stack.push(key);
            saved
        });
        Some(Self {
            saved,
            _mutator: std::marker::PhantomData,
        })
    }
}

impl Drop for BytePrintObjectStackGuard {
    fn drop(&mut self) {
        let _ = PRINT_BYTES_OBJECT_STACK.try_with(|stack| {
            if let Ok(mut stack) = stack.try_borrow_mut() {
                stack.truncate(self.saved);
            }
        });
    }
}

fn format_marker_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    if !super::marker::is_marker(value) {
        return None;
    }

    if !value.is_marker() {
        return None;
    };
    let marker = value.as_marker_data().unwrap().clone();
    let buffer_name = marker
        .buffer
        .and_then(|buffer_id| buffers.and_then(|manager| manager.get(buffer_id)))
        .map(|buffer| buffer.name_runtime_string_owned());

    let mut out = String::from("#<marker ");
    if marker.insertion_type {
        out.push_str("(moves after insertion) ");
    }
    // T7: read authoritative charpos (1-based Lisp shape). A marker with
    // no buffer prints as "in no buffer"; otherwise include its current
    // chain-tracked position.
    if let Some(name) = buffer_name.as_deref() {
        out.push_str(&format!("at {} in {}", marker.charpos + 1, name));
    } else {
        out.push_str("in no buffer");
    }
    out.push('>');
    Some(out)
}

fn format_overlay_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    if !value.is_overlay() {
        return None;
    };

    let overlay = value.as_overlay_data().unwrap();
    let Some(buffer_id) = overlay.buffer else {
        return Some("#<overlay in no buffer>".to_string());
    };
    let (start, end) = overlay.current_range();

    let Some(buffers) = buffers else {
        return Some(format!("#<overlay from {} to {}>", start, end));
    };

    let Some(buffer) = buffers.get(buffer_id) else {
        return Some("#<overlay in no buffer>".to_string());
    };

    Some(format!(
        "#<overlay from {} to {} in {}>",
        buffer
            .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start))
            .as_i64(),
        buffer
            .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end))
            .as_i64(),
        buffer.name_runtime_string_owned()
    ))
}

fn print_special_handle(
    value: &Value,
    buffers: Option<&crate::buffer::BufferManager>,
) -> Option<String> {
    super::terminal::pure::print_terminal_handle(value)
        .or_else(|| format_marker_handle(value, buffers))
        .or_else(|| format_overlay_handle(value, buffers))
}

fn format_frame_handle(id: u64) -> String {
    if id >= crate::window::FRAME_ID_BASE {
        let ordinal = id - crate::window::FRAME_ID_BASE + 1;
        format!("#<frame F{} 0x{:x}>", ordinal, id)
    } else {
        format!("#<frame {}>", id)
    }
}

fn write_lisp_propertized_string_stateful(
    bytes: &[u8],
    runs: &[StringTextPropertyRun],
    out: &mut StatefulPrintOutput,
    state: &mut PrintState<'_>,
) {
    let mut depth_guard = PrintDepthGuard::enter(state);
    let state = depth_guard.state();
    out.push_str("#(");
    out.extend_from_slice(bytes);
    for run in runs {
        out.push(' ');
        out.push_str(&run.start.to_string());
        out.push(' ');
        out.push_str(&run.end.to_string());
        out.push(' ');
        write_value_stateful(&run.plist, out, state);
        if state.failure.is_some() {
            return;
        }
    }
    out.push(')');
}

/// Print a `Value` as a Lisp string, with buffer-manager awareness for
/// proper buffer name / killed-buffer rendering.
pub fn print_value_with_buffers(value: &Value, buffers: &crate::buffer::BufferManager) -> String {
    // String result -> escape unibyte high bytes, consistent with `print_value`
    // (GNU `prin1-to-string` into a multibyte buffer); see `print_value`.
    let options = PrintOptions {
        print_escape_nonascii: true,
        ..PrintOptions::default()
    };
    print_value_with_buffers_and_options(value, buffers, options)
}

pub fn print_value_with_buffers_and_options(
    value: &Value,
    buffers: &crate::buffer::BufferManager,
    options: PrintOptions,
) -> String {
    print_value_stateful_with_buffers(value, Some(buffers), options)
}

/// Print a `Value` as a Lisp string.
pub fn print_value(value: &Value) -> String {
    // GNU renders a value to a String via `prin1-to-string`, which prints into a
    // multibyte buffer; `print_prepare' (print.c:170-177) then binds
    // `print-escape-nonascii' to t, so a unibyte string's raw 0x80..0xFF bytes come
    // out octal-escaped (`\NNN`) instead of raw. A String result must escape them
    // regardless: a raw eight-bit byte does not round-trip through UTF-8 (it
    // lossily becomes U+FFFD). The byte sink `print_value_bytes` deliberately stays
    // raw -- it mirrors `prin1' to stdout / a buffer, where the destination governs
    // the encoding.
    let options = PrintOptions {
        print_escape_nonascii: true,
        ..PrintOptions::default()
    };
    print_value_with_options(value, options)
}

/// Canonical spelling for the bare symbols in a JIT diagnostic fingerprint.
/// A symbol cannot invoke the evaluator or traverse an object graph. Keep the
/// usual encoding and numbering extent without entering the general printer.
#[cfg(any(test, feature = "jit"))]
pub(crate) fn diagnostic_symbol_string(id: SymId) -> String {
    let options = PrintOptions {
        print_escape_nonascii: true,
        ..PrintOptions::default()
    };
    let _print_guard = enter_print_call(&options);
    match id {
        super::intern::NIL_SYM_ID => "nil".to_owned(),
        super::intern::T_SYM_ID => "t".to_owned(),
        super::intern::UNBOUND_SYM_ID => "#<unbound>".to_owned(),
        _ => String::from_utf8_lossy(&symbol_bytes(id, options)).into_owned(),
    }
}

pub fn print_value_with_options(value: &Value, options: PrintOptions) -> String {
    print_value_stateful(value, options)
}

/// Print a `Value` as a Lisp byte sequence.
///
/// This preserves non-UTF-8 byte payloads encoded via NeoVM string sentinels.
pub fn print_value_bytes(value: &Value) -> Vec<u8> {
    print_value_bytes_with_options(value, PrintOptions::default())
}

pub fn print_value_bytes_with_options(value: &Value, options: PrintOptions) -> Vec<u8> {
    let _print_guard = enter_print_call(&options);
    // Delegate to the stateful printer when circle/level/length are active.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return print_value_stateful_bytes_with_buffers(value, None, options);
    }
    let mut out = Vec::new();
    append_print_value_bytes(value, &mut out, options);
    out
}

fn append_print_value_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    if let Some(handle) = print_special_handle(value, None) {
        out.extend_from_slice(handle.as_bytes());
        return;
    }
    match value.kind() {
        ValueKind::Nil => out.extend_from_slice(b"nil"),
        ValueKind::T => out.extend_from_slice(b"t"),
        ValueKind::Fixnum(v) => {
            let mut buf = String::new();
            if options.print_integers_as_characters
                && try_format_integer_as_character(v, !options.print_noescape, &mut |c| buf.push(c))
            {
                out.extend_from_slice(buf.as_bytes());
            } else {
                out.extend_from_slice(v.to_string().as_bytes());
            }
        }
        ValueKind::Float => {
            out.extend_from_slice(format_float_with_options(value.xfloat(), options).as_bytes())
        }
        ValueKind::Symbol(id) => append_symbol_bytes(id, out, options),
        ValueKind::String => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            let ls = value.as_lisp_string().unwrap();
            if options.print_noescape {
                out.extend_from_slice(&princ_string_bytes(ls, options.princ_string_output()));
            } else {
                let str_bytes = format_lisp_string_bytes_emacs(ls, &options);
                if let Some(runs) =
                    printed_string_property_runs(*value, options.charset_text_property)
                {
                    out.extend_from_slice(b"#(");
                    out.extend_from_slice(&str_bytes);
                    for run in runs {
                        out.push(b' ');
                        out.extend_from_slice(run.start.to_string().as_bytes());
                        out.push(b' ');
                        out.extend_from_slice(run.end.to_string().as_bytes());
                        out.push(b' ');
                        append_print_value_bytes(&run.plist, out, options);
                    }
                    out.push(b')');
                } else {
                    out.extend_from_slice(&str_bytes);
                }
            }
        }
        ValueKind::Cons => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            if let Some(shorthand) = print_list_shorthand_bytes(value, options) {
                out.extend_from_slice(&shorthand);
                return;
            }
            out.push(b'(');
            print_cons_bytes(value, out, options);
            out.push(b')');
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            if let Some(slots) = char_table_external_slots(value) {
                out.extend_from_slice(b"#^[");
                for (idx, item) in slots.iter().enumerate() {
                    if idx > 0 {
                        out.push(b' ');
                    }
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
            }
        }
        ValueKind::Veclike(VecLikeType::SubCharTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            if let Some((depth, min_char, slots)) =
                super::chartable::sub_char_table_external_slots(value)
            {
                out.extend_from_slice(b"#^^[");
                out.extend_from_slice(depth.to_string().as_bytes());
                out.push(b' ');
                out.extend_from_slice(min_char.to_string().as_bytes());
                for item in &slots {
                    out.push(b' ');
                    append_print_value_bytes(item, out, options);
                }
                out.push(b']');
            }
        }
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            let nbits = bool_vector_length(value).unwrap_or(0);
            append_bool_vector_bytes(value, nbits as usize, out, options);
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            out.push(b'[');
            let items = value.as_vector_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(b' ');
                }
                append_print_value_bytes(item, out, options);
            }
            out.push(b']');
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            out.extend_from_slice(b"#s(");
            let items = value.as_record_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(b' ');
                }
                append_print_value_bytes(item, out, options);
            }
            out.push(b')');
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            append_hash_table_bytes(value, out, options);
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            let count = value.as_obarray_obj().map_or(0, |obj| obj.count);
            out.extend_from_slice(format!("#<obarray n={count}>").as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            let bytes = with_print_object_guard(
                PrintObjectRef::Lambda(value.0),
                |index| format!("#{index}").into_bytes(),
                || format_interpreted_closure_bytes(value, options),
            );
            out.extend_from_slice(&bytes);
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            let params = value.closure_params().map_or_else(
                || "nil".to_string(),
                |params| format_params(params, options),
            );
            let body = value
                .closure_body_value()
                .map(|body| format_closure_body_forms(body, options))
                .unwrap_or_else(|| "nil".to_string());
            out.extend_from_slice(format!("(macro {} {})", params, body).as_bytes());
        }
        ValueKind::Subr(id) => {
            out.extend_from_slice(format!("#<subr {}>", resolve_sym(id)).as_bytes())
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = value.as_subr_id().unwrap();
            out.extend_from_slice(format!("#<subr {}>", resolve_sym(id)).as_bytes())
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            if append_bytes_cycle_ref_if_any(value, out) {
                return;
            }
            let _byte_guard = BytePrintObjectStackGuard::enter(value);
            append_bytecode_literal_bytes(value, out, options);
        }
        ValueKind::Veclike(VecLikeType::Marker) => out.extend_from_slice(
            print_special_handle(value, None)
                .unwrap_or_else(|| "#<marker>".to_string())
                .as_bytes(),
        ),
        ValueKind::Veclike(VecLikeType::Overlay) => out.extend_from_slice(
            print_special_handle(value, None)
                .unwrap_or_else(|| "#<overlay>".to_string())
                .as_bytes(),
        ),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            out.extend_from_slice(
                format!("#<buffer {}>", value.as_buffer_id().unwrap().0).as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            out.extend_from_slice(
                format!("#<window {}>", value.as_window_id().unwrap()).as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => {
            out.extend_from_slice(b"#<window-configuration>");
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            out.extend_from_slice(format_frame_handle(value.as_frame_id().unwrap()).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            out.extend_from_slice(format!("#<timer {}>", value.as_timer_id().unwrap()).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Process) => {
            out.extend_from_slice(
                super::process::print_process_handle(value)
                    .unwrap_or_else(|| "#<process>".to_string())
                    .as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Terminal) => {
            out.extend_from_slice(
                print_special_handle(value, None)
                    .unwrap_or_else(|| "#<terminal>".to_string())
                    .as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Xwidget) => {
            let xw = value.as_xwidget().unwrap();
            out.extend_from_slice(format!("#<xwidget {}>", xw.xwidget_id).as_bytes());
        }
        ValueKind::Veclike(VecLikeType::XwidgetView) => {
            out.extend_from_slice(b"#<xwidget-view>");
        }
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => {
            let id = value.as_surface_handle().unwrap();
            out.extend_from_slice(format!("#<neomacs-surface {id}>").as_bytes());
        }
        ValueKind::Veclike(VecLikeType::VideoHandle) => {
            let id = value.as_video_handle().unwrap();
            out.extend_from_slice(format!("#<neomacs-video {id}>").as_bytes());
        }
        ValueKind::Veclike(VecLikeType::Bignum) => {
            out.extend_from_slice(value.as_bignum().unwrap().to_string().as_bytes());
        }
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => {
            append_symbol_with_pos_bytes(value, out, options);
        }
        ValueKind::Veclike(VecLikeType::Finalizer) => {
            out.extend_from_slice(b"#<finalizer>");
        }
        ValueKind::Veclike(
            kind @ (VecLikeType::Thread | VecLikeType::Mutex | VecLikeType::CondVar),
        ) => {
            out.extend_from_slice(
                format!(
                    "#<{} {}>",
                    threading_handle_print_name(kind),
                    value.threading_handle_id().unwrap_or(0)
                )
                .as_bytes(),
            );
        }
        ValueKind::Veclike(VecLikeType::Sqlite) => {
            let obj = value.as_sqlite().unwrap();
            if obj.is_statement {
                out.extend_from_slice(format!("#<sqlite statement {}>", obj.id).as_bytes());
            } else {
                out.extend_from_slice(format!("#<sqlite db {}>", obj.id).as_bytes());
            }
        }
        ValueKind::Veclike(VecLikeType::UserPtr) => {
            out.extend_from_slice(b"#<user-ptr>");
        }
        ValueKind::Veclike(VecLikeType::ModuleFunction) => {
            out.extend_from_slice(b"#<module-function>");
        }
        ValueKind::Veclike(VecLikeType::Font) => {
            out.extend_from_slice(b"#<font-object>");
        }
        ValueKind::Unbound => {
            append_symbol_bytes(super::intern::UNBOUND_SYM_ID, out, options);
        }
        ValueKind::Unknown => {
            out.extend_from_slice(format!("#<unknown {:#x}>", value.0).as_bytes());
        }
    }
}

fn append_symbol_bytes(id: super::intern::SymId, out: &mut Vec<u8>, options: PrintOptions) {
    out.extend_from_slice(&symbol_bytes(id, options));
}

fn write_symbol_with_pos_stateful(
    value: &Value,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let Some(swp) = value.as_symbol_with_pos() else {
        out.push_str("#<symbol-with-pos>");
        return;
    };

    let sym = swp.sym;
    if state.options.print_symbols_bare {
        write_value_stateful(&sym, out, state);
        if state.failure.is_some() {
            return;
        }
        return;
    }

    out.push_str("#<symbol ");
    if sym.is_bare_symbol() {
        write_value_stateful(&sym, out, state);
        if state.failure.is_some() {
            return;
        }
    } else {
        out.push_str("NOT A SYMBOL!!");
    }
    if let Some(pos) = swp.pos.as_fixnum() {
        write!(out, " at {pos}").unwrap();
    } else {
        out.push_str(" NOT A POSITION!!");
    }
    out.push('>');
}

fn append_symbol_with_pos_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let Some(swp) = value.as_symbol_with_pos() else {
        out.extend_from_slice(b"#<symbol-with-pos>");
        return;
    };

    let sym = swp.sym;
    if options.print_symbols_bare {
        append_print_value_bytes(&sym, out, options);
        return;
    }

    out.extend_from_slice(b"#<symbol ");
    if sym.is_bare_symbol() {
        append_print_value_bytes(&sym, out, options);
    } else {
        out.extend_from_slice(b"NOT A SYMBOL!!");
    }
    if let Some(pos) = swp.pos.as_fixnum() {
        out.extend_from_slice(format!(" at {pos}").as_bytes());
    } else {
        out.extend_from_slice(b" NOT A POSITION!!");
    }
    out.push(b'>');
}

fn symbol_bytes(id: super::intern::SymId, options: PrintOptions) -> Vec<u8> {
    // Identity is decided on the name ATOM, the printed spelling on the name
    // OBJECT: GNU prints the string the symbol was created from, which Lisp can
    // have mutated in place since.
    // GNU print.c:2492-2494 consults interned status only for a gensym prefix.
    // Preserve atom identity when needed; visible mutable spelling stays below.
    let needs_gensym_prefix = options.print_gensym
        && lookup_interned_lisp_string(resolve_sym_lisp_string(id)) != Some(id);
    let visible_name = super::intern::resolve_lisp_visible_symbol_name(id);
    let name = visible_name.text();
    let mut out = Vec::new();
    if needs_gensym_prefix {
        out.extend_from_slice(b"#:");
        if name.is_empty() {
            return out;
        }
    }
    append_symbol_name_bytes_with_escape(name, &mut out, SymbolNameEscaping::from(options));
    out
}

/// Per-render symbol-name quoting policy; scalar metadata shared safely by
/// mutators, with no cached Lisp state. Symbols always promote raw name bytes
/// to byte8 characters, independently of the destination's string protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SymbolNameEscaping {
    Readable,
    Preserve,
}
static_assertions::assert_impl_all!(SymbolNameEscaping: Send, Sync);

impl From<PrintOptions> for SymbolNameEscaping {
    fn from(options: PrintOptions) -> Self {
        if options.print_noescape {
            Self::Preserve
        } else {
            Self::Readable
        }
    }
}

fn append_symbol_name_bytes_with_escape(
    name: &crate::heap_types::LispString,
    out: &mut Vec<u8>,
    escaping: SymbolNameEscaping,
) {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        out.extend_from_slice(b"##");
        return;
    }

    match escaping {
        SymbolNameEscaping::Preserve => {
            // GNU symbols fetch characters as multibyte even for callable
            // streams (print.c:2506-2510; character.h:464-483). Raw name bytes
            // become byte8, unlike a string sent to CallbackCharacters.
            out.extend_from_slice(&princ_string_bytes(name, PrincStringOutput::PreserveByte8));
            return;
        }
        SymbolNameEscaping::Readable => {}
    }

    let mut confusing = symbol_name_confusing(bytes);
    let mut position = 0;
    let characters = std::iter::from_fn(|| {
        if position >= bytes.len() {
            return None;
        }
        let byte = bytes[position];
        let (ch, width) = if byte < 128 {
            (u32::from(byte), 1)
        } else if name.is_multibyte() {
            let (c, w) = crate::heap_types::decode_declared_multibyte_char(&bytes[position..]);
            (c.code(), w)
        } else {
            (crate::emacs_core::emacs_char::unibyte_to_char(byte), 1)
        };
        position += width;
        Some(ch)
    });
    for ch in characters {
        let needs_escape = matches!(
            ch,
            40 | 41 | 91 | 93 | 34 | 92 | 59 | 35 | 39 | 96 | 44 | 160
        ) || ch <= 32
            || confusing;
        if needs_escape {
            out.push(b'\\');
            confusing = false;
        }
        if ch < 128 {
            out.push(ch as u8);
        } else {
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            let len = crate::emacs_core::emacs_char::char_string(ch, &mut buf);
            out.extend_from_slice(&buf[..len]);
        }
    }
}

pub(crate) fn format_float(f: f64) -> String {
    const NAN_QUIET_BIT: u64 = 1u64 << 51;
    const NAN_PAYLOAD_MASK: u64 = (1u64 << 51) - 1;

    if f.is_nan() {
        let bits = f.to_bits();
        let frac = bits & ((1u64 << 52) - 1);
        if (frac & NAN_QUIET_BIT) != 0 {
            let payload = frac & NAN_PAYLOAD_MASK;
            return if f.is_sign_negative() {
                format!("-{}.0e+NaN", payload)
            } else {
                format!("{}.0e+NaN", payload)
            };
        }
        return if f.is_sign_negative() {
            "-0.0e+NaN".to_string()
        } else {
            "0.0e+NaN".to_string()
        };
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "1.0e+INF".to_string()
        } else {
            "-1.0e+INF".to_string()
        };
    }
    format_float_dtoastr(f)
}

pub(crate) fn format_float_with_options(f: f64, options: PrintOptions) -> String {
    format_float_with_output_format(f, options.float_output_format)
}

/// Format a finite float matching GNU Emacs's `dtoastr` / `float_to_string`:
/// use `%g`-style formatting with the minimum precision (starting from DBL_DIG=15)
/// that round-trips through strtod, then ensure a decimal point or exponent is present.
fn format_float_dtoastr(f: f64) -> String {
    let abs_f = f.abs();
    let start_prec = if abs_f != 0.0 && abs_f < f64::MIN_POSITIVE {
        1
    } else {
        15 // DBL_DIG
    };
    for prec in start_prec..=20 {
        // %g: uses %e if exponent < -4 or >= precision, otherwise %f.
        // %g also trims trailing zeros.
        let s = format!("{:.prec$e}", f, prec = prec - 1);
        // Parse back and check round-trip
        if let Ok(parsed) = s.parse::<f64>()
            && parsed.to_bits() == f.to_bits()
        {
            // Convert from Rust's scientific notation to %g-style output
            return rust_sci_to_emacs_g(f, &s, prec);
        }
    }
    // Fallback: maximum precision
    let s = format!("{:.20e}", f);
    rust_sci_to_emacs_g(f, &s, 21)
}

/// Convert Rust scientific notation string to GNU Emacs %g-style output.
/// %g rules: use fixed notation unless exponent >= precision or exponent < -4.
/// %g trims trailing zeros (but keeps at least one digit after decimal point
/// for Emacs's post-processing).
fn rust_sci_to_emacs_g(f: f64, sci: &str, prec: usize) -> String {
    // Parse the exponent from Rust's scientific notation (e.g., "3.14e2")
    let (mantissa_str, exp_str) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp_str.parse().unwrap_or(0);

    // %g uses fixed notation when -4 <= exp < prec
    let result = if exp >= -4 && exp < prec as i32 {
        // Fixed notation
        format_g_fixed(f, mantissa_str, exp, prec)
    } else {
        // Scientific notation with Emacs-style exponent formatting
        format_g_scientific(mantissa_str, exp, prec)
    };

    // Emacs post-processing: ensure decimal point or exponent is present
    ensure_decimal_point(result)
}

/// Format as fixed-point notation for %g, trimming trailing zeros.
fn format_g_fixed(f: f64, _mantissa: &str, exp: i32, prec: usize) -> String {
    // %g precision = total significant digits.
    // digits_after_dot = prec - exp - 1 (works for both positive and negative exp)
    let digits_after_dot = (prec as i32 - exp - 1).max(0) as usize;
    let s = format!("{:.digits$}", f, digits = digits_after_dot);
    trim_trailing_zeros_g(&s)
}

/// Format as scientific notation for %g, trimming trailing zeros.
fn format_g_scientific(mantissa: &str, exp: i32, _prec: usize) -> String {
    // Trim trailing zeros from mantissa
    let trimmed = trim_trailing_zeros_g(mantissa);
    // Emacs uses e+XX / e-XX with at least 2-digit exponent for |exp| < 100,
    // but %g in glibc actually uses minimal digits. Let's match C's %g.
    if exp >= 0 {
        format!("{}e+{:02}", trimmed, exp)
    } else {
        format!("{}e-{:02}", trimmed, -exp)
    }
}

/// Trim trailing zeros after decimal point (%g style).
/// "3.1400" -> "3.14", "3.0000" -> "3", "100" -> "100"
fn trim_trailing_zeros_g(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    trimmed.to_string()
}

/// Ensure the output has a decimal point with trailing digit (Emacs requirement).
/// If no decimal point or exponent, append ".0".
fn ensure_decimal_point(mut s: String) -> String {
    // Check if there's already a decimal point or exponent
    let has_dot_or_exp = s.bytes().any(|b| b == b'.' || b == b'e' || b == b'E');
    if !has_dot_or_exp {
        s.push_str(".0");
    } else if s.ends_with('.') {
        s.push('0');
    }
    s
}

fn params_value(params: Option<&super::value::LambdaParams>) -> Value {
    params
        .map(crate::emacs_core::builtins::lambda_params_to_value)
        .unwrap_or(Value::NIL)
}

fn write_params_stateful(
    params: Option<&super::value::LambdaParams>,
    out: &mut StatefulPrintOutput,
    state: &mut PrintState,
) {
    let value = params_value(params);
    write_value_stateful(&value, out, state);
}

fn format_params(params: &super::value::LambdaParams, options: PrintOptions) -> String {
    let value = crate::emacs_core::builtins::lambda_params_to_value(params);
    print_value_with_options(&value, options)
}

fn format_closure_body_forms(body: Value, options: PrintOptions) -> String {
    let Some(forms) = list_to_vec(&body) else {
        return print_value_with_options(&body, options);
    };
    if forms.is_empty() {
        "nil".to_string()
    } else {
        forms
            .iter()
            .map(|form| print_value_with_options(form, options))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// GNU print.c:2611-2613 traverses closure slots in the current PE_vector
/// invocation, retaining the enclosing identities (print.c:2285-2300).
/// This synchronous byte-only path descends with its existing mutator-owned
/// traversal prefix; starting a new string invocation for each slot loses that
/// prefix and also converts raw byte payloads through UTF-8.
fn format_interpreted_closure_bytes(value: &Value, options: PrintOptions) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"#[");
    if let Some(slots) = value.closure_slots() {
        for (index, slot) in slots.iter().enumerate() {
            if index != 0 {
                out.push(b' ');
            }
            append_print_value_bytes(slot, &mut out, options);
        }
    }
    out.push(b']');
    out
}

fn print_list_shorthand_bytes(value: &Value, options: PrintOptions) -> Option<Vec<u8>> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    let head = match items[0].kind() {
        ValueKind::Symbol(id) => id,
        _ => return None,
    };

    if symbol_id_is(head, "make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        let mut out = Vec::new();
        out.extend_from_slice(b"#s");
        append_print_value_bytes(&payload, &mut out, options);
        return Some(out);
    }

    let shorthand = symbol_shorthand(head)?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, nested_options): (&[u8], PrintOptions) = match shorthand {
        SymbolShorthand::Quote => (b"'", options),
        SymbolShorthand::Function => (b"#'", options),
        SymbolShorthand::Backquote => (b"`", options.enter_backquote()),
        SymbolShorthand::Comma => {
            if !options.allow_unquote_shorthand() {
                return None;
            }
            (b",", options.exit_backquote())
        }
        SymbolShorthand::CommaAt => {
            if !options.allow_unquote_shorthand() {
                return None;
            }
            (b",@", options.exit_backquote())
        }
    };

    let mut out = Vec::new();
    out.extend_from_slice(prefix);
    append_print_value_bytes(&items[1], &mut out, nested_options);
    Some(out)
}

fn quote_payload(value: &Value) -> Option<Value> {
    let items = list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    value_is_symbol_named(&items[0], "quote").then_some(items[1])
}

fn print_cons_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = options
        .print_length
        .filter(|length| *length >= 0)
        .unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut remaining = 2_i64;
    let mut period = 2_i64;
    let mut tortoise_index = 0_i64;
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.extend_from_slice(b"...");
                        return;
                    }
                } else {
                    out.push(b' ');
                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.extend_from_slice(b"...");
                        return;
                    }
                    // GNU print.c:2740-2763: Brent compares cons identities,
                    // and advances the tortoise on exponentially spaced steps.
                    remaining -= 1;
                    if remaining == 0 {
                        tortoise_index = tortoise_index.saturating_add(period);
                        period = period.saturating_mul(2);
                        remaining = period;
                        tortoise = cursor;
                    } else if super::value::eq_value(&cursor, &tortoise) {
                        out.extend_from_slice(format!(". #{tortoise_index}").as_bytes());
                        return;
                    }
                }
                append_print_value_bytes(&cursor.cons_car(), out, options);
                cursor = cursor.cons_cdr();
                first = false;
            }
            ValueKind::Nil => return,
            _ => {
                if !first {
                    out.extend_from_slice(b" . ");
                }
                append_print_value_bytes(&cursor, out, options);
                return;
            }
        }
    }
}
// -- Bool-vector printing ---------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoolVectorByteSyntax {
    Literal,
    Quoted,
    NamedEscape(u8),
    Octal,
}

fn bool_vector_byte_syntax(byte: u8, options: PrintOptions) -> BoolVectorByteSyntax {
    match byte {
        b'\n' if options.print_escape_newlines => BoolVectorByteSyntax::NamedEscape(b'n'),
        b'\x0c' if options.print_escape_newlines => BoolVectorByteSyntax::NamedEscape(b'f'),
        b if b > 0x7f || (options.print_escape_control_characters && (b < 0x20 || b == 0x7f)) => {
            BoolVectorByteSyntax::Octal
        }
        b'"' | b'\\' => BoolVectorByteSyntax::Quoted,
        _ => BoolVectorByteSyntax::Literal,
    }
}

fn bool_vector_packed_bytes(value: &Value, nbits: usize) -> Vec<u8> {
    let Some(view) = crate::emacs_core::boolvec::BoolVectorView::of(value) else {
        return Vec::new();
    };
    debug_assert_eq!(view.len(), nbits);
    (0..nbits.div_ceil(8))
        .map(|index| view.byte(index))
        .collect()
}

fn format_bool_vector(value: &Value, nbits: usize, options: PrintOptions) -> String {
    let mut out = Vec::new();
    append_bool_vector_bytes(value, nbits, &mut out, options);
    String::from_utf8_lossy(&out).into_owned()
}

/// Append bool-vector bytes as `#&N"..."`.
fn append_bool_vector_bytes(value: &Value, nbits: usize, out: &mut Vec<u8>, options: PrintOptions) {
    let packed = bool_vector_packed_bytes(value, nbits);
    out.extend_from_slice(b"#&");
    out.extend_from_slice(nbits.to_string().as_bytes());
    out.push(b'"');

    let printed_len = options
        .print_length
        .and_then(|length| usize::try_from(length).ok())
        .map_or(packed.len(), |length| length.min(packed.len()));
    let printed = &packed[..printed_len];

    for (index, &byte) in printed.iter().enumerate() {
        match bool_vector_byte_syntax(byte, options) {
            BoolVectorByteSyntax::Literal => out.push(byte),
            BoolVectorByteSyntax::Quoted => {
                out.push(b'\\');
                out.push(byte);
            }
            BoolVectorByteSyntax::NamedEscape(name) => {
                out.push(b'\\');
                out.push(name);
            }
            BoolVectorByteSyntax::Octal => {
                super::string_escape::push_octal_escape_contextual_u32(
                    out,
                    byte,
                    printed.get(index + 1).map(|next| u32::from(*next)),
                );
            }
        }
    }

    if printed_len < packed.len() {
        out.extend_from_slice(b" ...");
    }
    out.push(b'"');
}

// -- Hash-table printing ----------------------------------------------------

fn append_hash_table_bytes(value: &Value, out: &mut Vec<u8>, options: PrintOptions) {
    let table = value.as_hash_table().unwrap().clone();
    out.extend_from_slice(b"#s(hash-table");

    append_hash_table_test_bytes(&table, out, options);

    if let Some(ref weakness) = table.weakness {
        out.extend_from_slice(b" weakness ");
        out.extend_from_slice(weakness.name().as_bytes());
    }

    if !table.data.is_empty() {
        out.extend_from_slice(b" data (");
        let mut first = true;
        for (_, entry) in table.data.keyed_entries_in_slot_order() {
            if !first {
                out.push(b' ');
            }
            append_print_value_bytes(&entry.key, out, options);
            out.push(b' ');
            append_print_value_bytes(&entry.value, out, options);
            first = false;
        }
        out.push(b')');
    }

    out.push(b')');
}

/// The name GNU prints a threading handle under.
///
/// GNU's `print_vectorlike_unreadable` writes `#<thread ...>`, `#<mutex ...>`
/// and `#<condvar ...>` -- note the last is `condvar`, not the
/// `condition-variable` that `type-of` answers.
fn threading_handle_print_name(kind: crate::tagged::header::VecLikeType) -> &'static str {
    match kind {
        crate::tagged::header::VecLikeType::Thread => "thread",
        crate::tagged::header::VecLikeType::Mutex => "mutex",
        _ => "condvar",
    }
}

#[cfg(test)]
#[path = "tests/print_test.rs"]
mod tests;
