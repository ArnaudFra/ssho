#!/usr/bin/env python3
"""Common keyboard, focus and return journeys in a real terminal.

Usage: pty_ux.py BINARY NEW_FIXTURE_DIRECTORY. Only owned SSH/config fixtures.
"""
import atexit
import fcntl
import json
import os
from pathlib import Path
import pty
import select
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
APP = ROOT / '.config/ssho'
APP.mkdir(parents=True, exist_ok=True)
CONFIG = ROOT / '.ssh/config'
CONFIG.write_text('Host alpha\n HostName 127.0.0.1\n User declared\n\nHost beta\n HostName 127.0.0.2\n')
ORIGINAL_CONFIG = CONFIG.read_bytes()
BASE = {'ping_on_start': False, 'check_for_updates': False, 'focus_search_on_start': False}
ssh = ROOT / 'bin/ssh'
ssh.write_text('#!' + sys.executable + '\n' + r'''
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with (Path(os.environ['HOME']) / 'ssh.jsonl').open('a') as out:
    out.write(json.dumps(args) + '\n')
if '-G' in args:
    print('hostname 127.0.0.1\nuser native-' + args[-1] + '\nport 2222')
    sys.exit(0)
print('fixture SSH return')
sys.exit(0)
''')
ssh.chmod(0o700)
ENV = dict(os.environ, HOME=str(ROOT), USERPROFILE=str(ROOT),
           XDG_CONFIG_HOME=str(ROOT / '.config'), APPDATA=str(ROOT / '.config'),
           SSH_AUTH_SOCK=str(ROOT / 'absent-agent'), TERM='xterm-256color',
           PATH=str(ROOT / 'bin') + os.pathsep + os.environ['PATH'])
F1, F2, ESC, CTRL_G, CTRL_C, DOWN, UP = b'\x1bOP', b'\x1bOQ', b'\x1b', b'\x07', b'\x03', b'\x1b[B', b'\x1b[A'


def preferences(**key_bindings):
    (APP / 'config.json').write_text(json.dumps(dict(BASE, key_bindings=key_bindings)))


def connections():
    log = ROOT / 'ssh.jsonl'
    return [a for a in map(json.loads, log.read_text().splitlines()) if '-G' not in a] if log.exists() else []


