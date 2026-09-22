#!/usr/bin/env python3
"""Linux/macOS PTY smoke checks using disposable SSH/config fixtures only."""
import atexit
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

from pty_screen import Screen

BINARY = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/ssho").resolve()


class Session:
    def __init__(self, env, args=()):
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        self.output = b""
        self.screen = Screen()
        self.mark = 0
        self.process = subprocess.Popen([str(BINARY), *args], stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env, start_new_session=True, preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
        atexit.register(self.cleanup)

    def cleanup(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()

    def drain(self, duration=0.25):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], max(0, deadline-time.monotonic()))
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.output += data
                self.screen.feed(data)
        return self.screen.text()

    def send(self, data):
        self.mark = len(self.output)
        os.write(self.master, data)
        self.drain()

    def expect(self, text, timeout=5):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            current = self.drain(0.1)
            if re.sub(r"\s+", "", text) in re.sub(r"\s+", "", current):
                return
        raise AssertionError(f"did not see {text!r} on terminal screen:\n{self.screen.text()}")

    def close(self, expected=0):
        try:
            code = self.process.wait(timeout=5)
            self.drain(0.05)
            assert code == expected, (code, self.output[-4000:])
            current = termios.tcgetattr(self.slave)
            assert current == self.original, "terminal attributes were not restored"
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            os.close(self.master)
            os.close(self.slave)


def interrupt_dispositions(process):
    # Linux exposes the parent's dispositions without sending a destructive signal.
    status = Path(f"/proc/{process.pid}/status")
    if not status.exists():
        return None
    fields = dict(line.split(":", 1) for line in status.read_text().splitlines())
    mask = (1 << (signal.SIGINT - 1)) | (1 << (signal.SIGQUIT - 1))
    return tuple(int(fields[field].strip(), 16) & mask for field in ("SigCgt", "SigIgn"))


def cli(env, *args):
    return subprocess.run([str(BINARY), *args], env=env, capture_output=True, check=True, text=True).stdout


