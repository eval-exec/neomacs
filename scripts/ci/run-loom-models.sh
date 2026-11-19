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
# FLOOR. Floor is 10: tests/pentry-keeper-loom is on main (cd575451b5) and
# the tests/*loom* glob picks it up. Its bound variable is PENTRY_PREEMPTIONS.
#
# Each crate's README.md or run.sh names its preemption variable
# (*PREEMPTIONS). The script exports a variable only when that crate's Rust
# reads it, or when the crate leaves Loom's Builder value alone and the named
# variable is LOOM_MAX_PREEMPTIONS (Builder::new reads it). Those crates are
# bounded. A harness that sets preemption_bound = None and nothing else is
# exhaustive: stronger than BOUND, so the script does not force Some(BOUND).
# A harness that hardcodes a literal (Some(N), or check_bounded(..., N)) is
# native: the tests still run, and the script prints a WARN line. At a
# requested bound the literal does not cover, that line says
# "native=<list> < requested BOUND".
#
# Exit 1 only when the discovery count is under FLOOR, a crate fails to
# compile or fails a test, or a crate's README/runner and harness state no
# bound at all. Exhaustive and native passes do not fail the run. The last
# line is:
#   loom-models: N pass (B bounded, E exhaustive, V native), F fail, W warn
set -euo pipefail

