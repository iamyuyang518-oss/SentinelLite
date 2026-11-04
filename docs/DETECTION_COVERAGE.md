# 检测覆盖与盲区

本页是当前实现的诚实覆盖图。未列入“已实现”的能力不能在产品文案中暗示为已具备。

## 1. 总览

| 能力 | 数据来源 | 状态 | 说明 |
| --- | --- | --- | --- |
| 持续进程事件 | ETW | **部分实现（独立管理员传感器 + 本地历史）** | Phase 2.1 `SentinelLiteSensor.exe` 采集进程 start/stop；Phase 2.2 `--serve` 通过只读 Named Pipe 暴露状态；Phase 2.3 首页已接入 `get_sensor_status`；Phase 2.4–2.8 标准化事件、配对、有界投递、SQLite 历史、事件页和 4 条可解释规则。仍无告警队列/通知/自动处置；文件/网络/线程/镜像事件仍未实现 |
| 父子进程关系实时追踪 | ETW/Toolhelp 轮询 | 未实现 | 标准化 Start 事件保留父 PID，但未建立实时进程树；只读快照仍含父 PID 和当前父名称 |
| 可疑脚本/执行链 | ETW/命令行审计 | 未实现 | 不读取完整命令行 |
| DLL 加载 | ETW | 未实现 | 不扫描内存模块 |
| 文件变化（创建/修改/删除/批量重命名） | USN/ETW/文件系统监视 | 未实现 | 不后台监控文件 |
| 启动项读取 | 注册表 Run/RunOnce + 启动文件夹 | **部分实现（只读）** | HKCU 一次共享视图；含 `.lnk` 只读目标与参数/工作目录存在性；不含审批状态、`.url` 目标、计划任务、服务；文件夹元数据不计入 |
| 计划任务/服务持久化 | Task Scheduler/SCM | **部分实现（只读盘点）** | 软件体检页可按需枚举服务与计划任务，显示状态/启动类型/账户/动作可执行路径；已可通过静态路径与软件关联；无触发器/计划/条件、无持续监控、无处置 |
| 网络连接与 DNS 关联 | ETW/WFP/连接表 | 未实现 | 完全不发起或读取连接 |
| Defender/防火墙状态 | WMI `MSFT_MpComputerStatus` + `INetFwPolicy2` | **部分实现（只读、按需）** | 首页或软件体检安全态势中用户主动查询 Defender 开关/签名和三个防火墙配置文件；失败/字段缺失保持未知；不持续监控、不审计策略/规则、不验证实际拦截，不改变整体保护状态 |
| 软件盘点 | HKLM 32/64 + HKCU 共享视图 | **部分实现（只读，有界）** | 不含 Store/AppX、MSIX、便携软件；分类字段类型异常显式标记 |
| 软件关联组件 | 软件/进程/服务/任务/启动项只读快照 | **部分实现（静态路径关联）** | 用户主动触发软件→进程/服务/任务/启动项的精确路径、安装目录包含和同名文件弱线索；多候选保留、宽泛根排除、来源不完整显示证据不足；无实时/跨重启关联、持久化关联历史或行为关联 |
| 安全态势整合 | 原生防护 + 软件/进程/服务/任务/启动项 + 静态路径关联 | **部分实现（只读视图）** | 软件体检顶部按需读取一次，分开显示来源读取状态、证据分类、持久化入口和进程/服务线索；无整体安全评分、无实时更新、无处置；来源不完整/失败不会被显示为安全或“没有关联” |
| 卸载残留 | 注册表/文件系统 | 未实现 | 只列出部分路径异常 |
| 进程快照 | Toolhelp32 | **部分实现（只读，有界）** | 单次快照；最多 2048 项；名称/路径有长度上限；不持续监控 |
| CPU 亲和性 | GetProcessAffinityMask | **已实现（只读）** | 当前处理器组，不能追踪设置来源；结论有作用域 |
| 文件签名/磁盘证据 | WinVerifyTrust via verifysign + SHA-256 | **已实现（只读，按需）** | 仅磁盘文件；不联网、不查吊销、不验证内存；哈希有大小上限 |
| 短时 CPU 占用 | GetProcessTimes | **已实现（只读）** | 选中进程约 500 ms 样本，不是长期统计 |
| 进程事件历史 | SQLite | **部分实现** | 可查询/分析；不加密、无防篡改/导出/删除界面；默认保留 30 天/1,000,000 条/512 MiB |
| 行为规则 | 进程 start/stop | **部分实现（4 条）** | 频率、配对覆盖、生命周期一致性；无恶意样本误报/漏报率，不执行处置 |
| 告警队列 | 本地事件存储 | 未实现 | 有本地事件历史，但没有告警规则、通知或告警队列；首页仍显示“尚无告警” |
| 审计与恢复 | SQLite/快照 | 未实现 | 事件历史不是防篡改审计链，没有恢复快照 |
| YARA-X/Sigma | 第二/三阶段评估 | 未实现 | 未集成、未验证兼容性和许可 |

