# ADR 0018：Windows 服务与计划任务只读盘点

- 状态：接受，已实现（Phase 3 第二切片）
- 日期：2026-11-01
- 范围：Windows SCM 服务与 Task Scheduler 计划任务的按需只读快照
- 明确：不启动/停止服务，不创建/删除/修改服务，不运行/创建/修改/删除/启用/禁用任务

## 1. 背景

路线图“下一阶段优先事项”第 3 项“软件体检关联模型”首先需要服务与计划任务的只读盘点数据。Phase 2 只有进程事件和 SQLite 历史，没有服务/任务清单；软件盘点也不包含这两类持久化来源。

本切片只建立盘点基础，不建立软件→服务/任务的关联图，不判断恶意性，不提供处置。

## 2. 决策

### 2.1 服务数据源

- 使用 SCM（Service Control Manager）：
  - `OpenSCManagerW(SC_MANAGER_ENUMERATE_SERVICE)`
  - `EnumServicesStatusExW(SC_ENUM_PROCESS_INFO, SERVICE_WIN32 | SERVICE_DRIVER, SERVICE_STATE_ALL)`
  - 对每个服务调用 `OpenServiceW(SERVICE_QUERY_CONFIG)` 与 `QueryServiceConfigW`
- 读取字段：服务名、显示名、当前状态、启动类型、服务类型、账户、可执行文件路径。
- `lpBinaryPathName` 只解析可执行文件路径，参数被丢弃。无法安全解析的未加引号路径保持未知并标记 `partial`；空路径视为“未提供”，不当作读取失败。
- 服务类型保留为位标志（内核/文件系统/适配器/识别器驱动、独立/共享/交互式进程、其他），不丢失未知位。

**实测注意：** 在测试的 Windows 11 26200 上，`QueryServiceConfigW` 传入 64 KiB 缓冲区会返回 `RPC_X_BAD_STUB_DATA (1783)`；8 KiB 和 32 KiB 正常。因此缓冲区从 8 KiB 开始，按 `pcbBytesNeeded` 增长但不超过 32 KiB；超过上限的条目保持 `partial`。这是对系统 API 行为的防御性处理，不改变只读语义。

### 2.2 计划任务数据源

