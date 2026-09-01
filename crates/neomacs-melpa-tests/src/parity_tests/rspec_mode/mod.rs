use std::time::Duration;

use crate::{CachedMelpaOracle, RSPEC_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const RSPEC_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

// The external replay below is an immutable recording of RSpec core 3.13.6,
// expectations 3.13.5, and support 3.13.6.  Only the unavailable Ruby/RSpec
// executable is adapted: rspec-mode still owns command construction and GNU
// compilation still owns the shell process, filters, sentinel, and navigation.
// The ambient Nix Ruby's broken-default-gem warnings (debug/racc/rbs) were on
// stderr and were excluded at recording time; semantic stderr was empty.
const RSPEC_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json)
(require 'imenu)
(require 'compile)
(require 'ruby-mode)
(require 'rspec-mode)

;; Establish GNU's reserved menu-bar row before any case captures windows.
(set-window-configuration (current-window-configuration))

(defconst rspec360-test-source
  "module Inventory\n  class Ledger\n    def total(lines)\n      lines.sum\n    end\n\n    def label(id)\n      \"order-界-#{id}\"\n    end\n  end\nend\n")

(defconst rspec360-test-spec
  "require_relative \"../../lib/inventory/ledger\"\nrequire \"rspec/expectations\"\n\nRSpec.describe Inventory::Ledger do\n  subject(:ledger) { described_class.new }\n\n  describe \"#total\" do\n    it \"adds realistic line items\" do\n      expect(ledger.total([12, 8, 5])).to eq(25)\n    end\n\n    it \"reports a bad expected total\" do\n      puts \"     [Screenshot Image]: ./capybara/order receipt_123.png\"\n      expect(ledger.total([12, 8, 5])).to eq(24)\n    end\n  end\n\n  describe \"#label\" do\n    it \"preserves Unicode identifiers\" do\n      expect(ledger.label(7)).to eq(\"order-界-8\")\n    end\n  end\nend\n")

(defconst rspec360-test-pass-output
  "Run options: include {:locations=>{\"./spec/inventory/ledger_spec.rb\"=>[8]}}\n\nInventory::Ledger\n  #total\n\e[32m    adds realistic line items\e[0m\n\nFinished in 0.00169 seconds (files took 0.13652 seconds to load)\n\e[32m1 example, 0 failures\e[0m\n\n")

(defconst rspec360-test-full-output
  "\nInventory::Ledger\n  #total\n\e[32m    adds realistic line items\e[0m\n     [Screenshot Image]: ./capybara/order receipt_123.png\n\e[31m    reports a bad expected total (FAILED - 1)\e[0m\n  #label\n\e[31m    preserves Unicode identifiers (FAILED - 2)\e[0m\n\nFailures:\n\n  1) Inventory::Ledger#total reports a bad expected total\n     \e[31mFailure/Error: expect(ledger.total([12, 8, 5])).to eq(24)\e[0m\n     \e[31m\e[0m\n     \e[31m  expected: 24\e[0m\n     \e[31m       got: 25\e[0m\n     \e[31m\e[0m\n     \e[31m  (compared using ==)\e[0m\n     \e[36m# ./spec/inventory/ledger_spec.rb:14:in `block (3 levels) in <top (required)>'\e[0m\n\n  2) Inventory::Ledger#label preserves Unicode identifiers\n     \e[31mFailure/Error: expect(ledger.label(7)).to eq(\"order-界-8\")\e[0m\n     \e[31m\e[0m\n     \e[31m  expected: \"order-界-8\"\e[0m\n     \e[31m       got: \"order-界-7\"\e[0m\n     \e[31m\e[0m\n     \e[31m  (compared using ==)\e[0m\n     \e[36m# ./spec/inventory/ledger_spec.rb:20:in `block (3 levels) in <top (required)>'\e[0m\n\nFinished in 0.02116 seconds (files took 0.31353 seconds to load)\n\e[31m3 examples, 2 failures\e[0m\n\nFailed examples:\n\n\e[31mrspec ./spec/inventory/ledger_spec.rb:12\e[0m \e[36m# Inventory::Ledger#total reports a bad expected total\e[0m\n\e[31mrspec ./spec/inventory/ledger_spec.rb:19\e[0m \e[36m# Inventory::Ledger#label preserves Unicode identifiers\e[0m\n\n")

(defconst rspec360-test-failed-output
  "Run options: include {:locations=>{\"./spec/inventory/ledger_spec.rb\"=>[12, 19]}}\n\nInventory::Ledger\n  #total\n     [Screenshot Image]: ./capybara/order receipt_123.png\n\e[31m    reports a bad expected total (FAILED - 1)\e[0m\n  #label\n\e[31m    preserves Unicode identifiers (FAILED - 2)\e[0m\n\nFailures:\n\n  1) Inventory::Ledger#total reports a bad expected total\n     \e[31mFailure/Error: expect(ledger.total([12, 8, 5])).to eq(24)\e[0m\n     \e[31m\e[0m\n     \e[31m  expected: 24\e[0m\n     \e[31m       got: 25\e[0m\n     \e[31m\e[0m\n     \e[31m  (compared using ==)\e[0m\n     \e[36m# ./spec/inventory/ledger_spec.rb:14:in `block (3 levels) in <top (required)>'\e[0m\n\n  2) Inventory::Ledger#label preserves Unicode identifiers\n     \e[31mFailure/Error: expect(ledger.label(7)).to eq(\"order-界-8\")\e[0m\n     \e[31m\e[0m\n     \e[31m  expected: \"order-界-8\"\e[0m\n     \e[31m       got: \"order-界-7\"\e[0m\n     \e[31m\e[0m\n     \e[31m  (compared using ==)\e[0m\n     \e[36m# ./spec/inventory/ledger_spec.rb:20:in `block (3 levels) in <top (required)>'\e[0m\n\nFinished in 0.00602 seconds (files took 0.05766 seconds to load)\n\e[31m2 examples, 2 failures\e[0m\n\nFailed examples:\n\n\e[31mrspec ./spec/inventory/ledger_spec.rb:12\e[0m \e[36m# Inventory::Ledger#total reports a bad expected total\e[0m\n\e[31mrspec ./spec/inventory/ledger_spec.rb:19\e[0m \e[36m# Inventory::Ledger#label preserves Unicode identifiers\e[0m\n\n")

(defconst rspec360-test-replay-script
  "import hashlib, json, os, sys\nroot = os.path.realpath(os.environ.get('RSPEC360_ROOT', ''))\ntrace = os.environ.get('RSPEC360_TRACE', '')\nmiss = os.environ.get('RSPEC360_MISS', '')\ndef digest(path):\n    try:\n        with open(path, 'rb') as stream:\n            return hashlib.sha256(stream.read()).hexdigest()\n    except OSError:\n        return None\nrecord = {'argv': sys.argv[1:], 'cwd': '[ROOT]' if os.path.realpath(os.getcwd()) == root else os.path.realpath(os.getcwd()), 'debug': os.environ.get('RUBY_DEBUG_NO_RELINE')}\nexpected_hashes = {'.rspec': 'cb9ddede06deb019921d4b43610e3204e7d24330cd85abe9acd1185f7e15fe62', 'lib/inventory/ledger.rb': 'aaa9b957d8bfeec87c878ae7ce61fcdac57bff3d350357980c74b4a6175d9bd9', 'spec/inventory/ledger_spec.rb': '34ba68622f147b0d14d875ed93d3a732e6e671d94e262a111f229f4e269e37b3', 'capybara/order receipt_123.png': '45aab7ce7a7aaae0e004141ec0879220c62b9a13d7eb38bca15491d3248f8b43', 'replay/pass.out': '0c2ce3db764e611f7dc953f47efeeec080734a33dcd84bc7b982373993e0f2da', 'replay/full.out': 'aafabb21d52cf38104f89840ede99a7cc9540ebd52710c5b0e3cadbca8018d3c', 'replay/failed.out': '28b32b5930d51481e88f8fd0a3549deeb237ad6aafe08d58827b18c604b8785f'}\nfiles_ok = bool(root) and all(digest(os.path.join(root, path)) == value for path, value in expected_hashes.items())\nkey = tuple(sys.argv[1:])\nrecordings = {('exec', 'rspec', '--options', '.rspec', 'spec/inventory/ledger_spec.rb:8'): ('replay/pass.out', 0), ('exec', 'rspec', '--options', '.rspec', 'spec/inventory/ledger_spec.rb'): ('replay/full.out', 1), ('exec', 'rspec', '--options', '.rspec', 'spec/inventory/ledger_spec.rb:12', 'spec/inventory/ledger_spec.rb:19'): ('replay/failed.out', 1)}\nif record['cwd'] != '[ROOT]' or record['debug'] != 'true' or not files_ok or key not in recordings:\n    if miss:\n        with open(miss, 'a', encoding='utf-8') as stream:\n            stream.write(json.dumps(record, ensure_ascii=False, separators=(',', ':')) + '\\n')\n    sys.stderr.write('UNRECORDED rspec invocation\\n')\n    sys.exit(86)\nwith open(trace, 'a', encoding='utf-8') as stream:\n    stream.write(json.dumps(record, ensure_ascii=False, separators=(',', ':')) + '\\n')\npath, status = recordings[key]\nwith open(os.path.join(root, path), 'rb') as stream:\n    sys.stdout.buffer.write(stream.read())\nsys.exit(status)\n")

(defconst rspec360-test-state-symbols
  '(rspec-last-directory rspec-last-arguments rspec-last-failed-specs
    rspec-use-rake-when-possible rspec-spec-command rspec-use-rvm
    rspec-use-chruby rspec-use-relative-path
    rspec-use-bundler-when-possible rspec-use-docker-when-possible
    rspec-use-vagrant-when-possible rspec-use-zeus-when-possible
    rspec-use-spring-when-possible rspec-use-opts-file-when-available
    rspec-command-options rspec-autosave-buffer
    rspec-allow-multiple-compilation-buffers rspec-compilation-skip-threshold
    rspec-before-verification-hook rspec-after-verification-hook
    compilation-finish-functions compilation-in-progress next-error-last-buffer
    enable-local-variables enable-dir-local-variables
    unread-command-events executing-kbd-macro this-command real-this-command
    last-command real-last-command last-command-event last-input-event
    current-prefix-arg prefix-arg deactivate-mark
    rspec360-test-hook-events rspec360-test-finish-events)
  "RSpec Mode and command-loop state restored after every shared case.")

(defvar rspec360-test-owned-buffers nil)
(defvar rspec360-test-owned-processes nil)
(defvar rspec360-test-hook-events nil)
(defvar rspec360-test-finish-events nil)

(defun rspec360-test-variable-state (symbol)
  "Return SYMBOL's boundness and exact value identity."
  (if (boundp symbol) (list :bound t :value (symbol-value symbol)) '(:bound nil)))

(defun rspec360-test-restore-variable (symbol state)
  "Restore SYMBOL to STATE."
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun rspec360-test-window-state ()
  "Return stable state for every ordinary window."
  (mapcar (lambda (window)
            (list window (window-buffer window) (window-edges window)
                  (window-point window) (window-start window)
                  (window-hscroll window) (window-vscroll window t)
                  (copy-tree (window-prev-buffers window))
                  (copy-tree (window-next-buffers window))))
          (window-list nil 'no-minibuf)))

(defun rspec360-test-restore-windows (configuration state)
  "Restore CONFIGURATION and per-window STATE."
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (nth 0 entry)))
      (unless (window-live-p window)
        (error "RSpec baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (nth 7 entry)))
      (set-window-next-buffers window (copy-tree (nth 8 entry)))
      (set-window-point window (nth 3 entry))
      (set-window-start window (nth 4 entry) 'noforce)
      (set-window-hscroll window (nth 5 entry))
      (set-window-vscroll window (nth 6 entry) t))))

(defun rspec360-test-write (path text &optional binary)
  "Write TEXT to PATH, creating its parent."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write (if binary 'no-conversion 'utf-8-unix)))
    (with-temp-file path
      (when binary (set-buffer-multibyte nil))
      (insert text)))
  path)

(defun rspec360-test-world-path (world relative)
  "Return RELATIVE below WORLD's project root."
  (expand-file-name relative (plist-get world :project)))

(defun rspec360-test-read (path)
  "Read PATH without properties, or return nil when absent."
  (when (file-exists-p path)
    (with-temp-buffer
      (insert-file-contents-literally path)
      (buffer-substring-no-properties (point-min) (point-max)))))

(defun rspec360-test-lines (path)
  "Return nonempty lines in PATH oldest first."
  (let ((text (rspec360-test-read path)))
    (and text (split-string text "\n" t))))

(defun rspec360-test-allocate-world (case-name)
  "Allocate and return the owned root record for CASE-NAME."
  (let* ((raw-owner (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (_owner-gate
          (unless (and raw-owner (not (string-empty-p raw-owner))
                       (file-name-absolute-p raw-owner)
                       (file-directory-p raw-owner))
            (error "RSpec sandbox root is unsafe: %S" raw-owner)))
         (owner (file-name-as-directory (file-truename raw-owner)))
         (python (executable-find "python3"))
         (_python-gate
          (unless (and python (file-name-absolute-p python)
                       (file-executable-p python))
            (error "RSpec replay interpreter is unsafe: %S" python)))
         (root (make-temp-file
                (expand-file-name (format "rspec360-%s-" case-name) owner) t))
         (project (expand-file-name "project space 界/" root))
         (no-project (expand-file-name "no-project/" root))
         (bin (expand-file-name "bin/" project))
         (trace (expand-file-name "replay/trace.jsonl" project))
         (miss (expand-file-name "replay/miss.jsonl" project)))
    (let (world)
      (unwind-protect
          (progn
            (unless (and (file-name-absolute-p root)
                         (string-prefix-p
                          owner (file-name-as-directory (file-truename root))))
              (error "RSpec unsafe world/interpreter: %S"
                     (list owner root python)))
            (setq world
                  (list :owner owner :root root :project project
                        :no-project no-project :bin bin :trace trace
                        :miss miss :python python)))
        (unless world
          (when (and (file-directory-p root)
                     (string-prefix-p owner (file-name-as-directory root)))
            (delete-directory root t))))
      world)))

(defun rspec360-test-materialize-world (world)
  "Materialize and strength-check the already-owned WORLD."
  (let* ((project (plist-get world :project))
         (no-project (plist-get world :no-project))
         (bin (plist-get world :bin))
         (trace (plist-get world :trace))
         (miss (plist-get world :miss))
         (python (plist-get world :python)))
    (dolist (entry `(("Gemfile" . "source 'https://example.invalid'\n")
                     (".rspec" . "--format documentation\n--force-color\n")
                     ("lib/inventory/ledger.rb" . ,rspec360-test-source)
                     ("spec/inventory/ledger_spec.rb" . ,rspec360-test-spec)
                     ("replay/pass.out" . ,rspec360-test-pass-output)
                     ("replay/full.out" . ,rspec360-test-full-output)
                     ("replay/failed.out" . ,rspec360-test-failed-output)))
      (rspec360-test-write (expand-file-name (car entry) project) (cdr entry)))
    (rspec360-test-write
     (expand-file-name "capybara/order receipt_123.png" project)
     (base64-decode-string
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL9WQAAAABJRU5ErkJggg==")
     t)
    (make-directory no-project t)
    (let ((program (rspec360-test-write
                    (expand-file-name "bundle" bin)
                    (concat "#!" python "\n" rspec360-test-replay-script))))
      (set-file-modes program #o700))
    (let* ((process-environment
            (append (list (concat "RSPEC360_ROOT=" (directory-file-name project))
                          (concat "RSPEC360_TRACE=" trace)
                          (concat "RSPEC360_MISS=" miss)
                          "RUBY_DEBUG_NO_RELINE=true")
                    process-environment))
           (exec-path (cons bin exec-path))
           results)
      (let ((recording (expand-file-name "replay/pass.out" project))
            (default-directory project)
            (output (generate-new-buffer " *rspec360-rejection*")))
        (unwind-protect
            (progn
              (rspec360-test-write recording
                                   (concat rspec360-test-pass-output "tampered"))
              (push (list :provenance
                          (call-process "bundle" nil output nil
                                        "exec" "rspec" "--options" ".rspec"
                                        "spec/inventory/ledger_spec.rb:8")
                          (with-current-buffer output (buffer-string)))
                    results))
          (rspec360-test-write recording rspec360-test-pass-output)
          (kill-buffer output)))
      (dolist
          (probe
           `((:line ,project t
                    ("exec" "rspec" "--options" ".rspec"
                     "spec/inventory/ledger_spec.rb:9"))
             (:order ,project t
                     ("exec" "rspec" "--options" ".rspec"
                      "spec/inventory/ledger_spec.rb:19"
                      "spec/inventory/ledger_spec.rb:12"))
             (:cwd ,no-project t
                   ("exec" "rspec" "--options" ".rspec"
                    "spec/inventory/ledger_spec.rb:8"))
             (:env ,project nil
                   ("exec" "rspec" "--options" ".rspec"
                    "spec/inventory/ledger_spec.rb:8"))))
        (cl-destructuring-bind (phase directory debug arguments) probe
          (let ((default-directory directory)
                (process-environment (copy-sequence process-environment))
                (output (generate-new-buffer " *rspec360-rejection*")))
            (unless debug (setenv "RUBY_DEBUG_NO_RELINE" nil))
            (unwind-protect
                (push (list phase
                            (apply #'call-process "bundle" nil output nil
                                   arguments)
                            (with-current-buffer output (buffer-string)))
                      results)
              (kill-buffer output)))))
      (let* ((miss-lines (rspec360-test-lines miss))
             (normalized-misses
              (mapcar
               (lambda (line)
                 (replace-regexp-in-string
                  (regexp-quote (directory-file-name no-project))
                  "[NO-PROJECT]" line t t))
               miss-lines))
             (expected-misses
              '("{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:8\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}"
                "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:9\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}"
                "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:19\",\"spec/inventory/ledger_spec.rb:12\"],\"cwd\":\"[ROOT]\",\"debug\":\"true\"}"
                "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:8\"],\"cwd\":\"[NO-PROJECT]\",\"debug\":\"true\"}"
                "{\"argv\":[\"exec\",\"rspec\",\"--options\",\".rspec\",\"spec/inventory/ledger_spec.rb:8\"],\"cwd\":\"[ROOT]\",\"debug\":null}")))
        (unless (and
                 (equal (nreverse results)
                        '((:provenance 86 "UNRECORDED rspec invocation\n")
                          (:line 86 "UNRECORDED rspec invocation\n")
                          (:order 86 "UNRECORDED rspec invocation\n")
                          (:cwd 86 "UNRECORDED rspec invocation\n")
                          (:env 86 "UNRECORDED rspec invocation\n")))
                 (equal normalized-misses expected-misses)
                 (null (rspec360-test-lines trace)))
          (error "RSpec replay rejection boundary failed: %S"
                 (list results normalized-misses
                       (rspec360-test-lines trace)))))
      (when (file-exists-p miss) (delete-file miss))
      world)))

(defun rspec360-test-configure (world)
  "Select the one documented owned runner represented by WORLD."
  ;; `setenv' may reuse a cons cell, so fork the environment before mutation.
  (setq process-environment (copy-sequence process-environment))
  (setq exec-path (cons (plist-get world :bin) exec-path))
  (setenv "PATH" (concat (plist-get world :bin) path-separator (getenv "PATH")))
  (setenv "RSPEC360_ROOT" (directory-file-name (plist-get world :project)))
  (setenv "RSPEC360_TRACE" (plist-get world :trace))
  (setenv "RSPEC360_MISS" (plist-get world :miss))
  (setq rspec-use-rake-when-possible nil
        rspec-use-rvm nil rspec-use-chruby nil
        rspec-use-relative-path t
        rspec-use-bundler-when-possible t
        rspec-use-docker-when-possible nil
        rspec-use-vagrant-when-possible nil
        rspec-use-zeus-when-possible nil
        rspec-use-spring-when-possible nil
        rspec-use-opts-file-when-available t
        rspec-autosave-buffer nil
        rspec-allow-multiple-compilation-buffers nil
        rspec-compilation-skip-threshold 2
        rspec-before-verification-hook '(rspec360-test-before-hook)
        rspec-after-verification-hook '(rspec360-test-after-hook)
        compilation-finish-functions '(rspec360-test-finish-hook)
        rspec-last-directory nil rspec-last-arguments nil
        rspec-last-failed-specs nil
        enable-local-variables nil enable-dir-local-variables nil
        rspec360-test-hook-events nil rspec360-test-finish-events nil))

(defun rspec360-test-before-hook ()
  "Record the public pre-verification hook."
  (setq rspec360-test-hook-events
        (append rspec360-test-hook-events
                (list (list :before rspec-last-failed-specs)))))

(defun rspec360-test-after-hook ()
  "Record that failure storage preceded the public after hook."
  (setq rspec360-test-hook-events
        (append rspec360-test-hook-events
                (list (list :after (copy-sequence rspec-last-failed-specs))))))

(defun rspec360-test-finish-hook (buffer message)
  "Record GNU compilation's terminal BUFFER and MESSAGE."
  (setq rspec360-test-finish-events
        (append rspec360-test-finish-events
                (list (list (buffer-name buffer)
                            (string-trim-right message))))))

(defun rspec360-test-normalize-string (world value)
  "Normalize only owned root and recorded/compilation time in VALUE."
  (let* ((project (directory-file-name (plist-get world :project)))
         (abbreviated (directory-file-name (abbreviate-file-name project)))
         (text (substring-no-properties value)))
    (setq text (replace-regexp-in-string
                (regexp-quote abbreviated) "[ROOT]" text t t))
    (setq text (replace-regexp-in-string
                (regexp-quote project) "[ROOT]" text t t))
    (setq text (replace-regexp-in-string
                "Finished in [0-9.]+ seconds (files took [0-9.]+ seconds to load)"
                "Finished in <DURATION> seconds (files took <DURATION> seconds to load)"
                text t t))
    (setq text (replace-regexp-in-string
                ", duration [0-9.]+ s" ", duration <DURATION>" text t t))
    (setq text (replace-regexp-in-string
                "\\(Compilation \\(?:started\\|finished\\|exited abnormally with code [0-9]+\\) at \\)[^,\n]*\\(, duration <DURATION>\\)?$"
                "\\1<TIME>\\2" text t nil))
    text))

(defun rspec360-test-normalize (world value)
  "Normalize stable owned path/time fields recursively in VALUE."
  (cond ((stringp value) (rspec360-test-normalize-string world value))
        ((consp value) (cons (rspec360-test-normalize world (car value))
                             (rspec360-test-normalize world (cdr value))))
        ((vectorp value) (vconcat (mapcar (lambda (item)
                                           (rspec360-test-normalize world item))
                                         value)))
        (t value)))

(defun rspec360-test-own-buffer (buffer)
  "Register BUFFER as case-owned and return it."
  (cl-pushnew buffer rspec360-test-owned-buffers :test #'eq)
  buffer)

(defun rspec360-test-visit (world relative)
  "Visit RELATIVE project file in WORLD through GNU file machinery."
  (let ((enable-local-variables nil)
        (enable-dir-local-variables nil))
    (rspec360-test-own-buffer
     (find-file-noselect (rspec360-test-world-path world relative)))))

(defun rspec360-test-command-loop (keys)
  "Drive KEYS through the real bounded command loop."
  (when unread-command-events
    (error "RSpec command loop began with unread events: %S" unread-command-events))
  (execute-kbd-macro (kbd keys))
  (when unread-command-events
    (error "RSpec left unread command events: %S" unread-command-events))
  (when (active-minibuffer-window)
    (error "RSpec left an active minibuffer")))

(defun rspec360-test-owned-compilation-buffer ()
  "Return and own the live RSpec compilation buffer."
  (let ((buffer (get-buffer rspec-compilation-buffer-name-base)))
    (unless buffer (error "RSpec did not create its compilation buffer"))
    (rspec360-test-own-buffer buffer)))

(defun rspec360-test-wait (world buffer expected-finish-count)
  "Wait for BUFFER's real compilation sentinel and stable parsed state."
  (let ((deadline (+ (float-time) 30)) process first second third)
    (setq process (get-buffer-process buffer))
    (unless process (error "RSpec compilation process was never attached"))
    (cl-pushnew process rspec360-test-owned-processes :test #'eq)
    (while (and (< (float-time) deadline)
                (or (process-live-p process)
                    (get-buffer-process buffer)
                    (< (length rspec360-test-finish-events) expected-finish-count)
                    (< (cl-count :after rspec360-test-hook-events
                                 :key #'car :test #'eq)
                       expected-finish-count)))
      (accept-process-output process 0.05))
    (unless (and (not (process-live-p process))
                 (null (get-buffer-process buffer))
                 (= (length rspec360-test-finish-events) expected-finish-count)
                 (= (cl-count :after rspec360-test-hook-events
                              :key #'car :test #'eq)
                    expected-finish-count))
      (error "RSpec compilation did not settle: %S"
             (list :process (process-status process)
                   :buffer-process (get-buffer-process buffer)
                   :finish rspec360-test-finish-events
                   :hooks rspec360-test-hook-events
                   :trace (rspec360-test-lines (plist-get world :trace))
                   :miss (rspec360-test-lines (plist-get world :miss))
                   :tail (with-current-buffer buffer
                           (buffer-substring-no-properties
                            (max (point-min) (- (point-max) 500)) (point-max))))))
    (cl-labels
        ((snapshot
          ()
          (list
           :text
           (with-current-buffer buffer
             (rspec360-test-normalize-string
              world (buffer-substring-no-properties (point-min) (point-max))))
           :finish (copy-tree rspec360-test-finish-events)
           :hooks (copy-tree rspec360-test-hook-events)
           :failed (copy-sequence rspec-last-failed-specs))))
      (setq first (snapshot))
      (accept-process-output process 0.02)
      (setq second (snapshot))
      (accept-process-output process 0.02)
      (setq third (snapshot)))
    (unless (and (equal first second) (equal second third))
      (error "RSpec compilation changed after terminal sentinel: %S"
             (list first second third)))
    (list :process (process-status process)
          :buffer-process (get-buffer-process buffer)
          :finish (plist-get third :finish)
          :hooks (plist-get third :hooks)
          :failed (plist-get third :failed)
          :text (plist-get third :text))))

(defun rspec360-test-compilation-observations (buffer patterns)
  "Return exact applied style and navigation runs at PATTERNS in BUFFER."
  (with-current-buffer buffer
    (compilation--ensure-parse (point-max))
    (font-lock-ensure)
    (mapcar
     (lambda (pattern)
       (goto-char (point-min))
       (unless (search-forward pattern nil t)
         (error "RSpec compilation lacks observation: %S" pattern))
       (let* ((start (line-beginning-position))
              (end (line-end-position))
              (position start)
              runs)
         (while (< position end)
           (let* ((next
                   (apply #'min
                          (mapcar
                           (lambda (property)
                             (next-single-property-change
                              position property nil end))
                           '(face font-lock-face compilation-message))))
                  (message (get-text-property position 'compilation-message))
                  (location (and message (compilation--message->loc message)))
                  (file-structure
                   (and location (compilation--loc->file-struct location))))
             (push
              (list :columns
                    (cons (- position start) (- next start))
                    :text (buffer-substring-no-properties position next)
                    :face (get-text-property position 'face)
                    :font-lock-face
                    (get-text-property position 'font-lock-face)
                    :message
                    (and message
                         (list :type (compilation--message->type message)
                               :rule (compilation--message->rule message)
                               :file (and file-structure
                                          (caar file-structure))
                               :line (and location
                                          (compilation--loc->line location))
                               :column (and location
                                            (compilation--loc->col location)))))
              runs)
             (setq position next)))
         (list
          :pattern pattern :line (line-number-at-pos start)
          :runs (nreverse runs)
          :overlays
          (sort
           (cl-loop for overlay in (overlays-in start (min (point-max) (1+ end)))
                    for overlay-start = (overlay-start overlay)
                    for overlay-end = (overlay-end overlay)
                    for face = (overlay-get overlay 'face)
                    when (and face (< overlay-start end) (< start overlay-end))
                    collect
                    (list :columns
                          (cons (- (max start overlay-start) start)
                                (- (min end overlay-end) start))
                          :face face
                          :font-lock-face (overlay-get overlay 'font-lock-face)
                          :priority (overlay-get overlay 'priority)))
           (lambda (left right)
             (string-lessp (prin1-to-string left)
                           (prin1-to-string right)))))))
     patterns)))

(defun rspec360-test-invocations (world)
  "Return accepted replay calls oldest first."
  (rspec360-test-lines (plist-get world :trace)))

(defun rspec360-test-misses (world)
  "Return rejected replay calls oldest first."
  (rspec360-test-lines (plist-get world :miss)))

(defun rspec360-test-condition (thunk)
  "Call THUNK and return its exact condition without opaque objects."
  (condition-case condition
      (list :value (funcall thunk))
    (error (list :signal (car condition) :data (cdr condition)
                 :message (substring-no-properties
                           (error-message-string condition))))))

(defun rspec360-test-relative-state (world buffer)
  "Return stable file/mode/point state for BUFFER in WORLD."
  (with-current-buffer buffer
    (list :file (file-relative-name buffer-file-name (plist-get world :project))
          :major major-mode :rspec rspec-mode :verifiable rspec-verifiable-mode
          :line (line-number-at-pos) :point (point)
          :text (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position)))))

(defun rspec360-test-index-positions (index)
  "Return imenu INDEX with markers replaced by exact positions."
  (mapcar
   (lambda (entry)
     (cond ((not (consp entry)) entry)
           ((markerp (cdr entry))
            (cons (car entry) (marker-position (cdr entry))))
           ((listp (cdr entry))
            (cons (car entry) (rspec360-test-index-positions (cdr entry))))
           (t entry)))
   index))

(defun rspec360-test-navigation-state (world)
  "Return the selected file location after public error navigation."
  (let* ((window (selected-window))
         (buffer (window-buffer window))
         (position (window-point window)))
    (rspec360-test-own-buffer buffer)
    (with-current-buffer buffer
      (goto-char position)
      (if buffer-file-name
          (list :file (file-relative-name buffer-file-name
                                          (plist-get world :project))
                :major major-mode :line (line-number-at-pos) :point (point)
                :text (if (string-equal (file-name-extension buffer-file-name)
                                         "png")
                          :binary-image
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position))))
        (list :buffer (buffer-name buffer) :major major-mode :point (point))))))

(defun rspec360-test-run (case-name thunk)
  "Run THUNK in one owned, reversible CASE-NAME world."
  (unless (string-match-p "\\`[a-z0-9-]+\\'" case-name)
    (error "RSpec invalid case name: %S" case-name))
  (let ((source (symbol-file 'rspec-verify 'defun)))
    (unless (and (featurep 'rspec-mode) source
                 (string-suffix-p "/rspec-mode.el" source)
                 (package-built-in-p 'ruby-mode '(1 0))
                 (package-built-in-p 'cl-lib '(0 4))
                 (equal load-suffixes '(".el")))
      (error "RSpec activation boundary failed: %S"
             (list (featurep 'rspec-mode) source
                   (package-built-in-p 'ruby-mode '(1 0))
                   (package-built-in-p 'cl-lib '(0 4)) load-suffixes))))
  (let* ((buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-timers-before (copy-sequence timer-idle-list))
         (current-buffer-before (current-buffer))
         (selected-window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (rspec360-test-window-state))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (rspec360-test-variable-state symbol)))
                  rspec360-test-state-symbols))
         (exec-path-before exec-path)
         (process-environment-before process-environment)
         (process-environment-value-before (copy-tree process-environment))
         (kill-ring-before kill-ring)
         (kill-pointer-before kill-ring-yank-pointer)
         (rspec360-test-owned-buffers nil)
         (rspec360-test-owned-processes nil)
         (rspec360-test-hook-events nil)
         (rspec360-test-finish-events nil)
         world body-value body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (setq world (rspec360-test-allocate-world case-name))
              (rspec360-test-materialize-world world)
              (rspec360-test-configure world)
              (setq body-value (funcall thunk world)))
          (t (setq body-error condition)))
      (cl-labels
          ((attempt (phase function)
             (condition-case condition (funcall function)
               (t (push (list phase condition) cleanup-errors))))
           (sweep (number)
             (dolist (process (seq-difference (process-list) processes-before #'eq))
               (attempt (list 'process number)
                        (lambda ()
                          (set-process-query-on-exit-flag process nil)
                          (when (process-live-p process) (delete-process process))
                          (let ((deadline (+ (float-time) 2)))
                            (while (and (process-live-p process)
                                        (< (float-time) deadline))
                              (accept-process-output process 0.05)))
                          (when (process-live-p process)
                            (error "RSpec owned process survived cleanup: %S"
                                   process)))))
             (dolist (timer
                      (delete-dups
                       (append (seq-difference timer-list timers-before #'eq)
                               (seq-difference timer-idle-list idle-timers-before #'eq))))
               (attempt (list 'timer number) (lambda () (cancel-timer timer))))
             (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
               (attempt (list 'buffer number)
                        (lambda ()
                          (when (buffer-live-p buffer)
                            (with-current-buffer buffer (set-buffer-modified-p nil))
                            (kill-buffer buffer)))))))
        (attempt 'window-first
                 (lambda ()
                   (rspec360-test-restore-windows
                    configuration-before windows-before)))
        (dotimes (number 2) (sweep number))
        (dolist (entry states-before)
          (attempt (list 'variable (car entry))
                   (lambda ()
                     (rspec360-test-restore-variable (car entry) (cdr entry)))))
        (attempt 'paths
                 (lambda ()
                   (setq exec-path exec-path-before
                         process-environment process-environment-before
                         kill-ring kill-ring-before
                         kill-ring-yank-pointer kill-pointer-before)))
        (attempt 'window-final
                 (lambda ()
                   (rspec360-test-restore-windows
                    configuration-before windows-before)))
        (attempt 'select-baseline
                 (lambda ()
                   (unless (and (buffer-live-p current-buffer-before)
                                (window-live-p selected-window-before))
                     (error "RSpec baseline selection died"))
                   (select-window selected-window-before)
                   (set-buffer current-buffer-before)))
        (when world
          (attempt
           'delete-root
           (lambda ()
             (let* ((root (plist-get world :root))
                    (owner (plist-get world :owner))
                    (true-root (file-name-as-directory (file-truename root))))
               (unless (and (file-name-absolute-p root)
                            (file-directory-p root)
                            (not (equal true-root owner))
                            (string-prefix-p owner true-root))
                 (error "RSpec refuses unsafe root deletion: %S" (list owner root)))
               (delete-directory root t)))))
        ;; Unicode root deletion may lazily create GNU's internal coding work
        ;; buffer.  It is post-baseline state and remains case-owned.
        (sweep 'after-root)))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let ((cleanup-state
           (list :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
                 :new-processes (seq-difference (process-list) processes-before #'eq)
                 :new-timers
                 (delete-dups
                  (append (seq-difference timer-list timers-before #'eq)
                          (seq-difference timer-idle-list idle-timers-before #'eq)))
                 :windows (equal (rspec360-test-window-state) windows-before)
                 :configuration
                 (compare-window-configurations
                  (current-window-configuration) configuration-before)
                 :buffer (eq (current-buffer) current-buffer-before)
                 :window (eq (selected-window) selected-window-before)
                 :variables
                 (cl-every
                  (lambda (entry)
                    (equal (rspec360-test-variable-state (car entry)) (cdr entry)))
                  states-before)
                 :paths (and (eq exec-path exec-path-before)
                             (eq process-environment process-environment-before)
                             (equal process-environment
                                    process-environment-value-before))
                 :kill (and (eq kill-ring kill-ring-before)
                            (eq kill-ring-yank-pointer kill-pointer-before))
                 :root (and world (not (file-exists-p (plist-get world :root))))
                 :body-error body-error :cleanup-errors cleanup-errors)))
      (unless (and (null (plist-get cleanup-state :new-buffers))
                   (null (plist-get cleanup-state :new-processes))
                   (null (plist-get cleanup-state :new-timers))
                   (plist-get cleanup-state :windows)
                   (plist-get cleanup-state :configuration)
                   (plist-get cleanup-state :buffer)
                   (plist-get cleanup-state :window)
                   (plist-get cleanup-state :variables)
                   (plist-get cleanup-state :paths)
                   (plist-get cleanup-state :kill)
                   (plist-get cleanup-state :root)
                   (null body-error) (null cleanup-errors))
        (error "RSpec workflow/cleanup failure: %S" cleanup-state))
      (list :result (rspec360-test-normalize world body-value)
            :cleanup 'clean))))
"####;

fn rspec_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RSPEC_MODE_MELPA_PIN, "rspec-mode.el")
        .expect("prepare exact shallow RSpec Mode source below ./tmp")
        .with_prelude(RSPEC_MODE_TEST_PRELUDE)
        .with_timeout(RSPEC_MODE_TEST_TIMEOUT)
}

#[test]
fn rspec_mode_package_batch() {
    assert_oracle_batch_cases(
        rspec_mode_oracle(),
        "rspec-mode-package-batch",
        "RSpec Mode",
        &workflows::workflow_batch_cases(),
    );
}
