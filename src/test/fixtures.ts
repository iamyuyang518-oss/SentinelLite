import type {
  AffinityInspection,
  AssetAssociationResult,
  AssetRelation,
  AssociationSourceStatus,
  EventBatchWire,
  EventDetailWire,
  EventEnvelopeWire,
  Evidence,
  InstalledSoftware,
  SoftwareAssociationSummary,
  SoftwareSourceStatus,
  NativeProtectionStatus,
  PairingRevisionWire,
  ScheduledTaskInventoryResult,
  ScheduledTaskSummary,
  ServiceInventoryResult,
  ServiceSummary,
  ProcessEventWire,
  ProcessListResult,
  ProtectionOverview,
  RiskAssessment,
  SensorEventsReport,
  SensorStatusDetails,
  SensorStatusReport,
  SessionEndStatsWire,
  SessionEndWire,
  SessionRecordWire,
  SoftwareScanResult,
  StartupItem,
  StartupScanResult,
  StartupSourceStatus,
  StorageRuntimeReport,
  StoredEventWire,
} from "../types";

const SAMPLE_TIME = "2026-10-29T02:30:00Z";

function makeEvidence(overrides: Partial<Evidence> = {}): Evidence {
  return {
    code: "TEST-EVIDENCE",
    statement: "这是仅用于测试的证据文本。",
    source: "fictional test fixture",
    ...overrides,
  };
}

export function makeRisk(overrides: Partial<RiskAssessment> = {}): RiskAssessment {
  return {
    scope: "softwareMetadata",
    level: "noClearIssue",
    confidence: "low",
    ruleIds: ["TEST-000"],
    summary: "本次虚构检查范围内未发现明确异常。",
    evidence: [makeEvidence()],
    blindSpots: ["虚构数据不包含文件内容与行为。"],
    recommendation: "保持人工核实，不依据单一结果做安全结论。",
    ...overrides,
  };
}

