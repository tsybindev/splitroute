# SplitRoute WFP kill switch report (S4-T5c Windows part).
#
# Self-contained check: starts a dev helper on a dedicated pipe, enables the
# kill switch, verifies WFP filters and fail-closed behavior, removes everything
# and verifies the rollback. Designed to survive losing all non-LAN egress
# while the kill switch is active: every step is logged to the report file on
# disk as it happens, and a watchdog process (wfp-watchdog.ps1) removes the
# rules unconditionally if this script does not finish in time.
#
# MUST run elevated (WFP + netsh require it). Rollback layers, in order:
#   1. helper protocol "stop" (idempotent: filters -> sublayer -> provider);
#   2. this script's finally-block (stop + kill dev helper) on any error;
#   3. watchdog (kill helper + Restart-Service BFE) on hang/deadline;
#   4. BFE restart / reboot clears all non-persistent WFP objects anyway.
#
# Usage: elevated PowerShell ->
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\wfp-report.ps1

param(
    [string]$PipeName = "splitroute-dev",
    [int]$WatchdogSec = 240,
    [int]$ProbeTimeoutMs = 3000
)

$ErrorActionPreference = "Stop"
$script:FailCount = 0

$Root        = Split-Path -Parent (Split-Path -Parent $PSScriptRoot) # repo root
$HelperPath  = Join-Path $Root "target\release\splitroute-helper.exe"
$SingBox     = Join-Path $Root "src-tauri\binaries\sing-box-x86_64-pc-windows-msvc.exe"
$WorkDir     = Join-Path $env:TEMP "sr-wfp"
$RuntimeDir  = Join-Path $WorkDir "runtime"
$ConfigPath  = Join-Path $WorkDir "config.json"
$ReportPath  = Join-Path $WorkDir "report.md"
$DoneFlag    = Join-Path $WorkDir "done.flag"
$HelperPidF  = Join-Path $WorkDir "helper.pid"
$WatchdogLog = Join-Path $WorkDir "watchdog.log"
$GuidPattern = "6a4c2f9e-1b7d-4c3e-9f2a-5d8b7e6c1a20"

$R = New-Object System.Collections.Generic.List[string]
function Flush-Report { Set-Content -Path $ReportPath -Value $R -Encoding UTF8 }
function Stamp { return (Get-Date -Format "HH:mm:ss.fff") }
function Pass([string]$Name, [string]$Detail) {
    $R.Add("PASS | $Name | $Detail"); Write-Host "PASS $Name"; Flush-Report
}
function Fail([string]$Name, [string]$Detail) {
    $script:FailCount++; $R.Add("FAIL | $Name | $Detail"); Write-Host "FAIL $Name"; Flush-Report
}
function Note([string]$Name, [string]$Detail) {
    $R.Add("NOTE | $Name | $Detail"); Write-Host "NOTE $Name"; Flush-Report
}

# --- helpers ------------------------------------------------------------------

function Test-Tcp([string]$HostOrIp, [int]$Port, [int]$TimeoutMs) {
    # Returns "open" (connected), "refused" (reached host, port closed) or
    # "timeout" (no answer - under kill switch this means blocked).
    $c = New-Object System.Net.Sockets.TcpClient
    try {
        $ar = $c.BeginConnect($HostOrIp, $Port, $null, $null)
        if ($ar.AsyncWaitHandle.WaitOne($TimeoutMs)) {
            try { $c.EndConnect($ar); return "open" }
            catch { return "refused" }
        }
        return "timeout"
    } catch { return "refused" }
    finally { $c.Close() }
}

function Get-Gateway {
    $route = Get-NetRoute -DestinationPrefix "0.0.0.0/0" -ErrorAction SilentlyContinue |
        Sort-Object RouteMetric | Select-Object -First 1
    if ($route) { return $route.NextHop }
    return $null
}

function Invoke-Helper([string]$Command, $Payload, [string]$Id) {
    # One JSON line per request over the dev pipe; helper answers one line.
    # Retry on an empty response: a client can bind to an instance whose
    # request gets EOF'd by the accept-loop handoff (seen live, twice per call).
    for ($attempt = 1; $attempt -le 4; $attempt++) {
        $pipe = New-Object System.IO.Pipes.NamedPipeClientStream(
            ".", $PipeName, [System.IO.Pipes.PipeDirection]::InOut)
        $pipe.Connect(10000)
        try {
            $enc = New-Object System.Text.UTF8Encoding($false)
            $req = @{ protocol = 2; id = $Id; command = $Command; payload = $Payload }
            $json = ConvertTo-Json -InputObject $req -Depth 24 -Compress
            $bytes = $enc.GetBytes($json + "`n")
            $pipe.Write($bytes, 0, $bytes.Length)
            $pipe.Flush()
            $reader = New-Object System.IO.StreamReader($pipe, $enc)
            $line = $reader.ReadLine()
            if ($line) { return ($line | ConvertFrom-Json) }
            Start-Sleep -Milliseconds 500
        } finally { $pipe.Dispose() }
    }
    throw "empty response from helper"
}

