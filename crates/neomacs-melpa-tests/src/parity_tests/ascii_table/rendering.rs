use expect_test::expect;

use super::ParityBatchCase;

fn ascii_table_wide_hex_render_has_exact_practical_table_text_structure_and_digest()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_wide_hex_render_has_exact_practical_table_text_structure_and_digest",
        r##"(let* ((render
                  (ascii-table-test-render
                   200
                   16
                   nil
                   nil))
                 (text (nth 0 render))
                 (lines (split-string text "\n")))
         (list
          text
          (length text)
          (length lines)
          (secure-hash 'sha256 text)
          (nth 1 render)
          (nth 2 render)
          (nth 3 render)
          (nth 4 render)
          (nth 5 render)
          (length (nth 6 render))))"##,
        expect![[
            r#"OK ("ASCII Table (hex)\n\n00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  \n01  SOH  11  DC1  21  !  31  1  41  A  51  Q  61  a  71  q  \n02  STX  12  DC2  22  \"  32  2  42  B  52  R  62  b  72  r  \n03  ETX  13  DC3  23  #  33  3  43  C  53  S  63  c  73  s  \n04  EOT  14  DC4  24  $  34  4  44  D  54  T  64  d  74  t  \n05  ENQ  15  NAK  25  %  35  5  45  E  55  U  65  e  75  u  \n06  ACK  16  SYN  26  &  36  6  46  F  56  V  66  f  76  v  \n07  BEL  17  ETB  27  '  37  7  47  G  57  W  67  g  77  w  \n08  BS   18  CAN  28  (  38  8  48  H  58  X  68  h  78  x  \n09  HT   19  EM   29  )  39  9  49  I  59  Y  69  i  79  y  \n0A  LF   1A  SUB  2A  *  3A  :  4A  J  5A  Z  6A  j  7A  z  \n0B  VT   1B  ESC  2B  +  3B  ;  4B  K  5B  [  6B  k  7B  {  \n0C  FF   1C  FS   2C  ,  3C  <  4C  L  5C  \\  6C  l  7C  |  \n0D  CR   1D  GS   2D  -  3D  =  4D  M  5D  ]  6D  m  7D  }  \n0E  SO   1E  RS   2E  .  3E  >  4E  N  5E  ^  6E  n  7E  ~  \n0F  SI   1F  US   2F  /  3F  ?  4F  O  5F  _  6F  o  7F  DEL\n" 995 19 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25" 1 ascii-table-mode "ASCII" t ascii-table--revert 256)"#
        ]],
    )
}

fn ascii_table_binary_caret_render_preserves_full_content_at_realistic_terminal_width()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_binary_caret_render_preserves_full_content_at_realistic_terminal_width",
        r##"(let* ((render
                  (ascii-table-test-render
                   100
                   2
                   'caret
                   nil))
                 (text (nth 0 render))
                 (lines (split-string text "\n")))
         (list
          (length text)
          (length lines)
          (seq-take lines 5)
          (seq-take
           (reverse lines)
           4)
          (string-match-p
           (regexp-quote "0000000  ^@")
           text)
          (string-match-p
           (regexp-quote "1111111  ^?")
           text)
          (secure-hash 'sha256 text)
          (length (nth 6 render))))"##,
        expect![[
            r#"OK (1590 19 ("ASCII Table (binary)" "" "0000000  ^@  0010000  ^P  0100000     0110000  0  1000000  @  1010000  P  1100000  `  1110000  p " "0000001  ^A  0010001  ^Q  0100001  !  0110001  1  1000001  A  1010001  Q  1100001  a  1110001  q " "0000010  ^B  0010010  ^R  0100010  \"  0110010  2  1000010  B  1010010  R  1100010  b  1110010  r ") ("" "0001111  ^O  0011111  ^_  0101111  /  0111111  ?  1001111  O  1011111  _  1101111  o  1111111  ^?" "0001110  ^N  0011110  ^^  0101110  .  0111110  >  1001110  N  1011110  ^  1101110  n  1111110  ~ " "0001101  ^M  0011101  ^]  0101101  -  0111101  =  1001101  M  1011101  ]  1101101  m  1111101  } ") 22 1578 "9991d0ae18b4e85329f36f18a29411ce70a9ddb4f196be260dad8f1d6d13755e" 256)"#
        ]],
    )
}

