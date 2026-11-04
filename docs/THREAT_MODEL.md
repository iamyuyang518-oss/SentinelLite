# 威胁模型

版本：v0.1.0 Phase 1.2 只读预览 + Phase 2.1–2.8 进程遥测闭环与封存前可靠性验证 + Phase 3 切片 1 原生防护只读状态 + 切片 2 服务/计划任务只读盘点 + 切片 3 启动项只读盘点 + 切片 4 软件/组件只读盘点 + 切片 5 只读资产关联 + 切片 6 安全态势只读整合（2026-11-01）。本文件描述设计假设与当前残余风险，不代表已经完成安全审计。

## 1. 系统范围

```text
WebView2 (React UI, untrusted display surface)
        │  Tauri IPC + application ACL: 16 fixed read-only/analysis commands;
        │  process inspection requires pid + backend-verified creation-time identity
        ▼
Tauri command layer (same normal-user process)
        │
        ├─ sentinel-ipc client ── Named Pipe ── SentinelLiteSensor.exe --serve (admin)
        │                                      read-only status only
        ▼
sentinel-core (Rust; Windows read-only APIs)
        │
        ├─ HKLM 32/64 + HKCU shared Uninstall/Run/RunOnce views
        ├─ Startup folders + read-only ShellLink (.lnk) metadata
        ├─ Toolhelp32 snapshot + creation-time identity tokens
        ├─ OpenProcess / GetProcessAffinityMask / GetProcessTimes
        ├─ QueryFullProcessImageNameW
        ├─ Bounded file metadata + SHA-256 (local selected file)
        └─ WinVerifyTrust via verifysign (network disabled)
```

主应用没有内核驱动、管理员服务、自动启动的后台监控或远程后端；Phase 2.6 起有一个仅位于当前用户本地应用数据目录的 SQLite 事件历史。Phase 2.1 增加管理员前台 ETW 传感器 `SentinelLiteSensor.exe`；Phase 2.2 增加 `--serve` 前台模式，通过受 DACL 限制的本地 Named Pipe 暴露只读状态。Phase 2.3 首页通过 `get_sensor_status` 每 4 秒查询该只读状态；主应用不自动启动传感器，不通过 IPC 下发停止/配置/任意命令，也不传输原始事件。Phase 2.4–2.8 在传感器进程内把 start/stop 转换为标准化事件、有界排队并通过独立只读事件管道交付；主应用写入 SQLite 并在事件页/检测中使用。`SessionEnd` 现在携带最终 ETW/pipeline 统计，未知值保持未知。Phase 3 切片 1 增加用户主动触发的原生防护只读查询：普通用户通过 WMI `MSFT_MpComputerStatus` 和 COM `INetFwPolicy2` 读取 Defender 开关/签名与三个防火墙配置文件；失败、字段缺失或无权限时显式未知，不修改设置，不改变整体保护状态。切片 2 增加服务与计划任务只读盘点：SCM 只读枚举服务配置，Task Scheduler COM 只读枚举任务/动作可执行路径；不启动/停止/运行/修改任何条目，结果仅在内存，不写 SQLite。切片 3 升级启动项盘点：Run/RunOnce 与启动文件夹保留来源/条目观察状态和注册表值类型；`.lnk` 通过 `IShellLinkW` + `IPersistFile::Load(STGM_READ)` 读取目标，参数/工作目录只记录存在性布尔值，不读取文本、不调用 `Resolve`、不执行目标；仍不写 SQLite。切片 4 升级软件盘点：HKLM 32/64 + HKCU Uninstall 来源/条目观察状态、字段状态、DisplayIcon 静态路径线索和 WindowsInstaller；卸载入口只保留存在性布尔值，不返回完整命令，不执行卸载器。切片 5 增加软件与进程/服务/任务/启动项的静态路径关联：复用现有快照、词法路径规范化、宽泛根排除、弱线索限运行进程、多候选保留歧义；不读取命令行/参数，不执行目标，不写 SQLite。切片 6 在前端把原生防护与资产关联整合成安全态势概览，应用壳层/软件体检页各自共享一个 `ScanController`，不新增扫描器、Tauri command、权限、SQLite 或网络能力；只显示读取状态、证据分类和关系，不生成安全评分或恶意判定。

## 2. 资产

