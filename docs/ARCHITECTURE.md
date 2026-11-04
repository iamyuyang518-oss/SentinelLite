# 架构

## 1. 总览

```text
React 19 + TypeScript (WebView2)
  home / events / cleanup pages
        │ typed API adapter (16 application commands)
        ▼
Tauri 2 IPC command layer (src-tauri)
  async wrapper, spawn_blocking, serialized errors
        │
        ├─ sentinel-core (read-only inventory)
        │    model → rules → scanners → Windows APIs
        │
        ├─ sentinel-storage (SQLite)
        │    ingestion worker + bounded queries + retention
        │         ▲
        │         │ event batches (read-only)
        │         │
        ├─ sentinel-detection
        │    stored ProcessEvent[] → explainable results
        │
        └─ sentinel-ipc client
             ├─ Status pipe \\.\pipe\SentinelLiteSensor.Status.v1
             └─ Events pipe \\.\pipe\SentinelLiteSensor.Events.v1
                   ▼
SentinelLiteSensor.exe --serve (administrator foreground process)
  ETW realtime process start/stop
    → bounded pipeline → normalized ProcessEvent
    → EventBroker (bounded cursor window) → event server

Windows 10/11 local OS
```

`sentinel-events` is the shared model used by the sensor, IPC, storage and
detection. The app never writes to the sensor; both pipes are read-only.

Phase 2.1 增加独立前台 `SentinelLiteSensor.exe`（`crates/sentinel-sensor`）：管理员运行的 ETW realtime 进程 start/stop 采集，定时模式输出 JSON summary；Phase 2.2 增加 `--serve` 前台服务模式，通过受限 Named Pipe 暴露只读状态。它不自动启动、不安装服务、不写启动项，普通用户运行必须 fail closed。

Phase 2.2 的 `crates/sentinel-ipc` 承载状态查询协议和受限管道；Phase 2.5 增加独立事件协议和管道。主应用只读取状态和事件，不通过 IPC 下发停止、配置或任意命令。Phase 2.3 将状态查询接入首页，每 4 秒轮询；整体保护状态仍保持“状态未知”，因为尚无告警或处置模型。

Phase 2.4 在传感器内部增加标准化进程事件与有界管道：ETW 回调只做有界解码和非阻塞入队，独立消费者完成标准化、配对、健康计数与 sink；队列满时丢弃最新事件并计数，不阻塞采集线程。

Phase 2.5 增加共享 `sentinel-events` 模型和独立只读事件管道。传感器消费者把记录追加到有界投递窗口，客户端用游标批量读取；`PairingRevision` 与 `SessionEnd` 通过同一顺序流交付，投递过期明确报告。状态管道协议不变。

Phase 2.6 增加 `sentinel-storage`：非提权应用后台线程从事件管道读取，幂等写入当前用户本地 SQLite，保存会话、事件、配对修正、缺口和 `SessionEnd`；查询有界，保留策略有时间和容量上限，数据库错误不影响传感器。

Phase 2.7 增加“进程事件”页面：实时事件来自事件管道，历史来自 SQLite；断开、游标过期、ETW/管道丢失、未配对、缺字段和数据库错误分别展示。

Phase 2.8 增加 `sentinel-detection`：基于已持久化事件的少量保守规则，输出带规则版本、证据事件 ID、误报说明和数据质量门禁的结果。检测不执行任何处置，也不判断恶意性。

Phase 3 切片 1 在 `sentinel-core` 增加原生防护只读观察：普通用户通过 WMI `MSFT_MpComputerStatus` 读取 Defender 开关/签名，通过 COM `INetFwPolicy2` 读取 Domain/Private/Public 防火墙配置文件的启用位。查询由用户主动触发，在 `spawn_blocking` 线程内执行；失败、字段缺失或无权限时返回结构化未知，不修改设置，也不改变整体保护状态。

Phase 3 切片 2 增加服务与计划任务只读盘点：`sentinel-core/src/services.rs` 通过 SCM 枚举服务并逐项读取配置，`sentinel-core/src/scheduled_tasks.rs` 通过 Task Scheduler COM 递归枚举任务和动作可执行路径。两个命令各自独立失败；结果只在内存中，不写 SQLite，不执行任何服务/任务操作。切片 3 升级现有 `startup.rs`：保留原 `scan_startup_items` 命令，增加来源级/条目级观察状态、注册表值类型、原始值可读性，以及通过只读 ShellLink 解析 `.lnk` 目标和参数/工作目录存在性；不读取参数文本，不执行任何启动项。切片 4 升级现有 `software.rs`：为 HKLM 32/64 与 HKCU Uninstall 视图增加来源/条目观察状态、字段状态、DisplayIcon 静态路径线索和 WindowsInstaller；卸载入口只保留存在性布尔值，不返回完整命令。切片 5 增加独立纯函数 `association.rs`：由现有软件、进程、服务、任务、启动项快照生成有界、可解释的静态路径关联；不新增底层扫描器，不执行目标，不读取命令行/参数，结果只在内存。切片 6 不新增 Rust 扫描器或 Tauri 命令，而是在前端把 `get_native_protection_status` 与 `scan_asset_associations` 组织成“安全态势概览”：应用壳层共享一个原生防护 `ScanController`，软件体检页共享一个关联 `ScanController`；只派生读取状态、证据分类和关系视图，不生成安全评分或处置入口。

