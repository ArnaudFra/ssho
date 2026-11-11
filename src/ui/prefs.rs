//! Preferences screen: every application option with its current value,
//! its effect and the moment it applies. The screen edits a draft; nothing is
//! written before Apply, and only the options shown here are written.
use super::{
    actions::{self, Keymap, PaletteAction},
    columns, key_name, nav, Action, Model, View,
};
use crate::i18n::Language;
use crate::storage::{AppConfig, AvailabilityMethod, GroupMode, KeyBindings, DEFAULT_PREFIX_KEY};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Language,
    Colors,
    Ascii,
    FocusSearch,
    GroupMode,
    GroupAliases,
    Columns,
    Availability,
    PingOnStart,
    Updates,
    SshCommand,
    Prefix,
    EscQuits,
    Shortcuts,
    DataFolder,
    DataPreferences,
    DataMetadata,
    DataHistory,
    DataProfiles,
    DataViews,
    DataCommands,
    DataBackups,
    DataImportBackups,
    DataSshConfig,
    ToolSsh,
    ToolSftp,
    ToolKeygen,
    ToolCopyId,
    ToolAdd,
    ToolPing,
}

/// Sections in display order; the rows keep a single Up/Down order across them.
pub(super) fn sections() -> [(&'static str, &'static [Row]); 6] {
    [
        (
            crate::t!("Appearance", "Apparence"),
            &[Row::Language, Row::Colors, Row::Ascii],
        ),
        (
            crate::t!("Host list", "Liste des hôtes"),
            &[
                Row::FocusSearch,
                Row::GroupMode,
                Row::GroupAliases,
                Row::Columns,
            ],
        ),
        (
            crate::t!("Connection and diagnostics", "Connexion et diagnostics"),
            &[
                Row::Availability,
                Row::PingOnStart,
                Row::Updates,
                Row::SshCommand,
            ],
        ),
        (
            crate::t!("Keyboard", "Clavier"),
            &[Row::Prefix, Row::EscQuits, Row::Shortcuts],
        ),
        (
            crate::t!("Data (read-only)", "Données (lecture seule)"),
            &[
                Row::DataFolder,
                Row::DataPreferences,
                Row::DataMetadata,
                Row::DataHistory,
                Row::DataProfiles,
                Row::DataViews,
                Row::DataCommands,
                Row::DataBackups,
                Row::DataImportBackups,
                Row::DataSshConfig,
            ],
        ),
        (
            crate::t!("Native tools (read-only)", "Outils natifs (lecture seule)"),
            &[
                Row::ToolSsh,
                Row::ToolSftp,
                Row::ToolKeygen,
                Row::ToolCopyId,
                Row::ToolAdd,
                Row::ToolPing,
            ],
        ),
    ]
}

pub(super) fn rows() -> Vec<Row> {
    sections()
        .iter()
        .flat_map(|(_, rows)| rows.iter().copied())
        .collect()
}

pub(super) fn buttons() -> [&'static str; 3] {
    [
        crate::t!("Apply", "Appliquer"),
        crate::t!("Cancel", "Annuler"),
        crate::t!("Restore defaults", "Valeurs par défaut"),
    ]
}

fn on_off(value: bool) -> &'static str {
    if value {
        crate::t!("on", "activé")
    } else {
        crate::t!("off", "désactivé")
    }
}

