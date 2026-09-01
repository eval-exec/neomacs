use std::time::Duration;

use crate::{AGENIX_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AGENIX_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// agenix.el edits `age'-encrypted secrets transparently: opening a `.age' file
/// in a directory with a `secrets.nix' enters `agenix-mode', which asks
/// `nix-instantiate' who the recipients are, runs `age --decrypt' with the
/// user's identities and replaces the buffer with the plaintext; saving pipes
/// the buffer through `age --encrypt' back into the same path and reverts.
///
/// `age' is not installed on this host, so it is the one thing stood in for --
/// and it is a sound boundary, because the package never inspects the encrypted
/// format.  It hands the `.age' file to `age' *by path* and takes `age''s
/// stdout as opaque plaintext, so nothing about the ciphertext has to be
/// authored here.  Everything else is real: `nix-instantiate' really evaluates
/// a real `secrets.nix' and the package parses its JSON, `ssh-keygen' really
/// decides whether an identity is password protected, the identities are real
/// generated ed25519 keys, and the package builds every argument vector itself.
/// The stand-in records each invocation's argv, working directory and stdin
/// byte for byte, and armours/unarmours the payload so the round trip is
/// genuinely end to end: what is typed is what comes back after the save
/// re-reads the file.
///
/// The secrets directory holds nothing but `secrets.nix' and the ciphertext, so
/// the workflows can assert that the plaintext never reaches it.  The stand-in
/// keeps its own recordings well outside that directory.
const AGENIX_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar agx-test-home
  (file-name-as-directory
   (expand-file-name "agenix" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar agx-test-root
  (file-name-as-directory (expand-file-name "project" agx-test-home))
  "The secrets directory itself: it must never hold anything but ciphertext.")

(defvar agx-test-bin
  (file-name-as-directory (expand-file-name "bin" agx-test-home)))

(defvar agx-test-keys
  (file-name-as-directory (expand-file-name "keys" agx-test-home)))

(defvar agx-test-records
  (file-name-as-directory (expand-file-name "age-runs" agx-test-home)))

(defconst agx-test-recipients
  '("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB1alicealicealicealicealicealiceal alice@example"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB2bobbobbobbobbobbobbobbobbobbobbo bob@example")
  "Fixed recipient public keys, so the encrypt argv is deterministic.")

(defun agx-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun agx-test-install-age ()
  "Install a recording stand-in `age' as the only extra entry on `exec-path'.
It records every invocation's argv, stdin and working directory, and stands in
for the cipher only: `--encrypt' armours stdin into the output file the package
names, `--decrypt' unarmours the file the package hands it.  The package never
inspects the encrypted format, so nothing about it is authored here."
  (let ((path (expand-file-name "age" agx-test-bin)))
    (make-directory agx-test-bin t)
    (make-directory agx-test-records t)
    (agx-test-write
     path
     (concat
      "#!/bin/sh\n"
      "root=" agx-test-home "\n"
      "records=" agx-test-records "\n"
      "n=1\n"
      "[ -f \"$root/.total\" ] && n=$(($(cat \"$root/.total\") + 1))\n"
      "printf '%s' \"$n\" > \"$root/.total\"\n"
      "record=$(printf '%s%02d-age' \"$records\" \"$n\")\n"
      "cat > \"$record.stdin\"\n"
      "{\n"
      "  printf 'argv:\\n'\n"
      "  for arg in \"$@\"; do printf '  %s\\n' \"$arg\"; done\n"
      "  printf 'cwd: %s\\n' \"$PWD\"\n"
      "  printf 'stdin:'\n"
      "  if [ -s \"$record.stdin\" ]; then printf '\\n'; cat \"$record.stdin\"\n"
      "  else printf ' <empty>\\n'; fi\n"
      "} > \"$record\"\n"
      "mode=$1\n"
      "out=\n"
      "target=\n"
      "identity=\n"
      "prev=\n"
      "for arg in \"$@\"; do\n"
      "  case $prev in\n"
      "    -o) out=$arg ;;\n"
      "    --identity) identity=$arg ;;\n"
      "  esac\n"
      "  case $arg in\n"
      "    -*) ;;\n"
      "    *) [ \"$prev\" = -o ] || [ \"$prev\" = --identity ] || \\\n"
      "       [ \"$prev\" = --recipient ] || target=$arg ;;\n"
      "  esac\n"
      "  prev=$arg\n"
      "done\n"
      "if [ \"$mode\" = --encrypt ]; then\n"
      "  { printf -- '-----BEGIN AGE ENCRYPTED FILE-----\\n'\n"
      "    base64 < \"$record.stdin\"\n"
      "    printf -- '-----END AGE ENCRYPTED FILE-----\\n'; } > \"$out\"\n"
      "  exit 0\n"
      "fi\n"
      "if [ \"$mode\" = --decrypt ]; then\n"
      "  case \":$AGENIX_TEST_AUTHORIZED:\" in\n"
      "    *\":$identity:\"*) ;;\n"
      "    *) echo 'age: error: no identity matched any of the recipients' >&2\n"
      "       exit 1 ;;\n"
      "  esac\n"
      "  sed '1d;$d' \"$target\" | base64 -d\n"
      "  exit 0\n"
      "fi\n"
      "echo 'age: error: unknown mode' >&2\n"
      "exit 2\n"))
    (set-file-modes path #o755)
    (setq exec-path (cons (directory-file-name agx-test-bin) exec-path))
    path))

(defun agx-test-keygen (name)
  "Generate a real unprotected ed25519 identity called NAME, return its path."
  (let ((path (expand-file-name name agx-test-keys)))
    (make-directory agx-test-keys t)
    (when (file-exists-p path) (delete-file path))
    (call-process "ssh-keygen" nil nil nil
                  "-t" "ed25519" "-N" "" "-C" name "-q" "-f" path)
    path))

(defun agx-test-authorize (&rest identities)
  (setenv "AGENIX_TEST_AUTHORIZED" (mapconcat #'identity identities ":")))

(defun agx-test-records ()
  "Every age invocation, in order, with the sandbox paths normalised."
  (mapcar (lambda (file)
            (cons (file-name-nondirectory file)
                  (with-temp-buffer
                    (let ((coding-system-for-read 'utf-8))
                      (insert-file-contents file))
                    (buffer-string))))
          (sort (directory-files agx-test-records t "\\`[0-9][0-9]-age\\'") #'string<)))

(defun agx-test-run-count ()
  (length (directory-files agx-test-records nil "\\`[0-9][0-9]-age\\'")))

(defun agx-test-reset ()
  "Reset state shared by otherwise-independent batched workflow cases."
  ;; Keep batched cases process-local while giving each one the clean project
  ;; it had when cases ran as separate tests.  In particular, do not retain a
  ;; file-visiting buffer whose backing ciphertext is about to be replaced:
  ;; `save-buffer' would correctly treat that as an external modification and
  ;; prompt on stdin in batch mode.
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (and buffer-file-name
                 (file-in-directory-p buffer-file-name agx-test-root))
        (set-buffer-modified-p nil)
        (kill-buffer buffer))))
  (when (file-directory-p agx-test-root)
    (delete-directory agx-test-root t))
  (when (file-directory-p agx-test-records)
    (delete-directory agx-test-records t))
  (let ((total (expand-file-name ".total" agx-test-home)))
    (when (file-exists-p total)
      (delete-file total)))
  (make-directory agx-test-records t))

(defun agx-test-project (&optional secret-name)
  "Create or extend a real agenix project with SECRET-NAME declared."
  (let* ((name (or secret-name "db-password.age"))
         (nix (expand-file-name "secrets.nix" agx-test-root)))
    (make-directory agx-test-root t)
    (agx-test-write
     nix
     (format "{\n  \"%s\".publicKeys = [ %s ];\n}\n"
             name
             (mapconcat (lambda (key) (format "\"%s\"" key))
                        agx-test-recipients " ")))
    nix))

(defun agx-test-encrypt-fixture (name plaintext)
  "Create NAME as an armoured secret holding PLAINTEXT, the way age would."
  (let ((path (expand-file-name name agx-test-root)))
    (agx-test-write
     path
     (concat "-----BEGIN AGE ENCRYPTED FILE-----\n"
             (base64-encode-string (encode-coding-string plaintext 'utf-8) t)
             "\n-----END AGE ENCRYPTED FILE-----\n"))
    path))

(defun agx-test-file-text (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8))
      (insert-file-contents path))
    (buffer-string)))

(defun agx-test-entries ()
  "Every entry in the secrets directory, dotfiles included.
A stray lock file next to an encrypted secret shows up here."
  (sort (directory-files agx-test-root) #'string<))

(defun agx-test-messages (regexp)
  "Return the echo-area lines matching REGEXP, in order."
  (with-current-buffer (get-buffer-create "*Messages*")
    (cl-remove-if-not
     (lambda (line) (string-match-p regexp line))
     (split-string
      (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))

(defun agx-test-warning ()
  "Return the package's own warning lines, plus whether Nix named the secret.
The rest of the warning is `nix-instantiate' output, whose formatting belongs
to the Nix version on the host rather than to the package."
  (let ((buffer (get-buffer "*Warnings*")))
    (and buffer
         (let* ((text (with-current-buffer buffer
                        (buffer-substring-no-properties (point-min) (point-max))))
                (lines (split-string text "\n")))
           (list :warning (seq-take lines 2)
                 :nix-reported-missing-attribute
                 (and (string-match-p "attribute .*undeclared\\.age.* missing" text) t))))))

(defun agx-test-open (name)
  "Visit NAME in the secrets directory the way a user does, in a live window."
  (let ((buffer (find-file-noselect (expand-file-name name agx-test-root))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun agx-test-plaintext-on-disk-p (needle)
  "Whether NEEDLE appears anywhere in the secrets directory."
  (cl-some (lambda (name)
             (let ((path (expand-file-name name agx-test-root)))
               (and (file-regular-p path)
                    (string-match-p (regexp-quote needle)
                                    (agx-test-file-text path)))))
           (directory-files agx-test-root nil "\\`[^.]")))

(defun agx-test-state ()
  (list :mode major-mode
        :read-only buffer-read-only
        :modified (buffer-modified-p)
        :point (point)
        :buffer (buffer-substring-no-properties (point-min) (point-max))
        :write-contents-functions write-contents-functions
        :auto-save buffer-auto-save-file-name))
"##;

fn agenix_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGENIX_MELPA_PIN, "agenix.el")
        .expect("prepare pinned agenix source below ./tmp")
        .with_prelude(AGENIX_TEST_PRELUDE)
        .with_timeout(AGENIX_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed agenix parity test").into()
}

/// Multi-probe batch for `assert_agenix_parity` cases (2a).
pub(crate) fn assert_agenix_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(agenix_oracle(), &name, "agenix_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn agenix_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_agenix_batch(&cases);
}

// END generated package batch tests
