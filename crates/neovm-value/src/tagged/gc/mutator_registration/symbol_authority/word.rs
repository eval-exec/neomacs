//! One-attempt adapters to proof 2's shared authority protocol.
//! Busy returns outside every loan, so the owner may service a stop.
#[cfg(test)]
use super::super::authority_word::{COUNT_MASK, WRITER};
pub(super) use super::super::authority_word::{
    quiescent, reader_exit, try_reader_enter as reader_enter, try_writer_enter as writer_enter,
    writer_exit,
};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn p3sym_word_refuses_writer_saturation_and_reserved_bits() {
        for occupied in [WRITER, COUNT_MASK, 1 << 50] {
            let word = AtomicU64::new(occupied);
            assert!(!reader_enter(&word));
            assert!(!writer_enter(&word));
            assert_eq!(word.load(Ordering::Relaxed), occupied);
        }
    }
}