with tempfile.TemporaryDirectory(prefix="ssho-pty-") as directory:
    home = Path(directory)
    (home/".ssh").mkdir()
    (home/"bin").mkdir()
    legacy = home/".config/sshm"
    legacy.mkdir(parents=True)
    legacy_config = '{"ping_on_start":false,"check_for_updates":false}'
    (legacy/"config.json").write_text(legacy_config)
    config = home/".ssh/config"
    config.write_text("# Tags: work\nHost a-server\n HostName a.example.org\n\nHost b-server\n HostName b.example.org\n")
    fake = home/"bin/ssh"
    fake.write_text(f"#!{sys.executable}\n" + r'''import json, os, signal, sys, termios, time
from pathlib import Path
if "-G" in sys.argv[1:]:
    os.execv("/usr/bin/ssh", ["ssh", *sys.argv[1:]])
Path(os.environ["SSHM_TEST_ARGS"]).write_text("\n".join(sys.argv[1:]))
Path(os.environ["SSHM_TEST_TERMIOS"]).write_text(json.dumps(termios.tcgetattr(0), default=lambda value: value.decode("latin1")))
print("SSH_TAKEOVER", flush=True)
mode = Path(os.environ["SSHM_TEST_MODE"]).read_text()
if mode == "wait":
    time.sleep(30)
if mode == "signal":
    os.kill(os.getpid(), signal.SIGTERM)
sys.exit(int(mode))
''')
    fake.chmod(0o700)
    env = dict(os.environ, HOME=directory, USERPROFILE=directory, XDG_CONFIG_HOME=str(home/".config"), APPDATA=str(home/".config"), TERM="xterm-256color", PATH=str(home/"bin")+":"+os.environ.get("PATH",""), SSHM_TEST_ARGS=str(home/"ssh-args"), SSHM_TEST_TERMIOS=str(home/"ssh-termios"))

    mode = home/"ssh-mode"
    mode.write_text("37")
    env["SSHM_TEST_MODE"] = str(mode)

    session = Session(env)
    session.expect("Last Attempt")
    # Resolution happens with ping disabled, and -G must not touch connection markers.
    session.send(b"\t" + b"i")
    session.expect("Resolved via ssh -G")
    session.expect("Port: 22")
    assert not (home/"ssh-args").exists()
    assert not (home/"ssh-termios").exists()
    session.send(b"\x1b")
    session.send(b"\t")
    browser_modes = termios.tcgetattr(session.slave)
    browser_signals = interrupt_dispositions(session.process)
    assert b"\x1b[1 q" in session.output, "focused input must select a blinking block cursor"
    assert b"\x1b[?25h" in session.output, "focused search must show the terminal cursor"
    session.send(b"server\x0e\r")
    session.expect("SSH exit 37: b-server")
    assert b"SSH_TAKEOVER" in session.output
    session.expect("Search: > server")
    assert termios.tcgetattr(session.slave) == browser_modes
    assert interrupt_dispositions(session.process) == browser_signals
    # JSON encodes control-character bytes as strings; compare the mode flags here.
    assert json.loads((home/"ssh-termios").read_text())[:6] == session.original[:6]
    mode.write_text("0")
    session.send(b"\r")
    session.expect("SSH exit 0: b-server")
    session.expect("Search: > server")
    assert (home/"ssh-args").read_text().splitlines()[-1] == "b-server"
    mode.write_text("signal")
    session.send(b"\r")
    session.expect("SSH exit 143: b-server")
    mode.write_text("wait")
    session.send(b"\r")
    session.expect("SSH_TAKEOVER")
    assert termios.tcgetattr(session.slave) == session.original
    if browser_signals is not None:
        assert interrupt_dispositions(session.process) == (6, 0), "parent must catch SIGINT/SIGQUIT only during SSH"
    # With setsid + TIOCSCTTY, this delivers a real foreground-group SIGINT.
    session.send(b"\x03")
    session.expect("SSH exit 130: b-server")
    session.expect("Search: > server")
    assert termios.tcgetattr(session.slave) == browser_modes
    assert interrupt_dispositions(session.process) == browser_signals
    session.send(b"\x03")
    session.close()
    history = json.loads((home/".config/ssho/sshm_history.json").read_text())
    assert history["connections"]["b-server"]["connect_count"] == 4
    assert history["connections"]["b-server"]["last_result"]["exit_code"] == 130
    assert history["connections"]["b-server"]["last_successful_completion"]
    mode.write_text("37")
    missing_env = dict(env, PATH=str(home/"missing-bin"))
    session = Session(missing_env)
    session.expect("Last Attempt")
    session.send(b"b-server\r")
    session.expect("Error executing SSH command")
    session.expect("Search: > b-server")
    session.send(b"\x03")
    session.close()
    assert (home/"ssh-args").read_text().splitlines()[-1] == "b-server"
    assert (legacy/"config.json").read_text() == legacy_config
    assert (home/".config/ssho/config.json").exists()
    print("PASS repeated SSH returns, search/selection, exit 0/37/signal, Ctrl+C, missing SSH, history and terminal modes")

    session = Session(env)
    session.expect("Last Attempt")
    session.send(b"\t")
    assert b"\x1b[?25l" in session.output[session.mark:], "table focus must hide the input cursor"
    session.send(b"i")
    session.expect("SSH Host Information")
    session.send(b"e")
    session.expect("Edit SSH Host")
    session.send(b"\x15renamed\x13")
    session.expect("SSH Host Information")
    session.expect("Host: renamed")
    assert json.loads(cli(env,"info","renamed"))["ok"]
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"a")
    session.expect("Add SSH Host Configuration")
    session.send(b"c-server\tc.example.org\x13")
    session.expect("Last Attempt")
    assert json.loads(cli(env,"info","c-server"))["ok"]
    session.send(b"i")
    session.expect("Host: c-server")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"d")
    session.expect("DELETE SSH HOST")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"h")
    session.expect("Commands")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"q")
    session.close()
    print("PASS info, edit, add, cancelled deletion and help")

    # A move uses an included file selector and preserves the original SSH block.
    (home/".ssh/config.d").mkdir()
    target = home/".ssh/config.d/target"
    target.write_text("# destination\n")
    config.write_text("Include config.d/*\n"+config.read_text())

    # A standalone add keeps its nested selector on cancel, then exits at its root.
    session = Session(env, ["add", "cancelled-standalone"])
    session.expect("Select config file to add host to:")
    session.send(b"\r")
    session.expect("Add SSH Host Configuration")
    session.send(b"-changed\x1b")
    session.expect("Discard changes?")
    assert b"\x1b[?25l" in session.output[session.mark:], "discard confirmation must hide the input caret"
    session.send(b"\x1b")
    session.expect("cancelled-standalone-changed")
    assert b"\x1b[?25h" in session.output[session.mark:], "resuming the form must restore its caret"
    session.send(b"\x1b")
    session.expect("Discard changes?")
    session.send(b"\r")
    session.expect("Select config file to add host to:")
    session.send(b"\x1b")
    session.close()
    assert "cancelled-standalone" not in config.read_text()
    session = Session(env, ["add", "z-standalone"])
    session.expect("Select config file to add host to:")
    session.send(b"\r")
    session.expect("Add SSH Host Configuration")
    session.send(b"\tz.example.org\x13")
    session.close()
    assert json.loads(cli(env, "info", "z-standalone"))["ok"]
    session = Session(env, ["edit", "z-standalone"])
    session.expect("Edit SSH Host")
    session.send(b"-discarded\x1b")
    session.expect("Discard changes?")
    session.send(b"\r")
    session.close()
    assert "-discarded" not in config.read_text()

    session = Session(env)
    session.expect("Last Attempt")
    session.send(b"\tm")
    session.expect("Select destination config file")
    session.send(b"\r")
    session.expect("Last Attempt")
    assert "Host b-server" in target.read_text(), target.read_text()
    session.send(b"f")
    session.expect("Port Forwarding")
    session.send(b"\x1b[C\x1b")
    session.expect("Discard changes?")
    session.send(b"\x1b")
    session.expect("Remote (-R)")
    session.send(b"\x1b")
    session.expect("Discard changes?")
    session.send(b"\r")
    session.expect("Last Attempt")
    session.send(b"f")
    session.expect("Port Forwarding")
    session.send(b"\x1b[C\x1b[C\t1080\t\x13")
    session.expect("SSH exit 37: b-server")
    assert b"SSH_TAKEOVER" in session.output
    session.send(b"\x03")
    session.close()
    arguments = (home/"ssh-args").read_text().splitlines()
    assert "-D" in arguments and "1080" in arguments[arguments.index("-D")+1], arguments
    history = json.loads((home/".config/ssho/sshm_history.json").read_text())
    assert history["connections"]["b-server"]["port_forwarding"]["type"] == "dynamic"
    print("PASS Include file move and saved SOCKS forwarding")

    # Manual refresh resolves effective values again with ping disabled and keeps
    # search/caret/selection, including when invoked inside the read-only Info view.
    reload_root = home/".ssh/reload-config"
    reload_parts = home/".ssh/reload-parts"
    reload_parts.mkdir()
    reload_keep = reload_parts/"keep"
    reload_removed = reload_parts/"removed"
    reload_keep.write_text("Host reload-keep\n HostName 127.0.0.1\n User before-user\n Port 2208\n")
    reload_removed.write_text("Host reload-removed\n HostName 127.0.0.3\n")
    reload_root.write_text(f"Include {reload_parts}/*\nHost reload-root\n HostName 127.0.0.4\n")
    session = Session(env, ["-c", str(reload_root)])
    session.expect("Last Attempt")
    session.send(b"reload-keep")
    reload_keep.write_text("Host reload-keep\n HostName 127.0.0.2\n User after-user\n Port 2210\n")
    reload_removed.unlink()
    (reload_parts/"added").write_text("Host reload-added\n HostName 127.0.0.5\n")
    session.send(b"\x1b[15~")  # F5 from focused search
    session.expect("Search: > reload-keep")
    session.send(b"\ti")
    session.expect("SSH Host Information: reload-keep")
    session.expect("after-user")
    session.expect("Port: 2210")
    reload_keep.write_text("Host reload-keep\n HostName 127.0.0.6\n User newest-user\n Port 2211\n")
    session.send(b"\x1b[15~")
    session.expect("newest-user")
    session.expect("Port: 2211")
    reload_root.unlink()
    session.send(b"\x1b[15~")
    session.expect("No such file")
    session.expect("newest-user")
    reload_root.write_text(f"Include {reload_parts}/*\nHost reload-root\n HostName 127.0.0.4\n")
    reload_keep.unlink()
    session.send(b"\x1b[15~")
    session.expect("No matching hosts")
    session.expect("Search (/ or Tab to focus): reload-keep")
    assert "SSH Host Information" not in session.screen.text()
    session.send(b"\t\x1b")  # Focus search, clear the retained query.
    session.expect("reload-added")
    assert "reload-removed" not in session.screen.text()
    session.send(b"\x03")
    session.close()
    print("PASS F5 external Include add/edit/remove, preserved search, Info refresh/removal, read failure and fresh native values")

    # App metadata is independent of SSH documents and survives UI restarts.
    notes_root = home/"notes-config"
    notes_root.write_text("Host note-a\n HostName 127.0.0.1\nHost note-b\n HostName 127.0.0.2\n")
    notes_before = notes_root.read_bytes()
    session = Session(env,["-c",str(notes_root)])
    session.expect("Last Attempt")
    session.send(b"\t\x1b[B*")
    session.send(b"N")
    session.expect("Personal note")
    session.expect("Host: note-b")
    session.send(b"Saved note\rsecond line\x13")
    session.expect("Last Attempt")
    session.send(b"i")
    session.expect("Personal note: Saved note")
    session.expect("second line")
    session.send(b"\x1b")
    session.send(b"\x1bOQ" + b"\x1b[B"*9 + b"\r")
    session.expect("favorites only")
    assert "note-a" not in session.screen.text()
    session.send(b"*")
    session.expect("No favorite matches")
    session.send(b"\x1bOQ" + b"\x1b[B"*9 + b"\r")
    session.expect("note-a")
    session.send(b"q")
    session.close()
    metadata_file = home/".config/ssho/metadata.json"
    metadata = json.loads(metadata_file.read_text())
    entry = metadata["configs"][str(notes_root.resolve())]["hosts"]["note-b"]
    assert entry == {"favorite":False,"note":"Saved note\nsecond line"}, entry
    session = Session(env,["-c",str(notes_root)])
    session.expect("Last Attempt")
    session.send(b"note-b\tN")
    session.expect("Saved note")
    session.send(b" draft\x1b")
    session.expect("Discard changes?")
    assert b"\x1b[?25l" in session.output[session.mark:], "note discard modal must hide native caret"
    session.send(b"\x1b")
    session.expect("draft")
    assert b"\x1b[?25h" in session.output[session.mark:], "resuming note must restore native caret"
    committed = metadata_file.read_bytes()
    metadata_file.unlink()
    metadata_file.mkdir()
    session.send(b"\x13")
    session.expect("Error:")
    session.expect("draft")
    metadata_file.rmdir()
    metadata_file.write_bytes(committed)
    session.send(b"\x13")
    session.expect("Last Attempt")
    session.send(b"q")
    session.close()
    assert notes_root.read_bytes() == notes_before
    assert json.loads(metadata_file.read_text())["configs"][str(notes_root.resolve())]["hosts"]["note-b"]["note"].endswith(" draft")
    print("PASS favorite ordering/filter empty recovery, persistent multiline notes, discard caret and failed-save draft; SSH config untouched")

    # Both empty existing and absent roots return to the browser after creation.
    for missing in (False, True):
        empty = home/("missing-config" if missing else "empty-config")
        if missing:
            assert not empty.exists()
        else:
            empty.write_text("")
        session = Session(env,["-c",str(empty)])
        session.expect("Would you like to add")
        session.send(b"y\n")
        session.expect("Add SSH Host Configuration")
        session.send(b"first\tfirst.example.org\x13")
        session.expect("Last Attempt")
        session.send(b"\tq")
        session.close()
    standalone_missing = home/"standalone-missing-config"
    assert not standalone_missing.exists()
    session = Session(env,["-c",str(standalone_missing),"add"])
    session.expect("Add SSH Host Configuration")
    session.send(b"standalone\tstandalone.example.org\x13")
    session.close()
    assert "Host standalone" in standalone_missing.read_text()
    print("PASS first host creation from empty/missing roots returns to browser; standalone Add accepts missing root")

    cancelled = home/"empty-cancel-config"
    cancelled.write_text("")
    session = Session(env,["-c",str(cancelled)])
    session.expect("Would you like to add")
    session.send(b"y\n")
    session.expect("Add SSH Host Configuration")
    session.send(b"\x1b")
    session.close(1)
    assert cancelled.read_text() == ""
    print("PASS cancelled first host creation preserves empty config and exits 1")

print("All PTY checks passed")
