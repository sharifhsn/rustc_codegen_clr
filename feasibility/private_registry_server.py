#!/usr/bin/env python3
"""Minimal authenticated sparse-registry server for hermetic cargo-dotnet acceptance.

It intentionally records only request classes, never headers or token values.  Cargo receives
the token through its ordinary credential provider; the test can then prove the token never
appears in cargo-dotnet logs or receipts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--crate", required=True, type=Path)
    parser.add_argument("--crate-name", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--token-env", required=True)
    parser.add_argument("--port-file", required=True, type=Path)
    parser.add_argument("--events", required=True, type=Path)
    args = parser.parse_args()

    token = os.environ[args.token_env]
    archive = args.crate.read_bytes()
    checksum = hashlib.sha256(archive).hexdigest()
    event_file = args.events

    class RegistryHandler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, _format: str, *_args: object) -> None:
            # BaseHTTPRequestHandler logs request paths to stderr.  Keep all acceptance logs
            # token-safe even if Cargo makes an unexpected request.
            return

        def event(self, name: str) -> None:
            with event_file.open("a", encoding="utf-8") as events:
                events.write(name + "\n")

        def response(self, status: int, body: bytes, content_type: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)

        def authorized(self) -> bool:
            # Cargo's token provider sends `Authorization: <token>` for a registry.
            return self.headers.get("Authorization") == token

        def do_GET(self) -> None:  # noqa: N802 - required BaseHTTPRequestHandler name.
            path = urlparse(self.path).path
            if path == "/config.json":
                self.event("config")
                host, port = self.server.server_address[:2]
                body = json.dumps(
                    {
                        "dl": f"http://{host}:{port}/api/v1/crates/{{crate}}/{{version}}/download",
                        "auth-required": True,
                    }
                ).encode()
                self.response(200, body, "application/json")
                return

            if not self.authorized():
                self.event("unauthorized")
                self.response(401, b"authentication required\n", "text/plain")
                return

            if path.startswith("/api/v1/crates/") and path.endswith("/download"):
                self.event("download-authorized")
                self.response(200, archive, "application/octet-stream")
                return

            # Sparse Cargo asks for the crate-name index path.  The exact shard path is
            # intentionally not duplicated here so this fixture stays valid across Cargo versions.
            self.event("index-authorized")
            body = (
                json.dumps(
                    {
                        "name": args.crate_name,
                        "vers": args.version,
                        "deps": [],
                        "cksum": checksum,
                        "features": {},
                        "yanked": False,
                        "links": None,
                    }
                )
                + "\n"
            ).encode()
            self.response(200, body, "text/plain")

    server = ThreadingHTTPServer(("127.0.0.1", 0), RegistryHandler)
    host, port = server.server_address[:2]
    args.port_file.write_text(str(port), encoding="utf-8")
    server.serve_forever()


if __name__ == "__main__":
    main()
