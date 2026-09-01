use expect_test::expect;

use super::ParityBatchCase;

fn apel_routes_incoming_mail_and_persists_a_safe_delivery_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_routes_incoming_mail_and_persists_a_safe_delivery_index",
        r####"
(progn
  (require 'filename)
  (let* ((root
        (file-name-as-directory
         (expand-file-name
          "apel-mail-router"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (index (expand-file-name "state/delivery-index.tsv" root))
       (default-directory root)
       (filename-limit-length 32)
       (filename-filters
        '(filename-special-filter
          filename-eliminate-top-low-lines
          filename-canonicalize-low-lines
          filename-maybe-truncate-by-size
          filename-eliminate-bottom-low-lines))
       tree
       counts
       deliveries
       result)
  (unwind-protect
      (progn
        (neomacs-apel-test-cleanup root)
        (make-directory (file-name-directory index) t)
        (in-calist-package 'mail-router)
        (dolist
            (rule
             '(((folder . "inbox")
                (priority . high)
                (route . "pager"))
               ((folder . "inbox")
                (priority . normal)
                (route . "review"))
               ((folder . "archive")
                (priority . low)
                (route . "cold-storage"))
               ((folder . t)
                (priority . t)
                (route . "quarantine"))))
          (setq tree
                (ctree-add-calist-strictly
                 tree
                 rule)))
        (dolist
            (message
             '(((id . 101)
                (folder . "inbox")
                (priority . high)
                (subject . "Production / database?"))
               ((id . 102)
                (folder . "inbox")
                (priority . normal)
                (subject . "Quarterly report: draft"))
               ((id . 103)
                (folder . "archive")
                (priority . low)
                (subject . "2024 / invoices"))
               ((id . 104)
                (folder . "inbox")
                (priority . high)
                (subject . "API latency / production"))
               ((id . 105)
                (folder . "unknown")
                (priority . urgent)
                (subject . "Unclassified: follow-up"))))
          (let* ((matched
                  (ctree-match-calist tree message))
                 (route (cdr (assq 'route matched)))
                 (safe-subject
                  (replace-as-filename
                   (cdr (assq 'subject message))))
                 (count (1+ (or (cdr (assoc route counts)) 0))))
            (setq counts (put-alist route count counts))
            (push
             (list
              (cdr (assq 'id message))
              route
              safe-subject)
             deliveries)))
        (setq deliveries (nreverse deliveries))
        (with-temp-file index
          (dolist (delivery deliveries)
            (insert
             (format
              "%d\t%s\t%s\n"
              (nth 0 delivery)
              (nth 1 delivery)
              (nth 2 delivery)))))
        (setq result
              (list
               :deliveries deliveries
               :counts
               (sort
                (copy-tree counts)
                (lambda (left right)
                  (string< (car left) (car right))))
               :index
               (neomacs-apel-test-file-string index))))
    (neomacs-apel-test-cleanup root))
    result))
"####,
        expect![[
            r#"OK (:deliveries ((101 "pager" "Production_database") (102 "review" "Quarterly_report_draft") (103 "cold-storage" "2024_invoices") (104 "pager" "API_latency_production") (105 "quarantine" "Unclassified_follow-up")) :counts (("cold-storage" . 1) ("pager" . 2) ("quarantine" . 1) ("review" . 1)) :index "101\11pager\11Production_database\n102\11review\11Quarterly_report_draft\n103\11cold-storage\0112024_invoices\n104\11pager\11API_latency_production\n105\11quarantine\11Unclassified_follow-up\n")"#
        ]],
    )
}

fn apel_selects_loads_and_runs_the_latest_installed_legacy_extension() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_selects_loads_and_runs_the_latest_installed_legacy_extension",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apel-extension-manager"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (older (expand-file-name "legacy-dashboard-1.4/" root))
       (newer (expand-file-name "legacy-dashboard-2.0/" root))
       (lisp-directory (expand-file-name "lisp/" newer))
       (resource-directory (expand-file-name "share/" newer))
       (program (expand-file-name "dashboard-render" newer))
       (config (expand-file-name "dashboard.conf" resource-directory))
       (default-directory root)
       (default-load-path (list root))
       (load-path nil)
       result)
  (unwind-protect
      (progn
        (neomacs-apel-test-cleanup root)
        (make-directory older t)
        (make-directory lisp-directory t)
        (make-directory resource-directory t)
        (with-temp-file
            (expand-file-name "legacy-dashboard.el" older)
          (insert
           "(defun legacy-dashboard-render (records)\n"
           "  (format \"legacy:%d\" (length records)))\n"
           "(provide 'legacy-dashboard)\n"))
        (with-temp-file
            (expand-file-name "legacy-dashboard.el" lisp-directory)
          (insert
           "(defun legacy-dashboard-render (records)\n"
           "  (format \"dashboard/2.0:%s\"\n"
           "          (mapconcat #'number-to-string records \",\")))\n"
           "(provide 'legacy-dashboard)\n"))
        (with-temp-file config
          (insert
           "endpoint=https://dashboard.example.test\n"
           "retries=3\n"))
        (with-temp-file program
          (insert
           "#!/bin/sh\n"
           "printf 'rendered:%s\\n' \"$*\"\n"))
        (set-file-modes program #o755)
        (unless
            (and
             (zerop
              (call-process
               "touch"
               nil
               nil
               nil
               "-d"
               "@10000"
               older))
             (zerop
              (call-process
               "touch"
               nil
               nil
               nil
               "-d"
               "@20000"
               newer)))
          (error "Could not prepare deterministic extension timestamps"))
        (add-latest-path
         "\\`legacy-dashboard-[0-9.]+\\'")
        (add-path "lisp" 'all-paths)
        (add-path resource-directory 'append)
        ;; A second addition must not duplicate an existing path.
        (add-path "lisp" 'all-paths)
        (let
            ((selected
              (get-latest-path
               "\\`legacy-dashboard-[0-9.]+\\'"))
             (module
              (module-installed-p
               'legacy-dashboard
               load-path))
             (installed-config
              (file-installed-p
               "dashboard.conf"
               load-path))
             (executable
              (exec-installed-p
               "dashboard-render"
               load-path)))
          (require 'legacy-dashboard)
          (with-temp-buffer
            (let ((status
                   (call-process
                    executable
                    nil
                    t
                    nil
                    "101"
                    "102"
                    "103")))
              (setq result
                    (list
                     :selected
                     (file-name-nondirectory
                      (directory-file-name selected))
                     :load-path
                     (mapcar
                      (lambda (path)
                        (file-name-nondirectory
                         (directory-file-name path)))
                      load-path)
                     :module
                     (file-name-nondirectory module)
                     :config
                     (list
                      (file-name-nondirectory installed-config)
                      (neomacs-apel-test-file-string
                       installed-config))
                     :executable
                     (file-name-nondirectory executable)
                     :render
                     (legacy-dashboard-render
                      '(101 102 103))
                     :process
                     (list status (buffer-string))))))))
    (neomacs-apel-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:selected "legacy-dashboard-2.0" :load-path ("lisp" "legacy-dashboard-2.0" "share") :module "legacy-dashboard.el" :config ("dashboard.conf" "endpoint=https://dashboard.example.test\nretries=3\n") :executable "dashboard-render" :render "dashboard/2.0:101,102,103" :process (0 "rendered:101 102 103\n"))"#
        ]],
    )
}

fn apel_registers_a_legacy_client_suite_and_builds_its_user_agent() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_registers_a_legacy_client_suite_and_builds_its_user_agent",
        r####"
(let ((product-obarray (make-vector 13 0))
      checks
      unsupported)
  (product-define "LegacyMail" nil '(5 1) "Cedar")
  (product-define "LegacySMTP" "LegacyMail" '(2 4) "Relay")
  (product-define "LegacyReader" "LegacyMail" '(3 7) "Spruce")
  (let ((smtp (product-find-by-name "LegacySMTP"))
        (reader (product-find-by-name "LegacyReader"))
        (suite (product-find-by-name "LegacyMail")))
    (product-add-checkers
     smtp
     (lambda (actual _target)
       (push actual checks)
       (unless
           (>=
            (product-version-compare actual '(2 0))
            0)
         (error
          "LegacySMTP %s is unsupported"
          (mapconcat #'number-to-string actual ".")))))
    (product-add-feature suite 'legacy-mail)
    (product-add-feature smtp 'legacy-smtp)
    (product-add-feature reader 'legacy-reader)
    (put 'legacy-mail 'product suite)
    (put 'legacy-smtp 'product smtp)
    (put 'legacy-reader 'product reader)
    (provide 'legacy-mail)
    (provide 'legacy-smtp)
    (provide 'legacy-reader)
    (product-run-checkers smtp '(2 4) t)
    (setq unsupported
          (condition-case error
              (progn
                (product-run-checkers smtp '(1 9) t)
                :accepted)
            (error
             (list (car error) (cadr error)))))
    (list
     :user-agent (product-string-verbose suite)
     :family (product-family-products suite)
     :features
     (mapcar
      (lambda (feature)
        (list
         feature
         (product-name
          (product-find feature))))
      '(legacy-mail legacy-smtp legacy-reader))
     :minimums
     (list
      (product-version>= smtp '(2 0))
      (product-version>= reader '(4 0)))
     :checks (nreverse checks)
     :unsupported unsupported)))
"####,
        expect![[
            r#"OK (:user-agent "LegacyMail/5.1 (Cedar) LegacyReader/3.7 (Spruce) LegacySMTP/2.4 (Relay)" :family ("LegacyReader" "LegacySMTP") :features ((legacy-mail "LegacyMail") (legacy-smtp "LegacySMTP") (legacy-reader "LegacyReader")) :minimums (t nil) :checks ((2 4) (1 9)) :unsupported (error "LegacySMTP 1.9 is unsupported"))"#
        ]],
    )
}

fn apel_archives_multilingual_attachments_with_safe_names_and_wire_encodings() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_archives_multilingual_attachments_with_safe_names_and_wire_encodings",
        r####"
(progn
  (require 'filename)
  (require 'pces)
  (let* ((root
        (file-name-as-directory
         (expand-file-name
          "apel-attachment-archive"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (archive (expand-file-name "attachments/" root))
       (manifest (expand-file-name "manifest.txt" root))
       (default-directory root)
       (filename-limit-length 24)
       (filename-filters
        '(filename-special-filter
          filename-eliminate-top-low-lines
          filename-canonicalize-low-lines
          filename-maybe-truncate-by-size
          filename-eliminate-bottom-low-lines))
       records
       result)
  (unwind-protect
      (progn
        (neomacs-apel-test-cleanup root)
        (make-directory archive t)
        (dolist
            (attachment
             '(("Q3 / plan?.txt" "Plain ASCII plan")
               ("Résumé: final.txt" "Résumé validé")
               ("設計 / notes.txt" "日本語の設計")))
          (let* ((original-name (nth 0 attachment))
                 (content (nth 1 attachment))
                 (safe-name
                  (replace-as-filename original-name))
                 (charset
                  (detect-mime-charset-string content))
                 (file (expand-file-name safe-name archive)))
            (with-temp-buffer
              (insert content)
              (write-region-as-mime-charset
               charset
               (point-min)
               (point-max)
               file))
            (let* ((wire
                    (neomacs-apel-test-file-string file))
                   (decoded
                    (decode-mime-charset-string
                     wire
                     charset)))
              (push
               (list
                original-name
                safe-name
                charset
                (neomacs-apel-test-file-bytes file)
                decoded)
               records))))
        (setq records (nreverse records))
        (with-temp-buffer
          (dolist (record records)
            (insert
             (format
              "%s\t%s\t%s\n"
              (nth 1 record)
              (nth 2 record)
              (nth 4 record))))
          (write-region-as-raw-text-CRLF
           (point-min)
           (point-max)
           manifest))
        (setq result
              (list
               :records records
               :manifest-bytes
               (neomacs-apel-test-file-bytes manifest))))
    (neomacs-apel-test-cleanup root))
    result))
"####,
        expect![[
            r#"OK (:records (("Q3 / plan?.txt" "Q3_plan_.txt" us-ascii (80 108 97 105 110 32 65 83 67 73 73 32 112 108 97 110) "Plain ASCII plan") ("Résumé: final.txt" "Résumé_final.txt" iso-8859-1 (82 233 115 117 109 233 32 118 97 108 105 100 233) #("Résumé validé" 0 13 (charset iso-8859-1))) ("設計 / notes.txt" "設計_notes.txt" iso-2022-jp (27 36 66 70 124 75 92 56 108 36 78 64 95 55 87) #("日本語の設計" 0 6 (charset japanese-jisx0208)))) :manifest-bytes (81 51 95 112 108 97 110 95 46 116 120 116 9 117 115 45 97 115 99 105 105 9 80 108 97 105 110 32 65 83 67 73 73 32 112 108 97 110 13 10 82 195 169 115 117 109 195 169 95 102 105 110 97 108 46 116 120 116 9 105 115 111 45 56 56 53 57 45 49 9 82 195 169 115 117 109 195 169 32 118 97 108 105 100 195 169 13 10 232 168 173 232 168 136 95 110 111 116 101 115 46 116 120 116 9 105 115 111 45 50 48 50 50 45 106 112 9 230 151 165 230 156 172 232 170 158 227 129 174 232 168 173 232 168 136 13 10))"#
        ]],
    )
}

fn apel_decodes_edits_and_reencodes_a_richtext_incident_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_decodes_edits_and_reencodes_a_richtext_incident_message",
        r####"
(with-temp-buffer
  (let ((fill-column 72)
        (enriched-verbose nil)
        decoded
        properties
        encoded)
    (insert
     "Content-Type: text/richtext\n"
     "Text-Width: 72\n\n"
     "<bold>Incident 42</bold><nl>\n"
     "<excerpt>Database recovered</excerpt>\n"
     "<comment>internal ticket SEC-9</comment>\n"
     "Operator saw <lt>healthy> status")
    (richtext-decode (point-min) (point-max))
    (setq decoded
          (buffer-substring-no-properties
           (point-min)
           (point-max))
          properties
          (mapcar
           (lambda (needle)
             (goto-char (point-min))
             (search-forward needle)
             (list
              needle
              (get-text-property
               (match-beginning 0)
               'face)
              (get-text-property
               (match-beginning 0)
               'invisible)
              (get-text-property
               (match-beginning 0)
               'hard)))
           '("Incident 42"
             "Database recovered"
             "internal ticket SEC-9"
             "Operator saw")))
    (goto-char (point-max))
    (insert "\nResolved by on-call")
    (put-text-property
     (- (point) 7)
     (point)
     'face
     'italic)
    (richtext-encode (point-min) (point-max))
    (setq encoded
          (buffer-substring-no-properties
           (point-min)
           (point-max)))
    (list
     :decoded decoded
     :properties properties
     :encoded encoded)))
"####,
        expect![[
            r#"OK (:decoded "Incident 42\nDatabase recovered\ninternal ticket SEC-9Operator saw <healthy> status" :properties (("Incident 42" (bold) nil nil) ("Database recovered" (excerpt) nil nil) ("internal ticket SEC-9" nil t nil) ("Operator saw" nil nil nil)) :encoded "Content-Type: text/richtext\nText-Width: 72\n\n<bold>Incident 42</bold><nl>\n<excerpt>Database recovered</excerpt><nl>\n<comment>internal ticket SEC-9</comment>Operator saw <lt>healthy> status<nl>\nResolved by <italic>on-call</italic>")"#
        ]],
    )
}

fn apel_byte_compiles_a_portable_plugin_with_a_broken_host_api_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_byte_compiles_a_portable_plugin_with_a_broken_host_api_fallback",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apel-portable-plugin"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "legacy-plugin.el" root))
       (compiled (concat source "c"))
       (default-directory root)
       (load-path (cons root load-path))
       result)
  (unwind-protect
      (progn
        (neomacs-apel-test-cleanup root)
        (require 'broken)
        (make-directory root t)
        (defun legacy-native-normalize (value)
          (concat "native:" (downcase value)))
        (with-temp-file source
          (insert
           ";; -*- lexical-binding: t -*-\n"
           "(require 'pym)\n"
           "(require 'broken)\n"
           "(broken-facility legacy-string-normalizer\n"
           "  \"Host has no legacy string normalizer\"\n"
           "  (fboundp 'legacy-host-normalize)\n"
           "  t)\n"
           "(defun-maybe legacy-host-normalize (value)\n"
           "  (upcase value))\n"
           "(defun-maybe-cond legacy-join (parts)\n"
           "  ((fboundp 'string-join)\n"
           "   (string-join parts \" | \"))\n"
           "  (t\n"
           "   (mapconcat #'identity parts \" | \")))\n"
           "(if-broken legacy-string-normalizer\n"
           "    (defun legacy-render-record (sender tags)\n"
           "      (format \"fallback:%s:%s\"\n"
           "              (legacy-host-normalize sender)\n"
           "              (legacy-join tags)))\n"
           "  (defun legacy-render-record (sender tags)\n"
           "    (format \"host:%s:%s\"\n"
           "            (legacy-host-normalize sender)\n"
           "            (legacy-join tags))))\n"
           "(broken-facility legacy-native-string-normalizer\n"
           "  \"Host provides its native string normalizer\"\n"
           "  (fboundp 'legacy-native-normalize)\n"
           "  t)\n"
           "(defun-maybe legacy-native-normalize (value)\n"
           "  (error \"Existing host normalizer was overwritten: %s\" value))\n"
           "(if-broken legacy-native-string-normalizer\n"
           "    (defun legacy-native-render-record (sender tags)\n"
           "      (format \"fallback:%s:%s\"\n"
           "              (legacy-native-normalize sender)\n"
           "              (legacy-join tags)))\n"
           "  (defun legacy-native-render-record (sender tags)\n"
           "    (format \"host:%s:%s\"\n"
           "            (legacy-native-normalize sender)\n"
           "            (legacy-join tags))))\n"
           "(provide 'legacy-plugin)\n"))
        (unless
            (byte-compile-file source)
          (error "Could not byte-compile portable plugin"))
        (delete-file source)
        (require
         'legacy-plugin
         compiled)
        (setq result
              (list
               :compiled
               (file-name-nondirectory compiled)
               :source-exists (file-exists-p source)
               :fallback-render
               (legacy-render-record
                "ops@example.test"
                '("urgent" "database"))
               :native-normalized
               (legacy-native-normalize
                "OPS@EXAMPLE.TEST")
               :host-render
               (legacy-native-render-record
                "OPS@EXAMPLE.TEST"
                '("urgent" "database")))))
    (neomacs-apel-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:compiled "legacy-plugin.elc" :source-exists nil :fallback-render "fallback:OPS@EXAMPLE.TEST:urgent | database" :native-normalized "native:ops@example.test" :host-render "host:native:ops@example.test:urgent | database")"#
        ]],
    )
}

fn apel_transcodes_a_binary_device_packet_through_a_portable_ccl_codec() -> ParityBatchCase {
    ParityBatchCase::value(
        "apel_transcodes_a_binary_device_packet_through_a_portable_ccl_codec",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apel-ccl-gateway"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (packet-file (expand-file-name "packets/device-17.bin" root))
       (default-directory root)
       (packet (unibyte-string 0 1 65 127 128 255))
       result)
  (unwind-protect
      (progn
        (neomacs-apel-test-cleanup root)
        (make-directory (file-name-directory packet-file) t)
        (define-ccl-program apel-device-identity
          '(1
            ((read r0)
             (loop
              (write-read-repeat r0)))))
        (make-ccl-coding-system
         'apel-device-packet
         ?D
         "Legacy device packet passthrough"
         'apel-device-identity
         'apel-device-identity)
        (let* ((wire
                (encode-coding-string
                 packet
                 'apel-device-packet))
               (decoded
                (decode-coding-string
                 wire
                 'apel-device-packet)))
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert wire)
            (write-region
             (point-min)
             (point-max)
             packet-file
             nil
             'silent))
          (setq result
                (list
                 :input (string-to-list packet)
                 :wire (string-to-list wire)
                 :disk
                 (neomacs-apel-test-file-bytes
                  packet-file)
                 :decoded
                 (string-to-list decoded)
                 :roundtrip
                 (equal
                  (string-to-list packet)
                  (string-to-list decoded))))))
    (neomacs-apel-test-cleanup root))
  result)
"####,
        expect![[
            r#"OK (:input (0 1 65 127 128 255) :wire (0 1 65 127 128 255) :disk (0 1 65 127 128 255) :decoded (0 1 65 127 128 255) :roundtrip t)"#
        ]],
    )
}

pub(super) fn workflows_calist_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_routes_incoming_mail_and_persists_a_safe_delivery_index()]
}

pub(super) fn workflows_path_util_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_selects_loads_and_runs_the_latest_installed_legacy_extension()]
}

pub(super) fn workflows_product_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_registers_a_legacy_client_suite_and_builds_its_user_agent()]
}

pub(super) fn workflows_mcharset_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_archives_multilingual_attachments_with_safe_names_and_wire_encodings()]
}

pub(super) fn workflows_richtext_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_decodes_edits_and_reencodes_a_richtext_incident_message()]
}

pub(super) fn workflows_pym_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_byte_compiles_a_portable_plugin_with_a_broken_host_api_fallback()]
}

pub(super) fn workflows_pccl_batch_cases() -> Vec<ParityBatchCase> {
    vec![apel_transcodes_a_binary_device_packet_through_a_portable_ccl_codec()]
}
