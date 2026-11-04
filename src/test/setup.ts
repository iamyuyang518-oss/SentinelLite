import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";

beforeEach(() => {
  try {
    window.localStorage.clear();
  } catch {
    // Storage can be unavailable in a restricted test environment.
  }
  delete document.documentElement.dataset.theme;
  document.documentElement.style.colorScheme = "";
});

afterEach(() => {
  cleanup();
});
