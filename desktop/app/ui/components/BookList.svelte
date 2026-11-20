<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { Row } from "../api";
  import { t } from "../i18n.svelte";

  let {
    rows,
    selection,
    progress,
    onselect,
    onmenu,
  }: {
    rows: Row[];
    selection: Set<string>;
    progress: Record<string, string>;
    onselect: (row: Row, event: MouseEvent) => void;
    onmenu: (row: Row, event: MouseEvent) => void;
  } = $props();
</script>

<ul class="list" role="listbox" aria-multiselectable="true" aria-label={t("Livres")}>
  {#each rows as row (row.id)}
    <li
      role="option"
      aria-selected={selection.has(row.id)}
      class:selected={selection.has(row.id)}
      class:unavailable={row.unavailable}
      onclick={(e) => onselect(row, e)}
      oncontextmenu={(e) => {
        e.preventDefault();
        onmenu(row, e);
      }}
      onkeydown={() => {}}
    >
      <div class="cover">
        {#if row.cover}
          <img src={convertFileSrc(row.cover)} alt="" loading="lazy" />
        {:else}
          <span aria-hidden="true">📕</span>
        {/if}
      </div>
      <div class="text">
        <div class="title">{row.title}</div>
        <div class="author">
          {row.authorLine}
          {#if row.series}<span class="series"> · {row.series}{row.seriesIndex != null ? " " + row.seriesIndex : ""}</span>{/if}
        </div>
      </div>
      <div class="badges">
        {#if progress[row.ids[0]]}<span class="status">{progress[row.ids[0]]}</span>{/if}
        {#if row.pending}<span class="badge" title={t("En attente d'envoi")}>⏳</span>{/if}
        {#if row.lastSentAt}<span class="badge" title={t("Envoyé à la liseuse")}>✓</span>{/if}
        {#if row.drm}<span class="badge" title={t("Protégé par DRM")}>🔒</span>{/if}
        {#each row.formats as format}<span class="format">{format}</span>{/each}
      </div>
    </li>
  {:else}
    <li class="empty">{t("Aucun livre")}</li>
  {/each}
</ul>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-lg);
    border-bottom: 1px solid var(--color-separator);
    cursor: default;
    user-select: none;
  }
  li.selected {
    background: var(--color-selection);
  }
  li.unavailable {
    opacity: 0.55;
  }
  .cover {
    width: var(--size-cover-width);
    height: var(--size-cover-height);
    flex: none;
    display: grid;
    place-items: center;
    border-radius: var(--radius-cover);
    background: var(--color-hover);
    overflow: hidden;
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title,
  .author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-weight: 600;
  }
  .author,
  .series {
    color: var(--color-secondary);
    font-size: var(--font-caption);
  }
  .badges {
    display: flex;
    gap: var(--space-xs);
    align-items: center;
  }
  .format {
    font-size: var(--font-small);
    padding: 0 var(--space-xs);
    border: 1px solid var(--color-separator);
    border-radius: var(--radius-field);
    color: var(--color-secondary);
  }
  .status {
    font-size: var(--font-caption);
    color: var(--color-accent);
  }
  .empty {
    justify-content: center;
    color: var(--color-secondary);
    border: 0;
    padding: var(--space-section);
  }
</style>
