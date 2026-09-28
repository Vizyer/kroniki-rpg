param(
    [Parameter(Mandatory=$true)][string]$Version,
    [ValidateSet('stable','preview')][string]$Channel = 'stable',
    [string]$Repository = 'OWNER/REPOSITORY',
    [string]$GodotVersion = '4.7.2'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Build = Join-Path $Root 'build'
$App = Join-Path $Build 'app'
$Tools = Join-Path $Root '.tools'
New-Item -ItemType Directory -Force -Path $Build,$App,$Tools | Out-Null

function Write-Step([string]$Text) { Write-Host "`n==> $Text" -ForegroundColor Cyan }
function Ensure-Godot {
    if ($env:GODOT_BIN -and (Test-Path $env:GODOT_BIN)) { return (Resolve-Path $env:GODOT_BIN).Path }
    $Tag = "$GodotVersion-stable"
    $ToolDir = Join-Path $Tools "godot-$Tag"
    $Exe = Join-Path $ToolDir "Godot_v${Tag}_win64.exe"
    if (!(Test-Path $Exe)) {
        Write-Step "Pobieranie Godot $Tag"
        New-Item -ItemType Directory -Force -Path $ToolDir | Out-Null
        $Zip = Join-Path $Tools "godot-$Tag.zip"
        Invoke-WebRequest -UseBasicParsing "https://github.com/godotengine/godot/releases/download/$Tag/Godot_v${Tag}_win64.exe.zip" -OutFile $Zip
        Expand-Archive -Force $Zip $ToolDir
    }
    return $Exe
}
function Ensure-Templates {
    $Tag = "$GodotVersion-stable"
    $TemplateDir = Join-Path $env:APPDATA "Godot\export_templates\$GodotVersion.stable"
    if (Test-Path (Join-Path $TemplateDir 'windows_release_x86_64.exe')) { return }
    Write-Step "Pobieranie Godot Export Templates $Tag"
    $Tpz = Join-Path $Tools "templates-$Tag.tpz"
    Invoke-WebRequest -UseBasicParsing "https://github.com/godotengine/godot/releases/download/$Tag/Godot_v${Tag}_export_templates.tpz" -OutFile $Tpz
    $Temp = Join-Path $Tools "templates-$Tag-extracted"
    if (Test-Path $Temp) { Remove-Item -Recurse -Force $Temp }
    New-Item -ItemType Directory -Force -Path $Temp | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Tpz,$Temp)
    New-Item -ItemType Directory -Force -Path $TemplateDir | Out-Null
    $Source = Join-Path $Temp 'templates'
    Copy-Item -Recurse -Force (Join-Path $Source '*') $TemplateDir
}

Write-Step "Czyszczenie katalogu wydania"
if (Test-Path $App) { Remove-Item -Recurse -Force $App }
New-Item -ItemType Directory -Force -Path $App | Out-Null

Write-Step "Build Rust Core"
& cargo build --manifest-path (Join-Path $Root 'rust-core\Cargo.toml') --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build Rust Core failed' }
Copy-Item (Join-Path $Root 'rust-core\target\release\kroniki_core.exe') (Join-Path $App 'kroniki_core.exe') -Force

Write-Step "Pakowanie lokalnego runtime MGAI (llama.cpp Vulkan)"
$AiDir = Join-Path $App 'ai'
New-Item -ItemType Directory -Force -Path $AiDir | Out-Null
$LlamaRelease = Invoke-RestMethod -UseBasicParsing -Headers @{ 'User-Agent'='KronikiRPG-Build' } 'https://api.github.com/repos/ggml-org/llama.cpp/releases/latest'
$LlamaAsset = $LlamaRelease.assets | Where-Object { $_.name -like '*bin-win-vulkan-x64.zip' } | Select-Object -First 1
if (!$LlamaAsset) { throw 'Nie znaleziono Windows Vulkan llama.cpp w najnowszym release.' }
$LlamaZip = Join-Path $Tools $LlamaAsset.name
if (!(Test-Path $LlamaZip)) { Invoke-WebRequest -UseBasicParsing -Headers @{ 'User-Agent'='KronikiRPG-Build' } $LlamaAsset.browser_download_url -OutFile $LlamaZip }
$LlamaTemp = Join-Path $Tools 'llama-win-vulkan'
if (Test-Path $LlamaTemp) { Remove-Item -Recurse -Force $LlamaTemp }
Expand-Archive -Force $LlamaZip $LlamaTemp
$LlamaServer = Get-ChildItem -Path $LlamaTemp -Filter 'llama-server.exe' -Recurse | Select-Object -First 1
if (!$LlamaServer) { throw 'Paczka llama.cpp nie zawiera llama-server.exe.' }
Copy-Item -Force -Recurse (Join-Path $LlamaServer.Directory.FullName '*') $AiDir
if (!(Test-Path (Join-Path $AiDir 'llama-server.exe'))) { throw 'Nie udało się spakować llama-server.exe.' }
@{
    profile='Qwen3-8B Q5_K_M'
    model_file='Qwen3-8B-Q5_K_M.gguf'
    model_url='https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q5_K_M.gguf?download=true'
    recommended_ram_gb=16
    recommended_vram_gb=8
    context=12288
    backend='llama.cpp Vulkan'
} | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $AiDir 'local-ai-profile.json')

