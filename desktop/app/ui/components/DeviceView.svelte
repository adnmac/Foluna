<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import type { DeviceInfo } from "../api";
  import { bytes, t } from "../i18n.svelte";

  let {
    device,
    busy,
    onsendmissing,
    oneject,
    ondelete,
    onrename,
  }: {
    device: DeviceInfo | null;
    busy: boolean;
    onsendmissing: () => void;
    oneject: () => void;
    ondelete: (id: string, title: string) => void;
    onrename: (name: string) => void;
  } = $props();

  let name = $state("");
  $effect(() => {
    name = device?.name ?? "";
  });
</script>

<section class="device" aria-label={t("Liseuse")}>
  {#if !device}
    <p class="empty">{t("Branchez une Kindle ou une Kobo par USB : elle apparaîtra ici.")}</p>
  {:else}
    <header>
      <input
        class="name"
        aria-label={t("Nom")}
        bind:value={name}
        onchange={() => onrename(name)}
        onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
      />
      <span class="caption">{device.brand} · {t("%@ libres", bytes(device.freeSpace))} · {device.documents.length === 1 ? t("1 livre") : t("%lld livres", device.documents.length)}</span>
      <span class="spacer"></span>
      <button disabled={busy} onclick={oneject}>⏏ {t("Éjecter")}</button>
    </header>

    {#if device.missing.length > 0}
      <div class="missing">
        <p>
          {device.missing.length === 1
            ? t("1 livre de l'étagère n'est pas sur la liseuse.")
            : t("%lld livres de l'étagère ne sont pas sur la liseuse.", device.missing.length)}
        </p>
        <button class="primary" disabled={busy} onclick={onsendmissing}>{t("Envoyer les livres manquants")}</button>
      </div>
    {/if}

    <ul>
      {#each device.documents as doc (doc.id)}
        <li>
          <div class="cover">
            {#if doc.cover}<img src={convertFileSrc(doc.cover)} alt="" loading="lazy" />{:else}<span aria-hidden="true">📕</span>{/if}
          </div>
          <div class="text">
            <div class="title">{doc.title}</div>
            <div class="caption">{doc.authorLine}</div>
          </div>
          <span class="caption">{doc.format} · {bytes(doc.size)}{doc.drm ? " · 🔒" : ""}</span>
          <button class="link danger" disabled={busy} onclick={() => ondelete(doc.id, doc.title)}>{t("Supprimer de la liseuse")}</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .device {
    overflow-y: auto;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-md) var(--space-lg);
    border-bottom: 1px solid var(--color-separator);
  }
  .name {
    font-size: var(--font-title);
    font-weight: 600;
    border: 1px solid transparent;
    background: none;
    max-width: var(--size-device-name);
  }
  .name:hover,
  .name:focus {
    border-color: var(--color-separator);
  }
  .spacer {
    flex: 1;
  }
  .missing {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-md) var(--space-lg);
    background: var(--color-hover);
  }
  .missing p {
    margin: 0;
    flex: 1;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-lg);
    border-bottom: 1px solid var(--color-separator);
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
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption {
    font-size: var(--font-caption);
    color: var(--color-secondary);
  }
  .empty {
    padding: var(--space-section);
    text-align: center;
    color: var(--color-secondary);
  }
  .danger {
    color: var(--color-danger);
  }
</style>
