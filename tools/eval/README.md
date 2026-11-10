# Dictation evaluation

Measures recognition and polish against the Typeless recordings on this machine. Recordings, references and results stay in `tools/eval/data/`, which git ignores. Scripts print counts and scores, never dictated text.

The reference is the final text Typeless produced, so scores are a proxy for accuracy. `FIX_LABELS=1` applies corrections from `data/label-fixes.json`, a `{"wrong": "right"}` map kept out of the repository.

## Sets

- Development set: `data/`, 70 clips. Use it for tuning.
- Holdout set: `data/holdout/`, the remaining clips. Run it once at the end of each delivery step and read the aggregates only. The 22-term vocabulary was drawn from these clips, so they are not independent for those terms.

## Commands

Prepare both sets. The shuffle is seeded, so the same Typeless database gives the same split:

```bash
python3 -I tools/eval/prep_manifest.py tools/eval/data
python3 -I tools/eval/prep_context.py tools/eval/data
```

Recognize with the packaged worker:

```bash
ot-stt-worker eval --model <gguf> --dir tools/eval/data --label <label> [--vocab-file <json>] [--context]
```

Polish recognized text through the real provider. This sends text to Groq, never audio:

```bash
set -a; source ~/.claude/.credentials/groq.env; set +a
cargo run --release --manifest-path src-tauri/Cargo.toml --example polish_eval -- --dir tools/eval/data --label <label>
```

Score every result file in a set:

```bash
FIX_LABELS=1 uv run --no-project --python 3.12 --with rapidfuzz --with opencc-python-reimplemented python -I tools/eval/score.py tools/eval/data
```
