# 路线图

这是一份能力顺序建议，不是交付承诺。任何阶段都必须保留“未知/降级”语义，不能为了演示效果提前宣称已保护。

## 已完成：第一阶段只读纵向切片

- Rust core + Tauri 2 + React/TypeScript 基础架构。
- 软件盘点、启动项、进程快照、CPU 亲和性、签名与短时 CPU 采样。
- 证据化风险分类、检查时间、读取问题与盲区。
- 首页在无持续监控时显示状态未知。
- 虚构数据单元测试、只读主机冒烟、Windows CI。
- 产品范围、威胁模型、清洗规范、架构决策、检测覆盖、隐私与安全文档。

## 已完成：Phase 1.1 安全修正

- 风险作用域：亲和性结论与进程整体结论分离；删除 `Protected`；NoClearIssue 改为中性“检查范围内未发现异常”。
- 软件分类：读取 SystemComponent/NoRemove/ParentKeyName/ReleaseType，区分系统/组件类条目，未知类别不猜测。
- 输入边界：注册表每视图/总条目、字符串、磁盘哈希有硬上限；结果级 `truncated` + DataIssue；UI 50 行分页。
- HKCU 去重：用户注册表只读共享视图一次，HKLM 保留 32/64；只读验证从旧 187 含重复降为 183 唯一条目、启动项从 23 降为 14。
- 启动路径：引号/未加引号歧义解析，`unquotedPath` 单独记录；路径区分 Present/Missing/AccessDenied/Unknown。
- 进程身份：创建时间令牌绑定 PID，前后核验；不再混合旧 PID 名称/父进程与新进程数据。
- Tauri AppManifest 应用级 command ACL：五个 allow 权限，capability 无 core:default，release 实测通过。
- 磁盘签名/元数据/有限 SHA-256 证据；明确磁盘文件 ≠ 内存映像。
- 当时 npm audit 0 漏洞；Dependabot 配置与 ADR；Cargo advisory 扫描延后记录（历史结论；当前审计见 THIRD_PARTY.md）。
- 测试/文档同步：新增 H1–H4、M1–M4、M3、M6–M7、L1 的回归覆盖。

## 已完成：Phase 1.2 输入边界与字段语义

- 进程快照：Toolhelp32 枚举最多 2048 项；名称最多 512、路径最多 4096 Unicode 字符；达到上限或截断时返回结果级 `truncated=true`、DataIssue 和 UI 警告；亲和性检查中的显示字段截断会把 `inspectionState` 降为 Partial。
- 注册表分类字段：新增原始类型解析与 `RegistryValueState`（present/missing/typeMismatch/invalid/readError）。SystemComponent/NoRemove 仅接受精确 4 字节 REG_DWORD；ParentKeyName/ReleaseType 仅接受字符串；类型异常、无效值或读取失败不会被当作缺失，分类保持 Unknown（非零系统标志优先）。
- 启动文件夹：desktop.ini/Thumbs.db 等 Windows 元数据文件不再计为启动项，跳过数量以 `startupFolderMetadataSkipped` 报告。
- 设计契约：[ADR 0008](adr/0008-sensor-health-and-protection-state.md) 定义 Sensor Health 与保护状态派生；[ADR 0009](adr/0009-sqlite-audit-storage.md) 定义 SQLite 审计存储。两者均为设计接受、尚未实现；本阶段不引入 ETW 或 SQLite 写入。
- 测试与文档：新增 Phase 1.2 契约测试和前端 UI 测试（核心 Rust 70 项、前端 40 项通过）；README、能力边界、威胁模型和验证记录同步更新。

## 已完成：Phase 2.1–2.4 实时遥测基础

- **Phase 2.1**：独立管理员 `SentinelLiteSensor.exe`、Kernel-Process ETW start/stop、`(pid, create_time)` 配对、健康/丢失/残留报告、明确退出码。
- **Phase 2.2**：受限 Named Pipe 只读状态 IPC、同用户/DACL/MIC 校验、`get_sensor_status`、`--serve` 前台模式。
- **Phase 2.3**：首页真实传感器状态、独立健康、计数/uptime、4 秒轮询、失败清空旧健康/计数；整体保护状态仍为未知。
- **Phase 2.4**：标准化进程事件模型（事件/会话 ID、三类时间、身份确定性、字段完整性、配对状态）、重复 Start/Stop/乱序/孤儿/PID 复用处理、有界 4,096 队列（可配置）、非阻塞生产者、独立消费者、丢弃/积压/关闭排空统计、SessionEnd 未完成生命周期报告。
- 这些能力只建立可靠数据基础，不提供事件 UI、告警、检测规则或处置；事件仍不离开传感器进程。

