Write a Python 3.11 script named verify_zip.py using only the standard library.

Purpose: compare an original HWPX (a ZIP file) with an edited HWPX and report, as JSON on stdout, whether ZIP-level preservation rules hold.

Usage: python3 verify_zip.py ORIGINAL.hwpx EDITED.hwpx [--changed NAME ...]
--changed lists entry names that are allowed to differ (may be given multiple times or zero times).

Read raw (still-compressed) entry bytes WITHOUT decompressing: for each zipfile.ZipInfo use info.header_offset; at that offset read the 30-byte local file header (signature 0x04034b50, little-endian; filename length at bytes 26-27, extra length at bytes 28-29), skip filename and extra, then read info.compress_size bytes. Raise ValueError on bad signature.

Checks (each a boolean in the JSON):
- "mimetype_first_stored": in EDITED, the first entry is named "mimetype" and its compress_type == zipfile.ZIP_STORED.
- "same_entry_order": the names of EDITED, in order, equal ORIGINAL's names in order, except names that exist only in EDITED may appear after all original names (appended at the end).
- "no_missing_entries": every ORIGINAL name exists in EDITED.
- "compression_preserved": for every name in both files, compress_type is equal.
- "unchanged_raw_identical": for every name in both files that is NOT in --changed, raw compressed bytes are identical AND CRC32 equal.
- "changed_entries_differ": for every name in --changed, the decompressed bytes differ between the two files (report false if a name is missing from either file).
- "edited_crc_ok": zipfile.ZipFile(EDITED).testzip() returns None.

Also output: "added_entries" (names only in EDITED), "raw_mismatches" (list of names failing unchanged_raw_identical), "ok" (true only if all booleans true).
Exit code 0 if ok, 1 if not ok, 2 on usage or I/O error (print an error JSON {"error": "..."} on stdout in that case).
Use argparse. No third-party packages. Keep it under 120 lines.
