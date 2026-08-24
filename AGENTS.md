# Quota Beacon Development Guide

## Project Overview

Quota Beacon 是基于 React、TypeScript、Vite、Tauri 2 和 Rust 的 Windows/macOS 桌面悬浮工具。它读取本机 Codex Desktop 登录状态并查询真实额度服务，显示 5 小时额度、本周额度、重置时间和重置机会。

本文件是本机 Quota Beacon 项目的产品线路由与开发规则。Windows 正式工作树位于 `C:\Users\AC\Documents\QuotaFloat\work\Windows`，macOS 正式工作树位于 `C:\Users\AC\Documents\QuotaFloat`。

## Product Line Identity And Default Routing

- 本机默认开发产品线固定为 Windows。只要用户没有明确说“Mac”“macOS”或指定 Mac 分支/产物，就必须把“QuotaFloat”“继续开发”“修复”“优化”“同步功能”等泛称路由到 Windows 正式工作树 `work/Windows`，不得读取、修改、测试、构建、提交或发布 Mac 基线。
- Windows 正式线位于 `work/Windows`；正式分支为 `Windows`，功能分支只能使用 `codex/windows-*`，正式版本和标签使用 `windows-v*`，版本提交使用 `windows-v<版本>: <说明>`，GitHub Release 标题使用 `Quota Beacon Windows v<版本>` 且只能包含 Windows 资产。进入该工作树后，读取源码、修改、测试、构建、提交或发布前必须先执行 `npm run preflight:product-line -- --expect windows`。
- 只有用户在当前任务中明确指定 Mac 或 macOS，才允许使用根工作树 `C:\Users\AC\Documents\QuotaFloat`。Mac 正式分支为 `macos`，功能分支只能使用 `codex/macos-*`，版本和标签固定使用 `macos-v*`；版本提交必须使用 `macos-v<版本>: <说明>`，其他提交使用 `macos-<类型>: <说明>`；GitHub Release 标题必须使用 `Quota Beacon macOS v<版本>` 且只能包含 macOS 资产。执行 Mac 任务前必须先运行 `npm run preflight:product-line -- --expect macos`。
- 如果用户同时提到两条产品线但没有明确要求都修改，仍默认只修改 Windows，并把 Mac 视为只读参考；只有明确授权“双线开发”时才分别处理和验证两条线。
- `PRODUCT_LINE.json` 是机器可读的平台身份；它必须与目标工作树、Git 分支、版本文件、原生配置和本文件一致。预检失败时立即停止，不得靠改分支名、跳过检查或修改身份文件绕过。
- Windows 与 Mac 是完全独立的正式线。不得在两者之间 merge、rebase、cherry-pick 或整文件复制；跨线同步必须重新按目标平台实现并独立验证。
- Windows 的 Win32、DWM、DPAPI、EXE、安装目录和实机结论不得写入 Mac 实现或验收结论；Mac 的原生窗口机制、Keychain、Universal 构建、Mac CI 和真机结论也不得当作 Windows 证据。

## Core Product Rules

- 5 小时剩余额度是主视觉、主数字和背景颜色的唯一驱动指标。
- 正常模式每 10 秒强制刷新一次真实额度。
- 后台刷新不能打断色卡预览；退出预览后立即恢复最新真实额度。
- 不根据本地 token 数估算额度，不伪造缺失数据，不自动兑换重置机会。
- 展开卡片与折叠圆球必须使用同一套颜色计算逻辑。
- 预览功能只模拟界面状态，不修改账户、额度或持久化设置。

## Color System

- 百分比先四舍五入并限制在 `0–100`，共 101 个离散状态。
- 每相邻 1% 必须得到不同的背景颜色，使用自动测试防止 RGB 取整产生重复状态。
- 当前关键色标：
  - `100%`：绿色
  - `60%`：浅绿色
  - `35%`：黄色
  - `20%`：橙色
  - `0%`：红色
- 相邻关键色标之间采用连续插值；卡片底色、光晕、中央渐变和进度条应协调变化。
- `60%` 必须保持明显浅绿色，不能提前呈现为黄色。
- `0%` 应明显接近全红，同时保持深色文字可读。
- 颜色算法集中维护在 `src/lib/quotaTheme.ts`，不要在组件中复制色值或插值代码。

## Window And Interaction Rules

- 主窗口折叠尺寸为 `100×100`，展开尺寸为 `320×320`。
- 色卡控制窗口尺寸为 `320×104`，与展开后的主窗口等宽。
- 点击卡片右下角白色 Codex 图标进入预览；再次点击、点击“完成”或关闭控制窗退出预览。
- 控制窗包含 `0–100%`、步进为 `1%` 的滑块，支持拖动、点击轨道和键盘方向键。
- 预览时主额度数字、背景、光晕和进度条同步跟随滑块。
- 预览期间暂停自动折叠；退出后恢复正常折叠行为。
- 主窗口移动或缩放时，控制窗口必须同步跟随。
- 控制窗默认显示在主窗口下方；若当前显示器可用工作区空间不足，则显示在上方，不能被 Dock 或菜单栏遮挡。
- 控制窗不参与窗口状态持久化，避免应用重启后错误地自动显示。
- 预览期间可临时置顶主窗口；退出后必须恢复用户原有置顶设置。

## Motion And Accessibility

- 白色图标按下时轻微缩小，松开时弹性回弹。
- 进入预览时增加旋转和光晕；退出时平滑恢复。
- 所有交互元素必须使用语义化 `button`、清晰的 `aria-label` 和键盘焦点样式。
- 必须支持 `prefers-reduced-motion: reduce`，在用户要求减少动态效果时关闭非必要动画。
- 背景变化后仍需保证主数字、说明文字、按钮和滑块具有足够对比度。

