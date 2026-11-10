"""For each test clip, collect the dictations made shortly before it. Prints counts only."""
import datetime as dt
import json
import os
import sqlite3
import sys

OUT = sys.argv[1]
WINDOW_MINUTES = 10
MAX_PRIOR = 3
MAX_CHARS = 400


def parse(stamp):
    return dt.datetime.fromisoformat(str(stamp).replace("Z", "+00:00"))


with open(os.path.join(OUT, "manifest.json")) as fh:
    manifest = json.load(fh)

db = os.path.expanduser("~/Library/Application Support/Typeless/typeless.db")
con = sqlite3.connect("file:%s?mode=ro&immutable=1" % db, uri=True)
rows = con.execute(
    "select created_at, refined_text, audio_local_path from history_v2 "
    "where mode='voice_transcript' and status='completed' and refined_text!='' order by created_at"
).fetchall()
timeline = [(parse(c), text, os.path.basename(path or "")) for c, text, path in rows]

contexts = {}
for item in manifest["items"]:
    name = os.path.basename(item["src"])
    position = next((i for i, row in enumerate(timeline) if row[2] == name), None)
    if position is None:
        continue
    when = timeline[position][0]
    prior = [
        text
        for moment, text, _ in timeline[max(0, position - MAX_PRIOR) : position]
        if dt.timedelta(0) <= when - moment <= dt.timedelta(minutes=WINDOW_MINUTES)
    ]
    if prior:
        contexts[str(item["idx"])] = "\n".join(prior)[-MAX_CHARS:]

with open(os.path.join(OUT, "contexts.json"), "w") as fh:
    json.dump(contexts, fh, ensure_ascii=False)
lengths = sorted(len(v) for v in contexts.values())
print("test clips", len(manifest["items"]), "with prior dictation within", WINDOW_MINUTES, "min:", len(contexts))
if lengths:
    print("context chars median", lengths[len(lengths) // 2], "max", lengths[-1])
