import { invoke } from "@tauri-apps/api/core";

import type {
  AffinityInspection,
  AssetAssociationResult,
  EventDetailWire,
  EventQueryResultWire,
  NativeProtectionStatus,
  ProcessListResult,
  ProtectionOverview,
  ScheduledTaskInventoryResult,
  SensorEventsReport,
  SensorStatusReport,
  ServiceInventoryResult,
  SessionAnalysisWire,
  SessionRecordWire,
  SoftwareScanResult,
  StartupScanResult,
  StorageRuntimeReport,
  StoredEventFilter,
} from "../types";

export class SentinelApiError extends Error {
  readonly code: string;
  readonly retryable: boolean;

  constructor(code: string, message: string, retryable: boolean) {
    super(message);
    this.name = "SentinelApiError";
    this.code = code;
    this.retryable = retryable;
  }
}

export interface SentinelApi {
  getProtectionOverview(): Promise<ProtectionOverview>;
  getNativeProtectionStatus(): Promise<NativeProtectionStatus>;
  getSensorStatus(): Promise<SensorStatusReport>;
  getSensorEvents(after: number, limit: number): Promise<SensorEventsReport>;
  getStorageStatus(): Promise<StorageRuntimeReport>;
  queryStoredEvents(filter: StoredEventFilter): Promise<EventQueryResultWire>;
  getStoredEvent(eventId: string): Promise<EventDetailWire | null>;
  listSensorSessions(limit: number): Promise<SessionRecordWire[]>;
  analyzeSession(sessionId: string): Promise<SessionAnalysisWire>;
  scanInstalledSoftware(): Promise<SoftwareScanResult>;
  scanStartupItems(): Promise<StartupScanResult>;
  scanAssetAssociations(): Promise<AssetAssociationResult>;
  listServices(): Promise<ServiceInventoryResult>;
  listScheduledTasks(): Promise<ScheduledTaskInventoryResult>;
  listProcesses(): Promise<ProcessListResult>;
  inspectProcessAffinity(pid: number, identity: string): Promise<AffinityInspection>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * Converts backend or bridge failures into one explicit, UI-safe error shape.
 *
 * Raw bridge errors can contain environment details, so they are not shown to the user.
 */
export function normalizeApiError(error: unknown): SentinelApiError {
  if (error instanceof SentinelApiError) {
    return error;
  }

  if (isRecord(error) && typeof error.code === "string" && typeof error.message === "string") {
    return new SentinelApiError(error.code, error.message, error.retryable === true);
  }

  return new SentinelApiError(
    "backendUnavailable",
    "无法连接 SentinelLite 本机服务。请从桌面应用启动，或重新打开应用后重试。",
    false,
  );
}

export function getUserFacingApiError(error: unknown): string {
  if (error instanceof SentinelApiError) {
    return error.message.trim().length > 0
      ? error.message
      : "检查失败，未收到可识别的错误信息。";
  }

  return "检查失败，未收到可识别的错误信息。";
}

function assertValidPid(pid: number): void {
  if (!Number.isInteger(pid) || pid <= 0 || pid > 0xffff_ffff) {
    throw new SentinelApiError("invalidPid", "进程 ID 无效。请刷新进程列表后重试。", false);
  }
}

function assertValidProcessIdentity(identity: string): void {
  if (identity.length === 0 || identity.length > 128) {
    throw new SentinelApiError(
      "invalidIdentity",
      "进程身份令牌无效。请刷新进程列表后重试。",
      false,
    );
  }
}

async function invokeReadOnly<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeApiError(error);
  }
}

export const tauriApi: SentinelApi = {
  getProtectionOverview() {
    return invokeReadOnly<ProtectionOverview>("get_protection_overview");
  },

  getNativeProtectionStatus() {
    return invokeReadOnly<NativeProtectionStatus>("get_native_protection_status");
  },

  getSensorStatus() {
    return invokeReadOnly<SensorStatusReport>("get_sensor_status");
  },

  async getSensorEvents(after, limit) {
    if (!Number.isSafeInteger(after) || after < 0) {
      throw new SentinelApiError("invalidCursor", "事件游标无效。", false);
    }
    if (!Number.isInteger(limit) || limit < 1 || limit > 128) {
      throw new SentinelApiError("invalidEventLimit", "事件批量大小无效。", false);
    }
    return invokeReadOnly<SensorEventsReport>("get_sensor_events", { after, limit });
  },

  getStorageStatus() {
    return invokeReadOnly<StorageRuntimeReport>("get_storage_status");
  },

  queryStoredEvents(filter) {
    return invokeReadOnly<EventQueryResultWire>("query_stored_events", { filter });
  },

  async getStoredEvent(eventId) {
    if (eventId.length === 0 || eventId.length > 128) {
      throw new SentinelApiError("invalidEventId", "事件 ID 无效。", false);
    }
    return invokeReadOnly<EventDetailWire | null>("get_stored_event", { eventId });
  },

  async listSensorSessions(limit) {
    if (!Number.isInteger(limit) || limit < 1 || limit > 100) {
      throw new SentinelApiError("invalidSessionLimit", "会话数量无效。", false);
    }
    return invokeReadOnly<SessionRecordWire[]>("list_sensor_sessions", { limit });
  },

  async analyzeSession(sessionId) {
    if (sessionId.length === 0 || sessionId.length > 128) {
      throw new SentinelApiError("invalidSessionId", "传感器会话 ID 无效。", false);
    }
    return invokeReadOnly<SessionAnalysisWire>("analyze_session", { sessionId });
  },

  scanInstalledSoftware() {
    return invokeReadOnly<SoftwareScanResult>("scan_installed_software");
  },

  scanStartupItems() {
    return invokeReadOnly<StartupScanResult>("scan_startup_items");
  },

  scanAssetAssociations() {
    return invokeReadOnly<AssetAssociationResult>("scan_asset_associations");
  },

  listServices() {
    return invokeReadOnly<ServiceInventoryResult>("list_services");
  },

  listScheduledTasks() {
    return invokeReadOnly<ScheduledTaskInventoryResult>("list_scheduled_tasks");
  },

  listProcesses() {
    return invokeReadOnly<ProcessListResult>("list_processes");
  },

  async inspectProcessAffinity(pid: number, identity: string) {
    assertValidPid(pid);
    assertValidProcessIdentity(identity);
    return invokeReadOnly<AffinityInspection>("inspect_process_affinity", { pid, identity });
  },
};
