#!/usr/bin/env bash

# Shared, read-only Mach-O inspection helpers for macOS packaging scripts.
# The caller owns tool availability checks and error policy.

is_macho() {
  file -b "$1" 2>/dev/null | grep -q 'Mach-O'
}

macho_dependency_paths() {
  local otool_command="$1"
  local image="$2"

  "$otool_command" -l "$image" 2>/dev/null | awk '
    $1 == "cmd" && $2 ~ /^(LC_LOAD_DYLIB|LC_LOAD_WEAK_DYLIB|LC_REEXPORT_DYLIB|LC_LOAD_UPWARD_DYLIB|LC_LAZY_LOAD_DYLIB)$/ {
      load_command = 1
      next
    }
    load_command && $1 == "name" {
      sub(/^[[:space:]]*name[[:space:]]+/, "")
      sub(/[[:space:]]+\(offset[[:space:]]+[0-9]+\)$/, "")
      print
      load_command = 0
    }
  '
}

# TWO root lists, because the bundle asks two different questions and one list
# answers them wrongly.
#
# NESTED roots are the subtrees codesign's V2 resource rules mark nested
# (Frameworks|SharedFrameworks|PlugIns|Plug-ins|XPCServices|Helpers|MacOS...).
# EVERY file under them is treated as nested code and must carry its own
# signature -- that is why the portable dump beside the executable had to be
# signed rather than skipped for not being Mach-O.
macos_bundle_nested_roots() {
  printf '%s\n' MacOS Frameworks Helpers PlugIns
}

# MODULE roots hold loadable code that is NOT nested: a subdirectory of a
# nested root must be a real bundle, and a plug-in directory is not one, so the
# GStreamer plug-ins and GIO modules live under Resources instead.  Files here
# are sealed as resources by the bundle signature, so they must NOT be signed
# one by one -- Contents/Resources/neomacs alone is ~4500 Lisp and etc files,
# and codesign --verify --strict fails on a text file.  Only the Mach-O images
# here need their own signature, because dlopen under the hardened runtime
# requires one.
macos_bundle_module_roots() {
  printf '%s\n' Resources
}

# Everything that must be WALKED when relocating load commands or auditing
# dependencies: both kinds carry Mach-O images we own.
macos_bundle_scan_roots() {
  macos_bundle_nested_roots
  macos_bundle_module_roots
}

# Roots whose images may be DELETED when the runtime cannot satisfy them.
# Contents/MacOS is absent on purpose: those are our own program binaries, and
# an unsatisfiable dependency there is a broken build, not something to trim.
# Dropping them is silent catastrophe - it removed the main executable itself
# and the failure only surfaced later as a missing staged executable.
macos_bundle_droppable_roots() {
  printf '%s\n' Frameworks Helpers PlugIns Resources
}

# Every LC_RPATH search path an image carries, in load-command order.
macho_rpaths() {
  local otool_command="$1"
  local image="$2"

  "$otool_command" -l "$image" 2>/dev/null | awk '
    $1 == "cmd" && $2 == "LC_RPATH" {
      load_command = 1
      next
    }
    load_command && $1 == "path" {
      sub(/^[[:space:]]*path[[:space:]]+/, "")
      sub(/[[:space:]]+\(offset[[:space:]]+[0-9]+\)$/, "")
      print
      load_command = 0
    }
  '
}

# Where a load command's target lives ON THE BUILD MACHINE, or nothing when it
# cannot be resolved.  loader_dir is the directory the image was READ FROM,
# which is not always the directory it now sits in: once a dylib is copied into
# Frameworks/ its @loader_path siblings are still back at the source, so
# resolving against the bundled location would lose every sibling dependency.  dyld's four spellings are not interchangeable: an
# absolute path is itself, @loader_path is relative to the referring image,
# @executable_path to the main executable's directory, and @rpath must be
# searched against that image's own LC_RPATH list -- which may itself be
# written in terms of the other two.
macho_resolve_dependency_source() {
  local otool_command="$1" dependency="$2" image="$3" loader_dir="$4"
  local executable_dir="$5"
  local rpath candidate

  case "$dependency" in
    @loader_path/*)
      candidate="$loader_dir/${dependency#@loader_path/}"
      [[ -f "$candidate" ]] && printf '%s\n' "$candidate"
      return
      ;;
    @executable_path/*)
      candidate="$executable_dir/${dependency#@executable_path/}"
      [[ -f "$candidate" ]] && printf '%s\n' "$candidate"
      return
      ;;
    @rpath/*)
      while IFS= read -r rpath; do
        [[ -n "$rpath" ]] || continue
        case "$rpath" in
          @loader_path*) rpath="$loader_dir${rpath#@loader_path}" ;;
          @executable_path*) rpath="$executable_dir${rpath#@executable_path}" ;;
        esac
        candidate="$rpath/${dependency#@rpath/}"
        if [[ -f "$candidate" ]]; then
          printf '%s\n' "$candidate"
          return
        fi
      done < <(macho_rpaths "$otool_command" "$image")
      return
      ;;
    /*)
      [[ -f "$dependency" ]] && printf '%s\n' "$dependency"
      return
      ;;
  esac
}

# The identity a dependency must have once it lives in the bundle.  A
# framework-shaped dependency keeps its own relative path, because its Mach-O
# image is nested inside a bundle whose layout dyld reads; a flat dylib
# collapses to a basename at the top of Frameworks/.
macos_bundled_relative_path() {
  local dependency="$1"
  local relative="${dependency#@rpath/}"
  relative="${relative#@loader_path/}"
  relative="${relative#@executable_path/}"

  if [[ "$relative" == *.framework/* ]]; then
    # An absolute spelling must be cut back to the framework directory itself:
    # /opt/x/Python3.framework/Versions/3.9/Python3 is bundled as
    # Python3.framework/Versions/3.9/Python3, not under /opt/x.
    local head name tail
    head="${relative%%.framework/*}"
    name="${head##*/}"
    tail="${relative#*.framework/}"
    printf '%s\n' "$name.framework/$tail"
  else
    printf '%s\n' "$(basename "$dependency")"
  fi
}

