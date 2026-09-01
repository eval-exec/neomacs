use expect_test::expect;

use super::ParityBatchCase;

fn ascii_table_binary_formats_every_ascii_codepoint_at_fixed_width_without_collisions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_binary_formats_every_ascii_codepoint_at_fixed_width_without_collisions",
        r##"(let ((values
                (mapcar
                 #'ascii-table--binary
                 (number-sequence 0 127))))
         (list
          (length values)
          (length (delete-dups (copy-sequence values)))
          (mapcar
           (lambda (codepoint)
             (cons
              codepoint
              (nth codepoint values)))
           '(0 1 2 3 7 8 15 16 31 32 63 64 65 90 97 126 127))
          (seq-every-p
           (lambda (value)
             (and
              (= 7 (length value))
              (string-match-p "\\`[01]\\{7\\}\\'" value)))
           values)
          (secure-hash
           'sha256
           (mapconcat #'identity values "\n"))))"##,
        expect![[
            r#"OK (128 128 ((0 . "0000000") (1 . "0000001") (2 . "0000010") (3 . "0000011") (7 . "0000111") (8 . "0001000") (15 . "0001111") (16 . "0010000") (31 . "0011111") (32 . "0100000") (63 . "0111111") (64 . "1000000") (65 . "1000001") (90 . "1011010") (97 . "1100001") (126 . "1111110") (127 . "1111111")) t "94ef0d2f71c8df44044fdc81a3d58f524ea0623ad48625355e2b6f92bfc723f5")"#
        ]],
    )
}

fn ascii_table_binary_rejects_out_of_range_and_non_integer_inputs_with_exact_signals()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_binary_rejects_out_of_range_and_non_integer_inputs_with_exact_signals",
        r##"(mapcar
         (lambda (value)
           (condition-case error-data
               (list value :ok (ascii-table--binary value))
             (error
              (list
               value
               :error
               (car error-data)
               (cdr error-data)))))
         '(-100 -1 128 255 nil "65" 65.0 symbol))"##,
        expect![[
            r#"OK ((-100 :error cl-assertion-failed (#1=(<= 0 codepoint 127))) (-1 :error cl-assertion-failed (#1#)) (128 :error cl-assertion-failed (#1#)) (255 :error cl-assertion-failed (#1#)) (nil :error wrong-type-argument (number-or-marker-p nil)) ("65" :error wrong-type-argument (number-or-marker-p "65")) (65.0 :error wrong-type-argument (integerp 65.0)) (symbol :error wrong-type-argument (number-or-marker-p symbol)))"#
        ]],
    )
}

fn ascii_table_character_class_covers_full_ascii_domain_and_outside_boundaries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_character_class_covers_full_ascii_domain_and_outside_boundaries",
        r##"(let ((inputs
                (number-sequence -2 130)))
         (list
          (mapcar
           (lambda (codepoint)
             (list
              codepoint
              (ascii-table--character-class codepoint)))
           '(-2 -1 0 8 9 12 13 14 31 32 33 47 48 57
             58 64 65 70 71 90 91 96 97 102 103 122
             123 126 127 128 129 130))
          (mapcar
           (lambda (class)
             (list
              class
              (seq-count
               (lambda (codepoint)
                 (eq
                  class
                  (ascii-table--character-class codepoint)))
               inputs)))
           '(nil control space punct digit upper lower))
          (secure-hash
           'sha256
           (prin1-to-string
            (mapcar
             #'ascii-table--character-class
             inputs)))))"##,
        expect![[
            r#"OK (((-2 nil) (-1 nil) (0 control) (8 control) (9 space) (12 space) (13 space) (14 control) (31 control) (32 space) (33 punct) (47 punct) (48 digit) (57 digit) (58 punct) (64 punct) (65 upper) (70 upper) (71 upper) (90 upper) (91 punct) (96 punct) (97 lower) (102 lower) (103 lower) (122 lower) (123 punct) (126 punct) (127 control) (128 nil) (129 nil) (130 nil)) ((nil 5) (control 28) (space 6) (punct 32) (digit 10) (upper 26) (lower 26)) "92a1a86b511e30c04e4046a76c4598c6634b1e2da9090f1ed7a0141540769ea8")"#
        ]],
    )
}

fn ascii_table_class_face_maps_every_class_and_unknown_value_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_class_face_maps_every_class_and_unknown_value_exactly",
        r##"(mapcar
         (lambda (class)
           (list
            class
            (ascii-table--class-face class)))
         '(control punct space digit upper lower
           nil unknown 0 "control"))"##,
        expect![[
            r#"OK ((control font-lock-keyword-face) (punct font-lock-preprocessor-face) (space font-lock-string-face) (digit font-lock-function-name-face) (upper font-lock-variable-name-face) (lower font-lock-variable-name-face) (nil nil) (unknown nil) (0 nil) ("control" nil))"#
        ]],
    )
}

