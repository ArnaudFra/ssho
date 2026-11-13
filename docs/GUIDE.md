# ssho user guide

**Français :** ce guide existe aussi en français, voir [GUIDE_fr.md](GUIDE_fr.md).

The detailed reference of the interface and of the command line. Installation is covered in the [README](../README.md) and in [Build and advanced installation](INSTALLATION.md).

## Configuration and first launch

Hosts stay in `~/.ssh/config`, along with any `Include` files. The `-c` option selects another SSH configuration.

The program's own preferences, history and backups live in `~/.config/ssho/`. On the first command that uses this data:

- If `ssho/` does not exist and `sshm/` does, its contents are copied into `ssho/`, including `config.json`, `sshm_history.json` and `backups/`.
- If `ssho/` already exists, even empty, no import and no merge are performed.
- If no previous directory exists, the default values are used.

The `sshm/` source is left untouched. The copy is prepared separately so that a failure can be retried on the next launch. Symbolic links in the source are copied as independent content; broken or cyclic links cause an error without publishing a partial copy. The destination directory must be a real directory.

`XDG_CONFIG_HOME` replaces `~/.config` on Linux/macOS. On Windows, the root is `%APPDATA%`, falling back to `%USERPROFILE%/.config`. Help, version and completion script generation do not create an application directory.

Example of preferences, in `ssho/config.json`:

```json
{
  "check_for_updates": false,
  "focus_search_on_start": true,
  "ping_on_start": false,
  "availability_method": "ssh",
  "ssh_command": ["ssh"],
  "group_aliases": false,
  "group_mode": "flat",
  "language": "en",
  "colors": true,
  "ascii": false,
  "key_bindings": {
    "quit_keys": ["q", "ctrl+c"],
    "disable_esc_quit": false,
    "prefix_key": "ctrl+g",
    "shortcuts": {}
  }
}
```

`ssh_command` is an argv array that can only be configured in this JSON file.
A missing or `null` field keeps `["ssh"]`. Example with a local wrapper:

```json
{"ssh_command": ["/usr/local/bin/my-wrapper", "--profile", "work"]}
```

The first element is the executable; the following ones come before the SSH arguments
(`-F`, configuration path, optional `-t`, alias, then remote command).
Each element remains a literal argument: no shell, expansion, splitting
on spaces or metacharacter interpretation is added. The array
must contain a non-blank executable and no argument may contain NUL.
An empty extra argument is allowed. The wrapper must accept the
SSH arguments and keep the session in the foreground so that the return, streams,
exit codes and signals describe the connection. This example does not vouch for
any third-party wrapper.

This setting applies to individual connections and remote commands without forwarding.
Batches, any forwarding, including with a shell, owned tunnels, `ssh -G`,
copy pre-validations and probes through a proxy always use `ssh` from the
`PATH`. Direct probes keep their native SSH negotiation. A launch
error names the configured executable and is kept in the history.

## Usage

```sh
ssho                            # Terminal interface
ssho --query 'tag:prod AND user:deploy'
ssho --query ''                  # Empty search, focus forced
ssho production                 # Direct connection
ssho production uptime          # Remote command and SSH exit code
ssho -t production sudo -v       # TTY allocation
ssho -c /path/to/ssh_config
ssho add [hostname]
ssho edit production
ssho move production
ssho search work --tags
ssho search prod --names --format json
ssho search --format simple
ssho info production --pretty
ssho --no-update-check
```

`--query TEXT` or `--query=TEXT` opens the interactive list with the advanced search already typed and the cursor active, even if `focus_search_on_start` is disabled or `--search=false` is given. The empty string and Unicode are accepted; control characters and line separators are rejected before the application is initialised. An invalid search syntax or an empty result remains editable. This option belongs to the root command and rejects any direct alias or subcommand; `--search` keeps its historical boolean role. Once a remote command has started, `--query` remains a literal argument of that command.

The initial search uses the loaded notes/favorites and the native effective values as soon as they arrive. It keeps the usual browser: F2, F5, groups, columns, availability method and return after SSH. It triggers no interactive connection and no history write before an explicit choice, even with a single result. `ssh -G` resolutions and diagnostics follow the usual preferences. An empty configuration opens a list that can be edited with F2; launching without `--query` keeps its historical offer to add a host. Completion consumes the `--query` text and then offers neither an alias nor an incompatible subcommand; relative Bash invocation is still supported.

`info` keeps the `sshm.info.v1` JSON schema and the declared values (with their null values) in `declared`, the historical fields and `source`. `target` now uses the effective values resolved by `ssh -G -F <config> <alias>`; `effective` contains in particular all identities and the repeated directives. `resolution` distinguishes success from error. Exit codes: 0 for a successful resolution, 2 for a missing host, 1 for a configuration or resolution error. A resolution error keeps the declared values, with `effective: null` and `RESOLUTION_ERROR`. This command neither migrates nor creates application data. The CLI search offers the `table`, `json` and `simple` formats.

A connection or a tunnel with a shell launched from the interface hands the terminal over to SSH, then returns to the list with the result (exit code or launch error). The search, focus, filters, selection and scroll position are preserved. The history is refreshed without reordering the displayed list; a new sort or search action uses the refreshed dates. Ctrl+C during SSH interrupts the connection and returns to the list; Ctrl+C in the list quits the application. A direct CLI connection keeps stdout, stderr and the SSH exit code (128 + signal if the process is interrupted), without opening the interface.

The history records connection attempts, with date, counter and last tunnel used. `last_result` separately keeps the end date, the SSH exit code or a launch error; `last_successful_completion` is the last process end with code 0. A remote command's exit code, even non-zero, does not allow concluding that authentication failed, and an interrupted tunnel never becomes an assumed success. Old files and their unknown fields are preserved. SSH changes are preceded by a backup in `ssho/backups/`. Unrelated blocks, comments, repeated options and multiple keys are preserved when editing. Moves prepare both writes and restore the destination if writing the source fails.


The sheet and the details panel share the effective values (address, user, port, all identities, bastion, command and TTY) and show the raw declaration and its file/line separately. Editing always preserves the declaration. The resolution of all aliases is cached in the background, even with `ping_on_start: false`; the Pending/Error states remain explicit. The cache is renewed after a change and with `p` (which also restarts the probes). An external change requires this refresh or reopening the application.

The symbols `? Unknown`, `~ Connecting`, `+ Online`, `! Offline` remain distinct without colours. Details/Info give the duration and the cause. By default (`availability_method: "ssh"`, missing value or `null`), a direct probe requires a successful SSH negotiation without authentication; an open HTTP port is not enough. A probe through a proxy uses OpenSSH and the `exit` command, and may fail at authentication: its cause is displayed. The subprocesses of the SSH diagnostic have a five-second limit. After capture into temporary files, only 256 KiB per stream are read into memory; these files may grow during the diagnostic window, with no disk size limit imposed. On Unix, the descendants of the diagnostic group are stopped when the process ends, successfully or not, as well as when the timeout expires; no shell is added around `ssh -G` (the OpenSSH `Match exec`/`ProxyCommand` directives keep their native behaviour).

**Columns**: `v` from the table or F2 → **Columns** opens an ordered draft. `↑/↓` selects, Space enables/disables an optional column, `Shift+↑/↓` moves an enabled column, `r` resets to the default, Enter saves and Esc cancels. `Name` remains mandatory and first. The available fields are Name, Hostname, User, Port, Tags and Last Attempt; User/Port follow the native resolution of the active alias, with the declared values as a fallback and `—` if absent. Long values can still be viewed with `i` / F4.

The `host_columns` preference is an ordered list in `ssho/config.json`, for example `["name", "port", "user", "hostname"]`. The default (or `null`) is `["name", "hostname", "tags", "last_attempt"]` and keeps the historical widths: Name alone up to 36 table cells, Hostname from 37, Tags from 65, Last Attempt from 95. For a custom list, the trailing columns are hidden first until everything fits; the selector shows **hidden: width**. Minimums: Name 12, Hostname 16, User 10, Port 5, Tags 10, Last Attempt 14 cells, plus one space between columns; Name alone adapts to narrower screens. The remaining space is shared between the displayed columns. Sorting remains tied to Name / Last Attempt. Groups keep their counters in Name. Saving re-reads the preferences and preserves external fields, target, search and collapsed groups; an externally changed availability method that gets adopted renews its diagnostics. A failure keeps the draft and displays the error. `v` remains text in the search. Empty lists, duplicates, unknown names or a missing/non-first Name are explicitly rejected.

**Availability**: `P` from the table or F2 → **Availability method** chooses SSH (default) or **ICMP local**; Enter saves `availability_method: "ssh"` / `"icmp"` in `ssho/config.json`, Esc cancels. Saving re-reads the preferences and keeps the external fields; a failure keeps the selector and the previous method. Switching preserves the search, selection and groups, replaces the diagnostics and restarts the probes in the background. A method changed externally and adopted during a group or alias choice also renews these diagnostics. The `is:online`/`is:offline` filters follow the displayed method. `P` remains text in the search.