## 2. 软件盘点

**数据源：**

- `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`（32 位与 64 位视图）
- `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`（共享用户视图，仅读取一次）

**读取字段：** DisplayName、DisplayVersion、Publisher、InstallLocation、InstallDate、DisplayIcon（仅解析后的静态路径线索）、WindowsInstaller、UninstallString/QuietUninstallString（只判断是否存在非空值）、SystemComponent、NoRemove、ParentKeyName、ReleaseType。字符串/DWORD 字段保留 present/missing/typeMismatch/invalid/readError 状态。

**分类（注册表语义，不做品牌硬编码）：**

- SystemComponent 或 NoRemove 非零 → SystemComponent。
- ReleaseType 明确包含 Driver/Firmware/Hardware → DriverOrHardwareComponent。
- 仅有 ParentKeyName 或一般 ReleaseType → Unknown（不猜测 Runtime/Driver）。
- 无组件/更新标记 → UserSoftware（普通 Uninstall 条目）。
- SecuritySoftware / RuntimeOrSharedComponent 为模型保留类别；缺少可靠字段时不强行归入。
- SystemComponent/NoRemove 仅接受精确 4 字节 REG_DWORD；ParentKeyName/ReleaseType 仅接受 REG_SZ/REG_EXPAND_SZ。存在但类型不符、DWORD 长度错误、UTF-16 无效或读取失败时，分别记录 `registryValueTypeMismatch` / `registryValueInvalid` / `registryValueReadError`；若没有非零系统标志，分类保持 Unknown。非零 SystemComponent/NoRemove 仍优先判定为 SystemComponent。
- 字段缺失（Missing）本身不强制 Unknown；只有存在但不可读的值才会阻止回退为普通用户软件。

**覆盖边界：**

- 仅包含在 Uninstall 键中留下条目的传统软件；便携软件、服务、驱动、MSIX/AppX 不在此范围。
- HKCU 不使用 WOW64 双视图，避免重复；HKLM 两个视图仍区分。
- 每个视图最多 512 项、总计 1024 项；每个字符串最多 2048 字符；DisplayIcon 结果路径最多 1024 字符；总时间 8 秒；超过会 `truncated=true`。
- 安装路径区分 Present/Missing/AccessDenied/Unknown；AccessDenied 不再合并为“不存在”。
- 分类字段的 present/missing/typeMismatch/invalid/readError 状态通过 IPC 暴露给 UI；类型异常不会被静默当作缺失。
- 不返回完整卸载命令；只记录 UninstallString/QuietUninstallString 是否存在非空值，不执行卸载器或 MSI 卸载接口。
- DisplayIcon 只作为静态路径线索，去掉常见 `,index` 后缀，不访问目标。
- 每个来源和条目有独立观察状态；来源不存在（`exists=false`）与访问拒绝/错误（`exists=null`）分开。
- 不同视图/用户范围的同名登记记录不自动合并；每条保留原始注册表来源证据。无 DisplayName 的注册项跳过并计数。

**规则：**

- `INV-001`：普通用户软件缺卸载命令 → 配置异常。
- `INV-002`：普通用户软件安装路径明确缺失 → 配置异常。
- `INV-003`：普通用户软件安装路径 AccessDenied → 证据不足。
- `INV-004`：普通用户软件安装路径 Unknown → 证据不足。
- `INV-005`：卸载入口类型异常或读取失败 → 证据不足，不按缺失处理。
- `INV-COMPONENT-000`：系统/组件类条目 → 检查范围内未发现异常，不按普通卸载目标处理。
- `INV-COMPONENT-UNKNOWN`：组件/更新元数据不足 → InsufficientEvidence。
- `INV-000`：普通软件元数据范围内没有明确异常 → “检查范围内未发现异常”，置信度低；不代表安全。

