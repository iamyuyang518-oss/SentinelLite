# ADR 0011：只读传感器状态 IPC

- 状态：接受，Phase 2.2 最小切片已实现；提交前审查 P1-1 的 Medium mandatory label 修复已由普通 Medium 客户端复验 PASS（`is_admin=False`、Medium Mandatory Level、probe exit 0）；P2-A/P2-B 已修复；最终二进制 `ae4314fe...` 尚未重新执行 elevated/Medium 实机复验（2026-10-31）
- 日期：2026-10-31
- 范围：`crates/sentinel-ipc`、`SentinelLiteSensor --serve`、Tauri `get_sensor_status` 命令
- 关系：扩展 [ADR 0010](0010-sensor-architecture.md) 的前台传感器；不改变 Phase 2.1 的 ETW/解码/配对语义

## 1. 背景

Phase 2.1 的 `SentinelLiteSensor.exe` 是定时前台工具：启动、采集、停止并输出 JSON。主应用无法查询它是否正在运行、当前健康如何或累计事件计数。Phase 2.2 只解决一条最小链路：

```text
SentinelLite 主程序 → 只读 Named Pipe → SentinelLiteSensor.exe
```

第一版只允许状态查询，不允许停止、重配、读取原始事件或执行任意命令。主应用仍然是普通用户权限；传感器仍由管理员显式启动，不自动提权、不自动拉起。

## 2. 决策

### 2.1 传输

- 使用 Windows Named Pipe，不引入 TCP、共享内存或第二套 IPC。
- 固定端点：`\\.\pipe\SentinelLiteSensor.Status.v1`。
- 服务端由 `SentinelLiteSensor --serve` 在 ETW session 成功打开后创建。
- 管道为 message 模式、双工、单实例、`PIPE_REJECT_REMOTE_CLIENTS`、`FILE_FLAG_FIRST_PIPE_INSTANCE`。
- 管道 DACL 只授予当前用户 `GRGW`，以及 SYSTEM/Administrators `GA`；不授予 `Everyone` 或 `Authenticated Users`。
- 管道显式设置 Medium mandatory label：`S:(ML;;NW;;;S-1-16-8192)`。提权传感器（High/System）创建对象时默认标签会使 Medium 主应用在 MIC 检查处被 `NO_WRITE_UP` 拒绝；显式 Medium 标签允许 Medium 客户端读写，同时仍拒绝 Low 完整性客户端。
- 创建带 SACL 的管道需要 `SeSecurityPrivilege`；服务端在创建管道期间临时启用该特权，创建/失败后恢复原状态；恢复失败则关闭管道并返回错误。
- `--serve` 只保持前台运行；不安装服务、不写启动项、不创建计划任务。

### 2.2 协议

- JSON、单请求/单响应、版本 `1`。
- 请求只允许 `{"version":1,"request":"status"}`；未知字段、未知版本、非法 UTF-8、超长消息全部拒绝。
- 请求上限 512 字节，响应上限 16 KiB；错误消息最多 1024 字符。
- 响应包含：`sensor_state`、`permission_state`、`events_received`、`events_processed`、`events_dropped`、`events_lost`、`realtime_buffers_lost`、`decode_errors`、`last_event_at`、`uptime_ms`、`health_ok`、`last_error`。
- `health_ok` 是 **运行中** 的 fail-closed 判定：状态 Active、至少收到/处理事件、无 dropped/decode/partial、实时丢失计数已知为零、无 start 覆盖问题、无配对完整性异常。它不是 Phase 2.1 完成窗口的 `ok`。
- `coverage` 不通过 IPC 暴露；主程序只能看到健康与计数，不能据此声明“保护正常”。

### 2.3 身份与访问控制

- 服务端在每次连接后调用 `ImpersonateNamedPipeClient`，读取客户端 token 用户 SID，并与传感器进程用户 SID 比较；不匹配返回 `unauthorized` 并断开。
- 服务端始终在读取请求前 `RevertToSelf`；`RevertToSelf` 失败视为致命错误，IPC 服务线程退出，不继续以客户端身份运行。
- 客户端使用 `SECURITY_IDENTIFICATION`，不授予管道服务端完整 impersonation。
- 客户端在发送请求前校验管道服务端进程：镜像 basename 必须是 `SentinelLiteSensor.exe`，且 token 必须处于 elevated 状态。这是对固定管道名被非管理员进程抢占的缓解；管理员级伪造不在本切片防御范围。
- 客户端只发送固定 status 请求；服务端没有命令分派、路径、PID、URL 或脚本参数。

### 2.4 生命周期与失败语义

