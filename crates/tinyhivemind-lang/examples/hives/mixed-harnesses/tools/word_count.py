"""Count words, lines and characters of the text passed as JSON on stdin."""
import json
import sys

text = json.load(sys.stdin)["text"]
json.dump(
    {"words": len(text.split()), "lines": len(text.splitlines()), "chars": len(text)},
    sys.stdout,
)
