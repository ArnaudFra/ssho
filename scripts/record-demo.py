#!/usr/bin/env python3
"""Record the demo of the interface as an asciinema cast.

Everything shown is made up: example hosts on documentation addresses, a fake
`ssh` and a fake `ping`. No real host, key or agent is used.

Usage: scripts/record-demo.py target/release/ssho demo.cast [work-directory]
Then:  agg --font-size 14 --last-frame-duration 3 demo.cast docs/demo.gif
       (https://github.com/asciinema/agg)

With bubblewrap installed the fixture is shown as /home/demo; without it the
paths of the work directory appear in the recording.
"""
import codecs, fcntl, json, os, pty, select, shutil, struct, subprocess, sys, tempfile, termios, time
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'tests'))
from pty_screen import Screen

WIDTH, HEIGHT = 120, 30
HOME = '/home/demo'          # what the recording shows, through bubblewrap

# alias: (file, tags, hostname, user, port, proxy, reachable)
HOSTS = {
    'bastion':     ('production', 'gateway',  '203.0.113.10',        'ops',      2222, None,      True),
    'web-01':      ('production', 'web',      'web-01.example.com',  'deploy',   22,   'bastion', True),
    'web-02':      ('production', 'web',      'web-02.example.com',  'deploy',   22,   'bastion', False),
    'api-01':      ('production', 'api',      '192.0.2.21',          'deploy',   22,   'bastion', True),
    'db-primary':  ('production', 'database', '192.0.2.31',          'postgres', 22,   'bastion', True),
    'db-replica':  ('production', 'database', '192.0.2.32',          'postgres', 22,   'bastion', True),
    'cache-01':    ('production', 'cache',    '192.0.2.41',          'deploy',   22,   'bastion', True),
    'staging-web': ('staging',    'web',      '198.51.100.11',       'deploy',   22,   None,      True),
    'staging-api': ('staging',    'api',      '198.51.100.21',       'deploy',   22,   None,      False),
    'staging-db':  ('staging',    'database', '198.51.100.31',       'postgres', 22,   None,      True),
    'pi-garage':   ('lab',        'home',     'pi-garage.example.net', 'pi',     22,   None,      True),
    'nas':         ('lab',        'storage',  'nas.example.net',     'admin',    2022, None,      True),
}
FILE_TAGS = {'production': 'prod', 'staging': 'staging', 'lab': 'lab'}
FAVORITES = ['bastion', 'web-01', 'db-primary']
NOTES = {
    'db-primary': 'Primary PostgreSQL.\nFailover: see runbook DB-12.\nMaintenance window: Sunday 02:00 UTC.',
    'bastion': 'Entry point for production. MFA required.',
    'nas': 'Backups run every night at 03:00.',
}
# alias: (days ago, number of connections)
HISTORY = {'web-01': (0.02, 41), 'bastion': (0.2, 87), 'db-primary': (1, 12), 'staging-web': (3, 9),
           'nas': (9, 4), 'api-01': (16, 6)}

FAKE_TOOL = r'''#!/usr/bin/env python3
"""Fake ssh and ping of the demo: they answer from a table and never open a socket."""
import json, os, sys, time, zlib
from pathlib import Path
hosts = json.loads((Path(os.environ['HOME']) / '.demo-hosts.json').read_text())
name, args = Path(sys.argv[0]).name, sys.argv[1:]
if name == 'ping':
    target = args[-1]
    time.sleep(0.4 + (zlib.crc32(target.encode()) % 12) / 10)
    for host in hosts.values():
        if host['hostname'] == target and host['reachable']:
            print(f'64 bytes from {target}: icmp_seq=1 ttl=57 time=11.8 ms')
            sys.exit(0)
    print('no reply', file=sys.stderr)
    sys.exit(1)
alias = args[-1]
host = hosts.get(alias)
if '-G' in args:
    if host is None:
        sys.exit(f'unknown host {alias}')
    print(f"hostname {host['hostname']}\nuser {host['user']}\nport {host['port']}")
    print('identityfile ~/.ssh/id_ed25519')
    if host['proxy']:
        print(f"proxyjump {host['proxy']}")
    print('serveraliveinterval 30')
    sys.exit(0)
if host is None or not host['reachable']:
    sys.exit(f'ssh: connect to host {alias} port 22: Connection timed out')
print('Welcome to Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-45-generic x86_64)\n')
print('Last login: Mon Nov  9 08:12:44 2026 from 203.0.113.10')
while True:
    try:
        line = input(f"{host['user']}@{alias}:~$ ").strip()
    except EOFError:
        break
    if line == 'exit':
        break
    if line == 'uptime':
        print(' 10:42:07 up 37 days,  4:12,  1 user,  load average: 0.08, 0.11, 0.09')
    elif line:
        print(f'{line.split()[0]}: command not found')
print(f"logout\nConnection to {host['hostname']} closed.")
'''


