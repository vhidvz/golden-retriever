#!/usr/bin/env python3
"""Evaluates a model served by Ollama for Buddy's needs: speed, bilingual intent
routing with a JSON schema, tool calling, image input, and a Persian reply.
Usage: ollama_eval.py MODEL [speed|intent|tools|vision|persian ...]"""
import base64, io, json, sys, time, urllib.request

URL = "http://127.0.0.1:11434"
MODEL = sys.argv[1] if len(sys.argv) > 1 else "gemma4:e4b"
WHICH = sys.argv[2:] or ["speed", "intent", "tools", "vision", "persian"]

def chat(messages, **extra):
    body = {"model": MODEL, "messages": messages, "stream": False, "think": False, "options": {"temperature": 0}, **extra}
    req = urllib.request.Request(URL + "/api/chat", json.dumps(body).encode(), {"Content-Type": "application/json"})
    started = time.time()
    out = json.load(urllib.request.urlopen(req, timeout=600))
    out["wall"] = time.time() - started
    return out

def rate(out, what):
    n, ns = out.get(f"{what}_count", 0), out.get(f"{what}_duration", 0)
    return n / (ns / 1e9) if ns else 0.0

if "speed" in WHICH:
    cold = chat([{"role": "user", "content": "Say hello in five words."}])
    warm = chat([{"role": "user", "content": "In about 120 words, describe what a golden retriever puppy does on a sunny morning."}])
    print(f"speed: load {cold.get('load_duration', 0) / 1e9:.1f}s; prompt {rate(warm, 'prompt_eval'):.0f} tok/s; generation {rate(warm, 'eval'):.1f} tok/s over {warm.get('eval_count')} tokens")

if "intent" in WHICH:
    INTENTS = ["come", "sit", "lie_down", "fetch", "sleep", "praise", "play", "remember", "computer_task", "chat", "unknown"]
    SCHEMA = {"type": "object", "properties": {"language": {"type": "string", "enum": ["en", "fa"]}, "intent": {"type": "string", "enum": INTENTS},
              "needs_computer_action": {"type": "boolean"}}, "required": ["language", "intent", "needs_computer_action"]}
    SYSTEM = ("You classify what a person says to their pet dog named Buddy (بادی). Reply with JSON only. "
              "language: en or fa. intent: come (approach the speaker), sit, lie_down, fetch (bring an object such as a ball), "
              "sleep, praise (good boy), play, remember (the person states a preference or fact to keep), "
              "computer_task (find/open/run something on the computer), chat (small talk or a question), unknown. "
              "needs_computer_action is true only for computer_task.")
    CASES = [("Buddy, come here.", "en", "come"), ("بادی بیا اینجا", "fa", "come"), ("Sit!", "en", "sit"), ("بشین", "fa", "sit"),
             ("Lie down, boy.", "en", "lie_down"), ("دراز بکش", "fa", "lie_down"), ("Bring me your ball.", "en", "fetch"), ("توپت رو بیار", "fa", "fetch"),
             ("Go to sleep.", "en", "sleep"), ("برو بخواب", "fa", "sleep"), ("Good boy!", "en", "praise"), ("آفرین پسر خوب", "fa", "praise"),
             ("Let's play!", "en", "play"), ("بیا بازی کنیم", "fa", "play"), ("Remember that I prefer working late.", "en", "remember"),
             ("یادت باشه من شب‌ها کار می‌کنم", "fa", "remember"), ("Find this file for me.", "en", "computer_task"), ("این فایل رو برام پیدا کن", "fa", "computer_task"),
             ("What do you think about this?", "en", "chat"), ("نظرت درباره این چیه؟", "fa", "chat")]
    ok, lang_ok, valid, times, misses = {"en": 0, "fa": 0}, 0, 0, [], []
    for text, lang, intent in CASES:
        out = chat([{"role": "system", "content": SYSTEM}, {"role": "user", "content": text}], format=SCHEMA)
        times.append(out["wall"])
        try:
            got = json.loads(out["message"]["content"]); valid += 1
        except Exception:
            misses.append((text, "invalid JSON")); continue
        lang_ok += got.get("language") == lang
        if got.get("intent") == intent: ok[lang] += 1
        else: misses.append((text, got.get("intent")))
    times.sort()
    print(f"intent: en {ok['en']}/10, fa {ok['fa']}/10; language {lang_ok}/20; valid JSON {valid}/20; median {times[len(times) // 2]:.2f}s, worst {times[-1]:.2f}s")
    for text, got in misses: print(f"  miss: {text!r} -> {got}")

if "tools" in WHICH:
    TOOLS = [{"type": "function", "function": {"name": "find_file", "description": "Search the user's files by name or content.",
              "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}},
             {"type": "function", "function": {"name": "open_application", "description": "Open an installed desktop application.",
              "parameters": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}}}]
    for text, want in [("Find my tax report from last year.", "find_file"), ("گزارش مالیات پارسال رو پیدا کن", "find_file"),
                       ("Open the calculator.", "open_application"), ("ماشین‌حساب رو باز کن", "open_application"), ("Good boy, Buddy!", None)]:
        out = chat([{"role": "system", "content": "You help a user with their computer. Call a tool only when the request needs one."}, {"role": "user", "content": text}], tools=TOOLS)
        calls = out["message"].get("tool_calls") or []
        got = calls[0]["function"]["name"] if calls else None
        args = calls[0]["function"].get("arguments") if calls else ""
        print(f"tools: {'ok  ' if got == want else 'MISS'} {text!r} -> {got} {json.dumps(args, ensure_ascii=False)} ({out['wall']:.1f}s)")

if "vision" in WHICH:
    from PIL import Image, ImageDraw, ImageFont
    im = Image.new("RGB", (640, 360), (30, 30, 40)); d = ImageDraw.Draw(im)
    try: font = ImageFont.truetype("/usr/share/fonts/TTF/DejaVuSans.ttf", 28)
    except Exception: font = ImageFont.load_default()
    d.text((30, 110), "error: test code 4721 failed", fill=(255, 90, 90), font=font); d.ellipse((480, 200, 600, 320), fill=(220, 40, 40))
    buf = io.BytesIO(); im.save(buf, "PNG")
    out = chat([{"role": "user", "content": "What numeric code appears in this image, and what colour is the circle? One short sentence.", "images": [base64.b64encode(buf.getvalue()).decode()]}])
    print(f"vision: {out['message']['content'].strip()!r} ({out['wall']:.1f}s, {out.get('prompt_eval_count')} prompt tokens)")

if "persian" in WHICH:
    out = chat([{"role": "user", "content": "من امروز خیلی خسته‌ام و کارهام عقب افتاده. در دو جمله‌ی کوتاه و دوستانه به فارسی دلداری‌ام بده."}])
    print(f"persian: {out['message']['content'].strip()!r} ({out['wall']:.1f}s, {rate(out, 'eval'):.1f} tok/s)")
