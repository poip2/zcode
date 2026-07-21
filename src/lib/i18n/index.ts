import { writable, derived } from 'svelte/store';
import en from './en.json';
import zh from './zh.json';

export type Locale = 'en' | 'zh';

const translations: Record<Locale, typeof en> = { en, zh };

// Reactive locale store — persist via settings store
export const locale = writable<Locale>('en');

/**
 * Derived translation function. Usage in Svelte components:
 *   import { t } from '$lib/i18n';
 *   {$t('sidebar.title')}
 *   {$t('agent.running', { tool: 'write' })}
 */
export const t = derived(locale, ($locale) => {
  return (key: string, params?: Record<string, string | number>): string => {
    const keys = key.split('.');
    let val: unknown = translations[$locale];
    for (const k of keys) {
      if (val && typeof val === 'object') {
        val = (val as Record<string, unknown>)[k];
      } else {
        return key; // fallback: show key itself
      }
    }
    if (typeof val !== 'string') return key;
    if (params) {
      return val.replace(/\{(\w+)\}/g, (_, k: string) =>
        String(params[k] ?? `{${k}}`),
      );
    }
    return val;
  };
});

/**
 * Sync version for use outside Svelte components (.ts files, stores, etc.)
 * Gets the current locale value from the store snapshot.
 */
import { get } from 'svelte/store';
export function tt(key: string, params?: Record<string, string | number>): string {
  const $locale = get(locale);
  const keys = key.split('.');
  let val: unknown = translations[$locale];
  for (const k of keys) {
    if (val && typeof val === 'object') {
      val = (val as Record<string, unknown>)[k];
    } else {
      return key;
    }
  }
  if (typeof val !== 'string') return key;
  if (params) {
    return val.replace(/\{(\w+)\}/g, (_, k: string) =>
      String(params[k] ?? `{${k}}`),
    );
  }
  return val;
}
