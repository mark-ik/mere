#!/usr/bin/env python3
# Copyright 2026 Mark AB (markik)
# SPDX-License-Identifier: MPL-2.0
"""Receives the page's receipt JSON and exported database bytes over POST.

Runs beside the static server (which only serves): the page calls
`window.munimentOpfsProbe.postReceipt(name)` and `postExport(name)`, which
POST to `http://127.0.0.1:8734/receipt?name=…` (written under `receipts/`)
and `/file?name=…` (written under `fixtures/`). CORS is open because the page
is served from another port on the same loopback.
"""

import http.server
import json
import re
import sys
from pathlib import Path
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parent
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8734
NAME = re.compile(r"[A-Za-z0-9._-]+")
TARGETS = {"/receipt": ROOT / "receipts", "/file": ROOT / "fixtures"}


class Sink(http.server.BaseHTTPRequestHandler):
    def _cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors()
        self.end_headers()

    def do_POST(self):
        url = urlparse(self.path)
        name = parse_qs(url.query).get("name", [""])[0]
        if url.path not in TARGETS or not NAME.fullmatch(name):
            self.send_response(400)
            self._cors()
            self.end_headers()
            return
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if url.path == "/receipt":
            body = json.dumps(json.loads(body), indent=2).encode()
        target = TARGETS[url.path] / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(body)
        answer = json.dumps({"saved": str(target), "bytes": len(body)}).encode()
        self.send_response(200)
        self._cors()
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        self.wfile.write(answer)

    def log_message(self, *_):
        pass


if __name__ == "__main__":
    print(f"receipt sink: http://127.0.0.1:{PORT}/receipt?name=<file>.json")
    http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Sink).serve_forever()
