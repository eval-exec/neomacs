#!/usr/bin/env bash
# Validate GNU-shaped Info output before packaging. GNU's install-info target
# depends on the manual build and installs its directory and manual inventory
# (GNU Emacs 31.1 Makefile.in:786-809). Our generator records that inventory.

# Resolve once when sourced: callers may change directory before validating.
_neomacs_info_manuals_inventory="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/docs/info-manuals.list"

# Report a missing generated artifact and the selected binary's rebuild command.
neomacs_info_manuals_error() {
  printf 'Info manuals are incomplete: %s\n' "$1" >&2
  printf 'Rebuild them before packaging, including with --skip-build:\n' >&2
  printf '  cargo xtask build-info --binary %q\n' "$2" >&2
}

# Require the generated inventory, its nonempty manuals, and the directory node.
# $1 is the generated Info directory; $2 optionally selects the rebuild binary.
# Basenames exclude path traversal and shell options. The generator and this
# validator share the tracked inventory; a generated manifest cannot narrow it.
neomacs_validate_info_manuals() {
  local info_dir="$1" binary="${2:-target/release/neomacs}"
  while [[ "$info_dir" == */ && "$info_dir" != / ]]; do info_dir="${info_dir%/}"; done
  local manifest="$info_dir/.neomacs-info-manifest" manual=""
  local inventory="$_neomacs_info_manuals_inventory" links=""

  if [[ ! -f "$inventory" || ! -s "$inventory" ]]; then
    neomacs_info_manuals_error "missing or empty canonical inventory: $inventory" "$binary"
    return 1
  fi
  # Our generator writes regular files only. Reject all links, including
  # internal ones and ancillary assets: cp -a would preserve their targets
  # rather than producing a self-contained installed Info tree.
  if [[ -L "$info_dir" ]]; then
    links="$info_dir"
  elif [[ -d "$info_dir" ]]; then
    if ! links="$(find "$info_dir" -type l -print)"; then
      neomacs_info_manuals_error "cannot inspect generated Info directory: $info_dir" "$binary"
      return 1
    fi
  fi
  if [[ -n "$links" ]]; then
    neomacs_info_manuals_error "symlinks are not allowed in generated Info: $links" "$binary"
    return 1
  fi
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

  # Compare multisets, preserving duplicate detection while allowing reordering.
  if ! LC_ALL=C awk '
    NR == FNR { expected[$0]++; next }
    { seen[$0]++ }
    END {
      for (name in expected) if (seen[name] != expected[name]) exit 1
      for (name in seen) if (!(name in expected)) exit 1
    }
  ' "$inventory" "$manifest"; then
    neomacs_info_manuals_error "manifest does not match canonical inventory: $manifest" "$binary"
    return 1
  fi

  # Check the install-info output format we generate, rather than implementing
  # a general Info reader. GNU info.el finds node headers after the US separator
  # and follows entries within that node's * Menu:. Description continuations
  # are ignored; generated file references occupy their entry's first line.
  if ! LC_ALL=C awk '
    NR == FNR { required[$0] = 1; next }
    FNR == 1 { next } # Preamble before the first Info node separator.
    {
      record = $0
      sub(/^\r?\n/, "", record)
      count = split(record, lines, "\n")
      if (lines[1] !~ /^File:[ \t]*dir([,\t\r]|$)/ ||
          lines[1] !~ /(^|[,\t])[ \t]*Node:[ \t]*Top([,\t\r]|$)/) next
      top_count++
      in_menu = 0
      for (i = 2; i <= count; i++) {
        line = lines[i]
        if (line ~ /^\* Menu:[ \t\r]*$/) {
          menu_count++
          in_menu = 1
        } else if (in_menu && line ~ /^\* [^:]+:[ \t]*\(/) {
          sub(/^\* [^:]+:[ \t]*/, "", line)
          if (line !~ /^\([^()]+\)(Top)?\./) continue
          sub(/^\(/, "", line)
          sub(/\).*/, "", line)
          sub(/\.info$/, "", line)
          found[line ".info"] = 1
        }
      }
    }
    END {
      if (top_count != 1 || menu_count != 1) exit 1
      for (name in required) if (!(name in found)) exit 1
    }
  ' "$inventory" RS="$(printf '\037')" "$info_dir/dir"; then
    neomacs_info_manuals_error "invalid Info directory Top/menu or missing manual targets: $info_dir/dir" "$binary"
    return 1
  fi
}
