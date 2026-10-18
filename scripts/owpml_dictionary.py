#!/usr/bin/env python3
"""Extract an element/attribute dictionary from the public OWPML C++ model.

Source: hancom-io/hwpx-owpml-model (Apache-2.0, Copyright 2022 Hancom Inc.).
The model's class files are the authoritative element registry of the OWPML
filter: `InitMap` registers every element name it can read/write, and
`ReadAttribute`/`WriteElement` name the attributes it understands.

This script only reads *names* (element names, attribute names, C++ class
names) and the namespace table; it never copies model code into this
repository. `document.validate` uses the result as a name dictionary, not as
a schema: a name missing from the dictionary is reported, never repaired.

Usage:
    python3 scripts/owpml_dictionary.py --source /path/to/hwpx-owpml-model \
        --out crates/hae-model/data/owpml-dictionary.json

The generated file starts with a `_comment` field naming this script and the
exact source revision, so the provenance of every element name is auditable.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import defaultdict

EXTRACTOR_VERSION = "1"

# `macro prefix -> namespace prefix` from OWPML/Class/MapMacro.h; parsed from the
# file so a model revision that renames a macro fails loudly instead of
# silently dropping elements.
MAP_MACRO = "OWPML/Class/MapMacro.h"
NAMESPACE_HEADER = "OWPML/Base/NamespacePrefix.h"
NAMESPACE_IMPL = "OWPML/Base/NamespacePrefix.cpp"
CLASS_DIR = "OWPML/Class"
# Element registrations also live in the shared base (`hp:switch`/`case`/`default`
# are registered by `OWPML/Base/Compatibility.cpp`), so the scan covers the whole
# model tree except the XML parser, ZIP reader and the sample application.
SCAN_DIRS = ("OWPML/Class", "OWPML/Base", "OWPML")

# OWPML_<NAME>_(ADD_)?REDIRECT_FUNC(...) declarations carry `#define
# OWPML_<NAME>_PREFIX <prefix>` in MapMacro.h, e.g. `HWPML_CORE_PREFIX hc`.
MACRO_FUNC_RE = re.compile(
    r"#define\s+OWPML_([A-Z0-9_]+?)_(?:ADD_)?REDIRECT_FUNC\d?\s*\\?\s*\n?\s*OWPML_BASE_(?:ADD_)?REDIRECT_FUNC\d?\(\s*([A-Z0-9_]+?)\s*,"
)

# Element registration: OWPML_PARALIST_ADD_REDIRECT_FUNC(tc, CTc, ID_PARA_Tc)
ADD_RE = re.compile(
    r"OWPML_([A-Z0-9_]+?)_ADD_REDIRECT_FUNC\(\s*(\w+)\s*,\s*(\w+)\s*,\s*(\w+)\s*\)"
)
# Element declaration: OWPML_PARALIST_REDIRECT_FUNC(CTr, tc); the `_REDIRECT_FUNC1`
# form renames the static variable: (parent, elementVariableName, elementName),
# where elementName may be a spaced expression like `config - item - set`.
DECL_RE = re.compile(
    r"OWPML_([A-Z0-9_]+?)_REDIRECT_FUNC(\d?)\(\s*(\w+)\s*,\s*([\w\s-]+?)\s*"
    r"(?:,\s*([\w\s-]+?)\s*)?\)",
)
# Explicit-namespace forms used by shared base classes.
BASE_ADD_RE = re.compile(
    r"OWPML_BASE_ADD_REDIRECT_FUNC2?\(\s*(\w+)\s*,\s*(\w+)\s*,\s*(\w+)\s*,\s*(\w+)\s*\)"
)
BASE_DECL_RE = re.compile(
    r"OWPML_(?:BASE|FOUNDATION)_REDIRECT_FUNC1?\(\s*(\w+)\s*,\s*(\w+)\s*,\s*(\w+)\s*(?:,\s*(\w+)\s*)?\)"
)
# Attribute names: Util::CXMLAttConverter::GetAttribute(att, L"name", ...)
ATTR_RE = re.compile(r'(?:Get|Set|Has|Remove)Attribute[A-Za-z]*\(\s*att\s*,\s*L"([^"]+)"')
# A class may inherit several bases (`class CCellMargin : public CExtObject,
# public CMarginAttrubute`), and shared attribute bundles implement their own
# `SetAttribute(CAttribute*)`. Both forms are collected.
INHERIT_RE = re.compile(r"\bclass\s+(\w+)\s*:\s*([^{;]+)\{")
METHOD_RE = re.compile(
    r"\b(?:bool|void|BOOL)\s+(C\w+)::(ReadAttribute|WriteAttribute|SetAttribute|WriteElement)\s*\("
)
# Part roots and self-registering elements name themselves: SetElemName(L"hh:head")
ELEM_NAME_RE = re.compile(r'SetElemName\(\s*L"([^"]+)"\s*\)')

# Namespace prefixes are C identifiers; the model writes shared namespaces with
# explicit tokens instead of a macro, so those are read from NamespacePrefix.h.
NS_URI_RE = re.compile(r'LPCWSTR\s+(str_\w+_NAMESPACE)\s*=\s*L"([^"]+)"')
NS_PREFIX_RE = re.compile(r"#define\s+(\w+_PREFIX)\s+(\w+)")
NS_PAIR_RE = re.compile(
    r"\{\s*(str_\w+_NAMESPACE)\s*,\s*_CONVERT_FINAL\((\w+)\)\s*\}"
)


def read(path: str) -> str:
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def block(text: str, start: int) -> str:
    """Body of the `{ ... }` block that follows `start`."""
    open_at = text.find("{", start)
    if open_at < 0:
        return ""
    depth = 0
    for i in range(open_at, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[open_at : i + 1]
    return text[open_at:]


def class_attr_pairs(text: str) -> list[tuple[str, str]]:
    """(class, attribute) for the attributes read/written by each class.

    A file may define several classes (`PType.cpp` holds `CParam`, `CPType`
    and `CLineSegArray`), so attributes are taken from the method bodies, not
    from the whole file.
    """
    out: list[tuple[str, str]] = []
    for m in METHOD_RE.finditer(text):
        for attr in ATTR_RE.findall(block(text, m.end())):
            # The model writes namespaced attribute names (`hp:required-namespace`)
            # where the document uses the local name.
            out.append((m.group(1), attr.rsplit(":", 1)[-1]))
    return out


def macro_prefixes(root: str) -> dict[str, str]:
    """`OWPML_<NAME>` -> namespace macro (`HWPML_CORE_PREFIX`)."""
    text = read(os.path.join(root, MAP_MACRO))
    # Each macro block looks like:
    #   #define OWPML_CORE_REDIRECT_FUNC(parentClass, elementName) \
    #       OWPML_BASE_REDIRECT_FUNC(HWPML_CORE_PREFIX, parentClass, elementName)
    found: dict[str, str] = {}
    for m in re.finditer(
        r"#define\s+OWPML_([A-Z0-9_]+?)_REDIRECT_FUNC\d?\([^)]*\)\s*\\\s*\n\s*"
        r"OWPML_(?:BASE|FOUNDATION)_REDIRECT_FUNC\d?\(\s*([A-Z0-9_]+?)\s*,",
        text,
    ):
        found.setdefault(m.group(1), m.group(2))
    if not found:
        raise SystemExit(f"no REDIRECT_FUNC macros found in {MAP_MACRO}")
    return found


def namespaces(root: str) -> tuple[dict[str, str], dict[str, str], list[dict[str, str]]]:
    """Namespace macro -> URI, macro -> model prefix, and the ordered table."""
    header = read(os.path.join(root, NAMESPACE_HEADER))
    impl = read(os.path.join(root, NAMESPACE_IMPL))
    uris = {name: uri for name, uri in NS_URI_RE.findall(impl)}
    prefixes = {name: prefix for name, prefix in NS_PREFIX_RE.findall(header)}
    table: list[dict[str, str]] = []
    macro_prefix: dict[str, str] = {}
    for macro, prefix_macro in NS_PAIR_RE.findall(impl):
        uri = uris.get(macro)
        prefix = prefixes.get(prefix_macro)
        if uri is None or prefix is None:
            raise SystemExit(f"unknown namespace pair {macro} / {prefix_macro}")
        table.append({"prefix": prefix, "uri": uri})
        macro_prefix[macro] = prefix
    if not table:
        raise SystemExit(f"no namespace pair table found in {NAMESPACE_IMPL}")
    # The class files spell namespaces as the short macro (`HWPML_CORE_PREFIX`).
    for name, prefix in prefixes.items():
        macro_prefix.setdefault(name, prefix)
    return uris, macro_prefix, table


def class_files(root: str) -> dict[str, dict[str, str]]:
    """Class name -> {path, cpp, h} for every file under OWPML/Class."""
    out: dict[str, dict[str, str]] = defaultdict(dict)
    base = os.path.join(root, CLASS_DIR)
    for dirpath, _dirnames, filenames in os.walk(base):
        for name in filenames:
            stem, ext = os.path.splitext(name)
            if ext not in (".cpp", ".h"):
                continue
            path = os.path.join(dirpath, name)
            out[stem][ext.lstrip(".")] = path
    return out


def model_files(root: str) -> list[tuple[str, str, str, str]]:
    """(path, relative path, stem, extension) for every model source file."""
    seen: set[str] = set()
    out: list[tuple[str, str, str, str]] = []
    for sub in SCAN_DIRS:
        base = os.path.join(root, sub)
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [
                d for d in sorted(dirnames) if d not in ("Expat", "Zip", "Hwp")
            ]
            for name in sorted(filenames):
                stem, ext = os.path.splitext(name)
                if ext not in (".cpp", ".h"):
                    continue
                path = os.path.join(dirpath, name)
                if path in seen:
                    continue
                seen.add(path)
                out.append((path, os.path.relpath(path, root), stem, ext))
    return out


def scan(root: str) -> dict:
    _uris, ns_prefix, ns_table = namespaces(root)
    by_owpml = macro_prefixes(root)
    # `OWPML_CORE` -> `HWPML_CORE_PREFIX` -> `hc`
    macro_ns: dict[str, str] = {}
    for name, macro in by_owpml.items():
        prefix = ns_prefix.get(macro)
        if prefix is None:
            raise SystemExit(f"macro {name} names unknown namespace {macro}")
        macro_ns[name] = prefix
    # Explicit namespace tokens (`OWPML_BASE_ADD_REDIRECT_FUNC(HWPML_CORE_PREFIX, ..)`).
    macro_ns.update({k: v for k, v in ns_prefix.items()})
    macro_ns.update({k.replace("str_", "").replace("_NAMESPACE", "").lower(): v
                     for k, v in ns_prefix.items()})
    # (ns, name) -> {classes, files, ids, root}. A name may be registered by
    # several classes (the model keys children by parent, and `hp:offset`
    # exists both as a character-shape offset and as a page-border offset), so
    # the dictionary keeps every registration and unions their attribute sets.
    registrations: dict[tuple[str, str], dict] = {}
    # Names that only appear as a child declaration (`OWPML_<NS>_REDIRECT_FUNC
    # (ParentClass, child)`): the name is known, its own class is not.
    declared_names: dict[tuple[str, str], list[str]] = {}
    class_attrs: dict[str, list[str]] = defaultdict(list)
    class_bases: dict[str, list[str]] = defaultdict(list)
    # parent class -> declared child elements
    child_decls: dict[str, list[str]] = defaultdict(list)
    unresolved: list[str] = []

    def register(ns: str, name: str, cls: str, element_id: str, rel: str, root: bool = False) -> None:
        entry = registrations.setdefault(
            (ns, name), {"classes": [], "files": [], "ids": [], "root": root}
        )
        if cls and cls not in entry["classes"]:
            entry["classes"].append(cls)
        if rel not in entry["files"]:
            entry["files"].append(rel)
        if element_id:
            entry["ids"].append(element_id)
        entry["root"] = entry["root"] or root

    for path, rel, stem, ext in model_files(root):
        text = read(path)
        if ext == ".cpp":
            for cls, attr in class_attr_pairs(text):
                if attr not in class_attrs[cls]:
                    class_attrs[cls].append(attr)
            for match in ELEM_NAME_RE.finditer(text):
                name = match.group(1)
                prefix_name = name.split(":", 1)
                ns = prefix_name[0] if len(prefix_name) == 2 else ""
                local = prefix_name[-1]
                # The nearest class-scoped definition before the call is the
                # owning class (a file may register several part roots).
                owner = ""
                for m2 in re.finditer(r"\b(\w+)::\w+\s*\(", text[: match.start()]):
                    owner = m2.group(1)
                if not owner:
                    classes = re.findall(r"\b(C\w+)::", text)
                    owner = max(classes, key=classes.count) if classes else stem
                register(ns, local, owner, "", rel, root=True)
            for m in ADD_RE.finditer(text):
                ns = macro_ns.get(m.group(1))
                if ns is None:
                    unresolved.append(f"{rel}: macro {m.group(1)}")
                    continue
                register(ns, m.group(2), m.group(3), m.group(4), rel)
            for m in BASE_ADD_RE.finditer(text):
                token = m.group(1)
                ns = macro_ns.get(token, token)
                register(ns, m.group(2), m.group(3), m.group(4), rel)
            add_by_var: dict[str, tuple[str, str]] = {}
            for m in list(ADD_RE.finditer(text)) + list(BASE_ADD_RE.finditer(text)):
                variable, cls, element_id = m.group(2), m.group(3), m.group(4)
                add_by_var.setdefault(variable, (cls, element_id))
            for m in DECL_RE.finditer(text):
                if m.group(1).endswith("_ADD"):
                    continue  # `OWPML_<NS>_ADD_REDIRECT_FUNC` is an ADD_RE match
                if m.group(1) in ("BASE", "FOUNDATION"):
                    continue  # explicit-namespace form, handled by BASE_DECL_RE
                ns = macro_ns.get(m.group(1))
                if ns is None:
                    unresolved.append(f"{rel}: macro {m.group(1)}")
                    continue
                # `_REDIRECT_FUNC1(parent, variable, element)` names the element
                # third; the plain form names it second. The element name may be
                # a spaced expression (`config - item - set`) that the XML spells
                # with hyphens.
                variable = m.group(4).replace(" ", "")
                element = (
                    m.group(5).replace(" ", "") if m.group(2) == "1" and m.group(5) else variable
                )
                key = (ns, element)
                child_decls[m.group(3)].append(f"{ns}:{element}")
                declared_names.setdefault(key, [])
                if rel not in declared_names[key]:
                    declared_names[key].append(rel)
                if element != variable:
                    # The ADD registration uses the C++ token; the XML name is
                    # the hyphenated one, so the class moves to the real name.
                    cls, element_id = add_by_var.get(variable, ("", ""))
                    if cls:
                        register(ns, element, cls, element_id, rel)
            for m in BASE_DECL_RE.finditer(text):
                # The first argument is either a namespace macro or a class
                # name; only names that resolve to a namespace are elements.
                ns = macro_ns.get(m.group(1))
                if ns is None:
                    continue
                key = (ns, m.group(3))
                child_decls[m.group(2)].append(f"{ns}:{m.group(3)}")
                declared_names.setdefault(key, [])
                if rel not in declared_names[key]:
                    declared_names[key].append(rel)
        else:
            for m in INHERIT_RE.finditer(text):
                for base_part in m.group(2).split(","):
                    base = re.search(r"(\w+)\s*$", base_part.strip())
                    if base and base.group(1) not in class_bases[m.group(1)]:
                        class_bases[m.group(1)].append(base.group(1))

    # Attribute closure over the C++ base chain (attributes of a base class are
    # read by the derived element's ReadAttribute; a class may have several).
    def chain(cls: str) -> list[str]:
        seen: set[str] = set()
        out: list[str] = []

        def walk(name: str) -> None:
            if not name or name in seen:
                return
            seen.add(name)
            out.append(name)
            for base in class_bases.get(name, []):
                walk(base)

        walk(cls)
        return out

    def closure(cls: str) -> list[str]:
        out: list[str] = []
        for c in chain(cls):
            for attr in class_attrs.get(c, []):
                if attr not in out:
                    out.append(attr)
        return sorted(out)

    def child_closure(cls: str) -> list[str]:
        out: list[str] = []
        for c in chain(cls):
            for child in child_decls.get(c, []):
                if child not in out:
                    out.append(child)
        return sorted(out)

    elements = []
    for key in sorted(set(registrations) | set(declared_names)):
        ns, name = key
        reg = registrations.get(key, {"classes": [], "files": [], "ids": [], "root": False})
        classes_of = list(reg["classes"])
        files = sorted(set(reg["files"]) | set(declared_names.get(key, [])))
        attributes: list[str] = []
        children: list[str] = []
        for cls in classes_of:
            for attr in closure(cls):
                if attr not in attributes:
                    attributes.append(attr)
            for child in child_closure(cls):
                if child not in children:
                    children.append(child)
        elements.append(
            {
                "ns": ns,
                "name": name,
                "classes": sorted(classes_of),
                "files": files,
                "ids": sorted(set(reg["ids"])),
                "attributes": sorted(attributes),
                "children": sorted(children),
                "root": reg["root"],
            }
        )

    classes = {
        cls: {"bases": class_bases.get(cls, []), "attributes": sorted(set(attrs))}
        for cls, attrs in sorted(class_attrs.items())
    }

    revision = "unknown"
    try:
        revision = (
            subprocess.run(
                ["git", "-C", root, "rev-parse", "HEAD"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
            or "unknown"
        )
    except Exception:  # pragma: no cover - source may be a plain directory
        pass
    tree_hash = hashlib.sha256()
    for el in elements:
        tree_hash.update(f"{el['ns']}:{el['name']}\n".encode())

    return {
        "_comment": (
            "Generated by scripts/owpml_dictionary.py from the public OWPML model "
            "(hancom-io/hwpx-owpml-model, Apache-2.0). Element and attribute names "
            "only; no model code. Do not edit by hand."
        ),
        "extractorVersion": EXTRACTOR_VERSION,
        "sourceRevision": revision,
        "namespaces": ns_table,
        "elements": elements,
        "classes": classes,
        "unresolved": sorted(unresolved),
        "elementFingerprint": tree_hash.hexdigest(),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True, help="hwpx-owpml-model checkout")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    data = scan(args.source)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=1, sort_keys=False)
        f.write("\n")
    by_ns: dict[str, int] = defaultdict(int)
    with_attrs = 0
    for el in data["elements"]:
        by_ns[el["ns"]] += 1
        with_attrs += bool(el["attributes"])
    print(
        f"{len(data['elements'])} elements, {with_attrs} with attributes, "
        f"{len(data['classes'])} classes, {len(data['unresolved'])} unresolved"
    )
    print("by namespace: " + ", ".join(f"{k}={v}" for k, v in sorted(by_ns.items())))
    unknown_macros = data["unresolved"][:10]
    if unknown_macros:
        print("unresolved examples:", *unknown_macros, sep="\n  ")
    return 0


if __name__ == "__main__":
    sys.exit(main())
