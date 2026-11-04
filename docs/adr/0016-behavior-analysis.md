# ADR 0016：可解释的基础行为分析

- 状态：接受，Phase 2.8 已实现并完成 Windows 实机验证（2026-10-31）
- 范围：`crates/sentinel-detection`、`src-tauri/src/commands.rs` 的 `analyze_session`、事件页行为分析面板
- 关系：消费 [ADR 0014](0014-event-persistence.md) 的历史事件；不做处置

## 1. 背景

在可靠的进程事件和本地历史之上，需要第一层行为分析。现有数据只包含进程 start/stop、PID/父 PID、映像、时间、配对和传感器健康；没有命令行、文件、网络或内存证据。分析必须对数据不足保持保守，不能把单条异常事件认定为恶意。

## 2. 决策

### 2.1 输入质量优先

`DetectionContext` 明确记录：

- 会话是否已结束（`SessionEnd`）；
- ETW `events_lost` / `realtime_buffers_lost` 是否已知；
- 管道丢弃、无法解码事件、投递缺口、事件加载截断；
- 缺少时间戳/映像名称的事件数；
- 传感器是否健康。

如果存在已知缺口，分析返回 `data_insufficient`；如果 ETW 丢失未知，返回 `indeterminate` 并明确说明不能宣称正常；如果会话未结束，返回 `data_insufficient`。只有数据完整且无规则命中时才返回 `no_anomaly_detected`。

### 2.2 规则（版本 1）

- `PROC-BURST-001`：10 秒窗口内 ≥30 次进程启动 → `suspicious`，严重程度中，置信度中。
- `PROC-EXIT-002`：10 秒窗口内 ≥20 次进程退出 → `suspicious`，严重程度低，置信度中。
- `PROC-ORPHAN-003`：会话事件 ≥20 且无法配对/身份不确定比例 ≥30% → `anomaly`，严重程度低，置信度高；解释为数据完整性异常。
- `PROC-INTEGRITY-004`：出现重复启动/重复退出/同 PID 身份不匹配 → `anomaly`，严重程度中，置信度高；解释为生命周期一致性异常。

窗口/阈值是固定默认配置，测试可覆盖。每条结果包含规则 ID、版本、状态、严重程度、置信度、阈值、窗口、证据事件 ID、误报说明和解释。

### 2.3 状态与语言

- `no_anomaly_detected`：仅在输入完整且无规则命中时使用；仍声明不是安全证明。
- `suspicious`：需要人工核实，不等于恶意。
- `anomaly`：可验证的生命周期/数据一致性异常，不等于恶意判定。
- `data_insufficient`：样本不足、会话未结束或存在已知缺口。
- `indeterminate`：关键数据未知（如 ETW 丢失），不能判断。

整体状态优先级：`anomaly` > `data_insufficient` > `suspicious` > `indeterminate` > `no_anomaly_detected`。数据缺口仍会作为独立结果出现。

### 2.4 可追溯性与 UI

- 结果引用存储中的 `event_id`；UI 可点击证据加载对应历史详情。
- 规则目录（ID、数据需求、误报说明、默认阈值）通过 `rule_catalog()` 提供。
- 新格式的 `SessionEnd.final_stats` 提供最终 ETW 丢失和管道计数；分析优先使用该持久化数据，没有时才回退到当前实时传感器健康。旧记录没有 `final_stats`，仍显示“ETW 丢失统计未知”；这是真实限制，不伪造成 0。
- 分析不自动运行；用户选择会话后点击“分析会话”。

### 2.5 明确不做

- 不自动结束/隔离/删除/修改系统。
- 不把进程名称、父子关系或单条事件单独判定为恶意。
- 不引入 YARA-X/Sigma、文件扫描、网络分析或云端信誉。
- 不宣称恶意软件检出率或替代 Defender。

## 3. 后果

优点：

- 首批规则可解释、可测试、带版本和误报说明。
- 数据不足/未知与正常明确分开。
- 证据可回溯到持久化事件。

限制：

- 规则只使用 start/stop、时间和配对；无法看到命令行、文件、网络或内存。
- 旧历史会话没有最终 ETW 统计，常导致整体 `indeterminate` 或 `data_insufficient`；新会话若最终统计已知为零，仍需满足样本/会话结束等其他门禁。
- 阈值是最小可用默认值，尚未用大规模正常/恶意样本校准；误报/漏报率未知。
- 没有跨会话关联、进程树重建或持久化基线。

## 4. 验收

- 自动化：正常样本无规则命中；30 次启动/退出触发 burst/exit；孤儿比例触发数据异常；重复启动触发一致性异常；样本不足/未结束/已知丢失/未知 ETW/缺时间均不返回正常；证据 ID 有界；规则目录版本/误报说明完整。
- 真实 Windows：应用已在运行，传感器采集 170 条真实事件（40 个短生命周期 `cmd.exe` 刺激），应用写入 SQLite；分析探测输出 `overall=suspicious`，触发 `PROC-BURST-001`、`PROC-EXIT-002`，证据 64 个事件 ID，含误报说明，解释明确“不判断恶意性”；`DATA-ETW-UNKNOWN-001` 同时显示 ETW 丢失未知。UI 截图 `docs/screenshots/events-analysis.png`。
