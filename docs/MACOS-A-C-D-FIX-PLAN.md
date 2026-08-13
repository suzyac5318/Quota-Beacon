# Quota Beacon macOS A/C/D 修复计划

> 用途：记录 macOS 追赶版本中阶段 A、C、D 修复批次的要求与实际完成结果。
>
> 状态日期：2026-08-13。修复批次已经完成并整合到正式 `macos` 分支；本文不再作为新的启动提示词。

## 0. 执行结果

| 批次 | 完成版本 | 本地标签 | 结果 |
| --- | --- | --- | --- |
| C1 | `1.9.1` | `macos-v1.9.1` | 账号库恢复、元数据校验、原子凭据文件与失败清理已实现 |
| C2 | `1.9.2` | `macos-v1.9.2` | 真实当前账号对账、重新登录事务与 Keychain 状态区分已实现 |
| D1 | `1.9.3` | `macos-v1.9.3` | 周额度窗口生命周期、缓存代次、有界并发与 weekly-only 查询已实现 |
| C3 | `1.9.4` | `macos-v1.9.4` | 多显示器定位、统一开关状态、幂等登录轮询与动画取消已实现 |
| A1 | `1.9.5` | `macos-v1.9.5` | 标签/版本/历史保护、Universal 最终工件验证与四件套白名单已实现 |
| R1 | `1.9.6` | `macos-v1.9.6` | 已证实冗余已清理；需用户确认的设计入口、跨平台代码和生成资产保留 |

- 最终整合基线：`macos-v1.9.6`，提交 `6f2dc8d675e6fb80012b2711b81ee49af0d7ff0b`。
- 本地验证：前端 35 项、Rust 39 项，以及前端构建、Rust check、Tauri 无 bundle 构建、发布保护脚本和 `git diff --check` 已通过。
- 未验证边界：真实 macOS CI、Keychain 系统交互、Universal 最终 DMG/ZIP、Retina/多显示器以及透明窗口视觉仍需外部验证。
- 阶段 B 已由用户取消，不属于本修复或收尾范围。

## 1. 当前基线与任务边界

- 目标产品线：macOS 正式线 `macos`，不得修改、合并、rebase 或覆盖 Windows `main`。
- 功能整合基线：`macos-v1.9.6`，提交 `6f2dc8d675e6fb80012b2711b81ee49af0d7ff0b`；当前文档收尾版本为 `macos-v1.9.7`（本文所属提交）。
- A 阶段：`macos-v1.5.7`，提交 `81bc2c5`。
- C 阶段：`macos-v1.7.0`，提交 `bebf03a`。
- D 阶段：`macos-v1.8.0`，提交 `8ec26df`。
- 当前远端基线必须在同步前通过 `git ls-remote origin` 重新核对；本文不把本地完成误写为远端 CI 或 Release 已完成。
- `docs/MACOS-CATCH-UP-PLAN.md` 已在收尾阶段同步更新，可与本文一并纳入文档收口版本。
- 本计划只授权修复和验证；不自动授权推送、创建 PR、上传工件或发布 Release。
- 不接触真实 `auth.json`、用户 Token、完整账号 ID、原始额度响应或浏览器 Cookie。

开始前执行：

```powershell
git status --short --branch
git log --oneline --decorate -6
git branch --all --verbose --no-abbrev
git tag --list "macos-v*" --sort=version:refname
git diff --check
```

若 `HEAD`、版本或工作区状态已变化，先更新本计划中的基线，不得直接沿用下面的建议版本号。

## 2. 总体修复顺序

按风险和依赖关系执行，不把所有修复堆进一个提交：

1. C1：账号库持久化与凭据文件安全。
2. C2：真实当前账号、重新登录与 Keychain 状态一致性。
3. D1：周额度生命周期、缓存代次与后台请求收口。
4. C3：账号窗口定位、开关状态和登录轮询竞态。
5. A1：Mac 发布标签、ZIP、DMG 和校验链加固。
6. R1：只清理已经证明无消费者的冗余；开发预览代码需单独确认。

实际执行时各批次使用独立 patch 版本，从 `1.9.1` 依次递增至 `1.9.6`；当前收尾版本为 `1.9.7`。

## 3. C1：账号库持久化与凭据文件安全

### 目标

修复 `vault.json` 崩溃恢复缺口，保证认证临时文件权限、路径和失败清理符合 Mac 本地凭据安全边界。

### 主要文件