## macOS-Only High-DPI And Transparent Window Rules

本节仅在用户明确指定 Mac/macOS 产品线时适用，不得据此把默认 Windows 任务路由到 Mac 基线。

- 不要给占满窗口的卡片设置固定 `min-width` 或 `min-height`；Retina 或非整数缩放可能导致右侧和底部溢出，从而裁掉三个圆角。
- 根节点、卡片和透明 Tauri 窗口的尺寸、背景与裁切边界必须一致。
- 修改窗口尺寸、圆角、阴影或透明度后，应在真实 Mac 的 Retina 与外接显示器环境中验证，不只看浏览器页面或 Windows 宿主构建。
- 控制窗定位应使用物理尺寸和显示器可用工作区，并考虑 `scale_factor`。
- 透明窗口必须同时保留 `app.macOSPrivateApi: true`、所有透明窗口的 `backgroundColor: "#00000000"` 和 Cargo `macos-private-api` feature。

## Code Organization

- `src/App.tsx`：真实额度刷新、主窗口状态和预览状态切换。
- `src/components/QuotaCard.tsx`：展开卡片和折叠圆球的展示组件。
- `src/components/ProviderMark.tsx`：白色图标按钮及预览开关入口。
- `src/components/PalettePreview.tsx`：独立色卡控制窗口。
- `src/lib/quotaTheme.ts`：101 级颜色插值和 CSS 变量。
- `src/lib/bridge.ts`：前端与 Tauri 命令、事件的桥接。
- `src-tauri/src/lib.rs`：窗口创建、定位、跟随、上下避让、置顶恢复和跨窗口事件。
- `src-tauri/tauri.conf.json`：主窗口和色卡窗口的基础配置。

## Change Discipline

- 修改前先检查 Git 状态，保留用户已有未提交改动。
- 修改前必须进入目标工作树并通过对应预检：默认 Windows 使用 `npm run preflight:product-line -- --expect windows`；只有明确的 Mac 任务使用 `npm run preflight:product-line -- --expect macos`。最终回复必须报告实际产品线、工作树、分支和版本。
- 优先最小必要改动，不做无关重构，不批量格式化上游未格式化文件。
- 从 `1.0.0` 起，每次完成一项产品开发后都必须在实际目标工作树创建版本化提交：同步更新该工作树的 `VERSION`、`package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和 `CHANGELOG.md`。
- 默认按语义化版本递增：修复用 patch，新功能用 minor，破坏性变更用 major。Windows 提交信息使用 `v<版本号>: <简短开发内容>` 并创建同名带注释标签；Mac 使用 `macos-v<版本号>: <简短开发内容>` 并创建同名带注释标签。
- 当前 `upstream` 仅用于读取上游历史，严禁推送；`origin` 也不得默认推送。只有用户在当前任务中明确授权推送目标产品线的分支和标签后，才能推送对应提交与标签。
- 用户授权修改、构建、本地提交或创建本地标签，不代表授权 GitHub 写入。未经明确允许，不执行 `git push`，不创建 PR、Release 或其他远端内容，也不覆盖 Git 历史。
- 替换本机运行程序前先备份上一版可执行文件。
- 临时截图、浏览器记录和构建缓存不得加入 Git。
- 不提交 Codex 登录信息、access token、`.codex`、`.env*`、个人截图或原始额度响应。

## Required Verification

前端或样式改动后，必须在目标工作树至少执行：

```powershell
# 默认 Windows；仅在用户明确指定 Mac 时把 windows 改为 macos
npm run preflight:product-line -- --expect windows
npm test
npm run build
git diff --check
```

Rust、窗口或 Tauri 改动后还应执行：

```powershell
cargo check --manifest-path src-tauri\Cargo.toml
npm run tauri build -- --no-bundle
```

注意：每条产品线都必须使用自身的平台证据完成验收。Windows 宿主上的 Rust/Tauri 检查不能替代 macOS Universal、Keychain、原生窗口或真机验收；Mac 构建也不能替代 Windows DPAPI、DWM、安装包或真机验收。上游部分 Rust 文件可能不符合当前新版 `rustfmt`；不要仅为通过全库格式检查而修改无关文件。

视觉或窗口交互改动后还需验证：

- 浏览器按实际窗口尺寸检查布局，例如主窗口 `320×320`、控制窗 `320×104`。
- 检查 `100%`、`60%`、`35%`、`20%`、`0%` 关键颜色节点。
- 在目标平台的真实 Tauri 程序中检查展开、折叠、打开预览、滑块变化、完成退出和再次点击图标退出。
- 验证主窗口移动时控制窗跟随，以及屏幕底部空间不足时显示到上方。
- 验证退出预览后恢复最新真实额度和用户原有置顶设置。
- Windows 任务验证 Windows 构建、安装产物、SHA-256 和真实 Windows 启动响应；明确的 Mac 任务验证 Universal 构建、签名、DMG/ZIP、SHA-256 和真实 Mac 安装启动响应。

## Definition Of Done

只有同时满足以下条件才算完成：

- 用户要求的行为已实现，现有圆角修复、10 秒刷新和 101 级配色未回归。
- 目标产品线预检、自动测试、TypeScript/Vite 构建、Rust 检查及对应平台的 CI/正式构建通过。
- 关键颜色和窗口布局完成视觉检查。
- 真实桌面双窗口打开、关闭和尺寸验证成功。
- 若本次包含安装交付，目标平台的当前运行版本已安全替换，上一版仍有备份。
- Git 工作区只包含预期源码改动，没有临时文件或无关格式化变更。