fn ascii_table_decimal_escape_render_uses_escapes_only_where_defined_and_real_characters_elsewhere()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_decimal_escape_render_uses_escapes_only_where_defined_and_real_characters_elsewhere",
        r##"(let* ((render
                  (ascii-table-test-render
                   85
                   10
                   nil
                   t))
                 (text (nth 0 render))
                 (lines (split-string text "\n")))
         (list
          (length text)
          (length lines)
          lines
          (mapcar
           (lambda (needle)
             (list
              needle
              (string-match-p
               (regexp-quote needle)
               text)))
           '("7  \\a"
             "8  \\b"
             "9  \\t"
             "10  \\n"
             "11  \\v"
             "12  \\f"
             "13  \\r"
             "27  \\e"
             "0  NUL"
             "127  DEL"))
          (secure-hash 'sha256 text)
          (length (nth 6 render))))"##,
        expect![[
            r#"OK (1031 19 ("ASCII Table (decimal)" "" " 0  NUL  16  DLE  32     48  0  64  @  80  P   96  `  112  p  " " 1  SOH  17  DC1  33  !  49  1  65  A  81  Q   97  a  113  q  " " 2  STX  18  DC2  34  \"  50  2  66  B  82  R   98  b  114  r  " " 3  ETX  19  DC3  35  #  51  3  67  C  83  S   99  c  115  s  " " 4  EOT  20  DC4  36  $  52  4  68  D  84  T  100  d  116  t  " " 5  ENQ  21  NAK  37  %  53  5  69  E  85  U  101  e  117  u  " " 6  ACK  22  SYN  38  &  54  6  70  F  86  V  102  f  118  v  " " 7  \\a   23  ETB  39  '  55  7  71  G  87  W  103  g  119  w  " " 8  \\b   24  CAN  40  (  56  8  72  H  88  X  104  h  120  x  " " 9  \\t   25  EM   41  )  57  9  73  I  89  Y  105  i  121  y  " "10  \\n   26  SUB  42  *  58  :  74  J  90  Z  106  j  122  z  " "11  \\v   27  \\e   43  +  59  ;  75  K  91  [  107  k  123  {  " "12  \\f   28  FS   44  ,  60  <  76  L  92  \\  108  l  124  |  " "13  \\r   29  GS   45  -  61  =  77  M  93  ]  109  m  125  }  " "14  SO   30  RS   46  .  62  >  78  N  94  ^  110  n  126  ~  " "15  SI   31  US   47  /  63  ?  79  O  95  _  111  o  127  DEL" "") (("7  \\a" 465) ("8  \\b" 528) ("9  \\t" 591) ("10  \\n" 653) ("11  \\v" 716) ("12  \\f" 779) ("13  \\r" 842) ("27  \\e" 725) ("0  NUL" 24) ("127  DEL" 1022)) "89e471b9dbb63a93eaadf7ea2480bac7cdd16583d715de255802a88ae652f730" 256)"#
        ]],
    )
}