fn ascii_table_control_caret_names_cover_all_controls_and_reject_printable_characters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_control_caret_names_cover_all_controls_and_reject_printable_characters",
        r##"(let ((inputs
                (append
                 '(-2 -1)
                 (number-sequence 0 33)
                 '(64 126 127 128))))
         (list
          (mapcar
           (lambda (codepoint)
             (list
              codepoint
              (ascii-table--control-caret codepoint)
              (ascii-table--control-name codepoint)))
           inputs)
          (secure-hash
           'sha256
           (prin1-to-string
            (mapcar
             (lambda (codepoint)
               (list
                (ascii-table--control-caret codepoint)
                (ascii-table--control-name codepoint)))
             inputs)))))"##,
        expect![[
            r#"OK (((-2 nil nil) (-1 nil nil) (0 "^@" "NUL") (1 "^A" "SOH") (2 "^B" "STX") (3 "^C" "ETX") (4 "^D" "EOT") (5 "^E" "ENQ") (6 "^F" "ACK") (7 "^G" "BEL") (8 "^H" "BS") (9 "^I" "HT") (10 "^J" "LF") (11 "^K" "VT") (12 "^L" "FF") (13 "^M" "CR") (14 "^N" "SO") (15 "^O" "SI") (16 "^P" "DLE") (17 "^Q" "DC1") (18 "^R" "DC2") (19 "^S" "DC3") (20 "^T" "DC4") (21 "^U" "NAK") (22 "^V" "SYN") (23 "^W" "ETB") (24 "^X" "CAN") (25 "^Y" "EM") (26 "^Z" "SUB") (27 "^[" "ESC") (28 "^\\" "FS") (29 "^]" "GS") (30 "^^" "RS") (31 "^_" "US") (32 nil nil) (33 nil nil) (64 nil nil) (126 nil nil) (127 "^?" "DEL") (128 nil nil)) "ecd5e63db60bd117d6331b4dbf40d6782ef26062c5de680ba81d2ddccd06feba")"#
        ]],
    )
}

fn ascii_table_control_escape_covers_c_escape_range_escape_and_non_escape_controls()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_control_escape_covers_c_escape_range_escape_and_non_escape_controls",
        r##"(mapcar
         (lambda (codepoint)
           (list
            codepoint
            (ascii-table--control-escape codepoint)
            (ascii-table--control-name codepoint)
            (ascii-table--control-caret codepoint)))
         (append
          '(-1)
          (number-sequence 0 32)
          '(65 126 127 128)))"##,
        expect![[
            r#"OK ((-1 nil nil nil) (0 nil "NUL" "^@") (1 nil "SOH" "^A") (2 nil "STX" "^B") (3 nil "ETX" "^C") (4 nil "EOT" "^D") (5 nil "ENQ" "^E") (6 nil "ACK" "^F") (7 "\\a" "BEL" "^G") (8 "\\b" "BS" "^H") (9 "\\t" "HT" "^I") (10 "\\n" "LF" "^J") (11 "\\v" "VT" "^K") (12 "\\f" "FF" "^L") (13 "\\r" "CR" "^M") (14 nil "SO" "^N") (15 nil "SI" "^O") (16 nil "DLE" "^P") (17 nil "DC1" "^Q") (18 nil "DC2" "^R") (19 nil "DC3" "^S") (20 nil "DC4" "^T") (21 nil "NAK" "^U") (22 nil "SYN" "^V") (23 nil "ETB" "^W") (24 nil "CAN" "^X") (25 nil "EM" "^Y") (26 nil "SUB" "^Z") (27 "\\e" "ESC" "^[") (28 nil "FS" "^\\") (29 nil "GS" "^]") (30 nil "RS" "^^") (31 nil "US" "^_") (32 nil nil nil) (65 nil nil nil) (126 nil nil nil) (127 nil "DEL" "^?") (128 nil nil nil))"#
        ]],
    )
}

