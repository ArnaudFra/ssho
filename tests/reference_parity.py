#!/usr/bin/env python3
"""Compare read-only CLI behavior with an optional locally built Go reference."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

if len(sys.argv) < 2:
    raise SystemExit("usage: reference_parity.py /path/to/sshm [target/release/ssho] [sshm_source_directory]")

reference = Path(sys.argv[1]).resolve()
rust = Path(sys.argv[2] if len(sys.argv) > 2 else "target/release/ssho").resolve()
source = Path(sys.argv[3] if len(sys.argv) > 3 else "../sshm")
fixtures = []
for file in sorted((source/"internal/config").glob("*test.go")):
    for text in re.findall(r"`([^`]+)`", file.read_text()):
        if re.search(r"(?im)^\s*Host\s", text) and "HostName" in text and len(text) < 6000 and "%s" not in text and "${" not in text:
            if text not in [fixture[1] for fixture in fixtures]:
                fixtures.append((file.name, text))

fixtures.append(("explicit info fixture", '''# Tags: work, prod
Host prod alias !excluded
 HostName db.example.org
 User deploy
 Port 2222
 IdentityFile "~/.ssh/a key"
 IdentityFile ~/.ssh/second
 ProxyCommand ssh -W %h:%p bastion
 SetEnv "KEY=two words" OTHER=ok

Match host prod
 User conditional

# Tags: hidden
Host secret
 HostName internal

Host plain
 HostName localhost
'''))


def result(binary, config, args, env):
    process = subprocess.run([str(binary), "-c", str(config), *args], env=env, capture_output=True, text=True, timeout=10)
    try:
        value = json.loads(process.stdout)
    except ValueError:
        value = process.stdout.strip()
    return process.returncode, value


checks = 0
with tempfile.TemporaryDirectory(prefix="ssho-reference-") as directory:
    home = Path(directory)
    env = dict(os.environ, HOME=directory, USERPROFILE=directory, XDG_CONFIG_HOME=str(home/".config"), APPDATA=str(home/".config"))
    config = home/"config"
    for origin, text in fixtures:
        config.write_text(text)
        commands = [["search", "--format", "json"]]
        if origin == "explicit info fixture":
            commands.extend([["info", name] for name in ["prod", "alias", "secret", "plain", "absent"]])
            commands.extend([["search", "work", "--tags", "--format", "json"], ["search", "prod", "--names", "--format", "json"], ["search", "--format", "simple"], ["search", "--format", "table"]])
        for args in commands:
            expected = result(reference, config, args, env)
            actual = result(rust, config, args, env)
            assert actual == expected, f"{origin} {args}\nexpected: {expected!r}\nactual: {actual!r}\nfixture:\n{text}"
            checks += 1

print(f"PASS {checks} CLI comparisons across {len(fixtures)} fixtures against the Go reference")
