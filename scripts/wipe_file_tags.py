#!/usr/bin/env python3
"""Strip all embedded tags from audio files under a directory.

After running this, a muserv rescan will clear the corresponding
file-sourced metadata and tags from the DB (user-sourced tags survive).
"""

import argparse
import sys
from pathlib import Path

try:
    import mutagen
except ImportError:
    print("mutagen not installed: pip install mutagen", file=sys.stderr)
    sys.exit(2)

AUDIO_EXTS = {
    ".mp3", ".flac", ".ogg", ".oga", ".opus", ".m4a", ".m4b", ".mp4",
    ".aac", ".wav", ".aiff", ".aif", ".wv", ".ape", ".mka",
}


def iter_audio(root: Path):
    for p in root.rglob("*"):
        if p.is_file() and p.suffix.lower() in AUDIO_EXTS:
            yield p


def wipe(path: Path) -> str:
    f = mutagen.File(path)
    if f is None:
        return "unsupported"
    if f.tags is None:
        return "no-tags"
    try:
        f.delete()
        f.save()
        return "wiped"
    except mutagen.MutagenError as e:
        return f"error: {e}"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root", type=Path, help="directory to walk")
    ap.add_argument("--dry-run", action="store_true",
                    help="list files that would be wiped, change nothing")
    ap.add_argument("-v", "--verbose", action="store_true",
                    help="print per-file status")
    args = ap.parse_args()

    if not args.root.is_dir():
        print(f"not a directory: {args.root}", file=sys.stderr)
        return 1

    counts = {"wiped": 0, "no-tags": 0, "unsupported": 0, "error": 0}
    for path in iter_audio(args.root):
        if args.dry_run:
            status = "would-wipe"
        else:
            status = wipe(path)
        bucket = "error" if status.startswith("error") else status.replace("would-", "")
        if bucket in counts:
            counts[bucket] += 1
        if args.verbose or status.startswith("error"):
            print(f"{status}\t{path}")

    print(
        f"wiped={counts['wiped']} no-tags={counts['no-tags']} "
        f"unsupported={counts['unsupported']} errors={counts['error']}"
    )
    return 0 if counts["error"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