- 用户对本机的控制权与对软件的决定权。
- 本机软件、启动项、进程路径等本地敏感元数据。
- 用户对“是否受保护”的理解与信任。
- 系统稳定性与已有安全产品（Defender、防火墙）的完整性。
- 构建与分发链路（依赖、锁文件、未来安装包）。

## 3. 信任边界与假设

- WebView2 渲染进程与 Rust 后端通过 Tauri IPC 隔离。
- 前端内容全部随应用打包，没有远程网页或用户内容注入点。
- 后端不信任前端输入：固定命令、强类型 PID/身份参数、应用级 Tauri ACL；不接受字符串路径、命令或脚本。
- Windows API 返回值、注册表值、文件路径、签名证书主体均视为可能畸形或具有欺骗性的**数据**，不视为安全声明。
- 主应用以普通用户运行；无法读取受保护进程属于预期状态，不应通过提权绕过。0.1.0 不提供应用内 UAC 启动传感器能力，也没有 `start_sensor` 命令；独立 `SentinelLiteSensor.exe` 必须由管理员在应用外手动启动，只创建 realtime ETW session 读取进程 start/stop，不自行提权、不提供写操作，普通用户运行 fail closed。
- 主机上可能存在比本应用权限更高的恶意代码；本产品不承诺抵御已获得管理员/内核权限的攻击者。

## 4. 威胁与缓解

