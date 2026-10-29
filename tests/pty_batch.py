#!/usr/bin/env python3
"""PTY regression: exclusively disposable config and fake native SSH."""
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
app = ROOT / '.config/ssho'
app.mkdir(parents=True, exist_ok=True)
(app / 'config.json').write_text(json.dumps({'ping_on_start': False, 'check_for_updates': False, 'focus_search_on_start': True, 'group_aliases': True, 'group_mode': 'files', 'ssh_command': ['/nonexistent/interactive-wrapper']}))
(ROOT / '.ssh/config').write_text('Host alpha secondary\n HostName invalid.test\nHost beta\n HostName invalid.test\n')
ssh = ROOT / 'bin/ssh'
ssh.write_text('''#!''' + sys.executable + '''
import json, os, sys, time
from pathlib import Path
args=sys.argv[1:]
if '-G' in args:
 print('hostname fixture.invalid\\nuser fixture\\nport 22')
 sys.exit(0)
with (Path(os.environ['HOME'])/'attempts.jsonl').open('a') as f: f.write(json.dumps(args)+'\\n')
print('stdout fixture',flush=True)
print('stderr fixture',file=sys.stderr,flush=True)
if args[-1]=='slow': time.sleep(60)
sys.exit(7 if args[-2]=='secondary' else 0)
''')
ssh.chmod(0o700)
env = dict(os.environ, HOME=str(ROOT), XDG_CONFIG_HOME=str(ROOT/'.config'), PATH=str(ROOT/'bin')+os.pathsep+os.environ['PATH'], TERM='xterm-256color')
env.pop('SSH_AUTH_SOCK', None)
master, slave = pty.openpty()
original=termios.tcgetattr(slave)
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',32,110,0,0))
process=subprocess.Popen([str(BINARY),'--no-update-check'],stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True,preexec_fn=lambda:fcntl.ioctl(0,termios.TIOCSCTTY,0))
screen=Screen();raw=bytearray()
def cleanup():
 if process.poll() is None: process.kill();process.wait()
 (ROOT/'terminal.raw').write_bytes(raw)
atexit.register(cleanup)
def drain(duration=.15):
 end=time.monotonic()+duration
 while time.monotonic()<end:
  ready,_,_=select.select([master],[],[],max(0,end-time.monotonic()))
  if ready:
   try: data=os.read(master,65536)
   except OSError: break
   if not data: break
   raw.extend(data);screen.feed(data)
 return screen.text()
def send(data): os.write(master,data);drain()
def expect(text,timeout=5):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  if ''.join(text.split()) in ''.join(drain(.05).split()): return
 raise AssertionError(f'Missing {text!r}:\n{screen.text()}')
def resize(w,h):
 fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0));os.kill(process.pid,signal.SIGWINCH);drain()
try:
 expect('alpha')
 send(b'xB');expect('xB');assert not (ROOT/'attempts.jsonl').exists()
 send(b'\x15');send(b'\x1b') # Ctrl+U clears the text, Esc returns to the table
 # Choose secondary alias, then mark only that active alias.
 send(b'\x1bOR');expect('secondary');send(b'\x1b[B');send(b'\r')
 send(b'x');send(b'B');expect('1 targets');expect('secondary')
 command=b"echo 'literal ; $HOME'"
 send(command);send(b'\r');assert not (ROOT/'attempts.jsonl').exists()
 send(b'\x0c');expect('Confirm batch');expect("echo 'literal ; $HOME'")
 resize(32,12);expect('Confirm batch');resize(110,32)
 # Enter keeps the safe choice (back to edit); the Launch button starts the batch.
 send(b'\r');expect('[Command]');assert not (ROOT/'attempts.jsonl').exists()
 send(b'\x0c');expect('[▶ Back to edit ]');send(b'\t');expect('[▶ Launch on 1 target(s) ]')
 send(b'\r');expect('Failed (7)');expect('stdout fixture');expect('[▶ Output ]')
 send(b'\t');expect('stderr fixture');expect('[▶ Errors ]')
 attempts=[json.loads(line) for line in (ROOT/'attempts.jsonl').read_text().splitlines()]
 assert len(attempts)==1 and attempts[0][-2:]==['secondary',command.decode()],attempts
 history=json.loads((app/'sshm_history.json').read_text())['connections']
 assert history['secondary']['connect_count']==1 and history['secondary']['last_result']['exit_code']==7
 # Start second batch and test responsive foreground guard and exit cancellation.
 send(b'\x1b');send(b'B');send(b'slow');send(b'\x0c');send(b'\x0c');expect('Running')
 send(b'\x1b');send(b'\r');expect('Batch active')
 # Leaving the results keeps the batch running; quitting from the list is confirmed.
 send(b'B');expect('Running');send(b'q');expect('?: help')
 send(b'q');expect('Lot en cours');send(b'\r');expect('?: help')
 send(b'B');expect('Running')
 send(b's');expect('Cancelled');send(b'q');expect('?: help');send(b'q')
 assert process.wait(timeout=5)==0
 assert termios.tcgetattr(slave)==original,'terminal not restored'
 history=json.loads((app/'sshm_history.json').read_text())['connections']
 assert history['secondary']['connect_count']==2
 assert 'cancelled' in history['secondary']['last_result']['launch_error']
 print('PTY PASS: search x/B, active secondary alias, frozen targets, confirmation, resize, separate streams/exit/history, foreground guard, cancel/quit, terminal restore')
finally:
 cleanup();os.close(master);os.close(slave)
