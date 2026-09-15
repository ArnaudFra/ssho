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
import struct
import subprocess
import sys
import tempfile
import termios
import time

ANSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)")
BINARY = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/ssho").resolve()


class Session:
    def __init__(self, env, args=()):
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
        self.output = b""
        self.mark = 0
        self.process = subprocess.Popen([str(BINARY), *args], stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env)
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
        return ANSI.sub(b"", self.output[self.mark:]).decode("utf-8", errors="replace")

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
        raise AssertionError(f"did not see {text!r}: {ANSI.sub(b'', self.output[-12000:]).decode('utf-8',errors='replace')}")

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
    fake.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$SSHM_TEST_ARGS"\nstty -g > "$SSHM_TEST_TERMIOS"\nprintf "SSH_TAKEOVER\\n"\nexit 37\n')
    fake.chmod(0o700)
    env = dict(os.environ, HOME=directory, USERPROFILE=directory, XDG_CONFIG_HOME=str(home/".config"), APPDATA=str(home/".config"), TERM="xterm-256color", PATH=str(home/"bin")+":"+os.environ.get("PATH",""), SSHM_TEST_ARGS=str(home/"ssh-args"), SSHM_TEST_TERMIOS=str(home/"ssh-termios"))

    session = Session(env)
    session.expect("Last Login")
    session.send(b"server\x0e\r")
    session.expect("SSH_TAKEOVER")
    session.close(37)
    assert (home/"ssh-args").read_text().splitlines()[-1] == "b-server"
    assert (legacy/"config.json").read_text() == legacy_config
    assert (home/".config/ssho/config.json").exists()
    print("PASS search selection, migration, SSH takeover, exit code and terminal restore")

    session = Session(env)
    session.expect("Last Login")
    session.send(b"\ti")
    session.expect("SSH Host Information")
    session.send(b"e")
    session.expect("Edit SSH Host")
    session.send(b"\x15renamed\x13")
    session.expect("Last Login")
    assert json.loads(cli(env,"info","renamed"))["ok"]
    session.send(b"a")
    session.expect("Add SSH Host Configuration")
    session.send(b"c-server\tc.example.org\x13")
    session.expect("Last Login")
    assert json.loads(cli(env,"info","c-server"))["ok"]
    session.send(b"d")
    session.expect("DELETE SSH HOST")
    session.send(b"\x1b")
    session.expect("Last Login")
    session.send(b"h")
    session.expect("Commands")
    session.send(b"\x1b")
    session.expect("Last Login")
    session.send(b"q")
    session.close()
    print("PASS info, edit, add, cancelled deletion and help")

    # A move uses an included file selector and preserves the original SSH block.
    (home/".ssh/config.d").mkdir()
    target = home/".ssh/config.d/target"
    target.write_text("# destination\n")
    config.write_text("Include config.d/*\n"+config.read_text())
    session = Session(env)
    session.expect("Last Login")
    session.send(b"\tm")
    session.expect("Select destination config file")
    session.send(b"\r")
    session.expect("Last Login")
    assert "Host b-server" in target.read_text(), target.read_text()
    session.send(b"f")
    session.expect("Port Forwarding")
    session.send(b"\x1b[C\x1b[C\t1080\t\x13")
    session.expect("SSH_TAKEOVER")
    session.close(37)
    arguments = (home/"ssh-args").read_text().splitlines()
    assert "-D" in arguments and "1080" in arguments[arguments.index("-D")+1], arguments
    history = json.loads((home/".config/ssho/sshm_history.json").read_text())
    assert history["connections"]["b-server"]["port_forwarding"]["type"] == "dynamic"
    print("PASS Include file move and saved SOCKS forwarding")

    # The initial empty-list flow must return to the browser after successful creation.
    empty = home/"empty-config"
    empty.write_text("")
    session = Session(env,["-c",str(empty)])
    session.expect("Would you like to add")
    session.send(b"y\n")
    session.expect("Add SSH Host Configuration")
    session.send(b"first\tfirst.example.org\x13")
    session.expect("Last Login")
    session.send(b"\tq")
    session.close()
    print("PASS first host creation returns to browser")

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