| # | 威胁 | 影响 | 当前缓解 | 状态/残余风险 |
| --- | --- | --- | --- | --- |
| T1 | UI 被注入并调用后端 | 读取本机信息或触发意外行为 | 无远程内容；严格 CSP；Tauri AppManifest 为十六个命令生成 ACL，capability 只含十六个 allow 权限且仅限 main；参数为数字 PID/游标/有界过滤条件；无 core 默认权限、Shell/FS/HTTP 插件；`get_sensor_status`/`get_sensor_events` 只读固定管道；`get_native_protection_status`、`scan_installed_software`、`scan_startup_items`、`scan_asset_associations`、`list_services`、`list_scheduled_tasks` 无参数、固定本机只读查询 | 部分缓解；未做 WebView 模糊测试 |
| T2 | 后端接受任意路径/命令 | 任意文件读取、命令执行、提权媒介 | 不接受用户提供的路径/命令参数；卸载入口只保留存在性，不返回命令文本；主应用不执行任何子进程 | 已从 API 面消除 |
| T3 | 前端把局部正常/未知显示为整体安全 | 用户错误判断防护状态 | 首页默认未知；删除 `Protected` 状态；风险带 scope；NoClearIssue 中性且标为“检查范围内未发现异常”；亲和性正常时整体进程仍为 InsufficientEvidence；安全态势只显示读取状态/证据分类，不生成 0–100 分数或“保护正常”，`complete` 仅表示来源可读；测试防止绿色 clean 结论 | 部分缓解；没有完整状态机形式化验证 |
| T4 | PID 复用或进程退出导致检查错位 | 展示错误进程的证据 | 进程列表返回高精度创建时间身份令牌；检查前后核验令牌；名称/路径来自已验证句柄；父进程信息不再混入检查对象；不一致返回 processIdentityChanged | 磁盘文件仍按路径读取，文件可在启动/检查后变化；身份只绑定进程对象，不绑定内存内容 |
| T5 | 恶意注册表值/超长命令/超大进程表导致解析或内存问题 | UI/后端资源消耗、错误展示 | 每注册表视图/总条目上限；进程快照最多 2048 项；名称/路径与字符串按 Unicode 字符上限截断；UI 分页 50 行；结果级 truncated + DataIssue；解析器无求值/无 shell | 未做系统化模糊测试；DataIssue 数量本身未独立分页；这是有界但仍可产生较多行的输入 |
| T6 | 注册表重定向导致盘点重复/不完整 | 漏报或重复软件/启动项 | HKLM 读取 32/64；HKCU 只读共享视图一次；稳定哈希 ID + 去重集合；来源在 UI 显示 | 仍有不可访问、无权限和未覆盖位置；不声称完整 |
| T7 | 启动文件夹快捷方式指向未知目标 | 误判或漏判持久化 | `.lnk` 使用只读 ShellLink 加载目标，不调用 Resolve、不执行；参数/工作目录只记录存在性布尔值；`.url` 和无法解析目标标为证据不足并列出盲区 | 快捷方式目标可能过期/被重定向；未验证签名/哈希/实际执行 |
| T8 | 处理器亲和性被误判为恶意 | 恐吓、误处置 | 亲和性风险 scope 明确；正常时文案“亲和性范围内未发现异常”；受限为配置异常；整体进程始终另报证据不足；明确无法追踪设置来源 | 单条规则置信度低；需用户核实 |
| T9 | CPU 高占用被误判为亲和性篡改 | 错误的因果结论 | CPU 采样与亲和性分开展示；说明采样窗口和限制 | 没有基于 CPU 的定罪规则 |
| T10 | 数字签名/哈希被误当作安全结论 | 放过恶意签名软件或误伤未签名软件 | 显示签名状态、磁盘路径/大小/时间/有限 SHA-256；明确 memoryVerified=false、不检查吊销；哈希/签名只对磁盘文件；进程身份单独核验 | 不检查运行中内存；文件可能在签名/哈希之间或之后变化；哈希有大小上限 |
| T11 | 依赖供应链被投毒/漏洞 | 代码执行/数据泄露 | 精确版本、锁文件、最小直接依赖、CI 只读权限、actions 固定 SHA、npm audit、Dependabot 配置；发布准备中执行 RustSec advisory-db 交叉检查 | 发现 `quick-xml 0.38.4` 两项 DoS 和 `time 0.3.45` 一项 DoS（均为 Tauri/plist/cookie 传递依赖，未确认用户输入可达路径）；`cargo-audit` CLI 未构建；修复需 `plist 1.10.3`/`time 0.3.55` + Rust 1.88 + Tauri 回归；Dependabot 未在托管仓库运行；非完整供应链审计 |
| T12 | 应用升级/安装包被替换 | 代码执行 | 当前无自动更新、无发布安装包 | 发布前必须补签名与发布完整性设计 |
| T13 | 数据外传 / 意外远程访问 | 隐私泄露、触发外部连接 | 无遥测、无 HTTP 插件、签名验证禁止联网、CSP 禁止远程连接；启动文件夹/安装路径/签名探测跳过 UNC、设备命名空间与相对路径；结果仅在内存 | 操作系统/WebView2 与本地重解析点的行为不在本应用完全控制内 |
| T14 | 提权滥用 | 系统被修改 | 主应用不请求管理员权限；0.1.0 不注册 `start_sensor`，没有 UAC/`runas` 启动路径；独立传感器需管理员在应用外启动，只读 ETW 进程 start/stop，不提供写操作/处置 | 当前无应用内提权入口；未来若加入提权代理或 UI 启动能力需单独威胁模型 |
| T15 | 观测失败/截断被解释成“扫描干净” | 漏报 | 权限、部分失败、AccessDenied、Unknown 和 truncated 都显式展示；UI 禁止完整声明；没有条目不代表没有软件；资产关联在来源部分/不可用/截断时显示证据不足；新 `SessionEnd.final_stats` 持久化最终 ETW/pipeline 统计 | 旧历史记录没有 final_stats，仍未知；字符串/问题列表仍有展示上限缺口 |
| T16 | 恶意文件名/路径欺骗 UI 或日志 | 欺骗用户、日志注入 | React 转义文本；不渲染 HTML；本地展示；外部字符串有界 | 未做 Unicode 同形字专项防护 |
| T17 | 系统组件/驱动条目被当作普通软件误报 | 诱导卸载组件或制造噪声 | 读取 SystemComponent/NoRemove/ParentKeyName/ReleaseType 并保留原始类型状态；类型不符/无效值/读取失败不会当作缺失，分类保持 Unknown；系统/组件语义不应用 INV-001；不做品牌硬编码 | 分类仍受 Uninstall 字段完整性限制；Driver/Runtime/Security 分类可能不可判定 |
| T18 | 未加引号启动路径被误判为缺失文件 | 错误告警/误导处置 | 引号与未加引号路径分别解析；检测到歧义时使用 InsufficientEvidence，记录 unquotedPath 与未探测候选路径 | 启发式解析不能覆盖所有 Windows CreateProcess 歧义；未解析快捷方式仍未知 |
| T19 | 磁盘哈希/签名证据过期或被替换 | 用户误认为运行进程可信 | 记录 checked path、有限 metadata、SHA-256 和时间；明确 memoryVerified=false；进程创建时间身份核验 | 不检查内存映像；文件内容可在启动后变化；hash/signature 读取非原子 |
| T20 | 本地恶意进程抢占/冒充传感器状态管道 | 主程序读取假状态、拒绝服务或泄露状态 | 固定端点 + 当前用户/SYSTEM/BA DACL + Medium mandatory label；`FILE_FLAG_FIRST_PIPE_INSTANCE`；服务端校验客户端用户 SID；客户端校验服务端镜像名与 elevation；仅 status 请求；所有失败 fail closed | 同用户进程可查询非敏感状态；同用户可抢占管道造成传感器 IPC 不可用；管理员级伪造超出本切片威胁模型；本次封存验证的最终二进制完成 Medium 客户端状态/事件读取；跨用户 DACL 拒绝仍未实机验证 |
| T21 | 进程风暴/慢消费者导致传感器内存或 ETW 采集被拖垮 | 内存耗尽、事件丢失或采集线程阻塞 | ETW 回调只做有界解码和非阻塞 `try_send`；队列默认 4,096、可配置 64–65,536；满时丢弃最新事件并计数；消费者独立线程，sink 错误不停止排空；关闭有 5 秒超时并报告积压；投递窗口独立有界 4,096（可配置），无人读取时淘汰最旧并让客户端看到游标过期 | 队列/投递满时确实丢失事件（显式报告）；真实注入未在实机执行；事件端点运行时线程故障由客户端查询失败观察 |
| T22 | 事件/历史数据泄露或被同机低权限进程读取 | 进程路径/父子关系/历史暴露 | 事件管道与状态管道相同的当前用户/SYSTEM/BA DACL、Medium label、SID/镜像/elevation 校验；数据库位于当前用户本地应用数据目录并继承用户 ACL；事件/查询不包含命令行或文件内容；无上传 | 同用户进程可读取；管理员/本地 SYSTEM 可访问；数据库不加密、无防篡改 |
| T23 | 事件/历史被伪造、重放或错误配对 | 误导用户、错误检测 | 事件 ID/会话 ID/游标；`(pid, create_time)` 身份；配对修正幂等更新；数据库按事件 ID 幂等；UI 显示来源/修正/不确定；检测带数据质量门禁和证据 ID | 同用户可写数据库；没有签名链或审计防篡改；检测不是恶意判定 |
| T24 | 原生防护状态被伪造、过期或误读 | 用户误以为系统已受保护或已失去保护 | 固定本机 WMI `MSFT_MpComputerStatus` 与 COM `INetFwPolicy2` 只读 getter；无用户输入、无子进程、无写入；失败/字段缺失/无实例/无权限分别返回 `unavailable`/`partial`/`accessDenied`，字段保持 `null`；UI 只显示观测值，不推导整体保护；安全态势概览将 enabled/disabled/unknown 分开并明确快照可能过期；用户主动触发，不后台轮询；共享快照不自动刷新 | 同用户恶意进程理论上可伪造本机 WMI/COM 响应；不验证实际拦截、规则、策略或篡改保护；快照可能过期；不产生处置 |
| T25 | 服务/计划任务清单被伪造、过期或误读 | 用户误判持久化项正常或恶意，或误以为扫描完整 | SCM 只读枚举 + `SERVICE_QUERY_CONFIG`；Task Scheduler COM 只读枚举；固定本机、无用户输入、无子进程；单条/单文件夹失败保持 partial/accessDenied；字段缺失为 `null`；数量/时间/字符串上限；结果仅在内存，不写 SQLite；无启动/停止/运行/修改/删除 | 同用户可伪造本机 SCM/COM 响应；不读取触发器/计划/条件/Arguments；不验证目标文件签名/存在性；快照可能过期；不产生处置 |
| T26 | 启动项清单被伪造、过期或误读；快捷方式读取被用作执行入口 | 用户误判启动项已启用/正常/恶意，或读取动作执行目标/访问远程位置 | Run/RunOnce 只读打开；启动文件夹只读枚举；`.lnk` 仅 `IPersistFile::Load(STGM_READ)` + getters，不调用 `Resolve`、不执行目标；参数/工作目录只保留存在性布尔值，不读取文本；网络/设备/相对目标不探测；来源不存在/无权限/错误分开；数量/时间/字符串上限；UI 不提供启用/禁用/删除/运行；结果不持久化 | 同用户可伪造注册表/ShellLink 响应；真实启动文件夹无 `.lnk` 成功样本；不验证启动审批、签名、哈希或实际执行；不读取 `.url`；快照可能过期 |
| T27 | 软件清单被伪造、过期或误读；卸载入口文本被泄露或误执行 | 用户误判软件身份/安全，或读取动作泄露/执行卸载命令 | 只读打开 HKLM 32/64 + HKCU Uninstall；字段解析保留类型/值/读取状态；UninstallString/QuietUninstallString 只保留非空存在性，不返回/保存/执行完整文本；DisplayIcon 只解析静态路径；无用户输入、无子进程、无写入；来源/条目观察状态、数量/时间/字符串上限；UI 不提供卸载/删除/清理/运行 | 同用户可伪造注册表响应；不验证软件身份、签名、哈希、安装来源或行为；不同来源同名登记不自动合并；无 DisplayName 条目被跳过；快照可能过期 |
| T28 | 资产关联被伪造、过期或误读；弱线索/多候选被包装成确定归属 | 用户误判软件与对象的关系，或泄露用户路径 | 只在内存比较现有快照；词法规范化大小写/斜杠/点段/设备前缀，拒绝 UNC/设备/相对/环境变量/参数文本；宽泛根排除；弱线索仅限进程且明确标注；多候选保留并标记歧义；来源不完整/截断显示证据不足；证据路径前后端脱敏；不读取命令行/参数、不执行目标、不写 SQLite；关系/候选/字符串有上限；安全态势只显示 group/evidence，不生成安全评分或恶意结论 | 同用户可伪造底层快照；路径可能过期/共享/重定向；不验证存在性/签名/哈希/行为；无实时/跨重启/持久化关联；真实机未注入故障路径 |