impl Row {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Language => crate::t!("Language", "Langue"),
            Self::Colors => crate::t!("Colours", "Couleurs"),
            Self::Ascii => crate::t!("ASCII symbols", "Symboles ASCII"),
            Self::FocusSearch => crate::t!("Search focused at start", "Recherche au démarrage"),
            Self::GroupMode => crate::t!("Groups", "Groupes"),
            Self::GroupAliases => crate::t!("Grouped aliases", "Alias regroupés"),
            Self::Columns => crate::t!("Columns", "Colonnes"),
            Self::Availability => crate::t!("Availability method", "Méthode de disponibilité"),
            Self::PingOnStart => crate::t!("Probes at start", "Sondes au démarrage"),
            Self::Updates => crate::t!("Update notification", "Avis de mise à jour"),
            Self::SshCommand => crate::t!("SSH launcher", "Lanceur SSH"),
            Self::Prefix => crate::t!("Quick actions key", "Touche des actions rapides"),
            Self::EscQuits => crate::t!("Esc quits from the list", "Échap quitte à la liste"),
            Self::Shortcuts => crate::t!("Shortcuts", "Raccourcis"),
            Self::DataFolder => crate::t!("Application folder", "Dossier de l'application"),
            Self::DataPreferences => crate::t!("Preferences", "Préférences"),
            Self::DataMetadata => crate::t!("Favorites and notes", "Favoris et notes"),
            Self::DataHistory => crate::t!("Connection history", "Historique des connexions"),
            Self::DataProfiles => crate::t!("Tunnel profiles", "Profils de tunnels"),
            Self::DataViews => crate::t!("Saved views", "Vues enregistrées"),
            Self::DataCommands => crate::t!("Saved commands", "Commandes enregistrées"),
            Self::DataBackups => crate::t!("Backups", "Sauvegardes"),
            Self::DataImportBackups => crate::t!("Copies before import", "Copies avant import"),
            Self::DataSshConfig => crate::t!("SSH configuration", "Configuration SSH"),
            Self::ToolSsh => "ssh",
            Self::ToolSftp => "sftp",
            Self::ToolKeygen => "ssh-keygen",
            Self::ToolCopyId => "ssh-copy-id",
            Self::ToolAdd => "ssh-add",
            Self::ToolPing => "ping",
        }
    }

    /// Whether Space, Enter or the arrows do something on this row.
    pub(super) fn editable(self) -> bool {
        !matches!(
            self,
            Self::SshCommand
                | Self::DataFolder
                | Self::DataPreferences
                | Self::DataMetadata
                | Self::DataHistory
                | Self::DataProfiles
                | Self::DataViews
                | Self::DataCommands
                | Self::DataBackups
                | Self::DataImportBackups
                | Self::DataSshConfig
                | Self::ToolSsh
                | Self::ToolSftp
                | Self::ToolKeygen
                | Self::ToolCopyId
                | Self::ToolAdd
                | Self::ToolPing
        )
    }

    pub(super) fn value(self, m: &Model, editor: &Editor) -> String {
        let draft = &editor.draft;
        let data = |name: &str| {
            m.batch.history_root.as_ref().map_or_else(
                || crate::t!("unknown", "inconnu").to_owned(),
                |root| root.join(name).display().to_string(),
            )
        };
        match self {
            Self::Language => draft.language.name().to_owned(),
            Self::Colors => {
                if m.no_color {
                    crate::t!("off (NO_COLOR is set)", "désactivé (NO_COLOR est défini)").to_owned()
                } else {
                    on_off(draft.colors).to_owned()
                }
            }
            Self::Ascii => on_off(draft.ascii).to_owned(),
            Self::FocusSearch => on_off(draft.focus_search_on_start).to_owned(),
            Self::GroupMode => super::groups::modes()
                .iter()
                .find(|(mode, _)| *mode == draft.group_mode)
                .map_or(String::new(), |(_, label)| (*label).to_owned()),
            Self::GroupAliases => on_off(draft.group_aliases).to_owned(),
            // The columns have their own editor, which saves on its own.
            Self::Columns => m
                .preferences
                .host_columns
                .iter()
                .map(|column| column.label())
                .collect::<Vec<_>>()
                .join(", "),
            Self::Availability => draft.availability_method.label().to_owned(),
            Self::PingOnStart => on_off(draft.ping_on_start).to_owned(),
            Self::Updates => on_off(draft.check_for_updates).to_owned(),
            Self::SshCommand => draft.ssh_command.join(" "),
            Self::Prefix => {
                let keymap = Keymap::new(&draft.key_bindings);
                keymap.prefix().unwrap_or_else(|| on_off(false).to_owned())
            }
            Self::EscQuits => on_off(esc_quits(&draft.key_bindings)).to_owned(),
            Self::Shortcuts => {
                let custom = draft.key_bindings.shortcuts.len();
                let refused = Keymap::new(&draft.key_bindings).conflicts.len();
                if refused > 0 {
                    crate::tf!(
                        "{custom} customised, {refused} refused",
                        "{custom} personnalisé(s), {refused} refusé(s)"
                    )
                } else {
                    crate::tf!("{custom} customised", "{custom} personnalisé(s)")
                }
            }
            Self::DataFolder => m.batch.history_root.as_ref().map_or_else(
                || crate::t!("unknown", "inconnu").to_owned(),
                |root| root.display().to_string(),
            ),
            Self::DataPreferences => data("config.json"),
            Self::DataMetadata => data("metadata.json"),
            Self::DataHistory => data("sshm_history.json"),
            Self::DataProfiles => data("tunnel_profiles.json"),
            Self::DataViews => data("views.json"),
            Self::DataCommands => data("commands.json"),
            Self::DataBackups => data("backups"),
            Self::DataImportBackups => data("data-backups"),
            Self::DataSshConfig => m.batch.root.display().to_string(),
            Self::ToolSsh
            | Self::ToolSftp
            | Self::ToolKeygen
            | Self::ToolCopyId
            | Self::ToolAdd
            | Self::ToolPing => match editor.tools.iter().find(|(row, _)| *row == self) {
                Some((_, Some(path))) => path.display().to_string(),
                _ => crate::t!("not found in PATH", "introuvable dans le PATH").to_owned(),
            },
        }
    }

    /// What the option does, then when a change takes effect.
    pub(super) fn detail(self) -> [&'static str; 2] {
        let now = crate::t!(
            "Takes effect as soon as it is applied.",
            "Prend effet dès l'application."
        );
        let next = crate::t!(
            "Takes effect at the next start.",
            "Prend effet au prochain démarrage."
        );
        let read_only = crate::t!(
            "Shown for reference: this screen never writes it.",
            "Affiché pour information : cet écran ne l'écrit jamais."
        );
        match self {
            Self::Language => [
                crate::t!(
                    "Language of every screen: English or French.",
                    "Langue de tous les écrans : anglais ou français."
                ),
                now,
            ],
            Self::Colors => [
                crate::t!(
                    "Off: no colour at all; states keep their text, sign or emphasis. NO_COLOR in the environment wins.",
                    "Désactivé : aucune couleur ; les états gardent leur texte, signe ou mise en valeur. La variable NO_COLOR passe avant."
                ),
                now,
            ],
            Self::Ascii => [
                crate::t!(
                    "On: frames, arrows and signs are drawn with ASCII characters, for fonts without the symbols.",
                    "Activé : cadres, flèches et signes en caractères ASCII, pour les polices sans ces symboles."
                ),
                now,
            ],
            Self::FocusSearch => [
                crate::t!(
                    "On: the application opens in the search input. Off: it opens on the host table.",
                    "Activé : l'application s'ouvre dans la recherche. Désactivé : elle s'ouvre sur le tableau des hôtes."
                ),
                next,
            ],
            Self::GroupMode => [
                crate::t!(
                    "Flat list, one group per configuration file, or per file then main tag.",
                    "Liste plate, un groupe par fichier de configuration, ou par fichier puis tag principal."
                ),
                now,
            ],
            Self::GroupAliases => [
                crate::t!(
                    "On: the aliases of one SSH block share a single row; F3 chooses the connection alias.",
                    "Activé : les alias d'un même bloc SSH partagent une ligne ; F3 choisit l'alias de connexion."
                ),
                now,
            ],
            Self::Columns => [
                crate::t!(
                    "Order and visibility of the table columns. Enter opens their editor, which saves on its own.",
                    "Ordre et visibilité des colonnes du tableau. Entrée ouvre leur éditeur, qui enregistre de lui-même."
                ),
                now,
            ],
            Self::Availability => [
                crate::t!(
                    "SSH: handshake with the server, through its proxy if any. ICMP local: ping from this machine only.",
                    "SSH : poignée de main avec le serveur, via son proxy s'il y en a un. ICMP local : ping depuis ce poste seulement."
                ),
                now,
            ],
            Self::PingOnStart => [
                crate::t!(
                    "On: every host is probed when the application starts. Off: only on request (Test availability).",
                    "Activé : chaque hôte est sondé au démarrage. Désactivé : seulement à la demande (Tester la disponibilité)."
                ),
                next,
            ],
            Self::Updates => [
                crate::t!(
                    "On: a newer published version is announced in the header. Nothing is ever installed.",
                    "Activé : une version publiée plus récente est annoncée dans l'en-tête. Rien n'est jamais installé."
                ),
                next,
            ],
            Self::SshCommand => [
                crate::t!(
                    "Literal argument list (\"ssh_command\" in config.json, no shell). It only launches ordinary connections: tunnels, batches, ssh -G and probes always use ssh.",
                    "Liste d'arguments littéraux (« ssh_command » de config.json, sans shell). Elle ne lance que les connexions ordinaires : tunnels, lots, ssh -G et sondes utilisent toujours ssh."
                ),
                crate::t!(
                    "Edited in config.json only; read at the next start.",
                    "Modifiable dans config.json seulement ; lu au prochain démarrage."
                ),
            ],
            Self::Prefix => [
                crate::t!(
                    "Key that opens the quick actions panel, then one letter runs an action. Off: F2 keeps every action.",
                    "Touche qui ouvre le panneau des actions rapides, puis une lettre lance une action. Désactivé : F2 garde toutes les actions."
                ),
                now,
            ],
            Self::EscQuits => [
                crate::t!(
                    "Off (default): Esc only goes back one level and never closes the application. On: Esc also quits from the list.",
                    "Désactivé (défaut) : Échap revient seulement d'un niveau et ne ferme jamais l'application. Activé : Échap quitte aussi depuis la liste."
                ),
                now,
            ],
            Self::Shortcuts => [
                crate::t!(
                    "Direct keys of the list actions. Enter opens the editor; conflicts are refused and explained.",
                    "Touches directes des actions de la liste. Entrée ouvre l'éditeur ; les conflits sont refusés et expliqués."
                ),
                now,
            ],
            Self::DataFolder => [
                crate::t!(
                    "Everything ssho owns is in this folder; the SSH files are never stored there.",
                    "Tout ce que ssho possède est dans ce dossier ; les fichiers SSH n'y sont jamais rangés."
                ),
                read_only,
            ],
            Self::DataPreferences => [
                crate::t!(
                    "The options of this screen, plus the SSH launcher.",
                    "Les options de cet écran, plus le lanceur SSH."
                ),
                read_only,
            ],
            Self::DataMetadata => [
                crate::t!(
                    "Favorites and personal notes, per SSH configuration file. Never written to the SSH files.",
                    "Favoris et notes personnelles, par fichier de configuration SSH. Jamais écrits dans les fichiers SSH."
                ),
                read_only,
            ],
            Self::DataHistory => [
                crate::t!(
                    "Last connection attempts and results, used by the recent sort and the host sheet.",
                    "Dernières tentatives de connexion et résultats, utilisés par le tri récent et la fiche d'hôte."
                ),
                read_only,
            ],
            Self::DataProfiles => [
                crate::t!(
                    "Saved tunnel profiles, per host.",
                    "Profils de tunnels enregistrés, par hôte."
                ),
                read_only,
            ],
            Self::DataViews => [
                crate::t!(
                    "Named list contexts (search, filters, sort, groups, columns), per SSH configuration file.",
                    "Contextes de liste nommés (recherche, filtres, tri, groupes, colonnes), par fichier de configuration SSH."
                ),
                read_only,
            ],
            Self::DataCommands => [
                crate::t!(
                    "Personal command catalogue, shared by every SSH configuration. Stored in clear text: never a secret.",
                    "Catalogue personnel de commandes, commun à toutes les configurations SSH. Enregistré en clair : jamais de secret."
                ),
                read_only,
            ],
            Self::DataBackups => [
                crate::t!(
                    "Copies of the SSH files taken before each write; F8 lists and restores them.",
                    "Copies des fichiers SSH prises avant chaque écriture ; F8 les liste et les restaure."
                ),
                read_only,
            ],
            Self::DataImportBackups => [
                crate::t!(
                    "Application files as they were before each import (F2: Export / import data), one dated folder per import.",
                    "Fichiers de l'application tels qu'ils étaient avant chaque import (F2 : Exporter / importer les données), un dossier daté par import."
                ),
                read_only,
            ],
            Self::ToolSsh => [
                crate::t!(
                    "Required: connections, tunnels, batches, effective configuration (ssh -G) and proxy checks. Missing: install the OpenSSH client.",
                    "Indispensable : connexions, tunnels, lots, configuration effective (ssh -G) et contrôles via proxy. Absent : installez le client OpenSSH."
                ),
                read_only,
            ],
            Self::ToolSftp => [
                crate::t!(
                    "Used by \"SFTP: open a session\". Missing: only that action fails, with the reason; it comes with the OpenSSH client.",
                    "Utilisé par « SFTP : ouvrir une session ». Absent : seule cette action échoue, avec la raison ; il est fourni avec le client OpenSSH."
                ),
                read_only,
            ],
            Self::ToolKeygen => [
                crate::t!(
                    "Used by the key screen: fingerprints and key generation. Missing: keys are listed without fingerprint and cannot be created here.",
                    "Utilisé par l'écran des clés : empreintes et génération. Absent : les clés sont listées sans empreinte et ne peuvent pas être créées ici."
                ),
                read_only,
            ],
            Self::ToolCopyId => [
                crate::t!(
                    "Used by \"Deploy public key\". Missing: only that action fails; it is a separate package on some systems.",
                    "Utilisé par « Déployer la clé publique ». Absent : seule cette action échoue ; c'est un paquet séparé sur certains systèmes."
                ),
                read_only,
            ],
            Self::ToolAdd => [
                crate::t!(
                    "Used by \"Add to agent\". Missing, or no agent running: only that action fails, with the message of the tool.",
                    "Utilisé par « Ajouter à l'agent ». Absent, ou sans agent lancé : seule cette action échoue, avec le message de l'outil."
                ),
                read_only,
            ],
            Self::ToolPing => [
                crate::t!(
                    "Used by the \"ICMP local\" availability method only. Missing or not permitted: the state is \"unknown\"; the SSH method does not need it.",
                    "Utilisé par la méthode de disponibilité « ICMP local » seulement. Absent ou non autorisé : l'état est « inconnu » ; la méthode SSH n'en a pas besoin."
                ),
                read_only,
            ],
            Self::DataSshConfig => [
                crate::t!(
                    "The OpenSSH configuration read by this session (option -c to choose another one).",
                    "La configuration OpenSSH lue par cette session (option -c pour en choisir une autre)."
                ),
                read_only,
            ],
        }
    }
}

