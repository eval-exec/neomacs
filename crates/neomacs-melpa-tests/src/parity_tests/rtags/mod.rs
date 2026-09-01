use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, RTAGS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const RTAGS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const RTAGS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'rtags)

(defvar rtags-test-root (make-temp-file "rtags-parity-" t))
(defvar rtags-test-bin
  (file-name-as-directory (expand-file-name "bin" rtags-test-root)))
(defvar rtags-test-requests
  (file-name-as-directory (expand-file-name "requests" rtags-test-root)))
(defvar rtags-test-responses
  (file-name-as-directory (expand-file-name "responses" rtags-test-root)))

(defun rtags-test-write (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (set-buffer-file-coding-system 'utf-8-unix)
    (insert text))
  path)

(defun rtags-test-install-rc ()
  (let ((path (expand-file-name "rc" rtags-test-bin)))
    (rtags-test-write
     path
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\n"
      "requests=$root/requests\n"
      "responses=$root/responses\n"
      "n=1\n"
      "while test -e \"$requests/$n.args\"; do n=$((n + 1)); done\n"
      ": > \"$requests/$n.args\"\n"
      "for arg in \"$@\"; do printf '%s\\n' \"$arg\" >> \"$requests/$n.args\"; done\n"
      "if test -f \"$responses/$n.stdout\"; then cat \"$responses/$n.stdout\"; fi\n"
      "status=0\n"
      "if test -f \"$responses/$n.status\"; then status=$(cat \"$responses/$n.status\"); fi\n"
      "exit \"$status\"\n"))
    (set-file-modes path #o755)
    path))

(defun rtags-test-reset-rc ()
  (dolist (directory (list rtags-test-requests rtags-test-responses))
    (when (file-directory-p directory)
      (delete-directory directory t))
    (make-directory directory t)))

(defun rtags-test-reply (stdout &optional status nth)
  (let ((number (or nth 1)))
    (rtags-test-write
     (expand-file-name (format "%d.stdout" number) rtags-test-responses)
     stdout)
    (rtags-test-write
     (expand-file-name (format "%d.status" number) rtags-test-responses)
     (number-to-string (or status 0)))))

(defun rtags-test-read (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun rtags-test-recorded-argv ()
  (mapcar
   (lambda (file)
     (split-string (rtags-test-read file) "\n" t))
   (sort
    (directory-files rtags-test-requests t "\\`[0-9]+\\.args\\'")
    #'string<)))

(defun rtags-test-normalize-string (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name rtags-test-root))
   "[PROJECT]" text t t))

(defun rtags-test-normalize-arg (arg)
  (setq arg (rtags-test-normalize-string arg))
  (if (string-match
       "\\`--unsaved-file=\\([^:]+\\):.*\\'" arg)
      (concat "--unsaved-file=" (match-string 1 arg) ":[UNSAVED]")
    arg))

(defun rtags-test-normalized-argv ()
  (mapcar
   (lambda (argv) (mapcar #'rtags-test-normalize-arg argv))
   (rtags-test-recorded-argv)))

(defun rtags-test-open (relative text)
  (let ((file (rtags-test-write
               (expand-file-name relative rtags-test-root) text)))
    (find-file-noselect file)))

(make-directory rtags-test-bin t)
(make-directory rtags-test-requests t)
(make-directory rtags-test-responses t)
(rtags-test-install-rc)
"####;

fn rtags_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RTAGS_MELPA_PIN, "rtags.el")
        .expect("prepare pinned rtags source below ./tmp")
        .with_prelude(RTAGS_TEST_PRELUDE)
        .with_timeout(RTAGS_TEST_TIMEOUT)
}

fn synchronous_rc_transport_builds_full_request_and_sends_the_unsaved_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source-buffer
        (rtags-test-open
         "src/widget.cpp"
         "int Widget::run() { return 40; }\n"))
       (source-file (buffer-file-name source-buffer))
       (config (rtags-test-write
                (expand-file-name "config/rc.conf" rtags-test-root)
                "jobs=2\n"))
       result
       unsaved-path
       unsaved-contents)
  (rtags-test-reset-rc)
  (rtags-test-reply "definition: Widget::run\n")
  (with-current-buffer source-buffer
    (goto-char (point-max))
    (insert "// unsaved λ edit\n")
    (let ((rtags-path rtags-test-bin)
          (rtags-verify-protocol-version t)
          (rtags-completions-enabled t)
          (rtags-show-containing-function t)
          (rtags-rc-config-path config)
          (rtags-socket-file "state/rdm.socket")
          (rtags-socket-address "127.0.0.1:4736")
          (rtags--socket-file-cache '("" ""))
          (rtags--socket-address-cache '("" "")))
      (with-temp-buffer
        (setq result
              (rtags-call-rc
               "-F" "Widget value"
               :path source-file
               :unsaved source-buffer
               :path-filter (expand-file-name "src" rtags-test-root)
               :path-filter-regex t
               :range-filter t
               :range-min 3
               :range-max 17
               :timeout 9
               :silent-query t
               :output t))
        (setq result (list result (buffer-string))))))
  (let* ((raw-argv (car (rtags-test-recorded-argv)))
         (unsaved-arg
          (cl-find-if
           (lambda (arg) (string-prefix-p "--unsaved-file=" arg))
           raw-argv)))
    (when (and unsaved-arg
               (string-match "\\`--unsaved-file=[^:]+:\\(.*\\)\\'" unsaved-arg))
      (setq unsaved-path (match-string 1 unsaved-arg)
            unsaved-contents
            (and (file-exists-p unsaved-path)
                 (rtags-test-read unsaved-path)))
      (when (file-exists-p unsaved-path)
        (delete-file unsaved-path))))
  (list
   :result result
   :argv (rtags-test-normalized-argv)
   :unsaved-contents unsaved-contents
   :unsaved-copy-removed (and unsaved-path (not (file-exists-p unsaved-path)))))
"####;
    let expect = expect![[
        r####"OK (:result (t "definition: Widget::run\n") :argv (("--socket-address=127.0.0.1:4736" "--socket-file=state/rdm.socket" "--current-file=[PROJECT]/src/widget.cpp" "-o" "--timeout=9" "--range-filter=3-17" "--silent-query" "-b" "--config=[PROJECT]/config/rc.conf" "--unsaved-file=[PROJECT]/src/widget.cpp:[UNSAVED]" "-Z" "--path-filter=[PROJECT]/src" "-z" "-t128" "-F" "Widget value")) :unsaved-contents "int Widget::run() { return 40; }\n// unsaved λ edit\n" :unsaved-copy-removed t)"####
    ]];
    ParityBatchCase::value(
        "synchronous_rc_transport_builds_full_request_and_sends_the_unsaved_buffer",
        elisp_form,
        expect,
    )
}

fn rc_exit_statuses_clear_stale_output_and_publish_connection_and_index_state() -> ParityBatchCase {
    let elisp_form = r####"
(let ((rtags-path rtags-test-bin)
      results)
  (rtags-test-reset-rc)
  (rtags-test-reply "stale connection output\n" 36 1)
  (rtags-test-reply "stale index output\n" 35 2)
  (rtags-test-reply "stale protocol output\n" 37 3)
  (dolist (kind '(connection not-indexed protocol))
    (with-temp-buffer
      (insert "prefix that process output replaces\n")
      (let ((ok (rtags-call-rc :path nil :noerror t :output t "--status")))
        (push
         (list kind
               :ok ok
               :buffer (buffer-string)
               :not-connected rtags-last-request-not-connected
               :not-indexed rtags-last-request-not-indexed)
         results))))
  (list :results (nreverse results)
        :argv (rtags-test-normalized-argv)))
"####;
    let expect = expect![[
        r####"OK (:results ((connection :ok nil :buffer "" :not-connected t :not-indexed nil) (not-indexed :ok nil :buffer "" :not-connected nil :not-indexed t) (protocol :ok nil :buffer "" :not-connected nil :not-indexed nil)) :argv (("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "--status") ("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "--status") ("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "--status")))"####
    ]];
    ParityBatchCase::value(
        "rc_exit_statuses_clear_stale_output_and_publish_connection_and_index_state",
        elisp_form,
        expect,
    )
}

fn unicode_locations_round_trip_byte_offsets_and_drive_real_cross_file_navigation()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((first
        (rtags-test-open
         "src/first.cpp"
         "// λ header\nint alpha = 1;\n"))
       (second
        (rtags-test-open
         "src/second.cpp"
         "// 发布\nWidget value;\n尾\n"))
       (second-file (buffer-file-name second))
       (hook-count 0)
       first-state
       destination)
  (with-current-buffer first
    (goto-char (point-min))
    (forward-line 1)
    (search-forward "alpha")
    (let ((offset (rtags-offset)))
      (goto-char (point-min))
      (rtags-goto-offset offset)
      (setq first-state
            (list
             :offset offset
             :point (point)
             :symbol (thing-at-point 'symbol t)
             :location
             (rtags-test-normalize-string (rtags-current-location))))))
  (let ((rtags-location-stack nil)
        (rtags-location-stack-index 0)
        (rtags-max-bookmark-count 3)
        (rtags-after-find-file-hook
         (list (lambda () (setq hook-count (1+ hook-count))))))
    (save-window-excursion
      (rtags-goto-location
       (format "%s:2:8:" second-file) nil nil t)
      (setq destination
            (list
             :file
             (rtags-test-normalize-string (buffer-file-name))
             :line (line-number-at-pos)
             :column (1+ (- (point) (pos-bol)))
             :symbol (thing-at-point 'symbol t)
             :mark (marker-position (mark-marker))
             :hook-count hook-count
             :stack
             (mapcar #'rtags-test-normalize-string
                     rtags-location-stack)))))
  (list :first first-state :destination destination))
"####;
    let expect = expect![[
        r####"OK (:first (:offset 22 :point 22 :symbol "alpha" :location "[PROJECT]/src/first.cpp:2:10:") :destination (:file "[PROJECT]/src/second.cpp" :line 2 :column 8 :symbol "value" :mark 1 :hook-count 1 :stack ("[PROJECT]/src/second.cpp:2:8:")))"####
    ]];
    ParityBatchCase::value(
        "unicode_locations_round_trip_byte_offsets_and_drive_real_cross_file_navigation",
        elisp_form,
        expect,
    )
}

fn source_extraction_handles_location_length_ranges_multiline_limits_and_unicode_offsets()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((buffer
        (rtags-test-open
         "include/widget.hpp"
         (concat
          "struct Widget {\n"
          "  int run(int λvalue, int count);\n"
          "  const char *label = \"发布\";\n"
          "};\n")))
       (file (buffer-file-name buffer))
       by-length
       by-range
       limited)
  (setq by-length
        (rtags-get-file-contents
         :location (format "%s:2:7" file)
         :length 18))
  (setq by-range
        (rtags-get-file-contents
         :file file
         :startLine 2
         :endLine 3))
  (setq limited
        (rtags-get-file-contents
         :file file
         :startLine 1
         :endLine 4
         :maxlines 2))
  (list :length by-length :range by-range :limited limited))
"####;
    let expect = expect![[
        r####"OK (:length ((contents . "run(int λvalue, in") (offset . 22)) :range ((contents . "  int run(int λvalue, int count);\n  const char *label = \"发布\";") (offset . 16)) :limited ((contents . "struct Widget {\n  int run(int λvalue, int count);") (offset . 0)))"####
    ]];
    ParityBatchCase::value(
        "source_extraction_handles_location_length_ranges_multiline_limits_and_unicode_offsets",
        elisp_form,
        expect,
    )
}

fn diagnostics_stream_consumes_complete_records_preserves_partial_input_and_quarantines_malformed_data()
-> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (let* ((rtags-diagnostics-errors nil)
        (rtags-last-index nil)
        (rtags-last-total nil)
        (rtags-remaining-jobs nil)
        (hook-count 0)
        (rtags-diagnostics-summary-in-mode-line nil)
        (rtags-diagnostics-hook
         (list (lambda () (setq hook-count (1+ hook-count))))))
    (insert
     (concat
      "'(progress 3 10 7)\n"
      "(broken . )\n"
      "'(progress 8 10 2)\n"
      "'(progress 9"))
    (rtags-parse-diagnostics)
    (list
     :progress (list rtags-last-index rtags-last-total rtags-remaining-jobs)
     :hook-count hook-count
     :errors rtags-diagnostics-errors
     :remaining (buffer-string))))
