#!/usr/bin/env bash
set -euo pipefail

source_tag="${1:?source model Release tag is required}"
release_tag="${2:?target model Release tag is required}"
model_version="${3:?model version is required}"
target_commit="${4:?target Core commit is required}"
repository="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

for value in "$source_tag" "$release_tag" "$model_version" "$target_commit"; do
  if [[ ! "$value" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Unsafe release parameter: $value" >&2
    exit 2
  fi
done

root="$(git rev-parse --show-toplevel)"
work="$root/target/lsmodel-release"
downloads="$work/downloads"
unpacked="$work/unpacked"
output="$work/output"
cli="$root/target/release/snipper"
catalog="$root/scripts/model-manifest.template.json"

rm -rf "$work"
mkdir -p "$downloads" "$unpacked" "$output"

gh release download "$source_tag" \
  --repo "$repository" \
  --dir "$downloads" \
  --pattern 'latexsnipper-*.zip' \
  --pattern 'SHA256SUMS'

(
  cd "$downloads"
  sha256sum --check SHA256SUMS
)

cargo build --locked --release -p latexsnipper-cli

while IFS=$'\t' read -r category variant legacy_asset; do
  legacy_archive="$downloads/$legacy_asset"
  if [[ ! -f "$legacy_archive" ]]; then
    echo "Missing source Release asset: $legacy_asset" >&2
    exit 3
  fi
  package_root="$unpacked/${category}-${variant}"
  mkdir -p "$package_root"
  unzip -q "$legacy_archive" -d "$package_root"
  mapfile -t configs < <(find "$package_root" -type f -name config.json -print)
  if [[ "${#configs[@]}" -ne 1 ]]; then
    echo "Expected one config.json in $legacy_asset, found ${#configs[@]}" >&2
    printf '%s\n' "${configs[@]}" >&2
    exit 4
  fi
  source_directory="$(dirname "${configs[0]}")"
  target_asset="${legacy_asset%.zip}.lsmodel"
  "$cli" models package \
    --source "$source_directory" \
    --output "$output/$target_asset" \
    --catalog "$catalog" \
    --category "$category" \
    --variant "$variant" \
    --model-version "$model_version"
  "$cli" models inspect "$output/$target_asset"
done < <(
  jq -r '
    .categories
    | to_entries[] as $category
    | $category.value.variants[]
    | [$category.key, .id, .zipFile]
    | @tsv
  ' "$catalog"
)

find "$output" -maxdepth 1 -type f -name '*.lsmodel' -print0 \
  | sort -z \
  | xargs -0 sha256sum \
  | sed "s#  $output/#  #" > "$work/model-checksums.txt"
jq -Rn '
  [inputs
   | capture("^(?<sha>[0-9a-f]{64})  (?<name>.+)$")
   | {key: .name, value: .sha}]
  | from_entries
' < "$work/model-checksums.txt" > "$work/model-checksums.json"

jq \
  --arg version "$model_version" \
  --arg tag "$release_tag" \
  --arg commit "$target_commit" \
  --slurpfile checks "$work/model-checksums.json" '
    .version = $version
    | .baseUrl = ("https://github.com/" + env.GITHUB_REPOSITORY + "/releases/download/" + $tag)
    | .sourceCommit = $commit
    | .checksums = $checks[0]
    | .categories[].variants[].zipFile |= sub("\\.zip$"; ".lsmodel")
  ' "$catalog" > "$output/model-manifest.json"

(
  cd "$output"
  sha256sum -- *.lsmodel model-manifest.json > SHA256SUMS
)

jq -Rn \
  --arg sourceTag "$source_tag" \
  --arg releaseTag "$release_tag" \
  --arg modelVersion "$model_version" \
  --arg targetCommit "$target_commit" '
    {
      schemaVersion: 1,
      transportVersion: 1,
      sourceTag: $sourceTag,
      releaseTag: $releaseTag,
      modelVersion: $modelVersion,
      targetCommit: $targetCommit,
      assets: [inputs
        | capture("^(?<sha>[0-9a-f]{64})  (?<name>.+)$")
        | {name: .name, sha256: .sha}]
    }
  ' < "$output/SHA256SUMS" > "$output/release-provenance.json"

if gh release view "$release_tag" --repo "$repository" >/dev/null 2>&1; then
  existing_commit="$(gh api "repos/$repository/git/ref/tags/$release_tag" --jq '.object.sha')"
  if [[ "$existing_commit" != "$target_commit" ]]; then
    echo "Refusing to replace Release $release_tag bound to $existing_commit" >&2
    exit 5
  fi
else
  gh release create "$release_tag" \
    --repo "$repository" \
    --target "$target_commit" \
    --title "LaTeXSnipper Models $model_version (.lsmodel v1)" \
    --notes "Runtime-ready model packages using .lsmodel transport v1. Every package has manifest.toml at the ZIP root and is bound to Core commit $target_commit."
fi

mapfile -t assets < <(find "$output" -maxdepth 1 -type f -printf '%p\n' | sort)
gh release upload "$release_tag" "${assets[@]}" --repo "$repository" --clobber

expected_names="$(printf '%s\n' "${assets[@]##*/}" | sort)"
for attempt in 1 2 3 4 5; do
  actual_names="$(gh release view "$release_tag" --repo "$repository" --json assets --jq '.assets[].name' | sort)"
  if [[ "$actual_names" == "$expected_names" ]]; then
    break
  fi
  if [[ "$attempt" -eq 5 ]]; then
    echo "Release asset set does not match the generated output" >&2
    diff -u <(printf '%s\n' "$expected_names") <(printf '%s\n' "$actual_names") || true
    exit 6
  fi
  sleep 3
done

for local_asset in "${assets[@]}"; do
  expected_sha="$(sha256sum "$local_asset" | cut -d' ' -f1)"
  asset_name="$(basename "$local_asset")"
  api_digest="$(gh release view "$release_tag" --repo "$repository" --json assets \
    --jq ".assets[] | select(.name == \"$asset_name\") | .digest")"
  if [[ "$api_digest" != "sha256:$expected_sha" ]]; then
    echo "Digest mismatch for $asset_name: expected sha256:$expected_sha, API returned $api_digest" >&2
    exit 7
  fi
done

echo "Published and verified $release_tag at $target_commit"
