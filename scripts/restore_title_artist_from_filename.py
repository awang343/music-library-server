#!/usr/bin/env python3
"""Restore title/artist on FLAC files from filenames shaped 'Title-Artist.flac'.

Splits the filename stem on '-' to recover the tags wiped by an earlier run.
Default: split on the LAST '-' (so titles may themselves contain dashes).
Pass --first to split on the FIRST '-' instead.

By default, only fills in fields that are currently empty. Pass --overwrite
to replace existing values. Files whose stem has no '-' are reported and
skipped.
"""

import argparse
import sys
from pathlib import Path

try:
    from mutagen.flac import FLAC
    import mutagen
except ImportError:
    print("mutagen not installed: pip install mutagen", file=sys.stderr)
    sys.exit(2)


def parse_stem(stem: str, split_first: bool) -> tuple[str, str] | None:
    if "-" not in stem:
        return None
    title, _, artist = stem.partition("-") if split_first else stem.rpartition("-")
    title = title.strip()
    artist = artist.strip()
    if not title or not artist:
        return None
    return title, artist


def restore(path: Path, split_first: bool, overwrite: bool, dry_run: bool) -> str:
    parsed = parse_stem(path.stem, split_first)
    if parsed is None:
        return "no-dash"
    title, artist = parsed

    f = FLAC(path)
    have_title = bool(f.get("title"))
    have_artist = bool(f.get("artist"))

    write_title = overwrite or not have_title
    write_artist = overwrite or not have_artist

    if not write_title and not write_artist:
        return "already-set"

    changes = []
    if write_title:
        changes.append(f"title={title!r}")
    if write_artist:
        changes.append(f"artist={artist!r}")

    if dry_run:
        return "would-set " + ", ".join(changes)

    if write_title:
        f["title"] = title
    if write_artist:
        f["artist"] = artist
    f.save()
    return "set " + ", ".join(changes)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root", type=Path, help="directory to walk")
    ap.add_argument("--first", action="store_true",
                    help="split on the FIRST '-' (default: last '-')")
    ap.add_argument("--overwrite", action="store_true",
                    help="overwrite even if title/artist already present")
    ap.add_argument("--dry-run", action="store_true",
                    help="report what would change, modify nothing")
    ap.add_argument("-v", "--verbose", action="store_true",
                    help="print per-file status (errors and no-dash always printed)")
    args = ap.parse_args()

    if not args.root.is_dir():
        print(f"not a directory: {args.root}", file=sys.stderr)
        return 1

    counts = {"set": 0, "no-dash": 0, "already-set": 0, "error": 0}
    for path in args.root.rglob("*.flac"):
        if not path.is_file():
            continue
        try:
            status = restore(path, args.first, args.overwrite, args.dry_run)
        except (mutagen.MutagenError, OSError) as e:
            status = f"error: {e}"

        if status.startswith("error"):
            bucket = "error"
        elif status.startswith(("set", "would-set")):
            bucket = "set"
        elif status == "no-dash":
            bucket = "no-dash"
        else:
            bucket = "already-set"
        counts[bucket] += 1

        if args.verbose or bucket in {"error", "no-dash"}:
            print(f"{status}\t{path}")

    print(
        f"set={counts['set']} already-set={counts['already-set']} "
        f"no-dash={counts['no-dash']} errors={counts['error']}"
    )
    return 0 if counts["error"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
