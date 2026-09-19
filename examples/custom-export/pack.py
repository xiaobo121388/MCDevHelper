"""Example MCDH exporter. Requires Python 3; uses only the standard library."""

import argparse
import os
from pathlib import Path
import stat
import sys
import zipfile


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", default=os.environ.get("MCDH_INPUT_DIR"))
    parser.add_argument("--output-dir", default=os.environ.get("MCDH_OUTPUT_DIR"))
    parser.add_argument("--name", default="custom-export.zip")
    args = parser.parse_args()
    if not args.input or not args.output_dir:
        parser.error("--input and --output-dir (or MCDH environment variables) are required")
    if Path(args.name).name != args.name or not args.name or args.name in (".", ".."):
        parser.error("--name must be a file name, not a path")
    source = Path(args.input).resolve(strict=True)
    output_dir = Path(args.output_dir).resolve(strict=True)
    if not source.is_dir() or not output_dir.is_dir():
        parser.error("input and output must be existing directories")
    if output_dir == source or source in output_dir.parents:
        parser.error("output must be outside input")
    output = output_dir / args.name
    print("Reading: {}".format(source), flush=True)
    count = 0
    # This example preserves all regular content. Supply your own filtering/preprocessing here.
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for directory, dirs, files in os.walk(source, followlinks=False):
            root = Path(directory)
            dirs[:] = sorted(name for name in dirs if not is_link(root / name))
            for name in dirs + sorted(files):
                entry = root / name
                if is_link(entry) or not (entry.is_file() or entry.is_dir()):
                    continue
                archive.write(entry, entry.relative_to(source).as_posix())
                count += 1
    print("Created {} ({} entries)".format(output.name, count), flush=True)


def is_link(entry):
    attributes = getattr(entry.lstat(), "st_file_attributes", 0)
    return entry.is_symlink() or bool(attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


if __name__ == "__main__":
    main()
