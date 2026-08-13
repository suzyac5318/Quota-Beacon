# Quota Beacon macOS 追赶计划摘要

> 本文是 macOS 正式产品线的跨会话开发交接文档。
>
> 状态日期：2026-08-13。本文已由“待执行计划”收口为“本地完成记录”；远端、macOS CI 和真机状态仍须以实时结果为准。

## 0. 收尾结论

- macOS 追赶阶段 A、C、D、E 已在独立 `macos` 产品线上完成；阶段 B 按用户决定取消，不再开发。
- C1、C2、D1、C3、A1、R1 修复批次已依次落在 `macos-v1.9.1` 至 `macos-v1.9.6`。
- 功能整合基线为 `macos-v1.9.6`，提交 `6f2dc8d675e6fb80012b2711b81ee49af0d7ff0b`；本文随收尾版本 `macos-v1.9.7` 纳入正式线。
- Windows `main` 未被合并、rebase 或覆盖；Mac 仍使用 `macos-v*` 独立版本和发布链。
- Windows 宿主上的前端、Rust、Tauri 无 bundle 构建及发布保护脚本已经通过；真实 macOS CI、Universal 最终工件和真机视觉/Keychain 验收尚不能由本机结果替代。

## 1. 产品线边界

Quota Beacon 的 Windows 与 macOS 是两条独立正式产品线：

- Windows 正式线：`main`，使用 `v*` 版本和标签。
- macOS 正式线：`macos`，使用 `macos-v*` 版本和标签。
- Mac 追赶时，Windows 仅作为功能需求、交互语义和测试案例的参考源。
- 禁止把 Windows `main` 直接合并、rebase 或覆盖到 `macos`。
- 禁止在 Mac 任务中修改 Windows `main` 的版本、提交、标签、安装程序或发布状态。
- Mac 的代码、版本、提交、标签、CI、构建工件和 Release 必须独立维护。

追赶目标是用户能力、数据行为和安全边界对等，不是底层平台代码完全一致。

## 2. 当前 Mac 基线

- 分支：`macos`
- 功能整合基线：`macos-v1.9.6`，提交 `6f2dc8d675e6fb80012b2711b81ee49af0d7ff0b`。
- 文档收尾版本：`macos-v1.9.7`（本文所属提交）。
- 共同历史基线：Windows/Mac 在 `v1.5.5`、提交 `e7ade899e155333e1e432faafc752d01345915ad` 后分开。
- 已有 Mac 发布能力：Universal `arm64 + x86_64`、ad-hoc 签名、DMG、ZIP 和 SHA-256。
- 历史自动验证已通过；真实 Mac 上的透明窗口视觉验收仍需重新确认。

Windows 的版本号只用于说明功能参照点，不得写入 Mac 版本文件或作为 Mac 标签。

## 3. 不可破坏的 Mac 专属能力

Mac `1.5.6` 修复了 Tauri/WKWebView 透明窗口白底。后续移植必须同时保留：

1. `src-tauri/tauri.conf.json` 中 `app.macOSPrivateApi: true`。
2. 每个透明窗口的 `backgroundColor: "#00000000"`。
3. `src-tauri/Cargo.toml` 中 Tauri 的 `macos-private-api` feature。
4. `src/macosWindowConfig.test.ts` 的配置和发布隔离回归测试。
5. `macos-v*` 标签触发的 Mac-only CI/Release。
6. Universal 架构、ad-hoc 签名、DMG 和 SHA-256 校验。

CSS 透明背景不能替代上述原生配置。缺失任意一层都可能导致白底回归或 Mac 构建失败。

## 4. 后续会话启动检查

修改前先执行只读核对：

```powershell
git status --short --branch
git branch --all --verbose --no-abbrev
git tag --list --sort=-version:refname
git merge-base main macos
git log --oneline --decorate macos..main
git log --oneline --decorate main..macos
```

必须确认：

- 当前位于 `macos`，或基于 `macos` 建立的 `codex/macos-*` 工作分支。
- 工作区没有用户未提交改动；不得清理、覆盖或混入已有修改。
- 当前 Mac 版本、基线提交、远端引用和 Mac-only workflow 没有变化。
- 本次只处理一个明确的 Mac 追赶阶段。
- 未获得当前任务的明确授权时，不推送、不创建 PR、不上传、不发布。