Write-Step "Build Launchera"
& cargo build --manifest-path (Join-Path $Root 'launcher\Cargo.toml') --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build launcher failed' }
Copy-Item (Join-Path $Root 'launcher\target\release\kroniki_launcher.exe') (Join-Path $Build 'KronikiLauncher.exe') -Force

$Godot = Ensure-Godot
Ensure-Templates
Write-Step "Eksport Godot -> KronikiRPG.exe"
Push-Location (Join-Path $Root 'godot')
& $Godot --headless --editor --quit
Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Godot)) -ErrorAction SilentlyContinue | Wait-Process
if ($LASTEXITCODE -ne 0) { throw 'Godot import failed' }
& $Godot --headless --export-release 'Windows Desktop' (Join-Path $App 'KronikiRPG.exe')
Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Godot)) -ErrorAction SilentlyContinue | Wait-Process
if ($LASTEXITCODE -ne 0) { throw 'Godot export failed' }
Pop-Location
if (!(Test-Path (Join-Path $App 'KronikiRPG.exe'))) { throw 'Godot export finished without KronikiRPG.exe' }

@{ version=$Version; channel=$Channel } | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $App 'version.json')
@{ repository=$Repository; manifest_asset='update-manifest.json'; package_asset='KronikiRPG-win-x64.zip' } | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Build 'launcher-config.json')

Write-Step "Tworzenie paczki aktualizacji"
$Package = Join-Path $Build 'KronikiRPG-win-x64.zip'
if (Test-Path $Package) { Remove-Item -Force $Package }
Compress-Archive -Path (Join-Path $App '*') -DestinationPath $Package -CompressionLevel Optimal
$Hash = (Get-FileHash -Algorithm SHA256 $Package).Hash.ToLowerInvariant()
$Size = (Get-Item $Package).Length
$Tag = "v$Version"
$Manifest = [ordered]@{
    version = $Version
    channel = $Channel
    package_url = "https://github.com/$Repository/releases/download/$Tag/KronikiRPG-win-x64.zip"
    sha256 = $Hash
    package_size = $Size
    notes = "Kroniki RPG $Version ($Channel)"
    min_launcher_version = '0.2.0'
}
$Manifest | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Build 'update-manifest.json')

Write-Step "Budowanie instalatora (jeśli Inno Setup jest dostępny)"
$IsccCandidates = @()
if (${env:ProgramFiles(x86)}) { $IsccCandidates += (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe') }
if ($env:ProgramFiles) { $IsccCandidates += (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe') }
$IsccFromPath = $null
try { $cmd = Get-Command ISCC.exe -ErrorAction Stop; if ($cmd -and $cmd.Source) { $IsccFromPath = $cmd.Source } } catch {}
$Iscc = @($IsccFromPath) + $IsccCandidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (!$Iscc) { throw 'Inno Setup compiler not found. Cannot produce Setup.exe.' }
& $Iscc "/DMyAppVersion=$Version" "/DBuildDir=$Build" (Join-Path $Root 'installer\KronikiRPG.iss')
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
if (!(Test-Path (Join-Path $Build 'KronikiRPG-Setup.exe'))) { throw 'Inno Setup finished but Setup.exe was not created.' }

Write-Step "Gotowe"
Get-ChildItem $Build | Select-Object Name,Length | Format-Table -AutoSize

