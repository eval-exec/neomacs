#!/usr/bin/env bash
# The ownership and mode contract of the macOS release tarball, shared by the
# script that writes it (scripts/package-macos-app.sh, which runs in the
# release job) and the one that verifies the published containers
# (scripts/test-macos-release-artifact.sh).
#
# Root extracting the tarball (into /Applications, say) restores each member's
# recorded owner and mode, so every member must be recorded as root's (0/0),
# not the build runner's, and none may be writable by every account (o+w; a
# symlink's own mode means nothing).  Group write is left alone: the members'
# group is 0 (wheel), which holds only root by default.
#
# bsdtar lists each member as "MODE NLINK UID GID SIZE DATE NAME"; a line whose
# fields say anything else, including one this does not parse, fails the
# audit.
neomacs_audit_macos_tarball() {
  local artifact="$1" listing offending
  if ! listing="$(tar --numeric-owner -tvzf "$artifact")"; then
    echo "$artifact: tar could not list the archive" >&2
    return 1
  fi
  if [[ -z "$listing" ]]; then
    echo "$artifact: tar listed no members" >&2
    return 1
  fi
  offending="$(awk '$3 != "0" || $4 != "0" ||
    (substr($1, 9, 1) == "w" && substr($1, 1, 1) != "l")' <<<"$listing")"
  if [[ -n "$offending" ]]; then
    echo "$artifact: $(grep -c '' <<<"$offending") of $(grep -c '' <<<"$listing") members are not owned by 0/0 (root:wheel) or are writable by every account (o+w):" >&2
    head -n 20 <<<"$offending" >&2
    return 1
  fi
}
