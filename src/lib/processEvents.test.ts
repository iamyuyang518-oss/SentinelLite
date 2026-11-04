import { describe, expect, it } from "vitest";

import {
  EMPTY_LIVE_STATE,
  MAX_LIVE_EVENTS,
  applyPairingRevision,
  completenessIssues,
  eventTimeLabel,
  eventTimeSource,
  mergeLiveBatch,
  pairingPresentation,
  processDisplayName,
} from "./processEvents";
import { makeEventBatch, makePairingRevision, makeProcessEvent } from "../test/fixtures";
import type { ProcessEventWire } from "../types";

function start(sequence = 1, overrides: Partial<ProcessEventWire> = {}): ProcessEventWire {
  return makeProcessEvent({
    event_id: `session-test:${sequence.toString(16).padStart(16, "0")}`,
    sequence,
    event_type: "started",
    pairing: { state: "waiting" },
    ...overrides,
  });
}

function stop(sequence = 2, overrides: Partial<ProcessEventWire> = {}): ProcessEventWire {
  return makeProcessEvent({
    event_id: `session-test:${sequence.toString(16).padStart(16, "0")}`,
    sequence,
    event_type: "stopped",
    etw: { ...makeProcessEvent().etw, event_id: 2 },
    pairing: {
      state: "unpaired",
      reason: { reason: "stop_without_observed_start" },
    },
    ...overrides,
  });
}

describe("process event presentation", () => {
  it("uses the image basename but never invents a name", () => {
    expect(processDisplayName("C:\\Windows\\System32\\notepad.exe")).toBe("notepad.exe");
    expect(processDisplayName("C:/Tools/tool.exe")).toBe("tool.exe");
    expect(processDisplayName(null)).toBe("名称未提供");
  });

  it("keeps the three clocks distinct and reports the source actually used", () => {
    const event = makeProcessEvent({
      etw: { ...makeProcessEvent().etw, occurred_at: null },
    });
    expect(eventTimeSource(event)).toBe("传感器接收时间");
    expect(eventTimeLabel(event)).not.toBe("时间未知");
    expect(eventTimeSource(makeProcessEvent({ received_at: null, normalized_at: null }))).toBe(
      "ETW 事件时间",
    );
    expect(
      eventTimeSource(
        makeProcessEvent({
          etw: { ...makeProcessEvent().etw, occurred_at: null },
          received_at: null,
          normalized_at: null,
        }),
      ),
    ).toBe("无可用时间");
  });

  it("translates pairing anomalies without calling them malicious", () => {
    expect(pairingPresentation({ state: "waiting" }).label).toBe("等待退出");
    expect(
      pairingPresentation({
        state: "unpaired",
        reason: { reason: "identity_mismatch", open_create_time: 42 },
      }).label,
    ).toContain("身份不匹配");
    expect(
      pairingPresentation({
        state: "indeterminate",
        reason: "missing_process_create_time",
      }).tone,
    ).toBe("indeterminate");
  });

  it("surfaces completeness issues instead of treating them as completeness", () => {
    const event = makeProcessEvent({
      parent_pid: null,
      completeness: {
        ...makeProcessEvent().completeness,
        parent_pid_available: false,
        issues: ["missing_parent_pid"],
      },
    });
    expect(completenessIssues(event)).toEqual(["缺少父进程 PID"]);
  });
});

describe("live event merge", () => {
  it("appends events, deduplicates by event id, and applies pairing revisions", () => {
    const initial = mergeLiveBatch(
      EMPTY_LIVE_STATE,
      makeEventBatch([{ cursor: 1, record: { record: "process_event", ...start(1) } }]),
    );
    const withStop = mergeLiveBatch(
      initial,
      makeEventBatch([
        { cursor: 2, record: { record: "process_event", ...stop(2) } },
        { cursor: 3, record: { record: "pairing_revision", ...makePairingRevision() } },
      ]),
    );

    expect(withStop.events).toHaveLength(2);
    expect(withStop.revisions).toHaveLength(1);
    const pairedStop = withStop.events.at(1);
    expect(pairedStop).toBeDefined();
    expect(pairedStop?.pairing).toMatchObject({
      state: "paired",
      order: "out_of_order",
    });

    const duplicate = mergeLiveBatch(
      withStop,
      makeEventBatch([{ cursor: 2, record: { record: "process_event", ...stop(2) } }]),
    );
    expect(duplicate.events).toHaveLength(2);
  });

  it("keeps cursor expiry explicit and advances the cursor", () => {
    const batch = {
      ...makeEventBatch([]),
      cursor_expired: true,
      missed_records: 9,
      next_cursor: 12,
      latest_cursor: 12,
    };
    const state = mergeLiveBatch(EMPTY_LIVE_STATE, batch);
    expect(state.cursorExpiredCount).toBe(1);
    expect(state.missedRecords).toBe(9);
    expect(state.nextAfter).toBe(12);
  });

  it("resets the list when the sensor session changes", () => {
    const first = mergeLiveBatch(
      EMPTY_LIVE_STATE,
      makeEventBatch([{ cursor: 1, record: { record: "process_event", ...start(1) } }]),
    );
    const restarted = mergeLiveBatch(first, {
      ...makeEventBatch([]),
      session_id: "session-new",
      next_cursor: 0,
      latest_cursor: 0,
    });
    expect(restarted.events).toHaveLength(0);
    expect(restarted.sessionId).toBe("session-new");
    expect(restarted.restarted).toBe(true);
  });

  it("bounds the live list and reports trimmed records", () => {
    const records = Array.from({ length: MAX_LIVE_EVENTS + 5 }, (_, index) => ({
      cursor: index + 1,
      record: {
        record: "process_event" as const,
        ...start(index + 1, {
          event_id: `session-test:${(index + 1).toString(16).padStart(16, "0")}`,
        }),
      },
    }));
    const state = mergeLiveBatch(EMPTY_LIVE_STATE, makeEventBatch(records));
    expect(state.events).toHaveLength(MAX_LIVE_EVENTS);
    expect(state.trimmed).toBe(5);
  });

  it("applies a revision to an event already in the list", () => {
    const events = [start(1), stop(2)];
    const updated = applyPairingRevision(events, makePairingRevision());
    const paired = updated.at(1);
    expect(paired).toBeDefined();
    expect(paired?.pairing).toMatchObject({ state: "paired", order: "out_of_order" });
  });
});
