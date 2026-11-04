# 验证记录

记录时间：2026-10-29 起；Phase 2.1–2.4 补充于 2026-10-31，Phase 2.5–2.8 与封存前可靠性验证补充于 2026-11-01，Phase 3 切片 1–6 补充于 2026-11-01（本机时间，UTC+8）。

验证对象：SentinelLite Phase 1.1/1.2 只读安全修正、Phase 2.1–2.8 独立传感器、事件交付、SQLite 历史、事件页和基础行为分析，以及 Phase 3 切片 1 原生防护只读状态、切片 2 服务/计划任务只读盘点、切片 3 启动项、切片 4 软件/组件、切片 5 只读资产关联、切片 6 安全态势只读整合，工作目录 `<repo root>`；Phase 1.2 补充验证见第 9 节，Phase 2.1 见第 10–11 节，Phase 2.2 见第 12 节，Phase 2.3 见第 13 节，Phase 2.4 见第 14 节，Phase 2.5 见第 15 节，Phase 2.6 见第 16 节，Phase 2.7 见第 17 节，Phase 2.8 见第 18 节，封存前可靠性验证见第 19 节，Phase 3 切片 1 见第 20 节，切片 2 见第 21 节，切片 3 见第 22 节，切片 4 见第 23 节，切片 5 见第 24 节，切片 6 见第 25 节。
> 注：文中以 `target/` 开头的路径是本机验证产物，不随仓库发布；公开仓库只保留脱敏后的验证记录。

> **Source-only release scope (2026-11-04):** 本次公开树只发布源码，不发布 Windows 安装包、`sentinel-lite.exe`、`SentinelLiteSensor.exe`、原生 `.node` 或其他可执行构建产物。本文后续章节中的二进制大小、SHA-256、安装器和二进制 SBOM 记录均为历史构建证据，不是本次 Release 的下载资产或资产清单。当前发布范围、许可证说明和验证结果以 `RELEASE_NOTES.md`、`SOURCE_ONLY_LICENSE_NOTICE.md` 和 `RELEASE_RISKS.md` 为准。

本页只记录实际执行过的命令和观察结果；未执行的测试、未完成的基准和未做的审计均明确标注。

## 1. 主机与工具链（未变化）

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows 11 Pro 10.0.26200，x64 |
| CPU / 内存 | 12th Gen Intel Core i7-12700H / 15.6 GB |
| 逻辑处理器 | 20 |
| Node.js / npm | v24.19.0 / 11.17.0 |
| Git | 2.55.0.windows.4 |
| Rust / Cargo | 1.99.1 stable，`x86_64-pc-windows-msvc` |
| rustfmt / clippy | 已安装，随 stable 1.99.1 |
| VS Build Tools | 17.14.42，标准路径 |
| MSVC cl / link | 19.44.35229.0 / 14.44.35229.0 |
| Windows SDK | 10.0.26100.0 |
| WebView2 | 156.0.4314.50 |

Phase 1.1 没有安装任何新软件或新系统组件。Phase 1 初次安装 Build Tools 时曾因带空格参数被误装到 `C:\Program`，随后只卸载该误装实例并重装到标准路径；Phase 1.1 未再变更工具链，`vswhere` 只报告标准路径实例。

## 2. Phase 1.1 实际验证的修正

- 风险作用域：亲和性正常只显示“亲和性范围内未发现异常”；整体进程结论为“证据不足”。
- `ProtectionState::Protected` 已从 Rust/TypeScript 删除。
- SystemComponent/NoRemove/ParentKeyName/ReleaseType 分类；SystemComponent=1 不再触发 INV-001，未知组件类别为 Unknown/证据不足。
- 注册表输入上限、字符串截断、UI 50 行分页和 truncated 警告。
- HKCU 单一共享视图，HKLM 32/64 仍区分；真实主机重复项减少。
- 引号/未引号启动路径区分，歧义路径不判为文件缺失。
- 路径 Present/Missing/AccessDenied/Unknown 四态；AccessDenied 不生成“不存在”异常。
- 创建时间身份令牌与 PID 复用检查；旧 PID 的 name/parent 不再混入检查对象。
- Tauri 应用级 command ACL；release 中实际调用五个命令成功。
- 选中进程磁盘签名、文件大小/修改时间/最多 256 MiB SHA-256 证据；明确内存映像未验证。
- npm audit、Dependabot 配置与 Cargo advisory 延后记录。

## 3. 自动化检查

以下命令在最终工作树执行并通过：

```powershell
npm run lint
npm run typecheck
npm test
npm run build
npm audit --audit-level=high

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

实际结果：

- 前端：6 个测试文件、37 个测试通过。
- Rust：62 个测试通过，0 个失败；1 个真实主机只读冒烟测试默认忽略。
  - sentinel-core unit：10（分类/去重/哈希/身份 mismatch 等）
  - domain contracts：21
  - Phase 1.1 contracts：21
  - signature contracts：5
  - Tauri 命令/ACL：5
- Rustfmt 通过；Clippy `--all-features -D warnings` 通过。
- 前端生产构建通过：JS 265.37 kB（gzip 80.77 kB），CSS 25.43 kB（gzip 5.77 kB）。
- Tauri capability 回归测试通过：capability 只包含五个应用命令的 allow permission，且没有 `core:default`。
- `npm audit`（完整依赖树）与 `npm audit --omit=dev` 均为 0 个已知漏洞。
- 许可证清单：232 npm 包、395 Cargo 包，0 缺失、0 自动阻断、31 个需要人工复核的表达式。

未执行/没有证据：

- 未执行 Cargo advisory 扫描（未安装 cargo-audit/cargo-deny；见 ADR 0006）。
- GitHub Actions 尚未实际运行；Dependabot 尚未在托管仓库运行。
- 没有 fuzz、真实恶意样本、提权、内核或长时间稳定性测试。
- 没有独立安全审计或渗透测试。

## 4. 真实主机只读冒烟（Phase 1.1）

命令：

```powershell
cargo test -p sentinel-core --test readonly_host_smoke -- --ignored --nocapture
```

实际输出摘要：

```text
software entries: 183, issues: 0, truncated: false
startup entries: 14, issues: 0, truncated: false
processes: 278, path access denied: 159, issues: 0
self inspection state: Complete, affinity restricted: Some(false),
  affinity risk: NoClearIssue, overall risk: InsufficientEvidence,
  signature: Unsigned, hash: Computed, cpu: Some(0.0)
```

### 对旧 187 条统计的更正

- Phase 1 曾报告 187 个软件条目和 23 个启动项；该结果是 HKCU 同时扫描 WOW64 32/64 视图造成的重复计数，**不得继续作为“不同软件/启动项数量”引用**。
- Phase 1.1 改为 HKCU 单次共享视图后，本次同机只读验证为 **183 个唯一注册表条目**、**14 个启动项**。数量仍会随软件与系统状态变化；这不是跨机器承诺。
- 本次扫描 `truncated=false`，未触及条目/字符串上限。

## 5. Release、UI 与 Tauri ACL

构建：

```powershell
npx tauri build --no-bundle
```

结果：release 构建成功，`target/release/sentinel-lite.exe` 约 6.0 MB，SHA-256 `bd51d840a79edf1a5c69d4190518f9f69ec5095019a7035cc47c12680ee57c2c`。应用以普通用户权限启动，测试时通过临时仅回环 CDP 端口驱动 UI；正式运行不需要调试端口。

真实 UI 结果（最终 release）：

- 首页标题“状态未知”，5 个监控组件均为尚未实现/尚未检查；页面不存在“保护正常”文案。
- 软件盘点：183 条，页面分页最多 50 行；未出现卸载/删除按钮。
- 启动项：14 条；显示启用状态未核验、四态路径/解析来源和风险；未出现禁用/删除按钮。
- 进程：宿主快照 278 个进程（数量随系统状态变化）；UI 每页 50 行。对可访问进程执行检查成功，`affinity` 检查完整，磁盘哈希已计算。
- 受保护/身份时间不可读的列表项显示原因且检查按钮禁用，不发起无法校验身份的检查。
- 进程详情明确显示“整体进程结论：证据不足”与局部“亲和性配置结论：检查范围内未发现异常”，并显示磁盘 SHA-256 和“内存映像未验证”。
- 页面运行期间无 CSP/IPC/JS runtime error；无结束进程、修改亲和性、卸载、删除按钮。

### ACL 语义

- `build.rs` 用 `AppManifest` 自动生成五个 `allow-*` 权限；capability 只授予 `main` 窗口这五个命令。
- `core:default` 已移除。应用不使用其他 Tauri core 快捷键/文件/HTTP 命令。
- capability 回归测试比对 `APP_COMMANDS` 与 capability permission 集合。
- release 中实际成功执行软件盘点、启动项检查、进程快照和亲和性检查，证明 ACL 不阻断合法命令。
- 自动化测试无法证明“所有未授权命令都必然被拒绝”，但 Tauri runtime authority 与能力清单是实际执行的 ACL，不是仅注释。

## 6. 资源采样（Phase 1 基线，未在 1.1 重测）

Phase 1.1 没有重新做性能基准。历史 Phase 1 release 首页空闲测量为：整个进程树工作集约 365.0 MB、私有内存约 195.3 MB，CPU 低于计时分辨率；**未达到 <100 MB 工程目标**。本次修正增加身份/哈希/上限逻辑，但没有证据表明长时间空闲或扫描负载达到该目标。

任何后续资源声明必须提供新的参考硬件、UI 状态、是否运行扫描和统计口径。

## 7. 安装状态与仍缺少项

- Rust、Cargo、MSVC、Windows SDK、Tauri CLI、WebView2 均可用；本阶段没有安装新软件。
- 未安装完整 IDE、CMake、Clang、vcpkg、WiX、驱动或第三方安全软件。
- 没有代码签名证书；没有测试 NSIS/MSI 安装包、升级或卸载。
- Cargo advisory scanner（cargo-audit/cargo-deny）未设置；Dependabot 配置尚未运行。
- 当时没有 SBOM、发布物签名或可复现构建证明；§29 后续增加了工程用 CycloneDX SBOM。

## 8. 可复现性与声明边界

- 依赖版本由 `package-lock.json` 和 `Cargo.lock` 固定。
- 许可证清单连续生成稳定；CI 会在构建后检查清单同步。
- 生成的应用权限文件位于 `src-tauri/permissions/autogenerated/`，CI 检查其与命令清单同步。
- 本页结果来自一次本机验证，不代表其他硬件、系统策略或恶意样本环境。
- 没有将任何计划功能写成已完成功能；Phase 2（ETW/SQLite/处置）尚未开始。

## 9. Phase 1.2 输入边界与字段语义补充验证（2026-10-29）

本节的验证对象是 Phase 1.2 的 N1–N3 修正，以及 ADR 0008/0009 的设计边界；两份 ADR 均为设计接受、尚未实现，本阶段没有新增 ETW 会话、SQLite 依赖或数据库文件。以下命令在最终工作树执行并通过。

### 9.1 自动化检查

```powershell
npm run lint
npm run typecheck
npm test
npm run build
npm audit --audit-level=high

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

实际结果：

- 前端：6 个测试文件、40 个测试通过；lint、typecheck、production build 通过。
- Rust：75 个测试通过，0 失败；1 个真实主机只读冒烟测试默认忽略，另行显式执行。
  - sentinel-core unit：14
  - domain contracts：21
  - Phase 1.1 contracts：21
  - Phase 1.2 contracts：9
  - signature contracts：5
  - Tauri 命令/ACL：5
- Rustfmt 通过；Clippy `--all-features -D warnings` 通过，并验证了 MSRV 1.85 兼容性。
- 前端生产构建：JS 266.61 kB（gzip 81.13 kB），CSS 25.43 kB（gzip 5.77 kB）。
- `npm audit --audit-level=high`：0 个已知漏洞。
- 许可证清单：232 个 npm 包、395 个 Cargo 包，0 缺失、31 个需要人工复核；重新生成后无文件差异。

### 9.2 Phase 1.2 行为验证

- N1：进程快照最多 2048 项，名称最多 512、路径最多 4096 Unicode 字符；测试覆盖达到上限、精确边界、名称/路径截断以及 `processInventoryTruncated` / `processStringTruncated`；UI 测试覆盖 `truncated=true` 时显示“快照不完整”，且不再显示“个可见进程”。
- N2：`parse_dword` 只接受精确 4 字节 REG_DWORD；`parse_string` 区分字符串/扩展字符串、类型不符、无效 UTF-16 和截断。类型不符、无效值或读取失败在无非零系统标志时会让分类保持 Unknown；非零 SystemComponent/NoRemove 仍优先判定为系统组件。UI 测试覆盖类型不匹配提示和“不会回退为普通用户软件”。
- N3：启动文件夹中的 desktop.ini/Thumbs.db 不再计为启动项；真实主机冒烟显示启动项 9 个、issues 1，issue 为 `startupFolderMetadataSkipped`，跳过 2 个元数据文件，`truncated=false`。

### 9.3 真实主机只读冒烟（Phase 1.2）

命令：

```powershell
cargo test -p sentinel-core --test readonly_host_smoke -- --ignored --nocapture
```

实际输出摘要：

```text
software entries: 183, issues: 0, truncated: false
startup entries: 9, issues: 1, truncated: false
startup issue: startupFolderMetadataSkipped [Startup folders] 跳过 2 个 Windows 文件夹元数据文件（如 desktop.ini/Thumbs.db），不计为启动程序。
processes: 264, path access denied: 158, truncated: false, issues: 0
self inspection state: Complete, affinity restricted: Some(false), affinity risk: NoClearIssue, overall risk: InsufficientEvidence, signature: Unsigned, hash: Computed, cpu: Some(0.0)
```

进程和启动项数量会随系统状态变化，不是跨机器承诺。本次未触及进程或注册表条目上限。

### 9.4 Release 构建

`npx tauri build --no-bundle` 成功（exit 0），release 编译约 1 分 09 秒；产物 `target/release/sentinel-lite.exe` 约 6.0 MB，SHA-256 `4dec61dcb5e04b8a838488572f698983f957357b62942ec7a5625030b784be33`。本阶段未重新启动 release UI 做人工点击验证；Phase 1.1 的 UI 与 ACL 实测记录保留在第 5 节。发布前仍需重新执行真实 UI 运行和 ACL 验证。

### 9.5 未执行/没有证据

- 未执行系统化 fuzz、压力测试、恶意样本、提权、内核或长时间稳定性测试。
- 没有仓库内自动化 UI 端到端测试；仍以组件测试和人工 release 验证为主。
- Cargo advisory 扫描仍未执行（未安装 cargo-audit/cargo-deny）。
- GitHub Actions 与 Dependabot 尚未在托管仓库实际运行。

## 10. Phase 2.1 ETW 进程传感器最小切片（2026-10-31）

本节记录 `crates/sentinel-sensor` / `SentinelLiteSensor.exe` 的第一个生产切片。源码基线为 `158afd9` 加本阶段未提交工作树；管理员 smoke 首跑取码失败见 10.5，修复后复跑 PASS 见 10.6。

### 10.1 实现范围

- 新增 `sentinel-sensor` library + `SentinelLiteSensor` 可执行程序。
- `etw.rs`：`StartTraceW`、`EnableTraceEx2`、`OpenTraceW`、`ProcessTrace`、定时 `ControlTraceW(STOP)`、`CloseTrace`、残留 session 查询；只创建 realtime session，不写 ETL。
- `decode.rs`：schema 驱动解码 `Microsoft-Windows-Kernel-Process` ProcessStart v0–v4、ProcessStop v0–v2；身份 `(pid, create_time)`；Stop ImageName 按 ANSI 诊断字段处理，不用于身份。
- `pair.rs`：start/stop 配对、orphan stop、unpaired start、duplicate start、PID reuse overlap、sequence mismatch。
- `health.rs`：received/processed/dropped/lost/realtime buffers/decode/partial/ignored、`last_event_at`、状态机与 fail-closed `ok` 判定。
- `main.rs`：session name、duration、JSON summary、`--output`、明确退出码。
- 明确未加入：SQLite、IPC、Tauri UI、行为检测、处置、安装器、签名、自动更新、Defender 修改。

### 10.2 本机 provider schema 只读实验

没有为此启动 ETW 会话；使用本机 Windows 11 26200 的 TDH 元数据 API 枚举事件模板，并用 TDH `TdhGetProperty` 对合成 payload 验证字段偏移：

- ProcessStart：v0（5 字段）、v1（6）、v2（10）、v3（15）、v4（16）；v4 尾部含 `SecurityMitigations`。
- ProcessStop：v0（9 字段）、v1（15）、v2（16）。
- Payload 为 packed 布局，无自然对齐填充；Start v3/v4 的 `MandatoryLabel` SID 长度按 `SubAuthorityCount` 动态计算。
- Start `ImageName` 为 UnicodeString；Stop `ImageName` 为 AnsiString。该结论与 v4.2 探针观察一致。

这些实验只证明 schema/偏移和合成解码，不证明管理员真实 ETW 采集路径。

### 10.3 自动化检查（实际执行）

在最终工作树执行：

```powershell
cargo test -p sentinel-sensor
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p sentinel-sensor --release
```

实际结果：

- `cargo test -p sentinel-sensor`：35 passed，0 failed。
  - decode 8；pair 9；health 14；etw 参数/配置 4。
- `cargo test --workspace`：110 passed，0 failed，1 ignored（Phase 1 的真实主机只读冒烟默认忽略）。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- release 构建：`target/release/SentinelLiteSensor.exe`，248,320 字节，SHA-256 `2dbcd6ea4aae820653b886978c36c82c4b3ec4e3dcedfe449ff959f14fc5f5ae`。
- 构建时 HEAD 为 `158afd965108c81477f1cdf807476c3b79c07f6e`，工作树包含 Phase 2.1 未提交修改；未创建 Git commit。

### 10.4 普通用户 fail-closed 实测（实际执行）

当前 PowerShell/终端非管理员（`IsInRole(Administrator)=False`）。命令：

```powershell
target/release/SentinelLiteSensor.exe --session-name SentinelLite-Sensor-NonAdmin-1793387543 --duration 2 --output target/phase21-nonadmin.json
```

实际结果：

- exit code：`2`。
- JSON：`state=unavailable`、`permission_state=denied`、`session_created=false`、`session_stopped=false`、`trace_opened=false`、`trace_closed=false`、`residual_session=false`、`events_received=0`、`events_lost=null`、`protection_claim=none`、`mode=realtime`、`etl_output=false`。
- `last_error`：`StartTraceW failed: 5: 拒绝访问。 (os error 5)`。
- `logman query -ets` 未发现该 session 名，确认没有残留。
- 没有自行提权、没有创建 session、没有把权限不足报告为采集成功。

