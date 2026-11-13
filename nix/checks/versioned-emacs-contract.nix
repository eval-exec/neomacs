{
  executable,
  compatibilityVersion,
  runtimeRoot,
  env,
  timeout,
}:
''
  test -L "${executable}"
  test "$(readlink "${executable}")" = neomacs
  test -x "${executable}"

  # Enter the public wrapper with its default runtime root. Keep HOME inherited.
  versioned_output="$(${timeout} --signal=TERM --kill-after=5s 30s \
    ${env} -u NEOMACS_RUNTIME_ROOT \
    "${executable}" -Q --batch --eval \
    '(let* ((root ${builtins.toJSON runtimeRoot})
            (library (locate-library "simple")))
       (unless (and
                (equal (getenv "NEOMACS_RUNTIME_ROOT") root)
                (equal (file-truename data-directory)
                       (concat root "/etc/"))
                library
                (string-prefix-p (concat root "/lisp/")
                                 (file-truename library)))
         (error "Versioned entry used an external runtime: %S %S %S"
                (getenv "NEOMACS_RUNTIME_ROOT") data-directory library))
       (princ emacs-version))')"
  test "$versioned_output" = "${compatibilityVersion}"
''