fn ascii_table_overlay_runs_cover_every_code_and_name_cell_with_exact_faces() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_overlay_runs_cover_every_code_and_name_cell_with_exact_faces",
        r##"(let* ((render
                  (ascii-table-test-render
                   200
                   16
                   nil
                   nil))
                 (runs (nth 6 render)))
         (list
          (length runs)
          (seq-take runs 12)
          (seq-take (reverse runs) 12)
          (mapcar
           (lambda (face)
             (list
              face
              (seq-count
               (lambda (run)
                 (eq face (nth 3 run)))
               runs)))
           '(font-lock-comment-face
             font-lock-keyword-face
             font-lock-preprocessor-face
             font-lock-string-face
             font-lock-function-name-face
             font-lock-variable-name-face
             nil))
          (secure-hash
           'sha256
           (prin1-to-string runs))))"##,
        expect![[
            r#"OK (256 ((20 22 "00" font-lock-comment-face) (24 27 "NUL" font-lock-keyword-face) (29 31 "10" font-lock-comment-face) (33 36 "DLE" font-lock-keyword-face) (38 40 "20" font-lock-comment-face) (42 43 " " font-lock-string-face) (45 47 "30" font-lock-comment-face) (49 50 "0" font-lock-function-name-face) (52 54 "40" font-lock-comment-face) (56 57 "@" font-lock-preprocessor-face) (59 61 "50" font-lock-comment-face) (63 64 "P" font-lock-variable-name-face)) ((992 995 "DEL" font-lock-keyword-face) (988 990 "7F" font-lock-comment-face) (985 986 "o" font-lock-variable-name-face) (981 983 "6F" font-lock-comment-face) (978 979 "_" font-lock-preprocessor-face) (974 976 "5F" font-lock-comment-face) (971 972 "O" font-lock-variable-name-face) (967 969 "4F" font-lock-comment-face) (964 965 "?" font-lock-preprocessor-face) (960 962 "3F" font-lock-comment-face) (957 958 "/" font-lock-preprocessor-face) (953 955 "2F" font-lock-comment-face)) ((font-lock-comment-face 128) (font-lock-keyword-face 28) (font-lock-preprocessor-face 32) (font-lock-string-face 6) (font-lock-function-name-face 10) (font-lock-variable-name-face 52) (nil 0)) "53bd3e8e4e3b9918672a413c3166357a4d06511c0767ad6be4e09077d4f98bcf")"#
        ]],
    )
}

fn ascii_table_revert_replaces_text_and_collapses_old_overlays_at_buffer_start() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_revert_replaces_text_and_collapses_old_overlays_at_buffer_start",
        r##"(with-temp-buffer
         (let ((ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil))
           (cl-letf
               (((symbol-function
                  'ascii-table--width-limit)
                 (lambda () 200)))
             (ascii-table-mode)
             (let ((first
                    (list
                     (buffer-string)
                     (length
                      (overlays-in
                       (point-min)
                       (point-max))))))
               (let ((inhibit-read-only t))
                 (goto-char (point-max))
                 (insert "stale")
                 (make-overlay
                 (max
                   (point-min)
                   (- (point-max) 5))
                  (point-max))
                 (set-buffer-modified-p t))
               (setq ascii-table-base 8
                     ascii-table-control 'caret
                     ascii-table-escape t)
               (ascii-table--revert)
               (list
                (list
                 (length (car first))
                 (secure-hash
                  'sha256
                  (car first))
                 (cadr first))
                (length (buffer-string))
                (secure-hash
                 'sha256
                 (buffer-string))
                (length
                 (overlays-in
                  (point-min)
                  (point-max)))
                (string-match-p
                 "stale"
                 (buffer-string))
                (point)
                (buffer-modified-p)
                buffer-read-only)))))"##,
        expect![[
            r#"OK ((995 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25" 256) 1077 "566c85968d5ae359a7e5cedbef08e7f64ed7d81a262e81a688b20a32796c7b03" 513 nil 1 t t)"#
        ]],
    )
}

fn ascii_table_revert_accepts_revert_protocol_arguments_and_resets_point_to_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_revert_accepts_revert_protocol_arguments_and_resets_point_to_start",
        r##"(with-temp-buffer
         (let ((ascii-table-base 16))
           (cl-letf
               (((symbol-function
                  'ascii-table--width-limit)
                 (lambda () 90)))
             (ascii-table-mode)
             (goto-char (point-max))
             (let ((first
                    (ascii-table--revert
                     :arbitrary
                     :also-arbitrary)))
               (goto-char (point-max))
               (let ((second
                      (funcall
                       revert-buffer-function
                       t
                       t)))
                 (list
                  first
                  second
                  (point)
                  (buffer-substring-no-properties
                   (point-min)
                   (line-end-position))
                  major-mode
                  buffer-read-only))))))"##,
        expect![[r#"OK (1 1 1 "ASCII Table (hex)" ascii-table-mode t)"#]],
    )
}