没有自动启动的后台守护进程、网络服务、内核驱动或通用提权组件；SQLite 由用户态应用打开，传感器不接触数据库。

## 2. 代码边界

| 位置 | 职责 | 禁止 |
| --- | --- | --- |
| `crates/sentinel-core/src/model.rs` | 与前端一致的可序列化证据模型 | 依赖 Tauri、UI 状态 |
| `crates/sentinel-core/src/rules.rs` | 从事实生成风险等级、规则、证据、盲区 | 根据品牌/名称定罪；执行动作 |
| `crates/sentinel-core/src/registry.rs` | 注册表原始值类型解析、显式 present/missing/typeMismatch/invalid/readError 状态 | 把类型异常当作缺失；求值/执行注册表内容 |
| `crates/sentinel-core/src/software.rs` | 有界 Uninstall 盘点、HKCU 共享视图、分类语义、来源/字段观察状态、卸载入口存在性 | 启动卸载器、写注册表、返回完整卸载命令 |
| `crates/sentinel-core/src/startup.rs` | 有界 Run/RunOnce/启动文件夹检查、注册表值类型与观察状态、路径歧义解析、元数据文件排除、快捷方式证据 | 修改/禁用/执行启动项；保留命令行或参数文本 |
| `crates/sentinel-core/src/shortcut.rs` | 只读 ShellLink `.lnk` 加载、目标路径、参数/工作目录存在性布尔值 | 调用 Resolve、执行目标、返回参数/工作目录文本、访问远程目标 |
| `crates/sentinel-core/src/process.rs` | 进程快照、创建时间身份、亲和性、CPU 采样与签名 | 终止进程、设置亲和性、混合 PID 身份 |
| `crates/sentinel-core/src/signature.rs` | 磁盘签名、有限 SHA-256/元数据证据 | 联网、吊销检查、内存校验 |
| `crates/sentinel-core/src/native_protection.rs` | Defender WMI 与防火墙 COM 的固定只读查询、字段/提供程序失败语义 | 修改/关闭防护、策略或规则读写、子进程、把未知填成已启用 |
| `crates/sentinel-core/src/services.rs` | SCM 服务枚举、逐项只读配置、状态/启动类型/账户/路径上限 | 启动/停止/创建/删除/修改服务、保留命令行参数 |
| `crates/sentinel-core/src/scheduled_tasks.rs` | Task Scheduler COM 只读枚举、任务主体/动作路径、递归文件夹与上限 | 运行/创建/修改/删除/启用/禁用任务、读取参数/触发器/执行程序 |
| `crates/sentinel-core/src/association.rs` | 纯函数路径规范化、精确/包含/弱线索匹配、候选歧义、来源/证据/完整性模型与上限 | 读取新数据、执行目标、读取命令行/参数、将弱线索当确定归属、修改任何对象 |
| `crates/sentinel-core/src/com.rs` | 只读查询共享的 COM apartment guard | 在 UI 线程初始化 COM、泄漏未平衡的 COM 引用 |
| `crates/sentinel-core/src/limits.rs` | 条目、来源、字符串、哈希大小上限 | 无界读取/渲染 |
| `crates/sentinel-sensor/src/etw.rs` | realtime ETW session 生命周期、停止/清理、残留确认；Phase 2.4 起作为有界管道生产者；`--serve` 时装配状态 provider | 写 ETL；停止非自己创建的 session；在回调中配对/写 sink；生成“保护正常” |
| `crates/sentinel-sensor/src/event.rs` | 标准化事件模型、事件/会话 ID、三类时间、字段完整性、配对状态、SessionEnd | 编造缺失字段；把 ETW 时间、接收时间、标准化时间混为一谈 |
| `crates/sentinel-sensor/src/pipeline.rs` | 有界同步队列、非阻塞生产者、独立消费者、sink、丢弃/积压统计、关闭排空 | 无界缓冲；在 ETW 回调中阻塞；把队列丢弃写成 ETW 丢失 |
| `crates/sentinel-sensor/src/time.rs` | FILETIME/Unix/RFC3339 转换 | 从未知/零值时间猜造默认时间 |
| `crates/sentinel-ipc/src/protocol.rs` | 版本化、长度受限的状态请求/响应 | 接受未知字段/版本；传输原始事件或命令 |
| `crates/sentinel-ipc/src/server.rs` | 受限 Named Pipe 服务端、同用户客户端校验、请求超时、只读 status 分发 | 远程管道；任意命令；无界消息；多实例抢占 |
| `crates/sentinel-ipc/src/client.rs` | 只读客户端、服务端镜像/elevation 校验、连接/读写超时 | 信任管道名；把通信失败当健康；发送非 status 请求 |
| `crates/sentinel-sensor/src/decode.rs` | Kernel-Process start/stop schema 解码与字段状态 | 猜测字段；把缺失/部分字段填成正常值 |
| `crates/sentinel-sensor/src/pair.rs` | `(pid, create_time)` 身份配对、重复 Stop、乱序回补、PID reuse/sequence 异常、有界身份/孤儿表 | 仅凭 PID 配对；跨身份强行配对；把不确定身份猜成确定 |
| `crates/sentinel-sensor/src/health.rs` | 传感器计数、状态机、队列丢弃计数、fail-closed `ok` | 在证据不足时报告健康 |
| `crates/sentinel-sensor/src/delivery.rs` | 有界游标投递窗口、批次/过期/确认、SessionEnd 保留 | 无界缓存；把投递淘汰写成 ETW 丢失；伪造缺口为零 |
| `crates/sentinel-sensor/src/main.rs` | 最小 CLI（含 `--queue-capacity`、`--event-buffer`）、JSON summary、明确退出码 | 自行提权；把失败报告为采集成功 |
| `crates/sentinel-events/` | 共享事件模型、三时钟、配对状态、`SessionEnd`/`final_stats`、时间转换 | 为缺失字段编造默认值；用 0 表示未知 ETW 丢失；在传感器/IPC/存储复制不同定义 |
| `crates/sentinel-ipc/src/events.rs` | 版本化事件请求/批次/响应、上限、游标语义 | 无界批次；把过期/超前游标当作空数据 |
| `crates/sentinel-ipc/src/event_server.rs` | 独立受限事件管道、同用户校验、批量响应 | 复用状态管道阻塞健康查询；远程访问；无界消息 |
| `crates/sentinel-storage/` | SQLite schema/迁移、幂等写入、修正更新、保留策略、摄取 worker | 在传感器内打开数据库；无界查询/写入；上传事件 |
| `crates/sentinel-detection/` | 输入质量门禁、版本化规则、证据、误报说明 | 恶意判定；自动处置；把数据不足/未知当作正常 |
| `src-tauri/src/storage_state.rs` | 数据库生命周期、摄取线程、退出清理、状态报告 | 在 UI 线程执行数据库/管道阻塞工作 |
| `src-tauri/src/commands.rs` | 固定命令白名单、阻塞任务包装、错误码 | 接受任意路径、命令、脚本、URL |
| `src-tauri/src/command_names.rs` | 应用命令名单一清单 | 未同步 generate_handler 或 capability |
| `src-tauri/permissions/autogenerated/` | Tauri AppManifest 生成的 allow/deny 权限 | 手工编辑/跳过 CI 同步检查 |
| `src/` | 呈现状态、证据、限制；用户触发的只读检查 | 直接访问 OS、保存敏感结果、发起外部网络请求 |
| `src/lib/posture.ts`、`src/components/SecurityPosture.tsx` | 组合已有快照、来源读取状态、证据分类与关系视图；共享 `ScanController` | 生成安全评分、把缺失当正常、重复采集或提供处置 |

