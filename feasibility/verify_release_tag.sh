#!/usr/bin/env bash
# Require a direct, annotated GitHub-verified signed tag for the exact checked-out commit.
set -euo pipefail

tag="${GITHUB_REF_NAME:?GITHUB_REF_NAME is required}"
repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GH_TOKEN:?GH_TOKEN is required}"

[[ "$tag" =~ ^rust-dotnet-v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    echo "invalid rust-dotnet release tag: $tag" >&2
    exit 2
}
tag_ref="refs/tags/$tag"
[[ "$(git cat-file -t "$tag_ref")" == tag ]] || {
    echo "release ref is not an annotated tag: $tag" >&2
    exit 2
}
tag_object="$(git rev-parse "$tag_ref^{tag}")"
tag_commit="$(git rev-parse "$tag_ref^{commit}")"
head_commit="$(git rev-parse HEAD)"
[[ "$tag_commit" == "$head_commit" ]] || {
    echo "release tag resolves to $tag_commit, but checkout HEAD is $head_commit" >&2
    exit 2
}

verification="$(gh api "repos/$repository/git/tags/$tag_object")"
jq -e --arg tag "$tag" --arg head "$head_commit" '
    .tag == $tag
    and .object.type == "commit"
    and .object.sha == $head
    and .verification.verified == true
    and .verification.reason == "valid"
' <<< "$verification" >/dev/null || {
    echo "GitHub did not verify a valid signature on annotated tag $tag" >&2
    exit 2
}

echo "verified signed release tag $tag -> $head_commit"
