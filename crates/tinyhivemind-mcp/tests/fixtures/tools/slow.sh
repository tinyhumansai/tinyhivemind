#!/bin/sh
# A write tool that starts, then waits until the test creates `release`.
cat > /dev/null
echo started >> slow.log
while [ ! -e release ]; do sleep 0.02; done
echo released
