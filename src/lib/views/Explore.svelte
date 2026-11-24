<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import CoverGrid from '../components/CoverGrid.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ContextMenu from '../components/ContextMenu.svelte';
  import { appState, notify, loadSettings, toggleSourcePin, plainSnapshot, mediaUrl } from '../stores/app.svelte.js';
  import { 
    Compass, Globe, Flame, Clock, ShieldAlert, 
    Search, ArrowLeft, Star, ExternalLink, RefreshCw,
    BookOpen, Copy, X
  } from 'lucide-svelte';

  export let initialMode = 'pinned'; // 'pinned' | 'catalog' | 'browse'

  let viewMode = initialMode;
  let browseReturnMode = 'pinned';
  let allSources = [];
  let pinnedSourceIds = [];
  let selectedSource = null;
  let mangaList = [];
  let loading = false;
  let loadingMore = false;
  let loadingSources = false;
  let loadError = null;
  let cloudflareNeeded = false;
  let sourceCatalogSearch = '';
  let inSourceSearch = '';
  let selectedLang = 'ALL';
  let activeTab = 'latest'; // Default to latest manhwa
  let currentPage = 1;
  let hasNextPage = true;
  let favoritesSet = new Set();
  let unlistenCf = null;
  let scrollContainer;
  let catalogDisplayLimit = 32;
  let showSourceMenu = false;
  let sourceMenuPos = { x: 0, y: 0 };
  let contextSource = null;

  // Global manhwa search across pinned sources
  let pinnedManhwaQuery = '';
  let globalSearchResults = [];
  let isGlobalSearching = false;
  let globalSearchDone = false;

  let failedIcons = new Set();
  const languages = ['ALL', 'en', 'ar', 'es', 'fr', 'id', 'ru', 'ja', 'ko', 'pt', 'tr', 'vi', 'de', 'it'];

  $: pinnedSourceIds = Array.isArray(appState.settings?.pinned_source_ids)
    ? appState.settings.pinned_source_ids
    : [];

  function markIconFailed(sourceId) {
    if (!failedIcons.has(sourceId)) {
      failedIcons.add(sourceId);
      failedIcons = new Set(failedIcons);
    }
  }

  async function togglePin(sourceId, e) {
    if (e) e.stopPropagation();
    const wasPinned = pinnedSourceIds.includes(sourceId);
    const updated = await toggleSourcePin(sourceId);
    if (updated !== null) {
      notify(wasPinned ? 'Unpinned source' : 'Pinned source to Explore');
    }
  }

  async function handlePinnedSearchKeydown(e) {
    if (e.key === 'Enter') {
      runPinnedSearch();
    }
  }

  async function runPinnedSearch() {
    const q = pinnedManhwaQuery.trim();
    if (!q) {
      clearPinnedSearch();
      return;
    }
    isGlobalSearching = true;
    globalSearchDone = true;
    try {
      globalSearchResults = await invoke('search_global', {
        query: q,
        pinnedIds: pinnedSourceIds,
        nsfwEnabled: appState.settings?.nsfw_enabled ?? false,
      });
    } catch (e) {
      console.error('Global search error:', e);
      notify('Search error: ' + e);
      globalSearchResults = [];
    } finally {
      isGlobalSearching = false;
    }
  }

  function clearPinnedSearch() {
    pinnedManhwaQuery = '';
    globalSearchResults = [];
    isGlobalSearching = false;
    globalSearchDone = false;
  }

  function handleSourceContextMenu(e, source) {
    e.preventDefault();
    e.stopPropagation();
    contextSource = source;
    sourceMenuPos = { x: e.clientX, y: e.clientY };
    showSourceMenu = true;
  }

  $: sourceMenuItems = contextSource ? [
    {
      label: pinnedSourceIds.includes(contextSource.id) ? "Unpin from Explore" : "Pin to Explore",
      icon: Star,
      action: () => togglePin(contextSource.id),
    },
    {
      label: `Browse ${contextSource.name}`,
      icon: BookOpen,
      action: () => openSourceBrowser(contextSource),
    },
    {
      label: "Solve Cloudflare (Z Browser)",
      icon: ShieldAlert,
      action: () => {
        invoke('open_cloudflare_solver', {
          url: contextSource.base_url,
          title: `${contextSource.name} — Z Browser Verification`,
          sourceId: contextSource.id,
        });
        notify(`Opening Z Browser for ${contextSource.name}...`);
      },
    },
    { divider: true },
    {
      label: "Copy Website URL",
      icon: Copy,
      action: () => {
        if (navigator?.clipboard?.writeText) {
          navigator.clipboard.writeText(contextSource.base_url);
          notify("Copied source URL to clipboard");
        }
      },
    },
    {
      label: "Copy Source Name",
      icon: Copy,
      action: () => {
        if (navigator?.clipboard?.writeText) {
          navigator.clipboard.writeText(contextSource.name);
          notify("Copied source name to clipboard");
        }
      },
    }
  ] : [];

  let cachedAllSources = null;

  function filterSourcesByLang() {
    if (!cachedAllSources) return;
    if (!selectedLang || selectedLang === 'ALL') {
      allSources = cachedAllSources;
    } else {
      allSources = cachedAllSources.filter(s => s.language.toLowerCase() === selectedLang.toLowerCase());
    }
  }

  $: if (selectedLang && cachedAllSources) {
    filterSourcesByLang();
  }

  $: if (sourceCatalogSearch || selectedLang) {
    catalogDisplayLimit = 32;
  }

  async function loadSources(force = false) {
    if (cachedAllSources && !force) {
      filterSourcesByLang();
      return;
    }
    loadingSources = true;
    try {
      cachedAllSources = await invoke('get_sources', {
        langFilter: 'ALL',
        query: '',
        nsfwEnabled: appState.settings?.nsfw_enabled ?? false,
      });
      filterSourcesByLang();
    } catch (e) {
      console.error('Failed to load sources:', e);
      notify('Failed to load sources: ' + e);
    } finally {
      loadingSources = false;
    }
  }

  async function loadFavorites() {
    try {
      const favs = await invoke('get_favorites');
      favoritesSet = new Set(favs.map(m => m.id));
    } catch (_) {}
  }

  async function openSourceBrowser(source) {
    if (viewMode !== 'browse') browseReturnMode = viewMode;
    selectedSource = source;
    viewMode = 'browse';
    currentPage = 1;
    mangaList = [];
    loadError = null;
    cloudflareNeeded = false;
    inSourceSearch = '';
    activeTab = 'latest';
    loadMangas(1, false);
  }

  async function loadMangas(page = 1, append = false) {
    if (!selectedSource) return;
    if (append) {
      loadingMore = true;
    } else {
      loading = true;
      loadError = null;
      cloudflareNeeded = false;
    }

    try {
      const res = await invoke('fetch_source_page', {
        sourceId: selectedSource.id,
        page,
        latest: activeTab === 'latest',
        query: inSourceSearch.trim() ? inSourceSearch.trim() : null,
        nsfwEnabled: appState.settings?.nsfw_enabled ?? false,
      });

      if (append) {
        mangaList = [...mangaList, ...res.items];
      } else {
        mangaList = res.items;
      }
      currentPage = page;
      hasNextPage = res.has_next_page;
      loadError = null;
      cloudflareNeeded = false;
    } catch (e) {
      console.error('Failed to load source page:', e);
      loadError = String(e);
      if (!append) {
        mangaList = [];
        const errStr = String(e).toLowerCase();
        cloudflareNeeded = errStr.includes('403') || errStr.includes('cloudflare') || errStr.includes('challenge');
      }
      notify(`Error from ${selectedSource.name}: ${e}`);
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  function handleTabChange(tab) {
    if (activeTab === tab) return;
    activeTab = tab;
    currentPage = 1;
    mangaList = [];
    loadMangas(1, false);
  }

  function handleSearchKeydown(e) {
    if (e.key === 'Enter') {
      currentPage = 1;
      mangaList = [];
      loadMangas(1, false);
    }
  }

  function handleScroll(e) {
    const { scrollTop, scrollHeight, clientHeight } = e.target;
    if (viewMode === 'catalog') {
      if (scrollTop + clientHeight >= scrollHeight - 450) {
        if (catalogDisplayLimit < filteredCatalogSources.length) {
          catalogDisplayLimit = Math.min(filteredCatalogSources.length, catalogDisplayLimit + 40);
        }
      }
    } else if (viewMode === 'browse') {
      if (loading || loadingMore || !hasNextPage) return;
      if (scrollTop + clientHeight >= scrollHeight - 450) {
        loadMangas(currentPage + 1, true);
      }
    }
  }

  async function handleSolveCloudflare() {
    if (!selectedSource) return;
    try {
      await invoke('open_cloudflare_solver', {
        url: selectedSource.base_url,
        title: `${selectedSource.name} — Z Browser Verification`,
        sourceId: selectedSource.id,
      });
      notify(`Opening Z Browser for ${selectedSource.name}...`);
    } catch (e) {
      notify('Solver error: ' + e);
    }
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
      }
      favoritesSet = new Set(favoritesSet);
    } catch (e) {
      notify('Error toggling favourite: ' + e);
    }
  }

  // Reactive computed lists
  $: pinnedSources = allSources.filter(s => pinnedSourceIds.includes(s.id));
  
  $: filteredCatalogSources = allSources.filter(s => {
    if (!sourceCatalogSearch.trim()) return true;
    const q = sourceCatalogSearch.toLowerCase().trim();
    return s.name.toLowerCase().includes(q) || s.base_url.toLowerCase().includes(q) || s.language.toLowerCase().includes(q);
  });

  onMount(async () => {
    if (!appState.settings) {
      await loadSettings();
    }
    await loadFavorites();
    await loadSources();

    try {
      unlistenCf = await listen('cloudflare-solved', (event) => {
        if (event.payload?.source_id === selectedSource?.id) {
          notify(`Cloudflare verification passed for ${selectedSource.name}! Loading manhwa...`);
          cloudflareNeeded = false;
          loadMangas(1, false);
        }
      });
    } catch (e) {
      console.warn('Could not register cloudflare listener', e);
    }
  });

  onDestroy(() => {
    if (unlistenCf) unlistenCf();
  });
