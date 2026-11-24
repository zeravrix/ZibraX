<script>
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import CoverGrid from '../components/CoverGrid.svelte';
  import SearchBar from '../components/SearchBar.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import { appState, notify, openReader, navigate, mediaUrl, plainSnapshot } from '../stores/app.svelte.js';
  import { Heart, History, Trash2, Rss, Download, BookOpen, X } from 'lucide-svelte';

  export let mode = 'history'; // 'history' | 'favorites' | 'feed' | 'downloads'

  let mangas = [];
  let feedItems = [];
  let downloadItems = [];
  let loading = true;
  let searchQuery = '';
  let activeFilter = 'ALL'; // 'ALL' | 'READING' | 'COMPLETED'
  let progressMap = {};
  let favoritesSet = new Set();
  let dlPollTimer = null;

  async function mapConcurrent(items, limit, worker) {
    const out = [];
    let cursor = 0;
    const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (cursor < items.length) {
        const idx = cursor++;
        const res = await worker(items[idx]);
        if (res) out.push(res);
      }
    });
    await Promise.all(runners);
    return out;
  }

  async function refreshDownloadsOnly() {
    try {
      const dls = await invoke('get_downloads');
      downloadItems = dls;
      const seen = new Map();
      for (const d of dls) {
        if (!seen.has(d.manga_id)) {
          seen.set(d.manga_id, {
            id: d.manga_id,
            title: d.manga_title || 'Downloaded Title',
            cover_url: d.cover_url || '',
            source_id: d.source_id,
            url: ''
          });
        }
      }
      mangas = Array.from(seen.values());
    } catch (_) {
      downloadItems = [];
      mangas = [];
    }
  }

  async function loadLibraryData() {
    loading = true;
    try {
      const favs = await invoke('get_favorites');
      favoritesSet = new Set(favs.map(m => m.id));

      if (mode === 'favorites') {
        mangas = favs;
      } else if (mode === 'feed') {
        mangas = favs;
        feedItems = await mapConcurrent(favs.slice(0, 20), 4, async (m) => {
          try {
            const chs = await invoke('get_chapters', { sourceId: m.source_id, manga: m });
            if (chs && chs.length > 0) {
              return { manga: m, chapter: chs[0] };
            }
          } catch (_) {}
          return null;
        });
      } else if (mode === 'downloads') {
        await refreshDownloadsOnly();
      } else {
        mangas = await invoke('get_history');
      }

      // Load progress for each manga
      const pMap = {};
      await mapConcurrent(mangas, 8, async (m) => {
        try {
          const p = await invoke('get_reading_progress', { mangaId: m.id });
          if (p && p.total_pages > 0) {
            pMap[m.id] = Math.min(1, (p.page_index + 1) / p.total_pages);
          }
        } catch (_) {}
      });
      progressMap = pMap;
    } catch (e) {
      console.error('Failed to load library:', e);
      notify('Failed to load library data: ' + e);
    } finally {
      loading = false;
    }
  }

  async function handleCancelDownload(chapterId) {
    try {
      await invoke('cancel_download', { chapterId });
      await refreshDownloadsOnly();
      notify('Download cancelled');
    } catch (e) {
      notify('Failed to cancel download: ' + e);
    }
  }

  async function handleDeleteDownload(dl) {
    if (!dl.archive_path) return;
    try {
      await invoke('delete_downloaded_chapter', { archivePath: dl.archive_path });
      await refreshDownloadsOnly();
      notify(`Deleted "${dl.chapter_title}"`);
    } catch (e) {
      notify('Failed to delete archive: ' + e);
    }
  }

  function handleReadDownloaded(dl) {
    const manga = {
      id: dl.manga_id,
      title: dl.manga_title,
      cover_url: dl.cover_url || null,
      source_id: dl.source_id,
      description: '',
      genres: [],
      is_manhwa: true,
    };
    const chapter = {
      id: dl.chapter_id,
      manga_id: dl.manga_id,
      title: dl.chapter_title,
      source_id: dl.source_id,
      is_locked: false,
    };
    openReader(manga, chapter, 0, [chapter]);
  }

  async function handleToggleFavorite(manga) {
    try {
      const isFav = await invoke('toggle_favorite', { manga: plainSnapshot(manga) });
      if (isFav) {
        favoritesSet.add(manga.id);
        notify(`Added "${manga.title}" to favourites`);
      } else {
        favoritesSet.delete(manga.id);
        notify(`Removed "${manga.title}" from favourites`);
        if (mode === 'favorites') {
          mangas = mangas.filter(m => m.id !== manga.id);
        }
      }
      favoritesSet = new Set(favoritesSet);
    } catch (e) {
      notify('Error toggling favourite: ' + e);
    }
  }

  async function handleDeleteHistory(manga) {
    try {
      await invoke('remove_history', { mangaId: manga.id });
      mangas = mangas.filter(m => m.id !== manga.id);
      notify(`Removed "${manga.title}" from history`);
    } catch (e) {
      notify('Error removing from history: ' + e);
    }
  }

  async function handleSetStatus(manga, status) {
    try {
      await invoke('set_manga_status', { mangaId: manga.id, status, manga: plainSnapshot(manga) });
      manga.status = status;
      mangas = [...mangas];
      notify(`Marked "${manga.title}" as ${status === 'COMPLETED' ? 'Completed' : 'Reading'}`);
    } catch (e) {
      notify('Error updating status: ' + e);
    }
  }

  $: filteredMangas = mangas.filter(m => {
    const matchesSearch = !searchQuery || 
      m.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (m.author && m.author.toLowerCase().includes(searchQuery.toLowerCase()));

    const prog = progressMap[m.id] || 0;
    const isCompleted = m.status === 'COMPLETED' || prog >= 0.98;
    if (activeFilter === 'READING') {
      return matchesSearch && !isCompleted;
    } else if (activeFilter === 'COMPLETED') {
      return matchesSearch && isCompleted;
    }
    return matchesSearch;
  });

  onMount(() => {
    loadLibraryData();
    if (mode === 'downloads') {
      dlPollTimer = setInterval(refreshDownloadsOnly, 1500);
    }
  });

  onDestroy(() => {
    if (dlPollTimer) clearInterval(dlPollTimer);
  });

  $: filteredDownloads = downloadItems.filter(d =>
    !searchQuery ||
    d.manga_title.toLowerCase().includes(searchQuery.toLowerCase()) ||
    d.chapter_title.toLowerCase().includes(searchQuery.toLowerCase())
  );
