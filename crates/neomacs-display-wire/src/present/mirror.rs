//! The sender's mirror of the receiver's row store (DESIGN §6.10).
//!
//! The receiver never evicts on its own, so the sender always knows exactly
//! what each slot holds (the X Render GlyphSet model). A row is found first
//! by `Arc` identity (a memo that keeps the row alive, so its address cannot
//! be reused), then by content key; otherwise it takes a free slot or the
//! least recently used slot this update does not reference, and a
//! `DefineRow` is emitted.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use neomacs_display_protocol::{GlyphRow, MatrixRow};

use super::{F32Bits, PresentOp, RowSlot, StoreLimits};
use crate::codec::WireError;
use crate::payload::Payload;
use crate::rows::{RowFlags, RowKey, RowPlacement, RowRuns};

/// Addresses remembered per slot.
const MEMO_PER_SLOT: usize = 4;

/// What a row's successor needs to place itself relative to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowMeta {
    pub(crate) height_px: F32Bits,
    pub(crate) continued: bool,
}

impl RowMeta {
    pub(crate) fn of(runs: &RowRuns) -> Self {
        Self {
            height_px: F32Bits::of(runs.height_px),
            continued: runs.flags.contains(RowFlags::CONTINUED),
        }
    }
}

#[derive(Debug)]
struct SlotEntry {
    key: RowKey,
    bytes: u64,
    last_used: u64,
}

#[derive(Debug)]
struct Memo {
    // Held so the row's address cannot be reused while it is memoized.
    _row: MatrixRow,
    slot: RowSlot,
    key: RowKey,
    placement: RowPlacement,
    meta: RowMeta,
}

/// One update's resolution state: the slots it references and the rows it
/// defines.
#[derive(Debug, Default)]
pub(crate) struct UpdateRows {
    pub(crate) used: HashSet<RowSlot>,
    pub(crate) defines: Vec<PresentOp>,
}

#[derive(Debug)]
pub(crate) struct StoreMirror {
    limits: StoreLimits,
    slots: Vec<Option<SlotEntry>>,
    by_key: HashMap<RowKey, RowSlot>,
    memo: HashMap<usize, Memo>,
    memo_by_slot: HashMap<RowSlot, Vec<usize>>,
    bytes: u64,
    clock: u64,
}

impl StoreMirror {
    pub(crate) fn new(limits: StoreLimits) -> Self {
        Self {
            limits,
            slots: Vec::new(),
            by_key: HashMap::new(),
            memo: HashMap::new(),
            memo_by_slot: HashMap::new(),
            bytes: 0,
            clock: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.limits);
    }

    fn touch(&mut self, slot: RowSlot, update: &mut UpdateRows) {
        self.clock += 1;
        if let Some(Some(entry)) = self.slots.get_mut(slot.0 as usize) {
            entry.last_used = self.clock;
        }
        update.used.insert(slot);
    }

    fn evict(&mut self, slot: RowSlot) {
        if let Some(entry) = self.slots[slot.0 as usize].take() {
            self.bytes -= entry.bytes;
            self.by_key.remove(&entry.key);
        }
        for address in self.memo_by_slot.remove(&slot).unwrap_or_default() {
            self.memo.remove(&address);
        }
    }

    /// The least recently used slot this update does not reference.
    fn victim(&self, update: &UpdateRows) -> Option<RowSlot> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let slot = RowSlot(index as u32);
                entry
                    .as_ref()
                    .filter(|_| !update.used.contains(&slot))
                    .map(|entry| (entry.last_used, slot))
            })
            .min()
            .map(|(_, slot)| slot)
    }

    fn allocate(&mut self, bytes: u64, update: &UpdateRows) -> Result<RowSlot, WireError> {
        if bytes > self.limits.bytes {
            return Err(WireError::Encode(format!(
                "a {bytes}-byte row exceeds the {}-byte row store",
                self.limits.bytes
            )));
        }
        while self.bytes + bytes > self.limits.bytes {
            let victim = self.victim(update).ok_or_else(|| {
                WireError::Encode("one presentation does not fit the row store".into())
            })?;
            self.evict(victim);
        }
        if let Some(free) = self.slots.iter().position(Option::is_none) {
            return Ok(RowSlot(free as u32));
        }
        if (self.slots.len() as u64) < u64::from(self.limits.rows) {
            self.slots.push(None);
            return Ok(RowSlot(self.slots.len() as u32 - 1));
        }
        let victim = self.victim(update).ok_or_else(|| {
            WireError::Encode("one presentation has more rows than the row store".into())
        })?;
        self.evict(victim);
        Ok(victim)
    }

    /// The slot holding `row`'s content, defining it when the receiver lacks
    /// it; `shared` is the row's copy-on-write handle when it has one.
    pub(crate) fn resolve(
        &mut self,
        row: &GlyphRow,
        shared: Option<&MatrixRow>,
        update: &mut UpdateRows,
    ) -> Result<(RowSlot, RowPlacement, RowMeta), WireError> {
        let address = shared.map(|row| std::ptr::from_ref::<GlyphRow>(row) as usize);
        if let Some(memo) = address.and_then(|address| self.memo.get(&address)) {
            let (slot, key, placement, meta) = (memo.slot, memo.key, memo.placement, memo.meta);
            if self.by_key.get(&key) == Some(&slot) {
                self.touch(slot, update);
                return Ok((slot, placement, meta));
            }
        }
        let (runs, placement) = RowRuns::of_row(row)?;
        let meta = RowMeta::of(&runs);
        let (key, bytes) = runs.keyed_bytes()?;
        let slot = match self.by_key.get(&key) {
            Some(slot) => *slot,
            None => {
                let slot = self.allocate(bytes.len() as u64, update)?;
                self.slots[slot.0 as usize] = Some(SlotEntry {
                    key,
                    bytes: bytes.len() as u64,
                    last_used: 0,
                });
                self.bytes += bytes.len() as u64;
                self.by_key.insert(key, slot);
                update.defines.push(PresentOp::DefineRow {
                    slot,
                    key,
                    row: Payload::Shared(Arc::new(runs)),
                });
                slot
            }
        };
        self.touch(slot, update);
        if let (Some(address), Some(shared)) = (address, shared) {
            self.memo.insert(
                address,
                Memo {
                    _row: shared.clone(),
                    slot,
                    key,
                    placement,
                    meta,
                },
            );
            let addresses = self.memo_by_slot.entry(slot).or_default();
            addresses.retain(|known| *known != address);
            addresses.push(address);
            // A row re-laid out every frame gets a new address each time;
            // keep only the recent ones so the memo stays bounded.
            if addresses.len() > MEMO_PER_SLOT {
                let stale = addresses.remove(0);
                self.memo.remove(&stale);
            }
        }
        Ok((slot, placement, meta))
    }
}
