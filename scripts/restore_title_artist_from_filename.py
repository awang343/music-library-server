#!/usr/bin/env python3
"""Restore title/artist on FLAC files from filenames shaped 'Title-Artist.flac'.

For each .flac under the given root, split the stem on '-' to recover
the wiped tags. Behavior depends on how many dashes the stem contains:

  0 dashes  -> skip (reported as 'no-dash')
  1 dash    -> split unambiguously
  2+ dashes -> ambiguous; by default, prompt the user interactively
               with both candidate splits and these choices:
                 1  use first-dash split
                 2  use last-dash split
                 s  skip this file
                 q  quit (no further writes)
                 1a apply first-dash to all remaining ambiguous files
                 2a apply last-dash to all remaining ambiguous files

Use --ambiguous {first,last,skip} to pick a non-interactive policy and
suppress prompts. By default only empty fields are filled; pass
--overwrite to replace existing tag values.
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


def split_on(stem: str, first: bool) -> tuple[str, str] | None:
    title, sep, artist = stem.partition("-") if first else stem.rpartition("-")
    if not sep:
        return None
    title = title.strip()
    artist = artist.strip()
    if not title or not artist:
        return None
    return title, artist


def choose_ambiguous(path: Path, sticky: dict) -> tuple[str, str] | None:
    """Return (title, artist) or None to skip. May set sticky['policy'] to
    'first' or 'last' or 'quit' to apply to subsequent files."""
    if sticky.get("policy") in {"first", "last"}:
        return split_on(path.stem, sticky["policy"] == "first")

    first_split = split_on(path.stem, True)
    last_split = split_on(path.stem, False)

    print(f"\nambiguous: {path}", file=sys.stderr)
    if first_split:
        print(f"  1) first-dash  title={first_split[0]!r} artist={first_split[1]!r}",
              file=sys.stderr)
    if last_split:
        print(f"  2) last-dash   title={last_split[0]!r} artist={last_split[1]!r}",
              file=sys.stderr)
    while True:
        try:
            choice = input("choose [1/2/s=skip/q=quit/1a/2a]: ").strip().lower()
        except EOFError:
            print("EOF on stdin; skipping", file=sys.stderr)
            return None
        if choice == "1" and first_split:
            return first_split
        if choice == "2" and last_split:
            return last_split
        if choice == "s":
            return None
        if choice == "q":
            sticky["policy"] = "quit"
            return None
        if choice == "1a" and first_split:
            sticky["policy"] = "first"
            return first_split
        if choice == "2a" and last_split:
            sticky["policy"] = "last"
            return last_split
        print("invalid choice", file=sys.stderr)


def restore(
    path: Path,
    ambiguous_mode: str,
    overwrite: bool,
    dry_run: bool,
    sticky: dict,
) -> str:
    stem = path.stem
    dash_count = stem.count("-")
    if dash_count == 0:
        return "no-dash"

    ambiguous = dash_count > 1
    if not ambiguous:
        parsed = split_on(stem, True)
    else:
        if ambiguous_mode == "first":
            parsed = split_on(stem, True)
        elif ambiguous_mode == "last":
            parsed = split_on(stem, False)
        elif ambiguous_mode == "skip":
            return "ambiguous-skipped"
        else:  # "ask"
            parsed = choose_ambiguous(path, sticky)
            if sticky.get("policy") == "quit":
                return "quit"
            if parsed is None:
                return "ambiguous-skipped"

    if parsed is None:
        return "no-dash"
    title, artist = parsed

    f = FLAC(path)
    have_title = bool(f.get("title"))
    have_artist = bool(f.get("artist"))

    # Ambiguous files always overwrite — the user (or --ambiguous policy)
    # made an explicit choice, so honor it regardless of existing tags.
    force = overwrite or ambiguous
    write_title = force or not have_title
    write_artist = force or not have_artist

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
    ap.add_argument(
        "--ambiguous",
        choices=["ask", "first", "last", "skip"],
        default="ask",
        help="policy for stems with 2+ dashes (default: ask)",
    )
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

    if args.ambiguous == "ask" and not sys.stdin.isatty():
        print("stdin is not a tty; --ambiguous=ask cannot prompt. "
              "Re-run with --ambiguous=first|last|skip.", file=sys.stderr)
        return 1

    counts = {
        "set": 0, "no-dash": 0, "already-set": 0,
        "ambiguous-skipped": 0, "error": 0,
    }
    sticky: dict = {}
    quit_early = False

    for path in args.root.rglob("*.flac"):
        if not path.is_file():
            continue
        try:
            status = restore(path, args.ambiguous, args.overwrite,
                             args.dry_run, sticky)
        except (mutagen.MutagenError, OSError) as e:
            status = f"error: {e}"

        if sticky.get("policy") == "quit":
            quit_early = True
            print("quit requested; stopping", file=sys.stderr)
            break

        if status.startswith("error"):
            bucket = "error"
        elif status.startswith(("set", "would-set")):
            bucket = "set"
        elif status == "no-dash":
            bucket = "no-dash"
        elif status == "ambiguous-skipped":
            bucket = "ambiguous-skipped"
        else:
            bucket = "already-set"
        counts[bucket] += 1

        if args.verbose or bucket in {"error", "no-dash", "ambiguous-skipped"}:
            print(f"{status}\t{path}")

    print(
        f"set={counts['set']} already-set={counts['already-set']} "
        f"ambiguous-skipped={counts['ambiguous-skipped']} "
        f"no-dash={counts['no-dash']} errors={counts['error']}"
        + (" (stopped early)" if quit_early else "")
    )
    return 0 if counts["error"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
