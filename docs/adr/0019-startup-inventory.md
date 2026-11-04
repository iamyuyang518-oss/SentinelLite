# ADR 0019：Windows 启动项只读盘点

- 状态：接受，已实现（Phase 3 第三切片）
- 日期：2026-11-01
- 范围：Run/RunOnce 注册表入口、用户/所有用户启动文件夹、`.lnk` 只读目标解析
- 明确：不创建、修改、删除、禁用、启用或执行任何启动项；不修改注册表；不写 SQLite

## 1. 背景

Phase 1 已有有限的启动项检查：HKLM Run/RunOnce 32/64 位视图、HKCU Run/RunOnce 共享视图和两个启动文件夹；它已经区分路径 Present/Missing/AccessDenied/Unknown、未加引号歧义路径和文件夹元数据文件。路线图第 3 项仍要求“解析启动文件夹快捷方式目标；显示未解析原因”，而本切片还要求可关联的稳定数据契约、来源级观察状态、注册表值类型和快捷方式参数/工作目录“是否存在”的布尔证据。

本切片选择**升级现有 `scan_startup_items` 命令和现有“启动项”标签**，而不是新增重复的数据源、命令或标签。这样避免同一注册表位置被两个功能读取两次，也保持现有 15 个 Tauri 命令 ACL 不变。

## 2. 决策

### 2.1 注册表来源

- HKLM：
  - `Software\Microsoft\Windows\CurrentVersion\Run`
  - `Software\Microsoft\Windows\CurrentVersion\RunOnce`
  - 分别通过 `KEY_WOW64_64KEY` 和 `KEY_WOW64_32KEY` 读取；32 位视图对应 `WOW6432Node`。
- HKCU：
  - 同名 Run/RunOnce，只读共享用户视图一次，不使用 WOW64 双视图重复读取。
- 每个来源记录 `StartupSourceStatus`：来源类型、用户范围、注册表视图、位置、观察状态、`exists`、条目数。
- 注册表键不存在：`observation=unavailable`、`exists=false`；不能和读取失败混为一谈。
- 注册表键访问拒绝/错误：`accessDenied`/`error`、`exists=null`；该来源失败不影响其他来源。
- 每个值保留：
  - 值名称；
  - 值类型（REG_SZ、REG_EXPAND_SZ、REG_MULTI_SZ、REG_DWORD、REG_QWORD、REG_BINARY、REG_NONE、其他）；
  - Raw Value 是否成功解码为文本；
  - 解析后的目标路径/候选路径；
  - 目标状态、路径解析方式、观察状态、逐项 issue。
- 只有 REG_SZ/REG_EXPAND_SZ 会被解析为启动命令。其他类型保留条目并标记 `partial`，不猜测可执行文件；空值、无效 UTF-16、超长文本同样保留并标记。
- **不返回完整原始命令/命令行文本**。解析后只保留可执行文件路径；参数被丢弃。无法安全解析时保留名称、类型和 issue，不把截断文本当完整命令。

### 2.2 启动文件夹

- `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`
- `%PROGRAMDATA%\Microsoft\Windows\Start Menu\Programs\Startup`
- 展开后的文件夹必须是本地绝对路径；网络、设备、相对路径不访问。
- 文件夹不存在：`unavailable + exists=false`；访问拒绝：`accessDenied + exists=null`；其他读错误：`error`。这三种状态在结果和 UI 中不同。
- 文件夹中的普通文件保留为 `startupFolderFile`，目标路径就是该文件；不读取文件内容。
- `.lnk` 使用只读 Shell COM：
  - `CoCreateInstance(ShellLink)` + `IPersistFile::Load(STGM_READ)`；
  - `GetPath` 读取目标；
  - `GetArguments` / `GetWorkingDirectory` 只用于判断非空布尔值，**不返回、不记录、不显示文本**；
  - **不调用 `IShellLink::Resolve`**，不执行快捷方式或目标；
  - 目标路径交给现有的本地路径探测；网络/设备/相对路径保持 Unknown。
- `.url` 不解析目标，保留条目并标记 `partial`；MSI 通告式快捷方式或没有可读目标时同样保留为 `partial`/`unresolved`。
- 快捷方式目标读取失败仍保留条目；`observation` 为 `error`/`accessDenied`（权限）或 `partial`（目标/参数字段不完整），绝不静默丢弃。

### 2.3 观察状态与聚合

- 逐项：`observed` / `partial` / `unavailable` / `accessDenied` / `error`。
- 来源级：同一枚举；`exists` 区分“确认不存在”和“无法确认存在性”。
- 结果级：
  - 所有已覆盖来源都可读、没有部分条目、没有截断 → `observed`；
  - 至少一个来源/条目部分失败、权限拒绝、错误或截断 → `partial`；
  - 没有任何可读来源且存在权限拒绝 → `accessDenied`；
  - 没有任何可读来源且存在错误 → `error`；
  - 没有任何可读来源（全部不存在/不可用）→ `unavailable`。
