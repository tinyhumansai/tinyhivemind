#!/bin/sh
# A pre_tool_use hook: logs each event and refuses one naming "forbidden".
input=$(cat)
printf '%s\n' "$input" >> guard.log
case "$input" in
  *forbidden*) printf '{"permission": "deny", "message": "forbidden by guard"}\n' ;;
  *) echo '{}' ;;
esac
