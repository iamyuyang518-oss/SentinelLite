# ADR 0001：Tauri 2 + Rust + React/TypeScript

- 状态：接受
- 日期：2026-10-29

## 背景

目标是一个 Windows 10/11 本地桌面工具，需要读取注册表、进程、签名等原生信息，并以可解释的界面呈现。第一版不能依赖内核驱动或云服务，安装体积和常驻资源应保持轻量。

## 决策

- 原生后端使用 Rust。
- 桌面壳使用 Tauri 2。
- UI 使用 React 19 + TypeScript + Vite。
- 原生扫描逻辑放在独立的 `sentinel-core` crate，Tauri 仅做命令和窗口集成。
- 使用 WebView2（Windows 11 已内置/更新安装）。

## 后果

优点：

- 前端不会直接访问系统 API；IPC 命令可维持窄白名单。
- Rust 适合处理原生格式、错误和后续低开销事件采集。
- Tauri 不需要捆绑 Chromium，初始体积比 Electron 小。
- `sentinel-core` 可独立测试规则和解析器。

代价：

- 需要 Rust/MSVC/Windows SDK 工具链。
- WebView2 版本和 CSP 行为需要在 Windows 上实际验证。
- Windows 应用签名、安装包和更新仍需单独工程。

## 未选择的方案

- Electron + Node 原生模块：安装与常驻体积更大，原生访问边界更宽。
- C# WinUI：可行，但当前团队/项目选择以 Rust 原生后端为核心。
- 网页 + 本地服务：增加本地端口、认证和远程面，不适合第一版权限模型。
