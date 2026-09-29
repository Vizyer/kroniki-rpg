param(
    [Parameter(Mandatory=$true)][string]$Version,
    [Parameter(Mandatory=$true)][string]$SourceSha
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$build = Join-Path $root 'build'
$manifest = Get-Content (Join-Path $build 'update-manifest.json') -Raw | ConvertFrom-Json
$zip = Join-Path $build 'KronikiRPG-win-x64.zip'
if ($manifest.version -ne $Version -or $manifest.source_sha -ne $SourceSha) { throw 'Wrong manifest provenance' }
if ($manifest.sha256 -ne (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Package hash mismatch' }
if ($manifest.package_size -ne (Get-Item $zip).Length) { throw 'Package size mismatch' }

$install = Join-Path $env:RUNNER_TEMP 'KronikiRPG-Update-Smoke'
# Isolated save folder: never delete a developer's real saves when running this script.
$previousLocalAppData = $env:LOCALAPPDATA
$env:LOCALAPPDATA = Join-Path $env:RUNNER_TEMP 'KronikiRPG-Smoke-Data'
$script:coreProcess = $null
$portBusy = $false
try { $null = Invoke-WebRequest 'http://127.0.0.1:17377/health' -TimeoutSec 1; $portBusy = $true } catch {}
if ($portBusy) { $env:LOCALAPPDATA = $previousLocalAppData; throw 'Port 17377 is already in use; close the game before running smoke tests' }
function Start-Core {
    $core = Join-Path $install 'app/kroniki_core.exe'
    $script:coreProcess = Start-Process $core -WorkingDirectory (Split-Path $core) -PassThru
    for ($i = 0; $i -lt 80; $i++) {
        if ($script:coreProcess.HasExited) { throw 'Core exited during startup' }
        try {
            $h = Invoke-RestMethod 'http://127.0.0.1:17377/health' -TimeoutSec 1
            if ($h.ok) { return $h }
        } catch {}
        Start-Sleep -Milliseconds 250
    }
    throw 'Core startup timed out'
}
function Stop-Core {
    if ($null -eq $script:coreProcess -or $script:coreProcess.HasExited) { return }
    Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/shutdown' -ContentType 'application/json' -Body '{}' | Out-Null
    if (!$script:coreProcess.WaitForExit(10000)) { throw 'Core did not shut down cleanly' }
}
function Assert-Save([long]$id, [string]$marker) {
    $loaded = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/save/load' -ContentType 'application/json' -Body (@{id=$id}|ConvertTo-Json)
    if (!$loaded.ok -or $loaded.state.schema -ne 10 -or $loaded.state.character.name -ne $marker) { throw 'Save content was not preserved' }
    if ($loaded.state.campaign.turns -ne 1 -or $loaded.state.recent_turns.Count -ne 1) { throw 'Campaign memory was not preserved' }
}
try {
    if (Test-Path $install) { Remove-Item $install -Recurse -Force }
    if (Test-Path $env:LOCALAPPDATA) { Remove-Item $env:LOCALAPPDATA -Recurse -Force }
    New-Item -ItemType Directory -Path $env:LOCALAPPDATA -Force | Out-Null
    $p = Start-Process (Join-Path $build 'KronikiRPG-Setup.exe') -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',("/DIR=`"$install`"")) -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw 'Installer failed' }
    foreach ($name in @('KronikiLauncher.exe','launcher-config.json','app/KronikiRPG.exe','app/kroniki_core.exe','app/ai-runtime/llama-server.exe')) {
        if (!(Test-Path (Join-Path $install $name))) { throw "Installer missing $name" }
    }
    $cfg = Get-Content (Join-Path $install 'launcher-config.json') -Raw | ConvertFrom-Json
    if ($cfg.repository -ne $env:GITHUB_REPOSITORY -or $cfg.channel -ne 'preview') { throw 'Wrong updater repository/channel' }
    Write-Host 'Installer completed. Checking Core and save persistence.'
    $h = Start-Core
    if ($h.core -ne '0.10.0' -or !$h.local_ai.runtime_present) { throw 'Wrong Core or missing local runtime' }
    $marker = "update-smoke-$env:GITHUB_RUN_ID"
    $null = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/character/create' -ContentType 'application/json' -Body (@{mode='manual';name=$marker}|ConvertTo-Json)
    $null = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/ai/config' -ContentType 'application/json' -Body '{"mode":"off"}'
    $campaign = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/campaign/new' -ContentType 'application/json' -Body (@{character_name=$marker}|ConvertTo-Json)
    if (!$campaign.ok -or $campaign.state.campaign.threads.Count -ne 1) { throw 'Campaign creation failed' }
    $turn = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/action' -ContentType 'application/json' -Body '{"text":"observe","mode":"freeform","request_id":"smoke-turn"}'
    if (!$turn.ok -or $turn.state.campaign.turns -ne 1) { throw 'Campaign turn failed' }
    $saved = Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/save' -ContentType 'application/json' -Body (@{name=$marker}|ConvertTo-Json)
    if (!$saved.ok -or !$saved.id) { throw 'Save failed' }
    Assert-Save $saved.id $marker
    Stop-Core
    $null = Start-Core
    $restored = Invoke-RestMethod 'http://127.0.0.1:17377/state'
    if ($restored.campaign.turns -ne 1) { throw 'Autosave was not restored on startup' }
    Assert-Save $saved.id $marker
    Stop-Core

    $stage = Join-Path $install '.staging'
    Expand-Archive $zip $stage -Force
    foreach ($name in @('KronikiRPG.exe','kroniki_core.exe','ai-runtime/llama-server.exe')) {
        if (!(Test-Path (Join-Path $stage $name))) { throw "Update missing $name" }
    }
    $v = Get-Content (Join-Path $stage 'version.json') -Raw | ConvertFrom-Json
    if ($v.version -ne $Version -or $v.source_sha -ne $SourceSha -or $v.channel -ne 'preview') { throw 'Package/manifest disagreement' }
    Move-Item (Join-Path $install 'app') (Join-Path $install '.rollback')
    Move-Item $stage (Join-Path $install 'app')
    $null = Start-Core
    $restored = Invoke-RestMethod 'http://127.0.0.1:17377/state'
    if ($restored.campaign.turns -ne 1) { throw 'Autosave was not restored on startup' }
    Assert-Save $saved.id $marker
    Stop-Core
    # Ensure returning to the previous binaries also preserves the save.
    Remove-Item (Join-Path $install 'app') -Recurse -Force
    Move-Item (Join-Path $install '.rollback') (Join-Path $install 'app')
    $null = Start-Core
    $restored = Invoke-RestMethod 'http://127.0.0.1:17377/state'
    if ($restored.campaign.turns -ne 1) { throw 'Autosave was not restored on startup' }
    Assert-Save $saved.id $marker
    Stop-Core
    Write-Host 'Save survived restart, update and rollback. Starting Godot boot test.'
    # Start-Process -Wait waits for the entire descendant tree on Windows.
    # Core is a separate server; wait for the game itself, then shut Core down.
    $game = Start-Process (Join-Path $install 'app/KronikiRPG.exe') -ArgumentList @('--headless','--quit-after','2') -PassThru
    if (!$game.WaitForExit(30000)) {
        Stop-Process -Id $game.Id -Force
        throw 'Godot boot timed out'
    }
    $game.Refresh()
    if ($game.ExitCode -ne 0) { throw 'Godot boot failed' }
} finally {
    try { Invoke-RestMethod -Method Post 'http://127.0.0.1:17377/shutdown' -ContentType 'application/json' -Body '{}' -TimeoutSec 3 | Out-Null } catch {}
    if ($null -ne $script:coreProcess -and !$script:coreProcess.HasExited) { Stop-Process -Id $script:coreProcess.Id -Force }
    $env:LOCALAPPDATA = $previousLocalAppData
}
