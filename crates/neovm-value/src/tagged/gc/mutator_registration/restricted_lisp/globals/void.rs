//! Reserved descriptor reuse, then real page growth, on distinct VOID cells.
use super::tests::{GlobalParts, case_many, words};
use super::*;
use crate::tagged::gc::PlainSymbolBacking;

fn debt(owner: &mut Mutator<'_>) -> usize {
    must(with_run(owner, |run| Ok(run.state().hot.bytes_since_gc)))
}

pub(super) fn p3sym_c3_void_siblings_are_independent_and_page_growth_is_charged<H: GlobalParts>(
    host: &mut H,
) {
    // The first VOID reserves 256 descriptors, one consumed by VOID itself.
    // 257 first writes exceed that page; the final sibling is never written.
    const WRITES: u16 = 257;
    let kinds = vec![PlainSymbolBacking::VoidProjection; usize::from(WRITES) + 1];
    let mut ops = vec![Op::ConstFixnum { dst: 0, value: 97 }];
    for symbol in 0..WRITES {
        ops.extend([
            Op::Global(GlobalOp::Makunbound { symbol, dst: 1 }),
            Op::Global(GlobalOp::Boundp { symbol, dst: 1 }),
            Op::Global(GlobalOp::Set {
                symbol,
                src: 0,
                dst: 1,
            }),
            Op::Global(GlobalOp::Value { symbol, dst: 1 }),
            Op::Global(GlobalOp::Boundp {
                symbol: WRITES,
                dst: 1,
            }),
        ]);
    }
    case_many(host, &kinds, ops, 2, |_era, session, owner, frame, ids| {
        let mut baseline = None;
        for symbol in 0..WRITES {
            let before_unbind = debt(owner);
            assert_eq!(session.step(owner, frame), Ok(Step::Progress));
            assert_eq!(
                words(owner, frame)[1],
                TaggedValue::from_sym_id(ids[usize::from(symbol)]).bits()
            );
            assert_eq!(
                debt(owner),
                before_unbind,
                "makunbound on VOID needs no descriptor"
            );
            assert_eq!(session.step(owner, frame), Ok(Step::Progress));
            assert_eq!(words(owner, frame)[1], TaggedValue::NIL.bits());
            let before = debt(owner);
            assert_eq!(session.step(owner, frame), Ok(Step::Progress));
            assert_eq!(words(owner, frame)[1], TaggedValue::fixnum(97).bits());
            if symbol == 0 {
                baseline = Some(before);
                assert_eq!(debt(owner), before, "first write reuses reserved storage");
            }
            assert_eq!(session.step(owner, frame), Ok(Step::Progress));
            assert_eq!(words(owner, frame)[1], TaggedValue::fixnum(97).bits());
            assert_eq!(session.step(owner, frame), Ok(Step::Progress));
            assert_eq!(
                words(owner, frame)[1],
                TaggedValue::NIL.bits(),
                "sibling VOID must remain unbound after every materialization"
            );
        }
        assert_eq!(session.step(owner, frame), Ok(Step::Done));
        let charged = debt(owner) - baseline.unwrap();
        let page = 256 * size_of::<crate::forward::ThreadProjectionDescriptor>();
        assert!(
            charged >= page,
            "new page must be charged, got {charged}, page {page}"
        );
        assert!(
            charged < page + 8192,
            "one page plus small pointer-vector growth, got {charged}"
        );
        eprintln!(
            "P3SYM_C3_VOID_GROWTH writes={WRITES} untouched=1 charged={charged} descriptor_page_bytes={page}"
        );
    });
}