fn ascii_table_revert_rejects_wrong_major_mode_and_file_visiting_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_revert_rejects_wrong_major_mode_and_file_visiting_buffers",
        r##"(let (results)
         (with-temp-buffer
           (push
            (condition-case error-data
                (list
                 :fundamental
                 :ok
                 (ascii-table--revert))
              (error
               (list
                :fundamental
                :error
                (car error-data)
                (cdr error-data))))
            results))
         (with-temp-buffer
           (setq major-mode 'ascii-table-mode
                 buffer-file-name
                 (expand-file-name
                  "ascii-table-fixture.txt"
                  (getenv
                   "NEOMACS_TEST_SANDBOX_ROOT")))
           (push
            (condition-case error-data
                (list
                 :file-visiting
                 :ok
                 (ascii-table--revert))
              (error
               (list
                :file-visiting
                :error
                (car error-data)
                (cdr error-data))))
            results))
         (nreverse results))"##,
        expect![
            "OK ((:fundamental :error cl-assertion-failed ((equal major-mode 'ascii-table-mode))) (:file-visiting :error cl-assertion-failed ((null (buffer-file-name)))))"
        ],
    )
}

fn ascii_table_width_limit_uses_current_window_width_when_ascii_buffer_is_absent() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_width_limit_uses_current_window_width_when_ascii_buffer_is_absent",
        r##"(let ((existing
                (get-buffer "*ASCII*"))
               calls)
         (when existing
           (kill-buffer existing))
         (cl-letf
             (((symbol-function
                'window-width)
               (lambda (&optional window)
                 (push window calls)
                 73))
              ((symbol-function
                'walk-windows)
               (lambda (&rest arguments)
                 (push
                  (cons :unexpected-walk arguments)
                  calls))))
           (list
            (ascii-table--width-limit)
            (nreverse calls))))"##,
        expect!["OK (73 (nil))"],
    )
}

fn ascii_table_width_limit_selects_narrowest_live_ascii_window_and_ignores_others()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_width_limit_selects_narrowest_live_ascii_window_and_ignores_others",
        r##"(let ((ascii-buffer
                (get-buffer-create "*ASCII*"))
               (other-buffer
                (get-buffer-create
                 " *ascii-table-other*"))
               (windows
                '((ascii-wide . 132)
                  (other-narrow . 20)
                  (ascii-narrow . 61)
                  (ascii-medium . 88)))
               calls)
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'walk-windows)
                   (lambda (function minibuffer all-frames)
                     (push
                      (list :walk minibuffer all-frames)
                      calls)
                     (dolist (window windows)
                       (funcall function window))))
                  ((symbol-function
                    'window-buffer)
                   (lambda (window)
                     (if
                         (string-prefix-p
                          "ascii-"
                          (symbol-name
                           (car window)))
                         ascii-buffer
                       other-buffer)))
                  ((symbol-function
                    'window-width)
                   (lambda (&optional window)
                     (push
                      (list :width window)
                      calls)
                     (if window
                         (cdr window)
                       999))))
               (list
                (ascii-table--width-limit)
                (nreverse calls)))
           (kill-buffer ascii-buffer)
           (kill-buffer other-buffer)))"##,
        expect![
            "OK (61 ((:walk nil t) (:width (ascii-wide . 132)) (:width (ascii-narrow . 61)) (:width (ascii-medium . 88))))"
        ],
    )
}

