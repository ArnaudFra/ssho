#!/usr/bin/env python3
"""Linux/macOS PTY smoke checks using disposable SSH/config fixtures only."""
import atexit
from contextlib import nullcontext
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
        self.capture()
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()

    def capture(self):
        if "SSHM_TEST_FIXTURE_DIR" in os.environ:
            root = Path(os.environ["SSHM_TEST_FIXTURE_DIR"])
            (root / f"terminal-{self.process.pid}.raw").write_bytes(self.output)

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

    def resize_and_wait(self, width, height, compact_footer, timeout=5):
        # Drain the old geometry before marking output. A text match alone could
        # be from the previous frame; Ratatui must clear and draw the new layout.
        self.drain(0.1)
        mark = len(self.output)
        self.screen = Screen(width, height)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        os.kill(self.process.pid, signal.SIGWINCH)
        deadline = time.monotonic() + timeout
        # A full-width box border of the new geometry; borders may carry a title.
        def bordered(text):
            return any(line.strip().startswith("╭") and line.strip().endswith("╮")
                       and len(line.strip()) == width - 2 for line in text.splitlines())
        while time.monotonic() < deadline:
            current = self.drain(0.05)
            if (b"\x1b[2J" in self.output[mark:] and bordered(current)
                    and compact_footer in current):
                return
        raise AssertionError(f"no fresh {width}x{height} frame after resize:\n{self.screen.text()}")

    def confirm_changes(self):
        self.expect("Enter: confirm exact changes")
        self.send(b"\r")

    def close(self, expected=0):
        self.capture()
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