- COM `ITaskService`，CLSID 使用 Task Scheduler 2.0 的 `TaskScheduler`（`0f87369f-a4e5-4cfc-bd3e-73e6154572dd`）。
- `Connect` 使用四个空 `VARIANT`（本机、当前用户上下文），不指定远程服务器。
- 从根文件夹 `\` 开始 BFS：
  - `GetFolders(0)` / `GetTasks(TASK_ENUM_HIDDEN)`；
  - Task Scheduler 集合索引为 1-based；
  - 对每个任务读取 `Name`、`Path`、`State`、`Enabled`、`Definition` 的 `RegistrationInfo.Author`、`Principal.UserId/GroupId`、`Actions`；
  - 仅对 `TASK_ACTION_EXEC` 读取 `IExecAction.Path`，不读取 `Arguments`、`WorkingDirectory`；不读取触发器或 XML。
- 动作最多保留 16 个/任务；超过标记 `actionsTruncated`，任务保持 `partial`。

### 2.3 失败与未知语义

- 结果级观察状态：`observed` / `partial` / `unavailable` / `accessDenied` / `error`。
- 服务：
  - SCM 打开/枚举失败 → 对应提供程序失败，空列表不表示没有服务；
  - 单个服务打开或配置读取失败 → 该条目 `accessDenied`/`partial`/`error`，其余条目照常返回；
  - 字段缺失/不可解析 → `null` + issue；
  - 空账户/空路径来自成功的配置查询时是“未提供”，不回退为“已读取”或“正常”。
- 任务：
  - Task Scheduler 服务不可用/Connect/根文件夹失败 → 提供程序失败；
  - 单个任务/文件夹读取失败 → 该条目或结果标记 partial/accessDenied，其余条目保留；
  - 任务定义、作者、主体、动作读取失败 → 已读取字段保留，失败字段 `null` + issue；
  - 动作截断 → `actionsTruncated=true` + issue。
- 所有 issue 使用稳定 code，消息有界；不把读取失败填成正常值。

### 2.4 数量、时间与响应边界

- 服务：最多 1,024 条；枚举缓冲最多 4 MiB，最多 64 页；总时间 8 秒；配置缓冲 32 KiB；名称/显示名 512、账户 512、路径 1,024 字符。
- 计划任务：最多 1,024 个任务、256 个文件夹、16 个动作/任务；总时间 10 秒；名称 512、路径 1,024、主体 512、动作路径 1,024 字符。
- 达到任一上限 → `truncated=true` + issue；不静默截断。
- 命令在 `spawn_blocking` 工作线程执行，不阻塞 WebView；UI 分页 50 行。
- 单次 COM/SCM 调用本身没有可中断的取消机制；时间上限只在枚举循环之间检查，极端卡死仍可能让该后台任务长时间不返回，但不会阻塞 UI。

### 2.5 隐私与持久化

- 结果只存在于本次 Tauri 命令响应和 React 内存中，不写入 SQLite。
- 不保存完整服务命令行、任务 Arguments、WorkingDirectory、触发器、XML 或文件内容。
- 只保留服务/任务的可执行文件路径；路径有长度上限，UI 在表格中显示。
- 不读取文件内容、签名、哈希、网络连接或注册表持久化位置。
- 不执行任何子进程、脚本或任务。

### 2.6 UI 契约

- “软件体检”页新增“服务与计划任务”标签；用户点击“开始只读盘点”后才调用两个命令。
- 服务与计划任务分开展示，各自有独立加载、错误、观察状态、计数、截断提示、筛选和分页。
- 一个来源失败不清空另一个来源；刷新失败清空该来源旧结果。
- 条目级 `accessDenied`/`partial`/`error` 显示对应标签和 issue code；`null` 字段显示“未知”或“未提供”（仅在成功观察时）。
- 不显示“恶意”“安全”或“保护正常”结论；只显示盘点事实。
- 不提供启动/停止/删除/运行/启用/禁用等按钮。

## 3. 非目标

- 不建立软件→服务/计划任务/启动项关联图。
- 不读取服务依赖图、失败操作、触发器、任务条件、计划或上次运行结果。
- 不验证服务/任务目标文件是否存在、签名、哈希或恶意性。
- 不修改、不控制、不运行任何服务或任务。
- 不持久化盘点结果，不做持续监控。

## 4. 后果与限制

- 服务枚举包含 Win32 服务和驱动；普通用户可能无法读取个别服务配置（本次实测 `McmSvc` 返回访问拒绝），因此结果可以是 `partial`。
- 计划任务集合可能包含隐藏任务；本切片使用 `TASK_ENUM_HIDDEN`，但受权限影响的任务/文件夹会显示未知。
- Task Scheduler COM 和 SCM 都是本机只读 API；同用户恶意进程可伪造本机响应，超出当前威胁模型。
- 空列表、部分列表和读取失败在 UI 中有明确不同文案，不能被解释为“没有服务/任务”或“配置正常”。
- 没有真实故障注入（SCM/Task Scheduler 服务停止、权限拒绝、超大列表、COM 卡死）；时间/数量上限只由代码路径和真实正常路径验证。

## 5. 验证

- `sentinel-core` 单元测试覆盖：服务状态/启动类型/类型位映射、服务路径解析（引号、参数、扩展名、设备路径、歧义）、错误码映射、配置失败保持未知、空路径/账户语义、任务状态/动作类型映射、任务错误映射、字符串/路径清洗。
- Tauri ACL 覆盖 `list_services` 与 `list_scheduled_tasks`。
- 前端组件测试覆盖：未查询、双来源加载、单来源失败、accessDenied 未知、截断、刷新失败清空旧结果、无处置按钮。
- 真实 Windows 非管理员探针：763 个服务（762 observed、1 accessDenied）、192 个计划任务（全部 observed）、无截断；服务 108 ms、任务 294 ms（本机单次观察）。
- release 应用通过 UI Automation 打开标签并触发盘点，真实 WebView2 表格显示服务/任务、来源状态和截断/权限语义；截图见 `docs/screenshots/services-tasks.png` 与 `docs/screenshots/scheduled-tasks.png`。
