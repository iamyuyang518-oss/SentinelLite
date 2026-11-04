# ADR 0008：Sensor Health 与保护状态派生契约

- 状态：设计接受，**尚未实现**
- 日期：2026-10-29（Phase 1.2）
- 范围：Phase 2 的 ETW/事件传感器、健康监控和首页保护状态
- 明确：本文件不创建 ETW 会话、不伪造健康数据；当前产品仍没有实时传感器。

## 1. 背景

没有传感器健康数据时，零告警很容易被误解为“没有风险”。一个传感器“启动成功”也不等于它获得了完整权限、持续收到事件或没有丢事件。保护状态必须由后端的健康事实推导，不能让 UI 根据“没有告警”自行推断。

## 2. 数据模型

Rust 侧拟采用以下契约（字段名序列化为 camelCase）：

```rust
pub struct SensorHealth {
    pub sensor_id: String,                 // 稳定 ID，<= 128 字符
    pub state: SensorHealthState,          // 后端派生，不由 UI 设置
    pub started_at: Option<String>,        // RFC3339 UTC
    pub last_event_at: Option<String>,     // RFC3339 UTC；零事件时可为 None
    pub last_health_check_at: Option<String>, // 健康探针/tick，RFC3339 UTC
    pub events_received: u64,              // 当前后端生命周期累计
    pub events_processed: u64,             // 成功交给规则/存储管线
    pub events_dropped: u64,               // 有界队列溢出/消费者拒绝等
    pub events_delayed: u64,               // 超过延迟阈值的事件累计计数
    pub queue_depth: u32,                  // 当前队列深度
    pub queue_capacity: u32,               // 固定容量，必须 > 0
    pub permission_state: PermissionState, // Granted/Limited/Denied/Unknown
    pub coverage: Vec<CoverageDomain>,     // 已实际覆盖的事件域，非“计划覆盖”
    pub last_error: Option<String>,        // 有界、清理控制字符
    pub restart_count: u32,                 // 当前应用生命周期内重启次数
}

pub enum SensorHealthState {
    Unknown,
    NotStarted,
    Starting,
    Active,
    Degraded,
    Failed,
    Stopped,
    Unavailable,
}

pub enum PermissionState {
    Granted,
    Limited,
    Denied,
    Unknown,
}
```

- 所有计数使用饱和/显式溢出处理；不得回绕为“零”。
- `coverage` 是已验证覆盖域的白名单；空 coverage 不能是 Active 依据。
- 时间必须由后端时钟与单调计时器结合派生；UI 仅格式化展示。
- 建议默认阈值（Phase 2 通过实测确认，可配置）：健康间隔 5 秒；超过 15 秒标记 stale；超过 30 秒标记 Failed；队列高水位 80%；事件处理延迟阈值 5 秒。
- `events_delayed` 是累计事件数，不是当前延迟队列长度；当前队列长度由 `queue_depth` 表达。重启后计数和 restart_count 的世代必须明确标注，避免跨世代相加。
- 字段长度上限至少要覆盖 sensor_id 128、last_error 2048、每个 coverage 项 128，并去除控制字符/换行注入。队列容量和事件载荷必须固定上限。

## 3. 状态语义

| 状态 | 含义 | 是否拥有覆盖 |
| --- | --- | --- |
| `Unknown` | 后端没有可信健康快照，或无法判断传感器状态 | 否 |
| `NotStarted` | 采集尚未开始（用户未启用或尚未启动） | 否 |
| `Starting` | 已请求启动，但未完成首次健康检查/会话就绪 | 否，不能宣称正常 |
| `Active` | 健康检查新鲜、权限 Granted、队列低于高水位、无未处理丢失，且 coverage 非空 | 是，但仅限列出的 coverage |
| `Degraded` | 传感器还在运行，但覆盖不完整、权限受限、事件延迟/丢失或健康过期 | 部分 |
| `Failed` | 启动失败、消费者退出或不可恢复错误；last_error 保留 | 否 |
| `Stopped` | 用户或生命周期管理主动停止；不是故障，也不提供覆盖 | 否 |
| `Unavailable` | 平台/权限/系统策略不允许启动 | 否 |

“真正零事件”必须与采集未开始/不可用区分：

- 采集尚未开始：`NotStarted` 或 `Starting`，没有覆盖，不得显示“无事件风险”。
- 权限不足：`Unavailable` 或 `Degraded`（运行但 Limited/Denied），明确权限状态。
- 运行中零事件：只有健康检查新鲜、Active 且 coverage 明确时，才能称为“当前窗口没有收到事件”；仍然不能说没有风险，因为可能存在未覆盖行为或事件源静默。
- 数据不完整：`Degraded`，并在 UI 显示丢失/延迟/覆盖缺口。

