#!/usr/bin/env bash
# Serialized end-to-end compatibility gate for the ecosystem and managed-library
# cases that exercise the rearchitecture's affected surfaces.
set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
canonical_repo="$(cd "$repo" && pwd -P)"
if command -v shasum >/dev/null 2>&1; then
    repo_namespace="$(printf '%s' "$canonical_repo" | shasum -a 256 | awk '{ print $1 }')"
else
    repo_namespace="$(printf '%s' "$canonical_repo" | sha256sum | awk '{ print $1 }')"
fi
default_tmp_root="/tmp/rustc_codegen_clr-matrix-$repo_namespace"
driver="$repo/target/release/cargo-dotnet"
summary="${RCL_MATRIX_SUMMARY:-$default_tmp_root/e2e-matrix.tsv}"
log_dir="${RCL_MATRIX_LOG_DIR:-$default_tmp_root/e2e-matrix-logs}"
native_target_root="${RCL_MATRIX_NATIVE_TARGET_ROOT:-$default_tmp_root/native-targets}"
dotnet_target_root="${RCL_MATRIX_DOTNET_TARGET_ROOT:-$default_tmp_root/dotnet-targets}"
read -r -a profiles <<< "${RCL_MATRIX_PROFILES:-release debug}"
case_filter="${RCL_MATRIX_CASES:-}"
dotnet_version="${DOTNET_VERSION:-10}"
matrix_cache="${RCL_MATRIX_CACHE:-${RCL_MATRIX_KEEP_TARGETS:-1}}"
cold_matrix="${RCL_MATRIX_COLD:-0}"

case "$matrix_cache:$cold_matrix" in
    0:0|1:0|1:1) ;;
    0:1)
        echo "RCL_MATRIX_COLD=1 requires RCL_MATRIX_CACHE=1" >&2
        exit 2
        ;;
    *)
        echo "RCL_MATRIX_CACHE and RCL_MATRIX_COLD must each be 0 or 1" >&2
        exit 2
        ;;
esac

if [[ ! -x "$driver" ]]; then
    echo "cargo-dotnet driver is missing; build it with:" >&2
    echo "  cargo build --manifest-path tools/cargo-dotnet/Cargo.toml --release" >&2
    exit 2
fi

# One matrix owns fixture configs, logs, cache retention, and managed-host targets at a time. The
# owner token prevents an interrupted process from deleting a lock that was replaced manually;
# stale locks are deliberately never guessed at or auto-removed.
matrix_lock="$repo/target/.rcl-e2e-matrix.lock"
matrix_lock_token="pid=$$ nonce=${RANDOM:-0} started=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
mkdir -p "$repo/target"
if ! mkdir "$matrix_lock" 2>/dev/null; then
    echo "another e2e matrix owns $matrix_lock; inspect its owner file instead of removing it automatically" >&2
    exit 2
fi
printf '%s\n' "$matrix_lock_token" > "$matrix_lock/owner"
matrix_cache_area=''
matrix_cache_state='disabled'
cold_parent=''

matrix_exit_cleanup() {
    local status=$? owner=''
    if [[ "$matrix_cache_state" == cold && -n "$matrix_cache_area" \
        && ! -L "$matrix_cache_area" \
        && "$matrix_cache_area" == "$dotnet_target_root/cold/"* \
        && "$(basename "$matrix_cache_area")" == run.* ]]; then
        rm -rf "$matrix_cache_area"
        [[ -n "$cold_parent" ]] && rmdir "$cold_parent" 2>/dev/null || true
    fi
    if [[ -f "$matrix_lock/owner" ]]; then
        IFS= read -r owner < "$matrix_lock/owner" || true
    fi
    if [[ "$owner" == "$matrix_lock_token" ]]; then
        rm -rf "$matrix_lock"
    else
        echo "matrix lock ownership changed; refusing cleanup of $matrix_lock" >&2
    fi
    return "$status"
}
trap matrix_exit_cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# Ordinary Rust ecosystem probes have a stronger oracle than a marker: their
# stdout and exit status must exactly match the same crate built natively.
native_diff_cases=(
    soak_ahash soak_aho-corasick soak_anyhow soak_arrayvec soak_base64 soak_bincode
    soak_bitflags soak_blake3 soak_bytes soak_chrono soak_csv soak_fastrand soak_fxhash
    soak_indexmap soak_itertools soak_num_bigint soak_num-rational soak_regex soak_serde_json
    soak_sha2 soak_smallvec soak_strsim soak_time soak_url soak_uuid
    survey_cgmath survey_chacha20 survey_dashmap survey_futures-lite survey_glam
    survey_hashbrown survey_jiff survey_json5 survey_lru survey_nalgebra survey_ndarray
    survey_rayon survey_regex-automata survey_serde_with survey_smol
)