# Vendor the complete non-system dependency closure of everything staged in the
# bundle, so the .app depends on nothing outside itself and /usr/lib.
#
# This is deliberately BEST EFFORT and never fatal.  Two later passes already
# own the failure policy and they disagree about what a missing dependency
# means: an unsatisfiable droppable image (the SDK ships libges but not the
# Python3.framework it wants) is removed by drop_unsatisfiable_images, while an
# unsatisfiable program binary is a broken build the relocation pass reports and
# dies on.  Making this pass fatal would turn the first case into a build
# failure and lose that distinction.
#
# The closure is what makes the bundle self-contained: before it existed the
# only path into Frameworks/ was a bulk copy of the GStreamer SDK, so our own
# binaries' Homebrew dependencies were vendored only by the accident of living
# in the same directory as the SDK's libraries.
macos_vendor_dependency_closure() {
  local contents="$1" frameworks_dir="$2" executable_dir="$3"
  local -a work=()
  local -a origins=()
  local root image loader_dir dependencies dependency relative target source
  local index=0 vendored=0 unresolved=0

  for root in $(macos_bundle_scan_roots); do
    [[ -d "$contents/$root" ]] || continue
    while IFS= read -r -d '' image; do
      if is_macho "$image"; then
        work+=("$image")
        origins+=("$(dirname "$image")")
      fi
    done < <(find "$contents/$root" -type f -print0)
  done

  while ((index < ${#work[@]})); do
    image="${work[index]}"
    loader_dir="${origins[index]}"
    index=$((index + 1))
    dependencies="$(macho_dependency_paths otool "$image")" || continue

    while IFS= read -r dependency; do
      [[ -n "$dependency" ]] || continue
      case "$dependency" in
        /usr/lib/*|/System/Library/*) continue ;;
      esac

      relative="$(macos_bundled_relative_path "$dependency")"
      target="$frameworks_dir/$relative"
      # Already bundled -- either staged earlier or vendored by this walk, in
      # which case its own dependencies are queued already.
      [[ -f "$target" ]] && continue

      source="$(macho_resolve_dependency_source otool "$dependency" "$image" "$loader_dir" "$executable_dir")"
      if [[ -z "$source" ]]; then
        echo "  unresolved: $dependency (required by $(basename "$image"))" >&2
        unresolved=$((unresolved + 1))
        continue
      fi

      # A framework is a BUNDLE, not a loose image: copying only its Mach-O
      # file would leave its Info.plist and Resources behind, and dyld reads
      # that layout.  Copy the whole .framework once, then carry on with the
      # image inside it.
      if [[ "$relative" == *.framework/* ]]; then
        local framework_source framework_target
        framework_source="${source%%.framework/*}.framework"
        framework_target="$frameworks_dir/${relative%%.framework/*}.framework"
        if [[ -d "$framework_source" && ! -d "$framework_target" ]]; then
          mkdir -p "$(dirname "$framework_target")"
          cp -R "$framework_source" "$framework_target"
          chmod -R u+w "$framework_target"
          vendored=$((vendored + 1))
          echo "  vendored ${relative%%.framework/*}.framework <- $framework_source"
          work+=("$target")
          origins+=("$(dirname "$source")")
          continue
        fi
      fi

      mkdir -p "$(dirname "$target")"
      # -L because a Homebrew dependency is usually a symlink to a versioned
      # file; the bundle needs the real image under the name the load command
      # asks for.  Homebrew installs libraries read-only and install_name_tool
      # rewrites them in place, so the copy must be writable.
      cp -L "$source" "$target"
      chmod u+w "$target"
      vendored=$((vendored + 1))
      echo "  vendored $relative <- $source"
      work+=("$target")
      origins+=("$(dirname "$source")")
    done <<<"$dependencies"
  done

  echo "dependency closure: vendored $vendored image(s), $unresolved unresolved"
}
