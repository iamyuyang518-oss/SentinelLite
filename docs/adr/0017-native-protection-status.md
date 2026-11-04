# ADR 0017：原生防护状态只读读取

- 状态：接受，已实现（Phase 3 第一切片）
- 日期：2026-11-01
- 范围：Windows Defender 与 Windows 防火墙的按需只读状态观测
- 明确：不修改、不关闭原生防护；不验证实际拦截能力；不改变整体保护状态

## 1. 背景

Phase 2 已完成进程事件采集、交付、持久化、可视化与基础行为分析，但首页仍显示“整体保护状态未知”。路线图“下一阶段优先事项”第 2 项要求使用有文档支持的接口读取 Defender 与防火墙状态，并要求任何读取失败显示“未检查/保护降级”，不能默认正常。

该能力必须与“整体保护正常”严格分离：Defender 实时保护开关为开，不等于规则完整、策略未被绕过或机器未受攻击；防火墙配置为开，也不等于网络流量已被有效拦截。

## 2. 决策

### 2.1 数据源

- **Defender**：WMI 命名空间 `ROOT\Microsoft\Windows\Defender`，类 `MSFT_MpComputerStatus`，只读取以下字段：
  - `AntivirusEnabled`
  - `RealTimeProtectionEnabled`
  - `AMServiceEnabled`
  - `AntivirusSignatureVersion`
  - `AntivirusSignatureAge`
- **防火墙**：COM 接口 `INetFwPolicy2`，只调用 `get_FirewallEnabled` 读取 Domain、Private、Public 三个配置文件。
- 查询固定、无用户输入；不复用 PowerShell、`netsh` 或任何子进程。

### 2.2 执行模型

- 在 Tauri 的 `spawn_blocking` 工作线程中执行，不阻塞 WebView 主线程。
- 每个线程初始化 COM MTA；若线程已初始化于其他 apartment，复用现有初始化但不释放非本调用持有的引用。
- WMI 连接设置 `CoSetProxyBlanket`（WinNT 认证、Impersonate），避免普通用户下出现不必要的 `WBEM_E_ACCESS_DENIED`。
- WMI 枚举 `Next` 超时 5 秒；所有返回的 `VARIANT` 在读取后调用 `VariantClear`。
- 查询不需要管理员权限；不以提权方式重试。

### 2.3 失败与未知语义

每个提供程序返回 `observed` / `partial` / `unavailable` / `accessDenied` / `error`：

- 查询成功但缺少任一预期字段 → `partial`，缺失字段保持 `null`，并返回 `DEFENDER-FIELD-MISSING`。
- Defender WMI 返回零个实例 → `unavailable`（可能由第三方 AV 替代或已被策略禁用），返回 `DEFENDER-NOT-OBSERVED`；不推断为“已禁用”。
- 防火墙单个配置文件读取失败 → 该配置文件 `enabled = null`，整体 `partial`；其余配置文件照常显示。
- 全部配置文件失败且均为访问拒绝 → `accessDenied`；全部不可用 → `unavailable`；混合错误 → `error`。
- COM/WMI 初始化失败 → 两个提供程序均为 `error`，不返回任何“已启用”。
- 任何失败都附带稳定 issue code 和有界中文说明；不显示原始本地化错误文本。

整体 `observation` 仅表示数据完整性：

- 两个提供程序都 `observed` → `complete`；
- 至少一方部分/失败、但未全部不可读 → `partial`；
- 两个提供程序均不可读 → `unavailable`。

`complete` 不是“保护正常”的同义词。UI 必须继续显示整体保护状态未知。

### 2.4 数据契约

`NativeProtectionStatus`：

```text
checkedAt
observation: complete | partial | unavailable
defender: { state, antivirusEnabled, realTimeProtectionEnabled, amServiceEnabled,
            signatureVersion, signatureAgeDays, issues[] }
firewall: { state, profiles[{ profile, enabled }], issues[] }
limitations[]
```

所有布尔值使用 `boolean | null`；`null` 表示未知，不能渲染为 `false`。签名版本最多保留 128 个 Unicode 字符并去除控制字符。

### 2.5 UI 契约

- 首页“原生防护状态”卡片，用户主动点击“读取原生防护状态”后才查询；不后台轮询。
- 未查询、加载中、查询失败、部分数据分别显示不同文案；查询失败时清空旧结果。
- 显示“已读取全部字段”“部分字段未知”或“无法读取”，而不是绿色“安全”。
- 若观测到开关关闭，列出具体关闭项，措辞为“观测到未启用”，不做恶意判定。
- 即使所有开关均为启用，也显示“不能证明实际拦截能力、规则完整性或系统未受其他方式影响”。
- 不改变 `ProtectionOverview.overallState`、首页“整体保护”字段或传感器健康结论。

## 3. 非目标

- 不读取第三方 AV 的 SecurityCenter2 产品状态（本切片只覆盖 Defender）。
- 不读取或修改 Defender 策略、排除项、ASR 规则、篡改保护状态或防火墙规则。
- 不验证实际拦截、网络过滤、签名吊销或规则有效性。
- 不做持续监控、状态变化告警或历史记录。
- 不自动修复、不启用/禁用任何保护项。

## 4. 安全与隐私

- 只调用只读 getter / WMI SELECT；代码中不存在 `put_FirewallEnabled`、Defender 写入或策略修改路径。
- 固定本机命名空间和类，不接受 URI、服务器名、路径或命令。
- 不启动子进程，不联网，不上传状态。
- 查询结果只包含开关、签名版本/年龄和配置文件状态，不包含规则、排除项、用户文件或网络内容。
- 非管理员用户可查询；若系统策略拒绝，结果为 `accessDenied`，不会降级为“已启用”。

## 5. 后果与限制

- 这是某一时刻的快照，不是持续监控；两次查询之间状态可能变化。
- Defender WMI 提供程序可能因第三方 AV、篡改保护、企业策略或系统版本而不可用；此时保持未知。
- 防火墙 COM 接口可能被策略或权限拒绝；此时逐配置文件保持未知。
- 不验证 Defender 的云保护、行为监控、篡改保护、ASR、排除项或防火墙规则。
- WMI `ConnectServer`/`ExecQuery` 没有独立的线程级超时；只有枚举 `Next` 有 5 秒超时。极端情况下命令可能长时间占用一个 `spawn_blocking` 线程，UI 保持“正在读取”且不阻塞 WebView；当前未实现取消或后台看门狗。
- 同用户恶意进程理论上可伪造本机 WMI/COM 响应；这超出当前“已获得同用户控制权”的威胁模型，且本能力不产生处置。
- 不改变 Phase 2 的整体保护状态模型；`ProtectionState::Protected` 仍不存在。

## 6. 验证

- `sentinel-core` 单元测试覆盖：完整字段、字段缺失、访问拒绝、无实例、防火墙部分/全部失败、缺失配置文件、观测完整性、错误码映射、序列化 camelCase、无 `protected`/`overallState` 字段。
- Tauri 命令 ACL 测试覆盖新增的 `get_native_protection_status`，capability 仍只允许固定应用命令。
- 前端组件测试覆盖：未查询、完整、部分、关闭项、刷新失败清空旧结果、不显示“保护正常”。
- 真实 Windows 非管理员探针：Defender 三个布尔字段、签名版本/年龄和三个防火墙配置文件均读取成功，与 PowerShell 输出一致。
- release 应用通过 UI Automation 触发“读取原生防护状态”，真实 WebView2 页面显示 `已读取全部字段`、Defender `1.463.70.0`、签名年龄 0 天和三个防火墙配置文件已启用；截图见 `docs/screenshots/native-protection.png`。