fn ascii_table_default_hex_table_places_every_codepoint_once_with_exact_contents_and_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_default_hex_table_places_every_codepoint_once_with_exact_contents_and_faces",
        r##"(let* ((ascii-table-base 16)
                (ascii-table-control nil)
                (ascii-table-escape nil)
                (table (ascii-table--table 8))
                (rows 16)
                decoded)
         (dotimes (codepoint 128)
           (let* ((row (mod codepoint rows))
                  (col (truncate codepoint rows))
                  (cell
                   (* 2 (+ (* 8 row) col)))
                  (code-pair (aref table cell))
                  (name-pair (aref table (1+ cell))))
             (push
              (list
               codepoint
               row
               col
               code-pair
               name-pair)
              decoded)))
         (setq decoded (nreverse decoded))
         (list
          (length table)
          (length decoded)
          (seq-every-p
           (lambda (entry)
             (equal
              (cadddr entry)
              (cons
               (format "%02X" (car entry))
               'font-lock-comment-face)))
           decoded)
          (mapcar
           (lambda (codepoint)
             (nth codepoint decoded))
           '(0 7 8 9 10 13 27 31 32 33 47 48
             57 64 65 90 91 96 97 122 126 127))
          (secure-hash
           'sha256
           (prin1-to-string decoded))))"##,
        expect![[
            r#"OK (512 128 t ((0 0 0 ("00" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 7 0 ("07" . font-lock-comment-face) ("BEL" . font-lock-keyword-face)) (8 8 0 ("08" . font-lock-comment-face) ("BS" . font-lock-keyword-face)) (9 9 0 ("09" . font-lock-comment-face) ("HT" . font-lock-string-face)) (10 10 0 ("0A" . font-lock-comment-face) ("LF" . font-lock-string-face)) (13 13 0 ("0D" . font-lock-comment-face) ("CR" . font-lock-string-face)) (27 11 1 ("1B" . font-lock-comment-face) ("ESC" . font-lock-keyword-face)) (31 15 1 ("1F" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 0 2 ("20" . font-lock-comment-face) (" " . font-lock-string-face)) (33 1 2 ("21" . font-lock-comment-face) ("!" . font-lock-preprocessor-face)) (47 15 2 ("2F" . font-lock-comment-face) ("/" . font-lock-preprocessor-face)) (48 0 3 ("30" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (57 9 3 ("39" . font-lock-comment-face) ("9" . font-lock-function-name-face)) (64 0 4 ("40" . font-lock-comment-face) ("@" . font-lock-preprocessor-face)) (65 1 4 ("41" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (90 10 5 ("5A" . font-lock-comment-face) ("Z" . font-lock-variable-name-face)) (91 11 5 ("5B" . font-lock-comment-face) ("[" . font-lock-preprocessor-face)) (96 0 6 ("60" . font-lock-comment-face) ("`" . font-lock-preprocessor-face)) (97 1 6 ("61" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (122 10 7 ("7A" . font-lock-comment-face) ("z" . font-lock-variable-name-face)) (126 14 7 ("7E" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 15 7 ("7F" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "f387ba32e2e86117a934630937d6ff8ae166b082b13208f4cabfefda69e0a66a")"#
        ]],
    )
}

