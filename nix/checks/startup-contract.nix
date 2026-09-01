{
  executable,
  marker,
}:
''
  export HOME="$TMPDIR/clean-home"
  export XDG_CACHE_HOME="$HOME/.cache"
  export XDG_CONFIG_HOME="$HOME/.config"
  export XDG_DATA_HOME="$HOME/.local/share"
  mkdir -p "$HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"

  # Deliberately omit --quick/-Q and --no-site-file.  Issue #60 was hidden by
  # those switches, so every installed-runtime contract retains site startup.
  output="$(${executable} --batch --eval \
    '(progn (princ "${marker}\n") (kill-emacs 0))')"
  grep -Fqx "${marker}" <<<"$output"
''
