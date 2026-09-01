#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dispatcher="$repo_root/scripts/dispatch-release-event.sh"

mkdir -p "$repo_root/tmp"
work_dir="$(mktemp -d "$repo_root/tmp/release-dispatch-test.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT

release_commit="abcdef1234567890abcdef1234567890abcdef12"
payload="$work_dir/payload.json"

"$dispatcher" \
  --dry-run \
  --target-repository Irfrit/neomacs-aur \
  --source-repository eval-exec/neomacs \
  --tag v9.8.7 \
  --commit "$release_commit" \
  >"$payload"

jq -e \
  --arg commit "$release_commit" \
  '
    .event_type == "neomacs_release_published_v1"
    and .client_payload.schema_version == 1
    and .client_payload.source_repository == "eval-exec/neomacs"
    and .client_payload.tag == "v9.8.7"
    and .client_payload.commit_sha == $commit
    and .client_payload.release_url
      == "https://github.com/eval-exec/neomacs/releases/tag/v9.8.7"
    and .client_payload.checksums_url
      == "https://github.com/eval-exec/neomacs/releases/download/v9.8.7/SHA256SUMS"
  ' \
  "$payload" >/dev/null

prerelease_output="$work_dir/prerelease.out"
prerelease_error="$work_dir/prerelease.err"
"$dispatcher" \
  --dry-run \
  --target-repository Irfrit/neomacs-aur \
  --source-repository eval-exec/neomacs \
  --tag v9.8.7-rc.1 \
  --commit "$release_commit" \
  >"$prerelease_output" 2>"$prerelease_error"
test ! -s "$prerelease_output"
grep -Fq 'not notifying stable package repositories for prerelease v9.8.7-rc.1' \
  "$prerelease_error"

if "$dispatcher" \
  --dry-run \
  --target-repository Irfrit/neomacs-aur \
  --source-repository eval-exec/neomacs \
  --tag 9.8.7 \
  --commit "$release_commit" \
  >"$work_dir/invalid-tag.out" 2>"$work_dir/invalid-tag.err"
then
  echo "release dispatcher accepted a tag without the required v prefix" >&2
  exit 1
fi
grep -Fq 'unsupported release tag: 9.8.7' "$work_dir/invalid-tag.err"

if "$dispatcher" \
  --dry-run \
  --target-repository Irfrit/neomacs-aur \
  --source-repository eval-exec/neomacs \
  --tag v9.8.7 \
  --commit abcdef \
  >"$work_dir/invalid-commit.out" 2>"$work_dir/invalid-commit.err"
then
  echo "release dispatcher accepted a short commit identity" >&2
  exit 1
fi
grep -Fq 'release commit must be a 40-character hexadecimal Git object ID' \
  "$work_dir/invalid-commit.err"

if "$dispatcher" \
  --dry-run \
  --target-repository Irfrit/neomacs-aur \
  --source-repository invalid \
  --tag v9.8.7 \
  --commit "$release_commit" \
  >"$work_dir/invalid-repository.out" 2>"$work_dir/invalid-repository.err"
then
  echo "release dispatcher accepted an invalid source repository" >&2
  exit 1
fi
grep -Fq 'invalid source repository: invalid' "$work_dir/invalid-repository.err"

if env -u GH_TOKEN "$dispatcher" \
  --target-repository Irfrit/neomacs-aur \
  --source-repository eval-exec/neomacs \
  --tag v9.8.7 \
  --commit "$release_commit" \
  >"$work_dir/missing-token.out" 2>"$work_dir/missing-token.err"
then
  echo "release dispatcher attempted authenticated delivery without GH_TOKEN" >&2
  exit 1
fi
grep -Fq 'GH_TOKEN is required to dispatch to Irfrit/neomacs-aur' \
  "$work_dir/missing-token.err"

echo "release dispatch payload contract passed"
