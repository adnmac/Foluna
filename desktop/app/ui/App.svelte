<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ask, open, save } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, message, type Detail, type DeviceInfo, type Row, type Sidebar as SidebarData, type UpdateInfo } from "./api";
  import BookDetail from "./components/BookDetail.svelte";
  import BookList from "./components/BookList.svelte";
  import ContextMenu, { type MenuItem } from "./components/ContextMenu.svelte";
  import DeviceView from "./components/DeviceView.svelte";
  import Settings from "./components/Settings.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import { setLanguage, t } from "./i18n.svelte";

  let page = $state(localStorage.getItem("page") ?? "all");
  let query = $state("");
  let sort = $state(localStorage.getItem("sort") ?? "title");
  let rows = $state<Row[]>([]);
  let sidebar = $state<SidebarData | null>(null);
  let selection = $state(new Set<string>());
  let anchor = $state<string | null>(null);
  let detail = $state<Detail | null>(null);
  let device = $state<DeviceInfo | null>(null);
  let busy = $state(false);
  let progress = $state<Record<string, string>>({});
  let errors = $state<string[]>([]);
  let notice = $state<string | null>(null);
  let update = $state<UpdateInfo | null>(null);
  let version = $state("");
  let menu = $state<{ x: number; y: number; items: (MenuItem | null)[] } | null>(null);
  let settingsOpen = $state(false);
  let dropping = $state(false);
  let searchField: HTMLInputElement | undefined = $state();

  const selectedRows = $derived(rows.filter((r) => selection.has(r.id)));
  const selectedIds = $derived(selectedRows.flatMap((r) => r.ids));
  const deviceName = $derived(sidebar?.device?.name ?? null);
  const allowsTrash = $derived(!["sent", "pending", "trash", "device"].includes(page) && !page.startsWith("shelf:"));

  function fail(e: unknown) {
    errors = [...errors, message(e)];
  }

  function inform(text: string) {
    notice = text;
    setTimeout(() => notice === text && (notice = null), 5000);
  }

  async function refresh() {
    try {
      sidebar = await api.sidebar();
      if (page === "device") {
        device = await api.device();
      } else {
        rows = await api.list(page, query, sort);
        const ids = new Set(rows.map((r) => r.id));
        selection = new Set([...selection].filter((id) => ids.has(id)));
      }
      await loadDetail();
    } catch (e) {
      fail(e);
    }
  }

  async function loadDetail() {
    const ids = selectedRows.length === 1 ? selectedRows[0].ids : [];
    detail = ids.length ? await api.detail(ids) : null;
  }

  function go(next: string) {
    page = next;
    localStorage.setItem("page", next);
    selection = new Set();
    detail = null;
    refresh();
  }

  function select(row: Row, e: MouseEvent) {
    const next = new Set(selection);
    if (e.shiftKey && anchor) {
      const a = rows.findIndex((r) => r.id === anchor);
      const b = rows.findIndex((r) => r.id === row.id);
      for (const r of rows.slice(Math.min(a, b), Math.max(a, b) + 1)) next.add(r.id);
    } else if (e.ctrlKey || e.metaKey) {
      if (next.has(row.id)) next.delete(row.id);
      else next.add(row.id);
      anchor = row.id;
    } else {
      next.clear();
      next.add(row.id);
      anchor = row.id;
    }
    selection = next;
    loadDetail().catch(fail);
  }

  async function send(ids: string[]) {
    if (!ids.length) return;
    busy = true;
    try {
      const results = await api.send(ids);
      if (!results.length) {
        inform(t("En attente : sera envoyé dès que la liseuse sera branchée."));
      } else {
        const failed = results.filter((r) => !r.ok);
        for (const f of failed) fail(f.message ?? f.title);
        const sent = results.length - failed.length;
        if (sent) inform(sent === 1 ? t("« %@ » envoyé sur la liseuse", results.find((r) => r.ok)!.title) : t("%lld livres envoyés sur la liseuse", sent));
      }
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
      progress = {};
      refresh();
    }
  }

  async function act(task: () => Promise<unknown>) {
    try {
      await task();
    } catch (e) {
      fail(e);
    }
    refresh();
  }

  async function deletePermanently(ids: string[]) {
    const ok = await ask(t("Les fichiers seront placés dans la Corbeille de Windows et ces livres quitteront Foluna. Ils resteront récupérables dans la Corbeille de Windows tant que vous ne la videz pas."), {
      title: t("Supprimer définitivement ?"),
      kind: "warning",
    });
    if (ok) act(() => api.deletePermanently(ids));
  }

  async function exportEpub(row: Row) {
    const target = await save({ defaultPath: `${row.title}.epub`, filters: [{ name: "EPUB", extensions: ["epub"] }] });
    if (target) act(() => api.exportEpub(row.ids[0], target));
  }

  function openMenu(row: Row, e: MouseEvent) {
    if (!selection.has(row.id)) select(row, e);
    const ids = selection.has(row.id) ? selectedIds : row.ids;
    const single = selection.size <= 1;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items:
        page === "trash"
          ? [
              { label: t("Remettre dans la bibliothèque"), action: () => act(() => api.restore(ids)) },
              { label: t("Supprimer définitivement…"), danger: true, action: () => deletePermanently(ids) },
            ]
          : [
              { label: deviceName ? t("Envoyer à « %@ »", deviceName) : t("Mettre en attente d'envoi"), action: () => send(ids), disabled: busy },
              row.pending ? { label: t("Retirer de la liste « En attente d'envoi »"), action: () => act(() => api.unqueue(ids)) } : null,
              page === "sent" ? { label: t("Retirer de la liste « Envoyés à la liseuse »"), action: () => act(() => api.forgetSent(ids)) } : null,
              null,
              single && row.formats.some((f) => ["MOBI", "AZW3", "AZW"].includes(f))
                ? { label: t("Exporter en EPUB…"), action: () => exportEpub(row), disabled: row.drm }
                : null,
              { label: t("Afficher dans l'Explorateur"), action: () => revealItemInDir(row.ids[0]) },
              allowsTrash ? { label: t("Mettre à la corbeille"), danger: true, action: () => act(() => api.trash(ids)) } : null,
            ].filter((item, i, all) => item !== null || (i > 0 && all[i - 1] !== null)),
    };
  }

  async function importBooks() {
    const paths = await open({
      multiple: true,
      title: t("Importer des livres"),
      filters: [{ name: t("Livres"), extensions: ["epub", "mobi", "azw3", "azw", "pdf"] }],
    });
    if (Array.isArray(paths) && paths.length) importPaths(paths);
  }

  async function importPaths(paths: string[]) {
    busy = true;
    try {
      const n = await api.import(paths);
      inform(n === 1 ? t("1 livre importé") : t("%lld livres importés", n));
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
      refresh();
    }
  }

  function keydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.closest("input, textarea, select");
    if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchField?.focus();
    } else if (e.key === "F5") {
      e.preventDefault();
      act(() => api.rescan());
    } else if (typing) {
      return;
    } else if (e.ctrlKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      selection = new Set(rows.map((r) => r.id));
      loadDetail().catch(fail);
    } else if (e.key === "Delete" && selectedIds.length) {
      if (page === "trash") deletePermanently(selectedIds);
      else if (allowsTrash) act(() => api.trash(selectedIds));
    } else if (e.key === "Enter" && selectedIds.length) {
      send(selectedIds);
    }
  }

  // Démarrage : langue, données, événements du cœur, glisser-déposer.
  (async () => {
    try {
      const info = await api.settings();
      setLanguage(info.language);
      version = info.version;
      if (info.startupError) fail(info.startupError);
      if (!info.folders.length) inform(t("Ajoutez un dossier de livres dans les réglages, ou faites glisser des livres dans la fenêtre."));
    } catch (e) {
      fail(e);
    }
    await refresh();
    api.checkUpdate(false).then((u) => (update = u)).catch(() => {});
    await listen("library-changed", () => refresh());
    await listen("device-changed", () => refresh());
    await listen<[string, boolean]>("send-progress", (e) => {
      progress = { ...progress, [e.payload[0]]: e.payload[1] ? t("Conversion…") : t("Envoi…") };
    });
    await getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") dropping = true;
      else if (e.payload.type === "leave") dropping = false;
      else if (e.payload.type === "drop") {
        dropping = false;
        importPaths(e.payload.paths);
      }
    });
  })();
