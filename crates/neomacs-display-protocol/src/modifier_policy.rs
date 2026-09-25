//! The typed NS modifier policy: GNU's `nsterm.m` modifier machinery as a
//! compile-time-checked value.
//!
//! GNU reads seven Lisp variables (`ns-command-modifier`,
//! `ns-right-command-modifier`, `ns-alternate-modifier`,
//! `ns-right-alternate-modifier`, `ns-control-modifier`,
//! `ns-right-control-modifier`, `ns-function-modifier`) at every key event
//! and converts the window server's raw modifier flags with `EV_MODIFIERS2`
//! (`src/nsterm.m:397-425`) and `ev_modifiers_helper` (`:371-395`).  A port
//! that renders through winit cannot read Lisp per event, so the values are
//! compiled once per change into a [`ModifierPolicy`] and shipped to the
//! render thread, which then cooks raw facts through the same arithmetic.
//!
//! Grammar and semantics reproduced here:
//!
//! * a variable value is a symbol (`control', `meta', `alt', `super',
//!   `hyper', `none') or a plist `(:ordinary SYM :function SYM :mouse SYM)'
//!   (`nsterm.m:11569-11575`);
//! * a right-side variable may also be `left', meaning "use the left
//!   variable's value" (`right_mod', `nsterm.m:2680-2684`);
//! * `none' and nil produce no modifier bit -- the key keeps its standard
//!   meaning, which is what makes Option chords fall through to the composed
//!   character path (`nil_or_none', `:2686-2690');
//! * when the window server cannot tell which side was pressed, the LEFT
//!   value applies (`:387-391`).

/// An Emacs modifier a physical key can be mapped onto.
///
/// `Shift` is absent because no NS-family variable maps to it, and `none` is
/// absent because it is the *absence* of a mapping
/// ([`Option::None`]), mirroring `parse_solitary_modifier'
/// returning 0 (`src/keyboard.c:7917`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EmacsModifierKey {
    Alt,
    Ctrl,
    Hyper,
    Meta,
    Super,
}

/// Which flavour of event a modifier assignment applies to.
///
/// `src/nsterm.m:7322` picks `QCfunction' over `QCordinary' when the keysym
/// is a function key; `QCmouse' covers pointer events.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModifierEventKind {
    Ordinary,
    Function,
    Mouse,
}

/// One variable's value, resolved per event kind.
///
/// Construction is struct-literal only, so adding a kind is a compile error
/// at every construction site rather than a silently-ignored slot.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ModifierAssignment {
    pub ordinary: Option<EmacsModifierKey>,
    pub function: Option<EmacsModifierKey>,
    pub mouse: Option<EmacsModifierKey>,
}

impl ModifierAssignment {
    /// GNU docstring: "Either SYMBOL, describing the behavior for any event,
    /// or (:ordinary SYMBOL :function SYMBOL :mouse SYmbol)" -- a plain
    /// symbol applies to every kind (`nsterm.m:11569-11571`).
    pub const fn uniform(value: Option<EmacsModifierKey>) -> Self {
        Self {
            ordinary: value,
            function: value,
            mouse: value,
        }
    }

    pub const fn for_kind(&self, kind: ModifierEventKind) -> Option<EmacsModifierKey> {
        match kind {
            ModifierEventKind::Ordinary => self.ordinary,
            ModifierEventKind::Function => self.function,
            ModifierEventKind::Mouse => self.mouse,
        }
    }
}

/// How one side of a modifier family was pressed.
///
/// `Unknown` reproduces GNU's fallback for window servers that do not
/// differentiate sides ("GNUstep (and possibly macOS in certain
/// circumstances) doesn't differentiate between the left and right keys, so
/// if we can't identify which key it is, we use the left key setting",
/// `nsterm.m:387-391`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ModifierSideState {
    #[default]
    Unknown,
    LeftOnly,
    RightOnly,
    Both,
}

/// The raw physical facts a window server hands over, before any policy
/// applies.
///
/// This is deliberately a *neutral* type: the winit backend translates its
/// `ModifiersState` plus per-key `KeyLocation` facts into it, the TTY
/// backend derives it from escape sequences, and tests construct it
/// directly.  The fn (NS_FUNCTION_KEY_MASK) and help flags are not
/// representable in winit and are documented as not supported yet.
///
/// `None` for a family means "that family's either-mask is up"; GNU guards
/// each family with its own either-mask (`nsterm.m:378`).  `Some`
/// means the mask is down, with `ModifierSideState::Unknown` carrying the
/// "down but side not identified" case.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RawModifiers {
    pub shift: bool,
    pub ctrl: Option<ModifierSideState>,
    pub command: Option<ModifierSideState>,
    pub option: Option<ModifierSideState>,
}