# Managed-only probes cannot be compiled natively. Each must print its exact
# completion marker only after its internal assertions/pass-count checks succeed.
managed_selfcheck_cases=(
    cd_bcl cd_collections cd_decimal cd_span cd_vtgen cd_gmethod cd_linq cd_sync cd_idiomatic
    cd_enumerate cd_json cd_net10_bcl
    cd_delegates cd_async cd_async_stream cd_generic cd_fatptr cd_fpfam cd_dynamic_invoke cd_pure
    cd_persisted_async cd_channel cd_tokio cd_static_field_offset cd_subword_atomics
    cd_htmlagility cd_linq_expr cd_efcore cd_linq_groupby cd_pdb cd_pdfsharp
)

# These PAL probes are materially slower than the ordinary managed selfchecks. Keep them out of
# the default matrix, but make each one available through an exact RCL_MATRIX_CASES name. Once
# selected they use the same zero-exit/no-diagnostics/completion-marker oracle as every other
# managed-only probe.
pal_managed_selfcheck_cases=(
    pal_fs pal_fsmeta pal_net pal_threads pal_probe pal_panic2 pal_async pal_tokio pal_tokio_net
)

managed_selfcheck_run_cases=("${managed_selfcheck_cases[@]}")

# This crate is a cdylib. `cargo dotnet run` cannot execute it; its C# consumer
# is the oracle and must be run explicitly after building the Rust assembly.
managed_host_cases=(cd_export_ergonomics)

diagnostics='(^error(\[|:)|compiler unexpectedly panicked|could not compile item|panicked at|final post-link verification failed|verification failed|unsupported inline assembly|unsupported operation|miscompilation|fatal error|^unhandled exception|^process terminated|warning: allocation requires alignment)'

remove_generated_config() {
    local case_dir="$1"
    local config="$case_dir/.cargo/config.toml"
    local first_line relative
    [[ -f "$config" ]] || return 0
    IFS= read -r first_line < "$config" || true
    case "$first_line" in
        '# GENERATED by cargo dotnet (overlays::apply) — do not hand-edit.'|'# GENERATED by feasibility/cargo-dotnet (apply_overlays) — do not hand-edit.'|'# GENERATED by feasibility/dev.sh apply_overlays — do not hand-edit.')
            rm -f "$config"
            ;;
        *)
            # Several historical PAL fixtures intentionally track a Cargo config for the Docker
            # `/work` mount model. The native driver layers its private explicit config above it,
            # so a successful build has already proven it did not control the target. Preserve any
            # tracked config exactly; cleanup authority applies only to known generated headers.
            if [[ "$config" == "$repo/"* ]]; then
                relative="${config#"$repo/"}"
                if git -C "$repo" ls-files --error-unmatch -- "$relative" >/dev/null 2>&1; then
                    return 0
                fi
            fi
            echo "refusing to remove unrecognized Cargo config: $config" >&2
            return 1
            ;;
    esac
}

profile_args() {
    case "$1" in
        release) printf '%s\n' --release ;;
        debug) printf '%s\n' --debug ;;
        *) echo "unsupported matrix profile: $1" >&2; return 2 ;;
    esac
}

