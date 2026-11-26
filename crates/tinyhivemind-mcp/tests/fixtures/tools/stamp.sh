#!/bin/sh
# A write tool: appends its argument object to stamp.log in the workspace.
input=$(cat)
printf '%s\n' "$input" >> stamp.log
printf 'stamped %s\n' "$THM_CALL_ID"
