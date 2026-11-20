<script lang="ts">
  export interface MenuItem {
    label: string;
    action: () => void;
    disabled?: boolean;
    danger?: boolean;
  }

  let { x, y, items, onclose }: { x: number; y: number; items: (MenuItem | null)[]; onclose: () => void } = $props();
</script>

<svelte:window onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} onblur={onclose} />

<div class="menu" role="menu" style:left="{x}px" style:top="{y}px">
  {#each items as item}
    {#if item}
      <button
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={(e) => {
          e.stopPropagation();
          onclose();
          item.action();
        }}>{item.label}</button
      >
    {:else}
      <hr />
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 20;
    min-width: var(--size-menu-width);
    padding: var(--space-xs);
    background: var(--color-background);
    border: 1px solid var(--color-separator);
    border-radius: var(--radius-hover);
    box-shadow: var(--shadow-menu);
  }
  button {
    display: block;
    width: 100%;
    padding: var(--space-xs) var(--space-md);
    border: 0;
    border-radius: var(--radius-field);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  button:hover:not(:disabled) {
    background: var(--color-selection);
  }
  button:disabled {
    color: var(--color-secondary);
  }
  .danger {
    color: var(--color-danger);
  }
  hr {
    border: 0;
    border-top: 1px solid var(--color-separator);
    margin: var(--space-xs) 0;
  }
</style>
