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
# These journeys read the French interface; the other PTY scripts read English,
# the default language.
BASE = {'ping_on_start': False, 'check_for_updates': False, 'focus_search_on_start': False,
        'language': 'fr'}
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
    # The French legend is longer: its reminder needs more than 120 columns.
    s = Session('shortcuts-default', width=140)
    s.expect('Ctrl+G : rapide')
    s.expect('? : aide')
    s.send(CTRL_G)
    s.expect('Actions rapides — alpha')
    s.expect('d Dupliquer')
    s.send(b'z')                      # unknown letter: nothing runs, no timeout
    time.sleep(1.2)
    s.expect('Actions rapides — alpha')
    s.send(ESC)
    s.absent('Actions rapides')
    s.send(CTRL_G + b'i')             # sequence letter runs its named action
    s.expect("Fiche de l'hôte SSH : alpha")
    s.send(ESC)
    s.send(CTRL_G + b'?')
    s.expect('Commandes')
    s.expect('Ctrl+G    Actions rapides')
    s.send(b'?')                      # the key that opens the help closes it
    s.absent('Commandes')
    s.send(F1)
    s.expect('Commandes')
    s.send(F1)
    # From the search input the prefix keeps query and focus; letters stay text.
    s.send(b'/alp' + CTRL_G)
    s.expect('Actions rapides — alpha')
    s.send(ESC)                       # alone: ESC followed by a byte would be Alt+key
    s.send(b'ha?')
    s.expect('Recherche : > alpha?')
    assert not connections()
    s.send(CTRL_C)
    s.close()

    # A configured shortcut updates dispatch, palette, footer and help together;
    # a conflicting preference is refused and explained, never applied silently.
    preferences(prefix_key='', shortcuts={'host.edit': ['E'], 'host.note': ['d']})
    s = Session('shortcuts-custom')
    s.expect('E : modifier')
    s.expect('Raccourcis : 1 préférence(s) refusée(s)')
    s.absent('rapide')
    s.send(CTRL_G)
    s.absent('Actions rapides')
    s.send(b'e')                      # released historical key
    s.absent('Hôtes > Modifier')
    s.send(b'E')
    s.expect('Hôtes > Modifier')
    s.send(ESC)
    s.send(F2)
    s.send(b'modifier')
    s.expect('Raccourci')
    row = next(line for line in s.screen.text().splitlines() if '▶' in line)
    assert 'Modifier' in row and ' E ' in row, row
    s.send(ESC)
    s.send(b'd')                      # `d` keeps its historical meaning
    s.expect('SUPPRESSION D\'UN HÔTE SSH — aperçu')
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
    s.expect('? : aide')
    # Esc walks back one level at a time and never closes the list.
    s.send(b'i')
    s.expect("Fiche de l'hôte SSH : alpha")
    s.send(b'e')
    s.expect('Hôtes > Modifier')
    s.send(b'\x1b[18~')              # F7 over an untouched form
    s.expect('Tunnels possédés')
    for screen in ['Hôtes > Modifier', "Fiche de l'hôte SSH : alpha", '? : aide']:
        s.send(ESC)
        s.expect(screen)
    s.send(ESC)
    s.expect('Échap ne quitte pas la liste — q / Ctrl+C : quitter')
    s.alive()
    # Ctrl+C follows Esc inside a secondary screen.
    for opener, title in [(b'?', 'Commandes'), (F2, 'Raccourci'), (b'\x1b[18~', 'Tunnels possédés')]:
        s.send(opener)
        s.expect(title)
        s.send(CTRL_C)
        s.absent(title)
        s.alive()
    # A modified draft survives Esc, Ctrl+C and Enter on the focused choice.
    s.send(b'e')
    s.expect('Hôtes > Modifier')
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
    s.expect('? : aide')
    s.absent('alpha-draft')
    # Simple selectors wrap at both ends.
    s.send(b'g')
    s.expect('Groupes — présentation')
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
    s.expect('F2 : actions')
    s.send(F2)
    s.expect('Actions — alpha')
    for column in ['Rechercher une action :', 'Action', 'Raccourci', 'État', 'Connexion']:
        s.expect(column)
    selected = lambda: next(line for line in s.drain(.2).splitlines() if '▶' in line)
    assert 'Se connecter' in selected() and ' Entrée ' in selected(), selected()
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
    s.expect('F2 : actions')
    # Enter runs only the selected, available action.
    s.send(F2)
    s.send(b'aide\r')
    s.expect('Commandes')
    s.send(ESC)
    # Contextual palette over a modified draft: the draft is found again.
    s.send(b'e')
    s.expect('Hôtes > Modifier')
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
    s.expect('F2 : actions')
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
    s.expect('Recherche : > ')
    s.absent('● Hôtes')
    s.send(b'alp')
    s.expect('Recherche : > alp')
    s.expect('1 hôte sur 2')
    assert b'\x1b[?25h' in s.raw, 'the search input shows the native caret'
    # Tab gives the focus to the table: no inactive input box is left behind.
    s.send(b'\x1b[D\t')
    s.expect('● Hôtes')
    s.absent('Recherche : > ')
    s.absent('● Recherche')
    s.expect('Filtres : alp')
    s.expect('( / : modifier • Backspace : effacer )')
    s.expect('1 hôte sur 2')
    s.absent('beta')
    # Back to the search: text and cursor are found again.
    s.send(b'/X')
    s.expect('Recherche : > alXp')
    s.send(b'\x7f')                   # Backspace edits inside the input
    s.expect('Recherche : > alp')
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


