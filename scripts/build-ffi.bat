@echo off
setlocal EnableExtensions

set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "REPO_ROOT=%%~fI"

set "DIST_DIR=%REPO_ROOT%\dist\ratex-ffi"
set "INCLUDE_DIR=%DIST_DIR%\inc\ratex"
set "LIB_DIR=%DIST_DIR%\libs"
set "LIB_SOURCE_DIR=%REPO_ROOT%\target\release"

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo not found in PATH 1>&2
    exit /b 1
)

if not exist "%INCLUDE_DIR%" mkdir "%INCLUDE_DIR%"
if not exist "%LIB_DIR%" mkdir "%LIB_DIR%"

cargo build --manifest-path "%REPO_ROOT%\Cargo.toml" --release -p ratex-ffi

copy /Y "%REPO_ROOT%\include\ratex_base.h" "%INCLUDE_DIR%\ratex_base.h" >nul
copy /Y "%REPO_ROOT%\include\ratex_svg.h" "%INCLUDE_DIR%\ratex_svg.h" >nul
copy /Y "%REPO_ROOT%\include\ratex_pdf.h" "%INCLUDE_DIR%\ratex_pdf.h" >nul
copy /Y "%REPO_ROOT%\crates\ratex-ffi\include\ratex.h" "%INCLUDE_DIR%\ratex.h" >nul
copy /Y "%LIB_SOURCE_DIR%\ratex_ffi.dll" "%LIB_DIR%\ratex_ffi.dll" >nul
copy /Y "%LIB_SOURCE_DIR%\ratex_ffi.dll.lib" "%LIB_DIR%\ratex_ffi.lib" >nul

dir /b /s "%DIST_DIR%"