**Bounded diagnostics.** Each native diagnostic operation (`ssh -G`, check through a proxy, `ping`) is limited to 5 seconds and to 256 KiB per output stream; at most 8 run at the same time, hence 16 capture files, anonymous and released by the system as soon as the operation ends, whatever its outcome. An output that keeps growing is stopped as soon as it is noticed (checked every 10 ms), with the error "output limit (256 KiB per stream) reached: the process was stopped", without waiting for the deadline. A large output from a finished command is truncated with the visible marker `[output truncated at 256 KiB]`, and an error message keeps only the useful end of a long diagnostic, preceded by `[beginning truncated]`. Reloading, changing the method or quitting **cancels** the running SSH diagnostics as well as the ICMP diagnostics: their process group is stopped, an old diagnostic no longer runs to completion, and closing waits for this clean-up to finish (a moment, never the deadline). The strict kernel limit on file size remains reserved for `ping`, a fully owned tool: a proxy helper or a `Match exec` command may legitimately write to its own log. The direct SSH check (a handshake without a process) is not interrupted by a cancellation; it keeps its timeouts of 5 s for the connection and 2 s for the exchange.

ICMP checks the network availability of the **effective Hostname obtained by `ssh -G`, from this machine**; it goes through neither ProxyJump nor ProxyCommand. A positive result proves neither that the SSH service is available nor that authentication succeeds. The duration shown is the diagnostic time (resolution included), **not an RTT**. The ICMP budget is five seconds per alias, resolution and ping included, with eight fixed workers. No reply as reported by the tool gives Offline; a missing tool, permission denied, external timeout exceeded or an error give Unknown with the cause. No sudo and no change of capabilities. On Unix, stdout/stderr are limited to 256 KiB per stream, and owned processes are cleaned up after completion, expiry, generation change or closing. Closing waits for this ICMP reaping; a kernel wait is not an absolute real-time guarantee.

Linux uses the iputils `ping` contract (IPv4/IPv6), macOS `ping` or `ping6` for a literal IPv6 address, FreeBSD the unified `ping`. The arguments and exit codes of the three platforms are covered by the tests. On macOS, a DNS name goes through IPv4 `ping`; `ping6` does not offer the same timeout options and an external expiry remains Unknown. Windows and the other platforms give Unknown: no success is inferred from the mere exit code 0 of `ping.exe`.

## File-level inherited tags

A header directive `# FileTags: work, lab` assigns these tags to all hosts declared in that file. It accepts any letter case for the word `FileTags` and spaces around `#`, the name and `:`. Repetitions add up in order, without exact duplicates; `Work` and `work` remain distinct. The header ends at the first `Host`, `Include` or `Match`, even without an argument. Included files apply their own header, without receiving the parent's tags.

The table and the sheet display `%work` for an inherited tag and `#prod` for an own tag. The TUI search (words and `tag:`), the CLI search and visibility use their union. The inherited tag `hidden` also hides the host; `H` shows it in the TUI, while the completion of `info` always allows finding it explicitly. These rules combine with the file filter and favorites.

The form only edits own tags; the destination's inheritance appears separately as read-only, including for an empty file. Editing, copying or moving never materialises inherited tags into `# Tags:`. After a copy or a move, the host receives the header of its destination. Headers stay in their file on deletion; late directives are kept as `# Inactive FileTags: ...` comments when a block is removed, so that they never become active by accident. The preview shows exactly these changes before confirmation.

Late directives are ignored and reported with file and line: only once on stderr per CLI command, in persistent lines in the TUI. F5 and saves renew these diagnostics; a failed reload keeps the last state. The full warnings can be read in the `h` help by scrolling, even in a narrow terminal.

In JSON, `tags` contains the union without graphical prefixes. When a host inherits tags, the additive keys `host_tags` and `file_tags` indicate the origin, in each search result and in `info.result`; `sshm.info.v1` is kept. Without inheritance, the historical fields and values are unchanged, in particular the null values of `info`. With a FileTags directive recognised in the loaded graph (even empty or ignored), an empty JSON search returns `[]`; without FileTags, the old text messages for an empty search are kept. The historical exit codes remain unchanged: 1 if no host is declared, 0 if the filter finds nothing.

## Keyboard navigation

### Native SFTP

`S` on a host, or F2 → **SFTP: open a session**, opens the OpenSSH `sftp` client in the terminal for **the exact alias selected**, with the SSH configuration of the session (`sftp -F <configuration> -- <alias>`). It is the native command-line client, not a two-pane file manager: `help` lists its commands there, `bye` or Ctrl+D returns to the list, found exactly as it was left (selection, search, filters).

- Before handing over the terminal, ssho writes a line there announcing the native client, the alias and the configuration; on return, the list says how it ended ("SFTP exit 0: beta (native client)", or the launch error if `sftp` is missing).
- **Custom launcher**: `ssh_command` does not apply to SFTP, since `sftp` starts its own `ssh`. If a launcher is configured, this is said before and after the session.
- The alias must be declared in the configuration; it is passed after `--` as a single argument and cannot be read as an option. No path or command is added by ssho.
- An SFTP session is not counted as a connection in the history. While a batch is running, the action is refused like the other foreground tools.
- On a group or with no host selected, the key explains why it does nothing.

### Exporting and importing application data

F2 → **Export / import data** is used to carry your data from one machine to another, or to keep a readable copy of it.

**Scope.** An export contains, as you choose (checkboxes, Space): favorites and notes, tunnel profiles and saved views **of the current SSH configuration**, saved commands, and portable preferences (language, display, list, keyboard). The number of entries of each section is shown before writing, and an empty section is not ticked. **Never included**: private keys, passwords, SSH files, connection history, backups, nor the SSH launcher (a local path). Since notes and commands may contain private information, the file is created readable by you alone (`0600`); an existing file is never replaced. `~/` stands for the home directory.

**Format.** A versioned JSON file (`"format": "ssho.export"`, `"version": 1`), with the date, the application version and the configuration it comes from. A file in another format, of an unknown version or containing an unknown section is refused with the reason. The source file of a view is written relative to the configuration folder when it lies inside it, and looked up the same way on import.

**Import.** An import always goes into the current configuration — the association is "this configuration + alias", never an alias name alone between two files. Reading a file changes nothing; the preview says where it comes from and where it will be imported, then for each section: new, identical and different entries, "for an alias unknown here" and "only here". Three explicit choices:

- **Entries present on both sides** (←/→): *Add only what is new* (default: nothing that exists here is changed), *Add, and replace what differs*, or *Replace the chosen sections entirely* (the entries that exist only here are removed).
- **Aliases unknown here**: leave them out (default) or import them anyway.
- the sections to import (Space).

One sentence sums up what the import would add, replace, remove and leave out; it is updated on every choice. A view whose source file does not exist in this configuration is flagged (applying it will ask for an update). The **Import** button asks for confirmation, with the focus on "Go back". Before anything is written, the result is first read back by the application's loaders in a separate folder — an invalid file changes nothing — then the current files are copied to `ssho/data-backups/<date>/`, and only then replaced. The list, the views, the catalogue and the preferences are reloaded immediately.

### Saved commands

The catalogue keeps personal commands under a name, with a description, the intended scope (one host or a batch, one host only, a batch only) and the parameters asked for on each use. `R` from the host list, or F2 → **Saved commands**, opens it; six named actions: **Use on this host**, **Use on the batch**, **New**, **Edit**, **Delete**, **Close**.

A command is written on one line, with its parameters in braces:

| Syntax | Parameter asked for |
| --- | --- |
| `{file}` | text, with no default value |
| `{file=/var/log/syslog}` | text, with a default value |
| `{lines:number=50}` | integer |
| `{level:choice=info\|warn\|error}` | one value among those listed (←/→) |
| `{{` and `}}` | literal braces, for example in `awk '{{print $1}}'` |

Example: `tail -n {lines:number=50} {file=/var/log/syslog}`. A name used more than once (`cp {f} {f}.bak`) is asked for only once. While you type, the form shows the parameters it recognises or the reason for a refusal (unclosed brace, unknown type, default value that is not a number…). Enter moves to the next field and never saves; the Save button or Ctrl+S saves.

**Substitution.** This is not a scripting engine: each value is checked against its type, then inserted as **a single protected word** for a POSIX shell (in single quotes, with the single quotes of the value escaped). A value can therefore add neither a command, nor a redirection, nor a substitution: typing `x; id` in a text gives `'x; id'`, and in a number an explained refusal. As a consequence, **do not put quotes around a parameter** in the command. Line breaks and control characters are refused. Limitation: the protection assumes a POSIX-like remote shell.

**Usage.** "Use" asks for the values (pre-filled with the defaults), shows the "Command to review" recomputed on every keystroke, then **hands the command over to batch preparation** (see Batches): a single explicit target for "on this host" — the marks of the list are left untouched — or the targets already marked for "on the batch". Checking the targets and the command, then the explicit launch, remain those of the batch: nothing runs from the catalogue, and no command is run when a host is opened. With no marked target, "on the batch" explains it and never chooses a target for you; a command meant for one host only is refused for a batch, and vice versa, with the reason.

**Secrets.** There is no "secret" parameter: a value would end up in the command line and in the batch history. Commands are stored in clear text in `ssho/commands.json` (a catalogue shared by all SSH configurations); never write a password there. The values typed at use time are not saved in the catalogue.

### Saved views

A view keeps the context of the list under a name: the search, the exact filter by file, "Favorites only", the visibility of hidden hosts, the sort order (name or recent connection), the grouping, grouped aliases and the columns. Examples: "Production", "To maintain", "Databases". Batch marks and the availability state are never part of it: they describe a moment, not a view.

`V` from the host list, or F2 → **Saved views**, opens the views screen: the current view, the list of views, the criteria of the selected one, then six named actions reached with Tab or ←/→ and run with Enter.