def scenario_host_form():
    preferences()
    work = ROOT / '.ssh/work'
    work.write_text('Host gamma\n HostName 127.0.0.3\n')
    CONFIG.write_text('Include work\n' + ORIGINAL_CONFIG.decode())
    main_before = CONFIG.read_bytes()
    try:
        s = Session('host_form', width=100, height=30)
        s.expect('? : aide')
        s.send(b'a')
        s.expect('Hôtes > Ajouter un hôte — fichier de destination')
        s.send(b'\r')
        # One page: destination visible, essentials first, advanced folded away.
        s.expect('Hôtes > Ajouter un hôte')
        s.expect('Fichier : ' + str(CONFIG))
        s.expect('[  Changer ]')
        s.expect('▸ Options avancées')
        s.absent('ProxyJump')
        s.expect('ex. : api-prod')
        s.send(b'web-01\r')
        s.expect('ex. : 192.168.1.100 ou example.com')
        s.send(b'web.example\r\r\r')
        s.expect('ex. : ~/.ssh/id_ed25519')
        # Enter after the last field focuses the main action; nothing is submitted.
        s.send(b'\r')
        s.expect('[▶ Prévisualiser les modifications ]')
        s.absent('confirmer ces modifications')
        # Change the target file from the draft and come back to the same draft.
        s.send(b'\t\t')
        s.expect('[▶ Changer ]')
        s.send(b'\r')
        s.expect('Changer le fichier de destination')
        s.expect('Le brouillon est conservé')
        s.expect('(actuel)')              # the file in effect is marked
        s.send(DOWN + b'\r')
        s.expect('Hôtes > Ajouter un hôte')
        s.expect('Fichier : ' + str(work))
        s.expect('web-01')
        s.expect('web.example')
        # Ctrl+S previews from anywhere; writing needs the explicit confirmation.
        s.send(b'\x13')
        s.expect('confirmer ces modifications')
        s.expect("Opération : AJOUTER l'hôte « web-01 »")
        s.expect('Fichier : ' + str(work))
        assert 'web-01' not in work.read_text()
        s.send(b'\r')                    # Enter while reading the diff writes nothing
        s.expect("Rien d'écrit : Tab atteint les boutons")
        assert 'web-01' not in work.read_text()
        s.send(b'\t')
        s.expect('[▶ Enregistrer ]')
        s.send(b'\r')
        s.expect('web-01')
        s.expect('? : aide')
        assert 'Host web-01' in work.read_text() and 'web.example' in work.read_text()
        assert CONFIG.read_bytes() == main_before, 'only the chosen file is written'
        # Edit names its mode; Ctrl+K edits text instead of switching tabs.
        s.send(b'/web-01\t')
        s.send(b'e')
        s.expect('Hôtes > Modifier web-01')
        s.absent('[  Changer ]')
        s.send(b'\x01\x0b')             # Ctrl+A, Ctrl+K: empty the alias input
        s.expect('> Alias *           requis')
        s.send(b'web-02')
        # Sections unfold on demand, from their header.
        s.send(b'\t' * 7)
        s.expect('> ▸ Options avancées')
        s.send(b' ')
        s.expect('> ▾ Options avancées')
        s.expect('ProxyJump')
        s.send(ESC)
        s.expect("[▶ Continuer l'édition ]")
        s.send(b'\t\r')
        s.expect('? : aide')
        assert 'web-02' not in work.read_text()
        assert not connections()
        s.send(CTRL_C)
        s.close()
    finally:
        CONFIG.write_bytes(ORIGINAL_CONFIG)
        work.unlink(missing_ok=True)
    print('PTY PASS [host_form]: one-page add with visible target, Enter never submits, file '
          'changed from the draft, preview then explicit write to the chosen file only, '
          'named edit mode, Ctrl+K as text editing, on-demand section, terminal restored')


