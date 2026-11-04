# ADR 0012：有界进程事件管道与标准化事件模型

- 状态：接受，Phase 2.4 已实现并完成 Windows 实机验证（2026-10-31）
- 日期：2026-10-31
- 范围：`crates/sentinel-sensor/src/{event,pipeline,pair,time,etw}.rs`
- 关系：扩展 [ADR 0010](0010-sensor-architecture.md)；不改变 [ADR 0011](0011-sensor-ipc.md) 的状态 IPC、Tauri 命令或前端

## 1. 背景

Phase 2.1–2.3 的传感器在 ETW 回调线程中直接解码、加锁配对并更新健康计数。这个切片能工作，但存在四个结构性问题：

1. 没有独立的事件身份；配对只存在于状态机内部，无法从结果追溯到具体事件。
2. 没有处理重复 Stop、乱序 Start/Stop、PID 复用导致的身份错位或“创建时间不可用”的不确定性。
3. 没有有界队列；如果未来加入事件持久化或更慢的消费者，ETW 回调将被拖慢或内存无界增长。
4. 没有区分 ETW session 丢失与管道丢弃，也没有消费者退出、队列积压或关闭排空的明确语义。

Phase 2.4 只解决事件模型、标准化、配对、有界缓冲与完整性统计，不引入检测、告警、事件 UI 或持久化。

## 2. 决策

### 2.1 标准化事件

每个成功解码的 Kernel-Process start/stop 事件产生一个 `ProcessEvent`：

- `event_id = <session_id>:<sequence, 16 hex>`，`sequence` 在每个采集会话内单调递增。
- `session_id` 由进程 ID + Unix 纳秒时间生成；传感器重启后不会与旧会话混淆。
- `event_type`：`started` / `stopped`；`source`：`microsoft_windows_kernel_process`。
- `identity = {pid, create_time}`。`create_time` 为 ETW CreateTime（FILETIME 身份令牌）；零值不是可用身份，标记为 `indeterminate` 且不参与配对。
- `parent_pid` / `image_name` 保留原始字段；缺失时保持 `None` 并在 `completeness` 中说明，不编造路径或父 PID。
- 三个时钟严格分开：
  - `etw.occurred_at`：ETW 记录头时间戳；
  - `received_at`：回调把记录放入有界队列的时间；
  - `normalized_at`：消费者生成标准化事件的时间。
- `process_started_at` 由 `create_time` 转换；转换失败时为 `None` 并记录数据问题。
- `completeness` 显式记录身份是否确定、事件/接收/标准化时间是否已知、父 PID 是否可用、ImageName 的 `present/partial/missing`、ETW sequence 是否可用，以及 `issues`（有界、去重）。
- `pairing` 四态：`paired`、`waiting`、`unpaired { reason }`、`indeterminate { reason }`。配对引用完整的事件 ID。

无法解码或版本不支持的事件不会被静默丢弃：它们进入同一条有界队列并产生 `InvalidProcessEvent`（事件 ID、ETW event/version、payload 长度、错误、时间戳）；健康计数记为 decode error。非 Kernel-Process provider 或非 start/stop 的 ETW 事件属于过滤范围，单独计入 `filtered_ignored` / `ignored_events`，不冒充进程事件。

### 2.2 配对规则

- 身份键始终是 `(pid, create_time)`，绝不只按 PID 配对。
- Start 正常打开 → `waiting`；对应 Stop 到达 → `paired { order: in_order }`。
- 重复 Start（仍在打开或已关闭）→ `unpaired { duplicate_start }`，保留首个事件 ID。
- 重复 Stop（身份已关闭或已有孤儿 Stop）→ `unpaired { duplicate_stop }`。
- 没有 Start 的 Stop → 保留在最多 4,096 条的有界孤儿表中；事件标为 `unpaired { stop_without_observed_start }`。如果之后的 Start 匹配，Start 标为 `paired { order: out_of_order }`，并额外产生一条 `PairingRevision` 指向先前 Stop；最终摘要记录 `out_of_order_pairs`。
- Stop 的 `(pid, create_time)` 与当前同 PID 打开身份不同 → `unpaired { identity_mismatch }`，不会跨身份合并；该 Stop 仍可进入有界孤儿表等待自己的 Start。
- 打开 Start 使用 65,536 上限；关闭身份使用 65,536 上限（FIFO 淘汰并计数）；孤儿 Stop 超出上限时不再保留，但事件和原因仍被记录。
- `sequence` 只在双方都存在时交叉校验；不一致会标记但身份仍可配对。缺失 sequence 不算不一致。
- 会话结束时仍打开的 Start 由 `SessionEnd.open_start_events`（最多 32 个样本）和配对摘要报告；它们不是错误，但明确表示生命周期未完成。
- 重复、乱序、pid reuse、sequence 不一致、身份不确定、容量淘汰都计入配对摘要，并使健康失败关闭（`has_integrity_issue`）。

### 2.3 有界队列与背压

