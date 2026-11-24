<script>
  import { BookOpen, Heart, Copy, Trash2, CheckCircle, Clock } from 'lucide-svelte';
  import ContextMenu from './ContextMenu.svelte';
  import { openManga, notify, mediaUrl } from '../stores/app.svelte.js';

  export let manga;
  export let progress = 0; // 0 to 1
  export let isFavorite = false;
  export let onToggleFavorite = null;
  export let onDelete = null;
  export let onSetStatus = null;

  let loaded = false;
  let error = false;
  let showMenu = false;
  let menuPos = { x: 0, y: 0 };

  $: rawCover = manga?.cover_url || '';
  $: coverUrl = rawCover ? mediaUrl(rawCover, null, 'cover') : '';
  $: title = manga?.title || 'Untitled';
  $: if (rawCover !== undefined) { loaded = false; error = false; }

  function handleKeydown(e) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.currentTarget.click();
    }
  }

  function handleContextMenu(e) {
    e.preventDefault();
    e.stopPropagation();
    menuPos = { x: e.clientX, y: e.clientY };
    showMenu = true;
  }

  $: isCompleted = manga.status === 'COMPLETED' || progress >= 0.99;
  $: menuItems = [
    {
      label: isFavorite ? "Remove from Favourites" : "Add to Favourites",
      icon: Heart,
      action: () => onToggleFavorite?.(manga),
    },
    ...(onSetStatus ? [
      {
        label: isCompleted ? "Mark as Reading" : "Mark as Completed",
        icon: isCompleted ? Clock : CheckCircle,
        action: () => onSetStatus(isCompleted ? 'READING' : 'COMPLETED'),
      }
    ] : []),
    {
      label: "Open Details",
      icon: BookOpen,
      action: () => openManga(manga),
    },
    {
      label: "Copy Title",
      icon: Copy,
      action: () => {
        if (navigator?.clipboard?.writeText) {
          navigator.clipboard.writeText(manga.title);
          notify("Copied title to clipboard");
        }
      },
    },
    ...(onDelete ? [
      { divider: true },
      {
        label: "Remove from History",
        icon: Trash2,
        danger: true,
        action: () => onDelete?.(manga),
      }
    ] : [])
  ];
</script>

<div class="card-container">
  <div
    class="card"
    role="button"
    tabindex="0"
    on:click
    on:keydown={handleKeydown}
    on:contextmenu={handleContextMenu}
    title={title}
  >
    <div class="cover-wrapper">
      {#if coverUrl && !error}
        <img
          src={coverUrl}
          alt={title}
          loading="lazy"
          decoding="async"
          class:loaded
          on:load={() => loaded = true}
          on:error={() => error = true}
        />
      {/if}

      {#if !loaded || error}
        <div class="placeholder">
          <BookOpen size={32} strokeWidth={1.4} />
        </div>
      {/if}

      {#if onToggleFavorite}
        <button
          type="button"
          class="fav-btn"
          class:active={isFavorite}
          on:click|stopPropagation={onToggleFavorite}
          title={isFavorite ? "Remove from favourites" : "Add to favourites"}
        >
          <Heart size={16} fill={isFavorite ? "currentColor" : "none"} strokeWidth={2} />
        </button>
      {/if}

      {#if progress > 0}
        <div class="progress-circle" title="{Math.round(progress * 100)}% completed">
          <svg class="progress-svg" viewBox="0 0 36 36">
            <path
              class="circle-bg"
              d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
            />
            <path
              class="circle-fill"
              stroke-dasharray="{Math.min(100, Math.round(progress * 100))}, 100"
              d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
            />
          </svg>
          <span class="circle-val">{Math.round(progress * 100)}%</span>
        </div>
      {/if}
    </div>

    <div class="meta">
      <p class="title">{title}</p>
    </div>
  </div>

  {#if showMenu}
    <ContextMenu
      x={menuPos.x}
      y={menuPos.y}
      items={menuItems}
      onClose={() => (showMenu = false)}
    />
  {/if}
</div>

<style>
  .card-container {
    width: 100%;
    position: relative;
  }

  .card {
    width: 100%;
    display: flex;
    flex-direction: column;
    text-align: left;
    cursor: pointer;
    outline: none;
    transition: transform var(--z-duration-normal) var(--z-ease-spring);
  }

  .card:hover {
    transform: translateY(-5px);
  }

  .cover-wrapper {
    width: 100%;
    aspect-ratio: 13 / 18;
    border-radius: var(--z-radius-lg);
    overflow: hidden;
    position: relative;
    background: var(--z-surface-raised);
    box-shadow: 0 8px 24px -4px rgba(0, 0, 0, 0.65), 0 2px 6px rgba(0, 0, 0, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.12);
    transition: transform var(--z-duration-normal) var(--z-ease-spring),
                box-shadow var(--z-duration-normal) var(--z-ease-out),
                border-color var(--z-duration-normal) var(--z-ease-out);
    border: 1px solid rgba(255, 255, 255, 0.08);
    isolation: isolate;
  }

  .card:hover .cover-wrapper {
    box-shadow: 0 18px 40px -6px rgba(0, 0, 0, 0.8), 0 0 24px var(--z-accent-glow);
    border-color: rgba(255, 255, 255, 0.22);
    transform: translateY(-4px);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    opacity: 0;
    transition: opacity var(--z-duration-normal) var(--z-ease-out);
    image-rendering: -webkit-optimize-contrast;
    image-rendering: high-quality;
  }

  img.loaded {
    opacity: 1;
  }

  .placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--z-text-muted);
    opacity: 0.4;
    background: linear-gradient(135deg, var(--z-surface-raised), var(--z-surface));
  }

  .fav-btn {
    position: absolute;
    top: var(--z-space-2);
    right: var(--z-space-2);
    width: 28px;
    height: 28px;
    border-radius: var(--z-radius-full);
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(8px);
    color: var(--z-text-muted);
    opacity: 0;
    transition: all var(--z-duration-fast) var(--z-ease-out);
    z-index: 10;
  }

  .fav-btn.active {
    opacity: 1;
    color: #ef4444;
  }

  .card:hover .fav-btn {
    opacity: 1;
  }

  .fav-btn:hover {
    transform: scale(1.15);
    background: rgba(0, 0, 0, 0.85);
  }

  .progress-circle {
    position: absolute;
    bottom: 8px;
    right: 8px;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: rgba(10, 10, 16, 0.88);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.7);
    border: 1px solid rgba(255, 255, 255, 0.12);
    z-index: 5;
  }

  .progress-svg {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }

  .circle-bg {
    fill: none;
    stroke: rgba(255, 255, 255, 0.15);
    stroke-width: 3.4;
  }

  .circle-fill {
    fill: none;
    stroke: #3b82f6;
    stroke-width: 3.4;
    stroke-linecap: round;
    transition: stroke-dasharray 0.3s ease;
  }

  .circle-val {
    font-size: 10px;
    font-weight: 700;
    color: #ffffff;
    z-index: 2;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }

  .meta {
    margin-top: var(--z-space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title {
    font-size: var(--z-text-sm);
    font-weight: 500;
    color: var(--z-text);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    transition: color var(--z-duration-fast) var(--z-ease-out);
  }

  .card:hover .title {
    color: var(--z-accent);
  }
</style>
