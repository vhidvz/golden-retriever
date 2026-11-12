#!/usr/bin/env python3
"""Bilingual intent routing against a local llama-server, with schema-constrained output."""
import json, sys, time, urllib.request
URL = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8187"
INTENTS = ["come", "sit", "lie_down", "fetch", "sleep", "praise", "play", "remember", "computer_task", "chat", "unknown"]
SCHEMA = {"type": "object", "properties": {
    "language": {"type": "string", "enum": ["en", "fa"]},
    "intent": {"type": "string", "enum": INTENTS},
    "needs_computer_action": {"type": "boolean"}},
    "required": ["language", "intent", "needs_computer_action"], "additionalProperties": False}
SYSTEM = ("You classify what a person says to their pet dog named Buddy (بادی). Reply with JSON only. "
          "language: en or fa. intent: come (approach the speaker), sit, lie_down, fetch (bring an object such as a ball), "
          "sleep, praise (good boy), play, remember (the person states a preference or fact to keep), "
          "computer_task (find/open/run something on the computer), chat (small talk or a question), unknown. "
          "needs_computer_action is true only for computer_task.")
CASES = [
    ("Buddy, come here.", "en", "come"), ("بادی بیا اینجا", "fa", "come"),
    ("Sit!", "en", "sit"), ("بشین", "fa", "sit"),
    ("Lie down, boy.", "en", "lie_down"), ("دراز بکش", "fa", "lie_down"),
    ("Bring me your ball.", "en", "fetch"), ("توپت رو بیار", "fa", "fetch"),
    ("Go to sleep.", "en", "sleep"), ("برو بخواب", "fa", "sleep"),
    ("Good boy!", "en", "praise"), ("آفرین پسر خوب", "fa", "praise"),
    ("Let's play!", "en", "play"), ("بیا بازی کنیم", "fa", "play"),
    ("Remember that I prefer working late.", "en", "remember"), ("یادت باشه من شب‌ها کار می‌کنم", "fa", "remember"),
    ("Find this file for me.", "en", "computer_task"), ("این فایل رو برام پیدا کن", "fa", "computer_task"),
    ("What do you think about this?", "en", "chat"), ("نظرت درباره این چیه؟", "fa", "chat"),
]
def ask(text):
    body = {"messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": text}], "temperature": 0, "max_tokens": 60,
            "response_format": {"type": "json_schema", "json_schema": {"name": "intent", "schema": SCHEMA, "strict": True}},
            "chat_template_kwargs": {"enable_thinking": False}}
    req = urllib.request.Request(URL + "/v1/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    t = time.time(); out = json.load(urllib.request.urlopen(req, timeout=120)); dt = time.time() - t
    return json.loads(out["choices"][0]["message"]["content"]), dt, out.get("timings", {})
ok = {"en": 0, "fa": 0}; lang_ok = 0; times = []; wrong = []
for text, lang, intent in CASES:
    try: got, dt, _ = ask(text)
    except Exception as e: wrong.append((text, f"ERROR {e}")); continue
    times.append(dt); lang_ok += got.get("language") == lang
    if got.get("intent") == intent: ok[lang] += 1
    else: wrong.append((text, got.get("intent")))
n = len(CASES) // 2; times.sort()
print(f"intent correct: en {ok['en']}/{n}, fa {ok['fa']}/{n}; language detected {lang_ok}/{len(CASES)}; valid JSON {len(times)}/{len(CASES)}")
print(f"latency per utterance: median {times[len(times)//2]:.2f}s, worst {times[-1]:.2f}s (first call includes prompt caching warm-up)")
for text, got in wrong: print(f"  miss: {text!r} -> {got}")
