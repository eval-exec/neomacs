#!/usr/bin/env bash
# Run the Linux CI steps the batch gate does not, with the same commands as
# .github/workflows/lint.yml and build-gate.yml.
#
# A self-check fails when a step in those two files is neither run below nor
# listed, with a reason, as covered elsewhere. It also fails when a new
# workflow file appears and is not listed as outside this pair. The default
# run prints the lead's staging commands and exits that stage NOT RUN
# (required-pending). It pushes only when PARITY_STAGING_PUSH=lead.
# build-gate.yml is on the default branch, so `gh workflow run --ref` is
# valid there. A workflow file that exists only on the staging ref is 404.
#
# Usage:
#   scripts/ci/local-ci-parity.sh
#   scripts/ci/local-ci-parity.sh --check-coverage [workflows-dir]
#   scripts/ci/local-ci-parity.sh msvc-staging <batch-number>
#
# On this host, pin the process away from CPUs 4-11. The pre-push hook runs
# cargo with no pin of its own; msvc-staging wraps its push in taskset when
# taskset exists.

set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

die() {
    printf 'local-ci-parity: %s\n' "$*" >&2
    exit 1
}

covered_steps() {
    cat <<'EOF'
Run actionlint
Install shellcheck
Lint release shell contracts
Test Docker runtime context contract
Test release dispatch contract
Test main cache build boundary
Test release-note generation contract
Test Info manual packaging contracts
Check Info manual sources stay LF
Check native font dependency boundaries
Test macOS dependency closure contract
cargo fmt
Check advisories, licenses, bans, and sources
Check integration test target count
Check native dependency type coherence
Test harness spawn architecture
cargo check minimal capability build
cargo check opt-in test mechanisms
Test SQLite-disabled capability surface
EOF
}

# name|why this script does not run the step
elsewhere_steps() {
    cat <<'EOF'
Checkout|actions/checkout setup, not a product check
Set up Go|runner setup; the actionlint step below still requires a Go toolchain
Set up Rust tooling|runner setup; the cargo steps below use the toolchain already on PATH
cargo check workspace (Unix)|the batch gate and the pre-push hook already run cargo check --workspace --all-targets
cargo check workspace (Windows)|windows-only; msvc-staging dispatches build-gate.yml, which runs this step
Test DirectWrite native font replay|windows-only; msvc-staging dispatches build-gate.yml, which runs this step
Test Windows w32 and readonly-delete rows|windows-only; msvc-staging dispatches build-gate.yml, which runs this step
Test Windows serial error shapes|windows-only; msvc-staging dispatches build-gate.yml, which runs this step
Test CoreText Thin-only font matching|macOS-only; this host has no CoreText
EOF
}

# filename|why the file is outside lint.yml + build-gate.yml
outside_workflows() {
    cat <<'EOF'
cachix-main.yml|release cache workflow, not the lint or build-gate gap
cachix-release.yml|release cache workflow, not the lint or build-gate gap
codeql.yml|code scanning, not the lint or build-gate gap
docker-release.yml|release packaging, not the lint or build-gate gap
gui-tests-macos.yml|macOS GUI workflow, not the lint or build-gate gap
labels.yml|labeler, not the lint or build-gate gap
linux.yml|full Linux CI workflow, not the lint or build-gate gap
macos-oracle-cache.yml|macOS oracle cache, not the lint or build-gate gap
merge-conflicts.yml|merge-conflict check, not the lint or build-gate gap
native-contract.yml|native contract workflow, not the lint or build-gate gap
native-display-contract.yml|display contract workflow, not the lint or build-gate gap
neomacs-perf.yml|perf workflow, not the lint or build-gate gap
nextest-shards.yml|nextest shard workflow, not the lint or build-gate gap
nix-smoke.yml|nix smoke workflow, not the lint or build-gate gap
regex-fuzz.yml|fuzz workflow, not the lint or build-gate gap
release.yml|release workflow, not the lint or build-gate gap
scroll120-cohort.yml|scroll cohort workflow, not the lint or build-gate gap
test-suite.yml|post-push test suite, not the lint or build-gate gap
tmp_mac_test.yml|temporary macOS workflow, not the lint or build-gate gap
window-oracle-nightly.yml|nightly oracle workflow, not the lint or build-gate gap
windows-installer.yml|Windows installer, not the lint or build-gate gap
windows-upgrade-contract.yml|Windows upgrade contract, not the lint or build-gate gap
youtube-cards.yml|card workflow, not the lint or build-gate gap
EOF
}