## 5. 已完成追赶与历史差距

### 5.1 差距口径

以本地已验证分支为准：

| 项目 | Windows | macOS |
| --- | --- | --- |
| 当前本地正式线 | Windows 独立维护，以 `main` 实时状态为准 | `macos` / `macos-v1.9.7` / 本文所属提交 |
| 当前远端正式线 | 以 `origin/main` 实时状态为准 | 同步前以 `git ls-remote origin` 实时状态为准 |
| 分线起点 | 共同基线 `v1.5.5` / `e7ade89` | 共同基线 `v1.5.5` / `e7ade89` |

- 下表保留的是追赶前的历史差距与平台取舍依据，不再代表当前待办。
- Mac 已按自身平台边界完成 A/C/D/E 及后续修复；阶段 B 明确取消，不应从 Windows 复制补做。
- “27 个版本条目”不等于需要在 Mac 上照做 27 次开发。大量 `1.6.x` 是 Windows DWM/Win32 专属视觉迭代，应转换成 Mac 真机视觉验收项，而不是复制代码。
- Windows `1.8.1` 有变更日志条目，但当前本地标签列表未发现 `v1.8.1`；它仍作为功能差异记录，后续开始移植前应重新核对 Git 历史。

### 5.2 Windows 逐版本差异

| Windows 版本 | 主要内容 | Mac 处理结论 |
| --- | --- | --- |
| `1.6.0` | Windows 11 DWM Acrylic；主卡、预览窗、调色盘原生模糊；DPI 圆角裁切；重整关键色标和辅助功能 | DWM/Win32 不移植；共享色标、对比度、减少动态效果需要核对 |
| `1.6.1` | 停用矩形 HWND 材质和外阴影，消除透明窗口灰色方底 | Windows 专属修复；Mac 只检查是否存在同类白底/灰底 |
| `1.6.2` | 独立 Host Backdrop 轻模糊面；与折叠/展开动画同步；修正缩放定位 | Windows 专属实现，不移植；Mac 需独立验证原生材质动画 |
| `1.6.3` | 原生模糊面内缩，修复高 DPI 白边凸点 | Windows 专属几何修复，不移植 |
| `1.6.4` | 模糊扩展到两个控制窗；三窗口统一内描边 | 原生层不移植；共享 CSS/视觉结果可参考 |
| `1.6.5` | 用客户区物理宽度计算比例，修复圆角凸起 | Windows 专属 DPI 计算，不移植 |
| `1.6.6` | 半像素内描边，修复非整数缩放叠亮 | CSS 视觉规则可在 Mac 实测后选择采用 |
| `1.6.7` | Windows 原生模糊从 `8.0` 降到 `6.0` | Windows 专属参数，不移植 |
| `1.6.8` | 三卡 CSS 模糊降到 `18px`，提高透明度和通透感 | 可作为 Mac 视觉参考，不能代替真机验收 |
| `1.6.9` | 原生模糊降到 `4.5`，CSS 主卡降到 `13.5px` | 仅作为视觉参考；Mac 使用自己的原生材质 |
| `1.6.10` | Windows Codex 窗口分层轮询；日志路径缓存；JSONL 追加式读取 | JSONL 追加读取可移植；Windows 窗口枚举需用 Mac 机制重做 |
| `1.6.11` | 额度请求合并；失败 `30/60/120s` 退避；手动刷新绕过；Rust 不排队 | 跨平台可靠性功能，应优先移植 |
| `1.7.0` | Windows DPAPI 多账号保险库；账号胶囊、托盘子菜单、账号窗；登录、切换、回滚和请求隔离 | 用户能力要追赶；必须用 Keychain 和 Mac 登录/窗口机制重做 |
| `1.7.1` | 托盘和账号子菜单中文化 | Mac 托盘/菜单文案需要同步 |
| `1.7.2` | 再点账号入口关闭；点主卡其他区域或应用外关闭账号窗 | 交互语义应移植，并在 Mac 焦点模型下验证 |
| `1.7.3` | 账号操作图标列对齐；隐藏可见滚动条并保留滚动 | 共享 UI，可移植 |
| `1.7.4` | 右上角加号按需展开新增账号表单 | 共享 UI，可移植 |
| `1.7.5` | 加号再次点击折叠；重开窗口重置表单 | 共享交互，可移植 |
| `1.7.6` | 账号窗紧凑/展开高度与表单动画；减少动态效果降级 | 共享交互；原生窗口尺寸动画需 Mac 实测 |
| `1.7.7` | 收紧折叠窗口透明遮挡区域 | Windows 实测修复；Mac 应按自身窗口尺寸重新验收 |
| `1.7.8` | 修复子 Agent/分叉会话 Token 被并入父会话而漏计 | 跨平台数据正确性修复，应优先移植 |
| `1.7.9` | 账号窗高度调整，完整显示两个账号标签 | Windows 尺寸结论不可照搬；Mac 需重新测量 |
| `1.7.10` | 提示出现时增加窗口高度；移除“切换并重启”按钮，只保留普通切换 | 交互结果应参考；Mac 是否需要重启须先实测 Codex 行为 |
| `1.8.0` | 每个账号显示周额度；当前账号复用快照；非当前账号独立查询并缓存 5 分钟；失效隔离 | 依赖 Mac 多账号，完成 Keychain 账号库后移植 |
| `1.8.1` | 切换成功提示 10 秒自动消失；旧定时器不误清后续提示 | 共享状态逻辑，可随 `1.8.0` 移植 |
| `1.8.2` | Windows 调色盘原生模糊层与外壳动画同步 | Windows 专属实现；Mac 只做同类闪烁/残留视觉检查 |
| `1.9.0` | 账号窗跟随当前账号 5 小时额度和调色盘；切换后同步新色调；账号窗玻璃视觉统一 | 用户视觉能力要追赶；使用 Mac 原生透明/模糊实现 |

