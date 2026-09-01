use std::time::Duration;

use crate::{CachedMelpaOracle, SCALA_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SCALA_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'imenu)
(require 'compile)
(require 'ansi-color)
(require 'scala-mode)
(require 'scala-organise)
(require 'scala-compile)

;; The pinned mode adds its previous-empty-line cleanup to the default/global
;; post-command hook on first activation.  Establish that irreversible package
;; baseline once so shared and isolated cases begin identically.
(with-temp-buffer (scala-mode))

;; GNU batch realizes its reserved menu-bar row on first window restoration.
(set-window-configuration (current-window-configuration))

;; Unicode fixture teardown can lazily create this GNU infrastructure buffer.
(get-buffer-create " *code-conversion-work*")

(defconst scala365-test-source-sha256
  '(("scala-mode.el" . "b6b36c2cc87e9d5fd947c4b47f364f9860c49419dba38575488ab7fc742521a2")
    ("scala-mode-syntax.el" . "ef8a3fa3da75e62262d03914ac5eaa577131158170b69f50121d0b0d3b40b711")
    ("scala-mode-indent.el" . "176ad15a4d8631a7dd7e2c01e150a4bdfcd51dd7cfb93a46f472b5345f267fc9")
    ("scala-mode-fontlock.el" . "06d0da90d49f31e4465748dcd51241b4b8cea5abf58836e368bd59741639e90b")
    ("scala-mode-map.el" . "9bf772541ef638a6da184249517f7bf17cf91a4574defa3b64a714d996cbba67")
    ("scala-mode-prettify-symbols.el" . "897d4debe8966224ab58c7f3bdb332ad65d211e9e75fdc82ea17e1b8ad86e7dc")
    ("scala-mode-lib.el" . "801b3c8c3f9c0ba247d3c60c75575c6babfd4ae73dc6be5ee7884f06d8a3a5b9")
    ("scala-mode-imenu.el" . "55a601e03f24399e14f4ba99b0aed50c7ac0f82ce7c0b91af130840c5a3ae2c6")
    ("scala-mode-paragraph.el" . "5472721bce109c062e93cc4782b59a317b744d652473609d522c3d15d10f6522")
    ("scala-compile.el" . "6275cdc73daec5f42209683cdab39ff5335efe74ce2a7cc888bcb36849ed2c29")
    ("scala-organise.el" . "71df577bed8259d4384f66be4544f97f7e832cc16a18eeec83e0bb0401457be5")))
(defconst scala365-test-installed-root-sha256
  "5ac029f2921d4b72df2e24501213c734aa05b95fd2928b1f07d39cef421dda9d")

(defconst scala365-test-recording-root
  "/home/exec/Projects/github.com/eval-exec/neomacs-windows/tmp/scala-mode-study/sbt-recording/project space 界")
(defconst scala365-test-sbt-tool
  '(:version "1.12.14" :java "21.0.10" :scala "2.13.16"
    :archive-sha256 "cd17daae220ff264faa4251334522444518584f0eb2ee82da01523a9b9002b7e"
    :script-sha256 "0479b7d305132e216bdd0c8aa376f916dc062c4cae010f21625c033b08435715"
    :launcher-sha256 "1750c8fb61c2d2f82da40b2cd9014f4d8a1bd49a361402ea5b1cec061ed66578"
    :sbtn-version "2.0.0-b4d628dd"
    :sbtn-sha256 "4527047664bce3f473f3bc960be888199947c74346a1ea9f717809e8dcefbcc6"))
(defconst scala365-test-failure-stream-sha256
  "ea0b785b102c1ae2348042b4b7815bcb6594e0601354b7ed2fc0f9a90fe0f005")
(defconst scala365-test-success-stream-sha256
  "cf44689cc6c5a78f3ba28bc053a8a3002e7d9062984182095a36d0264a70959b")
(defconst scala365-test-empty-stream-sha256
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
(defconst scala365-test-failure-inventory-sha256
  "1316bdc9c504e7eb4dd4d96bafff1ba4f6c0b9639d46fcdef20ee356e8550258")
(defconst scala365-test-success-inventory-sha256
  "adb70fc691579c749f58b812362b288bbda16ce3a34d7ae8d637b6db482786a4")
(defconst scala365-test-failure-manifest-sha256
  "57c49b00b4fb87028bd0b3027c568a84728b49dfba673c31bc7657d34cfd37a7")
(defconst scala365-test-success-manifest-sha256
  "77de301152e78576c170ff1cf9b389cf62f66c66ce48ed1b5a1b7b862d8ec44b")
(defconst scala365-test-build-sbt-sha256
  "4d6cf634af803906e930a6ae4d211026a00214e7c071842e3bbf381ba49b12f2")
(defconst scala365-test-build-properties-sha256
  "90ce0399257492491ae3573c0f0336cdec886f40c38f8f0df3ed4fe1850f5ef9")
(defconst scala365-test-warnings-sha256
  "99567883f5c8579236d5562e3ff3da3c7f23dfcd6caf2857169a3317b3fdfe9d")

(defconst scala365-test-failure-stream-base64
  "G1swbVsbWzBtG1swbWluZm8bWzBtXSAbWzBtG1swbXdlbGNvbWUgdG8gc2J0IDEuMTIuMTQgKE4vQSBKYXZhIDIxLjAuMTApG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtbG9hZGluZyBwcm9qZWN0IGRlZmluaXRpb24gZnJvbSAvaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3Qgc3BhY2Ug55WML3Byb2plY3QbWzBtChtbMG1bG1swbRtbMG1pbmZvG1swbV0gG1swbRtbMG1sb2FkaW5nIHNldHRpbmdzIGZvciBwcm9qZWN0IHByb2plY3Qtc3BhY2UtIGZyb20gYnVpbGQuc2J0Li4uG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtc2V0IGN1cnJlbnQgcHJvamVjdCB0byBwcm9qZWN0LXNwYWNlLSAoaW4gYnVpbGQgZmlsZTovaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3QlMjBzcGFjZSUyMOeVjC8pG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtRXhlY3V0aW5nIGluIGJhdGNoIG1vZGUuIEZvciBiZXR0ZXIgcGVyZm9ybWFuY2UgdXNlIHNidCdzIHNoZWxsG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtY29tcGlsaW5nIDIgU2NhbGEgc291cmNlcyB0byAvaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3Qgc3BhY2Ug55WML3RhcmdldC9zY2FsYS0yLjEzL2NsYXNzZXMgLi4uG1swbQobWzBtWxtbMG0bWzMxbWVycm9yG1swbV0gG1swbRtbMG0vaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3Qgc3BhY2Ug55WML3NyYy9tYWluL3NjYWxhL0ludmVudG9yeS5zY2FsYTozOjE2OiBub3QgZm91bmQ6IHZhbHVlIG1pc3NpbmcbWzBtChtbMG1bG1swbRtbMzFtZXJyb3IbWzBtXSAbWzBtG1swbSAgdmFsIGJyb2tlbiA9IG1pc3NpbmcbWzBtChtbMG1bG1swbRtbMzFtZXJyb3IbWzBtXSAbWzBtG1swbSAgICAgICAgICAgICAgIF4bWzBtChtbMG1bG1swbRtbMzFtZXJyb3IbWzBtXSAbWzBtG1swbW9uZSBlcnJvciBmb3VuZBtbMG0KG1swbVsbWzBtG1szMW1lcnJvchtbMG1dIBtbMG0bWzBtKENvbXBpbGUgLyAbWzMxbWNvbXBpbGVJbmNyZW1lbnRhbBtbMG0pIENvbXBpbGF0aW9uIGZhaWxlZBtbMG0KG1swbVsbWzBtG1szMW1lcnJvchtbMG1dIBtbMG0bWzBtVG90YWwgdGltZTogOCBzLCBjb21wbGV0ZWQgQXVnIDExLCAyMDI2LCA0OjIxOjA4IFBNG1swbQobWzBK")
(defconst scala365-test-success-stream-base64
  "G1swbVsbWzBtG1swbWluZm8bWzBtXSAbWzBtG1swbXdlbGNvbWUgdG8gc2J0IDEuMTIuMTQgKE4vQSBKYXZhIDIxLjAuMTApG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtbG9hZGluZyBwcm9qZWN0IGRlZmluaXRpb24gZnJvbSAvaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3Qgc3BhY2Ug55WML3Byb2plY3QbWzBtChtbMG1bG1swbRtbMG1pbmZvG1swbV0gG1swbRtbMG1sb2FkaW5nIHNldHRpbmdzIGZvciBwcm9qZWN0IHByb2plY3Qtc3BhY2UtIGZyb20gYnVpbGQuc2J0Li4uG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtc2V0IGN1cnJlbnQgcHJvamVjdCB0byBwcm9qZWN0LXNwYWNlLSAoaW4gYnVpbGQgZmlsZTovaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3QlMjBzcGFjZSUyMOeVjC8pG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtRXhlY3V0aW5nIGluIGJhdGNoIG1vZGUuIEZvciBiZXR0ZXIgcGVyZm9ybWFuY2UgdXNlIHNidCdzIHNoZWxsG1swbQobWzBtWxtbMG0bWzBtaW5mbxtbMG1dIBtbMG0bWzBtY29tcGlsaW5nIDIgU2NhbGEgc291cmNlcyB0byAvaG9tZS9leGVjL1Byb2plY3RzL2dpdGh1Yi5jb20vZXZhbC1leGVjL25lb21hY3Mtd2luZG93cy90bXAvc2NhbGEtbW9kZS1zdHVkeS9zYnQtcmVjb3JkaW5nL3Byb2plY3Qgc3BhY2Ug55WML3RhcmdldC9zY2FsYS0yLjEzL2NsYXNzZXMgLi4uG1swbQobWzBtWxtbMG0bWzMzbXdhcm4bWzBtXSAbWzBtG1swbS9ob21lL2V4ZWMvUHJvamVjdHMvZ2l0aHViLmNvbS9ldmFsLWV4ZWMvbmVvbWFjcy13aW5kb3dzL3RtcC9zY2FsYS1tb2RlLXN0dWR5L3NidC1yZWNvcmRpbmcvcHJvamVjdCBzcGFjZSDnlYwvc3JjL21haW4vc2NhbGEvV2FybmluZ3Muc2NhbGE6NDoxNzogbWV0aG9kIGxlZ2FjeSBpbiBvYmplY3QgV2FybmluZ3MgaXMgZGVwcmVjYXRlZCAoc2luY2UgMSk6IHVzZSBjdXJyZW50G1swbQobWzBtWxtbMG0bWzMzbXdhcm4bWzBtXSAbWzBtG1swbSAgdmFsIHdhcm5pbmcgPSBsZWdhY3kbWzBtChtbMG1bG1swbRtbMzNtd2FybhtbMG1dIBtbMG0bWzBtICAgICAgICAgICAgICAgIF4bWzBtChtbMG1bG1swbRtbMzNtd2FybhtbMG1dIBtbMG0bWzBtb25lIHdhcm5pbmcgZm91bmQbWzBtChtbMG1bG1swbRtbMG1pbmZvG1swbV0gG1swbRtbMG1kb25lIGNvbXBpbGluZxtbMG0KG1swbVsbWzBtG1szMm1zdWNjZXNzG1swbV0gG1swbRtbMG1Ub3RhbCB0aW1lOiAzIHMsIGNvbXBsZXRlZCBBdWcgMTEsIDIwMjYsIDQ6MjE6NDYgUE0bWzBtChtbMEo=")

(defconst scala365-test-build-sbt
  "ThisBuild / scalaVersion := \"2.13.16\"\n\nThisBuild / scalacOptions ++= Seq(\"-deprecation\", \"-feature\")\n\n")
(defconst scala365-test-build-properties "sbt.version=1.12.14\n\n")
(defconst scala365-test-failure-inventory
  "object Inventory {\n  val okay = 1\n  val broken = missing\n}\n\n")
(defconst scala365-test-success-inventory
  "object Inventory {\n  val okay = 1\n  val recovered = okay + 1\n}\n")
(defconst scala365-test-warnings-source
  "object Warnings {\n  @deprecated(\"use current\", \"1\") def legacy = 1\n  val okay = 2\n  val warning = legacy\n}\n\n")

(defconst scala365-test-state-symbols
  '(auto-mode-alist file-coding-system-alist post-command-hook
    scala-indent:step scala-indent:indent-value-expression
    scala-indent:align-parameters scala-indent:align-forms
    scala-indent:default-run-on-strategy
    scala-indent:add-space-for-scaladoc-asterisk
    scala-indent:use-javadoc-style
    scala-imenu:should-flatten-index scala-imenu:build-imenu-candidate
    scala-imenu:cleanup-hooks scala-organise-first
    scala-prettify-symbols-alist scala-font-lock:constant-list
    scala--compile-history scala-compile-always-ask
    scala-compile-suggestion scala-compile-alt scala--compile-project
    compilation-in-progress compilation-last-buffer next-error-last-buffer
    compilation-arguments compilation-directory compilation-environment
    compilation-auto-jump-to-first-error compilation-scroll-output
    compilation-ask-about-save compilation-save-buffers-predicate
    compilation-finish-functions shell-command-history
    process-environment exec-path default-directory shell-file-name
    explicit-shell-file-name
    minibuffer-history file-name-history extended-command-history
    command-history kill-ring kill-ring-yank-pointer
    interprogram-cut-function interprogram-paste-function
    suggest-key-bindings execute-extended-command--binding-timer
    global-mark-ring mark-ring imenu--history-list
    unread-command-events executing-kbd-macro this-command real-this-command
    last-command real-last-command last-command-event last-input-event
    current-prefix-arg prefix-arg deactivate-mark
    enable-local-variables enable-local-eval enable-dir-local-variables
    create-lockfiles vc-handled-backends
    undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers)
  "Mutable editor and package state restored after every Scala story.")

(defconst scala365-test-terminal-state-symbols
  '(undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers))

(defconst scala365-test-forbidden-external-functions
  '(call-process call-process-region process-file
    make-network-process open-network-stream
    url-retrieve url-retrieve-synchronously))

(defvar scala365-test-world nil)
(defvar scala365-test-external-events nil)
(defvar scala365-test-external-advices nil)
(defvar scala365-test-command-hook-installed nil)
(defvar scala365-test-process-records nil)
(defvar scala365-test-message-events nil)
(defvar scala365-test-read-events nil)
(defvar scala365-test-completion-events nil)
(defvar scala365-test-expected-reads nil)
(defvar scala365-test-expected-completions nil)
(defvar scala365-test-command-events nil)
(defvar scala365-test-watch-commands nil)
(defvar scala365-test-compile-phase nil)
(defvar scala365-test-minibuffer-initial nil)
(defvar scala365-test-minibuffer-final nil)
(defvar scala365-test-parked-outputs nil)
(defvar scala365-test-owned-markers nil)
(defvar scala365-test-body-stage nil)

(defun scala365-test-variable-state (symbol)
  (if (boundp symbol)
      (list :bound t :value (symbol-value symbol))
    '(:bound nil)))

(defun scala365-test-restore-variable (symbol state)
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun scala365-test-variable-restored-p (symbol state)
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value)))
    (not (boundp symbol))))

(defun scala365-test-window-state ()
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :point (window-point window)
           :start (window-start window) :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :prev (copy-tree (window-prev-buffers window))
           :next (copy-tree (window-next-buffers window))
           :dedicated (window-dedicated-p window)
           :parameters
           (copy-tree (seq-filter (lambda (entry) (cdr entry))
                                  (window-parameters window)))))
   (window-list nil 'no-minibuf)))

(defun scala365-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Scala Mode baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-dedicated-p window (plist-get entry :dedicated))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter (plist-get entry :parameters))
        (set-window-parameter window (car parameter) (cdr parameter)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun scala365-test-buffer-content-state (name)
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((minimum (point-min)) (maximum (point-max)))
        (list :buffer buffer
              :text (save-restriction (widen) (buffer-string))
              :point (point) :modified (buffer-modified-p)
              :undo (copy-tree buffer-undo-list) :read-only buffer-read-only
              :min minimum :max maximum)))))