def scenario_selectors():
    preferences()
    (ROOT / '.ssh/a').mkdir(exist_ok=True)
    (ROOT / '.ssh/b').mkdir(exist_ok=True)
    first, second = ROOT / '.ssh/a/hosts', ROOT / '.ssh/b/hosts'
    first.write_text('Host one\n HostName 127.0.0.11\n')
    second.write_text('Host two\n HostName 127.0.0.12\n')
    CONFIG.write_text('Include a/hosts b/hosts\n' + ORIGINAL_CONFIG.decode())
    try:
        s = Session('selectors', width=100, height=30)
        s.expect('4 hôtes')
        row = lambda: next(line for line in s.drain(.2).splitlines() if '▶' in line)
        # File filter: the value in effect is marked before anything is changed.
        s.send(b'c')
        s.expect('Filtrer par fichier source')
        assert 'Tous les fichiers' in row() and '✓' in row() and '(actuel)' in row(), row()
        s.expect('Aucun filtre par fichier.')
        s.send(UP)                        # wraps to the last file
        assert str(second) in row() and '✓' not in row(), row()
        s.expect('1 hôte dans ce fichier')
        s.send(ESC)                       # cancelling changes nothing
        s.expect('4 hôtes')
        s.absent('Fichier :')
        # Typing narrows; two files of the same name remain distinguishable.
        s.send(b'c')
        s.send(b'hosts')
        s.expect('Rechercher : hosts')
        s.expect(str(first))
        s.expect(str(second))
        s.absent('Tous les fichiers')
        s.send(b' b/')
        assert str(second) in row(), row()
        s.send(b'\r')
        s.expect('Fichier : ' + str(second))
        s.expect('1 hôte sur 4')
        s.send(b'c')
        assert str(second) in row() and '✓' in row(), row()
        s.send(ESC)
        s.send(b'C')
        s.expect('4 hôtes')
        # Narrow terminal: shortest distinguishing tails, full path in the detail.
        n = Session('selectors-narrow', width=48, height=24)
        n.expect('F2 : actions')
        n.send(b'c')
        n.send(DOWN + DOWN)
        n.expect('…/a/hosts')
        n.expect('…/b/hosts')
        n.send(ESC)
        n.send(CTRL_C)
        n.close()
        # Method and grouping use the same component, with an explanation.
        s.send(b'P')
        s.expect('Méthode de disponibilité')
        assert 'SSH' in row() and '✓' in row(), row()
        s.expect("Ne prouve pas l'authentification.")
        s.send(DOWN)
        s.expect('sans bastion ni authentification SSH')
        s.send(ESC)
        s.send(b'g')
        s.expect('Groupes — présentation de la liste')
        s.expect('une seule liste triée')
        s.send(ESC)
        assert not connections()
        s.send(CTRL_C)
        s.close()
    finally:
        CONFIG.write_bytes(ORIGINAL_CONFIG)
    print('PTY PASS [selectors]: marked current value, wrapping, cancel keeps the value, typed '
          'narrowing, same-named files distinguishable wide and narrow, shared component '
          'for method and grouping with explanations, terminal restored')