export function makeSensorStatus(
  overrides: Partial<SensorStatusDetails> = {},
): SensorStatusDetails {
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

export function makeSensorStatusReport(
  overrides: Partial<SensorStatusReport> = {},
): SensorStatusReport {
  return {
    linkState: "running",
    sensor: makeSensorStatus(),
    error: null,
    ...overrides,
  };
}

export function makeProcessEvent(
  overrides: Partial<ProcessEventWire> = {},
): ProcessEventWire {
  return {
    event_id: "session-test:0000000000000001",
    sequence: 1,
    session_id: "session-test",
    event_type: "started",
    source: "microsoft_windows_kernel_process",
    identity: { pid: 4242, create_time: 133_000_000_000_000_000 },
    identity_certainty: "exact",
    parent_pid: 4,
    image_name: "C:\\Windows\\System32\\notepad.exe",
    etw: {
      event_id: 1,
      version: 4,
      provider_sequence: 7,
      timestamp_100ns: 133_000_000_000_000_000,
      occurred_at: "2026-10-29T02:30:00.000Z",
    },
    process_started_at: "2026-10-29T02:30:00.000Z",
    received_at: "2026-10-29T02:30:00.100Z",
    normalized_at: "2026-10-29T02:30:00.200Z",
    completeness: {
      identity_certain: true,
      event_time_known: true,
      received_time_known: true,
      normalized_time_known: true,
      parent_pid_available: true,
      image_name: "present",
      etw_sequence_available: true,
      issues: [],
    },
    pairing: { state: "waiting" },
    ...overrides,
  };
}

export function makeEventBatch(records: EventEnvelopeWire[]): EventBatchWire {
  const last = records.at(-1)?.cursor ?? 0;
  return {
    session_id: "session-test",
    oldest_cursor: records[0]?.cursor ?? 0,
    latest_cursor: last,
    next_cursor: last,
    cursor_expired: false,
    missed_records: 0,
    cursor_ahead: false,
    evictions: 0,
    session_closed: false,
    has_more: false,
    records,
  };
}

export function makeSensorEventsReport(
  batch: EventBatchWire | null,
  overrides: Partial<SensorEventsReport> = {},
): SensorEventsReport {
  return {
    linkState: "running",
    batch,
    error: null,
    ...overrides,
  };
}

export function makeStoredEvent(
  overrides: Partial<StoredEventWire> = {},
): StoredEventWire {
  const record = overrides.record ?? makeProcessEvent();
  return {
    event_id: record.event_id,
    session_id: record.session_id,
    sequence: record.sequence,
    event_type: record.event_type,
    pid: record.identity.pid,
    create_time: record.identity.create_time,
    image_name: record.image_name,
    event_time: record.etw.occurred_at,
    occurred_at: record.etw.occurred_at,
    received_at: record.received_at,
    normalized_at: record.normalized_at,
    pairing_state: "waiting",
    pairing_order: null,
    paired_event_id: null,
    record,
    ...overrides,
  };
}

export function makePairingRevision(
  overrides: Partial<PairingRevisionWire> = {},
): PairingRevisionWire {
  return {
    session_id: "session-test",
    stop_event_id: "session-test:0000000000000002",
    start_event_id: "session-test:0000000000000001",
    sequence_match: true,
    reason: "start arrived after an orphan stop",
    normalized_at: "2026-10-29T02:30:01.000Z",
    ...overrides,
  };
}

export function makeEventDetail(
  overrides: Partial<EventDetailWire> = {},
): EventDetailWire {
  return {
    event: makeStoredEvent(),
    paired_event: null,
    revisions: [],
    session_end: null,
    ...overrides,
  };
}

export function makeSessionEndStats(
  overrides: Partial<SessionEndStatsWire> = {},
): SessionEndStatsWire {
  return {
    events_lost: 0,
    realtime_buffers_lost: 0,
    pipeline_capacity: 4096,
    pipeline_received: 4,
    pipeline_enqueued: 4,
    pipeline_consumed: 4,
    pipeline_normalized: 3,
    pipeline_invalid: 1,
    pipeline_dropped_queue_full: 0,
    pipeline_dropped_consumer_gone: 0,
    pipeline_filtered_ignored: 0,
    pipeline_backlog: 0,
    pipeline_peak_backlog: 2,
    pipeline_backlog_at_shutdown_timeout: null,
    pipeline_shutdown_timed_out: false,
    pipeline_sink_errors: 0,
    ...overrides,
  };
}

export function makeSessionEnd(
  overrides: Partial<SessionEndWire> = {},
): SessionEndWire {
  return {
    session_id: "session-test",
    ended_at: "2026-10-29T02:31:00.000Z",
    reason: "completed",
    error: null,
    open_start_events: [],
    pairing: {
      paired: 1,
      unpaired_starts: 0,
      orphan_stops: 0,
      duplicate_starts: 0,
      duplicate_stops: 0,
      pid_reuse_detected: 0,
      sequence_mismatches: 0,
      start_overflow: 0,
      orphan_stop_overflow: 0,
      identity_mismatched_stops: 0,
      out_of_order_pairs: 0,
      indeterminate_identities: 0,
      closed_identity_evictions: 0,
      pending_orphan_stops: 0,
      starts_seen: 1,
      stops_seen: 1,
      observed_start_image_names: ["notepad.exe"],
      observed_start_image_names_truncated: false,
    },
    final_stats: makeSessionEndStats(),
    ...overrides,
  };
}

export function makeSessionRecord(
  overrides: Partial<SessionRecordWire> = {},
): SessionRecordWire {
  return {
    session_id: "session-test",
    first_seen_at: "2026-10-29T02:30:00.000Z",
    last_seen_at: "2026-10-29T02:31:00.000Z",
    last_cursor: 3,
    closed: true,
    end_reason: "completed",
    ended_at: "2026-10-29T02:31:00.000Z",
    end_error: null,
    event_count: 3,
    has_session_end: true,
    ...overrides,
  };
}

export function makeStorageReport(
  overrides: Partial<StorageRuntimeReport> = {},
): StorageRuntimeReport {
  return {
    state: "ready",
    initError: null,
    ingestionError: null,
    storage: {
      path: "C:\\Users\\test\\AppData\\Local\\app.sentinellite.desktop\\SentinelLite\\events.db",
      schema_version: 1,
      event_count: 3,
      invalid_count: 0,
      revision_count: 1,
      gap_count: 0,
      session_count: 1,
      oldest_event_at: "2026-10-29T02:30:00.000Z",
      newest_event_at: "2026-10-29T02:31:00.000Z",
      database_bytes: 24_576,
      retention: { max_age_days: 30, max_events: 1_000_000, max_bytes: 536_870_912 },
      last_ingest_at: "2026-10-29T02:31:01.000Z",
      last_retention_at: "2026-10-29T02:30:00.000Z",
    },
    ingestion: {
      state: "running",
      source_available: true,
      last_error: null,
      last_success_at: "2026-10-29T02:31:01.000Z",
      poll_count: 10,
      batch_count: 3,
      record_count: 3,
      sessions_observed: 1,
      session_id: "session-test",
      session_closed: false,
      last_cursor: 3,
      last_gap_records: 0,
    },
    lastError: null,
    ...overrides,
  };
}

export function makeNativeProtectionStatus(
  overrides: Partial<NativeProtectionStatus> = {},
): NativeProtectionStatus {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "complete",
    defender: {
      state: "observed",
      antivirusEnabled: true,
      realTimeProtectionEnabled: true,
      amServiceEnabled: true,
      signatureVersion: "1.2.3.4",
      signatureAgeDays: 0,
      issues: [],
    },
    firewall: {
      state: "observed",
      profiles: [
        { profile: "domain", enabled: true },
        { profile: "private", enabled: true },
        { profile: "public", enabled: true },
      ],
      issues: [],
    },
    limitations: ["只读观测，不修改原生防护。"],
    ...overrides,
  };
}

