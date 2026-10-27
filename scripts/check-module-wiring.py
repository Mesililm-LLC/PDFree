#!/usr/bin/env python3
"""Fail if any Rust source file under a crate's src/ tree is unreachable.

Rust only compiles files that are part of the module tree, so a file that no
`mod` declaration reaches is silently skipped by `cargo fmt`, `cargo clippy`,
and `cargo test` — it can reference missing crates or APIs and still leave CI
green. This check closes that gap.

Usage: python scripts/check-module-wiring.py [workspace-root]
Exit codes: 0 = every file is wired in, 1 = orphaned file or missing module.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

PATH_ATTR = re.compile(r'^#\[\s*path\s*=\s*"([^"]+)"\s*\]')
MOD_DECL = re.compile(
    r'^(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;'
)


def parse_mods(file: Path) -> list[tuple[str, str | None]]:
    """Return (module-name, path-override) for file-backed modules in `file`.

    A `#[path = "..."]` attribute applies to the next `mod` declaration;
    intervening attributes (`#[cfg(...)]`) keep it, intervening code clears it.
    """
    mods: list[tuple[str, str | None]] = []
    pending_path: str | None = None
    for line in file.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        match = PATH_ATTR.match(stripped)
        if match:
            pending_path = match.group(1)
            continue
        match = MOD_DECL.match(stripped)
        if match:
            mods.append((match.group(1), pending_path))
            pending_path = None
            continue
        if not stripped or stripped.startswith(("#", "//", "/*", "*")):
            continue
        pending_path = None
    return mods


def module_target(declaring_file: Path, name: str, path_override: str | None,
                  crate_root: Path) -> Path:
    if path_override is not None:
        return (declaring_file.parent / path_override).resolve()
    if declaring_file.resolve() == crate_root.resolve():
        base = declaring_file.parent
    else:
        # `mod x;` inside src/foo.rs lives at src/foo/x.rs, not src/x.rs.
        base = declaring_file.with_suffix("")
    for candidate in (base / f"{name}.rs", base / name / "mod.rs"):
        if candidate.exists():
            return candidate
    return base / f"{name}.rs"


def reachable(crate_root: Path) -> tuple[set[Path], list[str]]:
    seen: set[Path] = set()
    missing: list[str] = []
    stack = [crate_root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        if not current.exists():
            missing.append(str(current))
            continue
        seen.add(current)
        for name, path_override in parse_mods(current):
            target = module_target(current, name, path_override, crate_root)
            stack.append(target)
    return seen, missing


def main() -> int:
    workspace = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    crates_dir = workspace / "crates"
    if not crates_dir.is_dir():
        print(f"error: no crates/ directory under {workspace}", file=sys.stderr)
        return 1

    failures = 0
    for crate_src in sorted(p for p in crates_dir.iterdir() if p.is_dir()):
        src = crate_src / "src"
        if not src.is_dir():
            continue
        roots = [r for r in (src / "lib.rs", src / "main.rs") if r.exists()]
        if not roots:
            continue
        seen: set[Path] = set()
        missing: list[str] = []
        for root in roots:
            reached, missing_roots = reachable(root)
            seen |= reached
            missing += missing_roots
        all_files = {p.resolve() for p in src.rglob("*.rs")}
        orphans = sorted(all_files - seen)
        for problem in missing:
            print(f"{crate_src.name}: declared module file not found: "
                  f"{problem}", file=sys.stderr)
            failures += 1
        for orphan in orphans:
            print(f"{crate_src.name}: orphaned file (never compiled): "
                  f"{orphan.relative_to(workspace)}", file=sys.stderr)
            failures += 1
        if not orphans and not missing:
            print(f"{crate_src.name}: {len(seen)} file(s) reachable from "
                  f"crate root")

    if failures:
        print(
            "\nThese files are skipped by rustfmt, clippy, and the test "
            "suite because no `mod` declaration reaches them. Wire them in "
            "or delete them.",
            file=sys.stderr,
        )
        return 1
    print("all src files are part of their crate's module tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