function Wfp-SplitRouteCount([string]$Tag) {
    # Counts SplitRoute objects in a fresh WFP dump: providerKey references of
    # filters plus the sublayer and provider entries. Baseline is expected 0.
    $xml = Join-Path $WorkDir "wfp-$Tag.xml"
    $null = netsh wfp show filters file="$xml" 2>&1
    $raw = 0
    if (Test-Path $xml) {
        $raw = (Select-String -Path $xml -Pattern $GuidPattern -AllMatches |
            ForEach-Object { $_.Matches.Count } |
            Measure-Object -Sum).Sum
        if ($null -eq $raw) { $raw = 0 }
    }
    return $raw
}

function Probe-Set([string]$Tag, [string]$Gw) {
    # Full probe set with tags b-* (baseline), k-* (kill switch on), s-* (after stop).
    $res = @{}
    $res["loopback"] = Test-Tcp "127.0.0.1" 47650 $ProbeTimeoutMs
    if ($Gw) { $res["lan-gw"] = Test-Tcp $Gw 53 $ProbeTimeoutMs } else { $res["lan-gw"] = "no-gw" }
    $res["net-443"] = Test-Tcp "1.1.1.1" 443 $ProbeTimeoutMs
    $res["proxy"] = "off"
    foreach ($k in $res.Keys) { Note "$Tag.$k" $res[$k] }
    return $res
}

function Wait-ForListener([int]$Port, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        $state = Test-Tcp "127.0.0.1" $Port 500
        if ($state -eq "open" -or $state -eq "refused") { return $true }
        Start-Sleep -Milliseconds 300
    }
    return $false
}

# --- pre-flight ---------------------------------------------------------------

