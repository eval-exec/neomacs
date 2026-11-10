//! Callback redefinition at every mapcar resume index.
//!
//! `runtime_startup_context` is minted in this crate. One fresh context per
//! snapshot, matching the original loop. The private map callee stays in eval.

#[test]
fn map_resume_observes_callback_redefinition_at_every_start_index() {
    fn snapshot(split: usize) -> String {
        let mut eval = crate::test_utils::runtime_startup_context();
        neovm_eval::test_support::authority::map_resume_redefining_snapshot_for_test(
            &mut eval, split,
        )
    }
    let expected = snapshot(0);
    for split in 1..=6 {
        assert_eq!(snapshot(split), expected, "split={split}");
    }
}
