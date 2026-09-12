<script lang="ts">
  import TitleBar from "$lib/shell/TitleBar.svelte";
  import StatusBar from "$lib/shell/StatusBar.svelte";
  import Toast from "$lib/shell/Toast.svelte";
  import WelcomePage from "$lib/pages/WelcomePage.svelte";
</script>

<div class="app">
  <TitleBar context="Local · nothing uploaded" />
  <main class="main">
    <WelcomePage />
  </main>
  <StatusBar />
  <Toast />
</div>

<style>
  .app {
    height: 100%;
    display: grid;
    grid-template-rows: var(--titlebar-h) 1fr var(--statusbar-h);
    position: relative;
    background: var(--bg);
  }
  .main {
    min-height: 0;
    display: grid;
  }
</style>
