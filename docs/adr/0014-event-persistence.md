# ADR 0014：本地事件持久化

- 状态：接受，Phase 2.6 已实现并完成 Windows 实机验证（2026-10-31）
- 范围：`crates/sentinel-storage`、`src-tauri/src/storage_state.rs`、`src-tauri/src/commands.rs`
- 关系：实现 [ADR 0009](0009-sqlite-audit-storage.md) 的事件存储部分；消费 [ADR 0013](0013-process-event-delivery.md) 的事件流

## 1. 背景

事件投递解决了传感器到主应用的传输，但应用关闭后事件仍会丢失。需要本地历史查询、重启后恢复、配对修正落盘和数据质量记录，同时不能阻塞传感器采集。

## 2. 决策

### 2.1 位置与权限

- 数据库位于 Tauri 解析的当前用户本地应用数据目录：`%LOCALAPPDATA%\app.sentinellite.desktop\SentinelLite\events.db`。
- 不使用项目目录或临时目录。
- 不创建服务、计划任务或全局可写路径。
- 文件继承 `%LOCALAPPDATA%` 的按用户 ACL。实机 `icacls` 显示 `SYSTEM`、`Administrators` 和当前用户；没有 Everyone/Users 访问。
- 不宣称 SQLite 加密；当前没有实现静态加密。保护依赖 Windows 用户 ACL 和磁盘权限。

### 2.2 Schema v2

表：

- `sessions`：会话 ID、首次/最近观察时间、最后游标、是否关闭、结束原因/时间/错误、完整 `SessionEnd` JSON（新记录含 `final_stats`；旧 JSON 缺少该字段时按未知读取）。
- `process_events`：稳定 `event_id` 主键，会话、序号、类型、PID、创建时间、映像、事件时间、三类时间、配对状态/顺序/配对事件、完整 `ProcessEvent` JSON。
- `invalid_events`：无法标准化的原始事件记录。
- `pairing_revisions`：以 `stop_event_id` 为主键的修正记录和原始 JSON。
- `ingestion_gaps`：投递窗口过期或缺口。
- `ingestion_state`：应用重启后恢复的当前会话与游标。

索引覆盖会话、事件时间、PID、事件类型、映像名和修正会话。`PRAGMA user_version` 记录 schema 版本；v1→v2 迁移修复了 in-order 退出事件曾把自身写成配对事件的问题。

### 2.3 幂等写入与配对修正

- 每个事件按 `event_id` `INSERT OR IGNORE`；重放同一批次不会产生重复行。
- `PairingRevision` 先按 `stop_event_id` 幂等插入，再更新原退出事件的 `record_json`、`pairing_state=paired`、`pairing_order=out_of_order` 和 `paired_event_id`。
- 如果修正先于退出事件到达，修正保留；退出事件插入时查找并应用。
- `SessionEnd` 以会话行为单位更新，不覆盖首次观察时间。
- 每批一个 SQLite 事务；游标在同一事务中推进，避免“事件写了、游标没写”或反向。

### 2.4 采集与应用隔离

- 传感器不打开 SQLite。主应用后台 `sentinel-storage-ingest` 线程通过生产客户端策略轮询事件管道。
- 本地查询走同一存储互斥锁；查询有 200 行上限和 100,000 偏移上限，单次操作短。传感器采集完全不受数据库锁影响。
- 应用启动时打开数据库、执行 `quick_check`、迁移和保留策略；失败时应用继续运行，状态和查询返回明确的 `storageUnavailable`，不会删除或重建数据库。
- 传感器缺失是正常状态：ingestion 继续以最多 2 秒的间隔重试，避免短采集会话被长退避错过。
- UI 实时列表和持久化 worker 是两个消费者。传感器在任一客户端确认 `SessionEnd` 后额外保留端点 2 秒，让另一个消费者读取尾部；实机在事件页打开时验证了 28/28 条和 `SessionEnd` 落库。

### 2.5 保留与容量

默认策略：30 天、1,000,000 条事件、512 MiB。打开时和每 100 个成功批次执行：

- 按事件时间删除过期事件/无效事件/修正/缺口；
- 按事件数删除最旧记录；
- 会话数超过 20,000 时删除已关闭且无事件的旧会话；
- 文件超过字节预算时按 10,000 条一批删除最旧事件，并执行增量 vacuum（最多 10 批）。

SQLite 可能不会立即把释放页归还文件系统；状态报告实际主文件 + WAL 大小。WAL 模式允许读取与写入并发。

### 2.6 应用命令

新增只读命令：`get_storage_status`、`query_stored_events`、`get_stored_event`、`list_sensor_sessions`。所有参数有长度/范围校验，命令加入 Tauri 应用级 ACL。Tauri 退出事件停止 ingestion 线程。

### 2.7 明确不做

- 不存命令行、文件内容、网络内容、键盘或屏幕数据。
- 不上传、不同步、不创建云备份。
- 不自动清理用户数据；保留策略是本地容量边界。
- 不把 SQLite 当作防篡改审计链；本地管理员仍可修改数据库。
- 不宣称数据库加密。

## 3. 后果

优点：

- 应用重启后可查询历史；事件与配对修正持久且幂等。
- 数据路径、Schema、权限、保留策略和失败状态明确。
- 数据库异常不影响传感器采集。

限制：

- 应用未运行时事件不会写入历史；传感器投递窗口不会无限保留。
- 查询与写入共享一个连接互斥锁；极端长查询会短暂延迟 ingestion，但不会延迟 ETW。
- 没有静态加密、完整性签名或防篡改保护。
- 新 `SessionEnd` 会持久化 `final_stats`；旧数据库中的历史会话没有该块，检测仍显示“未知”。ETW 停止失败或残余 session 时最终统计保持未知，不填 0。新增字段不改变 schema v2，老 JSON 通过 serde 默认值读取。

## 4. 验收

- 自动化：schema v2、重开迁移、重复批次幂等、修正更新原记录、修正先到、查询/过滤/分页、会话结束、未观察会话、保留策略、加载截断、WAL 字节统计。
- 真实 Windows：非提权应用连接提权传感器，数据库写入 28/28 事件、最近会话 `interrupted`、`SessionEnd` 已保存、0 缺口、0 无效记录、schema v2；`icacls` 验证路径 ACL；应用重启后数据库可重新打开并查询。
