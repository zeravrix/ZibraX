<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { appState, goBack, openReader, notify, toChronologicalChapters, mediaUrl } from '../stores/app.svelte.js';
  import { 
    ArrowLeft, Heart, Play, Download, BookOpen, 
    Check, ShieldAlert, X, CheckCircle, Clock
  } from 'lucide-svelte';

  let manga = appState.activeManga;
  let chapters = [];
  let loading = true;
  let isFavorite = false;
  let progress = null;
  let sortAscending = false;
  let chapterFilter = '';
  let showCookieModal = false;
  let cookieInput = '';
  let unlistenCf = null;
  let readChapters = new Set();
  let downloadedIds = new Set();
  let queuingIds = new Set();

  async function refreshChapterBadges() {
    if (!manga) return;
    try {
      const [readIds, dls] = await Promise.all([
        invoke('get_read_chapters', { mangaId: manga.id }),
        invoke('get_downloaded_chapters'),
      ]);
      readChapters = new Set(readIds || []);
      downloadedIds = new Set((dls || []).map(d => d.chapter_id));
    } catch (_) {}
  }

  async function loadDetails() {
    if (!manga) return;
    loading = true;
    try {
      isFavorite = await invoke('is_favorite', { mangaId: manga.id });
      progress = await invoke('get_reading_progress', { mangaId: manga.id });

      // Fetch enriched details from source
      try {
        const enriched = await invoke('get_manga_details', {
          sourceId: manga.source_id,
          manga,
        });
        manga = enriched;
        appState.activeManga = enriched;
      } catch (e) {
        console.warn('Enriched details fetch note:', e);
      }

      // Fetch chapters & badges
      chapters = await invoke('get_chapters', {
        sourceId: manga.source_id,
        manga,
      });
      await refreshChapterBadges();

      // Record in history
      invoke('record_history', { manga });
    } catch (e) {
      console.error('Failed to load manga details:', e);
      notify('Failed to load details: ' + e);
    } finally {
      loading = false;
    }
  }

  async function handleDownloadChapter(chapter) {
    if (!manga || downloadedIds.has(chapter.id) || queuingIds.has(chapter.id)) return;
    queuingIds.add(chapter.id);
    queuingIds = new Set(queuingIds);
    try {
      await invoke('queue_download', { manga, chapter });
      notify(`Queued download: ${chapter.title}`);
      setTimeout(refreshChapterBadges, 2500);
    } catch (e) {
      notify('Download error: ' + e);
      queuingIds.delete(chapter.id);
      queuingIds = new Set(queuingIds);
    }
  }

  async function handleSolveCloudflare() {
    if (!manga) return;
    try {
      cookieInput = await invoke('get_source_cookies', { sourceId: manga.source_id });
    } catch (_) {
      cookieInput = '';
    }
    showCookieModal = true;
    try {
      const targetUrl = manga.url || 'https://google.com';
      await invoke('open_cloudflare_solver', {
        url: targetUrl,
        title: `${manga.title} — Z Browser Verification`,
        sourceId: manga.source_id,
      });
      notify('Opening Z Browser for verification...');
    } catch (e) {
      notify('Solver error: ' + e);
    }
  }

  async function handleSaveCookies() {
    if (!manga) return;
    try {
      const saved = await invoke('save_source_cookies', {
        sourceId: manga.source_id,
        cookie: cookieInput,
        baseUrl: manga.url,
      });
      if (saved) {
        cookieInput = saved;
        notify('Cookies synced and saved for source');
      } else {
        notify('Saved custom cookies for source');
      }
      showCookieModal = false;
      loadDetails();
    } catch (e) {
      notify('Error saving cookies: ' + e);
    }
  }

  async function handleClearCookies() {
    if (!manga) return;
    try {
      await invoke('clear_source_cookies', { sourceId: manga.source_id });
      cookieInput = '';
      notify('Cleared cookies for source');
    } catch (e) {
      notify('Error clearing cookies: ' + e);
    }
  }

  async function handleToggleFavorite() {
    try {
      isFavorite = await invoke('toggle_favorite', { manga });
      notify(isFavorite ? 'Added to favourites' : 'Removed from favourites');
    } catch (e) {
      notify('Error updating favourite: ' + e);
    }
  }

  function handleStartReading() {
    if (chapters.length === 0) return;
    const chronological = toChronologicalChapters(chapters);
    let target = chronological[0];
    let startPage = 0;
    if (progress && progress.chapter_id) {
      const found = chronological.find(c => c.id === progress.chapter_id);
      if (found) {
        target = found;
        startPage = progress.page_index || 0;
      }
    }
    openReader(manga, target, startPage, chapters);
  }

  async function handleToggleStatus() {
    if (!manga) return;
    const nextStatus = manga.status === 'COMPLETED' ? 'READING' : 'COMPLETED';
    try {
      await invoke('set_manga_status', { mangaId: manga.id, status: nextStatus });
      manga.status = nextStatus;
      appState.activeManga = { ...manga };
      notify(`Status: ${nextStatus === 'COMPLETED' ? 'Completed' : 'Reading'}`);
    } catch (e) {
      notify('Failed to update status: ' + e);
    }
  }

  $: displayedChapters = (() => {
    let list = [...chapters].filter(c => !chapterFilter || c.title.toLowerCase().includes(chapterFilter.toLowerCase()));
    return sortAscending ? list.reverse() : list;
  })();

  onMount(async () => {
    loadDetails();
    try {
      unlistenCf = await listen('cloudflare-solved', (event) => {
        if (event.payload?.source_id === manga?.source_id) {
          notify('Cloudflare verification passed! Refreshing...');
          showCookieModal = false;
          loadDetails();
        }
      });
    } catch (e) {
      console.warn('Could not register cloudflare listener in Details', e);
    }
  });

  onDestroy(() => {
    if (unlistenCf) unlistenCf();
  });
