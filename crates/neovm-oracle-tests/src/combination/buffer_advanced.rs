//! Advanced oracle tests combining buffer operations with algorithms:
//! in-buffer binary search, longest common subsequence, multi-buffer
//! aggregation, fixed-width record parsing, and circular buffer simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// In-buffer binary search on sorted lines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_binary_search_sorted_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert sorted numeric lines into a buffer, then perform binary search
    // by line number to find a target value.
    let form = r#"(progn
  (fset 'neovm--buf-bsearch
    (lambda (target)
      (let ((lo 1)
            (hi (count-lines (point-min) (point-max)))
            (found nil))
        (while (and (<= lo hi) (not found))
          (let ((mid (/ (+ lo hi) 2)))
            (goto-char (point-min))
            (forward-line (1- mid))
            (let ((val (string-to-number
                         (buffer-substring
                           (line-beginning-position)
                           (line-end-position)))))
              (cond
               ((= val target) (setq found mid))
               ((< val target) (setq lo (1+ mid)))
               (t              (setq hi (1- mid)))))))
        found)))
  (unwind-protect
      (with-temp-buffer
        ;; Insert 20 sorted values: 5, 10, 15, ..., 100
        (let ((i 1))
          (while (<= i 20)
            (insert (number-to-string (* i 5)) "\n")
            (setq i (1+ i))))
        (list
          ;; Search for values that exist
          (funcall 'neovm--buf-bsearch 5)
          (funcall 'neovm--buf-bsearch 50)
          (funcall 'neovm--buf-bsearch 100)
          ;; Search for value that doesn't exist
          (funcall 'neovm--buf-bsearch 42)
          (funcall 'neovm--buf-bsearch 0)
          ;; Search for boundary
          (funcall 'neovm--buf-bsearch 10)
          (funcall 'neovm--buf-bsearch 95)))
    (fmakunbound 'neovm--buf-bsearch)))"#;
    let expect = expect_test::expect![[r#""OK (1 10 20 nil nil 2 19)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based diff: longest common subsequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_longest_common_subsequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute LCS of two strings using DP in a buffer-local vector table
    let form = r#"(progn
  (fset 'neovm--lcs
    (lambda (s1 s2)
      (let* ((m (length s1))
             (n (length s2))
             ;; dp table as vector of vectors: (m+1) x (n+1)
             (dp (make-vector (1+ m) nil)))
        ;; Initialize each row
        (dotimes (i (1+ m))
          (aset dp i (make-vector (1+ n) 0)))
        ;; Fill DP table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (= (aref s1 (1- i)) (aref s2 (1- j)))
                    (aset (aref dp i) j
                          (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to find the LCS string
        (let ((result nil)
              (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((= (aref s1 (1- i)) (aref s2 (1- j)))
              (setq result (cons (aref s1 (1- i)) result))
              (setq i (1- i) j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          (concat result)))))
  (unwind-protect
      (list
        (funcall 'neovm--lcs "ABCBDAB" "BDCAB")
        (funcall 'neovm--lcs "AGGTAB" "GXTXAYB")
        (funcall 'neovm--lcs "" "ABC")
        (funcall 'neovm--lcs "ABC" "ABC")
        (funcall 'neovm--lcs "ABC" "DEF"))
    (fmakunbound 'neovm--lcs)))"#;
    let expect = expect_test::expect![[r#""OK (\"BDAB\" \"GTAB\" \"\" \"ABC\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-buffer aggregation: create temp buffers, combine results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_multi_buffer_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create 3 temp buffers, each with different data. Parse each one,
    // then combine results in a fourth buffer.
    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-agg-a*"))
                        (buf-b (generate-new-buffer " *neovm-agg-b*"))
                        (buf-c (generate-new-buffer " *neovm-agg-c*")))
                    (unwind-protect
                        (progn
                          ;; Populate buffers with key=value pairs
                          (with-current-buffer buf-a
                            (insert "alpha=1\nbeta=2\ngamma=3\n"))
                          (with-current-buffer buf-b
                            (insert "delta=4\nepsilon=5\nzeta=6\n"))
                          (with-current-buffer buf-c
                            (insert "eta=7\ntheta=8\niota=9\n"))
                          ;; Parse each buffer into an alist
                          (let ((parse-buf
                                  (lambda (buf)
                                    (with-current-buffer buf
                                      (goto-char (point-min))
                                      (let ((result nil))
                                        (while (re-search-forward
                                                 "^\\([^=]+\\)=\\([0-9]+\\)$" nil t)
                                          (setq result
                                                (cons (cons (match-string 1)
                                                            (string-to-number
                                                              (match-string 2)))
                                                      result)))
                                        (nreverse result))))))
                            (let* ((all-data (append (funcall parse-buf buf-a)
                                                     (funcall parse-buf buf-b)
                                                     (funcall parse-buf buf-c)))
                                   ;; Compute total, find max entry
                                   (total (let ((sum 0))
                                            (dolist (pair all-data)
                                              (setq sum (+ sum (cdr pair))))
                                            sum))
                                   (max-entry (let ((best nil))
                                                (dolist (pair all-data)
                                                  (when (or (null best)
                                                            (> (cdr pair) (cdr best)))
                                                    (setq best pair)))
                                                best))
                                   ;; Write summary into a new temp buffer
                                   (summary-buf (generate-new-buffer " *neovm-summary*")))
                              (unwind-protect
                                  (progn
                                    (with-current-buffer summary-buf
                                      (insert (format "total=%d\n" total))
                                      (insert (format "max=%s:%d\n"
                                                      (car max-entry) (cdr max-entry)))
                                      (insert (format "count=%d\n" (length all-data)))
                                      (buffer-string)))
                                (kill-buffer summary-buf)))))
                      (kill-buffer buf-a)
                      (kill-buffer buf-b)
                      (kill-buffer buf-c)))"#;
    let expect = expect_test::expect![[r#""OK \"total=45\\nmax=iota:9\\ncount=9\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer as byte-stream: read fixed-width records
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_fixed_width_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a fixed-width record format:
    //   Name:  10 chars (padded with spaces)
    //   Age:    3 chars (zero-padded integer)
    //   Score:  5 chars (zero-padded integer)
    // Total: 18 chars per record
    let form = r#"(with-temp-buffer
                    ;; Insert fixed-width records (18 chars each)
                    (insert "Alice     02500085")
                    (insert "Bob       03000092")
                    (insert "Carol     03500078")
                    (insert "Dave      02800095")
                    (insert "Eve       03200088")
                    (let ((record-len 18)
                          (records nil))
                      ;; Parse each record
                      (goto-char (point-min))
                      (while (<= (+ (point) record-len) (1+ (point-max)))
                        (let* ((start (point))
                               (name (string-trim-right
                                       (buffer-substring start (+ start 10))))
                               (age (string-to-number
                                      (buffer-substring (+ start 10) (+ start 13))))
                               (score (string-to-number
                                        (buffer-substring (+ start 13) (+ start 18)))))
                          (setq records (cons (list name age score) records))
                          (goto-char (+ start record-len))))
                      (setq records (nreverse records))
                      ;; Compute statistics
                      (let ((avg-age (/ (apply #'+
                                          (mapcar (lambda (r) (nth 1 r)) records))
                                        (length records)))
                            (max-score-rec
                              (let ((best (car records)))
                                (dolist (r (cdr records))
                                  (when (> (nth 2 r) (nth 2 best))
                                    (setq best r)))
                                best))
                            ;; Sort by score descending
                            (sorted (sort (copy-sequence records)
                                          (lambda (a b) (> (nth 2 a) (nth 2 b))))))
                        (list
                          (length records)
                          avg-age
                          (car max-score-rec)
                          (mapcar #'car sorted)))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 30 \"Dave\" (\"Dave\" \"Bob\" \"Eve\" \"Alice\" \"Carol\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circular buffer simulation with wrap-around
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_circular_buffer_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a circular/ring buffer of capacity N using a temp buffer.
    // The buffer holds fixed-width slots. We track head/tail via markers.
    let form = r#"(progn
  ;; Ring buffer operations using a vector backend
  (fset 'neovm--ring-make
    (lambda (capacity)
      (list (make-vector capacity nil) 0 0 0 capacity)))
  ;; ring = (vec head tail count capacity)
  (fset 'neovm--ring-push
    (lambda (ring val)
      (let ((vec (nth 0 ring))
            (tail (nth 2 ring))
            (count (nth 3 ring))
            (cap (nth 4 ring)))
        (aset vec tail val)
        (setcar (nthcdr 2 ring) (% (1+ tail) cap))
        (if (< count cap)
            (setcar (nthcdr 3 ring) (1+ count))
          ;; Buffer full — advance head (overwrite oldest)
          (setcar (nthcdr 1 ring) (% (1+ (nth 1 ring)) cap))))))
  (fset 'neovm--ring-pop
    (lambda (ring)
      (let ((vec (nth 0 ring))
            (head (nth 1 ring))
            (count (nth 3 ring))
            (cap (nth 4 ring)))
        (if (= count 0)
            'empty
          (let ((val (aref vec head)))
            (aset vec head nil)
            (setcar (nthcdr 1 ring) (% (1+ head) cap))
            (setcar (nthcdr 3 ring) (1- count))
            val)))))
  (fset 'neovm--ring-to-list
    (lambda (ring)
      (let ((vec (nth 0 ring))
            (head (nth 1 ring))
            (count (nth 3 ring))
            (cap (nth 4 ring))
            (result nil))
        (dotimes (i count)
          (setq result (cons (aref vec (% (+ head i) cap)) result)))
        (nreverse result))))
  (unwind-protect
      (let ((rb (funcall 'neovm--ring-make 5)))
        ;; Push 1..7 (capacity 5, so 1 and 2 get overwritten)
        (dotimes (i 7)
          (funcall 'neovm--ring-push rb (1+ i)))
        (let ((after-overflow (funcall 'neovm--ring-to-list rb)))
          ;; Pop 2 elements
          (let ((p1 (funcall 'neovm--ring-pop rb))
                (p2 (funcall 'neovm--ring-pop rb))
                (remaining (funcall 'neovm--ring-to-list rb)))
            ;; Push more to wrap around again
            (funcall 'neovm--ring-push rb 100)
            (funcall 'neovm--ring-push rb 200)
            (funcall 'neovm--ring-push rb 300)
            (let ((final-state (funcall 'neovm--ring-to-list rb)))
              (list after-overflow p1 p2 remaining final-state)))))
    (fmakunbound 'neovm--ring-make)
    (fmakunbound 'neovm--ring-push)
    (fmakunbound 'neovm--ring-pop)
    (fmakunbound 'neovm--ring-to-list)))"#;
    let expect = expect_test::expect![[r#""OK ((3 4 5 6 7) 3 4 (5 6 7) (6 7 100 200 300))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-based run-length encoding and decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bufadv_run_length_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RLE encode a string: "AAABBBCCDDDDDE" -> "3A3B2C5D1E"
    // Then decode back and verify roundtrip
    let form = r#"(progn
  (fset 'neovm--rle-encode
    (lambda (str)
      (with-temp-buffer
        (insert str)
        (goto-char (point-min))
        (let ((result nil))
          (while (not (eobp))
            (let ((ch (char-after))
                  (count 1))
              (forward-char 1)
              (while (and (not (eobp)) (= (char-after) ch))
                (setq count (1+ count))
                (forward-char 1))
              (setq result (cons (format "%d%c" count ch) result))))
          (apply #'concat (nreverse result))))))
  (fset 'neovm--rle-decode
    (lambda (encoded)
      (with-temp-buffer
        (insert encoded)
        (goto-char (point-min))
        (let ((result nil))
          (while (re-search-forward "\\([0-9]+\\)\\([^0-9]\\)" nil t)
            (let ((count (string-to-number (match-string 1)))
                  (ch (match-string 2)))
              (dotimes (_ count)
                (setq result (cons ch result)))))
          (apply #'concat (nreverse result))))))
  (unwind-protect
      (let* ((original "AAABBBCCDDDDDEFFFFFFGGHIJJJJ")
             (encoded (funcall 'neovm--rle-encode original))
             (decoded (funcall 'neovm--rle-decode encoded))
             ;; Also test with single-char runs
             (s2 "ABCDE")
             (e2 (funcall 'neovm--rle-encode s2))
             (d2 (funcall 'neovm--rle-decode e2)))
        (list encoded
              (string= decoded original)
              e2
              (string= d2 s2)))
    (fmakunbound 'neovm--rle-encode)
    (fmakunbound 'neovm--rle-decode)))"#;
    let expect = expect_test::expect![[r#""OK (\"3A3B2C5D1E6F2G1H1I4J\" t \"1A1B1C1D1E\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