/// Which physical key the policy configures.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PhysicalModifierKey {
    LeftCommand,
    RightCommand,
    LeftOption,
    RightOption,
    LeftControl,
    RightControl,
    /// The `fn' key (`ns-function-modifier').
    Function,
}

impl PhysicalModifierKey {
    /// Whether this is a right-side physical key, whose GNU variable may be
    /// the `left' marker.
    pub const fn is_right(self) -> bool {
        matches!(
            self,
            PhysicalModifierKey::RightCommand
                | PhysicalModifierKey::RightOption
                | PhysicalModifierKey::RightControl
        )
    }
}

/// One assignment per physical key: the shape a value parser produces.
///
/// Construction is struct-literal only, so adding a family is a compile
/// error at every construction site rather than a silently-dropped slot.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ModifierPolicyParts {
    pub command_left: ModifierAssignment,
    pub command_right: SideAssignment,
    pub option_left: ModifierAssignment,
    pub option_right: SideAssignment,
    pub ctrl_left: ModifierAssignment,
    pub ctrl_right: SideAssignment,
    pub function: ModifierAssignment,
}

impl From<ModifierPolicyParts> for ModifierPolicy {
    fn from(parts: ModifierPolicyParts) -> Self {
        Self {
            command: ModifierFamily {
                left: parts.command_left,
                right: parts.command_right,
            },
            option: ModifierFamily {
                left: parts.option_left,
                right: parts.option_right,
            },
            ctrl: ModifierFamily {
                left: parts.ctrl_left,
                right: parts.ctrl_right,
            },
            function: parts.function,
        }
    }
}

/// A right-side assignment, keeping GNU's `left' marker explicit.
///
/// GNU compares the raw right variable with `left' *per event*
/// (`nsterm.m:382`), so the marker must survive into the policy rather than
/// being resolved away: [`ModifierPolicy::assignment`] resolves it at read
/// time against the live left value.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SideAssignment {
    /// GNU's `left' marker: use the family's left assignment.
    #[default]
    InheritLeft,
    Explicit(ModifierAssignment),
}

/// How winit should rewrite Option-key chords, mirror of
/// `winit::platform::macos::OptionAsAlt`.
///
/// Defined here so the protocol crate stays winit-free; the macOS display
/// runtime maps this 1:1 onto winit's shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OptionAsAltShape {
    None,
    LeftOnly,
    RightOnly,
    Both,
}

/// A modifier bit in the frontend transport.
///
/// Bit positions are wire values shared with `neovm-core`'s `RENDER_*` and
/// the display runtime's `NEOMACS_*` masks; `Shift` through `Super` keep
/// their historical positions, `Alt` and `Hyper` appended in that order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransportModifierBit {
    Shift,
    Ctrl,
    Meta,
    Super,
    Alt,
    Hyper,
}

impl TransportModifierBit {
    /// The wire mask for this bit.
    pub const fn mask(self) -> u32 {
        match self {
            TransportModifierBit::Shift => 1 << 0,
            TransportModifierBit::Ctrl => 1 << 1,
            TransportModifierBit::Meta => 1 << 2,
            TransportModifierBit::Super => 1 << 3,
            TransportModifierBit::Alt => 1 << 4,
            TransportModifierBit::Hyper => 1 << 5,
        }
    }

    /// `parse_solitary_modifier' ("alt"/"meta" are different bits,
    /// `src/keyboard.c:7941-7951`): the transport carries both, so a policy
    /// mapping to `alt' and one mapping to `meta' must cook differently.
    const fn for_modifier_key(key: EmacsModifierKey) -> Self {
        match key {
            EmacsModifierKey::Alt => TransportModifierBit::Alt,
            EmacsModifierKey::Ctrl => TransportModifierBit::Ctrl,
            EmacsModifierKey::Hyper => TransportModifierBit::Hyper,
            EmacsModifierKey::Meta => TransportModifierBit::Meta,
            EmacsModifierKey::Super => TransportModifierBit::Super,
        }
    }
}

/// A cooked set of transport modifier bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportModifierBits(u32);

impl TransportModifierBits {
    pub const fn get(&self, bit: TransportModifierBit) -> bool {
        self.0 & bit.mask() != 0
    }

    pub const fn set(&mut self, bit: TransportModifierBit) {
        self.0 |= bit.mask();
    }

    /// Merge an already-cooked mask in (`ev_modifiers_helper` ORs the
    /// families together, `nsterm.m:397-423`).
    pub const fn merge_mask(&mut self, mask: u32) {
        self.0 |= mask;
    }