</script>

<svelte:window onkeydown={keydown} />

<div class="app" class:dropping>
  <Sidebar data={sidebar} {page} onselect={go} />

  <main>
    <header class="toolbar">
      {#if page !== "device"}
        <input
          class="search"
          type="search"
          placeholder={t("Rechercher (titre, auteur)")}
          bind:this={searchField}
          bind:value={query}
          oninput={() => refresh()}
        />
        <select bind:value={sort} onchange={() => (localStorage.setItem("sort", sort), refresh())} disabled={page === "recent"} aria-label={t("Trier")}>
          <option value="title">{t("Titre")}</option>
          <option value="author">{t("Auteur")}</option>
          <option value="added">{t("Date d'ajout")}</option>
        </select>
      {/if}
      <span class="spacer"></span>
      {#if page === "trash" && rows.length}
        <button class="danger" onclick={() => deletePermanently(rows.flatMap((r) => r.ids))}>{t("Vider la corbeille")}</button>
      {/if}
      <button onclick={importBooks} disabled={busy}>{t("Importer…")}</button>
      <button onclick={() => (settingsOpen = true)} aria-label={t("Réglages")} title={t("Réglages")}>⚙</button>
    </header>

    {#if update}
      <div class="notice" role="status">
        <span>{t("Foluna %@ est disponible (vous avez la %@)", update.version, version)}</span>
        <button onclick={() => update && openUrl(update.url).catch(fail)}>{t("Télécharger")}</button>
        <button class="link" onclick={() => (update = null)}>{t("Fermer")}</button>
      </div>
    {/if}
    {#if notice}<div class="notice" role="status">{notice}</div>{/if}
    {#each errors as error, i}
      <div class="error" role="alert">
        <span>{error}</span>
        <button class="link" onclick={() => (errors = errors.filter((_, k) => k !== i))}>{t("Fermer")}</button>
      </div>
    {/each}

    {#if page === "device"}
      <DeviceView
        {device}
        {busy}
        onsendmissing={() => device && send(device.missing.flatMap((r) => r.ids))}
        oneject={() =>
          act(async () => {
            busy = true;
            try {
              const name = await api.eject();
              inform(t("« %@ » peut être débranchée.", name));
              go("all");
            } finally {
              busy = false;
            }
          })}
        ondelete={async (id, title) => {
          if (await ask(t("Supprimer « %@ » de la liseuse ?", title), { kind: "warning" })) act(() => api.deleteDocument(id));
        }}
        onrename={(name) => device && act(() => api.renameReader(device!.id, name))}
      />
    {:else}
      <div class="content">
        <BookList {rows} {selection} {progress} onselect={select} onmenu={openMenu} />
        {#if detail}
          <BookDetail
            {detail}
            {deviceName}
            onsave={(edits, tags) => act(() => api.saveBook(detail!.ids, edits, tags))}
            onsend={() => send(detail!.ids)}
          />
        {:else if selection.size > 1}
          <section class="multi">
            <p>{t("%lld livres sélectionnés", selection.size)}</p>
            <button class="primary" disabled={busy} onclick={() => send(selectedIds)}>
              {deviceName ? t("Envoyer à « %@ »", deviceName) : t("Mettre en attente d'envoi")}
            </button>
          </section>
        {/if}
      </div>
    {/if}
  </main>
</div>

{#if menu}<ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}
{#if settingsOpen}
  <Settings onclose={() => ((settingsOpen = false), refresh())} onerror={fail} onlanguage={(l) => setLanguage(l)} onupdate={(u) => (update = u)} />
{/if}

<style>
  .app {
    display: grid;
    grid-template-columns: var(--size-sidebar-width) 1fr;
    height: 100vh;
  }
  .app.dropping {
    outline: var(--size-drop-outline) solid var(--color-accent);
    outline-offset: calc(-1 * var(--size-drop-outline));
  }
  main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-sm) var(--space-lg);
    border-bottom: 1px solid var(--color-separator);
  }
  .search {
    width: var(--size-search-width);
  }
  .spacer {
    flex: 1;
  }
  .content {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr minmax(var(--size-detail-min), var(--size-detail-width));
  }
  .content > :global(:only-child) {
    grid-column: 1 / -1;
  }
  .multi {
    padding: var(--space-xl);
    border-left: 1px solid var(--color-separator);
  }
  .notice,
  .error {
    display: flex;
    gap: var(--space-md);
    align-items: center;
    padding: var(--space-sm) var(--space-lg);
    font-size: var(--font-caption);
  }
  .notice {
    background: var(--color-hover);
  }
  .error {
    color: var(--color-danger);
    background: var(--color-hover);
  }
  .notice span,
  .error span {
    flex: 1;
  }
  .danger {
    color: var(--color-danger);
  }
</style>
