<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // Version donnée par le cœur Rust : prouve que l'interface parle au cœur.
  let version = $state("…");
  invoke<string>("app_version").then((v) => (version = v)).catch(() => (version = "?"));
</script>

<main>
  <h1>Foluna</h1>
  <p>Version {version} pour Windows, en préparation.</p>
</main>

<style>
  main {
    padding: var(--space-section) var(--space-margin);
  }
  h1 {
    margin: 0 0 var(--space-md);
  }
  p {
    color: var(--color-secondary);
  }
</style>