- `src-tauri/src/account_vault.rs`
- `src-tauri/src/codex.rs`
- `src-tauri/src/lib.rs`
- `src/macosWindowConfig.test.ts`

### 必须实现

1. 账号元数据加载顺序改为：
   - 主文件有效：使用主文件。
   - 主文件缺失或损坏、备份有效：从 `.bak` 恢复，并显式记录安全日志，不输出凭据。
   - 主文件和备份都无效：返回可见错误或受控空状态，禁止静默覆盖已有损坏文件。
2. `vault.json` 提交避免“先移走主文件、再安装临时文件”的空窗；同文件系统上优先使用临时文件直接原子替换，并保留可恢复备份。
3. 临时文件使用唯一随机名称或 `create_new(true)`，拒绝预先存在的文件及符号链接。
4. macOS/Unix 上凭据临时文件和最终 `auth.json` 明确设置为 `0600`；账号元数据至少不能扩大原文件权限。
5. 所有写入、sync、rename、验证失败路径都清理含凭据的临时文件；清理失败只记录路径类别，不记录内容。
6. 原子替换失败后验证 last-known-good 是否真实恢复。若恢复也失败，返回明确的高优先级错误，不能写“已恢复”。
7. 账号元数据加载时校验：profile ID 唯一、fingerprint 唯一、`activeProfileId` 必须指向现存 profile；异常不得静默接受。

### 自动测试

- 主文件正常加载。
- 主文件损坏、备份有效时恢复。
- 主文件缺失、备份有效时恢复。
- 主文件与备份都损坏时不覆盖原文件。
- 提交中断模拟后可恢复。
- Unix 临时文件和最终 `auth.json` 权限为 `0600`。
- 预置同名普通文件、符号链接和目录时安全失败，不跟随链接。
- rename、sync、验证和回滚分别失败时，临时凭据不残留。
- 重复 profile ID、重复 fingerprint、悬空 active ID 被拒绝或修复为明确安全状态。

### 完成标准

- 崩溃或元数据损坏不会静默清空账号列表并遗留不可发现的 Keychain 凭据。
- 不产生权限宽于预期的明文 Token 文件。
- 错误信息只描述状态，不包含 Token、完整账号 ID 或原始 JSON。

## 4. C2：真实当前账号、重新登录与 Keychain 状态一致性

### 目标

让“当前账号”由真实 `auth.json` 身份决定，保证当前账号重新登录后新凭据立即生效，并允许清理缺失凭据的无效账号。

### 主要文件

- `src-tauri/src/account_vault.rs`
- `src-tauri/src/lib.rs`
- `src/components/AccountSwitcher.tsx`
- `src/components/AccountSwitcher.test.tsx`
- `src/lib/accounts.ts`

### 必须实现

1. `AccountVaultView` 生成前读取当前 `auth.json` 指纹并对账：
   - 匹配已保存 profile：该 profile 才是当前账号。
   - 当前登录未保存：`activeProfileId` 对前端应为 `null` 或明确的 unsaved 状态，不得继续标旧账号为当前。
   - 未登录或认证无效：不得把旧 profile 的主卡失败快照当成其真实额度。
2. 只在状态真实变化时持久化对账结果，避免每次 `view()` 都写磁盘。
3. 当前 profile 重新登录成功后采用与普通切换相同的事务：
   - 校验新凭据身份。
   - 更新 Keychain。
   - 原子替换 `auth.json`。
   - 验证替换结果。
   - 更新 profile 元数据。
   - 增加 `account_generation`、清空主快照和该 profile 周额度缓存、触发真实刷新。
   - 任一步失败时恢复 Keychain、`auth.json` 和元数据。
4. 非当前 profile 重新登录只更新其 Keychain/元数据，不切换当前 `auth.json`，但必须使旧周额度请求失效。
5. 删除缺失 Keychain 条目的非当前 profile 时，允许删除元数据；Keychain 的“条目不存在”视为已删除。权限拒绝、Keychain 锁定和格式损坏必须区分，不能全部映射成“重新登录”。
6. 重新登录成另一个已经存在的账号时拒绝重复身份，不覆盖其他 profile。

### 自动测试

- 外部把 `auth.json` 切到已保存账号时，当前账号随之对账。
- 外部把 `auth.json` 切到未保存账号时，旧 profile 不再标当前，D 阶段不串额度。
- `auth.json` 删除或损坏时，不保留虚假当前账号。
- 当前账号重新登录后，Keychain 与 `auth.json` 都变成新凭据，generation 增加且缓存清空。
- 当前账号重新登录中任一步失败，三份状态全部恢复。
- 非当前账号重新登录不改 `auth.json`。
- Keychain 条目缺失的非当前账号可以删除。
- Keychain 权限拒绝不会被误报为凭据过期。

