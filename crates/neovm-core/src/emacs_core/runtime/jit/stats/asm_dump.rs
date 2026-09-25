//! `NEOVM_JIT_DUMP_ASM=<path>`: append the final machine code of every JIT
//! leaf (baseline, MIR, OSR) to `path` — Cranelift's post-register-allocation
//! disassembly (physical registers, spills and reloads visible), plus the
//! finalized code bytes at their real address so a perf sample's
//! `sym+offset` can be located with objdump.
//!
//! Cranelift only renders the disassembly when asked (`Context::set_disasm`),
//! so with the knob unset the compile does no extra work: the builders ask
//! [`want_disasm`] (one cached read per compile) and never touch this module
//! again. Compile-time only; nothing here runs when leaves execute.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::perf_map::LabelTier;

/// The disassembly of the function just defined on this thread, waiting for
/// its wrapper to learn the finalized address.
struct PendingAsm {
    vcode: String,
    size: usize,
}

thread_local! {
    static PENDING: RefCell<Option<PendingAsm>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static PATH_OVERRIDE: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

/// Point the dump at `path` (or turn it off) for compiles on this thread
/// (tests only), without the process-global environment variable.
#[cfg(test)]
pub(crate) fn force_asm_dump_for_test(path: Option<PathBuf>) {
    PATH_OVERRIDE.with(|p| *p.borrow_mut() = Some(path));
}

/// The dump file, or `None` when `NEOVM_JIT_DUMP_ASM` is unset.
fn dump_path() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(forced) = PATH_OVERRIDE.with(|p| p.borrow().clone()) {
        return forced;
    }
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os("NEOVM_JIT_DUMP_ASM").map(PathBuf::from))
        .clone()
}

/// Whether the environment asks for the dump (read once; part of
/// `naming_enabled`, so dumped leaves carry their Lisp names).
pub(crate) fn dump_requested_by_env() -> bool {
    std::env::var_os("NEOVM_JIT_DUMP_ASM").is_some()
}

/// Whether a JIT compile should ask Cranelift for its disassembly. AOT
/// objects are never dumped (their code lives in a `.so`).
pub(crate) fn want_disasm(aot: bool) -> bool {
    !aot && dump_path().is_some()
}

/// Keep the disassembly of the function `ctx` just compiled (called right
/// after `define_function`, before the context is cleared).
pub(crate) fn stash(ctx: &cranelift_codegen::Context) {
    let Some(code) = ctx.compiled_code() else {
        return;
    };
    let pending = PendingAsm {
        vcode: code.vcode.clone().unwrap_or_default(),
        size: code.code_buffer().len(),
    };
    PENDING.with(|p| *p.borrow_mut() = Some(pending));
}

/// What the wrapper knows about the leaf it just finalized.
pub(crate) struct AsmLeafInfo<'a> {
    pub(crate) tier: LabelTier,
    /// The declared entry name (the perf-map label under naming).
    pub(crate) entry_name: &'a str,
    pub(crate) entry: *const u8,
    pub(crate) regalloc: &'static str,
    pub(crate) clif_insts: u32,
}

/// Append the stashed disassembly for the leaf just finalized at
/// `info.entry`. A no-op when nothing was stashed (the knob is off).
pub(crate) fn flush(info: &AsmLeafInfo<'_>) {
    let Some(pending) = PENDING.with(|p| p.borrow_mut().take()) else {
        return;
    };
    let Some(path) = dump_path() else {
        return;
    };
    let text = render(info, &pending);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => {
            let _ = file.write_all(text.as_bytes());
        }
        Err(err) => {
            tracing::warn!(
                target: "neovm_jit",
                path = %path.display(),
                %err,
                "NEOVM_JIT_DUMP_ASM cannot be opened"
            );
        }
    }
}

fn render(info: &AsmLeafInfo<'_>, pending: &PendingAsm) -> String {
    let (name, id) = match super::perf_map::label_parts(info.entry_name) {
        Some((name, id)) => (name.to_string(), id.to_string()),
        None => ("-".to_string(), "-".to_string()),
    };
    let tier = match info.tier {
        LabelTier::Baseline => "baseline".to_string(),
        LabelTier::Mir => "mir".to_string(),
        LabelTier::Osr(pc) => format!("osr@{pc}"),
    };
    let addr = info.entry as usize;
    let mut out = format!(
        ";; ==== {} name={name} id={id} tier={tier} addr={addr:#x} size={:#x} regalloc={} clif_insts={}\n",
        info.entry_name, pending.size, info.regalloc, info.clif_insts,
    );
    out.push_str(&pending.vcode);
    if !pending.vcode.ends_with('\n') {
        out.push('\n');
    }
    // The finalized bytes (relocations applied), for
    // `xxd -r -p | objdump -D -b binary -mi386:x86-64 --adjust-vma=<addr>`.
    out.push_str(";; bytes:\n");
    if !info.entry.is_null() && pending.size > 0 {
        // SAFETY: `entry` is the start of this leaf's finalized code, which
        // is `size` bytes long, mapped readable and never unmapped while the
        // leaf lives (the wrapper still owns its module).
        let bytes = unsafe { std::slice::from_raw_parts(info.entry, pending.size) };
        for chunk in bytes.chunks(32) {
            for b in chunk {
                out.push_str(&format!("{b:02x}"));
            }
            out.push('\n');
        }
    }
    out.push('\n');
    out
}