fn ascii_table_layout_selection_obeys_strict_width_boundary_for_every_candidate() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_layout_selection_obeys_strict_width_boundary_for_every_candidate",
        r##"(let ((ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil)
               candidates
               results)
         (dolist (codepoints-per-row
                  '(8 7 6 5 4 3 2 1))
           (let* ((table
                   (ascii-table--table
                    codepoints-per-row))
                  (cols
                   (* 2 codepoints-per-row))
                  (widths
                   (ascii-table--column-widths
                    table
                    cols))
                  (needed
                   (+ (cl-reduce #'+ widths)
                      (* 2 (length widths)))))
             (push
              (cons codepoints-per-row needed)
              candidates)))
         (setq candidates (nreverse candidates))
         (dolist (candidate candidates)
           (dolist (limit
                    (list
                     (cdr candidate)
                     (1+ (cdr candidate))))
             (let* ((render
                      (ascii-table-test-render
                       limit 16 nil nil))
                    (text (car render))
                    (first-data-line
                     (nth
                      2
                      (split-string text "\n"))))
               (push
                (list
                 candidate
                 limit
                 first-data-line
                 (length
                  (split-string
                   first-data-line
                   "  "))
                 (length
                  (split-string text "\n")))
                results))))
         (list
          candidates
          (nreverse results)))"##,
        expect![[
            r#"OK ((#1=(8 . 62) #2=(7 . 55) #3=(6 . 48) #4=(5 . 41) #5=(4 . 32) #6=(3 . 25) #7=(2 . 18) #8=(1 . 9)) ((#1# 62 "00  NUL  13  DC3  26  &  39  9  4C  L  5F  _  72  r  " 15 22) (#1# 63 "00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  " 17 19) (#2# 55 "00  NUL  16  SYN  2C  ,  42  B  58  X  6E  n  " 13 25) (#2# 56 "00  NUL  13  DC3  26  &  39  9  4C  L  5F  _  72  r  " 15 22) (#3# 48 "00  NUL  1A  SUB  34  4  4E  N  68  h  " 11 29) (#3# 49 "00  NUL  16  SYN  2C  ,  42  B  58  X  6E  n  " 13 25) (#4# 41 "00  NUL  20     40  @  60  `  " 9 35) (#4# 42 "00  NUL  1A  SUB  34  4  4E  N  68  h  " 11 29) (#5# 32 "00  NUL  2B  +  56  V  " 7 46) (#5# 33 "00  NUL  20     40  @  60  `  " 9 35) (#6# 25 "00  NUL  40  @  " 5 67) (#6# 26 "00  NUL  2B  +  56  V  " 7 46) (#7# 18 "00  NUL" 2 131) (#7# 19 "00  NUL  40  @  " 5 67) (#8# 9 "" 1 3) (#8# 10 "00  NUL" 2 131)))"#
        ]],
    )
}

