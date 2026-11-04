import { useCallback, useLayoutEffect, useState } from "react";

export type Theme = "light" | "dark";

export const THEME_STORAGE_KEY = "sentinellite.theme";
export const DEFAULT_THEME: Theme = "light";

function isTheme(value: unknown): value is Theme {
  return value === "light" || value === "dark";
}

/** Reads the saved choice; any storage failure falls back to the light default. */
export function readStoredTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isTheme(stored) ? stored : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

export function persistTheme(theme: Theme): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // A restricted storage environment keeps the in-memory theme for this session.
  }
}

/** Applies the saved theme before React mounts to avoid a light flash. */
export function initializeTheme(): Theme {
  const theme = readStoredTheme();
  applyTheme(theme);
  return theme;
}

export interface ThemeController {
  theme: Theme;
  toggleTheme: () => void;
}

export function useTheme(): ThemeController {
  const [theme, setTheme] = useState<Theme>(readStoredTheme);

  useLayoutEffect(() => {
    applyTheme(theme);
    persistTheme(theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === "light" ? "dark" : "light"));
  }, []);

  return { theme, toggleTheme };
}
