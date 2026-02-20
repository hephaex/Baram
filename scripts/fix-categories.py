#!/usr/bin/env python3
"""
Batch fix empty categories in crawled markdown files.
Reads each file's URL, fetches the Naver article page to extract section ID,
and updates the frontmatter category field.

Usage:
    python3 scripts/fix-categories.py --input ./output/raw --dry-run
    python3 scripts/fix-categories.py --input ./output/raw --workers 20
    nohup python3 scripts/fix-categories.py --input ./output/raw --workers 20 > fix-categories.log 2>&1 &
"""

import os
import sys
import re
import time
import argparse
import requests
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

SECTION_MAP = {
    "100": "politics",
    "101": "economy",
    "102": "society",
    "103": "culture",
    "104": "world",
    "105": "it",
}

SECTION_RE = re.compile(r'section=(\d+)')
URL_RE = re.compile(r'^url:\s*(.+)$', re.MULTILINE)
CATEGORY_RE = re.compile(r'^category:[ \t]*(.*)$', re.MULTILINE)

session = requests.Session()
session.headers.update({
    'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36'
})


def extract_section_from_html(url: str) -> str | None:
    """Fetch article page and extract section ID from meta tag."""
    try:
        resp = session.get(url, timeout=5)
        resp.raise_for_status()
        match = SECTION_RE.search(resp.text[:5000])  # Section is in head
        if match:
            sid = match.group(1)
            return SECTION_MAP.get(sid)
    except Exception:
        pass
    return None


def process_file(filepath: Path, dry_run: bool) -> tuple[str, str | None]:
    """Process a single markdown file. Returns (filename, category_or_None)."""
    try:
        content = filepath.read_text(encoding='utf-8')
    except Exception:
        return (filepath.name, None)

    # Check if category is already set (non-empty, non-whitespace)
    cat_match = CATEGORY_RE.search(content)
    if cat_match and cat_match.group(1).strip() not in ('', 'general'):
        return (filepath.name, f"already:{cat_match.group(1).strip()}")

    # Extract URL
    url_match = URL_RE.search(content)
    if not url_match:
        return (filepath.name, None)

    url = url_match.group(1).strip()
    if not url.startswith('http'):
        return (filepath.name, None)

    # Fetch category from Naver
    category = extract_section_from_html(url)
    if not category:
        return (filepath.name, None)

    if not dry_run:
        # Update the file
        new_content = CATEGORY_RE.sub(f'category: {category}', content, count=1)
        filepath.write_text(new_content, encoding='utf-8')

    return (filepath.name, category)


def main():
    parser = argparse.ArgumentParser(description='Fix empty categories in markdown files')
    parser.add_argument('--input', required=True, help='Input directory with markdown files')
    parser.add_argument('--workers', type=int, default=10, help='Number of parallel workers')
    parser.add_argument('--dry-run', action='store_true', help='Only report, do not modify files')
    parser.add_argument('--limit', type=int, default=0, help='Limit number of files to process (0=all)')
    args = parser.parse_args()

    input_dir = Path(args.input)
    if not input_dir.is_dir():
        print(f"Error: {input_dir} is not a directory")
        sys.exit(1)

    # Collect files with empty category
    files = sorted(input_dir.glob('*.md'))
    print(f"Total markdown files: {len(files)}")

    if args.limit > 0:
        files = files[:args.limit]
        print(f"Processing first {args.limit} files")

    stats = {'updated': 0, 'already': 0, 'failed': 0, 'total': len(files)}
    start_time = time.time()

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {executor.submit(process_file, f, args.dry_run): f for f in files}

        for i, future in enumerate(as_completed(futures), 1):
            filename, result = future.result()
            if result and result.startswith('already:'):
                stats['already'] += 1
            elif result:
                stats['updated'] += 1
            else:
                stats['failed'] += 1

            if i % 500 == 0:
                elapsed = time.time() - start_time
                rate = i / elapsed
                remaining = (stats['total'] - i) / rate if rate > 0 else 0
                print(f"[{i}/{stats['total']}] updated={stats['updated']} "
                      f"already={stats['already']} failed={stats['failed']} "
                      f"rate={rate:.1f}/s ETA={remaining/60:.1f}min")

    elapsed = time.time() - start_time
    print(f"\nDone in {elapsed:.1f}s")
    print(f"Updated: {stats['updated']}")
    print(f"Already had category: {stats['already']}")
    print(f"Failed: {stats['failed']}")
    print(f"Total: {stats['total']}")

    if args.dry_run:
        print("\n(Dry run - no files were modified)")


if __name__ == '__main__':
    main()
