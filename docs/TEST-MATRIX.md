# 测试矩阵

| 范围 | 场景 | 预期 | 状态 |
| --- | --- | --- | --- |
| 数据 | Codex 正常登录 | 显示真实 5 小时窗口、周窗口与会员类型 | 待 Windows/macOS 桌面环境验证 |
| 数据 | 未登录或登录过期 | 显示登录提示，不暴露响应或 token | 待 Windows/macOS 桌面环境验证 |
| 数据 | 401/403/429/断网 | 安全错误文案、保留旧数据并退避 | 快照合并单元测试通过，待桌面集成验证 |
| 数据 | 变形或缺字段响应 | 不崩溃，不显示虚假额度 | 解析器单元测试覆盖，待集成验证 |
| 登录态 | Windows `CODEX_HOME` 或用户目录 `.codex/auth.json` | 可以读取本机 Codex 登录态 | Windows 11 实机验证 |
| 登录态 | macOS `CODEX_HOME` 或 `~/.codex/auth.json` | 可以读取本机 Codex 登录态 | 待 macOS 实机验证 |
| 窗口 | 拖动、锁定、鼠标穿透 | 锁定后不拦截编辑器输入，托盘可解锁 | 待 Windows/macOS 验证 |
| 窗口 | 多显示器、缩放、移除显示器 | 恢复到可见工作区 | 依赖 window-state 插件，待实机验证 |
| 托盘 | Windows 托盘菜单 | 显示/隐藏、刷新、解锁、固定、语言切换、开机启动、退出可用 | Windows 11 实机验证 |
| 菜单栏 | macOS 菜单栏托盘 | 显示/隐藏、刷新、解锁、固定、语言切换、开机启动、退出可用 | 待 macOS 实机验证 |
| 视觉 | 悬浮球和展开卡片 | Windows/macOS 使用同一 CSS 参数，尺寸、透明度、圆角、文字布局保持一致 | 待双平台截图验收 |
| 生命周期 | 单实例、关闭隐藏、休眠恢复 | 无重复后台进程，窗口可恢复 | 待实机验证 |
| 性能 | 空闲 CPU/内存 | 无持续高 CPU，记录平台基线 | 待安装包验证 |
| 构建 | Windows unsigned 包 | 生成 `quota-beacon-windows-unsigned.zip` | CI/Release 验证 |
| 构建 | macOS Universal ad-hoc 包 | 生成 ad-hoc 签名的 ZIP 与 DMG，主程序同时包含 Apple Silicon 和 Intel 架构 | CI 自动检查，待 Release 验证 |
| 完整性 | macOS DMG | `codesign --verify --deep --strict`、`lipo`、`hdiutil verify` 通过，并发布 SHA-256 | CI 自动检查，待 Release 验证 |
| 隐私 | 日志与配置扫描 | 无 token、账号 ID、原始响应 | 静态审查通过，待安装包扫描 |

## 发布门槛

发布前应满足：

- 前端测试、前端构建、Rust 测试通过。
- 当前版本线的 macOS CI Universal bundle、DMG 和 SHA-256 成功生成；Windows 由独立版本线验收。
- macOS 实机完成 DMG 挂载、拖入 Applications、Gatekeeper 手动放行、再次正常启动、菜单栏托盘、透明悬浮窗、展开/收起、拖动、置顶、读取 `~/.codex/auth.json` 验证；在完成前必须在 README 与 Release 中明确标注“尚待 Mac 实机验证”。
- 严重和高风险问题清零。