- 传感器定时模式保持不变；`--serve` 忽略 `--duration`，保持运行直到 Ctrl-C。
- `--serve` 在 ETW trace 打开后启动 IPC 服务；IPC 创建失败只降级传感器健康，不把采集伪装成正常，也不让主程序看到健康状态。
- 主应用通过 `get_sensor_status` 返回五种链路状态：`running`、`degraded`、`stopped`、`unavailable`、`communicationFailed`（实现中以字符串区分）。
- 管道不存在 → `stopped`；访问拒绝/服务端身份不符 → `unavailable`；超时/格式错误/远端内部错误 → `communicationFailed`；成功但 `health_ok=false` 或状态非 Active → `degraded`。
- 通信失败绝不映射为 `health_ok=true` 或保护正常。

## 3. 明确不做

- 不通过 IPC 发送 stop/pause/config 命令。
- 不通过 IPC 传输原始进程事件、命令行、文件路径或 UI 数据。
- 不自动启动/停止管理员传感器；不引入 UAC manifest、服务、安装器或新提权 helper。
- 不接入 SQLite、Tauri UI 实时面板、行为检测、处置或告警。
- 不引入远程管道、TCP 或共享内存。

## 4. 已知限制

- 固定管道名允许同用户进程抢占/拒绝服务；传感器会 fail closed（IPC 不可用），主程序不会误报健康，但需要管理员清理抢占进程或重启传感器。
- `TerminateProcess` 硬杀会关闭状态管道，但可能留下 ETW session；主程序只能看到 `stopped`/`unavailable`，不能据此证明没有 ETW 残留，需管理员用 `logman query -ets` 检查。
- 同用户进程可以连接并查询非敏感状态；服务端只做同用户校验，不做应用签名/进程路径白名单。管理员级恶意进程可伪造服务端，超出本切片威胁模型。
- 传感器需要管理员权限；主程序是普通用户，因此不自动启动传感器。修复前二进制 `fff41bc9...` 的 elevated E2E 已 PASS；P1 修复版 `4505268a...` 已由非提权 Medium probe 复验 PASS（`health_ok=true`、6 事件、probe exit 0）；P2-A/B 后的最终二进制 `ae4314fe...` 尚未重新实机复验。
- `OpenProcessToken`/elevation 校验在某些受保护进程上可能不可用；客户端会 fail closed 为 `unavailable`，不会降级为健康。
- 未做长时间稳定性、并发连接压力、跨用户 DACL 拒绝。
- Phase 2.3 已把 `get_sensor_status` 接入首页（见第 6 节）；第 3 节的“不接入 UI 实时面板”仍适用于本切片：没有事件流、告警或实时事件列表，只展示状态/健康/计数。整体保护状态仍为未知。

## 5. 验收

- `sentinel-ipc` 协议、超长/非法消息、超时、缺失管道、匿名客户端、非传感器服务端、重复管道实例、瞬时连接错误恢复均有自动化测试。
- 生产 SDDL 含 Medium mandatory label，且该 SDDL 能被解析出 SACL；测试服务端因非提权使用 DACL-only 测试配置。
- `cargo test --workspace`、`cargo fmt --check`、`cargo clippy -D warnings` 通过。
- 修复前二进制 `fff41bc9...` 的 elevated E2E 已 PASS（probe exit 0、sensor exit 3、`ipc_started=true`、无残留）；P1 修复版 `4505268a...` 的 Medium 客户端复验 PASS（`is_admin=False`、Medium Mandatory Level、probe exit 0、`health_ok=true`）；P2-A/B 后的 `ae4314fe...` 已在 Phase 2.3 由非提权 release 应用连接提权传感器完成 Active+Healthy、Active+Unhealthy 和停止过渡验证，无残留 session。

## 6. Phase 2.3 首页集成

- 首页在挂载时调用 `get_sensor_status`，之后每 4 秒轮询；离开首页即卸载并停止轮询。
- 展示链路状态、传感器运行状态、独立健康状态、`events_received`、`events_processed`、`events_lost`、`events_dropped`、`decode_errors`、`last_event_at`、`uptime_ms` 和 `permission_state`。
- 查询失败或链路不可用时显示不可用/健康未知，清空旧计数；不会把 Active 当作 Healthy，也不会把传感器 Healthy 当作整体保护正常。
- 不自动启动/停止传感器，不增加 stop/config/事件流命令；传感器仍需管理员显式运行。
- 自动化测试覆盖映射、轮询、请求不重叠、卸载停止、失败清空、旧响应不覆盖；真实 Windows 截图覆盖 Active+Healthy、Active+Unhealthy 和停止后不可用。Tauri 命令调用被拒绝的场景仅由自动化测试覆盖。
