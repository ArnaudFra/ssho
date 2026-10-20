#!/usr/bin/env python3
"""Isolated configuration and fake SSH/ping; never contacts a remote host."""
import atexit, fcntl, json, os, pty, select, signal, struct, subprocess, sys, termios, time
from pathlib import Path
from pty_screen import Screen
BINARY=Path(sys.argv[1]).resolve();ROOT=Path(sys.argv[2]).resolve();ROOT.mkdir(parents=True,exist_ok=True)
(ROOT/'.ssh').mkdir(exist_ok=True);(ROOT/'bin').mkdir(exist_ok=True)
app=ROOT/'.config/ssho';app.mkdir(parents=True,exist_ok=True)
(app/'config.json').write_text(json.dumps({'ping_on_start':False,'check_for_updates':False,'focus_search_on_start':True,'future':{'kept':7}}))
(ROOT/'.ssh/config').write_text('Host alpha\n HostName 127.0.0.1\n ProxyJump ignored-bastion\n')
script = "#!"+sys.executable+"\n"+r'''import json, os, subprocess, sys, time
from pathlib import Path
root=Path(os.environ['HOME']);name=Path(sys.argv[0]).name;args=sys.argv[1:]
if name=='ssh':
 if '-G' in args:
  print('hostname 127.0.0.1\nuser fixture\nport 22\nproxyjump ignored-bastion');sys.exit(0)
 print('SSH proxy fixture offline',file=sys.stderr);sys.exit(1)
assert name=='ping' and args==['-n','-c','1','-W','4','-w','4','127.0.0.1'],args
with (root/'ping.jsonl').open('a') as f:f.write(json.dumps(args)+'\n')
mode=(root/'mode').read_text() if (root/'mode').exists() else 'success'
if mode=='slow':
 child=subprocess.Popen(['sleep','30']);(root/'descendant.pid').write_text(str(child.pid));(root/'ping.pid').write_text(str(os.getpid()));time.sleep(30)
if mode=='offline':print('no reply fixture',file=sys.stderr);sys.exit(1)
if mode=='denied':print('Operation not permitted fixture',file=sys.stderr);sys.exit(2)
print('echo reply fixture');sys.exit(0)
'''
for name in ['ssh','ping']:
 tool=ROOT/'bin'/name;tool.write_text(script);tool.chmod(0o700)
env=dict(os.environ,HOME=str(ROOT),XDG_CONFIG_HOME=str(ROOT/'.config'),PATH=str(ROOT/'bin')+os.pathsep+os.environ['PATH'],TERM='xterm-256color')
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
  if ''.join(text.split()) in ''.join(drain(.05).replace('│','').split()):return
 raise AssertionError(f'Missing {text!r}:\n{screen.text()}')
def resize(w,h):
 fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0));os.kill(process.pid,signal.SIGWINCH);drain()

def stopped(pid):
 try:return Path(f'/proc/{pid}/stat').read_text().rsplit(') ',1)[1][0] in 'ZX'
 except FileNotFoundError:return True

def check_stopped(pid):
 end=time.monotonic()+2
 while time.monotonic()<end:
  if stopped(pid):return
  time.sleep(.02)
 raise AssertionError(f'owned ICMP process survived: {pid}')

try:
 expect('alpha');send(b'P');expect('P');assert not (ROOT/'ping.jsonl').exists()
 send(b'\x15');send(b'\x1b');send(b'P');expect('Availability method');expect('ICMP local')
 resize(36,12);expect('Enter: save');resize(110,32)
 send(b'\x1b[B');send(b'\r');expect('ICMP local');expect('+ Online')
 preferences=json.loads((app/'config.json').read_text());assert preferences['availability_method']=='icmp' and preferences['future']['kept']==7
 send(b'i');expect('network availability');expect('no bastion');expect('diagnostic');send(b'\x1b')
 send(b'/');send(b'is:online');expect('alpha');send(b'\x15');send(b'\x1b')
 # Failure stays in the selector; retry after restoring a valid preferences file.
 send(b'P');send(b'\x1b[A');(app/'config.json').rename(app/'saved.json');(app/'config.json').mkdir();send(b'\r');expect('Availability method');expect('expected a regular file')
 (app/'config.json').rmdir();(app/'saved.json').rename(app/'config.json');send(b'\x1b');expect('ICMP local')
 (ROOT/'mode').write_text('offline');send(b'p');expect('! Offline');send(b'/');send(b'is:offline');expect('alpha');send(b'\x15');send(b'\x1b')
 (ROOT/'mode').write_text('denied');send(b'p');expect('? Unknown');send(b'i');expect('Operation not permitted');send(b'\x1b')
 # F2 is still the palette; the action is found by typing its name, without accents.
 send(b'\x1bOQ');expect('Actions');send(b'methode');send(b'\r');expect('Availability method');send(b'\x1b')
 # Switch while native ping has a live descendant. New SSH verdict must win.
 (ROOT/'mode').write_text('slow');send(b'p')
 end=time.monotonic()+3
 while not (ROOT/'ping.pid').exists() and time.monotonic()<end:drain(.05)
 pid=int((ROOT/'ping.pid').read_text());descendant=int((ROOT/'descendant.pid').read_text())
 send(b'P');send(b'\x1b[A');send(b'\r');expect('SSH');expect('! Offline');check_stopped(pid);check_stopped(descendant)
 (ROOT/'ping.pid').unlink();(ROOT/'descendant.pid').unlink()
 # Quit must also cancel/reap an active ICMP process before returning.
 send(b'P');send(b'\x1b[B');send(b'\r')
 end=time.monotonic()+3
 while not (ROOT/'ping.pid').exists() and time.monotonic()<end:drain(.05)
 pid=int((ROOT/'ping.pid').read_text());descendant=int((ROOT/'descendant.pid').read_text())
 send(b'q');assert process.wait(timeout=5)==0;check_stopped(pid);check_stopped(descendant)
 assert termios.tcgetattr(slave)==original
 print('PTY PASS: search P, selector/resize, fresh persistence/failure, method labels/details, filters, native outcomes, existing F2, generation switch cleanup, quit cleanup, terminal restoration')
finally:
 cleanup()
 # Failure cleanup is limited to fixture processes observed in this isolated run.
 for file in ['ping.pid','descendant.pid']:
  if (ROOT/file).exists():
   pid=int((ROOT/file).read_text())
   if not stopped(pid):os.kill(pid,signal.SIGKILL)
 os.close(master);os.close(slave)
