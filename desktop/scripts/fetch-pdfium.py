#!/usr/bin/env python3
"""Télécharge pdfium (rendu des couvertures des PDF) pour l'installeur Windows ou les tests.

Version figée des binaires de bblanchon/pdfium-binaries, somme SHA-256 vérifiée avant l'extraction.
Écrit app/src-tauri/pdfium/pdfium.dll (Windows) ou libpdfium.so (Linux), et les licences de pdfium et de ses
composants (freetype, libjpeg-turbo…) réunies dans app/src-tauri/licenses/pdfium.txt. Affiche le chemin de la bibliothèque (pour FOLUNA_PDFIUM).

    python desktop/scripts/fetch-pdfium.py [windows|linux]
"""
import hashlib
import io
import pathlib
import sys
import tarfile
import urllib.request

RELEASE = "chromium/8125"
ARCHIVES = {
    "windows": ("pdfium-win-x64.tgz", "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b", "bin/pdfium.dll"),
    "linux": ("pdfium-linux-x64.tgz", "e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659", "lib/libpdfium.so"),
}
APP = pathlib.Path(__file__).resolve().parent.parent / "app" / "src-tauri"


def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8")
    platform = sys.argv[1] if len(sys.argv) > 1 else ("windows" if sys.platform == "win32" else "linux")
    name, digest, member = ARCHIVES[platform]
    target = APP / "pdfium" / pathlib.PurePosixPath(member).name
    if target.exists() and (APP / "licenses" / "pdfium.txt").exists():
        print(target)
        return
    url = f"https://github.com/bblanchon/pdfium-binaries/releases/download/{RELEASE.replace('/', '%2F')}/{name}"
    data = urllib.request.urlopen(url, timeout=120).read()
    if hashlib.sha256(data).hexdigest() != digest:
        sys.exit(f"Somme SHA-256 inattendue pour {name} : téléchargement refusé.")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        library = archive.extractfile(member)
        if library is None:
            sys.exit(f"{member} absent de {name}.")
        notices = []
        for entry in sorted(archive.getmembers(), key=lambda m: m.name):
            if entry.isfile() and (entry.name == "LICENSE" or entry.name.startswith("licenses/")):
                text = archive.extractfile(entry).read().decode("utf-8", "replace").strip()
                notices.append(f"===== {entry.name} =====\n\n{text}\n")
        if not notices:
            sys.exit(f"Licences absentes de {name}.")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(library.read())
        (APP / "licenses").mkdir(exist_ok=True)
        (APP / "licenses" / "pdfium.txt").write_text("\n".join(notices), encoding="utf-8")
    print(target)


if __name__ == "__main__":
    main()
