#!/usr/bin/env python3
"""Initial browser query, real terminal, owned SSH/config fixtures only."""
import atexit
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

from pty_screen import Screen

BINARY = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve()
ROOT.mkdir(parents=True, exist_ok=True)
(ROOT / '.ssh').mkdir(exist_ok=True)
(ROOT / 'bin').mkdir(exist_ok=True)
APP = ROOT / '.config/ssho'
APP.mkdir(parents=True, exist_ok=True)
CONFIG = ROOT / '.ssh/config'
CONFIG.write_text('# Tags: prod\nHost alpha\n HostName 127.0.0.1\n User declared\n\nHost beta\n HostName 127.0.0.2\n')
original_config = CONFIG.read_bytes()
(APP / 'config.json').write_text(json.dumps({
    'ping_on_start': False, 'check_for_updates': False,
    'focus_search_on_start': False, 'host_columns': ['name', 'user', 'port'],
    'future': {'kept': 7},
}))
(APP / 'metadata.json').write_text(json.dumps({'configs': {str(CONFIG.resolve()): {
    'hosts': {'alpha': {'favorite': True, 'note': 'Release café 東京'}}
}}}))
original_metadata = (APP / 'metadata.json').read_bytes()
ssh = ROOT / 'bin/ssh'
ssh.write_text('#!' + sys.executable + '\n' + r'''
import json, os, sys
from pathlib import Path
root = Path(os.environ['HOME'])
args = sys.argv[1:]
with (root / 'ssh.jsonl').open('a') as out:
    out.write(json.dumps(args) + '\n')
if '-G' in args:
    print('hostname 127.0.0.1\nuser native-' + args[-1] + '\nport 2222')
    sys.exit(0)
print('fixture explicit SSH return')
sys.exit(37)
''')
ssh.chmod(0o700)
ping = ROOT / 'bin/ping'
ping.write_text('#!/bin/sh\nexit 0\n')
ping.chmod(0o700)
ENV = dict(os.environ, HOME=str(ROOT), USERPROFILE=str(ROOT),
           XDG_CONFIG_HOME=str(ROOT / '.config'), APPDATA=str(ROOT / '.config'),
           SSH_AUTH_SOCK=str(ROOT / 'absent-agent'), TERM='xterm-256color',
           PATH=str(ROOT / 'bin') + os.pathsep + os.environ['PATH'])


def connections():
    log = ROOT / 'ssh.jsonl'
    return [args for args in map(json.loads, log.read_text().splitlines()) if '-G' not in args] if log.exists() else []