fn ascii_table_table_configuration_matrix_changes_radix_control_and_escape_precedence()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_table_configuration_matrix_changes_radix_control_and_escape_precedence",
        r##"(let (results)
         (dolist (base '(2 8 10 16))
           (dolist (control '(nil caret))
             (dolist (escape '(nil t))
               (let* ((ascii-table-base base)
                      (ascii-table-control control)
                      (ascii-table-escape escape)
                      (table (ascii-table--table 4))
                      (rows 32))
                 (push
                  (list
                   base
                   control
                   escape
                   (mapcar
                    (lambda (codepoint)
                      (let* ((row
                              (mod codepoint rows))
                             (col
                              (truncate codepoint rows))
                             (cell
                              (* 2
                                 (+ (* 4 row) col))))
                        (list
                         codepoint
                         (aref table cell)
                         (aref table (1+ cell)))))
                    '(0 7 8 9 10 11 12 13 27 31
                      32 48 65 97 126 127))
                   (secure-hash
                    'sha256
                    (prin1-to-string table)))
                  results)))))
         (nreverse results))"##,
        expect![[
            r#"OK ((2 nil nil ((0 ("0000000" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("0000111" . font-lock-comment-face) ("BEL" . font-lock-keyword-face)) (8 ("0001000" . font-lock-comment-face) ("BS" . font-lock-keyword-face)) (9 ("0001001" . font-lock-comment-face) ("HT" . font-lock-string-face)) (10 ("0001010" . font-lock-comment-face) ("LF" . font-lock-string-face)) (11 ("0001011" . font-lock-comment-face) ("VT" . font-lock-string-face)) (12 ("0001100" . font-lock-comment-face) ("FF" . font-lock-string-face)) (13 ("0001101" . font-lock-comment-face) ("CR" . font-lock-string-face)) (27 ("0011011" . font-lock-comment-face) ("ESC" . font-lock-keyword-face)) (31 ("0011111" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("0100000" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("0110000" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("1000001" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("1100001" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("1111110" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("1111111" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "5553858c8607d02b3649e09c84725d82be5706163c500c5d3ab662b754b30692") (2 nil t ((0 ("0000000" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("0000111" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("0001000" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("0001001" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("0001010" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("0001011" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("0001100" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("0001101" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("0011011" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("0011111" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("0100000" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("0110000" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("1000001" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("1100001" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("1111110" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("1111111" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "f99bca023e0a9769de9b3da7796a33fa6fa2619825dfe0fc8cf5ff1877689255") (2 caret nil ((0 ("0000000" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("0000111" . font-lock-comment-face) ("^G" . font-lock-keyword-face)) (8 ("0001000" . font-lock-comment-face) ("^H" . font-lock-keyword-face)) (9 ("0001001" . font-lock-comment-face) ("^I" . font-lock-string-face)) (10 ("0001010" . font-lock-comment-face) ("^J" . font-lock-string-face)) (11 ("0001011" . font-lock-comment-face) ("^K" . font-lock-string-face)) (12 ("0001100" . font-lock-comment-face) ("^L" . font-lock-string-face)) (13 ("0001101" . font-lock-comment-face) ("^M" . font-lock-string-face)) (27 ("0011011" . font-lock-comment-face) ("^[" . font-lock-keyword-face)) (31 ("0011111" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("0100000" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("0110000" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("1000001" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("1100001" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("1111110" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("1111111" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "155a0bce8c7442b66847ce66d194b0a793a3e3299d9edcc502bc6806edb241d1") (2 caret t ((0 ("0000000" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("0000111" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("0001000" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("0001001" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("0001010" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("0001011" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("0001100" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("0001101" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("0011011" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("0011111" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("0100000" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("0110000" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("1000001" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("1100001" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("1111110" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("1111111" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "8c754b66fe3aa5176dcc70d00c31dafc784d9dde12690cf3da6da177c6d7b04a") (8 nil nil ((0 ("000" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("007" . font-lock-comment-face) ("BEL" . font-lock-keyword-face)) (8 ("010" . font-lock-comment-face) ("BS" . font-lock-keyword-face)) (9 ("011" . font-lock-comment-face) ("HT" . font-lock-string-face)) (10 ("012" . font-lock-comment-face) ("LF" . font-lock-string-face)) (11 ("013" . font-lock-comment-face) ("VT" . font-lock-string-face)) (12 ("014" . font-lock-comment-face) ("FF" . font-lock-string-face)) (13 ("015" . font-lock-comment-face) ("CR" . font-lock-string-face)) (27 ("033" . font-lock-comment-face) ("ESC" . font-lock-keyword-face)) (31 ("037" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("040" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("060" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("101" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("141" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("176" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("177" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "ddb29dafd5e71023151aca0607ae422b40cba3f840525e8b5250f312957b0be4") (8 nil t ((0 ("000" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("007" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("010" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("011" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("012" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("013" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("014" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("015" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("033" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("037" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("040" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("060" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("101" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("141" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("176" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("177" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "2777449c206dd1248d91f5df950c6e88f4e54e4712390b58fadccc862cd3bc0a") (8 caret nil ((0 ("000" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("007" . font-lock-comment-face) ("^G" . font-lock-keyword-face)) (8 ("010" . font-lock-comment-face) ("^H" . font-lock-keyword-face)) (9 ("011" . font-lock-comment-face) ("^I" . font-lock-string-face)) (10 ("012" . font-lock-comment-face) ("^J" . font-lock-string-face)) (11 ("013" . font-lock-comment-face) ("^K" . font-lock-string-face)) (12 ("014" . font-lock-comment-face) ("^L" . font-lock-string-face)) (13 ("015" . font-lock-comment-face) ("^M" . font-lock-string-face)) (27 ("033" . font-lock-comment-face) ("^[" . font-lock-keyword-face)) (31 ("037" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("040" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("060" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("101" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("141" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("176" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("177" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "591fd5dafaefa1bf73faa408059e95ff9b13627de9e959aa61df8c561955c6a6") (8 caret t ((0 ("000" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("007" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("010" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("011" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("012" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("013" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("014" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("015" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("033" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("037" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("040" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("060" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("101" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("141" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("176" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("177" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "95d99f2a8131dd9d33370a05bf9432acaf06036e00312dd64cdd525e897fbfb3") (10 nil nil ((0 ("0" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("7" . font-lock-comment-face) ("BEL" . font-lock-keyword-face)) (8 ("8" . font-lock-comment-face) ("BS" . font-lock-keyword-face)) (9 ("9" . font-lock-comment-face) ("HT" . font-lock-string-face)) (10 ("10" . font-lock-comment-face) ("LF" . font-lock-string-face)) (11 ("11" . font-lock-comment-face) ("VT" . font-lock-string-face)) (12 ("12" . font-lock-comment-face) ("FF" . font-lock-string-face)) (13 ("13" . font-lock-comment-face) ("CR" . font-lock-string-face)) (27 ("27" . font-lock-comment-face) ("ESC" . font-lock-keyword-face)) (31 ("31" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("32" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("48" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("65" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("97" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("126" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("127" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "bb719ab3c0e95fb5f0f61e5d8b636cb59cb1cba851e58bc63032e483ac647754") (10 nil t ((0 ("0" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("7" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("8" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("9" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("10" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("11" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("12" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("13" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("27" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("31" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("32" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("48" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("65" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("97" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("126" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("127" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "9ce9eb80e020e805d7ccecf838624ecb42ee472a5aae2d1a4f4ee926674719fa") (10 caret nil ((0 ("0" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("7" . font-lock-comment-face) ("^G" . font-lock-keyword-face)) (8 ("8" . font-lock-comment-face) ("^H" . font-lock-keyword-face)) (9 ("9" . font-lock-comment-face) ("^I" . font-lock-string-face)) (10 ("10" . font-lock-comment-face) ("^J" . font-lock-string-face)) (11 ("11" . font-lock-comment-face) ("^K" . font-lock-string-face)) (12 ("12" . font-lock-comment-face) ("^L" . font-lock-string-face)) (13 ("13" . font-lock-comment-face) ("^M" . font-lock-string-face)) (27 ("27" . font-lock-comment-face) ("^[" . font-lock-keyword-face)) (31 ("31" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("32" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("48" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("65" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("97" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("126" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("127" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "7f4583af5629780ed69aef60d9332e41c89267e21adc29658fe543c0af4b332d") (10 caret t ((0 ("0" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("7" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("8" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("9" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("10" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("11" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("12" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("13" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("27" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("31" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("32" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("48" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("65" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("97" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("126" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("127" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "a95cb7605ddd8f6f0ea192376fa13054a537c6f9596c1e83d7698fb35d95009c") (16 nil nil ((0 ("00" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("07" . font-lock-comment-face) ("BEL" . font-lock-keyword-face)) (8 ("08" . font-lock-comment-face) ("BS" . font-lock-keyword-face)) (9 ("09" . font-lock-comment-face) ("HT" . font-lock-string-face)) (10 ("0A" . font-lock-comment-face) ("LF" . font-lock-string-face)) (11 ("0B" . font-lock-comment-face) ("VT" . font-lock-string-face)) (12 ("0C" . font-lock-comment-face) ("FF" . font-lock-string-face)) (13 ("0D" . font-lock-comment-face) ("CR" . font-lock-string-face)) (27 ("1B" . font-lock-comment-face) ("ESC" . font-lock-keyword-face)) (31 ("1F" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("20" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("30" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("41" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("61" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("7E" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("7F" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "dba78b54f9b9a68be25753a4f88557d840c6cf002c5da023fe3aabbec6b28b53") (16 nil t ((0 ("00" . font-lock-comment-face) ("NUL" . font-lock-keyword-face)) (7 ("07" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("08" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("09" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("0A" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("0B" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("0C" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("0D" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("1B" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("1F" . font-lock-comment-face) ("US" . font-lock-keyword-face)) (32 ("20" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("30" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("41" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("61" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("7E" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("7F" . font-lock-comment-face) ("DEL" . font-lock-keyword-face))) "ad8903fcb332bb38f1cc2a368bbb559446259e5cea594785b2361008b838aefb") (16 caret nil ((0 ("00" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("07" . font-lock-comment-face) ("^G" . font-lock-keyword-face)) (8 ("08" . font-lock-comment-face) ("^H" . font-lock-keyword-face)) (9 ("09" . font-lock-comment-face) ("^I" . font-lock-string-face)) (10 ("0A" . font-lock-comment-face) ("^J" . font-lock-string-face)) (11 ("0B" . font-lock-comment-face) ("^K" . font-lock-string-face)) (12 ("0C" . font-lock-comment-face) ("^L" . font-lock-string-face)) (13 ("0D" . font-lock-comment-face) ("^M" . font-lock-string-face)) (27 ("1B" . font-lock-comment-face) ("^[" . font-lock-keyword-face)) (31 ("1F" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("20" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("30" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("41" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("61" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("7E" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("7F" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "690cad81b189c014a6f2db456bbf5cc03064bf6f3dcb4cdce10c9607cfc0fec4") (16 caret t ((0 ("00" . font-lock-comment-face) ("^@" . font-lock-keyword-face)) (7 ("07" . font-lock-comment-face) ("\\a" . font-lock-keyword-face)) (8 ("08" . font-lock-comment-face) ("\\b" . font-lock-keyword-face)) (9 ("09" . font-lock-comment-face) ("\\t" . font-lock-string-face)) (10 ("0A" . font-lock-comment-face) ("\\n" . font-lock-string-face)) (11 ("0B" . font-lock-comment-face) ("\\v" . font-lock-string-face)) (12 ("0C" . font-lock-comment-face) ("\\f" . font-lock-string-face)) (13 ("0D" . font-lock-comment-face) ("\\r" . font-lock-string-face)) (27 ("1B" . font-lock-comment-face) ("\\e" . font-lock-keyword-face)) (31 ("1F" . font-lock-comment-face) ("^_" . font-lock-keyword-face)) (32 ("20" . font-lock-comment-face) (" " . font-lock-string-face)) (48 ("30" . font-lock-comment-face) ("0" . font-lock-function-name-face)) (65 ("41" . font-lock-comment-face) ("A" . font-lock-variable-name-face)) (97 ("61" . font-lock-comment-face) ("a" . font-lock-variable-name-face)) (126 ("7E" . font-lock-comment-face) ("~" . font-lock-preprocessor-face)) (127 ("7F" . font-lock-comment-face) ("^?" . font-lock-keyword-face))) "95e3a302a62b58e12e117d520f91b9824671a4e62638f7c48d90457bbc82fc4b"))"#
        ]],
    )
}

