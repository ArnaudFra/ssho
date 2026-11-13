#!/usr/bin/env python3
"""Write the notes of a release from the commits since the previous tag.

Commits are grouped by the type of their subject, `prefix (type): message`:
features, bug fixes, others. Commits that do not change the program (docs,
test, ci, chore, build) are left out. The notes end with how to install the
archives that were built.

Usage: scripts/release-notes.py TAG OWNER/REPO [ARCHIVE...]
"""
import re
import subprocess
import sys

SECTIONS = [('Features', {'feat'}), ('Bug fixes', {'fix'})]
LEFT_OUT = {'docs', 'test', 'ci', 'chore', 'build'}
PLATFORMS = {
    'Linux_x86_64': 'Linux x86_64',
    'Linux_arm64': 'Linux ARM64',
    'Linux_armv7': 'Linux ARMv7 (32-bit)',
    'Linux_armv6': 'Linux ARMv6 (32-bit)',
    'Linux_i386': 'Linux x86 (32-bit)',
    'Darwin_arm64': 'macOS Apple Silicon',
    'Darwin_x86_64': 'macOS Intel',
    'Windows_x86_64': 'Windows x86_64',
    'Windows_i386': 'Windows x86 (32-bit)',
}
SUBJECT = re.compile(r'^(?P<prefix>[^():]+?)(?: \((?P<type>[a-z]+)\))?: (?P<message>.+)$')


def git(*args):
    return subprocess.run(['git', *args], check=True, capture_output=True, text=True).stdout.strip()


def previous_tag(tag):
    try:
        return git('describe', '--tags', '--abbrev=0', f'{tag}^')
    except subprocess.CalledProcessError:
        return None


def changes(previous, tag):
    """Commit subjects sorted into the sections, oldest first."""
    groups = {title: [] for title, _ in SECTIONS}
    groups['Others'] = []
    log = git('log', '--no-merges', '--reverse', '--format=%h%x09%s', f'{previous}..{tag}')
    for line in filter(None, log.split('\n')):
        commit, subject = line.split('\t', 1)
        match = SUBJECT.match(subject)
        prefix, kind, message = (match['prefix'], match['type'], match['message']) if match else ('', None, subject)
        if re.match(r'version \d', message):
            continue
        if kind is None and prefix.split('/')[0] in ('docs', 'github'):
            kind = 'docs'
        if kind in LEFT_OUT:
            continue
        title = next((title for title, kinds in SECTIONS if kind in kinds), 'Others')
        if not message.startswith('macOS'):
            message = message[0].upper() + message[1:]
        groups[title].append(f'- {message} (`{commit}`)')
    return groups


def installation(tag, repository, archives):
    lines = ['## Installation', '', '| Platform | Archive |', '| --- | --- |']
    known = {archive: key for archive in archives for key in PLATFORMS if f'_{key}.' in archive}
    for key, label in PLATFORMS.items():
        lines += [f'| {label} | `{archive}` |' for archive, found in sorted(known.items()) if found == key]
    example = next((archive for archive in archives if '_Linux_x86_64.' in archive), None)
    if example:
        lines += ['', '```sh',
                  f'curl -fsSLO https://github.com/{repository}/releases/download/{tag}/{example}',
                  f'tar -xzf {example} ssho',
                  'mkdir -p ~/.local/bin && mv ssho ~/.local/bin/',
                  '```']
    lines += ['', '`checksums.txt` holds the SHA-256 of every archive: '
              '`sha256sum --check --ignore-missing checksums.txt`. '
              'The OpenSSH client (`ssh`) must be in the `PATH`.']
    return lines


def notes(tag, repository, archives):
    previous = previous_tag(tag)
    lines = [f'## ssho {tag.lstrip("v")}', '']
    if previous is None:
        lines.append(f'First release. See the [README](https://github.com/{repository}#readme) for what ssho does.')
    else:
        sections = [(title, entries) for title, entries in changes(previous, tag).items() if entries]
        if not sections:
            lines.append('Maintenance release: no change in the program itself.')
        for number, (title, entries) in enumerate(sections):
            lines += ([''] if number else []) + [f'### {title}', ''] + entries
    lines += [''] + installation(tag, repository, archives)
    if previous is not None:
        lines += ['', f'**Full changelog**: https://github.com/{repository}/compare/{previous}...{tag}']
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    sys.stdout.write(notes(sys.argv[1], sys.argv[2], [name.rsplit('/', 1)[-1] for name in sys.argv[3:]]))