### 5.3 Mac 已有而 Windows 主线不能覆盖的内容

Mac `macos-v1.5.6` 自己拥有：

- `macOSPrivateApi: true`。
- 所有透明窗口的原生透明 `backgroundColor`。
- Cargo `macos-private-api` feature。
- `macosWindowConfig.test.ts`。
- 独立 `macos` 分支、`macos-v*` 标签和 Mac-only Release。
- Universal、ad-hoc 签名、DMG、ZIP 和 SHA-256。

这些不是“旧代码”，而是 Mac 正式线的必要基座。追赶过程中不得用 Windows 对应文件整文件覆盖。

## 6. 阶段完成记录

### 阶段 A：Mac 基线与回归保护（已完成）

目标：先证明 `macos-v1.5.6` 可重复构建，并冻结透明窗口和发布隔离规则。

- 记录起点分支、提交、标签和远端状态。
- 检查透明窗口三层配置。
- 检查 `macosWindowConfig.test.ts`。
- 检查 Mac-only workflow 只响应 `macos-v*`。
- 检查 Universal、签名、DMG 和校验文件流程。
- 在真实 Mac 的浅色/深色桌面检查折叠球和展开卡片四角。

完成标志：Mac 基线验证通过，没有修改 Windows 产品线。

### 阶段 B：Mac 稳定性与数据正确性（已取消）

> 用户已明确决定不再执行阶段 B。以下条目仅保留为历史计划，不是后续待办，也不得在收尾中补做。

建议目标版本：`macos-v1.6.0`。

从 Windows 后续实现中选择可共享逻辑：

- 单一在途额度请求，避免自动刷新、手动刷新、恢复和聚焦重复请求。
- 正常状态每 10 秒刷新；连续失败按 `30s → 60s → 120s` 退避。
- 手动刷新可绕过退避；成功后恢复 10 秒周期。
- Rust 已有请求运行时返回缓存，不形成请求队列。
- 鼠标移入只展示缓存，不触发额外请求。
- 修复子 Agent/分叉会话被错误并入父会话导致的 Token 漏计。
- 每个 JSONL 只采用首个 `session_meta` 作为自身会话 ID。
- 保留会话跨归档目录移动后的去重。
- 同步共享的五个颜色关键点、文字对比度和减少动态效果规则。

