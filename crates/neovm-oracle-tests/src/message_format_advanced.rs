//! Oracle parity tests for `message` and `format` interaction patterns:
//! message return value, all format specifiers (%s, %d, %f, %c, %o, %x, %e, %g),
//! field width and padding, mixed arg types, and complex formatting pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// message return value (the formatted string itself)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_returns_formatted_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `message` returns the formatted string as its value.
    let form = r#"(list
      (message "hello")
      (message "value: %d" 42)
      (message "%s is %d" "Alice" 30)
      (message nil)
      (message "")
      (message "100%%")
      (message "%s %s %s" 1 2.0 'sym))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"value: 42\" \"Alice is 30\" nil \"\" \"100%\" \"1 2.0 sym\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format with all basic specifiers: %s %d %f %c %o %x %e %g
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_all_specifiers_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; %s - princ representation
      (format "%s" "hello")
      (format "%s" 42)
      (format "%s" 3.14)
      (format "%s" nil)
      (format "%s" t)
      (format "%s" '(a b c))
      (format "%s" 'symbol)
      ;; %d - decimal integer
      (format "%d" 0)
      (format "%d" 42)
      (format "%d" -999)
      (format "%d" 2147483647)
      ;; %f - floating point
      (format "%f" 0.0)
      (format "%f" 3.14159265)
      (format "%f" -1.5)
      ;; %c - character
      (format "%c" 65)
      (format "%c" 97)
      (format "%c" 48)
      ;; %o - octal
      (format "%o" 0)
      (format "%o" 7)
      (format "%o" 8)
      (format "%o" 255)
      (format "%o" 511)
      ;; %x and %X - hex
      (format "%x" 0)
      (format "%x" 15)
      (format "%x" 255)
      (format "%x" 65535)
      (format "%X" 255)
      (format "%X" 65535)
      ;; %e - scientific
      (format "%e" 1.0)
      (format "%e" 123456.789)
      (format "%e" 0.001)
      ;; %g - general float (drops trailing zeros)
      (format "%g" 1.0)
      (format "%g" 0.00001)
      (format "%g" 100000.0)
      (format "%g" 3.14))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"42\" \"3.14\" \"nil\" \"t\" \"(a b c)\" \"symbol\" \"0\" \"42\" \"-999\" \"2147483647\" \"0.000000\" \"3.141593\" \"-1.500000\" \"A\" \"a\" \"0\" \"0\" \"7\" \"10\" \"377\" \"777\" \"0\" \"f\" \"ff\" \"ffff\" \"FF\" \"FFFF\" \"1.000000e+00\" \"1.234568e+05\" \"1.000000e-03\" \"1\" \"1e-05\" \"100000\" \"3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format field width and padding: %-Ns, %0Nd, %Ns, precision
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_field_width_padding_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Right-aligned strings
      (format "%10s" "hi")
      (format "%20s" "hello world")
      (format "%3s" "abcdef")
      ;; Left-aligned strings
      (format "%-10s" "hi")
      (format "%-20s" "hello world")
      (format "%-3s" "abcdef")
      ;; Zero-padded integers
      (format "%05d" 42)
      (format "%010d" -42)
      (format "%05d" 0)
      (format "%08x" 255)
      (format "%08X" 255)
      (format "%08o" 255)
      ;; Right-aligned integers
      (format "%10d" 42)
      (format "%10d" -42)
      ;; Left-aligned integers
      (format "%-10d" 42)
      (format "%-10d" -42)
      ;; Float precision
      (format "%.0f" 3.14159)
      (format "%.1f" 3.14159)
      (format "%.2f" 3.14159)
      (format "%.5f" 3.14159)
      (format "%.10f" 3.14159)
      ;; Float width + precision
      (format "%10.2f" 3.14)
      (format "%-10.2f" 3.14)
      (format "%010.2f" 3.14)
      ;; Scientific precision
      (format "%.2e" 12345.6789)
      (format "%.0e" 12345.6789)
      (format "%15.3e" 0.001234)
      ;; %g precision
      (format "%.2g" 3.14159)
      (format "%.6g" 3.14159)
      ;; Width with %s overflow (field width is minimum, not maximum)
      (format "%2s" "longstring")
      (format "%2d" 99999))"#;
    let expect = expect_test::expect![[
        r#""OK (\"        hi\" \"         hello world\" \"abcdef\" \"hi        \" \"hello world         \" \"abcdef\" \"00042\" \"-000000042\" \"00000\" \"000000ff\" \"000000FF\" \"00000377\" \"        42\" \"       -42\" \"42        \" \"-42       \" \"3\" \"3.1\" \"3.14\" \"3.14159\" \"3.1415900000\" \"      3.14\" \"3.14      \" \"0000003.14\" \"1.23e+04\" \"1e+04\" \"      1.234e-03\" \"3.1\" \"3.14159\" \"longstring\" \"99999\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format with multiple args of different types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_mixed_type_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Mix string, int, float
      (format "%s: %d items at $%.2f each" "Order" 5 9.99)
      ;; Mix hex, octal, decimal for same value
      (format "dec=%d hex=0x%x oct=0%o" 255 255 255)
      ;; Character in context
      (format "char %c has code %d and hex %x" 65 65 65)
      ;; Multiple %s with different types
      (format "%s %s %s %s %s" "str" 42 3.14 nil '(a b))
      ;; All specifiers in one format string
      (format "s=%s d=%d f=%.1f c=%c o=%o x=%x e=%.1e g=%g"
              "hi" 42 3.14 65 255 255 12345.6 100.0)
      ;; Repeated format with same specifier
      (format "%d+%d=%d, %d*%d=%d" 3 4 7 3 4 12)
      ;; %S (prin1) mixed with %s (princ)
      (format "princ=%s prin1=%S" "hello" "hello")
      (format "princ=%s prin1=%S" '(a b) '(a b)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Order: 5 items at $9.99 each\" \"dec=255 hex=0xff oct=0377\" \"char A has code 65 and hex 41\" \"str 42 3.14 nil (a b)\" \"s=hi d=42 f=3.1 c=A o=377 x=ff e=1.2e+04 g=100\" \"3+4=7, 3*4=12\" \"princ=hello prin1=\\\"hello\\\"\" \"princ=(a b) prin1=(a b)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building log messages with severity levels and timestamps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_log_message_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-format-log
    (lambda (level timestamp module msg &rest args)
      "Build a formatted log message."
      (let ((level-str (cond ((eq level 'debug) "DEBUG")
                             ((eq level 'info)  "INFO ")
                             ((eq level 'warn)  "WARN ")
                             ((eq level 'error) "ERROR")
                             (t                 "?????  ")))
            (formatted-msg (if args
                               (apply #'format msg args)
                             msg)))
        (format "[%s] %s [%s] %s" level-str timestamp module formatted-msg))))

  (unwind-protect
      (list
        (funcall 'neovm--test-format-log 'info "12:00:01" "net"
                 "Connection from %s:%d" "192.168.1.1" 8080)
        (funcall 'neovm--test-format-log 'error "12:00:02" "db"
                 "Query failed after %.1fs: %s" 2.5 "timeout")
        (funcall 'neovm--test-format-log 'debug "12:00:03" "gc"
                 "Collected %d objects (%.1f%% of heap)" 1500 23.5)
        (funcall 'neovm--test-format-log 'warn "12:00:04" "auth"
                 "Failed login attempt #%d for user %S" 3 "admin")
        ;; Build a multi-line log report
        (let ((entries '((info  "09:00" "app"  "Started")
                         (info  "09:01" "db"   "Connected to %s" "postgres://localhost")
                         (warn  "09:05" "mem"  "Usage at %d%%" 85)
                         (error "09:10" "disk" "Write failed: %s" "ENOSPC"))))
          (mapconcat
            (lambda (entry)
              (apply 'neovm--test-format-log entry))
            entries "\n")))
    (fmakunbound 'neovm--test-format-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[INFO ] 12:00:01 [net] Connection from 192.168.1.1:8080\" \"[ERROR] 12:00:02 [db] Query failed after 2.5s: timeout\" \"[DEBUG] 12:00:03 [gc] Collected 1500 objects (23.5% of heap)\" \"[WARN ] 12:00:04 [auth] Failed login attempt #3 for user \\\"admin\\\"\" \"[INFO ] 09:00 [app] Started\\n[INFO ] 09:01 [db] Connected to postgres://localhost\\n[WARN ] 09:05 [mem] Usage at 85%\\n[ERROR] 09:10 [disk] Write failed: ENOSPC\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: table formatting with aligned columns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_aligned_table_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((headers '("ID" "Name" "Score" "Grade" "Pct"))
            (data '((1 "Alice"   95.5 "A"  0.955)
                    (2 "Bob"     87.2 "B+" 0.872)
                    (3 "Carol"   92.8 "A-" 0.928)
                    (4 "Dave"    73.1 "C"  0.731)
                    (5 "Eve"    100.0 "A+" 1.0)))
            ;; Format header
            (header-line (format "%4s  %-10s  %6s  %5s  %7s"
                                 (nth 0 headers)
                                 (nth 1 headers)
                                 (nth 2 headers)
                                 (nth 3 headers)
                                 (nth 4 headers)))
            ;; Format separator
            (sep (make-string (length header-line) ?-))
            ;; Format data rows
            (rows (mapcar
                    (lambda (row)
                      (format "%4d  %-10s  %6.1f  %5s  %6.1f%%"
                              (nth 0 row)
                              (nth 1 row)
                              (nth 2 row)
                              (nth 3 row)
                              (* 100 (nth 4 row))))
                    data))
            ;; Compute summary
            (total (apply #'+ (mapcar (lambda (r) (nth 2 r)) data)))
            (avg (/ total (float (length data))))
            (summary (format "  Average: %.2f  |  Total: %.1f  |  Count: %d"
                             avg total (length data))))
       (mapconcat #'identity
                  (append (list header-line sep)
                          rows
                          (list sep summary))
                  "\n"))"#;
    let expect = expect_test::expect![[
        r#""OK \"  ID  Name         Score  Grade      Pct\\n----------------------------------------\\n   1  Alice         95.5      A    95.5%\\n   2  Bob           87.2     B+    87.2%\\n   3  Carol         92.8     A-    92.8%\\n   4  Dave          73.1      C    73.1%\\n   5  Eve          100.0     A+   100.0%\\n----------------------------------------\\n  Average: 89.72  |  Total: 448.6  |  Count: 5\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number base conversion display using format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_number_base_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-num-display
    (lambda (n)
      "Display a number in multiple bases with aligned columns."
      (format "%6d  0x%04X  0%06o  %c"
              n n n
              (if (and (>= n 32) (<= n 126)) n ?.)))) ;; printable or dot

  (unwind-protect
      (let* ((numbers '(0 1 9 10 15 16 31 32 48 57 65 90 97 122 126 127 255))
             (header (format "%6s  %6s  %7s  %4s" "Dec" "Hex" "Oct" "Chr"))
             (sep (make-string (length header) ?=))
             (rows (mapcar 'neovm--test-num-display numbers))
             ;; Also test: format with computed width
             (max-dec (apply #'max numbers))
             (width (length (format "%d" max-dec)))
             (dynamic-fmt (format "%%%dd" width))
             (dynamic-rows (mapcar (lambda (n) (format dynamic-fmt n)) numbers)))
        (list
          header
          sep
          rows
          dynamic-rows
          ;; Verify specific format results
          (format "%d in hex is %x" 255 255)
          (format "%d in oct is %o" 8 8)
          (format "ASCII %d = %c" 65 65)))
    (fmakunbound 'neovm--test-num-display)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"   Dec     Hex      Oct   Chr\" \"=============================\" (\"     0  0x0000  0000000  .\" \"     1  0x0001  0000001  .\" \"     9  0x0009  0000011  .\" \"    10  0x000A  0000012  .\" \"    15  0x000F  0000017  .\" \"    16  0x0010  0000020  .\" \"    31  0x001F  0000037  .\" \"    32  0x0020  0000040   \" \"    48  0x0030  0000060  0\" \"    57  0x0039  0000071  9\" \"    65  0x0041  0000101  A\" \"    90  0x005A  0000132  Z\" \"    97  0x0061  0000141  a\" \"   122  0x007A  0000172  z\" \"   126  0x007E  0000176  ~\" \"   127  0x007F  0000177  .\" \"   255  0x00FF  0000377  .\") (\"  0\" \"  1\" \"  9\" \" 10\" \" 15\" \" 16\" \" 31\" \" 32\" \" 48\" \" 57\" \" 65\" \" 90\" \" 97\" \"122\" \"126\" \"127\" \"255\") \"255 in hex is ff\" \"8 in oct is 10\" \"ASCII 65 = A\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: empty format, %% escaping, argument count edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Empty format string
      (format "")
      ;; Format with no specifiers
      (format "plain text")
      ;; Multiple %% escaping
      (format "%%")
      (format "%%%%")
      (format "100%% done")
      (format "%d%% of %d%%" 50 100)
      ;; Single-char format results
      (format "%c" 42)
      (format "%d" 0)
      ;; Very long format string built from parts
      (let ((parts nil))
        (dotimes (i 10)
          (setq parts (cons (format "item%d=%d" i (* i i)) parts)))
        (mapconcat #'identity (nreverse parts) ", "))
      ;; Nested format calls
      (format "outer(%s)" (format "inner(%s)" (format "deep(%d)" 42)))
      ;; format with nil arg
      (format "%s" nil)
      (format "%S" nil)
      ;; format with t arg
      (format "%s" t)
      (format "%S" t)
      ;; message vs format equivalence
      (let ((msg-result (message "%d + %d = %d" 3 4 7))
            (fmt-result (format "%d + %d = %d" 3 4 7)))
        (string= msg-result fmt-result)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"plain text\" \"%\" \"%%\" \"100% done\" \"50% of 100%\" \"*\" \"0\" \"item0=0, item1=1, item2=4, item3=9, item4=16, item5=25, item6=36, item7=49, item8=64, item9=81\" \"outer(inner(deep(42)))\" \"nil\" \"nil\" \"t\" \"t\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
