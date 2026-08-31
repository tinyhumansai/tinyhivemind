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

# Crates that may not depend on a runtime, a transport, or a web framework.
#
# `tinyhivemind-hive` is here rather than in the exempt list below because it
# defines no port of its own: an episode is a pure state machine over a
# transcript the caller already holds, and every host obligation it needs is
# already carried by `tinyhivemind`. See
# docs/adr/0002-hive-episodes-are-sequential.md.
pure_crates=("tinyhivemind-core" "tinyhivemind-hive")

# `tinyhivemind` (the session runtime) is exempt from `tokio`/`futures`/
# `async-trait`, which it needs for its ports — but not from the rest. Its
# ports are boxed `std::future::Future`s, so today it needs none of the three;
# the exemption exists so that adding one is not a CI failure.
exempt_async_crates=("tinyhivemind")

# This is a maintained blocklist of known offenders, not an exhaustive
# allowlist: it names every async runtime, transport, HTTP client, database
# client, and VCS binding this repository has needed to reject so far, plus
# `anyhow` (AGENTS.md requires the crate error type instead). Extend it — do
# not work around it — the day a new one shows up in the tree.
forbidden_pure='tokio|futures|async-trait|axum|hyper|reqwest|ureq|curl|anyhow|rusqlite|git2'

# The same list minus the three async primitives a port layer legitimately needs.
forbidden_exempt='axum|hyper|reqwest|ureq|curl|anyhow|rusqlite|git2'

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
  found="$(grep -Ei "$forbidden" <<<"$tree" || true)"
  if [ -n "$found" ]; then
    echo "$crate pulled in a dependency its manifest forbids:" >&2
    echo "$found" >&2
    status=1
  fi
}

for crate in "${pure_crates[@]}"; do
  check_crate "$crate" "$forbidden_pure"
done
for crate in "${exempt_async_crates[@]}"; do
  check_crate "$crate" "$forbidden_exempt"
done

if [ "$status" -ne 0 ]; then
  echo >&2
  echo "This crate is linked into the hot path of every agent turn and must" >&2
  echo "compile in a host's default build. It must stay free of async" >&2
  echo "runtimes, transports, HTTP clients and web frameworks." >&2
  exit 1
fi

echo "assert-pure: ${pure_crates[*]} ${exempt_async_crates[*]} — clean"