fn esc_quits(bindings: &KeyBindings) -> bool {
    bindings.should_quit_on_key("esc")
}

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub(super) draft: AppConfig,
    original: AppConfig,
    /// Index in `rows()`; the buttons take the focus when `button` is set.
    pub(super) row: usize,
    pub(super) button: Option<usize>,
    pub(super) scroll: usize,
    pub(super) notice: String,
    /// Prefix to give back when the quick actions key is switched on again.
    prefix: String,
    /// Native tools as found in PATH when the screen was opened; none is run.
    pub(super) tools: Vec<(Row, Option<std::path::PathBuf>)>,
}

impl Editor {
    pub(super) fn dirty(&self) -> bool {
        self.draft != self.original
    }
    pub(super) fn changed(&self, row: Row) -> bool {
        let (a, b) = (&self.draft, &self.original);
        match row {
            Row::Language => a.language != b.language,
            Row::Colors => a.colors != b.colors,
            Row::Ascii => a.ascii != b.ascii,
            Row::FocusSearch => a.focus_search_on_start != b.focus_search_on_start,
            Row::GroupMode => a.group_mode != b.group_mode,
            Row::GroupAliases => a.group_aliases != b.group_aliases,
            Row::Availability => a.availability_method != b.availability_method,
            Row::PingOnStart => a.ping_on_start != b.ping_on_start,
            Row::Updates => a.check_for_updates != b.check_for_updates,
            Row::Prefix => a.key_bindings.prefix_key != b.key_bindings.prefix_key,
            Row::EscQuits => esc_quits(&a.key_bindings) != esc_quits(&b.key_bindings),
            Row::Shortcuts => a.key_bindings.shortcuts != b.key_bindings.shortcuts,
            _ => false,
        }
    }
}

