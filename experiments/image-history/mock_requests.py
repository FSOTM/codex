#!/usr/bin/env python3
"""Exercise the compiled CLI against localhost, without account credentials."""

import base64
import hashlib
import http.server
import json
import os
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile
import threading
import zlib


def png(seed):
    def chunk(kind, content):
        return (
            struct.pack(">I", len(content))
            + kind
            + content
            + struct.pack(">I", zlib.crc32(kind + content))
        )

    rng = random.Random(seed)
    pixels = b"".join(b"\0" + rng.randbytes(256 * 3) for _ in range(256))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 256, 256, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(pixels))
        + chunk(b"IEND", b"")
    )


def image_urls(value):
    if isinstance(value, dict):
        for child in value.values():
            yield from image_urls(child)
    elif isinstance(value, list):
        for child in value:
            yield from image_urls(child)
    elif isinstance(value, str) and value.startswith("data:image/"):
        yield value


def run(binary, directory, patched):
    home = directory / "home"
    home.mkdir()
    workspace = directory / "workspace"
    workspace.mkdir()
    images = [workspace / "first.png", workspace / "second.png"]
    for seed, path in enumerate(images):
        path.write_bytes(png(seed))
    requests = []
    errors = []
    reread_bytes = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            try:
                raw = self.rfile.read(int(self.headers["Content-Length"]))
                if self.headers.get("Content-Encoding") == "zstd":
                    import zstandard

                    raw = zstandard.ZstdDecompressor().decompress(
                        raw, max_output_size=100_000_000
                    )
                body = json.loads(raw)
                requests.append(body)
                # On the first request, make the CLI reread an older image using its real tool.
                if len(requests) == 1:
                    archived = list((home / "image-history-cache").glob("*.png"))
                    target = archived[0] if patched else images[0]
                    reread_bytes.append(target.read_bytes())
                    item = {
                        "type": "function_call",
                        "call_id": "reread-test",
                        "name": "view_image",
                        "arguments": json.dumps({"path": str(target)}),
                    }
                else:
                    item = {
                        "type": "message",
                        "role": "assistant",
                        "id": "msg-test",
                        "content": [
                            {"type": "output_text", "text": "Local mock completed."}
                        ],
                    }
                events = [
                    {
                        "type": "response.created",
                        "response": {"id": f"resp-{len(requests)}"},
                    },
                    {"type": "response.output_item.done", "item": item},
                    {
                        "type": "response.completed",
                        "response": {
                            "id": f"resp-{len(requests)}",
                            "usage": {
                                "input_tokens": 10,
                                "output_tokens": 1,
                                "total_tokens": 11,
                            },
                        },
                    },
                ]
                response = "".join(
                    f"data: {json.dumps(event)}\n\n" for event in events
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)
            except Exception as error:
                errors.append(repr(error))
                self.send_error(500)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    (home / "config.toml").write_text(f"""model = "gpt-5.4"
model_provider = "lab"
approval_policy = "never"
sandbox_mode = "read-only"
[features]
code_mode = false
[model_providers.lab]
name = "Local experiment mock"
base_url = "http://127.0.0.1:{server.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
""")
    env = os.environ.copy()
    env.pop("CODEX_LAB_IMAGE_BUDGET_BYTES", None)
    env.pop("OPENAI_API_KEY", None)
    env.pop("OPENAI_BASE_URL", None)
    env["CODEX_HOME"] = str(home)
    if patched:
        env["CODEX_LAB_IMAGE_BUDGET_BYTES"] = "300000"
    command = [binary, "exec", "--json", "--skip-git-repo-check"]
    try:
        first = subprocess.run(
            command
            + [
                "-i",
                str(images[0]),
                "-i",
                str(images[1]),
                "--",
                "Inspect these test images.",
            ],
            cwd=workspace,
            env=env,
            text=True,
            capture_output=True,
            timeout=120,
        )
        if first.returncode:
            raise RuntimeError(first.stderr + first.stdout)
        events = [
            json.loads(line)
            for line in first.stdout.splitlines()
            if line.startswith("{")
        ]
        session_id = next(
            event["thread_id"]
            for event in events
            if event.get("type") == "thread.started"
        )
        resumed = subprocess.run(
            command + ["resume", session_id, "Continue the test."],
            cwd=workspace,
            env=env,
            text=True,
            capture_output=True,
            timeout=120,
        )
        if resumed.returncode:
            raise RuntimeError(resumed.stderr + resumed.stdout)
        assert not errors, errors
        assert len(requests) == 3, (
            f"Expected initial, tool follow-up, and resume requests; got {len(requests)}"
        )
        payloads = [list(image_urls(request["input"])) for request in requests]
        sizes = [sum(map(len, urls)) for urls in payloads]
        assert len(payloads[0]) == (1 if patched else 2), sizes
        if patched:
            assert all(size <= 300000 for size in sizes), sizes
            assert any(
                hashlib.sha1(base64.b64decode(url.split(",", 1)[1])).digest()
                == hashlib.sha1(reread_bytes[0]).digest()
                for url in payloads[1]
            ), "Reread image missing"
            assert "omitted from this request" in json.dumps(requests[0]["input"])
        else:
            assert sizes[1] > sizes[0], sizes
        return {
            "image_bytes_per_request": sizes,
            "image_count_per_request": list(map(len, payloads)),
            "request_json_bytes": [
                len(json.dumps(request).encode()) for request in requests
            ],
        }
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    binary = str(Path(sys.argv[1]).resolve())
    test_parent = Path.home() / ".cache" / "codex-image-lab-tests"
    test_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=test_parent) as name:
        root = Path(name)
        result = {}
        for mode in ("baseline", "patched"):
            directory = root / mode
            directory.mkdir()
            result[mode] = run(binary, directory, mode == "patched")
        Path("/tmp/image-history-lab-results.json").write_text(
            json.dumps(result, indent=2)
        )
        print(json.dumps(result, indent=2))
