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
    label: "Graphite",
    description: "Balanced charcoal with a restrained orange signal.",
    swatch: ["#16181c", "#20242a", "#e47b3a"],
  },
  {
    id: "midnight",
    label: "Midnight",
    description: "Deeper contrast for focused late-night sessions.",
    swatch: ["#0e1116", "#181d24", "#d8783f"],
  },
  {
    id: "peach",
    label: "Paper",
    description: "Warm, low-glare light surfaces with crisp type.",
    swatch: ["#ece8e4", "#f7f5f2", "#c9663f"],
  },
  {
    id: "mint",
    label: "Forest",
    description: "Cool green-black surfaces for softer contrast.",
    swatch: ["#111a18", "#1c2926", "#cf7848"],
  },
  {
    id: "bubblegum",
    label: "Ember",
    description: "Muted plum charcoal with a dusty coral accent.",
    swatch: ["#1c181c", "#2a242a", "#c57675"],
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
