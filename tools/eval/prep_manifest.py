"""Build a fixed test set from the user's Typeless recordings. Prints counts only."""
import collections
import json
import os
import random
import re
import sqlite3
import subprocess
import sys

OUT = sys.argv[1]
ROOT = os.path.expanduser("~/Library/Application Support/Typeless")
CJK = re.compile(r"[一-鿿]")
LATIN = re.compile(r"[A-Za-z][A-Za-z0-9+#._-]+")
MAX_DURATION = 90.0
TARGETS = {"mixed": 35, "zh": 28, "latin": 7}
STOP = set(
    "the a an and or of to in on at for is are was were be been it this that these those with as by from "
    "i you he she we they my your our their me us them not no yes so but if then than do does did have has had "
    "can could will would should may might just like about what when where which who how why all any some more "
    "there here out up down over into very also get got go going ok okay".split()
)


def bucket(text):
    has_cjk, has_latin = bool(CJK.search(text)), bool(LATIN.search(text))
    if has_cjk and has_latin:
        return "mixed"
    return "zh" if has_cjk else "latin"


con = sqlite3.connect("file:%s?mode=ro&immutable=1" % os.path.join(ROOT, "typeless.db"), uri=True)
rows = con.execute(
    "select refined_text, audio_local_path, duration from history_v2 "
    "where mode='voice_transcript' and status='completed'"
).fetchall()

usable = []
for refined, path, duration in rows:
    if not refined or not path or not duration or duration > MAX_DURATION:
        continue
    audio = path if os.path.isabs(path) else os.path.join(ROOT, path)
    if not os.path.exists(audio):
        audio = os.path.join(ROOT, "Recordings", os.path.basename(path))
    if os.path.exists(audio):
        usable.append({"refined": refined, "src": audio, "dur": float(duration), "bucket": bucket(refined)})

random.seed(20261109)
random.shuffle(usable)
test, train = [], []
taken = collections.Counter()
for item in usable:
    if taken[item["bucket"]] < TARGETS[item["bucket"]]:
        taken[item["bucket"]] += 1
        test.append(item)
    else:
        train.append(item)

counts = collections.Counter()
display = {}
for item in train:
    for word in LATIN.findall(item["refined"]):
        low = word.lower().rstrip("._-")
        if low in STOP or len(low) < 2:
            continue
        counts[low] += 1
        display.setdefault(low, collections.Counter())[word.rstrip("._-")] += 1
terms = [display[w].most_common(1)[0][0] for w, c in counts.most_common(40) if c >= 2]

def write_set(out, clips):
    wav_dir = os.path.join(out, "wav")
    os.makedirs(wav_dir, exist_ok=True)
    for index, item in enumerate(clips):
        item["idx"] = index
        item["wav"] = os.path.join(wav_dir, "%03d.wav" % index)
        if not os.path.exists(item["wav"]):
            subprocess.run(
                ["ffmpeg", "-nostdin", "-loglevel", "error", "-y", "-i", item["src"], "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", item["wav"]],
                check=True,
            )
    with open(os.path.join(out, "manifest.json"), "w") as fh:
        json.dump({"items": clips, "terms": terms}, fh, ensure_ascii=False)


write_set(OUT, test)
write_set(os.path.join(OUT, "holdout"), train)

print("rows", len(rows), "usable", len(usable), "test", len(test), "train", len(train))
print("test buckets", dict(taken), "audio_seconds", round(sum(i["dur"] for i in test)))
print("holdout", len(train))
print("hint terms from train set only:", len(terms))
print("terms:", ", ".join(terms))