### 完成标准

- 主卡、账号列表和周额度对于“谁是当前账号”只有一个真实来源。
- 当前账号重新登录后无需隐藏操作或额外切换即可恢复额度。
- 无效账号可恢复或删除，不形成永久僵尸 profile。

## 5. D1：周额度生命周期、缓存代次与后台请求收口

### 目标

只在账号窗口需要数据时查询周额度，避免旧凭据响应回写、失败结果长时间粘住和不必要的 Keychain/网络访问。

### 主要文件

- `src-tauri/src/lib.rs`
- `src-tauri/src/codex.rs`
- `src-tauri/src/models.rs`
- `src/components/AccountSwitcher.tsx`
- `src/components/AccountSwitcher.test.tsx`
- `src/lib/accounts.ts`

### 必须实现

1. 账号窗口维护明确的 opened/visible 状态：
   - 隐藏窗口不启动周额度定时轮询。
   - 打开时立即刷新一次。
   - 关闭时停止前端计时器；已经发出的后端请求可完成，但过期结果不得写缓存或 UI。
2. 为每个 profile 建立 credential/cache generation。开始请求时捕获 generation，写缓存前重新核对；重新登录、删除或身份改变必须增加对应 generation。
3. 删除账号后，任何旧请求都不能重新插入缓存。
4. 重新登录成功后，旧凭据结果不能覆盖新凭据额度。
5. 5 分钟 TTL 只用于成功且身份匹配的非当前账号额度。建议：
   - `ok`：缓存 5 分钟。
   - `signed_out`：可缓存短时间或直到凭据 generation 改变，但必须提供立即重新登录入口。
   - 网络失败、429、服务不可用：不缓存 5 分钟，采用短退避并允许窗口重新打开或用户操作触发重试。
6. 多账号查询采用有界并发，不按账号数串行累加 12 秒超时；限制同时请求数量，避免瞬时流量。
7. 新增只请求 usage/weekly window 的凭据查询函数。非当前账号周额度不得额外调用 reset-credits 接口。
8. 当前账号快照按 `provider == "codex"` 查找，不依赖 `Vec.first()`。
9. 避免一次调用对同一 profile 重复读取 Keychain；读取、校验和查询共用同一份短生命周期凭据数据，离开后尽快释放。

### 自动测试

- 窗口初始隐藏时不会调用 `getAccountWeeklyQuotas()`。
- 打开立即调用一次，打开期间按策略刷新，关闭后停止。
- 快速关闭再打开不会产生两个并行前端轮询器。
- 旧请求在重新登录后完成，不写入新 generation 缓存。
- 旧请求在删除后完成，不重新创建缓存项。
- 短暂网络错误恢复后不会被失败缓存阻塞 5 分钟。
- 成功缓存 299 秒有效，300 秒失效。
- 单、双、多账号有界并发顺序和隔离正确。
- 非当前账号查询只访问 usage 接口，不访问 reset-credits。
- 当前快照数组顺序变化时仍能找到 Codex。

### 完成标准

- 账号窗口隐藏时没有周期性 Keychain 读取和非当前账号额度网络请求。
- 任何旧凭据请求都无法覆盖新账号或新凭据状态。
- 一个账号失败不影响其他账号；网络恢复可在合理时间内显示新结果。

## 6. C3：账号窗口定位、开关状态与登录轮询

### 目标

修复多显示器边缘溢出、隐藏路径状态失步、重开残留状态以及登录轮询重叠。

### 主要文件

- `src-tauri/src/lib.rs`
- `src/lib/accounts.ts`
- `src/App.tsx`
- `src/components/AccountSwitcher.tsx`
- `src/components/AccountSwitcher.test.tsx`

### 必须实现

1. 抽取可测试的纯定位函数，同时处理 x/y：
   - 默认与主窗口左边缘对齐。
   - 靠右时向左夹紧到当前显示器可见工作区。
   - 靠左、负坐标显示器、Dock/菜单栏、上方/下方空间不足均不越界。
   - 使用物理尺寸和正确的 `scale_factor`。