(defun scala365-test-restore-buffer-content (state)
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Scala Mode baseline buffer died: %S" buffer))
      (with-current-buffer buffer
        (let ((inhibit-read-only t))
          (widen) (erase-buffer) (insert (plist-get state :text)))
        (goto-char (min (plist-get state :point) (point-max)))
        (setq buffer-undo-list (copy-tree (plist-get state :undo))
              buffer-read-only (plist-get state :read-only))
        (set-buffer-modified-p (plist-get state :modified))
        (narrow-to-region (plist-get state :min) (plist-get state :max))))))

(defun scala365-test-buffer-content-restored-p (state)
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (save-restriction (widen) (buffer-string))
                           (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (equal buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))
                    (= (point-min) (plist-get state :min))
                    (= (point-max) (plist-get state :max))))))))

(defun scala365-test-park-output-buffers ()
  (let ((names '("*Completions*" "*compilation*")))
    (when scala365-test-world
      (push (format "*scala-compilation-%s*"
                    (file-name-nondirectory
                     (directory-file-name
                      (plist-get scala365-test-world :root))))
            names))
    (dolist (name (delete-dups names))
      (when-let* ((buffer (get-buffer name)))
        (push (cons buffer name) scala365-test-parked-outputs)
        (with-current-buffer buffer
          (rename-buffer (generate-new-buffer-name
                          (format " *scala365 baseline %s*" name)) t))))))

(defun scala365-test-restore-output-buffer (entry)
  (unless (buffer-live-p (car entry))
    (error "Scala Mode parked output buffer died: %S" entry))
  (when-let* ((replacement (get-buffer (cdr entry))))
    (unless (eq replacement (car entry))
      (error "Scala Mode output replacement survived: %S" (cdr entry))))
  (with-current-buffer (car entry) (rename-buffer (cdr entry) t)))

(defun scala365-test-normalize-string (value)
  (if (not (stringp value)) value
    (let* ((root (and scala365-test-world
                      (plist-get scala365-test-world :root)))
           (external-root
            (and scala365-test-world
                 (plist-get scala365-test-world :external-root)))
           (encoded-root
            (and external-root
                 (replace-regexp-in-string " " "%20" external-root t t)))
           (shell (and scala365-test-world
                       (plist-get scala365-test-world :shell)))
           (normalized value))
      (when root
        (setq normalized
              (replace-regexp-in-string
               (regexp-quote root) "[ROOT]/" normalized t t)))
      (when (and external-root (equal normalized external-root))
        (setq normalized "[ROOT]"))
      (when (and encoded-root (not (equal encoded-root external-root)))
        (setq normalized
              (replace-regexp-in-string
               (regexp-quote encoded-root) "[ROOT-URI]" normalized t t)))
      (when shell
        (setq normalized
              (replace-regexp-in-string
               (regexp-quote shell) "[SHELL]" normalized t t)))
      normalized)))

(defun scala365-test-stable-datum (datum)
  (cond ((bufferp datum) (list :buffer (buffer-name datum)))
        ((markerp datum)
         (list :marker
               (and (marker-buffer datum)
                    (file-name-nondirectory
                     (or (buffer-file-name (marker-buffer datum))
                         (buffer-name (marker-buffer datum)))))
               (marker-position datum)))
        ((stringp datum) (scala365-test-normalize-string datum))
        ((consp datum) (mapcar #'scala365-test-stable-datum datum))
        (t datum)))

(defun scala365-test-condition-state (condition)
  (list :symbol (car condition)
        :data (mapcar #'scala365-test-stable-datum (cdr condition))
        :message
        (replace-regexp-in-string
         "#<buffer \\([^>]+\\)>" "[BUFFER:\\1]"
         (scala365-test-normalize-string (error-message-string condition))
         t nil)))

(defun scala365-test-attempt (phase thunk errors)
  (condition-case condition
      (progn (funcall thunk) errors)
    (t (cons (list phase (scala365-test-condition-state condition)) errors))))

(defun scala365-test-path (relative)
  (unless scala365-test-world (error "Scala Mode has no active world"))
  (expand-file-name relative (plist-get scala365-test-world :root)))

(defun scala365-test-file-bytes (path)
  (if (not (file-exists-p path)) :missing
    (let ((coding-system-for-read 'utf-8-unix))
      (with-temp-buffer
        (insert-file-contents path)
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun scala365-test-file-sha256 (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun scala365-test-string-sha256 (text)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert (encode-coding-string text 'utf-8-unix t))
    (secure-hash 'sha256 (current-buffer))))

(defun scala365-test-write (relative text)
  (let ((path (scala365-test-path relative))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-file path (insert text))
    (unless (equal (scala365-test-file-bytes path) text)
      (error "Scala Mode fixture write mismatch: %S" relative))
    path))

(defun scala365-test-new-timers (timers-before idle-before)
  (seq-difference (append timer-list timer-idle-list)
                  (append timers-before idle-before) #'eq))

(defun scala365-test-allocate-world (case-name root-profile)
  (unless (string-match-p "\\`[a-z0-9_-]+\\'" case-name)
    (error "Scala Mode invalid case name: %S" case-name))
  (let ((raw-workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
        (raw-owner (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
        (raw-tmp (getenv "TMPDIR"))
        (shell (executable-find "bash")))
    (dolist (entry `((:workspace . ,raw-workspace)
                     (:owner . ,raw-owner) (:tmp . ,raw-tmp)))
      (unless (and (stringp (cdr entry)) (not (string-empty-p (cdr entry)))
                   (file-name-absolute-p (cdr entry))
                   (file-directory-p (cdr entry)))
        (error "Scala Mode unsafe scratch input: %S" entry)))
    (unless (and shell (file-name-absolute-p shell) (file-executable-p shell))
      (error "Scala Mode requires an absolute real bash: %S" shell))
    (let* ((workspace (file-name-as-directory (file-truename raw-workspace)))
           (workspace-tmp
            (file-name-as-directory
             (file-truename (expand-file-name "tmp/" workspace))))
           (owner (file-name-as-directory (file-truename raw-owner)))
           (tmp (file-name-as-directory (file-truename raw-tmp)))
           (compile-case
            (pcase root-profile
              (:space-unicode nil)
              (:compile-no-space-unicode t)
              (_ (error "Scala Mode unknown root profile: %S" root-profile))))
           (root (expand-file-name
                  (if compile-case
                      (format "scala-mode365-%s-界/" case-name)
                    (format "scala-mode365-%s project 界/" case-name))
                  owner)))
      (unless (and (file-in-directory-p owner workspace-tmp)
                   (file-in-directory-p tmp workspace-tmp))
        (error "Scala Mode scratch paths escape workspace tmp: %S"
               (list workspace-tmp owner tmp)))
      (unless (and (file-name-absolute-p root) (not (equal owner root))
                   (string-prefix-p owner root) (not (file-exists-p root)))
        (error "Scala Mode refuses owned root: %S" (list owner root)))
      (unless (if compile-case
                  (and (string-match-p "界" root)
                       (not (string-match-p "[[:space:]]" root)))
                (and (string-match-p "界" root)
                     (string-match-p "[[:space:]]" root)))
        (error
         (concat "Scala Mode root shape violates pinned compilation regexp "
                 "whitespace boundary: %S")
         root))
      (make-directory root)
      (list :workspace workspace :owner owner :tmp tmp :root root
            :root-profile root-profile
            :external-root (directory-file-name root)
            :shell (file-truename shell)
            :home (expand-file-name "home/" root)
            :bin (expand-file-name "bin/" root)
            :replay (expand-file-name "bin/sbt" root)
            :invocations (expand-file-name "invocations.bin" root)
            :misses (expand-file-name "misses.bin" root)))))

(defun scala365-test-decoded-stream (encoded digest)
  (let ((stream (base64-decode-string encoded)))
    (unless (equal (scala365-test-string-sha256 stream) digest)
      (error "Scala Mode recorded stream digest mismatch: %S" digest))
    (decode-coding-string stream 'utf-8-unix t)))

(defun scala365-test-rewrite-recording-root (stream runtime-root)
  (let* ((recorded-encoded
          (replace-regexp-in-string " " "%20" scala365-test-recording-root t t))
         (runtime-encoded
          (replace-regexp-in-string " " "%20" runtime-root t t))
         (raw-count 0)
         (encoded-count 0)
         (rewritten
          (replace-regexp-in-string
           (regexp-quote scala365-test-recording-root)
           (lambda (_match) (setq raw-count (1+ raw-count)) runtime-root)
           stream t t)))
    (setq rewritten
          (replace-regexp-in-string
           (regexp-quote recorded-encoded)
           (lambda (_match)
             (setq encoded-count (1+ encoded-count)) runtime-encoded)
           rewritten t t))
    (unless (and (= raw-count 3) (= encoded-count 1)
                 (not (string-match-p
                       (regexp-quote scala365-test-recording-root) rewritten))
                 (not (string-match-p (regexp-quote recorded-encoded) rewritten))
                 (not (string-match-p
                       (regexp-quote (concat runtime-root "//")) rewritten)))
      (error "Scala Mode recorded-root rewrite drifted: %S"
             (list raw-count encoded-count runtime-root)))
    rewritten))

(defun scala365-test-create-replay ()
  (let* ((root (plist-get scala365-test-world :external-root))
         (shell (plist-get scala365-test-world :shell))
         (replay (plist-get scala365-test-world :replay))
         (invocations (plist-get scala365-test-world :invocations))
         (misses (plist-get scala365-test-world :misses))
         (failure
          (scala365-test-rewrite-recording-root
           (scala365-test-decoded-stream
            scala365-test-failure-stream-base64
            scala365-test-failure-stream-sha256)
           root))
         (success
          (scala365-test-rewrite-recording-root
           (scala365-test-decoded-stream
            scala365-test-success-stream-base64
            scala365-test-success-stream-sha256)
           root))
         (script
          (format
           (concat "#!%s\nset -u\n"
                   "printf '%%s\\0' \"$PWD\" \"$@\" >> %s\n"
                   "bad () { printf 'UNRECORDED:%%s\\n' \"$1\" >> %s; exit 86; }\n"
                   "[[ $# = 2 && $1 = --batch && $2 = compile ]] || bad argv\n"
                   "[[ $PWD = %s ]] || bad cwd\n"
                   "[[ ${SCALA365_PROJECT_ROOT-} = %s ]] || bad root-env\n"
                   "[[ ${HOME-} = %s ]] || bad home\n"
                   "[[ ${USER-} = scala365 && ${LOGNAME-} = scala365 ]] || bad user\n"
                   "[[ ${LANG-} = C.UTF-8 && ${LC_ALL-} = C.UTF-8 ]] || bad locale\n"
                   "[[ ${TERM-} = xterm-256color ]] || bad term\n"
                   "[[ ${XDG_CACHE_HOME-} = %s ]] || bad xdg\n"
                   "[[ ${COURSIER_CACHE-} = %s ]] || bad coursier\n"
                   "[[ ${SBT_OPTS-} = %s ]] || bad sbt-opts\n"
                   "IFS= read -r -d '' bytes < build.sbt || :\n"
                   "[[ $bytes = %s ]] || bad build\n"
                   "IFS= read -r -d '' bytes < project/build.properties || :\n"
                   "[[ $bytes = %s ]] || bad properties\n"
                   "IFS= read -r -d '' bytes < src/main/scala/Warnings.scala || :\n"
                   "[[ $bytes = %s ]] || bad warnings\n"
                   "IFS= read -r -d '' inventory < src/main/scala/Inventory.scala || :\n"
                   "if [[ $inventory = %s ]]; then printf '%%s' %s; exit 1; fi\n"
                   "if [[ $inventory = %s ]]; then printf '%%s' %s; exit 0; fi\n"
                   "bad inventory\n")
           shell (shell-quote-argument invocations)
           (shell-quote-argument misses) (shell-quote-argument root)
           (shell-quote-argument root)
           (shell-quote-argument (plist-get scala365-test-world :home))
           (shell-quote-argument
            (expand-file-name ".cache/" (plist-get scala365-test-world :home)))
           (shell-quote-argument
            (expand-file-name ".cache/coursier/"
                              (plist-get scala365-test-world :home)))
           (shell-quote-argument (plist-get scala365-test-world :sbt-opts))
           (shell-quote-argument scala365-test-build-sbt)
           (shell-quote-argument scala365-test-build-properties)
           (shell-quote-argument scala365-test-warnings-source)
           (shell-quote-argument scala365-test-failure-inventory)
           (shell-quote-argument failure)
           (shell-quote-argument scala365-test-success-inventory)
           (shell-quote-argument success))))
    (make-directory (file-name-directory replay) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file replay (insert script)))
    (set-file-modes replay #o700)
    (unless (and (file-executable-p replay)
                 (equal (scala365-test-file-bytes replay) script))
      (error "Scala Mode replay materialization mismatch"))
    (setq scala365-test-world
          (plist-put scala365-test-world :replay-sha256
                     (scala365-test-file-sha256 replay)))
    replay))

(defun scala365-test-materialize-compile-fixtures ()
  (scala365-test-write "build.sbt" scala365-test-build-sbt)
  (scala365-test-write "project/build.properties"
                       scala365-test-build-properties)
  (scala365-test-write "src/main/scala/Inventory.scala"
                       scala365-test-failure-inventory)
  (scala365-test-write "src/main/scala/Warnings.scala"
                       scala365-test-warnings-source))

(defun scala365-test-replay-rejection-call (phase directory environment arguments)
  (let ((default-directory directory)
        (process-environment environment))
    (with-temp-buffer
      (list phase
            (apply #'call-process (plist-get scala365-test-world :replay)
                   nil (current-buffer) nil arguments)
            (buffer-substring-no-properties (point-min) (point-max))))))

(defun scala365-test-probe-replay-rejections ()
  (let* ((root (plist-get scala365-test-world :root))
         (subdir (expand-file-name "wrong-cwd/" root))
         (environment (copy-sequence process-environment))
         (wrong-environment (copy-sequence process-environment))
         (build (scala365-test-path "build.sbt"))
         results)
    (make-directory subdir t)
    (setenv "SCALA365_PROJECT_ROOT" "wrong-root" t)
    (setq wrong-environment (copy-sequence process-environment))
    (setenv "SCALA365_PROJECT_ROOT"
            (plist-get scala365-test-world :external-root) t)
    (push (scala365-test-replay-rejection-call
           :argv root environment '("--batch" "test"))
          results)
    (push (scala365-test-replay-rejection-call
           :cwd subdir environment '("--batch" "compile"))
          results)
    (push (scala365-test-replay-rejection-call
           :env root wrong-environment '("--batch" "compile"))
          results)
    (unwind-protect
        (progn
          (scala365-test-write "build.sbt"
                               (concat scala365-test-build-sbt "tampered"))
          (push (scala365-test-replay-rejection-call
                 :manifest root environment '("--batch" "compile"))
                results))
      (scala365-test-write "build.sbt" scala365-test-build-sbt))
    (setq results (nreverse results))
    (let* ((misses (scala365-test-file-bytes
                    (plist-get scala365-test-world :misses)))
           (invocations
            (split-string
             (scala365-test-file-bytes
              (plist-get scala365-test-world :invocations))
             "\0" t)))
      (unless (and
               (equal results
                      '((:argv 86 "") (:cwd 86 "")
                        (:env 86 "") (:manifest 86 "")))
               (equal misses
                      (concat "UNRECORDED:argv\n" "UNRECORDED:cwd\n"
                              "UNRECORDED:root-env\n"
                              "UNRECORDED:build\n"))
               (= (length invocations) 12)
               (equal (scala365-test-project-manifest-sha256)
                      scala365-test-failure-manifest-sha256))
        (error "Scala Mode replay rejection boundary failed: %S"
               (list results misses invocations)))
      (dolist (path (list (plist-get scala365-test-world :misses)
                          (plist-get scala365-test-world :invocations)))
        (when (file-exists-p path) (delete-file path)))
      (setq scala365-test-world
            (plist-put scala365-test-world :replay-rejections results)))))

(defun scala365-test-configure-world ()
  (let* ((home (plist-get scala365-test-world :home))
         (cache (expand-file-name ".cache/" home))
         (coursier (expand-file-name "coursier/" cache))
         (root (plist-get scala365-test-world :external-root))
         (sbt-opts
          (format
           (concat "-Dsbt.global.base=%s/global -Dsbt.boot.directory=%s/boot "
                   "-Dsbt.ivy.home=%s/ivy -Dsbt.coursier.home=%s/coursier "
                   "-Dsbt.supershell=false -Dsbt.color=true "
                   "-Dsbt.log.noformat=false")
           root root root root)))
    (make-directory coursier t)
    (setq scala365-test-world
          (plist-put scala365-test-world :sbt-opts sbt-opts)))
  (setq process-environment nil
        exec-path (list (plist-get scala365-test-world :bin))
        default-directory (plist-get scala365-test-world :root)
        shell-file-name (plist-get scala365-test-world :shell)
        explicit-shell-file-name (plist-get scala365-test-world :shell)
        enable-local-variables nil enable-local-eval nil
        enable-dir-local-variables nil create-lockfiles nil
        vc-handled-backends nil suggest-key-bindings nil
        minibuffer-history nil file-name-history nil imenu--history-list nil
        extended-command-history nil command-history nil
        kill-ring nil kill-ring-yank-pointer nil
        interprogram-cut-function nil interprogram-paste-function nil
        scala--compile-history (copy-sequence scala--compile-history)
        scala--compile-project "build.sbt"
        scala-compile-always-ask nil
        compilation-in-progress nil compilation-last-buffer nil
        next-error-last-buffer nil shell-command-history nil
        compilation-auto-jump-to-first-error nil
        compilation-scroll-output nil compilation-ask-about-save nil
        compilation-save-buffers-predicate nil
        compilation-finish-functions nil
        auto-mode-alist (copy-tree auto-mode-alist)
        file-coding-system-alist (copy-tree file-coding-system-alist)
        post-command-hook (copy-sequence post-command-hook))
  (setenv "HOME" (plist-get scala365-test-world :home))
  (setenv "PATH" (plist-get scala365-test-world :bin))
  (setenv "TMPDIR" (plist-get scala365-test-world :tmp))
  (setenv "LC_ALL" "C.UTF-8")
  (setenv "LANG" "C.UTF-8")
  (setenv "TZ" "UTC")
  (setenv "USER" "scala365")
  (setenv "LOGNAME" "scala365")
  (setenv "TERM" "xterm-256color")
  (setenv "XDG_CACHE_HOME"
          (expand-file-name ".cache/" (plist-get scala365-test-world :home)))
  (setenv "COURSIER_CACHE"
          (expand-file-name ".cache/coursier/"
                            (plist-get scala365-test-world :home)))
  (setenv "SBT_OPTS" (plist-get scala365-test-world :sbt-opts))
  (setenv "SCALA365_PROJECT_ROOT"
          (plist-get scala365-test-world :external-root))
  (setenv "SCALA365_INVOCATION_LOG"
          (plist-get scala365-test-world :invocations))
  (setenv "SCALA365_MISS_LOG" (plist-get scala365-test-world :misses))
  (scala365-test-create-replay)
  (when (eq (plist-get scala365-test-world :root-profile)
            :compile-no-space-unicode)
    (scala365-test-materialize-compile-fixtures)
    (scala365-test-probe-replay-rejections)))

(defun scala365-test-project-manifest-sha256 ()
  (let ((rows
         `(("build.sbt" . ,scala365-test-build-sbt-sha256)
           ("project/build.properties" . ,scala365-test-build-properties-sha256)
           ("src/main/scala/Inventory.scala" .
            ,(scala365-test-file-sha256
              (scala365-test-path "src/main/scala/Inventory.scala")))
           ("src/main/scala/Warnings.scala" . ,scala365-test-warnings-sha256))))
    (dolist (entry rows)
      (unless (equal (scala365-test-file-sha256 (scala365-test-path (car entry)))
                     (cdr entry))
        (error "Scala Mode fixture digest drifted: %S" entry)))
    (scala365-test-string-sha256
     (concat
      (mapconcat (lambda (entry) (format "%s  %s" (cdr entry) (car entry)))
                 rows "\n")
      "\n"))))

(defun scala365-test-message-observer (original format-string &rest arguments)
  (let ((rendered (and format-string
                       (apply #'format-message format-string arguments))))
    (when (and rendered (not (string-empty-p rendered)))
      (push (substring-no-properties
             (scala365-test-normalize-string rendered))
            scala365-test-message-events))
    (apply original format-string arguments)))

(defun scala365-test-completing-read-observer
    (original prompt collection &optional predicate require-match
              initial-input history default inherit-input-method)
  (unless scala365-test-expected-completions
    (error "Unexpected Scala Mode completion: %S" prompt))
  (let* ((entry (pop scala365-test-expected-completions))
         (expected (plist-get entry :input))
         (candidates (all-completions "" collection predicate)))
    (unless (equal (plist-get entry :prompt) prompt)
      (error "Scala Mode completion prompt mismatch: %S" prompt))
    (setq unread-command-events
          (append (string-to-list expected) (list ?\r)))
    (setq scala365-test-minibuffer-initial nil
          scala365-test-minibuffer-final nil)
    (let* ((minibuffer-setup-hook
            (append minibuffer-setup-hook
                    '(scala365-test-minibuffer-setup-observer)))
           (minibuffer-exit-hook
            (append minibuffer-exit-hook
                    '(scala365-test-minibuffer-exit-observer)))
           (answer (funcall original prompt collection predicate require-match
                            initial-input history default inherit-input-method)))
      (unless (equal answer expected)
        (error "Scala Mode completion answer mismatch: %S" answer))
      (push (list :prompt (substring-no-properties prompt)
                  :input expected :require-match require-match
                  :initial scala365-test-minibuffer-initial
                  :final scala365-test-minibuffer-final
                  :candidates
                  (if (string-match-p "M-x" prompt)
                      (list :selected-present
                            (not (null (member expected candidates))))
                    (mapcar #'substring-no-properties candidates))
                  :history-argument (copy-tree history)
                  :history-after (scala365-test-history-state history))
            scala365-test-completion-events)
      answer)))

(defun scala365-test-history-state (history)
  (let ((symbol (cond ((symbolp history) history)
                      ((consp history) (car history)))))
    (and symbol (boundp symbol) (copy-tree (symbol-value symbol)))))

(defun scala365-test-minibuffer-setup-observer ()
  (setq scala365-test-minibuffer-initial
        (buffer-substring-no-properties (minibuffer-prompt-end) (point-max))))

(defun scala365-test-minibuffer-exit-observer ()
  (setq scala365-test-minibuffer-final
        (buffer-substring-no-properties (minibuffer-prompt-end) (point-max))))

(defun scala365-test-read-shell-command-observer
    (original prompt &rest arguments)
  (unless scala365-test-expected-reads
    (error "Unexpected Scala Mode shell read: %S" prompt))
  (let* ((entry (pop scala365-test-expected-reads))
         (answer-expected (plist-get entry :answer))
         (events (or (plist-get entry :events)
                     (list (list :text answer-expected) '(:keys "RET"))))
         (history (cadr arguments)))
    (unless (equal prompt (plist-get entry :prompt))
      (error "Scala Mode shell prompt mismatch: %S" prompt))
    (setq unread-command-events
          (append (apply #'scala365-test-chunks-to-events events) nil)
          scala365-test-minibuffer-initial nil
          scala365-test-minibuffer-final nil)
    (let* ((minibuffer-setup-hook
            (append minibuffer-setup-hook
                    '(scala365-test-minibuffer-setup-observer)))
           (minibuffer-exit-hook
            (append minibuffer-exit-hook
                    '(scala365-test-minibuffer-exit-observer)))
           (answer (apply original prompt arguments)))
      (unless (equal answer answer-expected)
        (error "Scala Mode shell answer mismatch: %S" answer))
      (push (list :prompt prompt :arguments (copy-tree arguments)
                  :answer answer-expected
                  :initial scala365-test-minibuffer-initial
                  :final scala365-test-minibuffer-final
                  :history-argument (copy-tree history)
                  :history-after (scala365-test-history-state history))
            scala365-test-read-events)
      answer)))

(defun scala365-test-command-observer ()
  (when (memq this-command scala365-test-watch-commands)
    (let* ((window (selected-window))
           (buffer (window-buffer window))
           (root (plist-get scala365-test-world :external-root))
           (file (and (buffer-live-p buffer)
                      (buffer-local-value 'buffer-file-name buffer)))
           (true-file (and file (file-truename file))))
      (unless (and true-file (file-in-directory-p true-file root))
        (error "Scala Mode navigation left the owned project: %S" file))
      (with-current-buffer buffer
        (save-restriction
          (widen)
          (push (list :command this-command
                      :file (file-relative-name true-file root)
                      :point (window-point window)
                      :line (line-number-at-pos (window-point window))
                      :column (save-excursion
                                (goto-char (window-point window))
                                (current-column)))
                scala365-test-command-events))))))

(defun scala365-test-make-process-observer (original &rest arguments)
  (let* ((command (plist-get arguments :command))
         (directory (file-name-as-directory (file-truename default-directory)))
         (root (plist-get scala365-test-world :root))
         (external-root (plist-get scala365-test-world :external-root))
         (shell (plist-get scala365-test-world :shell))
         (phase scala365-test-compile-phase))
    (unless (and phase (equal root directory)
                 (listp command) (equal (car command) shell)
                 (equal (cadr command) "-c")
                 (= (length command) 3)
                 (pcase phase
                   (:failure (equal "sbt --batch compile" (caddr command)))
                   (:success (equal "sbt --batch compile" (caddr command)))
                   (:missing
                    (equal "missing-sbt365 --batch compile" (caddr command)))
                   (_ nil)))
      (push (list :operation 'make-process :phase phase
                  :command (copy-tree command) :cwd directory)
            scala365-test-external-events)
      (error "Unexpected Scala Mode process boundary: %S" arguments))
    (let ((manifest (scala365-test-project-manifest-sha256)))
      (unless (equal manifest
                     (if (eq phase :failure)
                         scala365-test-failure-manifest-sha256
                       scala365-test-success-manifest-sha256))
        (error "Scala Mode fixture manifest drifted: %S" manifest)))
    (let ((identity
           (list
            :digest
            (equal (scala365-test-file-sha256
                    (plist-get scala365-test-world :replay))
                   (plist-get scala365-test-world :replay-sha256))
            :names
            (sort (mapcar (lambda (entry)
                            (car (split-string entry "=" t)))
                          process-environment)
                  #'string<)
            :home (getenv "HOME") :path (getenv "PATH")
            :tmp (getenv "TMPDIR") :lc-all (getenv "LC_ALL")
            :tz (getenv "TZ") :user (getenv "USER")
            :logname (getenv "LOGNAME") :lang (getenv "LANG")
            :term (getenv "TERM") :inside-emacs (getenv "INSIDE_EMACS")
            :pager (getenv "PAGER") :xdg (getenv "XDG_CACHE_HOME")
            :coursier (getenv "COURSIER_CACHE") :sbt (getenv "SBT_OPTS")
            :root (getenv "SCALA365_PROJECT_ROOT")
            :invocations (getenv "SCALA365_INVOCATION_LOG")
            :misses (getenv "SCALA365_MISS_LOG"))))
      (unless
          (and (plist-get identity :digest)
               (equal (plist-get identity :names)
                      '("COURSIER_CACHE" "HOME" "INSIDE_EMACS" "LANG"
                        "LC_ALL" "LOGNAME" "PAGER" "PATH" "SBT_OPTS"
                        "SCALA365_INVOCATION_LOG" "SCALA365_MISS_LOG"
                        "SCALA365_PROJECT_ROOT" "TERM" "TMPDIR" "TZ"
                        "USER" "XDG_CACHE_HOME"))
               (equal (plist-get identity :home)
                      (plist-get scala365-test-world :home))
               (equal (plist-get identity :path)
                      (plist-get scala365-test-world :bin))
               (equal (plist-get identity :tmp)
                      (plist-get scala365-test-world :tmp))
               (equal (plist-get identity :lc-all) "C.UTF-8")
               (equal (plist-get identity :tz) "UTC")
               (equal (plist-get identity :user) "scala365")
               (equal (plist-get identity :logname) "scala365")
               (equal (plist-get identity :lang) "C.UTF-8")
               (equal (plist-get identity :term) "xterm-256color")
               (equal (plist-get identity :inside-emacs)
                      (format "%s,compile" emacs-version))
               (equal (plist-get identity :pager) "")
               (equal (plist-get identity :xdg)
                        (expand-file-name
                         ".cache/" (plist-get scala365-test-world :home)))
               (equal (plist-get identity :coursier)
                        (expand-file-name
                         ".cache/coursier/"
                         (plist-get scala365-test-world :home)))
               (equal (plist-get identity :sbt)
                        (plist-get scala365-test-world :sbt-opts))
               (equal (plist-get identity :root) external-root)
               (equal (plist-get identity :invocations)
                        (plist-get scala365-test-world :invocations))
               (equal (plist-get identity :misses)
                      (plist-get scala365-test-world :misses)))
        (error "Scala Mode replay identity/environment drifted: %S"
               (scala365-test-stable-datum identity))))
    (let ((process (apply original arguments)))
      (push (list :phase phase :program :shell :argv (cdr command)
                  :cwd "[ROOT]/" :process process)
            scala365-test-process-records)
      process)))

(defun scala365-test-reject-external (operation _original &rest arguments)
  (push (list :operation operation :arguments (copy-tree arguments))
        scala365-test-external-events)
  (error "Unexpected Scala Mode external operation: %S %S"
         operation arguments))

(defun scala365-test-install-observers ()
  (dolist (entry '((message . scala365-test-message-observer)
                   (completing-read . scala365-test-completing-read-observer)
                   (read-shell-command . scala365-test-read-shell-command-observer)
                   (make-process . scala365-test-make-process-observer)))
    (advice-add (car entry) :around (cdr entry))
    (push entry scala365-test-external-advices))
  (dolist (function scala365-test-forbidden-external-functions)
    (let* ((observer
            (lambda (original &rest arguments)
              (apply #'scala365-test-reject-external
                     function original arguments))))
      (advice-add function :around observer)
      (push (cons function observer) scala365-test-external-advices)))
  (setq scala365-test-command-hook-installed t)
  (add-hook 'post-command-hook #'scala365-test-command-observer))

(defun scala365-test-remove-observers ()
  (let (survivors errors)
    (when scala365-test-command-hook-installed
      (condition-case condition
          (progn
            (remove-hook 'post-command-hook #'scala365-test-command-observer)
            (if (memq #'scala365-test-command-observer post-command-hook)
                (push '(post-command-hook . survived) errors)
              (setq scala365-test-command-hook-installed nil)))
        (t (push (list 'post-command-hook
                       (scala365-test-condition-state condition))
                 errors))))
    (dolist (entry scala365-test-external-advices)
      (condition-case condition
          (progn
            (advice-remove (car entry) (cdr entry))
            (when (advice-member-p (cdr entry) (car entry))
              (push entry survivors)))
        (t (push (list entry (scala365-test-condition-state condition)) errors)
           (push entry survivors))))
    (setq scala365-test-external-advices survivors)
    (when (or scala365-test-command-hook-installed survivors errors)
      (error "Scala Mode observer cleanup failed: %S"
             (list scala365-test-command-hook-installed survivors errors)))))

(defun scala365-test-run-keys (&rest chunks)
  (dolist (chunk chunks)
    (pcase (car chunk)
      (:keys (execute-kbd-macro (kbd (cadr chunk))))
      (:text (execute-kbd-macro (cadr chunk)))
      (_ (error "Scala Mode invalid key chunk: %S" chunk)))))

(defun scala365-test-run-contiguous (&rest chunks)
  (execute-kbd-macro (apply #'scala365-test-chunks-to-events chunks)))

(defun scala365-test-chunks-to-events (&rest chunks)
  (let ((events []))
    (dolist (chunk chunks)
      (setq events
            (vconcat events
                     (pcase (car chunk)
                       (:keys (kbd (cadr chunk)))
                       (:text (string-to-vector (cadr chunk)))
                       (_ (error "Scala Mode invalid key chunk: %S" chunk))))))
    events))

(defun scala365-test-visit (relative text)
  (let ((path (scala365-test-write relative text)))
    (let ((buffer (find-file-noselect path)))
      (set-window-buffer (selected-window) buffer)
      (select-window (selected-window))
      (with-current-buffer buffer
        (font-lock-ensure)
        (set-buffer-modified-p nil))
      buffer)))

(defun scala365-test-buffer-state (&optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (save-restriction
      (widen)
      (list :mode major-mode :text (buffer-substring-no-properties
                                    (point-min) (point-max))
            :point (point) :line (line-number-at-pos) :column (current-column)
            :mark (and (mark t) (marker-position (mark-marker)))
            :active mark-active :modified (buffer-modified-p)
            :undo (not (null buffer-undo-list)) :narrowed (buffer-narrowed-p)))))

(defun scala365-test-point-state ()
  (list :point (point) :line (line-number-at-pos) :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun scala365-test-selected-window-point-state (expected-buffer)
  (let* ((window (selected-window))
         (buffer (window-buffer window))
         (position (window-point window)))
    (unless (and (buffer-live-p expected-buffer)
                 (eq buffer expected-buffer))
      (error "Scala Mode selected window is not the owned source: %S" buffer))
    (with-current-buffer buffer
      (save-excursion
        (goto-char position)
        (list :buffer (if buffer-file-name
                          (file-name-nondirectory buffer-file-name)
                        (buffer-name buffer))
              :owned-source t
              :point position :line (line-number-at-pos)
              :column (current-column))))))

(defun scala365-test-property-runs (properties &optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (save-restriction
      (widen)
      (let ((position (point-min)) result)
        (while (< position (point-max))
          (let* ((values (mapcar (lambda (property)
                                   (copy-tree
                                    (get-text-property position property)))
                                 properties))
                 (next (or (next-property-change position nil (point-max))
                           (point-max))))
            (when (seq-some #'identity values)
              (push (list position next
                          (buffer-substring-no-properties position next)
                          values)
                    result))
            (setq position next)))
        (nreverse result)))))

(defun scala365-test-local-hooks ()
  (list
   :syntax-local (local-variable-p 'syntax-propertize-extend-region-functions)
   :syntax (copy-sequence syntax-propertize-extend-region-functions)
   :post-self-local (local-variable-p 'post-self-insert-hook)
   :post-self (copy-sequence post-self-insert-hook)
   :post-command-local (local-variable-p 'post-command-hook)
   :post-command (copy-sequence post-command-hook)))

(defun scala365-test-stable-imenu-index (index)
  (mapcar
   (lambda (entry)
     (cond
      ((markerp (cdr entry))
       (push (cdr entry) scala365-test-owned-markers)
       (list (substring-no-properties (car entry))
             (marker-position (cdr entry))))
      ((listp (cdr entry))
       (cons (substring-no-properties (car entry))
             (scala365-test-stable-imenu-index (cdr entry))))
      (t (list (car entry) :unsupported))))
   index))

(defun scala365-test-wait-process (process)
  (unless (processp process)
    (error "Scala Mode expected a compilation process: %S" process))
  (let ((deadline (+ (float-time) 5.0)) (stable 0) previous detached
        (buffer (process-buffer process)))
    (while (and (< (float-time) deadline)
                (or (not detached) (< stable 2)))
      (accept-process-output process 0.02)
      (let* ((text (and (buffer-live-p buffer)
                        (with-current-buffer buffer
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))
             (current-detached
              (and (not (process-live-p process))
                   (not (memq process compilation-in-progress))
                   (or (not (buffer-live-p buffer))
                       (not (eq (get-buffer-process buffer) process)))))
             (state (list (process-status process) text current-detached)))
        (setq detached current-detached)
        (if (and detached (equal state previous))
            (setq stable (1+ stable))
          (setq stable 0 previous state))))
    (when (or (not detached) (< stable 2))
      (error "Scala Mode compilation did not settle: %S" process))
    (list :status (process-status process) :exit (process-exit-status process)
          :detached detached :stable-polls stable)))

(defun scala365-test-compilation-message-runs (&optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (save-restriction
      (widen)
      (let ((position (point-min)) result)
        (while (< position (point-max))
          (let* ((value (get-text-property position 'compilation-message))
                 (next (or (next-single-property-change
                            position 'compilation-message nil (point-max))
                           (point-max))))
            (when value
              (let* ((loc (compilation--message->loc value))
                     (file-struct (and loc
                                       (compilation--loc->file-struct loc)))
                     (start (scala365-test-line-column position))
                     (text (scala365-test-normalize-string
                            (buffer-substring-no-properties position next))))
                (when (string-match-p "\n" text)
                  (error "Scala Mode compilation location spans lines: %S" text))
                (push (list :start start
                            :end (list (car start)
                                       (+ (cadr start) (string-width text)))
                            :text text
                            :type (compilation--message->type value)
                            :rule (compilation--message->rule value)
                            :line (and loc (compilation--loc->line loc))
                            :column (and loc (compilation--loc->col loc))
                            :file
                            (and file-struct
                                 (scala365-test-stable-datum
                                  (compilation--file-struct->file-spec
                                   file-struct))))
                    result)))
            (setq position next)))
        (nreverse result)))))

(defun scala365-test-line-column (position)
  (save-excursion
    (goto-char position)
    (list (line-number-at-pos) (current-column))))

(defun scala365-test-overlay-runs (&optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (mapcar
     (lambda (overlay)
       (list :start (scala365-test-line-column (overlay-start overlay))
             :end (scala365-test-line-column (overlay-end overlay))
             :text
             (and (overlay-start overlay) (overlay-end overlay)
                  (buffer-substring-no-properties
                   (overlay-start overlay) (overlay-end overlay)))
             :face (copy-tree (overlay-get overlay 'face))))
     (sort (copy-sequence (overlays-in (point-min) (point-max)))
           (lambda (a b) (< (overlay-start a) (overlay-start b)))))))

(defun scala365-test-normalize-compilation-text (text)
  (let ((normalized (scala365-test-normalize-string text)))
    (replace-regexp-in-string
     "^\\(scala-compilation \\(?:started\\|finished\\|exited abnormally with code [0-9]+\\) at \\).*$"
     "\\1[TIME]" normalized t nil)))

(defun scala365-test-compilation-state (buffer &optional include-messages)
  (with-current-buffer buffer
    (append
     (list :mode major-mode
           :process (when-let* ((process (get-buffer-process buffer)))
                      (list :status (process-status process)
                            :exit (process-exit-status process)))
           :text (scala365-test-normalize-compilation-text
                  (buffer-substring-no-properties (point-min) (point-max))))
     (when include-messages
       (list :messages (scala365-test-compilation-message-runs buffer)))
     (list :overlays (scala365-test-overlay-runs buffer)))))

(defun scala365-test-invocation-state ()
  (let ((path (plist-get scala365-test-world :invocations))
        (misses (plist-get scala365-test-world :misses)))
    (list :fields
          (if (file-exists-p path)
              (mapcar #'scala365-test-normalize-string
                      (split-string (scala365-test-file-bytes path) "\0" t))
            nil)
          :misses (if (file-exists-p misses)
                      (scala365-test-file-bytes misses) ""))))

(defun scala365-test-message-delta (before)
  (nreverse (seq-take scala365-test-message-events
                      (- (length scala365-test-message-events) before))))

(defun scala365-test-provenance ()
  (let* ((library (locate-library "scala-mode"))
         (directory (and library (file-name-directory library)))
         actual)
    (unless directory (error "Scala Mode installed library missing"))
    (dolist (entry scala365-test-source-sha256)
      (let ((path (expand-file-name (car entry) directory)))
        (unless (and (file-exists-p path)
                     (equal (scala365-test-file-sha256 path)
                            (if (equal (car entry) "scala-mode.el")
                                scala365-test-installed-root-sha256
                              (cdr entry))))
          (error "Scala Mode installed source drifted: %S" entry))
        (push (cons (car entry) (cdr entry)) actual)))
    (let ((global-count
           (cl-count #'scala-indent:remove-indent-from-previous-empty-line
                     (default-value 'post-command-hook) :test #'eq)))
      (unless (= global-count 1)
        (error "Scala Mode primed hook multiplicity drifted: %S" global-count))
      (list :version "20260118.942"
            :commit "50bcafa181baec7054e27f4bca55d5f9277c6350"
            :source-files (nreverse actual)
            :installed-root scala365-test-installed-root-sha256
            :dependency-closure nil
            :global-post-command-count global-count
            :default-syntax-hooks
            (copy-sequence
             (default-value 'syntax-propertize-extend-region-functions))
            :default-post-self-hooks
            (copy-sequence (default-value 'post-self-insert-hook))
            :tool scala365-test-sbt-tool
            :failure-stream scala365-test-failure-stream-sha256
            :success-stream scala365-test-success-stream-sha256))))

(defun scala365-test-run (case-name root-profile thunk)
  (scala365-test-provenance)
  (let* ((scala365-test-world nil)
         (scala365-test-external-events nil)
         (scala365-test-external-advices nil)
         (scala365-test-command-hook-installed nil)
         (scala365-test-process-records nil)
         (scala365-test-message-events nil)
         (scala365-test-read-events nil)
         (scala365-test-completion-events nil)
         (scala365-test-expected-reads nil)
         (scala365-test-expected-completions nil)
         (scala365-test-command-events nil)
         (scala365-test-watch-commands nil)
         (scala365-test-compile-phase nil)
         (scala365-test-parked-outputs nil)
         (scala365-test-owned-markers nil)
         (scala365-test-minibuffer-initial nil)
         (scala365-test-minibuffer-final nil)
         (scala365-test-body-stage :setup)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-before (copy-sequence timer-idle-list))
         (buffer-before (current-buffer))
         (window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (scala365-test-window-state))
         (warnings-before (scala365-test-buffer-content-state "*Warnings*"))
         (messages-before (scala365-test-buffer-content-state "*Messages*"))
         (code-conversion-before
          (scala365-test-buffer-content-state " *code-conversion-work*"))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (scala365-test-variable-state symbol)))
                  scala365-test-state-symbols))
         body-value body-error cleanup-errors root-gone)
    (unwind-protect
        (condition-case condition
            (progn
              (setq scala365-test-world
                    (scala365-test-allocate-world case-name root-profile))
              (scala365-test-park-output-buffers)
              (scala365-test-configure-world)
              (scala365-test-install-observers)
              (setq scala365-test-body-stage :body)
              (setq body-value (funcall thunk scala365-test-world)))
          (t (setq body-error
                   (list :stage scala365-test-body-stage
                         :condition (scala365-test-condition-state condition)))))
      ;; External guards remain installed until every asynchronous resource
      ;; and restoration reaction has been quiesced.
      (dotimes (pass 3)
        (let ((index 0))
          (dolist (timer (scala365-test-new-timers timers-before idle-before))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'cancel-timer pass index)
                   (lambda () (cancel-timer timer)) cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'reap-process pass index)
                   (lambda ()
                     (set-process-query-on-exit-flag process nil)
                     (when (process-live-p process) (delete-process process))
                     (when (process-live-p process)
                       (error "Scala Mode process survived: %S" process)))
                   cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'kill-buffer pass index (buffer-name buffer))
                   (lambda ()
                     (when (buffer-live-p buffer)
                       (with-current-buffer buffer
                         (setq kill-buffer-query-functions nil)
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer)))
                   cleanup-errors))
            (setq index (1+ index)))))
      (dolist (entry states-before)
        (unless (memq (car entry) scala365-test-terminal-state-symbols)
          (setq cleanup-errors
                (scala365-test-attempt
                 (list 'restore-variable (car entry))
                 (lambda ()
                   (scala365-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors))))
      (setq cleanup-errors
            (scala365-test-attempt
             'restore-warnings
             (lambda () (scala365-test-restore-buffer-content warnings-before))
             cleanup-errors))
      (setq cleanup-errors
            (scala365-test-attempt
             'restore-messages
             (lambda () (scala365-test-restore-buffer-content messages-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (entry (reverse scala365-test-parked-outputs))
          (setq cleanup-errors
                (scala365-test-attempt
                 (list 'restore-output-buffer index (cdr entry))
                 (lambda () (scala365-test-restore-output-buffer entry))
                 cleanup-errors))
          (setq index (1+ index))))
      (setq cleanup-errors
            (scala365-test-attempt
             'restore-windows
             (lambda ()
               (scala365-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (timer (scala365-test-new-timers timers-before idle-before))
          (setq cleanup-errors
                (scala365-test-attempt
                 (list 'restore-reaction-timer index)
                 (lambda ()
                   (unwind-protect
                       (unless (and (eq (timer--function timer)
                                        #'undo-auto--boundary-timer)
                                    (null (timer--repeat-delay timer)))
                         (error "Unexpected Scala Mode restore timer: %S" timer))
                     (cancel-timer timer)))
                 cleanup-errors))
          (setq index (1+ index))))
      (dolist (entry states-before)
        (when (memq (car entry) scala365-test-terminal-state-symbols)
          (setq cleanup-errors
                (scala365-test-attempt
                 (list 'restore-terminal-variable (car entry))
                 (lambda ()
                   (scala365-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors)))))
      ;; Restoration can schedule resources.  Attempt every sibling again
      ;; before dropping the external boundary guards.
      (dotimes (pass 2)
        (let ((index 0))
          (dolist (timer (scala365-test-new-timers timers-before idle-before))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'final-timer pass index)
                   (lambda () (cancel-timer timer)) cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'final-process pass index)
                   (lambda ()
                     (when (process-live-p process) (delete-process process))
                     (when (process-live-p process)
                       (error "Scala Mode late process survived: %S" process)))
                   cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
            (setq cleanup-errors
                  (scala365-test-attempt
                   (list 'final-buffer pass index (buffer-name buffer))
                   (lambda ()
                     (when (buffer-live-p buffer)
                       (with-current-buffer buffer
                         (setq kill-buffer-query-functions nil)
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer)))
                   cleanup-errors))
            (setq index (1+ index)))))
      ;; Killing a formerly displayed compilation buffer can add quit-restore
      ;; window parameters.  Reapply the exact baseline after the final buffer
      ;; sweep, while external guards still cover any restoration reaction.
      (setq cleanup-errors
            (scala365-test-attempt
             'restore-windows-after-final-sweep
             (lambda ()
               (scala365-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (setq cleanup-errors
            (scala365-test-attempt
             'remove-observers #'scala365-test-remove-observers cleanup-errors))
      ;; The exactly owned root is the final destructive action.
      (when scala365-test-world
        (setq cleanup-errors
              (scala365-test-attempt
               'delete-root
               (lambda ()
                 (let* ((root (plist-get scala365-test-world :root))
                        (owner (plist-get scala365-test-world :owner))
                        (true-root (and (file-exists-p root)
                                        (file-name-as-directory
                                         (file-truename root)))))
                   (when true-root
                     (unless (and (file-name-absolute-p root)
                                  (not (equal true-root owner))
                                  (string-prefix-p owner true-root))
                       (error "Scala Mode refuses root deletion: %S"
                              (list owner root)))
                     (delete-directory root t))
                   (setq root-gone (not (file-exists-p root)))))
               cleanup-errors)))
      ;; Deleting Unicode paths legitimately reuses this pre-existing GNU
      ;; scratch buffer.  Restore it after that final destructive action.
      (setq cleanup-errors
            (scala365-test-attempt
             'restore-code-conversion-after-root
             (lambda ()
               (scala365-test-restore-buffer-content code-conversion-before))
             cleanup-errors))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let* ((variable-mismatches
            (delq nil
                  (mapcar
                   (lambda (entry)
                     (unless (scala365-test-variable-restored-p
                              (car entry) (cdr entry))
                       (car entry)))
                   states-before)))
           (cleanup-state
            (list :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
                  :new-processes (seq-difference (process-list) processes-before #'eq)
                  :new-timers (scala365-test-new-timers timers-before idle-before)
                  :variables (null variable-mismatches)
                  :variable-mismatches variable-mismatches
                  :warnings (scala365-test-buffer-content-restored-p warnings-before)
                  :messages (scala365-test-buffer-content-restored-p messages-before)
                  :code-conversion
                  (scala365-test-buffer-content-restored-p code-conversion-before)
                  :windows (equal (scala365-test-window-state) windows-before)
                  :configuration
                  (compare-window-configurations
                   (current-window-configuration) configuration-before)
                  :buffer (eq (current-buffer) buffer-before)
                  :window (eq (selected-window) window-before)
                  :external-events scala365-test-external-events
                  :command-hook scala365-test-command-hook-installed
                  :read-queue scala365-test-expected-reads
                  :completion-queue scala365-test-expected-completions
                  :markers
                  (mapcar (lambda (marker)
                            (list (marker-buffer marker)
                                  (marker-position marker)))
                          scala365-test-owned-markers)
                  :root root-gone
                  :body-error body-error :cleanup-errors cleanup-errors)))
      (unless (and (null (plist-get cleanup-state :new-buffers))
                   (null (plist-get cleanup-state :new-processes))
                   (null (plist-get cleanup-state :new-timers))
                   (plist-get cleanup-state :variables)
                   (plist-get cleanup-state :warnings)
                   (plist-get cleanup-state :messages)
                   (plist-get cleanup-state :code-conversion)
                   (plist-get cleanup-state :windows)
                   (plist-get cleanup-state :configuration)
                   (plist-get cleanup-state :buffer)
                   (plist-get cleanup-state :window)
                   (null (plist-get cleanup-state :external-events))
                   (null (plist-get cleanup-state :command-hook))
                   (null (plist-get cleanup-state :read-queue))
                   (null (plist-get cleanup-state :completion-queue))
                   (cl-every (lambda (state) (null (car state)))
                             (plist-get cleanup-state :markers))
                   (plist-get cleanup-state :root)
                   (null body-error) (null cleanup-errors))
        (error "Scala Mode workflow/cleanup failure: %S" cleanup-state))
      (list :result body-value :cleanup 'clean))))
"####;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn scala_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SCALA_MODE_MELPA_PIN, "scala-mode.el")
        .expect("prepare exact shallow Scala Mode source below ./tmp")
        .with_prelude(SCALA_MODE_TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn scala_mode_package_batch() {
    assert_oracle_batch_cases(
        scala_mode_oracle(),
        "scala-mode-package-batch",
        "scala_mode_parity",
        &workflows::workflow_batch_cases(),
    );
}
