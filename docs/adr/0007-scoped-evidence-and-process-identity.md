# ADR 0007：作用域化证据、无 Protected 假设与进程创建时间身份

- 状态：接受
- 日期：2026-10-29（Phase 1.1）

## 背景

审计发现三类语义风险：

1. 正常 CPU 亲和性曾被作为进程级“无问题/绿色”结论呈现。
2. 没有真实传感器时仍存在 `ProtectionState::Protected` 枚举值，未来可能被直接误用。
3. 进程检查先按 PID 取快照，再按 PID 打开句柄，存在 PID 复用/TOCTOU 混合风险。

## 决策

### 风险作用域

- `RiskAssessment` 增加 `scope`：`processOverall`、`affinityConfiguration`、
  `softwareMetadata`、`startupMetadata`。
- `NoClearIssue` 改为中性色、标签“检查范围内未发现异常”，仅表示已检查证据范围。
- `AffinityInspection` 分别返回 `affinityRisk` 与 `overallRisk`。没有独立行为证据时，
  `overallRisk` 固定为 `InsufficientEvidence`，UI 显式显示“整体进程结论：证据不足”。
- 软件/启动项风险同样标明 scope，不能外推为对象整体安全。

### 保护状态

- 删除 `ProtectionState::Protected`（Rust 与 TypeScript）。Phase 1 没有任何传感器健康
  模型可以证明该状态。
- 首页继续返回 `Unknown`。只有未来的健康模型可以重新引入 `protected`，并且必须通过
  新 ADR、测试与传感器 health/loss 语义，而不是由“没有发现”推导。

### 进程身份

- 进程列表返回高精度 FILETIME 创建时间哈希令牌 `identity`，以及供 UI 显示的
  `startTime`。
- `inspect_process_affinity(pid, identity)` 在 OpenProcess 后、采样前和采样后读取
  创建时间并与令牌比较；任何不一致返回 `processIdentityChanged`。
- 检查对象中的名称来自已验证句柄的磁盘路径，父进程信息不混入检查结果，避免把旧 PID
  的 name/parent 与新进程的亲和性/签名拼接。
- 无身份令牌或路径不可读时不生成“部分拼接”对象，而是返回可重试错误。

## 后果

- PID 不再是唯一身份；未来 ETW 事件可使用同一创建时间原语关联。
- 受保护或身份时间不可读的进程无法进入亲和性检查，必须刷新或保持未知。
- UI/契约新增 identity 参数；Tauri ACL 仍限制命令集合，但身份值由后端核对。
- 风险标签更多，但不会制造新的检测能力；这是语义收紧而非能力扩张。

## 未来约束

- 任何 ETW/SQLite 事件都必须携带对象身份字段和明确 scope。
- 不得重新添加 `Protected` 或“整体安全”的绿色结论，除非真实 health model 与独立行为
  证据存在并通过评审。