Windows 只读检查依赖集中在 `sentinel-core`，ETW 依赖集中在 `sentinel-sensor`；规则、解码、配对与健康状态可以用虚构输入做确定性测试。

## 3. 启动与数据流

1. Tauri 创建主窗口并加载打包后的前端。
2. 首页并行调用 `get_protection_overview`（静态监控组件、只读检查、能力边界）和 `get_sensor_status`（独立传感器实时状态）。前端不自动启动传感器，也不提供应用内启动/UAC 入口；传感器未运行时显示静态“暂不支持在应用内启用实时保护”，管理员需在应用外手动运行传感器。原生防护状态只在用户点击时调用 `get_native_protection_status`，不自动轮询。“软件体检”的“软件盘点”标签由用户点击“开始只读盘点”后调用 `scan_installed_software`；“启动项”标签调用 `scan_startup_items`；“服务与计划任务”标签调用 `list_services` / `list_scheduled_tasks`。软件详情中的“读取资产关联”调用 `scan_asset_associations`，在后端复用上述只读来源并计算静态路径关系；每个来源独立呈现加载、失败和观察状态。软件体检顶部的“读取安全态势”并行触发 `get_native_protection_status` 与 `scan_asset_associations`；两者独立失败，概览与软件详情共享同一关联快照，原生防护快照由应用壳层共享，避免重复采集。
3. 管理员可以在应用外显式启动 `SentinelLiteSensor.exe --serve`；它创建受限状态管道。主应用不启动、停止或配置传感器，不提供 stop/config/任意命令，也不提供应用内提权入口。
4. 后端 `get_sensor_status` 以普通用户身份连接该管道，读取状态/计数；管道不存在、身份不符、超时或格式错误分别映射为 `stopped`、`unavailable`、`communicationFailed`，不会变成 `running`。
5. 首页在挂载时立即查询传感器状态，之后每 4 秒轮询；请求不重叠，失败时清空旧计数与健康结果，旧响应不能覆盖新响应。首页离开时组件卸载并停止轮询。整体保护状态仍由 `ProtectionOverview` 提供，当前固定为未知。
6. 用户在软件体检页选择一项检查；前端调用对应只读命令。
7. 命令通过 `spawn_blocking` 在后台线程执行，避免阻塞 WebView 主线程；CPU 采样约 500 ms。
8. 注册表扫描在 `SourceBudget` 控制下停止枚举；字符串超过上限时截断并标记，结果通过 `truncated` 明确为不完整。分类字段通过原始值解析保留类型异常状态。
9. 进程快照在固定条目上限内枚举；名称/路径超限时截断，结果通过 `truncated` 和 DataIssue 标记。进程列表返回创建时间身份令牌；亲和性命令必须提交该令牌，后端在打开句柄后、采样前和采样后核对。
10. 后端返回结构化结果；读取失败以 `DataIssue`、四态路径、显式注册表字段状态、空值或证据不足体现。
11. 前端对软件/启动项/进程列表各分页最多渲染 50 行，不一次性渲染无界结果。
12. 结果只保存在当前内存中；IPC 失败转为稳定的前端错误，不显示旧结果或绿色状态。

