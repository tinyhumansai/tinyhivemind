#!/usr/bin/env bash
set -euo pipefail

minimum="${1:-90}"
report="${2:-coverage.json}"
workspace_root="$(pwd -P)/"
# Every crate lives under `crates/<package>/src/`, so one prefix covers the
# whole workspace. Vendored submodules and `worktrees/` sit outside it and are
# excluded by the same test.
source_root="${workspace_root}crates/"

cargo llvm-cov \
  --locked \
  --workspace \
  --all-targets \
  --all-features \
  --json \
  --output-path "$report"

covered_files="$(jq --arg source_root "$source_root" '
  [
    .data[].files[]
    | select(.filename | startswith($source_root))
    | select(.summary.lines.count > 0)
  ]
  | length
' "$report")"

if [[ "$covered_files" -eq 0 ]]; then
  echo "coverage report contains no files with executable lines under crates/" >&2
  exit 1
fi

summary="$(jq -r --arg workspace_root "$workspace_root" --arg source_root "$source_root" '
  .data[].files[]
  | select(.filename | startswith($source_root))
  | select(.summary.lines.count > 0)
  | [
      (.filename | ltrimstr($workspace_root)),
      (.summary.lines.percent | tostring),
      (.summary.lines.covered | tostring),
      (.summary.lines.count | tostring)
    ]
  | @tsv
' "$report")"

printf 'File\tLine coverage\tCovered lines\tCoverable lines\n'
while IFS=$'\t' read -r file percent covered count; do
  printf '%s\t%.2f%%\t%s\t%s\n' "$file" "$percent" "$covered" "$count"
done <<< "$summary"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    printf '### Per-file line coverage\n\n'
    printf '| File | Coverage | Lines |\n'
    printf '| --- | ---: | ---: |\n'
    while IFS=$'\t' read -r file percent covered count; do
      printf '| %s | %.2f%% | %s/%s |\n' "$file" "$percent" "$covered" "$count"
    done <<< "$summary"
  } >> "$GITHUB_STEP_SUMMARY"
fi

failures="$(jq -r \
  --arg workspace_root "$workspace_root" \
  --arg source_root "$source_root" \
  --argjson minimum "$minimum" '
    .data[].files[]
    | select(.filename | startswith($source_root))
    | select(.summary.lines.count > 0)
    | select(.summary.lines.percent < $minimum)
    | "\(.filename | ltrimstr($workspace_root)): \(.summary.lines.percent)%"
  ' "$report")"

if [[ -n "$failures" ]]; then
  printf '\nFiles below %s%% line coverage:\n%s\n' "$minimum" "$failures" >&2
  exit 1
fi
