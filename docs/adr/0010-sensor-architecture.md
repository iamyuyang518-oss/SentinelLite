# ADR 0010：实时 ETW 进程传感器最小切片

- 状态：接受，Phase 2.1 最小切片已实现；第二轮提交前审查的 P1-1 与可本地验证的 P2 修复完成，第二轮修复版 `5e0e118a...` 已完成管理员 smoke + Ctrl-C PASS；跨代码页/真实 STOP 注入仍未验证（2026-10-31）
- 日期：2026-10-31
- 范围：`crates/sentinel-sensor` / `SentinelLiteSensor.exe`，只读进程 start/stop 遥测
- 关系：实现 [ADR 0008](0008-sensor-health-and-protection-state.md) 的多项健康与失败语义；不改变 [ADR 0007](0007-scoped-evidence-and-process-identity.md) 的证据作用域原则

## 1. 背景与边界

Phase 1 没有实时传感器，首页必须保持“状态未知”，不能因为零告警推导“保护正常”。

仓库外的 v4.2 ETW 探针已在管理员真机完成一次 realtime/off 验证：20 秒、27 个事件、13 start/14 stop、12 对配对、3/3 长文件名、`events_lost=0`、`decode_complete=true`、`session_stopped=true`。本 ADR 记录如何把该已验证能力收敛为正式仓库内的第一个生产切片，不重新讨论 ETW 是否可行。

本切片只做：

```text
ETW 实时会话 → 解码 → start/stop 身份配对 → 健康计数 → 停止/清理 → JSON summary
```

本切片明确不做：SQLite、IPC、Tauri UI 接入、行为检测、自动处置、告警、安装器、签名、更新、Defender 修改、文件/网络/线程/镜像等其它事件域。Phase 2.2 的只读状态 IPC 由 [ADR 0011](0011-sensor-ipc.md) 扩展；它不改变本切片的 ETW/解码/配对语义，也不传输原始事件或命令。

## 2. 组件与数据流

| 组件 | 职责 | 禁止 |
| --- | --- | --- |
| `etw.rs` | 创建/启用/消费/停止/关闭 realtime session；负责所有清理路径 | 写 ETL；停止非自己创建的 session；把启动成功当作采集成功 |
| `decode.rs` | 按 schema 解码 `Microsoft-Windows-Kernel-Process` 的 ProcessStart/ProcessStop | 猜测字段；把缺失字段填成正常值；无条件接受未知版本 |
| `pair.rs` | 以 `(pid, create_time)` 配对 start/stop；sequence 仅作交叉校验 | 仅凭 PID 配对；跨身份强行配对；静默丢弃异常 |
| `health.rs` | 计数、状态机、错误/丢失/部分字段降级、`ok` 判定 | 在证据不足时返回 `ok=true`；产生“保护正常” |
| `main.rs` | 最小 CLI、JSON summary、退出码 | 自行提权；隐藏启动失败；把 stderr 日志混入 JSON |

数据流：

```text
StartTraceW(realtime)
  → SetConsoleCtrlHandler(Ctrl-C/Break/Close/Shutdown → 幂等 stop)
  → EnableTraceEx2(Kernel-Process, keyword 0x10, level 4)
  → OpenTraceW(PROCESS_TRACE_MODE_REAL_TIME | EVENT_RECORD)
  → trace worker: ProcessTrace(callback) → CloseTrace
  → 定时线程 ControlTraceW(STOP)
  → 主线程有界等待 worker；超时则重试 stop 并按 fail-closed 返回
  → ControlTraceW(QUERY) 确认无残留
  → JSON summary / exit code
```

## 3. ETW 会话契约