fn ascii_table_all_supported_row_counts_preserve_layout_size_order_and_content() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_all_supported_row_counts_preserve_layout_size_order_and_content",
        r##"(mapcar
         (lambda (codepoints-per-row)
           (let* ((ascii-table-base 10)
                  (ascii-table-control 'caret)
                  (ascii-table-escape nil)
                  (table
                   (ascii-table--table
                    codepoints-per-row))
                  (rows
                   (ceiling
                    128
                    codepoints-per-row))
                  (cols
                   (* 2 codepoints-per-row))
                  (occupied
                   (seq-count
                    (lambda (pair)
                      (not
                       (equal pair
                              (cons "" nil))))
                    table)))
             (list
              codepoints-per-row
              rows
              cols
              (length table)
              occupied
              (ascii-table--column-widths table cols)
              (secure-hash
               'sha256
               (prin1-to-string table)))))
         '(1 2 3 4 5 6 7 8 9 16 32 64 128))"##,
        expect![[
            r#"OK ((1 128 2 512 256 [3 2] "3a4e8e66bdd1f6d5d70a35b770c98c7784844da62670a4186b0f0f09df3d119b") (2 64 4 512 256 [2 2 3 2] "3cecd868d8f63e452e45ecc1aacdadfa76705d729b3037d8e0f388585e0900af") (3 43 6 516 256 [2 2 2 1 3 2] "ef0c66584412670bc0bb27bb47543a353fb9a8765a39ea31112bf0f70d5f0aa0") (4 32 8 512 256 [2 2 2 1 2 1 3 2] "7f4583af5629780ed69aef60d9332e41c89267e21adc29658fe543c0af4b332d") (5 26 10 520 256 [2 2 2 2 2 1 3 1 3 2] "b6f82ebf4c567be2e3c65b4d8c055f8f24a6317f87355eae6fd5e4797126f074") (6 22 12 528 256 [2 2 2 2 2 1 2 1 3 1 3 2] "d031cc41192718f09fcf1f74c34fc9581c4bdfdeddc56800cabcb30a3df10516") (7 19 14 532 256 [2 2 2 2 2 1 2 1 2 1 3 1 3 2] "e3877d1b68ac5289bf9f88e55aff1bd0a4e92b1db6c4452650bbe229ba63822b") (8 16 16 512 256 [2 2 2 2 2 1 2 1 2 1 2 1 3 1 3 2] "c642f7994c991c5c43555c356cf83e56d71c0e56ecbf81d694b48adfe580999f") (9 15 18 540 256 [2 2 2 2 2 2 2 1 2 1 2 1 3 1 3 1 3 2] "6fc3d83a84b3144851ac5d2800ecc90bd6cc3addb9f7162097cc78ca880d5ac0") (16 8 32 512 256 [1 2 2 2 2 2 2 2 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 3 1 3 1 3 1 3 2] "a9340bd8d007b98bfd48cd91d6c3ab447a478a64d28ea457ee53b08cc648f08f") (32 4 64 512 256 [1 2 1 2 2 2 2 2 2 2 2 2 2 2 2 2 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 3 1 3 1 3 1 3 1 3 1 3 1 3 2] "b80d76f706442b0c309bc3b34109d2b9a3f960cc1f3eaad4aaba74d4c7d89fe6") (64 2 128 512 256 [1 2 1 2 1 2 1 2 1 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 2] "8402ca2d3edd9b97eee978244468fb7943616694c1daf5a5fdae27d0425d42d7") (128 1 256 512 256 [1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 2 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 1 3 2] "3a4e8e66bdd1f6d5d70a35b770c98c7784844da62670a4186b0f0f09df3d119b"))"#
        ]],
    )
}

