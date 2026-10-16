Write a Python 3.11 script named strip_lineseg_tail.py using only the standard library.

Usage: python3 strip_lineseg_tail.py SRC.hwpx DST.hwpx MARKER

An HWPX file is a ZIP. Section XML entries are named like "Contents/section<N>.xml" (match with the regex ^Contents/section\d+\.xml$). In a section XML, the root element is at depth 0. A top-level paragraph is an element named "hp:p" that opens at depth 1 (direct child of the root). A paragraph may contain an "hp:linesegarray" element as a direct child (opening at depth 2), either as open tag ... matching close tag, or as a self-closing tag.

Tokenizing: work on raw bytes. Use re.compile(rb'<\?.*?\?>|<!--.*?-->|<(/?)([A-Za-z0-9_:.-]+)[^>]*?(/?)>', re.S) with finditer. Skip matches where group(2) is None (processing instructions and comments). An open tag (not self-closing) increases depth by 1 after it; a close tag decreases depth by 1 before it; a self-closing tag does not change depth. Attribute values in these files never contain '>'.

Steps:
1. For every section entry, compute the byte span [start of open tag, end of close tag) of each top-level paragraph (a self-closing top-level hp:p is a span of just that tag). A paragraph matches if MARKER.encode('utf-8') occurs inside its span. Across all sections exactly one top-level paragraph must match; otherwise print {"error": "..."} and exit 2.
2. Let k be the index (0-based, among top-level hp:p of that section) of the matched paragraph. In that section only, collect the spans of every depth-2 hp:linesegarray element (open tag start to matching close tag end, or the self-closing tag span) that belongs to a top-level paragraph with index >= k. Do not touch linesegarray at other depths and do not touch paragraphs with index < k.
3. New section bytes = input bytes with exactly those spans removed (byte splice). Every other byte stays identical. Never parse-and-reserialize XML.
4. If DST already exists, print {"error": "..."} and exit 2 before writing anything. Write DST with zipfile: same entry order as SRC, each entry written with zipfile.ZipFile.writestr(original ZipInfo object, data, compress_type=original compress_type). Data is the new bytes for the edited section and the original uncompressed bytes for every other entry. The first entry must be "mimetype" and stored (ZIP_STORED); if SRC violates this, exit 2.
5. Print one JSON line: {"section": name, "paragraphIndex": k, "removed": count_of_removed_spans, "paragraphsInSection": number_of_top_level_paragraphs}. Exit 0.

Use json.dumps(..., ensure_ascii=False). Wrap file I/O errors into the error JSON with exit 2.
