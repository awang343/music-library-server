#!/usr/bin/env python3
"""Strip embedded tags from audio files under a directory, optionally
preserving a whitelist of fields (default: artist, title).

After running this, a muserv rescan will clear the corresponding
file-sourced metadata and tags from the DB. User-sourced tags
(source='user' in track_tags) and any preserved fields survive.

Uses mutagen's "easy" interface to normalize keys across formats
(mp3/flac/mp4/ogg/opus/etc.). Tags outside the easy interface
(e.g. embedded art, lyrics, custom frames) get wiped — they're not
re-readable through easy and aren't candidates for --keep.
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


def wipe(path: Path, keep: set[str]) -> str:
    # Capture preserved fields via the easy interface first.
    saved: dict[str, list[str]] = {}
    if keep:
        easy = mutagen.File(path, easy=True)
        if easy is not None and easy.tags is not None:
            for k in keep:
                v = easy.tags.get(k)
                if v:
                    saved[k] = list(v)

    # Strip everything using the raw interface.
    raw = mutagen.File(path)
    if raw is None:
        return "unsupported"
    if raw.tags is None and not saved:
        return "no-tags"
    try:
        raw.delete()
        raw.save()
    except mutagen.MutagenError as e:
        return f"error: {e}"

    if not saved:
        return "wiped"

    # Restore preserved fields via the easy interface.
    easy = mutagen.File(path, easy=True)
    if easy is None:
        return "wiped-no-restore"
    try:
        if easy.tags is None:
            easy.add_tags()
        for k, v in saved.items():
            easy[k] = v
        easy.save()
    except (mutagen.MutagenError, KeyError, ValueError) as e:
        return f"error-restoring: {e}"
    return f"wiped-kept({','.join(sorted(saved))})"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root", type=Path, help="directory to walk")
    ap.add_argument(
        "--keep",
        default="artist,title",
        help="comma-separated easy-interface keys to preserve "
             "(default: artist,title; pass empty string to strip everything)",
    )
    ap.add_argument("--dry-run", action="store_true",
                    help="list files that would be wiped, change nothing")
    ap.add_argument("-v", "--verbose", action="store_true",
                    help="print per-file status")
    args = ap.parse_args()

    if not args.root.is_dir():
        print(f"not a directory: {args.root}", file=sys.stderr)
        return 1

    keep = {k.strip().lower() for k in args.keep.split(",") if k.strip()}
    print(f"keeping: {sorted(keep) or '(nothing)'}", file=sys.stderr)

    counts = {"wiped": 0, "no-tags": 0, "unsupported": 0, "error": 0}
    for path in iter_audio(args.root):
        if args.dry_run:
            status = "would-wipe"
            bucket = "wiped"
        else:
            status = wipe(path, keep)
            if status.startswith("error"):
                bucket = "error"
            elif status.startswith("wiped"):
                bucket = "wiped"
            else:
                bucket = status
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
