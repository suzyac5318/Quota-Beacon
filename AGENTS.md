# Quota Beacon Development Guide

## Project Overview

Quota Beacon 是基于 React、TypeScript、Vite、Tauri 2 和 Rust 的 Windows/macOS 桌面悬浮工具。它读取本机 Codex Desktop 登录状态并查询真实额度服务，显示 5 小时额度、本周额度、重置时间和重置机会。

本文件只适用于 Windows 正式工作树：`C:\Users\AC\Documents\QuotaFloat\work\windows-main`。

## Product Line Identity

- 当前产品线固定为 Windows；正式分支为 `main`，功能分支只能使用 `codex/windows-*`，版本和标签固定使用 `v*`。
- 每次读取源码、修改、测试、提交或发布前，必须先执行 `npm run preflight:product-line -- --expect windows`。预检失败时立即停止，不得靠改分支名、跳过检查或修改身份文件绕过。
- `PRODUCT_LINE.json` 是机器可读的平台身份；它必须与当前工作树、Git 分支、版本文件、Windows 原生配置和本文件一致。
- macOS 正式线位于仓库的 `macos` 工作树，只能作为功能需求和交互语义参考。Windows 任务不得修改 Mac 工作树，不得把 `macos` merge、rebase、cherry-pick 或整文件复制到 `main`。
- macOS 的 Keychain、`macOSPrivateApi`、`macos-private-api`、Universal、DMG 和真机结论不得写入 Windows 实现或验收结论；对应能力必须使用 DPAPI、Win32/DWM、Windows 构建与真实 Windows 验证。
- 如果用户没有明确目标平台，先确认目标产品线；“QuotaFloat”“继续改进”“同步功能”等泛称不授权同时修改两条正式线。

## Product Lines And Personal Edition

- 项目长期保留三条相互独立的产品线：Windows 正式版、macOS 正式版和“个人版”。
- 个人版以届时最新、稳定、已完成验收且工作区干净的 Windows 正式版提交或标签为初始基线；创建时必须记录准确的基线提交与标签。未经用户确认，不得把含未提交修改的工作区直接作为个人版基线。
- 个人版必须放在独立本地目录和独立本地 Git 仓库中开发，不得直接使用或修改 Windows/macOS 正式版的 `main`、`macos` 等主线分支及其工作区。
- 个人版仅供用户个人使用，永久禁止任何 GitHub 写入：不得执行 `git push`，不得创建远程分支、远程标签、PR、Release 或其他远端内容。个人版本地提交和本地标签可用于版本记录与回滚。
- 个人版仓库不得配置可写 GitHub 推送目标；如需读取 Windows 正式版的新历史，只能使用明确的只读同步方式，并且每次把正式版变更同步到个人版前都要得到用户确认。
- 个人版的功能和修改不得默认合并回 Windows 或 macOS 正式版；任何跨产品线同步都必须由用户单独明确授权，并说明同步方向和范围。
- 个人版必须使用独立的产品名称、应用标识、可执行文件、安装路径、偏好设置、窗口状态、登录临时目录和构建产物；唯一例外是账号库明确与 Windows 正式版共享 `app.quotabeacon.desktop/accounts`，以直接使用同一组已保存账号。

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
- 控制窗默认显示在主窗口下方；若 Windows 可用工作区空间不足，则显示在上方，不能被任务栏遮挡。
- 控制窗不参与窗口状态持久化，避免应用重启后错误地自动显示。
- 预览期间可临时置顶主窗口；退出后必须恢复用户原有置顶设置。

## Motion And Accessibility

- 白色图标按下时轻微缩小，松开时弹性回弹。
- 进入预览时增加旋转和光晕；退出时平滑恢复。
- 所有交互元素必须使用语义化 `button`、清晰的 `aria-label` 和键盘焦点样式。
- 必须支持 `prefers-reduced-motion: reduce`，在用户要求减少动态效果时关闭非必要动画。
- 背景变化后仍需保证主数字、说明文字、按钮和滑块具有足够对比度。

## High-DPI And Transparent Window Rules

- 不要给占满窗口的卡片设置固定 `min-width` 或 `min-height`；Windows 非整数缩放可能导致右侧和底部溢出，从而裁掉三个圆角。
- 根节点、卡片和透明 Tauri 窗口的尺寸、背景与裁切边界必须一致。
- 修改窗口尺寸、圆角、阴影或透明度后，应在 Windows 实际缩放环境中验证，不只看浏览器页面。
- 控制窗定位应使用物理尺寸和显示器可用工作区，并考虑 `scale_factor`。

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
- 修改前必须通过 `npm run preflight:product-line -- --expect windows`，并在最终回复中报告产品线、分支和版本。
- 优先最小必要改动，不做无关重构，不批量格式化上游未格式化文件。
- 从 `1.0.0` 起，每次完成一项开发后都必须创建一个版本化提交：同步更新根目录 `VERSION`、`package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和 `CHANGELOG.md`。
- 默认按语义化版本递增：修复用 patch，新功能用 minor，破坏性变更用 major；提交信息使用 `v<版本号>: <简短开发内容>`，并创建同名带注释 Git 标签。
- 当前 `upstream` 仅用于读取上游历史，严禁推送；`origin` 也不得默认推送。只有用户在当前任务中明确授权“推送到 GitHub”后，才能推送对应提交与标签。
- 用户授权修改、构建、安装、本地提交或创建本地标签，均不代表授权 GitHub 写入。未经明确允许，不执行 `git push`，不创建 PR、Release 或其他远端内容，也不覆盖 Git 历史。
- 替换本机运行程序前先备份上一版可执行文件。
- 用户要求“修改后验收”“交付”或“安装”时，默认同时授权：备份当前安装 EXE、停止本项目旧进程、用本次已验证构建替换安装文件、重新启动并执行安装后运行检查；替换失败时优先恢复备份。若用户明确要求只改源码、不要安装或不要重启，则以该次限制为准。
- 临时截图、浏览器记录和构建缓存不得加入 Git。
- 不提交 Codex 登录信息、access token、`.codex`、`.env*`、个人截图或原始额度响应。

## Required Verification

前端或样式改动后至少执行：

```powershell
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

注意：上游部分 Rust 文件可能不符合当前新版 `rustfmt`。不要仅为通过全库格式检查而修改无关文件；应确保本次新增代码风格清晰且 Rust 编译通过。

视觉或窗口交互改动后还需验证：

- 浏览器按实际窗口尺寸检查布局，例如主窗口 `320×320`、控制窗 `320×104`。
- 检查 `100%`、`60%`、`35%`、`20%`、`0%` 关键颜色节点。
- 在真实 Windows Tauri 程序中检查展开、折叠、打开预览、滑块变化、完成退出和再次点击图标退出。
- 验证主窗口移动时控制窗跟随，以及屏幕底部空间不足时显示到上方。
- 验证退出预览后恢复最新真实额度和用户原有置顶设置。
- 验证生成的可执行文件与安装位置文件哈希一致，程序能够正常启动并响应。

## Definition Of Done

只有同时满足以下条件才算完成：

- 用户要求的行为已实现，现有圆角修复、10 秒刷新和 101 级配色未回归。
- 自动测试、TypeScript/Vite 构建、Rust 检查及 Windows 桌面构建通过。
- 关键颜色和窗口布局完成视觉检查。
- 真实桌面双窗口打开、关闭和尺寸验证成功。
- 当前运行版本已安全替换，上一版仍有备份。
- Git 工作区只包含预期源码改动，没有临时文件或无关格式化变更。