### 10.5 管理员真机 smoke（第一次执行：实质检查通过，脚本取码失败）

2026-10-31 03:27:18 在管理员执行窗口（elevated=true）执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/phase21-sensor-smoke.ps1
```

客观结果：

- 二进制：`target/release/SentinelLiteSensor.exe`，SHA-256 `2dbcd6ea4aae820653b886978c36c82c4b3ec4e3dcedfe449ff959f14fc5f5ae`。
- session：`SentinelLite-Sensor-Smoke-29040-20261031-032718`。
- 脚本 `status=FAIL`；唯一失败检查是 `exit_code_is_0`，因为 PowerShell `Start-Process` 在此主机上使用重定向 + 定时 `WaitForExit` 后 `ExitCode` 为空（已用 `cmd /c exit 7` 复现，传感器未崩溃）。
- 传感器自身 JSON：`exit_code=0`、`state=stopped`、`ok=true`、`session_created=true`、`session_stopped=true`、`trace_opened=true`、`trace_closed=true`、`residual_session=false`、`permission_state=granted`。
- 计数：`events_received=37`、`events_processed=37`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`、`partial_events=0`。
- 配对：`paired=18`、`starts_seen=18`、`stops_seen=19`、`unpaired_starts=0`、`orphan_stops=1`、`pid_reuse_detected=0`、`sequence_mismatches=0`。
- 长文件名：3/3 出现在 `pairing.observed_start_image_names`（basename）；`stimulus_launches_ok=true`。
- 残留：`no_residual_session_json=true`、`no_residual_session_logman=true`；stderr 记录 `[sensor] state=Stopped exit=0 received=37 processed=37 dropped=0 lost=Some(0) stopped=true residual=false`。
- 除 `exit_code_is_0` 外的 23 项检查全部为 true；`protection_claim=none`。
- 证据：`target/phase21-smoke/run-20261031-032718.txt`、`summary-20261031-032718.json`、`stderr-20261031-032718.log`（`target/` 被 Git 忽略）。

结论：**传感器管理员 realtime 采集、真实解码、配对、停止、残留检查和长文件名路径已实机通过；本次 FAIL 是 smoke 脚本取码缺陷，不是传感器失败。** 不能把该日志写成 PASS。

### 10.6 管理员真机 smoke 第二次执行（干净 PASS）

脚本已改为临时 `.cmd` 包装器，由 cmd 做 stdout/stderr 重定向并用 `exit /b %ERRORLEVEL%` 传播传感器退出码；脚本同时记录 `process_exit_code`、`json_exit_code`、`stderr_exit_code` 并检查三者一致。2026-10-31 03:35:31 在管理员执行窗口复跑：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "<repo root>\scripts\phase21-sensor-smoke.ps1"
```

实际结果：

- `status=PASS`；`exit_code=0`；`exit_code_source=process`；`process_exit_code=0`、`json_exit_code=0`、`stderr_exit_code=0`、`exit_code_sources_agree=true`；`timed_out=false`；`residual_session=false`。
- 二进制：SHA-256 `2dbcd6ea4aae820653b886978c36c82c4b3ec4e3dcedfe449ff959f14fc5f5ae`；session：`SentinelLite-Sensor-Smoke-25180-20261031-033531`。
- `events_received=37`、`events_processed=37`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`。
- `paired=18`、`starts_seen=18`、`stops_seen=19`、`unpaired_starts=0`、`orphan_stops=1`、`pid_reuse_detected=0`、`sequence_mismatches=0`。单个 orphan stop 是窗口边界内“进程在会话开始前启动、在窗口内退出”的正常现象，不是完整性异常。
- 长文件名 3/3 命中；`session_created=true`、`session_stopped=true`、`trace_opened=true`、`trace_closed=true`；`state=stopped`、`permission_state=granted`、`health.ok=true`、`protection_claim=none`。
- 全部 25 项脚本检查为 true；`logman query -ets` 无残留。
- 证据：`target/phase21-smoke/run-20261031-033531.txt`、`summary-20261031-033531.json`、`stdout-20261031-033531.json`、`stderr-20261031-033531.log`（`target/` 被 Git 忽略）；stderr 摘要为 `state=Stopped exit=0 received=37 processed=37 dropped=0 lost=Some(0) stopped=true residual=false`。

结论：Phase 2.1 管理员 realtime 采集、真实解码、配对、健康计数、停止/残留检查、长文件名 3/3 与明确退出码均已实机通过。

### 10.7 未执行/没有证据

- 未执行长时间稳定性、压力/丢事件注入、权限变化、系统策略变化或多会话并发测试。
- 未执行真实恶意样本、行为检测或任何处置。
- 未接入 SQLite、IPC、UI；本切片不改变首页“状态未知”的语义。
- 未创建 Git commit；本次管理员 smoke 记录 `source_worktree_dirty=true`，source HEAD 为 `158afd9`。

## 11. 提交前审查 P1/P2 修复（2026-10-31）

本节记录对第一次提交前审查（PASS WITH CONDITIONS）的处理。§10 的干净 PASS 属于修复前二进制 `2dbcd6...`；第一轮修复后的二进制已在 §11.3 完成管理员 smoke 与 Ctrl-C 实机复验。第二轮修复后的新二进制仍需复验，见 §11.5。

### 11.1 修复内容

- **P1-1 清理路径**：新增 `SetConsoleCtrlHandler`（Ctrl-C/Ctrl-Break/控制台关闭/关机 → 幂等 stop + `interrupted` 标记）；`RunContext` 实现 `Drop` 兜底 stop/close；session stop 失败缓存不再阻止后续重试。`TerminateProcess` 硬杀和回调内 panic abort 无法在进程内拦截，作为已知限制记录；彻底恢复需要后续稳定 per-install GUID/`owned_guids` 启动恢复设计。
- **P1-2 无 start 的健康误报**：`PairingSnapshot::coverage_issue()` 在 `starts_seen==0` 时返回 issue；`apply_health` 将其标记为 degraded，stop-only 窗口不再 `ok=true`。新增单测。
- **P1-3 STOP 失败挂死**：`ProcessTrace` 移入 trace worker；主线程有界等待 `duration + 10s`，超时后重试 stop、最多再等 2s，然后以 exit 5 返回；trace worker 负责 `CloseTrace`，主线程不会并发关闭同一 handle。新增窗口/退出码/缺失 session 单测；真实 STOP 失败注入仍无实机证据。
- **P2-1 ANSI 代码页**：Stop `AnsiString` 改用 `MultiByteToWideChar(CP_ACP, MB_ERR_INVALID_CHARS)`，不再按 UTF-8 解码；新增 CP936 `测试.exe` 单测（本机 ACP=936）。
- **P2-2 coverage**：只有 provider enabled、trace opened 且 `ProcessTrace` 返回 0 时才声明 `process_start_stop` coverage。
- **P2-3 permission_state**：`Granted` 移到 EnableTraceEx2 + OpenTraceW 都成功之后；Enable/Open 的权限错误映射为 `Denied`。
- **P2-4 open start 上限**：`PairingState.starts` 上限 65,536；溢出计 `start_overflow` 并进入完整性异常/降级。新增单测。
- **P2-5 生命周期测试**：新增 exit code、merge、窗口完整性、缺失 session 查询/停止、interrupt 标记、Drop 清理、stop-only 窗口健康、start overflow、stop-only coverage、CP936 ANSI、image basename 长度上限等单测；总数 47。
- **P2-6 长名证据边界**：文档明确 smoke 3/3 只验证 Start ImageName；Stop ANSI ImageName 仅诊断，不进入配对或长名断言。
- **P2-7 顶层文档**：同步 `README.md`、`docs/DETECTION_COVERAGE.md`、`docs/KNOWN_LIMITATIONS.md`、`docs/THREAT_MODEL.md`，明确主应用普通权限、独立传感器管理员权限/只读/不接入 UI。
- **P2-8 窗口完整性**：报告新增 `interrupted`、`elapsed_ms`、`expected_ms`、`window_complete`；Ctrl-C/外部提前停止/未跑满窗口都会降级。smoke 新增 `not_interrupted` 与 `window_complete` 检查。
- **P3**：`last_error` 按字符上限 1024；`--help` 文案改为“正常运行时 stdout 输出 JSON”；`ControlTraceW stop` 返回非 0 且非 absent 时降级；observed start image basename 每项 512 字符上限。

### 11.2 第一轮修复后的自动化检查（实际执行）

```powershell
cargo test -p sentinel-sensor
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p sentinel-sensor --release
```

实际结果：

- `cargo test -p sentinel-sensor`：47 passed，0 failed。
- `cargo test --workspace`：122 passed，0 failed，1 ignored。
- fmt、clippy 通过。
- release：`target/release/SentinelLiteSensor.exe`，275,456 字节，SHA-256 `c3496aa68cb8fdc0f336f1b1daf0e72e1431e2746c5169fbcdcb1627a4a44a0b`。
- 普通用户复测（新二进制）：session `SentinelLite-Sensor-NonAdmin-1793390451`，`exit=2`、`state=unavailable`、`permission_state=denied`、`session_created=false`、`trace_opened=false`、`residual_session=false`、`interrupted=false`、`window_complete=false`、`events_lost=null`；`logman query -ets` 无残留。该次 `health.last_error` 被 coverage issue 覆盖为 `no process start events observed`，第二轮已修复；新证据见 §11.5。

### 11.3 管理员复验（第一轮修复版二进制，实际执行，PASS）

修复后的二进制属于新的生命周期/健康/ANSI 实现，§10.6 的 PASS 不能直接沿用。2026-10-31 在管理员窗口执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "<repo root>\scripts\phase21-sensor-smoke.ps1"
powershell -NoProfile -ExecutionPolicy Bypass -File "<repo root>\scripts\phase21-sensor-ctrlc-smoke.ps1"
```

**普通 20 秒 smoke（2026-10-31 04:02:31，`status=PASS`）**

- binary SHA-256 `c3496aa6...`；session `SentinelLite-Sensor-Smoke-29268-20261031-040231`。
- `exit_code=0`，process/json/stderr 三个来源一致；`not_interrupted=true`、`window_complete=true`、`elapsed_ms=20017`/`expected_ms=20000`。
- `events_received=36`、`events_processed=36`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`。
- `paired=18`、`starts_seen=18`、`stops_seen=18`、`unpaired_starts=0`、`orphan_stops=0`、`pid_reuse_detected=0`、`sequence_mismatches=0`。
- 长文件名 3/3；`session_created/stopped=true`、`trace_opened/closed=true`、`state=stopped`、`permission_state=granted`、`health.ok=true`、`protection_claim=none`、无残留。
- 证据：`target/phase21-smoke/run-20261031-040231.txt`、`summary-20261031-040231.json`、`stderr-20261031-040231.log`。

**Ctrl-C 交互 smoke（2026-10-31 04:03:10，`status=PASS`）**

- binary SHA-256 `c3496aa6...`；session `SentinelLite-Sensor-CtrlC-23292-20261031-040310`。
- 在 120 秒窗口的第 42.309 秒按 Ctrl-C；process/json 退出码均为 `3`；`interrupted=true`、`window_complete=false`、`state=degraded`。
- `events_received=3`、`events_lost=0`；`session_created=true`、`session_stopped=true`、`trace_opened=true`、`trace_closed=true`、`residual_session=false`；`logman query -ets` 无残留。
- 证据：`target/phase21-ctrlc-smoke/run-20261031-040310.txt`、`summary-20261031-040310.json`。

结论：P1-1 的 Ctrl-C/Drop 清理、P1-2 的 start 覆盖、P1-3 的有界停止/关闭路径以及第一轮 P2 修复已在 `c3496aa6...` 上实机通过。硬杀（`TerminateProcess`）仍无法在进程内清理；真实 STOP 失败注入仍未执行。

### 11.4 未执行/没有证据（本轮）

- 真实 STOP 失败注入（权限/句柄/会话状态失败）。有界等待/重试/退出码路径有代码与单测，但没有实机失败注入。
- 硬杀后的自动恢复（明确后置到稳定 session 所有权设计）。
- 非 ASCII ProcessStop 的真机事件样本（目前只有 CP936 合成解码测试）。
- 长时间稳定性、丢事件注入、权限变化、多会话并发。
- 未创建 Git commit；工作树仍为 `158afd9` + 修改。

### 11.5 第二轮审查 P1-1/P2 修复（2026-10-31）

第二轮审查结论为“条件可提交”。本轮修复：

- **P1-1 timeout late result**：`window_is_complete` 现在显式接收 `failure_present`；失败或超时即使随后收到 worker 的 `(process_rc=0, close_rc=0)`，`window_complete` 也必须为 false。`coverage=["process_start_stop"]` 收紧为：provider enabled、完整健康窗口（`window_complete && health.ok`）、至少一个 ProcessStart 和一个 ProcessStop；Ctrl-C/stop-only/timeout late result 的 coverage 为空。新增 `timeout_late_result_is_not_a_complete_window`、`healthy_full_window_claims_coverage` 单测。
- **P2-1/P2-8 错误原因覆盖**：`apply_health` 只在 `session_created && trace_opened` 时应用 pairing coverage/integrity；`SensorHealth::mark_degraded` 仅在状态转换成功时更新 `last_error`，不会覆盖 `Unavailable/Failed` 的致命错误。新增 `permission_failure_keeps_the_permission_error`、`degraded_does_not_overwrite_a_fatal_error` 单测。
- **P2-5 Ctrl-C smoke**：脚本增加 `session_stop_error`、`session_close_error`、`process_trace_error`、`error` 必须为 null 的检查；该脚本已在第二轮修复版上重新执行并通过，见 §11.6。
- **P2-6 文档矛盾**：ADR 0010 §9 的 Ctrl-C 条目改为“Ctrl-C/Ctrl-Break 已有实机 PASS；控制台关闭/关机只保证尝试 stop，不保证 JSON 落盘”。
- **P2-7 Close/Shutdown**：ADR 与 `KNOWN_LIMITATIONS` 明确控制台关闭/关机可能来不及写报告；`TerminateProcess` 硬杀仍无法进程内清理。
- **P2-2 跨代码页**：保留按 CP_ACP 解码与 `MB_ERR_INVALID_CHARS`；本机 CP936 合成样本通过。未做非 CP936 实机/跨代码页验证；ADR 已注明非 UTF-8/GB18030 代码页可能发生替换字符而不计 Partial。
- **P2-3 coverage 语义**：ADR 明确 coverage 只在完整健康窗口内实际观察到 start 和 stop 时声明，不再表示“链路已打开”。
- **P2-4 真实 STOP 失败注入**：仍未执行；新增的 timeout late-result 单测覆盖报告字段，但不覆盖真实 `ControlTraceW(STOP)` 失败导致 `ProcessTrace` 不返回的端到端路径。

第二轮自动化结果：

- `cargo test -p sentinel-sensor`：51 passed，0 failed。
- `cargo test --workspace`：126 passed，0 failed，1 ignored。
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- 新 release：`target/release/SentinelLiteSensor.exe`，275,456 字节，SHA-256 `5e0e118aaf1bcffe6e80fbc9a0e01eb1a7392614a1c234e30f38de92b984541c`。
- 普通用户复测：session `SentinelLite-Sensor-NonAdmin-1793391483`，`exit=2`、`state=unavailable`、`permission_state=denied`、`session_created=false`、`trace_opened=false`、`residual_session=false`、`window_complete=false`、`events_lost=null`、`last_error="StartTraceW failed: 5: ..."`（不再是 coverage issue）；证据 `target/phase21-nonadmin-postreview.json`，`logman query -ets` 无残留。
- 管理员 smoke/Ctrl-C 已在第二轮修复版 `5e0e118a...` 上重新执行并通过，见 §11.6。§11.3 的 `c3496aa6...` 证据保持为第一轮历史记录，不与新哈希混用。

### 11.6 第二轮修复版管理员复验（2026-10-31，PASS）

**普通 20 秒 smoke（2026-10-31 04:24:24，`status=PASS`）**

- binary SHA-256 `5e0e118aaf1bcffe6e80fbc9a0e01eb1a7392614a1c234e30f38de92b984541c`；session `SentinelLite-Sensor-Smoke-6272-20261031-042424`。
- `exit_code=0`，process/json/stderr 三来源一致；`timed_out=false`、`not_interrupted=true`、`window_complete=true`、`elapsed_ms=20013`/`expected_ms=20000`。
- `events_received=36`、`events_processed=36`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`。
- `paired=18`、`starts_seen=18`、`stops_seen=18`、`unpaired_starts=0`、`orphan_stops=0`、`pid_reuse_detected=0`、`sequence_mismatches=0`。
- 长文件名 3/3；`session_created/stopped=true`、`trace_opened/closed=true`、`state=stopped`、`permission_state=granted`、`health.ok=true`、`protection_claim=none`、无残留。
- 证据：`target/phase21-smoke/run-20261031-042424.txt`、`summary-20261031-042424.json`、`stderr-20261031-042424.log`。

**Ctrl-C 交互 smoke（2026-10-31 04:31:36，`status=PASS`）**

- binary SHA-256 `5e0e118a...`；session `SentinelLite-Sensor-CtrlC-9352-20261031-043136`。
- 在 120 秒窗口的第 3.869 秒按 Ctrl-C；process/json 退出码均为 `3`；`interrupted=true`、`window_complete=false`、`state=degraded`。
- `events_received=3`、`events_lost=0`；`session_created/stopped=true`、`trace_opened/closed=true`、`residual_session=false`；`session_stop_error`/`session_close_error`/`process_trace_error`/`error` 均为 null；`logman query -ets` 无残留。
- 证据：`target/phase21-ctrlc-smoke/run-20261031-043136.txt`、`summary-20261031-043136.json`。

**操作说明**：同一命令曾被粘贴两次，第二个 Ctrl-C 实例（`run-20261031-042601.txt`）没有收到 Ctrl-C，跑满 120 秒后 exit 0、`window_complete=true`，因此脚本按预期记为 FAIL。这不是传感器失败，也不是 Ctrl-C 路径失败；该实例无残留。有效 Ctrl-C 证据为 `run-20261031-042554.txt`（第一次 PASS）和 `run-20261031-043136.txt`（最新 PASS）。最终残留检查为空。

## 12. Phase 2.2 只读传感器状态 IPC（2026-10-31）

> 本节记录 Phase 2.2 当时的“UI 未接入”状态；Phase 2.3 已在第 13 节把 `get_sensor_status` 接入首页。历史证据不改写。

本阶段新增 `crates/sentinel-ipc`、`SentinelLiteSensor --serve` 和 Tauri `get_sensor_status` 后端命令；没有接入 UI、SQLite、事件流或停止/配置命令。

### 12.1 实现范围

- `sentinel-ipc::protocol`：版本 1 JSON 请求/响应，单 status 请求；请求上限 512 字节、响应上限 16 KiB；未知字段/版本、非法 UTF-8、超长消息均拒绝。
- `sentinel-ipc::server`：固定端点 `\\.\pipe\SentinelLiteSensor.Status.v1`；message 模式、单实例、`PIPE_REJECT_REMOTE_CLIENTS`、`FILE_FLAG_FIRST_PIPE_INSTANCE`；DACL 只授予当前用户 `GRGW` 与 SYSTEM/Administrators `GA`；客户端用户 SID 校验。
- `sentinel-ipc::client`：`SECURITY_IDENTIFICATION`；校验服务端镜像 basename 为 `SentinelLiteSensor.exe` 且 token elevated；连接/读写超时；对服务端 Disconnect/Connect 过渡期做幂等重试。
- `etw.rs`：`--serve` 在 trace 打开后启动 IPC，忽略时长并保持运行；Ctrl-C 停止；IPC 失败只降级健康，不伪造健康。
- `src-tauri`：新增 `get_sensor_status` 命令，返回 `running`/`degraded`/`stopped`/`unavailable`/`communicationFailed`；命令 ACL 与 capability 同步为六个命令。
- 明确不做：IPC stop/config、事件流、原始事件、UI 接入、SQLite、自动启动/提权。

### 12.2 自动化检查（实际执行）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build -p sentinel-sensor --release
cargo build -p sentinel-ipc --example probe
```

