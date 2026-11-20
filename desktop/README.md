# Foluna pour Windows

App Windows de Foluna, réécrite en Rust avec [Tauri 2](https://tauri.app) ; l'app Mac reste en SwiftUI
(`Sources/`). Première version prévue : bibliothèque et envoi par USB à la Kindle et à la Kobo.

- `core/` : crate `foluna-core`, logique sans interface (portée de `Sources/AlexandrieCore`).
- `app/src-tauri/` : app Tauri (fenêtre, commandes appelées par l'interface).
- `app/ui/` : interface (Svelte + TypeScript) ; jetons de design dans `ui/tokens.css`.
- `scripts/make-icon.py` : icône Windows tirée des calques de `Icon/AppIcon.icon`.

## Installer les outils (une fois, sur le PC Windows)

1. Build Tools C++ de Visual Studio :

   ```bash
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

2. Rust, Node.js et Git, puis rouvrir le terminal :

   ```bash
   winget install Rustlang.Rustup OpenJS.NodeJS.LTS Git.Git
   ```

3. Tauri n'est pas à installer à part : son outil (`@tauri-apps/cli`) est une dépendance du projet, installée par
   `npm install`. WebView2 est déjà dans Windows 10 et 11.

## Lancer et construire

```bash
cd desktop/app
npm install
npm run tauri dev      # lance l'app, rechargée à chaque changement de l'interface
npm run tauri build    # installeurs dans desktop/target/release/bundle/ (nsis : .exe, msi : .msi)
```

Tests du cœur : `cargo test --workspace` dans `desktop/`. La CI (`.github/workflows/windows.yml`) lance les tests
et produit les installeurs à chaque changement dans `desktop/` ; ils sont téléchargeables dans l'onglet Actions
(artefact `foluna-windows`).

Sous Linux (développement), Tauri demande en plus les paquets de WebKitGTK :

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```