## 已完成：Phase 2.5–2.8 进程遥测闭环

- **Phase 2.5**：共享 `sentinel-events`、独立只读事件 Named Pipe、游标化批次、`PairingRevision`/`SessionEnd` 交付、有界投递窗口、游标过期/超前语义、Tauri `get_sensor_events`。
- **Phase 2.6**：`sentinel-storage` SQLite schema v2、WAL、迁移、幂等事件写入、配对修正更新、会话/缺口/`SessionEnd`、有界查询、30 天/1,000,000 条/512 MiB 保留、当前用户本地应用数据路径、应用后台摄取。
- **Phase 2.7**：“进程事件”页：实时事件、生命周期详情、配对修正、历史筛选/分页、ETW/管道/投递/数据库异常、数据缺失语义、真实截图。
- **Phase 2.8**：`sentinel-detection` 的 4 条保守规则（启动/退出突发、配对覆盖不足、生命周期一致性）、输入质量门禁、证据事件 ID、规则版本/误报说明、UI 分析面板。
- 仍不提供告警队列、自动处置、文件/网络/DLL/服务监控或恶意样本检测率。

## 已完成：Phase 2.5–2.8 封存前可靠性验证

- `SessionEnd.final_stats`：最终 ETW 丢失/实时缓冲丢失和 pipeline 接收/消费/标准化/无效/队列丢弃/过滤/最终积压/峰值/关闭超时；未知保持 `null`，旧记录保持未知。
- 应用端 SQLite 持久化和检测输入使用最终统计；旧记录仍显示未知。
- SQLite 写入失败/只读文件/损坏文件/重开故障测试；写入失败不推进游标，恢复后幂等重放。
- 事件页会话结束后显示“会话已结束”，不再把最后链路状态显示为“已连接”；手动重新连接隔离旧/新会话。
- 30 分钟持续采集资源与数据库增长记录见 [VALIDATION](VALIDATION.md) §19；不代表长期稳定性或所有机器。

## Phase 2 最终状态

- 最终验收：**PASS WITH LIMITATIONS**（2026-11-01，封存结论见 [VALIDATION](VALIDATION.md) §19.8）。
- Phase 2.1–2.8 已完成；封存前 Reliability Verification 已完成。
- 30 分钟单机持续采集已执行，但不是长期稳定性证明；应用进程树内存仍未达到 <100 MB 目标。
- 已知限制、未验证项目和 P3 未隐藏；没有告警队列、自动处置或写操作。
- **Phase 3 已于 2026-11-01 启动**；当前已完成切片 1–6，未自动进入 Phase 4。

## Phase 3（进行中）

Phase 3 尚无独立的正式范围文档。本节按“下一阶段优先事项”逐项交付，不提前宣称整个 Phase 3 完成。

### 已完成：切片 1 — 原生防护状态只读读取（2026-11-01）

- 新增 `get_native_protection_status`：按需只读查询 `ROOT\Microsoft\Windows\Defender` 的 `MSFT_MpComputerStatus`，以及 `INetFwPolicy2` 的 Domain/Private/Public 防火墙开关。
- 首页“原生防护状态”卡片区分完整、部分、不可读、无权限和查询失败；字段缺失保持未知，不推断“已启用”，不改变整体保护状态。
- 只读、无子进程、无管理员要求、无后台轮询、无处置；真实非管理员探针和 release WebView2 验证通过。限制见 [ADR 0017](adr/0017-native-protection-status.md) 与 [KNOWN_LIMITATIONS](KNOWN_LIMITATIONS.md)。
- 未实现：第三方 AV 的 SecurityCenter2 读取、持续监控、Defender 策略/排除项/ASR/篡改保护、防火墙规则审计、实际拦截验证。

### 已完成：切片 2 — 服务与计划任务只读盘点（2026-11-01）