case_selected() {
    [[ -z "$case_filter" || " $case_filter " == *" $1 "* ]]
}

if [[ -n "$case_filter" ]]; then
    read -r -a requested_cases <<< "$case_filter"
    known_cases=(
        "${native_diff_cases[@]}"
        "${managed_selfcheck_cases[@]}"
        "${pal_managed_selfcheck_cases[@]}"
        "${managed_host_cases[@]}"
    )
    for requested_case in "${requested_cases[@]}"; do
        requested_case_known=0
        for known_case in "${known_cases[@]}"; do
            if [[ "$requested_case" == "$known_case" ]]; then
                requested_case_known=1
                break
            fi
        done
        if [[ "$requested_case_known" != 1 ]]; then
            echo "unknown RCL_MATRIX_CASES entry: $requested_case" >&2
            echo "select exact case names from: ${known_cases[*]}" >&2
            exit 2
        fi
    done
    if ((${#requested_cases[@]} > 0)); then
        managed_selfcheck_run_cases+=("${pal_managed_selfcheck_cases[@]}")
    fi
fi

native_profile_args() {
    [[ "$1" == release ]] && printf '%s\n' --release
    return 0
}

file_sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    else
        sha256sum "$1" | awk '{ print $1 }'
    fi
}

stdin_sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 | awk '{ print $1 }'
    else
        sha256sum | awk '{ print $1 }'
    fi
}

tree_sha256() {
    local root="$1"
    [[ -d "$root" ]] || {
        echo "matrix producer tree is missing: $root" >&2
        return 1
    }
    (
        cd "$root" || exit 1
        # These SDK source trees use ordinary repository paths (no newlines). Bind each sorted
        # relative path and its file digest so edits, additions, removals, and renames all change
        # the matrix namespace without depending on mtimes or the checkout's absolute location.
        find . -type f -print | LC_ALL=C sort | while IFS= read -r relative; do
            printf '%s\0' "$relative"
            file_sha256 "$relative"
        done
    ) | stdin_sha256
}

# Cargo correctly separates profiles in one consumer's target directory, but it does not hash the
# bytes of a codegen backend named by an otherwise-stable -Zcodegen-backend path. Bind reuse to all
# producer binaries plus the host and target contract so rebuilt backend bytes can never inherit an
# older artifact. Keep consumers separate because each has a distinct Cargo home, remap flags,
# package graph, and final output namespace. Cargo remains authoritative for source/config changes.
matrix_provenance_material() {
    local host_os host_arch backend linker driver_path rustc_identity material pal_tree overlays_tree
    host_os="$(uname -s)"
    host_arch="$(uname -m)"
    driver_path="$driver"
    linker="$repo/target/release/linker"
    local target_spec="$repo/x86_64-unknown-dotnet.json"
    case "$host_os" in
        Darwin) backend="$repo/target/release/librustc_codegen_clr.dylib" ;;
        Linux) backend="$repo/target/release/librustc_codegen_clr.so" ;;
        MINGW*|MSYS*|CYGWIN*)
            backend="$repo/target/release/rustc_codegen_clr.dll"
            driver_path="$driver.exe"
            linker="$linker.exe"
            ;;
        *)
            echo "unsupported matrix cache host: $host_os" >&2
            return 1
            ;;
    esac
    for path in "$driver_path" "$backend" "$linker" "$target_spec"; do
        if [[ -z "$path" || ! -f "$path" ]]; then
            echo "matrix producer input is missing: $path" >&2
            return 1
        fi
    done
    rustc_identity="$(rustc +nightly-2026-09-18 -Vv)" || return 1
    pal_tree="$(tree_sha256 "$repo/dotnet_pal")" || return 1
    overlays_tree="$(tree_sha256 "$repo/dotnet_overlays")" || return 1
    material="schema=1