fn ascii_table_narrow_widths_switch_from_header_only_to_single_column_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_narrow_widths_switch_from_header_only_to_single_column_table",
        r##"(mapcar
         (lambda (width)
           (let* ((render
                   (ascii-table-test-render
                    width 16 nil nil))
                  (text (nth 0 render)))
             (list
              width
              text
              (length text)
              (length
               (split-string text "\n"))
              (length (nth 6 render)))))
         '(0 1 5 10 11 12 13))"##,
        expect![[
            r#"OK ((0 "ASCII Table (hex)\n\n" 19 3 0) (1 "ASCII Table (hex)\n\n" 19 3 0) (5 "ASCII Table (hex)\n\n" 19 3 0) (10 "ASCII Table (hex)\n\n00  NUL\n01  SOH\n02  STX\n03  ETX\n04  EOT\n05  ENQ\n06  ACK\n07  BEL\n08  BS \n09  HT \n0A  LF \n0B  VT \n0C  FF \n0D  CR \n0E  SO \n0F  SI \n10  DLE\n11  DC1\n12  DC2\n13  DC3\n14  DC4\n15  NAK\n16  SYN\n17  ETB\n18  CAN\n19  EM \n1A  SUB\n1B  ESC\n1C  FS \n1D  GS \n1E  RS \n1F  US \n20     \n21  !  \n22  \"  \n23  #  \n24  $  \n25  %  \n26  &  \n27  '  \n28  (  \n29  )  \n2A  *  \n2B  +  \n2C  ,  \n2D  -  \n2E  .  \n2F  /  \n30  0  \n31  1  \n32  2  \n33  3  \n34  4  \n35  5  \n36  6  \n37  7  \n38  8  \n39  9  \n3A  :  \n3B  ;  \n3C  <  \n3D  =  \n3E  >  \n3F  ?  \n40  @  \n41  A  \n42  B  \n43  C  \n44  D  \n45  E  \n46  F  \n47  G  \n48  H  \n49  I  \n4A  J  \n4B  K  \n4C  L  \n4D  M  \n4E  N  \n4F  O  \n50  P  \n51  Q  \n52  R  \n53  S  \n54  T  \n55  U  \n56  V  \n57  W  \n58  X  \n59  Y  \n5A  Z  \n5B  [  \n5C  \\  \n5D  ]  \n5E  ^  \n5F  _  \n60  `  \n61  a  \n62  b  \n63  c  \n64  d  \n65  e  \n66  f  \n67  g  \n68  h  \n69  i  \n6A  j  \n6B  k  \n6C  l  \n6D  m  \n6E  n  \n6F  o  \n70  p  \n71  q  \n72  r  \n73  s  \n74  t  \n75  u  \n76  v  \n77  w  \n78  x  \n79  y  \n7A  z  \n7B  {  \n7C  |  \n7D  }  \n7E  ~  \n7F  DEL\n" 1043 131 256) (11 "ASCII Table (hex)\n\n00  NUL\n01  SOH\n02  STX\n03  ETX\n04  EOT\n05  ENQ\n06  ACK\n07  BEL\n08  BS \n09  HT \n0A  LF \n0B  VT \n0C  FF \n0D  CR \n0E  SO \n0F  SI \n10  DLE\n11  DC1\n12  DC2\n13  DC3\n14  DC4\n15  NAK\n16  SYN\n17  ETB\n18  CAN\n19  EM \n1A  SUB\n1B  ESC\n1C  FS \n1D  GS \n1E  RS \n1F  US \n20     \n21  !  \n22  \"  \n23  #  \n24  $  \n25  %  \n26  &  \n27  '  \n28  (  \n29  )  \n2A  *  \n2B  +  \n2C  ,  \n2D  -  \n2E  .  \n2F  /  \n30  0  \n31  1  \n32  2  \n33  3  \n34  4  \n35  5  \n36  6  \n37  7  \n38  8  \n39  9  \n3A  :  \n3B  ;  \n3C  <  \n3D  =  \n3E  >  \n3F  ?  \n40  @  \n41  A  \n42  B  \n43  C  \n44  D  \n45  E  \n46  F  \n47  G  \n48  H  \n49  I  \n4A  J  \n4B  K  \n4C  L  \n4D  M  \n4E  N  \n4F  O  \n50  P  \n51  Q  \n52  R  \n53  S  \n54  T  \n55  U  \n56  V  \n57  W  \n58  X  \n59  Y  \n5A  Z  \n5B  [  \n5C  \\  \n5D  ]  \n5E  ^  \n5F  _  \n60  `  \n61  a  \n62  b  \n63  c  \n64  d  \n65  e  \n66  f  \n67  g  \n68  h  \n69  i  \n6A  j  \n6B  k  \n6C  l  \n6D  m  \n6E  n  \n6F  o  \n70  p  \n71  q  \n72  r  \n73  s  \n74  t  \n75  u  \n76  v  \n77  w  \n78  x  \n79  y  \n7A  z  \n7B  {  \n7C  |  \n7D  }  \n7E  ~  \n7F  DEL\n" 1043 131 256) (12 "ASCII Table (hex)\n\n00  NUL\n01  SOH\n02  STX\n03  ETX\n04  EOT\n05  ENQ\n06  ACK\n07  BEL\n08  BS \n09  HT \n0A  LF \n0B  VT \n0C  FF \n0D  CR \n0E  SO \n0F  SI \n10  DLE\n11  DC1\n12  DC2\n13  DC3\n14  DC4\n15  NAK\n16  SYN\n17  ETB\n18  CAN\n19  EM \n1A  SUB\n1B  ESC\n1C  FS \n1D  GS \n1E  RS \n1F  US \n20     \n21  !  \n22  \"  \n23  #  \n24  $  \n25  %  \n26  &  \n27  '  \n28  (  \n29  )  \n2A  *  \n2B  +  \n2C  ,  \n2D  -  \n2E  .  \n2F  /  \n30  0  \n31  1  \n32  2  \n33  3  \n34  4  \n35  5  \n36  6  \n37  7  \n38  8  \n39  9  \n3A  :  \n3B  ;  \n3C  <  \n3D  =  \n3E  >  \n3F  ?  \n40  @  \n41  A  \n42  B  \n43  C  \n44  D  \n45  E  \n46  F  \n47  G  \n48  H  \n49  I  \n4A  J  \n4B  K  \n4C  L  \n4D  M  \n4E  N  \n4F  O  \n50  P  \n51  Q  \n52  R  \n53  S  \n54  T  \n55  U  \n56  V  \n57  W  \n58  X  \n59  Y  \n5A  Z  \n5B  [  \n5C  \\  \n5D  ]  \n5E  ^  \n5F  _  \n60  `  \n61  a  \n62  b  \n63  c  \n64  d  \n65  e  \n66  f  \n67  g  \n68  h  \n69  i  \n6A  j  \n6B  k  \n6C  l  \n6D  m  \n6E  n  \n6F  o  \n70  p  \n71  q  \n72  r  \n73  s  \n74  t  \n75  u  \n76  v  \n77  w  \n78  x  \n79  y  \n7A  z  \n7B  {  \n7C  |  \n7D  }  \n7E  ~  \n7F  DEL\n" 1043 131 256) (13 "ASCII Table (hex)\n\n00  NUL\n01  SOH\n02  STX\n03  ETX\n04  EOT\n05  ENQ\n06  ACK\n07  BEL\n08  BS \n09  HT \n0A  LF \n0B  VT \n0C  FF \n0D  CR \n0E  SO \n0F  SI \n10  DLE\n11  DC1\n12  DC2\n13  DC3\n14  DC4\n15  NAK\n16  SYN\n17  ETB\n18  CAN\n19  EM \n1A  SUB\n1B  ESC\n1C  FS \n1D  GS \n1E  RS \n1F  US \n20     \n21  !  \n22  \"  \n23  #  \n24  $  \n25  %  \n26  &  \n27  '  \n28  (  \n29  )  \n2A  *  \n2B  +  \n2C  ,  \n2D  -  \n2E  .  \n2F  /  \n30  0  \n31  1  \n32  2  \n33  3  \n34  4  \n35  5  \n36  6  \n37  7  \n38  8  \n39  9  \n3A  :  \n3B  ;  \n3C  <  \n3D  =  \n3E  >  \n3F  ?  \n40  @  \n41  A  \n42  B  \n43  C  \n44  D  \n45  E  \n46  F  \n47  G  \n48  H  \n49  I  \n4A  J  \n4B  K  \n4C  L  \n4D  M  \n4E  N  \n4F  O  \n50  P  \n51  Q  \n52  R  \n53  S  \n54  T  \n55  U  \n56  V  \n57  W  \n58  X  \n59  Y  \n5A  Z  \n5B  [  \n5C  \\  \n5D  ]  \n5E  ^  \n5F  _  \n60  `  \n61  a  \n62  b  \n63  c  \n64  d  \n65  e  \n66  f  \n67  g  \n68  h  \n69  i  \n6A  j  \n6B  k  \n6C  l  \n6D  m  \n6E  n  \n6F  o  \n70  p  \n71  q  \n72  r  \n73  s  \n74  t  \n75  u  \n76  v  \n77  w  \n78  x  \n79  y  \n7A  z  \n7B  {  \n7C  |  \n7D  }  \n7E  ~  \n7F  DEL\n" 1043 131 256))"#
        ]],
    )
}