</script>

<div class="explore-container" on:scroll={handleScroll} bind:this={scrollContainer}>
  <!-- VIEW 1: PINNED SOURCES (EXPLORE) -->
  {#if viewMode === 'pinned'}
    <header class="explore-hero-header">
      <div class="header-top-row">
        <div class="header-titles">
          <h1 class="main-title">
            <Compass size={24} class="brand-icon" />
            <span>Explore</span>
          </h1>
          <p class="subtitle">Search & read from your {pinnedSources.length} pinned sources</p>
        </div>

        <!-- SMALL CATALOG BUTTON IN UPPER RIGHT -->
        <button class="catalog-mini-btn" on:click={() => (viewMode = 'catalog')} title="Browse full 1,190+ sources catalog">
          <Globe size={15} />
          <span>Catalog</span>
          <span class="catalog-badge">{allSources.length}</span>
        </button>
      </div>

      <!-- MANHWA SEARCH BAR ACROSS PINNED SOURCES -->
      <div class="explore-search-wrap">
        <Search size={16} class="search-bar-icon" />
        <input
          type="text"
          placeholder="Search manhwa across all {pinnedSources.length} pinned sources (Press Enter)..."
          bind:value={pinnedManhwaQuery}
          on:keydown={handlePinnedSearchKeydown}
          class="explore-search-input"
        />
        {#if pinnedManhwaQuery}
          <button class="clear-search-btn" on:click={clearPinnedSearch} title="Clear search">
            <X size={15} />
          </button>
        {/if}
      </div>
    </header>

    {#if isGlobalSearching}
      <div class="loading-state">
        <div class="spinner"></div>
        <p>Searching "{pinnedManhwaQuery}" across your pinned sources...</p>
      </div>
    {:else if globalSearchDone}
      <div class="search-results-section">
        <div class="results-meta-header">
          <span class="results-count-text">{globalSearchResults.length} titles found for "{pinnedManhwaQuery}"</span>
          <button class="clear-results-link" on:click={clearPinnedSearch}>
            <span>← Back to Pinned Sources</span>
          </button>
        </div>

        {#if globalSearchResults.length === 0}
          <EmptyState
            title="No Titles Found"
            description="No manhwa found matching '{pinnedManhwaQuery}' in your pinned sources."
            actionLabel="Search Full 1,190+ Catalog"
            onAction={() => { viewMode = 'catalog'; sourceCatalogSearch = pinnedManhwaQuery; }}
          />
        {:else}
          <CoverGrid
            mangas={globalSearchResults}
            {favoritesSet}
            onToggleFavorite={handleToggleFavorite}
          />
        {/if}
      </div>
    {:else if pinnedSources.length === 0}
      <EmptyState
        title="No Pinned Sources Yet"
        description="Select and star your favorite sources from the full 1,190+ catalog to view them here."
        actionLabel="Browse 1,190+ Sources"
        onAction={() => (viewMode = 'catalog')}
      />
    {:else}
      <div class="sources-grid">
        {#each pinnedSources as source (source.id)}
          <div
            class="source-card"
            role="button"
            tabindex="0"
            on:click={() => openSourceBrowser(source)}
            on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), openSourceBrowser(source))}
            on:contextmenu={(e) => handleSourceContextMenu(e, source)}
          >
            <div class="card-top">
              <div class="source-icon-wrap">
                {#if source.icon_url && !failedIcons.has(source.id)}
                  <img src={mediaUrl(source.icon_url, null, 'cover')} alt="" class="source-favicon" loading="lazy" decoding="async" on:error={() => markIconFailed(source.id)} />
                {:else}
                  <Globe size={24} class="source-fallback-icon" />
                {/if}
              </div>
              <button
                type="button"
                class="pin-btn active"
                title="Unpin from Explore"
                on:click={(e) => togglePin(source.id, e)}
              >
                <Star size={16} fill="currentColor" />
              </button>
            </div>

            <h3 class="source-name">{source.name}</h3>
            <span class="source-domain">{source.base_url.replace(/^https?:\/\//, '').replace(/\/$/, '')}</span>

            <div class="card-footer">
              <span class="lang-tag">{source.language.toUpperCase()}</span>
              <span class="open-link">Browse Titles →</span>
            </div>
          </div>
        {/each}
      </div>
    {/if}

  <!-- VIEW 2: FULL SOURCE CATALOG (1,190+ SOURCES) -->
  {:else if viewMode === 'catalog'}
    <header class="view-header">
      <div class="header-titles">
        <div class="title-with-back">
          <button class="back-link-btn" on:click={() => (viewMode = 'pinned')}>
            <ArrowLeft size={18} />
            <span>Pinned</span>
          </button>
          <h1 class="main-title">
            <Globe size={28} class="brand-icon" />
            <span>Source Catalog</span>
          </h1>
        </div>
      </div>

      <div class="search-wrap">
        <Search size={16} class="search-icon" />
        <input
          type="text"
          placeholder="Filter sources by name or url..."
          bind:value={sourceCatalogSearch}
          class="catalog-search-input"
        />
      </div>
    </header>

    <!-- Language Filter Tabs -->
    <div class="lang-strip">
      {#each languages as lang}
        <button
          class="lang-pill"
          class:active={selectedLang === lang}
          on:click={() => { selectedLang = lang; loadSources(); }}
        >
          {lang.toUpperCase()}
        </button>
      {/each}
    </div>

    {#if loadingSources}
      <div class="loading-state">
        <div class="spinner"></div>
        <p>Loading catalog sources...</p>
      </div>
    {:else if filteredCatalogSources.length === 0}
      <EmptyState
        title="No Sources Found"
        description="No sources matched '{sourceCatalogSearch}'. Try changing language or search keyword."
        actionLabel="Reset Filters"
        onAction={() => { sourceCatalogSearch = ''; selectedLang = 'ALL'; loadSources(); }}
      />
    {:else}
      <div class="sources-grid">
        {#each filteredCatalogSources.slice(0, catalogDisplayLimit) as source (source.id)}
          <div
            class="source-card"
            role="button"
            tabindex="0"
            on:click={() => openSourceBrowser(source)}
            on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), openSourceBrowser(source))}
            on:contextmenu={(e) => handleSourceContextMenu(e, source)}
          >
            <div class="card-top">
              <div class="source-icon-wrap">
                {#if source.icon_url && !failedIcons.has(source.id)}
                  <img src={mediaUrl(source.icon_url, null, 'cover')} alt="" class="source-favicon" loading="lazy" decoding="async" on:error={() => markIconFailed(source.id)} />
                {:else}
                  <Globe size={24} class="source-fallback-icon" />
                {/if}
              </div>
              <button
                type="button"
                class="pin-btn"
                class:active={pinnedSourceIds.includes(source.id)}
                title={pinnedSourceIds.includes(source.id) ? "Unpin" : "Pin to Explore"}
                on:click={(e) => togglePin(source.id, e)}
              >
                <Star size={16} fill={pinnedSourceIds.includes(source.id) ? "currentColor" : "none"} />
              </button>
            </div>

            <h3 class="source-name">{source.name}</h3>
            <span class="source-domain">{source.base_url.replace(/^https?:\/\//, '').replace(/\/$/, '')}</span>

            <div class="card-footer">
              <span class="lang-tag">{source.language.toUpperCase()}</span>
              <span class="open-link">Open Source →</span>
            </div>
          </div>
        {/each}
      </div>

      {#if catalogDisplayLimit < filteredCatalogSources.length}
        <div class="load-more-manual-wrap">
          <button
            class="load-more-btn"
            on:click={() => (catalogDisplayLimit = Math.min(filteredCatalogSources.length, catalogDisplayLimit + 32))}
          >
            <span>Load More Sources (Showing {catalogDisplayLimit} of {filteredCatalogSources.length})</span>
          </button>
        </div>
      {/if}
    {/if}

  <!-- VIEW 3: SOURCE MANHWA BROWSER (SELECTED SOURCE TITLES) -->
  {:else if viewMode === 'browse'}
    <header class="browse-header">
      <div class="browse-left">
        <button class="back-round-btn" on:click={() => (viewMode = browseReturnMode)} title="Back to sources">
          <ArrowLeft size={18} />
        </button>

        <div class="source-meta-block">
          <div class="source-icon-mini">
            {#if selectedSource?.icon_url && !failedIcons.has(selectedSource.id)}
              <img src={mediaUrl(selectedSource.icon_url, null, 'cover')} alt="" class="mini-favicon" on:error={() => markIconFailed(selectedSource.id)} />
            {:else}
              <Globe size={18} />
            {/if}
          </div>
          <div>
            <h2 class="browser-source-name">{selectedSource?.name}</h2>
            <span class="browser-source-lang">{selectedSource?.language.toUpperCase()} • {selectedSource?.base_url}</span>
          </div>
        </div>
      </div>

      <div class="browse-right">
        <!-- Segmented Tab: Popular / Latest -->
        <div class="segmented-control">
          <button
            class="segment-opt"
            class:active={activeTab === 'popular'}
            on:click={() => handleTabChange('popular')}
          >
            <Flame size={15} />
            <span>Popular</span>
          </button>
          <button
            class="segment-opt"
            class:active={activeTab === 'latest'}
            on:click={() => handleTabChange('latest')}
          >
            <Clock size={15} />
            <span>Latest</span>
          </button>
        </div>

        <!-- In-source search -->
        <div class="source-search-wrap">
          <Search size={15} class="search-icon" />
          <input
            type="text"
            placeholder="Search in {selectedSource?.name}..."
            bind:value={inSourceSearch}
            on:keydown={handleSearchKeydown}
            class="in-source-search"
          />
        </div>
      </div>
    </header>

    <div class="browse-body">
      {#if loading}
        <div class="loading-state">
          <div class="spinner"></div>
          <p>Fetching titles from {selectedSource?.name}...</p>
        </div>

      <!-- CENTERED CLOUDFLARE BANNER ONLY IF NEEDED -->
      {:else if cloudflareNeeded || (loadError && mangaList.length === 0)}
        <div class="cloudflare-center-card">
          <div class="cf-shield-glow">
            <ShieldAlert size={52} class="cf-icon" />
          </div>
          <h2 class="cf-title">Cloudflare Verification Required</h2>
          <p class="cf-desc">
            Access to <strong>{selectedSource?.name}</strong> is currently protected by Cloudflare Turnstile or anti-bot challenge.
          </p>
          <div class="cf-action-group">
            <button class="cf-primary-btn" on:click={handleSolveCloudflare}>
              <ShieldAlert size={18} />
              <span>Pass Cloudflare (Z Browser)</span>
            </button>
            <button class="cf-retry-btn" on:click={() => loadMangas(1, false)}>
              <RefreshCw size={16} />
              <span>Retry</span>
            </button>
          </div>
          <span class="cf-note">Opens Z Browser window. Cookies sync automatically when solved.</span>
        </div>

      {:else if mangaList.length === 0}
        <EmptyState
          title={loadError ? "Source Issue Detected" : "No Titles Found"}
          description={loadError || `No manga or manhwa returned from ${selectedSource?.name} for the ${activeTab} tab.`}
          actionLabel={activeTab === 'latest' ? "Try Popular Tab" : "Try Latest Tab"}
          onAction={() => handleTabChange(activeTab === 'latest' ? 'popular' : 'latest')}
        />

      {:else}
        <CoverGrid
          mangas={mangaList}
          {favoritesSet}
          onToggleFavorite={handleToggleFavorite}
          emptyMessage="No titles available."
        />

        {#if loadingMore}
          <div class="loading-more-bar">
            <div class="mini-spinner"></div>
            <span>Auto-loading next page...</span>
          </div>
        {:else if hasNextPage}
          <div class="load-more-manual-wrap">
            <button class="load-more-btn" on:click={() => loadMangas(currentPage + 1, true)}>
              <span>Load More Titles (Page {currentPage + 1})</span>
            </button>
          </div>
        {:else if mangaList.length > 0}
          <div class="end-catalog-bar">
            <span>✓ Reached end of {selectedSource?.name} • {mangaList.length} titles loaded</span>
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  {#if showSourceMenu}
    <ContextMenu
      x={sourceMenuPos.x}
      y={sourceMenuPos.y}
      items={sourceMenuItems}
      onClose={() => (showSourceMenu = false)}
    />
  {/if}
</div>

<style>
  .explore-container {
    flex: 1;
    height: 100vh;
    overflow-y: auto;
    padding: var(--z-space-6) var(--z-space-8);
    background: var(--z-bg);
  }

  .explore-hero-header {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-4);
    width: 100%;
    margin-bottom: var(--z-space-6);
  }

  .header-top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    gap: var(--z-space-3);
  }

  .catalog-mini-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 14px;
    background: var(--z-accent-muted);
    border: 1px solid var(--z-border-hover);
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    flex-shrink: 0;
  }

  .catalog-mini-btn:hover {
    background: var(--z-accent);
    border-color: var(--z-accent);
    color: #ffffff;
    transform: translateY(-2px);
    box-shadow: 0 4px 16px var(--z-accent-glow);
  }

  .catalog-badge {
    background: var(--z-accent);
    color: #ffffff;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: var(--z-radius-full);
  }

  .explore-search-wrap {
    position: relative;
    width: 100%;
    max-width: 600px;
    display: flex;
    align-items: center;
  }

  .explore-search-wrap :global(.search-bar-icon) {
    position: absolute;
    left: 14px;
    color: var(--z-text-muted);
    pointer-events: none;
  }

  .explore-search-input {
    width: 100%;
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    padding: 10px 40px 10px 40px;
    color: var(--z-text);
    font-size: var(--z-text-sm);
    outline: none;
    transition: all 0.2s ease;
  }

  .explore-search-input:focus {
    background: var(--z-surface-raised);
    border-color: var(--z-accent);
    box-shadow: 0 0 0 3px var(--z-accent-muted);
  }

  .clear-search-btn {
    position: absolute;
    right: 12px;
    background: none;
    border: none;
    color: var(--z-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .clear-search-btn:hover {
    color: var(--z-text);
  }

  .results-meta-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0 16px;
    border-bottom: 1px solid var(--z-border);
    margin-bottom: var(--z-space-4);
  }

  .results-count-text {
    font-size: 13px;
    font-weight: 600;
    color: var(--z-text);
  }

  .clear-results-link {
    font-size: 12px;
    font-weight: 600;
    color: var(--z-accent);
    background: none;
    border: none;
    cursor: pointer;
    text-decoration: underline;
  }

  .view-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--z-space-4);
    margin-bottom: var(--z-space-6);
    flex-wrap: wrap;
  }

  .header-titles {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .title-with-back {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .back-link-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text-muted);
    font-size: 13px;
    font-weight: 600;
    padding: 6px 12px;
    border-radius: var(--z-radius-full);
    cursor: pointer;
    transition: all 0.2s;
  }

  .back-link-btn:hover {
    color: var(--z-text);
    border-color: var(--z-border-hover);
  }

  .main-title {
    font-size: var(--z-text-2xl);
    font-weight: 800;
    color: var(--z-text);
    display: flex;
    align-items: center;
    gap: 10px;
    letter-spacing: -0.5px;
  }

  :global(.brand-icon) {
    color: var(--z-accent);
  }

  .search-wrap {
    position: relative;
    width: 300px;
  }

  .search-wrap :global(.search-icon),
  .source-search-wrap :global(.search-icon) {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--z-text-muted);
    pointer-events: none;
  }

  .catalog-search-input {
    width: 100%;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    padding: 9px 16px 9px 36px;
    color: var(--z-text);
    font-size: var(--z-text-xs);
    outline: none;
    transition: all 0.2s;
  }

  .catalog-search-input:focus {
    border-color: var(--z-accent);
    box-shadow: 0 0 0 3px var(--z-accent-muted);
  }

  .lang-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 12px;
    margin-bottom: var(--z-space-6);
  }

  .lang-pill {
    padding: 6px 14px;
    border-radius: var(--z-radius-full);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text-muted);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .lang-pill:hover {
    color: var(--z-text);
    border-color: var(--z-border-hover);
  }

  .lang-pill.active {
    background: var(--z-accent);
    border-color: var(--z-accent);
    color: #ffffff;
    box-shadow: 0 2px 10px var(--z-accent-glow);
  }

  /* Sources Grid */
  .sources-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--z-space-4);
    margin-bottom: var(--z-space-8);
  }

  .source-card {
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    padding: var(--z-space-4);
    display: flex;
    flex-direction: column;
    cursor: pointer;
    box-shadow: var(--z-shadow-card);
    transition: transform var(--z-duration-normal) var(--z-ease-spring),
                border-color var(--z-duration-fast) var(--z-ease-out),
                box-shadow var(--z-duration-normal) var(--z-ease-out);
  }

  .source-card:hover {
    transform: translateY(-4px);
    border-color: var(--z-accent);
    box-shadow: var(--z-shadow-card-hover);
  }

  .load-more-manual-wrap {
    display: flex;
    justify-content: center;
    padding: 24px 0 36px;
    width: 100%;
  }

  .load-more-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 12px 28px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border-hover);
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    font-size: var(--z-text-sm);
    font-weight: 600;
    cursor: pointer;
    box-shadow: var(--z-shadow-card);
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .load-more-btn:hover {
    background: var(--z-accent);
    border-color: var(--z-accent);
    color: #ffffff;
    transform: translateY(-2px);
    box-shadow: 0 8px 24px var(--z-accent-glow);
  }

  .end-catalog-bar {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 28px 0 40px;
    color: var(--z-text-muted);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.3px;
  }

  .card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--z-space-3);
  }

  .source-icon-wrap {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--z-surface-raised);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .source-favicon {
    width: 24px;
    height: 24px;
    object-fit: contain;
  }

  :global(.source-fallback-icon) {
    color: var(--z-accent);
  }

  .pin-btn {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: transparent;
    border: none;
    color: var(--z-text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.2s;
  }

  .pin-btn:hover {
    background: var(--z-surface-raised);
    color: #facc15;
  }

  .pin-btn.active {
    color: #facc15;
  }

  .source-name {
    font-size: var(--z-text-base);
    font-weight: 700;
    color: var(--z-text);
    margin-bottom: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .source-domain {
    font-size: 11px;
    color: var(--z-text-muted);
    margin-bottom: var(--z-space-4);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
    padding-top: var(--z-space-3);
    border-top: 1px solid var(--z-border);
  }

  .lang-tag {
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.5px;
    background: var(--z-accent-muted);
    color: var(--z-accent);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .open-link {
    font-size: 11px;
    font-weight: 600;
    color: var(--z-text-muted);
    transition: color 0.2s;
  }

  .source-card:hover .open-link {
    color: var(--z-accent);
  }

  /* BROWSE VIEW */
  .browse-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--z-space-4);
    margin-bottom: var(--z-space-6);
    flex-wrap: wrap;
    padding-bottom: var(--z-space-4);
    border-bottom: 1px solid var(--z-border);
  }

  .browse-left {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
  }

  .back-round-btn {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.2s;
  }

  .back-round-btn:hover {
    background: var(--z-border-hover);
    transform: translateX(-2px);
  }

  .source-meta-block {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .source-icon-mini {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: var(--z-surface-raised);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mini-favicon {
    width: 24px;
    height: 24px;
    object-fit: contain;
  }

  .browser-source-name {
    font-size: var(--z-text-lg);
    font-weight: 800;
    color: var(--z-text);
    line-height: 1.2;
  }

  .browser-source-lang {
    font-size: 11px;
    color: var(--z-text-muted);
  }

  .browse-right {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    flex-wrap: wrap;
  }

  .segmented-control {
    display: flex;
    background: var(--z-surface-raised);
    padding: 3px;
    border-radius: var(--z-radius-full);
    border: 1px solid var(--z-border);
  }

  .segment-opt {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: var(--z-radius-full);
    border: none;
    background: transparent;
    color: var(--z-text-muted);
    font-size: var(--z-text-xs);
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .segment-opt.active {
    background: var(--z-accent);
    color: #ffffff;
    box-shadow: 0 2px 8px var(--z-accent-glow);
  }

  .source-search-wrap {
    position: relative;
    width: 220px;
  }

  .in-source-search {
    width: 100%;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    padding: 7px 14px 7px 32px;
    font-size: var(--z-text-xs);
    color: var(--z-text);
    outline: none;
    transition: all 0.2s;
  }

  .in-source-search:focus {
    border-color: #3b82f6;
  }

  /* Centered Cloudflare Box */
  .cloudflare-center-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    max-width: 520px;
    margin: 60px auto;
    padding: 40px 32px;
    background: rgba(18, 18, 26, 0.85);
    backdrop-filter: blur(16px);
    border: 1px solid rgba(139, 92, 246, 0.35);
    border-radius: 20px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.7), 0 0 30px rgba(139, 92, 246, 0.15);
    text-align: center;
  }

  .cf-shield-glow {
    position: relative;
    margin-bottom: var(--z-space-4);
  }

  :global(.cf-icon) {
    color: #a78bfa;
    filter: drop-shadow(0 0 16px rgba(139, 92, 246, 0.6));
  }

  .cf-title {
    font-size: var(--z-text-xl);
    font-weight: 800;
    color: var(--z-text);
    margin-bottom: var(--z-space-2);
  }

  .cf-desc {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    line-height: 1.6;
    margin-bottom: var(--z-space-6);
  }

  .cf-action-group {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    margin-bottom: var(--z-space-4);
  }

  .cf-primary-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px;
    background: #7c3aed;
    color: #ffffff;
    font-size: var(--z-text-sm);
    font-weight: 700;
    border-radius: var(--z-radius-full);
    border: none;
    cursor: pointer;
    box-shadow: 0 4px 20px rgba(124, 58, 237, 0.4);
    transition: all 0.2s;
  }

  .cf-primary-btn:hover {
    background: #6d28d9;
    transform: translateY(-2px);
    box-shadow: 0 8px 26px rgba(124, 58, 237, 0.5);
  }

  .cf-retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 18px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    font-size: var(--z-text-sm);
    font-weight: 600;
    border-radius: var(--z-radius-full);
    cursor: pointer;
    transition: all 0.2s;
  }

  .cf-retry-btn:hover {
    background: var(--z-border-hover);
  }

  .cf-note {
    font-size: 11px;
    color: var(--z-text-muted);
    max-width: 380px;
    line-height: 1.4;
  }

  .loading-more-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px 0;
    color: var(--z-text-muted);
    font-size: var(--z-text-xs);
  }

  .mini-spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--z-border);
    border-top-color: #3b82f6;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 350px;
    gap: var(--z-space-3);
    color: var(--z-text-muted);
  }

  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid var(--z-border);
    border-top-color: #3b82f6;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
