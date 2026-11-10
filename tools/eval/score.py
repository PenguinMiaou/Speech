"""Score every result file against Typeless's final text. Prints aggregates only, never transcripts."""
import collections
import glob
import json
import os
import re
import statistics as st
import sys

from opencc import OpenCC
from rapidfuzz.distance import Levenshtein

OUT = sys.argv[1]
BASELINE = "whisper-turbo-auto"
LATIN = re.compile(r"[A-Za-z][A-Za-z0-9+#._-]+")
t2s = OpenCC("t2s")


def norm(text):
    return re.sub(r"[\W_]+", "", t2s.convert(text or "").lower(), flags=re.UNICODE)


def pct(values, q):
    values = sorted(values)
    return values[min(len(values) - 1, int(q * len(values)))]


with open(os.path.join(OUT, "manifest.json")) as fh:
    items = {i["idx"]: i for i in json.load(fh)["items"]}

# The reference has known errors. Corrections live next to the data, outside the repository.
if os.environ.get("FIX_LABELS"):
    with open(os.path.join(OUT, "label-fixes.json"), encoding="utf-8") as fh:
        fixes = json.load(fh)
    for item in items.values():
        for wrong, right in fixes.items():
            item["refined"] = re.sub(
                rf"(?<![A-Za-z]){re.escape(wrong)}(?![A-Za-z])", right, item["refined"], flags=re.I
            )

scores = {}
for path in sorted(glob.glob(os.path.join(OUT, "results", "*.jsonl"))):
    label = os.path.basename(path)[:-6]
    with open(path) as fh:
        rows = [json.loads(line) for line in fh]
    by_clip, secs, langs = {}, [], collections.Counter()
    empty = errors = traditional = term_hit = term_all = 0
    for row in rows:
        item = items[row["idx"]]
        errors += bool(row.get("err"))
        hyp = norm(row["text"])
        empty += not hyp
        traditional += t2s.convert(row["text"]) != row["text"]
        langs[row.get("lang")] += 1
        secs.append(row["secs"])
        by_clip[row["idx"]] = Levenshtein.normalized_similarity(hyp, norm(item["refined"]))
        if item["bucket"] == "mixed":
            for word in {w.lower() for w in LATIN.findall(item["refined"])}:
                term_all += 1
                term_hit += norm(word) in hyp
    scores[label] = by_clip
    group = lambda name: [v for k, v in by_clip.items() if items[k]["bucket"] == name] or [0.0]
    audio = sum(items[r["idx"]]["dur"] for r in rows)
    print(
        "%-26s n=%d all=%.3f zh=%.3f mixed=%.3f latin=%.3f term_recall=%d/%d=%.3f low<0.5=%d empty=%d err=%d trad=%d p50=%.2fs p90=%.2fs speed=%.1fx langs=%s"
        % (
            label, len(rows), st.mean(by_clip.values()), st.mean(group("zh")), st.mean(group("mixed")), st.mean(group("latin")),
            term_hit, term_all, term_hit / max(term_all, 1), sum(v < 0.5 for v in by_clip.values()), empty, errors, traditional,
            pct(secs, 0.5), pct(secs, 0.9), audio / max(sum(secs), 1e-9), langs.most_common(4),
        )
    )

if BASELINE in scores:
    print("\npaired against %s (same clips)" % BASELINE)
    base = scores[BASELINE]
    for label, by_clip in scores.items():
        if label == BASELINE:
            continue
        common = base.keys() & by_clip.keys()
        diffs = [by_clip[k] - base[k] for k in common]
        print(
            "%-26s mean_diff=%+.3f better=%d worse=%d same=%d"
            % (label, st.mean(diffs), sum(d > 0.005 for d in diffs), sum(d < -0.005 for d in diffs), sum(abs(d) <= 0.005 for d in diffs))
        )
