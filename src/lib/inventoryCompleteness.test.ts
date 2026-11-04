import { describe, expect, it } from "vitest";

import {
  deriveIncompleteReason,
  incompleteReasonSummary,
} from "./inventoryCompleteness";
import type { DataIssue } from "../types";

function issue(code: string): DataIssue {
  return { code, message: code, source: "test" };
}

describe("inventory incomplete reason", () => {
  it("separates count/read failures from capacity limits", () => {
    expect(
      deriveIncompleteReason([issue("TASK-INVENTORY-TASKS-COUNT-UNREADABLE")]),
    ).toBe("readFailure");
    expect(deriveIncompleteReason([issue("SERVICE-INVENTORY-PAGE-LIMIT")])).toBe(
      "capacity",
    );
    expect(deriveIncompleteReason([issue("TASK-INVENTORY-TRUNCATED")])).toBe(
      "capacity",
    );
  });

  it("distinguishes field truncation from an incomplete collection", () => {
    expect(deriveIncompleteReason([issue("SOFTWARE-FIELD-TRUNCATED")])).toBe(
      "fieldTruncation",
    );
    expect(deriveIncompleteReason([issue("registryStringTruncated")])).toBe(
      "fieldTruncation",
    );
  });

  it("reports a truncated source before individual issue details", () => {
    expect(
      deriveIncompleteReason([issue("ASSOCIATION-TRUNCATED")], true),
    ).toBe("sourceTruncated");
  });

  it("preserves read failure as the highest-priority explanation", () => {
    expect(
      deriveIncompleteReason([
        issue("SERVICE-INVENTORY-TRUNCATED"),
        issue("SERVICE-INVENTORY-ENUMERATION-FAILED"),
        issue("SERVICE-INVENTORY-BUFFER-LIMIT"),
      ]),
    ).toBe("readFailure");
  });

  it("does not invent a cause when the backend provides none", () => {
    expect(deriveIncompleteReason([])).toBe("unknown");
    expect(incompleteReasonSummary("unknown")).toContain("没有提供");
  });

  it("gives each reason a distinct user-facing explanation", () => {
    const summaries = new Set(
      (
        [
          "readFailure",
          "capacity",
          "fieldTruncation",
          "sourceTruncated",
          "unknown",
        ] as const
      ).map(incompleteReasonSummary),
    );
    expect(summaries.size).toBe(5);
  });
});
