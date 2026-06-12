// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

export interface ThemeDefinition {
  id: string;
  label: string;
  description: string;
  /** Swatch colors shown in the theme picker: [background, surface, accent]. */
  swatch: [string, string, string];
}

export const DEFAULT_THEME = "creamsicle";

export const THEMES: ThemeDefinition[] = [
  {
    id: "creamsicle",
    label: "Creamsicle",
    description: "Pastel orange on warm cream — the OrangeDL classic.",
    swatch: ["#ffedd9", "#fff8ef", "#ff9442"],
  },
  {
    id: "midnight",
    label: "Midnight Marmalade",
    description: "Dark cocoa with a bright orange glow.",
    swatch: ["#0f0d0a", "#1d160f", "#f97316"],
  },
  {
    id: "peach",
    label: "Peach Fizz",
    description: "Soft coral and peach, sweet and fizzy.",
    swatch: ["#ffe7df", "#fff6f2", "#ff7e5f"],
  },
  {
    id: "mint",
    label: "Mint Squeeze",
    description: "Cool pastel mint with an orange twist.",
    swatch: ["#e2f6ec", "#f4fcf8", "#ff9442"],
  },
  {
    id: "bubblegum",
    label: "Bubblegum",
    description: "Playful pastel pink, extra cheerful.",
    swatch: ["#fde4f2", "#fff4fa", "#f25cb1"],
  },
];

const THEME_STORAGE_KEY = "orangedl-theme";

function isKnownTheme(id: string): boolean {
  return THEMES.some((theme) => theme.id === id);
}

/** Sets the active theme on the document and mirrors it to localStorage so
 *  the next launch can apply it before settings load (no flash). */
export function applyTheme(id: string) {
  const theme = isKnownTheme(id) ? id : DEFAULT_THEME;
  document.documentElement.dataset.theme = theme;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // storage may be unavailable; theme still applies for this session
  }
}

/** Applies the last-used theme from localStorage before settings load. */
export function applyStoredTheme() {
  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  } catch {
    stored = null;
  }
  document.documentElement.dataset.theme =
    stored && isKnownTheme(stored) ? stored : DEFAULT_THEME;
}