def scenario_list_marks():
    (APP / 'config.json').write_text(json.dumps(dict(BASE, group_mode='files')))
    (APP / 'metadata.json').write_text(json.dumps({'configs': {str(CONFIG.resolve()): {
        'hosts': {'beta': {'favorite': True, 'note': ''}}}}}))
    try:
        s = Session('list_marks', width=110, height=30)
        s.expect('2 hôtes')
        row = lambda name: next(line for line in s.drain(.2).splitlines()
                                if name in line and '│' in line)
        # A favorite alone: no target zone, and a legend of the signs in use.
        assert '? *beta' in row('beta') and '[' not in row('beta'), row('beta')
        s.expect('Légende : * favori · ▼ ▶ groupe ouvert / replié')
        # On a group header Enter toggles, and host shortcuts explain themselves.
        s.send(b'\x1b[H')
        s.expect('Entrée : déplier')
        s.send(b'x')
        s.expect("Lot : marquer l'alias — Sélectionnez un hôte, pas un groupe")
        s.absent('Lot : 1 cible')
        s.send(b'e')
        s.expect('Modifier — Sélectionnez un hôte, pas un groupe')
        s.absent('Hôtes > Modifier')
        # Marking a host creates the target zone; the favorite keeps its own sign.
        s.send(b'\x1b[F')                # End: last host
        s.expect('Entrée : connexion')
        s.send(b'x')
        s.expect('Lot : 1 cible marquée • B : voir / préparer • F2 : tout retirer')
        s.expect('(2 • 1 cible)')
        marked = next(line for line in s.drain(.2).splitlines() if '[x]' in line)
        other = next(line for line in s.drain(.2).splitlines() if '[ ]' in line)
        assert ('beta' in marked) != ('beta' in other), (marked, other)
        assert '*beta' in row('beta'), row('beta')
        s.expect('Légende : [x] cible de lot · * favori')
        # A search that hides the target keeps it counted, and says so.
        hidden = 'alpha' if 'beta' in marked else 'beta'
        s.send(b'/' + hidden.encode() + b'\t')
        s.expect('Lot : 1 cible marquée, dont 1 hors du filtre')
        s.send(b'\x7f')                  # Backspace: clear the search
        s.expect('Lot : 1 cible marquée •')
        # Collapsing the group hides rows, not the count.
        s.send(b'\x1b[H')
        s.send(b'\x1b[D')
        s.expect('(2 • 1 cible)')
        assert not any('[x]' in line and '│' in line for line in s.drain(.3).splitlines())
        s.send(b'B')
        s.expect('1 cibles')
        s.send(ESC)
        assert not connections() and CONFIG.read_bytes() == ORIGINAL_CONFIG
        s.send(CTRL_C)
        s.close()
    finally:
        (APP / 'metadata.json').unlink(missing_ok=True)
        preferences()
    print('PTY PASS [list_marks]: separate zones for target and favorite, contextual legend, group '
          'header explains instead of acting, Enter announced, selection counted through '
          'search and collapse, terminal restored')