fn ascii_table_mode_hook_observes_initialized_special_mode_before_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_mode_hook_observes_initialized_special_mode_before_rendering",
        r##"(let ((events nil)
               (ascii-table-mode-hook nil))
         (add-hook
          'ascii-table-mode-hook
          (lambda ()
            (push
             (list
              :hook
              major-mode
              mode-name
              buffer-read-only
              revert-buffer-function
              (buffer-string)
              (point))
             events)))
         (with-temp-buffer
           (cl-letf
               (((symbol-function
                  'ascii-table--width-limit)
                 (lambda () 90)))
             (ascii-table-mode)
             (list
              (nreverse events)
              major-mode
              mode-name
              buffer-read-only
              revert-buffer-function
              (point)
              (secure-hash
               'sha256
               (buffer-string))))))"##,
        expect![[
            r#"OK (((:hook ascii-table-mode "ASCII" t ascii-table--revert "ASCII Table (hex)\n\n00  NUL  10  DLE  20     30  0  40  @  50  P  60  `  70  p  \n01  SOH  11  DC1  21  !  31  1  41  A  51  Q  61  a  71  q  \n02  STX  12  DC2  22  \"  32  2  42  B  52  R  62  b  72  r  \n03  ETX  13  DC3  23  #  33  3  43  C  53  S  63  c  73  s  \n04  EOT  14  DC4  24  $  34  4  44  D  54  T  64  d  74  t  \n05  ENQ  15  NAK  25  %  35  5  45  E  55  U  65  e  75  u  \n06  ACK  16  SYN  26  &  36  6  46  F  56  V  66  f  76  v  \n07  BEL  17  ETB  27  '  37  7  47  G  57  W  67  g  77  w  \n08  BS   18  CAN  28  (  38  8  48  H  58  X  68  h  78  x  \n09  HT   19  EM   29  )  39  9  49  I  59  Y  69  i  79  y  \n0A  LF   1A  SUB  2A  *  3A  :  4A  J  5A  Z  6A  j  7A  z  \n0B  VT   1B  ESC  2B  +  3B  ;  4B  K  5B  [  6B  k  7B  {  \n0C  FF   1C  FS   2C  ,  3C  <  4C  L  5C  \\  6C  l  7C  |  \n0D  CR   1D  GS   2D  -  3D  =  4D  M  5D  ]  6D  m  7D  }  \n0E  SO   1E  RS   2E  .  3E  >  4E  N  5E  ^  6E  n  7E  ~  \n0F  SI   1F  US   2F  /  3F  ?  4F  O  5F  _  6F  o  7F  DEL\n" 1)) ascii-table-mode "ASCII" t ascii-table--revert 1 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25")"#
        ]],
    )
}