### Phase 2.4 进程事件管道

```text
ETW record (ProcessTrace callback thread)
  → bounded schema decode + ImageName 1,024-char cap
  → sync_channel(capacity, default 4,096) try_send (never blocks)
      ├─ full → drop newest + dropped_queue_full + health events_dropped
      └─ consumer disconnected/closed → dropped_consumer_gone
  → normalizer consumer thread
      → session/event ID + three clocks + completeness
      → PairingState (pid, create_time): paired/waiting/unpaired/indeterminate
      → health counters, bounded diagnostics, EventSink
  → on close: drain queue → SessionEnd(reason, open starts, pairing snapshot)
```

- 队列容量可由 `--queue-capacity` 在 64–65,536 之间配置；默认 4,096。`backlog` 包含消费者当前持有的事件，瞬时峰值可能为 `capacity + 1`，实际 channel 占用严格不超过容量。
- 生产 sink 是有界游标投递窗口（默认 4,096 条记录），保留最近记录供 IPC 读取；完整历史由主应用写入 SQLite，不在传感器内持久化。诊断样本仍只保留最近 8 条和首个配对/未配对/无效/修正记录。
- 关闭时最多等待消费者 5 秒；超时记录 `shutdown_timed_out` 与 `backlog_at_shutdown_timeout`，健康降级，不宣称已排空。
- ETW `events_lost` / `realtime_buffers_lost` 与管道 `dropped_queue_full` / `dropped_consumer_gone` 是独立字段；健康要求两者都为零。
- 关闭后对账 `events_processed`、`events_dropped`、`ignored_events` 与 pipeline 计数；不一致会使健康降级。

### Phase 2.5–2.8 事件交付、持久化、界面与分析

```text
sensor consumer (normalizer)
  → EventBroker: cursor 1..N, bounded 4,096 records (configurable)
      ├─ no reader → evict oldest, evictions++
      └─ append SessionEnd once → closed
  → Events pipe batch request(after, limit ≤128)
      → cursor_expired?missed / cursor_ahead / session_closed
  → app ingestion worker (non-elevated)
      → SQLite transaction: INSERT OR IGNORE + revision update + cursor
  → query history (bounded 200 rows) / analyze session (≤20,000 events)
  → Events page: live from pipe, history from SQLite, detection results
```

- 事件投递与状态查询端点分离，批次响应上限 256 KiB；超限截断尾部并设置 `has_more`。
- 传感器在存在读取客户端时最多等待 2 秒确认 `SessionEnd`；没有客户端时立即退出。多客户端只等待任一确认，第二个客户端可能错过尾部。
- 应用摄取最多每 2 秒重试一次缺失的传感器，避免长退避错过短会话；写入失败不推进游标，重放幂等。
- SQLite 使用 WAL、`quick_check`、`user_version` 迁移；数据库位于当前用户本地应用数据目录，不加密，依赖用户 ACL。
- `SessionEnd.final_stats` 记录关闭时的最终 ETW `events_lost`/`realtime_buffers_lost` 和 pipeline 接收/消费/标准化/无效/队列丢弃/过滤/最终积压/峰值/关闭超时/ sink 错误。ETW 值只在停止成功且无残余 session 时写入；否则为 `null`。旧记录没有该块，不能当作零。
- 检测输入优先使用已持久化的 `SessionEnd.final_stats`；没有最终统计时才回退到当前实时传感器健康。已知最终统计为零不要求仍然存活的传感器健康；未知 ETW 丢失不会被当作零。