## 5. 明确不在防御范围

- 已获得管理员或内核权限的攻击者。
- Rootkit、Bootkit、固件级恶意代码。
- 内核驱动漏洞利用与内存注入检测。
- 实时阻断、勒索软件回滚、EDR 行为阻断。
- 网络拦截、DNS 过滤、漏洞利用防护。
- 浏览器扩展内容审计与网页安全。
- 软件漏洞本身（版本过旧不等于恶意）。

## 6. 数据流与失败语义

- 注册表无法打开/枚举：记录 `DataIssue`，继续读取其他位置。
- 单视图超过上限：停止继续枚举，设置 `truncated=true`，记录 `inventoryTruncated` / `startupTruncated`，UI 明确显示不完整。
- 字符串超限：按 Unicode 字符截断并添加 `…`，记录 `registryStringTruncated`，`truncated=true`。
- 进程快照超过 2048 项：停止枚举，`truncated=true`，记录 `processInventoryTruncated`，UI 显示“快照不完整”。
- 进程名称/路径超限：截断并记录 `processStringTruncated`；亲和性检查的显示字段截断时 `inspectionState` 为 Partial。
- 注册表分类字段类型异常/无效/读取失败：保留 `RegistryValueState` 并记录对应 DataIssue，不按 Missing 处理，分类保持 Unknown。
- HKCU：只读取共享视图一次；重复 ID 被抑制并记录。
- 启动文件夹元数据文件：desktop.ini/Thumbs.db 被跳过，不计为启动项，并以 `startupFolderMetadataSkipped` 报告数量。
- 启动目标无法解析或为 AccessDenied：保持未知/证据不足，不自动变成“缺失”。
- 未加引号且可能跨空格的路径：记录 `unquotedPath`，不探测候选，不判定缺失。
- 进程路径无权限/身份时间不可读：列表显示不可读，按钮禁用或命令返回 processUnavailable。
- 进程身份变化：返回 processIdentityChanged，不返回混合数据。
- 亲和性读取失败：保留检查结果，亲和性未知，亲和性风险为证据不足；整体进程风险仍为证据不足。
- 签名验证失败或访问出错：签名状态为未知/无效/不受信任，不据此自动定罪。
- IPC 失败：首页传感器区域显示不可用/健康未知，清空旧计数，不保留上一次的健康状态；整体保护状态仍为未知。
- 进程事件队列满：丢弃最新事件、增加 `dropped_queue_full` 与健康 `events_dropped`，ETW 采集线程不阻塞；ETW `events_lost` 与管道丢弃分开统计，任一项非零都使传感器健康失败关闭。
- 事件配对异常：重复 Start/Stop、乱序、PID 复用、身份不确定都保留事件和原因；无法确定时不猜测，配对摘要与健康会显式降级。
- 消费者退出或关闭超时：生产者记录 `dropped_consumer_gone`；关闭记录 `shutdown_timed_out` 与超时积压，不宣称已排空。
- 传感器硬杀：内存中的队列和未完成生命周期丢失，且可能留下 ETW session；没有持久化恢复承诺。
- 正常停止：`SessionEnd.final_stats` 保存最终 ETW 丢失/实时缓冲丢失和 pipeline 计数；停止失败、残余 session 或外部停止时 ETW 值保持未知，不填 0。旧记录没有该块。
- 事件投递游标过期/超前：返回 `cursor_expired`/`missed_records` 或 `cursor_ahead`，UI 明确显示缺口/重置，不静默返回空。
- 数据库不可用/损坏：应用继续运行，状态和查询返回 `storageUnavailable`/`storageError`；不自动删除或重建；实时事件不受影响。
- 数据库 schema 高于应用支持：拒绝打开并报告版本，不降级猜测。
- 检测输入存在 ETW/管道/投递/解码/截断缺口：返回 `data_insufficient`；ETW 丢失未知返回 `indeterminate`；不返回正常。
- 分析规则命中：只产生带证据/版本/误报说明的结果，不执行任何系统动作，不判定恶意。
- 原生防护查询字段缺失/无权限/提供程序不可用：返回 `partial`/`accessDenied`/`unavailable` 与 issue；布尔值保持 `null`，不填 `false`/`true`，不改变整体保护状态。
- 安全态势组合读取：原生防护或资产关联任一失败时分别清空对应旧快照并显示来源不可用/部分读取；一个来源失败不把另一个来源的结果清空。`notRead`、`complete`、`partial`、`unavailable` 只描述数据可读性，不生成“保护正常/安全”结论；来源不完整或截断不能变成 `noMatch`。
- 服务/计划任务单条读取失败：该条目保留已读取字段，失败字段为 `null` 并附 issue；其余条目继续；整个来源失败时返回 `unavailable`/`accessDenied`/`error`，空列表不表示没有服务/任务。
- 服务/计划任务达到数量/时间上限：设置 `truncated` 和 issue；不静默截断，不把不完整结果宣称完整。

