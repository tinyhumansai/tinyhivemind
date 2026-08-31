#!/usr/bin/env bash
# Assert that the pure crates stay pure.
#
# `tinyteams-core` is linked into the hot path of every agent turn and must
# compile in a host's default build with no feature flags. That promise is
# invisible in a diff, because a forbidden dependency arrives transitively
# through a feature someone enabled one crate away — so it is asserted rather
# than documented.
#
# The FORWARD form is required. `cargo tree -i <crate> -p tinyteams-core`
# discards the `-p` scope, prints the whole-workspace inverse tree, and exits 0
# looking clean even when this crate is the one at fault.
set -euo pipefail

# Crates that may not depend on a runtime, a transport, or a web framework.
pure_crates=("tinyteams-core")

# `tinyteams` (the session runtime) is exempt from `tokio`/`futures`/
# `async-trait`, which it needs for its ports — but not from the rest. It is
# listed separately once it exists.
forbidden_pure='tokio|futures|async-trait|axum|hyper|reqwest|ureq|anyhow|rusqlite|git2'

status=0
for crate in "${pure_crates[@]}"; do
  if ! cargo metadata --format-version 1 --no-deps \
    | jq -e --arg c "$crate" '.packages[] | select(.name == $c)' >/dev/null; then
    echo "assert-pure: no such package '$crate'" >&2
    exit 1
  fi

  found="$(cargo tree -p "$crate" -e normal,build --prefix none \
    | grep -Ei "$forbidden_pure" || true)"
  if [ -n "$found" ]; then
    echo "$crate pulled in a dependency its manifest forbids:" >&2
    echo "$found" >&2
    status=1
  fi
done

if [ "$status" -ne 0 ]; then
  echo >&2
  echo "This crate is linked into the hot path of every agent turn and must" >&2
  echo "compile in a host's default build. It must stay free of async" >&2
  echo "runtimes, transports, HTTP clients and web frameworks." >&2
  exit 1
fi

echo "assert-pure: ${pure_crates[*]} — clean"