pub(super) fn open(m: &mut Model) {
    let prefix = if m.preferences.key_bindings.prefix_key.is_empty() {
        DEFAULT_PREFIX_KEY.to_owned()
    } else {
        m.preferences.key_bindings.prefix_key.clone()
    };
    m.open_view(View::Preferences(Box::new(Editor {
        draft: m.preferences.clone(),
        original: m.preferences.clone(),
        row: 0,
        button: None,
        scroll: 0,
        notice: String::new(),
        prefix,
        tools: [
            Row::ToolSsh,
            Row::ToolSftp,
            Row::ToolKeygen,
            Row::ToolCopyId,
            Row::ToolAdd,
            Row::ToolPing,
        ]
        .into_iter()
        .map(|row| (row, crate::runtime::find_tool(row.label())))
        .collect(),
    })));
    m.error.clear();
}

/// Copy the options of this screen into a freshly read configuration: an
/// option edited elsewhere meanwhile (columns, launcher) is never overwritten.
pub(super) fn merge(config: &mut AppConfig, draft: &AppConfig) {
    config.language = draft.language;
    config.colors = draft.colors;
    config.ascii = draft.ascii;
    config.focus_search_on_start = draft.focus_search_on_start;
    config.group_mode = draft.group_mode;
    config.group_aliases = draft.group_aliases;
    config.availability_method = draft.availability_method;
    config.ping_on_start = draft.ping_on_start;
    config.check_for_updates = draft.check_for_updates;
    config.key_bindings = draft.key_bindings.clone();
}