2. 账号窗所有隐藏路径统一进入一个 close/hide 函数并发出同一事件：按钮关闭、主窗口失焦、托盘隐藏、主窗口关闭、单实例唤醒前的清理。
3. `account-switcher-opened` 只在 `show()` 成功后发出；后续定位或读取失败时要么关闭并回滚状态，要么明确保持打开并返回一致状态。
4. 重开账号窗时重置新增表单、别名、旧提示和已结束登录状态；运行中的登录任务是否继续显示必须有一致规则。
5. 登录轮询增加前端单一在途保护；后端完成任务后，对重复的同 task ID 轮询返回幂等的最终状态，而不是“No account login is running”。
6. 快速开关和焦点切换不能启动多个窗口尺寸动画；旧动画必须取消或由单一原生尺寸状态机接管。

### 自动测试

- 右边缘、左边缘、负坐标副屏、上方显示、下方显示、窗口高度变化定位测试。
- 托盘隐藏和关闭主窗口都触发一次且仅一次 closed 状态。
- `show()` 失败时前端不进入 active 状态。
- 关闭后重开表单和旧 notice 已重置。
- 750ms 轮询在慢 Keychain 操作期间仍只有一个调用在途。
- 登录完成后的重复轮询返回同一 completed 结果，不覆盖为错误。
- 快速展开/收起最终尺寸确定且无残留动画。

### 真机验收

- Retina 内屏、外接显示器和负坐标排列。
- 主窗口贴近四边与 Dock/菜单栏。
- 主卡、托盘两种入口开关账号窗。
- 打开浏览器登录导致应用失焦，再回到账号窗完成登录。
- 减少动态效果开启和关闭两种状态。

## 7. A1：Mac 发布链加固

### 目标

确保标签、应用版本、Universal App、DMG、ZIP 和校验文件一一对应，发布的每个工件都经过验证。

### 主要文件

- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `.github/scripts/verify-macos-bundle.sh`
- 可新增 `.github/scripts/verify-macos-release-version.sh`
- `src/macosWindowConfig.test.ts`
- `docs/RELEASE_TEMPLATE.md`

### 必须实现

1. Release 开始时校验：
   - 标签严格匹配 `macos-v<semver>`。
   - 标签版本等于 `VERSION`、`package.json`、`package-lock.json`、Cargo、Tauri 和 App bundle 版本。
   - 标签指向的提交属于 Mac 产品线预期历史。
2. Bundle 验证脚本断言恰好一个 `.app` 和一个预期 DMG；发现额外候选立即失败。
3. ZIP 使用能保留 macOS `.app` 权限、符号链接和 bundle 结构的原生工具生成，例如在 macOS runner 上使用 `ditto`；不要使用未经验证的通用压缩路径。
4. ZIP 生成后解压到全新临时目录，重新执行：
   - `codesign --verify --deep --strict`
   - 主程序 `arm64 + x86_64`
   - `Info.plist` 版本
   - 主程序可执行权限
5. 分别生成 DMG 和 ZIP 的 SHA-256；文件名和校验文件内容必须明确对应，避免一个模糊的 `.sha256` 名称只覆盖 DMG。
6. 上传前使用清单明确列出允许的固定数量工件，禁止 `release-assets/*` 无条件吞入额外文件。
7. `macosWindowConfig.test.ts` 只保留适合单元测试的配置约束；YAML/脚本语义由实际脚本执行测试验证，避免只靠字符串包含断言。

### 自动验证

- 正确标签和完全同步版本通过。
- 错误标签格式、标签版本不一致、任一版本源漂移均失败。
- 多个 App、多个 DMG、缺少 ZIP/DMG/校验文件均失败。
- ZIP 解压后签名、架构、版本和权限验证通过。
- 篡改 ZIP 或 DMG 后对应 SHA-256 失败。
- Release 工件清单只有预期文件。

### 完成标准

- 下载 DMG 或 ZIP 的用户都能拿到一一对应的 SHA-256。
- ZIP 和 DMG 均验证的是最终上传内容，不是压缩前的中间目录。
- 仅正确版本的 `macos-v*` 标签可以创建草稿 Release。

## 8. R1：冗余清理候选

该批次必须排在功能修复之后，且单独提交，便于回滚。

### 可直接验证后删除或收敛

1. `AccountSwitchOutcome.credentialsSwitched`：始终为 `true` 且前端不消费。
2. `AccountSwitchOutcome.restartRecommended`：始终为 `false` 且前端不消费。
3. `AccountVault::view() -> Result<...>`：当前没有 `Err` 路径；可改为直接返回值，或让真实对账错误成为明确 `Err`，二者只能选一。
4. `AccountWeeklyQuota.status` 中 Rust 永不产生的 `stale`。
5. 非当前周额度查询中不使用的 reset credits、5 小时额度和 plan 数据路径。
6. HTTP User-Agent `QuotaFloat/0.1`：改为当前产品名与构建版本，或集中为一个常量。