export function makeService(
  overrides: Partial<ServiceSummary> = {},
): ServiceSummary {
  return {
    id: "service:ExampleSvc",
    name: "ExampleSvc",
    displayName: "Example Service",
    state: "running",
    startType: "auto",
    serviceTypes: ["ownProcess"],
    account: "LocalSystem",
    executablePath: "C:\\Windows\\System32\\example.exe",
    observation: "observed",
    issues: [],
    ...overrides,
  };
}

export function makeServiceInventory(
  overrides: Partial<ServiceInventoryResult> = {},
): ServiceInventoryResult {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "observed",
    items: [makeService()],
    issues: [],
    limitations: ["只读服务快照，不修改服务。"],
    truncated: false,
    ...overrides,
  };
}

export function makeScheduledTask(
  overrides: Partial<ScheduledTaskSummary> = {},
): ScheduledTaskSummary {
  return {
    id: "task:\\ExampleTask",
    name: "ExampleTask",
    path: "\\ExampleTask",
    state: "ready",
    enabled: true,
    author: "Example Author",
    runAs: "SYSTEM",
    actions: [{ actionType: "exec", executablePath: "C:\\Windows\\System32\\example.exe" }],
    actionsTruncated: false,
    observation: "observed",
    issues: [],
    ...overrides,
  };
}

export function makeScheduledTaskInventory(
  overrides: Partial<ScheduledTaskInventoryResult> = {},
): ScheduledTaskInventoryResult {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "observed",
    items: [makeScheduledTask()],
    issues: [],
    limitations: ["只读计划任务快照，不运行任务。"],
    truncated: false,
    ...overrides,
  };
}

export function makeOverview(overrides: Partial<ProtectionOverview> = {}): ProtectionOverview {
  return {
    generatedAt: SAMPLE_TIME,
    appVersion: "0.1.0-test",
    overallState: "unknown",
    headline: "无法确认整体保护状态",
    explanation: "进程传感器只覆盖进程 start/stop；原生防护状态是用户主动触发的只读快照；没有告警队列、自动处置或完整覆盖，不能视为持续保护。",
    sensors: [
      {
        id: "realtime-process",
        name: "实时进程监控",
        state: "notImplemented",
        description: "记录新进程与父子关系。",
        coverage: [],
        limitations: ["第一阶段未接入 ETW。"],
      },
      {
        id: "filesystem",
        name: "文件变化监控",
        state: "notImplemented",
        description: "记录创建、修改和删除。",
        coverage: [],
        limitations: ["第一阶段未接入文件系统事件。"],
      },
      {
        id: "persistence",
        name: "持久化变化监控",
        state: "notImplemented",
        description: "观察启动项、服务与计划任务变化。",
        coverage: [],
        limitations: ["当前只能手动读取部分启动项。"],
      },
      {
        id: "native-protection",
        name: "原生防护状态",
        state: "notChecked",
        description: "按需只读读取 Defender 与防火墙状态。",
        coverage: [],
        limitations: ["未查询、查询失败或字段缺失时保持未知。"],
      },
    ],
    readOnlyChecks: [
      {
        id: "softwareInventory",
        name: "软件与组件盘点",
        description: "读取卸载注册信息，不执行卸载。",
      },
      {
        id: "startupItems",
        name: "启动项检查",
        description: "读取 Run/RunOnce 与启动文件夹，不修改启动项。",
      },
      {
        id: "affinityInspection",
        name: "进程与 CPU 亲和性",
        description: "只读观察处理器范围、签名与 CPU 时间。",
      },
    ],
    deepDefense: {
      state: "notImplemented",
      explanation: "深度防御模式尚未实现，不会显示虚假的启用状态。",
    },
    boundaries: [
      "不提供实时拦截、病毒清除或自动卸载。",
      "无法读取的数据会显示为未知或证据不足。",
    ],
    ...overrides,
  };
}