fn ascii_table_table_invalid_radix_control_and_row_counts_signal_exact_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_table_invalid_radix_control_and_row_counts_signal_exact_errors",
        r##"(let (results)
         (dolist (spec
                  '((3 nil nil 8)
                    (nil nil nil 8)
                    (16 unknown nil 8)
                    (16 nil nil 0)
                    (16 nil nil -1)
                    (16 nil nil 2.5)
                    (16 nil nil "8")))
           (let ((ascii-table-base (nth 0 spec))
                 (ascii-table-control (nth 1 spec))
                 (ascii-table-escape (nth 2 spec))
                 (rows (nth 3 spec)))
             (push
              (condition-case error-data
                  (list
                   spec
                   :ok
                   (length
                    (ascii-table--table rows)))
                (error
                 (list
                  spec
                  :error
                  (car error-data)
                  (cdr error-data))))
              results)))
         (nreverse results))"##,
        expect![[
            r#"OK (((3 nil nil 8) :error error ("cl-ecase failed: 3, (2 8 10 16)")) ((nil nil nil 8) :error error ("cl-ecase failed: nil, (2 8 10 16)")) ((16 unknown nil 8) :error error ("cl-ecase failed: unknown, (nil caret)")) ((16 nil nil 0) :error arith-error nil) ((16 nil nil -1) :ok 512) ((16 nil nil 2.5) :error wrong-type-argument (wholenump 520.0)) ((16 nil nil "8") :error wrong-type-argument (numberp "8")))"#
        ]],
    )
}

