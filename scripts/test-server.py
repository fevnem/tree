#!/usr/bin/env python3
"""Local range-capable HTTP server used to test Tree's engine end to end.

Serves one generated file of N MiB, honours `Range: bytes=a-b`, and reports
Content-Disposition like a real download endpoint.

    python3 scripts/test-server.py [port] [mib] [mib_per_second]
"""

import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
SIZE = (int(sys.argv[2]) if len(sys.argv) > 2 else 24) * 1024 * 1024
# Optional throttle in MiB/s so a local test can be observed in progress.
RATE = (int(sys.argv[3]) if len(sys.argv) > 3 else 0) * 1024 * 1024
# Optional slow zone: ranges starting below this offset are throttled to 1/8,
# which makes one connection lag so adaptive stealing can be observed.
import os
SLOW_ZONE = int(os.environ.get("SLOW_ZONE_MB", "0")) * 1024 * 1024
BLOB = bytes((i * 37 + 11) & 0xFF for i in range(1 << 20)) * (SIZE >> 20)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def _name(self):
        # /f/<name> serves the same blob under any filename, so demos and
        # screenshots can show realistic names without a different fixture.
        if self.path.startswith("/f/"):
            name = self.path[3:].split("?")[0]
            if name and "/" not in name:
                return name
        return "tree-test.bin"

    def _headers(self, code, length, extra=None):
        self.send_response(code)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(length))
        self.send_header("Content-Disposition", 'attachment; filename="%s"' % self._name())
        for key, value in (extra or {}).items():
            self.send_header(key, value)
        self.end_headers()

    def do_HEAD(self):
        self._headers(200, SIZE)

    def do_GET(self):
        start = 0
        rng = self.headers.get("Range")
        if rng:
            m = re.match(r"bytes=(\d+)-(\d*)$", rng)
            if not m:
                self.send_error(416)
                return
            start = int(m.group(1))
            end = int(m.group(2)) if m.group(2) else SIZE - 1
            end = min(end, SIZE - 1)
            body = BLOB[start : end + 1]
            self._headers(
                206, len(body), {"Content-Range": f"bytes {start}-{end}/{SIZE}"}
            )
        else:
            body = BLOB
            self._headers(200, SIZE)
        try:
            rate = RATE
            if SLOW_ZONE and start < SLOW_ZONE:
                rate = max(1, RATE // 8)
            if rate:
                deadline = time.monotonic()
                step = 64 * 1024
                for i in range(0, len(body), step):
                    self.wfile.write(body[i : i + step])
                    deadline += step / rate
                    delay = deadline - time.monotonic()
                    if delay > 0:
                        time.sleep(delay)
            else:
                self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass


if __name__ == "__main__":
    print(f"serving {SIZE} bytes on http://127.0.0.1:{PORT}/tree-test.bin", flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
