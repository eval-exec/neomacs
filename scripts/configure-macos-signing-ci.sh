#!/usr/bin/env bash
set -euo pipefail

required_names=(
  MACOS_CERTIFICATE_P12_BASE64
  MACOS_CERTIFICATE_PASSWORD
  APPLE_NOTARY_KEY_P8_BASE64
  APPLE_NOTARY_KEY_ID
  APPLE_NOTARY_ISSUER_ID
)

present=0
for name in "${required_names[@]}"; do
  [[ -n "${!name:-}" ]] && present=$((present + 1))
done

publish_distribution_mode() {
  local mode="$1"

  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'distribution_mode=%s\n' "$mode" >>"$GITHUB_OUTPUT"
  fi
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf 'MACOS_DISTRIBUTION_MODE=%s\n' "$mode" >>"$GITHUB_ENV"
  fi
}

if ((present == 0)); then
  if [[ "${MACOS_REQUIRE_SIGNING:-0}" == 1 ]]; then
    echo "MACOS_REQUIRE_SIGNING=1, but macOS signing secrets are absent" >&2
    exit 1
  fi
  publish_distribution_mode adhoc
  echo "Apple signing secrets are absent; this build will be ad-hoc signed and unnotarized"
  exit 0
fi
if ((present != ${#required_names[@]})); then
  echo "macOS signing configuration is partial; required secrets:" >&2
  printf '  %s\n' "${required_names[@]}" >&2
  exit 1
fi
if [[ -z "${GITHUB_ENV:-}" ]]; then
  echo "GITHUB_ENV is required; this helper is intended for GitHub Actions" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
signing_dir="$repo_root/tmp/macos-signing"
certificate_path="$signing_dir/developer-id.p12"
notary_key_path="$signing_dir/notary-api-key.p8"
keychain_path="$signing_dir/neomacs-signing.keychain-db"
keychain_password="$(openssl rand -base64 32)"

rm -rf "$signing_dir"
mkdir -p "$signing_dir"
chmod 700 "$signing_dir"
printf '%s' "$MACOS_CERTIFICATE_P12_BASE64" | base64 --decode >"$certificate_path"
printf '%s' "$APPLE_NOTARY_KEY_P8_BASE64" | base64 --decode >"$notary_key_path"
chmod 600 "$certificate_path" "$notary_key_path"

security create-keychain -p "$keychain_password" "$keychain_path"
security set-keychain-settings -lut 21600 "$keychain_path"
security unlock-keychain -p "$keychain_password" "$keychain_path"
security import "$certificate_path" \
  -P "$MACOS_CERTIFICATE_PASSWORD" \
  -A -t cert -f pkcs12 -k "$keychain_path"
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s -k "$keychain_password" "$keychain_path"
security list-keychains -d user -s "$keychain_path"
security default-keychain -d user -s "$keychain_path"

identity="$({ security find-identity -v -p codesigning "$keychain_path" || true; } \
  | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' \
  | head -n 1)"
if [[ -z "$identity" ]]; then
  echo "the imported certificate has no Developer ID Application identity" >&2
  exit 1
fi

rm -f "$certificate_path"

{
  printf 'MACOS_SIGNING_IDENTITY=%s\n' "$identity"
  printf 'MACOS_NOTARY_KEY_PATH=%s\n' "$notary_key_path"
  printf 'MACOS_NOTARY_KEY_ID=%s\n' "$APPLE_NOTARY_KEY_ID"
  printf 'MACOS_NOTARY_ISSUER_ID=%s\n' "$APPLE_NOTARY_ISSUER_ID"
} >>"$GITHUB_ENV"

publish_distribution_mode developer-id
echo "configured ephemeral Developer ID signing and App Store Connect notarization"
