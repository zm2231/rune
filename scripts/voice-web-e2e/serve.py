import http.server
import sys
import time

root = sys.argv[1]


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def log_message(self, format, *args):
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('content-length', 0))).decode()
        for line in body.splitlines():
            print(f'{time.time():.3f} {line}', flush=True)
        self.send_response(204)
        self.end_headers()


Handler.extensions_map.update({'.wasm': 'application/wasm', '.js': 'text/javascript'})
http.server.ThreadingHTTPServer(('127.0.0.1', 8000), Handler).serve_forever()