- 缺失的可选 RunOnce 键或启动文件夹不会把整体结果降级为 `partial`，但来源卡显示“不存在”，不能被解释为读取成功。
- 截断、部分条目、无权限条目、空结果都有稳定 issue code；空列表不会显示为“没有启动项”或“正常”。

### 2.4 数量、时间和响应边界

- 每个注册表视图和每个启动文件夹最多检查 256 项；全部来源合计最多 512 项。
- 值名称最多 512 字符；注册表值文本最多 2,048 字符；目标路径最多 1,024 字符。
- 扫描总时间 8 秒；只在枚举循环之间检查，单次注册表/COM 调用不可中断。
- 快捷方式目标路径缓冲 1,024 字符；参数/工作目录只读缓冲有界，文本不进入结果。
- 超限或超时 → `truncated=true` + `DataIssue`，不静默截断。
- 命令继续在 `spawn_blocking` 执行，不阻塞 WebView；UI 分页 50 行。

### 2.5 隐私与持久化

- 结果只在本次 Tauri 命令响应和 React 内存中；不新增 SQLite 表或写入。
- 不保存完整命令行、快捷方式参数文本、工作目录文本、文件内容、签名、哈希或网络目标。
- 注册表来源位置不含用户名；启动文件夹来源以 `%APPDATA%`/`%PROGRAMDATA%` 形式展示。
- 目标路径（包括可能位于用户目录的路径）只用于本次内存展示；UI 将 `C:\Users\<name>\` 脱敏为 `C:\Users\***\`。
- 不执行任何目标程序、脚本、Shell 命令或快捷方式；`.lnk` 读取不使用 `Resolve`。
- 新增的 Windows 依赖 feature 仅用于 `IShellLinkW`/`IPersistFile` 只读加载。

### 2.6 UI 契约

- “软件体检”的现有“启动项”标签升级为“启动项只读盘点”；用户点击“开始启动项检查”后才读取，不自动轮询。
- 显示结果级观察状态、来源卡（来源/范围/视图/状态/条目数）、数量、读取时间、截断提示。
- 列表显示名称、类型/注册表值类型、来源/范围、目标（脱敏）、目标状态、条目观察状态和 issue code。
- 详情显示值名称/类型、Raw Value 可读性、目标类型/路径/状态/解析方式、快捷方式参数/工作目录是否存在、启动审批未核验、注册表位置和逐项 issue。
- 原有静态线索保留在“静态线索（非恶意判定）”区域，并去掉证据中的完整路径；它不是恶意或安全结论。
- 刷新失败清空旧结果；一个来源失败不清空其他来源；不提供启用、禁用、删除、运行或清理按钮。

## 3. 非目标

- 不读取 WMI 永久事件订阅、IFEO、Winlogon、AppInit_DLLs、浏览器扩展、计划任务、服务或组策略启动项。
- 不解析 `.url`、MSI 通告式快捷方式或快捷方式参数/工作目录文本。
- 不建立软件→启动项/服务/任务关联图，不判断恶意性，不做自动处置。
- 不验证启动审批、签名、哈希、目标存在性之外的安全属性或实际执行。
- 不持久化盘点结果，不做变化监控。

## 4. 后果与限制

- 普通用户可能无法读取个别注册表键、启动文件夹或快捷方式；这些状态显式保留。
- 测试机启动文件夹只有 `desktop.ini`，没有真实 `.lnk`。`.lnk` 成功解析由自动化测试在临时目录创建并删除一个测试快捷方式验证；应用运行路径从未创建、修改或执行启动项。真实启动文件夹没有 `.lnk`，所以“真实启动文件夹中的快捷方式成功解析”标记为未实测。
- ShellLink COM 与注册表调用不可中断；极端卡死仍可能让后台命令超过 8 秒返回，但不会阻塞 WebView。
- 未实测故障注入：注册表/文件夹访问拒绝、COM 初始化失败、超大列表、同时写入/删除启动项。
- 本切片没有修改 Slice 1 的 `NativeProtectionCard` 维护待办；见路线图维护待办。

## 5. 验证

- `sentinel-core` 单元测试覆盖：注册表视图/去重、值类型映射、字符串/空值/不支持类型解码、缺失/拒绝/错误来源映射、来源状态聚合、快捷方式成功/失败解析。
- Tauri ACL 继续覆盖 `scan_startup_items`；未新增命令，命令总数仍为 15。
- 前端测试覆盖：未查询、注册表值类型/来源/观察状态、快捷方式布尔证据且不渲染参数、逐项无权限/来源不存在、截断提示、刷新失败清空旧结果。
- 真实 Windows 非管理员探针读取真实 Run/RunOnce 与两个启动文件夹；release WebView2 点击验证和截图见 `docs/VALIDATION.md` §22。
