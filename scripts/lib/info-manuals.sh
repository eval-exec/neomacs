#!/usr/bin/env bash
# Validate GNU-shaped Info output before packaging. GNU's install-info target
# depends on the manual build and installs its directory and manual inventory
# (GNU Emacs 31.1 Makefile.in:786-809). Our generator records that inventory.

# Report a missing generated artifact and the selected binary's rebuild command.
neomacs_info_manuals_error() {
  printf 'Info manuals are incomplete: %s\n' "$1" >&2
  printf 'Rebuild them before packaging, including with --skip-build:\n' >&2
  printf '  cargo xtask build-info --binary %q\n' "$2" >&2
}

# Require the generated inventory, its nonempty manuals, and the directory node.
# $1 is the generated Info directory; $2 optionally selects the rebuild binary.
# Basenames exclude path traversal and shell options. Do not duplicate the full
# GNU manual inventory here: the generator owns it and publishes its manifest.
neomacs_validate_info_manuals() {
  local info_dir="$1" binary="${2:-target/release/neomacs}"
  local manifest="$info_dir/.neomacs-info-manifest" manual="" required

  if [[ ! -f "$manifest" || ! -s "$manifest" ]]; then
    neomacs_info_manuals_error "missing or empty manifest: $manifest" "$binary"
    return 1
  fi
  if [[ ! -f "$info_dir/dir" || ! -s "$info_dir/dir" ]]; then
    neomacs_info_manuals_error "missing or empty directory node: $info_dir/dir" "$binary"
    return 1
  fi

  while IFS= read -r manual || [[ -n "$manual" ]]; do
    if [[ ! "$manual" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*\.info$ ]]; then
      neomacs_info_manuals_error "invalid manual basename in $manifest: $manual" "$binary"
      return 1
    fi
    if [[ ! -f "$info_dir/$manual" || ! -s "$info_dir/$manual" ]]; then
      neomacs_info_manuals_error "missing or empty manual: $info_dir/$manual" "$binary"
      return 1
    fi
  done <"$manifest"

  for required in emacs.info elisp.info eintr.info cl.info org.info modus-themes.info; do
    if ! grep -Fxq "$required" "$manifest"; then
      neomacs_info_manuals_error "required manual absent from $manifest: $required" "$binary"
      return 1
    fi
  done
}