- 使用 `std::sync::mpsc::sync_channel`，容量默认 4,096，可通过 `--queue-capacity` 配置为 64–65,536。
- ETW 回调只做有界 schema 解码、ImageName 截断（1,024 字符）和非阻塞 `try_send`；不在回调中做配对、sink 写入或 last_error 锁。
- 队列满时丢弃**最新**事件并立即返回；ETW 线程从不阻塞。丢弃计数为 `dropped_queue_full`；消费者已消失/管道已关闭时为 `dropped_consumer_gone`；两者都同步增加健康 `events_dropped`。
- 消费者线程负责标准化、配对、健康计数、诊断样本和可选 sink。生产 sink 只计数；测试 sink 可以记录、阻塞或报错，用于验证背压、排空和错误隔离。
- 统计字段：`received`、`enqueued`、`consumed`、`normalized`、`invalid`、`dropped_queue_full`、`dropped_consumer_gone`、`filtered_ignored`、`backlog`、`peak_backlog`、`sink_errors`。
- `backlog` 表示“已入队但消费者尚未完成接收记账”的数量，包含消费者当前持有的事件，因此瞬时峰值可能是 `capacity + 1`；实际 channel 占用严格不超过 `capacity`。
- 默认容量的依据：进程 start/stop 的典型事件远小于 1 KiB；4,096 条足以吸收数秒的突发，最坏情况下仍由容量和 1,024 字符 ImageName 上限约束。真实机器 smoke 的峰值积压为 10–11，远低于默认容量；不同机器/负载仍需独立压测。

### 2.4 生命周期

- 正常关闭时 `close()` 设置结束原因、丢弃生产者 sender，消费者先排空队列，再向 sink 发送 `SessionEnd`（原因、错误、仍打开的 Start 事件 ID 样本、最终配对摘要），最后退出。
- `close()` 等待消费者最多 5 秒；超时则记录 `shutdown_timed_out` 与超时瞬间的 `backlog_at_shutdown_timeout`，不假装队列为空，并让健康降级。
- 消费者异常退出时 sender 的 `try_send` 返回 Disconnected，生产者记录 `dropped_consumer_gone` 并把消费者状态置为 failed；最终健康为 failed。
- 消费者 panic 由 `catch_unwind` 捕获并标记 failed；sink 错误只记录 `sink_errors`/错误消息，消费者继续排空，避免一个慢或坏的 sink 阻塞 ETW。
- `TerminateProcess` 硬杀无法写 summary，也无法发送 `SessionEnd`；内存中的队列和未完成生命周期会丢失。这是明确的残余风险，不用其他机制伪装成已恢复。

### 2.5 健康与丢失

- ETW session 丢失（`events_lost`、`realtime_buffers_lost`）与管道丢弃（`dropped_queue_full`、`dropped_consumer_gone`）是不同字段，不合并。
- 健康 `ok`/`health_ok` 仍要求零 ETW 丢失、零解码错误、零 partial、零管道丢弃、消费者正常运行、配对覆盖/完整性通过。
- 关闭后执行计数对账：`events_processed == pipeline.normalized`、`events_dropped == pipeline.invalid + pipeline.dropped_total()`、`ignored_events == pipeline.filtered_ignored`、`pipeline.received == events_processed + events_dropped`；不一致会标记 degraded，而不是静默接受。
- 队列丢弃、消费者失败、sink 错误、关闭超时和配对完整性异常都会使健康失败关闭；不会产生“保护正常”。

### 2.6 明确不做

- 不持久化事件、不引入 SQLite/ETL/数据库迁移。
- 不提供实时事件 UI、告警队列、行为检测、威胁评分或自动处置。
- 不修改 `sentinel-ipc` 协议、状态响应字段、Tauri 命令或前端。
- 不改变 ETW provider、关键字、session 所有权或 Phase 2.1–2.3 的停止/残留语义。
- 不把标准化事件发送到主应用；主应用仍只能读取状态/健康/计数。

## 3. 后果

优点：

- 事件有稳定身份、会话身份、三类时间、字段完整性和可追溯的配对结果。
- ETW 回调不再执行配对/状态更新，队列有明确上限，背压可观测且不阻塞采集。
- 重复 Stop、乱序、PID 复用、身份不确定、消费者退出和关闭排空都有测试与明确语义。
- ETW 丢失与管道丢失分离，便于未来持久化与告警系统使用。

成本与限制：

- 回调仍执行 schema 解码和字符串分配；这是必要的解码工作，但高峰期仍有 CPU 成本。
- 生产 sink 只保留有界诊断样本（最近 8 条、首个 paired/unpaired/invalid、首个 revision、SessionEnd）；完整事件流需要未来持久化，不在本阶段。
- 有界身份表意味着极端长时间/高流失会话可能发生关闭身份淘汰；该情况被计数并使健康降级，而不是静默猜测。
- 乱序 Stop 的原始记录先以 `unpaired` 发出，随后由 `PairingRevision` 修正；没有持久化时，下游必须同时消费 revision 才能得到最终配对。
- 硬杀丢失内存队列；没有恢复承诺。

## 4. 验收

- 自动化：正常 Start/Stop、孤儿 Stop、重复 Start/Stop、PID 重用、乱序回补、缺失父/映像、身份不确定、队列满载、消费者退出、正常关闭排空、关闭超时、sink 错误、持续 20,000 事件负载、会话重启隔离、解码到标准化链、健康计数对账。
- 真实 Windows：20 秒管理员 sensor smoke（20 events、9 paired、0 dropped、peak backlog 11、session end completed、无残留）；Ctrl-C `--serve` smoke（28 events、28 processed、backlog 0、session end interrupted、2 个未完成 Start、无残留）；提权 probe 在运行中读取 `active/granted`、17/17、`health_ok=true`、`last_error=null`，随后 Ctrl-C 关闭且无残留。
- 未做：真实队列溢出注入、硬杀后的管道恢复、长时间稳定性、跨用户/并发压力；这些在 VALIDATION 与 KNOWN_LIMITATIONS 中单列，不能把自动化压力测试写成实机 PASS。
