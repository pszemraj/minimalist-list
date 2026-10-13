# Install for the current user and start in the tray at sign-in.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$installRoot = Join-Path $env:LOCALAPPDATA 'minimalist-list'

cargo install --locked --path $repository --root $installRoot --force
if ($LASTEXITCODE -ne 0) { throw 'The application could not be installed.' }

$binary = Join-Path $installRoot 'bin/minimalist-list.exe'
$programs = Join-Path $env:APPDATA 'Microsoft/Windows/Start Menu/Programs'
$startup = Join-Path $programs 'Startup'
New-Item -ItemType Directory -Force -Path $startup | Out-Null
$shell = New-Object -ComObject WScript.Shell

$launcher = $shell.CreateShortcut((Join-Path $programs 'Minimalist List.lnk'))
$launcher.TargetPath = $binary
$launcher.WorkingDirectory = $installRoot
$launcher.Save()

$login = $shell.CreateShortcut((Join-Path $startup 'Minimalist List.lnk'))
$login.TargetPath = $binary
$login.Arguments = '--start-hidden'
$login.WorkingDirectory = $installRoot
$login.Save()

Write-Output 'Installed Minimalist List. Open it from Start; it starts in the tray at your next sign-in.'
