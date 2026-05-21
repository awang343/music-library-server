#!/usr/bin/env python3
"""One-time sync: copy tracks.mtime into tracks.added_at for every row."""

import argparse
import sqlite3
import sys
from pathlib import Path

DEFAULT_DB = Path(__file__).resolve().parent.parent / "data" / "music-lib.db"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db", type=Path, default=DEFAULT_DB)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if not args.db.exists():
        print(f"db not found: {args.db}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(args.db, timeout=10.0)
    changed = conn.execute(
        "SELECT COUNT(*) FROM tracks WHERE added_at != mtime"
    ).fetchone()[0]
    total = conn.execute("SELECT COUNT(*) FROM tracks").fetchone()[0]
    print(f"{changed}/{total} rows will change")

    if args.dry_run or changed == 0:
        return 0

    with conn:
        conn.execute("UPDATE tracks SET added_at = mtime WHERE added_at != mtime")
    print(f"updated {changed} rows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