## 3. 启动项

**数据源：**

- HKLM 的 Run、RunOnce，32/64 位视图；HKCU 的 Run、RunOnce 共享视图（一次）。
- `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`
- `%PROGRAMDATA%\Microsoft\Windows\Start Menu\Programs\Startup`

**覆盖边界：**

- 注册表值保留类型（REG_SZ/REG_EXPAND_SZ/REG_MULTI_SZ/REG_DWORD/REG_QWORD/REG_BINARY/REG_NONE/其他）和 Raw Value 可读性；只有 REG_SZ/REG_EXPAND_SZ 会被解析为命令，其他类型保留为部分未知，不猜测目标。
- 未读取 Windows StartupApproved 审批状态；条目存在不代表会执行，也不代表启用。
- `.lnk` 通过只读 ShellLink 读取目标；参数/工作目录只记录“是否存在”的布尔值，不读取或展示文本；不调用 `Resolve`，不执行快捷方式。`.url`、MSI 通告式快捷方式和无法解析的目标保留为部分未知。
- 未读取计划任务、服务、Winlogon、IFEO、组策略、WMI 永久订阅、AppInit_DLLs、浏览器启动机制等。
- 路径区分 Present/Missing/AccessDenied/Unknown；不访问网络/设备/相对路径。
- 未加引号且跨空格的路径被单独标记为 ambiguous；可解析出的候选路径仅作为“未探测”证据展示，不探测候选文件，也不判为缺失。
- 启动文件夹中的 desktop.ini、Thumbs.db 等 Windows 文件夹元数据文件不计为启动项；跳过数量通过 `startupFolderMetadataSkipped` 报告。
- 每个注册表视图/启动文件夹最多 256 项、总计 512 项、值名称 512、值文本 2048 字符、目标路径 1024 字符、8 秒；超过会标记不完整。
- 未检查签名、父进程或实际执行；不修改/禁用/删除/执行启动项。
- 来源不存在（`exists=false`）与读取失败（`exists=null`、accessDenied/error）分开；一个来源失败不影响其他来源。

**规则：**

- `STARTUP-001`：命令/快捷方式目标无法解析 → 证据不足。
- `STARTUP-002`：目标明确不存在 → 配置异常。
- `STARTUP-003`：目标存在且位于临时目录 → 潜在不受欢迎软件，低置信度，必须人工核实。
- `STARTUP-004`：目标状态未知 → 证据不足。
- `STARTUP-005`：未加引号的歧义路径 → 证据不足，记录 `unquotedPath`。
- `STARTUP-006`：目标路径 AccessDenied → 证据不足（不是文件缺失）。
- `STARTUP-000`：目标存在且未命中异常规则 → “检查范围内未发现异常”，不代表启动项安全或已启用。

## 4. 进程与亲和性

**数据源：** Toolhelp32 进程快照、OpenProcess、QueryFullProcessImageNameW、GetProcessAffinityMask、GetProcessTimes、WinVerifyTrust。

**覆盖边界：**

- 快照是一次性的；创建时间身份令牌用于绑定，但检查时仍可能退出/身份变化。
- 快照最多收集 2048 项；达到上限时 `truncated=true` 并记录 `processInventoryTruncated`，不能视为完整进程列表。
- 进程名称最多 512 Unicode 字符、可执行路径最多 4096 字符；超限会截断并记录 `processStringTruncated`。亲和性检查中的名称、路径和磁盘证据路径同样受限；发生显示字段截断时 `inspectionState` 降为 Partial。
- 身份时间/路径不可读时不发起亲和性检查；部分系统/受保护进程路径不可读，逐项显示。
- 亲和性仅覆盖当前处理器组；超过 64 个逻辑处理器或跨组细节不完整。
- Windows 公开接口不提供设置亲和性的调用者、时间或持久化来源，产品不会编造这些值。
- 文件签名/哈希只验证本地磁盘文件，不验证内存映像、加载模块或运行行为；UNC/设备/非绝对路径不会被探测。选中文件哈希最多读取 256 MiB。
- 磁盘文件可在启动后或检查后变化；哈希/签名不绑定内存映像。
- CPU 百分比是约 500 ms 的短时采样，不反映长期占用，也不用于单独判定异常。
- 不读取完整命令行、环境变量、网络连接或线程级信息。父进程信息只保留在列表快照中，不混入验证过的检查对象。

**规则：**

