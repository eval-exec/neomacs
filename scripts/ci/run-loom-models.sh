#!/usr/bin/env bash
# Discover every standalone Loom crate and run it at one preemption bound.
#
#   BOUND=2 bash scripts/ci/run-loom-models.sh   # default; push CI
#   BOUND=3 bash scripts/ci/run-loom-models.sh   # Sunday schedule of test-suite.yml
#
# Discovery globs (peer #7096), relative to the repo root:
#   crates/*/tests/*loom*/Cargo.toml
#   tests/*loom*/Cargo.toml
# The list and count print before any crate runs. The count must be at least
# FLOOR. Floor is 9 on main e4cd6b50cf. Raise it to 10 when
# tests/pentry-keeper-loom lands in E2 slice L0.
#
# Each crate's README.md or run.sh names its preemption variable
# (*PREEMPTIONS). The script exports a variable only when that crate's Rust
# reads it, or when the crate leaves Loom's Builder value alone and the named
# variable is LOOM_MAX_PREEMPTIONS (Builder::new reads it). A crate whose
# harness hardcodes Some(N) is bound=N var=fixed, and only matches when
# BOUND=N. Anything else still runs (it is not skipped) and is reported
# unsupported: a preemption bound of None, or a README variable the harness
# overwrites. Unsupported and test failures exit non-zero. Later crates still
# run.
set -euo pipefail

readonly FLOOR=9

usage() {
    echo "usage: BOUND=<n> $0" >&2
    echo "BOUND defaults to 2 and must be a positive integer." >&2
}

