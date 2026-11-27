#!/usr/bin/env python3
"""OpenAPI 契約の破壊的変更（breaking change）を検出する。

oasdiff 相当の差分チェック。旧仕様（base）と新仕様（head）の 2 つの
OpenAPI 3.x ドキュメントを比較し、クライアントにとって破壊的な変更を
列挙する。破壊的変更が 1 件以上あれば終了コード 1 で失敗する。

検出対象（破壊的）:
  - パスの削除
  - 既存パス上の HTTP メソッド（operation）の削除
  - operationId の削除 / 名前変更
  - リクエストパラメータ（query/path/header/cookie）の削除、必須化
  - リクエストボディ必須フィールド（required）の削除
  - レスポンス HTTP ステータスコードの削除、必須フィールドの削除
  - スキーマフィールドの型変更、必須化、enum 値の削除
  - 共通コンポーネント（components.schemas）の削除、必須フィールド削除、型変更、enum 削除
  - securityScheme の削除、operation/グローバルな security 要件の厳格化（追加）

非破壊的（許可）:
  - パス・操作・フィールド・enum 値の「追加」
  - 必須項目の「追加」（ただし既存の必須項目の削除は破壊的）
  - レスポンスの追加、オプション項目の追加
"""

from __future__ import annotations

import argparse
import sys
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover
    sys.stderr.write("pyyaml が必要です: pip install pyyaml\n")
    sys.exit(2)


# ---------------------------------------------------------------------------
# 比較用の補助
# ---------------------------------------------------------------------------

def _norm(value: Any) -> Any:
    """比較用に値を正規化する（キー順に依存しない dict 比較用）。"""
    if isinstance(value, dict):
        return {k: _norm(v) for k, v in sorted(value.items())}
    if isinstance(value, list):
        return [_norm(v) for v in value]
    return value


def _types_compatible(old: Any, new: Any) -> bool:
    """型が互換（同じまたは窄くない方向）かを判定する。"""
    old_t = _type_of(old)
    new_t = _type_of(new)
    if old_t == new_t:
        return True
    # nullable 考慮: old が nullable で new が必須な同型は許容しにくいが、
    # ここでは型の実体のみ比較する。型が異なる場合は破壊的。
    return False


def _type_of(node: Any) -> str:
    if not isinstance(node, dict):
        return "unknown"
    if "type" in node:
        return str(node["type"])
    # oneOf / anyOf / allOf / enum / object(properties のみ) など
    if "oneOf" in node:
        return "oneOf"
    if "anyOf" in node:
        return "anyOf"
    if "enum" in node:
        return "enum"
    if "items" in node:
        return "array"
    if "properties" in node or "additionalProperties" in node:
        return "object"
    if "const" in node:
        return "const"
    return "unspecified"


class BreakingDiff:
    """検出された破壊的変更の集合。"""

    def __init__(self) -> None:
        self.items: list[str] = []

    def add(self, msg: str) -> None:
        self.items.append(msg)

    @property
    def has_breaking(self) -> bool:
        return bool(self.items)


# ---------------------------------------------------------------------------
# スキーマの比較
# ---------------------------------------------------------------------------

def _compare_schema(
    path: str,
    old: Any,
    new: Any,
    diff: BreakingDiff,
) -> None:
    """2 つの JSON Schema を比較し、破壊的変更を記録する。"""
    if old is None and new is None:
        return
    if old is None:
        # 新規追加は非破壊的
        return
    if new is None:
        diff.add(f"{path}: 必須スキーマが削除されました")
        return

    old_type = _type_of(old)
    new_type = _type_of(new)
    if old_type != new_type:
        diff.add(
            f"{path}: 型が {old_type} から {new_type} に変更されました（破壊的）"
        )
        return

    # enum: 既存の値が削除されていないか
    if "enum" in old and "enum" in new:
        old_vals = set(_norm(old["enum"]))
        new_vals = set(_norm(new["enum"]))
        removed = old_vals - new_vals
        if removed:
            diff.add(f"{path}: enum から値が削除されました {sorted(removed)}")

    # required フィールドの削除
    old_required = set(old.get("required", []))
    new_required = set(new.get("required", []))
    removed_required = old_required - new_required
    if removed_required:
        diff.add(
            f"{path}: 必須フィールドが削除されました {sorted(removed_required)}"
        )

    # properties の必須フィールド削除・型変更
    old_props = old.get("properties", {})
    new_props = new.get("properties", {})
    for key, old_sub in old_props.items():
        if key not in new_props:
            diff.add(f"{path}.properties.{key}: フィールドが削除されました")
            continue
        _compare_schema(f"{path}.properties.{key}", old_sub, new_props[key], diff)

    # items (配列要素) の比較
    if "items" in old and "items" in new:
        _compare_schema(f"{path}.items", old["items"], new["items"], diff)


# ---------------------------------------------------------------------------
# パス・operation の比較
# ---------------------------------------------------------------------------

_METHODS = {"get", "post", "put", "delete", "patch", "head", "options", "trace"}


def _compare_parameters(
    path: str,
    op_label: str,
    old_params: list[Any],
    new_params: list[Any],
    diff: BreakingDiff,
) -> None:
    """リクエストパラメータの比較。"""
    old_by_key = {
        (p.get("name"), p.get("in")): p for p in (old_params or []) if isinstance(p, dict)
    }
    new_by_key = {
        (p.get("name"), p.get("in")): p for p in (new_params or []) if isinstance(p, dict)
    }
    for key, old_param in old_by_key.items():
        if key not in new_by_key:
            diff.add(
                f"{op_label}: パラメータ {key[0]}({key[1]}) が削除されました"
            )
            continue
        new_param = new_by_key[key]
        if not old_param.get("required", False) and new_param.get("required", False):
            diff.add(
                f"{op_label}: パラメータ {key[0]}({key[1]}) が必須化されました"
            )


