import { describe, expect, it } from "vitest";

import {
  formatDateLabel,
  formatDateValue,
  parseDateBoundary,
  parseDateValue,
} from "./dateRange";

describe("dateRange", () => {
  it("parses local calendar dates and rejects impossible dates", () => {
    const date = parseDateValue("2026-11-02");
    expect(date).not.toBeNull();
    expect(date?.getFullYear()).toBe(2026);
    expect(date?.getMonth()).toBe(10);
    expect(date?.getDate()).toBe(2);
    expect(parseDateValue("2026-02-31")).toBeNull();
    expect(parseDateValue("not-a-date")).toBeNull();
  });

  it("turns a selected date into inclusive local day boundaries", () => {
    const start = parseDateBoundary("2026-11-02", false);
    const end = parseDateBoundary("2026-11-02", true);

    expect(start?.getHours()).toBe(0);
    expect(start?.getMinutes()).toBe(0);
    expect(end?.getHours()).toBe(23);
    expect(end?.getMinutes()).toBe(59);
    expect(end?.getSeconds()).toBe(59);
    expect(end?.getMilliseconds()).toBe(999);
  });

  it("formats date values and labels", () => {
    expect(formatDateValue(2026, 10, 2)).toBe("2026-11-02");
    expect(formatDateLabel("2026-11-02")).toBe("2026/11/02");
  });
});