host_os=$host_os
host_arch=$host_arch
dotnet=$dotnet_version
toolchain=nightly-2026-09-18
rustc=$rustc_identity
driver=$(file_sha256 "$driver_path")
backend=$(file_sha256 "$backend")
linker=$(file_sha256 "$linker")
target_spec=$(file_sha256 "$target_spec")
pal_tree=$pal_tree
overlays_tree=$overlays_tree"
    printf '%s\n' "$material"
}

matrix_material="$(matrix_provenance_material)" || exit 2
matrix_key="$(printf '%s' "$matrix_material" | stdin_sha256)"

validated_warm_root() {
    local candidate="$1" name stored stored_key
    name="$(basename "$candidate")"
    [[ ! -L "$candidate" && -d "$candidate" && "$name" =~ ^[0-9a-f]{64}$ ]] || return 1
    [[ -f "$candidate/.rcl-matrix-provenance" ]] || return 1
    stored="$(< "$candidate/.rcl-matrix-provenance")" || return 1
    stored_key="$(printf '%s' "$stored" | stdin_sha256)"
    [[ "$stored_key" == "$name" ]]
}

# Retain the current producer plus only the most recently used prior validated producer. Invalid
# or unexpected entries are never guessed at or removed automatically.
prune_warm_roots() {
    local warm_parent="$1" current="$2" candidate retained=''
    while IFS= read -r candidate; do
        [[ "$candidate" == "$current" ]] && continue
        if ! validated_warm_root "$candidate"; then
            echo "leaving unvalidated matrix cache entry untouched: $candidate" >&2
            continue
        fi
        if [[ -z "$retained" ]]; then
            retained="$candidate"
        elif [[ "$candidate/.rcl-matrix-provenance" -nt "$retained/.rcl-matrix-provenance" ]]; then
            rm -rf "$retained"
            retained="$candidate"
        else
            rm -rf "$candidate"
        fi
    done < <(find "$warm_parent" -mindepth 1 -maxdepth 1 -type d -print)
}

matrix_cache_state='disabled'
matrix_cache_area='default-consumer-targets'
if [[ "$matrix_cache" == 1 ]]; then
    if [[ "$cold_matrix" == 1 ]]; then
        matrix_cache_state='cold'
        cold_parent="$dotnet_target_root/cold/$matrix_key"
        mkdir -p "$cold_parent"
        matrix_cache_area="$(mktemp -d "$cold_parent/run.XXXXXX")" || exit 2
    else
        matrix_cache_state='persistent'
        matrix_cache_area="$dotnet_target_root/warm/$matrix_key"
    fi
    if [[ -L "$matrix_cache_area" ]]; then
        echo "refusing symlinked matrix cache root: $matrix_cache_area" >&2
        exit 2
    fi
    mkdir -p "$matrix_cache_area"
    printf '%s\n' "$matrix_material" > "$matrix_cache_area/.rcl-matrix-provenance"
    if [[ "$matrix_cache_state" == persistent ]]; then
        prune_warm_roots "$dotnet_target_root/warm" "$matrix_cache_area"
    fi
fi

mkdir -p "$log_dir" "$native_target_root" "$(dirname "$summary")"
producer_manifest="$log_dir/producer-provenance.start"
printf '%s\n' "$matrix_material" > "$producer_manifest"
echo "dotnet cache: $matrix_cache_state ($matrix_cache_area)"
acceptance_receipt="${summary%.*}.receipt.json"
rm -f "$acceptance_receipt"
printf 'kind|dotnet|profile|case|dotnet_exit|native_exit|stdout_match|diagnostic_hits|marker|required|result|receipt\n' > "$summary"
overall=0
index=0
selected_count=0
for candidate in "${native_diff_cases[@]}" "${managed_selfcheck_run_cases[@]}" "${managed_host_cases[@]}"; do
    case_selected "$candidate" && selected_count=$((selected_count + 1))
