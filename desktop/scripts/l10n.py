#!/usr/bin/env python3
"""Traductions de l'app Windows.

Les textes de l'interface (desktop/app/ui) sont écrits en français dans `t("…")` ou `k("…")`, comme les clés du
catalogue de l'app Mac (Resources/Localizable.xcstrings). Pour chaque langue (en, nl, de, es), la traduction vient
de ce catalogue ; un texte propre à Windows vient de desktop/app/ui/locales/windows.json.

    python3 desktop/scripts/l10n.py          écrit desktop/app/ui/locales/<langue>.json
    python3 desktop/scripts/l10n.py check    échoue si une traduction manque (appelé par la CI Windows)
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
UI = ROOT / "desktop" / "app" / "ui"
LOCALES = UI / "locales"
LANGUAGES = ["en", "nl", "de", "es"]
CALL = re.compile(r'\b[tk]\(\s*"((?:[^"\\]|\\.)*)"')


def keys():
    found = set()
    for path in list(UI.rglob("*.svelte")) + list(UI.rglob("*.ts")):
        for m in CALL.finditer(path.read_text(encoding="utf-8")):
            found.add(json.loads('"' + m.group(1) + '"'))
    return sorted(found)


def mac_catalog():
    data = json.loads((ROOT / "Resources" / "Localizable.xcstrings").read_text(encoding="utf-8"))
    out = {}
    for key, entry in data["strings"].items():
        locs = entry.get("localizations", {})
        values = {l: locs.get(l, {}).get("stringUnit", {}).get("value") for l in LANGUAGES}
        if all(values.values()):
            out[key] = values
    return out


def main():
    # La console de Windows écrit en cp1252 : sans cela, « ✅ » ou un texte accentué arrête le script.
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8")
    mac = mac_catalog()
    windows_path = LOCALES / "windows.json"
    windows = json.loads(windows_path.read_text(encoding="utf-8")) if windows_path.exists() else {}
    tables = {l: {} for l in LANGUAGES}
    missing = []
    for key in keys():
        source = windows.get(key) or mac.get(key)
        if not source or any(not source.get(l) for l in LANGUAGES):
            missing.append(key)
            continue
        for l in LANGUAGES:
            tables[l][key] = source[l]
    unused = sorted(set(windows) - set(keys()))
    if len(sys.argv) > 1 and sys.argv[1] == "check":
        for key in missing:
            print(f"✗ traduction manquante : {key!r}")
        for key in unused:
            print(f"✗ clé inutile dans windows.json : {key!r}")
        if missing or unused:
            sys.exit(1)
        print(f"✅ traductions complètes ({len(keys())} textes)")
        return
    LOCALES.mkdir(parents=True, exist_ok=True)
    for l in LANGUAGES:
        (LOCALES / f"{l}.json").write_text(json.dumps(tables[l], ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{len(keys())} textes, {len(missing)} sans traduction")
    for key in missing:
        print("  manque :", key)


if __name__ == "__main__":
    main()
