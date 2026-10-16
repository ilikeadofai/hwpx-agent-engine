```python
#!/usr/bin/env python3
"""Verify ZIP-level preservation rules between an original and an edited HWPX file."""

import argparse
import json
import struct
import sys
import zipfile

LOCAL_HEADER_SIG = 0x04034B50
LOCAL_HEADER_SIZE = 30


def raw_bytes(fh, info):
    """Return the still-compressed bytes of *info* without decompressing it."""
    fh.seek(info.header_offset)
    header = fh.read(LOCAL_HEADER_SIZE)
    if len(header) != LOCAL_HEADER_SIZE:
        raise ValueError("truncated local file header for %r" % info.filename)
    if struct.unpack_from("<I", header, 0)[0] != LOCAL_HEADER_SIG:
        raise ValueError("bad local file header signature for %r" % info.filename)
    name_len, extra_len = struct.unpack_from("<HH", header, 26)
    fh.seek(info.header_offset + LOCAL_HEADER_SIZE + name_len + extra_len)
    data = fh.read(info.compress_size)
    if len(data) != info.compress_size:
        raise ValueError("truncated compressed data for %r" % info.filename)
    return data


def verify(orig_path, edited_path, changed):
    with zipfile.ZipFile(orig_path) as zo, zipfile.ZipFile(edited_path) as ze:
        o_infos, e_infos = zo.infolist(), ze.infolist()
        o_names = [i.filename for i in o_infos]
        e_names = [i.filename for i in e_infos]
        o_map = {i.filename: i for i in o_infos}
        e_map = {i.filename: i for i in e_infos}
        o_set, e_set = set(o_names), set(e_names)
        changed_set = set(changed)

        first = e_infos[0] if e_infos else None
        mimetype_first_stored = bool(first) and first.filename == "mimetype" \
            and first.compress_type == zipfile.ZIP_STORED

        same_entry_order = e_names[:len(o_names)] == o_names and \
            all(n not in o_set for n in e_names[len(o_names):])

        no_missing_entries = o_set <= e_set

        compression_preserved = all(
            o_map[n].compress_type == e_map[n].compress_type
            for n in o_names if n in e_set)

        raw_mismatches = []
        with open(orig_path, "rb") as fo, open(edited_path, "rb") as fe:
            for n in o_names:
                if n not in e_set or n in changed_set:
                    continue
                if o_map[n].CRC != e_map[n].CRC or \
                        raw_bytes(fo, o_map[n]) != raw_bytes(fe, e_map[n]):
                    raw_mismatches.append(n)
        unchanged_raw_identical = not raw_mismatches

        changed_entries_differ = True
        for n in changed:
            if n not in o_set or n not in e_set or zo.read(n) == ze.read(n):
                changed_entries_differ = False
                break

        edited_crc_ok = ze.testzip() is None

        added_entries = [n for n in e_names if n not in o_set]

    result = {
        "mimetype_first_stored": mimetype_first_stored,
        "same_entry_order": same_entry_order,
        "no_missing_entries": no_missing_entries,
        "compression_preserved": compression_preserved,
        "unchanged_raw_identical": unchanged_raw_identical,
        "changed_entries_differ": changed_entries_differ,
        "edited_crc_ok": edited_crc_ok,
        "added_entries": added_entries,
        "raw_mismatches": raw_mismatches,
    }
    result["ok"] = all(result[k] for k in (
        "mimetype_first_stored", "same_entry_order", "no_missing_entries",
        "compression_preserved", "unchanged_raw_identical",
        "changed_entries_differ", "edited_crc_ok"))
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Check ZIP-level preservation rules for HWPX files.")
    parser.add_argument("original", help="original HWPX (zip) file")
    parser.add_argument("edited", help="edited HWPX (zip) file")
    parser.add_argument("--changed", action="append", default=[], metavar="NAME",
                        help="entry name allowed to differ (repeatable)")
    args = parser.parse_args(argv)

    try:
        result = verify(args.original, args.edited, args.changed)
    except (OSError, zipfile.BadZipFile, ValueError) as exc:
        print(json.dumps({"error": str(exc)}))
        return 2

    print(json.dumps(result, indent=2))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
```