done
total=$((selected_count * ${#profiles[@]}))

run_native_diff() {
    local case_name="$1" profile="$2" clean_flag="$3" target_dir="$4"
    local case_dir="$repo/cargo_tests/$case_name"
    local prefix="$log_dir/$profile-$case_name"
    local dotnet_exit native_exit hits stdout_match marker dotnet_profile native_profile row_result

    remove_generated_config "$case_dir" || return 1
    dotnet_profile="$(profile_args "$profile")" || return 1
    native_profile="$(native_profile_args "$profile")"

    env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_TARGET \
        -u DOTNET_VERSION \
        CARGO_TARGET_DIR="$native_target_root/$case_name/$profile" \
        cargo +nightly-2026-09-18 run --manifest-path "$case_dir/Cargo.toml" \
        $native_profile --quiet > "$prefix.native.stdout" 2> "$prefix.native.stderr"
    native_exit=$?

    if [[ -n "$target_dir" ]]; then
        RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native \
            "$driver" dotnet run "$case_dir" "$dotnet_profile" --dotnet "$dotnet_version" \
            --target-dir "$target_dir" $clean_flag \
            > "$prefix.dotnet.stdout" 2> "$prefix.dotnet.stderr"
    else
        RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native \
            "$driver" dotnet run "$case_dir" "$dotnet_profile" --dotnet "$dotnet_version" \
            $clean_flag \
            > "$prefix.dotnet.stdout" 2> "$prefix.dotnet.stderr"
    fi
    dotnet_exit=$?
    remove_generated_config "$case_dir" || return 1

    tr -d '\r' < "$prefix.native.stdout" > "$prefix.native.normalized"
    tr -d '\r' < "$prefix.dotnet.stdout" > "$prefix.dotnet.raw-normalized"
    # The linker invokes MSBuild helpers before the managed program and their standard
    # "Build succeeded" banner is tool output, not program output. Anchor the managed stream at
    # the native program's first non-empty line so the differential oracle compares only the two
    # executions while preserving every subsequent byte and line.
    first_native_line="$(awk 'NF { print; exit }' "$prefix.native.normalized")"
    if [[ -n "$first_native_line" ]]; then
        awk -v first="$first_native_line" '$0 == first { seen=1 } seen' \
            "$prefix.dotnet.raw-normalized" > "$prefix.dotnet.normalized"
    else
        cp "$prefix.dotnet.raw-normalized" "$prefix.dotnet.normalized"
    fi
    if cmp -s "$prefix.native.normalized" "$prefix.dotnet.normalized"; then
        stdout_match=yes
    else
        stdout_match=no
        diff -u "$prefix.native.normalized" "$prefix.dotnet.normalized" > "$prefix.stdout.diff" || true
    fi
    hits="$(rg -n -i "$diagnostics" "$prefix.dotnet.stdout" "$prefix.dotnet.stderr" 2>/dev/null | wc -l | tr -d ' ')"
    if rg -q "== $case_name done ==" "$prefix.dotnet.stdout"; then marker=yes; else marker=no; fi
    if ((dotnet_exit == 0 && native_exit == 0 && hits == 0)) && [[ "$stdout_match" == yes ]]; then
        row_result=PASS
    else
        row_result=FAIL
    fi
    printf 'native_diff|%s|%s|%s|%d|%d|%s|%s|%s|no|%s|\n' \
        "$dotnet_version" "$profile" "$case_name" "$dotnet_exit" "$native_exit" \
        "$stdout_match" "$hits" "$marker" "$row_result" >> "$summary"
    [[ "$row_result" == PASS ]]
}

run_managed_selfcheck() {
    local case_name="$1" profile="$2" clean_flag="$3" target_dir="$4"
    local case_dir="$repo/cargo_tests/$case_name"
    local prefix="$log_dir/$profile-$case_name"
    local dotnet_exit hits marker dotnet_profile row_result
    dotnet_profile="$(profile_args "$profile")" || return 1

    if [[ -n "$target_dir" ]]; then
        RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native \
            "$driver" dotnet run "$case_dir" "$dotnet_profile" --dotnet "$dotnet_version" \
            --target-dir "$target_dir" $clean_flag \
            > "$prefix.dotnet.stdout" 2> "$prefix.dotnet.stderr"
    else
        RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native \
            "$driver" dotnet run "$case_dir" "$dotnet_profile" --dotnet "$dotnet_version" \
            $clean_flag \
            > "$prefix.dotnet.stdout" 2> "$prefix.dotnet.stderr"
    fi
    dotnet_exit=$?
    hits="$(rg -n -i "$diagnostics" "$prefix.dotnet.stdout" "$prefix.dotnet.stderr" 2>/dev/null | wc -l | tr -d ' ')"
    if rg -q "== $case_name done ==" "$prefix.dotnet.stdout"; then marker=yes; else marker=no; fi
    if ((dotnet_exit == 0 && hits == 0)) && [[ "$marker" == yes ]]; then
        row_result=PASS
    else
        row_result=FAIL
    fi
    printf 'managed_selfcheck|%s|%s|%s|%d|na|na|%s|%s|yes|%s|\n' \
        "$dotnet_version" "$profile" "$case_name" "$dotnet_exit" "$hits" "$marker" \
        "$row_result" >> "$summary"
    [[ "$row_result" == PASS ]]
}

run_managed_host() {
    local case_name="$1" profile="$2" clean_flag="$3" _target_dir="$4"
    local case_dir="$repo/cargo_tests/$case_name"
    local prefix="$log_dir/$profile-$case_name"
    local dotnet_exit hits marker dotnet_profile row_result receipt rust_dll csharp_host
    dotnet_profile="$(profile_args "$profile")" || return 1

    RCL_ICE_LOG=1 CARGO_DOTNET_BACKEND=native \
        "$driver" dotnet build "$case_dir" "$dotnet_profile" --dotnet "$dotnet_version" \
        --source-link-url 'https://example.invalid/rust-dotnet-fixture/*' \
        $clean_flag \
        > "$prefix.build.stdout" 2> "$prefix.build.stderr"
    dotnet_exit=$?
    if ((dotnet_exit == 0)); then
        RustDotnetVersion="$dotnet_version" RustCheckoutPath="$repo" \
            dotnet run --project "$case_dir/csharp/cd_export_ergonomics_cs.csproj" \
            --property:RustProfile="$profile" > "$prefix.dotnet.stdout" 2> "$prefix.dotnet.stderr"
        dotnet_exit=$?
    else
        : > "$prefix.dotnet.stdout"
        : > "$prefix.dotnet.stderr"
    fi
    hits="$(rg -n -i "$diagnostics" "$prefix.build.stdout" "$prefix.build.stderr" \
        "$prefix.dotnet.stdout" "$prefix.dotnet.stderr" 2>/dev/null | wc -l | tr -d ' ')"
    if rg -q "== $case_name done ==" "$prefix.dotnet.stdout"; then marker=yes; else marker=no; fi
    if ((dotnet_exit == 0 && hits == 0)) && [[ "$marker" == yes ]]; then
        row_result=PASS
    else
        row_result=FAIL
    fi
    receipt=''
    if [[ "$row_result" == PASS ]]; then
        receipt="$(dirname "$summary")/${case_name}-${profile}.artifacts.json"
        rust_dll="$case_dir/target/x86_64-unknown-dotnet/$profile/cd_export_ergonomics.dll"
        csharp_host="$case_dir/csharp/bin/Debug/net${dotnet_version}.0/cd_export_ergonomics_cs.dll"
        bash "$repo/feasibility/write_acceptance_artifact_receipt.sh" \
            "$receipt" "$case_name" managed_host "$dotnet_version" "$profile" \
            "rust_dll=$rust_dll" "csharp_host=$csharp_host"
    fi
    printf 'managed_host|%s|%s|%s|%d|na|na|%s|%s|yes|%s|%s\n' \
        "$dotnet_version" "$profile" "$case_name" "$dotnet_exit" "$hits" "$marker" \
        "$row_result" "$(basename "$receipt")" >> "$summary"
    [[ "$row_result" == PASS ]]
}

for kind in native_diff managed_selfcheck managed_host; do
    case "$kind" in
        native_diff) cases=("${native_diff_cases[@]}") ;;
        managed_selfcheck) cases=("${managed_selfcheck_run_cases[@]}") ;;
        managed_host) cases=("${managed_host_cases[@]}") ;;
    esac
    for case_name in "${cases[@]}"; do
        case_selected "$case_name" || continue
        first_profile=yes
        for profile in "${profiles[@]}"; do
            index=$((index + 1))
            clean_flag=''
            target_dir=''
            if [[ "$matrix_cache" == 1 && "$kind" != managed_host ]]; then
                target_dir="$matrix_cache_area/$case_name"
            elif [[ "$first_profile" == yes ]]; then
                clean_flag=--clean
            fi
            printf '[%d/%d] START %s %s (%s)\n' "$index" "$total" "$profile" "$case_name" "$kind"
            if "run_$kind" "$case_name" "$profile" "$clean_flag" "$target_dir"; then
                result=PASS
            else
                result=FAIL
                overall=1
            fi
            printf '[%d/%d] END %s %s %s\n' "$index" "$total" "$profile" "$case_name" "$result"
            first_profile=no
        done
        remove_generated_config "$repo/cargo_tests/$case_name" || overall=1
        if [[ "$matrix_cache" != 1 || "$kind" == managed_host ]]; then
            rm -rf "$repo/cargo_tests/$case_name/target"
        fi
    done
