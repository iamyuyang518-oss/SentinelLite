# 贡献指南

感谢你帮助 SentinelLite 变得更可靠。这个项目处理的是用户的软件、进程与系统状态，因此“证据优先”和“不造成破坏”高于功能数量。

## 绝对约束

- 不要在开发或测试中关闭 Defender、防火墙、VBS、UAC 或其他安全功能。
- 不要提交自动卸载、批量删文件、批量删注册表、无差别杀进程、关闭安全功能的“便捷实现”。
- 不要在普通开发主机上运行真实恶意软件或破坏性测试。
- 不要提交真实用户数据、浏览器历史、凭据、未脱敏日志或他人的软件清单。
- 不要将“未实现”包装成“已保护”，不要伪造测试结果、性能数据或检测覆盖率。
- 未经维护者事先同意，不要在 CI 或应用中引入远程遥测、上传、云服务或内核驱动。

## 开发流程

1. Windows 10/11 + Node.js 22.13+ + rustup stable MSVC + VS 2022 C++ Build Tools。
2. `npm ci`
3. `npm run tauri dev`
4. 为新行为先写失败测试，再实现最小变更。
5. 运行全部检查：

```powershell
npm run lint
npm run typecheck
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

只读主机冒烟测试需显式运行：

```powershell
cargo test -p sentinel-core --test readonly_host_smoke -- --ignored --nocapture
```

## 代码边界

- `crates/sentinel-core`：不依赖 Tauri 的只读扫描、规则、类型和证据生成。
- `src-tauri`：Tauri 窗口、固定命令白名单、异步任务包装与错误序列化。
- `src`：展示证据、状态与限制；不得向后端传递任意路径、脚本、命令或可执行代码。
- 新命令必须使用严格的参数类型和白名单能力；不要让前端控制卸载器、Shell 或文件路径。
- 新增/修改 Tauri 命令时，必须同步更新 `src-tauri/src/command_names.rs`、`generate_handler!` 与 `capabilities/default.json` 的应用级 `allow-*` 权限。不要通过重新加入 `core:default` 来绕过 ACL。
- 任何新的外部输入都必须有明确条目/字符串/大小上限、截断状态和 UI 分页；不得静默截断。
- 进程相关检查必须使用创建时间身份令牌等稳定身份原语，不得只靠 PID 拼接不同来源的数据。
- 高风险动作即使未来获准实现，也必须经过独立设计、预览、明确确认、审计和回滚流程。

## 测试要求

- Rust 规则与解析器使用虚构输入进行单元测试，覆盖正常、异常、误报和边界案例。
- React 测试使用虚构数据，不得依赖宿主真实软件列表。
- 默认测试中不得执行真实卸载、结束真实进程或写系统设置。
- 对可能误报的规则，说明正常案例和置信度，不得仅凭品牌、文件名或“未签名”定罪。
- UI 状态测试必须证明未知、证据不足、AccessDenied 和截断不会被显示成“保护正常”或完整结果。
- 风险测试必须证明局部亲和性正常不会变成整体进程 Clean/Safe，系统组件不会被按普通用户软件规则误报。

## 依赖与许可证

- 新增依赖前说明用途、替代方案、维护状态、许可证和安全影响。
- 直接依赖使用精确版本；锁文件必须提交。
- 不接受来源不明的预编译二进制、驱动或遥测 SDK。
- 运行 `npm run licenses` 检查许可证清单；发布前人工复核不常见或受限许可证。
- 在本地/CI 运行 `npm audit --audit-level=high`；开发工具链仍有 moderate 项时记录，不盲目使用 `npm audit fix --force`。Cargo advisory 可使用 `cargo audit`（需单独安装）或在 CI 中增加；当前审计结果见 [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md)。Dependabot 在仓库托管后运行。

## 提交与 PR

- 保持提交小而可审查，说明行为变化、测试证据和已知限制。
- PR 不应包含自动格式化产生的无关大范围改动。
- 任何涉及写操作、提权、驱动或网络的功能，必须在 PR 中单独披露，并默认不符合第一阶段范围。
- 贡献代码默认按 MIT 许可证提交，除非文件另有说明。

## 误报反馈

误报修复与新增规则同等重要。提交前请先阅读 [误报与漏报反馈](docs/FEEDBACK.md)，提供规则 ID、判断依据和脱敏后的最小信息；不要上传真实恶意样本或敏感数据。

## 安全报告

不要通过公开 PR 或 Issue 披露漏洞细节，请遵循 [SECURITY.md](SECURITY.md)。