coverage_py=$(cat <<'PY'
import sys
from pathlib import Path

workflows = Path(sys.argv[1])
text = sys.stdin.read().split("\0")
if len(text) != 3:
    raise SystemExit("coverage tables were not passed")
covered_raw, elsewhere_raw, outside_raw = text

def lines(block):
    return [line for line in block.splitlines() if line.strip()]

covered = lines(covered_raw)
elsewhere = {}
for line in lines(elsewhere_raw):
    name, sep, reason = line.partition("|")
    if not sep or not name or not reason.strip():
        raise SystemExit(f"elsewhere entry needs name|reason: {line}")
    if name in elsewhere:
        raise SystemExit(f"duplicate elsewhere entry: {name}")
    elsewhere[name] = reason.strip()

outside = {}
for line in lines(outside_raw):
    name, sep, reason = line.partition("|")
    if not sep or not name or not reason.strip():
        raise SystemExit(f"outside entry needs file|reason: {line}")
    if name in outside:
        raise SystemExit(f"duplicate outside entry: {name}")
    outside[name] = reason.strip()

in_scope = ("lint.yml", "build-gate.yml")
for name in in_scope:
    if name in outside:
        raise SystemExit(f"{name} is both in scope and outside")

def step_names(path):
    found = []
    for line in path.read_text().splitlines():
        if line.startswith("      - name:"):
            found.append(line.split(":", 1)[1].strip().strip("'\""))
        elif line.startswith("        name:"):
            found.append(line.split(":", 1)[1].strip().strip("'\""))
    return found

seen = []
for name in in_scope:
    path = workflows / name
    if not path.is_file():
        raise SystemExit(f"missing in-scope workflow: {path}")
    seen.extend(step_names(path))
unique = set(seen)
if len(unique) != len(seen):
    dupes = sorted({n for n in seen if seen.count(n) > 1})
    # Checkout and Set up Rust tooling are repeated on purpose.
    allowed = {"Checkout", "Set up Rust tooling"}
    extra = [n for n in dupes if n not in allowed]
    if extra:
        raise SystemExit("unexpected duplicate step names: " + ", ".join(extra))

covered_set = set(covered)
if len(covered_set) != len(covered):
    raise SystemExit("duplicate covered step")
overlap = sorted(covered_set & set(elsewhere))
if overlap:
    raise SystemExit("listed as both run and elsewhere: " + ", ".join(overlap))

missing = sorted(unique - covered_set - set(elsewhere))
stale_covered = sorted(covered_set - unique)
stale_elsewhere = sorted(set(elsewhere) - unique)
problems = []
if missing:
    problems.append("uncovered steps:\n  " + "\n  ".join(missing))
if stale_covered:
    problems.append("covered steps not in the workflows:\n  " + "\n  ".join(stale_covered))
if stale_elsewhere:
    problems.append("elsewhere steps not in the workflows:\n  " + "\n  ".join(stale_elsewhere))

present = {p.name for p in workflows.glob("*.yml")}
known = set(in_scope) | set(outside)
new_files = sorted(present - known)
missing_files = sorted(known - present)
if new_files:
    problems.append("workflow files not classified:\n  " + "\n  ".join(new_files))
if missing_files:
    problems.append("classified files that are gone:\n  " + "\n  ".join(missing_files))
if problems:
    raise SystemExit("\n".join(problems))
print(f"coverage ok: {len(unique)} steps in {', '.join(in_scope)}; {len(present)} workflow files")
PY
)

check_coverage() {
    local workflows=${1:-$root/.github/workflows}
    [[ -d $workflows ]] || die "no workflows dir: $workflows"
    # -c keeps stdin free for the coverage tables.
    python3 -I -c "$coverage_py" "$workflows"
}

# Tables are passed as three NUL-separated blocks on stdin.
check_coverage_from_tables() {
    local workflows=$1
    {
        covered_steps
        printf '\0'
        elsewhere_steps
        printf '\0'
        outside_workflows
    } | check_coverage "$workflows"
}

require_shellcheck() {
    if ! command -v shellcheck >/dev/null 2>&1; then
        die "shellcheck is not on PATH. lint.yml pins v0.11.0. actionlint without shellcheck hides SC findings."
    fi
    local version
    version=$(shellcheck --version | awk '/^version:/ { print $2 }')
    [[ $version == 0.11.0 ]] || die "shellcheck $version is on PATH; lint.yml pins 0.11.0"
}

