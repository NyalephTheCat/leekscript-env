#!/usr/bin/env python3
"""
Generate a LeekScript-style API signature stub file from the Leek Wars repos.

Primary data (names, arg types, optional flags, operation costs):
  leek-wars/src/model/functions.ts
  leek-wars/src/model/constants.ts
  CHIP_* / WEAPON_* constants use Doxygen @par / @li for template stats (see Doxygen special commands).

Category labels:
  leek-wars/src/model/function_categories.ts

Function descriptions (per locale):
  leek-wars/src/lang/doc.<lang>.lang  (keys func_<name>, func_<name>_arg_<n>, func_<name>_return)
  Global constants: optional const_<NAME> in doc lang; deprecated constants get @deprecated / @sa from
  constants.ts replacement id and deprecated_constant / replaced_by strings; documented constants include
  @category (no // === category === section headers in the constants region).
  In emitted Doxygen blocks, Leek-style ``#Symbol`` cross-references become ``@ref Symbol``; encyclopedia ``[[Symbol]]`` likewise.
  Missing func_<name> entries are filled from the encyclopedia API (same <lang> code):
    GET https://www.leekwars.com/api/encyclopedia/get/<lang>/<functionName>
  Use --all-languages to emit sig/std.sig.<lang>.leek for every doc.*.lang; use --no-fetch to skip network.
  Encyclopedia cache (tools/.leek_encyclopedia_cache.json): JSON object ``{ "<lang>": { "<functionName>": "<markdown content>" } }``
  — one top-level key per API language (``fr``, ``de``, …), independent of other locales. With ``--all-languages``,
  each locale uses ``/api/encyclopedia/get/<that locale>/<function>`` and its own cache bucket (``--encyclopedia-lang`` is ignored).

Canonical runtime definitions (for cross-checking):
  leek-wars-generator/leekscript/src/main/java/leekscript/runner/LeekFunctions.java
  leek-wars-generator/src/main/java/com/leekwars/generator/FightFunctions.java
  leek-wars-generator/src/main/java/com/leekwars/generator/FightConstants.java
  leek-wars-generator/leekscript/src/main/java/leekscript/runner/LeekConstants.java
"""

from __future__ import annotations

import argparse
import ast
import html as html_module
import json
import re
import subprocess
import sys
import textwrap
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import NamedTuple
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

REPO = Path(__file__).resolve().parents[1]
TOOLS_DIR = Path(__file__).resolve().parent
SIG_DIR = REPO / "sig"
LEEK_WARS = REPO / "leek-wars" / "src" / "model"
LEEK_LANG = REPO / "leek-wars" / "src" / "lang"
ENCYCLOPEDIA_API = "https://www.leekwars.com/api/encyclopedia/get"
ENCYCLOPEDIA_CACHE = TOOLS_DIR / ".leek_encyclopedia_cache.json"
# Cache negative lookups (404/401/empty) to avoid hammering the API each run.
_ENCYCLOPEDIA_NEGATIVE = "__missing__"


