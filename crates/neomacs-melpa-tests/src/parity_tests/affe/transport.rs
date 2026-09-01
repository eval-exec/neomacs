use expect_test::expect;

use super::ParityBatchCase;

fn affe_send_serializes_symbols_lists_unicode_and_embedded_newlines() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_send_serializes_symbols_lists_unicode_and_embedded_newlines",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push (list process string)
                             calls)
                       'sent)))
                 (list
                  (affe--send 'client
                              '(search 20 "α" "a\nb"))
                  (affe--send 'client 'exit)
                  (nreverse calls))))"##,
        expect![[r#"OK (sent sent ((client "(search 20 \"α\" \"a\\nb\")\n") (client "exit\n")))"#]],
    )
}

fn affe_connect_frames_fragmented_lines_flushes_tail_and_builds_local_socket_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_connect_frames_fragmented_lines_flushes_tail_and_builds_local_socket_process",
        r##"(let (arguments callbacks)
               (cl-letf
                   (((symbol-function
                      'make-network-process)
                     (lambda (&rest args)
                       (setq arguments args)
                       'network-client)))
                 (let ((result
                        (affe--connect
                         "affe-socket"
                         (lambda (lines)
                           (push lines callbacks)))))
                   (let ((filter
                          (plist-get arguments :filter))
                         (sentinel
                          (plist-get arguments
                                     :sentinel)))
                     (funcall filter nil "one")
                     (funcall filter nil
                              " two\nthree\nfour")
                     (funcall filter nil "\nfive\n")
                     (funcall sentinel nil "open")
                     (funcall filter nil "tail")
                     (funcall sentinel nil "closed")
                     (list
                      result
                      (plist-get arguments :name)
                      (plist-get arguments :noquery)
                      (plist-get arguments :coding)
                      (plist-get arguments :family)
                      (file-name-nondirectory
                       (plist-get arguments
                                  :service))
                      (nreverse callbacks))))))"##,
        expect![[
            r#"OK (network-client "affe-socket" t utf-8 local "affe-socket" (("one two" "three") ("four" "five") ("tail")))"#
        ]],
    )
}

fn affe_connect_keeps_independent_fragment_state_per_connection() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_connect_keeps_independent_fragment_state_per_connection",
        r##"(let (processes first second)
               (cl-letf
                   (((symbol-function
                      'make-network-process)
                     (lambda (&rest arguments)
                       (push arguments processes)
                       (length processes))))
                 (affe--connect
                  "first"
                 (lambda (lines)
                    (push lines first)))
                 (let ((first-filter
                        (plist-get (car processes)
                                   :filter))
                       (first-sentinel
                        (plist-get (car processes)
                                   :sentinel)))
                   (affe--connect
                    "second"
                    (lambda (lines)
                      (push lines second)))
                   (let ((second-filter
                          (plist-get (car processes)
                                     :filter))
                         (second-sentinel
                          (plist-get (car processes)
                                     :sentinel)))
                     (funcall first-filter nil "a")
                     (funcall second-filter nil "b\n")
                     (funcall first-sentinel nil "closed")
                     (funcall second-filter nil "c")
                     (funcall second-sentinel nil "closed")
                     (list
                      (nreverse first)
                      (nreverse second))))))"##,
        expect![[r#"OK ((("a")) (("b") ("c")))"#]],
    )
}

pub(super) fn transport_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_send_serializes_symbols_lists_unicode_and_embedded_newlines(),
        affe_connect_frames_fragmented_lines_flushes_tail_and_builds_local_socket_process(),
        affe_connect_keeps_independent_fragment_state_per_connection(),
    ]
}
