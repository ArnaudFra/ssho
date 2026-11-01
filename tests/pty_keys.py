#!/usr/bin/env python3
"""Isolated native workflow: temporary HOME, fake key tools, no remote or agent."""
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
BINARY=Path(sys.argv[1]).resolve();ROOT=Path(sys.argv[2]).resolve();ROOT.mkdir(parents=True,exist_ok=True)
(ROOT/'.ssh').mkdir(exist_ok=True);(ROOT/'bin').mkdir(exist_ok=True)
app=ROOT/'.config/ssho';app.mkdir(parents=True,exist_ok=True)
(app/'config.json').write_text(json.dumps({'ping_on_start':False,'check_for_updates':False,'focus_search_on_start':True}))
private=ROOT/'.ssh/id_fixture';private.write_text('fake private fixture, never read')
PUBLIC='ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\n'
Path(str(private)+'.pub').write_text(PUBLIC)
second=ROOT/'.ssh/id_second';second.write_text('fake private fixture');Path(str(second)+'.pub').write_text(PUBLIC)
(ROOT/'.ssh/config').write_text(f'Host alpha secondary\n HostName invalid.test\n User original\n IdentityFile "{private}"\n')
script='''#!'''+sys.executable+'''
import json, os, sys, termios
from pathlib import Path
root=Path(os.environ['HOME']);name=Path(sys.argv[0]).name;args=sys.argv[1:]
if name=='ssh':
 assert args[0]=='-G', args
 print('hostname fixture.invalid\\nuser fixture\\nport 22');sys.exit(0)
if name=='ssh-keygen' and args[0]=='-lf':
 assert args[1].endswith('.pub');print('256 SHA256:fixture fake (ED25519)');sys.exit(0)
assert os.isatty(0) and os.isatty(1) and os.isatty(2)
assert termios.tcgetattr(0)[3]&termios.ICANON
with (root/'native.jsonl').open('a') as f:f.write(json.dumps([name,args])+'\\n')
if name=='ssh-keygen':
 assert args[0:2]==['-t','ed25519'] and '-N' not in args
 print('Native generation fixture: press Enter',flush=True);input()
 if (root/'fail-generation').exists():sys.exit(9)
 out=Path(args[args.index('-f')+1]);out.write_text('fake private fixture')
 Path(str(out)+'.pub').write_text('ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\\n')
 if args[-1]=='race':(out.parent.parent/'raced.pub').write_text('occupied public fixture')
elif name=='ssh-add':
 assert args in [[str(root/'.ssh/id_fixture')], [str(root/'keys/new key')]]
elif name=='ssh-copy-id':
 assert args[0]=='-i' and args[2]=='-F' and args[3]==str(root/'.ssh/config') and args[4]=='alpha'
 assert '-f' not in args
 public=Path(args[1]);assert public.name=='key.pub' and public.parent.parent==Path('/tmp')
 assert public.with_suffix('').is_symlink()
else:raise AssertionError(name)
'''
for name in ['ssh','ssh-keygen','ssh-add','ssh-copy-id']:
 tool=ROOT/'bin'/name;tool.write_text(script);tool.chmod(0o700)
env=dict(os.environ,HOME=str(ROOT),XDG_CONFIG_HOME=str(ROOT/'.config'),PATH=str(ROOT/'bin')+os.pathsep+os.environ['PATH'],TERM='xterm-256color',SSH_AUTH_SOCK=str(ROOT/'fake-agent-never-contacted'))
master,slave=pty.openpty();original=termios.tcgetattr(slave)
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',32,110,0,0))
process=subprocess.Popen([str(BINARY),'--no-update-check'],stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True,preexec_fn=lambda:fcntl.ioctl(0,termios.TIOCSCTTY,0))
screen=Screen();raw=bytearray()
def cleanup():
 if process.poll() is None:process.kill();process.wait()
 (ROOT/'terminal.raw').write_bytes(raw)
atexit.register(cleanup)
def drain(duration=.15):
 end=time.monotonic()+duration
 while time.monotonic()<end:
  ready,_,_=select.select([master],[],[],max(0,end-time.monotonic()))
  if ready:
   try:data=os.read(master,65536)
   except OSError:break
   if not data:break
   raw.extend(data);screen.feed(data)
 return screen.text()
def send(data):os.write(master,data);drain()
def expect(text,timeout=5):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  if ''.join(text.split()) in ''.join(drain(.05).split()):return
 raise AssertionError(f'Missing {text!r}:\n{screen.text()}')
def resize(w,h):
 fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0));os.kill(process.pid,signal.SIGWINCH);drain()
