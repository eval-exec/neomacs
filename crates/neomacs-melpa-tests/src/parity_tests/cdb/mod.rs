use std::time::Duration;

use expect_test::expect;

use crate::{CDB_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CDB_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CDB_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-cdb-test-u32le (value)
  "Encode VALUE as an independent unsigned 32-bit little-endian word."
  (unibyte-string (logand value 255)
                  (logand (ash value -8) 255)
                  (logand (ash value -16) 255)
                  (logand (ash value -24) 255)))

(defun neomacs-cdb-test-hash (bytes)
  "Compute the CDB hash of unibyte string BYTES independently of cdb.el."
  (let ((hash 5381))
    (dotimes (index (length bytes) hash)
      (setq hash
            (logand #xffffffff
                    (logxor (+ hash (ash hash 5))
                            (aref bytes index)))))))

(defun neomacs-cdb-test-build (path entries)
  "Write a standards-compliant CDB at PATH from unibyte ENTRIES."
  (let ((buckets (make-vector 256 nil))
        (position 2048)
        data-parts table-parts header-parts)
    (dolist (entry entries)
      (let* ((key (string-as-unibyte (car entry)))
             (value (string-as-unibyte (cdr entry)))
             (hash (neomacs-cdb-test-hash key))
             (record (concat (neomacs-cdb-test-u32le (length key))
                             (neomacs-cdb-test-u32le (length value))
                             key value)))
        (push (cons hash position) (aref buckets (logand hash 255)))
        (push record data-parts)
        (setq position (+ position (length record)))))
    (setq data-parts (nreverse data-parts))
    (dotimes (bucket 256)
      (let* ((records (nreverse (aref buckets bucket)))
             (slot-count (* 2 (length records)))
             (table (make-vector slot-count nil))
             slot-bytes)
        (dolist (record records)
          (let ((slot (% (ash (car record) -8) slot-count)))
            (while (aref table slot)
              (setq slot (% (1+ slot) slot-count)))
            (aset table slot record)))
        (dotimes (slot slot-count)
          (let ((record (aref table slot)))
            (push (if record
                      (concat (neomacs-cdb-test-u32le (car record))
                              (neomacs-cdb-test-u32le (cdr record)))
                    (unibyte-string 0 0 0 0 0 0 0 0))
                  slot-bytes)))
        (let ((table-bytes (apply #'concat (nreverse slot-bytes))))
          (push (concat (neomacs-cdb-test-u32le position)
                        (neomacs-cdb-test-u32le slot-count))
                header-parts)
          (push table-bytes table-parts)
          (setq position (+ position (length table-bytes))))))
    (let ((bytes (concat (apply #'concat (nreverse header-parts))
                         (apply #'concat data-parts)
                         (apply #'concat (nreverse table-parts))))
          (coding-system-for-write 'no-conversion))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (insert bytes)
        (write-region (point-min) (point-max) path nil 'silent)))
    path))

(defun neomacs-cdb-test-path (name)
  "Resolve deterministic fixture NAME inside the oracle sandbox."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
"##;

fn cdb_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CDB_MELPA_PIN, "cdb.el")
        .expect("prepare pinned CDB source below ./tmp")
        .with_prelude(CDB_TEST_PRELUDE)
        .with_timeout(CDB_TEST_TIMEOUT)
}

fn product_catalog_supports_indexed_lookup_missing_keys_and_ordered_scans() -> ParityBatchCase {
    ParityBatchCase::value(
        "product_catalog_supports_indexed_lookup_missing_keys_and_ordered_scans",
        r##"
(let ((path (neomacs-cdb-test-path "product-catalog.cdb")))
  (neomacs-cdb-test-build
   path
   '(("sku:1001" . "Widget|12|3.50")
     ("sku:1002" . "Cable|80|1.25")
     ("sku:1003" . "Adapter|4|8.00")
     ("warehouse:east" . "sku:1001,sku:1003")))
  (unwind-protect
      (progn
        (cdb-init path)
        (let (rows)
          (list :lookups
                (list (cdb-get path "sku:1002")
                      (cdb-get path "warehouse:east")
                      (cdb-get path "sku:9999"))
                :count
                (cdb-mapc path
                          (lambda (key value)
                            (push (cons key value) rows)))
                :rows (nreverse rows)
                :keys (cdb-keys path)
                :values (cdb-values path))))
    (cdb-uninit path)))
"##,
        expect![[
            r##"OK (:lookups ("Cable|80|1.25" "sku:1001,sku:1003" nil) :count 4 :rows (("sku:1001" . "Widget|12|3.50") ("sku:1002" . "Cable|80|1.25") ("sku:1003" . "Adapter|4|8.00") ("warehouse:east" . "sku:1001,sku:1003")) :keys ("sku:1001" "sku:1002" "sku:1003" "warehouse:east") :values ("Widget|12|3.50" "Cable|80|1.25" "Adapter|4|8.00" "sku:1001,sku:1003"))"##
        ]],
    )
}

fn hash_collisions_and_duplicate_keys_follow_cdb_probe_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "hash_collisions_and_duplicate_keys_follow_cdb_probe_order",
        r##"
(let ((path (neomacs-cdb-test-path "collision-index.cdb")))
  (neomacs-cdb-test-build
   path
   '(("de" . "00596d84")
     ("eD" . "00596d84")
     ("dE" . "00596da4")
     ("xxx" . "0b8791dd")
     ("xxxxx" . "0bb36ddd")
     ("xxxxxx" . "82212905")
     ("xxxxxxx" . "c64649dd")
     ("abc" . "0b873285")
     ("feature" . "stable")
     ("feature" . "canary")))
  (unwind-protect
      (progn
        (cdb-init path)
        (list :collision-lookups
              (mapcar (lambda (key) (cons key (cdb-get path key)))
                      '("de" "eD" "dE" "xxx" "xxxxx"
                        "xxxxxx" "xxxxxxx" "abc"))
              :duplicate-first (cdb-get path "feature")
              :misses (mapcar (lambda (key) (cdb-get path key))
                              '("ed" "x" "xxxx" "bbbb" "dd"))))
    (cdb-uninit path)))
"##,
        expect![[
            r##"OK (:collision-lookups (("de" . "00596d84") ("eD" . "00596d84") ("dE" . "00596da4") ("xxx" . "0b8791dd") ("xxxxx" . "0bb36ddd") ("xxxxxx" . "82212905") ("xxxxxxx" . "c64649dd") ("abc" . "0b873285")) :duplicate-first "stable" :misses (nil nil nil nil nil))"##
        ]],
    )
}

fn utf8_keys_and_binary_payloads_round_trip_as_literal_database_bytes() -> ParityBatchCase {
    ParityBatchCase::value(
        "utf8_keys_and_binary_payloads_round_trip_as_literal_database_bytes",
        r##"
(let* ((path (neomacs-cdb-test-path "binary-assets.cdb"))
       (tokyo (encode-coding-string "東京" 'utf-8 t))
       (resume (encode-coding-string "résumé" 'utf-8 t))
       (greeting (encode-coding-string "こんにちは" 'utf-8 t))
       (binary (unibyte-string 0 1 2 127 128 255 0 42)))
  (neomacs-cdb-test-build
   path (list (cons tokyo greeting)
              (cons resume (encode-coding-string "approved ✓" 'utf-8 t))
              (cons "asset:raw" binary)))
  (unwind-protect
      (progn
        (cdb-init path)
        (let ((raw (cdb-get path "asset:raw")))
          (list :tokyo
                (decode-coding-string (cdb-get path tokyo) 'utf-8 t)
                :resume
                (decode-coding-string (cdb-get path resume) 'utf-8 t)
                :binary-bytes (string-to-list raw)
                :binary-unibyte (not (multibyte-string-p raw))
                :key-byte-lengths
                (mapcar #'string-bytes (cdb-keys path)))))
    (cdb-uninit path)))
"##,
        expect![[
            r##"OK (:tokyo "こんにちは" :resume "approved ✓" :binary-bytes (0 1 2 127 128 255 0 42) :binary-unibyte t :key-byte-lengths (6 8 9))"##
        ]],
    )
}

fn cached_reader_observes_same_layout_updates_and_reopens_new_indexes() -> ParityBatchCase {
    ParityBatchCase::value(
        "cached_reader_observes_same_layout_updates_and_reopens_new_indexes",
        r##"
(let* ((path (neomacs-cdb-test-path "feature-flags.cdb"))
       (initial '(("checkout" . "off") ("search" . "on!")))
       (updated '(("checkout" . "on!") ("search" . "off")))
       first-buffer second-buffer before live-update reopened killed)
  (unwind-protect
      (progn
        (neomacs-cdb-test-build path initial)
        (setq first-buffer (cdb-init path)
              before (list (cdb-get path "checkout")
                           (cdb-get path "search")))
        (let ((same-buffer (equal first-buffer (cdb-init path))))
          (neomacs-cdb-test-build path updated)
          (setq live-update (list (cdb-get path "checkout")
                                  (cdb-get path "search")))
          (cdb-uninit path)
          (setq killed (not (get-buffer first-buffer)))
          (neomacs-cdb-test-build
           path (append updated '(("recommendations" . "on!"))))
          (setq second-buffer (cdb-init path)
                reopened (list (cdb-get path "checkout")
                               (cdb-get path "search")
                               (cdb-get path "recommendations")))
          (list :same-reader same-buffer
                :before before
                :live-update live-update
                :killed-on-uninit killed
                :reopened reopened
                :reader-name-stable (equal first-buffer second-buffer))))
    (cdb-uninit path)))
"##,
        expect![[
            r##"OK (:same-reader t :before ("off" "on!") :live-update ("on!" "off") :killed-on-uninit t :reopened ("on!" "off" "on!") :reader-name-stable t)"##
        ]],
    )
}

#[test]
fn cdb_package_batch() {
    let cases = vec![
        product_catalog_supports_indexed_lookup_missing_keys_and_ordered_scans(),
        hash_collisions_and_duplicate_keys_follow_cdb_probe_order(),
        utf8_keys_and_binary_payloads_round_trip_as_literal_database_bytes(),
        cached_reader_observes_same_layout_updates_and_reopens_new_indexes(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed CDB parity test");
    assert_oracle_batch_cases(cdb_oracle(), test_name, "cdb_parity", &cases);
}
