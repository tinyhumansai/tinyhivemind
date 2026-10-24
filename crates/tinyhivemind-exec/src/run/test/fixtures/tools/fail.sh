#!/bin/sh
# Writes more than 4 KiB to stderr, ending in a marker, and exits 3.
head -c 5000 /dev/zero | tr '\0' x >&2
printf 'TAIL-MARKER' >&2
exit 3
