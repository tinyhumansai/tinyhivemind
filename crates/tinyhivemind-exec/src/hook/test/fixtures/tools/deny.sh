#!/bin/sh
# Refuses the call, naming the hook's first argument in the message.
cat > /dev/null
printf '{"permission": "deny", "message": "refused: %s"}\n' "$1"
