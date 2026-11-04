# ADR 0013：标准化进程事件的安全投递

- 状态：接受，Phase 2.5 已实现并完成 Windows 实机验证（2026-10-31）
- 范围：`crates/sentinel-events`、`crates/sentinel-ipc/src/{events,event_server}.rs`、`crates/sentinel-sensor/src/delivery.rs`、`crates/sentinel-sensor/src/etw.rs`
- 关系：扩展 [ADR 0011](0011-sensor-ipc.md)；不改变状态查询协议

## 1. 背景

Phase 2.4 的标准化事件只存在于传感器进程内部。主应用只能读取状态，不能获得事件、配对修正或会话结束。事件模型需要成为传感器、IPC、持久化和检测共享的稳定契约，而不是各自复制结构。

## 2. 决策

### 2.1 共享事件模型

新增 `sentinel-events` crate，包含 `ProcessEvent`、`InvalidProcessEvent`、`PairingRevision`、`SessionEnd`、`PairingSnapshot`、`PipelineRecord` 和时间转换。传感器、IPC 和应用侧使用同一份 serde 定义；所有类型可序列化和反序列化。

### 2.2 独立事件管道

新增只读命名管道 `\\.\pipe\SentinelLiteSensor.Events.v1`，与状态管道分离。两者使用相同的：

- 当前用户/SYSTEM/Administrators DACL；
- `PIPE_REJECT_REMOTE_CLIENTS`；
- 客户端 SID 校验；
- 客户端对服务端镜像名和提权的校验；
- 消息模式、请求/响应上限和 2 秒 I/O 超时。

事件批次不会阻塞健康查询。事件服务器一次处理一个短请求，响应大小上限 256 KiB；超限时从尾部截断记录并设置 `has_more=true`，单个记录本身受事件模型限制。

### 2.3 游标协议

请求：

```json
{"version":1,"request":"events","after":0,"limit":64}
```

响应批次包含：

- `session_id`、`oldest_cursor`、`latest_cursor`、`next_cursor`；
- `cursor_expired`、`missed_records`、`cursor_ahead`；
- `evictions`、`session_closed`、`has_more`；
- `records: [{cursor, record}]`，`record` 为 `process_event` / `invalid_event` / `pairing_revision` / `session_end`。

游标单调递增。重复读取在保留窗口内是幂等的；游标落后于最旧记录时明确报告缺失数量，不返回静默空洞；游标超过当前会话最新值（通常来自传感器重启）时 `cursor_ahead=true`，由客户端重置。

### 2.4 有界投递窗口

传感器消费者把记录追加到 `EventBroker`，容量默认 4,096，可用 `--event-buffer` 配置为 64–65,536。窗口满时淘汰最旧记录并累计 `evictions`。这是投递缓存，不是 ETW 或管道丢失：没有客户端读取不会让采集健康失败，但客户端会通过 `cursor_expired`/`missed_records` 看到缺口。

`SessionEnd` 追加后窗口关闭，不再产生记录；传感器在存在读取客户端时最多等待 2 秒，直到客户端确认读取了 `SessionEnd`，随后再保留端点 2 秒，让第二个消费者（UI 和持久化 worker）读取尾部，然后停止事件端点。没有客户端时立即停止。该保留是有界尽力而为。硬杀仍会丢失内存窗口。

### 2.5 生命周期与统计

- `RunReport` 增加 `event_ipc_*` 和 `delivery` 快照；summary 记录 `batches_served`、`evictions`、`session_end_delivered`。
- `SessionEnd` 增加 `final_stats`：最终 ETW `events_lost`/`realtime_buffers_lost`（仅在停止成功且无残余 session 时已知，否则为 `null`）以及 pipeline 接收/消费/标准化/无效/队列丢弃/过滤/最终积压/峰值/关闭超时/ sink 错误。旧记录没有该块，不能当作零。
- 事件服务器启动失败按状态 IPC 同样的 fail-closed 规则处理：可恢复错误降级健康，不可恢复的权限恢复错误终止会话。
- 事件端点运行时故障由客户端查询失败观察；状态管道不直接探测其线程状态（见已知限制）。

### 2.6 明确不做

- 不修改状态协议版本或旧状态查询响应。
- 不持久化事件、不提供 UI 或检测规则。
- 不开放任意用户访问、不提供事件写入或控制命令。
- 不把投递窗口当作完整历史。

## 3. 后果

优点：

- 事件、修正和会话结束有稳定、可追溯的游标语义。
- 状态查询和事件读取互不阻塞；慢消费者不会阻塞 ETW。
- 传感器重启、游标过期和投递淘汰都能被客户端明确识别。
- 共享模型避免 IPC、存储和检测的字段漂移。

成本与限制：

- 投递窗口是内存缓存；应用未运行时事件不会被保留。
- 传感器退出后 `SessionEnd` 只保留一个短时间窗口，未被读取则随进程退出丢失。
- 多个客户端共享同一个单实例事件端点，传感器只等待任一客户端确认 `SessionEnd`；第二个客户端可能错过尾部（实机多客户端测试中观察到一次）。
- 事件端点运行时线程失败不会自动反映到状态管道。

## 4. 验收

- 自动化：协议编解码、请求上限、响应截断、游标排序/幂等/过期/超前、会话重启、SessionEnd 保留与确认、事件服务器顺序客户端、客户端断开、管道边界、完整解码→标准化→broker→IPC 链。
- 真实 Windows：管理员 `--serve` 采集，提权 watcher 通过生产客户端策略读取 32 个进程事件、33 条记录、13 个已配对退出、`SessionEnd=interrupted`、0 游标过期、0 丢失、无残留 session；传感器 summary `delivery.session_end_delivered=true`、`batches_served=84`。
- 配对修正的 IPC 传递由自动化端到端测试证明；真实 smoke 未出现乱序修正。
