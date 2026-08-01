# macOS GitHub 发布清单

## 版本线边界

- macOS 源码固定维护在 `macos` 分支。
- macOS 回退与发布标签固定使用 `macos-v*`，例如 `macos-v1.5.5`、`macos-v1.5.6`。
- Windows 使用独立分支和版本标签；不得从 `macos` 分支生成或上传 Windows 发布包。
- `upstream` 只用于读取上游历史，严禁推送；所有 macOS 分支与标签只推送到 `origin`。

## 本机准备

Windows 本机不能直接构建 macOS 安装包。macOS Universal 包由 GitHub Actions 的 `macos-latest` runner 构建，本机只需完成共享代码的前端、Rust 和 Windows Tauri 编译检查。

GitHub Actions 会自动安装：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

## 发布 macOS 版本

完成版本文件与 `CHANGELOG.md` 更新后，提交并创建 Mac 专用标签：

```bash
git switch macos
git add <expected-files>
git commit -m "macos-v1.5.6: fix transparent window background"
git tag -a macos-v1.5.6 -m "Quota Beacon macOS v1.5.6"
git push -u origin macos
git push origin macos-v1.5.6
```

推送 `macos-v*` 后，`.github/workflows/release.yml` 只构建 macOS Universal 工件，并创建草稿 Release。附件必须仅包含：

- `quota-beacon-macos-universal-ad-hoc.zip`
- macOS Universal `.dmg`
- `quota-beacon-macos-universal-ad-hoc.sha256`

如果附件中出现 Windows 包，或 Mac 构建、签名、双架构、DMG 校验任一失败，不得发布草稿，应保留失败记录并递增 macOS 补丁版本修复。

## 发给 Mac 用户时的说明

当前 macOS 包是 ad-hoc 签名且未公证：

1. 下载 `.dmg`，需要时使用同一 Release 的 `.sha256` 核对完整性。
2. 打开 DMG，把 Quota Beacon 拖入 Applications。
3. 在 Applications 中右键应用并选择 Open。
4. 如果仍被拦截，到 System Settings -> Privacy & Security 选择 Open Anyway。

不要建议用户关闭系统全局 Gatekeeper。
