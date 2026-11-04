import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import {
  applyTheme,
  initializeTheme,
  persistTheme,
  readStoredTheme,
  THEME_STORAGE_KEY,
  useTheme,
} from "./theme";

beforeEach(() => {
  window.localStorage.clear();
  delete document.documentElement.dataset.theme;
  document.documentElement.style.colorScheme = "";
});

describe("theme", () => {
  it("defaults to light and reads a stored choice", () => {
    expect(readStoredTheme()).toBe("light");

    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");
    expect(readStoredTheme()).toBe("dark");

    window.localStorage.setItem(THEME_STORAGE_KEY, "unexpected");
    expect(readStoredTheme()).toBe("light");
  });

  it("applies the theme to the document", () => {
    applyTheme("dark");

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("initializes from storage before the app renders", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");

    expect(initializeTheme()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("persists the theme choice", () => {
    persistTheme("dark");

    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
  });

  it("toggles and persists through the hook", () => {
    const { result } = renderHook(() => useTheme());

    expect(result.current.theme).toBe("light");

    act(() => {
      result.current.toggleTheme();
    });

    expect(result.current.theme).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