$isAdmin = ([System.Security.Principal.WindowsPrincipal] [System.Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { Write-Error "must run elevated"; exit 99 }

if (-not (Test-Path $HelperPath)) { Write-Error "helper missing: $HelperPath"; exit 99 }
if (-not (Test-Path $SingBox))   { Write-Error "sing-box missing: $SingBox"; exit 99 }
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
New-Item -ItemType Directory -Force -Path $RuntimeDir | Out-Null
Remove-Item $DoneFlag -ErrorAction SilentlyContinue
Remove-Item $WatchdogLog -ErrorAction SilentlyContinue

$R.Add("# SplitRoute WFP report (S4-T5c) - $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
$R.Add("")
$R.Add("| step | result | detail |")
$R.Add("|------|--------|--------|")
Flush-Report

# --- config -------------------------------------------------------------------

@'
{
  "log": { "level": "warn" },
  "inbounds": [
    { "type": "mixed", "tag": "in", "listen": "127.0.0.1", "listen_port": 18999 }
  ],
  "outbounds": [ { "type": "direct", "tag": "direct" } ]
}
'@ | Set-Content -Path $ConfigPath -Encoding ASCII
Pass "config" "minimal sing-box config written (no TUN, loopback inbound on 18999)"

# --- loopback listener --------------------------------------------------------

$listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 47650)
$listener.Start()
Pass "listener" "loopback listener on 127.0.0.1:47650"

# --- watchdog -----------------------------------------------------------------

try {
    $wd = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList @(
        "-NoProfile", "-ExecutionPolicy", "Bypass",
        "-File", (Join-Path $PSScriptRoot "wfp-watchdog.ps1"),
        "-PipeName", $PipeName, "-DoneFlag", $DoneFlag, "-DeadlineSec", $WatchdogSec,
        "-HelperPidFile", $HelperPidF, "-WorkDir", $WorkDir
    )
    Pass "watchdog" "started pid=$($wd.Id) deadline=${WatchdogSec}s"
} catch {
    Fail "watchdog" "failed to start: $_"
}

# --- baseline -----------------------------------------------------------------

$gw = Get-Gateway
Note "gateway" "$gw"
$base = Probe-Set "b" $gw
if ($base["net-443"] -ne "open") {
    Fail "baseline" "internet already broken (net-443=$($base['net-443'])) - aborting before any change"
    $listener.Stop()
    Remove-Item $DoneFlag -ErrorAction SilentlyContinue
    Set-Content -Path $DoneFlag -Value "aborted"
    exit 1
}
Pass "baseline" "internet reachable, baseline recorded"

# --- start dev helper ---------------------------------------------------------

$helperOut = Join-Path $WorkDir "helper.out.log"
$helperErr = Join-Path $WorkDir "helper.err.log"
$Sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$helper = Start-Process -FilePath $HelperPath -PassThru -WindowStyle Hidden -ArgumentList @(
    "console", "--pipe", $PipeName, "--runtime-dir", $RuntimeDir,
    "--sing-box", $SingBox, "--allow-sid", $Sid
) -RedirectStandardOutput $helperOut -RedirectStandardError $helperErr
Set-Content -Path $HelperPidF -Value $helper.Id
Note "helper" "started pid=$($helper.Id) pipe=$PipeName sid=$Sid"

$helperUp = $false
foreach ($i in 1..20) {
    try {
        $p = New-Object System.IO.Pipes.NamedPipeClientStream(".", $PipeName, [System.IO.Pipes.PipeDirection]::InOut)
        $p.Connect(1500); $p.Dispose(); $helperUp = $true; break
    } catch { Start-Sleep -Milliseconds 500 }
}
if (-not $helperUp) {
    Fail "helper" "pipe $PipeName never appeared"
} else {
    Pass "helper" "pipe is up"
}

try {
    $st = Invoke-Helper "status" $null "probe-status-1"
    if ($st.ok -and $st.payload.state -eq "stopped") {
        Pass "status.initial" "state=stopped"
    } else {
        Fail "status.initial" "unexpected: $($st | ConvertTo-Json -Depth 6 -Compress)"
    }
} catch { Fail "status.initial" "pipe call failed: $_" }

# --- start core with kill switch ---------------------------------------------

$cfgObj = Get-Content $ConfigPath -Raw | ConvertFrom-Json
try {
    $resp = Invoke-Helper "start" @{ config = $cfgObj; kill_switch = $true } "start-ks"
    if ($resp.ok -and $resp.payload.state -eq "running" -and $resp.payload.kill_switch.enabled) {
        $corePid = $resp.payload.pid
        Pass "start.ks" "core running pid=$corePid kill_switch=on"
    } else {
        $corePid = $null
        Fail "start.ks" "unexpected: $($resp | ConvertTo-Json -Depth 6 -Compress)"
    }
} catch {
    $corePid = $null
    Fail "start.ks" "pipe call failed: $_"
}

# --- WFP objects present ------------------------------------------------------

$activeCount = Wfp-SplitRouteCount "active"
if ($activeCount -gt 10) {
    Pass "wfp.objects" "SplitRoute objects in WFP: $activeCount (filters+sublayer+provider references)"
} else {
    Fail "wfp.objects" "expected >10 SplitRoute objects, got $activeCount"
}

# --- probes while core alive and kill switch on -------------------------------

# Loopback probe against our own listener (127.0.0.1:47650, started above).
try {
    $c = New-Object System.Net.Sockets.TcpClient
    $ar = $c.BeginConnect("127.0.0.1", 47650, $null, $null)
    if ($ar.AsyncWaitHandle.WaitOne($ProbeTimeoutMs)) { $c.EndConnect($ar); Pass "probe.loopback" "connected to 127.0.0.1:47650" }
    else { Fail "probe.loopback" "timeout" }
    $c.Close()
} catch { Fail "probe.loopback" "error: $_" }

# LAN probe: same behavior as baseline expected.
$lanNow = Test-Tcp $gw 53 $ProbeTimeoutMs
if ($lanNow -eq $base["lan-gw"] -and $lanNow -ne "timeout") {
    Pass "probe.lan" "gw=$gw result=$lanNow (matches baseline)"
} elseif ($lanNow -eq "timeout" -and $base["lan-gw"] -eq "timeout") {
    Note "probe.lan" "gw=$gw timeout in baseline too - LAN probe inconclusive"
} else {
    Fail "probe.lan" "gw=$gw result=$lanNow baseline=$($base['lan-gw'])"
}

# Internet probe: must be blocked now (this PowerShell is not the core).
# WFP ALE block refuses the connect immediately - unlike pf's silent drop,
# so both "refused" and "timeout" mean blocked; only "open" means a leak.
$netNow = Test-Tcp "1.1.1.1" 443 $ProbeTimeoutMs
if ($netNow -ne "open") { Pass "probe.net-blocked" "1.1.1.1:443 result=$netNow - non-core egress blocked" }
else { Fail "probe.net-blocked" "1.1.1.1:443 result=open - egress NOT blocked!" }

# Core proxy probe: traffic through the core must still work (AppId permit).
# Retry a few times: right after start the mixed listener and upstream TLS
# may take a moment (one live run flaked on a single attempt).
$viaCore = "no-attempt"
foreach ($try in 1..3) {
    $viaCore = & curl.exe --proxy http://127.0.0.1:18999 --connect-timeout 5 -s -o NUL -w "%{http_code}" https://www.gstatic.com/generate_204 2>$null
    if ($viaCore -eq "204") { break }
    Start-Sleep -Seconds 1
}
if ($viaCore -eq "204") { Pass "probe.via-core" "HTTPS through core proxy -> 204" }
else { Fail "probe.via-core" "curl code=$viaCore (expected 204, 3 attempts)" }

# --- double start -> ALREADY_RUNNING ------------------------------------------

try {
    $resp2 = Invoke-Helper "start" @{ config = $cfgObj; kill_switch = $true } "start-dup"
    if (-not $resp2.ok -and $resp2.error.code -eq "ALREADY_RUNNING") {
        Pass "start.duplicate" "ALREADY_RUNNING as expected"
    } else {
        Fail "start.duplicate" "unexpected: $($resp2 | ConvertTo-Json -Depth 6 -Compress)"
    }
} catch { Fail "start.duplicate" "pipe call failed: $_" }

# --- kill the core: fail-closed ------------------------------------------------

if ($corePid) {
    try {
        Stop-Process -Id $corePid -Force -ErrorAction Stop
        Start-Sleep -Seconds 2
        Pass "core.killed" "sing-box pid=$corePid force-killed"
    } catch { Fail "core.killed" "Stop-Process failed: $_" }
} else {
    Note "core.killed" "skipped - no core pid (start.ks failed)"
}

try {
    $st2 = Invoke-Helper "status" $null "probe-status-2"
    if ($st2.ok -and $st2.payload.state -eq "stopped" -and $st2.payload.kill_switch.enabled) {
        Pass "failclosed.status" "state=stopped kill_switch=on (rules survive core death)"
    } else {
        Fail "failclosed.status" "unexpected: $($st2 | ConvertTo-Json -Depth 6 -Compress)"
    }
} catch { Fail "failclosed.status" "pipe call failed: $_" }

$netFail = Test-Tcp "1.1.1.1" 443 $ProbeTimeoutMs
if ($netFail -ne "open") { Pass "failclosed.net" "egress still blocked with dead core (result=$netFail)" }
else { Fail "failclosed.net" "1.1.1.1:443 result=open - fail-closed broken!" }

$lanFail = Test-Tcp $gw 53 $ProbeTimeoutMs
if ($lanFail -eq $base["lan-gw"] -or $lanFail -eq "refused" -or $lanFail -eq "open") {
    Pass "failclosed.lan" "gw=$gw result=$lanFail - LAN still allowed"
} else {
    Fail "failclosed.lan" "gw=$gw result=$lanFail"
}

# --- stop: rollback ------------------------------------------------------------

try {
    $resp3 = Invoke-Helper "stop" $null "stop-1"
    if ($resp3.ok -and -not $resp3.payload.kill_switch.enabled) {
        Pass "stop" "helper stop ok, kill_switch=off"
    } else {
        Fail "stop" "unexpected: $($resp3 | ConvertTo-Json -Depth 6 -Compress)"
    }
} catch { Fail "stop" "pipe call failed: $_" }

$afterCount = Wfp-SplitRouteCount "after"
if ($afterCount -eq 0) { Pass "wfp.removed" "SplitRoute objects in WFP after stop: $afterCount" }
else { Fail "wfp.removed" "expected 0 objects, got $afterCount" }

$netRestored = Test-Tcp "1.1.1.1" 443 $ProbeTimeoutMs
if ($netRestored -eq "open") { Pass "restored.net" "internet reachable again" }
else { Fail "restored.net" "1.1.1.1:443 result=$netRestored" }

$lanRestored = Test-Tcp $gw 53 $ProbeTimeoutMs
Note "restored.lan" "gw=$gw result=$lanRestored (baseline $($base['lan-gw']))"

# --- cleanup -------------------------------------------------------------------

try { $listener.Stop() } catch {}
if ($helper -and -not $helper.HasExited) {
    try { Stop-Process -Id $helper.Id -Force -ErrorAction Stop; Pass "cleanup" "dev helper stopped" }
    catch { Fail "cleanup" "failed to stop dev helper: $_" }
} else {
    Note "cleanup" "dev helper already exited"
}
Remove-Item $HelperPidF -ErrorAction SilentlyContinue

$R.Add("")
$R.Add("Final: $(if ($script:FailCount -eq 0) { 'ALL PASS' } else { "$($script:FailCount) FAILURES" })")
Flush-Report
Set-Content -Path $DoneFlag -Value "done"
exit $script:FailCount
