<script>
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { appState, goBack, notify, updateSettings, toChronologicalChapters, mediaUrl } from '../stores/app.svelte.js';
  import {
    clampZoom,
    needsNextChapterFetch,
    getNextSequentialPageToLoad,
  } from '../utils/readerNav.js';
  import {
    ArrowLeft, Settings, ZoomIn, ZoomOut, ChevronLeft, ChevronRight,
    Sliders, Maximize2, Minimize2, RotateCcw, ShieldAlert,
    BookOpen, Columns, AlignJustify
  } from 'lucide-svelte';

  const SLOW_IMAGE_MS = 8000;

  $: preloadAheadCount = Math.max(1, Math.min(10, Number(appState.settings?.preload_pages_ahead ?? 3)));
  $: infiniteScrollEnabled = appState.settings?.infinite_chapter_scroll ?? true;
  $: fitToWidth = appState.settings?.fit_to_width ?? true;

  function pageMediaSrc(item) {
    if (!item?.page?.image_url) return '';
    return mediaUrl(
      item.page.image_url,
      item.page.referer || item.chapter?.url || item.chapter?.id || null,
      'page',
      item.page.headers || null,
    );
  }

  let manga = appState.activeManga;
  let chapter = appState.activeChapter;

  // Endless chapter stream state (chronological: first -> latest)
  let orderedChapters = toChronologicalChapters(appState.activeChapters);
  let loadedChapters = []; // [{ chapter, pages }]
  let nextChapterToLoadIdx = -1;
  let loadingNextChapter = false;
  let nextChapterError = null;

  let loading = true;
  let loadError = null;
  let unlistenCf = null;
  let currentGlobalIdx = 0;
  let showHud = true;
  let showSettings = false;
  let isFullscreen = false;

  // Sequential page load state: key -> 'loading' | 'loaded' | 'slow' | 'error'
  let pageStates = {};
  const loadTimers = new Map();
  const preloadImages = new Map();
  let scrollRafId = null;
  let lastSavedKey = '';

  // Reader configuration (loaded from settings or defaults)
  let readerMode = 'webtoon'; // 'webtoon' | 'single' | 'double'
  let readingDirection = 'LTR'; // 'LTR' | 'RTL'
  let pageGap = 0; // 0, 4, 8 px for webtoon
  let zoomPercent = 100;
  let contrast = 100; // 50 to 150
  let brightness = 100; // 50 to 150
  let invertColors = false;

  let scrollContainer;
  let draggingSlider = null;
  const measuredHeights = new Map();

  function captureHeight(node, key) {
    return {
      destroy() {
        const h = node?.getBoundingClientRect?.().height;
        if (h > 40) measuredHeights.set(key, Math.round(h));
      }
    };
  }

  $: activeChapterBlockIdx = activeFlatPage?.chapterBlockIdx ?? 0;
  function isChapterInWindow(blockIdx) {
    return blockIdx >= activeChapterBlockIdx - 1 && blockIdx <= activeChapterBlockIdx + 2;
  }

  $: flatPages = (() => {
    const out = [];
    let globalIdx = 0;
    for (let blockIdx = 0; blockIdx < loadedChapters.length; blockIdx++) {
      const block = loadedChapters[blockIdx];
      const chPages = block.pages || [];
      for (let pageIdx = 0; pageIdx < chPages.length; pageIdx++) {
        out.push({
          key: `${block.chapter.id}::${pageIdx}`,
          globalIdx,
          chapterBlockIdx: blockIdx,
          chapter: block.chapter,
          pageIdx,
          chapterTotalPages: chPages.length,
          page: chPages[pageIdx],
        });
        globalIdx++;
      }
    }
    return out;
  })();

  $: activeFlatPage = flatPages[currentGlobalIdx] || null;
  $: activeChapter = activeFlatPage?.chapter || chapter;
  $: currentPageInChapter = activeFlatPage?.pageIdx ?? 0;
  $: currentChapterTotalPages = activeFlatPage?.chapterTotalPages ?? 0;
  $: hasNextChapter =
    orderedChapters.length > 0 &&
    nextChapterToLoadIdx >= 0 &&
    nextChapterToLoadIdx < orderedChapters.length;
  $: nextChapterObj = hasNextChapter ? orderedChapters[nextChapterToLoadIdx] : null;

  // Double spread computed pages within the current chapter
  $: isCoverSpread = readerMode === 'double' && currentPageInChapter === 0;
  $: spreadEntries = (() => {
    if (readerMode !== 'double' || !activeFlatPage) return [];
    const blockStart = currentGlobalIdx - currentPageInChapter;
    const blockEnd = blockStart + currentChapterTotalPages - 1;

    if (currentPageInChapter === 0) {
      return [flatPages[blockStart]].filter(Boolean);
    }
    const basePageInCh = currentPageInChapter % 2 === 1 ? currentPageInChapter : currentPageInChapter - 1;
    const g1 = blockStart + basePageInCh;
    const g2 = g1 + 1 <= blockEnd ? g1 + 1 : -1;
    const p1 = flatPages[g1];
    const p2 = g2 >= 0 ? flatPages[g2] : null;

    if (readingDirection === 'RTL') {
      return [p2, p1].filter(Boolean);
    }
    return [p1, p2].filter(Boolean);
  })();

  function clearAllLoadTimers() {
    for (const timer of loadTimers.values()) {
      clearTimeout(timer);
    }
    loadTimers.clear();
    preloadImages.clear();
  }

  function clearTimerFor(key) {
    const t = loadTimers.get(key);
    if (t) {
      clearTimeout(t);
      loadTimers.delete(key);
    }
  }

  function markPageLoaded(key) {
    clearTimerFor(key);
    if (pageStates[key] !== 'loaded') {
      pageStates = { ...pageStates, [key]: 'loaded' };
    }
    pumpPageLoader();
  }

  function markPageError(key) {
    clearTimerFor(key);
    if (pageStates[key] !== 'error') {
      pageStates = { ...pageStates, [key]: 'error' };
    }
    pumpPageLoader();
  }

  function retryPage(key) {
    clearTimerFor(key);
    preloadImages.delete(key);
    const next = { ...pageStates };
    delete next[key];
    pageStates = next;
    pumpPageLoader();
  }

  function trackCachedLoad(node, key) {
    if (node.complete && node.naturalWidth > 0) {
      markPageLoaded(key);
    }
    return {
      update(newKey) {
        key = newKey;
        if (node.complete && node.naturalWidth > 0) {
          markPageLoaded(key);
        }
      },
    };
  }

  function pumpPageLoader() {
    if (flatPages.length === 0) return;
    const effectiveAhead = readerMode === 'double' && !isCoverSpread ? preloadAheadCount + 1 : preloadAheadCount;
    const nextIdx = getNextSequentialPageToLoad(flatPages, pageStates, currentGlobalIdx, effectiveAhead);
    if (nextIdx === null) return;

    const item = flatPages[nextIdx];
    if (!item?.page?.image_url) {
      markPageError(item.key);
      return;
    }

    const key = item.key;
    pageStates = { ...pageStates, [key]: 'loading' };

    clearTimerFor(key);
    const timer = setTimeout(() => {
      loadTimers.delete(key);
      // Allow queue to continue without unmounting the slow image.
      if (pageStates[key] === 'loading') {
        pageStates = { ...pageStates, [key]: 'slow' };
        pumpPageLoader();
      }
    }, SLOW_IMAGE_MS);
    loadTimers.set(key, timer);

    // In single/double modes, upcoming pages are not mounted in the DOM yet, so preload via Image().
    if (readerMode !== 'webtoon') {
      const isCurrentlyRendered =
        readerMode === 'single'
          ? nextIdx === currentGlobalIdx
          : spreadEntries.some((entry) => entry.globalIdx === nextIdx);

      if (!isCurrentlyRendered && !preloadImages.has(key)) {
        const img = new Image();
        preloadImages.set(key, img);
        img.onload = () => markPageLoaded(key);
        img.onerror = () => markPageError(key);
        img.src = pageMediaSrc(item);
      }
    }
  }

  function syncNextChapterPointer() {
    if (orderedChapters.length === 0 || loadedChapters.length === 0) return;
    const lastLoaded = loadedChapters[loadedChapters.length - 1].chapter;
    const idx = orderedChapters.findIndex((c) => c.id === lastLoaded.id);
    nextChapterToLoadIdx = idx >= 0 ? idx + 1 : -1;
  }

  async function ensureChapterOrderLoaded() {
    if (!manga || !chapter) return;
    if (orderedChapters.length === 0) {
      try {
        const fetched = await invoke('get_chapters', {
          sourceId: manga.source_id,
          manga,
        });
        orderedChapters = toChronologicalChapters(fetched);
      } catch (e) {
        console.warn('Background chapter list fetch failed:', e);
      }
    }
    syncNextChapterPointer();
  }

  async function loadNextChapter() {
    if (!manga || loadingNextChapter || !hasNextChapter) return false;
    const targetChapter = orderedChapters[nextChapterToLoadIdx];
    if (!targetChapter) return false;

    // Prevent duplicates if already loaded
    if (loadedChapters.some((b) => b.chapter.id === targetChapter.id)) {
      nextChapterToLoadIdx += 1;
      return true;
    }

    loadingNextChapter = true;
    nextChapterError = null;
    try {
      const nextPages = await invoke('get_pages', {
        sourceId: manga.source_id,
        chapter: targetChapter,
      });

      if (!nextPages || nextPages.length === 0) {
        nextChapterError = `No images returned for ${targetChapter.title || 'next chapter'}.`;
        return false;
      }

      loadedChapters = [...loadedChapters, { chapter: targetChapter, pages: nextPages }];
      nextChapterToLoadIdx += 1;
      await tick();
      pumpPageLoader();

      // Handle very short chapters (< preloadAheadCount pages) by immediately chaining if still near end
      if (infiniteScrollEnabled && needsNextChapterFetch(flatPages.length, currentGlobalIdx, preloadAheadCount, hasNextChapter)) {
        loadingNextChapter = false;
        return await loadNextChapter();
      }
      return true;
    } catch (e) {
      console.error('Failed to load next chapter:', e);
      nextChapterError = String(e);
      return false;
    } finally {
      loadingNextChapter = false;
    }
  }

  function ensureAheadLoaded(globalIdx) {
    if (!infiniteScrollEnabled || loadingNextChapter || nextChapterError) return;
    const effectiveAhead = readerMode === 'double' && !isCoverSpread ? preloadAheadCount + 1 : preloadAheadCount;
    if (needsNextChapterFetch(flatPages.length, globalIdx, effectiveAhead, hasNextChapter)) {
      loadNextChapter();
    }
  }

  async function loadInitialChapter() {
    if (!manga || !chapter) return;
    loading = true;
    loadError = null;
    nextChapterError = null;
    clearAllLoadTimers();
    pageStates = {};
    loadedChapters = [];
    currentGlobalIdx = 0;

    try {
      // Fetch pages for the opened chapter first for fastest time-to-first-page
      const initialPages = await invoke('get_pages', {
        sourceId: manga.source_id,
        chapter,
      });

      if (!initialPages || initialPages.length === 0) {
        loadError = 'No images returned for this chapter.';
        return;
      }

      loadedChapters = [{ chapter, pages: initialPages }];

      // Determine starting page: explicit initialPageIndex > saved progress for this chapter > Page 1 (0)
      let startIdx = 0;
      let savedScrollY = 0;
      try {
        if (appState.initialPageIndex > 0 && appState.initialPageIndex < initialPages.length) {
          startIdx = appState.initialPageIndex;
        } else {
          const prog = await invoke('get_reading_progress', { mangaId: manga.id });
          if (prog && prog.chapter_id === chapter.id) {
            if (prog.page_index > 0 && prog.page_index < initialPages.length) {
              startIdx = prog.page_index;
            }
            if (typeof prog.scroll_offset_y === 'number' && prog.scroll_offset_y > 0) {
              savedScrollY = prog.scroll_offset_y;
            }
          }
        }
      } catch (_) {}

      currentGlobalIdx = startIdx;
      loading = false;

      await tick();
      if (readerMode === 'webtoon' && scrollContainer) {
        if (savedScrollY > 0 && startIdx === 0) {
          scrollContainer.scrollTop = savedScrollY;
        } else if (startIdx > 0) {
          const targetSlot = scrollContainer.querySelector(`[data-global-index="${startIdx}"]`);
          if (targetSlot) {
            targetSlot.scrollIntoView({ behavior: 'auto' });
          } else if (savedScrollY > 0) {
            scrollContainer.scrollTop = savedScrollY;
          }
        } else {
          scrollContainer.scrollTop = 0;
        }
      }

      // Start strict sequential loading from startIdx forward (Page 1 -> 2 -> 3 -> 4)
      pumpPageLoader();
      saveCurrentProgress();

      // Ensure full chronological chapter list is available, then preload next chapter if near end
      await ensureChapterOrderLoaded();
      ensureAheadLoaded(currentGlobalIdx);
    } catch (e) {
      console.error('Failed to load chapter pages:', e);
      loadError = String(e);
      notify('Failed to load pages: ' + e);
    } finally {
      loading = false;
    }
  }

  async function handleSolveCloudflare() {
    if (!manga) return;
    try {
      const targetUrl = activeChapter?.url || nextChapterObj?.url || manga.url || 'https://google.com';
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

  function setZoom(val) {
    zoomPercent = clampZoom(val, 50, 250);
  }

  function handleWheel(e) {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const delta = e.deltaY < 0 ? 10 : -10;
      setZoom(zoomPercent + delta);
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Tab') {
      e.preventDefault();
      showSettings = !showSettings;
      if (showSettings) showHud = true;
      return;
    }

    if (e.key === 'Escape') {
      if (showSettings) {
        showSettings = false;
      } else if (isFullscreen) {
        toggleFullscreen();
      } else {
        goBack();
      }
      return;
    }

    if (e.key.toLowerCase() === 'f') {
      toggleFullscreen();
      return;
    }

    if (e.key === '+' || e.key === '=') {
      e.preventDefault();
      setZoom(zoomPercent + 10);
      return;
    }
    if (e.key === '-' || e.key === '_') {
      e.preventDefault();
      setZoom(zoomPercent - 10);
      return;
    }
    if (e.key === '0') {
      e.preventDefault();
      setZoom(100);
      return;
    }

    // Navigation hotkeys
    const isRtl = readingDirection === 'RTL';
    if (e.key === 'ArrowRight' || e.key === 'PageDown' || e.key === ' ') {
      e.preventDefault();
      if (isRtl) {
        prevStep();
      } else {
        nextStep();
      }
    } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
      e.preventDefault();
      if (isRtl) {
        nextStep();
      } else {
        prevStep();
      }
    }
  }

  async function nextStep() {
    if (readerMode === 'webtoon' && scrollContainer) {
      scrollContainer.scrollBy({ top: window.innerHeight * 0.75, behavior: 'smooth' });
      return;
    }

    if (readerMode === 'double' && activeFlatPage) {
      const blockStart = currentGlobalIdx - currentPageInChapter;
      const blockEnd = blockStart + currentChapterTotalPages - 1;
      let nextIdx;
      if (currentPageInChapter === 0) {
        nextIdx = blockStart + 1 <= blockEnd ? blockStart + 1 : blockEnd + 1;
      } else {
        const baseInCh = currentPageInChapter % 2 === 1 ? currentPageInChapter : currentPageInChapter - 1;
        const candidate = blockStart + baseInCh + 2;
        nextIdx = candidate <= blockEnd ? candidate : blockEnd + 1;
      }

      if (nextIdx >= flatPages.length && hasNextChapter) {
        const ok = await loadNextChapter();
        if (!ok) return;
      }
      if (nextIdx < flatPages.length) {
        setGlobalPage(nextIdx);
      }
      return;
    }

    // Single page mode
    if (currentGlobalIdx + 1 >= flatPages.length && hasNextChapter) {
      const ok = await loadNextChapter();
      if (!ok) return;
    }
    if (currentGlobalIdx + 1 < flatPages.length) {
      setGlobalPage(currentGlobalIdx + 1);
    }
  }

  function prevStep() {
    if (readerMode === 'webtoon' && scrollContainer) {
      scrollContainer.scrollBy({ top: -window.innerHeight * 0.75, behavior: 'smooth' });
      return;
    }

    if (readerMode === 'double' && activeFlatPage) {
      if (currentPageInChapter === 0) {
        if (currentGlobalIdx > 0) {
          setGlobalPage(currentGlobalIdx - 1);
        }
        return;
      }
      const blockStart = currentGlobalIdx - currentPageInChapter;
      if (currentPageInChapter <= 2) {
        setGlobalPage(blockStart);
      } else {
        const baseInCh = currentPageInChapter % 2 === 1 ? currentPageInChapter : currentPageInChapter - 1;
        setGlobalPage(blockStart + Math.max(1, baseInCh - 2));
      }
      return;
    }

    if (currentGlobalIdx > 0) {
      setGlobalPage(currentGlobalIdx - 1);
    }
  }

  function setGlobalPage(index) {
    if (flatPages.length === 0) return;
    const clamped = Math.max(0, Math.min(flatPages.length - 1, index));
    currentGlobalIdx = clamped;
    saveCurrentProgress();
    ensureAheadLoaded(clamped);
    pumpPageLoader();

    if (readerMode === 'webtoon' && scrollContainer) {
      const targetSlot = scrollContainer.querySelector(`[data-global-index="${clamped}"]`);
      if (targetSlot) {
        targetSlot.scrollIntoView({ behavior: 'smooth' });
      }
    } else if (scrollContainer) {
      scrollContainer.scrollTop = 0;
      scrollContainer.scrollLeft = 0;
    }
  }

  function processScrollPosition() {
    scrollRafId = null;
    if (readerMode !== 'webtoon' || !scrollContainer) return;

    const containerRect = scrollContainer.getBoundingClientRect();
    const probeY = containerRect.top + containerRect.height * 0.35;
    const slots = scrollContainer.querySelectorAll('.reader-page-slot');

    let matchedIdx = currentGlobalIdx;
    for (let i = 0; i < slots.length; i++) {
      const rect = slots[i].getBoundingClientRect();
      if (rect.top <= probeY && rect.bottom > probeY) {
        const idxAttr = Number(slots[i].dataset.globalIndex);
        if (!Number.isNaN(idxAttr)) matchedIdx = idxAttr;
        break;
      }
    }

    if (matchedIdx !== currentGlobalIdx) {
      currentGlobalIdx = matchedIdx;
      saveCurrentProgress();
      ensureAheadLoaded(currentGlobalIdx);
      pumpPageLoader();
    }

    // Fallback trigger near scroll bottom so endless chapter loading never misses
    const distFromBottom =
      scrollContainer.scrollHeight - (scrollContainer.scrollTop + scrollContainer.clientHeight);
    if (infiniteScrollEnabled && distFromBottom < 1200 && hasNextChapter && !loadingNextChapter && !nextChapterError) {
      loadNextChapter();
    }
  }

  function handleScroll() {
    if (readerMode !== 'webtoon') return;
    if (scrollRafId === null) {
      scrollRafId = requestAnimationFrame(processScrollPosition);
    }
  }

  function saveCurrentProgress() {
    const entry = flatPages[currentGlobalIdx];
    if (!manga || !entry) return;
    const ch = entry.chapter;
    const saveKey = `${ch.id}:${entry.pageIdx}`;
    if (saveKey === lastSavedKey) return;
    lastSavedKey = saveKey;
    appState.activeChapter = ch;

    invoke('save_reading_progress', {
      progress: {
        manga_id: manga.id,
        chapter_id: ch.id,
        page_index: entry.pageIdx,
        total_pages: entry.chapterTotalPages || null,
        scroll_offset_y: readerMode === 'webtoon' && scrollContainer ? scrollContainer.scrollTop : 0.0,
        chapter_number: ch.number ?? 0.0,
        updated_at: Date.now(),
      },
    }).catch(() => {});
  }

  function toggleHud() {
    showHud = !showHud;
    if (!showHud) showSettings = false;
  }

  function toggleFullscreen() {
    if (!document.fullscreenElement) {
      document.documentElement.requestFullscreen().then(() => (isFullscreen = true)).catch(() => {});
    } else {
      document.exitFullscreen().then(() => (isFullscreen = false)).catch(() => {});
    }
  }

  async function changeReaderMode(mode) {
    readerMode = mode;
    if (appState.settings) {
      appState.settings.reader_mode = mode === 'webtoon' ? 'Vertical' : mode === 'double' ? 'Double' : 'Single';
      updateSettings(appState.settings);
    }
    await tick();
    if (mode === 'webtoon' && scrollContainer) {
      const targetSlot = scrollContainer.querySelector(`[data-global-index="${currentGlobalIdx}"]`);
      if (targetSlot) targetSlot.scrollIntoView({ behavior: 'auto' });
    }
    pumpPageLoader();
  }

  function changeDirection(dir) {
    readingDirection = dir;
    if (appState.settings) {
      appState.settings.reading_direction = dir;
      updateSettings(appState.settings);
    }
  }

  // Custom slider mouse handler
  function startDrag(field, min, max, step, e) {
    e.preventDefault();
    draggingSlider = field;
    const track = e.currentTarget.closest('.custom-track');
    if (!track) return;

    const updateVal = (clientX) => {
      const rect = track.getBoundingClientRect();
      const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      let val = min + ratio * (max - min);
      if (step) val = Math.round(val / step) * step;

      if (field === 'zoom') setZoom(val);
      else if (field === 'brightness') brightness = val;
      else if (field === 'contrast') contrast = val;
    };

    updateVal(e.clientX);

    const onMove = (ev) => updateVal(ev.clientX);
    const onUp = () => {
      draggingSlider = null;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  onMount(async () => {
    window.addEventListener('keydown', handleKeydown);
    if (appState.settings?.reader_mode) {
      const rm = appState.settings.reader_mode.toLowerCase();
      if (rm.includes('double')) readerMode = 'double';
      else if (rm.includes('single')) readerMode = 'single';
      else if (rm === 'auto') readerMode = manga?.is_manhwa ? 'webtoon' : 'single';
      else readerMode = 'webtoon';
    }
    if (appState.settings?.reading_direction) {
      readingDirection = appState.settings.reading_direction;
    }
    if (typeof appState.settings?.seam_trim_px === 'number') {
      pageGap = appState.settings.seam_trim_px;
    }

    loadInitialChapter();
    try {
      unlistenCf = await listen('cloudflare-solved', (event) => {
        if (event.payload?.source_id === manga?.source_id) {
          notify('Cloudflare verification passed! Reloading...');
          if (loadedChapters.length === 0) {
            loadInitialChapter();
          } else if (nextChapterError) {
            nextChapterError = null;
            loadNextChapter();
          }
        }
      });
    } catch (_) {}
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeydown);
    if (scrollRafId !== null) cancelAnimationFrame(scrollRafId);
    clearAllLoadTimers();
    lastSavedKey = '';
    saveCurrentProgress();
    if (unlistenCf) unlistenCf();
  });
</script>

<div
  class="reader-container"
  class:invert={invertColors}
  on:click={toggleHud}
  on:keydown={(e) => { if (e.key === 'h' || e.key === 'H') toggleHud(); }}
  role="region"
  tabindex="-1"
  aria-label="Manga Reader"
>
  <!-- TOP HUD BAR -->
  <header class="hud-top" class:visible={showHud} role="toolbar" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
    <div class="hud-left">
      <button class="hud-btn" on:click={goBack} title="Back">
        <ArrowLeft size={18} strokeWidth={2} />
      </button>
      <div class="hud-titles">
        <span class="manga-name">{manga?.title || 'Manga'}</span>
        <span class="chapter-name">{activeChapter?.title || 'Chapter'}</span>
      </div>
    </div>

    <div class="hud-right">
      <div class="zoom-pill-group">
        <button
          class="zoom-mini-btn"
          on:click={() => setZoom(zoomPercent - 10)}
          disabled={zoomPercent <= 50}
          title="Zoom Out (-)"
        >
          <ZoomOut size={15} />
        </button>
        <button
          class="zoom-reset-btn"
          on:click={() => setZoom(100)}
          title="Reset Zoom to 100% (0)"
        >
          {zoomPercent}%
        </button>
        <button
          class="zoom-mini-btn"
          on:click={() => setZoom(zoomPercent + 10)}
          disabled={zoomPercent >= 250}
          title="Zoom In (+)"
        >
          <ZoomIn size={15} />
        </button>
      </div>

      <span class="page-counter">
        {#if readerMode === 'double' && currentPageInChapter > 0 && currentPageInChapter < currentChapterTotalPages - 1}
          {currentPageInChapter}–{currentPageInChapter + 1} / {currentChapterTotalPages}
        {:else}
          {currentPageInChapter + 1} / {currentChapterTotalPages}
        {/if}
      </span>

      <button
        class="hud-btn"
        on:click={toggleFullscreen}
        title={isFullscreen ? "Exit Fullscreen (F)" : "Fullscreen (F)"}
      >
        {#if isFullscreen}
          <Minimize2 size={17} />
        {:else}
          <Maximize2 size={17} />
        {/if}
      </button>
      <button
        class="hud-btn"
        on:click={handleSolveCloudflare}
        title="Pass Cloudflare Challenge"
      >
        <ShieldAlert size={17} />
      </button>
      <button
        class="hud-btn"
        class:active={showSettings}
        on:click={() => (showSettings = !showSettings)}
        title="Reader Settings (Tab)"
      >
        <Settings size={18} />
      </button>
    </div>
  </header>

  <!-- READER CANVAS -->
  <main
    class="reader-canvas"
    class:paged-canvas={readerMode !== 'webtoon'}
    style="--zoom: {zoomPercent / 100};"
    bind:this={scrollContainer}
    on:scroll={handleScroll}
    on:wheel|nonpassive={handleWheel}
  >
    {#if loading}
      <div class="loading-state">
        <div class="spinner"></div>
        <p>Loading chapter pages...</p>
      </div>
    {:else if loadError || flatPages.length === 0}
      <div class="reader-error-state" role="alert" on:click|stopPropagation on:keydown|stopPropagation>
        <ShieldAlert size={40} class="cf-alert-icon" />
        <h2>Unable to load chapter images</h2>
        <p class="error-msg">{loadError || 'Empty page list returned.'}</p>
        <p class="error-sub">This source may require passing a Cloudflare challenge or captcha check.</p>
        <div class="error-btn-row">
          <button class="cf-solve-btn" on:click={handleSolveCloudflare}>
            <ShieldAlert size={16} />
            Pass Cloudflare
          </button>
          <button class="cf-retry-btn" on:click={loadInitialChapter}>
            <RotateCcw size={15} />
            Retry
          </button>
          <button class="cf-back-btn" on:click={goBack}>
            <ArrowLeft size={15} />
            Back
          </button>
        </div>
      </div>

    {:else if readerMode === 'webtoon'}
      <!-- Endless Vertical Webtoon Strip Across All Chapters -->
      <div
        class="webtoon-strip"
        style="width: {fitToWidth ? 'calc(min(820px, 100vw) * var(--zoom, 1))' : 'fit-content'}; max-width: calc(100vw * var(--zoom, 1)); row-gap: {pageGap}px; filter: contrast({contrast}%) brightness({brightness}%);"
      >
        {#each flatPages as item (item.key)}
          {#if item.pageIdx === 0 && item.chapterBlockIdx > 0}
            <div class="chapter-transition-divider">
              <span class="divider-line"></span>
              <span class="divider-pill">{item.chapter.title || `Chapter ${item.chapter.number ?? ''}`}</span>
              <span class="divider-line"></span>
            </div>
          {/if}

          <div
            class="reader-page-slot"
            class:is-loaded={pageStates[item.key] === 'loaded'}
            data-global-index={item.globalIdx}
            data-chapter-index={item.chapterBlockIdx}
            data-page-index={item.pageIdx}
            use:captureHeight={item.key}
            style={!isChapterInWindow(item.chapterBlockIdx) ? `min-height: ${measuredHeights.get(item.key) || 520}px;` : undefined}
          >
            {#if !isChapterInWindow(item.chapterBlockIdx)}
              <div class="page-placeholder" style="min-height: {measuredHeights.get(item.key) || 520}px;">
                <span>Page {item.pageIdx + 1}</span>
              </div>
            {:else if pageStates[item.key] === 'loading' || pageStates[item.key] === 'loaded' || pageStates[item.key] === 'slow'}
              <img
                src={pageMediaSrc(item)}
                alt="Page {item.pageIdx + 1}"
                decoding="async"
                class="reader-page-img"
                use:trackCachedLoad={item.key}
                on:load={() => markPageLoaded(item.key)}
                on:error={() => markPageError(item.key)}
              />
              {#if pageStates[item.key] !== 'loaded'}
                <div class="page-loading-overlay">
                  <div class="mini-spinner"></div>
                  <span>Loading Page {item.pageIdx + 1}...</span>
                </div>
              {/if}
            {:else if pageStates[item.key] === 'error'}
              <button
                type="button"
                class="page-error-box"
                on:click|stopPropagation={() => retryPage(item.key)}
              >
                <span>Page {item.pageIdx + 1} failed to load</span>
                <span class="page-retry-btn">
                  <RotateCcw size={13} />
                  Retry
                </span>
              </button>
            {:else}
              <div class="page-placeholder">
                <span>Page {item.pageIdx + 1}</span>
              </div>
            {/if}
          </div>
        {/each}

        <!-- Endless Chapter Continuation Footer -->
        {#if loadingNextChapter}
          <div class="next-chapter-loading">
            <div class="mini-spinner"></div>
            <span>Loading {nextChapterObj?.title || 'next chapter'}...</span>
          </div>
        {:else if nextChapterError}
          <div class="chapter-end-card" role="region" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
            <span class="end-badge error">CHAPTER LOAD ERROR</span>
            <p class="end-title">{nextChapterObj?.title || 'Next Chapter'}</p>
            <p class="next-err-text">{nextChapterError}</p>
            <div class="end-actions">
              <button class="end-btn" on:click={() => { nextChapterError = null; loadNextChapter(); }}>
                <RotateCcw size={14} /> Retry Next Chapter
              </button>
              <button class="end-btn secondary" on:click={handleSolveCloudflare}>
                <ShieldAlert size={14} /> Pass Cloudflare
              </button>
            </div>
          </div>
        {:else if !hasNextChapter}
          <div class="chapter-end-card" role="region" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
            <span class="end-badge">LATEST CHAPTER REACHED</span>
            <p class="end-title">{activeChapter?.title || 'All available chapters completed'}</p>
            <div class="end-actions">
              <button class="end-btn" on:click={goBack}>Back to Details</button>
            </div>
          </div>
        {/if}
      </div>

    {:else if readerMode === 'double'}
      <!-- 2 Pages Double Spread Mode -->
      <div
        class="double-page-container"
        style="filter: contrast({contrast}%) brightness({brightness}%);"
      >
        <div class="double-page-row" class:single-cover={isCoverSpread}>
          {#each spreadEntries as entry (entry.key)}
            <img
              src={pageMediaSrc(entry)}
              alt="Page {entry.pageIdx + 1}"
              class="reader-spread-img"
              use:trackCachedLoad={entry.key}
              on:load={() => markPageLoaded(entry.key)}
              on:error={() => markPageError(entry.key)}
            />
          {/each}
        </div>
      </div>

    {:else}
      <!-- Single Page Mode -->
      <div
        class="single-page-wrapper"
        style="filter: contrast({contrast}%) brightness({brightness}%);"
      >
        {#if activeFlatPage}
          {#key activeFlatPage.key}
            <img
              src={pageMediaSrc(activeFlatPage)}
              alt="Page {activeFlatPage.pageIdx + 1}"
              class="reader-single-img"
              use:trackCachedLoad={activeFlatPage.key}
              on:load={() => markPageLoaded(activeFlatPage.key)}
              on:error={() => markPageError(activeFlatPage.key)}
            />
          {/key}
        {/if}
      </div>
    {/if}
  </main>

  <!-- READER SETTINGS DRAWER (TAB KEY) -->
  {#if showSettings}
    <div class="settings-drawer" on:click|stopPropagation on:keydown|stopPropagation role="dialog" tabindex="-1" aria-label="Reader Settings">
      <div class="drawer-header">
        <div class="drawer-title-row">
          <Sliders size={16} class="drawer-icon" />
          <span class="drawer-title">Reader Settings</span>
          <span class="tab-pill">TAB</span>
        </div>
        <button class="drawer-close-btn" on:click={() => (showSettings = false)}>✕</button>
      </div>

      <!-- VIEW MODE SELECTOR -->
      <div class="drawer-group">
        <span class="drawer-group-label">Reading Mode</span>
        <div class="segmented-control">
          <button
            class="seg-item"
            class:active={readerMode === 'webtoon'}
            on:click={() => changeReaderMode('webtoon')}
          >
            <AlignJustify size={14} />
            <span>Webtoon</span>
          </button>
          <button
            class="seg-item"
            class:active={readerMode === 'single'}
            on:click={() => changeReaderMode('single')}
          >
            <BookOpen size={14} />
            <span>Single</span>
          </button>
          <button
            class="seg-item"
            class:active={readerMode === 'double'}
            on:click={() => changeReaderMode('double')}
          >
            <Columns size={14} />
            <span>2 Pages</span>
          </button>
        </div>
      </div>

      <!-- READING DIRECTION (FOR DOUBLE OR SINGLE) -->
      {#if readerMode !== 'webtoon'}
        <div class="drawer-group">
          <span class="drawer-group-label">Reading Direction</span>
          <div class="segmented-control">
            <button
              class="seg-item"
              class:active={readingDirection === 'RTL'}
              on:click={() => changeDirection('RTL')}
            >
              <span>RTL (Manga)</span>
            </button>
            <button
              class="seg-item"
              class:active={readingDirection === 'LTR'}
              on:click={() => changeDirection('LTR')}
            >
              <span>LTR (Western)</span>
            </button>
          </div>
        </div>
      {/if}

      <!-- WEBTOON SEAM GAP -->
      {#if readerMode === 'webtoon'}
        <div class="drawer-group">
          <span class="drawer-group-label">Page Seam Gap</span>
          <div class="segmented-control">
            <button class="seg-item" class:active={pageGap === 0} on:click={() => (pageGap = 0)}>
              <span>0px (Seamless)</span>
            </button>
            <button class="seg-item" class:active={pageGap === 4} on:click={() => (pageGap = 4)}>
              <span>4px</span>
            </button>
            <button class="seg-item" class:active={pageGap === 8} on:click={() => (pageGap = 8)}>
              <span>8px</span>
            </button>
          </div>
        </div>
      {/if}

      <!-- ZOOM SLIDER -->
      <div class="drawer-group">
        <div class="slider-label-row">
          <span class="drawer-group-label">Zoom Scale</span>
          <div class="zoom-drawer-right">
            <span class="slider-val-badge">{zoomPercent}%</span>
            {#if zoomPercent !== 100}
              <button class="mini-reset-link" on:click={() => setZoom(100)}>Reset</button>
            {/if}
          </div>
        </div>
        <div
          class="custom-track"
          on:mousedown={(e) => startDrag('zoom', 50, 250, 5, e)}
        >
          <div class="track-fill" style="width: {((zoomPercent - 50) / 200) * 100}%"></div>
          <div class="track-thumb" style="left: {((zoomPercent - 50) / 200) * 100}%"></div>
        </div>
      </div>

      <!-- BRIGHTNESS SLIDER -->
      <div class="drawer-group">
        <div class="slider-label-row">
          <span class="drawer-group-label">Brightness</span>
          <span class="slider-val-badge">{brightness}%</span>
        </div>
        <div
          class="custom-track"
          on:mousedown={(e) => startDrag('brightness', 50, 150, 5, e)}
        >
          <div class="track-fill" style="width: {((brightness - 50) / 100) * 100}%"></div>
          <div class="track-thumb" style="left: {((brightness - 50) / 100) * 100}%"></div>
        </div>
      </div>

      <!-- CONTRAST SLIDER -->
      <div class="drawer-group">
        <div class="slider-label-row">
          <span class="drawer-group-label">Contrast</span>
          <span class="slider-val-badge">{contrast}%</span>
        </div>
        <div
          class="custom-track"
          on:mousedown={(e) => startDrag('contrast', 50, 150, 5, e)}
        >
          <div class="track-fill" style="width: {((contrast - 50) / 100) * 100}%"></div>
          <div class="track-thumb" style="left: {((contrast - 50) / 100) * 100}%"></div>
        </div>
      </div>

      <!-- INVERT TOGGLE -->
      <div class="toggle-row">
        <span class="drawer-group-label">Invert Colors (Night Reading)</span>
        <button
          class="switch-btn"
          class:checked={invertColors}
          on:click={() => (invertColors = !invertColors)}
        >
          <div class="switch-ball"></div>
        </button>
      </div>

      <!-- SHORTCUTS CHEAT SHEET -->
      <div class="shortcut-legend">
        <div class="sc-item"><kbd>←</kbd> <kbd>→</kbd> Flip / Scroll</div>
        <div class="sc-item"><kbd>+</kbd> <kbd>-</kbd> <kbd>0</kbd> Zoom</div>
        <div class="sc-item"><kbd>Tab</kbd> Settings</div>
        <div class="sc-item"><kbd>F</kbd> Fullscreen</div>
        <div class="sc-item"><kbd>Esc</kbd> Back</div>
      </div>
    </div>
  {/if}

  <!-- BOTTOM HUD BAR -->
  <footer class="hud-bottom" class:visible={showHud} on:click|stopPropagation>
    <div class="hud-bottom-content">
      <button
        class="slider-step-btn"
        disabled={currentGlobalIdx <= 0}
        on:click={prevStep}
        title="Previous Page (←)"
      >
        <ChevronLeft size={18} />
      </button>

      <div class="bottom-page-indicator">
        <span class="bottom-cur">{currentPageInChapter + 1}</span>
        <span class="bottom-sep">/</span>
        <span class="bottom-total">{currentChapterTotalPages}</span>
      </div>

      <button
        class="slider-step-btn"
        disabled={currentGlobalIdx >= flatPages.length - 1 && !hasNextChapter}
        on:click={nextStep}
        title="Next Page (→)"
      >
        <ChevronRight size={18} />
      </button>
    </div>
  </footer>
</div>

<style>
  .reader-container {
    position: fixed;
    inset: 0;
    background: var(--z-reader-bg, #1c1a24);
    overflow: hidden;
    z-index: 100;
    user-select: none;
  }

  .reader-container.invert {
    filter: invert(1) hue-rotate(180deg);
  }

  .hud-top, .hud-bottom {
    position: fixed;
    left: 0;
    right: 0;
    background: var(--z-surface-overlay);
    backdrop-filter: blur(16px);
    padding: var(--z-space-3) var(--z-space-6);
    color: var(--z-text);
    z-index: 110;
    transition: transform var(--z-duration-normal) var(--z-ease-out),
                opacity var(--z-duration-normal) var(--z-ease-out);
    pointer-events: none;
  }

  .hud-top {
    top: 0;
    transform: translateY(-100%);
    opacity: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--z-border);
  }

  .hud-top.visible {
    transform: translateY(0);
    opacity: 1;
    pointer-events: auto;
  }

  .hud-bottom {
    bottom: 0;
    transform: translateY(100%);
    opacity: 0;
    border-top: 1px solid var(--z-border);
  }

  .hud-bottom.visible {
    transform: translateY(0);
    opacity: 1;
    pointer-events: auto;
  }

  .hud-left {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    min-width: 0;
  }

  .hud-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    background: transparent;
    border: 1px solid transparent;
    transition: all var(--z-duration-fast) var(--z-ease-out);
    cursor: pointer;
    flex-shrink: 0;
  }

  .hud-btn:hover {
    background: var(--z-surface-raised);
    border-color: var(--z-border);
  }

  .hud-btn.active {
    background: var(--z-accent);
    color: #ffffff;
  }

  .hud-titles {
    display: flex;
    flex-direction: column;
    max-width: 38vw;
    min-width: 0;
  }

  .manga-name {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chapter-name {
    font-size: var(--z-text-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hud-right {
    display: flex;
    align-items: center;
    gap: var(--z-space-2);
    flex-shrink: 0;
  }

  .zoom-pill-group {
    display: flex;
    align-items: center;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-full);
    padding: 2px;
    gap: 2px;
  }

  .zoom-mini-btn {
    width: 26px;
    height: 26px;
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s ease;
  }

  .zoom-mini-btn:hover:not(:disabled) {
    background: var(--z-border-hover);
  }

  .zoom-mini-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .zoom-reset-btn {
    padding: 2px 7px;
    font-size: 11px;
    font-weight: 600;
    color: var(--z-text);
    border-radius: var(--z-radius-full);
    min-width: 44px;
  }

  .zoom-reset-btn:hover {
    background: var(--z-border-hover);
  }

  .page-counter {
    font-size: var(--z-text-xs);
    font-weight: 600;
    color: var(--z-text-muted);
    background: var(--z-surface-raised);
    padding: 4px 10px;
    border-radius: var(--z-radius-full);
  }

  .hud-bottom-content {
    max-width: 420px;
    margin: 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--z-space-4);
  }

  .bottom-page-indicator {
    font-size: var(--z-text-sm);
    font-weight: 600;
    color: var(--z-text);
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .bottom-sep {
    color: var(--z-text-muted);
    opacity: 0.6;
  }

  .slider-step-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--z-radius-full);
    color: var(--z-text);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    transition: all var(--z-duration-fast) var(--z-ease-out);
    cursor: pointer;
  }

  .slider-step-btn:hover:not(:disabled) {
    background: var(--z-border-hover);
    border-color: var(--z-accent);
  }

  .slider-step-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .reader-canvas {
    width: 100vw;
    height: 100vh;
    overflow: auto;
    display: block;
    background: var(--z-reader-bg, #1c1a24);
  }

  .reader-canvas.paged-canvas {
    display: flex;
  }

  /* WEBTOON STRIP */
  .webtoon-strip {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin: 0 auto;
    padding-bottom: 90px;
    min-width: 280px;
  }

  .chapter-transition-divider {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 22px 16px;
    color: var(--z-text-muted);
  }

  .divider-line {
    flex: 1;
    height: 1px;
    background: var(--z-border);
  }

  .divider-pill {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.4px;
    color: var(--z-accent);
    background: rgba(66, 133, 244, 0.12);
    border: 1px solid rgba(66, 133, 244, 0.28);
    padding: 4px 14px;
    border-radius: 9999px;
    white-space: nowrap;
  }

  .reader-page-slot {
    width: 100%;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .reader-page-slot:not(.is-loaded) {
    min-height: 65vh;
    background: rgba(255, 255, 255, 0.02);
  }

  .reader-page-img {
    width: 100%;
    height: auto;
    display: block;
    user-select: none;
  }

  .page-placeholder,
  .page-loading-overlay,
  .page-error-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--z-text-muted);
    font-size: var(--z-text-xs);
    padding: 24px;
  }

  .page-loading-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .page-retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: var(--z-radius-full);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    font-size: var(--z-text-xs);
    font-weight: 600;
  }

  .page-retry-btn:hover {
    border-color: var(--z-accent);
  }

  .next-chapter-loading {
    margin-top: 28px;
    padding: 18px 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--z-text-muted);
    font-size: var(--z-text-xs);
  }

  .mini-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--z-border);
    border-top-color: var(--z-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .chapter-end-card {
    margin-top: 40px;
    padding: 28px 24px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    max-width: 420px;
    width: 90%;
  }

  .end-badge {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.8px;
    color: var(--z-accent);
    background: rgba(66, 133, 244, 0.12);
    padding: 3px 10px;
    border-radius: 9999px;
  }

  .end-badge.error {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.12);
  }

  .end-title {
    font-size: var(--z-text-sm);
    color: var(--z-text);
    font-weight: 600;
  }

  .next-err-text {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
  }

  .end-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .end-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 18px;
    background: var(--z-accent);
    color: #ffffff;
    font-size: var(--z-text-xs);
    font-weight: 600;
    border-radius: var(--z-radius-full);
    border: none;
    cursor: pointer;
    transition: opacity 0.2s;
  }

  .end-btn.secondary {
    background: var(--z-surface);
    color: var(--z-text);
    border: 1px solid var(--z-border);
  }

  .end-btn:hover {
    opacity: 0.9;
  }

  /* 2 PAGES DOUBLE SPREAD */
  .double-page-container {
    margin: auto;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    padding: 0;
  }

  .double-page-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    box-sizing: border-box;
  }

  .double-page-row.single-cover {
    justify-content: center;
  }

  .reader-spread-img {
    height: calc(100vh * var(--zoom, 1));
    max-height: calc(100vh * var(--zoom, 1));
    max-width: calc(50vw * var(--zoom, 1) - 4px);
    width: auto;
    object-fit: contain;
    user-select: none;
    display: block;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  .double-page-row.single-cover .reader-spread-img {
    max-width: calc(100vw * var(--zoom, 1));
  }

  /* SINGLE PAGE */
  .single-page-wrapper {
    margin: auto;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    padding: 0;
  }

  .reader-single-img {
    height: calc(100vh * var(--zoom, 1));
    max-height: calc(100vh * var(--zoom, 1));
    width: auto;
    max-width: calc(100vw * var(--zoom, 1));
    object-fit: contain;
    user-select: none;
    display: block;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  /* SETTINGS DRAWER */
  .settings-drawer {
    position: fixed;
    top: 64px;
    right: 20px;
    width: 320px;
    background: var(--z-surface-overlay);
    backdrop-filter: blur(20px);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    padding: 16px 18px;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
    z-index: 120;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .drawer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--z-border);
    padding-bottom: 10px;
  }

  .drawer-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.drawer-icon) {
    color: var(--z-accent);
  }

  .drawer-title {
    font-size: var(--z-text-sm);
    font-weight: 700;
    color: var(--z-text);
  }

  .tab-pill {
    font-size: 9px;
    font-weight: 700;
    background: var(--z-surface-raised);
    color: var(--z-text-muted);
    padding: 2px 6px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }

  .drawer-close-btn {
    font-size: 13px;
    color: var(--z-text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
  }

  .drawer-close-btn:hover {
    color: var(--z-text);
  }

  .drawer-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .drawer-group-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--z-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  .segmented-control {
    display: flex;
    background: var(--z-surface-raised);
    padding: 2px;
    border-radius: var(--z-radius-md);
    border: 1px solid var(--z-border);
    gap: 2px;
  }

  .seg-item {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 6px 8px;
    border-radius: var(--z-radius-sm);
    font-size: var(--z-text-xs);
    font-weight: 500;
    color: var(--z-text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .seg-item:hover {
    color: var(--z-text);
  }

  .seg-item.active {
    background: var(--z-accent);
    color: #ffffff;
    font-weight: 600;
  }

  .slider-label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .zoom-drawer-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .mini-reset-link {
    font-size: 10px;
    font-weight: 600;
    color: var(--z-accent);
    padding: 1px 5px;
    border-radius: 4px;
  }

  .mini-reset-link:hover {
    background: rgba(66, 133, 244, 0.15);
  }

  .slider-val-badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--z-text);
    background: var(--z-surface-raised);
    padding: 1px 7px;
    border-radius: 9999px;
  }

  .custom-track {
    position: relative;
    height: 6px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 9999px;
    cursor: pointer;
    margin: 6px 0;
  }

  .track-fill {
    position: absolute;
    height: 100%;
    background: var(--z-accent);
    border-radius: 9999px;
  }

  .track-thumb {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 14px;
    height: 14px;
    background: #ffffff;
    border: 2px solid var(--z-accent);
    border-radius: 50%;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    cursor: grab;
  }

  .toggle-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 4px;
  }

  .switch-btn {
    width: 36px;
    height: 20px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: 9999px;
    position: relative;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .switch-btn.checked {
    background: var(--z-accent);
    border-color: var(--z-accent);
  }

  .switch-ball {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    background: #ffffff;
    border-radius: 50%;
    transition: transform 0.2s ease;
  }

  .switch-btn.checked .switch-ball {
    transform: translateX(16px);
  }

  .shortcut-legend {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    border-top: 1px solid var(--z-border);
    padding-top: 10px;
  }

  .sc-item {
    font-size: 11px;
    color: var(--z-text-muted);
  }

  kbd {
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: 4px;
    padding: 1px 4px;
    font-size: 10px;
    font-family: inherit;
    color: var(--z-text);
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100%;
    gap: var(--z-space-3);
    color: var(--z-text-muted);
  }

  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid var(--z-border);
    border-top-color: var(--z-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .reader-error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 80vh;
    padding: var(--z-space-8);
    text-align: center;
    max-width: 480px;
    margin: 0 auto;
  }

  :global(.cf-alert-icon) {
    color: #a78bfa;
    margin-bottom: var(--z-space-4);
  }

  .reader-error-state h2 {
    font-size: var(--z-text-xl);
    font-weight: 700;
    color: var(--z-text);
    margin-bottom: var(--z-space-2);
  }

  .error-msg {
    font-size: var(--z-text-sm);
    color: #ef4444;
    margin-bottom: var(--z-space-2);
  }

  .error-sub {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    line-height: 1.5;
    margin-bottom: var(--z-space-6);
  }

  .error-btn-row {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
  }

  .cf-solve-btn, .cf-retry-btn, .cf-back-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border-radius: var(--z-radius-full);
    font-size: var(--z-text-xs);
    font-weight: 600;
    border: 1px solid var(--z-border);
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .cf-solve-btn {
    background: #4f46e5;
    color: #ffffff;
    border-color: #4f46e5;
  }

  .cf-solve-btn:hover {
    background: #4338ca;
  }

  .cf-retry-btn {
    background: var(--z-surface-raised);
    color: var(--z-text);
  }

  .cf-retry-btn:hover {
    background: var(--z-border-hover);
  }

  .cf-back-btn {
    background: transparent;
    color: var(--z-text-muted);
  }

  .cf-back-btn:hover {
    color: var(--z-text);
  }
</style>