def build_fixture(root):
    """A home directory with example SSH files, preferences and fake tools."""
    home = Path(root)
    (home / '.ssh/conf.d').mkdir(parents=True)
    (home / '.demo-bin').mkdir()
    (home / '.config/ssho').mkdir(parents=True)
    (home / '.ssh/config').write_text('Include conf.d/*.conf\n\nHost *\n    ServerAliveInterval 30\n')
    for name, tag in FILE_TAGS.items():
        lines = [f'# FileTags: {tag}', '']
        for alias, (source, tags, hostname, user, port, proxy, _) in HOSTS.items():
            if source != name:
                continue
            lines += [f'# Tags: {tags}', f'Host {alias}', f'    HostName {hostname}', f'    User {user}']
            if port != 22:
                lines.append(f'    Port {port}')
            if proxy:
                lines.append(f'    ProxyJump {proxy}')
            lines.append('')
        (home / f'.ssh/conf.d/{name}.conf').write_text('\n'.join(lines))
    table = {alias: dict(hostname=h[2], user=h[3], port=h[4], proxy=h[5], reachable=h[6]) for alias, h in HOSTS.items()}
    (home / '.demo-hosts.json').write_text(json.dumps(table))
    for tool in ('ssh', 'ping'):
        path = home / '.demo-bin' / tool
        path.write_text(FAKE_TOOL)
        path.chmod(0o755)
    (home / '.config/ssho/config.json').write_text(json.dumps({
        'check_for_updates': False, 'focus_search_on_start': False, 'ping_on_start': True,
        'availability_method': 'icmp'}))