实际结果：

- `cargo test --workspace`：168 passed，0 failed，1 ignored（Phase 1 真实主机只读冒烟默认忽略）。
- fmt、clippy 通过。
- 新增覆盖：
  - 协议 9 项：合法 round-trip、错误版本、未知字段、非法 UTF-8、超长请求/响应、成功/错误响应、成功响应缺 status。
  - 管道 10 项：正常状态查询、缺失管道、非传感器服务端拒绝、重复管道实例拒绝、malformed/oversized 请求后恢复、匿名客户端拒绝后恢复、客户端超时、空闲客户端超时后恢复、快速 connect/close 后恢复。
  - server 9 项：生产 SDDL 含 Medium mandatory label、测试 SDDL 不含 label、生产 SDDL 可解析出 SACL、瞬时 connect 错误分类、P2-A 原始错误码保留（恢复成功/失败两种）、恢复失败关闭已创建句柄、fatal 分类。
  - win 3 项：特权恢复首次成功、首次失败后重试成功、连续失败达到上限。
  - sensor 1 项：运行中 `health_ok` 要求 Active、零已知丢失、start 覆盖、无完整性异常；另 1 项验证 fatal IPC 错误后 `cleanup` 停止 ETW session。
  - Tauri 6 项映射：healthy/degraded/stopped/unavailable/communicationFailed，以及 lost-connection 映射。
- release：`target/release/SentinelLiteSensor.exe`，362,496 字节，SHA-256 `ae4314fead31a0e1b07d7d55794ae7f03eb902c6a75fa1f9a48e59b07b4ce490`。
- probe：`target/debug/examples/probe.exe`，SHA-256 `4645765ea9dafa60baf5f29e30cb4a89a3c11942399b72b8b0a56a8052a55554`；使用与 Tauri 后端相同的 secure client，只用于管理员手工验证。
- `cargo build -p sentinel-lite --release` 与 `npx tauri build --no-bundle` 通过；Tauri 生成了 `allow-get-sensor-status` 权限，应用构建到 `target/release/sentinel-lite.exe`。未做 release UI 点击验证。

### 12.3 普通用户 fail-closed（实际执行）

**历史记录（旧二进制，不作为最终证据）**：session `SentinelLite-Sensor-Serve-NonAdmin-1793438560` 的早期回归见 `target/phase22-serve-nonadmin-final3.json`；它早于 P1/P2 最终修复，不绑定最终二进制。

**最终二进制回归（2026-10-31T21:12:09）**：

```powershell
target/release/SentinelLiteSensor.exe --serve --session-name SentinelLite-Sensor-FinalNonAdmin-1793452329 --duration 20 --output target/phase22-final-nonadmin.json
```

- 运行前后二进制 SHA-256 均为 `ae4314fead31a0e1b07d7d55794ae7f03eb902c6a75fa1f9a48e59b07b4ce490`。
- `exit_code=2`；`state=unavailable`；`permission_state=denied`。
- `session_created=false`、`trace_opened=false`、`ipc_requested=true`、`ipc_started=false`、`ipc_error=null`、`residual_session=false`。
- `last_error=StartTraceW failed: 5: 拒绝访问`；`logman query -ets` 无残留。
- 证据：`target/phase22-final-nonadmin.json`、`target/phase22-final-nonadmin-record.txt`。
- 结论：普通用户无法通过 `--serve` 绕过 ETW 权限；不会创建 IPC 服务或伪造运行状态。

**证据边界**：

- 最终二进制普通用户回归：**已完成**（上方记录绑定 `ae4314...`）。
- 最终二进制 Medium/elevated probe：**未完成**。
- 旧版 `4505268a...` 的 Medium PASS：**仅历史证据**，不代表最终版本。

### 12.4 管理员真实 ETW + IPC 复验（修复前二进制 PASS）

2026-10-31 17:32:46 在管理员窗口执行（二进制 `fff41bc9...`，尚未含 P1 Medium label 修复）：

```powershell
cargo build -p sentinel-sensor --release
cargo build -p sentinel-ipc --example probe
powershell -NoProfile -ExecutionPolicy Bypass -File "<repo root>\scripts\phase22-ipc-smoke.ps1"
```

实际结果：

- `status=PASS`；`probe_exit=0`；`probe_json_parsed=true`。
- `probe` JSON：`sensor_state=active`、`health_ok=false`、`events_received=0`、`events_processed=0`、`events_lost=0`、`uptime_ms=533`。刚启动且窗口内暂无事件，`health_ok=false` 是 fail-closed 预期。
- 传感器摘要：`ipc_started=true`、`ipc_error=null`、`exit_code=3`、`session_stopped=true`、`residual_session=false`。
- 脚本全部关键检查为 true：`sensor_state_valid`、`uptime_present`、`counts_present`、`health_ok_boolean`、`sensor_exit_3`、`no_ipc_error`、`no_residual_logman`。
- 证据：`target/phase22-ipc-smoke/run-20261031-173246.txt`、`probe-20261031-173246.json`、`summary-20261031-173246.json`。

此前的脚本失败是测试脚本本身的解析/错误处理缺陷：probe 直出 `SensorStatus` 被误当 `{version, ok, status}`，以及启动早期 probe 的预期 stderr 被 `$ErrorActionPreference=Stop` 终止。已分别修复；最终 17:32:46 运行干净 PASS。该 PASS 证明真实 elevated sensor + secure client + 只读状态查询链路可用，不表示窗口内已收到进程事件或系统受保护。

### 12.5 P1-1 Medium mandatory label 修复（PASS）

提交前审查指出 P1-1：提权传感器创建的管道如果只有 DACL，没有显式 mandatory label，会按 High/System 完整性并带 `NO_WRITE_UP`；普通 Medium 主应用即使 DACL 允许写，也会在 MIC 检查处收到 `ERROR_ACCESS_DENIED`。此前的管理员 E2E probe 与 sensor 同为 High，未覆盖该路径。

修复：

- 生产管道 SDDL 增加 `S:(ML;;NW;;;S-1-16-8192)`（Medium label + NO_WRITE_UP），DACL 保持当前用户 `GRGW` + SYSTEM/Administrators `GA`。
- 创建带 SACL 的管道期间临时启用 `SeSecurityPrivilege`，创建/失败后恢复原状态；恢复失败则关闭管道并返回错误。
- 新增 SDDL/ SACL 单元测试；非提权测试服务端使用 `ServerConfig::for_testing` 的 DACL-only 配置，不冒充生产标签路径。
- 新增 `scripts/phase22-medium-probe.ps1`：必须在非提权 PowerShell 运行，记录 `is_admin=False`、`whoami /groups`、probe JSON 和退出码。

复验步骤：

```powershell
# 管理员窗口：启动新二进制传感器，保持运行
Set-Location '<repo root>'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
target\release\SentinelLiteSensor.exe --serve --session-name "SentinelLite-Sensor-Medium-$stamp" --output "target\phase22-medium-summary-$stamp.json"

# 普通用户窗口：在传感器仍运行时执行
Set-Location '<repo root>'
powershell -NoProfile -ExecutionPolicy Bypass -File "<repo root>\scripts\phase22-medium-probe.ps1"

# 管理员窗口：按 Ctrl-C 停止传感器
```

**Medium 客户端复验（2026-10-31 20:30:14，PASS）**

管理员窗口用新二进制启动传感器，普通用户窗口运行 `phase22-medium-probe.ps1`：

- `identity=<non-admin user>`、`is_admin=False`；`whoami /groups` 明确显示 `Mandatory Label\Medium Mandatory Level`，SID `S-1-16-8192`。
- `probe_exit=0`；probe JSON：`sensor_state=active`、`permission_state=granted`、`health_ok=true`、`events_received=6`、`events_processed=6`、`events_lost=0`、`uptime_ms=31244`、`last_error=null`。
- 传感器摘要：`ipc_started=true`、`ipc_error=null`、`interrupted=true`、`exit_code=3`、`session_stopped=true`、`residual_session=false`；观察到的 start 镜像包含 `probe.exe` 和 `whoami.exe`。
- 证据：`target/phase22-medium-probe/run-medium-20261031-203014.txt`、`probe-medium-20261031-203014.json`、`target/phase22-medium-summary-20261031-202943.json`。

该结果证明：High/System 传感器创建的管道经 Medium mandatory label 后，普通 Medium 主应用客户端可以完成写打开和只读状态查询；P1-1 闭合。新二进制未再单独跑 elevated smoke，但 Medium 路径是更严格的 MIC 路径，且此前的 elevated smoke 已在 `fff41bc9...` 上 PASS。

### 12.6 未验证/已知限制

- P1 修复版 `4505268a...` 的 Medium 客户端复验已 PASS；P2-A/P2-B 后的最终 `ae4314fe...` 尚未重新跑 elevated/Medium 实机复验；此前 elevated smoke 已在 `fff41bc9...` 上 PASS。
- Medium probe 在 `health_ok=true`、6 个事件时通过；仍未做长时间连续事件、并发连接和压力测试。
- `OpenProcessToken`/elevation 校验已在修复前 elevated smoke 和新二进制 Medium probe 上通过。
- 跨用户 DACL 拒绝、并发连接压力、长时间运行。
- 固定管道名的同用户抢占/DoS；管理员级服务端伪造超出本切片威胁模型。
- UI 未接入 `get_sensor_status`，首页仍为“状态未知”。
- 未创建 Git commit；工作树包含 Phase 2.2 修改。

### 12.7 P2-A / P2-B 修复（2026-10-31）

- **P2-A**：`CreateNamedPipeW` 失败后立即保存原始 LastError，再执行 `LocalFree`、`SecurityPrivilegeGuard::restore()` 等可能改变 LastError 的操作；失败分支用保存的 code 构造 `ServerError::CreatePipe`。测试覆盖 restore 成功和 restore 失败（restore 错误码 1300 不覆盖原 code 5）。
- **P2-B**：`SecurityPrivilegeGuard::restore()` 现在最多尝试 3 次并返回 `Restored` / `RestoredAfterRetry` / `Failed`；连续失败产生 `ServerError::PrivilegeRestore` 或带 `privilege_restore_failed=true` 的 `CreatePipe`。`ServerError::is_fatal()` 明确标记这两类错误。Sensor 的 `handle_ipc_start` 对 fatal 错误 mark_failed 并返回 `Failure::etw`，`execute` 的 `?` 将错误带回 `run`，随后 `cleanup` 停止 ETW session；不会仅 degraded 后继续运行。已创建管道在恢复失败时由 `finalize_pipe_creation` 关闭；测试用注入的 close 函数验证句柄关闭。Drop 只做一次 best-effort 恢复，不承担生命周期退出决策。
- **Server::start 错误边界**：`run` 启动失败时可能丢弃 ready sender；`Server::start` 的 Disconnected/Timeout 分支现在检查 `thread.join()` 的 `Ok(Err(error))` 并返回原始错误，而不是统一 `ThreadPanic`。重复管道测试断言 `CreatePipe` 及非零 `code`；新增 `server_start_propagates_a_fatal_startup_error`、`server_start_propagates_a_fatal_create_pipe_error`；Sensor 的 `fatal_ipc_error_from_server_start_stops_the_sensor_lifecycle` 通过真实 `Server::start` 调用注入 fatal 启动错误，再验证 `cleanup` 停止 session。
- 测试无法真实制造 `AdjustTokenPrivileges` 持续失败；重试和致命决策使用注入闭包/纯函数验证，Windows 特权失败路径未做管理员实机注入。
- 新二进制 `ae4314fead31a0e1b07d7d55794ae7f03eb902c6a75fa1f9a48e59b07b4ce490`（362,496 字节）尚未重新执行 elevated smoke/Medium probe；此前的 Medium probe PASS 属于 P1 修复版 `4505268a...`，不能把本二进制写成已实机复验。

## 13. Phase 2.3 首页真实传感器状态接入（2026-10-31）

本阶段只修改前端、类型/API 适配和文档，不修改 `sentinel-ipc`、`sentinel-sensor` 的 Phase 2.2 IPC 核心，不增加命令、事件流、SQLite 或处置能力。

### 13.1 实现范围

- `src/types.ts` / `src/lib/api.ts`：新增与 Rust `SensorStatusReport` 对应的类型，并注册只读命令 `get_sensor_status`；嵌套 `sensor` 字段按真实 IPC 序列化为 snake_case。
- `src/lib/sensorStatus.ts`：纯映射把 `linkState`、`sensor_state`、`health_ok` 映射为 Active / Unavailable / Failed / Unknown 与独立的 Healthy / Unhealthy / Unknown；`events_lost=null` 显示“未知”，不伪造成 0；缺失传感器时所有计数显示“—”。
- `src/lib/useSensorStatus.ts`：页面挂载立即查询，之后每 4 秒轮询；请求不重叠；卸载停止；失败清空旧报告；旧的被取消响应不能覆盖新结果。
- `src/pages/HomePage.tsx`：首页主状态区展示真实传感器状态、独立健康状态、链路状态、权限、最近查询；采集统计展示已接收、已处理、丢失、运行时间，以及最近事件、处理丢弃、解码错误。
- `src/App.tsx`：首页离开导航时卸载（从而停止轮询），同步更新已失真的静态文案；整体保护状态仍来自 `ProtectionOverview`，保持“状态未知”。
- `src/styles-phase23.css`：新的传感器状态视觉采用克制的圆角卡片、等宽数字和文字+图形状态标记，不依赖颜色单独表达含义。

### 13.2 自动化检查（实际执行）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npx vitest run
npx eslint . --max-warnings 0
npm run build
npx tauri build --no-bundle
git diff --check
```

- Rust：**168 passed / 0 failed / 1 ignored**；fmt、clippy 通过。
- 前端：**67 passed / 0 failed**（8 个测试文件）；新增状态映射 14 项、轮询 hook 7 项、首页场景 7 项，并补充 API 命令名/返回契约测试。
- 前端构建、类型检查、eslint 通过；`npx tauri build --no-bundle` 通过，最终应用二进制为 `target/release/sentinel-lite.exe`。
- `git diff --check` 通过，仅有 CRLF 提示。

### 13.3 真实 Windows 实机验证（非提权应用 + 提权传感器）

验证时传感器为 Phase 2.2 最终二进制 `ae4314fead31a0e1b07d7d55794ae7f03eb902c6a75fa1f9a48e59b07b4ce490`（362,496 字节）；应用为 Phase 2.3 最终 release `sentinel-lite.exe`，SHA-256 `06628fdcc5c90c45fff1cefad8afd5cd3a574131a3d26c5a8d434ad3c52d956d`（6,287,360 字节）；probe SHA-256 `e0131056d3490787c10cacabba7f29811b037e28b0ffea86324a8ced18d11bc2`。

- 主应用由**非管理员** PowerShell 启动，`is_admin=False`、用户为普通 `<non-admin user>`；传感器通过一次性 UAC 提权助手启动。应用自身不自动提权。
- **场景 A（运行且健康）**：首页显示 `运行中` + `健康`，链路“已连接”、权限“已授予”；计数为 12/12/0、运行时间 8 秒；probe 在 uptime 3,173 ms 时报告 `sensor_state=active`、`health_ok=true`、`events_received=2`、`events_processed=2`、`events_lost=0`、`last_error=null`。
  - 截图：`target/phase23-real/scenario-a-healthy-20261031-213547.png`。
- **场景 D（运行但健康异常）**：传感器刚启动时 probe 报告 `sensor_state=active`、`events_received=0`、`events_processed=0`、`health_ok=false`、`last_error="sensor health not proven"`；首页显示 `运行中` + `健康异常`，两个状态独立呈现。
  - 截图：`target/phase23-real/scenario-startup-20261031-213547.png`。
- **场景 B（运行→停止过渡）**：在首页保持打开时 force-kill 传感器并由管理员 `logman stop` 清理 ETW session；首页在下一轮查询后显示 `不可用` + `健康未知`，链路“未连接”、权限“未知”，四个统计全部变为 `—`，**没有保留上一次的 Active 或 Healthy 或计数**。
  - 截图：`target/phase23-real/scenario-b-stopped-20261031-213547.png`。
  - 清理后 `logman query -ets` 无 Phase 2.3 session 残留；无 sensor/app 进程残留。
- **场景 C（Tauri 命令调用被拒绝）**：仅由前端自动化测试覆盖：mock `get_sensor_status` reject，页面显示 `不可用` + `健康未知` + 明确错误，不崩溃、不显示 Healthy；“重新查询”恢复。本次没有在真实 Tauri 环境中注入命令级 reject；真实的链路失败路径由场景 B 的停止过渡覆盖。

证据：
- `target/phase23-real/elevated-evidence-20261031-213547.json`
- `target/phase23-real/wrapper-evidence-20261031-213543.json`
- `target/phase23-real/probe-20261031-213547.json`
- 三张截图见上；当时 README 使用的 `docs/screenshots/home.png` 为场景 A 截图的客户端区域裁剪（1280×820），不含桌面或浏览器内容。

### 13.4 未验证/已知限制

- 场景 C 的真实 Tauri 命令级拒绝未注入；只有自动化模拟。
- 停止过渡使用 force-kill + `logman stop`，不是 Ctrl-C；传感器因此没有写 summary。硬杀可能留下 session 的通用限制仍成立。
- 未验证长时间连续轮询、并发 UI 操作、多显示器/DPI 缩放、传感器长时间运行后的计数回绕或丢失统计。
- 未验证跨用户 DACL 拒绝、并发连接压力；这些仍属于 Phase 2.2 的已知限制。
- 整体保护状态仍为“状态未知”；本阶段没有引入告警、事件流、行为检测或处置。

## 14. Phase 2.4 有界进程事件管道（2026-10-31）

本阶段只修改 `crates/sentinel-sensor` 与文档/验证脚本；不修改 `sentinel-ipc`、Tauri 命令、前端或 Phase 2.1–2.3 的 IPC/UI 契约。

### 14.1 实现范围

- `event.rs`：标准化 `ProcessEvent`（事件/会话 ID、`started/stopped`、`(pid, create_time)` 身份、父 PID、ImageName、ETW 信息、`process_started_at`、`occurred_at`/`received_at`/`normalized_at`、`completeness.issues`、四态配对）、`InvalidProcessEvent`、`PairingRevision`、`SessionEnd`。
- `pipeline.rs`：`sync_channel` 有界队列（默认 4,096，`--queue-capacity` 64–65,536）、回调非阻塞 `try_send`、独立 normalizer 消费者、计数/积压/丢弃/峰值、sink trait、关闭排空与 5 秒超时、消费者 panic/断开处理。
- `pair.rs`：重复 Start/Stop、孤儿 Stop 有界保留与乱序回补、PID 复用不跨身份、同 PID 不同创建时间的 Stop 身份不匹配、零创建时间不确定、打开/关闭/孤儿表上限与淘汰计数。
- `etw.rs`：回调只解码并入队；`RunReport` 增加 `session_id`/`pipeline`；关闭时先排空管道再生成 summary；健康对账；`live_health_ok` 纳入消费者/丢弃/关闭状态。
- `main.rs`：新增 `--queue-capacity`。
- `scripts/phase24-pipeline-smoke.ps1`、`scripts/phase24-ctrlc-smoke.ps1`：可复现的实机验证脚本。

### 14.2 自动化检查（实际执行）

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
npx vitest run
npx eslint . --max-warnings 0
npm run build
npx tauri build --no-bundle
git diff --check
```

