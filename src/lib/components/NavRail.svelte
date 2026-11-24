<script>
  import { 
    History, Heart, Compass, Rss, Download, Settings
  } from 'lucide-svelte';
  import { appState, navigate } from '../stores/app.svelte.js';

  const primaryNav = [
    { id: 'history', label: 'History', icon: History },
    { id: 'favorites', label: 'Favourites', icon: Heart },
    { id: 'explore', label: 'Explore', icon: Compass },
  ];

  const secondaryNav = [
    { id: 'feed', label: 'Feed', icon: Rss },
    { id: 'downloads', label: 'Downloads', icon: Download },
  ];
</script>

<nav class="nav-rail" class:expanded={appState.railExpanded}>
  <div class="rail-top">
    <!-- APP LOGO TOGGLES SIDEBAR EXPANSION (NO TEXT / NO SEPARATE TOGGLE BUTTON) -->
    <div class="rail-header">
      <button 
        class="logo-btn" 
        on:click={() => appState.railExpanded = !appState.railExpanded}
        title="Toggle Sidebar"
      >
        <img src="/logo.png" alt="ZibraX" class="brand-logo-img" />
      </button>
    </div>

    <!-- PRIMARY NAV ITEMS -->
    <div class="items-list">
      {#each primaryNav as item (item.id)}
        <button
          class="rail-item"
          class:active={appState.currentScreen === item.id}
          on:click={() => navigate(item.id)}
          title={!appState.railExpanded ? item.label : undefined}
        >
          <div class="icon-wrap">
            <svelte:component this={item.icon} size={20} strokeWidth={1.8} />
          </div>
          {#if appState.railExpanded}
            <span class="label">{item.label}</span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <!-- RAIL BOTTOM: FEED, DOWNLOADS, SETTINGS & ABOUT -->
  <div class="rail-bottom">
    <div class="items-list">
      {#each secondaryNav as item (item.id)}
        <button
          class="rail-item"
          class:active={appState.currentScreen === item.id}
          on:click={() => navigate(item.id)}
          title={!appState.railExpanded ? item.label : undefined}
        >
          <div class="icon-wrap">
            <svelte:component this={item.icon} size={20} strokeWidth={1.8} />
          </div>
          {#if appState.railExpanded}
            <span class="label">{item.label}</span>
          {/if}
        </button>
      {/each}
    </div>

    <div class="rail-divider"></div>

    <button
      class="rail-item"
      class:active={appState.currentScreen === 'settings'}
      on:click={() => navigate('settings')}
      title={!appState.railExpanded ? "Settings" : undefined}
    >
      <div class="icon-wrap">
        <Settings size={20} strokeWidth={1.8} />
      </div>
      {#if appState.railExpanded}
        <span class="label">Settings</span>
      {/if}
    </button>
  </div>
</nav>

<style>
  .nav-rail {
    width: 68px;
    height: 100vh;
    background: var(--z-surface);
    border-right: 1px solid var(--z-border);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: var(--z-space-3) var(--z-space-2);
    contain: paint;
    z-index: 50;
    flex-shrink: 0;
    user-select: none;
    box-sizing: border-box;
    overflow: hidden;
    transition: width var(--z-duration-normal) var(--z-ease-out),
                background var(--z-duration-normal) var(--z-ease-out);
  }

  .nav-rail.expanded {
    width: 190px;
  }

  .rail-top {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-3);
  }

  .rail-header {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    padding: 6px 0 10px 0;
  }

  .logo-btn {
    background: none;
    border: none;
    padding: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--z-radius-md);
    transition: transform 0.15s ease, background 0.15s ease;
  }

  .logo-btn:hover {
    background: var(--z-surface-raised);
    transform: scale(1.05);
  }

  .brand-logo-img {
    width: 32px;
    height: 32px;
    object-fit: contain;
    filter: drop-shadow(0 0 10px var(--z-accent-glow));
    display: block;
  }

  .items-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .rail-item {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    padding: 0 var(--z-space-2);
    border-radius: var(--z-radius-md);
    color: var(--z-text-muted);
    transition: background var(--z-duration-fast) var(--z-ease-out),
                color var(--z-duration-fast) var(--z-ease-out);
    position: relative;
    height: 42px;
    width: 100%;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    box-sizing: border-box;
  }

  .rail-item:hover {
    background: var(--z-surface-raised);
    color: var(--z-text);
  }

  .rail-item.active {
    background: var(--z-accent-muted);
    color: var(--z-accent);
    font-weight: 600;
  }

  .rail-item.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--z-accent);
  }

  .icon-wrap {
    width: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .label {
    font-size: var(--z-text-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rail-bottom {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .rail-divider {
    height: 1px;
    background: var(--z-border);
    margin: 6px 4px;
  }
</style>