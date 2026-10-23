#!/bin/sh
# pre_tool_use hook: refuse a shell call that force-pushes or hard-resets.
# The event arrives as JSON on stdin; a refusal is JSON on stdout.
set -eu
event=$(cat)
case "$event" in
*"push --force"* | *"push -f"* | *"reset --hard"*)
  printf '{"permission":"deny","message":"destructive git commands are not allowed on the release desk"}\n'
  ;;
esac