class Session:
    def __init__(self, name, *args, env=ENV):
        self.name = name
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
        self.screen = Screen()
        self.raw = bytearray()
        self.process = subprocess.Popen([str(BINARY), '--no-update-check', *args],
                                        stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                        env=env, start_new_session=True,
                                        preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
        atexit.register(self.cleanup)

    def cleanup(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()
        (ROOT / (self.name + '.raw')).write_bytes(self.raw)

    def drain(self, duration=.12):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            ready, _, _ = select.select([self.master], [], [], max(0, end - time.monotonic()))
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.raw.extend(data)
                self.screen.feed(data)
        return self.screen.text()

    def expect(self, text, timeout=5):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if ''.join(text.split()) in ''.join(self.drain(.05).split()):
                return
        raise AssertionError(f'{self.name}: missing {text!r}:\n{self.screen.text()}')

    def send(self, data):
        os.write(self.master, data)
        self.drain()

    def resize(self, width, height):
        self.drain()
        mark = len(self.raw)
        self.screen = Screen(width, height)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        os.kill(self.process.pid, signal.SIGWINCH)
        end = time.monotonic() + 5
        while time.monotonic() < end:
            text = self.drain(.05)
            # The search zone is an input while editing and a filter band otherwise.
            zone = any(marker in text for marker in ('/ > ', 'Search: > ', 'Filtres', 'Rechercher'))
            if b'\x1b[2J' in self.raw[mark:] and zone and 'F2: actions' in text:
                return
        raise AssertionError(f'{self.name}: missing fresh compact frame:\n{self.screen.text()}')

    def close(self, code=0):
        try:
            assert self.process.wait(timeout=5) == code
            self.drain()
            assert termios.tcgetattr(self.slave) == self.original
        finally:
            self.cleanup()
            os.close(self.master)
            os.close(self.slave)

    def quit(self):
        self.send(b'\x03')
        self.close()


query = 'is:favorite tag:prod AND "café 東京"'
session = Session('metadata', '--search=false', '--query', query)
session.expect('Search: > ' + query)
session.expect('native-alpha')
assert 'beta' not in session.screen.text()
assert not connections() and not (APP / 'sshm_history.json').exists()
# Query typing and palette cancellation remain in the ordinary browser.
session.send(b'v')
session.expect(query + 'v')
session.send(b'\x7f')
session.send(b'\x1bOQ')
session.expect('Actions')
session.send(b'\x1b')
session.expect('Search: > ' + query)
session.send(b'\x1b[15~')
session.expect('native-alpha')
session.expect(query)
# Save group/column/method preferences through existing dispatch; query survives.
session.send(b'\t')
session.send(b'g')
session.expect('Group view')
session.send(b'\x1b[B')
session.send(b'\r')
session.expect(query)
session.send(b'v')
session.expect('Columns')
session.send(b'\r')
session.expect(query)
session.send(b'P')
session.expect('Availability method')
session.send(b'\x1b[B')
session.send(b'\r')
session.expect('ICMP local')
session.expect(query)
assert not connections() and not (APP / 'sshm_history.json').exists()
session.send(b'\t')
session.resize(28, 8)
session.send(b'\x1b[D')
session.send(b'X')
assert 'X' in session.screen.text()
session.send(b'\x7f')
# Return to the host row after adding a group header, and explicitly connect.
session.send(b'\t')
session.send(b'\x1b[F')
session.send(b'\r')
session.expect('37')
assert len(connections()) == 1 and connections()[0][-1] == 'alpha', connections()
history = json.loads((APP / 'sshm_history.json').read_text())
assert history['connections']['alpha']['connect_count'] == 1
# Still a live browser after SSH, with the initial query and focus preserved.
session.resize(120, 40)
session.expect(query)
session.expect('?: help')
session.send(b'\t')
session.send(b'\x15')
session.send(b'alpha')
session.expect('Search: > alpha')
session.quit()
assert CONFIG.read_bytes() == original_config
assert (APP / 'metadata.json').read_bytes() == original_metadata
assert json.loads((APP / 'config.json').read_text())['future']['kept'] == 7

for name, value in [('native', 'user:native-alpha AND tag:prod'),
                    ('syntax', 'tag:'), ('no-match', 'no-such-host'), ('empty', '')]:
    before = len(connections())
    session = Session(name, '--query=' + value)
    session.expect('Search: > ' + value)
    if name == 'native':
        session.expect('native-alpha')
        assert 'beta' not in session.screen.text()
    elif name == 'syntax':
        session.expect('value')
    elif name == 'no-match':
        session.send(b'\r')
    assert len(connections()) == before
    session.send(b'\x15')
    session.send('café 東京'.encode())
    session.expect('Search: > café 東京')
    session.quit()

empty_config = ROOT / 'empty-config'
empty_config.write_text('')
session = Session('no-hosts', '-c', str(empty_config), '--query', '')
session.expect('Search: >')
assert b'Would you like' not in session.raw
session.send(b'\x1bOQ')
session.expect('Actions')
session.send(b'\x1b')
session.send(b'editable')
session.expect('Search: > editable')
session.quit()
# A new application directory is initialized without a new host-creation prompt.
fresh = ROOT / 'fresh-home'
fresh.mkdir()
fresh_env = dict(ENV, HOME=str(fresh), USERPROFILE=str(fresh),
                 XDG_CONFIG_HOME=str(fresh / '.config'), APPDATA=str(fresh / '.config'))
session = Session('fresh-no-hosts', '-c', str(empty_config), '--query=', env=fresh_env)
session.expect('Search: >')
assert b'Would you like' not in session.raw
assert (fresh / '.config/ssho').is_dir()
assert not (fresh / '.config/ssho/sshm_history.json').exists()
session.quit()
# The historical no-query prompt remains intact.
session = Session('legacy-empty', '-c', str(empty_config))
session.expect('Would you like to add a new host now?')
session.send(b'n\n')
session.close(1)
assert empty_config.read_bytes() == b'' and len(connections()) == 1
print('PTY PASS: metadata/favorite/tag/native user; empty/Unicode/syntax/no-match; forced editable focus; no implicit connection/history; F2/F5/groups/columns/method; compact caret; explicit SSH/return; empty config without creation prompt; legacy prompt; terminal restored')
