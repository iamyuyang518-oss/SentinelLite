import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { SentinelApi } from "./api";
import { SentinelApiError } from "./api";
import type { SensorStatusDetails, SensorStatusReport } from "../types";
import { SENSOR_STATUS_REFRESH_MS, useSensorStatus } from "./useSensorStatus";

function makeSensor(overrides: Partial<SensorStatusDetails> = {}): SensorStatusDetails {
  return {
    sensor_state: "active",
    permission_state: "granted",
    events_received: 21,
    events_processed: 20,
    events_dropped: 0,
    events_lost: 0,
    realtime_buffers_lost: 0,
    decode_errors: 0,
    last_event_at: "2026-10-29T02:30:00Z",
    uptime_ms: 3_723_000,
    health_ok: true,
    last_error: null,
    ...overrides,
  };
}

const healthyReport: SensorStatusReport = {
  linkState: "running",
  sensor: makeSensor(),
  error: null,
};

const stoppedReport: SensorStatusReport = {
  linkState: "stopped",
  sensor: null,
  error: "sensor status endpoint does not exist",
};

function makeApi(): SentinelApi {
  return {
    getProtectionOverview: vi.fn(),
    getNativeProtectionStatus: vi.fn(),
    getSensorStatus: vi.fn(),
    getSensorEvents: vi.fn(),
    getStorageStatus: vi.fn(),
    queryStoredEvents: vi.fn(),
    getStoredEvent: vi.fn(),
    listSensorSessions: vi.fn(),
    analyzeSession: vi.fn(),
    scanInstalledSoftware: vi.fn(),
    scanStartupItems: vi.fn(),
    scanAssetAssociations: vi.fn(),
    listServices: vi.fn(),
    listScheduledTasks: vi.fn(),
    listProcesses: vi.fn(),
    inspectProcessAffinity: vi.fn(),
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("useSensorStatus", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("queries immediately and exposes a ready report", async () => {
    const api = makeApi();
    vi.mocked(api.getSensorStatus).mockResolvedValue(healthyReport);

    const { result } = renderHook(() => useSensorStatus(api));
    await flush();

    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);
    expect(result.current.state.phase).toBe("ready");
    expect(result.current.state.report).toEqual(healthyReport);
    expect(result.current.state.lastUpdatedAt).toEqual(expect.any(String));
  });

  it("polls every interval without overlapping an in-flight query", async () => {
    const api = makeApi();
    const first = deferred<SensorStatusReport>();
    const second = deferred<SensorStatusReport>();
    vi.mocked(api.getSensorStatus)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const { result } = renderHook(() => useSensorStatus(api));
    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SENSOR_STATUS_REFRESH_MS);
    });
    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      first.resolve(healthyReport);
      await first.promise;
    });
    await flush();
    expect(result.current.state.phase).toBe("ready");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SENSOR_STATUS_REFRESH_MS);
    });
    expect(api.getSensorStatus).toHaveBeenCalledTimes(2);
  });

  it("stops polling after unmount", async () => {
    const api = makeApi();
    vi.mocked(api.getSensorStatus).mockResolvedValue(healthyReport);

    const { unmount } = renderHook(() => useSensorStatus(api));
    await flush();
    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);

    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SENSOR_STATUS_REFRESH_MS * 3);
    });

    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);
  });

  it("starts a fresh query after a remount", async () => {
    const api = makeApi();
    vi.mocked(api.getSensorStatus).mockResolvedValue(healthyReport);

    const first = renderHook(() => useSensorStatus(api));
    await flush();
    expect(first.result.current.state.phase).toBe("ready");
    first.unmount();

    const second = renderHook(() => useSensorStatus(api));
    expect(second.result.current.state.phase).toBe("loading");
    await flush();

    expect(api.getSensorStatus).toHaveBeenCalledTimes(2);
    expect(second.result.current.state.phase).toBe("ready");
  });

  it("drops a previously healthy report when the next query fails", async () => {
    const api = makeApi();
    vi.mocked(api.getSensorStatus)
      .mockResolvedValueOnce(healthyReport)
      .mockRejectedValueOnce(
        new SentinelApiError(
          "backendUnavailable",
          "无法连接 SentinelLite 本机服务。",
          false,
        ),
      );

    const { result } = renderHook(() => useSensorStatus(api));
    await flush();
    expect(result.current.state.phase).toBe("ready");
    expect(result.current.state.report).toEqual(healthyReport);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SENSOR_STATUS_REFRESH_MS);
    });
    await flush();

    expect(result.current.state.phase).toBe("error");
    expect(result.current.state.report).toBeNull();
    expect(result.current.state.error).toContain("无法连接");
  });

  it("ignores an older response after the API identity changes", async () => {
    const firstApi = makeApi();
    const secondApi = makeApi();
    const first = deferred<SensorStatusReport>();
    const second = deferred<SensorStatusReport>();
    vi.mocked(firstApi.getSensorStatus).mockReturnValueOnce(first.promise);
    vi.mocked(secondApi.getSensorStatus).mockReturnValueOnce(second.promise);

    const { result, rerender } = renderHook(
      ({ api }: { api: SentinelApi }) => useSensorStatus(api),
      { initialProps: { api: firstApi } },
    );
    expect(firstApi.getSensorStatus).toHaveBeenCalledTimes(1);

    rerender({ api: secondApi });
    expect(secondApi.getSensorStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      second.resolve(stoppedReport);
      await second.promise;
    });
    await flush();
    expect(result.current.state.report).toEqual(stoppedReport);

    await act(async () => {
      first.resolve(healthyReport);
      await first.promise;
    });
    await flush();

    expect(result.current.state.phase).toBe("ready");
    expect(result.current.state.report).toEqual(stoppedReport);
  });

  it("supports a manual refresh without waiting for the interval", async () => {
    const api = makeApi();
    vi.mocked(api.getSensorStatus).mockResolvedValue(healthyReport);

    const { result } = renderHook(() => useSensorStatus(api));
    await flush();
    expect(api.getSensorStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.refresh();
    });

    expect(api.getSensorStatus).toHaveBeenCalledTimes(2);
    expect(result.current.state.phase).toBe("ready");
  });
});