"####;
    let expect = expect![[
        r####"OK (:progress (8 10 2) :hook-count 3 :errors ("(broken . )") :remaining "'(progress 9")"####
    ]];
    ParityBatchCase::value(
        "diagnostics_stream_consumes_complete_records_preserves_partial_input_and_quarantines_malformed_data",
        elisp_form,
        expect,
    )
}

fn result_buffer_formats_real_locations_creates_bookmarks_and_preserves_navigation_metadata()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((first
        (rtags-test-open "src/results-a.cpp" "int alpha;\nint beta;\n"))
       (second
        (rtags-test-open "src/results-b.cpp" "int gamma;\nint delta;\n"))
       (first-file (buffer-file-name first))
       (second-file (buffer-file-name second))
       (results (rtags-get-buffer "*RTags parity results*"))
       summary)
  (unwind-protect
      (with-current-buffer results
        (insert
         (format
          "%s:2:5:\tint beta;\n%s:1:5:\tint gamma;\n"
          first-file second-file))
        (let ((rtags-use-bookmarks t)
              (rtags-verbose-results nil)
              (rtags-buffer-bookmarks 0))
          (rtags-format-results)
          (goto-char (point-min))
          (let ((first-prop (get-text-property (point) 'rtags-bookmark-index)))
            (forward-line 1)
            (setq summary
                  (list
                   :mode major-mode
                   :read-only buffer-read-only
                   :text
                   (rtags-test-normalize-string
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
                   :first-property
                   (list (car first-prop)
                         (= (cdr first-prop) (point-min)))
                   :second-property
                   (let ((property
                          (get-text-property
                           (point) 'rtags-bookmark-index)))
                     (list (car property)
                           (= (cdr property) (point))))
                   :bookmarks (sort (rtags-bookmark-all-names) #'string<)
                   :bookmark-count rtags-buffer-bookmarks)))
          (rtags-reset-bookmarks)))
    (when (buffer-live-p results)
      (kill-buffer results)))
  summary)
"####;
    let expect = expect![[
        r####"OK (:mode rtags-mode :read-only t :text "[PROJECT]/src/results-a.cpp:2:5:\11int beta;\n[PROJECT]/src/results-b.cpp:1:5:\11int gamma;" :first-property (0 t) :second-property (1 t) :bookmarks ("RTags_0" "RTags_1") :bookmark-count 2)"####
    ]];
    ParityBatchCase::value(
        "result_buffer_formats_real_locations_creates_bookmarks_and_preserves_navigation_metadata",
        elisp_form,
        expect,
    )
}

fn completion_table_queries_candidates_and_exact_matches_through_the_real_client_transport()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((rtags-path rtags-test-bin)
      (rtags-symbolnames-case-insensitive t)
      (rtags-wildcard-symbol-names t)
      try candidates exact)
  (rtags-test-reset-rc)
  (rtags-test-reply
   "'(\"Widget\" \"WidgetFactory\" \"WidgetRunner\")\n" 0 1)
  (rtags-test-reply
   "'(\"Widget\" \"WidgetFactory\" \"WidgetRunner\")\n" 0 2)
  (rtags-test-reply "Widget\n" 0 3)
  (setq try (rtags-symbolname-complete "Widget" nil nil)
        candidates (rtags-symbolname-complete "Widget" nil t)
        exact (rtags-symbolname-complete "Widget" nil 'lambda))
  (list :try try
        :candidates candidates
        :exact exact
        :argv (rtags-test-normalized-argv)))
"####;
    let expect = expect![[
        r####"OK (:try "Widget" :candidates ("Widget" "WidgetFactory" "WidgetRunner") :exact t :argv (("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "--elisp" "-S" "Widget" "-I" "--wildcard-symbol-names") ("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "--elisp" "-S" "Widget" "-I" "--wildcard-symbol-names") ("--current-file=[ORACLE-SANDBOX]/" "-z" "-t128" "-N" "-F" "Widget" "-I" "--wildcard-symbol-names")))"####
    ]];
    ParityBatchCase::value(
        "completion_table_queries_candidates_and_exact_matches_through_the_real_client_transport",
        elisp_form,
        expect,
    )
}

#[test]
fn rtags_package_batch() {
    let cases = vec![
        synchronous_rc_transport_builds_full_request_and_sends_the_unsaved_buffer(),
        rc_exit_statuses_clear_stale_output_and_publish_connection_and_index_state(),
        unicode_locations_round_trip_byte_offsets_and_drive_real_cross_file_navigation(),
        source_extraction_handles_location_length_ranges_multiline_limits_and_unicode_offsets(),
        diagnostics_stream_consumes_complete_records_preserves_partial_input_and_quarantines_malformed_data(),
        result_buffer_formats_real_locations_creates_bookmarks_and_preserves_navigation_metadata(),
        completion_table_queries_candidates_and_exact_matches_through_the_real_client_transport(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed rtags parity test");
    assert_oracle_batch_cases(rtags_oracle(), test_name, "rtags_parity", &cases);
}