## 7. 安全需求验证

已由自动化测试覆盖：

- 未知保护状态不会被显示为正常；`Protected` 状态从类型层删除。
- 没有实际传感器运行时，后端总状态始终为未知。
- 首页传感器查询失败或传感器停止时显示不可用/健康未知，不保留旧健康或计数；Active 与 Healthy 独立显示。
- 亲和性正常不会渲染为整体进程 Clean/Safe；整体为证据不足。
- 系统组件/NoRemove/ParentKeyName/ReleaseType 分类与风险语义有测试，无 NV 品牌硬编码。
- 注册表原始 DWORD/字符串解析、类型不符、无效 UTF-16、截断与 Unknown 分类有测试。
- HKCU 只扫描共享视图，ID/去重有回归测试。
- 字符串截断、每视图/总条目上限、进程快照 2048 项上限与名称/路径截断有测试；UI 分页与 truncated 警告有测试。
- 启动文件夹元数据文件排除有单元测试；真实主机冒烟可观察跳过计数。
- AccessDenied 不降级为 Missing；未加引号歧义路径与配置异常区分有测试。
- 进程创建时间身份令牌与 mismatch 错误有测试。
- 签名错误码不会把“无效”和“未签名”混为一类；远程路径不探测磁盘哈希。
- Tauri capability 只允许十六个应用命令，不含 core:default。
- 传感器状态 IPC 已测试：缺失管道、匿名客户端拒绝、非传感器服务端拒绝、重复管道实例拒绝、非法/超长请求、客户端超时、空闲客户端超时后恢复。
- 进程事件管道已测试：正常配对、孤儿 Stop、重复 Start/Stop、乱序回补与 revision、PID 复用、身份不确定、有界队列满载丢弃、消费者退出、正常关闭排空、关闭超时、sink 错误、20,000 事件持续负载、会话重启隔离、健康计数对账；事件协议/游标/过期/超前/截断/顺序客户端/SessionEnd 确认。
- 存储已测试：schema v2 重开、幂等批次、修正更新/先到、有界查询、保留、加载截断、会话关闭/未观察、WAL 大小；写入失败不推进游标、只读文件 fail closed、非数据库文件不替换、重开后 final_stats/游标保留、摄取恢复重放。
- 检测已测试：正常样本、频率 burst、孤儿比例、一致性异常、样本不足、未结束、已知/未知丢失、缺时间、证据有界、规则版本/误报说明；最终统计已知零/已知丢失/未知不回落实时值/pipeline 缺口/final invalid。
- 长跑已测试（30 分钟，见 VALIDATION §19）：sensor/app CPU、内存、SQLite 增长、队列峰值、事件处理、摄取延迟与缺口。
- 原生防护只读查询已测试：Defender 完整字段、字段缺失、访问拒绝、无实例、防火墙单个/全部配置文件失败、缺失配置文件、观测完整性、错误码映射、camelCase 序列化且无 `protected`/`overallState` 字段；`amServiceEnabled=false` 被计为未启用且不再显示“全部启用”脚注；release 应用通过 UI Automation 真实触发并显示观测值，整体保护状态仍为未知。
- 安全态势概览已测试：未读取不自动采集、五类来源状态、原生防护 enabled/disabled/unknown/unavailable、noMatch/insufficient/sourceUnavailable 分离、弱线索/多候选、组合刷新失败清空、跨页共享快照、无安全评分/处置按钮、无原始用户路径。
- 服务/计划任务盘点已测试：服务状态/启动类型/类型位映射、路径解析与参数丢弃、配置失败保持未知、空账户/路径语义、任务状态/动作类型/错误映射、字符串清洗；真实非管理员读取 763 个服务/192 个任务，release WebView2 标签和两个表格可显示；前端覆盖双来源独立失败、accessDenied、截断和刷新清空旧结果。
- 前端不存在卸载、删除、结束进程等处置按钮；事件/检测页面区分未知、缺失、断开和正常。

