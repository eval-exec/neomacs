#!/usr/bin/env bash
# Exercise the macOS dependency-closure walk without a Mac.
#
# The walk is pure path logic over otool output, so it can be driven with fake
# Mach-O images and a stub otool.  A dependency is encoded IN the image content
# rather than in a sidecar file, so that copying an image into the bundle also
# copies its dependency list -- which is what makes the transitive case real
# instead of a single hop.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$root/tmp/dep-closure-test"
rm -rf "$work"
mkdir -p "$work/bin" "$work/brew/lib" "$work/app/Contents/MacOS" "$work/app/Contents/Frameworks"

cat > "$work/bin/file" <<'EOF'
#!/usr/bin/env bash
target="${!#}"
if [[ -f "$target" ]] && head -n1 "$target" 2>/dev/null | grep -q '^MACHO$'; then
  echo "Mach-O 64-bit dynamically linked shared library arm64"
else
  echo "data"
fi
EOF

cat > "$work/bin/otool" <<'EOF'
#!/usr/bin/env bash
image="${!#}"
[[ -f "$image" ]] || exit 1
while IFS= read -r line; do
  case "$line" in
    "DEP "*)
      printf '      cmd LC_LOAD_DYLIB\n  cmdsize 56\n     name %s (offset 24)\n' "${line#DEP }" ;;
    "RPATH "*)
      printf '      cmd LC_RPATH\n  cmdsize 32\n     path %s (offset 12)\n' "${line#RPATH }" ;;
  esac
done < "$image"
EOF

chmod +x "$work/bin/file" "$work/bin/otool"
PATH="$work/bin:$PATH"

macho() { local f="$1"; shift; printf 'MACHO\n' > "$f"; for d in "$@"; do printf '%s\n' "$d" >> "$f"; done; }

# our binary -> a Homebrew dylib (absolute) + a system dylib (must be ignored)
macho "$work/app/Contents/MacOS/neomacs" \
  "DEP $work/brew/lib/libfontconfig.1.dylib" \
  "DEP /usr/lib/libSystem.B.dylib"
# the Homebrew dylib itself pulls a second level, via @loader_path
macho "$work/brew/lib/libfontconfig.1.dylib" "DEP @loader_path/libexpat.1.dylib"
macho "$work/brew/lib/libexpat.1.dylib"
# a dependency nothing can resolve must be reported, not fatal
macho "$work/app/Contents/MacOS/helper" "DEP /nonexistent/libghost.dylib"
chmod 0444 "$work/brew/lib/libfontconfig.1.dylib"

# @rpath, the spelling the GStreamer SDK uses: resolvable only by searching the
# referring image's own LC_RPATH list.
macho "$work/app/Contents/MacOS/rpathuser" \
  "RPATH $work/brew/lib" \
  "DEP @rpath/libgstfoo.dylib" \
  "DEP @rpath/Python3.framework/Versions/3.9/Python3"
macho "$work/brew/lib/libgstfoo.dylib"
mkdir -p "$work/brew/lib/Python3.framework/Versions/3.9"
macho "$work/brew/lib/Python3.framework/Versions/3.9/Python3"
mkdir -p "$work/brew/lib/Python3.framework/Resources"
echo "plist" > "$work/brew/lib/Python3.framework/Resources/Info.plist"
chmod -R a-w "$work/brew/lib/Python3.framework"

# shellcheck source=/dev/null
source "$root/scripts/lib/macos-macho.sh"

out="$work/out.log"
set +e
macos_vendor_dependency_closure "$work/app/Contents" \
  "$work/app/Contents/Frameworks" "$work/app/Contents/MacOS" >"$out" 2>&1
status=$?
set -e

fail=0
check() { if eval "$2"; then echo "  ok   $1"; else echo "  FAIL $1"; fail=1; fi; }

echo "closure output:"; sed 's/^/    /' "$out"
echo "assertions:"
check "exits 0 (best effort, never fatal)"        "[[ $status -eq 0 ]]"
check "vendored the absolute Homebrew dependency" "[[ -f '$work/app/Contents/Frameworks/libfontconfig.1.dylib' ]]"
check "vendored TRANSITIVELY via @loader_path"    "[[ -f '$work/app/Contents/Frameworks/libexpat.1.dylib' ]]"
check "did not vendor a system dylib"             "[[ ! -f '$work/app/Contents/Frameworks/libSystem.B.dylib' ]]"
check "made the read-only copy writable"          "[[ -w '$work/app/Contents/Frameworks/libfontconfig.1.dylib' ]]"
check "reported the unresolvable dependency"      "grep -q 'unresolved: /nonexistent/libghost.dylib' '$out'"
check "resolved @rpath via LC_RPATH"              "[[ -f '$work/app/Contents/Frameworks/libgstfoo.dylib' ]]"
check "kept a framework's internal layout"        "[[ -f '$work/app/Contents/Frameworks/Python3.framework/Versions/3.9/Python3' ]]"
check "copied the framework BUNDLE, not just image" "[[ -f '$work/app/Contents/Frameworks/Python3.framework/Resources/Info.plist' ]]"
check "made the copied framework writable"        "[[ -w '$work/app/Contents/Frameworks/Python3.framework/Versions/3.9/Python3' ]]"
check "counted 4 vendored, 1 unresolved"          "grep -q 'vendored 4 image(s), 1 unresolved' '$out'"

# Sensitivity: the assertions must be capable of failing.
rm -f "$work/app/Contents/Frameworks/libexpat.1.dylib"
if [[ -f "$work/app/Contents/Frameworks/libexpat.1.dylib" ]]; then
  echo "  FAIL sensitivity check is inert"; fail=1
else
  echo "  ok   sensitivity: transitive assertion tests a real file"
fi

if ((fail == 0)); then
  echo "PASS"
else
  echo "FAIL"
  exit 1
fi
