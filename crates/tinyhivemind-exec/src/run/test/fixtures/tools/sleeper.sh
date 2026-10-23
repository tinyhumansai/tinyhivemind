#!/bin/sh
# Starts a long-sleeping child, records its pid in "$1", and waits for it.
sleep 30 &
printf '%s' "$!" > "$1"
wait
