//! Bounded native discovery answers. Scoring and font opening remain callers'
//! responsibilities; every argument affecting native enumeration is in the key.
use super::{FcQueryKind, ListedFont};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Query {
    family: Option<String>,
    ranges: Vec<(u32, u32)>,
    required_char: Option<u32>,
    langs: Vec<String>,
    kind: FcQueryKind,
}

impl Query {
    pub(super) fn new(
        family: Option<&str>,
        ranges: &[(u32, u32)],
        required_char: Option<u32>,
        langs: &[String],
        kind: FcQueryKind,
    ) -> Option<Self> {
        // Oversized requests remain valid native queries, but cannot enlarge
        // the reusable cache or allocate an unbounded duplicate key.
        if ranges.len() > 1024
            || langs.len() > 64
            || family.map_or(0, str::len) + langs.iter().map(String::len).sum::<usize>() > 65536
        {
            return None;
        }
        Some(Self {
            family: family.map(str::to_owned),
            ranges: ranges.to_vec(),
            required_char,
            langs: langs.to_vec(),
            kind,
        })
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.family.as_ref().map_or(0, String::len)
            + self.ranges.len() * std::mem::size_of::<(u32, u32)>()
            + self
                .langs
                .iter()
                .map(|s| std::mem::size_of::<String>() + s.len())
                .sum::<usize>()
    }
}

pub(super) struct CandidateQueries {
    entries: VecDeque<(Query, Vec<ListedFont>, usize)>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}
impl Default for CandidateQueries {
    fn default() -> Self {
        Self::with_limits(128, 16 * 1024 * 1024)
    }
}
impl CandidateQueries {
    fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_entries,
            max_bytes,
        }
    }
    pub(super) fn get(&mut self, query: &Query) -> Option<Vec<ListedFont>> {
        let index = self.entries.iter().position(|(key, _, _)| key == query)?;
        let entry = self.entries.remove(index)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }
    pub(super) fn insert(&mut self, query: Query, fonts: &[ListedFont]) {
        let bytes = query.bytes() + fonts.iter().map(font_bytes).sum::<usize>();
        if self.max_entries == 0 || bytes > self.max_bytes {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(key, _, _)| *key == query) {
            self.bytes -= self.entries.remove(index).unwrap().2;
        }
        while self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes {
            self.bytes -= self.entries.pop_front().unwrap().2;
        }
        self.bytes += bytes;
        self.entries.push_back((query, fonts.to_vec(), bytes));
    }
}
fn font_bytes(font: &ListedFont) -> usize {
    let matched = &font.matched;
    std::mem::size_of::<ListedFont>()
        + matched.family.len()
        + font.style.len()
        + [&matched.file, &matched.postscript_name, &font.foundry]
            .into_iter()
            .map(|s| s.as_ref().map_or(0, String::len))
            .sum::<usize>()
        + matched.variation_coords.len()
            * std::mem::size_of::<neomacs_display_protocol::font::FontVariationCoord>()
}

#[cfg(test)]
#[path = "tests/candidate_cache_test.rs"]
mod tests;