- `AFF-001`：进程掩码是系统掩码的真子集 → 亲和性配置异常，不代表进程整体有恶意。
- `AFF-000`：掩码一致 → “亲和性范围内未发现异常”，仍不能证明进程整体安全。
- `AFF-002`：无法读取 → 证据不足。
- `PROC-001`：没有独立行为证据 → 整体进程结论为证据不足；不能由 AFF-000 外推。
- 没有“高 CPU = 恶意”“亲和性限制 = 恶意”的规则。

## 5. 输入边界

- 软件：每个 Uninstall 视图 512 项、总计 1024 项、字符串 2048 Unicode 字符、DisplayIcon 结果路径 1024 字符、8 秒。
- 启动项：每个注册表视图/启动文件夹 256 项、总计 512 项、值名称 512、值文本 2048 Unicode 字符、目标路径 1024 字符、8 秒；快捷方式参数只做非空判断，文本不进入结果。
- 进程快照：最多 2048 项、名称 512、路径 4096 Unicode 字符；达到上限或发生截断时同时返回结果级 `truncated`、DataIssue 和 UI 警告。
- 注册表分类字段：DWORD 必须精确 4 字节；字符串必须为有效 UTF-16；类型异常、无效值和读取失败不按缺失处理。
- 选中进程磁盘哈希：最多 256 MiB；元数据/哈希失败各自标记。
- UI 列表每页 50 行；软件/启动项/进程均分页。
- 截断从不静默：结果有 `truncated=true`、DataIssue 和明确 UI 警告；页面不宣称完整。
- Phase 2.4 的 ETW 进程事件管道已按这些原则实现：队列默认 4,096（可配置 64–65,536）、满时丢弃并计数、ImageName 1,024 字符上限、关闭排空有超时；这提升数据完整性，不增加检测范围。

## 6. Phase 2.8 进程行为规则（版本 1）

| 规则 | 条件 | 所需数据 | 输出 | 误报/边界 |
| --- | --- | --- | --- | --- |
| `PROC-BURST-001` | 10 秒窗口内 ≥30 次进程启动 | 事件类型、可用时间、事件 ID | `suspicious`，严重程度中，置信度中 | 安装器、更新、构建/登录脚本、批量任务 |
| `PROC-EXIT-002` | 10 秒窗口内 ≥20 次进程退出 | 事件类型、可用时间 | `suspicious`，严重程度低，置信度中 | 应用关闭、注销、进程树清理 |
| `PROC-ORPHAN-003` | ≥20 条事件且无法配对/身份不确定 ≥30% | 配对状态、事件类型 | `anomaly`，严重程度低，置信度高；描述数据完整性 | 会话边界的退出事件、ETW 丢失、传感器重启 |
| `PROC-INTEGRITY-004` | 任意重复启动/重复退出/同 PID 身份不匹配 | 配对状态/原因、身份 | `anomaly`，严重程度中，置信度高 | PID 复用、ETW 重传、传感器重启 |

输入质量门禁：

- 少于 10 条事件 → `DATA-SMALL-001`（`data_insufficient`）。
- 会话未结束 → `DATA-SESSION-001`（`data_insufficient`）。
- 已知 ETW/实时缓冲丢失、管道丢弃、最终积压、关闭超时、无法解码、投递缺口或加载截断 → `DATA-LOSS-001`（`data_insufficient`）。
- ETW 丢失未知（没有可用的 `SessionEnd.final_stats`，也没有实时传感器统计） → `DATA-ETW-UNKNOWN-001`（`indeterminate`）。
- 实时传感器健康不可用不再单独导致 ETW 未知；新会话的最终统计可以独立证明为零。实时健康已知但未通过 → `DATA-HEALTH-001`（`indeterminate`）。
- 缺时间/缺映像 → `DATA-TIME-001`/`DATA-FIELDS-001`，阻止“正常”结论。

这些规则只使用 start/stop、时间、PID/父 PID、映像和配对状态；不读取命令行、文件、网络或内存。`suspicious`/`anomaly` 不是恶意判定，规则不会执行任何处置或自动响应。真实验证用 40 个正常短生命周期 `cmd.exe` 触发频率规则，不能代表恶意样本检测能力。

## 7. 已知未监控状态

当前首页的**整体保护状态**仍永远是状态未知；这不是 UI 保守，而是：

