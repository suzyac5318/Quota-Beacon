# Windows 发布说明

## 产品线边界

本工作树只发布 Windows 正式版：

- 正式分支：`Windows`
- 功能分支：`codex/windows-*`
- 标签：`windows-v<semver>`
- 版本提交：`windows-v<semver>: <说明>`
- 其他提交：`windows-<类型>: <说明>`
- Release 标题：`Quota Beacon Windows v<semver>`
- 资产：`quota-beacon-windows-unsigned.zip` 与对应 `.sha256`

macOS 使用独立的 `macos` 分支、`macos-v*` 标签和 macOS 发布工作流。Windows Release 中出现 `.dmg`、`macos` 或 `universal` 资产时必须停止发布。

## 发布流程

发布前必须在 Windows 正式工作树运行：

```powershell
npm run preflight:product-line -- --expect windows
npm test
npm run build
cargo check --manifest-path src-tauri\Cargo.toml
npm run tauri build -- --no-bundle
git diff --check
```

验证通过后创建 Windows 版本提交与标签；远端写入必须得到用户对具体分支和标签的明确授权：

```powershell
git push origin Windows
git push origin windows-v1.10.2
```

标签必须指向 `Windows` 分支历史，且标签版本必须与六个版本源一致。工作流只生成 Windows ZIP 和 SHA-256，并创建草稿 Release；发布草稿前还要检查 Windows 安装、启动、托盘、窗口交互和 SHA-256。

## 签名边界

当前 Windows 包未签名，可能触发 SmartScreen 或“未知发布者”提示。正式公开分发前应配置 Windows 代码签名证书；仓库不得包含证书、私钥或密码。
