"""Shows the hub's window in an ordinary browser with made-up data, to check layout, states and
dark mode without the program. Not part of the program.

  python tools/ui-preview/serve.py      then open http://localhost:8765/
"""
import http.server
import socketserver
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
UI = ROOT / "ui"
MOCK = Path(__file__).resolve().parent / "mock.js"


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(UI), **k)

    def do_GET(self):
        if self.path in ("/", "/index.html") or self.path.startswith("/?"):
            html = (UI / "index.html").read_text(encoding="utf-8").replace('<script type="module" src="js/main.js">', '<script src="/__mock.js"></script><script type="module" src="js/main.js">')
            return self.send(html.encode(), "text/html")
        if self.path == "/__mock.js":
            return self.send(MOCK.read_bytes(), "text/javascript")
        return super().do_GET()

    def send(self, body, kind):
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)


socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("127.0.0.1", 8765), Handler) as s:
    print("http://localhost:8765/")
    s.serve_forever()
