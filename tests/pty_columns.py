#!/usr/bin/env python3
"""Isolated alias resolution, column preferences and a real terminal."""
import atexit
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path
from pty_screen import Screen

BINARY = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve()
ROOT.mkdir(parents=True, exist_ok=True)
(ROOT / '.ssh').mkdir(exist_ok=True)
(ROOT / 'bin').mkdir(exist_ok=True)
app = ROOT / '.config/ssho'
app.mkdir(parents=True, exist_ok=True)
preferences = {'ping_on_start': False, 'check_for_updates': False,
               'focus_search_on_start': True, 'group_aliases': True,
               'group_mode': 'files', 'host_columns': ['name', 'user', 'port', 'hostname'],
               'future': {'kept': 7}}
(app / 'config.json').write_text(json.dumps(preferences))
(ROOT / '.ssh/config').write_text('Host primary secondary\n HostName 127.0.0.1\n User declared\n Port 2200\n')
ssh = ROOT / 'bin/ssh'
ssh.write_text('#!' + sys.executable + '\n' + r'''
import json, os, sys
from pathlib import Path
args=sys.argv[1:];root=Path(os.environ['HOME'])
with (root/'ssh.jsonl').open('a') as f:f.write(json.dumps(args)+'\n')
if '-G' in args:
 alias=args[-1];secondary=alias=='secondary'
 print('hostname '+alias+'.example\nuser native-'+alias+'\nport '+('4321' if secondary else '22'))
 sys.exit(0)
print('fixture connection return')
sys.exit(37)
''')
ssh.chmod(0o700)
env = dict(os.environ, HOME=str(ROOT), XDG_CONFIG_HOME=str(ROOT / '.config'),
           SSH_AUTH_SOCK=str(ROOT / 'absent-agent'),
           PATH=str(ROOT / 'bin') + os.pathsep + os.environ['PATH'], TERM='xterm-256color')
master, slave = pty.openpty()
original = termios.tcgetattr(slave)
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
process = subprocess.Popen([str(BINARY), '--no-update-check'], stdin=slave, stdout=slave,
                           stderr=slave, env=env, start_new_session=True,
                           preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
screen = Screen()
raw = bytearray()

def cleanup():
    if process.poll() is None:
        process.kill()
        process.wait()
    (ROOT / 'terminal.raw').write_bytes(raw)

atexit.register(cleanup)

def drain(duration=.15):
    end = time.monotonic() + duration
    while time.monotonic() < end:
        ready, _, _ = select.select([master], [], [], max(0, end - time.monotonic()))
        if ready:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if not data:
                break
            raw.extend(data)
            screen.feed(data)
    return screen.text()

def send(data):
    os.write(master, data)
    drain()

def expect(text, timeout=5):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if ''.join(text.split()) in ''.join(drain(.05).replace('│', '').split()):
            return
    raise AssertionError(f'Missing {text!r}:\n{screen.text()}')

def resize(width, height):
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
    os.kill(process.pid, signal.SIGWINCH)
    drain()

try:
    expect('native-primary')
    send(b'v')
    expect('Search: > v')
    assert not any('-G' not in json.loads(line) for line in (ROOT / 'ssh.jsonl').read_text().splitlines())
    send(b'\x15'); send(b'\x1b')  # Ctrl+U clears the text, Esc returns to the table
    send(b'\x1b[F')  # Last row: concrete primary alias, after its file header.
    send(b'\x1bOR'); expect('Choisir un alias')
    send(b'\x1b[B'); send(b'\r'); expect('native-secondary'); expect('4321')
    send(b'v'); expect('Columns'); expect('secondary')
    send(b'\x1b[B'); send(b'\x1b[1;2B')  # User moves after Port.
    for width, height in [(28, 8), (32, 12), (80, 24), (120, 40)]:
        resize(width, height); expect('Columns'); expect('Enter save'); expect('Esc cancel')
        if width == 28:
            expect('hidden: width')
    assert json.loads((app / 'config.json').read_text())['host_columns'] == preferences['host_columns']
    # A failed save retains the draft and selector; recovery saves that same draft.
    (app / 'config.json').rename(app / 'saved.json')
    (app / 'config.json').mkdir()
    send(b'\r'); expect('Columns'); expect('regular file')
    (app / 'config.json').rmdir(); (app / 'saved.json').rename(app / 'config.json')
    external = json.loads((app / 'config.json').read_text())
    external['future']['kept'] = 9
    (app / 'config.json').write_text(json.dumps(external))
    send(b'\r'); expect('native-secondary'); expect('4321')
    saved = json.loads((app / 'config.json').read_text())
    assert saved['host_columns'] == ['name', 'port', 'user', 'hostname'] and saved['future']['kept'] == 9
    # Cancel/reset doesn't publish, and F2 finds the action by its name.
    send(b'v'); send(b'r'); send(b'\x1b')
    assert json.loads((app / 'config.json').read_text()) == saved
    send(b'\x1bOQ'); expect('Actions'); send(b'colonnes'); send(b'\r'); expect('Columns')
    send(b'\x1b'); send(b'\r'); expect('37'); expect('native-secondary')
    calls = [json.loads(line) for line in (ROOT / 'ssh.jsonl').read_text().splitlines()]
    connections = [args for args in calls if '-G' not in args]
    assert len(connections) == 1 and connections[0][-1] == 'secondary', connections
    history = json.loads((app / 'sshm_history.json').read_text())
    assert 'secondary' in json.dumps(history) and 'primary' not in json.dumps(history)
    send(b'q'); assert process.wait(timeout=5) == 0
    assert termios.tcgetattr(slave) == original
    print('PTY PASS: search v; native secondary User/Port and exact SSH target; reorder/hidden columns at 28x8/32x12/80x24/120x40; isolated draft, save failure/retry, external fields; F2 order; history and terminal restoration')
finally:
    cleanup()
    os.close(master)
    os.close(slave)