fn change(editor: &mut Editor, row: Row, forward: bool) {
    let draft = &mut editor.draft;
    match row {
        Row::Language => {
            let all = Language::ALL;
            let at = all.iter().position(|l| *l == draft.language).unwrap_or(0);
            draft.language = all[nav::wrap(at, all.len(), if forward { 1 } else { -1 })];
        }
        Row::Colors => draft.colors = !draft.colors,
        Row::Ascii => draft.ascii = !draft.ascii,
        Row::FocusSearch => draft.focus_search_on_start = !draft.focus_search_on_start,
        Row::GroupMode => {
            let all = [GroupMode::Flat, GroupMode::Files, GroupMode::FileTags];
            let at = all.iter().position(|g| *g == draft.group_mode).unwrap_or(0);
            draft.group_mode = all[nav::wrap(at, all.len(), if forward { 1 } else { -1 })];
        }
        Row::GroupAliases => draft.group_aliases = !draft.group_aliases,
        Row::Availability => {
            draft.availability_method = match draft.availability_method {
                AvailabilityMethod::Ssh => AvailabilityMethod::Icmp,
                AvailabilityMethod::Icmp => AvailabilityMethod::Ssh,
            };
        }
        Row::PingOnStart => draft.ping_on_start = !draft.ping_on_start,
        Row::Updates => draft.check_for_updates = !draft.check_for_updates,
        Row::Prefix => {
            let bindings = &mut draft.key_bindings;
            if bindings.prefix_key.is_empty() {
                bindings.prefix_key.clone_from(&editor.prefix);
            } else {
                bindings.prefix_key.clear();
            }
        }
        Row::EscQuits => {
            let bindings = &mut draft.key_bindings;
            if esc_quits(bindings) {
                bindings.quit_keys.retain(|key| key != "esc");
            } else {
                bindings.disable_esc_quit = false;
                bindings.quit_keys.push("esc".into());
            }
        }
        _ => {}
    }
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Preferences(editor) = &mut m.view else {
        return Action::None;
    };
    editor.notice.clear();
    let rows = rows();
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if nav::cancels(k) {
        return cancel(m);
    }
    if ctrl && k.code == KeyCode::Char('s') {
        return Action::Preferences(Box::new(editor.draft.clone()));
    }
    let count = buttons().len();
    match (k.code, editor.button) {
        (KeyCode::Tab, None) => editor.button = Some(0),
        (KeyCode::Tab, Some(at)) => editor.button = (at + 1 < count).then_some(at + 1),
        (KeyCode::BackTab, None) => editor.button = Some(count - 1),
        (KeyCode::BackTab, Some(at)) => editor.button = at.checked_sub(1),
        (KeyCode::Up, None) => editor.row = editor.row.saturating_sub(1),
        (KeyCode::Down, None) if editor.row + 1 < rows.len() => editor.row += 1,
        // Past the last option come the buttons, and back.
        (KeyCode::Down, None) => editor.button = Some(0),
        (KeyCode::Up, Some(_)) => editor.button = None,
        (KeyCode::Home, None) => editor.row = 0,
        (KeyCode::End, None) => editor.row = rows.len() - 1,
        (KeyCode::PageUp, None) => editor.row = editor.row.saturating_sub(5),
        (KeyCode::PageDown, None) => editor.row = (editor.row + 5).min(rows.len() - 1),
        (KeyCode::Left, Some(at)) => editor.button = Some(nav::wrap(at, count, -1)),
        (KeyCode::Right, Some(at)) => editor.button = Some(nav::wrap(at, count, 1)),
        (KeyCode::Enter, Some(0)) => {
            return Action::Preferences(Box::new(editor.draft.clone()));
        }
        (KeyCode::Enter, Some(1)) => return cancel(m),
        (KeyCode::Enter, Some(_)) => {
            // Options without a row of their own (columns, launcher) are kept.
            let mut defaults = editor.draft.clone();
            merge(&mut defaults, &AppConfig::default());
            editor.draft = defaults;
            editor.button = Some(0);
            editor.notice = crate::t!(
                "Defaults put in this draft: Apply writes them, Cancel forgets them",
                "Valeurs par défaut placées dans ce brouillon : Appliquer les écrit, Annuler les oublie"
            )
            .into();
        }
        (KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right, None) => {
            let row = rows[editor.row];
            match row {
                Row::Shortcuts if k.code != KeyCode::Left => {
                    let bindings = editor.draft.key_bindings.clone();
                    m.open_view(View::Shortcuts(Box::new(Shortcuts {
                        original: bindings.clone(),
                        bindings,
                        selected: 0,
                        offset: 0,
                        button: None,
                        capture: false,
                        notice: String::new(),
                    })));
                }
                Row::Columns if k.code == KeyCode::Enter => columns::open(m),
                Row::Colors if m.no_color => {
                    editor.notice = crate::t!(
                        "NO_COLOR is set in the environment: colours stay off whatever this option says",
                        "NO_COLOR est défini dans l'environnement : les couleurs restent coupées quelle que soit cette option"
                    )
                    .into();
                    change(editor, row, true);
                }
                _ if row.editable() && !matches!(row, Row::Shortcuts | Row::Columns) => {
                    change(editor, row, k.code != KeyCode::Left);
                }
                Row::Columns => {
                    editor.notice = crate::t!(
                        "Enter opens the columns editor",
                        "Entrée ouvre l'éditeur de colonnes"
                    )
                    .into();
                }
                _ => {
                    editor.notice = crate::t!(
                        "Read-only: shown for reference",
                        "Lecture seule : affiché pour information"
                    )
                    .into();
                }
            }
        }
        _ => {}
    }
    Action::None
}