def scenario_host_sheet():
    preferences()
    s = Session('host_sheet', width=110, height=40)
    s.expect('? : aide')
    s.send(b'i')
    s.expect("Fiche de l'hôte SSH : alpha")
    for section in ['── Résumé', '── Configuration effective', '── Déclaration et source',
                    '── Disponibilité mesurée', '── Historique des connexions']:
        s.expect(section)
    s.expect('[  Connexion ] [  Modifier ] [  Note ] [  Copier ] [  Fermer ]')
    # Reading never edits: Enter alone does nothing but say how to act.
    s.send(b'\r')
    s.expect('Entrée seule ne modifie rien')
    s.absent('Hôtes > Modifier')
    s.send(b'\t\t')
    s.expect('[▶ Modifier ]')
    s.send(b'\r')
    s.expect('Hôtes > Modifier alpha')
    s.send(ESC)
    s.expect("Fiche de l'hôte SSH : alpha")
    # Copy is explicit: one named value, a clipboard request, a selectable fallback.
    s.send(b'\t\t')
    s.expect('[▶ Copier ]')
    mark = len(s.raw)
    s.send(b'\r')
    s.expect('Copier une valeur — alpha')
    s.expect('ssh alpha')
    s.send(DOWN + b'\r')
    s.expect('Copie demandée au terminal (OSC 52) : Commande')
    assert b'\x1b]52;c;c3NoIGFscGhh\x07' in s.raw[mark:], 'OSC 52 request for "ssh alpha"'
    assert any(line.strip('│ ') == 'ssh alpha' for line in s.drain(.2).splitlines())
    s.send(ESC)
    s.expect('? : aide')
    assert not connections() and CONFIG.read_bytes() == ORIGINAL_CONFIG
    s.send(CTRL_C)
    s.close()
    print('PTY PASS [host_sheet]: sectioned sheet, named actions, Enter never edits, explicit copy '
          'with OSC 52 request and selectable value, terminal restored')


def scenario_notes():
    preferences()
    metadata = APP / 'metadata.json'
    try:
        s = Session('notes', width=110, height=30)
        s.expect('? : aide')
        s.send(b'N')
        s.expect('Note personnelle')
        s.expect('Enregistrée')
        s.expect("Fichier SSH de l'hôte : " + str(CONFIG))
        s.expect(str(metadata))
        s.expect('à ne pas confondre avec un tag')
        s.send(b'Bastion requis\rligne 2')
        s.expect('● Modifiée')
        s.send(b'\t')
        s.expect('[▶ Enregistrer ]')
        s.send(b'x')                      # a button receives no text
        s.send(b'\t')
        s.expect('[▶ Annuler ]')
        s.send(b'\r')
        s.expect("[▶ Continuer l'édition ]")
        s.send(b'\r')
        s.expect('ligne 2')
        s.send(b'\x13')                  # Ctrl+S saves from the text
        s.expect('? : aide')
        note = json.loads(metadata.read_text())['configs'][str(CONFIG.resolve())]['hosts']['alpha']['note']
        assert note == 'Bastion requis\nligne 2', note
        assert CONFIG.read_bytes() == ORIGINAL_CONFIG, 'a note never touches the SSH file'
        row = next(line for line in s.drain(.2).splitlines() if 'alpha' in line and '│' in line)
        assert '≡' in row and 'Bastion' not in row, row
        s.send(CTRL_C)
        s.close()
    finally:
        metadata.unlink(missing_ok=True)
    print('PTY PASS [notes]: note context and data location, modified state, buttons, resume '
          'after cancel, saved outside the SSH file, discreet list sign, terminal restored')


