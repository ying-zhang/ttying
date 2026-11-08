"""Bounded local checks against freshly built next/target/debug/ttying.exe."""
import base64, http.client, json, os, socket, struct, subprocess, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EXE = ROOT / 'target' / 'debug' / ('ttying.exe' if os.name == 'nt' else 'ttying')
EXE = Path(os.environ.get('TTYING_TEST_EXE', DEFAULT_EXE))
TOKEN = os.environ.get('TTYING_TEST_TOKEN', 'audit-local-token-1234567890')
DEFAULT_SHELL = ['cmd.exe'] if os.name == 'nt' else ['/bin/sh']
SHELL_CMD = json.loads(os.environ.get('TTYING_TEST_SHELL', json.dumps(DEFAULT_SHELL)))
DEFAULT_COMMAND = ['cmd.exe','/c','exit','0'] if os.name == 'nt' else ['/bin/sh','-c','exit 0']
COMMAND = json.loads(os.environ.get('TTYING_TEST_COMMAND', json.dumps(DEFAULT_COMMAND)))
def exact(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n-len(b))
        if not c: raise EOFError()
        b += c
    return b
def send(s, data, opcode=2):
    n=len(data); mask=os.urandom(4)
    h=bytes([0x80|opcode])
    h += bytes([0x80|n]) if n<126 else (bytes([0xfe])+struct.pack('!H',n) if n<65536 else bytes([0xff])+struct.pack('!Q',n))
    s.sendall(h+mask+bytes(x^mask[i%4] for i,x in enumerate(data)))
def recv(s):
    h=exact(s,2); n=h[1]&127
    if n==126: n=struct.unpack('!H',exact(s,2))[0]
    if n==127: n=struct.unpack('!Q',exact(s,8))[0]
    return h[0]&15, exact(s,n)
def post_request(port, path, cookie=None, token=None):
    c=http.client.HTTPConnection('127.0.0.1',port,timeout=3)
    headers={}
    if token is not None: headers['Authorization']='Bearer '+token
    if cookie: headers['Cookie']=cookie
    c.request('POST',path,headers=headers); r=c.getresponse(); r.read()
    result=(r.status,r.getheader('Set-Cookie')); c.close(); return result
def ws(port,cookie,origin):
    s=socket.create_connection(('127.0.0.1',port),timeout=3)
    s.sendall((f'GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {base64.b64encode(os.urandom(16)).decode()}\r\nSec-WebSocket-Version: 13\r\nCookie: {cookie}\r\nOrigin: {origin}\r\n\r\n').encode())
    header=b''
    while not header.endswith(b'\r\n\r\n'): header+=exact(s,1)
    return s,header.split(b'\r\n')[0].decode()
def terminate_tree(p):
    if os.name == 'nt':
        subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        p.terminate()
    try: p.wait(timeout=5)
    except Exception: pass

