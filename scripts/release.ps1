<#
.SYNOPSIS
  发布一个版本：打安装包，在 GitHub 上建 v<版本号> 的 Release，并把安装包传上去。
  版本号取根目录 Cargo.toml 的 workspace.package.version，发布前先改它（以及 tauri.conf.json 的 version）。
  需要已登录的 gh（gh auth login），工作树要干净（改动先提交并推送）。

.EXAMPLE
  .\scripts\release.ps1                # 发布正式版
  .\scripts\release.ps1 -Draft         # 先建草稿，在网页上检查后再发布
#>
param([switch]$Draft)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
  if (git status --porcelain) { throw '工作树有未提交的改动，先提交并推送。' }
  $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
  $tauri = (Get-Content app\src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version
  if ($version -ne $tauri) { throw "版本号不一致：Cargo.toml $version，tauri.conf.json $tauri" }
  $tag = "v$version"
  if (git tag --list $tag) { throw "标签 $tag 已存在，先改版本号。" }

  & (Join-Path $PSScriptRoot 'build-installer.ps1')
  $setup = Get-Item "target\release\bundle\nsis\LConnect Lite_${version}_x64-setup.exe"
  # GitHub 上的文件名不能有空格，传一份改名的。
  $asset = Join-Path $env:TEMP "LConnect-Lite_${version}_x64-setup.exe"
  Copy-Item $setup.FullName $asset -Force

  git tag -a $tag -m "LConnect Lite $version"
  git push origin $tag
  $ghArgs = @('release', 'create', $tag, $asset, '--title', "LConnect Lite $version", '--generate-notes')
  if ($Draft) { $ghArgs += '--draft' }
  gh @ghArgs
} finally {
  Pop-Location
}
