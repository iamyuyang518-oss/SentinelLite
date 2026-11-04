import { describe, expect, it } from "vitest";

import type { SensorStatusDetails, SensorStatusReport } from "../types";
import {
  deriveSensorView,
  formatEventCount,
  formatUptime,
  linkStateLabel,
  permissionStateLabel,
} from "./sensorStatus";

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

describe("deriveSensorView", () => {
  it("shows a running, healthy sensor as active and healthy", () => {
    const view = deriveSensorView(makeReport());

    expect(view.state).toBe("active");
    expect(view.stateLabel).toBe("运行中");
    expect(view.health).toBe("healthy");
    expect(view.healthLabel).toBe("健康");
    expect(view.linkLabel).toBe("已连接");
    expect(view.permissionLabel).toBe("已授予");
    expect(view.metrics.received).toEqual({ value: "21", unknown: false });
    expect(view.metrics.processed).toEqual({ value: "20", unknown: false });
    expect(view.metrics.lost).toEqual({ value: "0", unknown: false });
    expect(view.metrics.uptime).toEqual({ value: "1 小时 2 分", unknown: false });
    expect(view.lastEventLabel).not.toBe("—");
    expect(view.droppedLabel).toBe("0");
    expect(view.decodeErrorsLabel).toBe("0");
  });

  it("keeps running and healthy independent when the sensor reports unhealthy", () => {
    const view = deriveSensorView(
      makeReport({ linkState: "degraded" }, { health_ok: false }),
    );

    expect(view.state).toBe("active");
    expect(view.stateLabel).toBe("运行中");
    expect(view.health).toBe("unhealthy");
    expect(view.healthLabel).toBe("健康异常");
  });

  it("reports an explicit sensor failure as failed, not active", () => {
    const view = deriveSensorView(
      makeReport(
        { linkState: "degraded" },
        { sensor_state: "failed", health_ok: false, last_error: "decode failure" },
      ),
    );

    expect(view.state).toBe("failed");
    expect(view.stateLabel).toBe("已失败");
    expect(view.health).toBe("unhealthy");
    expect(view.diagnostic).toContain("decode failure");
  });

  it("maps a missing sensor to unavailable without a healthy claim", () => {
    const view = deriveSensorView({
      linkState: "stopped",
      sensor: null,
      error: "sensor status endpoint does not exist",
    });

    expect(view.state).toBe("unavailable");
    expect(view.health).toBe("unknown");
    expect(view.healthLabel).toBe("健康未知");
    expect(view.linkLabel).toBe("未连接");
    expect(view.permissionLabel).toBe("未知");
    expect(view.metrics.received.value).toBe("—");
    expect(view.metrics.processed.value).toBe("—");
    expect(view.metrics.lost.value).toBe("—");
    expect(view.metrics.uptime.value).toBe("—");
    expect(view.hasMetrics).toBe(false);
  });

  it("does not call a degraded link healthy when health_ok is true", () => {
    const view = deriveSensorView(
      makeReport({ linkState: "degraded" }, { health_ok: true }),
    );

    expect(view.state).toBe("active");
    expect(view.health).toBe("unknown");
    expect(view.healthLabel).toBe("健康未知");
  });

  it("does not trust sensor counts on a communication failure", () => {
    const view = deriveSensorView(makeReport({ linkState: "communicationFailed" }));

    expect(view.state).toBe("unavailable");
    expect(view.health).toBe("unknown");
    expect(view.hasMetrics).toBe(false);
    expect(view.metrics.received.value).toBe("—");
    expect(view.explanation).toContain("通信");
  });

  it("maps a rejected command to unavailable with an explicit diagnostic", () => {
    const view = deriveSensorView(null, "无法连接 SentinelLite 本机服务。");

    expect(view.state).toBe("unavailable");
    expect(view.stateLabel).toBe("不可用");
    expect(view.health).toBe("unknown");
    expect(view.linkLabel).toBe("查询失败");
    expect(view.diagnostic).toBe("无法连接 SentinelLite 本机服务。");
    expect(view.hasMetrics).toBe(false);
  });

  it("keeps the unknown state before the first valid result", () => {
    const view = deriveSensorView(null);

    expect(view.state).toBe("unknown");
    expect(view.stateLabel).toBe("状态未知");
    expect(view.health).toBe("unknown");
    expect(view.linkLabel).toBe("未知");
    expect(view.metrics.received.value).toBe("—");
  });

  it("shows an unavailable loss count as unknown, not zero", () => {
    const view = deriveSensorView(makeReport({}, { events_lost: null }));

    expect(view.metrics.lost).toEqual({ value: "未知", unknown: true });
  });

  it("translates the known 'health not proven' diagnostic", () => {
    const view = deriveSensorView(
      makeReport(
        { linkState: "degraded" },
        { events_received: 0, events_processed: 0, health_ok: false, last_error: "sensor health not proven" },
      ),
    );

    expect(view.health).toBe("unhealthy");
    expect(view.diagnostic).toContain("尚未证明健康");
  });
});

describe("sensor presentation helpers", () => {
  it("formats event counts with grouping", () => {
    expect(formatEventCount(0)).toBe("0");
    expect(formatEventCount(1_234_567)).toBe("1,234,567");
  });

  it("formats uptime without inventing precision", () => {
    expect(formatUptime(0)).toBe("0 秒");
    expect(formatUptime(65_000)).toBe("1 分 5 秒");
    expect(formatUptime(3_723_000)).toBe("1 小时 2 分");
    expect(formatUptime(90_061_000)).toBe("1 天 1 小时");
  });

  it("labels permission states", () => {
    expect(permissionStateLabel("granted")).toBe("已授予");
    expect(permissionStateLabel("limited")).toBe("受限");
    expect(permissionStateLabel("denied")).toBe("已拒绝");
    expect(permissionStateLabel("unknown")).toBe("未知");
    expect(permissionStateLabel(null)).toBe("未知");
  });

  it("labels link states without claiming more than the backend reports", () => {
    expect(linkStateLabel("running")).toBe("已连接");
    expect(linkStateLabel("degraded")).toBe("已连接");
    expect(linkStateLabel("stopped")).toBe("未连接");
    expect(linkStateLabel("unavailable")).toBe("不可用");
    expect(linkStateLabel("communicationFailed")).toBe("通信失败");
    expect(linkStateLabel(null)).toBe("未知");
  });
});