def _generation_header_lines(repo: Path) -> list[str]:
    """Git HEAD + commit date and wall-clock generation time for the stub header."""
    gen = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    base = ["// Stub generated: " + gen]
    try:
        full = subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).strip()
        short = subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "--short", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).strip()
        commit_date = subprocess.check_output(
            ["git", "-C", str(repo), "log", "-1", "--format=%cI", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).strip()
        porcelain = subprocess.check_output(
            ["git", "-C", str(repo), "status", "--porcelain"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).strip()
        dirty = bool(porcelain)
    except (subprocess.CalledProcessError, FileNotFoundError, OSError, subprocess.TimeoutExpired):
        base.append("// Git HEAD: (unavailable — not a git repo or git not in PATH)")
        return base

    dirty_note = " (dirty working tree)" if dirty else ""
    base.append(f"// Git HEAD: {short}{dirty_note} ({full})")
    base.append("// Commit date: " + commit_date)
    return base

# leek-wars/src/model/data.ts COMPLEXITIES — used in Documentation for non-O(1) functions
COMPLEXITIES: dict[str, str] = {
    "1": "O(1)",
    "2": "O(log(n))",
    "3": "O(√n)",
    "4": "O(n)",
    "5": "O(nlog*(n))",
    "6": "O(nlog(n))",
    "7": "O(n²)",
    "8": "O(n³)",
    "9": "2^poly(log(n))",
    "10": "2^poly(n)",
    "11": "O(n!)",
    "12": "2^2^poly(n)",
    "13": "∞",
}

# Numeric type codes in functions.ts — LeekScript-style names for generated signatures.
# Type "1" is documented as "number" in the client (int|real).
# Extra codes match leek-wars/src/lang/doc.en.lang keys arg_type_* (official doc strings).
TS_ARG_TYPE: dict[str, str] = {
    "-1": "any",
    "0": "void",
    "1": "integer|real",
    "2": "string",
    "3": "boolean",
    "4": "Array",
    "5": "Function",
    "6": "integer",
    "7": "real",
    "8": "Map",
    "9": "Set",
    "10": "Interval",  # arg_type_10
    "41": "Array<integer|real>",  # homogeneous number elements (see arg_type_41)
    "42": "Array<string>",
    "43": "Array<boolean>",
    "44": "Array<Array>",
    "46": "Array<integer>",
    "47": "Array<real>",  # doc.en.lang typo: "arary<real>"
    "96": "Set<integer>",  # e.g. getStates return (arg_type_96)
    "806": "Map<any, Integer>",  # e.g. arrayFrequencies (arg_type_806)
}

TS_RETURN_TYPE: dict[int, str] = {
    -1: "any",
    0: "void",
    1: "integer|real",
    2: "string",
    3: "boolean",
    4: "Array",
    5: "Function",
    6: "integer",
    7: "real",
    8: "Map",
    9: "Set",
    10: "Interval",  # arg_type_10
    41: "Array<integer|real>",  # homogeneous number elements (see arg_type_41)
    42: "Array<string>",
    43: "Array<boolean>",
    44: "Array<Array>",
    46: "Array<integer>",
    47: "Array<real>",  # doc.en.lang typo: "arary<real>"
    96: "Set<integer>",  # e.g. getStates return (arg_type_96)
    806: "Map<any, Integer>",  # e.g. arrayFrequencies (arg_type_806)
}


def _parse_functions_ts(path: Path) -> list[dict]:
    """Parse each `{ id: N, name: 'x', ... }` record from functions.ts."""
    text = path.read_text(encoding="utf-8")
    recs = []
    pat = re.compile(
        r"\{\s*id:\s*(\d+)\s*,\s*name:\s*'([^']+)'\s*,\s*category:\s*(\d+)\s*,\s*"
        r"operations:\s*(-?\d+)\s*,\s*arguments_names:\s*(\[[^\]]*\])\s*,\s*"
        r"arguments_types:\s*(\[[^\]]*\])\s*,\s*return_type:\s*(-?\d+)\s*,\s*"
        r"return_name:\s*((?:'[^']*'|null))\s*,\s*deprecated:\s*(true|false)\s*,\s*"
        r"replacement:\s*([^,]+)\s*,\s*optional:\s*(\[[^\]]*\])\s*,\s*"
        r"complexity:\s*(\d+)\s*\}"
    )
    for m in pat.finditer(text):
        names_s = m.group(5).replace("'", '"')
        types_s = m.group(6).replace("'", '"')
        opt_s = m.group(11).replace("'", '"').replace("true", "True").replace("false", "False")
        rn = m.group(8)
        return_name = None if rn == "null" else ast.literal_eval(rn.replace("'", '"'))
        rep_s = m.group(10).strip()
        replacement_id = None if rep_s == "null" else int(rep_s)
        recs.append(
            {
                "id": int(m.group(1)),
                "name": m.group(2),
                "category": int(m.group(3)),
                "operations": int(m.group(4)),
                "arguments_names": ast.literal_eval(names_s),
                "arguments_types": ast.literal_eval(types_s),
                "return_type": int(m.group(7)),
                "return_name": return_name,
                "deprecated": m.group(9) == "true",
                "replacement_id": replacement_id,
                "optional": ast.literal_eval(opt_s),
                "complexity": int(m.group(12)),
            }
        )
    if not recs:
        raise SystemExit(f"No function records parsed from {path}")
    return recs


def _parse_constants_ts(path: Path) -> list[dict]:
    text = path.read_text(encoding="utf-8")
    recs = []
    for m in re.finditer(
        r"\{\s*id:\s*(\d+)\s*,\s*name:\s*'([^']+)'\s*,\s*value:\s*'([^']*)'\s*,\s*"
        r"type:\s*(\d+)\s*,\s*category:\s*(\d+)\s*,\s*deprecated:\s*(true|false)\s*,\s*"
        r"replacement:\s*([^}]+?)\s*\}",
        text,
    ):
        rep_raw = m.group(7).strip()
        replacement_id = None if rep_raw == "null" else int(rep_raw)
        recs.append(
            {
                "id": int(m.group(1)),
                "name": m.group(2),
                "value": m.group(3),
                "type": int(m.group(4)),
                "category": int(m.group(5)),
                "deprecated": m.group(6) == "true",
                "replacement_id": replacement_id,
            }
        )
    if not recs:
        raise SystemExit(f"No constant records parsed from {path}")
    return recs


def _parse_categories(path: Path) -> dict[int, str]:
    text = path.read_text(encoding="utf-8")
    out = {}
    for m in re.finditer(r"(\d+):\s*\{\s*id:\s*\d+\s*,\s*name:\s*'([^']+)'\s*\}", text):
        out[int(m.group(1))] = m.group(2)
    return out


_EFFECT_MODIFIER_BITS: tuple[tuple[int, str], ...] = (
    (16, "EFFECT_MODIFIER_IRREDUCTIBLE"),
    (8, "EFFECT_MODIFIER_NOT_REPLACEABLE"),
    (4, "EFFECT_MODIFIER_ON_CASTER"),
    (2, "EFFECT_MODIFIER_MULTIPLIED_BY_TARGETS"),
    (1, "EFFECT_MODIFIER_STACKABLE"),
)


def _ts_object_literal_to_json(obj: str) -> str:
    """Turn a single-line TS object literal into JSON (chips.ts / weapons.ts rows)."""
    s = obj.strip()
    prev = None
    while prev != s:
        prev = s
        s = re.sub(r"([{,]\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:", r'\1"\2":', s)
    s = re.sub(r":\s*'([^']*)'(?=\s*[,}\]])", r': "\1"', s)
    return s


def _parse_frozen_ts_object_map(path: Path) -> dict[int, dict]:
    text = path.read_text(encoding="utf-8")
    out: dict[int, dict] = {}
    line_re = re.compile(r"^\s*'(\d+)'\s*:\s*(\{.*\})\s*,?\s*$")
    for line in text.splitlines():
        m = line_re.match(line)
        if not m:
            continue
        key = int(m.group(1))
        try:
            out[key] = json.loads(_ts_object_literal_to_json(m.group(2)))
        except (json.JSONDecodeError, TypeError, ValueError):
            continue
    return out


def _weapons_by_item_id(weapons: dict[int, dict]) -> dict[int, dict]:
    out: dict[int, dict] = {}
    for w in weapons.values():
        item = w.get("item")
        if isinstance(item, int):
            out[item] = w
    return out


def _const_names_by_value_prefix(path: Path, prefix: str) -> dict[int, str]:
    text = path.read_text(encoding="utf-8")
    out: dict[int, str] = {}
    rx = re.compile(rf"name:\s*'({re.escape(prefix)}[^']*)'\s*,\s*value:\s*'(-?\d+)'")
    for m in rx.finditer(text):
        v = int(m.group(2))
        out.setdefault(v, m.group(1))
    return out


def _effect_type_names_from_effect_ts(path: Path) -> dict[int, str]:
    """Map effect ``type`` id → ``EFFECT_*`` from ``enum EffectType`` in effect.ts."""
    text = path.read_text(encoding="utf-8")
    m = re.search(r"enum EffectType\s*\{([^}]+)\}", text, re.DOTALL)
    if not m:
        return {}
    out: dict[int, str] = {}
    for m2 in re.finditer(r"\b([A-Z][A-Z0-9_]*)\s*=\s*(-?\d+),", m.group(1)):
        out[int(m2.group(2))] = f"EFFECT_{m2.group(1)}"
    return out


def _parse_effect_type_market_enum(path: Path) -> dict[int, str]:
    text = path.read_text(encoding="utf-8")
    m = re.search(r"enum EffectTypeMarket\s*\{([^}]+)\}", text, re.DOTALL)
    if not m:
        return {}
    out: dict[int, str] = {}
    for m2 in re.finditer(r"\b([A-Z][A-Z0-9_]*)\s*=\s*(\d+),", m.group(1)):
        out[int(m2.group(2))] = m2.group(1)
    return out


def _effect_target_bits_from_constants(path: Path) -> dict[int, str]:
    """Single-bit target flags (``EFFECT_TARGET_*``) for decoding ``targets`` bitmasks."""
    text = path.read_text(encoding="utf-8")
    out: dict[int, str] = {}
    for m in re.finditer(r"name: '(EFFECT_TARGET_[^']+)'\s*,\s*value:\s*'(\d+)'", text):
        v = int(m.group(2))
        if v <= 0:
            continue
        out.setdefault(v, m.group(1))
    return out


class ItemTemplateContext(NamedTuple):
    chips_by_id: dict[int, dict]
    weapons_by_item: dict[int, dict]
    effect_type_name: dict[int, str]
    launch_type_name: dict[int, str]
    area_type_name: dict[int, str]
    chip_market_name: dict[int, str]
    effect_target_bits: dict[int, str]


def build_item_template_context(model_dir: Path) -> ItemTemplateContext | None:
    chips_path = model_dir / "chips.ts"
    weapons_path = model_dir / "weapons.ts"
    constants_path = model_dir / "constants.ts"
    effect_path = model_dir / "effect.ts"
    if not chips_path.is_file() or not weapons_path.is_file():
        return None
    if not constants_path.is_file() or not effect_path.is_file():
        return None
    chips = _parse_frozen_ts_object_map(chips_path)
    weapons_raw = _parse_frozen_ts_object_map(weapons_path)
    return ItemTemplateContext(
        chips_by_id=chips,
        weapons_by_item=_weapons_by_item_id(weapons_raw),
        effect_type_name=_effect_type_names_from_effect_ts(effect_path),
        launch_type_name=_const_names_by_value_prefix(constants_path, "LAUNCH_TYPE_"),
        area_type_name=_const_names_by_value_prefix(constants_path, "AREA_"),
        chip_market_name=_parse_effect_type_market_enum(effect_path),
        effect_target_bits=_effect_target_bits_from_constants(constants_path),
    )


def _maybe_int_constant_value(c: dict) -> int | None:
    try:
        v = c["value"]
        s = str(v).strip()
        if re.fullmatch(r"-?\d+", s):
            return int(s)
    except (KeyError, TypeError, ValueError):
        pass
    return None


def _ref_const(names: dict[int, str], key: int) -> str:
    n = names.get(key)
    return f"@ref {n}" if n else str(key)


def _format_effect_modifiers(m: int) -> str:
    if m == 0:
        return "none"
    parts: list[str] = []
    rem = int(m)
    for val, cname in _EFFECT_MODIFIER_BITS:
        if rem & val:
            parts.append(f"@ref {cname}")
            rem &= ~val
    if rem:
        parts.append(str(rem))
    return " + ".join(parts)


_KNOWN_EFFECT_JSON_KEYS = frozenset({"id", "type", "value1", "value2", "turns", "targets", "modifiers"})


def _fmt_effect_scalar(x) -> str:
    if isinstance(x, bool):
        return "true" if x else "false"
    if isinstance(x, float):
        if x == int(x):
            return str(int(x))
        return str(x)
    if x is None:
        return "null"
    return str(x)


def _effect_duration_phrase(turns: int) -> str:
    if turns == 0:
        return "0 (instant)"
    if turns == -1:
        return "-1 (persistent)"
    if turns < 0:
        return f"{turns} (special)"
    return f"{turns} turn(s)"


def _format_effect_targets_mask(mask: int, ctx: ItemTemplateContext) -> str:
    if mask == 0:
        return "0"
    remaining = int(mask)
    parts: list[str] = []
    for bit in sorted(ctx.effect_target_bits.keys(), reverse=True):
        if remaining & bit:
            parts.append(f"@ref {ctx.effect_target_bits[bit]}")
            remaining &= ~bit
    if remaining:
        parts.append(f"raw remainder 0x{remaining:X}")
    return ", ".join(parts)


def _append_effect_doc(lines: list[str], idx: int, eff: dict, ctx: ItemTemplateContext) -> None:
    """One effect as ``@par Effect N`` plus ``@li`` items (Doxygen \\par / \\li list)."""
    lines.append(f"@par Effect {idx}")
    lines.append("")
    t = eff.get("type")
    if isinstance(t, int):
        tn = ctx.effect_type_name.get(t)
        kind_text = f"@ref {tn}" if tn else f"numeric type code {t}"
    else:
        kind_text = _fmt_effect_scalar(t)
    lines.extend(_wrap_tag_continuation("@li", f"Kind: {kind_text}"))
    lines.extend(_wrap_tag_continuation("@li", f"Effect row id: {_fmt_effect_scalar(eff.get('id'))}"))
    lines.extend(
        _wrap_tag_continuation(
            "@li",
            f"Parameters: value1={_fmt_effect_scalar(eff.get('value1'))}, "
            f"value2={_fmt_effect_scalar(eff.get('value2'))}",
        )
    )
    turns = eff.get("turns")
    try:
        turns_i = int(turns) if turns is not None else 0
    except (TypeError, ValueError):
        lines.extend(_wrap_tag_continuation("@li", f"Duration: (raw turns {turns!r})"))
    else:
        lines.extend(_wrap_tag_continuation("@li", f"Duration: {_effect_duration_phrase(turns_i)}"))
    targets = eff.get("targets")
    try:
        mask = int(targets) if targets is not None else 0
    except (TypeError, ValueError):
        lines.extend(_wrap_tag_continuation("@li", f"Targets: (raw {targets!r})"))
    else:
        lines.extend(_wrap_tag_continuation("@li", f"Targets: {_format_effect_targets_mask(mask, ctx)}"))
    try:
        mods = int(eff.get("modifiers") or 0)
    except (TypeError, ValueError):
        mods = 0
    lines.extend(_wrap_tag_continuation("@li", f"Modifiers: {_format_effect_modifiers(mods)}"))
    for k in sorted(k for k in eff if k not in _KNOWN_EFFECT_JSON_KEYS):
        v = eff[k]
        lines.extend(_wrap_tag_continuation("@li", f"{k}: {_fmt_effect_scalar(v)}"))
    lines.append("")


def _format_item_range(min_r: int, max_r: int) -> str:
    if min_r == max_r:
        return str(min_r)
    return f"{min_r}–{max_r}"


def _item_template_display_name(raw: str) -> str:
    """Human-readable name from template slug (e.g. ``broadsword``, ``m_laser``)."""
    return raw.replace("_", " ").strip() or raw


def _item_template_doc_lines(c: dict, ctx: ItemTemplateContext | None) -> list[str]:
    """Doxygen ``@par`` sections; properties and effect details use ``@li`` lists."""
    if ctx is None:
        return []
    name = c.get("name") or ""
    vid = _maybe_int_constant_value(c)
    if vid is None:
        return []
    if name.startswith("CHIP_"):
        data = ctx.chips_by_id.get(vid)
        kind = "chip"
    elif name.startswith("WEAPON_"):
        data = ctx.weapons_by_item.get(vid)
        kind = "weapon"
    else:
        return []
    if not data:
        return []
    display = _item_template_display_name(str(data.get("name") or ""))
    lines: list[str] = []
    lines.append("@par Game template")
    lines.append("")
    lines.extend(_wrap_paragraph(f"Id for the {display}."))
    lt = int(data["launch_type"])
    area = int(data["area"])
    mn, mx = int(data["min_range"]), int(data["max_range"])
    props: list[str] = [
        f"Min level: {data['level']}",
        f"Cost: {data['cost']} TP",
        f"Range: {_format_item_range(mn, mx)}",
        f"Launch: {_ref_const(ctx.launch_type_name, lt)}",
        f"Area: {_ref_const(ctx.area_type_name, area)}",
        f"LOS: {'yes' if data.get('los') else 'no'}",
    ]
    if kind == "chip":
        props.append(f"Cooldown: {data['cooldown']}")
        props.append("Team cooldown: yes" if data.get("team_cooldown") else "Team cooldown: no")
        props.append(f"Initial cooldown: {data['initial_cooldown']}")
        mu = data.get("max_uses")
        props.append("Max uses: unlimited" if mu == -1 else f"Max uses: {mu}")
        mk = ctx.chip_market_name.get(int(data["type"]))
        props.append(f"Market type: {mk}" if mk else f"Market type id: {data['type']}")
    else:
        mu = data.get("max_uses")
        props.append("Max uses: unlimited" if mu == -1 else f"Max uses: {mu}")
        if data.get("forgotten"):
            props.append("Forgotten template: yes")
    lines.append("")
    lines.append("@par Properties")
    lines.append("")
    for p in props:
        lines.extend(_wrap_tag_continuation("@li", p))
    effs = data.get("effects") or []
    if effs:
        lines.append("")
        lines.append("@par Active effects")
        lines.append("")
        for i, eff in enumerate(effs, 1):
            _append_effect_doc(lines, i, eff, ctx)
    if kind == "weapon":
        pe = data.get("passive_effects") or []
        if pe:
            lines.append("")
            lines.append("@par Passive effects")
            lines.append("")
            for i, eff in enumerate(pe, 1):
                _append_effect_doc(lines, i, eff, ctx)
    return lines


def arg_type(ts_code: str) -> str:
    return TS_ARG_TYPE.get(ts_code.strip(), f"/*type {ts_code.strip()}*/ any")


def return_type(rt: int) -> str:
    return TS_RETURN_TYPE.get(rt, f"/*return {rt}*/ any")


def _constant_id_to_name(constants: list[dict]) -> dict[int, str]:
    return {c["id"]: c["name"] for c in constants}


def constant_leek_type(name: str, value: str, type_id: int) -> str:
    if name in ("PI", "E") or value in ("Infinity", "NaN"):
        return "real"
    if type_id == 7:
        return "real"
    if "." in value and value not in ("Infinity", "NaN") and re.fullmatch(r"-?\d+\.\d+", value):
        return "real"
    return "integer"


def trailing_optional_count(optional: list[bool], n: int) -> int:
    if not optional or n == 0:
        return 0
    c = 0
    for i in range(n - 1, -1, -1):
        if i < len(optional) and optional[i]:
            c += 1
        else:
            break
    return c


def _load_doc_lang(path: Path) -> dict[str, str]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    return {str(k): str(v) for k, v in raw.items()}


def _doc_entry_missing(doc: dict[str, str], key: str) -> bool:
    v = doc.get(key)
    return v is None or not str(v).strip()


def _load_encyclopedia_cache(path: Path) -> dict:
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (json.JSONDecodeError, OSError):
        return {}


def _save_encyclopedia_cache(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def _md_inline_to_doc_html(text: str) -> str:
    """Encyclopedia uses **bold** and `code`; doc.en.lang uses minimal HTML."""
    t = text.strip()
    if not t:
        return ""
    t = re.sub(r"\[\[([^\]]+)\]\]", r"@ref \1", t)
    t = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", t)
    t = re.sub(r"`([^`]+)`", r"<code>\1</code>", t)
    t = re.sub(r"\n\s*\n+", "<br><br>", t)
    t = re.sub(r"\n+", " ", t)
    t = re.sub(r"  +", " ", t)
    return t.strip()


def _split_encyclopedia_sections(content: str) -> dict[str, str]:
    sections: dict[str, str] = {}
    current = "preamble"
    buf: list[str] = []
    for line in content.replace("\r\n", "\n").splitlines():
        if line.startswith("#### "):
            sections[current] = "\n".join(buf).rstrip()
            current = line[5:].strip().lower()
            buf = []
        else:
            buf.append(line)
    sections[current] = "\n".join(buf).rstrip()
    return sections


def _parse_md_bullet_items(body: str) -> list[tuple[str, str]]:
    """Parse '- **name**: description' blocks (supports continuation lines)."""
    items: list[tuple[str, str]] = []
    current_name: str | None = None
    current_desc: list[str] = []
    for raw in body.splitlines():
        line = raw.strip()
        m = re.match(r"-\s*\*\*([^*]+)\*\*:\s*(.*)", line)
        if m:
            if current_name is not None:
                items.append((current_name, " ".join(current_desc).strip()))
            current_name = m.group(1).strip()
            rest = m.group(2).strip()
            current_desc = [rest] if rest else []
        elif current_name and line:
            current_desc.append(line)
    if current_name is not None:
        items.append((current_name, " ".join(current_desc).strip()))
    return items


def _encyclopedia_preamble_html(preamble: str) -> str:
    lines = preamble.strip().splitlines()
    i = 0
    if i < len(lines) and lines[i].lstrip().startswith("#"):
        i += 1
    while i < len(lines) and lines[i].strip().startswith(">"):
        i += 1
    while i < len(lines) and not lines[i].strip():
        i += 1
    body = "\n".join(lines[i:]).strip()
    return _md_inline_to_doc_html(body)


def encyclopedia_content_to_doc_entries(fn: dict, content: str) -> dict[str, str]:
    """Map encyclopedia markdown `content` to doc.en.lang-style keys."""
    fn_name = fn["name"]
    out: dict[str, str] = {}
    sections = _split_encyclopedia_sections(content)
    pre = sections.get("preamble", "").strip()
    if pre:
        main = _encyclopedia_preamble_html(pre)
        if main:
            out[f"func_{fn_name}"] = main

    arg_names: list[str] = fn["arguments_names"]
    params_body = sections.get("parameters", "").strip()
    if params_body and arg_names:
        bullets = _parse_md_bullet_items(params_body)
        by_bullet = {k: v for k, v in bullets}
        for i, aname in enumerate(arg_names):
            desc = by_bullet.get(aname)
            if desc is None:
                for bk, bv in bullets:
                    if bk.lower() == aname.lower():
                        desc = bv
                        break
            if desc:
                out[f"func_{fn_name}_arg_{i + 1}"] = _md_inline_to_doc_html(desc)

    ret_body = (sections.get("return") or sections.get("returns") or "").strip()
    rn = fn.get("return_name")
    if ret_body and rn:
        bullets = _parse_md_bullet_items(ret_body)
        desc: str | None = None
        for bk, bv in bullets:
            if bk.lower() == str(rn).lower():
                desc = bv
                break
        if desc is None and bullets:
            desc = bullets[0][1]
        if desc:
            out[f"func_{fn_name}_return"] = _md_inline_to_doc_html(desc)

    return out


def fetch_encyclopedia_content(lang: str, fn_name: str) -> str | None:
    url = f"{ENCYCLOPEDIA_API}/{quote(lang, safe='')}/{quote(fn_name, safe='')}"
    try:
        req = Request(
            url,
            headers={
                "User-Agent": (
                    "Mozilla/5.0 (compatible; generate_leek_api_signatures/1.0; "
                    "+https://github.com/)"
                ),
                "Accept": "application/json",
            },
        )
        with urlopen(req, timeout=45) as resp:
            payload = json.loads(resp.read().decode())
    except HTTPError as e:
        if e.code != 404:
            print(f"Warning: encyclopedia HTTP {e.code} for {fn_name!r}", file=sys.stderr)
        return None
    except (URLError, json.JSONDecodeError, OSError, TimeoutError) as e:
        print(f"Warning: encyclopedia fetch failed for {fn_name!r}: {e}", file=sys.stderr)
        return None
    if not isinstance(payload, dict):
        return None
    c = payload.get("content")
    return str(c).strip() if c else None


def merge_encyclopedia_docs(
    functions: list[dict],
    doc: dict[str, str],
    *,
    lang: str,
    do_fetch: bool,
    refresh: bool,
    quiet: bool = False,
) -> None:
    """Fill missing ``doc`` keys from the encyclopedia API.

    ``lang`` is both the URL segment ``.../get/<lang>/<fn>`` and the top-level key in
    ``.leek_encyclopedia_cache.json`` (e.g. ``fr`` → ``cache["fr"]["abs"]``).
    """
    if not do_fetch:
        return

    cache = _load_encyclopedia_cache(ENCYCLOPEDIA_CACHE)
    if lang not in cache:
        cache[lang] = {}
    lang_cache: dict[str, str] = cache[lang]
    dirty = False

    n_http = 0
    n_cache_hits = 0
    n_still_missing = 0

    for fn in functions:
        fn_name = fn["name"]
        main_key = f"func_{fn_name}"
        if not _doc_entry_missing(doc, main_key):
            continue

        content: str | None = None
        if not refresh and fn_name in lang_cache:
            n_cache_hits += 1
            cached = lang_cache[fn_name]
            content = None if cached == _ENCYCLOPEDIA_NEGATIVE else cached
        else:
            time.sleep(0.2)
            n_http += 1
            content = fetch_encyclopedia_content(lang, fn_name)
            dirty = True
            lang_cache[fn_name] = content if content else _ENCYCLOPEDIA_NEGATIVE

        if content:
            patch = encyclopedia_content_to_doc_entries(fn, content)
            for k, v in patch.items():
                if _doc_entry_missing(doc, k) and v:
                    doc[k] = v

        if _doc_entry_missing(doc, main_key):
            n_still_missing += 1

    if dirty:
        try:
            _save_encyclopedia_cache(ENCYCLOPEDIA_CACHE, cache)
        except OSError as e:
            print(f"Warning: could not save encyclopedia cache: {e}", file=sys.stderr)

    if not quiet and (n_http or n_cache_hits or n_still_missing):
        print(
            f"Encyclopedia ({lang}): {n_http} HTTP, {n_cache_hits} cache hits, "
            f"{n_still_missing} still missing func_* summary.",
            file=sys.stderr,
        )


def _html_to_plain(s: str) -> str:
    if not s:
        return ""
    t = s.replace("\r", "")

    def _flatten_ul(html: str) -> str:
        def repl(m: re.Match[str]) -> str:
            items = re.findall(r"(?is)<li>(.*?)</li>", m.group(0))
            parts = []
            for it in items:
                it = re.sub(r"<[^>]+>", "", it)
                parts.append(html_module.unescape(it).strip())
            return "; ".join(parts)

        return re.sub(r"(?is)<ul>.*?</ul>", repl, html)

    # Doc strings sometimes use <ul><li>…</li></ul> (e.g. func_useChip_return).
    while "<ul" in t.lower():
        nxt = _flatten_ul(t)
        if nxt == t:
            break
        t = nxt

    t = re.sub(r"(?i)<br\s*/?>", "\n", t)
    t = re.sub(r"<b>(.*?)</b>", r"\1", t, flags=re.DOTALL | re.IGNORECASE)
    t = re.sub(r"<strong>(.*?)</strong>", r"\1", t, flags=re.DOTALL | re.IGNORECASE)
    t = re.sub(r"<i>(.*?)</i>", r"\1", t, flags=re.DOTALL | re.IGNORECASE)
    t = re.sub(r"<code>(.*?)</code>", r"\1", t, flags=re.DOTALL | re.IGNORECASE)
    t = re.sub(r"<[^>]+>", "", t)
    t = html_module.unescape(t)
    t = re.sub(r"\t+", " ", t)
    t = re.sub(r"  +", " ", t)
    return t.strip()


def _plain_lines(s: str) -> list[str]:
    text = _html_to_plain(s)
    if not text:
        return []
    return [ln.strip() for ln in text.split("\n") if ln.strip()]


def _function_id_to_name(functions: list[dict]) -> dict[int, str]:
    return {f["id"]: f["name"] for f in functions}


# Width for wrapped prose inside `/** … */` (excluding " * " prefix).
_DOXY_WRAP = 90
_CONT = "    "  # Doxygen-style continuation indent inside block comment


def _wrap_paragraph(text: str, width: int = _DOXY_WRAP) -> list[str]:
    t = text.strip()
    if not t:
        return []
    return textwrap.wrap(
        t,
        width=width,
        break_long_words=False,
        break_on_hyphens=False,
    )


def _wrap_tag_continuation(tag_line_start: str, description: str, width: int = _DOXY_WRAP) -> list[str]:
    """One logical @tag line, wrapped with hanging indent for continuations."""
    desc = description.strip()
    if not desc:
        return [tag_line_start.rstrip()]
    first = f"{tag_line_start} {desc}".rstrip()
    if len(first) <= width + 2:
        return [first]
    head = tag_line_start.rstrip() + " "
    avail = width - len(_CONT)
    chunks = textwrap.wrap(desc, width=max(avail, 40), break_long_words=False, break_on_hyphens=False)
    if not chunks:
        return [head.rstrip()]
    out = [head + chunks[0]]
    out.extend(_CONT + c for c in chunks[1:])
    return out


def _brief_and_detail(summary_lines: list[str]) -> tuple[str, list[str]]:
    """Split first sentence into @brief; remainder is detailed description."""
    if not summary_lines:
        return "", []
    first = summary_lines[0].strip()
    if ". " in first and len(first) > 40:
        sent, rest = first.split(". ", 1)
        brief = sent.strip()
        if not brief.endswith("."):
            brief += "."
        tail = rest.strip()
        detail: list[str] = []
        if tail:
            detail.extend(_wrap_paragraph(tail))
        for ln in summary_lines[1:]:
            detail.extend(_wrap_paragraph(ln))
        return brief, detail
    brief = first
    if len(brief) > 120:
        parts = _wrap_paragraph(brief)
        brief = parts[0] + ("…" if len(parts) > 1 else "")
        return brief, parts[1:] + [ln for x in summary_lines[1:] for ln in _wrap_paragraph(x)]
    detail = [ln for x in summary_lines[1:] for ln in _wrap_paragraph(x)]
    return brief, detail


def _leek_hash_refs_to_doxygen_ref(text: str) -> str:
    """Turn Leek ``#Name`` markers into Doxygen ``@ref Name`` cross-references."""
    return re.sub(
        r"(?<![A-Za-z0-9_#])#([A-Za-z_][A-Za-z0-9_]*)",
        r"@ref \1",
        text,
    )


def _doxygen_block(lines: list[str]) -> str:
    """Emit a single `/** … */` comment; empty strings become blank ` *` rows."""
    out = ["/**"]
    for line in lines:
        if line == "":
            out.append(" *")
        else:
            out.append(" * " + _leek_hash_refs_to_doxygen_ref(line))
    out.append(" */")
    return "\n".join(out)


def _cost_tag(f: dict) -> str:
    """Short @cost from functions.ts (same rules as documentation-function.vue)."""
    cx = f["complexity"]
    ops = f["operations"]
    if cx == 1:
        if ops < 0:
            return "@cost variable"
        if ops == 1:
            return "@cost 1 op"
        return f"@cost {ops} ops"
    big_o = COMPLEXITIES.get(str(cx), f"class {cx}")
    return f"@cost {big_o}"


def emit_constant_block(
    c: dict,
    doc: dict[str, str],
    id_by_constant_id: dict[int, str],
    cat_name: str,
    item_tpl: ItemTemplateContext | None = None,
) -> list[str]:
    """Emit ``/** … */`` plus ``global``; every constant includes at least ``@category``."""
    lt = constant_leek_type(c["name"], c["value"], c["type"])
    decl = f"global {lt} {c['name']} = {c['value']};"

    key = f"const_{c['name']}"
    raw = doc.get(key)
    summary_lines = _plain_lines(str(raw)) if raw and str(raw).strip() else []

    block: list[str] = []
    if summary_lines:
        brief, detail = _brief_and_detail(summary_lines)
        if brief:
            block.append(f"@brief {brief}")
        if detail:
            block.append("")
            block.extend(detail)

    tpl_lines = _item_template_doc_lines(c, item_tpl)
    if tpl_lines:
        if block:
            block.append("")
        block.extend(tpl_lines)

    if c["deprecated"]:
        if block:
            block.append("")
        dep_msg = _html_to_plain(doc.get("deprecated_constant", "This constant is deprecated."))
        dep_msg = " ".join(dep_msg.split())
        rid = c.get("replacement_id")
        dep_parts = [dep_msg]
        rep_name: str | None = None
        if rid is not None and rid in id_by_constant_id:
            rep_name = id_by_constant_id[rid]
            rb = _html_to_plain(doc.get("replaced_by", "It is replaced by {0}."))
            repl = " ".join(rb.replace("{0}", "#" + rep_name).split())
            if repl:
                dep_parts.append(repl)
        block.extend(_wrap_tag_continuation("@deprecated", " ".join(dep_parts)))
        if rep_name is not None:
            block.append(f"@sa {rep_name}")

    if block:
        block.append("")
    block.append(f"@category {cat_name}")
    return [_doxygen_block(block), decl]


def discover_doc_lang_codes(lang_dir: Path) -> list[str]:
    """Sorted list of locale codes from ``doc.<code>.lang`` files."""
    codes: list[str] = []
    for p in sorted(lang_dir.glob("doc.*.lang")):
        name = p.name
        if name.startswith("doc.") and name.endswith(".lang"):
            codes.append(name[4:-5])
    return codes


def build_stub_lines(
    lang_code: str,
    doc: dict[str, str],
    functions: list[dict],
    constants: list[dict],
    categories: dict[int, str],
    id_to_name: dict[int, str],
    item_tpl: ItemTemplateContext | None = None,
) -> list[str]:
    """Assemble the full .leek stub text for one locale."""
    lines = [
        "// Autogenerated by tools/generate_leek_api_signatures.py — do not edit by hand.",
        "//",
        "// Data: leek-wars/src/model/functions.ts (operation costs, parameters, types),",
        "//       leek-wars/src/model/constants.ts (constant values),",
        f"//       leek-wars/src/lang/doc.{lang_code}.lang (funcs + optional const_* constant docs, {lang_code}).",
        "//",
    ]
    lines.extend(_generation_header_lines(REPO))
    lines.extend(["", "// --- Constants ---", ""])

    id_by_cst = _constant_id_to_name(constants)
    constants_sorted = sorted(constants, key=lambda c: (c["category"], c["name"]))
    n_cst = len(constants_sorted)
    for i, c in enumerate(constants_sorted):
        cat_cst = categories.get(c["category"], f"category_{c['category']}")
        em = emit_constant_block(c, doc, id_by_cst, cat_cst, item_tpl)
        lines.extend(em)
        if i < n_cst - 1:
            lines.append("")

    lines.append("")
    lines.append("// --- Functions ---")
    lines.append("")

    functions.sort(key=lambda f: (f["category"], f["name"]))
    cur_cat = None
    for f in functions:
        if f["category"] != cur_cat:
            cur_cat = f["category"]
            cn = categories.get(cur_cat, f"category_{cur_cat}")
            lines.append(f"// === {cn} ===")
            lines.append("")
        cat_name = categories.get(f["category"], f"category_{f['category']}")
        lines.append(emit_function(f, cat_name, doc, id_to_name))
        lines.append("")

    return lines


def emit_function(
    f: dict,
    cat_name: str,
    doc: dict[str, str],
    id_to_name: dict[int, str],
) -> str:
    names: list[str] = f["arguments_names"]
    types: list[str] = f["arguments_types"]
    optional: list[bool] = f["optional"]
    n = len(names)
    if len(types) != n:
        types = (types + ["-1"] * n)[:n]
    if len(optional) < n:
        optional = optional + [False] * (n - len(optional))
    elif len(optional) > n:
        optional = optional[:n]

    k = trailing_optional_count(optional, n)
    versions = list(range(n - k, n + 1))
    versions.reverse()

    fn = f["name"]
    block: list[str] = []

    main_key = f"func_{fn}"
    main_html = doc.get(main_key)
    if main_html:
        summary_lines = _plain_lines(main_html)
        brief, detail = _brief_and_detail(summary_lines)
        if brief:
            block.append(f"@brief {brief}")
        if detail:
            block.append("")
            block.extend(detail)
    else:
        block.append(
            f"@brief (No `{main_key}` in doc lang — add an entry to match the client Documentation tab.)"
        )

    if f["deprecated"]:
        block.append("")
        dep_msg = _html_to_plain(doc.get("deprecated_function", "This function is deprecated."))
        dep_msg = " ".join(dep_msg.split())
        rid = f.get("replacement_id")
        dep_parts = [dep_msg]
        rep_name: str | None = None
        if rid is not None and rid in id_to_name:
            rep_name = id_to_name[rid]
            rb = _html_to_plain(doc.get("replaced_by", "It is replaced by {0}."))
            repl = " ".join(rb.replace("{0}", "#" + rep_name).split())
            if repl:
                dep_parts.append(repl)
        block.extend(_wrap_tag_continuation("@deprecated", " ".join(dep_parts)))
        if rep_name is not None:
            block.append(f"@sa {rep_name}")

    block.append("")
    block.append(f"@category {cat_name}")
    block.append(_cost_tag(f))

    if n > 0:
        block.append("")
        opt_label = doc.get("optional", "optional")
        for i in range(n):
            opt_note = f" ({opt_label})" if i < len(optional) and optional[i] else ""
            arg_key = f"func_{fn}_arg_{i + 1}"
            arg_html = doc.get(arg_key, "")
            arg_plain = _html_to_plain(arg_html) if arg_html else ""
            arg_plain = " ".join(arg_plain.split())
            param_head = f"@param[in] {names[i]}{opt_note}"
            if arg_plain:
                block.extend(_wrap_tag_continuation(param_head, arg_plain))
            else:
                block.append(param_head)

    if f["return_type"] != 0 and f.get("return_name"):
        if n == 0:
            block.append("")
        rk = f"func_{fn}_return"
        ret_html = doc.get(rk, "")
        ret_plain = _html_to_plain(ret_html) if ret_html else ""
        ret_plain = " ".join(ret_plain.split())
        rn = f["return_name"]
        ret_head = f"@return {rn}"
        if ret_plain:
            block.extend(_wrap_tag_continuation(ret_head, ret_plain))
        else:
            block.append(ret_head)

    comment = _doxygen_block(block)
    sig_lines: list[str] = []
    ret = return_type(f["return_type"])
    for vn in versions:
        params = []
        for i in range(vn):
            params.append(f"{arg_type(types[i])} {names[i]}")
        param_s = ", ".join(params)
        sig_lines.append(f"function {fn}({param_s}) => {ret};")
    return comment + "\n" + "\n".join(sig_lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate Leek Wars API signature stubs (std.sig.<lang>.leek).",
    )
    parser.add_argument(
        "output",
        nargs="?",
        type=Path,
        default=None,
        help="Output .leek path (single-language mode only; default: sig/std.sig.leek or sig/std.sig.<lang>.leek)",
    )
    parser.add_argument(
        "--all-languages",
        action="store_true",
        help="Write std.sig.<code>.leek (under sig/ by default) for every doc.*.lang",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=SIG_DIR,
        help="Directory for stubs when using --all-languages (default: sig/)",
    )
    parser.add_argument(
        "--lang",
        default="en",
        metavar="CODE",
        help="Locale for doc.<CODE>.lang when not using --all-languages (default: en)",
    )
    parser.add_argument(
        "--no-fetch",
        action="store_true",
        help="Do not call the encyclopedia API for missing doc entries",
    )
    parser.add_argument(
        "--encyclopedia-lang",
        default=None,
        metavar="CODE",
        help="Override encyclopedia API language in single-locale mode only (ignored with --all-languages)",
    )
    parser.add_argument(
        "--refresh-encyclopedia",
        action="store_true",
        help="Ignore cached encyclopedia responses and re-download",
    )
    args = parser.parse_args()

    fn_path = LEEK_WARS / "functions.ts"
    co_path = LEEK_WARS / "constants.ts"
    cat_path = LEEK_WARS / "function_categories.ts"

    functions = _parse_functions_ts(fn_path)
    constants = _parse_constants_ts(co_path)
    categories = _parse_categories(cat_path)
    id_to_name = _function_id_to_name(functions)
    item_tpl = build_item_template_context(LEEK_WARS)

    if args.all_languages:
        if args.encyclopedia_lang:
            print(
                "Note: --encyclopedia-lang is ignored with --all-languages; "
                "each locale uses its own code for the API and cache (e.g. fr → …/get/fr/…).",
                file=sys.stderr,
            )
        codes = discover_doc_lang_codes(LEEK_LANG)
        if not codes:
            raise SystemExit(f"No doc.*.lang files under {LEEK_LANG}")
        out_dir = args.output_dir
        out_dir.mkdir(parents=True, exist_ok=True)
        for code in codes:
            doc_path = LEEK_LANG / f"doc.{code}.lang"
            doc = _load_doc_lang(doc_path)
            merge_encyclopedia_docs(
                functions,
                doc,
                lang=code,
                do_fetch=not args.no_fetch,
                refresh=args.refresh_encyclopedia,
                quiet=True,
            )
            lines = build_stub_lines(code, doc, functions, constants, categories, id_to_name, item_tpl)
            text = "\n".join(lines).rstrip() + "\n"
            out_path = out_dir / f"std.sig.{code}.leek"
            out_path.write_text(text, encoding="utf-8")
            print(f"Wrote {out_path} ({len(functions)} functions, {len(constants)} constants)")
            if code == "en":
                legacy = out_dir / "std.sig.leek"
                legacy.write_text(text, encoding="utf-8")
                print(f"Wrote {legacy} ({len(functions)} functions, {len(constants)} constants)")
        print(f"Done: {len(codes)} locales → std.sig.<code>.leek under {out_dir}")
        return 0

    lang = args.lang
    doc_path = LEEK_LANG / f"doc.{lang}.lang"
    if not doc_path.is_file():
        raise SystemExit(f"Missing documentation file: {doc_path}")

    out_path = args.output
    if out_path is None:
        out_path = SIG_DIR / "std.sig.leek" if lang == "en" else SIG_DIR / f"std.sig.{lang}.leek"

    doc = _load_doc_lang(doc_path)
    enc = args.encyclopedia_lang or lang
    merge_encyclopedia_docs(
        functions,
        doc,
        lang=enc,
        do_fetch=not args.no_fetch,
        refresh=args.refresh_encyclopedia,
        quiet=False,
    )
    lines = build_stub_lines(lang, doc, functions, constants, categories, id_to_name, item_tpl)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print(f"Wrote {out_path} ({len(functions)} functions, {len(constants)} constants)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
