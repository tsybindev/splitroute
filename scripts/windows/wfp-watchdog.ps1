# SplitRoute WFP watchdog - emergency rollback for wfp-report.ps1.
#
# Started (hidden, elevated) by the report script before any change. Normally
# it just polls the done flag and exits. If the report script hangs or dies
# while the kill switch is active, this process removes the rules:
#   1. helper protocol "stop" on the dev pipe (best effort);
#   2. kill the dev helper process by pid file;
#   3. Restart-Service BFE - non-persistent WFP objects die with the engine,
#      the Windows Firewall service reloads its persistent rules after that.
# Only the dev helper (pid file) is ever killed - the installed service and
# its process are not touched.

param(
    [string]$PipeName = "splitroute-dev",
    [string]$DoneFlag,
    [int]$DeadlineSec = 240,
    [string]$HelperPidFile,
    [string]$WorkDir
)

$log = Join-Path $WorkDir "watchdog.log"
function Log([string]$m) {
    Add-Content -Path $log -Value "$(Get-Date -Format 'HH:mm:ss.fff') $m"
}

$start = Get-Date
Log "watchdog started (pid $PID), deadline ${DeadlineSec}s, pipe=$PipeName"

while (((Get-Date) - $start).TotalSeconds -lt $DeadlineSec) {
    if (Test-Path $DoneFlag) { Log "done flag present - clean exit"; exit 0 }
    Start-Sleep -Seconds 2
}

Log "DEADLINE HIT - emergency rollback begins"

# 1) protocol stop on the dev pipe
try {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream(
        ".", $PipeName, [System.IO.Pipes.PipeDirection]::InOut)
    $pipe.Connect(3000)
    $enc = New-Object System.Text.UTF8Encoding($false)
    $json = '{"protocol":2,"id":"wd-stop","command":"stop","payload":null}'
    $b = $enc.GetBytes($json + "`n")
    $pipe.Write($b, 0, $b.Length)
    $pipe.Flush()
    $r = New-Object System.IO.StreamReader($pipe, $enc)
    $resp = $r.ReadLine()
    Log "pipe stop response: $resp"
    $pipe.Dispose()
} catch { Log "pipe stop failed: $_" }

# 2) kill the dev helper (pid file only - never the installed service)
if (Test-Path $HelperPidFile) {
    $hp = Get-Content $HelperPidFile -ErrorAction SilentlyContinue
    if ($hp) {
        try {
            Stop-Process -Id $hp -Force -ErrorAction Stop
            Log "dev helper pid $hp killed"
        } catch { Log "kill helper failed: $_" }
    }
} else {
    Log "no helper pid file - nothing to kill"
}

# 3) BFE restart as the last resort: clears every non-persistent WFP object,
#    including ours. Windows Firewall reloads its persistent rules on start.
try {
    Log "restarting BFE"
    Restart-Service BFE -Force
    Start-Service mpssvc -ErrorAction SilentlyContinue
    Log "BFE restarted, mpssvc state: $((Get-Service mpssvc).Status)"
} catch { Log "BFE restart failed: $_" }

Log "emergency rollback finished"
exit 2
