<script>
  import { onMount } from 'svelte';
  import NavRail from './lib/components/NavRail.svelte';
  import FloatingContinue from './lib/components/FloatingContinue.svelte';
  import Library from './lib/views/Library.svelte';
  import Explore from './lib/views/Explore.svelte';
  import Details from './lib/views/Details.svelte';
  import Reader from './lib/views/Reader.svelte';
  import Settings from './lib/views/Settings.svelte';
  import About from './lib/views/About.svelte';
  import { appState, loadSettings } from './lib/stores/app.svelte.js';

  onMount(async () => {
    await loadSettings();
  });
</script>

<div class="app-layout">
  {#if appState.currentScreen !== 'reader'}
    <NavRail />
  {/if}

  <main class="main-content">
    {#key appState.currentScreen}
      <div class="screen-stage">
        {#if appState.currentScreen === 'history'}
          <Library mode="history" />
        {:else if appState.currentScreen === 'favorites'}
          <Library mode="favorites" />
        {:else if appState.currentScreen === 'explore'}
          <Explore initialMode="pinned" />
        {:else if appState.currentScreen === 'sources'}
          <Explore initialMode="catalog" />
        {:else if appState.currentScreen === 'feed'}
          <Library mode="feed" />
        {:else if appState.currentScreen === 'downloads'}
          <Library mode="downloads" />
        {:else if appState.currentScreen === 'settings'}
          <Settings />
        {:else if appState.currentScreen === 'about'}
          <About />
        {:else if appState.currentScreen === 'details'}
          <Details />
        {:else if appState.currentScreen === 'reader'}
          <Reader />
        {/if}
      </div>
    {/key}
  </main>

  {#if appState.currentScreen === 'history'}
    <FloatingContinue />
  {/if}

  {#if appState.notification}
    <div class="toast-notification">
      <span>{appState.notification}</span>
    </div>
  {/if}
</div>

<style>
  .app-layout {
    display: flex;
    width: 100vw;
    height: 100vh;
    background: var(--z-bg);
    color: var(--z-text);
    overflow: hidden;
    position: relative;
  }

  .main-content {
    flex: 1;
    height: 100vh;
    min-width: 0;
    width: 100%;
    overflow: hidden;
    display: flex;
    position: relative;
    background: var(--z-bg);
  }

  .screen-stage {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    animation: screenEnter 180ms var(--z-ease-out) both;
  }

  @keyframes screenEnter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .toast-notification {
    position: fixed;
    bottom: 24px;
    right: 24px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-left: 4px solid var(--z-accent);
    padding: var(--z-space-3) var(--z-space-5);
    border-radius: var(--z-radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    font-size: var(--z-text-sm);
    font-weight: 500;
    color: var(--z-text);
    z-index: 200;
    animation: toast-in 0.25s var(--z-ease-out);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(12px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
