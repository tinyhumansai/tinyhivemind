#!/usr/bin/env bash
# Assert that the pure crates stay pure.
#
# `tinyhivemind-core` is linked into the hot path of every agent turn and must
# compile in a host's default build with no feature flags. That promise is
# invisible in a diff, because a forbidden dependency arrives transitively
# through a feature someone enabled one crate away — so it is asserted rather
# than documented.
#
# The FORWARD form is required. `cargo tree -i <crate> -p tinyhivemind-core`
# discards the `-p` scope, prints the whole-workspace inverse tree, and exits 0
# looking clean even when this crate is the one at fault.
set -euo pipefail

# Core owns host-neutral folds and ports, and must stay free of harness,
# transport, runtime, and `anyhow` dependencies. Tools uses `tinytools::ToolSpec`,
# whose vocabulary dependency includes `anyhow` and `async-trait`.
pure_crates=("tinyhivemind-core" "tinyhivemind-lang")
tool_crates=("tinyhivemind-tools")

forbidden_pure='tokio|futures|async-trait|axum|hyper|reqwest|ureq|curl|anyhow|rusqlite|git2|openhuman|tinyagents|tinytools'
forbidden_tools='tokio|futures|axum|hyper|reqwest|ureq|curl|rusqlite|git2|openhuman|tinyagents'

status=0
check_crate() {
  local crate="$1" forbidden="$2"
  if ! cargo metadata --format-version 1 --no-deps \
    | jq -e --arg c "$crate" '.packages[] | select(.name == $c)' >/dev/null; then
    echo "assert-pure: no such package '$crate'" >&2
    exit 1
  fi

  tree="$(cargo tree -p "$crate" -e normal,build --all-features --prefix none)" || {
    echo "assert-pure: cargo tree failed for '$crate'" >&2
    exit 1
  }
  found="$(grep -Ei "^(${forbidden})(-embed)? v" <<<"$tree" || true)"
  if [ -n "$found" ]; then
    echo "$crate pulled in a dependency its manifest forbids:" >&2
    echo "$found" >&2
    status=1
  fi
}

for crate in "${pure_crates[@]}"; do
  check_crate "$crate" "$forbidden_pure"
done
for crate in "${tool_crates[@]}"; do
  check_crate "$crate" "$forbidden_tools"
done

if [ "$status" -ne 0 ]; then
  echo >&2
  echo "This crate is linked into the hot path of every agent turn and must" >&2
  echo "compile in a host's default build. It must stay free of async" >&2
  echo "runtimes, transports, HTTP clients and web frameworks." >&2
  exit 1
fi

echo "assert-pure: ${pure_crates[*]} ${tool_crates[*]} — clean"