- 新增 `list_services`：SCM 枚举 Win32 服务与驱动，逐项只读 `QueryServiceConfig` 读取启动类型、账户和可执行文件路径（参数丢弃）。
- 新增 `list_scheduled_tasks`：Task Scheduler COM 递归只读任务与文件夹，读取状态、作者、运行身份、动作类型和可执行文件路径（不读 Arguments/WorkingDirectory/触发器）。
- “软件体检”新增“服务与计划任务”标签，两个来源独立加载/失败/分页；`accessDenied`/`partial`/`error` 与字段缺失保持未知，不判断恶意性。
- 数量、时间、字符串和响应均有上限；结果只在内存，不写入 SQLite；无任何启动/停止/运行/修改/删除操作。
- 真实非管理员验证：763 个服务（762 observed、1 accessDenied）、192 个计划任务（全部 observed），详见 [ADR 0018](adr/0018-service-task-inventory.md) 与 [VALIDATION](VALIDATION.md) §21。
- 未实现：AppX、服务触发器/计划/条件、任务 Arguments/WorkingDirectory、目标文件签名/存在性、持续监控。

### 已完成：切片 3 — 启动项只读盘点（2026-11-01）

- 读取 HKLM 32/64 与 HKCU 共享视图的 Run/RunOnce，以及用户/所有用户启动文件夹；保留注册表值类型、来源/条目观察状态和 Raw Value 可读性。
- `.lnk` 通过只读 ShellLink 读取目标，参数/工作目录只记录存在性布尔值；不读取文本、不调用 Resolve、不执行目标；`.url` 和歧义路径保持未知。
- 未实现：启动审批、签名/哈希、真实执行、Winlogon/IFEO/组策略/WMI 订阅等启动机制。

### 已完成：切片 4 — 软件与组件只读盘点（2026-11-01）

- HKLM 32/64 + HKCU Uninstall 来源级/条目级观察状态、字段状态、DisplayIcon 静态路径线索、WindowsInstaller；卸载入口仅存在性，不返回/执行完整命令。
- 安装路径脱敏；不同视图同名记录不合并；31 个无 DisplayName 注册项跳过并计数。
- 未实现：AppX/MSIX/Store/便携软件、签名/哈希、卸载残留。

### 已完成：切片 5 — 只读系统资产关联（2026-11-01）

- 新增独立 `association` 纯函数模块与 `scan_asset_associations` 命令；复用软件、进程、服务、任务、启动项现有快照，不新增底层扫描器。
- 规范化 Windows 路径，区分精确路径、安装目录内路径、同名文件弱线索；宽泛根不参与归属，多软件候选保留并标记歧义。
- 每个关系记录软件/目标 ID、关系类型、证据类型/值、匹配方法、确定性、观察状态和 issues；脱敏用户目录，不读取命令行/参数，不写 SQLite。
- UI 在软件详情中按需读取并展示来源完整性、关系证据与 issues；`noMatch`、`insufficientEvidence`、`sourceUnavailable` 分开，不提供任何处置按钮。
- 未实现：实时/跨重启关联、持久化关联历史、AppX/MSIX/Store/便携软件、签名/哈希/行为关联、服务依赖/任务参数。

### 已完成：切片 6 — 安全态势与 UI 整合（2026-11-01）

- “软件体检”顶部增加安全态势概览；复用 `get_native_protection_status` 与 `scan_asset_associations`，不新增扫描器、Tauri command 或 SQLite 持久化。
- 分开显示 `notRead`/`complete`/`partial`/`unavailable`、防护开关 enabled/disabled/unknown、资产来源状态、软件关联状态和证据分类；不生成 0–100 安全评分，不声称整体保护正常。
- 持久化入口（计划任务/启动项）、进程/服务关系、弱线索/多候选和软件关联状态均可追踪到证据、匹配方法、确定性、来源和观察状态。
- 应用壳层共享原生防护快照，软件体检页共享关联快照；刷新失败清空旧结果，不把旧快照显示为最新结果。
- 修复 Slice 1 P2：`amServiceEnabled=false` 现在与防病毒、实时保护和防火墙未启用项一样显示为未启用，不再显示“已读取到的开关均为启用状态”。
- 未实现：实时刷新、安全评分、告警、处置、AppX/MSIX/Store、持续监控和目标存在性/签名/哈希验证。

## 下一阶段优先事项

