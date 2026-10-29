# 自动发布与手动打包

## 主分支自动发布

`.github/workflows/desktop-release.yml` 在每次推送到 `main` 时运行，合并 PR 到 `main` 也会触发；无需手动创建 tag。它先运行发布逻辑测试，再调用已有的签名构建工作流，并将同一提交的 Windows x64 `.exe`、`.msi` 和已公证的 macOS 通用 `.dmg`、`.app` ZIP 发布到 GitHub Releases。

每次构建使用独立的 tag：`v<应用版本>-build.<Actions运行编号>`，例如 `v3.20.2-build.123456789`。tag 指向该次构建的确切提交；Release 名称和说明包含源码提交与构建链接。只有全部安装包签名校验、公证及上传检查通过后，Release 才会从草稿变为正式发布。较旧构建晚完成时，不会覆盖更新构建的 Latest 标记。

Apple 尚未完成公证时，原包、提交 UUID 和 Release 草稿都会保留。`.github/workflows/desktop-release-resume.yml` 每半小时查询待处理草稿，使用已保存的原包和原提交编号完成装订、检查和发布；该任务没有重新提交公证的命令，也可以手动运行。Apple 拒绝的构建保持草稿并报告错误。定时任务的实际启动时间可能受 GitHub 调度影响。

发布任务共用串行队列（最多排队 100 个），不会取消已经进行的发布；构建任务可并行执行。签名、公证沿用下方的 `release` 环境和 secrets，无需把个人 GitHub token 配置为发布凭据。发布任务使用 GitHub 自动提供的 `GITHUB_TOKEN`，仅在发布阶段授予 `contents: write` 和 `actions: read`。

发布逻辑位于 `.github/scripts/desktop_release.py`。本地回归检查：

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py' -v
```

## 手动打包

仓库的 `Signed desktop builds` 工作流位于独立的 `.github/workflows/signed-desktop-builds.yml`，通过 GitHub Actions 手动构建 Windows x64 和 macOS 通用版本，产物保存在该次运行的 Actions artifacts 中，不会自动创建或修改 GitHub Release。它不改变原有的 `.github/workflows/release.yml` 多平台发布流程。工作流需先存在于仓库默认分支，才会出现在 Actions 的手动运行菜单；运行时选择的可信分支或 tag 同时决定工作流代码和构建代码。

## 准备密钥

工作流使用仓库的 `release` 环境，并遵循该环境现有的保护规则。可在仓库 **Settings → Environments → release → Environment secrets** 中设置以下 secrets；仓库级或组织级 secrets 也可使用，但组织级 secrets 必须授权本仓库访问。不要将证书、私钥、密码或 token 提交到仓库，或放入构建产物。

| Secret | 内容 |
| --- | --- |
| `APPLE_CERTIFICATE` | 包含私钥的 **Developer ID Application** 证书 `.p12` 文件的 Base64 内容 |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码 |
| `APPLE_ID` | 用于公证的 Apple 账户邮箱；不公证时不需要 |
| `APPLE_PASSWORD` | 该 Apple 账户的应用专用密码，不是账户登录密码；不公证时不需要 |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `WINDOWS_CERTIFICATE` | 包含私钥的 Windows 代码签名 `.pfx` 文件的 Base64 内容 |
| `WINDOWS_CERTIFICATE_PASSWORD` | 导出 `.pfx` 时设置的密码 |
| `PRIVATE_REPO_TOKEN` | 仅当 `SSYCloud/ssy-sdk-rust` 为私有仓库时需要：有该仓库读取权限的最小权限 token |

Apple 开发者账号、App Store Connect API key 或普通开发证书本身不等同于可用于分发的 Developer ID Application 证书；`.p12` 必须包含对应私钥。此工作流使用 Apple ID、应用专用密码和 Team ID 公证，尚未使用 App Store Connect API key 公证。

在持有证书的安全电脑上，可将 `.p12` 或 `.pfx` 转为单行 Base64 后粘贴到对应 secret。例如 Windows PowerShell：

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('C:\secure\certificate.p12')) | Set-Clipboard
```

将路径替换为实际文件；复制后及时清空剪贴板。证书原件和密码应存放在受控的安全位置。GitHub secret 内容不能在保存后再次查看，换证或更换密码时需更新对应 secret。

### 创建 Windows 自签名证书

如尚无持久的代码签名证书，可在受控的 Windows 电脑上以 PowerShell 创建一次，并把导出的 PFX 安全备份。以下示例中的发布者名称和路径请按实际情况修改，目标目录应位于仓库外。密码在提示时输入，不要写进脚本或终端命令历史。

```powershell
$cert = New-SelfSignedCertificate -Type CodeSigningCert `
  -Subject 'CN=SSY-Switch' -CertStoreLocation 'Cert:\CurrentUser\My' `
  -KeyExportPolicy Exportable -HashAlgorithm SHA256
$password = Read-Host 'PFX password' -AsSecureString
Export-PfxCertificate -Cert $cert -FilePath 'C:\secure\ssy-switch-signing.pfx' -Password $password
Export-Certificate -Cert $cert -FilePath 'C:\secure\ssy-switch-signing.cer'
```

