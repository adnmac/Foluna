<script lang="ts">
  import type { Sidebar } from "../api";
  import { k, t } from "../i18n.svelte";

  let { data, page, onselect }: { data: Sidebar | null; page: string; onselect: (page: string) => void } = $props();

  // Sections repliables, comme dans l'app Mac.
  let open = $state<Record<string, boolean>>({ folders: true, formats: false, authors: false, series: false, shelves: true });

  const fixed = [
    { key: "all", label: k("Tous les livres"), icon: "📚" },
    { key: "recent", label: k("Ajoutés récemment"), icon: "🕒" },
    { key: "pending", label: k("En attente d'envoi"), icon: "⏳" },
    { key: "sent", label: k("Envoyés à la liseuse"), icon: "📤" },
    { key: "trash", label: k("Corbeille"), icon: "🗑" },
  ];
</script>

<nav class="sidebar" aria-label={t("Barre latérale")}>
  <ul>
    {#each fixed as item}
      <li>
        <button class:selected={page === item.key} onclick={() => onselect(item.key)}>
          <span class="icon" aria-hidden="true">{item.icon}</span>
          <span class="label">{t(item.label)}</span>
          {#if data?.counts[item.key]}<span class="count">{data.counts[item.key]}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>

  {#if data?.device}
    <h2>{t("Liseuse")}</h2>
    <ul>
      <li>
        <button class:selected={page === "device"} onclick={() => onselect("device")}>
          <span class="icon" aria-hidden="true">📖</span>
          <span class="label">{data.device.name}</span>
        </button>
      </li>
    </ul>
  {/if}

  {#each [
    { id: "shelves", title: k("Étagères"), prefix: "shelf", icon: "📖", entries: data?.shelves ?? [] },
    { id: "folders", title: k("Dossiers de livres"), prefix: "folder", icon: "📁", entries: data?.folders ?? [] },
    { id: "formats", title: k("Formats"), prefix: "format", icon: "📄", entries: data?.formats ?? [] },
    { id: "authors", title: k("Auteurs"), prefix: "author", icon: "👤", entries: data?.authors ?? [] },
    { id: "series", title: k("Séries"), prefix: "series", icon: "🔢", entries: data?.series ?? [] },
  ] as section}
    {#if section.entries.length > 0}
      <h2>
        <button class="toggle" aria-expanded={open[section.id]} onclick={() => (open[section.id] = !open[section.id])}>
          <span aria-hidden="true">{open[section.id] ? "▾" : "▸"}</span>
          {t(section.title)}
          <span class="count">{section.entries.length}</span>
        </button>
      </h2>
      {#if open[section.id]}
        <ul>
          {#each section.entries as entry (entry.key)}
            {@const key = `${section.prefix}:${entry.key}`}
            <li>
              <button
                class:selected={page === key}
                class:unavailable={entry.unavailable}
                title={entry.unavailable ? t("Indisponible : disque débranché ou dossier introuvable. Ses livres restent dans la bibliothèque et reviendront au rebranchement.") : entry.key}
                onclick={() => onselect(key)}
              >
                <span class="icon" aria-hidden="true">{section.icon}</span>
                <span class="label">{entry.name}</span>
                {#if entry.count}<span class="count">{entry.count}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  {/each}
</nav>

<style>
  .sidebar {
    background: var(--color-sidebar);
    border-right: 1px solid var(--color-separator);
    overflow-y: auto;
    padding: var(--space-md) var(--space-sm);
  }
  ul {
    list-style: none;
    margin: 0 0 var(--space-md);
    padding: 0;
  }
  h2 {
    font-size: var(--font-caption);
    font-weight: 600;
    color: var(--color-secondary);
    margin: var(--space-md) var(--space-sm) var(--space-xs);
  }
  button {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    width: 100%;
    min-height: 0;
    padding: var(--space-xs) var(--space-sm);
    border: 0;
    border-radius: var(--radius-hover);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover {
    background: var(--color-hover);
  }
  button.selected {
    background: var(--color-selection);
  }
  .toggle {
    font-size: var(--font-caption);
    font-weight: 600;
    color: var(--color-secondary);
    padding: 0;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--color-secondary);
    font-size: var(--font-caption);
    font-variant-numeric: tabular-nums;
  }
  .unavailable {
    color: var(--color-secondary);
  }
  .icon {
    width: var(--space-xl);
    text-align: center;
  }
</style>
