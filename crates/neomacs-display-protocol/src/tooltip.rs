//! Owned tooltip presentation requests. Scheduling belongs to the producer.
mod lifetime;
pub use lifetime::{TooltipClient, TooltipContext, TooltipGeneration, TooltipTicket};

#[derive(Clone, Debug, PartialEq)]
pub struct MenuTooltips {
    pub appearance: TooltipRequest,
    pub delay: std::time::Duration,
    pub short_delay: std::time::Duration,
    pub recent: std::time::Duration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TooltipRequest {
    pub generation: Option<TooltipGeneration>,
    pub text: String,
    pub runs: Vec<TooltipTextRun>,
    pub offset: (i32, i32),
    pub timeout: std::time::Duration,
    pub foreground: Option<u32>,
    pub background: Option<u32>,
    pub border: Option<u32>,
    pub border_width: u32,
    pub padding: u32,
    pub max_size: Option<TooltipLimits>,
}

impl Default for TooltipRequest {
    fn default() -> Self {
        Self {
            generation: None,
            text: String::new(),
            runs: Vec::new(),
            offset: (5, -10),
            timeout: std::time::Duration::from_secs(10),
            foreground: None,
            background: None,
            border: None,
            border_width: 1,
            padding: 2,
            max_size: None,
        }
    }
}

impl TooltipRequest {
    /// Placement, timeout and delivery identity do not change painted content.
    pub fn same_content(&self, other: &Self) -> bool {
        self == &Self {
            offset: self.offset,
            timeout: self.timeout,
            generation: self.generation,
            ..other.clone()
        }
    }
}

/// Character-indexed styling, with an exact replayable font when available.
#[derive(Clone, Debug, PartialEq)]
pub struct TooltipTextRun {
    pub range: std::ops::Range<usize>,
    pub face: crate::face::Face,
    pub font: Option<crate::font::ResolvedFont>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipLimits {
    pub columns: std::num::NonZeroU32,
    pub rows: std::num::NonZeroU32,
}

impl TooltipLimits {
    pub fn new(columns: u32, rows: u32) -> Option<Self> {
        Some(Self {
            columns: std::num::NonZeroU32::new(columns)?,
            rows: std::num::NonZeroU32::new(rows)?,
        })
    }
}
