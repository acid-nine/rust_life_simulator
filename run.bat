@echo off
rem Build the wasm and serve the page. Browsers block wasm/ES modules on file://, so it must be served over HTTP.
cd /d "%~dp0"
wasm-pack build --target web || exit /b 1
start "" http://localhost:8000/index.html
python -m http.server 8000
