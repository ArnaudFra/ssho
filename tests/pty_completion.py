#!/usr/bin/env python3
"""Bash completion in a real interactive shell: installed name and path invocation.

Usage: pty_completion.py BINARY FIXTURE_DIR
The fixture directory receives a temporary HOME, a fake SSH configuration and a
copy of the binary named ssho; nothing is read from the real HOME.
"""
import os
import pty
import select
import shutil
import sys
import time
from pathlib import Path

BINARY = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve()
if shutil.which('bash') is None:
    print('BASH COMPLETION SKIPPED: bash is not installed')
    sys.exit(0)
home = ROOT / 'home'
(home / '.ssh').mkdir(parents=True, exist_ok=True)
(home / '.ssh/config').write_text('Host BSalpha\n HostName a.example\nHost other\n HostName o.example\n')
installed = ROOT / 'bin/ssho'
installed.parent.mkdir(exist_ok=True)
shutil.copy2(BINARY, installed)
# The same binary reached through a path, as during development.
(ROOT / 'build').mkdir(exist_ok=True)
by_path = ROOT / 'build/ssho'
shutil.copy2(BINARY, by_path)
env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
           PATH=str(installed.parent) + os.pathsep + os.environ['PATH'], PS1='$ ', TERM='dumb')
pid, fd = pty.fork()
if pid == 0:
    os.chdir(ROOT)
    os.execvpe('bash', ['bash', '--norc', '--noprofile', '-i'], env)


def read(wait=.6):
    out = b''
    end = time.time() + wait
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], .1)
        if ready:
            try:
                out += os.read(fd, 65536)
            except OSError:
                break
    return out.decode(errors='replace').replace('\r', '').replace('\n', ' ').strip()


def complete(command):
    os.write(fd, command.encode() + b'\t')
    line = read()
    os.write(fd, b'\x15')                 # Ctrl+U: the line is dropped, nothing runs
    read(.2)
    return line


try:
    read()
    os.write(fd, b'source <(ssho completion bash)\n')
    read()
    for typed, expected in [
        ('ssho BS', 'ssho BSalpha'),
        ('ssho comp', 'ssho completion'),
        # Bash falls back to the last path component: the same completion applies.
        ('./build/ssho BS', './build/ssho BSalpha'),
        ('./build/ssho comp', './build/ssho completion'),
    ]:
        line = complete(typed)
        assert line.endswith(expected), f'{typed!r}: expected {expected!r}, got {line!r}'
finally:
    os.write(fd, b'exit\n')
    os.waitpid(pid, 0)
assert not (home / '.config/ssho').exists(), 'completion must not create application data'
print('BASH COMPLETION PASS: hosts and commands for the installed name and for a path invocation')
