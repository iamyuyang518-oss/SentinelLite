import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

import {
  SentinelApiError,
  getUserFacingApiError,
  normalizeApiError,
  tauriApi,
} from "./api";

describe("tauri API adapter", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("uses only the registered application command names", async () => {
    mocks.invoke.mockResolvedValue({});

    await tauriApi.getProtectionOverview();
    await tauriApi.getNativeProtectionStatus();
    await tauriApi.getSensorStatus();
    await tauriApi.getSensorEvents(0, 64);
    await tauriApi.getStorageStatus();
    await tauriApi.queryStoredEvents({ limit: 10 });
    await tauriApi.getStoredEvent("session:0000000000000001");
    await tauriApi.listSensorSessions(5);
    await tauriApi.analyzeSession("session-test");
    await tauriApi.scanInstalledSoftware();
    await tauriApi.scanStartupItems();
    await tauriApi.scanAssetAssociations();
    await tauriApi.listServices();
    await tauriApi.listScheduledTasks();
    await tauriApi.listProcesses();

    expect(mocks.invoke.mock.calls.map(([command]) => command)).toEqual([
      "get_protection_overview",
      "get_native_protection_status",
      "get_sensor_status",
      "get_sensor_events",
      "get_storage_status",
      "query_stored_events",
      "get_stored_event",
      "list_sensor_sessions",
      "analyze_session",
      "scan_installed_software",
      "scan_startup_items",
      "scan_asset_associations",
      "list_services",
      "list_scheduled_tasks",
      "list_processes",
    ]);
  });

  it("passes event cursors and batch limits to the read-only endpoint", async () => {
    mocks.invoke.mockResolvedValue({ linkState: "running", batch: null, error: null });
    await tauriApi.getSensorEvents(9, 32);
    expect(mocks.invoke).toHaveBeenCalledWith("get_sensor_events", {
      after: 9,
      limit: 32,
    });
    await expect(tauriApi.getSensorEvents(-1, 32)).rejects.toMatchObject({
      code: "invalidCursor",
    });
    await expect(tauriApi.getSensorEvents(0, 129)).rejects.toMatchObject({
      code: "invalidEventLimit",
    });
  });

  it("passes a camelCase stored-event id to the backend bridge", async () => {
    mocks.invoke.mockResolvedValue(null);
    await tauriApi.getStoredEvent("session:0000000000000001");
    expect(mocks.invoke).toHaveBeenCalledWith("get_stored_event", {
      eventId: "session:0000000000000001",
    });
  });

  it("passes a camelCase session id to the analysis command", async () => {
    mocks.invoke.mockResolvedValue({});
    await tauriApi.analyzeSession("session-test");
    expect(mocks.invoke).toHaveBeenCalledWith("analyze_session", {
      sessionId: "session-test",
    });
    await expect(tauriApi.analyzeSession("")).rejects.toMatchObject({
      code: "invalidSessionId",
    });
  });

  it("returns the sensor status report without reinterpreting its fields", async () => {
    const report = {
      linkState: "running",
      sensor: {
        sensor_state: "active",
        permission_state: "granted",
        events_received: 2,
        events_processed: 2,
        events_dropped: 0,
        events_lost: 0,
        realtime_buffers_lost: 0,
        decode_errors: 0,
        last_event_at: null,
        uptime_ms: 1000,
        health_ok: true,
        last_error: null,
      },
      error: null,
    };
    mocks.invoke.mockResolvedValue(report);

    await expect(tauriApi.getSensorStatus()).resolves.toEqual(report);
    expect(mocks.invoke).toHaveBeenCalledWith("get_sensor_status", undefined);
  });

  it("requires both PID and creation-time identity before requesting affinity data", async () => {
    await expect(tauriApi.inspectProcessAffinity(0, "pidct1:00000000000000a1")).rejects.toMatchObject({
      code: "invalidPid",
    });
    await expect(tauriApi.inspectProcessAffinity(4242, "")).rejects.toMatchObject({
      code: "invalidIdentity",
    });
    await expect(tauriApi.inspectProcessAffinity(4242, "x".repeat(129))).rejects.toMatchObject({
      code: "invalidIdentity",
    });
    expect(mocks.invoke).not.toHaveBeenCalled();

    mocks.invoke.mockResolvedValue({});
    await tauriApi.inspectProcessAffinity(4242, "pidct1:00000000000000a1");
    expect(mocks.invoke).toHaveBeenCalledWith("inspect_process_affinity", {
      pid: 4242,
      identity: "pidct1:00000000000000a1",
    });
  });

  it("preserves a structured backend error without exposing it as an unknown failure", async () => {
    mocks.invoke.mockRejectedValue({
      code: "accessDenied",
      message: "无法读取该进程的处理器信息。",
      retryable: false,
    });

    await expect(tauriApi.listProcesses()).rejects.toMatchObject({
      code: "accessDenied",
      message: "无法读取该进程的处理器信息。",
      retryable: false,
    });
  });

  it("turns a bridge or unknown failure into an explicit backend error", () => {
    const normalized = normalizeApiError(new Error("window.__TAURI_INTERNALS__ is undefined"));

    expect(normalized).toBeInstanceOf(SentinelApiError);
    expect(normalized.code).toBe("backendUnavailable");
    expect(getUserFacingApiError(normalized)).toContain("无法连接");
    expect(getUserFacingApiError(normalized)).not.toContain("__TAURI_INTERNALS__");
  });

  it("provides an actionable message for malformed errors", () => {
    expect(getUserFacingApiError({ nope: true })).toBe("检查失败，未收到可识别的错误信息。");
  });
});
