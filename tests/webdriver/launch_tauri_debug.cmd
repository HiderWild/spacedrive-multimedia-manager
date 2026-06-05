@echo off
setlocal

set ROOT_DIR=%~dp0..\..
set VSCMD=C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat
if exist "%VSCMD%" call "%VSCMD%" -arch=x64 -host_arch=x64

set PATH=%USERPROFILE%\.cargo\bin;%PATH%
set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
cd /d "%ROOT_DIR%\apps\tauri"
cargo tauri dev --no-watch
