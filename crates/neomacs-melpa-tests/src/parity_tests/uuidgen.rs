use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, UUIDGEN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'uuidgen)

(defun neomacs-uuidgen-test-description (uuid)
  "Describe UUID's canonical shape, version, variant, and binary payload."
  (let ((variant-nibble (string-to-number (substring uuid 19 20) 16)))
    (list
     :value uuid
     :length (length uuid)
     :canonical
     (not (null
           (string-match-p
            "\\`[0-9a-f]\\{8\\}-[0-9a-f]\\{4\\}-[0-9a-f]\\{4\\}-[0-9a-f]\\{4\\}-[0-9a-f]\\{12\\}\\'"
            uuid)))
     :version (substring uuid 14 15)
     :variant
     (cond ((< variant-nibble 8) 'ncs)
           ((< variant-nibble 12) 'rfc4122)
           ((< variant-nibble 14) 'microsoft)
           (t 'future))
     :octets (uuidgen--string-to-octets uuid)
     :binary-length (length (uuidgen--decode uuid))
     :binary-unibyte (not (multibyte-string-p (uuidgen--decode uuid))))))
"####;

fn rfc_namespace_vectors_generate_stable_service_identifiers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((domain "www.widgets.com")
       (dns-v3 (uuidgen-3 uuidgen-ns-dns domain))
       (dns-v5 (uuidgen-5 uuidgen-ns-dns domain))
       (url-v5 (uuidgen-5 uuidgen-ns-url "https://例.example/路径")))
  (list :dns-v3 (neomacs-uuidgen-test-description dns-v3)
        :dns-v5 (neomacs-uuidgen-test-description dns-v5)
        :url-v5 (neomacs-uuidgen-test-description url-v5)
        :repeat-stable
        (list (equal dns-v3 (uuidgen-3 uuidgen-ns-dns domain))
              (equal dns-v5 (uuidgen-5 uuidgen-ns-dns domain)))
        :namespace-separation
        (uuidgen-5 uuidgen-ns-oid domain)))
"####;
    let expected = expect![[
        r#"OK (:dns-v3 (:value "3d813cbb-47fb-32ba-91df-831e1593ac29" :length 36 :canonical t :version "3" :variant rfc4122 :octets (61 129 60 187 71 251 50 186 145 223 131 30 21 147 172 41) :binary-length 16 :binary-unibyte t) :dns-v5 (:value "21f7f8de-8051-5b89-8680-0195ef798b6a" :length 36 :canonical t :version "5" :variant rfc4122 :octets (33 247 248 222 128 81 91 137 134 128 1 149 239 121 139 106) :binary-length 16 :binary-unibyte t) :url-v5 (:value "4e27b3bf-38c0-5e61-8a4c-4370aa88def6" :length 36 :canonical t :version "5" :variant rfc4122 :octets (78 39 179 191 56 192 94 97 138 76 67 112 170 136 222 246) :binary-length 16 :binary-unibyte t) :repeat-stable (t t) :namespace-separation "d9aa9182-d54e-59ca-ab63-e34cd374d360")"#
    ]];
    ParityBatchCase::value(
        "rfc_namespace_vectors_generate_stable_service_identifiers",
        elisp_form,
        expected,
    )
}

fn deterministic_time_and_random_sources_set_exact_version_and_variant_bits() -> ParityBatchCase {
    let elisp_form = r####"
(let (v1 v4 selected-interfaces)
  (cl-letf (((symbol-function 'uuidgen--system-clock)
             (lambda () #x123456789abcdef))
            ((symbol-function 'random)
             (lambda (&optional _limit) #x2abc))
            ((symbol-function 'network-interface-list)
             (lambda () '(("lo") ("eth-release"))))
            ((symbol-function 'network-interface-info)
             (lambda (interface)
               (push interface selected-interfaces)
               (when (equal interface "eth-release")
                 '(nil nil nil (hardware 0 17 34 51 68 85))))))
    (setq v1 (uuidgen-1)))
  (cl-letf (((symbol-function 'uuidgen--random-clock)
             (lambda () #x0fedcba987654321))
            ((symbol-function 'random)
             (lambda (&optional _limit) #x1abc))
            ((symbol-function 'uuidgen--random-address)
             (lambda () '(222 173 190 239 0 1))))
    (setq v4 (uuidgen-4)))
  (list :v1 (neomacs-uuidgen-test-description v1)
        :interfaces (nreverse selected-interfaces)
        :v4 (neomacs-uuidgen-test-description v4)))
"####;
    let expected = expect![[
        r#"OK (:v1 (:value "89abcdef-4567-1123-aabc-001122334455" :length 36 :canonical t :version "1" :variant rfc4122 :octets (137 171 205 239 69 103 17 35 170 188 0 17 34 51 68 85) :binary-length 16 :binary-unibyte t) :interfaces ("eth0" "eth-release") :v4 (:value "87654321-cba9-4fed-9abc-deadbeef0001" :length 36 :canonical t :version "4" :variant rfc4122 :octets (135 101 67 33 203 169 79 237 154 188 222 173 190 239 0 1) :binary-length 16 :binary-unibyte t))"#
    ]];
    ParityBatchCase::value(
        "deterministic_time_and_random_sources_set_exact_version_and_variant_bits",
        elisp_form,
        expected,
    )
}

fn live_random_generation_produces_distinct_canonical_v4_identifiers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((values (let (result)
                 (dotimes (_ 48 (nreverse result))
                   (push (uuidgen-4) result))))
       (unique (delete-dups (copy-sequence values))))
  (list :count (length values)
        :unique-count (length unique)
        :canonical-count
        (cl-count-if
         (lambda (uuid)
           (string-match-p
            "\\`[0-9a-f]\\{8\\}-[0-9a-f]\\{4\\}-4[0-9a-f]\\{3\\}-[89ab][0-9a-f]\\{3\\}-[0-9a-f]\\{12\\}\\'"
            uuid))
         values)
        :binary-lengths
        (delete-dups (mapcar (lambda (uuid) (length (uuidgen--decode uuid))) values))
        :node-count
        (length (delete-dups
                 (mapcar (lambda (uuid) (substring uuid 24)) values)))))
"####;
    let expected = expect![
        "OK (:count 48 :unique-count 48 :canonical-count 48 :binary-lengths (16) :node-count 48)"
    ];
    ParityBatchCase::value(
        "live_random_generation_produces_distinct_canonical_v4_identifiers",
        elisp_form,
        expected,
    )
}

fn clock_conversion_and_network_fallback_preserve_uuid_epoch_policy() -> ParityBatchCase {
    let elisp_form = r####"
(let (warnings fallback)
  (cl-letf (((symbol-function 'current-time)
             (lambda () '(0 1 250000 0))))
    (setq fallback
          (list :unix-clock (uuidgen--current-unix-clock)
                :system-clock (uuidgen--system-clock))))
  (cl-letf (((symbol-function 'network-interface-list)
             (lambda () '(("lo"))))
            ((symbol-function 'network-interface-info)
             (lambda (_interface) nil))
            ((symbol-function 'uuidgen--random-address)
             (lambda () '(1 35 69 103 137 171)))
            ((symbol-function 'display-warning)
             (lambda (type message &rest _)
               (push (list type (car (split-string message "\n"))) warnings))))
    (setq fallback
          (append fallback
                  (list :fallback-address (uuidgen--get-ieee-address)
                        :warnings (nreverse warnings)))))
  fallback)
"####;
    let expected = expect![[
        r#"OK (:unix-clock 12500000 :system-clock 122192928012500000 :fallback-address (17 35 69 103 137 171) :warnings (((uuid network-interface-info) "`network-interface-info' returned nil address.")))"#
    ]];
    ParityBatchCase::value(
        "clock_conversion_and_network_fallback_preserve_uuid_epoch_policy",
        elisp_form,
        expected,
    )
}

fn urn_cid_and_interactive_commands_serialize_release_ids_in_place() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((id "00112233-4455-4677-8899-aabbccddeeff")
       (default-cid (uuidgen-cid id))
       (uuidgen-cid-format-string
        "<%02x%02x%02x%02x:%02x%02x:%02x%02x:%02x%02x:%02x%02x%02x%02x%02x%02x>")
       (custom-cid (uuidgen-cid id))
       prompt)
  (with-temp-buffer
    (insert "release-id=\nclass-id=\ntime-id=\n")
    (goto-char (point-min))
    (search-forward "=")
    (cl-letf (((symbol-function 'uuidgen-4) (lambda () id)))
      (call-interactively #'uuidgen))
    (forward-line 1)
    (end-of-line)
    (cl-letf (((symbol-function 'uuidgen-4) (lambda () id))
              ((symbol-function 'read-string)
               (lambda (text initial &rest _)
                 (setq prompt (list text initial))
                 initial)))
      (call-interactively #'insert-uuid-cid))
    (forward-line 1)
    (end-of-line)
    (let ((uuidgen-upcase t)
          (current-prefix-arg '(4)))
      (cl-letf (((symbol-function 'uuidgen-1)
                 (lambda () "89abcdef-4567-1123-aabc-001122334455")))
        (call-interactively #'uuidgen)))
    (list :urn (uuidgen-urn id)
          :default-cid default-cid
          :custom-cid custom-cid
          :prompt prompt
          :buffer (buffer-string)
          :point (point)
          :modified (buffer-modified-p))))
"####;
    let expected = expect![[
        r#"OK (:urn "urn:uuid:00112233-4455-4677-8899-aabbccddeeff" :default-cid "{ 0x00112233, 0x4455, 0x4677, { 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff } }" :custom-cid "<00112233:4455:4677:8899:aabbccddeeff>" :prompt ("UUID: " "00112233-4455-4677-8899-aabbccddeeff") :buffer "release-id=00112233-4455-4677-8899-aabbccddeeff\nclass-id=<00112233:4455:4677:8899:aabbccddeeff>\ntime-id=89ABCDEF-4567-1123-AABC-001122334455\n" :point 141 :modified t)"#
    ]];
    ParityBatchCase::value(
        "urn_cid_and_interactive_commands_serialize_release_ids_in_place",
        elisp_form,
        expected,
    )
}

fn malformed_hashes_and_identifiers_report_the_same_atomic_failures() -> ParityBatchCase {
    let elisp_form = r####"
(let ((before "prefix:"))
  (with-temp-buffer
    (insert before)
    (let ((hash-error
           (condition-case condition
               (progn (uuidgen-from-hash "abcd" 5) :accepted)
             (error (list (car condition) (error-message-string condition)))))
          (cid-error
           (condition-case condition
               (progn (insert-uuid-cid "not-a-uuid") :accepted)
             (error (list (car condition) (error-message-string condition))))))
      (list :hash-error hash-error
            :cid-error cid-error
            :buffer (buffer-string)
            :unchanged (equal (buffer-string) before)
            :point (point)))))
"####;
    let expected = expect![[
        r#"OK (:hash-error (args-out-of-range "Args out of range: \"abcd\", 0, 8") :cid-error (error "Not enough arguments for format string") :buffer "prefix:" :unchanged t :point 8)"#
    ]];
    ParityBatchCase::value(
        "malformed_hashes_and_identifiers_report_the_same_atomic_failures",
        elisp_form,
        expected,
    )
}

fn uuidgen_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(UUIDGEN_MELPA_PIN, "uuidgen.el")
        .expect("prepare pinned Uuidgen source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn uuidgen_practical_workflows_batch() {
    let cases = vec![
        rfc_namespace_vectors_generate_stable_service_identifiers(),
        deterministic_time_and_random_sources_set_exact_version_and_variant_bits(),
        live_random_generation_produces_distinct_canonical_v4_identifiers(),
        clock_conversion_and_network_fallback_preserve_uuid_epoch_policy(),
        urn_cid_and_interactive_commands_serialize_release_ids_in_place(),
        malformed_hashes_and_identifiers_report_the_same_atomic_failures(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("uuidgen parity batch");
    assert_oracle_batch_cases(uuidgen_oracle(), test_name, "uuidgen parity", &cases);
}