- Provider：`Microsoft-Windows-Kernel-Process`，GUID `{22FB2CD6-0E7B-422B-A0C7-2FAD1FD0E716}`。
- Keyword：`WINEVENT_KEYWORD_PROCESS`（`0x10`）；level：`TRACE_LEVEL_INFORMATION`（4）。
- 只使用 realtime session，不写 ETL 文件。
- 会话名由 CLI 指定，默认 `SentinelLite-Sensor-<pid>`；只允许 ASCII 字母、数字、`-`、`_`、`.`，最多 128 字节。
- 会话归属明确：只有 `StartTraceW` 成功的 session 才允许被 stop。若会话已存在（`ERROR_ALREADY_EXISTS`），明确失败，不触碰已有 session。
- 启动失败、provider 启用失败、`OpenTraceW` 失败、`ProcessTrace` 失败、线程创建失败、停止失败都必须尝试清理；清理使用幂等 stop，失败缓存不会阻止后续重试。
- `ProcessTrace` 在独立线程运行；主线程等待 `duration + 10s`。若 stop 失败导致 `ProcessTrace` 不返回，主线程重试 stop、最多再等 2s，然后返回 exit 5 并退出，避免永久挂死。`CloseTrace` 由 trace worker 负责；worker 已启动时主线程不并发关闭同一 handle。
- 注册 `SetConsoleCtrlHandler`：Ctrl-C、Ctrl-Break、控制台关闭/关机会触发同一幂等 stop，并标记 `interrupted`；正常清理后返回 JSON 并退出。Ctrl-C/Ctrl-Break 有管理员实机证据；控制台关闭/关机只保证尝试 stop，Windows 可能在报告落盘前终止进程。`RunContext` 的 `Drop` 作为 panic unwind 的兜底再次尝试 stop/close。
- 停止后通过 `ControlTraceW(QUERY)` 轮询确认 session 已消失。查询无法确认时按 fail-closed 处理，不报告干净停止。
- 明确限制：`TerminateProcess`/Task Manager 硬杀无法在进程内拦截，可能留下 session；回调内 panic 跨 FFI 边界会 abort。彻底兜底需要后续稳定 session 所有权/启动恢复设计，不在本切片。
- `EventsLost`/`RealTimeBuffersLost` 在停止前查询，并在 STOP 返回后读取最终统计；任一来源可用即取较大值，两个来源都不可用时报告 `null` 并降级，不伪造 0。

## 4. 解码契约

本机 Windows 11 26200 的 TDH 元数据已确认：

- ProcessStart：v0–v4；ProcessStop：v0–v2。
- Payload 为 packed 布局（无自然对齐填充）；已用 TDH `TdhGetProperty` 对合成 payload 验证偏移。
- ProcessStart 的 `ImageName` 为 UnicodeString，是权威名称；ProcessStop 的 `ImageName` 为 AnsiString，按系统 ANSI 代码页 CP_ACP 解码（不是 UTF-8），可能被 provider 截断，仅作诊断，不用于身份。
- Start v3/v4 在 `ImageName` 前有 `MandatoryLabel` SID；SID 长度按 `SubAuthorityCount` 动态计算，不假定固定 12 字节。
- 未知事件 ID 或未知版本不猜测：事件 ID 不在 1/2 的按范围外忽略；1/2 的未知版本计为 decode error 并降级。
- 字段读取越界、SID 异常、身份字段缺失必须返回 decode error，不得生成“看似正常”的事件。
- 字符串未以 NUL 结尾、Unicode 含无效 UTF-16，或 ANSI 无法按 CP_ACP 解码时，保留可用部分并标记 `Partial`，同时进入健康统计；不静默替换为完整正常值。

本切片只解码到 `ImageName` 为止；其后的字段（如 ImageChecksum、PackageFullName、SecurityMitigations）明确不消费。

## 5. 配对契约

- 持久身份：`ProcessKey { pid, create_time }`。
- `ProcessSequenceNumber`（若 start/stop 都有）用于会话内交叉校验；不匹配时仍按身份配对，但计为完整性异常并使健康降级。
- 正常 start/stop：从 open 集合移除并计入 paired。
- orphan stop：stop 找不到对应 start，计数；在窗口边界属于正常现象，不单独降级。
- unpaired start：会话结束时仍未 stop 的进程，计数；在窗口边界属于正常现象，不单独降级。
- duplicate start：同一 `(pid, create_time)` 重复 start，计为完整性异常。
- PID reuse：同一 PID 出现不同 create_time 时按新身份处理；若旧身份仍处于 open，计为 `pid_reuse_detected`，不跨身份配对。
- open start 表上限 65,536；超过后不再插入，计 `start_overflow` 并使健康降级，避免长时间运行无界增长。
- 窗口内没有 ProcessStart 时，即使有 ProcessStop，也标记 `coverage_issue` 并降级；只有 stop 不证明 start 覆盖。
- 为验证解码正确性，最多保留 64 个不同的 start ImageName basename（每个名称最多 512 字符，去重、上限、截断标记）；不保留完整路径或命令行。

## 6. 健康与退出语义

健康计数至少包括：`events_received`、`events_processed`、`events_dropped`、`events_lost`、`realtime_buffers_lost`、`decode_errors`、`partial_events`、`ignored_events`、`last_event_at`、`last_error`。

状态：`not_started`、`starting`、`active`、`degraded`、`failed`、`stopped`、`unavailable`。状态优先级为失败/不可用高于降级，降级不会被后续 stop 覆盖。

`ok=true` 的必要条件：

