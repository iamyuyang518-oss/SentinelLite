import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { makeEventBatch, makeProcessEvent, makeSensorEventsReport } from "../test/fixtures";
import type { SentinelApi } from "./api";
import type { SensorEventsReport } from "../types";
import { useLiveEvents } from "./useLiveEvents";

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

afterEach(() => {
  cleanup();
});

describe("useLiveEvents reconnect", () => {
  it("ignores an in-flight response from the old read after reconnect", async () => {
    const oldEvent = makeProcessEvent({
      event_id: "session-test:0000000000000001",
      sequence: 1,
      image_name: "C:\\Tools\\old-read.exe",
      pairing: { state: "waiting" },
    });
    const newEvent = makeProcessEvent({
      event_id: "session-test:0000000000000002",
      sequence: 2,
      image_name: "C:\\Tools\\new-read.exe",
      pairing: { state: "waiting" },
    });
    const first = deferred<SensorEventsReport>();
    const second = deferred<SensorEventsReport>();
    const getSensorEvents = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const api = { getSensorEvents } as unknown as SentinelApi;

    const { result } = renderHook(() => useLiveEvents(api));
    act(() => result.current.resume());

    await act(async () => {
      first.resolve(
        makeSensorEventsReport(
          makeEventBatch([
            { cursor: 1, record: { record: "process_event", ...oldEvent } },
          ]),
        ),
      );
      await Promise.resolve();
    });

    await waitFor(() => expect(getSensorEvents).toHaveBeenCalledTimes(2), {
      timeout: 3_000,
    });

    await act(async () => {
      second.resolve(
        makeSensorEventsReport(
          makeEventBatch([
            { cursor: 1, record: { record: "process_event", ...newEvent } },
          ]),
        ),
      );
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(result.current.state.raw.events).toHaveLength(1);
      expect(result.current.state.raw.events[0]?.event_id).toBe(newEvent.event_id);
    });
  });
});
