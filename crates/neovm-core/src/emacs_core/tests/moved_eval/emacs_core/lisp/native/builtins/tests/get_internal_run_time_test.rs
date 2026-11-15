use neovm_value::value::list_to_vec;

#[test]
fn get_internal_run_time_tracks_cpu_and_output_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    let before_sleep = eval
        .eval_str("(float-time (get-internal-run-time))")
        .expect("get-internal-run-time before sleep")
        .as_float()
        .expect("run time is convertible to a float");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let after_sleep = eval
        .eval_str("(float-time (get-internal-run-time))")
        .expect("get-internal-run-time after sleep")
        .as_float()
        .expect("run time is convertible to a float");
    let sleep_delta = after_sleep - before_sleep;
    assert!(
        sleep_delta < 0.1,
        "sleep accrued {sleep_delta:.3}s CPU time"
    );

    let busy_delta = eval
        .eval_str(
            "(let ((start (float-time (get-internal-run-time))) (n 0))\
             (while (< n 500000) (setq n (1+ n)))\
             (- (float-time (get-internal-run-time)) start))",
        )
        .expect("get-internal-run-time around busy loop")
        .as_float()
        .expect("busy-loop delta is a float");
    assert!(
        busy_delta > 0.001,
        "busy loop accrued {busy_delta:.3}s CPU time"
    );

    let list_value = eval
        .eval_str("(let ((current-time-list t)) (get-internal-run-time))")
        .expect("legacy get-internal-run-time shape");
    let fields = list_to_vec(&list_value).expect("legacy time value is a proper list");
    assert_eq!(fields.len(), 4);
    assert!(fields.iter().all(|field| field.is_integer()));
    #[cfg(not(windows))]
    assert_eq!(fields[3].as_fixnum(), Some(0));

    #[cfg(windows)]
    {
        let nil_list = eval
            .eval_str("(let ((current-time-list nil)) (get-internal-run-time))")
            .expect("Windows GNU-compatible get-internal-run-time shape");
        assert_eq!(
            list_to_vec(&nil_list)
                .expect("Windows time value list")
                .len(),
            4
        );
    }
    #[cfg(not(windows))]
    {
        let ticks_hz = eval
            .eval_str("(let ((current-time-list nil)) (get-internal-run-time))")
            .expect("ticks/hz get-internal-run-time shape");
        assert!(ticks_hz.is_cons());
        assert!(ticks_hz.cons_car().is_integer());
        assert_eq!(ticks_hz.cons_cdr().as_fixnum(), Some(1_000_000));
    }
}