1. **验证/可观测性基础**
   - 已完成一次首页空闲 CPU/内存采样；仍需补充扫描期间、长时间和未来监控负载下的资源数据。
   - 调查 WebView2 进程树内存占用，决定是否能满足 <100 MB 目标，或需要调整技术/产品假设。
   - 输入上限、字符串截断、HKCU 去重、AccessDenied/Unicode 路径已有测试；仍需 fuzz/压力测试。
   - 依赖清单自动生成、npm audit、Dependabot 配置和 action SHA 固定已加入；CI 尚未在托管仓库实际运行。发布准备中已用 RustSec advisory-db 做手工交叉检查；CI 自动 advisory 检查仍待加入，`plist`/`time` 的修复需 Rust 1.88 + Tauri 回归。

2. **原生防护状态只读读取（切片 1 已完成）**
   - 已使用 WMI `MSFT_MpComputerStatus` 和 COM `INetFwPolicy2` 读取 Defender 与三个防火墙配置文件的启用位。
   - 任何读取失败或字段缺失都显示未知/部分，不能默认正常；不改变整体保护状态。
   - 不修改、不关闭原生防护。
   - 仍未实现：第三方 AV SecurityCenter2、持续监控、策略/规则/排除项/ASR/篡改保护、实际拦截验证。

3. **软件体检关联模型（静态路径关联已完成）**
   - AppX/Store 应用只读盘点；**尚未实现**。
   - 软件与组件只读盘点：**切片 4 已完成**（见上节）。
   - 服务与计划任务只读读取：**切片 2 已完成**（见上节）。
   - 启动项只读盘点：**切片 3 已完成**（见上节）。
   - 软件→进程/服务/任务/启动项静态路径关联：**切片 5 已完成**（见上节）；安全态势只读整合：**切片 6 已完成**（见上节）；实时/跨重启关联、持久化关联历史和行为关联未实现。
   - Slice 1 遗留 P2 已在切片 6 修复：`amServiceEnabled=false` 现在计入未启用开关，且不再显示“已读取到的开关均为启用状态”；已补充自动化测试。

4. **本地审计与事件存储（基础已完成）**
   - 已完成进程事件 SQLite schema v2、保留期限、迁移、访问路径和查询；仍缺加密、防篡改、导出/删除 UI 和扫描记录存储。
   - 首页可以显示真实最近扫描时间与问题，但仍不得在无实时监控时显示“保护正常”。

5. **实时监控（按传感器逐个上线）**
   - 进程 start/stop 已上线；下一步补充更可靠的父子关系、进程树和事件延迟统计。
   - 文件系统事件、网络连接、DLL 加载、持久化变化后续推进。
   - 每个传感器必须有启动/停止、健康检查、权限、覆盖范围、事件丢失和失败降级。
   - 在传感器没有实际通过测试前，首页不能显示“保护正常”。

6. **检测规则与误报治理（基础已完成）**
   - 已完成 4 条进程频率/配对/一致性规则和输入质量门禁；仍需正常/异常/误报/边界样本库和阈值校准。
   - 进程链、脚本、临时目录、启动项、异常签名等规则后续推进。
   - 评估 YARA-X/Sigma 的必要性、许可、性能、兼容性和维护成本；未通过评估不集成。

7. **深度防御模式**
   - 用户主动开启 15/30/60 分钟或持续。
   - 更细事件、目录监控、持久化检查、网络关联。
   - 明确额外资源/权限和结束后风险摘要。
   - 不承诺拦截所有攻击。

8. **受控处置与恢复**
   - 遵循 [清洗与恢复规范](CLEANING_POLICY.md)。
   - 官方卸载入口、可逆启动项禁用、快照、确认、验证与恢复。
   - 只有在此前审计、权限与测试基础完善后才考虑。
   - 默认仍不提供批量删除/无差别终止。

## 明确推迟

- 内核驱动和反 Rootkit。
- 云沙箱、上传样本、在线信誉服务。
- 自动上传文件或进程信息。
- 一键优化/清理营销功能。
- 自动升级机制（未完成签名与供应链设计前）。
- 对外声称可替代 Defender 或杀毒引擎。

## 每阶段发布门槛

- 已实现能力有可复现测试，且不依赖虚假数据。
- 未实现能力在 UI 和文档中明确标注。
- 资源、权限和盲区已记录。
- 没有新增未评估的高权限/写操作。
- CI、格式检查、lint、测试全部通过。
- README、能力边界、隐私与威胁模型随代码同步更新。
