//! Recover the actual Context, then traverse its ordinary initial obarray.
//! No serial intern may repair a missing C5 member before this audit.
use super::*;
use crate::emacs_core::print_value;

fn printed(ctx: &mut Context, form: &str) -> String {
    print_value(&ctx.eval_str(form).expect("C5 serial audit Lisp form"))
}

fn audit(scenario: Scenario) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut ctx = Context::new();
    // Use the real serial Lisp surface, including registered builtin function
    // cells and its startup initial-obarray proxy. Retire the default weak
    // table BEFORE any test name; these Context rows are not Miri fixtures.
    ctx.set_variable("window-dead-windows-table", Value::NIL);
    ctx.gc_collect_exact();
    let prefix = format!("p3sym-c5-serial-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let spellings: Vec<_> = (0..2)
        .map(|index| {
            format!(
                "{prefix}-{}",
                if scenario == Scenario::DifferentNames {
                    index
                } else {
                    0
                }
            )
        })
        .collect();
    let oracle: Vec<_> = include_str!("../../../symbol_intern_oracle.expected")
        .lines()
        .collect();
    assert_eq!(oracle.len(), 24);
    let soft_forms: Vec<_> = spellings
        .iter()
        .map(|spelling| format!("(intern-soft \"{spelling}\")"))
        .collect();
    for (spelling, form) in spellings.iter().zip(&soft_forms) {
        assert!(crate::emacs_core::intern::lookup_interned(spelling).is_none());
        assert_eq!(
            printed(&mut ctx, form),
            oracle[0],
            "GNU soft miss before era"
        );
    }
    let filtered = format!(
        r#"(let ((p3sym-c5-audit-count 0))
        (mapatoms (lambda (p3sym-c5-audit-member)
          (if (or (equal (symbol-name p3sym-c5-audit-member) "{}")
                    (equal (symbol-name p3sym-c5-audit-member) "{}"))
            (setq p3sym-c5-audit-count (1+ p3sym-c5-audit-count)))))
        p3sym-c5-audit-count)"#,
        spellings[0], spellings[1]
    );
    let all = "(let ((p3sym-c5-audit-count 0)) (mapatoms (lambda (_p3sym-c5-member) (setq p3sym-c5-audit-count (1+ p3sym-c5-audit-count)))) p3sym-c5-audit-count)";
    // Warm the identical traversal/forms and their bound variable names before
    // baseline. Later reader/callback setup cannot hide extra World membership.
    assert_eq!(
        printed(&mut ctx, &filtered),
        oracle[11],
        "GNU mapatoms soft miss is absent"
    );
    ctx.eval_str(all).unwrap();
    let holders: Vec<_> = (0..2)
        .map(|index| {
            let id = crate::emacs_core::intern::intern(&format!("p3sym-c5-serial-holder-{index}"));
            ctx.obarray
                .set_symbol_value_id(id, Value::string(&spellings[index]));
            id
        })
        .collect();
    // Holder installation also changed membership. Warm the traversal again
    // so the serial cache is current immediately before C5 alone changes it.
    assert_eq!(printed(&mut ctx, &filtered), oracle[11]);
    assert_eq!(
        ctx.eval_str(all).unwrap().as_fixnum(),
        Some(ctx.obarray.len() as i64)
    );
    let mut gap = crate::emacs_core::intern::intern_uninterned("p3sym-c5-serial-gap");
    let target = (u64::from(gap.0) / 4096 + 2) * 4096 + 17;
    while u64::from(gap.0) + 1 < target {
        gap = crate::emacs_core::intern::intern_uninterned("p3sym-c5-serial-gap");
    }
    let before: BTreeSet<_> = ctx.obarray.global_member_ids().map(|id| id.0).collect();
    assert_eq!(
        ctx.obarray.len(),
        before.len(),
        "actual initial count before era"
    );
    let receipt = ctx
        .admit_owned_parallel(|projection| {
            // Only scalar holder identities/scenario enter this closure. There is
            // no Context/evaluator/Obarray view until both capabilities return.
            let result = exercise(projection.era(), &holders, scenario);
            let (collector, keeper) = projection
                .finish()
                .expect("C5 serial audit clean era finish");
            (collector, keeper, result)
        })
        .expect("C5 serial audit owned admission")
        .expect("C5 serial audit history after normal Context recovery");
    assert_history(&receipt, scenario, None, true);
    assert!(
        ctx.obarray.admission_matches_canonical(),
        "returning serial view refreshed after actual spine growth"
    );
    let after: BTreeSet<_> = ctx.obarray.global_member_ids().map(|id| id.0).collect();
    let winners: BTreeSet<_> = receipt.rows.iter().map(|row| row.id.0).collect();
    assert!(
        before.is_subset(&after),
        "growth preserves every old member"
    );
    assert_eq!(
        after.difference(&before).copied().collect::<BTreeSet<_>>(),
        winners
    );
    assert_eq!(
        ctx.obarray.len(),
        after.len(),
        "actual count equals serial member iterator"
    );
    assert_eq!(after.len(), before.len() + winners.len());
    for (form, row) in soft_forms.iter().zip(&receipt.rows) {
        let value = ctx.eval_str(form).unwrap();
        assert_eq!(
            value.as_bare_symbol_id(),
            Some(row.id),
            "serial intern-soft must observe actual C5 winner; a second miss is not evidence"
        );
    }
    assert_eq!(
        printed(&mut ctx, &filtered),
        oracle[if scenario == Scenario::DifferentNames {
            9
        } else {
            6
        }],
        "GNU filtered mapatoms count"
    );
    assert_eq!(
        ctx.eval_str(all).unwrap().as_fixnum(),
        Some(after.len() as i64),
        "real unfiltered mapatoms count equals actual initial membership"
    );
    assert_eq!(
        ctx.obarray.len(),
        after.len(),
        "serial soft/traversal must not repair or add members"
    );
    eprintln!(
        "P3SYM_C5_SERIAL_MAPATOMS {scenario:?} winners={} count_before={} count_after={} chunks_added={}",
        winners.len(),
        before.len(),
        after.len(),
        receipt.after.chunks - receipt.before.chunks
    );
}

#[test]
fn p3sym_c5_serial_mapatoms_counts_same_name_winner_once_after_era() {
    audit(Scenario::SameName);
}

#[test]
fn p3sym_c5_serial_mapatoms_counts_distinct_name_growth_after_era() {
    audit(Scenario::DifferentNames);
}