- Rust workspace：**202 passed / 0 failed / 1 ignored**；其中 `sentinel-sensor` 库 **88 passed**。fmt、clippy 通过。
- 新增覆盖：正常 Start/Stop、孤儿 Stop、重复 Start、重复 Stop（已关闭与孤儿）、乱序回补 + revision、PID 复用、身份不匹配 Stop、零创建时间、缺失父/ImageName、序列缺失、队列满载、消费者退出、正常关闭排空、关闭超时、sink 错误、20,000 事件持续负载、会话重启隔离、解码到标准化链、健康计数对账、队列容量校验、SessionEnd 原因。
- 前端回归：**67 passed / 0 failed**（8 个文件）；eslint、`npm run build` 通过。Phase 2.4 未修改前端。
- `npx tauri build --no-bundle` 通过，应用 `target/release/sentinel-lite.exe` SHA-256 `aa7c337d8c475cf9215e708f7098dba18d7a2961939ea1d18330449889cb5b20`（6,287,360 字节）；最终传感器 `target/release/SentinelLiteSensor.exe` SHA-256 `62c730472974e4e9ef5a1988425881d5e56c5ecb2c756b3ce2d2df0d10918c7d`（456,704 字节）。
- `git diff --check` 通过（仅 CRLF 提示）。

### 14.3 真实 Windows 实机验证

环境：Windows 11 Pro 10.0.26200，x64；传感器为上述最终 `62c730...` release；probe 为 Phase 2.2/2.3 最终 `e0131056d3490787c10cacabba7f29811b037e28b0ffea86324a8ced18d11bc2`。所有实机传感器均通过一次性 UAC 辅助进程以管理员权限启动；应用/探针脚本不自行提权。

**A. 20 秒管道 smoke（实际执行，PASS）**

- 证据：`target/phase24-pipeline-smoke/run-20261031-221928.txt`、`summary-20261031-221928.json`、`stdout-20261031-221928.json`、`stderr-20261031-221928.log`。
- 进程退出码 0，JSON/进程/stderr 三个退出码来源一致；`window_complete=true`；`interrupted=false`。
- `events_received=20`、`events_processed=20`、`events_dropped=0`、`ignored_events=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`、`state=stopped`、`ok=true`、`last_error=null`。
- 管道：`pipeline.received=20`、`normalized=20`、`invalid=0`、`dropped_queue_full=0`、`dropped_consumer_gone=0`、`backlog=0`、`peak_backlog=11`、`shutdown_timed_out=false`、`consumer_state=stopped`、`sink_errors=0`；`session_end.reason=completed`、`open_start_events=0`。
- 配对：`paired=9`、`starts_seen=9`、`stops_seen=11`、`orphan_stops=2`、`unpaired_starts=0`、重复/pid reuse/sequence/乱序/身份不确定均为 0。
- 首个 paired stop：`event_id=7294-18e3a310b759fef4:000000000000000c`，ImageName `timeout.exe`，`pairing.state=paired`，`start_event_id=...:0000000000000009`，`sequence_match=true`；三个时间字段均存在（`occurred_at=2026-10-31T14:19:32.176Z`、`received_at=...14:19:33.388Z`、`normalized_at=...14:19:33.388Z`）。
- 刺激进程 `cmd.exe`/`timeout.exe` 出现在 observed start images；`cmd_stimulus_observed=true`；无 session 停止/关闭/ProcessTrace/overall 错误；`logman query -ets` 无残留。

**B. Ctrl-C 排空 smoke（实际执行，PASS）**

- 证据：`target/phase24-ctrlc-smoke/run-20261031-222015.txt`、`summary-20261031-222015.json`。
- 自动化 helper 附着传感器控制台并发送 `CTRL_C_EVENT`（helper exit 0）；传感器在 60 秒窗口内退出。
- 进程/JSON 退出码均为 3；`interrupted=true`、`window_complete=false`、`state=degraded`（Ctrl-C 预期降级）。
- `events_received=28`、`events_processed=28`、`events_dropped=0`、`events_lost=0`、`decode_errors=0`。
- `pipeline.consumed=28`、`normalized=28`、`backlog=0`、`peak_backlog=8`、`dropped_queue_full=0`、`dropped_consumer_gone=0`、`shutdown_timed_out=false`、`consumer_state=stopped`；`session_end.reason=interrupted`，`open_start_events=2`（中断时仍未结束的进程生命周期）。
- `residual_session=false`；`logman query -ets` 无残留。

**C. IPC 健康状态（实际执行，PASS；提权 probe）**

- 证据：`target/phase24-ipc/run-20261031-222037.txt`、`probe-20261031-222037.json`、`summary-20261031-222037.json`。
- 提权 probe 在传感器运行中返回：`sensor_state=active`、`permission_state=granted`、`events_received=17`、`events_processed=17`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`decode_errors=0`、`health_ok=true`、`last_error=null`、`uptime_ms=5630`。
- 随后 Ctrl-C：`interrupted=true`、`pipeline.consumer_state=stopped`、`session_end.reason=interrupted`、无残留 session。该次 probe 在提权 PowerShell 中运行，不是 Phase 2.3 的 Medium 客户端复验；Phase 2.2/2.3 的 Medium mandatory label 与 Phase 2.3 非提权应用截图仍是同一条 IPC 路径的历史实机证据，Phase 2.4 未修改 IPC 代码或响应字段。

**D. 失败/修正记录（透明说明）**

- 第一次管道 smoke（`run-20261031-220815.txt`）除 `session_end.reason` 外全部通过；原因是 `SessionEndReason` 被内部标记序列化为对象 `{reason:"completed"}`。已改为字符串 + 独立 `error` 字段，重跑 PASS（最终二进制证据为 22:19:28）。
- IPC 健康脚本最初两次失败（`run-20261031-221139.txt`、`run-20261031-221247.txt`）是验证脚本的 PowerShell 变量名大小写冲突（`$ProbePath` 与 `$probePath` 指向同一变量，导致把 JSON 输出路径当命令执行），不是产品故障；修正脚本后 PASS，最终二进制复验为 22:20:37（上面的 C）。该脚本位于 `target/`，未提交。

### 14.4 未验证/已知限制（本阶段）

- 队列满载、消费者退出、关闭超时、慢 sink：仅有自动化测试，未在真实 Windows 上注入队列溢出/慢消费者；不能写成实机 PASS。
- 硬杀（`TerminateProcess`）不会写 summary、不会发送 SessionEnd；内存队列和未完成生命周期丢失，且仍可能留下 ETW session。正常时长结束和 Ctrl-C 已验证排空；硬杀恢复未验证。
- 未做长时间连续运行、并发多会话、跨用户 DACL、多显示器/DPI、性能基准或真实 ETW loss 注入。
- 生产 sink 不持久化完整事件流；`recent_events`/`first_*` 只是有界诊断样本，不能当作完整历史。
- 应用二进制因 Phase 2.4 重新构建而哈希变化，但应用源码未改；本次没有重新点击 UI，使用与 Tauri 后端相同客户端策略的探针验证了 IPC 健康路径。整体保护状态仍为“状态未知”。

## 15. Phase 2.5 标准化进程事件交付（2026-10-31）

### 15.1 实现范围

- 新增共享 `sentinel-events` crate；传感器、IPC、存储和检测使用同一事件定义。
- 新增独立只读事件管道 `\.\pipe\SentinelLiteSensor.Events.v1`，与状态管道共用 DACL/MIC/SID/镜像名/提权校验。
- 新协议：`after` 游标、1–128 条批次、`cursor_expired`/`missed_records`/`cursor_ahead`/`session_closed`/`has_more`；`PairingRevision` 与 `SessionEnd` 作为同流记录交付。
- 传感器 `EventBroker` 默认 4,096 条、`--event-buffer` 可配置 64–65,536；满时淘汰最旧并累计 `evictions`，不阻塞 ETW。
- Tauri `get_sensor_events(after, limit)`；probe 支持 `--events` 和 `--watch-events`。

### 15.2 自动化检查（实际执行）

- `cargo test --workspace`：**252 passed / 0 failed / 1 ignored**；其中 IPC 协议/管道测试、事件端到端 `delivery_ipc`、broker 游标/过期/确认测试。
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- 前端：**85 passed / 0 failed**；`tsc`、eslint、`npm run build`、`npx tauri build --no-bundle` 通过。

### 15.3 真实 Windows 实机验证（PASS）

- 证据：`target/phase25-event-delivery/run-20261031-233130.txt`、`summary-20261031-233130.json`、`events-20261031-233130.json`。
- 传感器 release SHA-256 `b583910e7dbb1c54aca6c1a8e272a10c62f58de47486aab1830729539f66ad9a`。
- 管理员 `--serve`，提权 watcher 使用生产客户端策略：`processEvents=32`、`records=33`、`pairedStopped=13`、`unpairedStopped=2`、`sessionEndRecords=1`、`sessionEndReason=interrupted`、`cursorExpired=0`、`missedRecords=0`、`sessionRestarts=0`、`transientErrors=0`。
- 传感器 summary：`event_ipc_started=true`、`event_ipc_error=null`、`delivery.batches_served=84`、`delivery.session_end_delivered=true`、`delivery.evictions=0`、`delivery.session_closed=true`；`events_processed=32`、`events_lost=0`、`dropped=0`；退出码 3（Ctrl-C）；无残留 session。
- 配对修正的 IPC 传递由 `crates/sentinel-sensor/tests/delivery_ipc.rs` 自动化端到端测试证明；该次真实运行未出现乱序修正，不能写成“实机观察到 revision”。

### 15.4 失败/修正记录

- 第一次 smoke（`run-20261031-230516.txt`）产品侧全部通过，但脚本把 probe 的 camelCase 字段（`processEvents`）当成 snake_case 读取，导致 4 个 watch 检查为 false。修正脚本字段后重跑 PASS；这是测试 harness 问题，不是产品故障。

## 16. Phase 2.6 本地事件持久化（2026-10-31）

### 16.1 实现范围

- `sentinel-storage`：SQLite schema v2、WAL、`user_version` 迁移、幂等事件写入、配对修正更新、会话/缺口/`SessionEnd`、有界查询、保留策略、摄取 worker。
- 数据库路径 `%LOCALAPPDATA%\app.sentinellite.desktop\SentinelLite\events.db`；当前用户 ACL；不加密、不上传。
- Tauri `get_storage_status`、`query_stored_events`、`get_stored_event`、`list_sensor_sessions`；应用启动时启动摄取线程，退出时停止。

### 16.2 自动化检查（实际执行）

- `cargo test --workspace`：252 passed；存储库 15 项，覆盖 schema v2、重开迁移、重复批次幂等、修正更新/先到、查询过滤/分页、会话结束/未观察、保留、加载截断、WAL 大小。
- 前端 85 passed；Tauri build 通过。

### 16.3 真实 Windows 实机验证（PASS）

- 证据：`target/phase26-app-ingest/run-20261031-234142.txt`、`summary-20261031-234142.json`、`database-20261031-234142.json`。
- 非提权 `sentinel-lite.exe` 运行中；管理员传感器 `--serve` 采集 28 个事件；应用摄取后数据库：`schema_version=2`、`session_count=1`、`event_count=28`、最新会话 `has_session_end=true`、`end_reason=interrupted`、`event_count=28`；`gap_count=0`、`invalid_count=0`；传感器 summary `session_end_delivered=true`、`batches_served=85`；无残留 session。
- 应用重启后 `storage_probe` 成功打开同一数据库，`schema_version=2`；旧测试数据中的 in-order 退出 `paired_event_id` 已由 v1→v2 迁移修正为对应启动事件。
- `icacls`：`NT AUTHORITY\SYSTEM:(I)(F)`、`BUILTIN\Administrators:(I)(F)`、当前用户 `(I)(F)`；无 Everyone/Users 访问。数据库位于当前用户本地应用数据目录，不在仓库或临时目录。

### 16.4 失败/修正记录（真实机器发现）

- **P1 schema 重开缺陷**：将 `SCHEMA_VERSION` 升到 2 后，`migrate` 只接受 0/1，导致重启应用报告“schema version 2 is unsupported (supported: 2)”。修复为接受 2，并新增文件数据库重开测试。应用重启后迁移/读取通过。
- **P2 摄取退避过长**：传感器缺席时退避上限 15 秒；一个约 7 秒的短会话在长缺席后被完全错过（`batches_served=0`）。将本地管道错误退避上限改为 2 秒，并在真实传感器缺席 20 秒后重跑，摄取恢复。
- **P2 配对链接**：schema v1 将为 in-order 退出事件把自身写成 `paired_event_id`，历史详情会显示自身而非启动事件；修正为按事件类型存另一侧，并加入 v1→v2 迁移。
- **P1 多消费者尾部竞争**：事件页打开时，UI 实时轮询和后台持久化 worker 同时读取事件端点。传感器在任一客户端确认 `SessionEnd` 后立即关闭，导致另一次运行中持久化 worker 只拿到 13/28 条且没有 `SessionEnd`。修复为在任一确认后额外保留事件端点 2 秒，让第二个消费者读取尾部；在 UI 打开的真实运行中重跑，持久化得到 28/28 条和 `has_session_end=true`。该修复仍是有界尽力而为，不保证任意慢客户端。

## 17. Phase 2.7 进程事件可视化（2026-10-31）

### 17.1 实现范围

- 新“进程事件”页面：状态条、异常/不确定项、实时事件列表、生命周期详情、历史筛选/分页、行为分析面板、本地数据状态。
- 实时列表来自 `get_sensor_events`，1 秒轮询，500 条上限；历史来自 SQLite；配对修正应用到实时退出事件；断开/过期/未知/空数据分别显示。
- 样式沿用现有浅色纸面/仪表语言；无远程字体/图表/演示数据。

### 17.2 自动化检查（实际执行）

- 前端 **85 passed / 0 failed**（8 个文件）：新增 `processEvents` 合并/修正/过期/重启/上限测试和 `EventsPage` 渲染/历史/分析/断开/未知 ETW 测试。
- `tsc --noEmit`、`eslint --max-warnings 0`、`npm run build`、`npx tauri build --no-bundle` 通过。

### 17.3 真实 Windows 实机验证（PASS）

- 应用 release 运行；真实传感器会话结束后事件页显示：170 条实时事件、会话已结束、2 条未配对事件、数据库正常、持久化运行中（最后一次链路状态仍显示已连接）；实时行显示真实 `conhost.exe`/`cmd.exe` 等事件和配对/完整性。
- 历史查询分页显示本地事件（截图时 371 条/3 个会话；最终数据库 1009 条/11 个会话）；本地数据状态显示路径、计数、保留策略和摄取源不可用错误。
- 点击“分析会话”后显示真实会话 170 条事件、`可疑模式`、`PROC-BURST-001`、证据事件 ID 和“本结果不判断恶意性”；数据质量显示 ETW 丢失未知、传感器健康未取得。
- 截图：`docs/screenshots/events.png`、`docs/screenshots/events-analysis.png`。
- UI Automation 未暴露 WebView2 内容；验证使用窗口截图和坐标点击，未做自动化 E2E 点击套件。

### 17.4 未验证/已知限制

- 未在 100% 缩放以外的多 DPI/多显示器组合复验；未做移动尺寸/键盘全流程人工测试。
- 真实运行未观察到 out-of-order `PairingRevision`；修正显示由自动化测试覆盖。
- 历史查询/分页/筛选的真实交互点按了会话列表与分析按钮；未逐项点击全部筛选控件。

## 18. Phase 2.8 可解释行为分析（2026-10-31）

### 18.1 实现范围

- 新 `sentinel-detection` crate：输入质量门禁、4 条规则、证据/阈值/误报说明/规则版本、会话分析。
- Tauri `analyze_session` 读取 SQLite（最多 20,000 条）、当前会话可附加实时传感器健康，其他会话明确未知。
- 事件页行为分析面板：整体状态、输入质量、规则结果、证据事件跳转、误报说明。

### 18.2 自动化检查（实际执行）

- 检测库 12 项：正常样本无规则命中、启动/退出 burst、孤儿比例、重复启动一致性、样本不足、会话未结束、已知丢失、未知 ETW、缺时间、证据有界、规则目录/版本/误报说明、配置校验。
- workspace 252 passed；前端 85 passed；fmt/clippy/build/Tauri build 通过。

### 18.3 真实 Windows 实机验证（PASS）