- 首页已接入独立 `SentinelLiteSensor` 的只读状态、健康、计数与运行时间；进程事件页已接入事件投递、SQLite 历史和基础规则，但传感器只覆盖进程 start/stop，没有告警、通知、自动处置或文件/网络/DLL/服务监控；
- 传感器只在管理员显式运行时存在；主应用不自动启动或提权；
- 只有用户主动触发的 Defender/防火墙只读快照；没有持续监控、策略/规则审计或实际拦截验证；
- 只有用户主动触发的服务/计划任务只读盘点与静态路径关联；没有触发器/计划/条件、目标文件存在性/签名、持续监控或处置；
- 安全态势概览只是用户主动触发的快照组合；没有自动刷新、实时关联、整体安全评分或“保护正常”状态；
- 没有后台告警队列；没有恶意样本误报/漏报率或检测覆盖率结论。

首页会把传感器状态与整体保护状态分开显示：Active/Healthy 不会被推导为“整体保护正常”；传感器未运行时显示静态“当前版本暂不支持在应用内启用实时保护”状态；查询失败时显示断开/未知，并清空旧计数。原生防护卡片也只显示用户主动读取的观测值；即使 Defender/防火墙字段全部为启用，整体保护状态仍为未知。任何改变首页整体保护状态的能力，必须同时实现健康状态、权限状态、覆盖范围和丢失事件统计，并经 [ADR 0008](adr/0008-sensor-health-and-protection-state.md) 引入。独立传感器的健康/丢失/停止语义见 [ADR 0010](adr/0010-sensor-architecture.md)；本地历史存储见 [ADR 0014](adr/0014-event-persistence.md)；事件投递/可视化/分析见 [ADR 0013](adr/0013-process-event-delivery.md)、[ADR 0015](adr/0015-event-visualization.md)、[ADR 0016](adr/0016-behavior-analysis.md)。

### 7.1 Phase 3 原生防护只读状态

**数据源：** WMI `ROOT\Microsoft\Windows\Defender` / `MSFT_MpComputerStatus`；COM `INetFwPolicy2`。

**读取字段：** Defender `AntivirusEnabled`、`RealTimeProtectionEnabled`、`AMServiceEnabled`、`AntivirusSignatureVersion`、`AntivirusSignatureAge`；防火墙 Domain/Private/Public 的 `FirewallEnabled`。

**语义：**

- 提供程序状态为 `observed` / `partial` / `unavailable` / `accessDenied` / `error`；整体数据状态为 `complete` / `partial` / `unavailable`。
- 字段缺失、无实例、无权限、WMI/COM 错误都不会回退为 `false` 或 `true`；布尔值保持 `null` 并显示 issue。
- “已读取全部字段”只表示读取完整，不表示保护有效；整体保护状态仍为未知。
- 用户主动触发，不后台轮询；查询失败清空旧结果。
- 不修改 Defender/防火墙，不读取策略、排除项、ASR 或防火墙规则，不验证实际拦截。

**未覆盖：** 第三方 AV 的 SecurityCenter2 产品、篡改保护状态、企业策略、持续变化、规则有效性和网络流量。

### 7.2 Phase 3 软件资产关联

**数据源：** 现有软件、进程、服务、计划任务、启动项只读快照；不新增底层扫描器。

**规则：**

- **Exact**：规范化后 DisplayIcon 或安装位置文件路径与目标可执行路径完全一致。
- **Strong**：目标位于非宽泛软件安装目录内，且路径边界正确。
- **Weak**：普通用户软件与运行进程文件名相同但目录不同；不用于服务/任务/启动项，明确不能证明归属。
- 宽泛根（磁盘根、Program Files/Common Files/WindowsApps/ProgramData/用户配置根及已知厂商根）不作为归属证据。
- 多软件候选全部保留，标记 `ambiguous=true`；不能仅凭名称或品牌选择一个。
- 路径规范化考虑大小写、斜杠、`.`/`..`、本地设备前缀；拒绝 UNC、设备、相对、环境变量、通配符和疑似参数文本。
- 不使用 8.3 短名、junction、符号链接或文件系统解析；这些路径记录为不可比较。

**语义：** 每个关系记录软件/目标 ID、关系类型、证据类型/值、匹配方法、确定性、观察状态、候选数和 issues。软件摘要区分 `linked`、`weakOnly`、`noMatch`、`insufficientEvidence`、`sourceUnavailable`；没有关联不等于异常。

