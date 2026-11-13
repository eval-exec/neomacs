{
  executable,
  printenv,
  ncursesLibrary,
  env ? "env",
  timeout ? "timeout",
}:
let
  shellQuote = value: "'" + builtins.replaceStrings [ "'" ] [ "'\\''" ] value + "'";
in
''
  (
    set -eu
    n749_priority_dir="$(mktemp -d "''${TMPDIR:-/tmp}/neomacs-library-priority.XXXXXX")"
    trap 'rm -rf -- "$n749_priority_dir"' EXIT
    mkdir "$n749_priority_dir/caller"
    n749_runtime_library="$(readlink -f -- ${shellQuote ncursesLibrary})"
    case "$n749_runtime_library" in /nix/store/*) ;; *) exit 1 ;; esac
    test -f "$n749_runtime_library"
    # Copy the actual ABI-compatible Nix library unchanged, under its SONAME.
    # There is no synthetic library, injected constructor, or host library.
    cp -L -- "$n749_runtime_library" "$n749_priority_dir/caller/libncursesw.so.6"
    cmp -- "$n749_runtime_library" "$n749_priority_dir/caller/libncursesw.so.6"
    cat > "$n749_priority_dir/check.el" <<'NEOMACS_LIBRARY_PRIORITY_EL'
  ;;; -*- lexical-binding: t; -*-
  (require 'json)
  (let* ((expected (getenv "NEOMACS_LIBRARY_CONTRACT_LD"))
         (private-library (file-truename (getenv "NEOMACS_LIBRARY_CONTRACT_RUNTIME")))
         (parent (getenv "LD_LIBRARY_PATH"))
         mapped child-status child-bytes failures)
    (unless (equal parent expected) (push 'parent-environment failures))
    (let ((coding-system-for-read 'no-conversion)
          (coding-system-for-write 'no-conversion))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (setq child-status (call-process ${builtins.toJSON printenv}
                                        nil (current-buffer) nil "LD_LIBRARY_PATH")
              child-bytes (buffer-string))))
    (unless (and (equal child-status 0)
                 (equal child-bytes (concat expected "\n")))
      (push 'child-environment failures))
    ;; Read only this editor's map. Log the matching ncurses paths, never the
    ;; full map or complete process environment.
    (with-temp-buffer
      (insert-file-contents "/proc/self/maps")
      (dolist (line (split-string (buffer-string) "\n" t))
        (when (string-match " /" line)
          (let ((path (substring line (1+ (match-beginning 0)))))
            (when (string-match "/libncursesw[.]so[^/]*\\'" path)
              (let ((canonical (file-truename path)))
                (unless (member canonical mapped) (push canonical mapped))))))))
    (setq mapped (nreverse mapped))
    (unless (equal mapped (list private-library))
      (push 'private-runtime-library failures))
    (princ (format "ncurses priority observation %s\n"
                   (json-encode `((private-library . ,private-library)
                                  (mapped-ncurses . ,(vconcat mapped))
                                  (parent-ld . ,parent)
                                  (child-status . ,child-status)
                                  (child-bytes . ,child-bytes)
                                  (failures . ,(vconcat (nreverse failures)))))))
    (when failures (error "Nix ncurses priority/environment contract failed"))
    (princ "nix ncurses priority and subprocess environment ok\n"))
  (kill-emacs 0)
  NEOMACS_LIBRARY_PRIORITY_EL
    # Only the editor child receives this controlled caller path. Its children
    # must inherit the same value; the surrounding HOME/LD are never changed.
    cd "$n749_priority_dir"
    ${shellQuote timeout} --signal=TERM --kill-after=5s 30s \
      ${shellQuote env} LD_LIBRARY_PATH="$n749_priority_dir/caller" \
      NEOMACS_LIBRARY_CONTRACT_LD="$n749_priority_dir/caller" \
      NEOMACS_LIBRARY_CONTRACT_RUNTIME="$n749_runtime_library" \
      ${shellQuote executable} -Q --batch --load "$n749_priority_dir/check.el"
  )
''