- 状态为 `stopped`；
- 至少收到并处理过事件（零事件不得称为成功）；
- 至少观察到一个 ProcessStart（stop-only 窗口不得 `ok`）；
- 计数平衡：`received == processed + dropped + ignored`；
- 无 decode error、无 partial 事件；
- `events_lost == Some(0)` 且 `realtime_buffers_lost == Some(0)`（未知不等于 0）；
- 会话停止成功且无残留；
- 配对无 duplicate/sequence mismatch/PID reuse overlap/start overflow 完整性异常；
- 未被 Ctrl-C 中断，且实际采集时长达到请求窗口（`window_complete=true`）。

`window_complete=true` 要求：无失败/超时（`failure.is_none()`）、未被中断、trace 已打开且 `ProcessTrace` 返回 0、无残余 session，且实际采集时长达到请求窗口。失败或超时即使随后收到 worker 的 late success，也不得把窗口标记为完整。

`coverage=["process_start_stop"]` 只在一个完整且健康（`window_complete && health.ok`）的窗口内同时观察到至少一个 ProcessStart 和一个 ProcessStop 时声明；它不表示“provider 已启用”，也不表示系统已受保护。Ctrl-C、stop-only、超时 late result 或窗口不完整时 coverage 必须为空。

`protection_claim` 固定为 `"none"`。本切片不接入 ADR 0008 的 `Protected` 派生；即使 `ok=true` 也只表示“该采集窗口健康”，不表示系统安全。

退出码：

| 码 | 含义 |
| --- | --- |
| 0 | 会话创建、采集、停止、无残留，且健康 `ok` |
| 2 | 权限不足，fail closed；不创建/不残留 session |
| 3 | 降级：无事件/无 start、丢事件、解码失败、配对完整性异常、丢失统计未知、Ctrl-C 中断或窗口未跑满 |
| 4 | session 启动、provider 启用、`OpenTraceW` 或 `ProcessTrace` 失败 |
| 5 | stop 失败或存在残余 session |
| 6 | 参数错误或输出写入失败 |

## 7. 权限模型

- 普通用户直接运行，不自行提权。
- 当前权限不足以创建 ETW session 时，`StartTraceW` 的 access denied/privilege not held 映射为退出码 2，状态 `unavailable`，权限 `denied`，且不声称传感器运行。
- 管理员运行允许创建/停止 session。
- 本切片没有 UAC manifest、服务、提权 helper 或安装器；提权模型仍由后续独立设计处理。

## 8. 数据与隐私边界

- 只处理进程 start/stop；不读取命令行、键盘、屏幕、浏览器数据或文件内容。
- 不写数据库、ETL、网络或远程服务。
- JSON summary 只在显式 stdout/`--output` 路径产生；包含计数、健康、配对统计和最多 64 个 start ImageName basename。
- `last_error` 去除控制字符并限制 1024 字符；会话语义限长。

## 9. 已知限制

- 只覆盖进程 start/stop，不等于完整进程行为覆盖，更不等于系统保护。
- 依赖管理员权限；普通用户路径只能验证 fail-closed。
- 未做长时间稳定性、压力/丢事件注入、权限变化、系统策略变化或多会话并发测试。
- 未接入 Tauri UI、SQLite、告警或处置。
- `TerminateProcess` 硬杀与回调内 panic abort 无法在进程内清理；Ctrl-C/Ctrl-Break 已有管理员实机 PASS，控制台关闭/关机只保证尝试 stop，不保证 JSON 报告来得及落盘。
- stop 失败 watchdog 已实现并有退出码/窗口/超时 late-result 单测，但真实 STOP 失败注入仍无实机证据。
- 管理员 smoke 的 3/3 长名只验证 Start ImageName；Stop ANSI ImageName 不进入配对或长名断言。CP_ACP 非 ASCII stop 名有 CP936 单测，无真实非 ASCII 事件样本；`MB_ERR_INVALID_CHARS` 在非 UTF-8/GB18030 代码页上的行为未跨代码页验证，Stop 名称替换字符可能不计为 Partial。
- 提交前审查前的二进制 `2dbcd6...` 有 2026-10-31 的 20 秒 smoke PASS（`docs/VALIDATION.md` §10.6）。第一轮修复后的二进制 `c3496aa6...` 于 2026-10-31 04:02/04:03 完成管理员 20 秒 smoke + Ctrl-C 复验，均 PASS（36 events、0 loss、18 pairs、3/3 长名；Ctrl-C exit 3、no residual），见 §11.3。第二轮修复版 `5e0e118a...` 于 2026-10-31 04:24/04:31 完成管理员 smoke（36 events、0 loss、18 pairs、3/3 长名、window_complete）与 Ctrl-C（exit 3、interrupted、window incomplete、no residual）复验，均 PASS，见 §11.6。
- 尚无稳定 per-install GUID/`owned_guids` 启动恢复；硬杀后残留 session 需管理员处理或后续所有权设计。