class Session:
    def __init__(self, name, *args, width=120, height=40):
        self.name = name
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
        self.screen = Screen(width, height)
        self.raw = bytearray()
        self.process = subprocess.Popen([str(BINARY), '--no-update-check', *args],
                                        stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                        env=ENV, start_new_session=True,
                                        preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
        atexit.register(self.cleanup)

    def cleanup(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()
        (ROOT / (self.name + '.raw')).write_bytes(self.raw)

    def drain(self, duration=.12):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            ready, _, _ = select.select([self.master], [], [], max(0, end - time.monotonic()))
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.raw.extend(data)
                self.screen.feed(data)
        return self.screen.text()

    def expect(self, text, timeout=5):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if ''.join(text.split()) in ''.join(self.drain(.05).split()):
                return
        raise AssertionError(f'{self.name}: missing {text!r}:\n{self.screen.text()}')

    def absent(self, text):
        screen = self.drain(.3)
        assert ''.join(text.split()) not in ''.join(screen.split()), f'{self.name}: unexpected {text!r}:\n{screen}'

    def send(self, data):
        os.write(self.master, data)
        self.drain()

    def alive(self):
        self.drain(.2)
        assert self.process.poll() is None, f'{self.name}: the application closed unexpectedly'

    def close(self, code=0):
        try:
            assert self.process.wait(timeout=5) == code
            self.drain()
            assert termios.tcgetattr(self.slave) == self.original, f'{self.name}: terminal not restored'
        finally:
            self.cleanup()
            os.close(self.master)
            os.close(self.slave)


def scenario_shortcuts():
    # Default profile: the prefix opens a panel that stays until a choice or Esc.
    preferences()
    s = Session('shortcuts-default')
    s.expect('Ctrl+G: quick')
    s.expect('?: help')
    s.send(CTRL_G)
    s.expect('Actions rapides — alpha')
    s.expect('d Dupliquer')
    s.send(b'z')                      # unknown letter: nothing runs, no timeout
    time.sleep(1.2)
    s.expect('Actions rapides — alpha')
    s.send(ESC)
    s.absent('Actions rapides')
    s.send(CTRL_G + b'i')             # sequence letter runs its named action
    s.expect('SSH Host Information: alpha')
    s.send(ESC)
    s.send(CTRL_G + b'?')
    s.expect('Commands')
    s.expect('Ctrl+G    Actions rapides')
    s.send(b'?')                      # the key that opens the help closes it
    s.absent('Commands')
    s.send(F1)
    s.expect('Commands')
    s.send(F1)
    # From the search input the prefix keeps query and focus; letters stay text.
    s.send(b'/alp' + CTRL_G)
    s.expect('Actions rapides — alpha')
    s.send(ESC)                       # alone: ESC followed by a byte would be Alt+key
    s.send(b'ha?')
    s.expect('Search: > alpha?')
    assert not connections()
    s.send(CTRL_C)
    s.close()

    # A configured shortcut updates dispatch, palette, footer and help together;
    # a conflicting preference is refused and explained, never applied silently.
    preferences(prefix_key='', shortcuts={'host.edit': ['E'], 'host.note': ['d']})
    s = Session('shortcuts-custom')
    s.expect('E: edit')
    s.expect('Raccourcis : 1 préférence(s) refusée(s)')
    s.absent('quick')
    s.send(CTRL_G)
    s.absent('Actions rapides')
    s.send(b'e')                      # released historical key
    s.absent('Edit SSH Host')
    s.send(b'E')
    s.expect('Edit SSH Host')
    s.send(ESC)
    s.send(F2)
    s.send(b'modifier')
    s.expect('Raccourci')
    row = next(line for line in s.screen.text().splitlines() if '▶' in line)
    assert 'Modifier' in row and ' E ' in row, row
    s.send(ESC)
    s.send(b'd')                      # `d` keeps its historical meaning
    s.expect('DELETE SSH HOST — review')
    s.send(ESC)
    s.send(b'?' + DOWN * 45)
    s.expect('d sert déjà à « Supprimer »')
    s.send(ESC)
    s.send(CTRL_C)
    s.close()
    assert CONFIG.read_bytes() == ORIGINAL_CONFIG
    print('PTY PASS [shortcuts]: prefix panel without timeout, unknown letter, sequence letters, '
          'F1/? help toggle, search text preserved, disabled prefix, configured shortcut in '
          'dispatch/palette/footer/help, explained conflict, terminal restored')


def scenario_navigation():
    preferences()
    s = Session('navigation')
    s.expect('?: help')
    # Esc walks back one level at a time and never closes the list.
    s.send(b'i')
    s.expect('SSH Host Information: alpha')
    s.send(b'e')
    s.expect('Edit SSH Host')
    s.send(b'\x1b[18~')              # F7 over an untouched form
    s.expect('Owned tunnels')
    for screen in ['Edit SSH Host', 'SSH Host Information: alpha', '?: help']:
        s.send(ESC)
        s.expect(screen)
    s.send(ESC)
    s.expect('Esc ne quitte pas la liste — q / Ctrl+C : quitter')
    s.alive()
    # Ctrl+C follows Esc inside a secondary screen.
    for opener, title in [(b'?', 'Commands'), (F2, 'Raccourci'), (b'\x1b[18~', 'Owned tunnels')]:
        s.send(opener)
        s.expect(title)
        s.send(CTRL_C)
        s.absent(title)
        s.alive()
    # A modified draft survives Esc, Ctrl+C and Enter on the focused choice.
    s.send(b'e')
    s.expect('Edit SSH Host')
    s.send(b'-draft')
    for keep in [ESC, CTRL_C, b'\r']:
        s.send(ESC)
        s.expect("[▶ Continuer l'édition ]")
        s.send(keep)
        s.expect('alpha-draft')
        s.absent('Abandonner les modifications')
    s.send(ESC)
    s.send(b'\t')
    s.expect('[▶ Abandonner ]')
    s.send(b'\r')
    s.expect('?: help')
    s.absent('alpha-draft')
    # Simple selectors wrap at both ends.
    s.send(b'g')
    s.expect('Group view')
    first = next(line for line in s.screen.text().splitlines() if '▶' in line)
    s.send(UP)
    last = next(line for line in s.drain(.2).splitlines() if '▶' in line)
    assert first != last, (first, last)
    s.send(DOWN)
    assert next(line for line in s.drain(.2).splitlines() if '▶' in line) == first
    s.send(ESC)
    assert CONFIG.read_bytes() == ORIGINAL_CONFIG and not connections()
    s.send(CTRL_C)                    # the list is the root: Ctrl+C asks to quit
    s.close()
    print('PTY PASS [navigation]: Esc one level at a time and inert at the root, Ctrl+C as Esc in '
          'secondary screens, draft kept until discard is chosen, wrapping selector, '
          'quit from the list, terminal restored')


def scenario_palette():
    preferences()
    s = Session('palette', width=80, height=24)
    s.expect('F2: actions')
    s.send(F2)
    s.expect('Actions — alpha')
    for column in ['Rechercher une action :', 'Action', 'Raccourci', 'État', 'Connexion']:
        s.expect(column)
    selected = lambda: next(line for line in s.drain(.2).splitlines() if '▶' in line)
    assert 'Se connecter' in selected() and ' Enter ' in selected(), selected()
    s.send(UP)                        # first -> last
    assert 'Quitter' in selected(), selected()
    s.send(DOWN)                      # last -> first, category title back in view
    assert 'Se connecter' in selected(), selected()
    s.expect('Connexion')
    # Typing searches, without accents; the single match waits for Enter.
    s.send(b'tunnel')
    s.expect('Rechercher une action : tunnel')
    rows = [line for line in s.drain(.2).splitlines() if 'F6' in line or 'F7' in line]
    assert len(rows) == 2 and 'Profils de tunnels' in rows[0] and 'Tunnels actifs' in rows[1], rows
    s.absent('Ajouter un hôte')
    s.send(b'\x15methode')           # Ctrl+U clears the search field
    assert 'Méthode de disponibilité' in selected() and ' P ' in selected() and 'SSH' in selected()
    time.sleep(.6)
    s.expect('Actions — alpha')       # still open: nothing ran on its own
    # An unavailable action explains itself and runs nothing.
    s.send(b'\x15tous les fichiers')
    s.expect('Indisponible : Aucun filtre de fichier actif.')
    s.send(b'\r')
    s.expect("Rien n'a été lancé.")
    s.expect('Actions — alpha')
    # Letters are search text: `q` and `d` neither quit nor delete.
    s.send(b'\x15qd')
    s.expect('Aucune action ne correspond')
    s.alive()
    s.send(ESC)
    s.absent('Rechercher une action')
    s.expect('F2: actions')
    # Enter runs only the selected, available action.
    s.send(F2)
    s.send(b'aide\r')
    s.expect('Commands')
    s.send(ESC)
    # Contextual palette over a modified draft: the draft is found again.
    s.send(b'e')
    s.expect('Edit SSH Host')
    s.send(b'-draft')
    s.send(F2)
    s.expect('Actions — brouillon conservé')
    s.expect('Tunnels actifs')
    s.absent('Supprimer')
    s.send(F2)                        # the key that opened it closes it
    s.absent('brouillon conservé')
    s.expect('alpha-draft')
    s.send(ESC)
    s.send(b'\t\r')
    s.expect('F2: actions')
    assert CONFIG.read_bytes() == ORIGINAL_CONFIG and not connections()
    s.send(CTRL_C)
    s.close()
    print('PTY PASS [palette] at 80x24: columns, wrap at both ends, accent-insensitive search, no '
          'auto-run, explained unavailability, letters as text, Enter runs the selection, '
          'contextual palette keeps the draft, terminal restored')


def scenario_search():
    preferences()
    (APP / 'config.json').write_text(json.dumps(dict(BASE, focus_search_on_start=True)))
    s = Session('search', width=100, height=30)
    # Editing: a real input, named as the focused element, with a caret.
    s.expect('● Recherche')
    s.expect('Search: > ')
    s.absent('● Hôtes')
    s.send(b'alp')
    s.expect('Search: > alp')
    s.expect('1 hôte sur 2')
    assert b'\x1b[?25h' in s.raw, 'the search input shows the native caret'
    # Tab gives the focus to the table: no inactive input box is left behind.
    s.send(b'\x1b[D\t')
    s.expect('● Hôtes')
    s.absent('Search: > ')
    s.absent('● Recherche')
    s.expect('Filtres : alp')
    s.expect('( / : modifier • Backspace : effacer )')
    s.expect('1 hôte sur 2')
    s.absent('beta')
    # Back to the search: text and cursor are found again.
    s.send(b'/X')
    s.expect('Search: > alXp')
    s.send(b'\x7f')                   # Backspace edits inside the input
    s.expect('Search: > alp')
    # Esc leaves the edit and keeps the filter applied and visible.
    s.send(ESC)
    s.expect('Filtres : alp')
    s.absent('beta')
    s.alive()
    # Clearing is explicit: Backspace from the table, or the palette action.
    s.send(b'\x7f')
    s.expect('/ Rechercher')
    s.expect('beta')
    s.absent('Filtres :')
    s.send(F2)
    s.send(b'effacer la recherche')
    s.expect('Indisponible : Aucune recherche active.')
    s.send(ESC)
    # Hidden hosts and favorites join the same band, with the host count.
    s.send(b'H')
    s.expect('Filtres : Masqués visibles (H)')
    s.send(F2)
    s.send(b'favoris seuls\r')
    s.expect('Filtres : Favoris seuls • Masqués visibles (H)')
    s.expect('0 hôte sur 2')
    s.send(F2)
    s.send(b'effacer tous les filtres\r')
    s.absent('Filtres :')
    s.expect('2 hôtes')
    assert CONFIG.read_bytes() == ORIGINAL_CONFIG and not connections()
    s.send(CTRL_C)
    s.close()
    print('PTY PASS [search]: editing input with caret, no inactive box after Tab, visible '
          'filter band and host count, query and cursor kept, Esc keeps the filter, '
          'explicit clearing from table and palette, combined criteria, terminal restored')


SCENARIOS = {'search': scenario_search, 'palette': scenario_palette, 'shortcuts': scenario_shortcuts, 'navigation': scenario_navigation}
selected = sys.argv[3:] or list(SCENARIOS)
for name in selected:
    SCENARIOS[name]()
print('UX PTY PASS: ' + ', '.join(selected))