## 4. IPC 契约

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `get_protection_overview` | 无 | `ProtectionOverview` |
| `get_native_protection_status` | 无 | `NativeProtectionStatus`：Defender/防火墙只读快照、提供程序状态、字段/错误 issue |
| `get_sensor_status` | 无 | `SensorStatusReport`：链路状态 + 可选只读传感器状态 |
| `get_sensor_events` | `after: u64`、`limit: 1..=128` | `SensorEventsReport`：只读批次/游标/会话关闭 |
| `get_storage_status` | 无 | `StorageRuntimeReport`：数据库/摄取状态 |
| `query_stored_events` | 有界 `EventFilter` | 历史事件分页结果 |
| `get_stored_event` | `eventId: String` | 配对事件、修正和 `SessionEnd` 详情 |
| `list_sensor_sessions` | `limit: 1..=100` | 本地会话列表 |
| `analyze_session` | `sessionId: String` | 可解释 `SessionAnalysis` |
| `scan_installed_software` | 无 | `SoftwareScanResult`：Uninstall 来源/条目观察状态、字段状态、卸载入口存在性（无命令文本） |
| `scan_startup_items` | 无 | `StartupScanResult`：Run/RunOnce/启动文件夹、来源与条目观察状态、值类型、快捷方式布尔证据 |
| `scan_asset_associations` | 无 | `AssetAssociationResult`：软件↔进程/服务/计划任务/启动项的静态路径关系、证据、确定性、候选歧义与来源完整性 |
| `list_services` | 无 | `ServiceInventoryResult`：SCM 服务快照、观察状态、逐项 issue |
| `list_scheduled_tasks` | 无 | `ScheduledTaskInventoryResult`：任务/文件夹快照、动作路径、观察状态 |
| `list_processes` | 无 | `ProcessListResult` |
| `inspect_process_affinity` | `pid: u32`（> 0）、`identity: String`（创建时间令牌，后端重新核验） | `AffinityInspection` |

服务、计划任务、软件和启动项命令在返回 WebView 前执行值级脱敏：用户目录段替换为 `***`，个人服务/任务账户替换为 `domain\***`；系统身份保留。扫描器和 `scan_asset_associations` 内部仍使用原始快照进行匹配，DTO 形状、字段和能力声明不变。

约束：

- 参数只有 `u32` PID 和后端签发的创建时间身份字符串；不允许路径、命令、URL、脚本或任意标志。身份长度和格式受限，且与句柄实际创建时间比对。
- 所有字段枚举和字段名使用 camelCase，前端类型在 `src/types.ts` 中显式维护。
- 命令错误序列化为 `{ code, message, retryable }`；前端只展示受控消息。进程身份变化返回 `processIdentityChanged`。
- 前端不缓存失败结果；后端不会自动重试。
- 所有扫描均由用户在 UI 中触发；只有首页的传感器状态查询每 4 秒自动轮询，不涉及扫描或命令执行。
- `get_sensor_status` 是独立于 Tauri IPC 的第二条本地 IPC：主应用作为普通用户连接管理员传感器的受限 Named Pipe，只读取状态；没有 stop/config/任意命令。通信失败、权限不足、管道不存在或服务端身份不符都不得产生“传感器健康”。首页把查询失败显示为不可用/健康未知，并清空旧计数。

### Tauri 应用级 ACL

- `build.rs` 使用 `tauri_build::AppManifest` 为十六个应用命令自动生成 `allow-*` 权限；生成文件提交到 `src-tauri/permissions/autogenerated/` 并由 CI 检查同步。
- capability 只包含十六个应用权限，仅作用于 `main` 窗口；**不授予 `core:default`**，因为 UI 不使用其他 core/plugin 命令。
- `generate_handler!` 仍是代码注册白名单；ACL 是额外的运行时授权边界，两者都由同一 `APP_COMMANDS` 清单和回归测试约束。
- 没有安装 `shell`、`fs`、`http`、`process`、`updater` 等插件。

## 5. 安全配置