    /// The raw wire value.
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// The side assignment for one modifier family.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ModifierFamily {
    left: ModifierAssignment,
    right: SideAssignment,
}

impl ModifierFamily {
    const fn effective_right(&self) -> ModifierAssignment {
        match self.right {
            SideAssignment::InheritLeft => self.left,
            SideAssignment::Explicit(assignment) => assignment,
        }
    }
}

/// The compiled NS modifier policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ModifierPolicy {
    command: ModifierFamily,
    option: ModifierFamily,
    ctrl: ModifierFamily,
    function: ModifierAssignment,
}

impl Default for ModifierPolicy {
    /// The values GNU's `syms_of_nsterm` installs under NS_IMPL_COCOA
    /// (`src/nsterm.m:11576-11643`).
    fn default() -> Self {
        Self::gnu_ns_default()
    }
}

impl ModifierPolicy {
    /// GNU Cocoa defaults: Option -> meta, Command -> super, Control ->
    /// control, fn -> none, every right-side variable -> `left'.
    pub const fn gnu_ns_default() -> Self {
        Self {
            command: ModifierFamily {
                left: ModifierAssignment::uniform(Some(EmacsModifierKey::Super)),
                right: SideAssignment::InheritLeft,
            },
            option: ModifierFamily {
                left: ModifierAssignment::uniform(Some(EmacsModifierKey::Meta)),
                right: SideAssignment::InheritLeft,
            },
            ctrl: ModifierFamily {
                left: ModifierAssignment::uniform(Some(EmacsModifierKey::Ctrl)),
                right: SideAssignment::InheritLeft,
            },
            function: ModifierAssignment::uniform(None),
        }
    }

    /// The resolved assignment for a physical key.
    pub const fn assignment(&self, key: PhysicalModifierKey) -> ModifierAssignment {
        match key {
            PhysicalModifierKey::LeftCommand => self.command.left,
            PhysicalModifierKey::RightCommand => self.command.effective_right(),
            PhysicalModifierKey::LeftOption => self.option.left,
            PhysicalModifierKey::RightOption => self.option.effective_right(),
            PhysicalModifierKey::LeftControl => self.ctrl.left,
            PhysicalModifierKey::RightControl => self.ctrl.effective_right(),
            PhysicalModifierKey::Function => self.function,
        }
    }

    /// Whether Option acts as a command (control-like) modifier for KIND,
    /// considering both sides.
    ///
    /// `src/nsterm.m:2722-2730` consults `nil_or_none' on the resolved
    /// values when re-deriving a character with `UCKeyTranslate'; the
    /// function-kind slot matters for function-key chords because the same
    /// resolution is consulted with the event's kind.
    pub fn option_is_command_like(&self, kind: ModifierEventKind) -> bool {
        self.family_bits_for_option(Some(ModifierSideState::LeftOnly), kind) != 0
            || self.family_bits_for_option(Some(ModifierSideState::RightOnly), kind) != 0
    }

    /// Per-side Option mapping, the shape winit's `set_option_as_alt`
    /// needs.
    ///
    /// A side is command-like when EITHER of its ordinary or function slots
    /// maps to a real modifier: GNU re-derives characters with
    /// `UCKeyTranslate' only when a control-like modifier is also down, and
    /// a chord on a function key consults the :function slot
    /// (`nsterm.m:2722-2725`).  Sides mapped to `none' must keep composing
    /// characters, which is exactly what winit's non-rewriting shapes do.
    pub fn option_as_alt_shape(&self) -> OptionAsAltShape {
        let left = self.family_bits_for_option(
            Some(ModifierSideState::LeftOnly),
            ModifierEventKind::Ordinary,
        ) != 0
            || self.family_bits_for_option(
                Some(ModifierSideState::LeftOnly),
                ModifierEventKind::Function,
            ) != 0;
        let right = self.family_bits_for_option(
            Some(ModifierSideState::RightOnly),
            ModifierEventKind::Ordinary,
        ) != 0
            || self.family_bits_for_option(
                Some(ModifierSideState::RightOnly),
                ModifierEventKind::Function,
            ) != 0;
        match (left, right) {
            (true, true) => OptionAsAltShape::Both,
            (true, false) => OptionAsAltShape::LeftOnly,
            (false, true) => OptionAsAltShape::RightOnly,
            (false, false) => OptionAsAltShape::None,
        }
    }

    fn family_bits_for_option(
        &self,
        side: Option<ModifierSideState>,
        kind: ModifierEventKind,
    ) -> u32 {
        family_bits(
            side,
            self.option.left.for_kind(kind),
            self.option.effective_right().for_kind(kind),
        )
    }