### 需要用户确认后再处理

1. `?designer`、`DesignPlayground`、非 Tauri `mockVault` 和假周额度：若只用于开发，应使用 dev-only 条件，不进入生产包；若仍是视觉验收入口则保留。
2. `codex_overlay.rs` Windows 实现、`windows-sys` target 依赖和 Windows 图标：Mac 已是独立产品线，但删除前要确认是否仍要求同一源码树可在 Windows 构建。
3. iOS/Android 图标及其他 Tauri 生成资产：先证明不被 Mac bundle、文档或发布脚本引用。

### 完成标准

- 每项删除都有 `rg` 引用证据和编译/测试证据。
- 不把“当前 Mac 不执行”误判为“整个仓库无用”。
- 不把冗余清理与安全修复混在同一提交。

## 9. 每批次验证要求

基础自动验证：

```powershell
npm test
npm run build
cargo check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
npm run tauri build -- --no-bundle
git diff --check
```

Mac CI 验证：

- Universal App 同时包含 `arm64` 和 `x86_64`。
- ad-hoc 签名、最终 ZIP、DMG、两份 SHA-256 均通过。
- 不使用真实用户 Keychain 数据；如增加 Keychain 集成测试，使用唯一测试 service/account，并在成功或失败后清理。

真实 Mac 验收必须单独记录：

- 当前账号、外部登录变化、当前账号重新登录、非当前账号重新登录。
- 删除 Keychain 条目后的恢复和删除路径。
- 单、双、多账号周额度与真实账号一致。
- 断网、恢复、401/403/429、慢请求和快速切换。
- 隐藏账号窗时无周期性非当前账号请求。
- Retina、多显示器、Dock/菜单栏避让、快速开关、应用失焦。
- 透明窗口白底、灰底、四角裁切和减少动态效果无回归。

自动测试通过、Mac CI 通过、真实 Mac 行为通过和发布工件可下载是四个不同结论，不得互相替代。

## 10. Git 与版本纪律

每个完成批次：

1. 只暂存本批次文件，排除 `docs/MACOS-CATCH-UP-PLAN.md` 和无关并行改动。
2. 按 patch 版本同步：
   - `VERSION`
   - `package.json`
   - `package-lock.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/Cargo.lock`
   - `src-tauri/tauri.conf.json`
   - `CHANGELOG.md`
3. 提交格式：`macos-v<版本>: <简短修复内容>`。
4. 创建同名带注释本地标签。
5. 提交和标签前再次执行 `git status`、`git diff --cached --stat` 与完整验证。
6. 没有当前任务明确授权时，不推送 `macos`、不推送标签、不创建 PR、不上传工件、不发布 Release。

若某批次无法完成真实 Mac 验收，应在变更日志和交付中明确写“源码与自动测试完成，真实 Mac 验收未完成”，不得直接宣称修复已交付。

## 11. 已归档的启动提示词

> 以下内容仅保留当时的任务交接记录。所有批次均已完成，不应再次执行或按旧基线启动修复。

```text
请在 C:\Users\AC\Documents\QuotaFloat 中执行 macOS A/C/D 修复。

先完整阅读：
1. 根目录/当前目录适用的 AGENTS.md
2. docs/MACOS-A-C-D-FIX-PLAN.md
3. docs/MACOS-CATCH-UP-PLAN.md（只读参考，不要覆盖或顺手提交）

先只读核对当前分支、HEAD、版本、origin/macos、Git 状态和并行改动。当前计划的时间点基线是 macos-v1.9.0 / 0d82498，但必须以现场事实为准。

本次只选择修复计划中的一个批次，不要同时做其他批次，不要修改 Windows main，不要清理现有未提交文件。开始前先告诉我：选择的批次、目标文件、风险、测试和验收标准；确认范围后再实施。

完成后执行该计划要求的前端、Rust、Tauri 和 git diff 验证，说明自动测试、Mac CI、真实 Mac 验收各自状态。按当前实际版本递增 patch，创建仅含本批次的本地提交和同名带注释标签。没有明确授权时不推送、不创建 PR、不上传、不发布。
```

历史上从 **C1：账号库持久化与凭据文件安全** 开始；该批次及其后续依赖现已全部完成。
