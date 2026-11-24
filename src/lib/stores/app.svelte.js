import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { pushNavHistory, popNavHistory, toChronologicalChapters } from '../utils/readerNav.js';

export { toChronologicalChapters };

export function mediaUrl(rawUrl, referer = null, kind = 'page', headers = null) {
  if (!rawUrl || typeof rawUrl !== 'string') return '';
  const trimmed = rawUrl.trim();
  if (
    !trimmed ||
    trimmed.startsWith('data:') ||
    trimmed.startsWith('blob:') ||
    trimmed.startsWith('zibra-media:') ||
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
  return convertFileSrc(`v1/${b64url}`, 'zibra-media');
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
  try {
    await invoke('save_settings', { settings: newSettings });
    appState.settings = newSettings;
    applyTheme(newSettings);
  } catch (e) {
    console.error('Failed to save settings:', e);
  }
}

const PALETTE_COLORS = {
  totoro: '#4285f4',
  monet: '#38bdf8',
  expressive: '#c084fc',
  miku: '#06b6d4',
  asuka: '#f87171',
  mion: '#34d399',
  rikka: '#a78bfa',
  sakura: '#fb7185',
  mamimi: '#afc6ff',
  kanade: '#cbd5e1',
  itsuka: '#f97316',
  amoledvoid: '#3b82f6',
  highcontrast: '#ffffff',
  blue: '#4285f4',
  purple: '#a855f7',
  teal: '#14b8a6',
  green: '#22c55e',
  red: '#ef4444',
  orange: '#f97316',
  pink: '#ec4899',
  yellow: '#eab308',
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
  const motion = String(settings.animation_level || 'full').toLowerCase();
  root.setAttribute('data-motion', motion);
  const themeKey = String(settings.color_theme || 'totoro').toLowerCase();
  const hex = PALETTE_COLORS[themeKey] || (themeKey.startsWith('#') ? themeKey : '#4285f4');
  root.setAttribute('data-accent', themeKey);
  root.style.setProperty('--z-accent', hex);
  root.style.setProperty('--z-accent-glow', hex + '55');
  root.style.setProperty('--z-accent-muted', hex + '22');
}
