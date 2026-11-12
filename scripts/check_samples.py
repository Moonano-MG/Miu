#!/usr/bin/env python3
"""Validate the sample documents shipped with the engine.

For every `samples/scenes/*.miu.json` file this script checks that:

* the file is valid JSON with the keys the loader requires (`viewport`, `nodes`);
* every declared asset path exists on disk next to the document;
* every node references a declared asset name, and node types are ones the loader
  understands;
* every animation targets a node name that exists and a property the engine animates.

It is a fast structural check, not a replacement for the Rust loader tests: it catches
edited samples that would fail at run time, without needing a Rust toolchain.

Usage:
    python3 scripts/check_samples.py
"""

from __future__ import annotations

import json
import pathlib
import sys

SCENES = pathlib.Path(__file__).resolve().parent.parent / "samples" / "scenes"
NODE_TYPES = {"group", "sprite", "text", "shape"}
PROPERTIES = {"x", "y", "rotation", "angle", "scale_x", "scaleY", "scale_y", "opacity", "alpha", "layer"}
ASSET_FIELDS = {"sprite": "texture", "text": "font"}


def walk(nodes: list[dict], names: list[str], problems: list[str], textures: set[str],
         fonts: set[str], where: str) -> None:
    for index, node in enumerate(nodes):
        label = f"{where}[{index}]"
        if not isinstance(node, dict):
            problems.append(f"{label}: node is not an object")
            continue
        kind = node.get("type")
        if kind not in NODE_TYPES:
            problems.append(f"{label}: unknown node type {kind!r}")
            continue
        name = node.get("name")
        if name:
            names.append(name)
        field = ASSET_FIELDS.get(kind)
        if field is not None:
            reference = node.get(field)
            pool = textures if kind == "sprite" else fonts
            if reference not in pool:
                problems.append(f"{label} ({name}): {field} {reference!r} is not declared in assets")
        children = node.get("children")
        if children:
            if not isinstance(children, list):
                problems.append(f"{label}: children must be an array")
            else:
                walk(children, names, problems, textures, fonts, f"{label}.children")


def check(path: pathlib.Path) -> list[str]:
    problems: list[str] = []
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        return [f"{path.name}: invalid JSON: {error}"]

    if not isinstance(document, dict):
        return [f"{path.name}: document must be an object"]
    viewport = document.get("viewport")
    if not (isinstance(viewport, list) and len(viewport) == 2):
        problems.append(f"{path.name}: viewport must be [width, height]")
    nodes = document.get("nodes", [])
    if not isinstance(nodes, list):
        problems.append(f"{path.name}: nodes must be an array")
        nodes = []

    assets = document.get("assets", {})
    textures = set((assets.get("textures") or {}).keys())
    fonts = set((assets.get("fonts") or {}).keys())
    for section, declared in (("textures", textures), ("fonts", fonts)):
        for name in declared:
            relative = assets[section][name]
            asset_path = path.parent / relative
            if not asset_path.is_file():
                problems.append(f"{path.name}: asset {section}.{name} -> {relative} is missing")

    names: list[str] = []
    walk(nodes, names, problems, textures, fonts, "nodes")

    for animation in document.get("animations", []) or []:
        label = animation.get("name", "animation")
        for track in animation.get("tracks", []) or []:
            target = track.get("node")
            if target not in names:
                problems.append(f"{path.name}: animation {label} targets unknown node {target!r}")
            property_name = track.get("property")
            if property_name not in PROPERTIES:
                problems.append(f"{path.name}: animation {label} animates unsupported property {property_name!r}")
            for key in track.get("keys", []) or []:
                if not (isinstance(key, list) and len(key) >= 2):
                    problems.append(f"{path.name}: animation {label} has a malformed keyframe")
    return problems


def main() -> int:
    documents = sorted(SCENES.glob("*.miu.json"))
    if not documents:
        print(f"no documents found under {SCENES}")
        return 1
    problems: list[str] = []
    for path in documents:
        problems.extend(check(path))
    for problem in problems:
        print(f"  {problem}")
    if problems:
        print(f"{len(problems)} problem(s) across {len(documents)} document(s)")
        return 1
    print(f"{len(documents)} document(s) valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
