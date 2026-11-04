# ADR 0020：Windows 软件与组件只读盘点

- 状态：接受，已实现（Phase 3 第四切片）
- 日期：2026-11-01
- 范围：HKLM 32/64 位与 HKCU Uninstall 注册表视图的按需只读快照
- 明确：不卸载、不删除、不修改软件；不展示、不保存、不执行完整卸载命令

## 1. 背景

Phase 1 已有有界软件盘点：HKLM 32/64 位视图、HKCU 共享视图、Uninstall 字段分类、路径状态和风险解释。该实现仍存在两个与本切片目标不一致的问题：

1. 把完整 `UninstallString` / `QuietUninstallString` 放入响应并在详情中展示；
2. 没有来源级/条目级 `InventoryObservation`，读取失败与空来源的区分主要依赖 issue 文本。

本切片升级现有 `scan_installed_software` 和“软件盘点”标签，不新增重复命令或注册表数据源。

## 2. 决策

### 2.1 数据来源

- HKLM 64 位：`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
- HKLM 32 位：同一路径的 `KEY_WOW64_32KEY` 视图（对应 `WOW6432Node`）
- HKCU：共享用户视图，只读取一次，不使用 WOW64 双视图重复读取
- 每个来源记录 `SoftwareSourceStatus`：范围、注册表视图、位置、观察状态、`exists`、条目数。
- 来源不存在（`unavailable + exists=false`）与访问拒绝/错误（`exists=null`）分开。
- 单个子项或来源失败不影响其他来源。

### 2.2 字段

读取并保留：

- `DisplayName`
- `DisplayVersion` 及字段状态
- `Publisher` 及字段状态
- `InstallLocation` 及字段状态、文件系统路径状态
- `InstallDate` 规范化值及字段状态
- `DisplayIcon` 解析后的静态路径提示及字段状态
- `WindowsInstaller` 及字段状态
- `SystemComponent`、`NoRemove`、`ParentKeyName`、`ReleaseType` 及字段状态
- `UninstallString` / `QuietUninstallString` 的“存在非空字符串”布尔值

**不返回完整原始卸载命令**；命令文本只在本地解析存在性后丢弃。不执行卸载器、不调用 MSI 卸载接口、不启动安装程序。

### 2.3 字段与条目状态

- 字符串字段使用 `parse_string` 区分 present/missing/typeMismatch/invalid/readError；DWORD 使用精确 4 字节解析。
- 字段为空：值为 `null`，状态保持 present（“已读取（空值）”），不伪造默认值。
- InstallDate 只接受以四位数字开头、后续仅数字/`-`/`/` 的形状；明显非日期文本保留为 `invalid`，不猜测日期。
- DisplayIcon 只保留解析后的静态路径提示，删除常见 `,index` 后缀，不访问目标。
- `DisplayName` 缺失/空值：跳过该注册项并计数；`DisplayName` 类型异常/读取失败：同样跳过、来源标记 partial 并记录 issue，不把它当作“没有软件”。
- 条目 `observation`：默认 observed；任一字段类型异常、值无效、读取失败或截断 → partial。
- 结果 `observation`：所有可读来源 observed 且无部分条目/截断 → observed；部分失败 → partial；无可读来源时按 accessDenied/error/unavailable 聚合。
- 不合并不同注册表视图或不同键中的同名软件；每条保留原始来源证据。UI 使用“可显示注册项”而不是“唯一软件数”。

### 2.4 数量、时间与响应边界

- 每个注册表视图最多 512 项；全部来源最多 1,024 项；总时间 8 秒。
- 文本字段最多 2,048 字符；DisplayIcon 结果路径最多 1,024 字符。
- 超限/超时 → `truncated=true` + DataIssue。
- 命令在 `spawn_blocking` 执行，不阻塞 WebView；UI 分页 50 行。
- 结果只在内存，不新增 SQLite 表或写入。

### 2.5 隐私

- 不保存/返回完整卸载命令、原始 DisplayIcon 文本或多余参数。
- UI 将 `C:\Users\<name>\...` 和 `\\?\C:\Users\<name>\...` 脱敏为 `C:\Users\***\...`。
- 注册表键不包含用户名；安装路径和图标路径只在本次内存结果中。
- 软件名称、发布者和路径不作为恶意或安全结论；静态线索标签明确“非恶意判定”。

### 2.6 UI

- “软件盘点”标签升级为“软件与组件只读盘点”；用户点击“开始只读盘点”后才读取。
- 显示结果观察状态、三个来源卡、数量、读取时间、截断提示。
- 表格显示软件/版本/发布者、来源/范围、安装位置（脱敏）、观察状态和逐项 issue。
- 详情显示字段状态、DisplayIcon、WindowsInstaller、卸载入口存在性、条目状态和逐项 issue。
- 不显示完整卸载命令；不提供卸载、删除、清理或运行按钮。
- 刷新失败清空旧结果；一个来源失败不清空其他来源。

## 3. 非目标

- 不覆盖 Microsoft Store/AppX、MSIX、便携软件、服务、计划任务、启动项关联、签名、哈希或卸载残留。
- 不合并/关联软件与进程、服务、任务或启动项。
- 不判断恶意性，不执行任何处置。
- 不持久化盘点结果。

## 4. 后果与限制

- 31 个本机 Uninstall 项没有可用 DisplayName，本切片跳过并报告数量；这不等同于没有软件。
- 不同视图/用户范围的同名条目不会自动合并；可能显示多条登记记录。
- DisplayIcon 只是静态线索，可能过期或指向图标索引；不验证目标存在性。
- 注册表读取不可中断；极端卡死可能让后台任务超过 8 秒返回，但不阻塞 WebView。
- 未实机注入来源访问拒绝、超大列表、8 秒超时、字段类型异常或并发修改。

## 5. 验证

- 单元/集成测试覆盖：来源位置与 HKCU 去重、DisplayIcon 解析、空字段、InstallDate 无效输入、来源聚合、卸载入口未知不产生配置异常。
- 前端测试覆盖：未查询、成功字段/脱敏路径、来源部分/不存在、字段类型异常、截断、首次完全失败、刷新失败清空。
- 真实非管理员探针与 release WebView2 验证见 `docs/VALIDATION.md` §23。