fn cancel(m: &mut Model) -> Action {
    let dirty = matches!(&m.view, View::Preferences(editor) if editor.dirty());
    if dirty {
        m.ask_discard();
    } else {
        m.back();
    }
    Action::None
}

/// Shortcut editor: a draft of `key_bindings.shortcuts`, handed back to the
/// preferences draft by Done. Every refusal reuses the reason of the keymap.
#[derive(Clone, Debug)]
pub(super) struct Shortcuts {
    pub(super) bindings: KeyBindings,
    original: KeyBindings,
    pub(super) selected: usize,
    pub(super) offset: usize,
    pub(super) button: Option<usize>,
    /// The next key press is the new shortcut of the selected action.
    pub(super) capture: bool,
    pub(super) notice: String,
}

/// Quit has its own preference (`quit_keys`), so it is not listed here.
pub(super) fn shortcut_actions() -> Vec<PaletteAction> {
    PaletteAction::CATALOGUE
        .into_iter()
        .filter(|action| *action != PaletteAction::Quit)
        .collect()
}

pub(super) fn shortcut_buttons() -> [&'static str; 4] {
    [
        crate::t!("Change key", "Changer la touche"),
        crate::t!("No key", "Aucune touche"),
        crate::t!("Default", "Défaut"),
        crate::t!("Done", "Terminer"),
    ]
}