- **Apply** sets the list according to the view and returns to the list (`✓ View "Production" applied`). The grouping and the columns are saved in `config.json`, like a choice made by hand.
- **Save current as…** creates a view from the current list: the criteria that will be kept are shown before the name is typed. Enter in the name leads to the Save button and writes nothing; Ctrl+S saves. A name already taken is refused and explained.
- **Update** replaces the criteria of the selected view with the current list; **Rename** changes only the name; **Delete** asks for confirmation, with the focus on "Go back", and touches neither the list nor the SSH files.

After a view is applied, the banner of the list shows "View: Production". As soon as the search, a filter, the sort order, the grouping or the columns change, it shows "View: Production (modified)": the saved view is never overwritten without "Update". "Clear all filters" leaves the view. If the source file of a view is no longer part of the SSH configuration, applying it is refused with an explanation and nothing is applied: the filter is never silently widened to all files.

Views are kept in `ssho/views.json`, separately for each SSH configuration file (like favorites and notes). Names of 40 characters at most.

### Preferences, data and first launch

`o` from the host list, or F2 → **Preferences**, opens a single screen for the options of the application, in six sections:

- **Appearance**: language, colours, ASCII symbols.
- **Host list**: search focused at start, groups, grouped aliases, columns (Enter opens their editor, which saves on its own).
- **Connection and diagnostics**: availability method (SSH / ICMP local), probes at start, update notification, SSH launcher (read-only: array of literal arguments of `ssh_command`, which only launches ordinary connections — tunnels, batches, `ssh -G` and probes always use `ssh`).
- **Keyboard**: quick actions key, whether Esc quits from the list or not, shortcuts.
- **Native tools (read-only)**: `ssh`, `sftp`, `ssh-keygen`, `ssh-copy-id`, `ssh-add`, `ping` — found or not in the `PATH`, what it is used for, and what to do if it is missing.
- **Data (read-only)**: application folder, `config.json` (preferences), `metadata.json` (favorites and notes), `sshm_history.json` (history), `tunnel_profiles.json` (tunnel profiles), `views.json` (saved views), `commands.json` (saved commands), `backups/` (backups), `data-backups/` (copies before import) and the SSH configuration read by the session.

Each row shows the current value; under the list, the focused option is explained (its effect, then "takes effect as soon as it is applied" or "at the next start") and a long value, such as a path, is repeated there in full. ↑↓ chooses an option; Space, Enter or ←/→ changes its value **in the draft**: the row carries "● modified" and nothing is written. Tab reaches the **Apply**, **Cancel** and **Restore defaults** buttons; Ctrl+S applies too. "Restore defaults" only fills the draft (the options without a row of their own, columns and launcher, are kept) and puts the focus on Apply. Esc or Cancel on a modified draft asks for confirmation, with the focus on "Keep editing". Apply reads `config.json` again, replaces only the options of this screen in it — an option changed elsewhere in the meantime is not overwritten — then confirms "✓ Preferences saved".

**Shortcuts.** The Shortcuts row opens an editor: one row per action of the list, with its keys and where they come from (default, customised, F2 only). Enter or **Change key** waits for the next key, which becomes the shortcut of the action; **No key** leaves the action to F2; **Default** removes the customisation; **Done** (or Ctrl+S) hands the shortcuts back to the preferences draft, which writes them with the rest. A key that is already taken, reserved for navigation or kept by the quit keys is refused with the reason given by the shortcut table (`d is already used by "Delete"`). Quit keys (`quit_keys`) other than Esc are still set in `config.json`. An existing configuration that assigned `o` to another action sees that shortcut refused and explained, since `o` now opens the preferences.

**First launch.** When the `ssho/` folder has just been created by copying `sshm/`, the list announces it once ("Application data copied from the former sshm folder — !: details") and the Messages screen (`!`) keeps the full sentence for the session, with both paths: the original folder is unchanged, only the copy is used. No notice if the folder already existed or if there was nothing to copy; the copy rule itself is unchanged.

**Empty list.** Three situations are told apart, each with the useful key taken from the shortcuts in force: a configuration with no host ("No host in *file*. a: add a host"), no result for the filters ("… clear the search • F2: Clear all filters") and an unreadable configuration ("… !: see the error • F5: reload").

### Language, colours, symbols and small terminals

