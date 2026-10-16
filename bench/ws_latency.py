#!/usr/bin/env python3
"""测量 WS 推送延迟：HTTP 提交变更 → 收到 atom 广播事件的时间差。纯标准库 WS 客户端。"""
import socket, struct, os, sys, base64, json, time, urllib.request, urllib.error
from urllib.parse import urlparse

def http(method, url, token=None, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    if token: req.add_header("Authorization", "Bearer " + token)
    if data: req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"null")

class WS:
    def __init__(self, host, port, path):
        self.s = socket.create_connection((host, port), timeout=10)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (f"GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\n"
               f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
               f"Sec-WebSocket-Version: 13\r\n\r\n")
        self.s.sendall(req.encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(4096)
        assert b"101" in buf.split(b"\r\n")[0], buf
        self.buf = buf.split(b"\r\n\r\n", 1)[1]
    def send(self, text):
        payload = text.encode()
        mask = os.urandom(4)
        header = b"\x81"
        n = len(payload)
        if n < 126: header += bytes([0x80 | n])
        elif n < 65536: header += bytes([0x80 | 126]) + struct.pack("!H", n)
        else: header += bytes([0x80 | 127]) + struct.pack("!Q", n)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.s.sendall(header + mask + masked)
    def _readn(self, n):
        while len(self.buf) < n:
            chunk = self.s.recv(65536)
            if not chunk: raise EOFError
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out
    def recv(self):
        b1 = self._readn(2)[1] & 0x7F
        n = b1
        if b1 == 126: n = struct.unpack("!H", self._readn(2))[0]
        elif b1 == 127: n = struct.unpack("!Q", self._readn(8))[0]
        return self._readn(n).decode(errors="replace")

def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8710"
    u = urlparse(base)
    host, port = u.hostname, u.port or 80
    st, body = http("POST", base + "/api/documents", body={"doc_id": "perf-doc", "width": 512, "height": 512})
    assert st == 200, body
    token = body["token"]
    ws = WS(host, port, "/ws?doc=perf-doc&token=" + token)
    ws.s.settimeout(0.3)
    try:
        while True: ws.recv()
    except Exception: pass
    latencies = []
    for i in range(20):
        t0 = time.perf_counter()
        st, resp = http("POST", base + f"/api/tools/create_layer?doc=perf-doc", token,
                        body={"name": f"L{i}"})
        assert st == 200, resp
        ws.s.settimeout(5)
        while True:
            frame = ws.recv()
            try: msg = json.loads(frame)
            except Exception: continue
            if isinstance(msg, dict) and msg.get("type") == "event":
                latencies.append((time.perf_counter() - t0) * 1000)
                break
    latencies.sort()
    print(json.dumps({
        "n": len(latencies),
        "min_ms": round(latencies[0], 2),
        "median_ms": round(latencies[len(latencies)//2], 2),
        "p90_ms": round(latencies[int(len(latencies)*0.9)], 2),
        "max_ms": round(latencies[-1], 2),
    }))

if __name__ == "__main__":
    main()
