#!/usr/bin/env python3
"""ローカルでの簡易 cargo audit（cargo-audit と同等の判定）。

advisory-db（Markdown+TOML frontmatter）と Cargo.lock を照合し、
既知の脆弱性を持つ依存を列挙する。バージョン範囲は cargo-audit と同様に
複合条件（カンマ区切り AND）を扱い、patched / unaffected のどちらにも
当てはまらない場合に「影響中（脆弱）」と判定する。

用法: python3 check-cargo-audit.py <advisory-db-dir> <Cargo.lock>
"""

from __future__ import annotations

import os
import re
import sys


def parse_version(v: str) -> tuple:
    """'1.2.3' -> (1, 2, 3)。"""
    parts = []
    for p in v.split("."):
        m = re.match(r"(\d+)", p)
        parts.append(int(m.group(1)) if m else 0)
    while len(parts) < 3:
        parts.append(0)
    return tuple(parts[:3])


def parse_lock(lock_path: str) -> dict:
    pkgs = {}
    cur = None
    with open(lock_path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("name = "):
                cur = line.split(" = ", 1)[1].strip().strip('"')
            elif line.startswith("version = ") and cur:
                ver = line.split(" = ", 1)[1].strip().strip('"')
                pkgs[cur] = ver
                cur = None
    return pkgs


def _op_match(version: tuple, op: str, bound: str) -> bool:
    b = parse_version(bound)
    if op == ">=":
        return version >= b
    if op == "<=":
        return version <= b
    if op == ">":
        return version > b
    if op == "<":
        return version < b
    if op == "=":
        return version == b
    if op == "^":
        return version[0] == b[0] and version >= b
    if op == "~":
        return version[0] == b[0] and version[1] == b[1] and version >= b
    return False


def _parse_range(spec: str):
    """'>= 1.51.6, < 1.52.0' を [('>=', '1.51.6'), ('<', '1.52.0')] に分解。"""
    clauses = []
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        m = re.match(r"^(>=|<=|>|<|=|\^|~)\s*(\S+)", part)
        if m:
            clauses.append((m.group(1), m.group(2)))
    return clauses


def _satisfies_range(version_str: str, spec: str) -> bool:
    """バージョンが範囲 spec（複合 AND）を満たすか。"""
    v = parse_version(version_str)
    clauses = _parse_range(spec)
    if not clauses:
        return False
    return all(_op_match(v, op, bound) for op, bound in clauses)


def parse_advisories(db_dir: str) -> dict:
    """{package: [(adv_id, aliases, patched, unaffected, informational)]}"""
    adv = {}
    crates_dir = os.path.join(db_dir, "crates")
    for pkg in os.listdir(crates_dir):
        pkg_dir = os.path.join(crates_dir, pkg)
        if not os.path.isdir(pkg_dir):
            continue
        entries = []
        for fn in os.listdir(pkg_dir):
            if not fn.endswith(".md"):
                continue
            with open(os.path.join(pkg_dir, fn), encoding="utf-8") as f:
                text = f.read()
            m = re.match(r"^```toml\n(.*?)\n```", text, re.S)
            if not m:
                continue
            fm = m.group(1)
            adv_id = _fm(fm, "id")
            aliases = _fm_list(fm, "aliases")
            patched = _fm_list(fm, "patched")
            unaffected = _fm_list(fm, "unaffected")
            informational = _fm(fm, "informational")
            entries.append((adv_id, aliases, patched, unaffected, informational))
        if entries:
            adv[pkg] = entries
    return adv


def _fm(fm: str, key: str) -> str:
    m = re.search(rf'^{key}\s*=\s*"([^"]*)"', fm, re.M)
    return m.group(1) if m else ""


def _fm_list(fm: str, key: str) -> list[str]:
    m = re.search(rf'^{key}\s*=\s*\[(.*?)\]', fm, re.M | re.S)
    if not m:
        return []
    return [s.strip().strip('"').strip("'") for s in m.group(1).split(",") if s.strip()]


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    db_dir, lock_path = sys.argv[1], sys.argv[2]
    pkgs = parse_lock(lock_path)
    adv = parse_advisories(db_dir)

    findings = []
    for pkg, ver in pkgs.items():
        if pkg not in adv:
            continue
        for adv_id, aliases, patched, unaffected, informational in adv[pkg]:
            # patched のいずれかに当てはまれば安全
            patched_ok = any(_satisfies_range(ver, spec) for spec in patched)
            # unaffected のいずれかに当てはまれば安全
            unaffected_ok = any(_satisfies_range(ver, spec) for spec in unaffected)
            if patched_ok or unaffected_ok:
                continue
            # patched / unaffected の指定が両方ない場合は情報不足（非脆弱とみなす）
            if not patched and not unaffected:
                continue
            findings.append((pkg, ver, adv_id, aliases, informational))

    if not findings:
        print("cargo audit 相当: 既知脆弱性は検出されませんでした（0 件）")
        return 0

    print(f"::error::cargo audit 相当: 既知脆弱性が {len(findings)} 件検出されました")
    for pkg, ver, adv_id, aliases, informational in findings:
        al = ", ".join(aliases) if aliases else "-"
        kind = f" (informational: {informational})" if informational else ""
        print(f"  - {pkg} {ver}: {adv_id} ({al}){kind}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
