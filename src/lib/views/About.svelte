<script>
  import { 
    Globe, MessageSquare, Code2, ShieldCheck, Cpu, ArrowUpRight, Heart, ArrowLeft
  } from 'lucide-svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { goBack, notify } from '../stores/app.svelte.js';

  async function openExternal(url) {
    try {
      await invoke('open_external_url', { url });
    } catch (e) {
      notify('Could not open browser: ' + e);
    }
  }

  const links = [
    {
      title: 'Official Website',
      url: 'https://zeravrix.pages.dev/',
      desc: 'Project portal, release downloads, and web updates',
      icon: Globe,
      badge: 'Portal'
    },
    {
      title: 'Discord Community',
      url: 'https://discord.gg/YrrmFNCvXx',
      desc: 'Direct developer support, suggestions, and source discussions',
      icon: MessageSquare,
      badge: 'Chat'
    },
    {
      title: 'Source Code Repository',
      url: 'https://github.com/zeravrix/ZibraX',
      desc: 'Native Rust backend & Svelte 5 desktop engine, GPL-3.0 open source code',
      icon: Code2,
      badge: 'GitHub'
    },
    {
      title: 'Microsoft Store',
      url: 'https://apps.microsoft.com/search?query=ZERAVRIX',
      desc: 'Official ZERAVRIX Windows Store package edition',
      icon: Globe,
      badge: 'Store'
    }
  ];
</script>