fn ascii_table_revert_preserves_read_only_mode_and_uses_inhibit_read_only_internally()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_revert_preserves_read_only_mode_and_uses_inhibit_read_only_internally",
        r##"(with-temp-buffer
         (let ((ascii-table-base 16))
           (cl-letf
               (((symbol-function
                  'ascii-table--width-limit)
                 (lambda () 90)))
             (ascii-table-mode)
             (let ((initial
                    (list
                     buffer-read-only
                     (buffer-modified-p)
                     (secure-hash
                      'sha256
                      (buffer-string)))))
               (set-buffer-modified-p nil)
               (ascii-table--revert)
               (list
                initial
                buffer-read-only
                (buffer-modified-p)
                (point)
                (secure-hash
                 'sha256
                 (buffer-string)))))))"##,
        expect![[
            r#"OK ((t t "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25") t t 1 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25")"#
        ]],
    )
}

pub(super) fn rendering_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ascii_table_wide_hex_render_has_exact_practical_table_text_structure_and_digest(),
        ascii_table_binary_caret_render_preserves_full_content_at_realistic_terminal_width(),
        ascii_table_decimal_escape_render_uses_escapes_only_where_defined_and_real_characters_elsewhere(),
        ascii_table_overlay_runs_cover_every_code_and_name_cell_with_exact_faces(),
        ascii_table_revert_replaces_text_and_collapses_old_overlays_at_buffer_start(),
        ascii_table_revert_accepts_revert_protocol_arguments_and_resets_point_to_start(),
        ascii_table_revert_rejects_wrong_major_mode_and_file_visiting_buffers(),
        ascii_table_width_limit_uses_current_window_width_when_ascii_buffer_is_absent(),
        ascii_table_width_limit_selects_narrowest_live_ascii_window_and_ignores_others(),
        ascii_table_layout_selection_obeys_strict_width_boundary_for_every_candidate(),
        ascii_table_narrow_widths_switch_from_header_only_to_single_column_table(),
        ascii_table_mode_hook_observes_initialized_special_mode_before_rendering(),
        ascii_table_revert_preserves_read_only_mode_and_uses_inhibit_read_only_internally(),
    ]
}
