#!/usr/bin/env python3
"""Distribution contract tests; all writes are confined to an owned temporary HOME."""
import hashlib
import io
import os
import re
import shutil
import sys
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / 'target/distribution-tests'
TARGET = 'x86_64-unknown-linux-gnu'
ASSET = f'ssho-{TARGET}.tar.gz'
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
MISMATCH_VERSION = '0.2.0' if VERSION != '0.2.0' else '0.1.0'


class Distribution(unittest.TestCase):
    def setUp(self):
        WORK.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='dist-test-', dir=WORK)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / 'home with spaces'
        self.home.mkdir()
        self.env = dict(os.environ, HOME=str(self.home), TMPDIR=str(self.root),
                        PATH='/usr/bin:/bin', LC_ALL='C')
        for key in ('SSHO_INSTALL_DIR', 'SSHO_REPOSITORY', 'TAR_OPTIONS'):
            self.env.pop(key, None)
        self.binary = self.root / 'source with spaces' / 'ssho'
        self.binary.parent.mkdir()
        self.binary.write_text('#!/bin/sh\nprintf "ssho 0.1.0\\n"\n')
        self.binary.chmod(0o755)
        self.default = self.home / '.local/bin/ssho'

    def run_script(self, script, *args, ok=True, env=None, error=None):
        result = subprocess.run(['sh', str(ROOT / script), *map(str, args)],
                                env=env or self.env, cwd=self.root,
                                text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
        if error is not None:
            self.assertIn(error, result.stderr)
        return result

    def install(self, *args, **kwargs):
        return self.run_script('install.sh', *args, **kwargs)

    def package(self, *args, ok=True, output=None, error=None):
        output = output or self.root / 'release output'
        self.run_script('scripts/package-release.sh', '--binary', self.binary,
                        '--target', TARGET, '--version', VERSION,
                        '--output-dir', output, *args, ok=ok, error=error)
        return Path(output) / ASSET

    def checksum(self, archive):
        Path(str(archive) + '.sha256').write_text(
            hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')

    def archive(self, entries=None, manifest=None):
        archive = self.root / ASSET
        entries = entries if entries is not None else [
            ('ssho', self.binary.read_bytes(), tarfile.REGTYPE),
            ('LICENSE', b'MIT', tarfile.REGTYPE),
            ('INSTALLATION.md', b'Instructions', tarfile.REGTYPE),
            ('MANIFEST', manifest or f'format=1\nversion=0.1.0\ntarget={TARGET}\n'.encode(), tarfile.REGTYPE)]
        with tarfile.open(archive, 'w:gz') as output:
            for name, data, kind in entries:
                entry = tarfile.TarInfo(name)
                entry.type = kind
                entry.mode = 0o755
                entry.linkname = '/tmp/ssho-should-never-be-written'
                entry.size = len(data) if kind == tarfile.REGTYPE else 0
                output.addfile(entry, io.BytesIO(data) if entry.size else None)
        self.checksum(archive)
        return archive

    def test_local_install_permissions_and_data_preserving_uninstall(self):
        for name in ('.ssh/config', '.config/ssho/config.json', '.config/sshm/keys', '.profile'):
            file = self.home / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text('preserve')
        self.install('--binary', self.binary)
        self.assertEqual(self.default.read_bytes(), self.binary.read_bytes())
        self.assertEqual(self.default.stat().st_mode & 0o777, 0o755)
        self.install('--uninstall')
        self.assertFalse(self.default.exists())
        self.install('--uninstall')
        for name in ('.ssh/config', '.config/ssho/config.json', '.config/sshm/keys', '.profile'):
            self.assertEqual((self.home / name).read_text(), 'preserve')

    def test_destination_precedence_and_literal_shell_characters(self):
        env = dict(self.env, SSHO_INSTALL_DIR=str(self.root / 'env bin'))
        literal = self.root / 'bin $(touch INJECTED) `touch INJECTED2` ; [x]'
        prefix = self.root / 'prefix with spaces'
        self.install('--binary', self.binary, '--prefix', prefix, '--bin-dir', literal, env=env)
        self.assertTrue((literal / 'ssho').is_file())
        self.assertFalse((self.root / 'INJECTED').exists())
        self.install('--binary', self.binary, '--prefix', prefix, env=env)
        self.assertTrue((prefix / 'bin/ssho').is_file())
        self.install('--binary', self.binary, env=env)
        self.assertTrue((self.root / 'env bin/ssho').is_file())
        self.assertFalse(self.default.exists())

    def test_collision_requires_replace(self):
        self.install('--binary', self.binary)
        self.binary.write_text('#!/bin/sh\necho replacement\n')
        self.install('--binary', self.binary, ok=False, error='already exists')
        self.assertNotEqual(self.default.read_bytes(), self.binary.read_bytes())
        self.install('--binary', self.binary, '--replace')
        self.assertEqual(self.default.read_bytes(), self.binary.read_bytes())
        self.assertEqual(list(self.default.parent.iterdir()), [self.default])

    def test_destination_symlinks_directories_and_ancestors_refused(self):
        self.default.parent.mkdir(parents=True)
        self.default.symlink_to(self.binary)
        for args in [('--binary', self.binary), ('--binary', self.binary, '--replace'), ('--uninstall',)]:
            self.install(*args, ok=False, error='refusing symlink')
        self.assertTrue(self.default.is_symlink())
        self.default.unlink()
        self.default.mkdir()
        self.install('--binary', self.binary, '--replace', ok=False, error='refusing non-file')
        self.install('--uninstall', ok=False, error='refusing non-file')
        ancestor = self.root / 'alias'
        ancestor.symlink_to(self.home, target_is_directory=True)
        self.install('--binary', self.binary, '--bin-dir', ancestor / 'new/bin', ok=False, error='symlink destination component')
        self.assertFalse((self.home / 'new').exists())

    def test_invalid_sources_and_arguments_do_not_publish(self):
        link = self.root / 'ssho'
        link.symlink_to(self.binary)
        for args, error in [
                (('--binary', link), 'not a symlink'),
                (('--binary', self.root), 'must be named ssho'),
                (('--binary', self.binary, '--archive', 'x'), 'choose exactly one'),
                (('--binary', self.binary, '--bin-dir', ''), 'missing value'),
                (('--binary',), 'missing value'), (('--unknown',), 'unknown option'),
                (('--uninstall', '--binary', self.binary), 'destination options only'),
                (('--repo', 'owner/repo;bad'), 'invalid repository'),
                (('--version', '$(touch INJECTED)'), 'invalid version')]:
            with self.subTest(args=args):
                self.install(*args, ok=False, error=error)
        self.binary.chmod(0o644)
        self.install('--binary', self.binary, ok=False, error='executable regular file')
        self.assertFalse(self.default.exists())

    def test_path_shadow_warning(self):
        old = self.root / 'old bin'
        old.mkdir()
        other = old / 'ssho'
        other.write_bytes(self.binary.read_bytes())
        other.chmod(0o755)
        result = self.install('--binary', self.binary, env=dict(self.env, PATH=f'{old}:/usr/bin:/bin'))
        self.assertIn('PATH', result.stderr)
        self.assertIn(str(other), result.stderr)
        self.assertEqual(other.read_bytes(), self.binary.read_bytes())

    def test_packaging_contract_roundtrip_and_no_execution(self):
        self.binary.write_text('#!/bin/sh\ntouch SHOULD_NOT_EXECUTE\n')
        archive = self.package('--tag', f'v{VERSION}')
        self.assertFalse((self.root / 'SHOULD_NOT_EXECUTE').exists())
        with tarfile.open(archive) as package:
            self.assertEqual(sorted(package.getnames()), ['INSTALLATION.md', 'LICENSE', 'MANIFEST', 'ssho'])
            self.assertTrue(all(item.isfile() for item in package))
            self.assertEqual(package.extractfile('MANIFEST').read(),
                             f'format=1\nversion={VERSION}\ntarget={TARGET}\n'.encode())
        self.assertEqual(Path(str(archive) + '.sha256').read_text(),
                         hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + ASSET + '\n')
        self.install('--archive', archive, '--target', TARGET, '--version', VERSION)
        self.assertEqual(self.default.read_bytes(), self.binary.read_bytes())

    def test_packaging_rejects_mismatch_and_existing_output(self):
        for extra, error in [
                (('--version', MISMATCH_VERSION), 'version must match Cargo.toml'),
                (('--tag', f'v{MISMATCH_VERSION}'), 'tag must be vVERSION'),
                (('--target', 'bad'), 'unsupported target'),
                (('--version', '1;echo BAD'), 'invalid version')]:
            with self.subTest(extra=extra):
                self.package(*extra, ok=False, error=error)
                self.assertFalse((self.root / 'release output').exists())
        self.package()
        before = (self.root / 'release output' / ASSET).read_bytes()
        self.package(ok=False)
        self.assertEqual((self.root / 'release output' / ASSET).read_bytes(), before)
        self.assertFalse(list(self.root.glob('.sshm-package.*')))

    def test_checksum_required_exact_and_verified_before_install(self):
        archive = self.archive()
        checksum = Path(str(archive) + '.sha256')
        for content in ['0' * 64 + '  ' + ASSET + '\n',
                        hashlib.sha256(archive.read_bytes()).hexdigest() + '  other.tar.gz\n',
                        'malformed\n', checksum.read_text() * 2]:
            checksum.write_text(content)
            self.install('--archive', archive, '--target', TARGET, ok=False, error='SHA256 checksum mismatch')
            self.assertFalse(self.default.exists())
        checksum.unlink()
        self.install('--archive', archive, '--target', TARGET, ok=False, error='checksum file is required')

    def test_wrong_manifest_version_target_or_format_refused(self):
        for manifest in [f'format=2\nversion=0.1.0\ntarget={TARGET}\n',
                         'format=1\nversion=0.1.0\ntarget=other\n',
                         f'format=1\nversion=$(touch INJECTED)\ntarget={TARGET}\n']:
            archive = self.archive(manifest=manifest.encode())
            self.install('--archive', archive, '--target', TARGET, ok=False, error='invalid manifest')
        archive = self.archive()
        self.install('--archive', archive, '--version', '0.2.0', '--target', TARGET, ok=False, error='archive version does not match')
        self.install('--archive', archive, '--target', 'aarch64-apple-darwin', ok=False, error='archive must be named')
        self.assertFalse(self.default.exists())

    def test_incomplete_traversal_absolute_duplicate_link_and_extra_members_refused(self):
        base = [('ssho', b'x', tarfile.REGTYPE), ('LICENSE', b'MIT', tarfile.REGTYPE),
                ('INSTALLATION.md', b'doc', tarfile.REGTYPE),
                ('MANIFEST', f'format=1\nversion=0.1.0\ntarget={TARGET}\n'.encode(), tarfile.REGTYPE)]
        variants = [base[:3], base + [base[0]]]
        for name in ('../escape', '/tmp/escape', 'extra', 'ssho/child'):
            variants.append(base + [(name, b'bad', tarfile.REGTYPE)])
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.DIRTYPE, tarfile.FIFOTYPE):
            variants.append([('ssho', b'', kind)] + base[1:])
        for entries in variants:
            with self.subTest(names=[entry[0] for entry in entries]):
                self.install('--archive', self.archive(entries), '--target', TARGET, ok=False, error='archive')
                self.assertFalse(self.default.exists())
        self.assertFalse((self.root / 'escape').exists())

    def test_locks_block_install_uninstall_and_packaging_without_changes(self):
        self.default.parent.mkdir(parents=True)
        lock = self.default.parent / '.ssho-install-lock'
        lock.mkdir()
        self.install('--binary', self.binary, ok=False, error='installation lock unavailable')
        self.install('--uninstall', ok=False, error='installation lock unavailable')
        self.assertTrue(lock.is_dir())
        self.assertFalse(self.default.exists())
        output = self.root / 'locked output'
        Path(str(output) + '.lock').mkdir()
        self.package(output=output, ok=False)
        self.assertFalse(output.exists())

    def test_download_explicit_repo_latest_version_and_failure(self):
        # Stub only the network boundary: all checksum/archive/install logic is real.
        archive = self.archive()
        fakebin = self.root / 'network tools'
        fakebin.mkdir()
        curl = fakebin / 'curl'
        curl.write_text("""#!/usr/bin/python3
import os, pathlib, shutil, sys
args = sys.argv[1:]
expected = ['--fail', '--silent', '--show-error', '--location', '--proto', '=https', '--proto-redir', '=https', '--output']
if args[:9] != expected or len(args) != 11:
    sys.exit(80)
url = args[-1]
with open(os.environ['REQUESTS'], 'a') as log:
    log.write(url + '\\n')
if os.environ.get('NETWORK_FAIL'):
    sys.exit(22)
source = os.environ['ARCHIVE'] + ('.sha256' if url.endswith('.sha256') else '')
shutil.copyfile(source, args[9])
""")
        curl.chmod(0o755)
        requests = self.root / 'requests'
        env = dict(self.env, PATH=f'{fakebin}:/usr/bin:/bin', ARCHIVE=str(archive), REQUESTS=str(requests))
        for version, route in [('0.1.0', 'download/v0.1.0'), ('latest', 'latest/download')]:
            self.install('--repo', 'rust-owner/rust-repository', '--version', version, '--target', TARGET, env=env)
            self.assertEqual(self.default.read_bytes(), self.binary.read_bytes())
            expected = f'https://github.com/rust-owner/rust-repository/releases/{route}/{ASSET}'
            self.assertEqual(requests.read_text().splitlines()[-2:], [expected, expected + '.sha256'])
            self.install('--uninstall')
        self.install('--target', TARGET, env=dict(env, SSHO_REPOSITORY='rust-owner/rust-repository'))
        self.install('--uninstall')
        # Without --repo or SSHO_REPOSITORY the project repository is used.
        self.install('--target', TARGET, env=env)
        default = f'https://github.com/ArnaudFra/ssho/releases/latest/download/{ASSET}'
        self.assertEqual(requests.read_text().splitlines()[-2:], [default, default + '.sha256'])
        self.install('--uninstall')
        self.install('--repo', 'rust-owner/rust-repository', '--target', TARGET,
                     env=dict(env, NETWORK_FAIL='yes'), ok=False)
        self.assertFalse(self.default.exists())
        self.assertFalse(list(self.root.glob('sshm-install.*')))

    def test_hash_tool_failure_cannot_bypass_checksum_or_publish_package(self):
        archive = self.archive()
        Path(str(archive) + '.sha256').write_text('  ' + ASSET + '\n')
        fakebin = self.root / 'failing tools'
        fakebin.mkdir()
        hasher = fakebin / 'sha256sum'
        hasher.write_text('#!/bin/sh\nexit 1\n')
        hasher.chmod(0o755)
        env = dict(self.env, PATH=f'{fakebin}:/usr/bin:/bin')
        self.install('--archive', archive, '--target', TARGET, env=env, ok=False, error='SHA256 tool failed')
        self.assertFalse(self.default.exists())
        output = self.root / 'hash failure output'
        self.run_script('scripts/package-release.sh', '--binary', self.binary,
                        '--target', TARGET, '--version', VERSION, '--output-dir', output,
                        env=env, ok=False, error='SHA256 tool failed')
        self.assertFalse(output.exists())
        self.assertFalse(list(self.root.glob('.sshm-package.*')))

    def test_package_trailing_slash_and_symlink_parent(self):
        output = self.root / 'trailing slash'
        self.package(output=str(output) + '/')
        self.assertTrue((output / ASSET).is_file())
        link = self.root / 'output alias'
        link.symlink_to(self.home, target_is_directory=True)
        self.package(output=link / 'release', ok=False)
        self.assertFalse((self.home / 'release').exists())

    def test_release_suite_tracks_an_isolated_alternate_project_version(self):
        # A future Cargo/tag release must still pass the actual CI package tests.
        project = self.root / 'alternate project'
        for name in ('Cargo.toml', 'LICENSE', 'docs/INSTALLATION.md', 'install.sh',
                     'scripts/package-release.sh', 'tests/distribution.py'):
            destination = project / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)
        manifest = project / 'Cargo.toml'
        alternate_version = '0.2.0' if VERSION != '0.2.0' else '0.3.0'
        manifest.write_text(re.sub(r'^version = "[^"]+"$', f'version = "{alternate_version}"',
                                   manifest.read_text(), count=1, flags=re.MULTILINE))
        cases = ('test_packaging_contract_roundtrip_and_no_execution',
                 'test_packaging_rejects_mismatch_and_existing_output',
                 'test_hash_tool_failure_cannot_bypass_checksum_or_publish_package',
                 'test_package_trailing_slash_and_symlink_parent')
        result = subprocess.run([sys.executable, str(project / 'tests/distribution.py'),
                                 *(f'Distribution.{case}' for case in cases)],
                                cwd=project, env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_nonwritable_destination_fails_without_partial_executable(self):
        if os.geteuid() == 0:
            self.skipTest('permission denial requires an unprivileged test user')
        destination = self.root / 'readonly'
        destination.mkdir()
        destination.chmod(0o555)
        try:
            self.install('--binary', self.binary, '--bin-dir', destination, ok=False)
            self.assertEqual(list(destination.iterdir()), [])
        finally:
            destination.chmod(0o755)


if __name__ == '__main__':
    unittest.main(verbosity=2)