if [[ $# -ne 0 ]]; then
    usage
    exit 2
fi

if [[ ! ${BOUND:-2} =~ ^[1-9][0-9]*$ ]]; then
    usage
    exit 2
fi
readonly BOUND=${BOUND:-2}

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

log_dir=${LOOM_LOG_DIR:-$root/target/loom-model-logs}
mkdir -p "$log_dir"

# Prints discovered crate directories, one relative path per line, sorted.
discover_crates() {
    local manifest rel
    local -a found=()
    shopt -s nullglob
    for manifest in "$root"/crates/*/tests/*loom*/Cargo.toml "$root"/tests/*loom*/Cargo.toml; do
        rel=${manifest#"$root"/}
        rel=${rel%/Cargo.toml}
        found+=("$rel")
    done
    shopt -u nullglob
    if ((${#found[@]} == 0)); then
        return 0
    fi
    printf '%s\n' "${found[@]}" | sort -u
}

# Sets CLASS_MODE, CLASS_BOUND, CLASS_VAR, and CLASS_EXPORT (array).
# CLASS_MODE is env, fixed, or unsupported.
classify() {
    local dir=$1
    local -a scan=() vars=() honored=() literals=()
    local rust_blob="" token has_none=0 has_dynamic=0

    CLASS_MODE=unsupported
    CLASS_BOUND=unapplied
    CLASS_VAR=none
    CLASS_EXPORT=()

    [[ -f $dir/README.md ]] && scan+=("$dir/README.md")
    [[ -f $dir/run.sh ]] && scan+=("$dir/run.sh")
    if ((${#scan[@]} > 0)); then
        mapfile -t vars < <(grep -hoE '\b[A-Z][A-Z0-9_]*PREEMPTIONS\b' "${scan[@]}" | sort -u || true)
    fi

    local -a rust_files=()
    while IFS= read -r -d '' token; do
        rust_files+=("$token")
    done < <(find "$dir" -name '*.rs' -not -path '*/target/*' -print0)
    if ((${#rust_files[@]} > 0)); then
        rust_blob=$(grep -hE 'preemption_bound|env::var' "${rust_files[@]}" || true)
    fi

    for token in "${vars[@]+"${vars[@]}"}"; do
        if printf '%s\n' "$rust_blob" | grep -qE "env::var(_os)?\\([[:space:]]*\"${token}\""; then
            honored+=("$token")
        fi
    done

    if printf '%s\n' "$rust_blob" | grep -qE 'preemption_bound[[:space:]]*=[[:space:]]*None\b'; then
        has_none=1
    fi
    mapfile -t literals < <(printf '%s\n' "$rust_blob" \
        | grep -oE 'preemption_bound[[:space:]]*=[[:space:]]*Some\([[:space:]]*[0-9]+' \
        | grep -oE '[0-9]+$' | sort -u || true)
    if printf '%s\n' "$rust_blob" | grep -E 'preemption_bound[[:space:]]*=' \
        | grep -vE 'preemption_bound[[:space:]]*=[[:space:]]*None([[:space:]]|;|$)' \
        | grep -vE 'preemption_bound[[:space:]]*=[[:space:]]*Some\([[:space:]]*[0-9]+[[:space:]]*\)' \
        | grep -q .; then
        has_dynamic=1
    fi

    if ((${#honored[@]} > 0)); then
        CLASS_MODE=env
        CLASS_BOUND=$BOUND
        CLASS_VAR=$(IFS=,; echo "${honored[*]}")
        CLASS_EXPORT=("${honored[@]}")
        return 0
    fi

    # Builder::new reads LOOM_MAX_PREEMPTIONS. Honor it only when the harness
    # does not assign preemption_bound itself.
    local mentioned=" ${vars[*]+"${vars[*]}"} "
    if [[ $mentioned == *" LOOM_MAX_PREEMPTIONS "* ]] && ((has_none == 0 && has_dynamic == 0 && ${#literals[@]} == 0)); then
        CLASS_MODE=env
        CLASS_BOUND=$BOUND
        CLASS_VAR=LOOM_MAX_PREEMPTIONS
        CLASS_EXPORT=(LOOM_MAX_PREEMPTIONS)
        return 0
    fi

    if ((has_dynamic == 0 && has_none == 0 && ${#literals[@]} > 0)); then
        local literal_field
        literal_field=$(IFS=,; echo "${literals[*]}")
        CLASS_BOUND=$literal_field
        CLASS_VAR=fixed
        if [[ $literal_field == "$BOUND" ]]; then
            CLASS_MODE=fixed
        else
            CLASS_MODE=unsupported
        fi
        return 0
    fi

    if ((has_none == 1 && has_dynamic == 0 && ${#literals[@]} == 0)); then
        CLASS_BOUND=none
        CLASS_VAR=none
    elif ((${#vars[@]} > 0)); then
        CLASS_BOUND=unapplied
        CLASS_VAR=$(IFS=,; echo "${vars[*]}")
    fi
    CLASS_MODE=unsupported
}

rustflags_for() {
    local dir=$1
    local flags=${RUSTFLAGS-}
    if [[ -f $dir/README.md ]] && grep -q -- '--cfg loom' "$dir/README.md"; then
        case " $flags " in
            *" --cfg loom "*) ;;
            *) flags+="${flags:+ }--cfg loom" ;;
        esac
    fi
    printf '%s' "$flags"
}

run_one() {
    local rel=$1
    local dir=$root/$rel
    local manifest=$dir/Cargo.toml
    local safe=${rel//\//__}
    local log=$log_dir/$safe.log
    local flags fetch_rc run_rc status tests summary failed_n

    classify "$dir"
    echo "loom-run $rel mode=$CLASS_MODE bound=$CLASS_BOUND var=$CLASS_VAR" >&2

    local -a child_env=("CARGO_TERM_COLOR=never")
    local export_name
    for export_name in "${CLASS_EXPORT[@]+"${CLASS_EXPORT[@]}"}"; do
        child_env+=("$export_name=$BOUND")
    done
    flags=$(rustflags_for "$dir")
    if [[ -n $flags ]]; then
        child_env+=("RUSTFLAGS=$flags")
    fi

    : >"$log"
    set +e
    env -u RUST_LOG -u CARGO_TARGET_DIR \
        -u LOOM_MAX_PREEMPTIONS -u LOOM_MAX_BRANCHES -u LOOM_MAX_DURATION \
        -u LOOM_MAX_PERMUTATIONS -u LOOM_CHECKPOINT_INTERVAL -u LOOM_CHECKPOINT_FILE \
        -u LOOM_LOG \
        "${child_env[@]}" \
        cargo fetch --locked --manifest-path "$manifest" >>"$log" 2>&1
    fetch_rc=$?
    if ((fetch_rc == 0)); then
        if [[ -x $dir/run.sh ]]; then
            env -u RUST_LOG -u CARGO_TARGET_DIR \
                -u LOOM_MAX_PREEMPTIONS -u LOOM_MAX_BRANCHES -u LOOM_MAX_DURATION \
                -u LOOM_MAX_PERMUTATIONS -u LOOM_CHECKPOINT_INTERVAL -u LOOM_CHECKPOINT_FILE \
                -u LOOM_LOG \
                "${child_env[@]}" \
                "$dir/run.sh" --locked --test-threads=1 --no-fail-fast >>"$log" 2>&1
        else
            env -u RUST_LOG -u CARGO_TARGET_DIR \
                -u LOOM_MAX_PREEMPTIONS -u LOOM_MAX_BRANCHES -u LOOM_MAX_DURATION \
                -u LOOM_MAX_PERMUTATIONS -u LOOM_CHECKPOINT_INTERVAL -u LOOM_CHECKPOINT_FILE \
                -u LOOM_LOG \
                "${child_env[@]}" \
                cargo nextest run --manifest-path "$manifest" --locked --test-threads=1 --no-fail-fast >>"$log" 2>&1
        fi
        run_rc=$?
    else
        run_rc=$fetch_rc
    fi
    set -e

    summary=$(grep -E '[0-9]+ tests run:' "$log" | tail -1 || true)
    summary=$(printf '%s\n' "$summary" | sed -E 's/\x1b\[[0-9;]*[A-Za-z]//g')
    # "5/30 tests run" is a fail-fast partial. Count the tests that actually ran.
    tests=0
    if [[ $summary =~ ([0-9]+)/([0-9]+)[[:space:]]tests[[:space:]]run ]]; then
        tests=${BASH_REMATCH[1]}
    elif [[ $summary =~ ([0-9]+)[[:space:]]tests[[:space:]]run ]]; then
        tests=${BASH_REMATCH[1]}
    fi
    failed_n=0
    if [[ $summary =~ ([0-9]+)[[:space:]]failed ]]; then
        failed_n=${BASH_REMATCH[1]}
    fi

    if ((fetch_rc != 0 || run_rc != 0 || tests == 0 || failed_n > 0)); then
        status=fail
    elif [[ $CLASS_MODE == unsupported ]]; then
        status=unsupported
    else
        status=pass
    fi

    echo "loom-result $rel bound=$CLASS_BOUND var=$CLASS_VAR $status tests=$tests"
    if [[ $status == fail ]]; then
        echo "loom-log $rel $log" >&2
        tail -n 40 "$log" >&2 || true
        return 1
    fi
    if [[ $status != pass ]]; then
        return 1
    fi
}

main() {
    local -a crates=()
    local rel failed=0

    mapfile -t crates < <(discover_crates)
    echo "loom-discover count=${#crates[@]} floor=$FLOOR"
    for rel in "${crates[@]+"${crates[@]}"}"; do
        echo "loom-crate $rel"
    done
    if ((${#crates[@]} < FLOOR)); then
        echo "loom-discover FAIL count ${#crates[@]} < floor $FLOOR" >&2
        exit 1
    fi

    for rel in "${crates[@]}"; do
        if ! run_one "$rel"; then
            failed=1
        fi
    done
    exit "$failed"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
