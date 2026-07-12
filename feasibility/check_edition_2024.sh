#!/usr/bin/env bash
# Keep the backend, tools, overlays, fixtures, and archived patch manifests on
# the same language edition. Workspace-only manifests have no [package] table
# and are intentionally excluded from the package count.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

checked=0
failed=0
while IFS= read -r manifest; do
    rg -q '^\[package\]$' "$manifest" || continue
    checked=$((checked + 1))
    if ! rg -q '^[[:space:]]*edition[[:space:]]*=[[:space:]]*"2024"' "$manifest"; then
        echo "package manifest is not Edition 2024: $manifest" >&2
        failed=1
    fi
done < <(rg --files -g 'Cargo.toml' -g 'Cargo.toml.orig' | sort)

if rg -n --glob '*.rs' --glob '*.sh' --glob '*.yml' --glob '*.yaml' \
    -- '--edition(?:=|[[:space:]])20(15|18|21)' .; then
    echo "found a hard-coded pre-2024 rustc/cargo edition" >&2
    failed=1
fi

((failed == 0)) || exit 1
echo "Edition 2024 audit passed: $checked package manifests"