impl Shortcuts {
    pub(super) fn dirty(&self) -> bool {
        self.bindings != self.original
    }
    /// Where the keys of an action come from, in words.
    pub(super) fn state(&self, action: PaletteAction) -> &'static str {
        match self.bindings.shortcuts.get(action.id()) {
            Some(keys) if keys.is_empty() => crate::t!("F2 only", "F2 seul"),
            Some(_) => crate::t!("customised", "personnalisé"),
            None => crate::t!("default", "défaut"),
        }
    }
    fn assign(&mut self, action: PaletteAction, name: &str) {
        let shown = actions::display_key(name);
        let mut candidate = self.bindings.clone();
        candidate
            .shortcuts
            .insert(action.id().to_owned(), vec![name.to_owned()]);
        let before = Keymap::new(&self.bindings).conflicts;
        let after = Keymap::new(&candidate);
        if let Some(reason) = after.conflicts.iter().find(|c| !before.contains(c)) {
            self.notice = crate::tf!("Refused: {reason}", "Refusé : {reason}");
        } else if !after.keys(action).contains(&shown) {
            // A quit key keeps its precedence over any other action.
            self.notice = crate::tf!(
                "Refused: {shown} is kept by another preference (quit keys)",
                "Refusé : {shown} est gardé par une autre préférence (touches de sortie)"
            );
        } else {
            self.bindings = candidate;
            self.notice = crate::tf!(
                "{shown} now runs \"{}\" in this draft",
                "{shown} lance maintenant « {} » dans ce brouillon",
                action.label()
            );
        }
    }
}

