import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { pushNavHistory, popNavHistory, toChronologicalChapters } from '../utils/readerNav.js';

export { toChronologicalChapters };

export function plainSnapshot(obj) {
  if (obj === null || obj === undefined || typeof obj !== 'object') return obj;
  try {
    return $state.snapshot(obj);
  } catch (_) {
    return JSON.parse(JSON.stringify(obj));
  }
}

export function mediaUrl(rawUrl, referer = null, kind = 'page', headers = null) {
  if (!rawUrl || typeof rawUrl !== 'string') return '';
  const trimmed = rawUrl.trim();
  if (
    !trimmed ||
    trimmed.startsWith('data:') ||
    trimmed.startsWith('blob:') ||
    trimmed.startsWith('zibra-media:') ||
    trimmed.startsWith('http://zibra-media.localhost') ||
    trimmed.startsWith('https://zibra-media.localhost') ||
    trimmed.startsWith('/logo') ||
    trimmed.startsWith('/chibi')
  ) {
    return trimmed;
  }
  const payloadObj = { u: trimmed, r: referer || null, k: kind };
  if (headers && typeof headers === 'object' && Object.keys(headers).length > 0) {
    payloadObj.h = headers;
  }
  const bytes = new TextEncoder().encode(JSON.stringify(payloadObj));
  let bin = '';
  for (let i = 0; i < bytes.length; i++) {
    bin += String.fromCharCode(bytes[i]);
  }
  const b64url = btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  return convertFileSrc(b64url, 'zibra-media');
}

export const appState = $state({
  currentScreen: 'history',
  historyStack: [],
  railExpanded: false,
  settings: null,
  activeManga: null,
  activeChapter: null,
  activeChapters: [],
  initialPageIndex: 0,
  selectedSourceId: 'MANGADEX',
  notification: null,
});

export function notify(message, duration = 3000) {
  appState.notification = message;
  setTimeout(() => {
    if (appState.notification === message) {
      appState.notification = null;
    }
  }, duration);
}

export function navigate(screen) {
  const next = pushNavHistory(appState.historyStack, appState.currentScreen, screen);
  appState.historyStack = next.stack;
  appState.currentScreen = next.currentScreen;
}

export function openManga(manga) {
  appState.activeManga = manga;
  navigate('details');
}

export function openReader(manga, chapter, page = 0, chapters = null) {
  appState.activeManga = manga;
  appState.activeChapter = chapter;
  appState.activeChapters = Array.isArray(chapters) ? chapters : [];
  appState.initialPageIndex = page;
  navigate('reader');
}

export function goBack() {
  const next = popNavHistory(appState.historyStack, appState.currentScreen, Boolean(appState.activeManga));
  appState.historyStack = next.stack;
  appState.currentScreen = next.currentScreen;
}

export async function loadSettings() {
  try {
    const s = await invoke('get_settings');
    appState.settings = s;
    applyTheme(s);
    return s;
  } catch (e) {
    console.error('Failed to load settings:', e);
  }
}

export async function updateSettings(newSettings) {
  const clean = plainSnapshot(newSettings);
  try {
    await invoke('save_settings', { settings: clean });
    appState.settings = clean;
    applyTheme(clean);
    return true;
  } catch (e) {
    console.error('Failed to save settings:', e);
    notify('Failed to save settings: ' + e);
    return false;
  }
}

export async function toggleSourcePin(sourceId) {
  if (!appState.settings) {
    await loadSettings();
  }
  try {
    const nextIds = await invoke('toggle_source_pin', { sourceId });
    if (appState.settings) {
      appState.settings = { ...plainSnapshot(appState.settings), pinned_source_ids: nextIds };
    }
    return nextIds;
  } catch (e) {
    notify('Failed to save pinned sources: ' + e);
    return null;
  }
}

