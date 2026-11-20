<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, message, type SettingsInfo } from "../api";
  import { bytes, languages, t } from "../i18n.svelte";

  let { onclose, onerror, onlanguage }: { onclose: () => void; onerror: (m: string) => void; onlanguage: (l: string | null) => void } = $props();

  let info = $state<SettingsInfo | null>(null);
  const names: Record<string, string> = { fr: "Français", en: "English", nl: "Nederlands", de: "Deutsch", es: "Español" };

  async function reload() {
    info = await api.settings();
  }
  reload();

  async function run(task: () => Promise<unknown>) {
    try {
      await task();
    } catch (e) {
      onerror(message(e));
    }
    await reload();
  }

  async function addFolder() {
    const path = await open({ directory: true, title: t("Ajouter un dossier…") });
    if (typeof path === "string") await run(() => api.addFolder(path));
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="panel" role="dialog" aria-modal="true" aria-label={t("Réglages")} tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <header>
      <h2>{t("Réglages")}</h2>
      <button onclick={onclose} aria-label={t("Fermer")}>✕</button>
    </header>
    {#if info}
      <section>
        <h3>{t("Dossiers de livres")}</h3>
        <ul>
          {#each info.folders as folder (folder.key)}
            <li>
              <span class="path" class:unavailable={folder.unavailable} title={folder.key}>{folder.key}</span>
              <button class="link" disabled={info.folders.length <= 1} onclick={() => run(() => api.removeFolder(folder.key))}>{t("Retirer")}</button>
            </li>
          {/each}
        </ul>
        <button onclick={addFolder}>{t("Ajouter un dossier…")}</button>
        <p class="caption">{t("Ces dossiers sont lus sans jamais être modifiés. Les livres importés, les couvertures et les conversions sont stockés dans le dossier de l'app.")}</p>
      </section>

      <section>
        <h3>{t("Général")}</h3>
        <label class="row">
          {t("Langue")}
          <select
            value={info.language ?? ""}
            onchange={(e) => {
              const value = (e.currentTarget as HTMLSelectElement).value || null;
              run(() => api.setLanguage(value));
              onlanguage(value);
            }}
          >
            <option value="">{t("Langue de Windows")}</option>
            {#each languages as l}<option value={l}>{names[l]}</option>{/each}
          </select>
        </label>
        <label class="row">
          <input type="checkbox" checked={info.showDuplicates} onchange={(e) => run(() => api.setShowDuplicates((e.currentTarget as HTMLInputElement).checked))} />
          {t("Afficher les copies en double (%lld masquées)", info.hiddenDuplicates)}
        </label>
      </section>

      <section>
        <h3>{t("Liseuses")}</h3>
        <ul>
          {#each info.readers as reader (reader.key)}
            <li>
              <span class="path">{reader.name}</span>
              <button class="link" onclick={() => run(() => api.forgetReader(reader.key))}>{t("Oublier")}</button>
            </li>
          {:else}
            <li class="caption">{t("Aucune liseuse pour l'instant : branchez une Kindle ou une Kobo pour lui donner un nom.")}</li>
          {/each}
        </ul>
      </section>

      <section>
        <h3>{t("Bibliothèque")}</h3>
        <p class="row">{t("Couvertures")} <span class="caption">{bytes(info.coversBytes)}</span></p>
        <p class="row">
          {t("Conversions pour les liseuses")} <span class="caption">{bytes(info.conversionsBytes)}</span>
          <button class="link" disabled={info.conversionsBytes === 0} onclick={() => run(() => api.clearConversions())}>{t("Vider")}</button>
        </p>
        <button class="link" onclick={() => info && revealItemInDir(info.dataFolder)}>{t("Afficher les données de l'app")}</button>
        <p class="caption">Foluna {info.version}</p>
      </section>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: var(--color-backdrop);
  }
  .panel {
    width: min(var(--size-settings-width), calc(100vw - 2 * var(--space-section)));
    max-height: calc(100vh - 2 * var(--space-section));
    overflow-y: auto;
    padding: var(--space-lg) var(--space-xl);
    background: var(--color-background);
    border-radius: var(--radius-drop-zone);
    box-shadow: var(--shadow-panel);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    margin: 0;
    font-size: var(--font-title);
  }
  h3 {
    font-size: var(--font-body);
    margin: var(--space-lg) 0 var(--space-sm);
  }
  ul {
    list-style: none;
    margin: 0 0 var(--space-sm);
    padding: 0;
  }
  li,
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin: var(--space-xs) 0;
  }
  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .unavailable {
    color: var(--color-secondary);
  }
  .caption {
    font-size: var(--font-caption);
    color: var(--color-secondary);
  }
</style>