step_actionlint() {
    require_shellcheck
    # Copied from lint.yml "Run actionlint".
    go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 .github/workflows/*.yml
}

step_install_shellcheck() {
    # Copied from lint.yml "Install shellcheck". If v0.11.0 is already on
    # PATH, skip the download. CI uses sudo install into /usr/local/bin.
    if command -v shellcheck >/dev/null 2>&1 \
        && [[ $(shellcheck --version | awk '/^version:/ { print $2 }') == 0.11.0 ]]; then
        shellcheck --version
        return 0
    fi
    curl -sSfL -o /tmp/shellcheck.tar.xz \
        https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.x86_64.tar.xz
    tar -xJf /tmp/shellcheck.tar.xz -C /tmp
    sudo install -m 0755 /tmp/shellcheck-v0.11.0/shellcheck /usr/local/bin/shellcheck
    shellcheck --version
}

step_shell_contracts() {
    require_shellcheck
    # Copied from lint.yml "Lint release shell contracts".
    shellcheck -x \
        install.sh \
        scripts/dispatch-release-event.sh \
        scripts/generate-release-notes.sh \
        scripts/prepare-docker-runtime-context.sh \
        scripts/test-dispatch-release-event.sh \
        scripts/test-docker-runtime-context.sh \
        scripts/test-generate-release-notes.sh \
        scripts/test-info-manuals.sh \
        scripts/check-info-manual-eol.sh \
        scripts/check-native-font-boundaries.sh \
        scripts/lib/macos-macho.sh \
        scripts/vendor-macos-runtime.sh \
        scripts/test-macos-dependency-closure.sh \
        scripts/package-release.sh \
        scripts/package-appimage.sh \
        scripts/package-deb.sh \
        scripts/package-rpm.sh \
        scripts/test-linux-release-artifacts.sh \
        scripts/ci/build-main-cache.sh \
        scripts/test-main-cache.sh \
        scripts/ci/setup-linux.sh
    sh -n install.sh
    bash -n \
        scripts/dispatch-release-event.sh \
        scripts/generate-release-notes.sh \
        scripts/prepare-docker-runtime-context.sh \
        scripts/test-dispatch-release-event.sh \
        scripts/test-docker-runtime-context.sh \
        scripts/test-generate-release-notes.sh \
        scripts/test-info-manuals.sh \
        scripts/check-info-manual-eol.sh \
        scripts/check-native-font-boundaries.sh \
        scripts/ci/build-main-cache.sh \
        scripts/test-main-cache.sh \
        scripts/lib/macos-macho.sh \
        scripts/vendor-macos-runtime.sh \
        scripts/test-macos-dependency-closure.sh
}

step_fmt() {
    # Copied from lint.yml "cargo fmt".
    cargo fmt --all --check
    cargo fmt --manifest-path crates/neovm-core/fuzz/Cargo.toml --check
}

deny_bin() {
    local cache=${XDG_CACHE_HOME:-$HOME/.cache}/neomacs/cargo-deny-0.20.2
    local bin=$cache/cargo-deny
    if [[ ! -x $bin ]]; then
        mkdir -p "$cache"
        local tarball=$cache/cargo-deny.tar.gz
        curl -sSfL -o "$tarball" \
            https://github.com/EmbarkStudios/cargo-deny/releases/download/0.20.2/cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz
        tar -xzf "$tarball" -C "$cache" --strip-components=1
    fi
    [[ -x $bin ]] || die "cargo-deny 0.20.2 did not extract"
    printf '%s\n' "$bin"
}

step_deny() {
    # cargo-deny-action v2 at 3c6349835b2b7b196a839186cb8b78e02f7b5f25 runs
    # cargo-deny 0.20.2 with: global --all-features, command check,
    # command-arguments --hide-inclusion-graph. Log level and manifest path
    # are the action defaults.
    local bin
    bin=$(deny_bin)
    "$bin" --log-level warn --manifest-path ./Cargo.toml --all-features check --hide-inclusion-graph
}

step_one_binary() {
    # Copied from lint.yml "Check integration test target count".
    cargo metadata --no-deps --format-version 1 | python3 -I scripts/ci/check-test-targets.py
}

step_coherence() {
    # Copied from build-gate.yml "Check native dependency type coherence".
    cargo run --locked -p xtask -- check-dependency-coherence
}

step_harness() {
    # Copied from build-gate.yml "Test harness spawn architecture".
    cargo nextest run --locked -p neomacs-test-env
}

step_minimal() {
    # Copied from build-gate.yml "cargo check minimal capability build".
    cargo check -p neomacs --no-default-features
}

step_opt_in() {
    # Copied from build-gate.yml "cargo check opt-in test mechanisms".
    cargo check -p neovm-core --tests --features perf-probes,contract-fixtures
    cargo check -p neomacs-layout-engine --tests --features perf-probes
    cargo check -p neomacs-tui-tests --tests --features doom-repro
    cargo check -p neomacs --tests --features release-pdump-tests
}

step_sqlite() {
    # Copied from build-gate.yml "Test SQLite-disabled capability surface".
    cargo nextest run -p neovm-core --no-default-features \
        -E 'test(disabled_build_exposes_only_gnu_capability_probes)'
}

run_named() {
    local name=$1
    shift
    printf '== %s ==\n' "$name"
    "$@"
}

run_local() {
    run_named "Run actionlint" step_actionlint
    run_named "Install shellcheck" step_install_shellcheck
    run_named "Lint release shell contracts" step_shell_contracts
    run_named "Test Docker runtime context contract" ./scripts/test-docker-runtime-context.sh
    run_named "Test release dispatch contract" ./scripts/test-dispatch-release-event.sh
    run_named "Test main cache build boundary" ./scripts/test-main-cache.sh
    run_named "Test release-note generation contract" ./scripts/test-generate-release-notes.sh
    run_named "Test Info manual packaging contracts" ./scripts/test-info-manuals.sh
    run_named "Check Info manual sources stay LF" ./scripts/check-info-manual-eol.sh
    run_named "Check native font dependency boundaries" ./scripts/check-native-font-boundaries.sh
    run_named "Test macOS dependency closure contract" ./scripts/test-macos-dependency-closure.sh
    run_named "cargo fmt" step_fmt
    run_named "Check integration test target count" step_one_binary
    run_named "Check native dependency type coherence" step_coherence
    run_named "Test harness spawn architecture" step_harness
    run_named "cargo check minimal capability build" step_minimal
    run_named "cargo check opt-in test mechanisms" step_opt_in
    run_named "Test SQLite-disabled capability surface" step_sqlite
    run_named "Check advisories, licenses, bans, and sources" step_deny
}

staging_commands() {
    local n=${1:-<N>}
    cat <<EOF
git push origin HEAD:refs/heads/staging/batch-${n}
gh workflow run build-gate.yml --ref staging/batch-${n}
gh workflow run test-suite.yml --ref staging/batch-${n} -f job=neomacs-xwidget-webview-gui
EOF
}

print_staging_pending() {
    local n=${1:-<N>}
    printf 'msvc-staging: NOT RUN (required-pending)\n'
    printf 'The landing owner runs, with PARITY_STAGING_PUSH=lead:\n'
    staging_commands "$n"
}

msvc_staging() {
    local n=${1:-}
    [[ $# -eq 1 && $n =~ ^[0-9]+$ ]] || die "usage: scripts/ci/local-ci-parity.sh msvc-staging <batch-number>"
    if [[ ${PARITY_STAGING_PUSH:-} != lead ]]; then
        print_staging_pending "$n"
        exit 2
    fi
    if ! git diff --quiet || ! git diff --cached --quiet; then
        die "worktree is dirty; commit before pushing a staging ref"
    fi
    local sha ref
    sha=$(git rev-parse HEAD)
    ref="staging/batch-${n}"
    printf 'push %s to refs/heads/%s\n' "$sha" "$ref"
    # The hook runs cargo fmt and cargo check with no CPU pin. Keep it off
    # the measurement island (CPUs 4-11) on this host.
    if command -v taskset >/dev/null 2>&1; then
        taskset -c 0-3,14-31 git push origin "HEAD:refs/heads/${ref}"
    else
        git push origin "HEAD:refs/heads/${ref}"
    fi
    # build-gate.yml is on the default branch, so this dispatch is valid.
    # test-suite.yml is too; its job input selects the xwidget job alone.
    gh workflow run build-gate.yml --ref "$ref"
    gh workflow run test-suite.yml --ref "$ref" -f job=neomacs-xwidget-webview-gui
    local run_id="" attempt=0
    while [[ $attempt -lt 30 ]]; do
        attempt=$((attempt + 1))
        run_id=$(gh run list --workflow build-gate.yml --branch "$ref" --commit "$sha" --limit 1 --json databaseId --jq '.[0].databaseId')
        if [[ -n $run_id && $run_id != null ]]; then
            break
        fi
        sleep 10
    done
    [[ -n $run_id && $run_id != null ]] || die "no build-gate run appeared for $sha on $ref"
    printf 'watching run %s\n' "$run_id"
    gh run watch "$run_id" --exit-status
}

usage() {
    cat <<'EOF'
usage: scripts/ci/local-ci-parity.sh [--check-coverage [workflows-dir] | msvc-staging <batch-number>]
EOF
}

main() {
    case ${1:-} in
        "")
            check_coverage_from_tables "$root/.github/workflows"
            run_local
            print_staging_pending "<N>"
            exit 2
            ;;
        --check-coverage)
            check_coverage_from_tables "${2:-$root/.github/workflows}"
            ;;
        msvc-staging)
            shift
            check_coverage_from_tables "$root/.github/workflows"
            msvc_staging "$@"
            ;;
        -h|--help)
            usage
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

# check_coverage reads the tables from stdin. The wrapper feeds them.
# When main calls check_coverage_from_tables, check_coverage's python reads
# that pipe, not the script's stdin. That is what we want.
main "$@"
