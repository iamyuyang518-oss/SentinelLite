/**
 * Returns a clamped 0–1 position for the transient scroll indicator.
 * A page with no scrollable range always returns zero.
 */
export function scrollProgress(
  scrollTop: number,
  scrollHeight: number,
  clientHeight: number,
): number {
  if (
    !Number.isFinite(scrollTop) ||
    !Number.isFinite(scrollHeight) ||
    !Number.isFinite(clientHeight)
  ) {
    return 0;
  }

  const range = scrollHeight - clientHeight;
  if (range <= 0) {
    return 0;
  }

  return Math.min(1, Math.max(0, scrollTop / range));
}