done

final_matrix_material="$(matrix_provenance_material)"
final_material_status=$?
printf '%s\n' "$final_matrix_material" > "$log_dir/producer-provenance.final"
producer_stable=1
if ((final_material_status != 0)) || [[ "$final_matrix_material" != "$matrix_material" ]]; then
    echo "matrix producer provenance changed while the matrix was running; refusing passing evidence" >&2
    producer_stable=0
    overall=1
    if [[ "$matrix_cache_state" == persistent \
        && "$matrix_cache_area" == "$dotnet_target_root/warm/$matrix_key" ]]; then
        if validated_warm_root "$matrix_cache_area"; then
            rm -rf "$matrix_cache_area"
            echo "deleted the exact persistent cache root built under changed producer provenance" >&2
        fi
    fi
fi

echo "summary: $summary"
echo "logs: $log_dir"
capability_args=(
    capabilities --manifest "$repo/acceptance/capabilities.toml"
    --results "$summary" --output "${summary%.*}.capabilities.md"
    --evidence-scope "${RCL_MATRIX_CAPABILITY_SCOPE:-presubmit}"
)
if [[ "${RCL_MATRIX_STRICT_CAPABILITIES:-0}" == 1 ]]; then
    capability_args+=(--strict)
fi
"$driver" "${capability_args[@]}" || overall=1
if [[ "$producer_stable" == 1 ]]; then
    RCL_MATRIX_COMMAND="DOTNET_VERSION=$dotnet_version RCL_MATRIX_PROFILES=${RCL_MATRIX_PROFILES:-release debug} RCL_MATRIX_CASES=${RCL_MATRIX_CASES:-<all>} RCL_MATRIX_CACHE=$matrix_cache RCL_MATRIX_CACHE_STATE=$matrix_cache_state RCL_MATRIX_COLD=$cold_matrix RCL_MATRIX_PRODUCER_SHA256=$matrix_key RCL_MATRIX_PRODUCER_MANIFEST=$producer_manifest RCL_MATRIX_CACHE_AREA=$matrix_cache_area feasibility/e2e_matrix.sh" \
        "$repo/feasibility/write_acceptance_receipt.sh" \
        "$summary" "$acceptance_receipt" "$log_dir" || overall=1
else
    echo "receipt: omitted because producer provenance changed" >&2
fi
exit "$overall"