try:
 expect('alpha');send(b'K');expect('K');assert not (ROOT/'native.jsonl').exists()
 send(b'\x15');send(b'\x1b');send(b'e');expect('Hosts > Edit')
 # F9 picker from an edited name; cancel preserves caret and parent draft.
 send(b'Z');send(b'\x1b[20~');expect('IdentityFile picker');expect('SHA256:fixture');send(b'\x1b');expect('alphaZ')
 send(b'\x1b[20~');expect('IdentityFile picker');send(b'\r');expect('alphaZ')
 send(b'\x1b');expect('Discard the changes');send(b'\t\r');expect('alpha')
 send(b'K');expect('OpenSSH assistant');expect('SHA256:fixture');expect('alpha, secondary')
 for w,h in [(32,12),(32,8),(28,8)]:
  resize(w,h);expect('id_fixture');send(b'\x1b[B');expect('id_second');send(b'\x1b[A')
  send(b'n');send(b'/long/common/prefix/destinationZ');expect('destinationZ')
  send(b'\t');send(b'comment-visibleZ');expect('comment-visibleZ');send(b'\x1b')
 resize(110,32)
 send(b'a');expect('Native key operation completed')
 # Review is explicit; modifying the public source prevents launch.
 send(b'd');expect('Confirm deployment');assert len((ROOT/'native.jsonl').read_text().splitlines())==1
 Path(str(private)+'.pub').write_text(PUBLIC.replace(' fixture',' changed'));expect('[▶ Back ]');send(b'\t');expect('[▶ Deploy to');send(b'\r');expect('public key changed after review')
 assert len((ROOT/'native.jsonl').read_text().splitlines())==1
 send(b'\x1b');Path(str(private)+'.pub').write_text(PUBLIC);send(b'\x1b[15~');expect('SHA256:fixture');send(b'd');send(b'\t\r');expect('Native key operation completed')
 # Failure returns to intact generation form; retry publishes a pair.
 (ROOT/'keys').mkdir(exist_ok=True)
 send(b'n');dest=ROOT/'keys/new key';send(str(dest).encode());send(b'\t');send(b'fixture comment')
 (ROOT/'fail-generation').touch();send(b'\r');expect('[▶ Continue in terminal ]');send(b'\r');expect('Native generation fixture');send(b'\r');expect('exited with');expect('fixture comment');assert not dest.exists()
 (ROOT/'fail-generation').unlink();send(b'\r');expect('Native generation fixture');send(b'\r');expect('Native key operation completed');assert dest.exists() and Path(str(dest)+'.pub').exists()
 earlier=ROOT/'.ssh/aaa';earlier.write_text('fake private fixture');Path(str(earlier)+'.pub').write_text(PUBLIC)
 send(b'\x1b[15~');expect(str(earlier));expect(str(dest));send(b'a');expect('Native key operation completed')
 assert json.loads((ROOT/'native.jsonl').read_text().splitlines()[-1])==['ssh-add',[str(dest)]]
 # A public collision after native generation retains the private destination
 # and the generation draft, with explicit manual recovery instead of rollback.
 send(b'n');raced=ROOT/'.ssh/raced';send(str(raced).encode());send(b'\t');send(b'race');send(b'\r\r');expect('Native generation fixture');send(b'\r');expect('publication failed')
 resize(28,8);expect('race');send(b'\x1bOS')  # F4 exposes complete native failure without dropping form.
 seen=''
 for _ in range(65):
  seen+=''.join(screen.text().split());send(b'\x1b[B')
 for text in [str(raced),str(raced)+'.pub','left untouched','Inspect both paths before retrying','recover the public with native ssh-keygen -y','File exists']:
  assert ''.join(text.split()) in seen,(text,seen)
 send(b'\x1b');expect('race');resize(110,32)
 assert raced.read_text()=='fake private fixture' and Path(str(raced)+'.pub').read_text()=='occupied public fixture'
 assert not list((ROOT/'.ssh').glob('.sshm-key-*'))
 send(b'\x1b');send(b'\x1b');send(b'q');assert process.wait(timeout=5)==0;assert termios.tcgetattr(slave)==original
 calls=[json.loads(line) for line in (ROOT/'native.jsonl').read_text().splitlines()]
 assert [call[0] for call in calls]==['ssh-add','ssh-copy-id','ssh-keygen','ssh-keygen','ssh-add','ssh-keygen'],calls
 print('PTY PASS: focused search, dirty form picker cancel/choose, public inventory/usages, resize, agent, deploy review/revalidation/safe snapshot, native failure draft/retry, generation pair/partial publication preservation, terminal restoration')
finally:cleanup();os.close(master);os.close(slave)