将 PFX 转为 Base64 填入 `WINDOWS_CERTIFICATE`，将导出密码填入 `WINDOWS_CERTIFICATE_PASSWORD`。保留并复用同一证书；更换证书会改变签名身份。`.cer` 只含公钥，可经可信渠道分发给需要验证的管理员，不能代替 PFX 签名。

**自签证书默认不受其他 Windows 设备信任。** 工作流在临时 runner 中添加的信任不会传递给用户；只有管理员在确认身份和证书指纹后，才能按其组织的证书管理策略部署公钥信任。自签名也不能消除 SmartScreen 提示，在启用 Smart App Control 或企业策略的设备上还可能被阻止。面向公众分发时，应考虑受信任 CA 的代码签名证书。参见 [Microsoft 的 SmartScreen 说明](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation)。

## 运行和检查

1. 在仓库 **Actions → Signed desktop builds → Run workflow** 中选择可信分支或 tag 并启动。`Notarize macOS packages` 默认开启；关闭后生成仅签名、不公证的 macOS 测试包。`Build Windows packages` 默认开启，关闭后只构建 macOS。不要对未经审查的代码运行持有签名 secrets 的工作流；所选 ref 必须已包含该工作流版本。
2. 在运行详情的 **Artifacts** 下载对应平台的安装包和 `SHA256SUMS.txt`。已公证 Mac 包的 artifact 名称为 `ssy-switch-macos-universal`；`ssy-switch-macos-universal-not-notarized` 是提前保存的原始签名包，不代表公证完成。如果 Apple 仍在处理，构建任务会结束并在 Summary 中注明 `In Progress`，此时按下方步骤查询和恢复。工作流不会将文件发布到 Release；如需发布，请先检查产物再由维护者操作。
3. 对照 `SHA256SUMS.txt` 验证下载文件的 SHA-256。Windows 可用 `Get-FileHash -Algorithm SHA256 <文件路径>`；macOS 可用 `shasum -a 256 <文件路径>`。
4. 已公证版本在 macOS 检查应用签名、公证及装订，例如对解压后的 `.app` 执行 `codesign --verify --deep --strict --verbose=2 <应用路径>`，并对 DMG 执行 `xcrun stapler validate <DMG路径>` 和 `spctl -a -t open --context context:primary-signature -vv <DMG路径>`。在 Windows 可用 `Get-AuthenticodeSignature <安装包路径>` 检查签名证书及状态；应与预先通过可信渠道核对的证书指纹比较。未导入自签证书的电脑可能报告证书链不受信任，即使文件签名存在。
5. 在干净的 Windows 和 macOS 测试设备上安装、启动并检查实际系统提示。真实签名、公证和安装验证需要有效 secrets 及对应平台；仅检查工作流语法不能证明发布包已签名或通过公证。

关闭公证时，macOS artifact 和文件名含 `not-notarized`。应用和 DMG 仍使用 Developer ID 签名并验证签名，但不提交 Apple 公证，也不进行装订或 Gatekeeper 通过检查。下载到其他 Mac 后，Gatekeeper 可能阻止打开；测试者可在确认来源后按系统提示到「系统设置 → 隐私与安全性」允许打开。仅签名测试包不应被当作已公证的正式分发包。

### 公证等待超时后的恢复

开启公证时，工作流先构建和签名，再保存原始 `.app` ZIP、DMG 和校验值，最后向 Apple 提交外层 DMG。Apple 会检查其中的已签名代码，因此不再先通过 Tauri 提交一次 app、再提交一次 DMG。提交时不等待处理结束，返回的提交 UUID 立即单独保存为 `macos-notary-submission`，并写入运行 Summary。

原始签名包和提交记录保留 90 天（受仓库允许的保留期限制）。随后最多等待 10 分钟；Apple 若仍在处理，任务保留待处理状态和原包，只在 `Accepted` 且装订、Gatekeeper 检查都通过后上传已公证包。GitHub 等待超时不会取消 Apple 的提交，不应因此重复构建或提交。

1. 运行 **macOS notarization status**。`submission_id` 留空会查询当前 Apple 团队的历史记录；填写 UUID 会查询该次提交，并在已完成时保存 Apple 的详细日志。此任务只查询，不上传应用。
2. 状态为 `Accepted` 后，运行 **Recover notarized macOS package**。填写原 `submission_id`，并将 `source_run_id` 设置为保存原始签名包的 GitHub Actions 运行编号。
3. 恢复任务会校验原包 SHA-256、Developer ID 签名，并将 Intel 和 Apple 芯片的代码签名指纹与该次 Apple 票据逐一比较。匹配后才进行装订和 Gatekeeper 检查，生成 `ssy-switch-macos-universal` 下载产物，全程不重新提交公证。旧版只提交 app ZIP 的记录仅恢复 app ZIP；提交 DMG 的记录还会验证和恢复 DMG。

提交 UUID 用于查询原来的公证任务，不能给任意新包使用。若更新代码、资源或构建产物导致签名指纹变化，恢复任务会停止；只有指纹仍与已通过的票据完全一致的包才可复用。Apple 工作流说明见 [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)。

来自 fork 的 pull request 默认无法使用仓库 secrets，因此不应把 PR 检查视为签名发布验证。参见 [GitHub Actions secrets 文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets)。