pub(super) fn shortcuts_key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Shortcuts(editor) = &mut m.view else {
        return Action::None;
    };
    let actions = shortcut_actions();
    let action = actions[editor.selected.min(actions.len() - 1)];
    if editor.capture {
        editor.capture = false;
        if nav::cancels(k) {
            editor.notice = crate::t!("Key unchanged", "Touche inchangée").into();
        } else {
            editor.assign(action, &key_name(k));
        }
        return Action::None;
    }
    editor.notice.clear();
    if nav::cancels(k) {
        if editor.dirty() {
            m.ask_discard();
        } else {
            m.back();
        }
        return Action::None;
    }
    let count = shortcut_buttons().len();
    let done = k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('s');
    match (k.code, editor.button) {
        _ if done => return finish(m),
        (KeyCode::Up, _) => {
            editor.button = None;
            editor.selected = editor.selected.saturating_sub(1);
        }
        (KeyCode::Down, _) => {
            editor.button = None;
            editor.selected = (editor.selected + 1).min(actions.len() - 1);
        }
        (KeyCode::PageUp, _) => editor.selected = editor.selected.saturating_sub(10),
        (KeyCode::PageDown, _) => editor.selected = (editor.selected + 10).min(actions.len() - 1),
        (KeyCode::Home, _) => editor.selected = 0,
        (KeyCode::End, _) => editor.selected = actions.len() - 1,
        (KeyCode::Tab | KeyCode::Right, None) => editor.button = Some(0),
        (KeyCode::Tab | KeyCode::Right, Some(at)) => {
            editor.button = Some(nav::wrap(at, count, 1));
        }
        (KeyCode::BackTab | KeyCode::Left, None) => editor.button = Some(count - 1),
        (KeyCode::BackTab | KeyCode::Left, Some(at)) => {
            editor.button = Some(nav::wrap(at, count, -1));
        }
        (KeyCode::Enter, None | Some(0)) => {
            editor.capture = true;
            editor.notice = crate::tf!(
                "Press the new key for \"{}\" — Esc: keep the current one",
                "Appuyez sur la nouvelle touche de « {} » — Échap : garder l'actuelle",
                action.label()
            );
        }
        (KeyCode::Enter, Some(1)) => {
            editor
                .bindings
                .shortcuts
                .insert(action.id().to_owned(), Vec::new());
            editor.notice = crate::tf!(
                "\"{}\" has no direct key in this draft: F2 still runs it",
                "« {} » n'a plus de touche directe dans ce brouillon : F2 la lance toujours",
                action.label()
            );
        }
        (KeyCode::Enter, Some(2)) => {
            editor.bindings.shortcuts.remove(action.id());
            editor.notice = crate::tf!(
                "\"{}\" is back to its default key in this draft",
                "« {} » retrouve sa touche par défaut dans ce brouillon",
                action.label()
            );
        }
        (KeyCode::Enter, Some(_)) => return finish(m),
        _ => {}
    }
    Action::None
}

/// Hand the edited shortcuts to the preferences draft; nothing is written yet.
fn finish(m: &mut Model) -> Action {
    let View::Shortcuts(editor) = &m.view else {
        return Action::None;
    };
    let bindings = editor.bindings.clone();
    m.back();
    if let View::Preferences(preferences) = &mut m.view {
        preferences.draft.key_bindings = bindings;
        preferences.notice = crate::t!(
            "Shortcuts kept in this draft: Apply writes them",
            "Raccourcis conservés dans ce brouillon : Appliquer les écrit"
        )
        .into();
    }
    Action::None
}