fn ascii_table_column_widths_compute_real_table_and_irregular_fixture_maxima() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_column_widths_compute_real_table_and_irregular_fixture_maxima",
        r##"(let* ((fixture
                  (vector
                   (cons "a" 'first)
                   (cons "" 'second)
                   (cons "long" 'third)
                   (cons "xy" 'fourth)
                   (cons "日本" 'fifth)
                   (cons "z" 'sixth)))
                 (ascii-table-base 2)
                 (ascii-table-control nil)
                 (ascii-table-escape nil)
                 (real
                  (ascii-table--table 8)))
         (list
          (ascii-table--column-widths fixture 1)
          (ascii-table--column-widths fixture 2)
          (ascii-table--column-widths fixture 3)
          (ascii-table--column-widths fixture 4)
          (ascii-table--column-widths fixture 6)
          (ascii-table--column-widths real 16)))"##,
        expect!["OK ([4] [4 2] [2 2 4] [2 1 4 2] [1 0 4 2 2 1] [7 3 7 3 7 1 7 1 7 1 7 1 7 1 7 3])"],
    )
}

fn ascii_table_column_widths_handles_empty_tables_missing_cells_and_invalid_columns()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_column_widths_handles_empty_tables_missing_cells_and_invalid_columns",
        r##"(mapcar
         (lambda (spec)
           (condition-case error-data
               (list
                spec
                :ok
                (ascii-table--column-widths
                 (car spec)
                 (cadr spec)))
             (error
              (list
               spec
               :error
               (car error-data)
               (cdr error-data)))))
         (list
          (list [] 0)
          (list [] 3)
          (list [(cons "a" nil)] 1)
          (list [(cons "a" nil)] 2)
          (list [(cons "a" nil)] 0)
          (list [(cons nil nil)] 1)
          (list [nil] 1)
          (list 'not-a-vector 2)
          (list [] -1)
          (list [] 1.5)))"##,
        expect![[
            r#"OK ((([] 0) :ok []) (([] 3) :ok [0 0 0]) (([(cons "a" nil)] 1) :error wrong-type-argument (sequencep cons)) (([(cons "a" nil)] 2) :error wrong-type-argument (sequencep cons)) (([(cons "a" nil)] 0) :error arith-error nil) (([(cons nil nil)] 1) :error wrong-type-argument (sequencep cons)) (([nil] 1) :ok [0]) ((not-a-vector 2) :error wrong-type-argument (sequencep not-a-vector)) (([] -1) :error wrong-type-argument (wholenump -1)) (([] 1.5) :error wrong-type-argument (wholenump 1.5)))"#
        ]],
    )
}

fn ascii_table_table_character_faces_match_classification_for_all_ascii_codepoints()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_table_character_faces_match_classification_for_all_ascii_codepoints",
        r##"(let* ((ascii-table-base 16)
                (ascii-table-control nil)
                (ascii-table-escape nil)
                (codepoints-per-row 8)
                (rows 16)
                (table
                 (ascii-table--table
                  codepoints-per-row))
                mismatches
                distribution)
         (dotimes (codepoint 128)
           (let* ((row (mod codepoint rows))
                  (col (truncate codepoint rows))
                  (cell
                   (* 2
                      (+ (* codepoints-per-row row)
                         col)))
                  (pair (aref table (1+ cell)))
                  (class
                   (ascii-table--character-class
                    codepoint))
                  (wanted
                   (ascii-table--class-face class)))
             (unless (eq (cdr pair) wanted)
               (push
                (list codepoint pair class wanted)
                mismatches))))
         (dolist (face
                  '(font-lock-keyword-face
                    font-lock-preprocessor-face
                    font-lock-string-face
                    font-lock-function-name-face
                    font-lock-variable-name-face
                    nil))
           (push
            (list
             face
             (seq-count
              (lambda (codepoint)
                (eq
                 face
                 (ascii-table--class-face
                  (ascii-table--character-class
                   codepoint))))
              (number-sequence 0 127)))
            distribution))
         (list
          (nreverse mismatches)
          (nreverse distribution)))"##,
        expect![
            "OK (nil ((font-lock-keyword-face 28) (font-lock-preprocessor-face 32) (font-lock-string-face 6) (font-lock-function-name-face 10) (font-lock-variable-name-face 52) (nil 0)))"
        ],
    )
}

