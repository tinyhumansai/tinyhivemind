#!/usr/bin/env bash
# fetch-harness.sh <kind> <directory> [target]
#
# Fetch the harness binary `harnesses.lock` pins for <kind> and <target>
# (default: this machine's), verify it, and print the binary's absolute path.
# The asset's sha256 must match the lock, and so must the binary's where the
# lock records it. Nothing is installed or run; the library takes the path.
#
#   THM_OPENHUMAN_CORE=$(.github/scripts/fetch-harness.sh openhuman target/harness)
set -euo pipefail

kind="${1:?usage: fetch-harness.sh <kind> <directory> [target]}"
dest="${2:?usage: fetch-harness.sh <kind> <directory> [target]}"
target="${3:-$(uname -m)-unknown-linux-gnu}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
lock="$root/harnesses.lock"

# The entry of the lock for this kind and target, as `key=value` lines.
entry="$(awk -v kind="[[$kind]]" -v target="$target" '
  $0 == kind { inside = 1; delete fields; next }
  /^\[\[/ { inside = 0 }
  inside && /^[a-z_0-9]+ = "/ {
    key = $1; value = $0; sub(/^[^"]*"/, "", value); sub(/"[^"]*$/, "", value)
    fields[key] = value
    if (key == "binary" && fields["target"] == target) {
      for (k in fields) print k "=" fields[k]
    }
  }
  inside && /^binary_sha256 = / && fields["target"] == target { print "binary_sha256=" fields["binary_sha256"] }
' "$lock" | sort -u)"
field() { sed -n "s/^$1=//p" <<<"$entry"; }
url="$(field url)"; sha="$(field sha256)"; version="$(field version)"
binary="$(field binary)"; binary_sha="$(field binary_sha256)"
if [ -z "$url" ] || [ -z "$sha" ] || [ -z "$binary" ]; then
  echo "fetch-harness: no $kind entry for $target in harnesses.lock" >&2
  exit 1
fi

mkdir -p "$dest"
dest="$(cd "$dest" && pwd)"
asset="$dest/$(basename "$url")"
if ! echo "$sha  $asset" | sha256sum -c --status 2>/dev/null; then
  curl -fsSL --proto '=https' -o "$asset.partial" "$url"
  mv "$asset.partial" "$asset"
fi
if ! echo "$sha  $asset" | sha256sum -c --status; then
  echo "fetch-harness: $asset does not have the sha256 harnesses.lock pins" >&2
  rm -f "$asset"
  exit 1
fi

unpacked="$dest/$kind-$version-$target"
rm -rf "$unpacked" && mkdir -p "$unpacked"
tar -xzf "$asset" -C "$unpacked"
path="$(find "$unpacked" -type f -name "$binary" | head -n 1)"
if [ -z "$path" ]; then
  echo "fetch-harness: $asset holds no $binary" >&2
  exit 1
fi
if [ -n "$binary_sha" ] && ! echo "$binary_sha  $path" | sha256sum -c --status; then
  echo "fetch-harness: $path does not have the sha256 harnesses.lock pins" >&2
  exit 1
fi
echo "$path"