<div class="about-view">
  <div class="about-content">
    <!-- TOP NAVIGATION -->
    <div class="top-nav-row">
      <button class="back-pill" on:click={goBack}>
        <ArrowLeft size={16} />
        <span>Back to Settings</span>
      </button>
    </div>

    <!-- HERO HEADER -->
    <div class="about-hero">
      <div class="logo-pod">
        <img src="/logo.png" alt="ZibraX Logo" class="about-logo-img" />
        <div class="logo-ambient-glow"></div>
      </div>
      <h1 class="app-title">ZibraX</h1>
      <div class="version-badges">
        <span class="v-pill">v2.2.2 Desktop</span>
        <span class="v-pill rust">Rust Engine & Svelte 5</span>
        <span class="v-pill">Zero JVM</span>
      </div>
      <p class="app-tagline">
        The high-performance, native comic and manga reader with zero Kotlin/JVM overhead, ultra-responsive reading layouts, and 1,190+ sources.
      </p>
    </div>

    <!-- LINK CARDS -->
    <div class="link-grid">
      {#each links as item}
        <button
          type="button"
          class="link-card"
          on:click={() => openExternal(item.url)}
        >
          <div class="card-icon-box">
            <svelte:component this={item.icon} size={22} />
          </div>
          <div class="card-text">
            <div class="card-title-row">
              <span class="card-title">{item.title}</span>
              <span class="badge">{item.badge}</span>
            </div>
            <p class="card-desc">{item.desc}</p>
            <span class="card-url">{item.url}</span>
          </div>
          <ArrowUpRight size={18} class="arrow-icon" />
        </button>
      {/each}
    </div>

    <!-- SPECS & TECH FOOTER -->
    <div class="tech-specs-card">
      <div class="spec-item">
        <Cpu size={16} class="spec-icon" />
        <span class="spec-label">Backend Engine</span>
        <span class="spec-val">Rust Engine • Tokio Async • WebKitGTK 4.1</span>
      </div>
      <div class="spec-item">
        <ShieldCheck size={16} class="spec-icon" />
        <span class="spec-label">Architecture</span>
        <span class="spec-val">Standalone Native ELF • 13 MB Compact Binary</span>
      </div>
      <div class="spec-item">
        <Heart size={16} class="spec-icon" />
        <span class="spec-label">Project Family</span>
        <span class="spec-val">ZERAVRIX Ecosystem → ZibraX 2.2.2 Edition</span>
      </div>
    </div>
  </div>
</div>

<style>
  .about-view {
    flex: 1;
    height: 100vh;
    overflow-y: auto;
    padding: var(--z-space-8) var(--z-space-6);
    background: var(--z-bg);
    display: flex;
    justify-content: center;
  }

  .about-content {
    width: 100%;
    max-width: 680px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--z-space-6);
    margin: 0 auto;
    padding-bottom: 60px;
  }

  .top-nav-row {
    width: 100%;
    display: flex;
    justify-content: flex-start;
  }

  .back-pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-radius: 9999px;
    border: 1px solid var(--z-border);
    background: var(--z-surface);
    color: var(--z-text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .back-pill:hover {
    color: var(--z-text);
    background: var(--z-surface-hover);
    border-color: var(--z-accent);
  }

  .about-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }

  .logo-pod {
    position: relative;
    width: 96px;
    height: 96px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--z-space-4);
  }

  .about-logo-img {
    width: 80px;
    height: 80px;
    object-fit: contain;
    position: relative;
    z-index: 2;
    filter: drop-shadow(0 0 20px rgba(66, 133, 244, 0.5));
  }

  .logo-ambient-glow {
    position: absolute;
    inset: 0;
    background: radial-gradient(circle, rgba(66, 133, 244, 0.3) 0%, transparent 70%);
    border-radius: 50%;
    filter: blur(14px);
    z-index: 1;
  }

  .app-title {
    font-size: var(--z-text-2xl);
    font-weight: 800;
    color: var(--z-text);
    margin-bottom: var(--z-space-2);
    letter-spacing: -0.5px;
  }

  .version-badges {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: var(--z-space-3);
    flex-wrap: wrap;
    justify-content: center;
  }

  .v-pill {
    font-size: 11px;
    font-weight: 600;
    background: var(--z-surface-raised);
    color: var(--z-text-muted);
    border: 1px solid var(--z-border);
    padding: 3px 10px;
    border-radius: var(--z-radius-full);
  }

  .v-pill.rust {
    background: rgba(66, 133, 244, 0.12);
    color: var(--z-accent);
    border-color: rgba(66, 133, 244, 0.25);
  }

  .app-tagline {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    line-height: 1.6;
    max-width: 520px;
  }

  .link-grid {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
  }

  .link-card {
    display: flex;
    align-items: center;
    gap: var(--z-space-4);
    width: 100%;
    text-align: left;
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    padding: var(--z-space-4) var(--z-space-5);
    text-decoration: none;
    color: inherit;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .link-card:hover {
    border-color: var(--z-accent);
    background: var(--z-surface-raised);
    transform: translateY(-2px);
  }

  .card-icon-box {
    width: 44px;
    height: 44px;
    border-radius: var(--z-radius-md);
    background: var(--z-surface-raised);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--z-accent);
    flex-shrink: 0;
  }

  .card-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-title {
    font-size: var(--z-text-base);
    font-weight: 600;
    color: var(--z-text);
  }

  .badge {
    font-size: 10px;
    font-weight: 700;
    background: rgba(255, 255, 255, 0.08);
    color: var(--z-text-muted);
    padding: 1px 7px;
    border-radius: 9999px;
    text-transform: uppercase;
  }

  .card-desc {
    font-size: var(--z-text-xs);
    color: var(--z-text-muted);
    line-height: 1.4;
  }

  .card-url {
    font-size: 11px;
    color: var(--z-accent);
    font-family: var(--z-font-mono);
    opacity: 0.85;
    margin-top: 2px;
  }

  :global(.arrow-icon) {
    color: var(--z-text-muted);
    flex-shrink: 0;
    transition: transform 0.2s, color 0.2s;
  }

  .link-card:hover :global(.arrow-icon) {
    color: var(--z-accent);
    transform: translate(2px, -2px);
  }

  .tech-specs-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    background: var(--z-surface);
    border: 1px solid var(--z-border);
    border-radius: var(--z-radius-lg);
    padding: 14px 18px;
  }

  .spec-item {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--z-text-xs);
  }

  :global(.spec-icon) {
    color: var(--z-text-muted);
    flex-shrink: 0;
  }

  .spec-label {
    font-weight: 600;
    color: var(--z-text-muted);
    min-width: 120px;
  }

  .spec-val {
    color: var(--z-text);
    font-family: var(--z-font-mono);
    font-size: 11px;
  }
</style>