## 4. 后端派生规则

保护状态只能由后端函数计算；前端只能显示后端返回的 `ProtectionState`、传感器字段和原因，不得自行把 `Unknown`/`Degraded` 覆盖成 `Protected`。

建议派生顺序：

1. 若存在待处理的高风险证据：`RiskDetected`（不能声称 Protected）。
2. 若没有任何关键传感器启动过，或没有健康快照：`Unknown`。
3. 若关键传感器有 `Failed`/`Unavailable`/权限不足/健康过期/事件丢失/队列超阈值：`Degraded`。
4. 若关键传感器被用户主动停止或处于 `NotStarted`：`Unknown`（不是 Protected；UI 说明没有连续覆盖）。
5. 只有所有关键传感器 `Active`、健康时间新鲜、`permission_state == Granted`、`coverage` 覆盖必要域、`events_dropped == 0`、延迟低于阈值、队列低于高水位且没有高风险的未处理发现时，才允许 `Protected`。
6. 零告警不是第 5 条的证据；它只代表当前没有可展示事件。

任何关键传感器 `Active` 但 `last_health_check_at` 过期，必须至少 `Degraded`；不能因为最后一次事件很久以前没有告警而保持 Protected。

## 5. PID 与事件时间关联

- 事件必须携带事件时间、PID 和可获得的进程创建时间/稳定身份令牌。
- 只有当事件时间位于该进程创建时间之后、退出时间之前，且身份令牌与事件来源匹配时，才可建立关联。
- 若创建时间缺失、PID 复用或事件时间与身份不一致，关联状态必须是 `Unknown`，不得把两个 PID 世代拼成一个父子链。
- 事件延迟处理时使用原始事件时间，不使用“写入数据库时间”替代。
- 进程启动事件、父进程引用和 ETW 事件之间必须保留身份字段；仅相同数值的 PID 不是相同进程。

## 6. 队列、丢失和背压

- 事件队列必须有固定容量（建议默认 4096，最终以负载测试确认）和固定最大事件载荷。
- 队列满时：优先采用有界背压；若无法背压，必须增加 `events_dropped`，记录受限错误/计数，并把状态降级。不得静默丢弃。
- 每个消费者有独立队列深度、延迟和错误计数；全局计数也必须可追溯。
- 健康检查本身不得无限重试或阻塞主处理线程。
- 重试必须有次数上限和退避；超过上限进入 Failed/Degraded，并保留 last_error。
- 任何新的传感器在未通过健康、权限、覆盖、丢失和降级测试前，首页不能显示 Protected。

## 7. UI 契约

- UI 显示 `state`、`permission_state`、`last_health_check_at`、队列深度/容量、received/processed/dropped/delayed、coverage 和 last_error。
- UI 不得从 `events_received == 0` 推导“没有威胁”；必须显示“当前窗口未收到事件”与“覆盖不足”的区别。
- UI 不得用颜色或排序覆盖后端状态；`Active` 只表示传感器健康，不表示系统安全。
- 历史不可用、传感器失败、启动中、未启动和主动停止都必须有独立文案。
- 首页必须提供指向覆盖矩阵和已知盲区的可见入口。

## 8. 测试规范（Phase 2 实施前）

至少覆盖：

1. 未启动、启动中、Active、Degraded、Failed、Stopped、Unavailable、Unknown 的派生；特别区分 NotStarted 与 Stopped、Unknown 与 Failed。
2. 健康检查新鲜但零事件：Active 可成立，但 Protected 仍需 coverage/权限/无丢失和风险条件；零告警不能生成“安全”结论。
3. 健康检查过期：状态至少 Degraded，禁止 Protected。
4. 权限 Granted/Limited/Denied/Unknown 的边界。
5. 事件丢失、延迟、队列高水位和队列溢出的计数、可见 issue 与降级。
6. 消费者崩溃/重启：restart_count、last_error 和计数器世代。
7. PID 复用：事件时间与创建时间不匹配时不建立关联。
8. 后端派生状态的单元测试：任何关键传感器非 Active 或字段异常时，Protected 不可能为真。
9. UI 测试：UI 不能覆盖后端状态，不能把 Unknown/Degraded/Failed/Stopped 显示为 Protected。

在真实 ETW 负载、权限变化、事件丢失和长时间运行测试完成之前，Sensor Health 不能使首页出现 `Protected`。本 ADR 只定义设计，不提供任何真实健康数据。