def scenario_tunnels():
    preferences()
    profiles = APP / 'tunnel_profiles.json'
    try:
        s = Session('tunnels', width=110, height=40)
        s.expect('? : aide')
        s.send(b'f')
        s.expect('Configuration du transfert de port')
        s.expect('(•) Local (-L)   ( ) Remote (-R)   ( ) Dynamic (-D)')
        s.expect('Écouter sur ce poste, port ? → joindre localhost:? depuis le serveur SSH')
        s.send(b'\t5433\t\x15db.internal\t5432')
        s.expect('Écouter sur ce poste, port 5433 → joindre db.internal:5432 depuis le serveur SSH')
        # Enter repeated stays on the last field: it neither saves nor launches.
        s.send(b'\r' * 8)
        s.expect('Configuration du transfert de port')
        assert not connections()
        s.send(b'\t')                   # bind address -> mode
        s.expect('> (•) Session shell   ( ) Tunnel seul')
        s.send(b'\t')
        s.expect('[▶ Lancer ]  [  Enregistrer le profil ]  [  Annuler ]')
        # Saving needs a name; the error says so and nothing is launched.
        s.send(b'\x1b[C\r')
        s.expect('Nom du profil (requis pour enregistrer seulement) :')
        assert not connections() and not profiles.exists()
        s.send(b'db\x13')                # name it, Ctrl+S
        s.expect('Profil enregistré')
        assert not connections(), 'saving never launches'
        s.send(ESC)
        s.expect('? : aide')
        # The profile list shows the route in words and names its actions.
        s.send(b'\x1b[17~')              # F6
        s.expect('Profils de tunnels')
        s.expect('Écouter sur ce poste, port 5433 → joindre db.internal:5432 depuis le serveur SSH')
        s.expect('[▶ Ouvrir ] [  Lancer ] [  Dupliquer ] [  Supprimer ] [  Nouveau ]')
        s.send(b'\x1b[C\x1b[C\r')       # Duplicate
        s.expect('db-copy')
        s.expect('Modifications non enregistrées')
        s.send(ESC)
        s.send(b'\t\r')
        s.expect('Profils de tunnels')
        s.send(ESC)
        assert CONFIG.read_bytes() == ORIGINAL_CONFIG
        s.send(CTRL_C)
        s.close()
    finally:
        profiles.unlink(missing_ok=True)
        (APP / 'sshm_history.json').unlink(missing_ok=True)
    print('PTY PASS [tunnels]: named types and route in words, Enter never saves or launches, '
          'visible mode and separate buttons, name needed only to save, profile actions and '
          'route, duplicate as unsaved draft, terminal restored')


def scenario_batch():
    preferences()
    s = Session('batch', width=110, height=30)
    s.expect('? : aide')
    # No target yet: a page explains it and names the two ways out.
    s.send(b'B')
    s.expect('Lot : 0 cibles • [Cibles] → Commande → Vérification → Résultats')
    s.expect("le filtre n'est jamais transformé en cibles de lui-même")
    s.expect('[▶ Retour à la liste ]')
    s.send(b'uptime')                 # no command can be typed without a target
    s.send(b'\t')
    s.expect('[▶ Ajouter les 2 hôte(s) affichés par le filtre ]')
    s.send(b'\r')
    s.expect('Lot : 2 cibles • Cibles ✓ → [Commande] → Vérification → Résultats')
    s.absent('uptime')
    s.send(b'hostname')
    # Enter walks the inputs and stops on Review: nothing is launched.
    s.send(b'\r\r\r')
    s.expect('[▶ Vérification ]')
    assert not connections()
    s.send(b'\r')
    s.expect('Confirmer le lot : 2 cibles • Cibles ✓ → Commande ✓ → [Vérification] → Résultats')
    s.expect('Commande : hostname')
    s.expect("[▶ Retour à l'édition ]  [  Lancer sur 2 cible(s) ]")
    s.send(b'\r')                    # the focused choice goes back to the edit step
    s.expect('[Commande]')
    assert not connections()
    s.send(ESC)
    s.expect('Lot : 2 cibles marquées')
    s.send(F2)
    s.send(b'effacer les marques\r')
    s.absent('Lot :')
    assert not connections() and CONFIG.read_bytes() == ORIGINAL_CONFIG
    s.send(CTRL_C)
    s.close()
    print('PTY PASS [batch]: explained no-target page, explicit use of filter results, step '
          'indicator, Enter stops on Review, review launches nothing by itself, terminal restored')


