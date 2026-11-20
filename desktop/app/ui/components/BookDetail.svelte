<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import type { Detail, Edits } from "../api";
  import { bytes, date, t } from "../i18n.svelte";

  let {
    detail,
    deviceName,
    onsave,
    onsend,
  }: {
    detail: Detail;
    deviceName: string | null;
    onsave: (edits: Edits, tags: string[]) => Promise<void>;
    onsend: () => void;
  } = $props();

  // Brouillon de la fiche, remis à zéro quand un autre livre est choisi.
  let draft = $state<Edits>({ title: "", authors: [] });
  let authors = $state("");
  let tags = $state("");
  let seriesIndex = $state("");
  let saving = $state(false);

  $effect(() => {
    draft = { ...detail.edits };
    authors = detail.edits.authors.join(", ");
    tags = detail.tags.join(", ");
    seriesIndex = detail.edits.seriesIndex != null ? String(detail.edits.seriesIndex) : "";
  });

  function edits(): Edits {
    const index = parseFloat(seriesIndex.replace(",", "."));
    return {
      ...draft,
      authors: authors.split(/[;,]\s*(?=\S)/).map((a) => a.trim()).filter(Boolean),
      seriesIndex: Number.isFinite(index) ? index : null,
    };
  }

  const changed = $derived(
    JSON.stringify(edits()) !== JSON.stringify({ ...detail.edits, seriesIndex: detail.edits.seriesIndex ?? null }) ||
      tags !== detail.tags.join(", "),
  );

  async function save() {
    saving = true;
    try {
      await onsave(edits(), tags.split(",").map((t) => t.trim()).filter(Boolean));
    } finally {
      saving = false;
    }
  }

  const note = $derived(
    detail.sendNote === "azw3"
      ? t("Sera converti en AZW3 avant l'envoi (la Kindle ne lit pas l'EPUB par USB).")
      : detail.sendNote === "epub"
        ? t("Sera converti en EPUB avant l'envoi : la %@ le lit mieux.", deviceName ?? "")
        : detail.sendNote === "drm"
          ? t("Protégé par DRM : il ne peut pas être lu sur cette %@.", deviceName ?? "")
          : null,
  );
</script>

<section class="detail" aria-label={t("Fiche du livre")}>
  <div class="head">
    <div class="cover">
      {#if detail.cover}<img src={convertFileSrc(detail.cover)} alt="" />{:else}<span aria-hidden="true">📕</span>{/if}
    </div>
    <div class="actions">
      <button class="primary" disabled={!detail.canSend || detail.unavailable} onclick={onsend}>
        {deviceName ? t("Envoyer à « %@ »", deviceName) : t("Mettre en attente d'envoi")}
      </button>
      {#if detail.trashed}
        <p class="caption">{t("Dans la corbeille de Foluna : ses fichiers sont intacts tant qu'elle n'est pas vidée.")}</p>
      {:else if detail.unavailable}
        <p class="caption">{t("Indisponible : disque débranché ou dossier introuvable. Ses livres restent dans la bibliothèque et reviendront au rebranchement.")}</p>
      {:else if detail.pending && !deviceName}
        <p class="caption">{t("En attente : sera envoyé dès que la liseuse sera branchée.")}</p>
      {:else if !deviceName}
        <p class="caption">{t("Liseuse non branchée : le livre peut être mis en attente et partira dès sa connexion.")}</p>
      {:else if note}
        <p class="caption">{note}</p>
      {/if}
      {#if detail.drm}<p class="warning">🔒 {t("Ce livre est protégé par DRM : il ne peut pas être converti.")}</p>{/if}
      {#if detail.lastSentAt}<p class="caption">{t("Dernier envoi : %@", date(detail.lastSentAt))}</p>{/if}
    </div>
  </div>

  <form
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <label>{t("Titre")}<input bind:value={draft.title} required /></label>
    <label>{t("Auteurs")}<input bind:value={authors} placeholder={t("Séparés par des virgules")} /></label>
    <div class="pair">
      <label>{t("Série")}<input bind:value={draft.series} /></label>
      <label class="narrow">{t("Numéro")}<input bind:value={seriesIndex} inputmode="decimal" /></label>
    </div>
    <div class="pair">
      <label>{t("Éditeur")}<input bind:value={draft.publisher} /></label>
      <label class="narrow">{t("Date")}<input bind:value={draft.publishedDate} /></label>
    </div>
    <div class="pair">
      <label>{t("ISBN")}<input bind:value={draft.isbn} /></label>
      <label class="narrow">{t("Langue")}<input bind:value={draft.language} /></label>
    </div>
    <label>{t("Genre")}<input bind:value={draft.genre} /></label>
    <label>{t("Étiquettes")}<input bind:value={tags} placeholder={t("Séparées par des virgules")} /></label>
    <label>{t("Résumé")}<textarea rows="5" bind:value={draft.summary}></textarea></label>
    <p class="caption">{t("Corrections enregistrées dans Foluna et appliquées aux copies envoyées sur la liseuse. Le fichier d'origine n'est jamais modifié.")}</p>
    <div class="save">
      <button type="submit" class="primary" disabled={!changed || saving}>{t("Enregistrer")}</button>
    </div>
  </form>

  <h3>{t("Fichiers")}</h3>
  <ul class="files">
    {#each detail.files as file}
      <li>
        <span class="format">{file.format}</span>
        <span class="path" title={file.path}>{file.path}</span>
        <span class="caption">{bytes(file.size)}{file.hiddenCopy ? " · " + t("copie masquée") : ""}</span>
        <button class="link" onclick={() => revealItemInDir(file.path)}>{t("Afficher dans l'Explorateur")}</button>
      </li>
    {/each}
  </ul>
  <p class="caption">{t("Ajouté le %@", date(detail.addedAt))}</p>
</section>

<style>
  .detail {
    overflow-y: auto;
    padding: var(--space-lg) var(--space-xl);
    border-left: 1px solid var(--color-separator);
  }
  .head {
    display: flex;
    gap: var(--space-lg);
    margin-bottom: var(--space-lg);
  }
  .cover {
    width: var(--size-detail-cover-width);
    aspect-ratio: 2 / 3;
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
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    min-width: 0;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
    font-size: var(--font-caption);
    color: var(--color-secondary);
    flex: 1;
  }
  .pair {
    display: flex;
    gap: var(--space-md);
  }
  .narrow {
    flex: 0 0 var(--size-narrow-field);
  }
  .save {
    display: flex;
    justify-content: flex-end;
  }
  .caption {
    margin: 0;
    font-size: var(--font-caption);
    color: var(--color-secondary);
  }
  .warning {
    margin: 0;
    color: var(--color-warning);
  }
  h3 {
    font-size: var(--font-body);
    margin: var(--space-lg) 0 var(--space-sm);
  }
  .files {
    list-style: none;
    margin: 0 0 var(--space-sm);
    padding: 0;
  }
  .files li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-xs) 0;
  }
  .path {
    flex: 1 1 100%;
    font-size: var(--font-caption);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .format {
    font-size: var(--font-small);
    padding: 0 var(--space-xs);
    border: 1px solid var(--color-separator);
    border-radius: var(--radius-field);
  }
</style>
