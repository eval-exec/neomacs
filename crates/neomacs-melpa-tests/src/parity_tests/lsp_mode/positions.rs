use expect_test::expect;

use super::ParityBatchCase;

fn negotiated_position_encodings_round_trip_real_unicode_source_locations() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "alpha 😀 café\n東京 λ-report\n")
  (let* ((emoji-start
          (progn
            (goto-char (point-min))
            (search-forward "😀")
            (match-beginning 0)))
         (emoji-end (match-end 0))
         (accent-end
          (progn
            (search-forward "é")
            (match-end 0)))
         (tokyo-end
          (progn
            (search-forward "東京")
            (match-end 0)))
         (points (list emoji-start emoji-end accent-end tokyo-end (point-max))))
    (mapcar
     (lambda (encoding)
       (lsp--set-position-encoding encoding)
       (list
        :encoding encoding
        :locations
        (mapcar
         (lambda (source-point)
           (let* ((position (lsp--point-to-position source-point))
                  ;; A real language server sends the outbound position back
                  ;; through JSON, changing the local plist into LSP Mode's
                  ;; configured wire representation.
                  (wire-position
                   (lsp--read-json (lsp--json-serialize position)))
                  (round-trip (lsp--position-to-point wire-position)))
             (list
              :position (neomacs-lsp-test-position-shape position)
              :line-prefix
              (buffer-substring-no-properties
               (save-excursion
                 (goto-char round-trip)
                 (line-beginning-position))
               round-trip)
              :next-character (char-after round-trip)
              :round-trip (= source-point round-trip))))
         points)))
     '("utf-16" "utf-8" "utf-32"))))
"##;
    let expected = expect![[
        r##"OK ((:encoding "utf-16" :locations ((:position (0 6) :line-prefix "alpha " :next-character 128512 :round-trip t) (:position (0 8) :line-prefix "alpha 😀" :next-character 32 :round-trip t) (:position (0 13) :line-prefix "alpha 😀 café" :next-character 10 :round-trip t) (:position (1 2) :line-prefix "東京" :next-character 32 :round-trip t) (:position (2 0) :line-prefix "" :next-character nil :round-trip t))) (:encoding "utf-8" :locations ((:position (0 6) :line-prefix "alpha " :next-character 128512 :round-trip t) (:position (0 10) :line-prefix "alpha 😀" :next-character 32 :round-trip t) (:position (0 16) :line-prefix "alpha 😀 café" :next-character 10 :round-trip t) (:position (1 6) :line-prefix "東京" :next-character 32 :round-trip t) (:position (2 0) :line-prefix "" :next-character nil :round-trip t))) (:encoding "utf-32" :locations ((:position (0 6) :line-prefix "alpha " :next-character 128512 :round-trip t) (:position (0 7) :line-prefix "alpha 😀" :next-character 32 :round-trip t) (:position (0 12) :line-prefix "alpha 😀 café" :next-character 10 :round-trip t) (:position (1 2) :line-prefix "東京" :next-character 32 :round-trip t) (:position (2 0) :line-prefix "" :next-character nil :round-trip t))))"##
    ]];
    ParityBatchCase::value(
        "negotiated_position_encodings_round_trip_real_unicode_source_locations",
        elisp_form,
        expected,
    )
}

pub(super) fn positions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![negotiated_position_encodings_round_trip_real_unicode_source_locations()]
}
