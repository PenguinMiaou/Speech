#!/usr/bin/env python3
"""Copies every model file the app downloads to the project's ModelScope repository.

The files, their sizes and SHA-256 are read from the app's own source, so the copy
always matches what the app will check. A file already on ModelScope with the same
SHA-256 is skipped, so running this again carries on where an interrupted run stopped.

    MODELSCOPE_TOKEN=... python scripts/modelscope-seed.py [--check]
    MODELSCOPE_TOKEN=... python scripts/modelscope-seed.py --update-files DIR NAME...

--check only reports which files are missing. --update-files puts a release's update
files from DIR on ModelScope.
"""

import hashlib
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO = "penguinmiaou/speech-files"
BASE = f"https://www.modelscope.cn/models/{REPO}/resolve/master/"
ROOT = Path(__file__).resolve().parent.parent
SOURCES = [
    "src-tauri/src/local_stt/catalog.rs",
    "src-tauri/src/speakers.rs",
    "src-tauri/src/word_times.rs",
    "src-tauri/src/voice_isolation.rs",
]
EXTRA = ROOT / "tools" / "modelscope"


def entries():
    """(url, sha256, size) for every file the app downloads."""
    found = []
    for source in SOURCES:
        text = (ROOT / source).read_text()
        start = 0
        for match in re.finditer(r'sha256:\s*"([0-9a-f]{64})"', text):
            block = text[start : match.end()]
            start = match.end()
            size = int(re.findall(r"size_bytes:\s*([0-9_]+)", block)[-1].replace("_", ""))
            urls = re.findall(r'url:\s*"(https://[^"]+)"', block)
            if urls:
                url = urls[-1]
            else:
                repo = re.findall(r'repo:\s*"([^"]+)"', block)[-1]
                revision = re.findall(r'revision:\s*"([^"]+)"', block)[-1]
                name = re.findall(r'file_name:\s*"([^"]+)"', block)[-1]
                url = f"https://huggingface.co/{repo}/resolve/{revision}/{name}"
            found.append((url, match.group(1), size))
    return found


def modelscope_path(url):
    """The same mapping as `mirrors::modelscope_path` in the app."""
    hf = "https://huggingface.co/"
    gh = "https://github.com/"
    if url.startswith(hf):
        repo, rest = url[len(hf) :].split("/resolve/", 1)
        return f"hf/{repo}/{rest}"
    repo, rest = url[len(gh) :].split("/releases/download/", 1)
    return f"gh/{repo}/{rest}"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        return None


def remote_sha256(path):
    """The SHA-256 of the file kept at `path`: read from where ModelScope sends a download
    of a large file, or by reading a file it serves itself."""
    # A HEAD request is answered without the redirect, so ask for the first byte.
    request = urllib.request.Request(BASE + path, headers={"Range": "bytes=0-0"})
    try:
        urllib.request.build_opener(NoRedirect).open(request, timeout=30)
    except urllib.error.HTTPError as error:
        location = error.headers.get("Location", "")
        match = re.search(r"/lfs-objects/([0-9a-f]{2})/([0-9a-f]{2})/([0-9a-f]{60})", location)
        if error.code in (301, 302, 307) and match:
            return "".join(match.groups())
        return None
    except OSError:
        return None
    digest = hashlib.sha256()
    try:
        with urllib.request.urlopen(BASE + path, timeout=60) as response:
            for block in iter(lambda: response.read(1 << 20), b""):
                digest.update(block)
    except OSError:
        return None
    return digest.hexdigest()


def sha256_of(file):
    digest = hashlib.sha256()
    with open(file, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def login():
    from modelscope.hub.api import HubApi

    api = HubApi()
    api.login(os.environ["MODELSCOPE_TOKEN"])
    return api


def update_files(folder, names):
    """Puts a release's update files where the app looks for them on ModelScope, the
    manifest and its signature last so they never name a file not yet there."""
    api = login()
    late = ("update-manifest.json.sig", "update-manifest.json")
    ordered = [n for n in names if n not in late] + [n for n in late if n in names]
    failed = []
    for name in ordered:
        local = Path(folder) / name
        path = f"gh/PenguinMiaou/Speech/update-files/{name}"
        print(f"uploading {path}", flush=True)
        api.upload_file(repo_id=REPO, path_or_fileobj=str(local), path_in_repo=path, commit_message=name)
        if remote_sha256(path) != sha256_of(local):
            failed.append(name)
    print("failed: " + ", ".join(failed) if failed else "the update files are on ModelScope")
    return 1 if failed else 0


def main():
    if sys.argv[1:2] == ["--update-files"]:
        return update_files(sys.argv[2], sys.argv[3:])
    check = "--check" in sys.argv
    files = sorted(entries(), key=lambda entry: entry[2])
    print(f"{len(files)} files, {sum(f[2] for f in files) / 1e9:.2f} GB")
    missing = [f for f in files if remote_sha256(modelscope_path(f[0])) != f[1]]
    for url, _, size in files:
        state = "missing" if any(m[0] == url for m in missing) else "present"
        print(f"  {state:8} {size / 1e6:9.1f} MB  {modelscope_path(url)}")
    if check:
        return 1 if missing else 0

    api = login()
    for extra in sorted(EXTRA.rglob("*")):
        if extra.is_file():
            api.upload_file(
                repo_id=REPO,
                path_or_fileobj=str(extra),
                path_in_repo=str(extra.relative_to(EXTRA)),
                commit_message=f"{extra.name}",
            )
    work = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "modelscope-seed"
    work.mkdir(parents=True, exist_ok=True)
    failed = []
    for url, sha256, size in missing:
        path = modelscope_path(url)
        local = work / path.replace("/", "_")
        print(f"downloading {path}", flush=True)
        subprocess.run(
            ["curl", "-fL", "--retry", "5", "--retry-all-errors", "-C", "-", "-sS", "-o", str(local), url],
            check=False,
        )
        if not local.exists() or local.stat().st_size != size or sha256_of(local) != sha256:
            print(f"  wrong or incomplete: {path}", flush=True)
            local.unlink(missing_ok=True)
            failed.append(path)
            continue
        print(f"uploading {path}", flush=True)
        try:
            api.upload_file(
                repo_id=REPO, path_or_fileobj=str(local), path_in_repo=path, commit_message=path.rsplit("/", 1)[-1]
            )
        except Exception as error:  # the next run tries this file again
            print(f"  upload failed: {type(error).__name__}", flush=True)
            failed.append(path)
        local.unlink(missing_ok=True)
        if path not in failed and remote_sha256(path) != sha256:
            print(f"  not readable as uploaded: {path}", flush=True)
            failed.append(path)
    print("failed: " + ", ".join(failed) if failed else "all files are on ModelScope")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
