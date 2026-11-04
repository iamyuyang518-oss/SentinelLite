import { flushSync } from "react-dom";

import type { MouseEvent } from "react";
import type { Theme } from "../lib/theme";

type ViewTransitionDocument = Document & {
  startViewTransition?: (update: () => void) => { finished: Promise<void> };
};

function MoonIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M15.8 12.2A6.4 6.4 0 0 1 7.8 4.2 6.4 6.4 0 1 0 15.8 12.2Z" />
    </svg>
  );
}

function SunIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <circle cx="10" cy="10" r="3.4" />
      <path d="M10 2.4v1.4M10 16.2v1.4M2.4 10h1.4M16.2 10h1.4M4.6 4.6l1 1M14.4 14.4l1 1M15.4 4.6l-1 1M5.6 14.4l-1 1" />
    </svg>
  );
}

export function ThemeToggle({
  theme,
  onToggle,
}: {
  theme: Theme;
  onToggle: () => void;
}) {
  const target = theme === "dark" ? "浅色模式" : "深色模式";

  function handleToggle(event: MouseEvent<HTMLButtonElement>): void {
    const reduceMotion =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const viewTransitionDocument = document as ViewTransitionDocument;

    if (
      reduceMotion ||
      typeof viewTransitionDocument.startViewTransition !== "function"
    ) {
      onToggle();
      return;
    }

    const rect = event.currentTarget.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const radius = Math.hypot(
      Math.max(x, window.innerWidth - x),
      Math.max(y, window.innerHeight - y),
    );
    const root = document.documentElement;
    root.style.setProperty("--theme-x", `${x}px`);
    root.style.setProperty("--theme-y", `${y}px`);
    root.style.setProperty("--theme-radius", `${radius}px`);

    let toggled = false;
    try {
      viewTransitionDocument.startViewTransition(() => {
        toggled = true;
        flushSync(onToggle);
      });
    } catch {
      if (!toggled) {
        onToggle();
      }
    }
  }

  return (
    <button
      type="button"
      className="theme-toggle"
      onClick={handleToggle}
      aria-label={`切换到${target}`}
      title={`切换到${target}`}
    >
      {theme === "dark" ? <SunIcon /> : <MoonIcon />}
    </button>
  );
}
