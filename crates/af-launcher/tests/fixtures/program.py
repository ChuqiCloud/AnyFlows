#!/usr/bin/env python3
"""Synthetic managed process used only by remote launcher integration tests."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import uuid

VERSION = __VERSION__
ROOT = Path(__ROOT__)
UPDATE = __UPDATE__

if sys.argv[1:] == ['--version']:
    print(json.dumps({'edition': 'community', 'version': VERSION, 'target': 'test-target', 'launcher_protocol': 1}))
    raise SystemExit(0)

runtime = open(os.environ['ANYFLOWS_LAUNCHER_RUNTIME_LOCK'], 'a+')
fcntl.flock(runtime, fcntl.LOCK_EX | fcntl.LOCK_NB)


def emit(event):
    with (ROOT / 'events.jsonl').open('a') as stream:
        stream.write(json.dumps({'event': event, 'version': VERSION, 'pid': os.getpid(), 'cwd': os.getcwd(), 'args': sys.argv[1:], 'panel_value': os.environ.get('ANYFLOWS_TEST_PANEL_INJECTED_VALUE'), 'dotenv_value': os.environ.get('ANYFLOWS_TEST_DOTENV_DEFAULT_VALUE')}) + '\n')


def call(action, pid=None):
    host, port = os.environ['ANYFLOWS_LAUNCHER_ENDPOINT'].rsplit(':', 1)
    request = {'protocol': 1, 'token': os.environ['ANYFLOWS_LAUNCHER_TOKEN'], 'generation': os.environ['ANYFLOWS_LAUNCHER_GENERATION'], 'pid': os.getpid() if pid is None else pid, 'action': action}
    with socket.create_connection((host, int(port)), timeout=5) as connection:
        connection.sendall(json.dumps(request).encode() + b'\n')
        return json.loads(connection.makefile('rb').readline(16385))


emit('start')
print('managed stdout', flush=True)
print('managed stderr', file=sys.stderr, flush=True)
if VERSION == '2.0.0' and (ROOT / 'fail-new').exists():
    emit('failed')
    raise SystemExit(7)

assert not call({'command': 'status'}, pid=0)['accepted']
assert call({'command': 'ready'})['accepted']
emit('ready')
assert Path('preserved.txt').read_text() == 'persistent data'

if UPDATE:
    source = ROOT / 'new-program'
    candidate = Path(sys.argv[0]).resolve().parent / ('.anyflows-update-' + str(uuid.uuid4()))
    candidate.write_bytes(source.read_bytes())
    candidate.chmod(0o755)
    install = {'command': 'install', 'candidate': candidate.name, 'tag': 'v2.0.0', 'sha256': hashlib.sha256(candidate.read_bytes()).hexdigest(), 'size': candidate.stat().st_size}
    invalid = dict(install, sha256='0' * 64)
    assert not call(invalid)['accepted']
    assert call(install)['accepted']
    emit('accepted')

for line in sys.stdin:
    request = json.loads(line)
    if request.get('protocol') == 1 and request.get('token') == os.environ['ANYFLOWS_LAUNCHER_TOKEN'] and request.get('command') == 'shutdown':
        emit('shutdown')
        raise SystemExit(0)

emit('eof')