尚未覆盖：

- 系统性模糊测试和超大/畸形注册表压力测试（已有边界/截断测试，非 fuzz）。
- 真实恶意样本、提权、持久化攻击链。
- WebView2 沙箱逃逸或 Tauri IPC 漏洞。
- 小时级长期稳定性/性能基准；本轮只有 30 分钟封存前持续采集（VALIDATION §19）。
- Cargo advisory 扫描：发布准备中已做一次 RustSec advisory-db 交叉检查；CI 自动 advisory 检查仍待加入。
- 传感器 IPC 的跨用户 DACL 拒绝和长时间并发压力；管理员真实 ETW 端到端已覆盖状态/事件正常路径。
- 进程事件队列/投递的真实溢出、慢/多客户端竞争、硬杀后的恢复、小时级稳定性；当前队列满载、消费者退出和关闭超时仅由自动化测试覆盖，30 分钟正常路径见 VALIDATION §19。
- 物理磁盘满/文件系统配额/真实 ACL 拒绝/并发多实例/运行中损坏/保留清理的真实注入；已有 SQLite query_only 写入失败、只读文件、非数据库文件和重开测试，不是完整故障注入。
- 检测规则的真实恶意样本、误报/漏报率、阈值校准和长期基线；当前只验证正常管理活动触发频率规则。
- 原生防护的第三方 AV、SecurityCenter2、篡改保护、企业策略、规则/排除项/ASR、防火墙规则和实际拦截能力；当前只验证本机 Defender/防火墙开关快照。
- 服务/计划任务的触发器、计划/条件、上次运行结果、服务依赖、目标文件存在性/签名/哈希、实时/跨重启资产关联、持续变化；未注入 SCM/Task Scheduler 停止、权限拒绝、超大列表或 COM 卡死。
- 资产关联的来源访问拒绝、路径边界、同名多候选、无效路径、超大关系集、超时或并发修改未实机注入；相关路径由单元/前端测试覆盖。
- 安全态势概览未真实注入来源不可用、刷新失败、超大关系页面或并发修改；release UI 只验证正常组合读取、部分读取来源和证据不足显示，失败清空由自动化测试覆盖。
- 安装包签名与升级安全。

## 8. 未来动作功能的加分条件

任何写操作功能在合入前必须新增：

- 独立威胁建模与操作白名单；
- 执行前快照、回滚/恢复验证；
- 用户确认与影响预览；
- 审计记录访问控制；
- 失败、部分成功和重启后恢复测试；
- 针对误报的回归案例。

在此之前，本产品保持只读。
