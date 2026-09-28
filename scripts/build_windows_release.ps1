param(
    [string]$Version = '0.10.0-preview.1',
    [string]$Channel = 'preview',
    [string]$Repository = 'Vizyer/kroniki-rpg',
    [string]$GodotVersion = '4.5.1',
    [string]$LlamaVersion = 'b11228'
)

$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Build = Join-Path $Root 'build'
$App = Join-Path $Build 'app'
$Temp = Join-Path $Build '.temp'

function Step([string]$Text) {
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}
function Download([string]$Url,[string]$Out) {
    if (!(Test-Path $Out)) {
        Write-Host "Download: $Url"
        Invoke-WebRequest -Uri $Url -OutFile $Out -UseBasicParsing
    }
}

Step 'Clean build'
if (Test-Path $Build) { Remove-Item $Build -Recurse -Force }
New-Item -ItemType Directory -Force -Path $App,$Temp | Out-Null

Step 'Build Rust Core'
cargo build --release --manifest-path (Join-Path $Root 'rust-core\Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Rust Core build failed' }
Copy-Item (Join-Path $Root 'rust-core\target\release\kroniki_core.exe') (Join-Path $App 'kroniki_core.exe')

Step 'Build launcher'
cargo build --release --manifest-path (Join-Path $Root 'launcher\Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Launcher build failed' }
Copy-Item (Join-Path $Root 'launcher\target\release\kroniki_launcher.exe') (Join-Path $Build 'KronikiLauncher.exe')

Step 'Prepare Godot'
$GodotTag = "$GodotVersion-stable"
$GodotZip = Join-Path $Temp 'godot.zip'
$GodotUrl = "https://github.com/godotengine/godot/releases/download/$GodotTag/Godot_v$($GodotTag)_win64.exe.zip"
Download $GodotUrl $GodotZip
Expand-Archive -Path $GodotZip -DestinationPath (Join-Path $Temp 'godot-bin') -Force
$Godot = Get-ChildItem (Join-Path $Temp 'godot-bin') -Filter 'Godot*.exe' | Select-Object -First 1 -ExpandProperty FullName
if (!$Godot) { throw 'Godot executable not found' }

$TemplatesZip = Join-Path $Temp 'templates.zip'
$TemplatesUrl = "https://github.com/godotengine/godot/releases/download/$GodotTag/Godot_v$($GodotTag)_export_templates.tpz"
Download $TemplatesUrl $TemplatesZip
$TemplateDir = Join-Path $env:APPDATA "Godot\export_templates\$GodotVersion.stable"
New-Item -ItemType Directory -Force -Path $TemplateDir | Out-Null
$TemplateTmp = Join-Path $Temp 'templates'
Expand-Archive -Path $TemplatesZip -DestinationPath $TemplateTmp -Force
$TemplateSource = Join-Path $TemplateTmp 'templates'
if (!(Test-Path $TemplateSource)) { $TemplateSource = $TemplateTmp }
Copy-Item (Join-Path $TemplateSource '*') $TemplateDir -Recurse -Force

Step 'Import and export Godot game'
$Project = Join-Path $Root 'godot'
$p = Start-Process -FilePath $Godot -WorkingDirectory $Project -ArgumentList @('--headless','--editor','--quit') -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Godot import failed: $($p.ExitCode)" }

$GameExe = Join-Path $App 'KronikiRPG.exe'
$p = Start-Process -FilePath $Godot -WorkingDirectory $Project -ArgumentList @('--headless','--export-release','Windows Desktop',$GameExe) -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Godot export failed: $($p.ExitCode)" }
if (!(Test-Path $GameExe)) { throw 'Godot export did not create KronikiRPG.exe' }

Step 'Bundle llama.cpp Vulkan runtime'
$LlamaZip = Join-Path $Temp 'llama-vulkan.zip'
$LlamaUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$LlamaVersion/llama-$LlamaVersion-bin-win-vulkan-x64.zip"
Download $LlamaUrl $LlamaZip
$LlamaExtract = Join-Path $Temp 'llama-vulkan'
Expand-Archive -Path $LlamaZip -DestinationPath $LlamaExtract -Force
$Runtime = Join-Path $App 'ai-runtime'
New-Item -ItemType Directory -Force -Path $Runtime | Out-Null
Copy-Item (Join-Path $LlamaExtract '*') $Runtime -Recurse -Force
if (!(Test-Path (Join-Path $Runtime 'llama-server.exe'))) {
    $server = Get-ChildItem $LlamaExtract -Filter 'llama-server.exe' -Recurse | Select-Object -First 1
    if (!$server) { throw 'llama-server.exe not found in llama.cpp release' }
    Copy-Item (Join-Path $server.Directory.FullName '*') $Runtime -Recurse -Force
}

Step 'Write runtime manifests'
@{
    version = $Version
    channel = $Channel
    core = '0.10.0'
    save_schema = 10
    ai_profile = 'Qwen3-8B-Q5_K_M'
} | ConvertTo-Json | Set-Content (Join-Path $App 'version.json') -Encoding utf8

@{
    repository = $Repository
    channel = $Channel
} | ConvertTo-Json | Set-Content (Join-Path $Build 'launcher-config.json') -Encoding utf8

Copy-Item (Join-Path $Root 'AI_MODEL_PROFILE.json') (Join-Path $App 'AI_MODEL_PROFILE.json')

Step 'Create update ZIP'
$Zip = Join-Path $Build 'KronikiRPG-win-x64.zip'
Compress-Archive -Path (Join-Path $App '*') -DestinationPath $Zip -CompressionLevel Optimal
$Sha = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLowerInvariant()
$PackageUrl = "https://github.com/$Repository/releases/download/v$Version/KronikiRPG-win-x64.zip"
@{
    version = $Version
    channel = $Channel
    package_url = $PackageUrl
    url = $PackageUrl
    download_url = $PackageUrl
    sha256 = $Sha
    save_schema = 10
} | ConvertTo-Json | Set-Content (Join-Path $Build 'update-manifest.json') -Encoding utf8

Step 'Build Inno Setup installer'
$Iscc = $null
try { $Iscc = (Get-Command ISCC.exe -ErrorAction Stop).Source } catch {}
if (!$Iscc) {
    $candidates = @()
    $pf86 = [Environment]::GetFolderPath('ProgramFilesX86')
    if ($pf86) { $candidates += (Join-Path $pf86 'Inno Setup 6\ISCC.exe') }
    if ($env:ProgramFiles) { $candidates += (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe') }
    $Iscc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (!$Iscc) { throw 'Inno Setup compiler not found' }

& $Iscc "/DMyAppVersion=$Version" "/DBuildDir=$Build" (Join-Path $Root 'installer\KronikiRPG.iss')
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
if (!(Test-Path (Join-Path $Build 'KronikiRPG-Setup.exe'))) { throw 'Installer output missing' }

Step 'Done'
Get-ChildItem $Build -Recurse | Select-Object FullName,Length
