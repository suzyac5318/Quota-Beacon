# macOS 发布说明

## 当前发布目标

Windows 与 macOS 共享 React/CSS/Tauri 业务代码，但版本线完全分离：macOS 使用 `macos` 分支和 `macos-v*` 标签，Windows 使用自己的分支与标签，发布包不得混合。

macOS 版本默认输出：

- `quota-beacon-macos-universal-ad-hoc.zip`
- macOS Universal `.dmg`
- `quota-beacon-macos-universal-ad-hoc.sha256`

Universal 包同时支持 Apple Silicon 和 Intel Mac，并使用无需付费账号的 ad-hoc 签名。ad-hoc 签名不能替代 Apple Developer ID 签名和公证。

## 发布一个 macOS 下载版本

推送 `macos-v*` 标签会触发 `.github/workflows/release.yml`：

```bash
git tag -a macos-v1.5.6 -m "Quota Beacon macOS v1.5.6"
git push origin macos-v1.5.6
```

工作流只构建 macOS Universal 包，并在构建、签名、双架构和 DMG 校验全部成功后创建仅含 Mac 附件的草稿 Release。

## CI 与构建

`.github/workflows/ci.yml` 在 `macos` 分支执行前端测试/构建、npm audit、Rust 测试和 macOS Universal Tauri build。macOS runner 会安装：

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

并执行：

```bash
npm run tauri -- build --target universal-apple-darwin
```

`.github/scripts/verify-macos-bundle.sh` 自动检查：

- `.app` 的严格递归签名完整性和 ad-hoc 签名身份。
- 主可执行文件同时包含 `arm64` 与 `x86_64`。
- DMG 通过 `hdiutil verify`。
- 发布 DMG 的 SHA-256 校验文件已生成。

## macOS ad-hoc 包使用说明

1. 下载 `.dmg`，并可用同一 Release 中的 `.sha256` 核对完整性。
2. 打开 DMG，将 Quota Beacon 拖入 Applications。
3. 在 Applications 中右键 Quota Beacon，选择 Open。
4. 如果仍被阻止，到 System Settings -> Privacy & Security 选择 Open Anyway。

不要关闭系统全局 Gatekeeper。

## 跨平台维护原则

- 共享行为优先维护在公共 React/CSS/Rust 代码中。
- 操作系统差异放在 Tauri 壳层或平台专用配置中。
- Windows 与 macOS 的分支、版本号、标签、构建工件和 Release 各自独立。
- macOS 透明窗口必须保留 `macOSPrivateApi` 与窗口级透明 `backgroundColor`。
