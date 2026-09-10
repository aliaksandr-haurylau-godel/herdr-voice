#!/usr/bin/env python3
"""Regenerate the pinned model catalogue in src/stt/catalogue.rs.

Reads the Hugging Face API for each model, takes the current commit, and for
each of the three files the exact byte count and SHA-256. The API reports a
SHA-256 for LFS files; the other two are downloaded and hashed, because for
those it reports a git blob SHA-1 instead.

Prints the Rust table on standard output. It does not edit the file: a change
to a pinned model is deliberate, and a person should read the diff.
"""
import hashlib
import json
import sys
import urllib.request

REPOS = [
    ("tiny", "openai/whisper-tiny"),
    ("base", "openai/whisper-base"),
    ("small", "openai/whisper-small"),
    ("large-v3-turbo", "openai/whisper-large-v3-turbo"),
    ("medium", "openai/whisper-medium"),
    ("large-v3", "openai/whisper-large-v3"),
]
WANTED = ("model.safetensors", "config.json", "tokenizer.json")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "herdr-voice"})
    return urllib.request.urlopen(request, timeout=600)


def entry(repo):
    info = json.load(fetch(f"https://huggingface.co/api/models/{repo}"))
    revision = info["sha"]
    tree = json.load(fetch(f"https://huggingface.co/api/models/{repo}/tree/main"))
    files = {}
    for item in tree:
        if item["path"] in WANTED:
            files[item["path"]] = {
                "bytes": item["size"],
                "sha256": (item.get("lfs") or {}).get("oid"),
            }
    missing = set(WANTED) - set(files)
    if missing:
        sys.exit(f"{repo}: {sorted(missing)} not in the repository")
    for name, meta in files.items():
        if not meta["sha256"]:
            body = fetch(f"https://huggingface.co/{repo}/resolve/{revision}/{name}").read()
            if len(body) != meta["bytes"]:
                sys.exit(f"{repo}/{name}: {len(body)} bytes, the API said {meta['bytes']}")
            meta["sha256"] = hashlib.sha256(body).hexdigest()
    config = json.load(fetch(f"https://huggingface.co/{repo}/resolve/{revision}/config.json"))
    return revision, config["num_mel_bins"], files


def main():
    print(f"pub const MODELS: [Entry; {len(REPOS)}] = [")
    for identifier, repo in REPOS:
        revision, mel_bins, files = entry(repo)
        print("    Entry {")
        print(f'        identifier: "{identifier}",')
        print(f'        repo: "{repo}",')
        print(f'        revision: "{revision}",')
        print(f"        mel_bins: {mel_bins},")
        print("        files: [")
        for name in WANTED:
            meta = files[name]
            print("            File {")
            print(f'                name: "{name}",')
            print(f'                bytes: {meta["bytes"]:_},')
            print(f'                sha256: "{meta["sha256"]}",')
            print("            },")
        print("        ],")
        print("    },")
    print("];")


if __name__ == "__main__":
    main()
