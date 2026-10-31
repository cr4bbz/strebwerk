# Strebwerk als MCP-Server in Claude Desktop einrichten (Windows).
#
# Aufruf im Repo-Ordner:
#   powershell -ExecutionPolicy Bypass -File tools\mcp-einrichten.ps1
#
# Schritte: neuesten Stand holen, Release bauen, Selbsttest ueber das Protokoll,
# Eintrag "strebwerk" in claude_desktop_config.json ergaenzen (mit Sicherungskopie,
# andere Server bleiben unangetastet). Danach Claude Desktop ganz beenden und neu starten.

param(
    [string]$Repo = (Split-Path -Parent $PSScriptRoot),
    [string]$Scenes = (Join-Path $env:USERPROFILE 'Documents\strebwerk-szenen'),
    [string]$Branch = 'schnitte-und-interaktionen',
    [switch]$NoPull
)

$ErrorActionPreference = 'Stop'
function Step($t) { Write-Host "`n== $t" -ForegroundColor Cyan }
function Fail($t) { Write-Host "FEHLER: $t" -ForegroundColor Red; exit 1 }

if (-not (Test-Path (Join-Path $Repo 'Cargo.toml'))) {
    # Skript liegt nicht im Repo (z. B. aus dem Chat heruntergeladen): bekannten Ort versuchen
    $Repo = Join-Path $env:USERPROFILE 'Downloads\strebwerk\strebwerk'
}
if (-not (Test-Path (Join-Path $Repo 'Cargo.toml'))) { Fail "Kein Strebwerk-Repo gefunden - bitte -Repo <Ordner> angeben" }
Set-Location $Repo

# 1. Neuester Stand -----------------------------------------------------------
if (-not $NoPull) {
    Step "Git: $Branch holen"
    if (git status --porcelain --untracked-files=no) {
        git status --short
        Fail "Es gibt lokale Aenderungen. Bitte committen oder 'git stash', dann erneut starten (oder -NoPull)."
    }
    git fetch origin
    if ($LASTEXITCODE -ne 0) { Fail 'git fetch fehlgeschlagen' }
    git checkout $Branch
    if ($LASTEXITCODE -ne 0) { Fail "git checkout $Branch fehlgeschlagen" }
    git pull --ff-only origin $Branch
    if ($LASTEXITCODE -ne 0) { Fail 'git pull fehlgeschlagen' }
}
if (-not (Test-Path 'src\mcp.rs')) { Fail 'src\mcp.rs fehlt - der Stand enthaelt den MCP-Server noch nicht.' }

# 2. Bauen ----------------------------------------------------------------------
Step 'cargo build --release'
$running = Get-Process strebwerk -ErrorAction SilentlyContinue
if ($running) { Fail 'strebwerk.exe laeuft noch (GUI oder MCP) - bitte schliessen, sonst kann die Datei nicht ersetzt werden.' }
cargo build --release
if ($LASTEXITCODE -ne 0) { Fail 'Build fehlgeschlagen' }
$exe = (Resolve-Path 'target\release\strebwerk.exe').Path

# 3. Selbsttest -----------------------------------------------------------------
Step 'Selbsttest ueber das Protokoll'
New-Item -ItemType Directory -Force $Scenes | Out-Null
$msgs = @(
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"selbsttest","version":"0"}}}',
    '{"jsonrpc":"2.0","method":"notifications/initialized"}',
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}',
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"evaluate","arguments":{"expr":"rho^2-1"}}}'
)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.Arguments = "--mcp --root `"$Scenes`""
$psi.UseShellExecute = $false
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
$p = [System.Diagnostics.Process]::Start($psi)
foreach ($m in $msgs) { $p.StandardInput.WriteLine($m) }
$p.StandardInput.Close()
$out = $p.StandardOutput.ReadToEnd()
$err = $p.StandardError.ReadToEnd()
$p.WaitForExit()
$lines = @($out -split "`n" | Where-Object { $_.Trim() })
if ($lines.Count -ne 3) { Write-Host $out; Write-Host $err; Fail "Erwartet 3 Antworten, erhalten $($lines.Count)" }
$tools = ($lines[1] | ConvertFrom-Json).result.tools.Count
$text = ($lines[2] | ConvertFrom-Json).result.content[0].text
Write-Host "Server antwortet: $tools Werkzeuge"
Write-Host "evaluate(rho^2-1): $($text.Split("`n")[0])"
if ($tools -lt 20 -or $text -notmatch '\^\(-1\)') { Fail 'Selbsttest: unerwartete Antwort' }

# 4. Konfiguration --------------------------------------------------------------
Step 'Claude-Desktop-Konfiguration'
$cands = @(Join-Path $env:APPDATA 'Claude\claude_desktop_config.json')
$store = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Packages') -Directory -Filter 'Claude_*' -ErrorAction SilentlyContinue
foreach ($s in $store) { $cands += (Join-Path $s.FullName 'LocalCache\Roaming\Claude\claude_desktop_config.json') }
# bevorzugt die Datei, in der schon andere Server (z. B. logos-table) stehen
$cfg = $cands | Where-Object { (Test-Path $_) -and ((Get-Content $_ -Raw) -match 'mcpServers') } | Select-Object -First 1
if (-not $cfg) { $cfg = $cands | Where-Object { Test-Path $_ } | Select-Object -First 1 }
if (-not $cfg) { $cfg = $cands[0] }
Write-Host "Datei: $cfg"

if (Test-Path $cfg) {
    $raw = Get-Content $cfg -Raw -Encoding UTF8
    $bak = "$cfg.vor-strebwerk.bak"
    Copy-Item $cfg $bak -Force
    Write-Host "Sicherung: $bak"
    if ($raw.Trim()) { $json = $raw | ConvertFrom-Json } else { $json = New-Object PSObject }
} else {
    New-Item -ItemType Directory -Force (Split-Path $cfg) | Out-Null
    $json = New-Object PSObject
}
if (-not $json.PSObject.Properties['mcpServers']) {
    $json | Add-Member -NotePropertyName mcpServers -NotePropertyValue (New-Object PSObject)
}
$entry = [pscustomobject]@{ command = $exe; args = @('--mcp', '--root', $Scenes) }
if ($json.mcpServers.PSObject.Properties['strebwerk']) {
    $json.mcpServers.strebwerk = $entry
} else {
    $json.mcpServers | Add-Member -NotePropertyName strebwerk -NotePropertyValue $entry
}
# Windows PowerShell 5.1 schreibt Arrays sonst manchmal als {"value":[...],"Count":n}
Remove-TypeData System.Array -ErrorAction SilentlyContinue
$text = $json | ConvertTo-Json -Depth 64
# ohne BOM schreiben - JSON.parse in Claude Desktop vertraegt kein BOM
[System.IO.File]::WriteAllText($cfg, $text, (New-Object System.Text.UTF8Encoding $false))
$null = Get-Content $cfg -Raw | ConvertFrom-Json   # nochmal parsen = Kontrolle
Write-Host 'Eingetragene Server:' ((Get-Content $cfg -Raw | ConvertFrom-Json).mcpServers.PSObject.Properties.Name -join ', ')

Step 'Fertig'
Write-Host "Szenenordner: $Scenes"
Write-Host 'Jetzt Claude Desktop GANZ beenden (Infobereich der Taskleiste -> Beenden) und neu starten.' -ForegroundColor Green