const PALETTE_THEMES = {
  totoro: { accent: '#4285f4', darkBg: '#23212c', darkSurface: '#2b2837', darkRaised: '#343042', lightBg: '#f9f9ff', lightSurface: '#ffffff', lightRaised: '#ededf4' },
  monet: { accent: '#38bdf8', darkBg: '#0d1219', darkSurface: '#161c24', darkRaised: '#1f2833', lightBg: '#f8faff', lightSurface: '#ffffff', lightRaised: '#eceff5' },
  expressive: { accent: '#c084fc', darkBg: '#140f19', darkSurface: '#1e1724', darkRaised: '#2a2033', lightBg: '#fcf7ff', lightSurface: '#ffffff', lightRaised: '#f1ebf5' },
  miku: { accent: '#06b6d4', darkBg: '#0b1213', darkSurface: '#141c1d', darkRaised: '#1d282a', lightBg: '#f5fafa', lightSurface: '#ffffff', lightRaised: '#eaefee' },
  asuka: { accent: '#f87171', darkBg: '#160e0d', darkSurface: '#221715', darkRaised: '#2f201e', lightBg: '#fff8f6', lightSurface: '#ffffff', lightRaised: '#fceae7' },
  mion: { accent: '#34d399', darkBg: '#0d140c', darkSurface: '#172016', darkRaised: '#212d20', lightBg: '#f7fbf1', lightSurface: '#ffffff', lightRaised: '#ecefe6' },
  rikka: { accent: '#a78bfa', darkBg: '#120e18', darkSurface: '#1c1724', darkRaised: '#272133', lightBg: '#fef7ff', lightSurface: '#ffffff', lightRaised: '#f2ecf4' },
  sakura: { accent: '#fb7185', darkBg: '#160e12', darkSurface: '#22161b', darkRaised: '#2f2027', lightBg: '#fff8f8', lightSurface: '#ffffff', lightRaised: '#faeaed' },
  mamimi: { accent: '#afc6ff', darkBg: '#0e1118', darkSurface: '#161b26', darkRaised: '#212838', lightBg: '#faf8ff', lightSurface: '#ffffff', lightRaised: '#eeedf4' },
  kanade: { accent: '#cbd5e1', darkBg: '#101012', darkSurface: '#1b1b1e', darkRaised: '#26262b', lightBg: '#fcf8f9', lightSurface: '#ffffff', lightRaised: '#ebe7e8' },
  itsuka: { accent: '#f97316', darkBg: '#16100a', darkSurface: '#231a12', darkRaised: '#31241a', lightBg: '#fff8f5', lightSurface: '#ffffff', lightRaised: '#fdeae1' },
  amoledvoid: { accent: '#3b82f6', darkBg: '#000000', darkSurface: '#0d0d0d', darkRaised: '#17171a', lightBg: '#f9f9ff', lightSurface: '#ffffff', lightRaised: '#ededf4' },
  highcontrast: { accent: '#ffffff', darkBg: '#050505', darkSurface: '#141414', darkRaised: '#222222', lightBg: '#ffffff', lightSurface: '#ffffff', lightRaised: '#f0f0f0' },
  blue: { accent: '#4285f4', darkBg: '#23212c', darkSurface: '#2b2837', darkRaised: '#343042', lightBg: '#f8fafc', lightSurface: '#ffffff', lightRaised: '#f1f5f9' },
  purple: { accent: '#a855f7', darkBg: '#140f19', darkSurface: '#1e1724', darkRaised: '#2a2033', lightBg: '#fcf7ff', lightSurface: '#ffffff', lightRaised: '#f1ebf5' },
  teal: { accent: '#14b8a6', darkBg: '#0b1213', darkSurface: '#141c1d', darkRaised: '#1d282a', lightBg: '#f5fafa', lightSurface: '#ffffff', lightRaised: '#eaefee' },
  green: { accent: '#22c55e', darkBg: '#0d140c', darkSurface: '#172016', darkRaised: '#212d20', lightBg: '#f7fbf1', lightSurface: '#ffffff', lightRaised: '#ecefe6' },
  red: { accent: '#ef4444', darkBg: '#160e0d', darkSurface: '#221715', darkRaised: '#2f201e', lightBg: '#fff8f6', lightSurface: '#ffffff', lightRaised: '#fceae7' },
  orange: { accent: '#f97316', darkBg: '#16100a', darkSurface: '#231a12', darkRaised: '#31241a', lightBg: '#fff8f5', lightSurface: '#ffffff', lightRaised: '#fdeae1' },
  pink: { accent: '#ec4899', darkBg: '#160e12', darkSurface: '#22161b', darkRaised: '#2f2027', lightBg: '#fff8f8', lightSurface: '#ffffff', lightRaised: '#faeaed' },
  yellow: { accent: '#eab308', darkBg: '#16140a', darkSurface: '#221f11', darkRaised: '#2f2a18', lightBg: '#fffdf5', lightSurface: '#ffffff', lightRaised: '#f7f3e3' },
};

