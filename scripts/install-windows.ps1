# Install for the current user and start in the tray at sign-in.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$installRoot = Join-Path $env:LOCALAPPDATA 'minimalist-list'
$binary = [System.IO.Path]::GetFullPath((Join-Path $installRoot 'bin/minimalist-list.exe'))

# Windows cannot replace a running executable, and closing the window only hides it.
$processes = Get-Process minimalist-list -ErrorAction SilentlyContinue
$runningIds = $processes | Where-Object Path -eq $binary | Select-Object -ExpandProperty Id
if (!$runningIds -and ($processes | Where-Object { !$_.Path })) {
  # A 32-bit PowerShell host cannot read Get-Process.Path for a 64-bit process.
  $runningIds = Get-CimInstance -ClassName Win32_Process -Filter "Name = 'minimalist-list.exe'" -ErrorAction SilentlyContinue |
    Where-Object ExecutablePath -eq $binary | Select-Object -ExpandProperty ProcessId
}
if ($runningIds) {
  throw "Minimalist List is still running from $binary (process $($runningIds -join ', ')). Choose Quit from its tray icon menu, then run this installer again."
}

cargo install --locked --path $repository --root $installRoot --force
if ($LASTEXITCODE -ne 0) { throw 'The application could not be installed.' }

$programs = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs'
$startup = Join-Path $programs 'Startup'
New-Item -ItemType Directory -Force -Path $startup | Out-Null
$shell = New-Object -ComObject WScript.Shell

$launcher = $shell.CreateShortcut((Join-Path $programs 'Minimalist List.lnk'))
$launcher.TargetPath = $binary
$launcher.WorkingDirectory = $installRoot
$launcher.IconLocation = "$binary,0"
$launcher.Save()

$login = $shell.CreateShortcut((Join-Path $startup 'Minimalist List.lnk'))
$login.TargetPath = $binary
$login.Arguments = '--start-hidden'
$login.WorkingDirectory = $installRoot
$login.IconLocation = "$binary,0"
$login.Save()

Write-Output 'Installed Minimalist List. Open it from Start; it starts in the tray at your next sign-in.'