fn ascii_table_printable_names_round_trip_to_original_characters_across_all_layouts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_printable_names_round_trip_to_original_characters_across_all_layouts",
        r##"(mapcar
         (lambda (codepoints-per-row)
           (let* ((ascii-table-base 16)
                  (ascii-table-control nil)
                  (ascii-table-escape nil)
                  (rows
                   (ceiling
                    128
                    codepoints-per-row))
                  (table
                   (ascii-table--table
                    codepoints-per-row))
                  failures)
             (dolist (codepoint
                      (number-sequence 32 126))
               (let* ((row
                       (mod codepoint rows))
                      (col
                       (truncate codepoint rows))
                      (cell
                       (* 2
                          (+ (* codepoints-per-row row)
                             col)))
                      (name
                       (car
                        (aref table (1+ cell)))))
                 (unless
                     (and
                      (= 1 (length name))
                      (= codepoint (aref name 0)))
                   (push
                    (list codepoint name)
                    failures))))
             (list
              codepoints-per-row
              (nreverse failures))))
         '(1 2 3 4 5 6 7 8 16 32 64 128))"##,
        expect![
            "OK ((1 nil) (2 nil) (3 nil) (4 nil) (5 nil) (6 nil) (7 nil) (8 nil) (16 nil) (32 nil) (64 nil) (128 nil))"
        ],
    )
}

pub(super) fn formatting_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ascii_table_binary_formats_every_ascii_codepoint_at_fixed_width_without_collisions(),
        ascii_table_binary_rejects_out_of_range_and_non_integer_inputs_with_exact_signals(),
        ascii_table_character_class_covers_full_ascii_domain_and_outside_boundaries(),
        ascii_table_class_face_maps_every_class_and_unknown_value_exactly(),
        ascii_table_control_caret_names_cover_all_controls_and_reject_printable_characters(),
        ascii_table_control_escape_covers_c_escape_range_escape_and_non_escape_controls(),
        ascii_table_default_hex_table_places_every_codepoint_once_with_exact_contents_and_faces(),
        ascii_table_table_configuration_matrix_changes_radix_control_and_escape_precedence(),
        ascii_table_all_supported_row_counts_preserve_layout_size_order_and_content(),
        ascii_table_table_invalid_radix_control_and_row_counts_signal_exact_errors(),
        ascii_table_column_widths_compute_real_table_and_irregular_fixture_maxima(),
        ascii_table_column_widths_handles_empty_tables_missing_cells_and_invalid_columns(),
        ascii_table_table_character_faces_match_classification_for_all_ascii_codepoints(),
        ascii_table_printable_names_round_trip_to_original_characters_across_all_layouts(),
    ]
}
