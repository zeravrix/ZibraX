<script>
  export let x = 0;
  export let y = 0;
  export let items = []; // [{ label, icon, danger, action, divider }]
  export let onClose = () => {};

  function handleKeydown(e) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window on:click={onClose} on:keydown={handleKeydown} />

<div
  class="context-menu"
  role="menu"
  tabindex="-1"
  style="top: {Math.max(10, Math.min(y, (typeof window !== 'undefined' ? window.innerHeight : 800) - (items.length * 38 + 24)))}px; left: {Math.max(10, Math.min(x, (typeof window !== 'undefined' ? window.innerWidth : 1200) - 200))}px;"
  on:click|stopPropagation
  on:keydown|stopPropagation={handleKeydown}
>
  {#each items as item}
    {#if item.divider}
      <div class="menu-divider"></div>
    {:else}
      <button
        type="button"
        role="menuitem"
        class="menu-item"
        class:danger={item.danger}
        on:click={() => { item.action?.(); onClose(); }}
      >
        {#if item.icon}
          <svelte:component this={item.icon} size={15} class="menu-icon" />
        {/if}
        <span>{item.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  .context-menu {
    position: fixed;
    z-index: 10000;
    min-width: 185px;
    background: var(--z-surface-overlay);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid var(--z-border-hover);
    border-radius: 12px;
    padding: 6px;
    box-shadow: var(--z-shadow-card);
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: menuIn 0.14s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes menuIn {
    from { opacity: 0; transform: scale(0.95) translateY(-4px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: 8px;
    color: var(--z-text);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    width: 100%;
    transition: all 0.15s ease;
  }

  .menu-item:hover {
    background: var(--z-accent-muted);
    color: var(--z-text);
  }

  .menu-item.danger {
    color: #ef4444;
  }

  .menu-item.danger:hover {
    background: rgba(239, 68, 68, 0.18);
    color: #dc2626;
  }

  :global(.menu-icon) {
    flex-shrink: 0;
    opacity: 0.85;
  }

  .menu-divider {
    height: 1px;
    background: var(--z-border);
    margin: 4px 0;
  }
</style>
