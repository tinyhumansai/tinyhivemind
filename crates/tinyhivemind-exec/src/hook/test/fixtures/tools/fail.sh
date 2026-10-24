#!/bin/sh
# Fails.
cat > /dev/null
printf 'policy store unreachable' >&2
exit 1
