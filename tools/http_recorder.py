#!/usr/bin/env python3
"""A stand-in for the game servers: records every request and answers from a table.

    tools/http_recorder.py <port> <log.jsonl> [answers.json]

The harness's CADENCE_HTTP_REDIRECT=http://127.0.0.1:<port> sends the real game's requests here
instead of to the servers, so its account and upload actions can be recorded without reaching any
real account. Each request is one JSON line (method, path, headers, body). `answers.json` maps an
endpoint's file name (`uploadGJComment21.php`) to the text to answer; anything else gets "-1".
Cadence's own requests can be pointed here too (CADENCE_SERVER=http://127.0.0.1:<port>/database).
"""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port, log = int(sys.argv[1]), sys.argv[2]
answers = json.load(open(sys.argv[3])) if len(sys.argv) > 3 else {}


class Handler(BaseHTTPRequestHandler):
    def handle_one(self, body):
        with open(log, "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "headers": dict(self.headers), "body": body.decode("utf-8", "replace")}) + "\n")
        answer = answers.get(self.path.rsplit("/", 1)[-1].split("?")[0], "-1").encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        self.wfile.write(answer)

    def do_POST(self):
        self.handle_one(self.rfile.read(int(self.headers.get("Content-Length", 0))))

    def do_GET(self):
        self.handle_one(b"")

    def log_message(self, *_):
        pass


ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