export function makeInstalledSoftware(
  overrides: Partial<InstalledSoftware> = {},
): InstalledSoftware {
  return {
    id: "test:software:1",
    displayName: "虚构样本应用",
    version: "1.2.3-test",
    versionState: "present",
    publisher: "Example Publisher",
    publisherState: "present",
    installLocation: "C:\\Program Files\\Fictional Sample",
    installLocationState: "present",
    installLocationValueState: "present",
    installDate: "2026-01-15",
    installDateState: "present",
    displayIconPath: "C:\\Program Files\\Fictional Sample\\app.exe",
    displayIconState: "present",
    windowsInstaller: 0,
    windowsInstallerState: "present",
    uninstallCommandPresent: true,
    quietUninstallCommandPresent: false,
    scope: "machine",
    registryView: "registry64",
    registryKey: "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\FictionalSample",
    systemComponent: null,
    systemComponentState: "missing",
    noRemove: null,
    noRemoveState: "missing",
    parentKeyName: null,
    parentKeyNameState: "missing",
    releaseType: null,
    releaseTypeState: "missing",
    classification: "userSoftware",
    observation: "observed",
    issues: [],
    risk: makeRisk({
      level: "noClearIssue",
      ruleIds: ["TEST-INV-000"],
      summary: "在普通用户软件元数据的检查范围内未发现明确配置异常。",
      evidence: [
        makeEvidence({
          code: "TEST-INV-METADATA",
          statement: "已读取到软件名称、发布者与卸载入口。",
        }),
      ],
    }),
    ...overrides,
  };
}

export function makeSoftwareSource(
  overrides: Partial<SoftwareSourceStatus> = {},
): SoftwareSourceStatus {
  return {
    id: "HKEY_LOCAL_MACHINE:registry64:SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    scope: "machine",
    registryView: "registry64",
    registryKey: "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    observation: "observed",
    exists: true,
    itemCount: 1,
    ...overrides,
  };
}

export function makeSoftwareScan(
  overrides: Partial<SoftwareScanResult> = {},
): SoftwareScanResult {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "observed",
    sources: [makeSoftwareSource()],
    items: [makeInstalledSoftware()],
    issues: [],
    limitations: ["不包含 Microsoft Store 应用、服务关联与卸载残留。"],
    truncated: false,
    ...overrides,
  };
}

export function makeStartupItem(overrides: Partial<StartupItem> = {}): StartupItem {
  return {
    id: "test:startup:1",
    name: "虚构启动助手",
    valueName: "虚构启动助手",
    valueType: "regSz",
    rawValueReadable: true,
    source: "registryRun",
    scope: "user",
    registryView: "shared",
    registryKey: "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
    targetKind: "registryCommand",
    targetPath: "C:\\Users\\Example\\AppData\\Local\\Temp\\fictional-helper.exe",
    targetCandidate: null,
    targetState: "present",
    pathResolution: "quotedExecutable",
    shortcutArgumentsPresent: null,
    shortcutWorkingDirectoryPresent: null,
    approvalState: "notChecked",
    observation: "observed",
    issues: [],
    risk: makeRisk({
      scope: "startupMetadata",
      level: "potentialUnwanted",
      confidence: "low",
      ruleIds: ["TEST-STARTUP-TEMP-001"],
      summary: "启动项指向用户临时目录，且本次未核验签名与来源。",
      evidence: [
        makeEvidence({
          code: "TEST-STARTUP-TEMP-PATH",
          statement: "目标路径位于用户临时目录。",
        }),
      ],
      blindSpots: ["未检查启动审批状态。", "未核验文件签名。"],
      recommendation: "先核实来源，再决定是否处理；不要依据单一规则直接删除。",
    }),
    ...overrides,
  };
}

export function makeStartupSource(
  overrides: Partial<StartupSourceStatus> = {},
): StartupSourceStatus {
  return {
    id: "startup-folder:HKEY_CURRENT_USER",
    source: "startupFolder",
    scope: "user",
    registryView: null,
    location: "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup",
    observation: "observed",
    exists: true,
    itemCount: 1,
    ...overrides,
  };
}

