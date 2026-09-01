use expect_test::expect;

use super::ParityBatchCase;

fn asilea_default_acceptance_always_accepts_lower_energy_for_unit_interval_draws() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_default_acceptance_always_accepts_lower_energy_for_unit_interval_draws",
        r##"(mapcar
         (lambda (spec)
           (let (limits)
             (list
              spec
              (asilea-default-acceptance-function
               (nth 0 spec)
               (nth 1 spec)
               (nth 2 spec)
               (lambda (limit)
                 (push limit limits)
                 (nth 3 spec)))
              (nreverse limits))))
         '((9 10 1 0.0)
           (9 10 1 0.999999)
           (0 100 0.001 0.999999)
           (-10 -9 100 0.999999)
           (9.5 10.0 2.0 0.75)))"##,
        expect![
            "OK (((9 10 1 0.0) t (1.0)) ((9 10 1 0.999999) t (1.0)) ((0 100 0.001 0.999999) t (1.0)) ((-10 -9 100 0.999999) t (1.0)) ((9.5 10.0 2.0 0.75) t (1.0)))"
        ],
    )
}

fn asilea_default_acceptance_equal_energy_uses_strict_comparison_at_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_default_acceptance_equal_energy_uses_strict_comparison_at_one",
        r##"(mapcar
         (lambda (draw)
           (list
            draw
            (asilea-default-acceptance-function
             10 10 5
             (lambda (_limit) draw))))
         '(0.0 0.5 0.999999 1.0 1.5 -1.0))"##,
        expect!["OK ((0.0 t) (0.5 t) (0.999999 t) (1.0 nil) (1.5 nil) (-1.0 t))"],
    )
}

fn asilea_default_acceptance_worse_energy_respects_temperature_probability_thresholds()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_default_acceptance_worse_energy_respects_temperature_probability_thresholds",
        r##"(mapcar
         (lambda (spec)
           (let* ((new (nth 0 spec))
                  (old (nth 1 spec))
                  (temperature (nth 2 spec))
                  (threshold
                   (exp
                    (/ (- old new)
                       temperature))))
             (list
              new
              old
              temperature
              threshold
              (mapcar
               (lambda (draw)
                 (list
                  draw
                  (asilea-default-acceptance-function
                   new old temperature
                   (lambda (_limit) draw))))
               (list
                0.0
                (/ threshold 2.0)
                threshold
                (min
                 0.999999
                 (* threshold 2.0)))))))
         '((11 10 1.0)
           (20 10 10.0)
           (100 0 25.0)
           (10.5 10.0 0.5)))"##,
        expect![
            "OK ((11 10 1.0 0.36787944117144233 ((0.0 t) (0.18393972058572117 t) (0.36787944117144233 nil) (0.7357588823428847 nil))) (20 10 10.0 0.36787944117144233 ((0.0 t) (0.18393972058572117 t) (0.36787944117144233 nil) (0.7357588823428847 nil))) (100 0 25.0 0.01831563888873418 ((0.0 t) (0.00915781944436709 t) (0.01831563888873418 nil) (0.03663127777746836 nil))) (10.5 10.0 0.5 0.36787944117144233 ((0.0 t) (0.18393972058572117 t) (0.36787944117144233 nil) (0.7357588823428847 nil))))"
        ],
    )
}

fn asilea_default_acceptance_calls_random_function_once_with_float_limit() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_default_acceptance_calls_random_function_once_with_float_limit",
        r##"(let (calls)
         (list
          (asilea-default-acceptance-function
           12 10 4
           (lambda (&rest arguments)
             (push arguments calls)
             0.25))
          (nreverse calls)))"##,
        expect!["OK (t ((1.0)))"],
    )
}

fn asilea_default_acceptance_invalid_numeric_and_callback_inputs_signal_exact_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_default_acceptance_invalid_numeric_and_callback_inputs_signal_exact_errors",
        r##"(mapcar
         (lambda (spec)
           (condition-case error-data
               (list
                spec
                :ok
                (asilea-default-acceptance-function
                 (nth 0 spec)
                 (nth 1 spec)
                 (nth 2 spec)
                 (nth 3 spec)))
             (error
              (list
               (seq-take spec 3)
               :error
               (car error-data)
               (cdr error-data)))))
         `((10 9 0 ,(lambda (_limit) 0.5))
           (10 9 -1 ,(lambda (_limit) 0.5))
           ("10" 9 1 ,(lambda (_limit) 0.5))
           (10 nil 1 ,(lambda (_limit) 0.5))
           (10 9 "hot" ,(lambda (_limit) 0.5))
           (10 9 1 nil)
           (10 9 1 ,(lambda (_limit) "draw"))))"##,
        expect![[
            r#"OK (((10 9 0) :error arith-error nil) ((10 9 -1 #[(_limit) (0.5) (t)]) :ok t) (("10" 9 1) :error wrong-type-argument (number-or-marker-p "10")) ((10 nil 1) :error wrong-type-argument (number-or-marker-p nil)) ((10 9 "hot") :error wrong-type-argument (number-or-marker-p "hot")) ((10 9 1) :error void-function (nil)) ((10 9 1) :error wrong-type-argument (number-or-marker-p "draw")))"#
        ]],
    )
}

pub(super) fn acceptance_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_default_acceptance_always_accepts_lower_energy_for_unit_interval_draws(),
        asilea_default_acceptance_equal_energy_uses_strict_comparison_at_one(),
        asilea_default_acceptance_worse_energy_respects_temperature_probability_thresholds(),
        asilea_default_acceptance_calls_random_function_once_with_float_limit(),
        asilea_default_acceptance_invalid_numeric_and_callback_inputs_signal_exact_errors(),
    ]
}
