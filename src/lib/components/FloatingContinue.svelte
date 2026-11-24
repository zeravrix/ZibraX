<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { openReader, openManga, mediaUrl } from '../stores/app.svelte.js';
  import { Play, BookOpen } from 'lucide-svelte';

  let recentManga = null;
  let recentProgress = null;

  async function checkRecentRead() {
    try {
      const history = await invoke('get_history');
      if (history && history.length > 0) {
        const top = history[0];
        const prog = await invoke('get_reading_progress', { mangaId: top.id });
        if (prog) {
          recentManga = top;
          recentProgress = prog;
        }
      }
    } catch (_) {}
  }

  async function handleContinue() {
    if (!recentManga) return;
    if (recentProgress && recentProgress.chapter_id) {
      try {
        const chapters = await invoke('get_chapters', {
          sourceId: recentManga.source_id,
          manga: recentManga,
        });
        if (chapters && chapters.length > 0) {
          const targetChapter = chapters.find(c => c.id === recentProgress.chapter_id) || chapters[0];
          openReader(recentManga, targetChapter, recentProgress.page_index || 0, chapters);
          return;
        }
      } catch (_) {}
    }
    openManga(recentManga);
  }

  onMount(() => {
    checkRecentRead();
  });
</script>

{#if recentManga}
  <button type="button" class="floating-continue-card" on:click={handleContinue}>
    <div class="thumb-wrap">
      {#if recentManga.cover_url}
        <img src={mediaUrl(recentManga.cover_url, null, 'cover')} alt={recentManga.title} class="thumb-img" />
      {:else}
        <div class="thumb-fallback">
          <BookOpen size={16} />
        </div>
      {/if}
    </div>

    <div class="continue-info">
      <div class="continue-badge">
        <span class="pulse-dot"></span>
        <span>CONTINUE READING</span>
      </div>
      <p class="manga-title">{recentManga.title}</p>
      {#if recentProgress && recentProgress.chapter_number}
        <p class="chapter-sub">Chapter {recentProgress.chapter_number}</p>
      {/if}
    </div>

    <span class="play-action-btn" aria-hidden="true">
      <Play size={16} fill="currentColor" />
    </span>
  </button>
{/if}

<style>
  .floating-continue-card {
    position: fixed;
    bottom: 24px;
    right: 28px;
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--z-surface-overlay);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
    border: 1px solid var(--z-border-hover);
    border-radius: 14px;
    padding: 8px 12px;
    box-shadow: var(--z-shadow-card), 0 0 20px var(--z-accent-muted);
    cursor: pointer;
    z-index: 999;
    text-align: left;
    transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
    max-width: 320px;
  }

  .floating-continue-card:hover {
    transform: translateY(-3px);
    border-color: rgba(59, 130, 246, 0.6);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.7), 0 0 28px rgba(59, 130, 246, 0.25);
  }

  .thumb-wrap {
    width: 40px;
    height: 52px;
    border-radius: 8px;
    overflow: hidden;
    flex-shrink: 0;
    background: var(--z-surface-raised);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
  }

  .thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-fallback {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--z-text-muted);
  }

  .continue-info {
    flex: 1;
    min-width: 0;
  }

  .continue-badge {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.8px;
    color: #60a5fa;
    margin-bottom: 2px;
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #3b82f6;
    box-shadow: 0 0 8px #3b82f6;
    animation: pulse 1.8s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.4; transform: scale(0.85); }
  }

  .manga-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--z-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.25;
  }

  .chapter-sub {
    font-size: 11px;
    color: var(--z-text-muted);
    line-height: 1.2;
  }

  .play-action-btn {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: var(--z-accent);
    color: #ffffff;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    box-shadow: 0 4px 12px rgba(37, 99, 235, 0.4);
    transition: transform 0.2s ease, background 0.2s ease;
  }

  .floating-continue-card:hover .play-action-btn {
    transform: scale(1.1);
    background: #1d4ed8;
  }
</style>
