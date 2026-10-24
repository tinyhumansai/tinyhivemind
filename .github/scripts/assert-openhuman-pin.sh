#!/bin/sh
# Assert the `rev` of the git `openhuman-embed` dependency in Cargo.toml equals
# the commit recorded for the `vendor/openhuman` submodule. The `[patch]` in
# Cargo.toml builds this workspace against the submodule; a consumer without
# that patch builds against the rev. If they drift, the two build different
# OpenHuman trees and nothing else would say so.
set -eu
cd "$(dirname "$0")/../.."
# Each harness crate's pinned rev against the tree it is patched onto.
# `openhuman-embed` and `openhuman` come from the OpenHuman submodule; the
# tool crates come from the tinytools submodule nested inside it, at whatever
# commit OpenHuman's own tree vendors, since that is the copy the patch
# resolves them onto.
pin() {
  sed -n "s/^$1 = { git = \"https:\/\/github.com\/tinyhumansai\/$2\", rev = \"\([0-9a-f]*\)\".*/\1/p" Cargo.toml
}
check() {
  crate="$1" rev="$2" tree="$3" sub="$4"
  if [ -z "$rev" ] || [ -z "$sub" ]; then
    echo "assert-openhuman-pin: could not read the rev for $crate ($rev) or the submodule pointer for $tree ($sub)" >&2
    exit 1
  fi
  if [ "$rev" != "$sub" ]; then
    echo "assert-openhuman-pin: Cargo.toml pins $crate at $rev but $tree records $sub" >&2
    exit 1
  fi
  echo "assert-openhuman-pin: $crate $rev"
}
openhuman_sub=$(git ls-tree HEAD vendor/openhuman | awk '{print $3}')
check openhuman-embed "$(pin openhuman-embed openhuman)" vendor/openhuman "$openhuman_sub"
check openhuman "$(pin openhuman openhuman)" vendor/openhuman "$openhuman_sub"
tinytools_sub=$(git -C vendor/openhuman/vendor/tinyagents ls-tree HEAD vendor/tinytools | awk '{print $3}')
check tinytools "$(pin tinytools tinytools)" vendor/openhuman/vendor/tinyagents/vendor/tinytools "$tinytools_sub"
check tinytools-agent "$(pin tinytools-agent tinytools)" vendor/openhuman/vendor/tinyagents/vendor/tinytools "$tinytools_sub"
# The memory crates come from the tinymemory submodule inside OpenHuman.
tinymemory_sub=$(git -C vendor/openhuman ls-tree HEAD vendor/tinymemory | awk '{print $3}')
check tinymemory-api "$(pin tinymemory-api tinymemory)" vendor/openhuman/vendor/tinymemory "$tinymemory_sub"
check tinymemory-tools "$(pin tinymemory-tools tinymemory)" vendor/openhuman/vendor/tinymemory "$tinymemory_sub"
# The schedule validator comes from the tinyflows submodule inside OpenHuman.
tinyflows_sub=$(git -C vendor/openhuman ls-tree HEAD vendor/tinyflows | awk '{print $3}')
check tinyflows-schedule "$(pin tinyflows-schedule tinyflows)" vendor/openhuman/vendor/tinyflows "$tinyflows_sub"