with (nullcontext(os.environ["SSHM_TEST_FIXTURE_DIR"])
      if "SSHM_TEST_FIXTURE_DIR" in os.environ
      else tempfile.TemporaryDirectory(prefix="ssho-pty-")) as directory:
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
if "-N" in sys.argv[1:]:
    Path(os.environ["SSHM_TEST_ARGS"] + ".pid").write_text(str(os.getpid()))
    assert not os.isatty(0)
    assert sys.stdin.read() == ""
    mode = Path(os.environ["SSHM_TEST_MODE"]).read_text()
    if mode == "bind":
        print("bind: Address already in use", file=sys.stderr, flush=True)
        sys.exit(255)
    print("TUNNEL_STDOUT_MUST_NOT_REACH_UI", flush=True)
    print("waiting for tunnel fixture", file=sys.stderr, flush=True)
    time.sleep(60)
    sys.exit(0)
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

    if "--groups-only" in sys.argv[2:]:
        first = home/"one/config"
        second = home/"two/config"
        first.parent.mkdir()
        second.parent.mkdir()
        first.write_text("# FileTags: inherited\n# Tags: own, other\nHost production prod\n HostName primary.target\n User deploy\n\nHost beta\n HostName beta.target\n")
        second.write_text("Host gamma\n HostName gamma.target\n")
        config.write_text(f'Include "{first}" "{second}"\n')
        original = {path: path.read_bytes() for path in (config, first, second)}
        (legacy/"config.json").write_text(json.dumps(dict(ping_on_start=False, check_for_updates=False, focus_search_on_start=False, group_aliases=True)))
        session = Session(env)
        session.expect("3 blocks")
        session.send(b"g")
        session.expect("Group view")
        session.send(b"\x1b[B\r")
        session.expect("2 groups")
        app_config = home/".config/ssho/config.json"
        assert json.loads(app_config.read_text())["group_mode"] == "files"
        # Header Enter/Space toggle only; every host mutation and F3/F6 is inert.
        session.send(b"\x1b[H\r")
        session.expect("Selected: 1/3")
        for sequence in (b"edmfN*Dp", b"\x1bOR", b"\x1b[17~"):
            session.send(sequence)
            session.expect("2 groups")
        assert not (home/"ssh-args").exists()
        session.send(b"\x1bOQmodifier\r")  # F2 Edit is unavailable on a group header
        session.expect("Indisponible : Sélectionnez un hôte, pas un groupe")
        session.send(b"\x1b")
        session.send(b"/prod")
        session.expect("1 blocks")
        session.send(b"\r")
        session.expect("SSH exit 37: production")
        assert (home/"ssh-args").read_text().splitlines()[-1] == "production"
        session.send(b"\x1bOR\x1b[B\r")
        session.expect("→prod")
        session.send(b"\r")
        session.expect("SSH exit 37: prod")
        assert (home/"ssh-args").read_text().splitlines()[-1] == "prod"
        session.send(b"\x15")  # Ctrl+U clears the query; Esc would only leave the edit
        session.send(b"\t\x1b[H")  # table focus; manual first-file collapse restored
        session.expect("Selected: 1/3")
        session.send(b"\x1b[15~")  # F5 keeps selected header + collapse
        session.expect("Selected: 1/3")
        # Fresh preference write preserves launcher, alias mode and unknown fields.
        latest = json.loads(app_config.read_text())
        latest.update(ssh_command=["ssh", "-v"], future_setting={"preserve": [1, 2]})
        app_config.write_text(json.dumps(latest))
        session.send(b"\x1bOQarborescence\r")
        session.expect("Group view")
        session.send(b"\x1b[F\r")
        session.expect("5 groups")
        saved = json.loads(app_config.read_text())
        assert saved["group_mode"] == "file_tags"
        assert saved["group_aliases"] is True
        assert saved["ssh_command"] == ["ssh", "-v"]
        assert saved["future_setting"] == {"preserve": [1, 2]}
        session.send(b"\x1b[C")  # expand first file, then children use hierarchy
        session.expect("inherited")
        session.expect("own")
        session.expect("Sans tag")
        # Group info exposes the full path in a narrow terminal and never starts SSH.
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 20, 24, 0, 0))
        os.kill(session.process.pid, signal.SIGWINCH)
        session.expect("3 blocks • 5 groups")  # wait for compact redraw before keyboard input
        session.send(b"i")
        session.expect("Group info")
        session.expect(str(first))
        session.send(b"\r")
        session.expect("groups")
        assert (home/"ssh-args").read_text().splitlines()[-1] == "prod"
        # Failed atomic write reports the error INSIDE the still-open picker.
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
        os.kill(session.process.pid, signal.SIGWINCH)
        session.expect("3 blocks • 4/4 aliases • 5 groups")
        protected = home/"protected-config.json"
        app_config.rename(protected)
        app_config.symlink_to(protected)
        session.send(b"g\x1b[H\r")
        session.expect("Group view")
        session.expect("refusing application file")
        assert json.loads(protected.read_text())["group_mode"] == "file_tags"
        app_config.unlink()
        protected.rename(app_config)
        session.send(b"\x1b")
        session.expect("5 groups")
        session.send(b"\x03")
        session.close()
        # Restart retains mode, but collapse is deliberately session-only.
        session = Session(env)
        session.expect("5 groups")
        session.expect("production")
        session.expect("gamma")
        # Query always starts on a concrete host, even with file + tag parents.
        session.send(b"/gamma\r")
        session.expect("SSH exit 37: gamma")
        assert (home/"ssh-args").read_text().splitlines() == ["-v", "-F", str(config), "gamma"]
        session.send(b"\x03")
        session.close()
        assert all(path.read_bytes() == before for path, before in original.items())
        assert set(cli(env, "search", "--format", "simple").splitlines()) == {"production", "prod", "beta", "gamma"}
        print("Groups PTY passed: modes/F2, header safety, hierarchy/query/reload/SSH, alias targets, narrow full paths, fresh persistence and visible failure")
        sys.exit(0)

    if "--aliases-only" in sys.argv[2:]:
        config.write_text("Match host production\n HostName primary.target\n User primary-user\n"
                          "Match host prod\n HostName secondary.target\n User secondary-user\n"
                          "Host production prod\n Port 22\n")
        original = config.read_bytes()
        session = Session(env)
        session.expect("production")
        app_config = home/".config/ssho/config.json"
        latest = json.loads(app_config.read_text())
        latest.update(ssh_command=["ssh", "-v"], check_for_updates=True,
                      future_setting={"preserve": [1, 2]})
        app_config.write_text(json.dumps(latest))
        session.send(b"\tA")
        session.expect("1 blocks")
        saved = json.loads(app_config.read_text())
        assert saved["group_aliases"] is True
        assert saved["ssh_command"] == ["ssh", "-v"]
        assert saved["check_for_updates"] is True
        assert saved["future_setting"] == {"preserve": [1, 2]}
        session.send(b"/secondary.target")
        session.expect("prod")
        session.expect("1/2 aliases")
        session.send(b"\r")
        session.expect("SSH exit 37: prod")
        assert (home/"ssh-args").read_text().splitlines() == ["-v", "-F", str(config), "prod"]
        session.send(b"\x1bOR")  # F3
        session.expect("Choose alias")
        session.expect("primary-user@primary.target")
        session.expect("secondary-user@secondary.target")
        session.send(b"\x1b[H\r")  # Home + select primary: incompatible query is cleared
        session.expect("production")
        session.send(b"\r")
        session.expect("SSH exit 37: production")
        assert (home/"ssh-args").read_text().splitlines()[-1] == "production"
        session.send(b"\x03")
        session.close()
        (home/"aliases-terminal.raw").write_bytes(session.output)
        # Persisted preference is read at restart; failed safe publication leaves mode intact.
        session = Session(env, ("--no-update-check",))
        session.expect("1 blocks")
        session.send(b"\x1bOQalias regroupes\r")  # F2: alias grouping toggle
        session.expect("Hosts: 2/2")
        session.send(b"\tA")
        session.expect("1 blocks")
        # A compact terminal must keep an explicit primary -> active alias label.
        session.resize_and_wait(24, 20, "F2: actions")
        session.send(b"/secondary.target")
        session.expect("→prod")
        session.expect("1/2 aliases")
        session.send(b"\x15")  # Ctrl+U clears the query; Esc would only leave the edit
        session.expect("2/2 aliases")
        session.send(b"\t")  # table focus only after the cleared-query frame
        protected = home/"protected-config.json"
        app_config.rename(protected)
        app_config.symlink_to(protected)
        session.send(b"A")
        session.expect("Error:")
        session.expect("1 blocks")
        assert json.loads(protected.read_text())["group_aliases"] is True
        app_config.unlink()
        protected.rename(app_config)
        session.send(b"\x03")
        session.close()
        (home/"aliases-errors.raw").write_bytes(session.output)
        assert config.read_bytes() == original
        # UI grouping never removes raw CLI aliases or shell completion candidates.
        listing = cli(env, "search", "--format", "simple")
        assert set(listing.splitlines()) == {"production", "prod"}
        completions = cli(env, "__complete", "pro")
        assert set(completions.splitlines()) == {"production", "prod", ":4"}
        print("Aliases PTY passed: grouping, native secondary search/targets, chooser, persistence and errors")
        sys.exit(0)

    if "--launcher-only" in sys.argv[2:]:
        # Same foreground process fixture, preceded by two literal wrapper argv.
        wrapper = home/"wrapper space ;$"
        wrapper.write_text(fake.read_text().replace(
            'if "-G" in sys.argv[1:]:',
            'Path(os.environ["SSHM_WRAPPER_PREFIX"]).write_text(json.dumps(sys.argv[1:3]))\n'
            'del sys.argv[1:3]\nif "-G" in sys.argv[1:]:'
        ))
        wrapper.chmod(0o700)
        prefix = ["prefix space", "$(touch SHOULD_NOT_EXIST); *"]
        env["SSHM_WRAPPER_PREFIX"] = str(home/"wrapper-prefix")
        (legacy/"config.json").write_text(json.dumps({
            "ping_on_start": False, "check_for_updates": False,
            "ssh_command": [str(wrapper), *prefix]
        }))
        session = Session(env)
        session.expect("a-server")
        session.send(b"server\x0e")
        session.expect("b-server")
        browser_modes = termios.tcgetattr(session.slave)
        browser_signals = interrupt_dispositions(session.process)
        session.send(b"\r")
        session.expect("SSH exit 37: b-server")
        assert json.loads((home/"wrapper-prefix").read_text()) == prefix
        assert not (home/"SHOULD_NOT_EXIST").exists()
        assert (home/"ssh-args").read_text().splitlines() == ["-F", str(config), "b-server"]
        assert json.loads((home/"ssh-termios").read_text())[:6] == session.original[:6]
        session.expect("Search: > server")
        assert termios.tcgetattr(session.slave) == browser_modes
        assert interrupt_dispositions(session.process) == browser_signals
        for child_mode, expected in [("0", 0), ("signal", 143)]:
            mode.write_text(child_mode)
            session.send(b"\r")
            session.expect(f"SSH exit {expected}: b-server")
        mode.write_text("wait")
        session.send(b"\r")
        deadline = time.monotonic() + 5
        while b"SSH_TAKEOVER" not in session.output[session.mark:] and time.monotonic() < deadline:
            session.drain(0.1)
        assert b"SSH_TAKEOVER" in session.output[session.mark:]
        assert termios.tcgetattr(session.slave) == session.original
        session.send(b"\x03")  # Real foreground-group SIGINT reaches wrapper.
        session.expect("SSH exit 130: b-server")
        session.expect("Search: > server")
        assert termios.tcgetattr(session.slave) == browser_modes
        assert interrupt_dispositions(session.process) == browser_signals
        history = json.loads((home/".config/ssho/sshm_history.json").read_text())
        assert history["connections"]["b-server"]["connect_count"] == 4
        assert history["connections"]["b-server"]["last_result"]["exit_code"] == 130
        assert history["connections"]["b-server"]["last_successful_completion"]
        # Missing executable also returns to the same search and restores terminal state.
        wrapper.unlink()
        session.send(b"\r")
        session.expect("ssh_command")
        session.expect("Search: > server")
        assert termios.tcgetattr(session.slave) == browser_modes
        session.send(b"\x03")
        session.close()
        (home/"launcher-terminal.raw").write_bytes(session.output)
        print("Launcher PTY passed: literal wrapper prefix, selected alias/search, foreground modes/signals, exit 37/0/143/130, history and missing-program return")
        raise SystemExit(0)

    if "--file-tags-only" in sys.argv[2:]:
        included = home/".ssh/included"
        included.write_text("#FileTags: hidden, child\nHost secret\n HostName secret.invalid\n")
        config.write_text("#FileTags: team\nInclude included\n# Tags: own\nHost alpha\n HostName alpha.invalid\n#FileTags: ignored\n")
        session = Session(env)
        session.expect("FileTags warning")
        session.expect("%team")
        session.expect("#own")
        assert "secret" not in session.screen.text()
        session.send(b"tag:team own")
        session.expect("alpha")
        session.send(b"\t" + b"c")
        session.expect("Filtrer par fichier source")
        session.send(b"\x1b[B\r")
        session.expect("Fichier"); session.expect("C/F2")
        session.expect("FileTags warning")
        session.send(b"H")
        session.expect("alpha")
        assert "secret" not in session.screen.text()
        session.send(b"e")
        session.expect("hérités : %team")
        session.send(b"\x1b")
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 10, 60, 0, 0))
        session.screen = Screen(60, 10)
        session.process.send_signal(signal.SIGWINCH)
        session.expect("FileTags warning")
        session.expect("Fichier"); session.expect("C/F2")
        config.write_text("#FileTags: team\nInclude included\n# Tags: own\nHost alpha\n HostName alpha.invalid\n")
        session.send(b"\x1b[15~")
        session.expect("alpha")
        assert "FileTags warning" not in session.screen.text()
        session.send(b"C\x7f")  # all sources, then Backspace from the table clears the query
        session.expect("secret")
        session.send(b"\x03")
        session.close()
        print("File tags PTY passed: inherited/own display, tag search, hidden, exact source, read-only edit inheritance, durable compact warnings and F5 refresh")
        sys.exit(0)

    if "--source-filter-only" in sys.argv[2:]:
        # Real terminal dispatch: c/C remain text in search and act from the table.
        (home/".ssh/sources").mkdir()
        source = home/".ssh/sources/hosts"
        source.write_text("Host included\n HostName included.invalid\n")
        empty = home/".ssh/empty"
        empty.write_text("# Known empty file\n")
        config.write_text("Include sources/hosts empty\nHost global\n HostName global.invalid\n")
        session = Session(env)
        session.expect("global")
        session.send(b"cC")
        session.expect("cC")
        assert "Filtrer par fichier source" not in session.screen.text()
        session.send(b"\x15")  # Ctrl+U clears the query, retaining search focus
        session.send(b"\t" + b"c")
        session.expect("Filtrer par fichier source")
        session.expect("Tous les fichiers")
        session.expect(str(empty))
        session.send(b"\x1b[B" * 3 + b"\r")
        session.expect("Fichier"); session.expect("C/F2")
        session.expect(str(empty))
        assert "included" not in session.screen.text()
        assert "global" not in session.screen.text()
        session.send(b"H")
        session.expect("Fichier"); session.expect("C/F2")
        session.send(b"\x1b[15~")  # F5 keeps the empty scope
        session.expect("Fichier"); session.expect("C/F2")
        # A compact terminal keeps the source banner alongside errors/hidden badges.
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 10, 60, 0, 0))
        session.screen = Screen(60, 10)
        session.process.send_signal(signal.SIGWINCH)
        session.expect("Fichier"); session.expect("C/F2")
        session.send(b"C")
        session.expect("included")
        session.expect("global")
        assert "File:" not in session.screen.text()
        # F2 works from focused search with no results; the action is found by name.
        session.send(b"\tmissing")
        session.send(b"\x1bOQ")
        session.expect("Actions")
        session.send(b"filtrer par fichier\r")
        session.expect("Tous les fichiers")
        session.send(b"\x1b")
        session.expect("missing")
        session.send(b"\x03")
        session.close()
        print("Source filter PTY passed: actual c/C, empty scope, H/F5, compact banner, search/F2")
        sys.exit(0)

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
    session.expect("Hôtes > Modifier")
    session.send(b"\x15renamed\x1b[D\x1b[D")
    draft_caret = (session.screen.x, session.screen.y)
    session.send(b"\x1b[18~")
    session.expect("Owned tunnels")
    session.send(b"\x1b[200~q\r\n\x13inert\x1b[201~")
    session.expect("Owned tunnels")
    session.send(b"q")  # returning from owned tunnels never quits: the draft is back
    session.expect("Hôtes > Modifier")
    session.expect("renamed")
    assert (session.screen.x, session.screen.y) == draft_caret
    session.send(b"\x13")
    session.expect("Review SSH changes")
    session.send(b"\x1b[18~")
    session.expect("Owned tunnels")
    session.send(b"\x03")  # Ctrl+C follows Esc: one level back, never a quit
    session.expect("Review SSH changes")
    session.send(b"\x1b")
    session.expect("Hôtes > Modifier")
    assert (session.screen.x, session.screen.y) == draft_caret
    session.send(b"\x13")
    session.confirm_changes()
    session.expect("SSH Host Information")
    session.expect("Host: renamed")
    assert json.loads(cli(env,"info","renamed"))["ok"]
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"a")
    session.expect("Hôtes > Ajouter un hôte")
    session.send(b"c-server\tc.example.org\x13")
    session.confirm_changes()
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
    session.expect("Hôtes > Ajouter un hôte")
    session.send(b"-changed\x1b")
    session.expect("Abandonner les modifications")
    assert b"\x1b[?25l" in session.output[session.mark:], "discard confirmation must hide the input caret"
    session.send(b"\r")  # Enter keeps the focused, safe choice: continue editing
    session.expect("cancelled-standalone-changed")
    assert b"\x1b[?25h" in session.output[session.mark:], "resuming the form must restore its caret"
    session.send(b"\x1b")
    session.expect("Abandonner les modifications")
    session.send(b"\t\r")
    session.expect("Select config file to add host to:")
    session.send(b"\x1b")
    session.close()
    assert "cancelled-standalone" not in config.read_text()
    session = Session(env, ["add", "z-standalone"])
    session.expect("Select config file to add host to:")
    session.send(b"\r")
    session.expect("Hôtes > Ajouter un hôte")
    session.send(b"\tz.example.org\x13")
    session.confirm_changes()
    session.close()
    assert json.loads(cli(env, "info", "z-standalone"))["ok"]
    session = Session(env, ["edit", "z-standalone"])
    session.expect("Hôtes > Modifier")
    session.send(b"-discarded\x1b")
    session.expect("Abandonner les modifications")
    session.send(b"\t\r")
    session.close()
    assert "-discarded" not in config.read_text()

    session = Session(env)
    session.expect("Last Attempt")
    session.send(b"\tm")
    session.expect("Select destination config file")
    session.send(b"\r")
    session.confirm_changes()
    session.expect("Last Attempt")
    assert "Host b-server" in target.read_text(), target.read_text()
    session.send(b"f")
    session.expect("Port Forwarding")
    session.send(b"\x1b[C\x1b")
    session.expect("Abandonner les modifications")
    session.send(b"\x1b")
    session.expect("Remote (-R)")
    session.send(b"\x1b")
    session.expect("Abandonner les modifications")
    session.send(b"\t\r")
    session.expect("Last Attempt")
    session.send(b"f")
    session.expect("Port Forwarding")
    session.send(b"\x1b[C\x1b[C\t1080\t\x0c")
    session.expect("SSH exit 37: b-server")
    assert b"SSH_TAKEOVER" in session.output
    session.send(b"\x03")
    session.close()
    arguments = (home/"ssh-args").read_text().splitlines()
    assert "-D" in arguments and "1080" in arguments[arguments.index("-D")+1], arguments
    history = json.loads((home/".config/ssho/sshm_history.json").read_text())
    assert history["connections"]["b-server"]["port_forwarding"]["type"] == "dynamic"
    print("PASS Include file move and saved SOCKS forwarding")

    # Real owned child: save is inert, UI stays interactive, F7 owns stable IDs.
    tunnel_root = home/"active-config"
    tunnel_root.write_text("Host active-host\n HostName localhost\n RemoteCommand configured-command\n RequestTTY force\n")
    mode.write_text("wait")
    (home/"ssh-args").unlink(missing_ok=True)
    session = Session(env, ["-c", str(tunnel_root)])
    session.expect("Last Attempt")
    session.send(b"\x1b[17~a")
    session.expect("Profile name")
    session.send(b"owned\t\x1b[C\x1b[C\t1081\x14\x13")
    session.expect("Profile saved")
    session.expect("Tunnel only")
    assert not (home/"ssh-args").exists()
    session.send(b"\x0c")
    session.expect("Owned tunnels")
    session.expect("Active")
    args = (home/"ssh-args").read_text().splitlines()
    assert all(x in args for x in ["-N", "-T", "BatchMode=yes", "ExitOnForwardFailure=yes", "RemoteCommand=none", "RequestTTY=no"])
    pid = int((home/"ssh-args.pid").read_text())
    assert b"TUNNEL_STDOUT_MUST_NOT_REACH_UI" not in session.output
    session.send(b"q")  # returning neither quits nor stops the tunnel
    session.expect("Port Forwarding Setup")
    os.kill(pid, 0)
    session.send(b"\x14")
    session.expect("Shell session")
    session.send(b"\x1b[18~")
    session.expect("Owned tunnels")
    session.send(b"\x03")
    session.expect("Shell session")
    os.kill(pid, 0)
    session.send(b"\x14")
    session.expect("Tunnel only")
    session.send(b"\x1b[18~")
    session.expect("Owned tunnels")
    session.send(b"s")
    session.expect("Stopped")
    try:
        os.kill(pid, 0)
        raise AssertionError("stopped tunnel survived")
    except ProcessLookupError:
        pass
    session.send(b"\x1b")
    session.expect("Port Forwarding Setup")
    mode.write_text("bind")
    session.send(b"\x0c")
    session.expect("Address already in use")
    session.expect("Error")
    session.send(b"\x1b")
    session.expect("Port Forwarding Setup")
    mode.write_text("wait")
    session.send(b"\x0c")
    session.expect("Owned tunnels")
    pid = int((home/"ssh-args.pid").read_text())
    # Quitting is a decision taken from the list: it names the tunnel, keeps the
    # safe choice focused and stops the process only when that is chosen.
    session.send(b"q")
    session.expect("Port Forwarding Setup")
    session.send(b"\x1b")
    if "Abandonner les modifications" in session.screen.text():
        session.send(b"\t\r")
    session.expect("Tunnel profiles")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"\x03")
    session.expect("Quitter ssho ?")
    session.expect("1 tunnel(s) actif(s) : ils seront arrêtés")
    session.send(b"\r")
    session.expect("Last Attempt")
    os.kill(pid, 0)
    session.send(b"\x03")
    session.expect("Quitter ssho ?")
    session.send(b"\t\r")
    session.close()
    try:
        os.kill(pid, 0)
        raise AssertionError("quit tunnel survived")
    except ProcessLookupError:
        pass
    hist = json.loads((home/".config/ssho/sshm_history.json").read_text())["connections"]["active-host"]
    assert hist["connect_count"] == 3
    assert hist["last_result"]["exit_code"] == 137
    assert hist.get("last_successful_completion") is None
    mode.write_text("37")
    print("PASS mode save/no launch, active UI, flags, no stdout corruption, bind cause, explicit stop, quit cancel and stop+quit with real children/history")

    # Named profiles: saves are data-only; only Ctrl+L runs the temporary SSH.
    profile_root = home/"profiles-config"
    (home/".ssh/profile-key").write_text("temporary test identity\n")
    profile_original = "Host profile-host\n HostName profile.example\n User deploy\n IdentityFile ~/.ssh/profile-key\n RemoteCommand keep-command\n RequestTTY force\n LocalForward 7000 localhost:70\n"
    profile_root.write_text(profile_original)
    profile_file = home/".config/ssho/tunnel_profiles.json"
    history_file = home/".config/ssho/sshm_history.json"
    history_before = history_file.read_bytes()
    (home/"ssh-args").unlink()
    session = Session(env, ["-c", str(profile_root)])
    session.expect("Last Attempt")
    session.send(b"\x1b[17~")  # F6 from focused search
    session.expect("Tunnel profiles")
    for name, arrows, local, remote in [("db", b"", b"8080", b"80"), ("remote", b"\x1b[C", b"9090", b"90"), ("socks", b"\x1b[C\x1b[C", b"1080", None)]:
        session.send(b"a")
        session.expect("Profile name")
        session.send(name.encode()+b"\t"+arrows+b"\t"+local+b"\t")
        if remote is not None:
            session.send(b"\x15::1\t"+remote+b"\t")
        session.send(b"127.0.0.1\x13")
        session.expect("Profile saved")
        assert not (home/"ssh-args").exists(), "Save launched SSH"
        assert history_file.read_bytes() == history_before
        assert profile_root.read_text() == profile_original
        session.send(b"\r")
        session.expect("Profile saved")
        assert not (home/"ssh-args").exists(), "Enter launched SSH"
        session.send(b"\x1b")
        session.expect("Tunnel profiles")
    stored = json.loads(profile_file.read_text())["configs"][str(profile_root)]["hosts"]["profile-host"]["profiles"]
    assert set(stored) == {"db", "remote", "socks"}
    session.send(b"\x1b[Hr\x15socks\x13")
    session.expect("already exists")
    session.send(b"\x15db-renamed\x13")
    session.expect("Profile saved")
    session.send(b"\x1b")
    session.expect("Tunnel profiles")
    # Launch all profiles via the same foreground SSH/UI-return path.
    for index, flag, specification in [(0, "-L", "127.0.0.1:8080:[::1]:80"), (1, "-R", "127.0.0.1:9090:[::1]:90"), (2, "-D", "127.0.0.1:1080")]:
        session.send(b"\x1b[H"+b"\x1b[B"*index+b"\x0c")
        session.expect("SSH exit 37: profile-host")
        arguments = (home/"ssh-args").read_text().splitlines()
        assert arguments[arguments.index(flag)+1] == specification, arguments
        assert arguments[-1] == "profile-host"
        session.send(b"\x1b[17~")
        session.expect("Tunnel profiles")
    before_delete = profile_file.read_bytes()
    session.send(b"\x1b[Fd")
    session.expect("Delete profile 'socks'?")
    session.send(b"\x1b")
    assert profile_file.read_bytes() == before_delete
    session.send(b"d\r")
    session.expect("Tunnel profiles")
    stored = json.loads(profile_file.read_text())["configs"][str(profile_root)]["hosts"]["profile-host"]["profiles"]
    assert set(stored) == {"db-renamed", "remote"}
    session.send(b"aunsaved\x1b")
    session.expect("Abandonner les modifications")
    session.send(b"\t\r")
    session.expect("Tunnel profiles")
    # f still resumes the last SOCKS launch, even after its named profile is deleted.
    session.send(b"f")
    session.expect("Dynamic (-D)")
    session.expect("1080")
    session.send(b"\x1b")
    session.expect("Tunnel profiles")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"\te")
    session.expect("Hôtes > Modifier")
    session.send(b"\x15renamed-host\x13")
    session.confirm_changes()
    session.expect("Last Attempt")
    session.send(b"\x1b[17~")
    session.expect("Tunnel profiles")
    session.expect("db-renamed")
    stored_hosts = json.loads(profile_file.read_text())["configs"][str(profile_root)]["hosts"]
    assert "profile-host" not in stored_hosts
    assert set(stored_hosts["renamed-host"]["profiles"]) == {"db-renamed", "remote"}
    assert profile_root.read_text() == profile_original.replace("Host profile-host", "Host renamed-host")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"\x03")
    session.close()
    print("PASS named profiles: CRUD/collision/discard, Save and Enter never launch, local/remote/SOCKS Ctrl+L return, history draft and SSH alias rename")

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
    session.expect("Error: reading SSH config")
    session.expect("(os error 2)")  # No-such-file text may wrap across table borders
    session.expect("newest-user")
    reload_root.write_text(f"Include {reload_parts}/*\nHost reload-root\n HostName 127.0.0.4\n")
    reload_keep.unlink()
    session.send(b"\x1b[15~")
    session.expect("No matching hosts")
    session.expect("Filtres : reload-keep")
    assert "SSH Host Information" not in session.screen.text()
    session.send(b"\x7f")  # Backspace from the table clears the retained query.
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
    session.send(b"\x1bOQfavoris seuls\r")
    session.expect("Filtres : Favoris seuls")
    assert "note-a" not in session.screen.text()
    session.send(b"*")
    session.expect("No favorite matches")
    session.send(b"\x1bOQfavoris seuls\r")
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
    session.expect("Abandonner les modifications")
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

    # Combines inherited effective values, notes, favorite and status filters.
    mode.write_text("37")
    search_root = home/"advanced search config"
    search_root.write_text("Host *\n User inherited-user\n# Tags: prod\nHost search-a\n HostName 127.0.0.1\n Port 1\nHost search-b\n HostName 127.0.0.2\n Port 1\n")
    session = Session(env, ["-c", str(search_root)])
    session.expect("Last Attempt")
    session.send(b"search-a\t*N")
    session.expect("Personal note")
    assert b"\x1b[?2004h" in session.output, "UI must enable bracketed paste"
    session.send(b"\x1b[200~" + 'Release café 東京\nqae'.encode() + b"\x1b[201~")
    session.expect("qae")
    session.send(b"\x13")
    session.expect("Last Attempt")
    session.send(b"\t\x15")  # Focus search and clear old query.
    expression = 'tag:prod AND user:inherited-user ET "café 東京" is:favorite is:unknown'
    session.send(b"\x1b[200~" + expression.replace(" AND ", "\nAND ").encode() + b"\x1b[201~")
    session.expect("Hosts: 1/2")
    session.send(b"\r")
    session.expect("SSH exit 37: search-a")
    session.expect(expression)
    assert b"\x1b[?2004l" in session.output, "SSH takeover must disable bracketed paste"
    assert session.output.count(b"\x1b[?2004h") >= 2, "UI return must enable bracketed paste again"
    assert (home/"ssh-args").read_text().splitlines()[-1] == "search-a"
    session.send(b"\x1b[15~")
    session.expect("Hosts: 1/2")
    session.expect(expression)
    # A real unrelated form edit must keep inherited user + effective address filters.
    session.send(b"\x15")
    inherited_expression = "user:inherited-user AND 127.0.0.1"
    session.send(inherited_expression.encode() + b"\te")
    session.expect("Hôtes > Modifier")
    # General fields: name -> hostname -> user -> port. Port is unrelated to query.
    session.send(b"\t\t\t\x152\x13")
    session.confirm_changes()
    session.expect("Last Attempt")
    session.expect(inherited_expression)
    session.expect("Hosts: 1/2")
    assert json.loads(cli(env, "-c", str(search_root), "info", "search-a"))["result"]["target"]["port"] == 2
    before_paste = session.output.count(b"SSH_TAKEOVER")
    session.send(b"\x1b[200~qae\n\r\x1b[201~")
    session.expect("Last Attempt")
    assert session.process.poll() is None
    assert session.output.count(b"SSH_TAKEOVER") == before_paste
    # Ctrl+U clears the search text; Esc only leaves the edit and keeps the filter.
    session.send(b"\t\x15")
    session.send(b"is:unknown\tp")  # Start real probes to disposable loopback closed ports.
    session.expect("No matching hosts")
    session.send(b"\t\x15")
    session.send(b"is:offline")
    session.expect("Hosts: 2/2")
    session.expect("Offline")
    session.send(b"\x15")
    session.send(b"alpha AND ET beta")
    session.expect("conjunction")
    session.send(b"\x15")
    session.send(b'user:inherited-user file:"advanced search config"')
    session.expect("Hosts: 2/2")
    session.send(b"\x03")
    session.close()

    assert b"\x1b[?2004l" in session.output[session.mark:], "UI exit must disable bracketed paste"
    saved_note = json.loads(metadata_file.read_text())["configs"][str(search_root.resolve())]["hosts"]["search-a"]["note"]
    assert saved_note == "Release café 東京\nqae", saved_note
    print("PASS advanced filters, effective user, notes/status transitions, AND/ET, SSH/F5 query preservation and safe bracketed paste passed")

    # Duplicates a secondary alias, preserving source declaration and native values.
    duplicate_root = home/"duplicate-config"
    duplicate_include = home/"duplicate-include"
    duplicate_key = home/"duplicate-key"
    duplicate_key.write_text("fixture")
    duplicate_root.write_text(f"Host *\n User inherited-copy-user\n Port 2207\nInclude {duplicate_include}\n# original bytes and aliases\nHost primary secondary\n HostName original.example\n IdentityFile {duplicate_key}\n IdentityFile /missing/extra-key\n Compression yes\n ServerAliveInterval 17\nHost taken\n HostName taken.example\n")
    duplicate_include.write_text("# unchanged destination prefix\n# Tags: hidden\nHost secret-copy-name\n HostName hidden.example\n")
    duplicate_before = duplicate_root.read_bytes()
    include_before = duplicate_include.read_bytes()
    def native_duplicate(alias):
        result = subprocess.run(["/usr/bin/ssh", "-G", "-F", str(duplicate_root), alias], capture_output=True, check=True, text=True)
        return [line for line in result.stdout.splitlines() if not line.startswith("host ")]
    native_before = native_duplicate("secondary")
    session = Session(env, ["-c", str(duplicate_root)])
    session.expect("Last Attempt")
    session.send(b"secondary")
    session.expect("Hosts: 1/4")
    # The appended palette command works directly from search, preserving its query.
    session.send(b"\x1bOQdupliquer\r")
    session.expect("Select destination config file")
    session.send(b"\x1b")
    session.expect("secondary")
    session.send(b"\tD")
    session.expect("Select destination config file")
    session.send(b"\x1b[B\r")
    session.expect("Hôtes > Dupliquer")
    session.expect("/missing/extra-key")
    session.send(b"\x13")
    session.expect("host name is required")
    session.send(b"taken\x13")
    session.expect("already exists")
    assert duplicate_root.read_bytes() == duplicate_before
    assert duplicate_include.read_bytes() == include_before
    session.send(b"\x15secret-copy-name\x13")
    session.expect("already exists")
    session.send(b"\x15fresh-copy\x13")
    session.confirm_changes()
    session.expect("Last Attempt")
    session.send(b"i")
    session.expect("SSH Host Information: fresh-copy")
    session.expect("inherited-copy-user")
    session.expect("2207")
    assert duplicate_root.read_bytes() == duplicate_before
    assert duplicate_include.read_bytes().endswith(include_before)
    assert native_duplicate("fresh-copy") == native_before
    assert native_duplicate("secondary") == native_before
    # Same destination file and cancellation both keep the source intact.
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"D")
    session.expect("Select destination config file")
    session.send(b"\x1b[B\r")
    session.expect("Hôtes > Dupliquer")
    session.send(b"cancelled-copy\x1b")
    session.expect("Abandonner")
    session.send(b"\x1b")
    session.expect("cancelled-copy")
    session.send(b"\x1b")
    session.expect("Abandonner")
    session.send(b"\t\r")
    session.expect("Select destination config file")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"q")
    session.close()
    assert "cancelled-copy" not in duplicate_include.read_text()
    assert duplicate_root.read_bytes() == duplicate_before
    print("PASS D/palette secondary-alias duplication, Include/same-file selection, required new name, global/hidden collision, extra keys, inherited native ssh-G values and cancel/discard")

    # Reviewer regression: append-after-fallback would silently change specific/22.
    fallback_root = home/"copy-trailing-fallback"
    fallback_text = "# Tags: source-tag\n# source comment\nHost source secondary\n HostName source.example\n User specific\n Port 22\nHost other\n HostName other.example\n User other-user\n Port 2244\nHost *\n User fallback\n Port 2207\n"
    fallback_root.write_text(fallback_text)
    def native_fallback(alias):
        result = subprocess.run(["/usr/bin/ssh", "-G", "-F", str(fallback_root), alias], capture_output=True, check=True, text=True)
        return [line for line in result.stdout.splitlines() if not line.startswith("host ")]
    fallback_source = native_fallback("secondary")
    fallback_other = native_fallback("other")
    session = Session(env, ["-c", str(fallback_root)])
    session.expect("Last Attempt")
    session.send(b"secondary\tD")
    session.expect("Select destination config file")
    session.send(b"\r")
    session.expect("Hôtes > Dupliquer")
    session.send(b"specific-copy\x13")
    session.confirm_changes()
    session.expect("Last Attempt")
    session.send(b"i")
    session.expect("SSH Host Information: specific-copy")
    session.expect("User: specific")
    session.expect("Port: 22")
    assert native_fallback("specific-copy") == fallback_source
    assert native_fallback("secondary") == fallback_source
    assert native_fallback("other") == fallback_other
    assert fallback_root.read_text().endswith(fallback_text)
    assert "Host specific-copy\n    HostName source.example\n    User specific\n    Port 22\n" in fallback_root.read_text()
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"q")
    session.close()

    # Earlier root defaults make a later Include incompatible: reject before backups.
    rejected_root = home/"copy-rejected-root"
    rejected_include = home/"copy-rejected-include"
    rejected_text = f"Host source secondary\n HostName source.example\n User specific\n Port 22\nHost *\n User fallback\n Port 2207\n Include {rejected_include}\n"
    rejected_root.write_text(rejected_text)
    rejected_include.write_text("# target comment\nHost target-existing\n HostName target.example\n")
    rejected_before = rejected_include.read_bytes()
    backup_directory = home/".config/ssho/backups"
    backups_before = {path.name: path.read_bytes() for path in backup_directory.iterdir()} if backup_directory.exists() else {}
    session = Session(env, ["-c", str(rejected_root)])
    session.expect("Last Attempt")
    session.send(b"secondary\tD")
    session.expect("Select destination config file")
    session.send(b"\x1b[B\r")
    session.expect("Hôtes > Dupliquer")
    session.send(b"rejected-copy\x13")
    session.expect("destination precedence")
    session.expect("choose an earlier destination")
    assert rejected_root.read_text() == rejected_text
    assert rejected_include.read_bytes() == rejected_before
    assert {path.name: path.read_bytes() for path in backup_directory.iterdir()} == backups_before
    session.send(b"\x1b")
    session.expect("Abandonner")
    session.send(b"\t\r")
    session.expect("Select destination config file")
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"q")
    session.close()
    print("PASS native trailing-fallback insertion preserves specific/22, original tags/aliases/other targets; incompatible Include rejects unchanged with no backups")

    # Previews, exact-byte concurrency and guided provenance-aware restoration.
    restore_base = home/"restore-data"
    restore_app = restore_base/"ssho"
    restore_app.mkdir(parents=True)
    (restore_app/"config.json").write_bytes((home/".config/ssho/config.json").read_bytes())
    restore_env = dict(env, XDG_CONFIG_HOME=str(restore_base), APPDATA=str(restore_base))
    restore_root = home/"restore-config"
    restore_original = b"# original CRLF\r\nHost restore-me alternate\r\n HostName 127.0.0.1\r\n Port 1"
    restore_root.write_bytes(restore_original)
    restore_root.chmod(0o640)
    session = Session(restore_env, ["-c", str(restore_root)])
    session.expect("Last Attempt")
    session.send(b"restore-me\te")
    session.expect("Hôtes > Modifier")
    session.send(b"\x15changed-name\x13")
    session.expect("Review SSH changes")
    session.expect("-Host restore-me alternate")
    session.expect("+Host changed-name alternate")
    assert restore_root.read_bytes() == restore_original
    assert not (restore_app/"backups").exists()
    session.send(b"\x1b[200~\r\nq\x1b[201~")
    session.expect("Review SSH changes")
    assert restore_root.read_bytes() == restore_original
    session.confirm_changes()
    session.expect("Last Attempt")
    edited = restore_root.read_bytes()
    assert edited != restore_original
    session.send(b"\x1b[19~")
    session.expect("Backups — explicit recorded destinations")
    session.expect(str(restore_root))
    session.send(b"\r")
    session.expect("Review restoration")
    session.send(b"\x1b")
    session.expect("Backups")
    assert restore_root.read_bytes() == edited
    session.send(b"\r")
    session.expect("Review restoration")
    concurrent = edited + b"\r\n# external edit\r\n"
    restore_root.write_bytes(concurrent)
    session.confirm_changes()
    session.expect("changed")
    assert restore_root.read_bytes() == concurrent
    assert len(list((restore_app/"backups").glob("version-*.json"))) == 1
    session.send(b"\x1b")
    session.expect("Backups")
    session.send(b"\r")
    session.expect("Review restoration")
    session.confirm_changes()
    session.expect("Last Attempt")
    assert restore_root.read_bytes() == restore_original
    assert restore_root.stat().st_mode & 0o777 == 0o640
    versions = [json.loads(path.read_text()) for path in (restore_app/"backups").glob("version-*.json")]
    assert len(versions) == 2
    assert any(bytes(record["bytes"]) == concurrent for record in versions)
    assert all(record["destination"] == str(restore_root) for record in versions)
    session.send(b"\x1b[19~")
    session.expect("Backups")
    session.send(b"\x1b[B"*10 + b"\r")
    session.expect("unknown backup provenance")
    assert restore_root.read_bytes() == restore_original
    session.send(b"\x1b")
    session.expect("Last Attempt")
    session.send(b"\x03")
    session.close()
    print("PASS exact CRLF/multi-alias diff, inert paste/cancel, external-change refusal, F8 trustworthy restore, legacy refusal, pre-restore backup and mode preservation")

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
        session.expect("Hôtes > Ajouter un hôte")
        session.send(b"first\tfirst.example.org\x13")
        session.confirm_changes()
        session.expect("Last Attempt")
        session.send(b"\tq")
        session.close()
    standalone_missing = home/"standalone-missing-config"
    assert not standalone_missing.exists()
    session = Session(env,["-c",str(standalone_missing),"add"])
    session.expect("Hôtes > Ajouter un hôte")
    session.send(b"standalone\tstandalone.example.org\x13")
    session.confirm_changes()
    session.close()
    assert "Host standalone" in standalone_missing.read_text()
    print("PASS first host creation from empty/missing roots returns to browser; standalone Add accepts missing root")

    cancelled = home/"empty-cancel-config"
    cancelled.write_text("")
    session = Session(env,["-c",str(cancelled)])
    session.expect("Would you like to add")
    session.send(b"y\n")
    session.expect("Hôtes > Ajouter un hôte")
    session.send(b"\x1b")
    session.close(1)
    assert cancelled.read_text() == ""
    print("PASS cancelled first host creation preserves empty config and exits 1")

print("All PTY checks passed")