</script>

{#if manga}
  <div class="details-view">
    <!-- Top Bar -->
    <div class="top-nav">
      <button class="nav-btn" on:click={goBack} title="Back">
        <ArrowLeft size={20} strokeWidth={2} />
      </button>
      <span class="nav-title">{manga.title}</span>
      <button class="nav-btn" class:active={isFavorite} on:click={handleToggleFavorite} title="Favourite">
        <Heart size={20} fill={isFavorite ? "currentColor" : "none"} strokeWidth={2} />
      </button>
    </div>

    <!-- Hero Backdrop Banner -->
    <div class="hero-section">
      {#if manga.cover_url}
        <div class="backdrop" style:background-image="url('{mediaUrl(manga.cover_url, null, 'cover')}')"></div>
      {/if}
      <div class="backdrop-overlay"></div>

      <div class="hero-content">
        <div class="cover-card">
          {#if manga.cover_url}
            <img src={mediaUrl(manga.cover_url, null, 'cover')} alt={manga.title} />
          {:else}
            <div class="placeholder"><BookOpen size={40} /></div>
          {/if}
        </div>

        <div class="hero-info">
          <div class="tags-row">
            {#if manga.is_manhwa}
              <span class="badge manhwa">Manhwa</span>
            {/if}
            {#if manga.status}
              <span class="badge status">{manga.status}</span>
            {/if}
            <span class="badge source">{manga.source_id.replace(/^[^:]+:/, '')}</span>
          </div>

          <h1 class="manga-title">{manga.title}</h1>

          <div class="meta-row">
            {#if manga.author}
              <span class="meta-item"><User size={15} /> {manga.author}</span>
            {/if}
            <span class="meta-item"><BookOpen size={15} /> {chapters.length} Chapters</span>
          </div>

          <div class="genres-row">
            {#each (manga.genres || []) as genre}
              <span class="genre-pill">{genre}</span>
            {/each}
          </div>

          <div class="actions-row">
            <button class="primary-btn" on:click={handleStartReading} disabled={chapters.length === 0}>
              <Play size={18} fill="currentColor" />
              <span>{progress ? 'Continue Reading' : 'Start Reading'}</span>
            </button>
            <button class="status-btn" class:completed={manga.status === 'COMPLETED'} on:click={handleToggleStatus} title="Toggle Reading or Completed status">
              {#if manga.status === 'COMPLETED'}
                <CheckCircle size={16} />
                <span>Completed</span>
              {:else}
                <Clock size={16} />
                <span>Reading</span>
              {/if}
            </button>
            <button class="icon-action-btn" class:fav={isFavorite} on:click={handleToggleFavorite} title={isFavorite ? "In Library" : "Add to Library"}>
              <Heart size={18} fill={isFavorite ? "currentColor" : "none"} />
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Description & Chapters -->
    <div class="body-section">
      {#if manga.description}
        <div class="description-box">
          <h3 class="section-title">Synopsis</h3>
          <p class="description-text">{manga.description}</p>
        </div>
      {/if}

      <div class="chapters-header">
        <h3 class="section-title">Chapters ({displayedChapters.length})</h3>
        <div class="chapter-controls">
          <input
            type="search"
            class="ch-filter-input"
            placeholder="Filter chapters..."
            aria-label="Filter chapters"
            bind:value={chapterFilter}
          />
          <button type="button" class="sort-toggle" on:click={() => sortAscending = !sortAscending}>
            {sortAscending ? 'Oldest First' : 'Newest First'}
          </button>
        </div>
      </div>

      {#if loading}
        <div class="loading-chapters">
          <div class="spinner"></div>
          <p>Fetching chapter list...</p>
        </div>
      {:else if displayedChapters.length === 0}
        <div class="empty-cf-banner">
          <ShieldAlert size={28} class="empty-cf-icon" />
          <div class="empty-cf-info">
            <h3 class="empty-cf-title">No chapters available or access restricted</h3>
            <p class="empty-cf-desc">If this source is protected by Cloudflare Turnstile, click below to open Z Browser and pass the check.</p>
          </div>
          <button type="button" class="empty-cf-action" on:click={handleSolveCloudflare}>
            <ShieldAlert size={16} />
            Pass Cloudflare (Z Browser)
          </button>
        </div>
      {:else}
        <div class="chapters-list">
          {#each displayedChapters as chapter (chapter.id)}
            {@const isRead = readChapters.has(chapter.id)}
            {@const isDownloaded = downloadedIds.has(chapter.id)}
            <div
              class="chapter-row"
              class:current={progress?.chapter_id === chapter.id}
              class:read={isRead}
            >
              <button
                type="button"
                class="chapter-open-btn"
                on:click={() => openReader(manga, chapter, 0, chapters)}
              >
                <div class="chapter-info">
                  <span class="chapter-name">{chapter.title}</span>
                  {#if chapter.upload_date}
                    <span class="chapter-date">
                      {new Date(chapter.upload_date).toLocaleDateString()}
                    </span>
                  {/if}
                </div>
                <div class="chapter-actions">
                  {#if progress?.chapter_id === chapter.id}
                    <span class="reading-badge">Reading</span>
                  {:else if isRead}
                    <span class="read-badge">Read</span>
                  {/if}
                  <Play size={16} class="play-icon" />
                </div>
              </button>
              <button
                type="button"
                class="ch-dl-btn"
                class:downloaded={isDownloaded}
                title={isDownloaded ? 'Downloaded for offline reading' : 'Download chapter (.cbz)'}
                aria-label={isDownloaded ? `Downloaded ${chapter.title}` : `Download ${chapter.title}`}
                disabled={isDownloaded || queuingIds.has(chapter.id)}
                on:click={() => handleDownloadChapter(chapter)}
              >
                {#if isDownloaded}
                  <Check size={15} />
                {:else}
                  <Download size={15} />
                {/if}
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if showCookieModal}
    <div
      class="modal-backdrop"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      on:click|self={() => (showCookieModal = false)}
      on:keydown={(e) => e.key === 'Escape' && (showCookieModal = false)}
    >
      <div class="modal-card">
        <div class="modal-header">
          <div class="modal-title-group">
            <ShieldAlert size={20} class="modal-icon" />
            <h3>Z Browser Solver: {manga?.title}</h3>
          </div>
          <button class="close-icon-btn" on:click={() => (showCookieModal = false)}>
            <X size={18} />
          </button>
        </div>
        <p class="modal-desc">
          Z Browser has been launched. Complete any Cloudflare verification challenge and click <em>"Sync & Close"</em>.
        </p>
        <div class="manual-cookie-section">
          <label for="det-cookie-box" class="cookie-label">Or paste cookies manually (cf_clearance):</label>
          <textarea
            id="det-cookie-box"
            class="cookie-textarea"
            placeholder="cf_clearance=...; __cf_bm=...;"
            bind:value={cookieInput}
            rows="3"
          ></textarea>
        </div>
        <div class="modal-actions">
          <button class="modal-btn secondary" on:click={handleClearCookies}>Clear</button>
          <button class="modal-btn primary" on:click={handleSaveCookies}>
            <Check size={16} /> Save & Apply
          </button>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .details-view {
    flex: 1;
    height: 100vh;
    overflow-y: auto;
    background: var(--z-bg);
  }

  .top-nav {
    position: sticky;
    top: 0;
    left: 0;
    right: 0;
    height: 56px;
    background: var(--z-surface-overlay);
    backdrop-filter: blur(16px);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--z-space-4);
    z-index: 40;
    border-bottom: 1px solid var(--z-border);
  }

  .nav-btn {
    width: 38px;
    height: 38px;
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    transition: all var(--z-duration-fast) var(--z-ease-out);
  }

  .nav-btn:hover {
    background: var(--z-surface-raised);
  }

  .nav-btn.active {
    color: #ef4444;
  }

  .nav-title {
    font-size: var(--z-text-sm);
    font-weight: 600;
    color: var(--z-text);
    max-width: 60%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hero-section {
    position: relative;
    padding: var(--z-space-8) var(--z-space-8) var(--z-space-6);
    overflow: hidden;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background-size: cover;
    background-position: center;
    filter: blur(35px) brightness(0.25);
    transform: scale(1.1);
  }

  .backdrop-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, transparent 0%, var(--z-bg) 100%);
  }

  .hero-content {
    position: relative;
    display: flex;
    gap: var(--z-space-6);
    max-width: 1100px;
    margin: 0 auto;
  }

  .cover-card {
    width: 220px;
    aspect-ratio: 13 / 18;
    border-radius: var(--z-radius-lg);
    overflow: hidden;
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.6);
    flex-shrink: 0;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
  }

  .cover-card img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .hero-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--z-space-3);
  }

  .tags-row {
    display: flex;
    gap: var(--z-space-2);
  }

  .badge {
    padding: 3px 10px;
    border-radius: var(--z-radius-full);
    font-size: var(--z-text-xs);
    font-weight: 600;
    text-transform: uppercase;
  }

  .badge.manhwa {
    background: var(--z-accent-muted);
    color: var(--z-accent);
  }

  .badge.status {
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;
  }

  .badge.source {
    background: var(--z-surface-raised);
    color: var(--z-text-muted);
  }

  .manga-title {
    font-size: var(--z-text-2xl);
    font-weight: 800;
    color: var(--z-text);
    line-height: 1.25;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
    color: var(--z-text-muted);
    font-size: var(--z-text-sm);
  }

  .meta-item {
    display: flex;
    align-items: center;
    gap: var(--z-space-1);
  }

  .genres-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--z-space-2);
    margin: var(--z-space-1) 0;
  }

  .genre-pill {
    padding: 4px 12px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
  }

  .actions-row {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    margin-top: var(--z-space-2);
  }

  .primary-btn {
    display: flex;
    align-items: center;
    gap: var(--z-space-2);
    padding: 12px 28px;
    background: var(--z-accent);
    color: #ffffff;
    font-weight: 600;
    border-radius: var(--z-radius-md);
    box-shadow: 0 4px 16px var(--z-accent-glow);
    transition: all var(--z-duration-fast) var(--z-ease-out);
  }

  .primary-btn:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 6px 20px var(--z-accent-glow);
  }

  .icon-action-btn {
    width: 44px;
    height: 44px;
    border-radius: var(--z-radius-md);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    transition: all var(--z-duration-fast) var(--z-ease-out);
  }

  .icon-action-btn.fav {
    color: #ef4444;
  }

  .icon-action-btn:hover {
    background: var(--z-border-hover);
  }

  .status-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    border-radius: var(--z-radius-md);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    font-size: var(--z-text-sm);
    font-weight: 600;
    cursor: pointer;
    transition: all var(--z-duration-fast) var(--z-ease-out);
  }

  .status-btn:hover {
    background: var(--z-border-hover);
    border-color: var(--z-accent);
  }

  .status-btn.completed {
    background: rgba(34, 197, 94, 0.16);
    border-color: #22c55e;
    color: #22c55e;
  }

  .empty-cf-banner {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
    background: rgba(139, 92, 246, 0.08);
    border: 1px solid rgba(139, 92, 246, 0.3);
    border-radius: var(--z-radius-lg);
    padding: var(--z-space-5) var(--z-space-6);
    margin: var(--z-space-6) 0;
  }

  :global(.empty-cf-icon) {
    color: #a78bfa;
    flex-shrink: 0;
  }

  .empty-cf-info {
    flex: 1;
  }

  .empty-cf-title {
    font-size: var(--z-text-base);
    font-weight: 700;
    color: var(--z-text);
    margin-bottom: 4px;
  }

  .empty-cf-desc {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    line-height: 1.5;
  }

  .empty-cf-action {
    display: flex;
    align-items: center;
    gap: 8px;
    background: #7c3aed;
    color: #ffffff;
    font-size: var(--z-text-xs);
    font-weight: 600;
    padding: 8px 16px;
    border-radius: var(--z-radius-md);
    border: none;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.2s;
  }

  .empty-cf-action:hover {
    background: #6d28d9;
  }

  .body-section {
    max-width: 1100px;
    margin: 0 auto;
    padding: var(--z-space-6) var(--z-space-8) var(--z-space-8);
  }

  .description-box {
    margin-bottom: var(--z-space-6);
  }

  .section-title {
    font-size: var(--z-text-lg);
    font-weight: 700;
    margin-bottom: var(--z-space-3);
  }

  .description-text {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    line-height: 1.7;
    white-space: pre-line;
  }

  .chapters-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--z-space-3);
    border-bottom: 1px solid var(--z-border);
    padding-bottom: var(--z-space-2);
  }

  .sort-toggle {
    font-size: var(--z-text-xs);
    color: var(--z-accent);
    font-weight: 600;
    padding: 4px 8px;
    border-radius: var(--z-radius-sm);
  }

  .chapters-list {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-2);
  }

  .chapter-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px 4px 4px;
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    transition: all var(--z-duration-fast) var(--z-ease-out);
  }

  .chapter-row.read {
    opacity: 0.72;
  }

  .chapter-row:hover {
    background: var(--z-surface-raised);
    border-color: var(--z-border-hover);
    opacity: 1;
  }

  .chapter-row.current {
    border-color: var(--z-accent);
    background: var(--z-accent-muted);
    opacity: 1;
  }

  .chapter-open-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--z-space-2) var(--z-space-3);
    text-align: left;
  }

  .ch-filter-input {
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    padding: 5px 12px;
    font-size: var(--z-text-xs);
    color: var(--z-text);
    width: 160px;
  }

  .ch-filter-input:focus {
    border-color: var(--z-accent);
  }

  .chapter-controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ch-dl-btn {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    color: var(--z-text-muted);
    background: var(--z-surface-raised);
    flex-shrink: 0;
    transition: all 0.15s ease;
  }

  .ch-dl-btn:hover:not(:disabled) {
    color: var(--z-accent);
    background: var(--z-accent-muted);
  }

  .ch-dl-btn.downloaded {
    color: #10b981;
    background: rgba(16, 185, 129, 0.14);
  }

  .read-badge {
    font-size: var(--z-text-xs);
    background: var(--z-surface-raised);
    color: var(--z-text-muted);
    padding: 2px 8px;
    border-radius: var(--z-radius-full);
    font-weight: 500;
  }

  .chapter-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
  }

  .chapter-name {
    font-size: var(--z-text-sm);
    font-weight: 500;
    color: var(--z-text);
  }

  .chapter-date {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
  }

  .chapter-actions {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    color: var(--z-text-muted);
  }

  .reading-badge {
    font-size: var(--z-text-xs);
    background: var(--z-accent);
    color: #ffffff;
    padding: 2px 8px;
    border-radius: var(--z-radius-full);
    font-weight: 600;
  }

  .loading-chapters {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 200px;
    gap: var(--z-space-3);
    color: var(--z-text-muted);
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--z-border);
    border-top-color: var(--z-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.7);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--z-space-4);
  }

  .modal-card {
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    width: 100%;
    max-width: 520px;
    padding: var(--z-space-6);
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: var(--z-space-4);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .modal-title-group {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
  }

  .modal-title-group h3 {
    margin: 0;
    font-size: var(--z-text-lg);
    color: var(--z-text);
  }

  :global(.modal-icon) {
    color: var(--z-accent);
  }

  .close-icon-btn {
    background: transparent;
    border: none;
    color: var(--z-text-muted);
    cursor: pointer;
    padding: var(--z-space-1);
    border-radius: var(--z-radius-sm);
  }

  .close-icon-btn:hover {
    color: var(--z-text);
    background: var(--z-surface-variant);
  }

  .modal-desc {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .manual-cookie-section {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-2);
  }

  .cookie-label {
    font-size: var(--z-text-xs);
    font-weight: 600;
    color: var(--z-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .cookie-textarea {
    width: 100%;
    background: var(--z-bg);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    padding: var(--z-space-3);
    color: var(--z-text);
    font-family: monospace;
    font-size: var(--z-text-xs);
    resize: vertical;
    box-sizing: border-box;
  }

  .cookie-textarea:focus {
    outline: none;
    border-color: var(--z-accent);
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--z-space-3);
    margin-top: var(--z-space-2);
  }

  .modal-btn {
    padding: var(--z-space-2) var(--z-space-4);
    border-radius: var(--z-radius-md);
    font-size: var(--z-text-sm);
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: var(--z-space-2);
    border: none;
    transition: all 0.2s ease;
  }

  .modal-btn.primary {
    background: var(--z-accent);
    color: #ffffff;
  }

  .modal-btn.primary:hover {
    filter: brightness(1.1);
  }

  .modal-btn.secondary {
    background: var(--z-surface-variant);
    color: var(--z-text);
    border: 1px solid var(--z-border);
  }

  .modal-btn.secondary:hover {
    background: var(--z-border);
  }
</style>
