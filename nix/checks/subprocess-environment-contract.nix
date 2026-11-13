{
  executable,
  printenv,
  env ? "env",
  timeout ? "timeout",
}:
let
  shellQuote = value: "'" + builtins.replaceStrings [ "'" ] [ "'\\''" ] value + "'";
in
''
  (
    set -eu
    n749_home_present="''${HOME+x}"
    n749_home_value="''${HOME-}"
    n749_contract_dir="$(mktemp -d "''${TMPDIR:-/tmp}/neomacs-subprocess-env.XXXXXX")"
    trap 'rm -rf -- "$n749_contract_dir"' EXIT
    mkdir -p "$n749_contract_dir/first" "$n749_contract_dir/second"
    n749_sentinel="$n749_contract_dir/first:$n749_contract_dir/second"

    cat > "$n749_contract_dir/check.el" <<'NEOMACS_SUBPROCESS_ENV_EL'
  ;;; -*- lexical-binding: t; -*-
  (let* ((case-name (getenv "NEOMACS_CONTRACT_CASE"))
         (expected-present (equal (getenv "NEOMACS_CONTRACT_PRESENT") "1"))
         (expected (and expected-present (getenv "NEOMACS_CONTRACT_VALUE")))
         (expected-status (if expected-present 0 1))
         (expected-bytes (if expected-present (concat expected "\n") ""))
         (program ${builtins.toJSON printenv})
         failures
         (expected-home
          (and (equal (getenv "NEOMACS_CONTRACT_HOME_PRESENT") "x")
               (getenv "NEOMACS_CONTRACT_HOME_VALUE"))))
    (unless (equal (getenv "HOME") expected-home)
      (push 'home failures))
    (princ (format "environment observation %s parent=%S home-preserved=%S\n"
                   case-name (getenv "LD_LIBRARY_PATH")
                   (equal (getenv "HOME") expected-home)))
    (unless (equal (getenv "LD_LIBRARY_PATH") expected)
      (push 'parent failures))
    (let ((coding-system-for-read 'no-conversion)
          (coding-system-for-write 'no-conversion))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (let ((status (call-process program nil (current-buffer) nil
                                    "LD_LIBRARY_PATH")))
          (princ (format "environment observation %s sync-status=%S sync-bytes=%S\n"
                         case-name status (buffer-string)))
          (unless (and (equal status expected-status)
                       (equal (buffer-string) expected-bytes))
            (push 'sync failures))))
      (let ((output (generate-new-buffer " *nix-subprocess-environment*"))
            process)
        (unwind-protect
            (progn
              (with-current-buffer output (set-buffer-multibyte nil))
              (setq process
                    (make-process
                     :name "nix-subprocess-environment"
                     :buffer output :command (list program "LD_LIBRARY_PATH")
                     :connection-type 'pipe :coding 'no-conversion :noquery t
                     ;; The default sentinel appends an exit message to BUFFER.
                     :sentinel #'ignore))
              (let ((deadline (+ (float-time) 10)))
                (while (process-live-p process)
                  (when (> (float-time) deadline)
                    (error "Async environment child timed out in %s" case-name))
                  (accept-process-output process 0.05))
                ;; Drain any buffered output after observing explicit completion.
                (while (accept-process-output process 0.01)
                  (when (> (float-time) deadline)
                    (error "Async output drain timed out in %s" case-name))))
              (princ (format "environment observation %s async-status=%S async-exit=%S async-bytes=%S\n"
                             case-name (process-status process)
                             (process-exit-status process)
                             (with-current-buffer output (buffer-string))))
              (unless (and (eq (process-status process) 'exit)
                           (equal (process-exit-status process) expected-status)
                           (with-current-buffer output
                             (equal (buffer-string) expected-bytes)))
                (push 'async failures)))
          (when (and process (process-live-p process)) (delete-process process))
          (kill-buffer output))))
    (when failures
      (error "Environment contract failed in %s for %S" case-name (nreverse failures)))
    (princ (format "nix subprocess environment %s ok\n" case-name)))
  (kill-emacs 0)
  NEOMACS_SUBPROCESS_ENV_EL

    # Each editor gets a controlled child environment; this shell's LD and HOME
    # are never set/unset. Empty directories cannot replace a runtime library.
    cd "$n749_contract_dir"
    n749_failed=0
    for n749_case in absent empty sentinel; do
      case "$n749_case" in
        absent) n749_present=0; n749_value= ;;
        empty) n749_present=1; n749_value= ;;
        sentinel) n749_present=1; n749_value="$n749_sentinel" ;;
      esac
      n749_case_status=0
      if test "$n749_present" = 0; then
        ${shellQuote timeout} --signal=TERM --kill-after=5s 30s \
          ${shellQuote env} -u LD_LIBRARY_PATH \
          NEOMACS_CONTRACT_CASE="$n749_case" \
          NEOMACS_CONTRACT_PRESENT="$n749_present" \
          NEOMACS_CONTRACT_VALUE="$n749_value" \
          NEOMACS_CONTRACT_HOME_PRESENT="$n749_home_present" \
          NEOMACS_CONTRACT_HOME_VALUE="$n749_home_value" \
          ${shellQuote executable} -Q --batch --load "$n749_contract_dir/check.el" \
          || n749_case_status=$?
      else
        ${shellQuote timeout} --signal=TERM --kill-after=5s 30s \
          ${shellQuote env} LD_LIBRARY_PATH="$n749_value" \
          NEOMACS_CONTRACT_CASE="$n749_case" \
          NEOMACS_CONTRACT_PRESENT="$n749_present" \
          NEOMACS_CONTRACT_VALUE="$n749_value" \
          NEOMACS_CONTRACT_HOME_PRESENT="$n749_home_present" \
          NEOMACS_CONTRACT_HOME_VALUE="$n749_home_value" \
          ${shellQuote executable} -Q --batch --load "$n749_contract_dir/check.el" \
          || n749_case_status=$?
      fi
      if test "$n749_case_status" != 0; then
        echo "environment contract $n749_case editor exit=$n749_case_status" >&2
        n749_failed=1
      fi
    done
    test "''${HOME+x}" = "$n749_home_present"
    test "''${HOME-}" = "$n749_home_value"
    test "$n749_failed" = 0
  )
''