class Session:
    """The program in a pseudo-terminal, with what it draws kept for the cast."""

    def __init__(self, binary, root, record=None):
        self.record, self.screen, self.started = record, Screen(WIDTH, HEIGHT), time.monotonic()
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        sandbox = shutil.which('bwrap') is not None
        home = HOME if sandbox else str(root)
        env = {'HOME': home, 'XDG_CONFIG_HOME': home + '/.config', 'TERM': 'xterm-256color',
               'PATH': f'{home}/.demo-bin:/usr/bin:/bin', 'LANG': 'C.UTF-8'}
        command = []
        if sandbox:
            command = ['bwrap', '--bind', '/', '/', '--dev-bind', '/dev', '/dev', '--tmpfs', '/home',
                       '--bind', str(root), HOME, '--chdir', HOME]
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', HEIGHT, WIDTH, 0, 0))
        self.process = subprocess.Popen(command + [str(binary), '--no-update-check'], stdin=slave, stdout=slave,
                                        stderr=slave, env=env, cwd=str(root), start_new_session=True,
                                        preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
        os.close(slave)

    def wait(self, duration):
        end = time.monotonic() + duration
        while True:
            remaining = end - time.monotonic()
            if remaining <= 0:
                return
            ready, _, _ = select.select([self.master], [], [], remaining)
            if not ready:
                return
            try:
                data = os.read(self.master, 65536)
            except OSError:
                return
            if not data:
                return
            self.screen.feed(data)
            if self.record is not None:
                text = self.decoder.decode(data)
                if text:
                    self.record.append((time.monotonic(), text))

    def expect(self, text, timeout=8):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.wait(0.05)
            if text in self.screen.text():
                return
        raise AssertionError(f'missing {text!r}:\n{self.screen.text()}')

    def send(self, data, pause=0.4):
        os.write(self.master, data if isinstance(data, bytes) else data.encode())
        self.wait(pause)

    def type(self, text, delay=0.09, pause=0.5):
        for character in text:
            self.send(character, delay)
        self.wait(pause)

    def close(self):
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        os.close(self.master)


UP, DOWN, ENTER, ESC, TAB, BACKSPACE = b'\x1b[A', b'\x1b[B', b'\r', b'\x1b', b'\t', b'\x7f'
F2, F4, CTRL_S = b'\x1bOQ', b'\x1bOS', b'\x13'


def select_host(session, alias):
    session.send('/', 0.2)
    session.type(alias, 0.01, 0.2)
    session.send(TAB, 0.2)


def prepare(binary, root):
    """Favorites, notes and history are created through the program itself, off the record."""
    session = Session(binary, root)
    session.expect('bastion')
    for alias in FAVORITES:
        select_host(session, alias)
        session.send('*', 0.2)
        session.send(BACKSPACE, 0.2)
    for alias, note in NOTES.items():
        select_host(session, alias)
        session.send('N', 0.3)
        for index, line in enumerate(note.split('\n')):
            if index:
                session.send(ENTER, 0.05)
            session.type(line, 0.005, 0.05)
        session.send(CTRL_S, 0.4)
        session.send(BACKSPACE, 0.2)
    for alias in HISTORY:
        select_host(session, alias)
        session.send(ENTER, 0.1)
        session.expect(':~$')
        session.send('exit\n', 0.6)
        session.expect('Hosts')
        session.send(BACKSPACE, 0.2)
    session.send('q', 0.5)
    session.close()
    # Spread the connections over the last weeks.
    path = Path(root) / '.config/ssho/sshm_history.json'
    history = json.loads(path.read_text())
    now = datetime.now(timezone.utc)
    for key, entry in history['connections'].items():
        for alias, (days, count) in HISTORY.items():
            if key.endswith(alias) or entry.get('host_name') == alias:
                stamp = (now - timedelta(days=days)).isoformat().replace('+00:00', 'Z')
                for field in ('last_connect', 'last_attempt'):
                    if field in entry:
                        entry[field] = stamp
                if 'connect_count' in entry:
                    entry['connect_count'] = count
    path.write_text(json.dumps(history, indent=2))
    return history


def write_cast(path, events, started):
    with open(path, 'w', encoding='utf-8') as cast:
        cast.write(json.dumps({'version': 2, 'width': WIDTH, 'height': HEIGHT,
                               'env': {'TERM': 'xterm-256color', 'SHELL': '/bin/bash'}}) + '\n')
        for moment, text in events:
            cast.write(json.dumps([round(moment - started, 3), 'o', text], ensure_ascii=False) + '\n')


CTRL_U, FIRST = b'\x15', b'\x1b[H'
PROMPT = '\x1b[1;32mdemo@laptop\x1b[0m:\x1b[1;34m~\x1b[0m$ '


def typed_at_prompt(events, text):
    """What a shell would echo while the command is typed."""
    now = time.monotonic()
    events.append((now, PROMPT))
    now += 0.8
    for character in text:
        events.append((now, character))
        now += 0.14
    events.append((now + 0.4, '\r\n'))
    time.sleep(now + 0.5 - time.monotonic())


def record(binary, root, checkpoints=None):
    """The journey shown in the README."""
    events = []
    started = time.monotonic()
    typed_at_prompt(events, 'ssho')
    session = Session(binary, root, events)
    look = lambda name: checkpoints is not None and checkpoints.append((name, session.screen.text()))
    session.expect('bastion')
    session.send(FIRST, 0.1)
    session.wait(2.8)                      # availability arrives host by host
    look('list')
    session.send(DOWN, 0.8)
    session.send(F4, 3.0)                  # details of a host with a note and a bastion
    look('details')
    session.send(DOWN, 1.3)
    session.send(DOWN, 1.3)
    session.send(F4, 0.8)
    session.send('/', 0.6)                 # search by tag, then by state
    session.type('tag:database', 0.1, 1.7)
    look('search tag')
    session.send(CTRL_U, 0.5)
    session.type('is:offline', 0.1, 1.9)
    look('search offline')
    session.send(ESC, 0.7)
    session.send(BACKSPACE, 1.0)
    session.send('g', 1.3)                 # groups by file, then by tag
    session.send(DOWN, 0.6)
    session.send(DOWN, 0.9)
    session.send(ENTER, 2.5)
    look('groups')
    session.send(F2, 1.4)                  # every action is in the palette
    session.type('tunnel', 0.12, 1.7)
    look('palette')
    session.send(ESC, 0.8)
    session.send('/', 0.5)                 # connect, work, come back
    session.type('web-01', 0.11, 0.9)
    session.send(ENTER, 0.2)
    session.expect(':~$')
    session.wait(1.2)
    session.type('uptime', 0.12, 0.3)
    session.send(ENTER, 1.8)
    look('ssh')
    session.type('exit', 0.12, 0.3)
    session.send(ENTER, 0.2)
    session.expect('SSH exit 0')
    session.wait(2.0)
    look('back')
    session.send(ESC, 0.6)
    session.send(BACKSPACE, 1.4)
    session.send('q', 0.8)
    look('quit')
    session.close()
    events.append((time.monotonic() + 0.2, PROMPT))
    events.append((time.monotonic() + 2.5, ''))
    return started, events


if __name__ == '__main__':
    binary, output = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    work = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else Path(tempfile.mkdtemp(prefix='ssho-demo.'))
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    build_fixture(work)
    prepare(binary, work)
    checkpoints = [] if os.environ.get('SSHO_DEMO_SCREENS') else None
    started, events = record(binary, work, checkpoints)
    write_cast(output, events, started)
    for name, text in checkpoints or []:
        print('=====', name)
        print('\n'.join(line.rstrip() for line in text.split('\n') if line.strip()))
    print(f'{output}: {len(events)} events, {events[-1][0] - started:.1f} s')
