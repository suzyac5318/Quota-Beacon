# GitHub 发布与分享清单

## 需要提前安装或准备什么

Windows 与 macOS 使用完全独立的分支、标签和发布工作流。本文件只描述 Windows 正式线；macOS 包不得由 Windows 分支或 Windows 标签构建。

本机需要：

- Git
- Node.js 20+
- Rust stable
- npm 依赖已安装

GitHub 需要：

- 一个 GitHub 仓库
- GitHub Actions 已启用
- 代码已推送到默认分支

## 第一次上传到 GitHub

当前 `upstream` 只用于读取衍生项目的上游历史，绝不可推送。`origin` 应指向 `suzyac5318/Quota-beacon`。首次只推送主分支，不使用 `--follow-tags`，避免所有历史版本标签同时触发 Release：

```bash
git remote add origin https://github.com/suzyac5318/Quota-beacon.git
git branch -M Windows
git push -u origin Windows
```

后续版本完成后，更新 `VERSION`、各构建配置和 `CHANGELOG.md`，创建版本化提交和标签，再推送：

```bash
git add <expected-files>
git commit -m "windows-v1.10.2: separate Windows product line"
git tag -a windows-v1.10.2 -m "Quota Beacon Windows v1.10.2"
git push origin Windows
git push origin windows-v1.10.2
```

## 生成可分享版本

只有从 `Windows` 正式分支创建并推送的 `windows-v*` tag 才允许触发 Windows release workflow：

```bash
git tag -a windows-v1.10.2 -m "Quota Beacon Windows v1.10.2"
git push origin windows-v1.10.2
```

构建完成后，到 GitHub 仓库的 Releases 页面检查草稿 release。Release 标题必须为 `Quota Beacon Windows v<版本>`，附件只能包含：

- `quota-beacon-windows-unsigned.zip`
- `quota-beacon-windows-unsigned.zip.sha256`

若出现 `.dmg`、`macos`、`universal` 或其他非 Windows 资产，必须停止发布并删除错误草稿。确认 Windows 附件、SHA-256 和自动生成的说明无误后再发布草稿。

首次公开仓库还应确认：

- `Windows` 分支规则和 Windows 必需状态检查已启用。
- Dependabot、依赖漏洞警报、secret scanning 与 push protection 已启用。
- About 描述、Topics、Issues 和 Social Preview 已配置。
- Private vulnerability reporting 已启用。

## 以后公开分发还需要什么

如果要面向非技术用户公开分发，建议补：

- Windows 代码签名证书。
- GitHub Secrets 中的 Windows 签名配置。

这些账号、证书和密码不能由代码生成，需要项目所有者申请或购买。
