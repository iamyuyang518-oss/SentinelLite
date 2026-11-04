import { useEffect, useRef } from "react";

import { scrollProgress } from "../lib/scrollIndicator";

const TOP_OFFSET = 56;
const BOTTOM_OFFSET = 12;
const INDICATOR_HEIGHT = 44;

/**
 * A small, transient scroll-position cue at the right edge.
 *
 * It only reads scroll metrics and writes a transform/opacity; the page keeps native wheel,
 * trackpad and keyboard scrolling. The native scrollbar track is hidden in CSS.
 */
export function ScrollIndicator() {
  const indicatorRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = indicatorRef.current;
    if (element === null) {
      return;
    }
    const indicator = element;

    let hideTimer = 0;
    let frame = 0;

    function update(show: boolean): void {
      const root = document.documentElement;
      const progress = scrollProgress(
        window.scrollY,
        root.scrollHeight,
        window.innerHeight,
      );
      const travel = Math.max(
        0,
        window.innerHeight - TOP_OFFSET - BOTTOM_OFFSET - INDICATOR_HEIGHT,
      );

      indicator.style.transform = `translateY(${Math.round(progress * travel)}px)`;

      if (show) {
        indicator.classList.add("is-visible");
        window.clearTimeout(hideTimer);
        hideTimer = window.setTimeout(() => {
          indicator.classList.remove("is-visible");
        }, 650);
      } else if (root.scrollHeight <= window.innerHeight) {
        indicator.classList.remove("is-visible");
      }
    }

    function onScroll(): void {
      if (frame !== 0) {
        return;
      }
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        update(true);
      });
    }

    function onResize(): void {
      update(false);
    }

    update(false);
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onResize);

    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onResize);
      window.clearTimeout(hideTimer);
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, []);

  return <div ref={indicatorRef} className="scroll-indicator" aria-hidden="true" />;
}
