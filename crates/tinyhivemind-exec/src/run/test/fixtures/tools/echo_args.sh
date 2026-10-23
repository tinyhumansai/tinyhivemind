#!/bin/sh
# Prints each argument on its own line.
for arg in "$@"; do
  printf '%s\n' "$arg"
done
