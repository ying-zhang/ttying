import http.client, socket, base64, os, struct, subprocess, time, json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EXE = ROOT / 'dist' / ('ttying-windows-x86_64.exe' if os.name == 'nt' else 'ttying-linux-x86_64')
EXE = str(Path(os.environ.get('TTYING_TEST_EXE', DEFAULT_EXE)))
TOKEN = os.environ.get('TTYING_TEST_TOKEN', 'smoke-token-1234567890')
PORT = int(os.environ.get('TTYING_TEST_PORT', '17801'))
COMMAND = json.loads(os.environ.get('TTYING_TEST_COMMAND', json.dumps(['cmd.exe'] if os.name == 'nt' else ['/bin/sh'])))
SRV_OUTPUT, SRV_SESSION, SRV_PONG, SRV_ERROR = 0x30, 0x31, 0x32, 0x33

def terminate_tree(p):
    if os.name == 'nt':
        subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        p.terminate()
    try: p.wait(timeout=5)
    except Exception: pass

def start(extra):
    p = subprocess.Popen([EXE, '--port', str(PORT), '--token', TOKEN] + extra + ['--'] + COMMAND,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(50):
        try:
            s = socket.create_connection(('127.0.0.1', PORT), timeout=.2)
            s.close()
            return p
        except OSError:
            time.sleep(.1)
    raise RuntimeError('no listen')

def post(token=None, cookie=None):
    c = http.client.HTTPConnection('127.0.0.1', PORT, timeout=5)
    h = {}
    if token: h['Authorization'] = 'Bearer ' + token
    if cookie: h['Cookie'] = cookie
    c.request('POST', '/api/session', headers=h)
    r = c.getresponse(); r.read(); c.close()
    return r.status, r.getheader('set-cookie')

def ws(cookie, origin):
    s = socket.create_connection(('127.0.0.1', PORT), timeout=10)
    k = base64.b64encode(os.urandom(16)).decode()
    s.sendall((f'GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n'
               f'Upgrade: websocket\r\nConnection: Upgrade\r\n'
               f'Sec-WebSocket-Key: {k}\r\nSec-WebSocket-Version: 13\r\n'
               f'Cookie: {cookie}\r\nOrigin: {origin}\r\n\r\n').encode())
    resp = b''
    while b'\r\n\r\n' not in resp:
        resp += s.recv(4096)
    return s, resp.split(b'\r\n', 1)[0].decode()

def send(s, payload):
    m = os.urandom(4); n = len(payload); h = bytearray([0x82])
    if n < 126:
        h.append(0x80 | n)
    elif n < 65536:
        h.append(0x80 | 126); h.extend(struct.pack('!H', n))
    else:
        h.append(0x80 | 127); h.extend(struct.pack('!Q', n))
    s.sendall(h + m + bytes(b ^ m[i % 4] for i, b in enumerate(payload)))

def exact(s, n):
    data = b""
    while len(data) < n:
        chunk = s.recv(n - len(data))
        if not chunk: raise EOFError("websocket closed")
        data += chunk
    return data

def recv(s):
    h = exact(s, 2)
    if len(h) < 2: return None
    opcode = h[0] & 0x0f
    n = h[1] & 127
    if n == 126: n = struct.unpack('!H', exact(s, 2))[0]
    elif n == 127: n = struct.unpack('!Q', exact(s, 8))[0]
    d = b''
    while len(d) < n:
        x = s.recv(n - len(d))
        if not x: return None
        d += x
    if opcode == 8: return None
    return d
def attach(s, session=None):
    body = json.dumps({'session': session, 'cols': 80, 'rows': 24}).encode()
    send(s, bytes([0x32]) + body)
    while True:
        d = recv(s)
        if d is None:
            raise RuntimeError('socket closed before SESSION')
        if d[0] == SRV_SESSION:
            return json.loads(d[1:])
        if d[0] == SRV_ERROR:
            raise RuntimeError('server error: ' + d[1:].decode(errors='replace'))

def collect_output(s, needle, timeout=3.0):
    s.settimeout(.2)
    out = b""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            d = recv(s)
        except socket.timeout:
            continue
        if d is None:
            break
        if d[0] != SRV_OUTPUT:
            continue
        payload = d[1:]
        out += payload
        if b"\x1b[6n" in payload:
            send(s, b"0\x1b[1;1R")
        if needle in out:
            return out
    return out

p = start(['-W'])
try:
    st, _ = post('wrong-token-xxxxxxx')
    print('wrong_token_status', st); assert st == 401
    st, cookieh = post(TOKEN)
    print('login_status', st); assert st == 204
    cookie = cookieh.split(';')[0]
    st, cookieh = post(cookie=cookie)
    print('cookie_refresh', st); assert st == 204

    s, r = ws(cookie, 'http://evil.example')
    print('bad_origin', r); assert '403' in r; s.close()

    s, r = ws(cookie, f'http://127.0.0.1:{PORT}')
    print('good_origin', r); assert '101' in r
    info = attach(s)
    print('first_attach', info['reattached'], info['id'][:8]); assert info['reattached'] is False
    session_id = info['id']
    time.sleep(.4)

    send(s, b'3')
    s.settimeout(.5)
    pong = None
    deadline = time.time() + 3
    while time.time() < deadline:
        try:
            candidate = recv(s)
        except socket.timeout:
            continue
        if candidate and candidate[0] == SRV_OUTPUT and b'\x1b[6n' in candidate[1:]:
            send(s, b'0\x1b[1;1R')
        if candidate == bytes([SRV_PONG]):
            pong = candidate
            break
    print('ping_pong', pong == bytes([SRV_PONG])); assert pong == bytes([SRV_PONG])

    send(s, b'0echo SMOKE_OK\r\n')
    out = collect_output(s, b'SMOKE_OK')
    print('input_echo', b'SMOKE_OK' in out); assert b'SMOKE_OK' in out
    s.close()
    time.sleep(.8)

    s2, r2 = ws(cookie, f'http://127.0.0.1:{PORT}')
    print('reattach_origin', r2); assert '101' in r2
    info2 = attach(s2, session_id)
    print('session_reattached', info2['reattached'], info2['id'][:8]); assert info2['reattached'] is True
    replay = collect_output(s2, b'SMOKE_OK', timeout=2.0)
    print('history_replay', b'SMOKE_OK' in replay); assert b'SMOKE_OK' in replay
    s2.close()
finally:
    terminate_tree(p)
print('SMOKE_OK')
