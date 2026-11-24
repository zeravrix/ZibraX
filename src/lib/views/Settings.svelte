<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { appState, updateSettings, loadSettings, notify, navigate } from '../stores/app.svelte.js';
  import { 
    Palette, BookOpen, Zap, ShieldCheck, Check, Trash2, Settings as SettingsIcon, Info, Server, Download, Upload
  } from 'lucide-svelte';

  let currentSettings = $state(null);
  let backupInputEl = $state(null);

  const palettes = [
    { id: 'Totoro', name: 'ZibraX Blue', color: '#4285f4' },
    { id: 'Monet', name: 'Sky Azure', color: '#38bdf8' },
    { id: 'Miku', name: 'Teal', color: '#06b6d4' },
    { id: 'Mion', name: 'Emerald', color: '#34d399' },
    { id: 'Itsuka', name: 'Sunset Orange', color: '#f97316' },
    { id: 'Mamimi', name: 'Periwinkle', color: '#afc6ff' },
    { id: 'Asuka', name: 'Crimson', color: '#ef4444' },
    { id: 'Sakura', name: 'Rose', color: '#fb7185' },
    { id: 'Expressive', name: 'Lilac', color: '#c084fc' },
    { id: 'Rikka', name: 'Lavender', color: '#a78bfa' },
    { id: 'Kanade', name: 'Slate', color: '#94a3b8' },
    { id: 'HighContrast', name: 'Monochrome', color: '#ffffff' },
  ];

  const backgroundModes = [
    {
      id: 'dark',
      label: 'Obsidian Dark',
      desc: 'Default slate purple (#23212c) for dark reading',
      isAmoled: false,
      theme: 'Dark',
      colorPreview: '#23212c',
    },
    {
      id: 'amoled',
      label: 'Midnight OLED',
      desc: 'Pure #000000 pitch black for OLED screens',
      isAmoled: true,
      theme: 'Dark',
      colorPreview: '#000000',
    },
    {
      id: 'light',
      label: 'Porcelain Light',
      desc: 'Crisp high-contrast daytime brightness',
      isAmoled: false,
      theme: 'Light',
      colorPreview: '#f8fafc',
    },
    {
      id: 'auto',
      label: 'System Auto',
      desc: 'Follow OS light or dark appearance automatically',
      isAmoled: false,
      theme: 'Auto',
      colorPreview: 'linear-gradient(135deg, #23212c 50%, #f8fafc 50%)',
    },
  ];

  const animationLevels = [
    { id: 'Smooth', label: 'Smooth Motion' },
    { id: 'Reduced', label: 'Reduced' },
    { id: 'Off', label: 'No Motion' },
  ];

  const readerModes = [
    { id: 'Vertical', label: 'Webtoon', desc: 'Continuous vertical strip' },
    { id: 'Single', label: 'Single Page', desc: 'Horizontal flip' },
    { id: 'Double', label: 'Double Spread', desc: 'Side-by-side spread' },
    { id: 'Auto', label: 'Auto Detect', desc: 'Detect comic format' },
  ];

  const readingDirections = [
    { id: 'LTR', label: 'Left to Right (LTR)', desc: 'Standard manhwa / webtoon' },
    { id: 'RTL', label: 'Right to Left (RTL)', desc: 'Traditional Japanese manga' },
  ];

  let selectedPaletteIndex = $state(0);

  onMount(async () => {
    if (!appState.settings) {
      await loadSettings();
    }
    if (appState.settings) {
      currentSettings = JSON.parse(JSON.stringify(appState.settings));
      syncPaletteIndex();
    }
  });

  function syncPaletteIndex() {
    if (!currentSettings) return;
    const current = String(currentSettings.color_theme || 'Totoro').toLowerCase();
    const idx = palettes.findIndex(p => p.id.toLowerCase() === current);
    if (idx !== -1) selectedPaletteIndex = idx;
  }

  async function selectPalette(p, idx) {
    if (!currentSettings) return;
    selectedPaletteIndex = idx;
    currentSettings.color_theme = p.id;
    await save();
  }

  async function selectBackgroundMode(mode) {
    if (!currentSettings) return;
    currentSettings.is_amoled = mode.isAmoled;
    currentSettings.theme = mode.theme;
    await save();
  }

  async function selectReaderMode(modeId) {
    if (!currentSettings) return;
    currentSettings.reader_mode = modeId;
    await save();
  }

  async function selectReadingDirection(dirId) {
    if (!currentSettings) return;
    currentSettings.reading_direction = dirId;
    await save();
  }

  async function toggleSetting(field) {
    if (!currentSettings) return;
    currentSettings[field] = !currentSettings[field];
    await save();
  }

  async function handleRangeChange(field, val) {
    if (!currentSettings) return;
    currentSettings[field] = Number(val);
    await save();
  }

  async function selectAnimationLevel(lvl) {
    if (!currentSettings) return;
    currentSettings.animation_level = lvl;
    await save();
  }

  async function save() {
    await updateSettings(currentSettings);
  }

  async function handleClearCache() {
    try {
      await invoke('clear_caches');
      notify('Image RAM and disk caches cleared');
    } catch (e) {
      notify('Failed to clear caches: ' + e);
    }
  }

  async function handleExportBackup() {
    try {
      const res = await invoke('export_backup');
      const raw = atob(res.base64);
      const arr = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
      const blob = new Blob([arr], { type: 'application/zip' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = res.file_name || 'zibra-backup.zip';
      a.click();
      URL.revokeObjectURL(url);
      notify(`Backup saved to ${res.path}`);
    } catch (e) {
      notify('Backup export failed: ' + e);
    }
  }

  async function handleImportBackup(e) {
    const file = e.target?.files?.[0];
    if (!file) return;
    try {
      const buf = await file.arrayBuffer();
      const restored = await invoke('import_backup_bytes', { bytes: Array.from(new Uint8Array(buf)) });
      const fresh = await invoke('get_settings');
      appState.settings = fresh;
      currentSettings = JSON.parse(JSON.stringify(fresh));
      syncPaletteIndex();
      notify(`Restored ${restored} backup files. Restart app if library changes don't appear immediately.`);
    } catch (err) {
      notify('Backup restore failed: ' + err);
    } finally {
      e.target.value = '';
    }
  }

  function handleTrackClick(e, field, min, max, step) {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    let val = min + ratio * (max - min);
    if (step) val = Math.round(val / step) * step;
    handleRangeChange(field, val);
  }

  function startSliderDrag(field, min, max, step, e) {
    e.preventDefault();
    e.stopPropagation();
    const onMove = (ev) => {
      const track = document.getElementById(`track-${field}`);
      if (!track) return;
      const rect = track.getBoundingClientRect();
      const ratio = Math.max(0, Math.min(1, (ev.clientX - rect.left) / rect.width));
      let val = min + ratio * (max - min);
      if (step) val = Math.round(val / step) * step;
      handleRangeChange(field, val);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  let activePalette = $derived(palettes[selectedPaletteIndex] || palettes[0]);
</script>

<div class="settings-view">
  <header class="settings-header">
    <h1 class="page-title">
      <SettingsIcon size={26} class="title-icon" />
      Settings
    </h1>
    <p class="page-desc">Theme styling, reader layout, and system performance preferences.</p>
  </header>

  {#if currentSettings}
    <div class="settings-sections">
      <!-- 1. APPEARANCE -->
      <section class="settings-card">
        <div class="card-header">
          <div class="icon-wrap"><Palette size={18} /></div>
          <div>
            <h2 class="card-title">Appearance</h2>
            <p class="card-subtitle">Choose your color accent and background atmosphere.</p>
          </div>
        </div>

        <!-- ACCENT PALETTE GRID -->
        <div class="field-block">
          <label class="field-label">Accent Palette</label>
          <div class="palette-grid">
            {#each palettes as p, i}
              <button
                class="palette-btn"
                class:active={selectedPaletteIndex === i}
                on:click={() => selectPalette(p, i)}
              >
                <span class="color-dot" style="background: {p.color}"></span>
                <span class="color-label">{p.name}</span>
                {#if selectedPaletteIndex === i}
                  <Check size={14} class="check-icon" />
                {/if}
              </button>
            {/each}
          </div>
        </div>

        <!-- DISPLAY THEME -->
        <div class="field-block">
          <span class="field-label">Display Theme</span>
          <div class="theme-mode-grid">
            {#each backgroundModes as mode}
              {@const isSelected = (mode.isAmoled && currentSettings.is_amoled) || (!currentSettings.is_amoled && currentSettings.theme === mode.theme)}
              <button
                type="button"
                class="theme-card"
                class:selected={isSelected}
                on:click={() => selectBackgroundMode(mode)}
              >
                <div class="theme-swatch" style="background: {mode.colorPreview}"></div>
                <div class="theme-info">
                  <div class="theme-row">
                    <span class="theme-name">{mode.label}</span>
                    {#if isSelected}
                      <span class="active-badge">Active</span>
                    {/if}
                  </div>
                  <span class="theme-desc">{mode.desc}</span>
                </div>
              </button>
            {/each}
          </div>
        </div>

        <!-- ANIMATION MOTION -->
        <div class="field-block">
          <span class="field-label">Motion & Animations</span>
          <div class="segmented-bar">
            {#each animationLevels as lvl}
              <button
                type="button"
                class="seg-btn"
                class:active={(currentSettings.animation_level || 'Smooth') === lvl.id || (lvl.id === 'Smooth' && currentSettings.animation_level === 'Full')}
                on:click={() => selectAnimationLevel(lvl.id)}
              >
                <span>{lvl.label}</span>
              </button>
            {/each}
          </div>
        </div>
      </section>

      <!-- 2. READER ENGINE -->
      <section class="settings-card">
        <div class="card-header">
          <div class="icon-wrap"><BookOpen size={18} /></div>
          <div>
            <h2 class="card-title">Reader Engine & Layout</h2>
            <p class="card-subtitle">Reading format, page gaps, and scaling options.</p>
          </div>
        </div>

        <!-- READING MODE -->
        <div class="field-block">
          <label class="field-label">Default Reading Mode</label>
          <div class="segmented-bar">
            {#each readerModes as mode}
              <button
                class="seg-btn"
                class:active={currentSettings.reader_mode === mode.id}
                on:click={() => selectReaderMode(mode.id)}
              >
                <span>{mode.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <!-- READING DIRECTION -->
        <div class="field-block">
          <label class="field-label">Reading Direction</label>
          <div class="segmented-bar">
            {#each readingDirections as dir}
              <button
                class="seg-btn"
                class:active={currentSettings.reading_direction === dir.id}
                on:click={() => selectReadingDirection(dir.id)}
              >
                <span>{dir.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <!-- PAGE SEAM GAP SLIDER -->
        <div class="slider-field">
          <div class="slider-meta">
            <div>
              <span class="slider-title">Page Seam Gap</span>
              <span class="slider-desc">Distance between continuous webtoon slices</span>
            </div>
            <span class="num-pill">{currentSettings.seam_trim_px} px</span>
          </div>
          <div
            id="track-seam_trim_px"
            class="slider-bar"
            on:click={(e) => handleTrackClick(e, 'seam_trim_px', 0, 32, 2)}
          >
            <div class="slider-fill" style="width: {(currentSettings.seam_trim_px / 32) * 100}%"></div>
            <div
              class="slider-handle"
              style="left: {(currentSettings.seam_trim_px / 32) * 100}%"
              on:mousedown={(e) => startSliderDrag('seam_trim_px', 0, 32, 2, e)}
            ></div>
          </div>
        </div>

        <!-- FIT TO WIDTH -->
        <div class="setting-toggle-row">
          <div>
            <span class="toggle-title">Fit to Width</span>
            <span class="toggle-desc">Automatically scale webtoon images to match window width</span>
          </div>
          <button
            class="switch-control"
            class:checked={currentSettings.fit_to_width}
            on:click={() => toggleSetting('fit_to_width')}
          >
            <div class="switch-dot"></div>
          </button>
        </div>

        <!-- AUTO-CROP MARGINS -->
        <div class="setting-toggle-row">
          <div>
            <span class="toggle-title">Auto-Crop Scan Margins</span>
            <span class="toggle-desc">Trim excessive whitespace around scanned manga pages</span>
          </div>
          <button
            class="switch-control"
            class:checked={currentSettings.auto_crop_margins}
            on:click={() => toggleSetting('auto_crop_margins')}
          >
            <div class="switch-dot"></div>
          </button>
        </div>
      </section>

      <!-- 3. PERFORMANCE & NETWORK -->
      <section class="settings-card">
        <div class="card-header">
          <div class="icon-wrap"><Zap size={18} /></div>
          <div>
            <h2 class="card-title">Performance & Network</h2>
            <p class="card-subtitle">Control memory allocation and concurrent scrapers.</p>
          </div>
        </div>

        <!-- RAM CACHE SLIDER -->
        <div class="slider-field">
          <div class="slider-meta">
            <div>
              <span class="slider-title">RAM Image Cache</span>
              <span class="slider-desc">Memory buffer for instant page rendering</span>
            </div>
            <span class="num-pill">{currentSettings.ram_cache_budget_mb} MB</span>
          </div>
          <div
            id="track-ram_cache_budget_mb"
            class="slider-bar"
            on:click={(e) => handleTrackClick(e, 'ram_cache_budget_mb', 128, 2048, 128)}
          >
            <div
              class="slider-fill"
              style="width: {((currentSettings.ram_cache_budget_mb - 128) / (2048 - 128)) * 100}%"
            ></div>
            <div
              class="slider-handle"
              style="left: {((currentSettings.ram_cache_budget_mb - 128) / (2048 - 128)) * 100}%"
              on:mousedown={(e) => startSliderDrag('ram_cache_budget_mb', 128, 2048, 128, e)}
            ></div>
          </div>
        </div>

        <!-- CONCURRENT DOWNLOADS SLIDER -->
        <div class="slider-field">
          <div class="slider-meta">
            <div>
              <span class="slider-title">Download Concurrency</span>
              <span class="slider-desc">Parallel network threads for chapters and covers</span>
            </div>
            <span class="num-pill">{currentSettings.max_concurrent_downloads} threads</span>
          </div>
          <div
            id="track-max_concurrent_downloads"
            class="slider-bar"
            on:click={(e) => handleTrackClick(e, 'max_concurrent_downloads', 1, 8, 1)}
          >
            <div
              class="slider-fill"
              style="width: {((currentSettings.max_concurrent_downloads - 1) / 7) * 100}%"
            ></div>
            <div
              class="slider-handle"
              style="left: {((currentSettings.max_concurrent_downloads - 1) / 7) * 100}%"
              on:mousedown={(e) => startSliderDrag('max_concurrent_downloads', 1, 8, 1, e)}
            ></div>
          </div>
        </div>

        <!-- CONTINUOUS CHAPTER FLOW -->
        <div class="setting-toggle-row">
          <div>
            <span class="toggle-title">Continuous Chapter Flow</span>
            <span class="toggle-desc">Automatically transition to next chapter at the end of a chapter</span>
          </div>
          <button
            class="switch-control"
            class:checked={currentSettings.infinite_chapter_scroll}
            on:click={() => toggleSetting('infinite_chapter_scroll')}
          >
            <div class="switch-dot"></div>
          </button>
        </div>
      </section>

      <!-- 4. CONTENT & PRIVACY -->
      <section class="settings-card">
        <div class="card-header">
          <div class="icon-wrap"><ShieldCheck size={18} /></div>
          <div>
            <h2 class="card-title">Content & Storage</h2>
            <p class="card-subtitle">Source filtering, privacy, and cache management.</p>
          </div>
        </div>

        <!-- NSFW TOGGLE -->
        <div class="setting-toggle-row">
          <div>
            <span class="toggle-title">Include 18+ Mature Content</span>
            <span class="toggle-desc">Show adult sources and titles in search</span>
          </div>
          <button
            class="switch-control"
            class:checked={currentSettings.nsfw_enabled}
            on:click={() => toggleSetting('nsfw_enabled')}
          >
            <div class="switch-dot"></div>
          </button>
        </div>

        <!-- DNS OVER HTTPS -->
        <div class="setting-toggle-row">
          <div>
            <span class="toggle-title">DNS-over-HTTPS (DoH)</span>
            <span class="toggle-desc">Bypass local DNS blocking and protect scraping requests</span>
          </div>
          <button
            class="switch-control"
            class:checked={currentSettings.dns_over_https}
            on:click={() => toggleSetting('dns_over_https')}
          >
            <div class="switch-dot"></div>
          </button>
        </div>

        <!-- CLEAR CACHE -->
        <div class="setting-action-row">
          <div>
            <span class="toggle-title">Storage & Cache</span>
            <span class="toggle-desc">Purge cached page buffers and cover thumbnails</span>
          </div>
          <button type="button" class="clear-btn" on:click={handleClearCache}>
            <Trash2 size={14} />
            <span>Clear Cache</span>
          </button>
        </div>

        <!-- BACKUP & RESTORE -->
        <div class="setting-action-row">
          <div>
            <span class="toggle-title">Library & Settings Backup</span>
            <span class="toggle-desc">Export or restore your library database and settings as a .zip archive</span>
          </div>
          <div class="row-actions">
            <button type="button" class="action-secondary-btn" on:click={handleExportBackup}>
              <Download size={14} />
              <span>Export</span>
            </button>
            <button type="button" class="action-secondary-btn" on:click={() => backupInputEl?.click()}>
              <Upload size={14} />
              <span>Import</span>
            </button>
            <input
              bind:this={backupInputEl}
              type="file"
              accept=".zip"
              class="hidden-file-input"
              on:change={handleImportBackup}
            />
          </div>
        </div>
      </section>

      <!-- 5. SELF-HOSTED SERVERS -->
      <section class="settings-card">
        <div class="card-header">
          <div class="icon-wrap"><Server size={18} /></div>
          <div>
            <h2 class="card-title">Self-Hosted Servers</h2>
            <p class="card-subtitle">Configure Komga or Kavita instances and optional API keys.</p>
          </div>
        </div>

        <div class="server-grid">
          <div class="field-block">
            <label for="komga-url" class="field-label">Komga Server URL</label>
            <input id="komga-url" type="url" class="text-input" bind:value={currentSettings.komga_server_url} placeholder="http://localhost:25600" />
          </div>
          <div class="field-block">
            <label for="komga-key" class="field-label">Komga API Key</label>
            <input id="komga-key" type="password" class="text-input" bind:value={currentSettings.komga_api_key} placeholder="Optional X-API-Key" />
          </div>
          <div class="field-block">
            <label for="kavita-url" class="field-label">Kavita Server URL</label>
            <input id="kavita-url" type="url" class="text-input" bind:value={currentSettings.kavita_server_url} placeholder="http://localhost:5000" />
          </div>
          <div class="field-block">
            <label for="kavita-key" class="field-label">Kavita API Key / Token</label>
            <input id="kavita-key" type="password" class="text-input" bind:value={currentSettings.kavita_api_key} placeholder="Optional API key" />
          </div>
        </div>

        <div class="setting-action-row">
          <span class="toggle-desc">Applied immediately to Komga and Kavita sources upon saving.</span>
          <button type="button" class="about-action-btn" on:click={async () => { await save(); notify('Server settings saved'); }}>
            <Check size={14} />
            <span>Save Servers</span>
          </button>
        </div>
      </section>

      <!-- 4. ABOUT SECTION -->
      <section class="settings-card about-shortcut-card">
        <div class="about-card-content">
          <div class="about-brand-group">
            <img src="/logo.png" alt="ZibraX" class="settings-logo-img" />
            <div>
              <h2 class="card-title">ZibraX Desktop</h2>
              <p class="card-subtitle">v2.2.2 • Rust Engine & Svelte 5</p>
            </div>
          </div>
          <button class="about-action-btn" on:click={() => navigate('about')}>
            <Info size={16} />
            <span>About & Links</span>
          </button>
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .settings-view {
    flex: 1;
    height: 100vh;
    overflow-y: auto;
    padding: var(--z-space-6) var(--z-space-8);
    background: var(--z-bg);
  }

  .settings-header {
    margin-bottom: var(--z-space-6);
  }

  .page-title {
    font-size: var(--z-text-2xl);
    font-weight: 700;
    color: var(--z-text);
    margin-bottom: 4px;
    letter-spacing: -0.3px;
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
  }

  :global(.title-icon) {
    color: var(--z-accent);
  }

  .about-shortcut-card {
    border-color: var(--z-border-hover);
  }

  .about-card-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--z-space-4);
    flex-wrap: wrap;
  }

  .about-brand-group {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
  }

  .settings-logo-img {
    width: 44px;
    height: 44px;
    object-fit: contain;
    filter: drop-shadow(0 0 10px var(--z-accent-glow));
  }

  .about-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    background: var(--z-accent);
    color: var(--z-accent-fg, #ffffff);
    font-size: var(--z-text-sm);
    font-weight: 600;
    border-radius: var(--z-radius-md);
    transition: all var(--z-duration-fast) var(--z-ease-out);
    box-shadow: 0 4px 16px var(--z-accent-glow);
  }

  .about-action-btn:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 20px var(--z-accent-glow);
  }

  .page-desc {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
  }

  .settings-sections {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-6);
    max-width: 860px;
    padding-bottom: 60px;
  }

  .settings-card {
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    padding: var(--z-space-5) var(--z-space-6);
    display: flex;
    flex-direction: column;
    gap: var(--z-space-5);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: var(--z-space-3);
    border-bottom: 1px solid var(--z-border);
    padding-bottom: var(--z-space-4);
  }

  .icon-wrap {
    width: 36px;
    height: 36px;
    border-radius: var(--z-radius-md);
    background: var(--z-surface-raised);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--z-accent);
    flex-shrink: 0;
  }

  .card-title {
    font-size: var(--z-text-base);
    font-weight: 600;
    color: var(--z-text);
    margin-bottom: 2px;
  }

  .card-subtitle {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
  }

  .field-block {
    display: flex;
    flex-direction: column;
    gap: var(--z-space-2);
  }

  .field-label {
    font-size: var(--z-text-xs);
    font-weight: 600;
    color: var(--z-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  /* PALETTE GRID */
  .palette-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 8px;
  }

  .palette-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: left;
  }

  .palette-btn:hover {
    border-color: var(--z-border-hover);
    background: var(--z-border-hover);
  }

  .palette-btn.active {
    border-color: var(--z-accent);
    background: var(--z-accent-muted);
  }

  .color-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .color-label {
    font-size: var(--z-text-xs);
    font-weight: 500;
    color: var(--z-text);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.check-icon) {
    color: var(--z-accent);
    flex-shrink: 0;
  }

  /* THEME MODE GRID */
  .theme-mode-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }

  .theme-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }

  .theme-card:hover {
    border-color: var(--z-border-hover);
  }

  .theme-card.selected {
    border-color: var(--z-accent);
    box-shadow: 0 0 0 1px var(--z-accent);
  }

  .theme-swatch {
    width: 32px;
    height: 32px;
    border-radius: var(--z-radius-sm);
    border: 1px solid rgba(255, 255, 255, 0.12);
    flex-shrink: 0;
  }

  .theme-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .theme-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .theme-name {
    font-size: var(--z-text-xs);
    font-weight: 600;
    color: var(--z-text);
  }

  .active-badge {
    font-size: 9px;
    font-weight: 700;
    background: var(--z-accent);
    color: var(--z-accent-fg, #ffffff);
    padding: 1px 6px;
    border-radius: 9999px;
    letter-spacing: 0.3px;
  }

  .theme-desc {
    font-size: 11px;
    color: var(--z-text-muted);
    line-height: 1.35;
  }

  /* SEGMENTED CONTROL */
  .segmented-bar {
    display: flex;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
    padding: 3px;
    gap: 3px;
  }

  .seg-btn {
    flex: 1;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: var(--z-radius-sm);
    font-size: var(--z-text-xs);
    font-weight: 500;
    color: var(--z-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .seg-btn:hover {
    color: var(--z-text);
  }

  .seg-btn.active {
    background: var(--z-accent);
    color: var(--z-accent-fg, #ffffff);
    font-weight: 600;
  }

  /* TACTILE SLIDERS */
  .slider-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-md);
  }

  .slider-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .slider-title {
    font-size: var(--z-text-sm);
    font-weight: 600;
    color: var(--z-text);
    display: block;
  }

  .slider-desc {
    font-size: 11px;
    color: var(--z-text-muted);
    display: block;
  }

  .num-pill {
    font-size: 11px;
    font-weight: 600;
    color: var(--z-text);
    background: var(--z-border);
    padding: 2px 8px;
    border-radius: 9999px;
  }

  .slider-bar {
    position: relative;
    height: 6px;
    background: var(--z-border);
    border-radius: 9999px;
    cursor: pointer;
    margin: 6px 0;
  }

  .slider-fill {
    position: absolute;
    height: 100%;
    background: var(--z-accent);
    border-radius: 9999px;
  }

  .slider-handle {
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

  /* TOGGLE SWITCH ROWS */
  .setting-toggle-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 0;
    border-top: 1px solid var(--z-border);
  }

  .toggle-title {
    font-size: var(--z-text-sm);
    font-weight: 500;
    color: var(--z-text);
    display: block;
  }

  .toggle-desc {
    font-size: 11px;
    color: var(--z-text-muted);
    display: block;
  }

  .switch-control {
    width: 38px;
    height: 22px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    border-radius: 9999px;
    position: relative;
    cursor: pointer;
    transition: all 0.2s ease;
    flex-shrink: 0;
  }

  .switch-control.checked {
    background: var(--z-accent);
    border-color: var(--z-accent);
  }

  .switch-dot {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: #ffffff;
    border-radius: 50%;
    transition: transform 0.2s ease;
  }

  .switch-control.checked .switch-dot {
    transform: translateX(16px);
  }

  /* ACTION ROW */
  .setting-action-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 0;
    border-top: 1px solid var(--z-border);
  }

  .clear-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.25);
    color: #ef4444;
    font-size: var(--z-text-xs);
    font-weight: 600;
    border-radius: var(--z-radius-md);
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .clear-btn:hover {
    background: rgba(239, 68, 68, 0.22);
  }

  .row-actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }

  .action-secondary-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    font-size: var(--z-text-xs);
    font-weight: 600;
    border-radius: var(--z-radius-md);
    transition: all 0.2s ease;
  }

  .action-secondary-btn:hover {
    border-color: var(--z-accent);
  }

  .hidden-file-input {
    display: none;
  }

  .server-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 12px;
  }

  .text-input {
    width: 100%;
    padding: 9px 12px;
    border-radius: var(--z-radius-md);
    background: var(--z-surface-raised);
    border: 1px solid var(--z-border);
    color: var(--z-text);
    font-size: var(--z-text-sm);
  }

  .text-input:focus {
    border-color: var(--z-accent);
  }
</style>