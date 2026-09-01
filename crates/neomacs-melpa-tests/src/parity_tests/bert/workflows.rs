use expect_test::expect;

use super::ParityBatchCase;

fn an_external_erlang_reply_fixture_decodes_and_reencodes_byte_for_byte() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_external_erlang_reply_fixture_decodes_and_reencodes_byte_for_byte",
        r##"
(let* ((wire
        (unibyte-string
         131 104 4
         100 0 4 98 101 114 116
         100 0 5 114 101 112 108 121
         97 200
         109 0 0 0 8 97 99 99 101 112 116 101 100))
       (decoded (bert-unpack wire))
       (repacked (bert-pack decoded)))
  (list :decoded decoded
        :bytes (length wire)
        :hex (neomacs-bert-test-hex wire)
        :repacked-byte-for-byte (equal repacked wire)
        :unibyte (not (multibyte-string-p repacked))))
"##,
        expect![[
            r##"OK (:decoded [bert reply 200 "accepted"] :bytes 33 :hex "836804640004626572746400057265706c7961c86d000000086163636570746564" :repacked-byte-for-byte t :unibyte t)"##
        ]],
    )
}

fn an_rpc_request_round_trips_nested_tuples_lists_atoms_and_floats() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_rpc_request_round_trips_nested_tuples_lists_atoms_and_floats",
        r##"
(let* ((request
        [call inventory reserve
              (["SKU-42" 3] ["SKU-9" 1])
              [trace "7f9c" 19.875 -0.125]])
       (wire (bert-pack request))
       (decoded (bert-unpack wire)))
  (list :request decoded
        :round-trip (equal decoded request)
        :wire-bytes (length wire)
        :wire-prefix (neomacs-bert-test-hex
                      (substring wire 0 (min 32 (length wire))))
        :tuple-tag (aref wire 1)
        :magic (aref wire 0)))
"##,
        expect![[
            r##"OK (:request [call inventory reserve (["SKU-42" 3] ["SKU-9" 1]) [trace "7f9c" 19.875 -0.125]] :round-trip t :wire-bytes 150 :wire-prefix "83680564000463616c6c640009696e76656e746f727964000772657365727665" :tuple-tag 104 :magic 131)"##
        ]],
    )
}

fn signed_metric_boundaries_use_the_required_small_and_network_integer_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "signed_metric_boundaries_use_the_required_small_and_network_integer_tags",
        r##"
(let* ((metrics '(0 1 254 255 256 -1 -42 536870911 -536870912))
       (frames (mapcar #'bert-pack metrics)))
  (list :decoded (mapcar #'bert-unpack frames)
        :tags (mapcar (lambda (frame) (aref frame 1)) frames)
        :hex (mapcar #'neomacs-bert-test-hex frames)
        :all-unibyte
        (cl-every (lambda (frame) (not (multibyte-string-p frame))) frames)))
"##,
        expect![[
            r##"OK (:decoded (0 1 254 255 256 4294967295 4294967254 536870911 3758096384) :tags (97 97 97 97 98 98 98 98 98) :hex ("836100" "836101" "8361fe" "8361ff" "836200000100" "8362ffffffff" "8362ffffffd6" "83621fffffff" "8362e0000000") :all-unibyte t)"##
        ]],
    )
}

fn a_utf8_binary_frame_survives_disk_transport_without_implicit_recoding() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_utf8_binary_frame_survives_disk_transport_without_implicit_recoding",
        r##"
(let* ((path (expand-file-name "wire/event.bert"
                               (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (message "café 東京 🚀")
       (utf8 (encode-coding-string message 'utf-8 t))
       (wire (bert-pack utf8)))
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'no-conversion))
    (write-region wire nil path nil 'silent))
  (let ((read-back
         (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents-literally path)
           (buffer-string))))
    (let* ((decoded-bytes (bert-unpack read-back))
           (decoded (decode-coding-string
                     (apply #'unibyte-string
                            (string-to-list decoded-bytes))
                     'utf-8)))
      (list :decoded-codepoints (string-to-list decoded)
            :matches-original (equal decoded message)
            :payload-unibyte (not (multibyte-string-p utf8))
            :wire-unibyte (not (multibyte-string-p wire))
            :disk-byte-for-byte (equal read-back wire)
            :wire-bytes (length wire)
            :hex (neomacs-bert-test-hex wire)))))
"##,
        expect![[
            r##"OK (:decoded-codepoints (99 97 102 233 32 26481 20140 32 128640) :matches-original t :payload-unibyte t :wire-unibyte t :disk-byte-for-byte t :wire-bytes 23 :hex "836d00000011636166c3a920e69db1e4baac20f09f9a80")"##
        ]],
    )
}

fn bulk_result_tuples_cross_the_small_tuple_arity_boundary_without_data_loss() -> ParityBatchCase {
    ParityBatchCase::value(
        "bulk_result_tuples_cross_the_small_tuple_arity_boundary_without_data_loss",
        r##"
(let* ((small (vconcat (number-sequence 0 254)))
       (large (vconcat (number-sequence 0 255)))
       (small-wire (bert-pack small))
       (large-wire (bert-pack large))
       (small-decoded (bert-unpack small-wire))
       (large-decoded (bert-unpack large-wire)))
  (list :small (list :tag (aref small-wire 1)
                     :count (length small-decoded)
                     :last (aref small-decoded 254)
                     :prefix (neomacs-bert-test-hex (substring small-wire 0 8)))
        :large (list :tag (aref large-wire 1)
                     :count (length large-decoded)
                     :last (aref large-decoded 255)
                     :prefix (neomacs-bert-test-hex (substring large-wire 0 11)))
        :round-trips (list (equal small small-decoded)
                           (equal large large-decoded))))
"##,
        expect![[
            r##"OK (:small (:tag 104 :count 255 :last 254 :prefix "8368ff6100610161") :large (:tag 105 :count 256 :last 255 :prefix "8369000001006100610161") :round-trips (t t))"##
        ]],
    )
}

fn a_corrupt_external_term_magic_byte_is_rejected() -> ParityBatchCase {
    ParityBatchCase::signal(
        "a_corrupt_external_term_magic_byte_is_rejected",
        r##"
(bert-unpack (unibyte-string 130 106))
"##,
        expect![[r##"ERR (error "bad magic: 130")"##]],
    )
}

fn an_unsupported_external_bignum_is_rejected_explicitly() -> ParityBatchCase {
    ParityBatchCase::signal(
        "an_unsupported_external_bignum_is_rejected_explicitly",
        r##"
(bert-unpack (unibyte-string 131 110 1 0 1))
"##,
        expect![[r##"ERR (error "cannot decode bignums")"##]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        an_external_erlang_reply_fixture_decodes_and_reencodes_byte_for_byte(),
        an_rpc_request_round_trips_nested_tuples_lists_atoms_and_floats(),
        signed_metric_boundaries_use_the_required_small_and_network_integer_tags(),
        a_utf8_binary_frame_survives_disk_transport_without_implicit_recoding(),
        bulk_result_tuples_cross_the_small_tuple_arity_boundary_without_data_loss(),
        a_corrupt_external_term_magic_byte_is_rejected(),
        an_unsupported_external_bignum_is_rejected_explicitly(),
    ]
}