- 生产 CSP：只允许本地资源、`ipc:` 连接与必要的样式；禁止对象、表单、框架和远程来源。
- 开发 CSP 单独允许 Vite HMR；发布构建不使用开发 CSP。
- `withGlobalTauri` 未启用；前端显式导入 Tauri API。
- 应用级 command ACL 已启用；详见 ADR 0005 与 `ARCHITECTURE` 上节。
- 无远程内容，无内联执行的用户数据。
- 传感器状态管道和事件管道都使用 message 模式、单实例、`PIPE_REJECT_REMOTE_CLIENTS`，DACL 只授予当前用户与 SYSTEM/Administrators，并显式设置 Medium mandatory label，避免 High 管理员传感器创建的对象按默认 `NO_WRITE_UP` 拒绝 Medium 主应用；服务端校验客户端用户 SID，客户端校验服务端镜像名与 elevation，并设置超时/长度上限。
- 事件批次限制 128 条/256 KiB；事件请求限制 1 KiB；游标过期/超前显式返回，不静默返回空。
- SQLite 位于 `%LOCALAPPDATA%\app.sentinellite.desktop\SentinelLite\events.db`，依赖当前用户 ACL；不加密、不防篡改、不上传。
- 应用以标准用户权限运行；不包含 UAC 清单中的 `requireAdministrator`，也不包含应用内 UAC 启动命令。原生防护查询同样以标准用户执行，权限不足时返回 `accessDenied`，不以提权重试。
- 服务/计划任务盘点同样以标准用户执行；SCM 只打开 `SC_MANAGER_ENUMERATE_SERVICE` / `SERVICE_QUERY_CONFIG`，Task Scheduler 只调用枚举/Definition getter。权限不足按条目保留，不提权、不执行任务。
- 不关闭 Defender、防火墙、攻击面减少规则或其他安全策略；原生防护查询只调用 WMI SELECT 和 `INetFwPolicy2` getter，没有写入路径。

## 6. 模型与证据

每个条目返回 `RiskAssessment`：

- `scope`：`softwareMetadata` / `startupMetadata` / `affinityConfiguration` / `processOverall`，防止局部结论冒充整体判断；
- `level`：七类风险语言之一；`NoClearIssue` 表示“检查范围内未发现异常”，不是绿色安全结论；
- `confidence`：判断置信度，不是“病毒感染概率”；
- `ruleIds`：可追踪的规则标识；
- `summary`：当前证据能支持的最小结论；
- `evidence[]`：代码、陈述与数据源；
- `blindSpots[]`：没有检查、无法检查或证据不足的内容；
- `recommendation`：不会自动执行的人工后续建议。

扫描结果另有 `checkedAt`、`issues[]` 和 `limitations[]`。任何空结果都必须同时有范围说明。

## 7. Windows API 选择