</script>

<div class="library-view">
  <header class="header">
    <div class="header-left">
      <h1 class="page-title">
        {#if mode === 'favorites'}
          <Heart size={26} class="title-icon fav" fill="currentColor" />
          Favourites
        {:else if mode === 'feed'}
          <Rss size={26} class="title-icon feed" />
          Feed
        {:else if mode === 'downloads'}
          <Download size={26} class="title-icon dl" />
          Downloads
        {:else}
          <History size={26} class="title-icon hist" />
          History
        {/if}
      </h1>
      <span class="count-badge">{filteredMangas.length} titles</span>
    </div>

    <div class="header-right">
      <SearchBar bind:value={searchQuery} placeholder="Filter your shelf..." />
    </div>
  </header>

  <div class="filter-bar">
    <button 
      class="chip" 
      class:active={activeFilter === 'ALL'} 
      on:click={() => activeFilter = 'ALL'}
    >
      All Titles
    </button>
    <button 
      class="chip" 
      class:active={activeFilter === 'READING'} 
      on:click={() => activeFilter = 'READING'}
    >
      Reading
    </button>
    <button 
      class="chip" 
      class:active={activeFilter === 'COMPLETED'} 
      on:click={() => activeFilter = 'COMPLETED'}
    >
      Completed
    </button>
  </div>

  <div class="content-area">
    {#if loading}
      <div class="loading-state">
        <div class="spinner"></div>
        <p>Loading shelf...</p>
      </div>
    {:else if mode === 'feed'}
      {#if feedItems.length === 0}
        <EmptyState
          title="No Updates in Feed"
          description="Add manga to your Favourites to receive the newest chapter releases here."
          actionLabel="Explore Sources"
          onAction={() => navigate('explore')}
        />
      {:else}
        <div class="feed-list">
          {#each feedItems as item}
            <div 
              class="feed-item" 
              role="button"
              tabindex="0"
              on:click={() => openReader(item.manga, item.chapter, 0)}
              on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && openReader(item.manga, item.chapter, 0)}
            >
              <div class="feed-thumb-wrap">
                {#if item.manga.cover_url}
                  <img src={mediaUrl(item.manga.cover_url, null, 'cover')} alt={item.manga.title} class="feed-thumb" />
                {:else}
                  <div class="feed-thumb-fallback"><BookOpen size={16} /></div>
                {/if}
              </div>
              <div class="feed-info">
                <span class="feed-manga-title">{item.manga.title}</span>
                <div class="feed-chapter-row">
                  <span class="feed-ch-badge">{item.chapter.title || ('Chapter ' + (item.chapter.number ?? 'New'))}</span>
                  <span class="feed-source-tag">{item.manga.source_id}</span>
                </div>
              </div>
              <button class="feed-read-action" type="button">Read</button>
            </div>
          {/each}
        </div>
      {/if}
    {:else if mode === 'downloads'}
      {#if filteredDownloads.length === 0}
        <EmptyState
          title="No Downloads"
          description="Download chapters from any manga details page to read them offline."
          actionLabel="Explore Sources"
          onAction={() => navigate('explore')}
        />
      {:else}
        <div class="feed-list">
          {#each filteredDownloads as dl (dl.chapter_id)}
            <div class="feed-item">
              <div class="feed-thumb-wrap">
                {#if dl.cover_url}
                  <img src={mediaUrl(dl.cover_url, null, 'cover')} alt={dl.manga_title} class="feed-thumb" />
                {:else}
                  <div class="feed-thumb-fallback"><BookOpen size={16} /></div>
                {/if}
              </div>
              <div class="feed-info">
                <span class="feed-manga-title">{dl.manga_title}</span>
                <div class="feed-chapter-row">
                  <span class="feed-ch-badge">{dl.chapter_title}</span>
                  <span class="feed-source-tag">
                    {dl.status}{#if dl.total_pages > 0} • {dl.downloaded_pages}/{dl.total_pages} pages{/if}
                  </span>
                </div>
                {#if dl.status === 'Downloading' || dl.status === 'Queued'}
                  <div class="dl-progress-track">
                    <div class="dl-progress-fill" style="width: {Math.round(dl.progress * 100)}%"></div>
                  </div>
                {/if}
              </div>
              <div class="dl-actions">
                {#if dl.status === 'Completed'}
                  <button class="feed-read-action" type="button" on:click={() => handleReadDownloaded(dl)}>
                    Read Offline
                  </button>
                  {#if dl.archive_path}
                    <button
                      class="dl-icon-btn danger"
                      type="button"
                      title="Delete archive"
                      aria-label="Delete {dl.chapter_title}"
                      on:click={() => handleDeleteDownload(dl)}
                    >
                      <Trash2 size={15} />
                    </button>
                  {/if}
                {:else if dl.status === 'Downloading' || dl.status === 'Queued'}
                  <button
                    class="dl-icon-btn danger"
                    type="button"
                    title="Cancel download"
                    aria-label="Cancel {dl.chapter_title}"
                    on:click={() => handleCancelDownload(dl.chapter_id)}
                  >
                    <X size={15} />
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {:else}
      <CoverGrid
        mangas={filteredMangas}
        {progressMap}
        {favoritesSet}
        onToggleFavorite={handleToggleFavorite}
        onDelete={mode === 'history' ? handleDeleteHistory : null}
        onSetStatus={handleSetStatus}
        emptyMessage={
          mode === 'favorites' 
            ? 'No favourite manga yet. Explore to add some!' 
            : mode === 'downloads'
            ? 'No downloaded chapters found.'
            : 'No reading history yet.'
        }
      />
    {/if}
  </div>
</div>

<style>
  .library-view {
    flex: 1;
    height: 100vh;
    overflow-y: auto;
    padding: var(--z-space-6) var(--z-space-8);
    background: var(--z-bg);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--z-space-4);
    margin-bottom: var(--z-space-4);
    flex-wrap: wrap;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    flex-shrink: 0;
  }

  .header-right {
    flex: 1;
    max-width: 360px;
    min-width: 180px;
    display: flex;
    justify-content: flex-end;
  }

  @media (max-width: 680px) {
    .library-view {
      padding: var(--z-space-4);
    }
    .header {
      flex-direction: column;
      align-items: stretch;
      gap: var(--z-space-3);
    }
    .header-right {
      max-width: 100%;
      min-width: 0;
      width: 100%;
    }
  }

  .page-title {
    font-size: var(--z-text-2xl);
    font-weight: 700;
    color: var(--z-text);
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
  }

  :global(.title-icon.fav) {
    color: var(--z-accent);
  }

  :global(.title-icon.feed) {
    color: var(--z-accent);
  }

  :global(.title-icon.dl) {
    color: var(--z-accent);
  }

  :global(.title-icon.hist) {
    color: var(--z-accent);
  }

  /* FEED VIEW STYLING */
  .feed-list {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-3);
    width: 100%;
    max-width: 820px;
    padding: var(--z-space-4) 0;
  }

  .feed-item {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    padding: 8px 14px;
    cursor: pointer;
    transition: background var(--z-duration-fast) var(--z-ease-out), border-color var(--z-duration-fast) var(--z-ease-out);
  }

  .feed-item:hover {
    background: var(--z-surface-raised);
    border-color: var(--z-accent);
  }

  .feed-thumb-wrap {
    width: 44px;
    height: 58px;
    border-radius: var(--z-radius-sm);
    overflow: hidden;
    background: var(--z-surface-raised);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .feed-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .feed-thumb-fallback {
    color: var(--z-text-muted);
  }

  .feed-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .feed-manga-title {
    font-size: var(--z-text-sm);
    font-weight: 600;
    color: var(--z-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .feed-chapter-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .feed-ch-badge {
    font-size: var(--z-text-xs);
    font-weight: 600;
    color: var(--z-accent);
    background: var(--z-accent-muted);
    padding: 2px 8px;
    border-radius: 9999px;
  }

  .feed-source-tag {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    text-transform: uppercase;
  }

  .feed-read-action {
    padding: 6px 14px;
    background: var(--z-accent);
    color: #ffffff;
    font-size: var(--z-text-xs);
    font-weight: 600;
    border-radius: var(--z-radius-full);
    flex-shrink: 0;
    border: none;
    cursor: pointer;
  }

  .count-badge {
    background: var(--z-surface-raised);
    padding: 3px 10px;
    border-radius: var(--z-radius-full);
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    font-weight: 600;
  }

  .filter-bar {
    display: flex;
    align-items: center;
    gap: var(--z-space-2);
    margin-bottom: var(--z-space-4);
    border-bottom: 1px solid var(--z-border);
    padding-bottom: var(--z-space-3);
  }

  .chip {
    padding: 6px 14px;
    border-radius: var(--z-radius-full);
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    background: var(--z-surface-raised);
    transition: all var(--z-duration-fast) var(--z-ease-out);
    font-weight: 500;
  }

  .chip:hover {
    color: var(--z-text);
    background: var(--z-border-hover);
  }

  .chip.active {
    background: var(--z-accent);
    color: #ffffff;
    font-weight: 600;
  }

  .content-area {
    min-height: 400px;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 300px;
    gap: var(--z-space-3);
    color: var(--z-text-muted);
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--z-border);
    border-top-color: var(--z-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .dl-progress-track {
    width: 100%;
    height: 4px;
    border-radius: 9999px;
    background: var(--z-surface-raised);
    overflow: hidden;
    margin-top: 4px;
  }

  .dl-progress-fill {
    height: 100%;
    background: var(--z-accent);
    transition: width 0.2s ease;
  }

  .dl-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .dl-icon-btn {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--z-surface-raised);
    color: var(--z-text-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .dl-icon-btn.danger:hover {
    background: rgba(239, 68, 68, 0.2);
    color: #ef4444;
  }
</style>