不得移植 Windows 窗口枚举、Win32 轮询、DWM 或 Host Backdrop 实现。

完成标志：请求合并、失败退避、分叉会话和归档移动均有自动测试；真实 Mac 断网、恢复和手动刷新通过。

### 阶段 C：Mac 原生多账号（已完成，后续修复至 `macos-v1.9.2`）

建议目标版本：`macos-v1.7.0`。

目标能力：

- 使用 macOS Keychain 保存账号凭据，不能复制 Windows DPAPI。
- 前端只接收账号别名、脱敏邮箱、账号指纹和状态，不接收 Token。
- 支持保存当前账号、隔离登录添加账号、重命名、删除、重新登录和普通切换。
- 切换前校验目标凭据，原子替换 `auth.json`，失败时恢复 last-known-good。
- 使用账号代次隔离请求，禁止旧账号在途响应覆盖新账号界面。
- 新增 Mac 的 `account-switcher` 窗口、主卡账号胶囊和托盘账号入口。
- 账号窗口跟随主窗口，并依据 macOS 可见工作区上下避让。
- 支持多显示器、Retina、键盘焦点和减少动态效果。
- 调研并实测 Mac 上 Codex Desktop/CLI 的登录、凭据刷新及可选重启行为。

安全边界：

- 不读取浏览器 Cookie。
- 不在日志或前端暴露 Token、原始凭据和完整额度响应。
- 隔离登录目录只能位于应用控制目录内。
- 可能中断 Codex 任务的操作必须独立入口、明确说明并二次确认。

完成标志：双账号保存/切换/回滚、重复账号、无效凭据、超大文件、符号链接、恢复账号保护和旧请求隔离测试通过；真实 Mac 完成交互验收。

### 阶段 D：Mac 多账号周额度（已完成，后续修复至 `macos-v1.9.3`）

建议目标版本：`macos-v1.8.0`。依赖阶段 C。

- 每个已保存账号显示每周剩余额度。
- 账号列表不重复显示 5 小时额度、相对更新时间或过期标签。
- 当前账号复用主卡每 10 秒更新的真实快照。
- 非当前账号使用其 Keychain 凭据独立查询，不切换或覆盖当前 `auth.json`。
- 非当前账号额度严格缓存 5 分钟。
- 删除账号时清除对应额度缓存。
- 单个账号凭据失效只影响该账号，并提供重新登录入口。
- 切换成功提示 10 秒后自动消失；旧定时器不得清除后续错误。

完成标志：单账号、双账号、多账号、缓存边界和失效隔离测试通过；真实 Mac 显示与实际账号一致。

### 阶段 E：Mac 账号主题与视觉收口（已完成）

建议目标版本：`macos-v1.9.0`。依赖阶段 D。

- 账号窗口外壳跟随当前账号真实 5 小时剩余额度和用户调色盘。
- 额度未知时使用中性玻璃色。
- 账号列表保持中性，当前账号行只做弱状态染色。
- 从主卡或托盘打开时传递同一主题。
- 账号切换后等待新账号真实额度，再更新窗口色调。
- 使用 macOS 原生透明/模糊能力，不复制 Windows Host Backdrop。
- 检查主卡、预览、调色盘和账号窗的打开、关闭、跟随和边缘避让。
- 检查 `100% / 60% / 35% / 20% / 0%` 以及未知额度状态。

完成标志：Retina、外接显示器、Dock/菜单栏避让、减少动态效果和 Universal 安装包均通过真实 Mac 验收。

## 7. Windows 只作参考、禁止直接移植的内容

- DWM Desktop Acrylic、`DWMWA_USE_HOSTBACKDROPBRUSH`。
- Win32 原生模糊承载窗口和 Region 圆角裁切。
- Windows 客户区物理尺寸及 DPI 几何代码。
- Windows Codex 窗口枚举和进程前后台判断实现。
- DPAPI 凭据加密。
- Windows Codex 进程停止、启动和重启实现。
- Windows EXE、安装目录、ACL、备份和替换流程。
- Windows `main` 的版本号、提交、标签、CI 工件和 Release。