- **软件**：HKLM 的 32/64 位视图，以及 HKCU 的单一共享视图（不使用 WOW64 32/64 重复读取）。每条风险按 Uninstall 字段分类；SystemComponent/NoRemove/ParentKeyName/ReleaseType 只作为注册表语义证据。DWORD 必须精确 4 字节；类型不符、值无效或读取失败会保留为显式状态，不让分类静默回退为普通用户软件。DisplayIcon 只解析为静态路径线索（去掉常见 `,index`），WindowsInstaller 保留 DWORD/状态；UninstallString/QuietUninstallString 只保留存在性布尔值，不返回命令文本。每个来源和条目分别记录观察状态；无 DisplayName 的注册项跳过并计数。
- **启动项**：HKLM Run/RunOnce 32/64 位 + HKCU Run/RunOnce 共享视图 + 两个启动文件夹；逐值保留注册表值类型、Raw Value 可读性和来源/条目观察状态。路径解析区分 quoted、unquoted、ambiguous unquoted、shortcut resolved/unresolved。`.lnk` 通过 `IShellLinkW` + `IPersistFile::Load(STGM_READ)` 读取目标；参数/工作目录只记录是否存在，不读取或展示文本；不调用 `Resolve`，不执行目标。歧义候选路径仅记录为未探测证据，不访问文件系统。desktop.ini/Thumbs.db 等文件夹元数据不计为启动项。
- **路径状态**：`std::fs::metadata` 只读取本地绝对路径，并映射 Present/Missing/AccessDenied/Unknown；AccessDenied 不降级为 Missing。
- **进程**：Toolhelp32 取得当前快照，最多 2048 项；名称 512、路径 4096 字符；`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` 只读查询；创建时间 FILETIME 作为身份令牌；路径/名称来自已验证句柄。
- **亲和性**：`GetProcessAffinityMask` 返回进程和当前处理器组的系统掩码；当前版本不做跨组遍历。亲和性风险与 `overallRisk` 分离。
- **CPU**：两次 `GetProcessTimes` 的 kernel+user 差值，以当前处理器组逻辑处理器数归一化；这是短时采样。
- **签名与磁盘证据**：`verifysign` 调用 WinVerifyTrust，`allow_network(false)`，不检查吊销；验证对象是磁盘文件。对本地文件记录路径、大小、修改时间、有限读取的 SHA-256；不读取内存映像。签名/哈希与运行时内存可能不同。
- **环境变量/路径**：`ExpandEnvironmentStringsW`；无法完整展开时返回未知，而非虚构存在性。UNC/设备命名空间和相对路径不会用于文件存在性或签名探测，避免隐式远程访问与误报。
- **原生防护**：WMI `ROOT\Microsoft\Windows\Defender` / `MSFT_MpComputerStatus` 只读取 5 个固定字段；COM `INetFwPolicy2::get_FirewallEnabled` 只读取 Domain/Private/Public。WMI 枚举 `Next` 超时 5 秒，所有 `VARIANT` 读取后清理；失败/字段缺失/无权限均返回结构化未知。没有用户输入、子进程、远程命名空间或写入路径。
- **服务**：`OpenSCManagerW(SC_MANAGER_ENUMERATE_SERVICE)` + `EnumServicesStatusExW(SC_ENUM_PROCESS_INFO, SERVICE_WIN32 | SERVICE_DRIVER, SERVICE_STATE_ALL)` 获取状态；逐项 `OpenServiceW(SERVICE_QUERY_CONFIG)` + `QueryServiceConfigW` 获取启动类型、账户、路径。缓冲区从 8 KiB 开始、上限 32 KiB（测试机上 64 KiB 会返回 1783）；只保留可执行文件路径，不保留参数。
- **计划任务**：Task Scheduler COM `TaskScheduler` CLSID + `ITaskService`，从 `\` 递归 `GetFolders`/`GetTasks(TASK_ENUM_HIDDEN)`；集合索引为 1-based；逐任务读取 Definition/RegistrationInfo/Principal/Actions，仅读取 ExecAction 的 Path，不读取 Arguments/WorkingDirectory/触发器/XML；最多 1,024 任务、256 文件夹、16 动作/任务、10 秒。
- **资产关联**：只在内存中比较现有快照的静态路径；规范化盘符路径的大小写、`/`/`\`、`.`/`..` 和 `\\?\` 前缀，拒绝 UNC、设备、相对、环境变量、通配符和疑似带参数的命令文本；宽泛根目录不作为归属证据；弱线索只用于普通用户软件的运行进程，且不用于系统目录；同一目标多个软件候选全部保留并标记歧义。不读取新字段、不执行目标、不返回命令行/参数。
- **输入边界**：每个注册表视图、总软件条目、启动项、进程快照、关联关系、字符串、磁盘哈希文件大小均有明确上限；截断写入 `DataIssue` 和结果级 `truncated`。UI 分页。

不访问键盘、屏幕、浏览器数据、文档内容或完整命令行。

## 8. 暂不引入的能力

- **SQLite**：已在 Phase 2.6 引入，但仅用于进程事件历史；没有审计链、加密、导出/删除 UI 或恢复快照。Schema、保留和权限见 ADR 0014。
- **完整 ETW**：仍只覆盖 Kernel-Process start/stop；文件、网络、DLL、服务/计划任务等 provider 未接入。
- **YARA-X / Sigma**：未评估/未集成；当前规则只使用进程事件字段。
- **ETW**：Phase 2.1 只引入管理员 realtime 进程 start/stop 最小切片（[ADR 0010](adr/0010-sensor-architecture.md)）；文件/网络/线程/镜像事件、事件持久化、丢事件注入和长时间稳定性仍未实现，不能以“有事件订阅”冒充完整保护。
- **YARA-X**：需要规则来源、误报测试、扫描范围和许可证审查；当前不扫描文件内容。
- **Sigma**：面向日志事件的规则格式；在事件存储和字段映射稳定前集成会产生虚假覆盖。

详见 [ADR](adr/)。Sensor Health 与保护状态派生见 [ADR 0008](adr/0008-sensor-health-and-protection-state.md)，SQLite 历史存储见 [ADR 0014](adr/0014-event-persistence.md)，ETW 最小切片见 [ADR 0010](adr/0010-sensor-architecture.md)，事件投递/可视化/分析见 [ADR 0013](adr/0013-process-event-delivery.md)、[ADR 0015](adr/0015-event-visualization.md)、[ADR 0016](adr/0016-behavior-analysis.md)，原生防护只读状态见 [ADR 0017](adr/0017-native-protection-status.md)，服务/计划任务盘点见 [ADR 0018](adr/0018-service-task-inventory.md)，启动项盘点见 [ADR 0019](adr/0019-startup-inventory.md)，软件/组件盘点见 [ADR 0020](adr/0020-software-inventory.md)，软件资产关联见 [ADR 0021](adr/0021-asset-associations.md)，安全态势整合见 [ADR 0022](adr/0022-security-posture-integration.md)。完整保护状态派生与告警/处置仍未实现。

## 9. 测试策略

- **Rust 单元/集成测试**：作用域化风险、软件分类、注册表原始类型/字段状态、字符串/条目/进程上限、四态路径、未加引号路径、HKCU 共享视图去重、进程创建时间身份、序列化契约、掩码、CPU、签名/哈希错误码；软件来源/观察状态、DisplayIcon 解析、InstallDate 异常、卸载入口未知不产生配置异常。
- **资产关联测试**：Windows 路径大小写/斜杠/点段/设备前缀规范化、路径边界、相同文件名不同目录不升级为精确关联、安装目录包含、服务/任务/启动项目标、缺失任务动作、无效路径、用户目录脱敏、多软件候选歧义、弱/强证据分级、来源不可用不变成无关联、关系 ID/顺序稳定性。
- **安全态势测试**：原生防护 enabled/disabled/unknown 分离、反恶意软件服务关闭不再显示“全部启用”、来源状态推导读取完整度、noMatch 与 insufficientEvidence/sourceUnavailable 分离、弱/多候选分类、共享关联快照、刷新失败清空两个快照、无评分/无处置/无原始用户路径。
- **Sensor 单元测试**：Kernel-Process start/stop schema 解码、partial/truncated、start-stop 配对、orphan/unpaired、重复 Start/Stop、乱序回补与 revision、PID reuse、身份不确定、sequence mismatch、有界身份/孤儿表、队列满载/消费者退出/关闭排空/关闭超时/sink 错误、20,000 事件持续负载、会话重启隔离、解码到标准化链、健康计数对账与 fail-closed `ok`。
- **前端测试**：真实组件渲染 + 假 API；断言未知状态、传感器状态/健康独立映射、事件计数与 uptime、4 秒轮询不重叠、卸载停止、失败清空旧健康/计数、旧响应不覆盖新响应；以及作用域化“检查范围内未发现异常”、系统组件提示、注册表字段类型异常、进程/软件/启动项截断警告、分页、无整体绿色结论、证据/哈希显示和没有处置按钮；原生防护卡片的未查询/完整/部分/关闭项/刷新失败清空旧结果；软件来源卡/观察状态/安装路径脱敏/不展示卸载命令；关联证据/弱线索/歧义/来源不足/刷新失败清空/无处置按钮；安全态势未读取/组合读取/五来源/证据分类/跨页共享/刷新失败/无评分/无处置。
- **原生防护测试**：字段缺失、访问拒绝、无实例、防火墙单/全配置文件失败、缺失配置文件、观测完整性、错误码映射、camelCase 序列化且无 `protected`/`overallState`；真实非管理员探针与 release WebView2 点击验证。
- **服务/计划任务测试**：服务状态/启动类型/类型位、路径解析（引号/参数/扩展名/设备路径/歧义）、配置失败保持未知、空账户/路径语义、任务状态/动作类型/错误映射、字符串清洗、个人账户/路径命令边界脱敏；真实非管理员 763 服务/192 任务与 release WebView2 盘点。
- **启动项测试**：注册表 32/64/HKCU 共享视图、值类型映射、空值/不支持类型/无效文本、缺失/拒绝/错误来源聚合、`.lnk` 成功与失败解析（测试临时文件，不触碰真实启动位置）、前端观察状态/来源卡/快捷方式布尔证据/刷新清空；真实非管理员 Run/RunOnce 与启动文件夹探针、release WebView2 盘点。
- **只读主机冒烟**：默认忽略，显式运行，检查真实注册表/进程读取不会崩溃且结果可序列化；它不断言具体软件数量。
- **CI**：Windows 上执行 lint、typecheck、前端测试、Rust fmt/clippy/test。
- **未做**：真实恶意软件、性能基准、故障注入、长时间稳定性、独立安全审计。

## 10. 发布构建

当前验证了 `npx tauri build --no-bundle` 的 Windows 可执行文件。Phase 2.5 验证了提权传感器经独立事件管道向生产客户端策略交付 32 个事件和 `SessionEnd`；Phase 2.6 验证了非提权应用把提权传感器的 28 个事件写入 SQLite，重启后可重新打开 schema v2 数据库；Phase 2.7/2.8 在 release 应用中实际展示了实时事件、历史查询和行为分析页面；Phase 2.8 用 170 个真实事件触发 `PROC-BURST-001`/`PROC-EXIT-002` 并显示证据；Phase 3 切片 1 在 release 应用中通过 UI Automation 触发原生防护读取并显示 Defender/防火墙观测值；切片 2 在 release 应用中打开“服务与计划任务”标签并真实盘点 763 个服务和 192 个计划任务；切片 5 在软件详情中真实读取 Git 与运行进程的静态路径关联；切片 6 在 release 应用中读取安全态势，显示 68 条关系、13 个有线索软件、171 个证据不足软件、3 个已读取来源和 2 个部分读取来源，且无原始用户路径、处置按钮或安全评分。详细证据见 [VALIDATION](VALIDATION.md) §15–§25。安装包已可在本地构建（未签名）；代码签名、自动更新、发布流水线和正式发布物仍未完成；不可将未签名的开发构建当成已签名或已审计的正式发布物。

## 11. 未来演进原则

1. 先增加只读观测，再增加告警，再增加用户确认后的可逆动作。
2. 传感器必须报告健康状态、覆盖范围和丢失事件；失败要降级。
3. 引入 SQLite 时，审计数据本地化、可删除、最小化，且不让 UI 直接持有文件路径能力。
4. 高权限操作通过独立代理和静态白名单命令完成，不使用通用 Shell。
5. 任何新检测规则都需要正常案例、异常案例、误报案例和边界案例。
6. 所有新外部输入必须通过明确上限、截断标记和 UI 分页；进程身份优先使用创建时间而非 PID。
