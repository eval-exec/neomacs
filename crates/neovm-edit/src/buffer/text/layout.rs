use crate::buffer::position::{CharLen, CharPos0, EmacsByteLen, EmacsBytePos};
use crate::buffer::text::TextMetrics;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::buffer) struct GapCompatState {
    pos: CharPos0,
    byte_len: EmacsByteLen,
}

impl GapCompatState {
    pub(in crate::buffer) const fn new(pos: CharPos0, byte_len: EmacsByteLen) -> Self {
        Self { pos, byte_len }
    }

    pub(in crate::buffer) const fn pos(self) -> CharPos0 {
        self.pos
    }

    pub(in crate::buffer) const fn byte_len(self) -> EmacsByteLen {
        self.byte_len
    }

    pub(in crate::buffer) fn lisp_position(self) -> i64 {
        self.pos.to_lisp().as_i64()
    }

    pub(in crate::buffer) fn lisp_size(self) -> i64 {
        self.byte_len.get() as i64
    }

    pub(in crate::buffer) const fn with_pos(self, pos: CharPos0) -> Self {
        Self {
            pos,
            byte_len: self.byte_len,
        }
    }

    pub(in crate::buffer) const fn with_byte_len(self, byte_len: EmacsByteLen) -> Self {
        Self {
            pos: self.pos,
            byte_len,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(in crate::buffer) struct GapDebugLayout {
    pub(in crate::buffer) gpt: CharPos0,
    pub(in crate::buffer) z: CharPos0,
    pub(in crate::buffer) gpt_byte: EmacsBytePos,
    pub(in crate::buffer) z_byte: EmacsBytePos,
    pub(in crate::buffer) gap_byte_len: EmacsByteLen,
}

impl GapDebugLayout {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) const fn compat_state(self) -> GapCompatState {
        GapCompatState::new(self.gpt, self.gap_byte_len)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(in crate::buffer) enum TextBackendDebugLayout {
    Gap(GapDebugLayout),
    PieceTree(TextMetrics),
    Rope(TextMetrics),
}

impl TextBackendDebugLayout {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) fn metrics(self) -> TextMetrics {
        match self {
            Self::Gap(layout) => TextMetrics::from_lengths(
                CharLen::new(layout.z.get()),
                EmacsByteLen::new(layout.z_byte.get()),
            ),
            Self::PieceTree(metrics) => metrics,
            Self::Rope(metrics) => metrics,
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(in crate::buffer) fn gap(self) -> Option<GapDebugLayout> {
        match self {
            Self::Gap(layout) => Some(layout),
            Self::PieceTree(_) | Self::Rope(_) => None,
        }
    }
}

impl Default for TextBackendDebugLayout {
    fn default() -> Self {
        Self::Gap(GapDebugLayout::default())
    }
}
