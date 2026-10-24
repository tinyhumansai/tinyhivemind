#!/bin/sh
# Refuses the call without a message.
cat > /dev/null
printf '{"permission": "deny"}\n'