允许参考的是这些实现背后的用户结果、数据规则、错误隔离、安全约束、交互语义和测试案例。

## 8. 关键代码参考映射

从 Windows 版本挑选共享逻辑时逐文件审查，不整文件覆盖：

- `src/App.tsx`：刷新状态机、账号状态、提示生命周期。
- `src/components/QuotaCard.tsx`：主卡账号入口。
- `src/components/AccountSwitcher.tsx`：账号管理 UI 参考。
- `src/lib/accounts.ts`：前端账号类型和格式化参考。
- `src/lib/refreshPolicy.ts`：刷新退避策略。
- `src/lib/quotaTheme.ts`：101 级颜色和主题变量。
- `src/lib/bridge.ts`：Tauri command/event 桥接。
- `src-tauri/src/codex.rs`：认证解析和额度请求。
- `src-tauri/src/token_usage.rs`：会话日志和 Token 汇总。
- `src-tauri/src/account_vault.rs`：账号行为参考；DPAPI 实现禁止复用。
- `src-tauri/src/lib.rs`：窗口、托盘、command 和事件编排参考。

Mac 自己维护：

- `src-tauri/tauri.conf.json`：以 Mac 分支配置为底稿增量修改。
- `src-tauri/Cargo.toml`：保留 `macos-private-api`。
- `src/macosWindowConfig.test.ts`：保护透明窗口和发布隔离。
- `.github/workflows/release.yml`：保持 Mac-only、Universal 和 Mac 工件。

`src-tauri/src/window_material.rs` 是 Windows 专属，不属于 Mac 移植范围。

## 9. 每阶段验证

基础自动验证：

```powershell
npm test
npm run build
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
git diff --check
```

Mac CI/设备验证：

- Universal 主程序包含 `arm64` 和 `x86_64`。
- ad-hoc 签名、应用包、DMG 和 SHA-256 通过。
- `CFBundleShortVersionString` 与 `CFBundleVersion` 为预期 Mac 语义版本。
- 折叠、展开、预览、调色盘、账号窗、托盘和多显示器行为正常。
- 透明窗口四角无白底、灰色矩形或裁切。
- 源码检查、自动测试、构建、CI 工件、真机运行和视觉验收必须分别报告；不能互相替代。

## 10. Mac 版本与发布纪律

- 每个完成阶段只在 Mac 产品线更新 `VERSION`、`package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和 `CHANGELOG.md`。
- Mac 提交格式：`macos-v<版本号>: <简短内容>`。
- 创建同名带注释本地标签 `macos-v<版本号>`。
- 不在 Mac 提交中混入 Windows 源码、构建缓存、截图、登录信息或原始额度响应。
- 本地提交、标签、构建和安装均不代表允许推送或发布。
- 只有用户在当前任务明确授权后，才能推送 Mac 分支/标签或创建 Mac Release。
- 推送、上传工件、Draft Release、公开 Release 和匿名下载必须分别验证。

## 11. 收尾后仍需外部验证的项目

- `macos-v1.9.6` 仍缺少当前真实 Mac 上的透明窗口视觉复核。
- Mac 上 Codex Desktop/CLI 的登录启动、退出和凭据刷新行为尚需实机调研。
- Keychain 的 service/account 命名、迁移和系统提示体验尚未确定。
- Mac 账号窗口的原生材质、焦点、置顶和多显示器避让需要实机验证。
- Windows 账号代码混合共享逻辑和 Win32 实现，必须按函数和平台边界挑选移植。
- Codex 额度接口不是公开稳定 API；字段或认证变化时应显示不可用，不得猜测额度。

## 12. 后续会话交付模板

每次 Mac 开发结束时说明：

1. 起始分支、提交、标签和工作区状态。
2. 本次 Mac 阶段、目标版本和实际修改文件。
3. 实现了哪些能力，明确没有修改 Windows 产品线。
4. 自动测试、Mac CI、真机运行和视觉验收各自结果。
5. 当前 Mac 提交和本地标签。
6. 是否推送、是否发布；没有授权时写明“未推送、未发布”。
7. 剩余风险、阻塞项和下一阶段入口。
