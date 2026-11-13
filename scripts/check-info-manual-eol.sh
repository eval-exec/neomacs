#!/usr/bin/env bash
# Every committed input of xtask info_manuals must be eol=lf.
#
# '@' at the end of a line is the Texinfo @NL command. The "Multiple Spaces"
# node says @NL inserts a single space. A checkout that rewrites the manuals
# to CRLF turns @NL into '@\r', and texinfo 7.2 stops at the first one
# (docs/emacs/search.texi:1427). @image includes FILENAME.txt verbatim in
# Info output (Texinfo "Image Syntax").
#
# --list-entries prints the same source paths as inventory() in
# crates/xtask/src/info_manuals.rs. The xtask test compares the two so the
# case statement cannot drift. The default mode checks git attributes.
# bash 3.2 is enough: membership is a file, not an associative array.
# Set NEOMACS_EOL_GUARD_ROOT to point the check at another tree.
set -euo pipefail

mode=check
case "${1:-}" in
  "") ;;
  --list-entries) mode=entries ;;
  --list-sources) mode=sources ;;
  *)
    echo "usage: check-info-manual-eol.sh [--list-entries|--list-sources]" >&2
    exit 2
    ;;
esac

root="${NEOMACS_EOL_GUARD_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
docs="$root/docs"
list="$docs/info-manuals.list"

if [[ ! -f "$list" ]]; then
  echo "missing Info manual inventory: $list" >&2
  exit 1
fi

# Relative path inventory() would return for this manual name. "generated"
# is the placeholder directory build() passes for Org exports.
inventory_rel() {
  local name="$1"
  case "$name" in
    emacs) printf '%s\n' "docs/emacs/emacs.texi" ;;
    elisp) printf '%s\n' "docs/lispref/elisp.texi" ;;
    eintr) printf '%s\n' "docs/lispintro/emacs-lisp-intro.texi" ;;
    ccmode) printf '%s\n' "docs/misc/cc-mode.texi" ;;
    org | modus-themes) printf '%s\n' "generated/${name}.texi" ;;
    *) printf '%s\n' "docs/misc/${name}.texi" ;;
  esac
}

if [[ "$mode" == entries ]]; then
  while IFS= read -r filename || [[ -n "$filename" ]]; do
    [[ -n "$filename" ]] || continue
    name="${filename%.info}"
    if [[ "$filename" == "$name" ]] \
      || [[ ! "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ ]]; then
      echo "invalid canonical Info filename: $filename" >&2
      exit 1
    fi
    inventory_rel "$name"
  done <"$list"
  exit 0
fi

if ! git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "not a git worktree: $root" >&2
  exit 1
fi

seen_file=$(mktemp "${TMPDIR:-/tmp}/neomacs-info-eol-seen.XXXXXX")
skipped_file=$(mktemp "${TMPDIR:-/tmp}/neomacs-info-eol-skipped.XXXXXX")
trap 'rm -f "$seen_file" "$skipped_file"' EXIT
: >"$seen_file"
: >"$skipped_file"
text_sources=()
binary_sources=()

is_seen() {
  grep -Fxq -- "$1" "$seen_file"
}

record() {
  local abs="$1"
  local rel="${abs#"$root"/}"
  if [[ "$rel" == "$abs" ]]; then
    echo "manual source is outside $root: $abs" >&2
    exit 1
  fi
  if is_seen "$rel"; then
    return 0
  fi
  printf '%s\n' "$rel" >>"$seen_file"
  case "$rel" in
    *.jpg | *.jpeg | *.png | *.gif | *.pdf | *.eps)
      binary_sources+=("$rel")
      ;;
    *)
      text_sources+=("$rel")
      ;;
  esac
}

note_skipped() {
  local name="$1"
  if ! grep -Fxq -- "$name" "$skipped_file"; then
    printf '%s\n' "$name" >>"$skipped_file"
  fi
}

