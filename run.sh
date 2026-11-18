#!/bin/sh
# Build the wasm and serve the page. Browsers block wasm/ES modules on file://, so it must be served over HTTP.
cd "$(dirname "$0")"
wasm-pack build --target web || exit 1
xdg-open "http://localhost:8000/index.html" &
python3 -m http.server 8000