- 证据：`target/phase28-detection/run-20261031-234210.txt`、`summary-20261031-234210.json`、`analysis-20261031-234210.json`、`docs/screenshots/events-analysis.png`。
- 非提权应用运行；管理员传感器采集 170 条真实事件（40 个短生命周期 `cmd.exe` 刺激），应用写入 SQLite；分析：`overall=suspicious`、`event_count=170`、`session_closed=true`、规则 `PROC-BURST-001`（85 次启动/10 秒）、`PROC-EXIT-002`（85 次退出/10 秒）、`DATA-ETW-UNKNOWN-001`；证据 64 个事件 ID；含误报说明；解释未使用“恶意软件”判定。
- `delivery.session_end_delivered=true`、`batches_served=16`、无残留 session。
- 最终产物：`SentinelLiteSensor.exe` SHA-256 `b583910e7dbb1c54aca6c1a8e272a10c62f58de47486aab1830729539f66ad9a`（514,560 字节）；`sentinel-lite.exe` SHA-256 `1d739a5a1f43fd3dc444d3f3617bceec23fa6e2188af8fcdba69ff513b4bfdd4`（8,093,696 字节）。
- 这是对真实历史事件的处理，不是模拟事件；但刺激是正常管理活动，不是恶意样本。

### 18.4 未验证/已知限制

- 没有真实恶意样本、误报/漏报率、长期基线或跨会话规则；不能宣称检测覆盖率。
- 此次变更前的历史会话没有最终 ETW 丢失统计（旧 `SessionEnd` 未包含），因此整体常为 `indeterminate` 或 `data_insufficient`；新会话的 `final_stats` 在 §19 加入。这是保守行为，不是正常结论。
- 规则阈值未用大规模样本校准；没有自动处置；没有命令行/文件/网络/内存证据。
- 真实运行未观察到 `PROC-ORPHAN-003` 或 `PROC-INTEGRITY-004` 触发；它们由自动化测试覆盖。

## 19. Phase 2.5–2.8 封存前可靠性验证（2026-11-01）

### 19.1 范围与发现

本轮只处理四项与数据可信度和长期运行有关的问题：`SessionEnd` 最终统计、持续采集资源、SQLite 故障、UI 会话结束/重连。没有进入 Phase 3，没有新增检测规则、自动处置或写操作。

实际发现并修复：

- **P2 SessionEnd 缺少最终统计**：旧 `SessionEnd` 只有配对快照；最终 ETW 丢失和 pipeline 计数没有随会话结束持久化，检测只能显示未知。
- **P2 停止重试成功后仍可能保留失败 probe**：`stop_and_close` 在已有失败 `stop_probe` 时不更新重试结果，导致健康/最终统计仍报失败。改为成功 probe 不被后续失败覆盖。

### 19.2 SessionEnd final_stats

新增 `SessionEndStats`（`crates/sentinel-events/src/model.rs`）：

- ETW：`events_lost`、`realtime_buffers_lost`；只有在本次运行成功 `ControlTraceW STOP`（`stop_rc == 0`）且无残余 session 时才写入，否则保持 `null`。
- Pipeline：`pipeline_capacity`、`pipeline_received`、`pipeline_enqueued`、`pipeline_consumed`、`pipeline_normalized`、`pipeline_invalid`、`pipeline_dropped_queue_full`、`pipeline_dropped_consumer_gone`、`pipeline_filtered_ignored`、`pipeline_backlog`、`pipeline_peak_backlog`、`pipeline_backlog_at_shutdown_timeout`、`pipeline_shutdown_timed_out`、`pipeline_sink_errors`。
- 配对未完成状态仍在 `SessionEnd.pairing`（`unpaired_starts`、`pending_orphan_stops` 等）和 `open_start_events`（最多 32 个样本）中；没有另造重复字段。

数据流：

1. `RunContext::cleanup` 在停止 ETW、检查残余 session 后调用 `EventPipeline::set_final_etw_stats`。
2. 消费者排空队列时在自己的 `SessionEnd` 中快照 pipeline 计数；最终 ETW 值从共享槽读取。
3. `EventPipeline` 的 summary 快照包含同一个 `SessionEnd`，所以即使没有 IPC 客户端，`--output` summary 也有最终统计。
4. SQLite 继续把整个 `SessionEnd` 写入 `sessions.session_end_json`；schema v2 不变。旧 JSON 缺少 `final_stats` 时通过 serde 默认值读为 `null`，不能当作零。
5. `analyze_session` 和 `analyze_probe` 优先用 `final_stats` 构造 `SessionDataInput`；没有最终统计时才回退到当前实时传感器健康。最终 ETW 已知为零时不再因为“传感器健康不可得”而自动判未知；未知 ETW 仍为未知。
6. 事件详情显示最终统计；实时页在会话结束后显示“会话已结束”，不再把最后链路状态显示为“已连接”。

验证：

- 模型：`session_end_final_stats_round_trip_and_old_records_stay_unknown`（新块往返、旧 JSON 解析为 `None`）。
- Pipeline：`session_end_contains_final_pipeline_and_etw_stats`、`session_end_preserves_known_and_unknown_etw_loss_values`（未知不填 0）。
- 停止语义：`final_etw_loss_stats_require_a_clean_stop`、`a_successful_stop_retry_replaces_a_failed_probe`。
- 存储/检测：`reopening_a_file_database_preserves_cursor_and_final_stats`、检测库最终统计用例（已知零、已知 ETW 丢失、最终统计未知不回落实时值、pipeline 缺口、最终 invalid 计数、未知 ETW）。
- 真实运行见 §19.5：summary、SQLite、`analyze_probe` 三处均出现 `final_stats`，且最终 ETW 丢失为 0、未知规则不再触发。

### 19.3 数据库故障验证

使用独立测试数据库/临时文件，不修改用户现有数据库。测试覆盖：

- **写入失败**：SQLite `query_only=ON` 注入（`#[cfg(test)]`，不进入生产 API）。`Storage::apply_batch` 返回错误，游标不前进；恢复后同一批次幂等重放并前进。
- **只读文件**：Windows 只读属性设置后重新打开，`Storage::open` fail closed 或写入被拒绝；不会替换/删除文件。
- **非数据库/损坏文件**：`Storage::open` 返回错误，原文件字节保持不变。
- **重新打开**：文件数据库关闭后重新打开，`ingestion_state` 游标保持；`session_end_json` 中的 `final_stats` 仍可读取。
- **摄取 worker**：写入失败时 `IngestionState::Degraded`，`last_cursor` 不变；恢复后重放并回到 `Running`。sensor 与数据库是不同进程，数据库故障不会阻塞 ETW。

测试：`write_failure_does_not_advance_the_cursor_and_recovery_replays`、`a_read_only_database_is_fail_closed_for_writes`、`a_non_database_file_is_rejected_without_replacement`、`reopening_a_file_database_preserves_cursor_and_final_stats`、`a_write_failure_leaves_the_cursor_unchanged_and_replays_after_recovery`。

未验证：物理磁盘满、文件系统配额、真实 ACL 拒绝、SQLite 运行中损坏、并发多实例。

### 19.4 UI 会话结束与重连

- 修复：事件页状态条在 `sessionClosed` 时显示“会话已结束”并使用 caution 语气，不再显示“已连接”。
- 最后一帧事件保留并明确标记会话结束；不能当作当前完整活动。
- “重新连接”清空实时游标/事件并重新轮询；新传感器会话的 `session_id` 不同，`mergeLiveBatch` 重置列表，旧会话事件不会混入。重连同时使在途旧请求失效（`requestId` 递增），避免旧响应写回重置后的列表。
- 自动化：`EventsPage` 新增“会话结束后重连并隔离旧/新事件”和“存储 SessionEnd 最终统计显示”测试；`useLiveEvents` 新增在途旧响应失效测试；前端 88 passed。
- 未做：WebView2 自动化点击 E2E、多 DPI/多显示器复验；真实 UI 验证仍依赖人工截图/点击。

### 19.5 30 分钟持续采集

最终二进制从干净工作区 `5f9fb24` 构建并运行 30 分钟（2026-11-01 01:00:44–01:31:09，1820.3 秒，119 个采样点）。测试使用独立 SQLite 数据库（`target/phase29-longrun/db`），真实用户数据库通过目录 junction 隔离；运行前后真实数据库 SHA-256 均为 `dc62b26cfb6dc2fa1cac606bc60eafa68cedfad118879a28aad3d228c9d00d8d`，未改变。证据：`target/phase29-longrun/run-20261101-010044.txt`、`samples-20261101-010044.csv`、`summary-20261101-010044.json`、`storage-20261101-010044.json`、`analysis-20261101-010044.json`、`wrapper-20261101-005927.txt`。

二进制：
- `SentinelLiteSensor.exe` SHA-256 `2e3866100591225c5d8ff9b965b70d82d6d70e6bb39fb1af658a772ee193fa9e`（518,656 字节）。
- `sentinel-lite.exe` SHA-256 `ac87349af208354be5c4b868856e952b3bf57454bc38919ac8f8229ba89be154`（8,115,200 字节）。
- `probe.exe` SHA-256 `56f312aa7cb47a8b3dcab05eebd5ddacb852f23d5e759f55eef283bac97fd2af`。

运行结果（PASS，全部 harness 检查通过）：
- 传感器：`events_received=1435`、`events_processed=1435`、`events_dropped=0`、`events_lost=0`、`realtime_buffers_lost=0`、`pipeline_peak_backlog=14`、`pipeline_normalized=1435`、`pipeline_invalid=0`、`pipeline_filtered_ignored=0`、最终 `pipeline_backlog=0`、`pipeline_shutdown_timed_out=false`、`session_end_reason=interrupted`、退出码 3、`residual_session=false`。delivery：`batches_served=3211`、`evictions=0`、`session_end_delivered=true`。
- SQLite：schema v2、`event_count=1435`、`invalid_count=0`、`session_count=1`、`gap_count=0`、`final_stats_sessions=1`；最新会话 `closed=true`、`end_reason=interrupted`、`has_session_end=true`；事件数与 `events_processed` 一致。数据库主文件 + WAL 从 263,688 字节增长到 6,557,992 字节（净增长约 6.0 MiB；最终主文件 2,293,760 + WAL 4,264,232）。SQLite 可能延迟归还空间，不能把该数字当作固定上限。
- 检测：`total_events=1435`、`session_closed=true`、`etw_events_lost=0`、`realtime_buffers_lost=0`、`pipeline_dropped=0`、`pipeline_backlog=0`、`pipeline_shutdown_timed_out=false`、`invalid_records=0`、`ingestion_gaps=0`、`truncated=false`、`overall=no_anomaly_detected`，无规则命中。该结论只表示当前规则范围内未发现异常，不是安全或恶意判定。
- 配对：`paired=711`、`unpaired_starts=6`、`orphan_stops=7`、`pending_orphan_stops=7`、`starts_seen=717`、`stops_seen=718`；没有重复/身份不确定/序号不匹配/溢出。未配对属于会话边界正常情况，检测未把它升级为异常。
- 资源：sensor CPU 平均 0.003%（最大 0.02%），工作集 6.66–9.14 MB（最后 7.83 MB），私有内存 5.72–6.82 MB；应用进程树 CPU 平均 1.508%（最小 0.708%，启动峰值 36.687%），工作集 388.21–398.68 MB（最后 393.2 MB），私有内存 205.25–239.38 MB（最后 221.68 MB），进程数 7。应用包含 WebView2 子进程，**仍未达到 <100 MB 的初始目标**，不能宣称满足历史内存目标。
- 吞吐/摄取：1435 事件 / 1820.3 秒 ≈ 0.79 事件/秒（刺激为每 15 秒 2 个短生命周期 cmd.exe，加上本机背景活动）；采样到的 DB 摄取滞后最大 4 条，最终 DB 事件数与传感器处理数一致；无缺口。
- 正式 30 分钟前还完成了一次最终二进制 2 分钟预检（`run-20261101-005626.txt`）：除时长检查外全部通过（116 events、final stats、DB、检测），用于确认最终制品和 harness。
- 结论边界：这只是单机 30 分钟窗口的观察，不是长期稳定性、所有负载或性能基准的证明。

### 19.6 自动化检查（本轮实际执行）

