//! Layout-input freshness records owned by the evaluator (stage-4 W2/E5).

pub mod display_table_input;
pub mod face_input;
pub mod invisibility_input;
pub mod pixel_input;
pub mod string_property_input;

use neovm_edit::buffer::{BufferId, CharLen, CharRange, EmacsByteLen, EmacsByteRange};
use neovm_edit::emacs_core::word_wrap::WordWrapPolicy;
use neovm_value::symbol::SymbolPropertyRevision;

use self::display_table_input::LayoutDisplayTableInput;
use self::invisibility_input::LayoutInvisibilityInput;
use super::window_state::{
    BodyRedisplayRevision, FrameLayoutInputState, WindowLayoutInputState, WindowLayoutValueIdentity,
};

/// Owned prefix inputs shared by redisplay skipping, geometry freshness and
/// row reuse. String contents can change without a buffer modification tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutPrefixInputs {
    pub(crate) line: LayoutPrefixInput,
    pub(crate) wrap: LayoutPrefixInput,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum LayoutPrefixInput {
    #[default]
    Missing,
    String {
        content: string_property_input::StringContentInput,
        display_properties: string_property_input::StringDisplayInputs,
    },
    Space(pixel_input::SpaceInput),
    // Unsupported display specs retain the existing identity contract. This
    // snapshot does not claim to detect mutations inside arbitrary Lisp graphs.
    Other(usize),
}

/// Buffer inputs that can change display-row positions or geometry.
///
/// The shared projection is used by redisplay skipping. Retained rows and
/// speculative attempts require the additional typed fontification identity.
/// All other fields are captured by one monomorphized collector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferLayoutInputState<Fontification = ()> {
    pub(crate) id: BufferId,
    pub(crate) modified_tick: i64,
    pub(crate) chars_modified_tick: i64,
    pub(crate) props_modified_tick: i64,
    pub(crate) overlay_modified_tick: i64,
    pub(crate) accessible_chars: CharRange,
    pub(crate) accessible_bytes: EmacsByteRange,
    pub(crate) total_chars: CharLen,
    pub(crate) total_emacs_bytes: EmacsByteLen,
    pub(crate) fontification_functions: Fontification,
    pub(crate) prefixes: LayoutPrefixInputs,
    pub(crate) invisibility: LayoutInvisibilityInput,
    pub(crate) display_table: LayoutDisplayTableInput,
    pub(crate) word_wrap_policy: WordWrapPolicy,
}

/// Effective callback identity, captured on this Context's mutator. The
/// numeric word owns no Lisp object and is never dereferenced from here.
/// Hook replacement must invalidate retained rows and speculative attempts;
/// mutable contents and callback visitation retain their existing conservative
/// redisplay-generation boundary. A unit-valued redisplay projection cannot be
/// substituted for this typed identity at either freshness boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontificationHookIdentity(pub(crate) Option<WindowLayoutValueIdentity>);

pub(crate) type WindowBufferLayoutInputState = BufferLayoutInputState<FontificationHookIdentity>;

/// Opaque identity of every mutable input used to lay out one live window.
///
/// Construction and comparison live on `Context`; keeping these fields opaque
/// prevents individual snapshot consumers from inventing partial freshness
/// checks that drift apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowDisplaySnapshotFreshness {
    pub(crate) fontset_generation: u64,
    pub(crate) context_instance_id: u64,
    pub(crate) window_topology_generation: u64,
    pub(crate) frame: FrameLayoutInputState,
    pub(crate) window: WindowLayoutInputState,
    pub(crate) buffer: WindowBufferLayoutInputState,
    pub(crate) face_change_count: u64,
    pub(crate) display_var_change_count: u64,
    pub(crate) redisplay_generation: u64,
    /// Body-only invalidation; see [`BodyRedisplayRevision`]. Unlike
    /// `redisplay_generation` this is NOT aligned away by scroll-surface
    /// compatibility, so a forced body redisplay also refuses scroll reuse.
    pub(crate) body_redisplay: BodyRedisplayRevision,
    pub(crate) media_generation: u64,
    pub(crate) function_epoch: u64,
    pub(crate) symbol_property_revision: SymbolPropertyRevision,
    /// Every overlay in the buffer could only change faces when this token was
    /// captured. This excludes structural overlays from point-only query reuse;
    /// it does not prove that overlay modifications preserve row geometry.
    pub(crate) overlays_face_only: bool,
}