def check(name,fn,extra=None,command=None):
    with socket.socket() as probe:
        probe.bind(('127.0.0.1',0)); port=probe.getsockname()[1]
    p=subprocess.Popen([str(EXE),'--port',str(port),'--token',TOKEN]+(extra or [])+['--']+(command or COMMAND),stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    sockets=[]
    try:
        for _ in range(60):
            try:
                st,h=post_request(port,'/api/session',token=TOKEN)
                break
            except OSError: time.sleep(.05)
        cookie=h.split(';')[0]
        result=fn(port,cookie,sockets)
        print(json.dumps({'check':name,'observed':result},ensure_ascii=False),flush=True)
    finally:
        for s in sockets: s.close()
        terminate_tree(p)
def open_ws(port,cookie,sockets,scheme='http'):
    s,h=ws(port,cookie,f'{scheme}://127.0.0.1:{port}'); sockets.append(s); return s,h
def shell_exit(port,cookie,sockets):
    s,h=open_ws(port,cookie,sockets); send(s,b'2'+json.dumps({'cols':80,'rows':24}).encode())
    events=[]; end=time.monotonic()+2
    while time.monotonic()<end:
        s.settimeout(.25)
        try:
            op,d=recv(s)
            if op==2:
                events.append(d[:1].decode())
                if d[:1]==b'1': info=json.loads(d[1:])
                if b'\x1b[6n' in d: send(s,b'0\x1b[1;1R')
        except socket.timeout: pass
    s2,h2=open_ws(port,cookie,sockets)
    send(s2,b'2'+json.dumps({'session':info['id']}).encode())
    _,d=recv(s2)
    return {'events':events,'ended_frame': '3' in events,'reattached_to_exited_shell':json.loads(d[1:])['reattached']}
def logout(port,cookie,sockets):
    s,h=open_ws(port,cookie,sockets)
    status,_=post_request(port,'/api/logout',cookie=cookie)
    send(s,b'3'); _,d=recv(s)
    return {'logout':status,'existing_ws_pong':d==b'2','new_login':post_request(port,'/api/session',cookie=cookie)[0]}
def huge_text(port,cookie,sockets):
    s,h=open_ws(port,cookie,sockets)
    send(s,b'x'*(1024*1024+1),opcode=1); send(s,b'3'); _,d=recv(s)
    return {'text_bytes':1024*1024+1,'socket_still_works':d==b'2'}
def scheme(port,cookie,sockets):
    s,h=open_ws(port,cookie,sockets,'https'); return h


def assert_check(name, fn, expected, **kwargs):
    def wrapped(port,cookie,sockets):
        value=fn(port,cookie,sockets)
        assert expected(value), (name,value)
        return value
    check(name,wrapped,**kwargs)

def exited(port,cookie,sockets):
    s,_=open_ws(port,cookie,sockets)
    send(s,b'2{}')
    end=time.monotonic()+5
    events=[]
    while time.monotonic()<end:
        s.settimeout(.5)
        try:
            op,d=recv(s)
            if op==8: break
            if op==2 and d[:1]==b'3': return json.loads(d[1:])['code']
        except socket.timeout: pass
    return 'missing_exit'

def closed_after_logout(port,cookie,sockets):
    s,_=open_ws(port,cookie,sockets)
    post_request(port,'/api/logout',cookie=cookie)
    send(s,b'3')
    try:
        op,d=recv(s)
        return op==8
    except (EOFError,ConnectionError): return True

def rejects_large(port,cookie,sockets):
    s,_=open_ws(port,cookie,sockets)
    try:
        send(s,b'x'*(1024*1024+1),opcode=1)
        op,d=recv(s)
        return op==8
    except (EOFError,ConnectionError): return True

def slot_limit(port,cookie,sockets):
    s,h=open_ws(port,cookie,sockets)
    s2,h2=open_ws(port,cookie,sockets)
    return '101' in h and '503' in h2

def same_cookie(port,cookie,sockets):
    status,h=post_request(port,'/api/session',cookie=cookie)
    return status==204 and h.split(';')[0]==cookie

def read_only_blocks_input(port,cookie,sockets):
    s,_=open_ws(port,cookie,sockets); send(s,b'2{}')
    while True:
        _,d=recv(s)
        if d[:1]==b'1':
            assert json.loads(d[1:])['read_only'] is True
            break
    send(s,b'0echo AUDIT_INPUT_MUST_NOT_RUN\r\n')
    end=time.monotonic()+1; output=b''
    while time.monotonic()<end:
        s.settimeout(.1)
        try:
            op,d=recv(s)
            if op==2 and d[:1]==b'0': output+=d[1:]
        except socket.timeout: pass
    return b'AUDIT_INPUT_MUST_NOT_RUN' not in output

if __name__ == '__main__':
    assert_check('shell_exit',exited,lambda x:x=='session_ended')
    assert_check('logout_revokes_websocket',closed_after_logout,bool)
    assert_check('over_1MiB_rejected',rejects_large,bool)
    assert_check('wrong_scheme_origin',scheme,lambda x:'403' in x)
    assert_check('connection_limit',slot_limit,bool,extra=['--max-clients','1'])
    assert_check('cookie_renewal_reuses_id',same_cookie,bool)
    assert_check('read_only_blocks_input',read_only_blocks_input,bool,command=SHELL_CMD)
    print('REGRESSION_OK')