- 目标 Rust 包：`sentinel-events`、`sentinel-ipc`、`sentinel-sensor`、`sentinel-storage`、`sentinel-detection`；**183 passed / 0 failed**（18 + 10 + 27 + 4 + 12 + 91 + 1 + 20）。
- 前端：**88 passed / 0 failed**（11 个文件）；`tsc --noEmit`、eslint、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy`（变更包 + `sentinel-lite`，`-D warnings`）、`npx tauri build --no-bundle` 通过；最终 release 二进制见 §19.5。
- 未重复运行整套 workspace 基线；本轮只跑受影响包与新增测试，避免把未变化模块的旧结果混入本轮证据。

### 19.7 未验证/限制

- 30 分钟不是长期稳定性证明；只代表该机器、该负载窗口。
- 没有物理磁盘满/配额/ACL 故障注入；没有真实恶意样本、fuzz/压力或并发多实例。
- 最终二进制的 UI 会话结束/重连只由前端自动化测试覆盖，未做 WebView2 E2E。
- 旧历史会话没有 `final_stats`，检测仍是未知；新会话的最终统计也依赖应用在会话结束窗口内读取到 `SessionEnd`。
- ETW 停止失败、残余 session、硬杀后的最终统计保持未知；硬杀仍可能留下 ETW session。

### 19.8 Phase 2 封存结论

**Phase 2 最终验收：PASS WITH LIMITATIONS（2026-11-01）。**

已完成条件：

- Phase 2.1–2.8 功能、测试、文档和验证记录完整；阶段提交与封存前可靠性验证提交均在 `phase-1-foundation` 历史中。
- 最终可靠性验证覆盖 `SessionEnd.final_stats`、SQLite 故障/重放、UI 会话结束/重连和单机 30 分钟持续采集；详见 §19.1–§19.7。
- 工作区 clean；`target/phase29-longrun/` 证据保留在本机并被 `.gitignore` 的 `/target/` 忽略，不进入 Git。

明确限制（不得在封存报告或后续宣传中省略）：

- 30 分钟单机观察不是长期稳定性、所有负载或性能基准的证明。
- 应用进程树工作集约 388–399 MB，**未达到 <100 MB 的初始目标**。
- 没有真实恶意样本误报/漏报率；Phase 2.8 的 4 条规则不是恶意软件检测引擎。
- SQLite 不加密、无防篡改/审计链；旧历史会话缺少 `final_stats` 时保持未知。
- 未做 WebView2 自动化 E2E、物理磁盘满/配额/ACL 故障注入、并发多实例、硬杀恢复、跨用户 DACL 或小时级长期运行。
- 没有告警队列、通知、自动处置或写操作。

**Phase 3 尚未启动；本封存不授权进入 Phase 3。**

## 20. Phase 3 切片 1：原生防护状态只读读取（2026-11-01）

### 20.1 范围

- 本节记录用户于 2026-11-01 明确授权启动 Phase 3 后的第一个开发切片；Phase 2 封存结论 §19.8 保持有效，不因本切片改变。
- 新增 `sentinel-core::native_protection`：用户主动触发的 Defender/防火墙只读快照。
- Defender：WMI `ROOT\Microsoft\Windows\Defender` / `MSFT_MpComputerStatus` 的 5 个固定字段。
- 防火墙：COM `INetFwPolicy2::get_FirewallEnabled` 的 Domain/Private/Public 三个配置文件。
- 不修改/关闭任何设置，不启动子进程，不读取策略、排除项、ASR、篡改保护或防火墙规则，不改变整体保护状态。

### 20.2 数据与失败语义

- `DefenderStatus` / `FirewallStatus` 的提供程序状态为 `observed` / `partial` / `unavailable` / `accessDenied` / `error`。
- 字段缺失、无实例、无权限、WMI/COM 错误均保持 `null` 并返回稳定 issue code；不填 `false`/`true`，不回退为“已启用”。
- 整体 `observation` 只表示数据完整性（`complete`/`partial`/`unavailable`），不是保护结论。
- UI 只有用户点击后才查询；刷新失败清空旧结果；不显示“保护正常”。

### 20.3 自动化检查（实际执行）

- `cargo test -p sentinel-core`：82 passed / 0 failed（其中 `native_protection` 12 项：完整字段、字段缺失、访问拒绝、无实例、防火墙单/全失败、缺失配置文件、观测完整性、错误码映射、camelCase 序列化且无 `protected`/`overallState`）。
- `cargo test -p sentinel-lite`：15 passed / 0 failed（含 capability 与应用命令名单一致性）。
- `cargo test --workspace`：279 passed / 0 failed / 1 ignored（`readonly_host_smoke` 默认忽略）。
- 前端：94 passed / 0 failed（12 个文件），新增 5 个 `NativeProtectionCard` 测试，并更新/新增首页分组与集成测试；`npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check` 通过；`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings` 通过；`npx tauri build --no-bundle` 通过。
- 新增命令 ACL：`get_native_protection_status` 自动生成 `allow-get-native-protection-status`，capability 只允许固定命令；生成文件 `src-tauri/permissions/autogenerated/get_native_protection_status.toml`。

### 20.4 真实 Windows 验证（PASS）

- 当前 shell 非管理员（`admin=False`）；`cargo run -p sentinel-core --example native_protection_probe -- --output target/phase30-native-protection/probe-final.json` 成功。
- 探针结果：`observation=complete`；Defender `antivirusEnabled=true`、`realTimeProtectionEnabled=true`、`amServiceEnabled=true`、`signatureVersion=1.463.70.0`、`signatureAgeDays=0`；防火墙 Domain/Private/Public 均为 `true`；无 issue。与 PowerShell 的 `Get-MpComputerStatus` / `Get-NetFirewallProfile` 输出一致。
- release 应用 `target/release/sentinel-lite.exe` 启动后，通过 UI Automation 查找并调用“读取原生防护状态”按钮；真实 WebView2 页面显示“已读取全部字段”、Defender 三项“已启用”、签名版本 `1.463.70.0`、签名年龄 `0 天`、防火墙三个配置文件“已启用”，并显示“不能证明实际拦截能力”的脚注。截图：`docs/screenshots/native-protection.png`。
- 验证后应用进程已停止；本切片不启动传感器、不创建 ETW session，不需要管理员权限。
- 最终 release 二进制品：`target/release/sentinel-lite.exe` SHA-256 `965ac5a31c2ac3986e902964bcab29d1338899615ca0c4186f3622be6fbde54b`（8,160,256 字节）。
- 这验证的是本机正常路径，不是企业策略、第三方 AV、篡改保护或故障注入场景。

### 20.5 已知限制

- 快照不是持续监控；两次查询之间状态可能变化。
- Defender WMI 可能因第三方 AV、策略、篡改保护或 provider 损坏而不可用；此时保持未知。
- 防火墙只读取三个配置文件的启用位，不读取规则或实际拦截状态。
- WMI 的 `ConnectServer`/`ExecQuery` 没有独立超时（仅枚举 `Next` 为 5 秒）；极端 WMI 卡死时查询可能长时间不返回，但不阻塞 WebView，也尚未实现取消。
- 未实机注入 WMI/COM 无权限、无实例、字段缺失、企业策略拒绝或第三方 AV 场景；这些路径由纯解析测试覆盖。
- 不验证 Defender 云保护、行为监控、ASR、排除项、规则完整性或签名吊销。
- 同用户恶意进程伪造本机 WMI/COM 超出当前威胁模型；本能力不产生处置。

## 21. Phase 3 切片 2：服务与计划任务只读盘点（2026-11-01）

### 21.1 范围

- 新增 `sentinel-core::services::list_services`：SCM 服务/驱动枚举 + 逐项 `QueryServiceConfigW` 只读配置。
- 新增 `sentinel-core::scheduled_tasks::list_scheduled_tasks`：Task Scheduler COM 递归枚举任务、主体和动作可执行路径。
- “软件体检”新增“服务与计划任务”标签；两个来源独立加载、失败和分页。
- 不启动/停止/创建/删除/修改服务；不运行/创建/修改/删除/启用/禁用任务；不读取 Arguments/WorkingDirectory/触发器；结果仅在内存，不写 SQLite。

### 21.2 实现中发现并修正的问题

- **P2 QueryServiceConfigW 64 KiB 缓冲返回 1783**：在本机 Windows 11 26200 上，`QueryServiceConfigW` 传入 64 KiB 缓冲会返回 `RPC_X_BAD_STUB_DATA (1783)`；8 KiB/32 KiB 正常。改为 8 KiB 起始、按 `pcbBytesNeeded` 增长、上限 32 KiB，超过则条目保持 partial。最小 windows/windows-sys 探针均复现，确认不是单一 wrapper 问题。
- **P2 Task Scheduler 集合索引为 1-based**：初版使用 0-based，导致动作全部读取失败、任务只返回 88/192；改为 1-based 后 192/192 任务、动作路径均读取成功。
- **P2 服务账户/空路径语义**：驱动通常没有服务账户，文件系统驱动可能没有 ImagePath；初版把这些成功读取的空字段当作 partial，导致 470 个条目误报。改为为空字段显示“未提供”，只有配置读取失败或路径无法解析才标记 partial。
- **P2 服务路径解析**：补充扩展名缺失的 `svchost -k ...`、设备命名空间 `\??\...` 含空格路径；仍拒绝无法安全解析的未加引号普通路径，避免把参数当路径展示。
- **P2 TaskScheduler CLSID**：初版误用 Task Scheduler 1.0 `CLSID_CTaskScheduler`，`ITaskService` 返回 `E_NOINTERFACE (0x80004002)`；改用 `TaskScheduler`（`0f87369f-...`）后正常。
- **P2 任务动作路径引号**：`IExecAction.Path` 可能带外层引号；新增 `bounded_path` 只去除包裹引号，不改变内部路径。

### 21.3 自动化检查（实际执行）

- `cargo test -p sentinel-core`：**95 passed / 0 failed / 1 ignored**（lib 39 项，其中新增 13 项服务/任务测试）。
- `cargo test -p sentinel-lite`：**15 passed / 0 failed**（含 capability/命令名单一致性）。
- `cargo test --workspace`：**293 passed / 0 failed / 1 ignored**（原记录误写为 292；各测试二进制实际合计 293）。
- 前端：**101 passed / 0 failed**（13 个文件），新增 6 个 `ServiceTaskPanel` 测试和 1 个 CleanupPage 标签测试；`npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings`、`npx tauri build --no-bundle` 通过。
- 新增 ACL：`list_services`、`list_scheduled_tasks` 自动生成 `allow-list-services` / `allow-list-scheduled-tasks`；capability 仅允许固定命令。

### 21.4 真实 Windows 验证（PASS）

- 当前 shell 非管理员（`admin=False`）；`cargo run -p sentinel-core --example inventory_probe -- --output target/phase3-slice2/inventory-probe-final3.json` 成功。
- 服务：**763 项**，`observation=partial`，`truncated=false`；762 observed、1 accessDenied（`McmSvc`），0 error。读取耗时 **108 ms**。
  - 样例：`Audiosrv` running / auto / `NT AUTHORITY\LocalService` / `C:\WINDOWS\System32\svchost.exe`；`Spooler` running / auto / `LocalSystem` / `C:\WINDOWS\System32\spoolsv.exe`。
  - 与 `sc.exe qc` 的启动类型、账户和二进制路径一致；路径已去除参数（如 `svchost.exe` 后不保留 `-k ...`）。
- 计划任务：**192 项**，`observation=observed`，`truncated=false`；192/192 observed，0 error/denied。读取耗时 **294 ms**。
- 本次 JSON 大小：服务约 233 KB、任务约 81 KB；UI 每页 50 行，未一次渲染全部记录。
  - 样例：`\<sample task>_...` running / enabled / runAs `<user>` / Exec `C:\Users\<user>\<sample target>.exe`；`\<system task>_{...}` ready / author `<vendor>` / runAs `Users` / Exec `C:\Program Files\<vendor>\<app>.exe`。
  - 动作路径已去除外层引号；未读取参数、WorkingDirectory 或触发器。
- release 应用启动后，通过 UI Automation 打开“软件体检” → “服务与计划任务” → 点击“开始只读盘点”；真实 WebView2 表格显示 763 个服务、192 个计划任务、`部分读取`/`已读取` 徽标和逐项状态。
- 截图：`docs/screenshots/services-tasks.png`（服务）、`docs/screenshots/scheduled-tasks.png`（计划任务）。
- 验证后应用进程已停止；未启动传感器、未创建 ETW session、未修改任何服务/任务、未执行任何任务。
- 最终 release 二进制品：`target/release/sentinel-lite.exe`（SHA-256/大小见 §21.6）。

### 21.5 数量/时间边界

- 服务：最多 1,024 条；枚举缓冲 4 MiB / 64 页；8 秒；配置缓冲 8 KiB 起始、32 KiB 上限；名称/显示名/账户 512、路径 1,024 字符。
- 计划任务：最多 1,024 个任务、256 个文件夹、16 个动作/任务；10 秒；名称 512、路径 1,024、主体 512、动作路径 1,024 字符。
- 达到上限显示 `truncated` 与 issue；单条/单文件夹失败不使整个列表不可用。
- 命令在 `spawn_blocking` 中执行；UI 分页 50 行。单次 SCM/COM 调用不可中断，极端卡死时后台任务可能长时间不返回，但不阻塞 WebView。

### 21.6 未验证/限制

- 未实机注入 SCM/Task Scheduler 服务停止、权限拒绝、超大列表、COM 卡死或并发读取；`truncated` 路径只由代码/单元路径覆盖，未用真实 1,024+ 条目触发。
- 未建立软件→服务/任务关联图；未读取触发器/计划/条件、上次运行结果、服务依赖、目标文件存在性/签名/哈希。
- 服务枚举包含驱动；计划任务包含隐藏任务（`TASK_ENUM_HIDDEN`）。这不表示它们已启用或会运行。
- 普通用户权限下 1/763 服务配置访问拒绝；不同系统/权限模型可能更多，结果会按条目保持 partial/accessDenied。
- 结果不持久化，刷新后需重新读取；不提供持续监控或变化检测。
- 最终 release 二进制品：`target/release/sentinel-lite.exe` SHA-256 `0a44869ab5c2478808cf9c0a9eaf18b36de890380a98acb9a82618797a3d46f3`（8,266,752 字节）。

## 22. Phase 3 切片 3：启动项只读盘点（2026-11-01）

### 22.1 范围与路线图的差异

- Phase 1 已有有限启动项检查；本切片升级现有 `sentinel-core::startup::scan_startup_items` 和“软件体检”的“启动项”标签，不新增重复的数据源、Tauri 命令或标签。
- 因而 Tauri 命令仍为 15 个；现有 `scan_startup_items` / `allow-scan-startup-items` ACL 继续生效，不需要新增 capability。
- 路线图第 3 项还包含 AppX/Store 盘点与软件→启动项/服务/任务关联图；本切片只完成启动项盘点，关联图仍为“尚未实现”。
- 新增来源级/条目级 `InventoryObservation`、`StartupSourceStatus`、注册表值类型、Raw Value 可读性、`.lnk` 目标与参数/工作目录存在性布尔证据。
- 不读取 Winlogon、IFEO、组策略、WMI 永久订阅、AppInit_DLLs、浏览器扩展；不解析 `.url` 或 MSI 通告式快捷方式。

### 22.2 实现与隐私边界

- Run/RunOnce：HKLM 64/32 位视图 + HKCU 共享视图一次；值类型保留，只有 REG_SZ/REG_EXPAND_SZ 解析为命令，其他类型保留为部分未知。
- `.lnk`：`IShellLinkW` + `IPersistFile::Load(STGM_READ)`；只调用 `GetPath`/`GetArguments`/`GetWorkingDirectory`，参数和工作目录只保留非空布尔值，文本不进入结果；不调用 `Resolve`、不执行目标。
- 不返回完整原始命令/命令行或参数文本；UI 将 `C:\Users\<name>\` 脱敏为 `C:\Users\***\`。
- 启动文件夹不存在（`unavailable + exists=false`）与访问拒绝/错误（`exists=null`）分开；一个来源失败不清空其他来源。
- 结果只在内存，不新增 SQLite 表或写入；不修改注册表/启动文件夹/快捷方式；不运行目标程序或脚本。

### 22.3 自动化检查（实际执行）

- `cargo test -p sentinel-core`：**104 passed / 0 failed / 1 ignored**（lib 48，其中新增 9 项启动项/快捷方式测试；另含 21+21+9+5 项集成/契约测试）。
- `cargo test -p sentinel-lite`：**15 passed / 0 failed**（含 capability/命令名单一致性）。
- `cargo test --workspace`：**302 passed / 0 failed / 1 ignored**（`readonly_host_smoke` 默认忽略）。
- 前端：**109 passed / 0 failed**（14 个文件），新增 7 个 `StartupPanel` 测试，扩充 uiLabels 测试。
- `npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings` 通过。
- `npx tauri build --no-bundle` 通过。
- `.lnk` 成功路径由测试在临时目录创建并删除一个测试快捷方式验证；失败路径验证条目保留且不 panic。应用运行路径未创建、修改或执行真实启动项。

### 22.4 真实 Windows 验证（PASS）

- 当前 shell 非管理员（`admin=False`）；`cargo run -p sentinel-core --example startup_probe -- --output target/phase3-slice3/startup-probe-final4.json` 成功。
- 结果：**11 项**，`observation=observed`，`truncated=false`；8/8 来源 observed；11/11 条目 observed；0 error/denied；唯一 issue 为 `startupFolderMetadataSkipped`（两个启动文件夹仅有 desktop.ini）。读取耗时 **0–1 ms**。
- PowerShell 只读交叉检查：HKLM 64 位 Run 2 项、32 位 Run 1 项、HKCU Run 6 项、HKLM RunOnce 2 项、其余 0；两个启动文件夹除 desktop.ini/Thumbs.db 外均为 0。合计 11 项，与探针一致。
- 样例：`<browser auto-launch entry>` → `C:\Program Files\<vendor>\<browser>.exe`，REG_SZ，路径存在；`<security health UI>` → `C:\WINDOWS\system32\<system component>.exe`，REG_EXPAND_SZ；`<cloud sync client>` → 用户目录路径（UI 显示 `C:\Users\***\...`）。
- release 应用 `target/release/sentinel-lite.exe` 启动后，通过 UI Automation 打开“软件体检” → “启动项” → 点击“开始启动项检查”；真实 WebView2 显示 11 个启动项、8 个来源卡、`已读取`、值类型、目标路径和条目观察状态；点击第一行后详情显示 Raw Value/值类型/目标状态/启动审批未核验/静态线索。截图未随公开发布（含本机启动项清单，原始证据保留在本地）；另保留 `target/phase3-slice3/ui-startup-sources.png`、`ui-startup-detail.png`。
- UI 检查结果：`target/phase3-slice3/ui-result.json` 为 **PASS**；点击到“重新盘点”约 **245 ms**，应用 `Responding=true`；未出现 `恶意软件`/`已确认威胁`，无禁用/删除/运行/清理按钮；UI 中未出现原始 `C:\Users\<user>` 路径。
- 验证后应用进程已停止；未启动传感器、未创建 ETW session、未修改任何注册表 Run/RunOnce、启动文件夹或快捷方式，未执行任何目标程序。
- 最终 release 二进制品：`target/release/sentinel-lite.exe` SHA-256 `a7f136c34d7626f1d1b8e0c209645340a2ecb90883236d107327c14fdac03f34`（8,309,760 字节）。

### 22.5 数量/时间边界

- 每个注册表视图/启动文件夹最多 256 项；全部来源合计最多 512 项；值名称 512、值文本 2048、目标路径 1024 字符；8 秒。
- `.lnk` 目标路径读取缓冲 4096、结果上限 1024；参数/工作目录只读缓冲有界且文本不进入结果。
- 超限/超时 → `truncated=true` + DataIssue；来源/条目失败保持 partial/accessDenied/error；UI 分页 50 行。
- 命令在 `spawn_blocking` 中执行；单次注册表/COM 调用不可中断，极端卡死可能超过 8 秒返回，但不阻塞 WebView。

### 22.6 未验证/限制

- 本机两个启动文件夹没有真实 `.lnk`；真实启动文件夹中的快捷方式成功解析 **NOT RUN**。`.lnk` 成功/失败由自动化测试在临时目录覆盖，未在真实启动文件夹中执行。
- 未实机注入注册表/文件夹访问拒绝、COM 初始化失败、超大列表、8 秒超时或并发修改；这些路径由单元/集成测试覆盖。
- `.url`、MSI 通告式快捷方式、Winlogon/IFEO/组策略/WMI 订阅/AppInit_DLLs/浏览器扩展未覆盖。
- 未验证启动审批、签名、哈希或实际执行；启动项存在不代表启用或会运行。
- 不建立软件→启动项/服务/任务关联图；结果不持久化。

## 23. Phase 3 切片 4：软件与组件只读盘点（2026-11-01）

### 23.1 范围与数据来源

- 升级现有 `sentinel-core::software::scan_installed_software` 和“软件盘点”标签；不新增重复命令。
- 来源：HKLM 64/32 位 Uninstall 视图 + HKCU 共享 Uninstall 视图一次。
- 字段：DisplayName、DisplayVersion、Publisher、InstallLocation、InstallDate、DisplayIcon（解析后的静态路径线索）、WindowsInstaller、SystemComponent/NoRemove/ParentKeyName/ReleaseType，以及 UninstallString/QuietUninstallString 是否存在非空值。
- **不返回完整卸载命令**；不执行卸载器或 MSI 卸载接口；不修改注册表/软件。
- 不同视图/用户范围的同名登记记录不自动合并；UI 使用“可显示注册项”而不是唯一软件数。

### 23.2 实现与失败语义

- 新增 `SoftwareSourceStatus`、结果级 `observation`、条目级 `observation`/`issues`。
- 每个 Uninstall 视图独立读取；来源不存在（`unavailable + exists=false`）与访问拒绝/错误（`exists=null`）分开。
- 字符串/DWORD 字段保留 present/missing/typeMismatch/invalid/readError；空值显示“已读取（空值）”，不伪造默认值。
- InstallDate 只接受四位数字开头、后续数字/`-`/`/` 的形状；明显非日期文本标记 Invalid，不猜测日期。
- DisplayIcon 去掉常见 `,index` 后缀，仅保留有界静态路径；不访问目标。
- 卸载入口类型异常/读取失败时，风险规则使用 `uninstall_command_known=false` 返回证据不足（INV-005），不会误报“缺少卸载命令”（INV-001）。
- 无 DisplayName 的 31 个本机注册项跳过并计数；DisplayName 类型异常/读取失败标记来源 partial。
- 字段/名称/路径超限、8 秒超时 → `truncated=true` + DataIssue。
- 结果只在内存，不新增 SQLite 表或写入；安装路径/图标路径在 UI 中脱敏。

### 23.3 自动化检查（实际执行）

- `cargo test -p sentinel-core`：**112 passed / 0 failed / 1 ignored**（lib 55，其中新增 7 项软件测试；phase11 新增 1 项卸载入口未知规则测试；另含 21+22+9+5 项集成/契约测试）。
- `cargo test -p sentinel-lite`：**15 passed / 0 failed**。
- `cargo test --workspace`：**310 passed / 0 failed / 1 ignored**。
- 前端：**119 passed / 0 failed**（16 个文件），新增 7 个 `SoftwarePanel` 测试、3 个路径脱敏测试，并更新 CleanupPage 断言。
- `npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings` 通过。
- `npx tauri build --no-bundle` 通过。

### 23.4 真实 Windows 验证（PASS）

- 当前 shell 非管理员（`admin=False`，`<host>\<user>`）。
- 探针：`cargo run -p sentinel-core --example software_probe -- --output target/phase3-slice4/software-probe-final.json`。
- 结果：**184 个可显示注册项**，`observation=observed`，`truncated=false`；来源全部 observed；184/184 条目 observed；耗时 **14 ms**。
- 来源计数：HKLM 64 位 71、HKLM 32 位 109、HKCU 4；合计 184。
- PowerShell 只读交叉检查：原始键 215（HKLM64 86、HKLM32 125、HKCU 4），有 DisplayName 184（71/109/4），有非空卸载入口 169（56/109/4），与探针一致；31 个无 DisplayName 条目被跳过并计数。
- 样例：`<sample application>` 2.0.3.75 / <sample publisher> / 32 位 / `<sample install root>` / DisplayIcon `<sample install root>\<sample target>.exe` / 卸载入口存在；`<sample application 2>` 安装位置在用户目录（UI 显示 `C:\Users\***\...`）。
- 未返回 `uninstallCommand`/`quietUninstallCommand` 字段；探针 JSON 中只有存在性布尔值。
- release 应用启动后通过 UI Automation 打开“软件体检” → “软件盘点” → “开始只读盘点”；真实 WebView2 显示 184 个可显示注册项、三个来源卡、版本/发布者、安装位置、观察状态和详情；点击第一行后显示卸载入口“存在（内容未读取）”、DisplayIcon、字段状态和静态线索。截图未随公开发布（含本机软件清单，原始证据保留在本地）；另保留 `target/phase3-slice4/ui-software-sources.png`、`ui-software-detail.png`。
- UI 检查结果：`target/phase3-slice4/ui-result.json` 为 **PASS**；点击到“重新盘点”约 **245 ms**，`Responding=true`；无卸载/删除/清理/运行按钮；UI 树中未出现 `C:\Users\<user>`、`uninstall.exe` 或 `msiexec`。
- 验证后应用进程已停止；未启动传感器、未创建 ETW session、未修改任何注册表项或文件，未执行任何卸载命令或软件。
- 最终 release 二进制品：`target/release/sentinel-lite.exe` SHA-256 `f26243f0dc0e93c79f684544dc94e22b720c4c6dc24e2d9397630230ee76eea6`（8,332,288 字节）。

### 23.5 数量/时间边界

- 每个 Uninstall 视图最多 512 项；全部来源最多 1,024 项；总时间 8 秒。
- 文本字段最多 2,048 字符；DisplayIcon 结果路径最多 1,024 字符。
- 超限/超时 → `truncated=true` + DataIssue；UI 分页 50 行。
- 命令在 `spawn_blocking` 执行；单次注册表调用不可中断，极端卡死可能超过 8 秒返回，但不阻塞 WebView。

### 23.6 未验证/限制

- 未实机注入来源访问拒绝、字段类型异常、超大列表、8 秒超时或并发修改；这些路径由单元/集成/前端测试覆盖。
- 31 个无 DisplayName 注册项被跳过；其身份和用途未进一步解析。
- 不同视图/用户范围的同名登记记录不自动合并，可能显示多条注册项。
- DisplayIcon 仅静态线索，不验证目标存在、签名或图标可用性。
- 未覆盖 AppX/MSIX/Store/便携软件、服务、计划任务、启动项关联、签名/哈希或卸载残留。
- 不展示/执行完整卸载命令；不调用 MSI 卸载接口；不执行任何卸载。
- 未在 Windows 10、ARM64 或受限企业策略环境验证。

## 24. Phase 3 切片 5：只读系统资产关联（2026-11-01）

### 24.1 范围与实现

- 新增独立纯函数模块 `sentinel-core/src/association.rs` 和 `scan_asset_associations` 命令；复用现有软件、进程、服务、计划任务、启动项只读快照，不新增底层扫描器，不扩大前四个切片读取的字段。
- 匹配依据：软件安装位置/DisplayIcon 静态路径、进程可执行路径、服务二进制路径（上游已去参数）、计划任务动作可执行路径、启动项目标路径。
- 规则：规范化路径完全一致为 `exact`；目标位于非宽泛安装目录内为 `strong`；仅普通用户软件与运行进程的“同名不同目录”为 `weak`。宽泛根不参与归属；多软件候选保留并标记 `ambiguous`；路径不完整/来源不可读/截断时为 `insufficientEvidence` 或 `sourceUnavailable`。
- 关系记录：软件/目标 ID、关系类型、证据类型/值、匹配方法、确定性、观察状态、候选软件数和 issues。证据路径和软件路径线索脱敏用户目录；不读取/返回完整命令行或参数。
- UI：软件详情中的“资产关联”需用户点击后才读取；显示来源完整性、当前软件关系、确定性、匹配方式、证据路径和 issues；无处置按钮。

### 24.2 自动化检查（实际执行）

- `cargo test -p sentinel-core`：**131 passed / 0 failed / 1 ignored**（lib 74，其中新增 19 项路径/关联测试）。
- `cargo test -p sentinel-lite`：**15 passed / 0 failed**（含 capability/命令名单一致性）。
- `cargo test --workspace`：**329 passed / 0 failed / 1 ignored**。
- 前端：**126 passed / 0 failed**（16 个文件），新增 6 个关联 UI 测试、1 个标签测试，并更新测试 API mock。
- `npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings` 通过。
- `npx tauri build --no-bundle` 通过。
- 测试覆盖：路径大小写/斜杠/点段/设备前缀、路径边界、同文件名不同目录、服务/任务/启动项目标、缺失任务动作、无效路径、用户目录脱敏、多软件候选歧义、弱/强证据分级、来源不可用/部分读取、关系 ID/顺序稳定性、序列化结果无命令行字段，以及 UI 未读取/成功/弱线索/歧义/不足/失败清空/无处置按钮。

### 24.3 真实 Windows 验证（PASS）

- 当前 shell 非管理员；探针：`cargo run -p sentinel-core --example association_probe -- --output target/phase3-slice5/association-probe-final.json`。
- 结果：**62 条关系**，`observation=partial`，`truncated=false`，耗时 **455 ms**。
- 关联来源：软件 184（56 有可比较路径）、进程 269（112）、服务 763（318）、计划任务 192（11）、启动项 11（11）；所有来源各自有观察状态，底层截断均为 false。
- 关系按目标：服务 11、进程 43、启动项 5、计划任务 3；按证据：exact 15、strong 47、weak 0。
- 软件状态：13 个 `linked`、171 个 `insufficientEvidence`；0 个 `noMatch`、0 个 `ambiguous`。主要原因是服务/进程来源部分读取，系统没有把“未找到”伪造成完整结论。
- 样例：`<developer tool>` 的 4 条进程关系均位于其安装目录，如 `<helper>.exe` → `<install root>\bin\<helper>.exe`，`strong` / `containedInInstallLocation`；`<browser A>` 的进程/服务/启动项关系；`<browser B>` 的任务关系。宽泛根（如直接位于 Program Files 下的厂商根）不产生归属关系；扫描发现 1 个宽泛安装位置并报告。
- 两次连续探针的非进程关系 ID **19/19 完全一致**；对全部 62 条关系检查：证据路径等于软件证据路径或位于其下（弱线索除外）**PASS**；关系 JSON 中无 `C:\Users\<user>` **PASS**；`ambiguous=0`，未发现需要强行合并的候选。
- 本次真实机没有弱线索样本；弱/强证据、同文件名不同目录、路径边界和多候选由自动化测试覆盖。

### 24.4 Release UI 验证

- 使用当前 release `target/release/sentinel-lite.exe`，通过 UI Automation 打开“软件体检” → “软件盘点” → 过滤 `Git` → 打开详情 → 点击“读取资产关联”。
- 真实 WebView2 显示 5 个关联来源卡、当前软件 `linked` 状态、4 条运行进程关系、`安装目录内路径`、匹配方式、证据路径和软件路径线索。
- `target/phase3-slice5/ui-result.json` 为 **PASS**；关联点击到“重新读取关联”约 **710.4 ms**，`Responding=true`。
- UI 树中未出现 `C:\Users\<user>`、`uninstall.exe` 或 `msiexec`；无卸载/删除/清理/运行按钮；未出现“已确认威胁”或“恶意软件”。截图：`docs/screenshots/asset-associations.png`；原始证据：`target/phase3-slice5/ui-association-sources.png`、`ui-association-evidence.png`。
- 验证后未留下 `sentinel-lite`/`SentinelLiteSensor` 进程或 SentinelLite ETW session；审计见 `target/phase3-slice5/ui-tree-after.txt`、`association-probe-final.json`、`association-probe-repeat.json`。
- 最终 release 二进制品：`target/release/sentinel-lite.exe` SHA-256 `e7fe7acd8a9bfdfc7bff647b0f7268cad9c3dd1dad01d66621d237c7e70e74bf`（**8,454,144 字节**）。

### 24.5 数量/时间边界

- 最多 2,048 条关系、256 条弱线索、256 条/软件、32 个候选/目标、1,024 字符证据文本、256 条 issues；底层软件/启动项/服务/任务/进程来源沿用既有上限。
- 超限或底层截断 → `truncated=true` + DataIssue；UI 显示“结果不完整”。
- 关联命令串行复用五个只读来源；本机 455 ms，UI 点击到完成 710.4 ms；单次 Windows 调用不可中断，极端卡死可能让后台任务超时返回，但不阻塞 WebView。

### 24.6 未验证/限制

- 未实机注入来源访问拒绝、超大关系集、超时、并发修改、路径重定向、junction/符号链接、8.3 短名或环境变量路径；相关逻辑由自动化测试覆盖。
- 真实机没有弱线索样本，也没有同一目标多软件候选样本；这两条路径由自动化测试覆盖，不能据此宣称实际误关联率。
- 只使用静态路径证据；不验证文件存在性、签名、哈希、行为或实际归属。路径可过期、共享或重定向。
- 不读取/展示完整命令行、服务/任务参数、任务触发器或工作目录；不执行任何目标；不写 SQLite；不提供处置。
- 未覆盖 AppX/MSIX/Store/便携软件、实时/跨重启关联、持久化关联历史、服务依赖或任务参数。
- 未在 Windows 10、ARM64 或受限企业策略环境验证；未做关联结果的大规模误报/漏报评估。

## 25. Phase 3 切片 6：安全态势与 UI 整合（2026-11-01）

### 25.1 范围与实现

- 开发前基线：`fc523a7239ca715e05b81ac4f97501ff1290cae3`（`feat(inventory): add read-only asset associations`），工作区 clean。
- 在“软件体检”顶部增加安全态势概览；不新增扫描器、Tauri command、capability、SQLite 表或网络能力。
- 用户点击“读取安全态势”时并行调用现有 `get_native_protection_status` 与 `scan_asset_associations`；后者继续复用软件、进程、服务、计划任务和启动项只读快照。
- 应用壳层只创建一个原生防护 `ScanController`，首页卡片和软件体检概览共享；软件体检页只创建一个关联 `ScanController`，概览和软件详情共享。刷新失败清空旧结果；结果不自动刷新，界面明确标记为快照。
- 前端新增 `src/lib/posture.ts` / `src/lib/nativeProtection.ts` 纯函数和 `SecurityPosture` 组件；读取状态为 `notRead`/`complete`/`partial`/`unavailable`，没有 0–100 评分或整体“保护正常”。
- 证据分类：已观察事实、值得核实、证据不足、来源不可用；持久化区块为计划任务/启动项，进程与服务区块为进程/服务；弱线索/多候选单独交叉筛选。
- 修复 Slice 1 遗留 P2：`amServiceEnabled=false` 计入未启用开关；全启用脚注只在真正没有未启用/未知开关时显示；新增自动化测试。

### 25.2 自动化检查（实际执行）

- `cargo test -p sentinel-core`：**131 passed / 0 failed / 1 ignored**。
- `cargo test -p sentinel-lite`：**15 passed / 0 failed**。
- `cargo test --workspace`：**329 passed / 0 failed / 1 ignored**。
- 前端：**146 passed / 0 failed**（19 个文件）；新增 `nativeProtection`、`posture` 和 `SecurityPosture` 测试。
- `npm run typecheck`、`npm run lint`、`npm run build` 通过。
- `cargo fmt --all -- --check`、`cargo clippy -p sentinel-core -p sentinel-lite --all-targets -- -D warnings` 通过。
- `npx tauri build --no-bundle` 通过。
- 新增覆盖：enabled/disabled/unknown 分离、反恶意软件服务关闭、来源状态推导读取完整度、noMatch/insufficient/sourceUnavailable 分离、弱线索/多候选、持久化与进程/服务分组、跨页共享关联快照、组合刷新失败清空、无安全评分/处置按钮/原始用户路径。

### 25.3 真实只读探针

- 当前 shell 非管理员；`cargo run -q -p sentinel-core --example association_probe -- --output target/phase3-slice6/association-probe.json`。
- 结果：**62 条关系**，`observation=partial`，`truncated=false`，耗时 **434 ms**，8 个 issues。
- 来源：软件 184（56 有可比较路径）、进程 272（113）、服务 763（318）、计划任务 192（11）、启动项 11（11）；进程/服务来源为 `partial`，其余为 `observed`；无来源截断。
- 关系：exact **16**、strong **46**、weak **0**；按目标：进程 43、服务 11、启动项 5、计划任务 3。
- 软件状态：13 个 `linked`、171 个 `insufficientEvidence`、0 个 `noMatch`、0 个 `sourceUnavailable`、0 个 `ambiguous`。
- 关系 JSON 无 `C:\Users\<user>`；全部关系证据路径位于软件路径线索之下（弱线索除外）；没有弱线索或多候选真实样本。

### 25.4 Release UI 验证

- 使用 release `target/release/sentinel-lite.exe`，普通用户启动，通过 UI Automation 打开“软件体检” → 点击“读取安全态势”。
- `target/phase3-slice6/ui-result.json` 为 **PASS**；点击到“重新读取安全态势”约 **692.2 ms**，`Responding=true`。
- UI 显示 4 个概览指标、5 个关联来源、原生防护 6 个开关、四类线索、持久化入口、进程/服务、弱线索、软件关联状态和读取问题。
- 本次 UI 真实数据：**68 条关系**（49 进程、11 服务、8 计划任务/启动项）、13 个有线索软件、171 个证据不足、0 个未匹配；3 个来源已读取、2 个部分读取、0 个不可用；原生防护 6 个开关已启用、0 个未启用/未知。
- UI 树中未出现 `C:\Users\<user>`、`uninstall.exe`、`msiexec`、`保护正常` 或 `已确认威胁`；无卸载/删除/清理/运行/停止/禁用/隔离按钮；未出现安全评分数值。
- 截图：`docs/screenshots/security-posture.png`；原始证据：`target/phase3-slice6/ui-posture-top.png`、`ui-posture-triage.png`、`ui-posture-persistent.png`、`ui-posture-process-service.png`、`ui-posture-software.png`、`ui-tree-after.txt`。
- 验证后已关闭 release 应用；未启动传感器，未留下 `sentinel-lite`/`SentinelLiteSensor` 进程或 SentinelLite ETW session。

### 25.5 最终 release 二进制

- 路径：`target/release/sentinel-lite.exe`
- SHA-256：`5f2404792dde1eaa015a2b4af7babfcc5b1916b6f743e35738081c3b9b3d54d6`
- 大小：**8,459,776 字节**

### 25.6 未验证/限制

- 未实机注入原生防护或关联读取失败、来源访问拒绝、超大关系集、2,048 条上限、并发修改或 WebView 卡顿；失败清空和来源不可用由自动化测试覆盖。
- 真实机没有弱线索、多候选、`noMatch` 或 `sourceUnavailable` 样本；本次只验证 2 个来源 `partial` 和 171 个软件证据不足。
- 概览是快照视图，不是持续监控；系统变化后结果可能过期。没有实时关联、跨重启归属、关联历史或完整资产图。
- 不验证文件存在性、签名、哈希、行为或恶意性；不读取完整命令行/参数；不提供处置。
- 未在 Windows 10、ARM64、受限企业策略或多 DPI/多显示器自动化下验证。
- `npm audit` 当前完整依赖树为 11 个 moderate（`minimatch` 传递依赖，开发工具链）、0 high/0 critical；`npm audit --omit=dev` 为 0。自动修复建议会降级 eslint，未采用；记录为独立 P3 依赖维护项，不属于 Slice 6 功能改动。

## 26. Phase 3 UI 集中调整（2026-11-02）

### 26.1 变更范围

- 完整性问题：新增共享 `inventoryCompleteness` 分类和提示，区分读取/计数失败、容量/时间上限、字段截断、来源截断和未知原因；不再把所有 `truncated=true` 统一解释为“达到安全上限”。
- 安全态势：标题区合并读取状态、最后读取时间和范围说明；摘要由 4 项压缩为 3 项，并显示部分读取/不可用数量；持久化来源不可用时不再显示为“0 项”；保护/关联失败时仍显示可用的线索分类；移除重复截断提示。
- 隐私：Tauri 命令边界对服务账户/路径、任务名称/路径/作者/运行身份/动作路径、软件安装位置/DisplayIcon、启动项目标路径和风险证据文本做值级脱敏；内部扫描与 `scan_asset_associations` 仍使用原始路径。前端保留二次脱敏。
- 一致性/可访问性：软件表格读取失败/类型不匹配不再显示为“未提供”；服务/任务/软件/启动项增加加载态、空状态与筛选空状态区分；键盘可聚焦的横向表格区域；详情面板标签与 `aria-expanded`；分页区标签和页状态播报；任务动作预览前 3 项并折叠其余；完善焦点/对比度/标签间距。

### 26.2 自动化验证

- `cargo test --workspace`：**350 passed / 0 failed / 1 ignored**。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- `npm test -- --run`：**158 passed / 0 failed**（20 个文件）。
- `npm run typecheck`、`npm run lint`、`npm run build`：通过。
- `npx tauri build --no-bundle`：通过。

### 26.3 真实 Release UI 验证

- 二进制：`target/release/sentinel-lite.exe`
- SHA-256：`D5AF814D24B50297746D5C9B698B9ADD146D3FA47B32407CFC5A678A5262D3E5`
- 大小：**8,475,136 字节**
- 普通用户启动，隔离 `LOCALAPPDATA`；通过 UI Automation 依次读取安全态势、服务/计划任务、软件和启动项。
- `target/phase3-ui-polish/ui-result-final.json` 为 **PASS**；应用 `Responding=true`，任务枚举 **192 项**、`observed`、`truncated=false`，无 Count 失败 issue。
- 安全态势显示读取状态徽标、最后读取时间、3 项摘要、来源卡和线索分类；任务来源已读取，部分读取/不可用来源用文字和状态样式同时表达。
- UI 文本中没有 `C:\Users\<user>` 或裸账户 `<user>`；任务名/任务路径 `<task name>_<user>` 显示为 `<task name>_***`，任务作者/运行身份显示为 `***`，启动项/任务动作/软件路径显示为 `C:\Users\***\...`。关联结果的任务目标名称和 ID 也在命令边界脱敏。
- 截图证据：`target/phase3-ui-polish/posture-top-final.png`、`posture-sources-final.png`、`services-redacted-final.png`、`tasks-redacted-final.png`、`software-redacted-final.png`、`startup-redacted-final.png`；UI 树：`ui-tree-final.txt`。

### 26.4 未验证/限制

- 未实机注入 Tauri 命令序列化失败、超大表格、极长账户/路径、320px 窄屏键盘全流程或多 DPI 视觉回归；相关状态由自动化测试和 CSS 规则覆盖。
- 进程快照页面仍显示进程可执行路径，未纳入本轮服务/任务/软件/启动项的命令边界脱敏范围。
- 本地探针 `target/phase3-p1-1/*.json` 和部分 target 证据文件可能含原始路径，属于本机验证产物，不随应用分发；不应提交或外传。

## 27. 公开版 0.1.0 发布准备验证（2026-11-02）

### 27.1 本次自动化检查

- `cargo test --workspace`：**350 passed / 0 failed / 1 ignored**。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- `npm test -- --run`：**191 passed / 0 failed**（27 个文件）。
- `npm run typecheck`、`npm run lint`、`npm run build`：通过。
- `npm audit --audit-level=high`：通过；完整依赖树 11 个 moderate（开发工具链），生产依赖 0。
- `npm run licenses`：232 个 npm 包、403 个 Cargo 包；缺失许可证 0；需人工复核 31；自动阻止的强 copyleft/source-available 表达式 0。
- RustSec advisory-db 交叉检查（DB commit `263a4d951a0867d4bddc29dddd6d59427c78f9a7`，2026-11-02）：发现 3 项 DoS advisory——`quick-xml 0.38.4`（RUSTSEC-2026-0194/0195，经 `plist`/`tauri-utils`）和 `time 0.3.45`（RUSTSEC-2026-0009，经 `cookie`/`plist`）；未确认 SentinelLite 用户输入可达路径。另有 `glib 0.18.5` unsound（GTK/Linux 路径）和 `proc-macro-error 1.0.4` unmaintained（构建期）两条 informational。
- `cargo-audit` CLI 未能构建：其 `aws-lc-sys` 依赖需要 CMake/NASM，当前环境缺失且 crate 下载受阻。上述结果来自同一 RustSec advisory-db 的本地手工交叉检查，不冒充 `cargo-audit` 结果。
- 定向解析检查：`cargo update -p plist --precise 1.10.3` 可带出 `quick-xml 0.42.0` 和 `time 0.3.55`，但要求 Rust 1.88；本轮未升级，需单独做 Tauri 兼容性回归。

### 27.2 先前同源 Release 构建（已被 §28 取代）

以下为公开树隔离构建前的同源构建记录，保留用于对比；最终发布资产使用 §28 的隔离公开树构建。

- 构建来源：HEAD `34ca9bd2254d92047f304aeef1f847c2f2e2b45b` 加当前已审查的未提交源码修改。
- 最终应用二进制：`src-tauri/target/release-prep/build-target-nostart/release/sentinel-lite.exe`。
- 应用大小：**8,479,744 字节**；SHA-256：`2693fd800fbc6284f709cd1a7724f7e73136f569a13a67da51d9b91a704702ba`。
- 应用 FileVersion：`0.1.0`；ProductVersion：`0.1.0`；FileDescription：`SentinelLite`。
- NSIS 安装包：`src-tauri/target/release-prep/build-target-nostart/release/bundle/nsis/SentinelLite_0.1.0_x64-setup.exe`。
- 安装包大小：**2,535,602 字节**；SHA-256：`4c23428eaa263cbf6c0210ea5a1f4230f061a1188210b30ddccaf700612954b6`。
- 安装包 FileVersion：`0.1.0`；ProductVersion：`0.1.0`；FileDescription：`SentinelLite`。
- 安装模式：生成的 `installer.nsi` 为 `INSTALLMODE "currentUser"`、`RequestExecutionLevel user`，默认安装目录为 `%LOCALAPPDATA%\SentinelLite`。
- 命令与权限：16 个应用命令、16 个 `allow-*` 权限；生成的权限目录中没有 `start_sensor.toml`。
- 二进制字符串检查：app 与 installer 中未发现 `start_sensor`、`runas` 或 `allow-start-sensor` 字符串。app 二进制仍有 1 个 `ShellExecuteW` 导入符号，来自 `windows` crate 的 `Win32_UI_Shell` 特性（该特性同时用于只读 `IShellLinkW` 快捷方式解析）；源码中没有 `ShellExecuteW` 调用点，也没有 UAC/`runas` 启动路径。app 中的 `SentinelLiteSensor` 来自只读 IPC 客户端常量/管道名，不是启动器；installer 压缩层未发现该字面量，且未包含传感器二进制。
- 传感器二进制：`target/release/SentinelLiteSensor.exe`，**518,656 字节**，SHA-256 `2e3866100591225c5d8ff9b965b70d82d6d70e6bb39fb1af658a772ee193fa9e`；无 FileVersion/ProductVersion 资源；安装包字符串检查未发现 `SentinelLiteSensor` 引用，未随安装包打包。
- 安装包/应用二进制均未签名（`Get-AuthenticodeSignature` 返回 `NotSigned`）。

### 27.3 公开树与隐私处理

- 生成独立公开树，排除 `docs/ui-preview/`、`target/`、`dist/`、`node_modules/` 和本地验证产物。
- `docs/VALIDATION.md` 已替换账户名、主机名和工作目录；历史验证中保留少量通用软件/系统组件名称，但不包含个人软件清单；保留验证计数、PASS/FAIL 和限制。
- 两张含真实软件清单的旧截图未进入公开树。
- 公开截图仅使用已检查的当前 UI 预览图或已检查的系统/通用证据截图；4 张 README 预览图在 `docs/screenshots/README.md` 中标注为合成/空状态，8 张历史验证截图标注为真实测试机只读证据。
- 公开树最初为 274 个文件、约 4.3 MB；§29 第三方材料生成后增至 **1564 个文件、约 25 MB**；发布门禁又新增根目录 `RELEASE_NOTES.md`、`RELEASE_RISKS.md`、`SHA256SUMS.txt`，增至 **1567 个文件**；source-only 调整新增 `SOURCE_ONLY_LICENSE_NOTICE.md` 和 `third-party-source/mpl/lightningcss-win32-x64-msvc-1.33.0/NO_BINARY_INCLUDED.md`，最终为 **1569 个文件**；不含 `docs/ui-preview/`、`target/`、`dist/`、`node_modules/`、数据库或探针 JSON。新增内容包括 `THIRD_PARTY_NOTICES.md`、完整 `third-party-licenses/`、`third-party-source/mpl/`、`third-party-sbom/`、`docs/screenshots/README.md` 和 `scripts/build-release.ps1`。
- 公开树文本扫描未发现真实账户名、主机名、邮箱、私钥或访问令牌；历史验证文本保留了少量通用系统组件/常见软件名称，但不包含个人软件清单。
- 已逐张检查 12 张公开截图的内容和 PNG 元数据；未发现真实用户名、个人路径或本机软件清单。

### 27.4 发布前仍需处理

- 不得发布当前 Git 历史；历史中的 `docs/VALIDATION.md` 仍含账户名/主机名，两张旧截图仍含真实软件清单。
- 0.1.0 已移除应用内启动/UAC 路径；当前不存在该提权入口。本文更早的 UAC 辅助进程描述属于移除前的验证环境，保留为历史证据，不代表 0.1.0 应用能力。
- 安装包/二进制未签名；无自动更新、SBOM 或可复现构建证明。
- 安装包已从隔离公开树构建并确认不包含传感器二进制；最终构建已移除二进制内嵌的构建用户 profile 路径，最终公开提交生成后仍需复核源码一致性和发布资产哈希（见 §28）。
- `quick-xml`/`time` 的 3 项 RustSec DoS advisory 尚未修复；发布说明必须如实记录，不能声称“无已知漏洞”。

## 28. 隔离公开树构建验证（2026-11-02）

### 28.1 隔离方式

- 将 `target/release-prep/public-tree/` 复制到 `target/release-prep/isolated-public/`，不使用开发仓库的 `node_modules`、`dist` 或既有 Cargo 构建缓存。
- 在该副本执行 `npm ci`，重新安装 234 个 npm 包；另执行 `npm install` 确认依赖已是最新，`package-lock.json` 哈希未变化；前端构建与测试均使用副本内的 `node_modules`。
- Cargo/Tauri 使用独立 `CARGO_TARGET_DIR=target/release-prep/isolated-public-target`（最终重映射构建使用 `isolated-public-target-remap`），未复用开发仓库的 `target/release`。
- 隔离副本未包含 `target/`、`node_modules/`、`dist/`、`src-tauri/gen/` 或 `docs/ui-preview/`。

### 28.2 源码一致性

- 公开树与工作树共有 259 个同名文件；其中 245 个（全部源码、Cargo/npm 锁定文件、配置、测试、CI、脚本、图标）逐字节一致。
- 14 个差异文件均为公开文档/元数据：`README.md`、`SECURITY.md`、`PRIVACY.md`、`CONTRIBUTING.md`、`.gitignore`、`docs/ARCHITECTURE.md`、`docs/THREAT_MODEL.md`、`docs/PRODUCT_SCOPE.md`、`docs/KNOWN_LIMITATIONS.md`、`docs/VALIDATION.md`、`docs/FEEDBACK.md`、`docs/ROADMAP.md`、`docs/THIRD_PARTY.md`、`docs/adr/0006-dependency-vulnerability-scanning.md`（公开版本为英文/脱敏/补充发布说明）。没有源码、capability、权限、锁定文件或测试文件的差异。
- 工作树独有文件仅为两张含真实软件清单的旧截图，未进入公开树；§28.2 比较时公开树额外 15 个发布材料；§29 又生成完整许可证 bundle、MPL 源码和 SBOM，文件总数增至 1564；发布门禁新增 3 个根目录发布文档后为 1567；source-only 调整新增 2 个说明文件后为 1569。所有额外文件均为许可证/发布材料，不是运行时代码差异。

### 28.3 隔离构建结果

- `npm run typecheck`：通过。
- `npm run lint`：通过。
- `npm test -- --run`：**191 passed / 0 failed**（27 个文件）。
- `npm run build`：通过。
- `cargo test --workspace --locked`：**350 passed / 0 failed / 1 ignored**。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：通过。
- `npx tauri build`：通过。首次隔离构建耗时 **2m 03s**；发现二进制内嵌本机 Cargo 路径中的真实用户名后，使用 `--remap-path-prefix` 重新构建，最终构建耗时 **1m 53s**，NSIS 安装包生成成功。
- 公开树新增 `scripts/build-release.ps1`：运行时读取当前用户 profile 路径并设置 `CARGO_ENCODED_RUSTFLAGS`，将 `C:\Users\<build-user>` 重映射为 `C:\Users\redacted`；最终构建使用等价命令完成，脚本另以 `--no-bundle` 实测通过，产物中真实用户名计数为 0。

### 28.4 隔离构建产物

| 产物 | 大小 | SHA-256 | FileVersion / ProductVersion | 签名 |
| --- | --- | --- | --- | --- |
| `sentinel-lite.exe` | **8,480,256 字节** | **`0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be`** | `0.1.0` / `0.1.0` | `NotSigned` |
| `SentinelLite_0.1.0_x64-setup.exe` | **2,535,872 字节** | **`2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df`** | `0.1.0` / `0.1.0` | `NotSigned` |

- 安装包仍为 `INSTALLMODE "currentUser"`、`RequestExecutionLevel user`，默认目录 `%LOCALAPPDATA%\SentinelLite`。
- 生成的 NSIS 脚本在本次构建中只打包主程序 `sentinel-lite.exe`；WebView2 bootstrapper 定义为空。没有 `SentinelLiteSensor.exe` 或传感器资源。
- app/installer 的 `start_sensor`、`runas`、`allow-start-sensor` 字符串计数为 0。app 中的 `ShellExecuteW` 为 `windows` crate 的导入符号（用于只读 ShellLink 支持），源码无调用点；app 中的 `SentinelLiteSensor` 为只读 IPC 常量/管道名，不是启动器。installer 压缩层未发现这些字面量。
- **隐私修复：** 首次隔离构建的二进制在 panic location 字符串中内嵌了 `C:\Users\<real-user>\.cargo\registry\...`（真实用户名）。最终构建使用 `RUSTFLAGS=--remap-path-prefix=C:\Users\<real-user>=C:\Users\redacted`，重建后 app 中真实用户名计数为 **0**，仅保留 `C:\Users\redacted`（180 处）；主机名、测试账户名和 `WpsUpdate` 等夹具名计数为 0。发布资产只使用本节的重映射构建；§27.2 的先前构建和本轮未重映射构建均不发布。
- 隔离构建哈希与 §27.2 的先前同源构建哈希不同，原因是 Rust/Tauri 构建目录和时间戳不同，构建不是逐字节可复现的；§27.2 保留为先前构建记录，**最终发布资产使用本节哈希**。

### 28.5 未完成项

- 未完成正式法律审查；许可证材料是工程整理，不是法律意见。
- 39 个包在已安装源码根目录没有独立许可证文件；已记录表达式和来源，仍需人工/上游复核。二进制图内只剩 `selectors`；`alloc-stdlib`、`webview2-com`、`webview2-com-sys` 已补入精确上游许可证文件。
- `lightningcss` 已包含 npm 包快照、npm `gitHead` 精确源码归档和 GitHub `v1.33.0` 标签快照；标签只比 npm 源码多一个 `rust-toolchain.toml` 提交。根 `Cargo.toml` 的 `1.0.0-alpha.72` 是内部 Rust crate 版本，不是 npm wrapper 版本。`lightningcss-win32-x64-msvc` 只发布原生二进制、无源码；MPL Section 3.2 对构建期包的适用范围仍需法律确认。
- SBOM 已做 JSON 解析、引用闭包检查和官方 CycloneDX 1.5 schema 校验（0 错误）。
- 未在 Windows 10、ARM64、企业策略环境或干净 VM 上重复隔离构建或安装。

## 29. 第三方发布材料生成（2026-11-02）

- 以 `Cargo.lock`、`package-lock.json`、`cargo metadata`、`cargo tree`、`npm query` 和已安装源码为基础生成：
  - `third-party-licenses/MANIFEST.csv`：635 行（403 Cargo + 232 npm）。
  - `third-party-licenses/packages/`：876 个原始许可证/版权文件 + 19 个补充上游证据/许可证文件，共 895 个。
  - `third-party-licenses/texts/`：361 个去重许可证/NOTICE 文本。
  - `third-party-licenses/COPYRIGHT_INDEX.md`：逐包版权索引；`COPYRIGHT_REVIEW.md`：143 个初始缺失版权行的逐类分析。
  - `third-party-licenses/MISSING_LICENSE_FILES.md`：39 个无独立许可证文件的包；二进制图内只剩 `selectors`。
  - `third-party-licenses/NOTICE_SCAN.md`：635 个包根目录中未发现独立 `NOTICE*` 文件（快照结果）；另有 1 个内嵌原生 NOTICE。
  - `third-party-licenses/EMBEDDED_NATIVE_COMPONENTS.md`：Microsoft WebView2Loader 1.0.3800.47 的 license/NOTICE/来源。
  - `third-party-source/mpl/`：5 个 Cargo MPL `.crate` 源码包 + `lightningcss` npm 包快照 + npm `gitHead` 精确源码归档 + GitHub `v1.33.0` 标签快照 + 原生包 license/metadata；`LIGHTNINGCSS_PROVENANCE.md` 和 SHA-256 清单。
  - `third-party-sbom/SBOM.source.cdx.json`：642 个组件（403 Cargo + 232 npm + 7 一方 crate）。
  - `third-party-sbom/SBOM.binary.cdx.json`：201 个组件（196 Cargo runtime 上界 + 4 npm production candidate + 1 个内嵌原生加载器）；仅保留为历史构建记录，source-only 发布不包含二进制。
- SBOM 为 CycloneDX 1.5 JSON；JSON 解析、`dependsOn` 引用闭包和官方 schema 校验均通过（0 错误）；见 `third-party-sbom/VALIDATION.md`。
- 未修改运行时代码、Cargo/npm 锁定版本、IPC、权限或安全边界；未重新生成或覆盖最终 Release 资产。
- 所有材料仍不是法律审查结论；`selectors` 的独立许可证缺口、MPL Section 3.2 对构建期/原生包的适用范围和 `lightningcss` 原生绑定源码可复现性仍是发布前需人工确认的事项。


## 30. Source-only public release verification（2026-11-04）

### 30.1 发布范围

- 本次 0.1.0 公开树只包含源码、配置、文档、许可证材料和 MPL 源码归档。
- 不包含 Windows 安装包、`sentinel-lite.exe`、`SentinelLiteSensor.exe`、`.node`、DLL/PDB/OBJ/LIB/MSI 或其他可执行构建产物。
- 不包含 `target/`、`node_modules/`、`dist/`、数据库、日志或本地测试数据。
- 不继承原始开发仓库的 `.git` 历史。
- 前半文中出现的二进制哈希、安装器和 binary SBOM 均为历史构建证据，不是本次 Release 的资产。

### 30.2 许可证门禁

- 本次为 Source Code Form 分发，不发布 Executable Form；MPL-2.0 Section 3.2 的可执行形式源码提供机制不由本次仓库触发。
- Section 3.1 的源码分发通知见 `SOURCE_ONLY_LICENSE_NOTICE.md`；MPL-2.0 文本见 `third-party-licenses/MPL-2.0.txt`。
- 5 个 MPL Cargo `.crate`、npm `lightningcss` 包快照、npm `gitHead` 精确源码归档和 GitHub `v1.33.0` 标签快照均保留。
- `selectors@0.38.0` 的 16 个 Rust 源文件均带 MPL-2.0 文件头；精确源码归档已包含，但没有独立上游 `LICENSE` 文件。该缺口和 39/134 许可证/版权缺口继续记录，不伪造文本。
- 该门禁为工程判断，不是法律审查或法律合规证明。

### 30.3 文件与隐私检查

- 公开树文件总数以 `../public-tree-files.txt` 和根目录 `SHA256SUMS.txt` 为准。
- 未发现 `.exe`、`.dll`、`.pdb`、`.obj`、`.lib`、`.node`、`.msi`、静态库、数据库、日志或 `target/` 产物。
- 未发现 `.git` 目录；未继承原始 Git 历史。
- 未发现真实用户名、主机名或开发机路径。
- Markdown 相对链接检查通过。
- `SHA256SUMS.txt` 按 Git LF 归一化内容生成；验证时应使用 `core.autocrlf=false` 的干净 clone 或 `git archive` 解包，而不是直接对 CRLF 工作树运行。
- `Cargo.lock`、`package-lock.json`、运行时源码、IPC、权限和安全边界未修改。

### 30.4 Source SBOM 与许可证材料

- `third-party-sbom/SBOM.source.cdx.json`：642 个组件、643 个依赖；官方 CycloneDX 1.5 schema 校验 0 错误。
- Source SBOM 是锁定依赖图清单，不是仓库逐文件清单；仓库只包含选定的 MPL 源码归档，而不是所有锁定包的源码。
- `third-party-sbom/SBOM.binary.cdx.json` 仅保留为历史构建组成记录，不是本次 release 的资产清单；其 201 个组件和 198 个依赖记录仍通过 schema 校验。
- 许可证 bundle、MPL 源码和 SBOM 的 `SHA256SUMS` 已按最终文件重新生成并验证。

### 30.5 未完成项

- 未进行正式法律审查；`selectors` 独立许可证文件缺失和 39/134 许可证/版权缺口仍需人工/法务确认。
- 未在 Windows 10、ARM64、企业策略环境或干净 VM 上重复构建或安装。
- 本次不提供任何二进制；未来若发布二进制，必须重新执行 MPL Section 3.2、原生 binding 和源码要约审查。