**Language.** The interface exists in English and in French. English is the default language; it is changed in the application from the Preferences (`o`) or directly with F2 → **Language** (in French: **Langue de l'interface**), an action that the palette search finds in both languages (`language`, `langue`). The selector has a title readable in both languages, marks the current language with `✓`, saves the choice in `ssho/config.json` (`"language": "en"` or `"fr"`) and applies it immediately, without a restart. A missing, `null` or unknown value gives English and never blocks startup. Key names follow the language (Enter / Entrée, Esc / Échap, Space / Espace). The following stay in English in both languages: OpenSSH terms and directives (`ProxyJump`, `RequestTTY`, `Local (-L)`…), the search syntax (`tag:`, `is:online`, `AND`), the command-line help (`ssho --help`) and the technical detail of errors returned by the system or by OpenSSH.

This guide quotes the English labels; the application itself shows only one language at a time, and the French interface uses direct equivalents (Filters / Filtres, Save / Enregistrer, Launch / Lancer, Go back / Revenir…).

**Without colours.** F2 → **Display: colours** turns colours off or back on (`"colors": false` in `config.json`). The `NO_COLOR` environment variable (any non-empty value, [no-color.org](https://no-color.org) convention) turns them off too, and takes precedence over the preference: the action explains it instead of toggling for nothing. Without colours, no colour sequence is sent to the terminal; the selection stays visible (reverse video and `▶`), indicative values are dimmed, and each state keeps its text or its marker (`Error:`, `+ Online`, `● Modified`…). The content of the screens is identical character for character.

**ASCII symbols.** F2 → **Display: ASCII symbols** (`"ascii": true`) replaces frames and markers with ASCII characters for fonts or terminals that do not display them: `+`, `-`, `|` for the frames, `>` for `▶`, `o` for `●`, `+` for `✓`, `=` for `≡`, `v` for `▼`, `^v<>` for the arrows, `|` for `•`. The accented letters of the text and the user's data (host names, notes) are not rewritten.

**Widths.** The legend of the list picks its version according to the width, then gives up a line that would be cut rather than showing it without its end: a longer language never loses the end of a line. In French, the reminder of the quick sequence (`Ctrl+G : rapide`) appears when a line has room for it (for example at 100 or at 140 columns, not at 120); the help and F2 always show it. The help takes up to 126 columns.

**Terminal too small.** Below 24 × 6, the host list announces "Terminal too small (W×H)" with the minimum size, instead of a truncated table. Forms, selectors and confirmations keep their compact layout, with the active control and the cursor; Esc always goes back one level there.

Not supported: screen readers and mouse. On a light theme, the display without colours serves as a fallback.

### Searching, filtering, navigating

The home screen distinguishes three states, and a single element has the focus (its frame carries a `●` in addition to the colour):

- **Input**: `/`, Ctrl+F or Tab open the "● Search" zone, with the native cursor. Every character and every editing gesture is text there; Enter connects to the selected host. The bottom of the frame, on the left, shows the count, for example "12 hosts of 240".
- **Navigation**: Tab or Esc give the focus back to the "● Hosts" table. The input zone disappears entirely: no inactive field stays on screen, and the table gains two rows. The query and the position of its cursor are kept for the next edit.
- **Active filters**: a single "Filters: …" banner sums up everything that restricts the list — search text, "Favorites only", "Hidden shown", grouping, and on its own line "File: exact path". It recalls the useful keys (`/` edit, Backspace clear, `C`/F2 all files) and the host count. With no filter, it shrinks to "/ Search". On a short terminal, everything fits on one line, the file first.

**Esc no longer empties the search**: it only closes the input, the filter stays applied and displayed. *A change from previous versions, where the first Esc cleared the query.* Clearing is an explicit action: **Backspace from the table** ("Clear the search"), Ctrl+U in the input, or F2 → "Clear all filters", which also removes the filter by file, favorites only and the display of hidden hosts, without touching the grouping.

The "n hosts of total" count counts the hosts kept by the filters, never the group rows; the selection position and the range of rows stay under the table. A search syntax error is shown just above the zone, and the list stays as it is.

### Creating, editing and duplicating a host

The host form fits on **one page**, without tabs, and its title names the mode: "Hosts > Add host", "Edit alpha", "Duplicate alpha".

- **Target file always visible**, at the top. For an addition or a copy, the **Change** button opens the file selector then returns to the same draft: values, cursors and focus are kept, and the inherited tags follow the new file. An existing block is moved with the Move action (`m`), which shows its source and its destination.
- **The essentials first**: Alias, Address, User, Port, SSH key (with **Choose**, equivalent to F9). Three collapsible sections follow: **Additional aliases**, **Advanced options** (ProxyJump, ProxyCommand, SSH options, remote command, RequestTTY) and **Tags and note**. They are closed for an addition; when editing or copying, a section that already contains values is open, and its title says what it contains.
- **A single traversal order**: Tab and ↓ move forward, Shift+Tab and ↑ move back, through the fields, the section titles and the buttons, in a loop. Tab never expands a section; Enter or Space on its title expands or collapses it.
- **Enter no longer submits**: in a field it moves to the next field, and after the last one it puts the focus on **Preview the changes**. Only this button, or Ctrl+S from anywhere, opens the preview; writing then requires the preview to be confirmed. *Previously Enter on the last field of the Advanced tab started the save.*
- **An empty field writes nothing, and says so** instead of showing a greyed-out example that looks like a value: "required", "left to OpenSSH", "22 — OpenSSH default, not written". The example for the active field is shown under the form ("e.g. …"). *Change: a new host created without a user no longer writes `User <local user>`; OpenSSH then uses the local name by itself, or the value of a `Host *` block. What you type is what is written.*
- **Additional aliases**: visible commands **+ Add an alias** and **Remove** (Alt+A and Alt+D remain accelerators). A new host can now declare several aliases as soon as it is created. They share the same block; favorites, notes and connections target one exact alias.
- **Personal note**: when editing, the Tags and note section gives access to the note, kept in the data of `ssho` and not in the SSH file; its editor returns to the draft.
- **Errors**: a summary at the top, the message under the field concerned, the focus on that field, and only the section of the first error is expanded. The other values are kept.
- **Text editing**: Ctrl+K once again deletes to the end of the line; Ctrl+J/Ctrl+K no longer switch tabs.

**Wide terminals** (full-screen Full HD, 4K, 32:9): the form does not exceed 96 columns and each button immediately follows its value ("Choose" after the key, "Remove" after an alias, "Edit" after the note, "Change" after the path). No button is pushed to the right edge. Likewise, the host table, the active tunnels table and the profiles table no longer spread beyond 160 columns, and the "n hosts of total" count follows the filter banner instead of being right-aligned.

On a terminal narrower than 44 columns, the label moves above its input. The native cursor stays visible in the active field; a focused button or section title has no cursor and receives neither keystrokes nor pasted text.

### Help, messages and running jobs

- **Contextual help** (`?`, F1, or "Help" in F2 from a sheet or a form): it starts by saying what the screen it was opened from is for (list, sheet, host form, tunnel, note), then lists the actions with the shortcuts actually in force. `/` starts a **search** there: only the matching lines remain, case and accents ignored; while typing, letters are text, and Esc clears the search before closing the help.
- **Message levels spelled out**: `Error:`, `Warning:`, and a short confirmation after each write ("✓ Saved alpha", "✓ Deleted beta", "✓ Moved … to …") that disappears on the next key. A warning is no longer presented as an error.
- **Message readable in full**: an error too long for its line ends with "(!: full text)". `!` (or F2 → "Messages and last error") opens a screen that shows in full, with line wrapping and scrolling, the last error, the current warnings (refused shortcuts, ignored FileTags) and the last SSH result. Reading a message does not clear it. In the column editor and the selectors, an error now takes as many lines as it needs.
- **Running jobs**: as long as an owned tunnel, a batch or an availability probe is running, a line "Jobs — tunnels: 2 active (F7: follow or stop) • batch: 3/10 done (B in the list: follow or cancel) • availability: 5 probe(s) running" stays displayed at the bottom of **all** screens. A running probe is confused there neither with a past measurement nor with an SSH session.

### List columns

The column editor (`v`) shows, under the checklist, the **result at the current width**: "Shown at this width: Name | Hostname | Tags" in the final order, then "Active but short of room: …" for the columns that are enabled but hidden for lack of room. A disabled column (`[ ]`, "off") can therefore no longer be mistaken for a column short of room ("hidden: width"). `Name` remains mandatory and first ("fixed").

The actions are named and reached with Tab: **Move up**, **Move down**, **Reset**, **Apply**, **Cancel**. Moving a column is thus an action distinct from choosing the row, without having to memorise Shift+↑/↓ (still available, like Space, `r`, Enter and Esc). The move does not wrap around, and everything remains a draft until Apply.

### Change preview, deletion and restore

Before any write to an SSH file, the preview first shows a **plain-language summary**: the operation (`ADD`, `EDIT`, `COPY`, `DELETE`, `MOVE`, `RESTORE`) and the host concerned, the file or files (source → destination for a move), and what happens to the personal data — for example "Favorite, note and tunnel profiles follow a renamed alias", or that they stay in place after a deletion. A deletion therefore names its object without relying on the colour of the diff.

- The **diff has the focus for reading**. *Enter no longer confirms anything there*: a message says so and not a single byte is written. Tab reaches the button named after the operation — **Save**, **Copy**, **Delete**, **Move** or **Restore** — then **Back**. Pressing Enter repeatedly from the form can therefore no longer validate two steps at once.
- **Ctrl+S** is the explicit confirmation, shown in the footer: Ctrl+S opens the preview from the form, Ctrl+S confirms it.
- Esc or **Back** return to the draft, intact.
- **Backups (F8)**: typing filters the list by date, file or type of change; Enter opens the restore preview, which follows the same rules. An unknown origin stays flagged and cannot be restored.

### Keys: local and remote actions kept apart

The keys screen states its context. Opened with `K`, it **manages the keys** of the selected alias; opened with F9 or the Choose button of a form, it only serves to **choose a key for the draft** ("only its IdentityFile is filled, nothing is deployed") and offers neither association nor deployment.

- **Named actions**, grouped by scope: `Local:` **Create key**, **Add to agent**, **Associate with this host** • `Remote server:` **Deploy public key**. ←/→ or Tab chooses, Enter runs; `n`, `a`, `t`, `d` remain direct shortcuts. In the selector of a draft: **Use this key**, Create key, Add to agent.
- **Generation**: path and comment, then the **Continue in terminal** and **Back** buttons. *Enter in a field no longer generates: it moves to the next field, then to the button.* The screen states that `ssh-keygen` will ask for the passphrase in the terminal before returning to the application.
- **Deployment**: the review states at the top that it acts on the remote server, the exact alias and the public fingerprint, then offers **Back** (initial focus) and **Deploy to alias**. *Enter alone no longer deploys.* The freezing of the target and the revalidation of the public key are unchanged.
- Passphrases remain entirely entrusted to the native tools; no vault is added.

### Batches: a step-by-step flow

A line at the top shows where you are in the flow: **Targets → Command → Review → Results**, with the current step in square brackets.

- **No marked target**: `B` opens a page that explains it and names two ways out, **Back to the list** (initial focus) and **Add the n host(s) shown by the filter**. The filter is never turned into targets without this choice, and no command can be typed before there is a target.
- **Preparation**: Tab walks through the command, the parallelism, the timeout, the target list, then the **Review** and **Back** buttons. The arrows act within the active control: they only move the target selection when the list has the focus (its frame says so), otherwise they move to the neighbouring control. Enter advances from one field to the next and stops on Review.
- **Review**: full command, parallelism, timeout and frozen targets; buttons **Back to edit** (initial focus) and **Launch on n target(s)**. *Enter no longer launches the batch: it confirms the marked choice, so it goes back to editing as long as Launch has not been chosen with Tab. Ctrl+L, shown in the footer, remains the explicit confirmation.*
- **Results**: two tabs named **Output** and **Errors** (Tab), the open tab marked `▶`, truncation reported. Going back does not stop the batch; `s` cancels it. While a batch is running, the "Jobs" line recalls it at the bottom of every screen.

### Tunnels: form, profiles and monitoring

- **Forwarding direction in plain words**: the type is chosen among three named options — Local (-L), Remote (-R), Dynamic (-D) — and a computed sentence spells out the path with your values, for example "Listen on this machine, port 5433 → reach db.internal:5432 from the SSH server". For a remote forwarding the sentence is reversed. The labels distinguish the listening port from the destination.
- **Visible launch mode**: "Shell session" or "Tunnel only", with an explanation of their life cycle (the tunnel stops when you leave the shell, or runs in the background and is monitored in F7). Ctrl+T remains a shortcut.
- **Distinct buttons** **Launch**, **Save profile** and **Cancel**, reached with Tab. The profile name is only required for saving. Enter advances from field to field and stops at the last one: repeating it neither saves nor launches anything. Ctrl+S and Ctrl+L remain available.
- **Profiles (F6)**: under the list, the path of the selected profile spelled out in full and the actions **Open**, **Launch**, **Duplicate**, **Delete**, **New** (←/→ to choose, Enter to run the action). Duplicating opens an unsaved draft named `name-copy`. Without a profile, only New acts.
- **Active tunnels (F7)**: `s` no longer stops directly; a confirmation names the tunnel (identifier, host, ports) and keeps the focus on **Go back**. *Previously `s` stopped immediately.* Going back from the screen stops nothing.

### Personal notes

The note editor (`N`, or the Note button of a sheet or of the form) states what the note is attached to and where it lives: the alias, the SSH file of the host, then the `metadata.json` file of the `ssho` data, with the reminder that a note is never written to the SSH file and is not the same thing as a tag. The **Saved** or **● Modified** state is shown at the top.

- Enter inserts a new line; Tab reaches the **Save** and **Cancel** buttons (←/→ moves from one to the other, Enter or Space activates it); Ctrl+S saves from the text. A button that has the focus receives neither keystrokes nor pasted text.
- Cancelling a modified note always offers to keep editing.
- In the list, a host that carries a note shows the discreet marker `≡` between its availability and its favorite; the text of the note stays in the sheet and the F4 panel, with no dedicated column.

### Host and group sheets

The sheet of a host (`i`) is organised in five sections, the same as the F4 panel: **Summary** (alias, address, user, port, aliases of the block, tags, note), **Effective configuration** (`ssh -G` resolution, keys, proxies), **Declaration and source** (file and line, declared target, options), **Measured availability** (SSH or ICMP method, state, duration, cause) and **Connection history**. An attempt, an exit with code 0, an availability state and an authentication remain distinct pieces of information, each on its own line.

- **Reading never modifies anything**: Enter alone does nothing and says so. *Previously Enter opened the edit form.* The actions are named under the sheet — **Connect**, **Edit**, **Note**, **Copy**, **Close** — and Tab or ←/→ put the focus on them before Enter runs them; `e` remains a direct shortcut to editing, F2 gives the other actions.
- **Copy** opens a choice of a single value (alias, `ssh alias` command, address, `user@address`, source file). The copy is requested from the terminal through the OSC 52 sequence, then the value stays displayed alone on its line: if the terminal or the multiplexer does not honour the request (tmux requires `set -g set-clipboard on`), it can be selected by hand. Nothing else is exported.
- The **sheet of a group** states its criterion: hosts of a file, or hosts of that file sharing a main tag.

### List markers, groups and multiple selection

Each marker of the Name column has its own zone, so that a favorite never looks like a batch target:

| Zone | Marker | Meaning |
| --- | --- | --- |
| Batch target | `[x]` / `[ ]` | The alias is marked for a batch. This zone only exists when at least one target is marked. |
| Availability | `+` `!` `~` `?` | Online, offline, probe in progress, unknown. |
| Note | `≡` | The host carries a personal note. |
| Favorite | `*` | Pinned host. |
| Group | `▼` / `▶` | Header open or collapsed; "(4 • 2 targets)" gives its hosts and its marked targets, even when collapsed. |
| Alias | `primary→active`, `+2 alias` | Grouped block: alias used for the connection, and number of other aliases of the block (F3 to choose). |

A **legend** of the markers present is shown above the footer when the terminal has at least 20 lines; the help (`?`) always gives it.

- **Visible multiple selection**: as soon as a target is marked, the line "Batch: 4 marked targets, 1 outside the filter • B: view / prepare • F2: clear all" stays displayed. Marks are frozen targets: a search, a filter or a collapsed group neither removes them nor distorts the count.
- **Enter** connects on a host and expands or collapses on a group header; the footer says which, depending on the selected row.
- **A group header is never the silent target of an action**: `x`, `e`, `d`, `m`, `N`… on a header display "… — Select a host, not a group" instead of doing nothing. To target a whole group, F2 → "Batch: target the filter results" after filtering.

### Shared selectors

Choosing a destination file, the source file filter, an exact alias, the grouping and the availability method all use the same screen:

- a **title** and a line saying what is being chosen; the **value in effect** carries a check mark `✓` and the note "(current)", distinct from the cursor `▶`;
- ↑/↓ **wrap around**, Home/End go to the ends, **Enter** applies, **Esc or Ctrl+C** cancel without changing anything;
- a **detail zone** under the list describes the highlighted choice: full path and number of hosts of a file ("no host: empty file…"), file and scope of an alias, what SSH or ICMP measures, effect of a grouping mode;
- **paths** are shown in full as long as the width allows; otherwise as the shortest ending that tells them apart (`…/a/hosts` and `…/b/hosts`), the full path remaining in the detail zone. "All files" is a choice in its own right;
- in the file and alias selectors, **typing narrows the list** (case and accents ignored, on the full path). Narrowing chooses nothing: the check mark stays on the value in effect until Enter. *In these three selectors letters are therefore search text: `j`, `k` and `q` no longer navigate or close there; they keep that role in the grouping and method selectors, which have no search.*

### Back, close and confirmations

- **Esc goes back one level**, everywhere. In the list, which is the root, Esc no longer closes the application: a line recalls the exit keys. Adding `"esc"` to `key_bindings.quit_keys` restores the former exit with Esc; `disable_esc_quit: true` always forbids it. *Change from previous versions, where Esc quit from the table.*
- **Ctrl+C follows the same rule as Esc** in every sub-screen, including the key details (F4), the active tunnels and the results of a batch. In the list, Ctrl+C and `q` ask to quit.
- **Going back stops nothing**: `q`, Esc or Ctrl+C from Active tunnels or Batch results return to the previous screen; the tunnels and the batch keep running. *Previously `q` asked to quit there.* Stopping remains the explicit action `s`.
- **Quitting** with active tunnels, a running batch or an unsaved draft opens a summary of what will be stopped or lost, with two named choices: **Go back** (initial focus) and **Stop and quit**.
- **Modified draft**: Esc offers **Keep editing** (initial focus) and **Discard**. Tab or ←/→ changes the choice, Enter confirms the choice marked `▶`, Esc and Ctrl+C keep editing. Repeated Enter can therefore no longer discard a draft. *Previously Enter discarded directly.*
- **Wrap-around**: the simple selectors (target file, file filter, alias, groups, availability method, tunnel profiles) go from the first choice to the last and back. The host list, the logs, the diffs and the column order remain bounded.

### Shortcuts, prefix sequence and customisation

F2 remains the main way to reach every action. Three complements are added to it:

- **Help**: `?` or F1 from the table (F1 also from the search); `h` remains available. The key that opens the help closes it again.
- **Prefix sequence**: Ctrl+G, released, then a letter. A "Quick actions" panel lists the letters and stays open until a choice or Esc, with no timeout. An unknown letter runs nothing; an unavailable action shows its reason. These letters belong to the sequence only: `d` duplicates there, whereas direct `d` still deletes from the table. Letters: `a` Add, `e` Edit, `d` Duplicate, `m` Move, `n` Note, `i` Host sheet, `t` Tunnel profiles, `k` Keys, `b` Batches, `f` Source file, `g` Groups, `c` Columns, `r` Reload, `?` Help, `q` Quit.
- **Historical direct shortcuts**: unchanged by default, active only when the table has the focus. In the search, every character and every editing gesture (Ctrl+A/E/K…) remains text.

`key_bindings` in `ssho/config.json` controls these choices:

| Field | Effect |
| --- | --- |
| `prefix_key` | Prefix key, for example `"ctrl+g"` (default, also for `null`) or `"alt+x"`. `""` disables the sequence. A navigation key, a plain character or a key that is already assigned are refused. An editing gesture such as `"ctrl+a"` is accepted but stays reserved for text in fields: the prefix then only acts outside text entry. Ctrl+A and Ctrl+B are often captured by tmux or screen. |
| `shortcuts` | Table `action identifier → list of keys`, which replaces the historical keys of that action only. `{"host.edit": ["E"]}` frees `e`; `[]` leaves the action to F2. Two actions can swap their keys. |
| `quit_keys` | Exit keys from the list (unchanged). |

Keys are written `a`, `D` (case matters for a character), `f6`, `ctrl+f`, `alt+x`, `enter`. A conflicting preference is never applied silently: it is ignored, a "Shortcuts: n preference(s) refused" banner appears in the list and the help gives the reason (`d is already used by "Delete"`). The F2 palette, the footer and the help always show the keys actually in effect. These preferences are edited in the JSON file or with the shortcut editor of the Preferences screen.

Action identifiers: `host.connect`, `host.add`, `host.edit`, `host.duplicate`, `host.move`, `host.delete`, `host.info`, `host.details`, `host.favorite`, `host.note`, `host.alias`, `tunnel.form`, `tunnel.profiles`, `tunnel.active`, `availability.test`, `availability.method`, `search.focus`, `search.clear`, `filter.clear`, `filter.favorites`, `filter.hidden`, `filter.source`, `filter.source.clear`, `view.aliases`, `view.groups`, `view.columns`, `sort.toggle`, `sort.name`, `sort.recent`, `batch.mark`, `batch.select_visible`, `batch.clear`, `batch.open`, `batch.results`, `keys.open`, `app.backups`, `app.reload`, `app.palette`, `app.help`. Exit is configured through `quit_keys`.

Whether a combination is recognised depends on the terminal and on a possible multiplexer: if the chosen prefix does not reach the application, choose another one or disable it; every action remains reachable through F2.

The main screen uses a compact title and the whole available height for the list. **Name** stays visible; **Hostname**, **Tags**, then **Last Attempt** are added depending on the width. Texts that are too long end with an ellipsis `…`, measured in Unicode columns. Below the table are the number of filtered/total hosts (including hidden hosts), the selected position and the range of rows displayed. The forms keep the **General / Advanced** tabs and their field groups.

| Context | Keys |
| --- | --- |
| Search | Type to filter; ↑/↓ or Ctrl+P/Ctrl+N to select; Enter to connect |
| Reload external changes | F5 from the search, the table or the host sheet; or Reload in F2 |
| Group aliases | `A` from the table or F2 → Grouped aliases; persisted preference, disabled by default |
| Collapsible groups | `g` from the table or F2 → Groups: flat, files, files → main tag |
| Navigate within groups | ← collapses or selects the parent; → expands or selects the first child; Enter/Space toggles a group; `i` shows its sheet |
| Choose a specific alias | F3 or F2 → Choose an alias; ↑/↓ then Enter, Esc cancels |
| Action palette | F2 from the search, the table, a sheet or a form; type to search, ↑/↓ wrap around, Enter runs; Esc cancels without changing the search or the focus |
| Change focus | Tab or Shift+Tab; `/` to return to the search from the table |
| Clear or go back | Esc closes the input without clearing the filter; Backspace from the table or Ctrl+U in the input clear the search; from the table, Esc does not quit (`q` or Ctrl+C, depending on `quit_keys`) |
| Table | ↑/↓ or j/k; a add, e edit, d delete with confirmation, m move, D duplicate |
| Table scrolling | PageUp/PageDown; Home/End for the first/last host |
| Information and help | i for the host sheet, h for the help |
| Details next to the list | F4 from the search or the table; Alt+↑/↓ to scroll line by line, Ctrl+PageUp/PageDown page by page |
| Connectivity and visibility | P/F2 chooses SSH or local ICMP; p reruns the checks, H to show the hosts carrying the `hidden` tag |
| Favorites and notes | `*` pins/unpins, `N` opens the personal note; F2 → Favorites only turns the filter on/off |
| Personal note | Ctrl+S saves, Enter adds a line, ↑/↓ and Home/End move the cursor; Esc/Ctrl+C cancels, with confirmation if modified |
| Sort | s toggles name/date, n sorts by name, r by last connection |
| Tunnel | f; ←/→ for Local, Remote or Dynamic, then ports and addresses |
| Host form | Tab/↓ and Shift+Tab/↑ walk through fields, sections and buttons; Enter: next field or activate; Enter/Space on a title: expand; Ctrl+S: preview; Esc or Ctrl+C: cancel |
| Key assistant | K from the table or F2 → OpenSSH keys; F9 from a form only fills IdentityFile; ↑/↓, PageUp/PageDown, Home/End navigate; F4 shows the full details/errors |
| Tunnel profiles | F6 or F2 → Tunnel profiles; a creates, Enter/e/r edits or renames, d deletes with confirmation, Ctrl+L launches |
| Backups | F8 or F2 → Backups; type to search; Enter opens the diff; Tab then Enter on Restore, or Ctrl+S, confirms; Esc goes back |
| SSH preview | Add/edit/copy/delete/move: per-file diff; Enter applies the bytes shown, Esc returns to the input |
| Multi-host commands (Unix) | x marks the active alias; B prepares the batch; F2 explicitly selects the filter results, clears the marks or opens the last results |
| Batch preparation | Tab: command, concurrency, timeout, targets, buttons; ↑/↓: target if the list has the focus, otherwise neighbouring control; Enter: next field then Review; Ctrl+L: review, then Ctrl+L or the Launch button to launch; Esc: back |
| Batch results | ↑/↓: host; Tab: Output / Errors; PageUp/PageDown: output; s: cancel the batch; q, Esc or Ctrl+C: back, the batch keeps running |
| Active tunnels | F7 or F2 → Active tunnels; s asks for confirmation then stops the selected process; q, Esc or Ctrl+C go back without stopping anything |
| Tunnel form | Ctrl+T chooses shell session or tunnel only; Ctrl+S saves the named profile without SSH; Ctrl+L launches the current forwarding; Enter advances without launching; Esc/Ctrl+C cancels |
| Text editing | Ctrl+A/E or Home/End for start/end of line; ←/→ or Ctrl+B/F for one character; Ctrl+←/→ or Alt+B/F for one word; Backspace/Ctrl+H deletes before, Delete/Ctrl+D after; Ctrl+U deletes before the cursor, Ctrl+K after, Ctrl+W or Alt+Backspace deletes the previous word |
| Discarding changes | Tab or ←/→ chooses between Keep editing (initial focus) and Discard; Enter confirms the marked choice; Esc or Ctrl+C resumes the form exactly as it was |
| Additional aliases | "+ Add an alias" and "Remove" buttons in the section; Alt+A adds, Alt+D removes the active additional alias |

Each error appears under its field and validation puts the focus on the first error, expanding its section. The native cursor stays visible during input, including with wide characters, combining accents and emoji. Cancelling leaves an unchanged form immediately; only modified values, aliases or tunnel type ask for confirmation. Going back to the initial values removes this confirmation. In the confirmation, Esc or Enter on the marked choice "Keep editing" resume the form with its text, its cursor and its navigation; "Discard", chosen with Tab then Enter, returns to the parent view.

Hidden hosts remain reachable by direct connection. The TUI search combines several words on the name, the address, the user and the tags. The CLI search keeps its own field filters.

**F2 palette.** F2 opens a searchable palette of every action, from the search as well as from the table:

- **Search**: type words of the name, of a synonym or of the category directly (`tunnel`, `copy`, `method`, `keys`). The search ignores case and accents; every word must match. In the palette, letters are only search text: `d` or `q` neither delete nor quit. A single match is never run on its own.
- **Choose**: ↑/↓ (or Ctrl+P/Ctrl+N) **wrap around** from the first action shown to the last and back, without stopping on category titles; PageUp/PageDown and, when the search is empty, Home/End. **Enter** runs only the selected action. Esc, Ctrl+C or F2 close the palette without changing anything in the search, the focus or the selection of the list.
- **Action / Shortcut / State columns**: a label never contains a key; the Shortcut column shows the key actually in effect (`—` if the action only exists in F2); State gives the current value ("on", SSH/ICMP method, group mode, filtered file, active tunnels, marked targets). Labels stay whole at 80 columns.
- **Categories**: Connection, Host, Organization, Tools, Application. A line under the list describes the selected action.
- **Unavailability with a reason**: an impossible action stays listed in grey with its reason ("Select a host", "Select a host, not a group", "No file filter is active", "No target is marked", "No batch was launched"). Enter on that row explains it and runs nothing.
- **Contextual palette**: F2 also works from a host sheet (actions on that host) and from a host form, a tunnel form or a note. In the latter case the draft stays intact under the palette, which only offers the screens from which you come back to the draft (Active tunnels, Help).

Without a selected host, Add, Help, Reload, Active tunnels, Backups, the filters, the groups and the batch actions (except marking an alias) remain available; "Favorites only" therefore lets you leave a filter with no results. "Test availability" reuses the check of all hosts, like `p`. The bar under the table adapts its reminders to the width; on a narrow screen, F2 then "help" gives access to the detailed shortcuts.

**Grouped aliases** (`group_aliases: true`, absent/null: `false`) shows one row
per `Host` declaration, without modifying SSH. Only the aliases of the same file,
the same reliable line and the same declaration are brought together. The primary
name stays visible; `primary→active` indicates a secondary target, even in a
narrow column. The details list the aliases. The counters distinguish the
projected blocks and the aliases matching the filters, out of the raw total.

The search examines each alias with its own native values, notes and
favorites. The active alias is kept if it is still eligible; otherwise the main
one is chosen if it matches, then the first matching alias. F3 lets you choose
another alias explicitly, even when their names are ambiguous for the
search. This choice only removes the incompatible search/favorite/hidden
constraints, keeps the exact source file and does not start a connection.
Esc keeps the cursor, the query and the context. Actions, information,
notes and profiles always apply to the active alias; the CLI and completion keep
all aliases. The toggle rereads the preferences before saving so as to keep
the other settings changed during the session; an error leaves the previous mode
active and displays the diagnostic.

**Collapsible groups.** `g` from the table or F2 → Groups chooses the flat list (`flat`, the default, including when the field is absent/null), the files (`files`) or the files then their main tag (`file_tags`). The main tag is the first own tag, otherwise the first inherited FileTag, otherwise "No tag". Each host, or each block when alias grouping is enabled, appears only once; all its other tags remain searchable. Favorites and the recent sort order the hosts within their group. Files are told apart by exact path; a source number distinguishes them even if the path is truncated. `i` opens a group sheet with the full path, line wrapping and scrolling on small screens; F4 also shows this information on wide screens.

On a header, Enter/Space expands or collapses, ← collapses or moves up to the parent, → expands or moves down to the first child. Host actions (SSH, editing, deletion, notes, favorites, profiles, alias choice) are unavailable; Enter on a host still connects. In the search, Space and ←/→ remain typing keys. A new query opens the parents and selects the first matching host; collapses made during that search are temporary. Clearing the query restores the previous manual collapses. F5 and returning from SSH keep the selected identity and the collapses; a save opens the parents of the saved host only if the exact source filter allows it. The counters distinguish hosts/blocks, aliases and groups; pagination also counts the headers.

Only the view mode is persisted, once the reread preferences have been written successfully; collapses last for the session. External changes to the launcher, alias grouping and unknown fields are preserved. A failure leaves the mode and the selection intact and displays its diagnostic in the selector. The view creates no virtual folder and makes no change to the SSH configuration.

**Duplicate** (`D` from the table or F2 → Duplicate from the search) always asks for the target file, including the source file, then opens a prefilled add form. A new concrete name is required: no alias of the original, SSH pattern or name already present in the root configuration and its Includes, even hidden and regardless of case, is accepted. The copy keeps the represented SSH fields, the options and the additional declared identities; the target, the user and the port come from resolving the source alias with `ssh -G`, to keep the inherited values. The implicit key paths of `ssh -G` do not become declared identities. If this resolution is still pending or fails, a message explains to retry after resolution or F5; navigation remains available. Before any write or safety backup, a private copy of the Include graph lets `ssh -G` verify the requested target, user and port. The new block is inserted before the first `Host`/`Match` of the target file, keeping the comments and tags of the original block; the preceding global directives and Includes stay in place, and port 22 is written explicitly. A destination whose earlier values force a different connection is refused: choose a file earlier in the configuration (often the root), or fix the global values, then try again. Files remain unchanged when the operation is refused. This check accepts literal Includes, globs, absolute paths, `~/` and OpenSSH's native relative paths; it cautiously refuses Include paths with `$` variables, `%` tokens, `~user`, or matches that are not files. Outside Unix, this check only accepts Includes without globs. An Include file with no `Host`/`Match` boundary cannot receive the copy; choose another file. The original and its aliases remain unchanged. A copy is an ordinary new host: its favorites and personal notes are not cloned. Ctrl+S saves then selects the copy; Esc cancels with the usual discard rules.

Sorting, visibility and reloading keep the selected host by its name and source file. An add, a rename or a move selects the resulting host; if the previous search hides it, the search is cleared, and hidden hosts are shown if necessary. An edit opened from the sheet returns to that refreshed sheet after saving, or to the initial sheet after cancelling. Cancelling an add opened from the file selector returns to that selector. Forms launched directly from the CLI exit after saving or after cancelling their initial view.

**Tunnel profiles**: F6 opens the profiles of the selected alias, even from the search; the F2 palette also offers "Tunnel profiles". Several local (`-L`), remote (`-R`) and SOCKS (`-D`) profiles can coexist. The name accepts up to 100 characters, with no control characters or leading/trailing spaces; names are case-sensitive. Editing the name field then pressing Ctrl+S renames the open profile; a name already taken is refused without overwriting. Ctrl+S stays in the form, shows "Profile saved" and starts no SSH. Ctrl+L launches the current values in the chosen mode, including an unsaved or modified draft; launching does not implicitly save a profile. Enter on the last field launches nothing. `d` asks for confirmation before deleting. Esc cancels without writing, with a confirmation only if the form has changed.

The `f` key keeps the draft of the last forwarding from the history (also available in the profile list). It can be launched directly with Ctrl+L without a name, or named then saved with Ctrl+S. Profiles are stored in `~/.config/ssho/tunnel_profiles.json` (or the XDG/APPDATA directory), by canonical path of the root configuration then by alias, with atomic writes, private permissions on creation and preservation of unknown JSON fields. A root that does not exist yet keeps a stable identity after creation. Profiles follow the alias renames made through the SSH form; a destination already taken stays intact and the old association is kept, with a separate warning after the SSH save. Profiles modify neither the SSH files nor the `sshm/` folder. Each profile also keeps its launch mode; older profiles without a mode use a shell session. The mode and the ports are saved in the same atomic write.

**Multi-host commands (Unix)**: `x` marks/unmarks the active alias; a header is never a target. `B` prepares the existing marks only, without implicitly selecting the current row. F2 also offers "Batch: target the filter results" (projected active aliases, including under collapsed groups), clearing the marks and reviewing the last results. Search, alias choice, collapsing and F5 do not change these captures. Targets keep the exact alias, the canonical SSH root and the source file; an alias that has disappeared or moved is refused at launch. The content of the SSH files remains what OpenSSH reads at the time of each connection: it is not an immutable copy of the configuration.

The preparation screen shows the exact, navigable list, the command and the full, scrollable paths. Tab chooses command/concurrency/timeout/targets; ↑/↓ moves through the targets, PageUp/PageDown through the details. Ctrl+L opens the preview, then Enter actually confirms the launch. Defaults: **4 simultaneous connections**, **30 seconds per target**; settings **1–16** and **1–3600 seconds**. Only one batch is active at a time; `B` returns to its results. The command is a literal argument after the alias, passed to native SSH, independently of `ssh_command`; no local shell is involved. SSH is non-interactive (`BatchMode=yes`, closed input), with no TTY, multiplexing, inherited forwarding or detaching, and with `RemoteCommand` replaced. Access must already be usable without a prompt; no host key is accepted automatically.

The results distinguish waiting, running, success, failure, timeout and cancellation, with the exit code, stdout and stderr kept separate. Each stream keeps at most **256 KiB per target**, reports its truncation and continues to be drained without a temporary output file. `s` cancels the owned processes and prevents waiting targets from starting; quitting from the list with an active batch opens the exit summary; closing the results screen does not cancel it. Descendants remaining in the owned group are cleaned up even if the leader has already finished, before its PID is reaped. A detached remote command, or a local descendant that leaves this group, cannot be guaranteed to be stopped. The history sequentially receives the events of actual attempts and observed completions on the UI thread; targets cancelled before any attempt do not appear in it. Interactive SSH connections and the native copy prevalidation are temporarily refused while the batch is running, to keep the supervision responsive. Marks, settings and outputs belong to the session; only the history is persisted.

**Key assistant (native OpenSSH tools)**: `K` from the table or F2 → **OpenSSH keys** inventories the public files of `~/.ssh` (without recursion) and the declared identities of the hosts. `F9` opens the selector from a form: Enter only replaces its IdentityFile, Esc keeps all fields, cursors and parents. The inventory shows the path, the `ssh-keygen -lf` fingerprint of the public key and the declared aliases that use it. `~/` references are resolved; `%`, variables, `~user` and relative paths are reported without inventing a match. This is not a complete resolution of what OpenSSH effectively uses. Private keys without a public key, known by reference or by an `id_*` name, get a diagnostic; no private content is read by the application.

Even at 28×8, the selection and the active field remain visible. `F4` opens the full details of the key and of the last error; ↑/↓, PageUp/PageDown and Home/End scroll through the path, the fingerprint, the usages, the diagnostic and the recovery instructions. Esc or F4 returns to the same draft. A refresh keeps the selection by path, including in a suspended selector; if the key disappears, no other key is selected before an explicit navigation. A deployment review keeps its public snapshot and its alias.

`n` opens Ed25519 generation: enter an absolute private path and a comment, then press Enter. `ssh-keygen` asks for its passphrase in the native terminal; no empty passphrase is imposed. The pair is created in a neighbouring private directory, then published by atomic creations without overwriting; a private or public name already taken, even one that appeared during generation, is refused. On Unix the private key stays in mode 0600. If the private key has been published but publishing the public key fails, the private path is left intact: no rollback deletes a key that may have been replaced by another process. The diagnostic names both paths and the cause; inspect them before retrying, recover the public key if needed with native `ssh-keygen -y`, or choose another destination. Only the temporary staging is cleaned up after an error. `a` calls `ssh-add` for the agent already indicated by `SSH_AUTH_SOCK`; no persistent agent is started.

With an exact alias selected before opening Keys, `t` prepares the transactional attachment diff: the new identity becomes the first one and the old ones remain present; the alias, other fields and tags are kept. Enter in the diff confirms. `d` shows the public key, its fingerprint, the alias and the SSH root; Enter runs `ssh-copy-id -i PUBLIC -F ROOT ALIAS`. A public key modified since the review is refused. The reread public key is frozen in a safe temporary path with a link to its private key, to preserve the native duplicate check without exposing the original names to the script's internal shell evaluation. The matching private key is required; no forced mode or automatic host key acceptance is added. Passwords and passphrases stay in the native dialogs, without capture. A missing tool is reported without automatic installation. Errors keep the draft; the terminal is restored after the tool.

The inventory uses a single worker, at most 256 candidates / 1024 directory entries, a budget of 8 s between keys and a timeout of 2 s per fingerprint (so at most about 10 s). F5 during a scan merges the requests into a single following refresh. Successfully generated destinations, even outside `~/.ssh`, remain candidates for the session and are selected after the scan; the 256 most recent ones have priority within the same overall limit of 256 candidates. No additional generated path is saved to disk by the application. Public keys must be regular files, not symlinked on Unix, with a single public key per file and 64 KiB at most; fingerprint captures are bounded to 64 KiB per stream on Unix. Generation, adding to the agent and deployment are refused during an active batch to keep its timeouts and its cancellation available. Publishing two files is not a transaction that survives a power cut. Safe deployment requires Unix and `/tmp`.

**Active tunnels (Unix)**: Ctrl+T in the form chooses "Shell session" (usual terminal behaviour) or "Tunnel only". Ctrl+L explicitly starts the tunnel alone in the background and opens F7, without saving the draft. SSH receives `-N -T -o ExitOnForwardFailure=yes -o BatchMode=yes`, a closed input and a captured stderr of which only the last 32 KiB are kept. Keys/known hosts and authentication methods remain those of OpenSSH; authentication requires a key/agent already usable without a prompt. RemoteCommand, RequestTTY, forking and multiplexing are neutralised so that the process and its descendants belong to this session. No automatic host key acceptance is added.

F7 shows the host, the profile and the ports at launch time, the canonical configuration path, the state and the exit cause. "Active" only means that the process is alive, with no guarantee of the forwarding's availability. `s` stops only the selected child and its descendants; finished entries remain viewable. Renaming/deleting a profile or reloading with F5 does not change the identity of the owned processes. Quitting from the list with active tunnels opens a summary: **Go back** has the focus, **Stop and quit** must be chosen with Tab then Enter; returning from F7 stops no tunnel. Normal exit and errors reap only the owned children; no external PID or history is used to stop an SSH. The history keeps the actual attempts and results, and a write error displays a warning without preventing the tunnel.

**Preview and restore.** Every add, edit, copy, deletion and move in the TUI prepares a per-file diff before any SSH write or backup. The access paths and physical destinations, the expected permissions and the `+`/`-` lines are displayed; a move shows both of its files. `␍` represents CR and `→` a tab in the diff. Enter confirms, Esc returns to the form or selector with its input and cursor; personal notes, favorites and profiles are saved directly to their JSON files. Non-interactive CLI commands keep their immediate behaviour and use the same transformations.

The confirmation checks the content, file identity, permissions and access path again. An external modification refuses the operation and keeps the preview; go back with Esc then prepare a new preview (F5 from the list if necessary). A copy also checks the Include graph and the assumptions of the native `ssh -G` prevalidation. No SSH file or directory is created during preparation; the copy mirror is private and temporary.

**F8 / Backups** lists first the versions with an explicitly recorded destination, then the old `.backup` copies without provenance. The latter remain viewable in the data directory but their automatic restore is disabled: no path is deduced from the name. A private `backups/version-*.json` version contains the bytes, the canonical path, the original access path, the date and the original mode. The restore preview validates the recorded destination and the backup file itself; confirming first backs up the current content, then replaces it atomically. The current permissions are kept; a missing destination is recreated with the recorded mode. Files with the same name in different directories keep their versions separate. The legacy "last backup" copies are maintained for compatibility.

Return to the list before F8 when a form, selector or preview is open, even under F7: a restore must not discard a suspended draft. F7 can cover a form or a preview and returns without modifying it. Closing F7 (`q`, Esc or Ctrl+C) fully restores the suspended draft or preview: you no longer quit the application from F7. Backups are neither encrypted nor purged automatically; they are private to the local account and their provenance relies on this directory, not on a cryptographic signature. A restore is per file, not an atomic rollback of a whole Include graph; it does not restore the associated notes, favorites or JSON profiles. The move uses a protected rollback; if an external intervention prevents it, the error keeps the backups and asks for a manual recovery.

**F5 / Reload** rereads the root SSH file and its Includes after an external modification. The search (and its cursor), the sort, the visibility of hidden hosts and the selection by name and source file are kept; scrolling stays bounded to the new list. The open sheet is refreshed if the host still exists, otherwise the list reappears. Stale diagnostics are cleared and the effective configuration is recomputed, even if the startup ping is disabled; reloading does not start connectivity probes. If a file can no longer be read, the last list and its diagnostics remain available with the cause displayed. An Include with no match, literal or with a glob, remains optional; the disappearance of the root file and traversal or read errors are reported. F5 also reloads notes, favorites and tunnel profiles. If the root path is a symbolic link whose target has changed, these data are reassociated together with the new canonical root; all reads are prepared before the new state is published. A failure leaves the previous list and data displayed. F5 does not leave forms or confirmations in progress.

The **Details** panel is disabled at launch and opens on the right when the terminal reaches **110 columns**. It follows the selected host without reducing the number of rows of the table; the table columns adapt to the remaining space. Below 110 columns, the list takes the full width again and a reminder shows **Tab, then i** from the search, or **i** from the table, to open the sheet. Enlarging the terminal shows the panel again if its activation has been kept. The Alt+↑/↓ (line by line) and Ctrl+PageUp/PageDown (page by page) shortcuts scroll only when the panel is visible, without changing the searched text or the selected host. Alt+↑/↓ remains available if the terminal reserves Ctrl+PageUp/PageDown for its tabs.

The details show the effective values resolved by `ssh -G`: user, port, all identities, ProxyJump, ProxyCommand and other directives, with the source file and line of the declaration and the available connectivity diagnostic. During resolution or on error, the declared values are shown with that state explicitly indicated. The form always edits the declaration. Long values are wrapped and remain fully accessible by scrolling, including in the **i** sheet.

Favorites carry a `*` marker and come before the sort by name or by last connection, without losing the selection when pinning. The **Favorites only** filter combines with the search and the visibility of hidden hosts. Notes appear in **Details** and in the host sheet (`i`). A save error keeps the draft open; a pinning failure keeps the previous state.

Favorites and notes are saved only in `~/.config/ssho/metadata.json` (or the XDG/APPDATA application directory), with atomic publication and private permissions on creation. Each entry is indexed by the canonical path of the root SSH file then by the alias name: two configurations remain independent, and existing symbolic paths leading to the same root share their data. A root that does not exist yet uses its absolute path with the canonical parent, stable after creation. These actions modify neither any SSH file nor the `sshm/` folder; unknown JSON fields are preserved.

When renaming in the form, kept aliases keep their data; removed and added aliases are paired in their declaration order. Removing only an alias transfers nothing to a kept alias. A destination that already has metadata blocks the transfer to preserve both notes. As the SSH save has already succeeded, the interface displays the saved host and a separate warning; the old metadata remain available for recovery. Standalone CLI forms end with an error if this transfer fails after the SSH save.

## Advanced search in the interface

The TUI search accepts up to 1,024 characters. Terms are combined with **AND**, implicitly with spaces or explicitly with `AND` / `ET` (case-insensitive): `tag:prod user:deploy`, `tag:prod AND user:deploy` and `tag:prod ET user:deploy` are equivalent. A conjunction on its own, at the start, at the end or repeated (`alpha AND ET beta`) produces an error asking for a term on each side. The words `"AND"` and `"ET"` in quotes and values such as `tag:and` remain literal text. No OR/OU syntax, negation or parentheses are interpreted.

- A plain term searches for a substring in the name, the address, the user, the tags or the personal note. Each term can match a different field.
- `tag:text`, `user:text` and `file:text` restrict this search to the tag, the user or the path of the declared source file. `file:` does not search the content of files.
- `is:online`, `is:offline`, `is:unknown`, `is:connecting`, `is:favorite` filter on the states and the local favorite. Several `is:` are also combined with AND; incompatible states therefore give no result.
- Double quotes group an exact phrase, for example `"release café"`, `tag:"équipe infra"` or `file:"Team Hosts"`. They can surround a whole term; single quotes and backslashes are literal (no escaping). An unknown prefix, such as `ticket:42`, remains a literal term.
- Case is ignored through Unicode lowercase conversion, without removing accents or normalising Unicode forms. Fields and states are also case-insensitive. An empty query shows all the hosts allowed by the other filters.
- An empty value (`tag:`, `""`), an unclosed quote or an unknown state (`is:maybe`) displays a search error and no result. Correcting the input immediately restores the list; operation errors are kept separately.

The address and the user come from the `ssh -G` cache as soon as it is resolved, falling back on the declared values while waiting or after an error. Results update as resolutions and probes arrive. `Unknown` is the initial state or that of a host that has not been probed; `Connecting` indicates a probe in progress. The title always shows the state of the selected host in plain text, including after returning from SSH and in a compact terminal. The kept selection is identified by name and file; an arrival that does not change the members of the list does not rerun its sort.

From the table, `c` opens the known SSH files, including empty files, with an "All files" entry; `C` removes this filter. The same actions appear in F2, also from a search with no result. The exact selected path appears in the "Filters" banner ("File: …" line) and in the help. This filter combines with the query, the favorites and `H`: even a file with no result keeps its filter after a search, an `H` toggle or F5. Only a deletion or a move of the last host of the file removes it automatically, also counting hidden hosts and those outside the search. The query, its cursor and the focus are kept while choosing the file. Two files with the same name remain distinct by their full path. While typing, `c` and `C` remain text.

Examples: `tag:prod AND user:deploy is:online`, `"maintenance prévue" ET is:favorite`, `file:"Team Hosts" is:unknown`. The hidden hosts filter and "Favorites only" apply on top of the query. Matches in the table, the details panel and the sheet are underlined and highlighted without splitting Unicode characters; the state/favorite markers remain distinct. The query stays in place after SSH and F5. After an add, edit or move in the active file (or with no file filter), an expression compatible with the saved host is kept; an incompatible expression is cleared to show that host. The previously resolved values are only used for a provisional check. Even an explicit change of user/address waits for the native result before clearing a search compatible with those values: a global directive or an earlier `Host *` block may keep priority. This provisional context is kept across several saves before the result is received, without being published to the resolved cache. A save outside the filtered file keeps that filter and stays hidden. In the active file, the saved host stays visible during the new resolution; its fresh result confirms the search or clears it if the inheritance has changed (notably after a move). A resolution error settles the matter with the declared values, and any new search input cancels this wait.

This syntax only concerns the interactive interface. The `ssho search` CLI command keeps its legacy search and its output formats.

Bracketed paste is enabled while the interface is running and disabled for SSH and on exit. In the search and simple fields, line breaks and tabs become spaces; in notes, line breaks are kept (CRLF/CR normalised to LF). Other control characters are ignored. Pasting only modifies the active field, without running a shortcut or submitting a form; in the table, the palettes and the confirmations, it is ignored. This behaviour requires a terminal that sends bracketed paste sequences.

## Completion

Completion must be loaded into the shell. With the locally
built binary, from the project root:

```bash
source <(./target/release/ssho completion bash)
./target/release/ssho BS<Tab>
```

To load it in every new Bash terminal, add the following line to `~/.bashrc`,
replacing the path with the absolute path of the binary:

```bash
source <(/absolute/path/to/ssho/target/release/ssho completion bash)
```

Once `ssho` is installed in the `PATH`, use:

```sh
# Bash
source <(ssho completion bash)

# Zsh
source <(ssho completion zsh)

# Fish
ssho completion fish | source
```

```powershell
# PowerShell
ssho completion powershell | Out-String | Invoke-Expression
```

The completion script is registered for the name `ssho`. Bash applies the same completion to a launch by path (`./target/release/ssho`), because it falls back on the last component of the path: this is verified in a real interactive Bash by `tests/pty_completion.py`, for the installed name as well as for a path, for hosts and subcommands. The tests of the Zsh, Fish and PowerShell scripts cover their content. Requesting a completion creates no application folder.