# Same lookup makeinfo uses: the entry directory, then the -I paths info_manuals
# passes. emacsver.texi is generated; the committed input is emacsver.texi.in.
resolve_include() {
  local cwd="$1"
  local name="$2"
  local candidate
  for candidate in "$cwd/$name" "$docs/emacs/$name" "$docs/misc/$name"; do
    if [[ -f "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  if [[ "$name" == *.texi ]]; then
    local template="${name}.in"
    for candidate in "$cwd/$template" "$docs/emacs/$template" "$docs/misc/$template"; do
      if [[ -f "$candidate" ]]; then
        printf '%s\n' "$candidate"
        return 0
      fi
    done
  fi
  return 1
}

walk_texi() {
  local file="$1"
  local cwd="$2"
  local rel line name image found
  [[ -f "$file" ]] || {
    echo "missing Texinfo input: $file" >&2
    exit 1
  }
  rel="${file#"$root"/}"
  if is_seen "$rel"; then
    return 0
  fi
  record "$file"
  while IFS= read -r line || [[ -n "$line" ]]; do
    # Info output inserts FILENAME.txt verbatim when the image has one.
    if [[ "$line" =~ @image\{([^},]+) ]]; then
      image="${BASH_REMATCH[1]}"
      if found="$(resolve_include "$cwd" "${image}.txt")"; then
        record "$found"
      fi
    fi
    [[ "$line" =~ ^[[:space:]]*@include[[:space:]]+([^[:space:]]+) ]] || continue
    name="${BASH_REMATCH[1]}"
    name="${name%\"}"
    name="${name#\"}"
    # @ifset installchapter includes trampinst.texi, which this tree does
    # not ship and which info_manuals does not define. A missing target is
    # not an input. A tracked file that the lookup missed is a path bug.
    if ! found="$(resolve_include "$cwd" "$name")"; then
      note_skipped "$name"
      continue
    fi
    walk_texi "$found" "$cwd"
  done <"$file"
}

walk_org() {
  local file="$1"
  local dir line name target
  [[ -f "$file" ]] || {
    echo "missing Org manual input: $file" >&2
    exit 1
  }
  record "$file"
  dir="$(dirname "$file")"
  while IFS= read -r line || [[ -n "$line" ]]; do
    name=""
    if [[ "$line" =~ ^#\+texinfo:[[:space:]]*@include[[:space:]]+([^[:space:]]+) ]]; then
      name="${BASH_REMATCH[1]}"
    elif [[ "$line" =~ ^#\+setupfile:[[:space:]]+([^[:space:]]+) ]]; then
      name="${BASH_REMATCH[1]}"
    else
      continue
    fi
    target="$dir/$name"
    if [[ ! -f "$target" ]]; then
      echo "unresolved Org input $name from ${file#"$root"/}" >&2
      exit 1
    fi
    case "$name" in
      *.org) walk_org "$target" ;;
      *.texi) walk_texi "$target" "$dir" ;;
      *) record "$target" ;;
    esac
  done <"$file"
}

while IFS= read -r filename || [[ -n "$filename" ]]; do
  [[ -n "$filename" ]] || continue
  name="${filename%.info}"
  if [[ "$filename" == "$name" ]] \
    || [[ ! "$name" =~ ^[A-Za-z0-9][A-Za-z0-9._+-]*$ ]]; then
    echo "invalid canonical Info filename: $filename" >&2
    exit 1
  fi
  rel="$(inventory_rel "$name")"
  case "$rel" in
    generated/*.texi) walk_org "$docs/misc/${name}.org" ;;
    *) walk_texi "$root/$rel" "$(dirname "$root/$rel")" ;;
  esac
done <"$list"

record "$docs/emacs/emacsver.texi.in"
record "$docs/info-manuals.list"
# inventory() publishes these next to elisp.info. The .txt is the Info
# text of @image; the .jpg is the binary asset and must stay binary.
record "$docs/lispref/elisp_type_hierarchy.txt"
record "$docs/lispref/elisp_type_hierarchy.jpg"

if [[ -s "$skipped_file" ]]; then
  while IFS= read -r name; do
    tracked="$(git -C "$root" ls-files -- "*/$name" "$name")"
    if [[ -n "$tracked" ]]; then
      echo "tracked Texinfo input was not on the makeinfo path: $name" >&2
      printf '%s\n' "$tracked" >&2
      exit 1
    fi
  done <"$skipped_file"
fi

required=(
  docs/emacs/emacs.texi
  docs/emacs/search.texi
  docs/emacs/emacsver.texi.in
  docs/lispref/elisp.texi
  docs/lispref/elisp_type_hierarchy.txt
  docs/lispref/elisp_type_hierarchy.jpg
  docs/lispintro/emacs-lisp-intro.texi
  docs/misc/cc-mode.texi
  docs/misc/gnus.texi
  docs/misc/doclicense.texi
  docs/misc/org.org
  docs/misc/modus-themes.org
  docs/misc/org-setup.org
  docs/info-manuals.list
)
for path in "${required[@]}"; do
  if ! is_seen "$path"; then
    echo "manual source list omitted $path" >&2
    exit 1
  fi
done

if [[ "${#text_sources[@]}" -lt 80 ]]; then
  echo "manual source list has only ${#text_sources[@]} text files" >&2
  exit 1
fi

if [[ "$mode" == sources ]]; then
  printf '%s\n' "${text_sources[@]}"
  if [[ "${#binary_sources[@]}" -gt 0 ]]; then
    printf '%s\n' "${binary_sources[@]}"
  fi
  exit 0
fi

fail=0
while IFS= read -r line; do
  case "$line" in
    *": eol: lf") ;;
    *)
      printf '%s\n' "$line" >&2
      fail=1
      ;;
  esac
done < <(git -C "$root" check-attr eol -- "${text_sources[@]}")

if [[ "${#binary_sources[@]}" -gt 0 ]]; then
  while IFS= read -r line; do
    case "$line" in
      *": binary: set") ;;
      *)
        printf '%s\n' "$line" >&2
        fail=1
        ;;
    esac
  done < <(git -C "$root" check-attr binary -- "${binary_sources[@]}")
fi

if [[ "$fail" -ne 0 ]]; then
  echo "Info manual text must have eol=lf and image binaries must stay binary (${#text_sources[@]} text, ${#binary_sources[@]} binary)" >&2
  exit 1
fi

printf 'eol=lf for %s Info manual sources\n' "${#text_sources[@]}"
