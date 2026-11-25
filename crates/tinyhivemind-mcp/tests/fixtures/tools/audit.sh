#!/bin/sh
# A post_tool_use hook: logs each event.
input=$(cat)
printf '%s\n' "$input" >> audit.log
