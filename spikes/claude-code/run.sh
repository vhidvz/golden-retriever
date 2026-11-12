#!/bin/bash
# Checks the Claude Code integration boundary using the installed `claude` binary
# and your own subscription login. Makes six small Haiku calls.
set -u
S="$(mktemp -d)"; cd "$S"; trap 'rm -rf "$S"' EXIT
# A clean environment, as a daemon would use: no inherited API keys, stdin closed.
ce() { env -i HOME="$HOME" PATH="$PATH" USER="$USER" LANG="${LANG:-en_US.UTF-8}" "$@" < /dev/null; }
COMMON=(--model haiku --setting-sources "" --strict-mcp-config)
python3 - <<'PY'
from PIL import Image, ImageDraw
im = Image.new("RGB", (640, 360), (30, 30, 40)); d = ImageDraw.Draw(im)
d.text((30, 110), "error: test code 4721 failed", fill=(255, 90, 90)); d.ellipse((480, 200, 600, 320), fill=(220, 40, 40))
im.save("synthetic.png")
PY
echo "== preflight"; ce claude auth status --json | jq -c '{loggedIn, authMethod, loginExpiresAt}'
echo "== structured output for a Persian utterance"
SCHEMA='{"type":"object","properties":{"language":{"type":"string","enum":["en","fa"]},"intent":{"type":"string","enum":["come","sit","fetch","sleep","praise","play","unknown"]}},"required":["language","intent"],"additionalProperties":false}'
ce claude -p 'Classify this utterance spoken to a pet dog named Buddy: "بادی بیا اینجا و توپت رو بیار"' "${COMMON[@]}" --output-format json --json-schema "$SCHEMA" --tools "" --no-session-persistence | jq -c '{is_error, duration_ms, structured_output}'
echo "== stream-json event types (note rate_limit_event: machine-readable limits and reset times)"
ce claude -p 'Reply with exactly: woof' "${COMMON[@]}" --output-format stream-json --verbose --tools "" --no-session-persistence > t.jsonl
jq -r '[.type, (.subtype // "-")] | join("/")' t.jsonl | sort | uniq -c; jq -c 'select(.type=="rate_limit_event") | .rate_limit_info | {status, resetsAt, rateLimitType}' t.jsonl
echo "== image input through the Read tool (a synthetic image, never the screen)"
ce claude -p "Read the image at $S/synthetic.png: what numeric code appears and what colour is the circle?" "${COMMON[@]}" --output-format json --tools "Read" --allowedTools "Read" --no-session-persistence | jq -c '{is_error, result}'
echo "== a session resumed by a second process"
SID=$(python3 -c 'import uuid; print(uuid.uuid4())')
ce claude -p 'Remember this word: biscuit. Reply only: ok' "${COMMON[@]}" --output-format json --tools "" --session-id "$SID" | jq -c '{result}'
ce claude -p 'Which word did I ask you to remember? One word.' "${COMMON[@]}" --output-format json --tools "" --resume "$SID" | jq -c '{result}'
echo "== failure shape with the network unreachable (expect exit 1 and is_error true)"
ce env HTTPS_PROXY=http://127.0.0.1:9 CLAUDE_CODE_MAX_RETRIES=1 claude -p 'hi' "${COMMON[@]}" --output-format json --tools "" --no-session-persistence | jq -c '{subtype, is_error, result}'; echo "exit=${PIPESTATUS[0]}"
echo "== an API key in the environment silently changes who is billed; the preflight shows it"
ce env ANTHROPIC_API_KEY=sk-ant-not-a-real-key claude auth status --json | jq -c '{authMethod, apiKeySource}'