**边界：** 只读、无执行、无完整命令行/参数、无 SQLite；最多 2,048 条关系、256 条弱线索、256 条/软件、32 个候选/目标；来源截断或不完整会标记结果不完整。

### 7.3 Phase 3 服务与计划任务只读盘点

**数据源：** SCM `EnumServicesStatusExW` + `QueryServiceConfigW`；Task Scheduler COM `ITaskService`。

**读取字段：**

- 服务：名称、显示名、状态、启动类型、服务类型位、账户、可执行文件路径（参数丢弃）。
- 计划任务：名称、完整路径、状态、启用位、作者、运行身份、动作类型、ExecAction 可执行文件路径（不读 Arguments/WorkingDirectory/触发器/XML）。

**语义：**

- 结果级和条目级观察状态为 `observed` / `partial` / `unavailable` / `accessDenied` / `error`。
- 单个服务/任务/文件夹失败不导致整个列表不可用；已读取字段保留，失败字段保持 `null` 并附 issue。
- 服务配置成功但账户/路径为空显示“未提供”，不当作读取失败；无法解析的路径标记 partial。
- 服务最多 1,024、计划任务最多 1,024/256 文件夹/16 动作；服务 8 秒、任务 10 秒上限；超限明确标记 `truncated`。
- 不启动/停止/创建/删除/修改服务；不运行/创建/修改/删除/启用/禁用任务；不执行任何程序。
- 不判断服务/任务是否恶意；名称、作者、路径或账户不能单独构成恶意结论。
- 结果只在内存，不写入 SQLite；不做持续监控。

**未覆盖：** 实时/跨重启资产关联、持久化关联历史、服务依赖、触发器/计划/条件、上次运行结果、目标文件存在性/签名/哈希、AppX/Store、浏览器扩展、持续变化。

### 7.4 Phase 3 安全态势只读整合

**数据源：** 用户主动触发的 `get_native_protection_status` 和 `scan_asset_associations`；后者复用软件、进程、服务、计划任务和启动项快照。没有新增扫描器或 Tauri 命令。

**读取状态：** 概览分别展示 `notRead`、`complete`（来源已读且未截断）、`partial` 和 `unavailable`；这些状态只描述数据可读性，不是“安全/危险”。读取失败清空旧结果，不自动刷新，快照可能过期。

**证据分类：**

- **已观察事实**：精确路径、安装目录内路径、已启用防护开关、已读取来源。
- **值得核实**：同名文件弱线索、多候选、未启用防护开关。
- **证据不足**：证据不足的软件、未知防护字段、部分读取来源和观察不完整的关系。
- **来源不可用**：不可用的软件/资产来源和 Defender/防火墙提供程序。

**关系视图：**

- 持久化入口：计划任务 + 启动项关系。
- 进程与服务：运行进程 + Windows 服务关系；服务不在持久化区块重复。
- 弱线索与多候选：交叉筛选，可能与上方列表重复。
- 软件关联状态：保留 `linked`、`weakOnly`、`noMatch`、`insufficientEvidence`、`sourceUnavailable` 的区分。
- 每个关系显示软件、目标、匹配方法、确定性、观察状态、来源、证据路径和软件路径线索；不显示完整命令行或参数。

**边界：** 没有 0–100 安全评分、整体“保护正常”或恶意判定；没有处置按钮；不写入 SQLite；不新增提权、扫描或网络能力。真实机本次 68 条关系中 49 条进程、11 条服务、8 条任务/启动项；13 个软件有线索、171 个证据不足、0 个未匹配。

**未覆盖：** 持续监控、实时/跨重启关联、关联历史、文件存在性/签名/哈希、AppX/Store、第三方 AV、企业策略、实际拦截能力。

## 8. WPS/亲和性专项测试计划

WPS 只作为未来人工测试样本，不写入黑名单或硬编码逻辑。

计划步骤：

1. 启动 WPS 相关进程，记录 PID、磁盘路径、签名状态与当前掩码。
2. 不预设其存在“恶意绑定”；把限制当作待核实的配置事实。
3. 记录系统可用范围与进程允许范围，保留测试时间和环境。
4. 如用户或软件自身改变亲和性，尝试通过系统行为观察差异；不得声称能追踪调用者，除非有可验证的审计来源。
5. 形成报告时区分“观测到限制”和“原因/意图未知”。

当前没有执行针对 WPS 的专项实测，因此不应在产品或 README 中声称已验证 WPS 行为。
