#!/bin/sh
# Print lines matching an extended regular expression, with file and line.
# Arguments: <pattern> [path]
set -eu
pattern="$1"
path="${2:-.}"
grep -rnE --exclude-dir=.git --exclude-dir=target -- "$pattern" "$path" | head -n 200 || true
