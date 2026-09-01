use expect_test::expect;

use super::ParityBatchCase;

fn asilea_start_process_builds_pipe_process_with_exact_program_and_flattened_options()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_start_process_builds_pipe_process_with_exact_program_and_flattened_options",
        r##"(let ((options
                [["-O0" "-O3"]
                 [nil ("-march=native" "-mtune=native")]
                 ["file name.c"]])
               calls
               created-buffer)
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'start-process)
                   (lambda (&rest arguments)
                     (push
                      (list
                       arguments
                       process-connection-type
                       (buffer-live-p
                        (nth 1 arguments))
                       (buffer-name
                        (nth 1 arguments)))
                      calls)
                     :fake-process)))
               (let ((result
                      (asilea--start-process
                       "/opt/compiler driver"
                       [1 1 0]
                       options)))
                 (setq created-buffer
                       (nth 1
                            (car
                             (car calls))))
                 (list
                  result
                  (nreverse calls))))
           (when
               (buffer-live-p created-buffer)
             (kill-buffer created-buffer))))"##,
        expect![[
            r#"OK (:fake-process ((("/opt/compiler driver" (:buffer nil) "/opt/compiler driver" "-O3" "-march=native" "-mtune=native" "file name.c") nil t " *asilea process output*")))"#
        ]],
    )
}

fn asilea_start_process_creates_unique_internal_buffers_for_repeated_jobs() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_start_process_creates_unique_internal_buffers_for_repeated_jobs",
        r##"(let ((options [["-O2"]])
               buffers
               calls)
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'start-process)
                   (lambda (name buffer program &rest arguments)
                     (push buffer buffers)
                     (push
                      (list
                       name
                       (if
                           (string-equal
                            (buffer-name buffer)
                            " *asilea process output*")
                           :base-name
                         :unique-suffixed-name)
                       (string-prefix-p
                        " *asilea process output*"
                        (buffer-name buffer))
                       program
                       arguments
                       process-connection-type)
                      calls)
                     (length calls))))
               (list
                (asilea--start-process
                 "compiler" [0] options)
                (asilea--start-process
                 "compiler" [0] options)
                (nreverse calls)
                (mapcar #'buffer-live-p buffers)
                (apply #'eq buffers)))
           (dolist (buffer buffers)
             (when
                 (buffer-live-p buffer)
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (1 2 (("compiler" :base-name t "compiler" ("-O2") nil) ("compiler" :unique-suffixed-name t "compiler" ("-O2") nil)) (t t) nil)"#
        ]],
    )
}

fn asilea_start_process_omits_nil_groups_and_preserves_duplicate_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_start_process_omits_nil_groups_and_preserves_duplicate_arguments",
        r##"(let ((options
                [[nil]
                 [("x" "x" "")]
                 ["λ"]
                 [nil]])
               captured-buffer)
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'start-process)
                   (lambda (&rest arguments)
                     (setq captured-buffer
                           (nth 1 arguments))
                     arguments)))
               (asilea--start-process
                "driver"
                [0 0 0 0]
                options))
           (when
               (buffer-live-p captured-buffer)
             (kill-buffer captured-buffer))))"##,
        expect![[r#"OK ("driver" (:buffer nil) "driver" "x" "x" "" "λ")"#]],
    )
}

fn asilea_start_process_surfaces_state_shape_and_start_process_errors_without_leaking_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_start_process_surfaces_state_shape_and_start_process_errors_without_leaking_contract",
        r##"(let ((options [["x"]])
               created)
         (cl-letf
             (((symbol-function
                'generate-new-buffer)
               (lambda (name)
                 (let ((buffer
                        (get-buffer-create name)))
                   (push buffer created)
                   buffer)))
              ((symbol-function
                'start-process)
               (lambda (&rest arguments)
                 (signal
                  'file-missing
                  (list
                   "Searching for program"
                   "No such file"
                   (nth 2 arguments))))))
           (unwind-protect
               (mapcar
                (lambda (spec)
                  (condition-case error-data
                      (list
                       spec
                       :ok
                       (asilea--start-process
                        (car spec)
                        (cadr spec)
                        (caddr spec)))
                    (error
                     (list
                      spec
                      :error
                      (car error-data)
                      (cdr error-data)))))
                `(("missing-driver" [0] ,options)
                  ("driver" [1] ,options)
                  ("driver" [] ,options)))
             (dolist (buffer created)
               (when
                   (buffer-live-p buffer)
                 (kill-buffer buffer))))))"##,
        expect![[
            r#"OK ((("missing-driver" [0] #1=[#2=["x"]]) :error file-missing ("Searching for program" "No such file" "missing-driver")) (("driver" [1] #1#) :error args-out-of-range (#2# 1)) (("driver" [] #1#) :error file-missing ("Searching for program" "No such file" "driver")))"#
        ]],
    )
}

pub(super) fn process_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_start_process_builds_pipe_process_with_exact_program_and_flattened_options(),
        asilea_start_process_creates_unique_internal_buffers_for_repeated_jobs(),
        asilea_start_process_omits_nil_groups_and_preserves_duplicate_arguments(),
        asilea_start_process_surfaces_state_shape_and_start_process_errors_without_leaking_contract(
        ),
    ]
}
