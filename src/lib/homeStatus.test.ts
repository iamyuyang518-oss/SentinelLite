import { describe, expect, it } from "vitest";

import type { SensorStatusDetails, SensorStatusReport } from "../types";
import { deriveHomeStatus } from "./homeStatus";

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

function makeReport(
  overrides: Partial<SensorStatusReport> = {},
  sensorOverrides: Partial<SensorStatusDetails> = {},
): SensorStatusReport {
  return {
    linkState: "running",
    sensor: makeSensor(sensorOverrides),
    error: null,
    ...overrides,
  };
}

describe("deriveHomeStatus", () => {
  it("stays in loading before the first status query settles", () => {
    const status = deriveHomeStatus(null, null, "loading");

    expect(status.phase).toBe("loading");
    expect(status.statusLabel).toBe("正在确认");
    expect(status.title).toBe("正在确认保护状态。");
    expect(status.tone).toBe("unknown");
    expect(status.showEnable).toBe(false);
    expect(status.attention).toBeNull();
  });

  it("treats a stopped sensor as not enabled and keeps connection failures separate", () => {
    const stopped = deriveHomeStatus(
      makeReport({ linkState: "stopped", sensor: null }),
      null,
      "ready",
    );

    expect(stopped.phase).toBe("notEnabled");
    expect(stopped.statusLabel).toBe("尚未启用");
    expect(stopped.title).toBe("让电脑更安全。");
    expect(stopped.description).toBe(
      "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
    );
    expect(stopped.showEnable).toBe(true);
    expect(stopped.tone).toBe("idle");

    const disconnected = deriveHomeStatus(
      null,
      "无法连接 SentinelLite 本机服务。",
      "error",
    );

    expect(disconnected.phase).toBe("disconnected");
    expect(disconnected.statusLabel).toBe("未连接");
    expect(disconnected.title).toBe("无法确认保护状态。");
    expect(disconnected.showEnable).toBe(false);
    expect(disconnected.attention?.message).toBe(
      "无法连接 SentinelLite 本机服务。",
    );
  });

  it("does not treat a stopped or unavailable sensor as active", () => {
    for (const sensorState of ["stopped", "unavailable", "not_started"] as const) {
      const status = deriveHomeStatus(
        makeReport({}, { sensor_state: sensorState }),
        null,
        "ready",
      );

      expect(status.phase).toBe("notEnabled");
      expect(status.showEnable).toBe(true);
    }
  });

  it("reports a connected sensor that is still starting", () => {
    const status = deriveHomeStatus(
      makeReport({}, { sensor_state: "starting", health_ok: false }),
      null,
      "ready",
    );

    expect(status.phase).toBe("starting");
    expect(status.statusLabel).toBe("正在启动");
    expect(status.title).toBe("监测正在启动。");
    expect(status.showEnable).toBe(false);
  });

  it("reports a healthy active sensor as active", () => {
    const status = deriveHomeStatus(makeReport(), null, "ready");

    expect(status.phase).toBe("active");
    expect(status.statusLabel).toBe("监测正常");
    expect(status.title).toBe("实时监测正在运行。");
    expect(status.tone).toBe("success");
    expect(status.attention).toBeNull();
  });

  it("moves to attention when health is not proven and keeps the reason visible", () => {
    const status = deriveHomeStatus(
      makeReport(
        { linkState: "degraded" },
        { health_ok: false, last_error: "sensor health not proven" },
      ),
      null,
      "ready",
    );

    expect(status.phase).toBe("attention");
    expect(status.statusLabel).toBe("部分可用");
    expect(status.title).toBe("监测需要关注。");
    expect(status.tone).toBe("warning");
    expect(status.attention?.message).toContain("传感器尚未证明健康");
  });

  it("treats a degraded link as partial availability even when the sensor claims health", () => {
    const status = deriveHomeStatus(
      makeReport({ linkState: "degraded" }),
      null,
      "ready",
    );

    expect(status.phase).toBe("attention");
    expect(status.statusLabel).toBe("部分可用");
  });

  it("keeps an explicit sensor failure separate from a query failure", () => {
    const status = deriveHomeStatus(
      makeReport(
        { linkState: "degraded" },
        {
          sensor_state: "failed",
          health_ok: false,
          last_error: "decode failure",
        },
      ),
      null,
      "ready",
    );

    expect(status.phase).toBe("failed");
    expect(status.statusLabel).toBe("启动失败");
    expect(status.title).toBe("监测启动失败。");
    expect(status.tone).toBe("danger");
    expect(status.attention?.message).toContain("decode failure");
  });

  it("keeps a linked but unavailable state unknown instead of healthy", () => {
    const status = deriveHomeStatus(
      makeReport({ linkState: "running", sensor: null }),
      null,
      "ready",
    );

    expect(status.phase).toBe("unknown");
    expect(status.statusLabel).toBe("状态未知");
    expect(status.title).toBe("保护状态未知。");
    expect(status.tone).toBe("unknown");
    expect(status.showEnable).toBe(false);
  });
});