let systemThemeListenerRegistered = false;

export function applyTheme(settings) {
  if (!settings) return;
  const root = document.documentElement;
  let theme = settings.is_amoled
    ? 'amoled'
    : (settings.theme?.toLowerCase() || settings.app_theme?.toLowerCase() || 'dark');

  if (theme === 'auto') {
    const prefersDark = typeof window !== 'undefined'
      && window.matchMedia?.('(prefers-color-scheme: dark)').matches;
    theme = prefersDark ? 'dark' : 'light';
    if (typeof window !== 'undefined' && window.matchMedia && !systemThemeListenerRegistered) {
      systemThemeListenerRegistered = true;
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        if (appState.settings) applyTheme(appState.settings);
      });
    }
  }

  root.setAttribute('data-theme', theme);
  const rawMotion = String(settings.animation_level || 'smooth').toLowerCase();
  const motion = rawMotion === 'full' ? 'smooth' : rawMotion;
  root.setAttribute('data-motion', motion);

  const themeKey = String(settings.color_theme || 'totoro').toLowerCase();
  const pal = PALETTE_THEMES[themeKey] || PALETTE_THEMES.totoro;
  const hex = themeKey === 'highcontrast' && theme === 'light' ? '#111111' : pal.accent;
  const accentFg = themeKey === 'highcontrast' && theme !== 'light' ? '#050505' : '#ffffff';
  root.setAttribute('data-accent', themeKey);
  root.style.setProperty('--z-accent', hex);
  root.style.setProperty('--z-accent-fg', accentFg);
  root.style.setProperty('--z-accent-glow', hex + '55');
  root.style.setProperty('--z-accent-muted', hex + '24');

  if (theme === 'light') {
    root.style.setProperty('--z-bg', pal.lightBg);
    root.style.setProperty('--z-surface', pal.lightSurface);
    root.style.setProperty('--z-surface-raised', pal.lightRaised);
    root.style.setProperty('--z-surface-hover', pal.lightRaised);
    root.style.setProperty('--z-surface-variant', pal.lightRaised);
    root.style.setProperty('--z-surface-overlay', 'rgba(255, 255, 255, 0.92)');
    root.style.setProperty('--z-reader-bg', pal.lightBg);
    root.style.setProperty('--z-accent-contrast', themeKey === 'highcontrast' ? '#ffffff' : '#ffffff');
  } else if (theme === 'amoled') {
    root.style.setProperty('--z-bg', '#000000');
    root.style.setProperty('--z-surface', '#07080a');
    root.style.setProperty('--z-surface-raised', '#111318');
    root.style.setProperty('--z-surface-hover', '#171920');
    root.style.setProperty('--z-surface-variant', '#111318');
    root.style.setProperty('--z-surface-overlay', 'rgba(0, 0, 0, 0.92)');
    root.style.setProperty('--z-reader-bg', '#000000');
  } else {
    root.style.setProperty('--z-bg', pal.darkBg);
    root.style.setProperty('--z-surface', pal.darkSurface);
    root.style.setProperty('--z-surface-raised', pal.darkRaised);
    root.style.setProperty('--z-surface-hover', pal.darkRaised);
    root.style.setProperty('--z-surface-variant', pal.darkRaised);
    root.style.setProperty('--z-surface-overlay', pal.darkBg + 'e6');
    root.style.setProperty('--z-reader-bg', pal.darkBg);
  }
}