def _compare_operation(
    path: str,
    method: str,
    old_op: dict,
    new_op: dict,
    diff: BreakingDiff,
) -> None:
    label = f"{method} {path}"
    old_op_id = old_op.get("operationId")
    new_op_id = new_op.get("operationId")
    if old_op_id and new_op_id and old_op_id != new_op_id:
        diff.add(
            f"{label}: operationId が {old_op_id} から {new_op_id} に変更されました"
        )

    # リクエストボディ必須フィールド
    old_rb = old_op.get("requestBody")
    new_rb = new_op.get("requestBody")
    if old_rb and new_rb:
        _compare_schema(
            f"{label} requestBody",
            _extract_schema(old_rb),
            _extract_schema(new_rb),
            diff,
        )

    # パラメータ
    _compare_parameters(
        path,
        label,
        _collect_params(old_op, path),
        _collect_params(new_op, path),
        diff,
    )

    # レスポンス
    old_responses = old_op.get("responses", {}) or {}
    new_responses = new_op.get("responses", {}) or {}
    for code, old_resp in old_responses.items():
        if code not in new_responses:
            diff.add(f"{label}: レスポンスコード {code} が削除されました")
            continue
        new_resp = new_responses[code]
        _compare_schema(
            f"{label} responses.{code}",
            _extract_schema(old_resp),
            _extract_schema(new_resp),
            diff,
        )

    # security 要件の厳格化（追加）
    old_sec = _security_sets(old_op)
    new_sec = _security_sets(new_op)
    if old_sec != new_sec:
        # 追加された必須 scheme があれば破壊的
        for sec in new_sec:
            if sec not in old_sec:
                diff.add(
                    f"{label}: security 要件が追加されました {sec}（厳格化）"
                )


def _extract_schema(node: Any) -> Any:
    """requestBody / response からスキーマ部分を抽出する。"""
    if not isinstance(node, dict):
        return None
    if "content" in node:
        # content[mediaType].schema を結合（最初の型を代表とする）
        schemas = []
        for media, body in node["content"].items():
            if isinstance(body, dict) and "schema" in body:
                schemas.append(body["schema"])
        # 複数のときは first を代表、ただし必須性を考慮
        if not schemas:
            return None
        if len(schemas) == 1:
            return schemas[0]
        return {"allOf": schemas}
    if "schema" in node:
        return node["schema"]
    return None


def _collect_params(op: dict, path: str) -> list[Any]:
    """operation 自身の params のみ（パスレベルは別で比較）。"""
    return op.get("parameters", []) or []


def _security_sets(op: dict) -> list[tuple]:
    """operation の security を正規化した集合表現にする。"""
    sec = op.get("security")
    if sec is None:
        return []
    out = []
    for alt in sec:
        if isinstance(alt, dict):
            out.append(tuple(sorted(alt.keys())))
        else:
            out.append(tuple(alt))
    return sorted(out)


def compare_specs(old: dict, new: dict, diff: BreakingDiff) -> None:
    """2 つの OpenAPI ドキュメント全体を比較する。"""
    # --- components.schemas の比較 ---
    old_schemas = (old.get("components") or {}).get("schemas", {}) or {}
    new_schemas = (new.get("components") or {}).get("schemas", {}) or {}
    for name in old_schemas:
        if name not in new_schemas:
            diff.add(f"components.schemas.{name}: スキーマが削除されました")
            continue
        _compare_schema(
            f"components.schemas.{name}",
            old_schemas[name],
            new_schemas[name],
            diff,
        )

    # --- securitySchemes の削除 ---
    old_schemes = (old.get("components") or {}).get("securitySchemes", {}) or {}
    new_schemes = (new.get("components") or {}).get("securitySchemes", {}) or {}
    for name in old_schemes:
        if name not in new_schemes:
            diff.add(f"components.securitySchemes.{name}: セキュリティスキームが削除されました")

    # --- グローバル security の厳格化 ---
    old_global = _security_sets(old)
    new_global = _security_sets(new)
    for sec in new_global:
        if sec not in old_global:
            diff.add(f"(グローバル) security 要件が追加されました {list(sec)}（厳格化）")

    # --- paths の比較 ---
    old_paths = old.get("paths", {}) or {}
    new_paths = new.get("paths", {}) or {}
    for p in old_paths:
        if p not in new_paths:
            diff.add(f"{p}: パスが削除されました")
            continue
        old_item = old_paths[p] or {}
        new_item = new_paths[p] or {}
        # パスレベルパラメータ
        _compare_parameters(p, p, old_item.get("parameters", []), new_item.get("parameters", []), diff)
        for method in _METHODS:
            if method in old_item:
                if method not in new_item:
                    diff.add(f"{method} {p}: 操作が削除されました")
                    continue
                _compare_operation(p, method, old_item[method], new_item[method], diff)


def main() -> int:
    parser = argparse.ArgumentParser(description="OpenAPI 破壊的変更検出")
    parser.add_argument("base", help="変更前の OpenAPI ドキュメント")
    parser.add_argument("head", help="変更後の OpenAPI ドキュメント")
    args = parser.parse_args()

    with open(args.base, encoding="utf-8") as f:
        old = yaml.safe_load(f)
    with open(args.head, encoding="utf-8") as f:
        new = yaml.safe_load(f)

    diff = BreakingDiff()
    compare_specs(old, new, diff)

    if diff.has_breaking:
        print("::error::OpenAPI に破壊的変更が検出されました")
        for item in diff.items:
            print(f"  - {item}")
        return 1

    print("openapi.yaml: 破壊的変更なし（互換性を維持）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
