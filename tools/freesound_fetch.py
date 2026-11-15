#!/usr/bin/env python3
"""Finds recordings of real dogs on Freesound for the sounds Buddy has none of yet,
and downloads their previews to listen to and cut (renderer/sounds/SOURCES.md says
which were kept, and from whom).

    python3 tools/freesound_fetch.py [KIND ...]      # kinds: drink eat sniff yawn shake

Only sounds in the public domain (Creative Commons 0) and tagged "dog" are taken. It
uses Freesound's documented API with an API key (https://freesound.org/apiv2/apply),
read from ~/.config/buddy/freesound-key; never a password. The key is sent to
freesound.org only. Downloads go to ~/.cache/buddy-spikes/sounds/freesound/<kind>/,
each with a .json file saying where it came from.
"""
import json, os, sys, time, urllib.error, urllib.parse, urllib.request

API = "https://freesound.org/apiv2/search/"
KEY_FILE = os.path.expanduser("~/.config/buddy/freesound-key")
OUT = os.path.expanduser("~/.cache/buddy-spikes/sounds/freesound")
FIELDS = "id,name,username,license,duration,tags,description,previews,num_downloads,avg_rating,url"
SEARCHES = {
    "drink": ["dog drinking water", "dog lapping water", "dog drinking"],
    "eat": ["dog eating", "dog eating dry food", "dog chewing", "dog crunching kibble"],
    "sniff": ["dog sniffing", "dog sniff", "dog snuffling"],
    "yawn": ["dog yawn", "dog yawning", "puppy yawn"],
    "shake": ["dog shaking", "dog shake off", "dog shaking ears", "dog shaking collar"],
}
PER_KIND = 12


def get(url, key, tries=4):
    """One request, tried again after a pause if the network fails (it sometimes times out)."""
    request = urllib.request.Request(url, headers={"Authorization": f"Token {key}", "User-Agent": "buddy-sound-search/1.0"})
    for attempt in range(tries):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return response.read()
        except (OSError, urllib.error.URLError) as error:
            if attempt == tries - 1:
                raise
            print(f"   (network: {error}; trying again)", flush=True)
            time.sleep(5 * (attempt + 1))


def search(query, key):
    params = {
        "query": query,
        "filter": 'license:"Creative Commons 0" tag:dog duration:[0.3 TO 30]',
        "fields": FIELDS,
        "sort": "score",
        "page_size": "30",
    }
    return json.loads(get(API + "?" + urllib.parse.urlencode(params), key))["results"]


def main():
    try:
        key = open(KEY_FILE).read().strip()
    except FileNotFoundError:
        sys.exit(f"No Freesound API key yet: get one at https://freesound.org/apiv2/apply and save it in {KEY_FILE}")
    if not key:
        sys.exit(f"{KEY_FILE} is empty")
    kinds = sys.argv[1:] or list(SEARCHES)
    for kind in kinds:
        found = {}
        for query in SEARCHES[kind]:
            for sound in search(query, key):
                found.setdefault(sound["id"], sound)
            time.sleep(1.2)             # well inside the API's limit of 60 requests a minute
        # Most downloaded first: a rough measure of a recording others found good and clean.
        best = sorted(found.values(), key=lambda s: (-s.get("num_downloads", 0), s["id"]))[:PER_KIND]
        folder = os.path.join(OUT, kind)
        os.makedirs(folder, exist_ok=True)
        print(f"{kind}: {len(found)} CC0 recordings tagged dog; downloading the {len(best)} most used")
        for sound in best:
            assert sound["license"].endswith("/zero/1.0/"), sound["license"]
            target = os.path.join(folder, f"{sound['id']}.ogg")
            if not os.path.exists(target):
                try:
                    data = get(sound["previews"]["preview-hq-ogg"], key)
                except (OSError, urllib.error.URLError) as error:
                    print(f"   skipped {sound['id']}: {error}", flush=True)
                    continue
                with open(target + ".part", "wb") as f:
                    f.write(data)
                os.replace(target + ".part", target)
                time.sleep(0.5)
            with open(target[:-4] + ".json", "w") as f:
                json.dump({k: sound.get(k) for k in ("id", "name", "username", "license", "duration", "tags", "description", "url")}, f, indent=1)
            print(f"   {sound['id']:>7}  {sound['duration']:5.1f} s  {sound['username'][:20]:20}  {sound['name'][:60]}")


if __name__ == "__main__":
    main()
