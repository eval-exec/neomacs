#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/dispatch-release-event.sh [options]

Send the versioned Neomacs release event consumed by downstream package
repositories.

Required:
  --target-repository OWNER/REPO  Repository receiving repository_dispatch
  --source-repository OWNER/REPO  Repository that published the release
  --tag TAG                       Published stable tag (vMAJOR.MINOR.PATCH)
  --commit SHA                    Exact 40-character released Git commit

Optional:
  --dry-run                       Print the request JSON instead of dispatching
  -h, --help                      Show this help

The authenticated path requires GH_TOKEN with permission to create a
repository dispatch event in the target repository.
USAGE
}

die() {
  echo "$*" >&2
  exit 2
}

target_repository=""
source_repository=""
release_tag=""
release_commit=""
dry_run=false

while (($# > 0)); do
  case "$1" in
    --target-repository)
      (($# >= 2)) || die "--target-repository requires a value"
      target_repository="$2"
      shift 2
      ;;
    --source-repository)
      (($# >= 2)) || die "--source-repository requires a value"
      source_repository="$2"
      shift 2
      ;;
    --tag)
      (($# >= 2)) || die "--tag requires a value"
      release_tag="$2"
      shift 2
      ;;
    --commit)
      (($# >= 2)) || die "--commit requires a value"
      release_commit="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$target_repository" ]] || die "--target-repository is required"
[[ -n "$source_repository" ]] || die "--source-repository is required"
[[ -n "$release_tag" ]] || die "--tag is required"
[[ -n "$release_commit" ]] || die "--commit is required"

repository_pattern='^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$'
[[ "$target_repository" =~ $repository_pattern ]] \
  || die "invalid target repository: $target_repository"
[[ "$source_repository" =~ $repository_pattern ]] \
  || die "invalid source repository: $source_repository"
[[ "$release_commit" =~ ^[0-9a-fA-F]{40}$ ]] \
  || die "release commit must be a 40-character hexadecimal Git object ID"

stable_tag_pattern='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
prerelease_tag_pattern='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-[0-9A-Za-z.-]+$'
if [[ "$release_tag" =~ $prerelease_tag_pattern ]]; then
  echo "not notifying stable package repositories for prerelease $release_tag" >&2
  exit 0
fi
[[ "$release_tag" =~ $stable_tag_pattern ]] \
  || die "unsupported release tag: $release_tag"

command -v jq >/dev/null 2>&1 || die "jq is required to construct the dispatch payload"

event_type="neomacs_release_published_v1"
release_url="https://github.com/$source_repository/releases/tag/$release_tag"
checksums_url="https://github.com/$source_repository/releases/download/$release_tag/SHA256SUMS"
payload="$(
  jq -cn \
    --arg event_type "$event_type" \
    --arg source_repository "$source_repository" \
    --arg tag "$release_tag" \
    --arg commit_sha "$release_commit" \
    --arg release_url "$release_url" \
    --arg checksums_url "$checksums_url" \
    '{
      event_type: $event_type,
      client_payload: {
        schema_version: 1,
        source_repository: $source_repository,
        tag: $tag,
        commit_sha: $commit_sha,
        release_url: $release_url,
        checksums_url: $checksums_url
      }
    }'
)"

if [[ "$dry_run" == true ]]; then
  jq . <<<"$payload"
  exit 0
fi

[[ -n "${GH_TOKEN:-}" ]] \
  || die "GH_TOKEN is required to dispatch to $target_repository"
command -v gh >/dev/null 2>&1 || die "gh is required to send the repository dispatch"

gh api \
  --method POST \
  "repos/$target_repository/dispatches" \
  --input - \
  <<<"$payload"

echo "dispatched $event_type for $release_tag to $target_repository"
