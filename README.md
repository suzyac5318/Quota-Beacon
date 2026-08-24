# Quota Beacon

[![CI](https://github.com/suzyac5318/Quota-beacon/actions/workflows/ci.yml/badge.svg)](https://github.com/suzyac5318/Quota-beacon/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/suzyac5318/Quota-beacon?display_name=tag)](https://github.com/suzyac5318/Quota-beacon/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Quota Beacon Windows 是一款独立维护、local-first 的 Tauri 桌面悬浮工具。它读取 Windows 本机已有的 Codex Desktop 登录状态，以悬浮卡片展示真实额度、重置时间、本机 Token 汇总和可定制的额度主题。

> 当前 Windows 版本：`1.10.3`
>
> Quota Beacon 是非官方社区项目，与 OpenAI 不存在隶属、认可或赞助关系。

![Quota Beacon 产品主视觉](docs/images/quota-beacon-overview.png)

![Quota Beacon 额度配色系统](docs/images/quota-beacon-color-system.png)

## 主要功能

- 展示 5 小时额度、周额度与下一次重置时间；不根据本地数据猜测额度。
- 空闲时折叠为悬浮球，悬停后展开；支持拖动、置顶、托盘控制和锁定。
- 每 10 秒刷新真实额度；异常时显示不可用或旧数据状态，而不是伪造数值。
- 提供 101 级连续颜色主题、色彩预览、调色盘编辑和键盘交互。
- 汇总本机 Codex 会话 Token 使用量，并在主卡片中展示当前窗口使用量。
- 不包含遥测、分析或第三方追踪。

## 平台状态

| 平台 | 发布包 | 验证状态 |
| --- | --- | --- |
| Windows | unsigned ZIP/安装包 | Windows 11 实机验证 |

本分支只维护 Windows。macOS 使用独立的 `macos` 分支、`macos-v*` 标签、macOS CI 和 macOS Release，不从本分支构建或发布。

## 下载与安装

先在同一台 Windows 电脑登录 Codex Desktop，再前往 [GitHub Releases](https://github.com/suzyac5318/Quota-beacon/releases) 下载标题含 `Windows` 的最新版本。

### Windows 11

1. 下载 `quota-beacon-windows-unsigned.zip` 并解压。
2. 运行 ZIP 内的 Windows 安装包。
3. 当前构建未签名，Windows 可能显示“未知发布者”或 SmartScreen 提示；请确认下载来源是本仓库 Release 后再继续。

更多边界和已知限制见 [发布说明](docs/RELEASE.md) 与 [已知限制](docs/KNOWN-LIMITATIONS.md)。

## 工作方式与隐私边界

Quota Beacon 只使用本机既有的 Codex Desktop 登录状态，向额度服务发起只读查询。它不会兑换重置机会、修改账户设置，也不会持久化 token、账号 ID、提示词、聊天内容、原始额度响应或本机认证路径。

浏览器预览使用 mock 数据；真实额度读取只能在已登录 Codex Desktop 的 Tauri 桌面环境验证。详细说明见 [PRIVACY.md](PRIVACY.md) 与 [SECURITY.md](SECURITY.md)。发现安全问题时，请使用仓库的[私密安全报告](https://github.com/suzyac5318/Quota-beacon/security/advisories/new)，不要在公开 Issue 中提交敏感信息。

## 开发

前置条件：Windows、Node.js 20+、Rust stable，以及 Windows 所需的 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
npm install
npm test
npm run build
npm run tauri dev
```

浏览器开发模式只使用 mock 数据。桌面构建使用：

```bash
npm run tauri -- build
```

贡献代码前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。不要提交本机凭据、`.codex`、`.env*`、个人截图、`node_modules`、构建输出或安装包。

## 发布

GitHub Actions 会在 push/PR 时执行前端与 Windows 桌面检查。只有 `Windows` 分支上的 `windows-v*` 标签可生成 Windows 工件；版本提交使用 `windows-v<版本>: <说明>`。Release 标题固定为 `Quota Beacon Windows v<版本>`，且只能包含 Windows ZIP 与 SHA-256。维护流程见 [GitHub 发布清单](docs/GITHUB-RELEASE-CHECKLIST.md)。

## 衍生项目、许可证与商标

Quota Beacon 基于上游开源项目衍生并独立维护。上游来源、贡献边界与许可要求见 [UPSTREAM-NOTICE.md](UPSTREAM-NOTICE.md)，代码按 [MIT License](LICENSE) 分发。

OpenAI、ChatGPT 与 Codex 是其各自权利人的商标或产品名称。本项目仅为说明兼容性而引用这些名称和 Codex 标识，不代表官方认可或合作关系。
