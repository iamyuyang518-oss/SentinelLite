import { describe, expect, it } from "vitest";

import { scrollProgress } from "./scrollIndicator";

describe("scrollProgress", () => {
  it("maps the scroll range to a 0–1 position", () => {
    expect(scrollProgress(0, 1_000, 500)).toBe(0);
    expect(scrollProgress(250, 1_000, 500)).toBe(0.5);
    expect(scrollProgress(500, 1_000, 500)).toBe(1);
  });

  it("clamps out-of-range values", () => {
    expect(scrollProgress(-20, 1_000, 500)).toBe(0);
    expect(scrollProgress(900, 1_000, 500)).toBe(1);
  });

  it("returns zero when there is no scrollable range", () => {
    expect(scrollProgress(0, 500, 500)).toBe(0);
    expect(scrollProgress(0, 100, 500)).toBe(0);
    expect(scrollProgress(Number.NaN, 1_000, 500)).toBe(0);
  });
});