readonly FLOOR=10

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
# CLASS_MODE is bounded, exhaustive, native, or unbound.
classify() {
    local dir=$1
    local -a scan=() vars=() honored=() literals=()
    local rust_blob="" token has_none=0 has_dynamic=0

    CLASS_MODE=unbound
    CLASS_BOUND=none
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

    local -a call_lits=()
    if ((${#rust_files[@]} > 0)); then
        mapfile -t call_lits < <(grep -hE 'check_bounded\(' "${rust_files[@]}" \
            | grep -oE 'check_bounded\([^,)]*,[[:space:]]*[0-9]+' \
            | grep -oE '[0-9]+$' | sort -u || true)
    fi
    local passes_none=0
    if ((${#rust_files[@]} > 0)) && grep -qE 'check_impl\([^)]*None|,[[:space:]]*None,' "${rust_files[@]}"; then
        passes_none=1
    fi

    if ((${#honored[@]} > 0)); then
        CLASS_MODE=bounded
        CLASS_BOUND=$BOUND
        CLASS_VAR=$(IFS=,; echo "${honored[*]}")
        CLASS_EXPORT=("${honored[@]}")
        return 0
    fi

    # Builder::new reads LOOM_MAX_PREEMPTIONS. Honor it only when the harness
    # does not assign preemption_bound itself.
    local mentioned=" ${vars[*]+"${vars[*]}"} "
    if [[ $mentioned == *" LOOM_MAX_PREEMPTIONS "* ]] && ((has_none == 0 && has_dynamic == 0 && ${#literals[@]} == 0 && ${#call_lits[@]} == 0)); then
        CLASS_MODE=bounded
        CLASS_BOUND=$BOUND
        CLASS_VAR=LOOM_MAX_PREEMPTIONS
        CLASS_EXPORT=(LOOM_MAX_PREEMPTIONS)
        return 0
    fi

    # Exhaustive is stronger than any requested bound. Do not force Some(BOUND).
    if ((has_none == 1 && has_dynamic == 0 && ${#literals[@]} == 0 && ${#call_lits[@]} == 0)); then
        CLASS_MODE=exhaustive
        CLASS_BOUND=none
        CLASS_VAR=none
        return 0
    fi

    # Literal bounds the environment cannot move: Some(N), or check_bounded(..., N).
    if ((has_dynamic == 1 || ${#literals[@]} > 0 || ${#call_lits[@]} > 0 || passes_none == 1)); then
        local -a native_parts=()
        if ((has_none == 1 || passes_none == 1)); then
            native_parts+=(none)
        fi
        local n numbers
        numbers=$(printf '%s\n' "${literals[@]+"${literals[@]}"}" "${call_lits[@]+"${call_lits[@]}"}" | grep -E '^[0-9]+$' | sort -u || true)
        for n in $numbers; do
            native_parts+=("$n")
        done
        CLASS_MODE=native
        if ((${#native_parts[@]} == 0)); then
            CLASS_BOUND=unapplied
        else
            CLASS_BOUND=$(IFS=,; echo "${native_parts[*]}")
        fi
        if ((${#vars[@]} > 0)); then
            CLASS_VAR=$(IFS=,; echo "${vars[*]}")
        else
            CLASS_VAR=fixed
        fi
        return 0
    fi

    # README/runner and harness state no bound at all.
    CLASS_MODE=unbound
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
    echo "loom-run $rel class=$CLASS_MODE bound=$CLASS_BOUND var=$CLASS_VAR" >&2

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
    # cargo nextest directly. A crate run.sh may pin `taskset -c 0-3,14-31`
    # (tests/pentry-keeper-loom), which is not a valid mask on a 4-vCPU
    # runner. The preemption variable is exported above; run.sh is only scanned.
    if ((fetch_rc == 0)); then
        env -u RUST_LOG -u CARGO_TARGET_DIR \
            -u LOOM_MAX_PREEMPTIONS -u LOOM_MAX_BRANCHES -u LOOM_MAX_DURATION \
            -u LOOM_MAX_PERMUTATIONS -u LOOM_CHECKPOINT_INTERVAL -u LOOM_CHECKPOINT_FILE \
            -u LOOM_LOG \
            "${child_env[@]}" \
            cargo nextest run --manifest-path "$manifest" --locked --test-threads=1 --no-fail-fast >>"$log" 2>&1
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
    elif [[ $CLASS_MODE == unbound ]]; then
        # README/runner and harness state no bound at all.
        status=fail
    else
        status=pass
    fi

    echo "loom-result $rel class=$CLASS_MODE bound=$CLASS_BOUND var=$CLASS_VAR $status tests=$tests"
    if [[ $status == pass && $CLASS_MODE == bounded ]]; then
        PASS_BOUNDED=$((PASS_BOUNDED + 1))
    elif [[ $status == pass && $CLASS_MODE == exhaustive ]]; then
        PASS_EXHAUSTIVE=$((PASS_EXHAUSTIVE + 1))
    elif [[ $status == pass && $CLASS_MODE == native ]]; then
        PASS_NATIVE=$((PASS_NATIVE + 1))
        local label msg
        case $rel in
            */host-service-loom) label=host-service ;;
            */p2ht-authority-loom) label=p2ht ;;
            *) label=${rel##*/} ;;
        esac
        msg="WARN $label native=$CLASS_BOUND"
        if [[ ",$CLASS_BOUND," != *",$BOUND,"* ]]; then
            msg+=" < requested $BOUND"
        fi
        echo "$msg"
        WARN_N=$((WARN_N + 1))
    else
        FAIL_N=$((FAIL_N + 1))
        echo "loom-log $rel $log" >&2
        tail -n 40 "$log" >&2 || true
        return 1
    fi
}

main() {
    local -a crates=()
    local rel pass

    mapfile -t crates < <(discover_crates)
    echo "loom-discover count=${#crates[@]} floor=$FLOOR"
    for rel in "${crates[@]+"${crates[@]}"}"; do
        echo "loom-crate $rel"
    done
    if ((${#crates[@]} < FLOOR)); then
        echo "loom-discover FAIL count ${#crates[@]} < floor $FLOOR" >&2
        exit 1
    fi

    PASS_BOUNDED=0
    PASS_EXHAUSTIVE=0
    PASS_NATIVE=0
    FAIL_N=0
    WARN_N=0
    for rel in "${crates[@]}"; do
        run_one "$rel" || true
    done
    pass=$((PASS_BOUNDED + PASS_EXHAUSTIVE + PASS_NATIVE))
    echo "loom-models: $pass pass ($PASS_BOUNDED bounded, $PASS_EXHAUSTIVE exhaustive, $PASS_NATIVE native), $FAIL_N fail, $WARN_N warn"
    if ((FAIL_N > 0)); then
        exit 1
    fi
    exit 0
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