    /// `EV_MODIFIERS2` + `ev_modifiers_helper': cook raw facts for one event
    /// kind into transport bits.
    pub fn cook(&self, raw: RawModifiers, kind: ModifierEventKind) -> TransportModifierBits {
        let mut bits = TransportModifierBits::default();
        if raw.shift {
            bits.set(TransportModifierBit::Shift);
        }
        cook_family(
            &mut bits,
            raw.ctrl,
            self.ctrl.left.for_kind(kind),
            self.ctrl.effective_right().for_kind(kind),
        );
        cook_family(
            &mut bits,
            raw.command,
            self.command.left.for_kind(kind),
            self.command.effective_right().for_kind(kind),
        );
        cook_family(
            &mut bits,
            raw.option,
            self.option.left.for_kind(kind),
            self.option.effective_right().for_kind(kind),
        );
        bits
    }

    // -- builders (tests and the parser) -----------------------------------

    /// Replace BOTH sides of the Command family, the way a plain-symbol
    /// `ns-command-modifier' value does.
    pub fn with_command(self, value: EmacsModifierKey) -> Self {
        self.with_command_assignment(Some(value))
    }

    pub fn with_command_none(self) -> Self {
        self.with_command_assignment(None)
    }

    pub fn with_command_assignment(self, value: Option<EmacsModifierKey>) -> Self {
        Self {
            command: ModifierFamily {
                left: ModifierAssignment::uniform(value),
                right: self.command.right,
            },
            ..self
        }
    }

    /// Set the right Command side to its own mapping (GNU: a non-`left'
    /// right value).
    pub fn with_right_command(self, value: EmacsModifierKey) -> Self {
        Self {
            command: ModifierFamily {
                right: SideAssignment::Explicit(ModifierAssignment::uniform(Some(value))),
                ..self.command
            },
            ..self
        }
    }

    pub fn with_option(self, value: EmacsModifierKey) -> Self {
        self.with_option_assignment(Some(value), Some(value), Some(value))
    }

    pub fn with_option_none(self) -> Self {
        self.with_option_assignment(None, None, None)
    }

    /// Replace BOTH sides of the Option family with per-kind assignments.
    pub fn with_option_assignment(
        self,
        ordinary: Option<EmacsModifierKey>,
        function: Option<EmacsModifierKey>,
        mouse: Option<EmacsModifierKey>,
    ) -> Self {
        Self {
            option: ModifierFamily {
                left: ModifierAssignment {
                    ordinary,
                    function,
                    mouse,
                },
                right: self.option.right,
            },
            ..self
        }
    }

    pub fn with_right_option(self, value: EmacsModifierKey) -> Self {
        Self {
            option: ModifierFamily {
                right: SideAssignment::Explicit(ModifierAssignment::uniform(Some(value))),
                ..self.option
            },
            ..self
        }
    }
}

/// `parse_solitary_modifier' applied to a resolved value.
const fn modifier_bit(value: Option<EmacsModifierKey>) -> u32 {
    match value {
        None => 0,
        Some(key) => TransportModifierBit::for_modifier_key(key).mask(),
    }
}

/// `ev_modifiers_helper' (`src/nsterm.m:371-395`).
fn cook_family(
    bits: &mut TransportModifierBits,
    side: Option<ModifierSideState>,
    left: Option<EmacsModifierKey>,
    right: Option<EmacsModifierKey>,
) {
    bits.merge_mask(family_bits(side, left, right));
}

/// The modifier mask one family cooks to, mirroring `ev_modifiers_helper`'s
/// left/right arithmetic.
///
/// * `None`: the family's either-mask is up, so nothing cooks
///   (`nsterm.m:378`).
/// * `Some(Unknown)`: the mask is down but the side was not identified, so
///   LEFT applies (`nsterm.m:387-391`).
/// * `Some(LeftOnly)` / `Some(RightOnly)`: the identified side's value
///   (`:384-391`); a right value of `left' resolves to the left value, so
///   both arithmetics agree.
/// * `Some(Both)`: GNU parses right first and then left (`:384-391`), so
///   the two cooked masks OR together.
const fn family_bits(
    side: Option<ModifierSideState>,
    left: Option<EmacsModifierKey>,
    right: Option<EmacsModifierKey>,
) -> u32 {
    let Some(side) = side else {
        return 0;
    };
    match side {
        ModifierSideState::Unknown => modifier_bit(left),
        ModifierSideState::LeftOnly => modifier_bit(left),
        ModifierSideState::RightOnly => modifier_bit(right),
        ModifierSideState::Both => modifier_bit(left) | modifier_bit(right),
    }
}

#[cfg(test)]
mod tests;
