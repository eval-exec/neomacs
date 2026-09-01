use expect_test::expect;

use super::ParityBatchCase;

fn asilea_sanitize_configuration_covers_step_and_temperature_requirement_matrix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_sanitize_configuration_covers_step_and_temperature_requirement_matrix",
        r##"(mapcar
         (lambda (spec)
           (let ((asilea-max-steps (nth 0 spec))
                 (asilea-initial-temperature (nth 1 spec))
                 (asilea-final-temperature (nth 2 spec)))
             (condition-case error-data
                 (list
                  spec
                  :ok
                  (asilea--sanitize-variables))
               (error
                (list
                 spec
                 :error
                 (car error-data)
                 (cdr error-data))))))
         '((nil nil nil)
           (nil 10 nil)
           (nil nil 1)
           (nil 10 1)
           (1 nil nil)
           (1 10 nil)
           (0 nil nil)
           (-1 nil nil)
           ("steps" nil nil)))"##,
        expect![[
            r#"OK (((nil nil nil) :error error ("At least one of ‘asilea-max-steps’ and ‘asilea-initial-temperature’ must be non-nil")) ((nil 10 nil) :error error ("At least one of ‘asilea-max-steps’ and ‘asilea-final-temperature’ must be non-nil")) ((nil nil 1) :error error ("At least one of ‘asilea-max-steps’ and ‘asilea-initial-temperature’ must be non-nil")) ((nil 10 1) :ok nil) ((1 nil nil) :ok nil) ((1 10 nil) :ok nil) ((0 nil nil) :ok nil) ((-1 nil nil) :ok nil) (("steps" nil nil) :ok nil))"#
        ]],
    )
}

fn asilea_initial_temperature_honors_explicit_values_without_coercion() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_initial_temperature_honors_explicit_values_without_coercion",
        r##"(mapcar
         (lambda (value)
           (let ((asilea-initial-temperature value)
                 (asilea-max-steps 100)
                 (asilea-cooling-rate 0.005))
             (list
              value
              (asilea--initial-temperature)
              (eq
               value
               (asilea--initial-temperature)))))
         '(1 1.0 0 -3 1/2 "hot" symbol (10)))"##,
        expect![[
            r#"OK ((1 1 t) (1.0 1.0 t) (0 0 t) (-3 -3 t) (1/2 1/2 t) ("hot" "hot" t) (symbol symbol t) (#1=(10) #1# t))"#
        ]],
    )
}

fn asilea_automatic_initial_temperature_matches_cooling_schedule_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_automatic_initial_temperature_matches_cooling_schedule_boundaries",
        r##"(mapcar
         (lambda (spec)
           (let ((asilea-initial-temperature nil)
                 (asilea-max-steps (car spec))
                 (asilea-cooling-rate (cadr spec)))
             (condition-case error-data
                 (let ((temperature
                        (asilea--initial-temperature)))
                   (list
                    spec
                    :ok
                    temperature
                    (* temperature
                       (expt
                        (- 1.0 asilea-cooling-rate)
                        asilea-max-steps))))
               (error
                (list
                 spec
                 :error
                 (car error-data)
                 (cdr error-data))))))
         '((1 0.005)
           (10 0.005)
           (100 0.005)
           (1000 0.005)
           (5 0.5)
           (2 0.9)
           (0 0.005)
           (-1 0.005)
           (10 0.0)
           (10 1.0)
           (nil 0.005)
           ("10" 0.005)))"##,
        expect![[
            r#"OK (((1 0.005) :ok 2.0 1.99) ((10 0.005) :ok 2.0 1.9022202609315437) ((100 0.005) :ok 2.0 1.2115408729814559) ((1000 0.005) :ok 151.0 1.0047492554036268) ((5 0.5) :ok 32.0 1.0) ((2 0.9) :ok 101.0 1.0099999999999996) ((0 0.005) :ok 1.0 1.0) ((-1 0.005) :ok 1.0 1.0050251256281406) ((10 0.0) :ok 1.0 1.0) ((10 1.0) :ok 1.0e+INF -0.0e+NaN) ((nil 0.005) :error wrong-type-argument (numberp nil)) (("10" 0.005) :error wrong-type-argument (numberp "10")))"#
        ]],
    )
}

fn asilea_sanitize_only_enforces_documented_temperature_presence_not_other_types() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asilea_sanitize_only_enforces_documented_temperature_presence_not_other_types",
        r##"(mapcar
         (lambda (bindings)
           (let ((asilea-max-steps (nth 0 bindings))
                 (asilea-initial-temperature (nth 1 bindings))
                 (asilea-final-temperature (nth 2 bindings))
                 (asilea-concurrent-jobs (nth 3 bindings))
                 (asilea-cooling-rate (nth 4 bindings))
                 (asilea-random-generator-function
                  (nth 5 bindings)))
             (condition-case error-data
                 (list
                  bindings
                  :ok
                  (asilea--sanitize-variables))
               (error
                (list
                 bindings
                 :error
                 (car error-data)
                 (cdr error-data))))))
         '((1 nil nil 0 2.0 nil)
           (1 nil nil -5 -1.0 missing)
           ("one" nil nil "jobs" "cool" 7)
           (nil "hot" "cold" nil nil nil)))"##,
        expect![[
            r#"OK (((1 nil nil 0 2.0 nil) :ok nil) ((1 nil nil -5 -1.0 missing) :ok nil) (("one" nil nil "jobs" "cool" 7) :ok nil) ((nil "hot" "cold" nil nil nil) :ok nil))"#
        ]],
    )
}

fn asilea_configuration_variables_support_independent_dynamic_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_configuration_variables_support_independent_dynamic_bindings",
        r##"(let ((defaults
                (mapcar
                 #'symbol-value
                 '(asilea-concurrent-jobs
                   asilea-max-steps
                   asilea-cooling-rate
                   asilea-initial-temperature
                   asilea-final-temperature))))
         (list
          defaults
          (let ((asilea-concurrent-jobs 4)
                (asilea-max-steps 9)
                (asilea-cooling-rate 0.25)
                (asilea-initial-temperature 80)
                (asilea-final-temperature 2))
            (mapcar
             #'symbol-value
             '(asilea-concurrent-jobs
               asilea-max-steps
               asilea-cooling-rate
               asilea-initial-temperature
               asilea-final-temperature)))
          (mapcar
           #'symbol-value
           '(asilea-concurrent-jobs
             asilea-max-steps
             asilea-cooling-rate
             asilea-initial-temperature
             asilea-final-temperature))))"##,
        expect!["OK ((1 nil 0.005 nil nil) (4 9 0.25 80 2) (1 nil 0.005 nil nil))"],
    )
}

pub(super) fn configuration_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_sanitize_configuration_covers_step_and_temperature_requirement_matrix(),
        asilea_initial_temperature_honors_explicit_values_without_coercion(),
        asilea_automatic_initial_temperature_matches_cooling_schedule_boundaries(),
        asilea_sanitize_only_enforces_documented_temperature_presence_not_other_types(),
        asilea_configuration_variables_support_independent_dynamic_bindings(),
    ]
}