export function makeStartupScan(overrides: Partial<StartupScanResult> = {}): StartupScanResult {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "observed",
    sources: [makeStartupSource()],
    items: [makeStartupItem()],
    issues: [],
    limitations: ["启动审批状态未核验；未读取计划任务、服务与策略启动项。"],
    truncated: false,
    ...overrides,
  };
}

export function makeAssociationSource(
  overrides: Partial<AssociationSourceStatus> = {},
): AssociationSourceStatus {
  return {
    id: "processes:toolhelp32",
    source: "processSnapshot",
    observation: "observed",
    itemCount: 1,
    addressableCount: 1,
    unresolvedPathCount: 0,
    truncated: false,
    ...overrides,
  };
}

export function makeAssetRelation(overrides: Partial<AssetRelation> = {}): AssetRelation {
  return {
    id: "association:test-relation",
    softwareId: "test:software:1",
    softwareName: "虚构样本应用",
    target: {
      kind: "process",
      id: "process:4242:pidct1:00000000000000a1",
      name: "fictional-process.exe",
      detail: "PID 4242",
      path: "C:\\Program Files\\Fictional Sample\\app.exe",
      observation: "observed",
    },
    relationType: "softwareProcess",
    evidenceType: "displayIconPath",
    matchMethod: "exactPath",
    softwareEvidencePath: "C:\\Program Files\\Fictional Sample\\app.exe",
    evidenceValue: "C:\\Program Files\\Fictional Sample\\app.exe",
    certainty: "exact",
    observation: "observed",
    ambiguous: false,
    candidateSoftwareCount: 1,
    issues: [],
    ...overrides,
  };
}

export function makeSoftwareAssociationSummary(
  overrides: Partial<SoftwareAssociationSummary> = {},
): SoftwareAssociationSummary {
  return {
    softwareId: "test:software:1",
    softwareName: "虚构样本应用",
    state: "linked",
    pathEvidenceCount: 2,
    relationCount: 1,
    strongRelationCount: 1,
    weakRelationCount: 0,
    issues: [],
    ...overrides,
  };
}

export function makeAssetAssociationResult(
  overrides: Partial<AssetAssociationResult> = {},
): AssetAssociationResult {
  return {
    checkedAt: SAMPLE_TIME,
    observation: "observed",
    sources: [
      makeAssociationSource({
        id: "software:uninstall",
        source: "softwareInventory",
        itemCount: 1,
      }),
      makeAssociationSource(),
      makeAssociationSource({
        id: "services:scm",
        source: "serviceInventory",
        itemCount: 0,
        addressableCount: 0,
      }),
      makeAssociationSource({
        id: "scheduled-tasks:task-scheduler",
        source: "scheduledTaskInventory",
        itemCount: 0,
        addressableCount: 0,
      }),
      makeAssociationSource({
        id: "startup:run-and-folders",
        source: "startupInventory",
        itemCount: 0,
        addressableCount: 0,
      }),
    ],
    software: [makeSoftwareAssociationSummary()],
    relations: [makeAssetRelation()],
    issues: [],
    limitations: ["关联只使用静态路径证据，没有行为或签名证据。"],
    truncated: false,
    ...overrides,
  };
}

export function makeProcessList(overrides: Partial<ProcessListResult> = {}): ProcessListResult {
  return {
    checkedAt: SAMPLE_TIME,
    processes: [
      {
        pid: 4242,
        parentPid: 1000,
        name: "fictional-process.exe",
        executablePath: "C:\\Program Files\\Fictional\\fictional-process.exe",
        pathAccess: "available",
        parentName: "explorer.exe",
        identity: "pidct1:00000000000000a1",
        startTime: "2026-10-29T01:00:00Z",
      },
      {
        pid: 4343,
        parentPid: 4,
        name: "fictional-protected.exe",
        executablePath: null,
        pathAccess: "denied",
        parentName: "System",
        identity: null,
        startTime: null,
      },
    ],
    issues: [],
    limitations: ["这是某一时刻的进程快照，不构成实时监控。"],
    truncated: false,
    ...overrides,
  };
}

