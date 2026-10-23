#!/bin/sh
# Writes a little over 1 MiB to stdout.
head -c 1100000 /dev/zero | tr '\0' a