def scenario_help():
    preferences()
    s = Session('help', width=100, height=30)
    s.expect('? : aide')
    # Help: what the screen is for, then a search over its lines.
    s.send(b'?')
    s.expect('Cet écran : la liste des hôtes.')
    s.send(b'/dupliquer')
    s.expect('Recherche : dupliquer_')
    s.expect('D         Dupliquer')
    s.absent('Se connecter')
    s.send(b'q')                      # a closing letter is text while searching
    s.expect('Aucune ligne de l\'aide ne correspond.')
    s.send(ESC)
    s.expect('Se connecter')
    s.send(ESC)
    s.expect('? : aide')
    # A write is confirmed in words, until the next key.
    s.send(b'\x1b[F')                 # last host
    s.send(b'd')
    s.expect("Opération : SUPPRIMER l'hôte « beta »")
    s.send(b'\x13')                   # Ctrl+S: explicit confirmation
    s.expect('✓ beta supprimé')
    s.send(DOWN)
    s.absent('supprimé')
    assert b'Host beta' not in CONFIG.read_bytes()
    # A failing reload gives an error that is readable in full.
    CONFIG.write_bytes(ORIGINAL_CONFIG)
    CONFIG.chmod(0)
    try:
        s.send(b'\x1b[15~')           # F5
        s.expect('Erreur :')
        s.send(b'!')
        s.expect('Messages')
        s.expect('[Erreur]')
        s.send(ESC)
    finally:
        CONFIG.chmod(0o600)
    s.send(b'\x1b[15~')
    s.expect('beta')
    assert not connections()
    s.send(CTRL_C)
    s.close()
    CONFIG.write_bytes(ORIGINAL_CONFIG)
    print('PTY PASS [help]: contextual help with search, write confirmed in words, error '
          'readable in full, terminal restored')


def scenario_language():
    # Without a "language" preference the interface is in English.
    plain = {key: value for key, value in BASE.items() if key != 'language'}
    (APP / 'config.json').write_text(json.dumps(plain))
    s = Session('language', width=100, height=30)
    s.expect('F2: actions')
    s.expect('/ Search')
    # The language is a named action, found in either language, changed in the application.
    s.send(F2)
    s.send(b'langue')
    s.expect('Language')
    s.send(b'\r')
    s.expect("Language / Langue de l'interface")
    s.expect('English')
    s.expect('Français')
    s.send(ESC)                       # cancelling changes nothing
    s.expect('F2: actions')
    assert 'language' not in json.loads((APP / 'config.json').read_text())
    s.send(F2)
    s.send(b'language')
    s.send(b'\r')
    s.expect("Language / Langue de l'interface")
    s.send(DOWN)
    s.send(b'\r')
    s.expect('F2 : actions')          # applied at once
    s.expect('/ Rechercher')
    s.absent('Enter: connect')
    assert json.loads((APP / 'config.json').read_text())['language'] == 'fr'
    s.send(CTRL_C)
    s.close()
    # The choice is kept: the next start is in French.
    s = Session('language-restart', width=100, height=30)
    s.expect('F2 : actions')
    s.send(F2)
    s.expect('Rechercher une action :')
    s.send(ESC)
    s.send(CTRL_C)
    s.close()
    assert not connections()
    print('PTY PASS [language]: English by default, language changed from the palette in either '
          'language, cancel keeps it, saved and applied at once, kept at restart, terminal restored')


SCENARIOS = {'search': scenario_search, 'palette': scenario_palette, 'shortcuts': scenario_shortcuts, 'navigation': scenario_navigation, 'host_form': scenario_host_form, 'selectors': scenario_selectors,
             'list_marks': scenario_list_marks, 'host_sheet': scenario_host_sheet, 'notes': scenario_notes, 'tunnels': scenario_tunnels, 'batch': scenario_batch, 'help': scenario_help,
             'language': scenario_language}
selected = sys.argv[3:] or list(SCENARIOS)
for name in selected:
    SCENARIOS[name]()
print('UX PTY PASS: ' + ', '.join(selected))