export function makeAffinityInspection(
  overrides: Partial<AffinityInspection> = {},
): AffinityInspection {
  return {
    checkedAt: SAMPLE_TIME,
    pid: 4242,
    identity: "pidct1:00000000000000a1",
    name: "fictional-process.exe",
    executablePath: "C:\\Program Files\\Fictional\\fictional-process.exe",
    startTime: "2026-10-29T01:00:00Z",
    inspectionState: "complete",
    signature: {
      state: "valid",
      signerName: "Example Publisher",
      detail: "文件签名验证通过（仅检查磁盘文件，未检查内存映像）。",
      verificationScope: "fileOnDiskOnly",
      memoryVerified: false,
      networkUsed: false,
      revocationChecked: false,
      fileEvidence: {
        checkedPath: "C:\\Program Files\\Fictional\\fictional-process.exe",
        sizeBytes: 1_234_567,
        modifiedAt: "2026-10-29T01:00:00Z",
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        hashState: "computed",
        note: "SHA-256 仅对应检查时的磁盘文件；不证明内存映像或进程行为。",
      },
    },
    affinity: {
      processMask: {
        maskHex: "0x000000000000000F",
        cpuIndices: [0, 1, 2, 3],
        count: 4,
      },
      systemMask: {
        maskHex: "0x00000000000FFFFF",
        cpuIndices: Array.from({ length: 20 }, (_, index) => index),
        count: 20,
      },
      restricted: true,
      comparisonScope: "currentProcessorGroup",
      sourceTrace: {
        available: false,
        explanation: "Windows 当前接口不能提供设置亲和性的调用者和时间。",
      },
    },
    cpuUsage: {
      percentOfTotalCapacity: 3.2,
      sampleDurationMs: 500,
      sampledAt: SAMPLE_TIME,
      method: "两次 GetProcessTimes 采样，按当前处理器组逻辑处理器容量归一化。",
      caveat: "短时采样只代表该时间窗口，不能直接说明资源滥用。",
    },
    issues: [],
    affinityRisk: makeRisk({
      scope: "affinityConfiguration",
      level: "configurationAnomaly",
      confidence: "low",
      ruleIds: ["TEST-AFF-001"],
      summary: "检测到受限亲和性配置，原因无法从现有接口确认。",
      evidence: [
        makeEvidence({
          code: "TEST-AFF-RESTRICTED",
          statement: "进程允许 4 个逻辑处理器，系统可用 20 个。",
        }),
      ],
      blindSpots: ["无法追踪是谁或何时设置亲和性。"],
      recommendation: "先确认是否为软件自身配置或用户设置；亲和性受限不等同于恶意行为。",
    }),
    overallRisk: makeRisk({
      scope: "processOverall",
      level: "insufficientEvidence",
      confidence: "unknown",
      ruleIds: ["TEST-PROC-001"],
      summary: "没有独立行为证据，不能形成整体进程安全结论。",
      evidence: [
        makeEvidence({
          code: "TEST-PROC-NO-BEHAVIOR",
          statement: "亲和性、签名和 CPU 采样不是完整行为证据。",
        }),
      ],
      blindSpots: ["未检查命令行动作、网络、模块加载或内存映像。"],
      recommendation: "只依据专项检查范围解释结果。",
    }),
    limitations: ["仅反映当前处理器组，不能覆盖跨组细节。"],
    ...overrides,
  };
}

export function makeNormalAffinityInspection(): AffinityInspection {
  return makeAffinityInspection({
    affinity: {
      processMask: {
        maskHex: "0x00000000000FFFFF",
        cpuIndices: Array.from({ length: 20 }, (_, index) => index),
        count: 20,
      },
      systemMask: {
        maskHex: "0x00000000000FFFFF",
        cpuIndices: Array.from({ length: 20 }, (_, index) => index),
        count: 20,
      },
      restricted: false,
      comparisonScope: "currentProcessorGroup",
      sourceTrace: {
        available: false,
        explanation: "Windows 当前接口不能提供设置亲和性的调用者和时间。",
      },
    },
    affinityRisk: makeRisk({
      scope: "affinityConfiguration",
      level: "noClearIssue",
      confidence: "low",
      ruleIds: ["TEST-AFF-000"],
      summary: "亲和性范围内未发现异常；该结论只涉及当前处理器组亲和性，不代表进程整体安全。",
      evidence: [
        makeEvidence({
          code: "TEST-AFF-EQUAL",
          statement: "进程允许范围与系统范围一致。",
        }),
      ],
      blindSpots: ["未检查行为、网络或内存证据。"],
      recommendation: "将亲和性视为环境事实，而不是安全结论。",
    }),
  });
}
