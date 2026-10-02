//! Controls for bounded folded whole-literal searches.
//!
//! | Knob | Values | Default | Purpose |
//! |---|---|---|---|
//! | `NEOVM_REGEX_SHORT_LITERAL` | `on`, `off` (boolean aliases accepted) | off | Compare short folded multibyte whole literals directly. |

/// The process-wide switch contains no Lisp state. Each mutator sees the
/// same immutable boolean after OnceLock publishes it.
pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(value) = OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        super::regex_knob_on(std::env::var("NEOVM_REGEX_SHORT_LITERAL").ok().as_deref())
    })
}

#[cfg(test)]
thread_local! {
    // A test policy flag, never a Lisp-state cache or production structure.
    static OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(super) fn with_enabled<R>(value: bool, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(OVERRIDE.with(|slot| slot.replace(Some(value))));
    run()
}

use super::{CompiledPattern, MatchRegisters, RegexOp, emacs_char, re_text_char, re_tr};

/// Whole-literal codes derived from sealed bytecode. They are immutable after
/// compilation and contain no Lisp values, so the descriptor itself permits
/// concurrent reads. Its owner retains the existing CaseTranslation mutator
/// ownership: this optimization introduces no translation cache or sharing.
#[derive(Clone)]
pub(super) struct Literal {
    codes: Box<[u32]>,
}

impl Literal {
    pub(super) fn compile(pattern: &CompiledPattern) -> Option<Self> {
        if !enabled()
            || !pattern.buffer_sealed
            || !pattern.multibyte
            || pattern.translate.is_none()
            || pattern.re_nsub != 0
            || pattern.buffer.len() > 40
        {
            return None;
        }
        let mut literal = Vec::new();
        let mut pc = 0;
        loop {
            let op = *pattern.buffer.get(pc)?;
            pc += 1;
            if op == RegexOp::Exactn as u8 {
                let count = *pattern.buffer.get(pc)? as usize;
                pc += 1;
                literal.extend_from_slice(pattern.buffer.get(pc..pc + count)?);
                pc += count;
                if literal.len() > 32 {
                    return None;
                }
            } else if op == RegexOp::NoOp as u8 {
                continue;
            } else if op == RegexOp::Succeed as u8 || op == RegexOp::PosixEnd as u8 {
                if pc != pattern.buffer.len() {
                    return None;
                }
                break;
            } else {
                return None;
            }
        }
        // ASCII-only literals already use the existing fastmap/scanners.
        if !literal.iter().any(|&byte| byte >= 0x80) {
            return None;
        }
        let mut codes = Vec::new();
        let mut at = 0;
        while at < literal.len() {
            let (code, width) = emacs_char::string_char(&literal[at..]);
            codes.push(code);
            at += width;
        }
        Some(Self {
            codes: codes.into_boxed_slice(),
        })
    }

    /// GNU search.c's trivial_regexp_p/simple_search character comparison,
    /// bounded to the spans for which the SIMD prefilter is deliberately
    /// cold. Unsupported representations and backward search use the old
    /// matcher. The outer Option distinguishes fallback from a failed search.
    pub(super) fn search(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        start: usize,
        range: isize,
    ) -> Option<Option<(usize, MatchRegisters)>> {
        if !pattern.target_multibyte || range < 0 || start > text.len() {
            return None;
        }
        let stop = start.saturating_add(range as usize).min(text.len());
        if stop - start >= super::PREFILTER_MIN_BUILD_SPAN {
            return None;
        }
        if crate::emacs_core::eval::tls_quit_pending() {
            return None;
        }
        let mut candidate = start;
        while candidate < stop {
            // Match starts, as in re_search, must be character boundaries.
            if text[candidate] & 0xc0 == 0x80 {
                candidate += 1;
                continue;
            }
            let mut at = candidate;
            let mut matched = true;
            for &code in &self.codes {
                if at >= stop {
                    matched = false;
                    break;
                }
                let (actual, width) = re_text_char(text, at, true)?;
                if width > stop - at || re_tr(&pattern.translate, actual) != code {
                    matched = false;
                    break;
                }
                at += width;
            }
            if matched {
                let mut registers = MatchRegisters::new(1);
                registers.start[0] = candidate as i64;
                registers.end[0] = at as i64;
                return Some(Some((candidate, registers)));
            }
            candidate += re_text_char(text, candidate, true)?.1;
        }
        Some(None)
    }
}
