//! Action catalogue: one stable identity, label, category and availability per
//! action. The palette, the list shortcuts, the footer and the help all read it,
//! so a shortcut is never spelled inside a label.
use crate::storage::KeyBindings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PaletteAction {
    Add,
    Edit,
    Tunnel,
    Test,
    Help,
    Details,
    Reload,
    Favorite,
    Note,
    FavoritesOnly,
    Duplicate,
    Profiles,
    Active,
    Backups,
    SourceFilter,
    ClearSourceFilter,
    ToggleAliases,
    ChooseAlias,
    GroupMode,
    BatchMark,
    BatchSelectVisible,
    BatchClear,
    BatchOpen,
    BatchResults,
    Keys,
    Availability,
    Columns,
    // Catalogue entries reached from the list; not yet offered by the palette.
    Connect,
    Info,
    Move,
    Delete,
    ToggleHidden,
    SortToggle,
    SortName,
    SortRecent,
    Search,
    Palette,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Connection,
    Host,
    Organization,
    Tools,
    Application,
}

impl Category {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connexion",
            Self::Host => "Hôte",
            Self::Organization => "Organisation",
            Self::Tools => "Outils",
            Self::Application => "Application",
        }
    }
}

impl PaletteAction {
    /// Actions listed by the F2 palette, in display order.
    pub(super) const ALL: [Self; 27] = [
        Self::Add,
        Self::Edit,
        Self::Tunnel,
        Self::Test,
        Self::Help,
        Self::Details,
        Self::Reload,
        Self::Favorite,
        Self::Note,
        Self::FavoritesOnly,
        Self::Duplicate,
        Self::Profiles,
        Self::Active,
        Self::Backups,
        Self::SourceFilter,
        Self::ClearSourceFilter,
        Self::ToggleAliases,
        Self::ChooseAlias,
        Self::GroupMode,
        Self::BatchMark,
        Self::BatchSelectVisible,
        Self::BatchClear,
        Self::BatchOpen,
        Self::BatchResults,
        Self::Keys,
        Self::Availability,
        Self::Columns,
    ];

    /// Every catalogue entry, grouped by category for help and documentation.
    pub(super) const CATALOGUE: [Self; 38] = [
        Self::Connect,
        Self::Tunnel,
        Self::Profiles,
        Self::Active,
        Self::Test,
        Self::Availability,
        Self::Add,
        Self::Edit,
        Self::Duplicate,
        Self::Move,
        Self::Delete,
        Self::Info,
        Self::Details,
        Self::Favorite,
        Self::Note,
        Self::ChooseAlias,
        Self::Search,
        Self::FavoritesOnly,
        Self::ToggleHidden,
        Self::SourceFilter,
        Self::ClearSourceFilter,
        Self::ToggleAliases,
        Self::GroupMode,
        Self::SortToggle,
        Self::SortName,
        Self::SortRecent,
        Self::Columns,
        Self::BatchMark,
        Self::BatchSelectVisible,
        Self::BatchClear,
        Self::BatchOpen,
        Self::BatchResults,
        Self::Keys,
        Self::Backups,
        Self::Reload,
        Self::Palette,
        Self::Help,
        Self::Quit,
    ];

    /// Stable identifier, independent of language and display order.
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Add => "host.add",
            Self::Edit => "host.edit",
            Self::Tunnel => "tunnel.form",
            Self::Test => "availability.test",
            Self::Help => "app.help",
            Self::Details => "host.details",
            Self::Reload => "app.reload",
            Self::Favorite => "host.favorite",
            Self::Note => "host.note",
            Self::FavoritesOnly => "filter.favorites",
            Self::Duplicate => "host.duplicate",
            Self::Profiles => "tunnel.profiles",
            Self::Active => "tunnel.active",
            Self::Backups => "app.backups",
            Self::SourceFilter => "filter.source",
            Self::ClearSourceFilter => "filter.source.clear",
            Self::ToggleAliases => "view.aliases",
            Self::ChooseAlias => "host.alias",
            Self::GroupMode => "view.groups",
            Self::BatchMark => "batch.mark",
            Self::BatchSelectVisible => "batch.select_visible",
            Self::BatchClear => "batch.clear",
            Self::BatchOpen => "batch.open",
            Self::BatchResults => "batch.results",
            Self::Keys => "keys.open",
            Self::Availability => "availability.method",
            Self::Columns => "view.columns",
            Self::Connect => "host.connect",
            Self::Info => "host.info",
            Self::Move => "host.move",
            Self::Delete => "host.delete",
            Self::ToggleHidden => "filter.hidden",
            Self::SortToggle => "sort.toggle",
            Self::SortName => "sort.name",
            Self::SortRecent => "sort.recent",
            Self::Search => "search.focus",
            Self::Palette => "app.palette",
            Self::Quit => "app.quit",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Columns => "Colonnes (ordre / visibilité)",
            Self::Availability => "Méthode de disponibilité (SSH / ICMP)",
            Self::Keys => "Clés OpenSSH (assistant)",
            Self::BatchMark => "Lot : marquer l'alias",
            Self::BatchSelectVisible => "Lot : sélectionner les résultats du filtre",
            Self::BatchClear => "Lot : effacer les marques",
            Self::BatchOpen => "Lot : préparer / suivre",
            Self::BatchResults => "Lot : derniers résultats",
            Self::GroupMode => "Groupes",
            Self::ToggleAliases => "Alias regroupés",
            Self::ChooseAlias => "Choisir un alias",
            Self::SourceFilter => "Fichier source",
            Self::ClearSourceFilter => "Tous les fichiers",
            Self::Add => "Ajouter",
            Self::Edit => "Modifier",
            Self::Tunnel => "Tunnel",
            Self::Test => "Tester",
            Self::Help => "Aide",
            Self::Details => "Détails",
            Self::Reload => "Recharger",
            Self::Favorite => "Favori",
            Self::Note => "Note",
            Self::FavoritesOnly => "Favoris seuls",
            Self::Duplicate => "Dupliquer",
            Self::Backups => "Sauvegardes",
            Self::Active => "Tunnels actifs",
            Self::Profiles => "Profils de tunnels",
            Self::Connect => "Se connecter",
            Self::Info => "Informations",
            Self::Move => "Déplacer",
            Self::Delete => "Supprimer",
            Self::ToggleHidden => "Hôtes masqués",
            Self::SortToggle => "Tri : alterner",
            Self::SortName => "Tri : par nom",
            Self::SortRecent => "Tri : par connexion récente",
            Self::Search => "Rechercher",
            Self::Palette => "Actions",
            Self::Quit => "Quitter",
        }
    }

    pub(super) fn category(self) -> Category {
        match self {
            Self::Connect
            | Self::Tunnel
            | Self::Profiles
            | Self::Active
            | Self::Test
            | Self::Availability => Category::Connection,
            Self::Add
            | Self::Edit
            | Self::Duplicate
            | Self::Move
            | Self::Delete
            | Self::Info
            | Self::Details
            | Self::Favorite
            | Self::Note
            | Self::ChooseAlias => Category::Host,
            Self::Search
            | Self::FavoritesOnly
            | Self::ToggleHidden
            | Self::SourceFilter
            | Self::ClearSourceFilter
            | Self::ToggleAliases
            | Self::GroupMode
            | Self::SortToggle
            | Self::SortName
            | Self::SortRecent
            | Self::Columns => Category::Organization,
            Self::BatchMark
            | Self::BatchSelectVisible
            | Self::BatchClear
            | Self::BatchOpen
            | Self::BatchResults
            | Self::Keys
            | Self::Backups => Category::Tools,
            Self::Reload | Self::Palette | Self::Help | Self::Quit => Category::Application,
        }
    }

    /// `Err` carries the reason shown to the user instead of running the action.
    pub(super) fn availability(self, has_host: bool) -> Result<(), &'static str> {
        let needs_host = matches!(
            self,
            Self::Edit
                | Self::Tunnel
                | Self::Test
                | Self::Favorite
                | Self::Note
                | Self::Duplicate
                | Self::Profiles
                | Self::ChooseAlias
                | Self::BatchMark
                | Self::Move
                | Self::Delete
        );
        if needs_host && !has_host {
            Err("Sélectionnez un hôte")
        } else {
            Ok(())
        }
    }

    pub(super) fn enabled(self, has_host: bool) -> bool {
        self.availability(has_host).is_ok()
    }
}

/// Historical direct shortcuts of the host list. Printable keys only apply while
/// the table has the focus; function keys also apply from the search input.
const HISTORICAL: [(&str, PaletteAction); 36] = [
    ("enter", PaletteAction::Connect),
    ("f2", PaletteAction::Palette),
    ("f3", PaletteAction::ChooseAlias),
    ("f4", PaletteAction::Details),
    ("f5", PaletteAction::Reload),
    ("f6", PaletteAction::Profiles),
    ("f7", PaletteAction::Active),
    ("f8", PaletteAction::Backups),
    ("/", PaletteAction::Search),
    ("ctrl+f", PaletteAction::Search),
    ("K", PaletteAction::Keys),
    ("v", PaletteAction::Columns),
    ("P", PaletteAction::Availability),
    ("x", PaletteAction::BatchMark),
    ("B", PaletteAction::BatchOpen),
    ("A", PaletteAction::ToggleAliases),
    ("g", PaletteAction::GroupMode),
    ("c", PaletteAction::SourceFilter),
    ("C", PaletteAction::ClearSourceFilter),
    ("*", PaletteAction::Favorite),
    ("N", PaletteAction::Note),
    ("D", PaletteAction::Duplicate),
    ("a", PaletteAction::Add),
    ("e", PaletteAction::Edit),
    ("i", PaletteAction::Info),
    ("m", PaletteAction::Move),
    ("d", PaletteAction::Delete),
    ("f", PaletteAction::Tunnel),
    ("p", PaletteAction::Test),
    ("H", PaletteAction::ToggleHidden),
    ("s", PaletteAction::SortToggle),
    ("n", PaletteAction::SortName),
    ("r", PaletteAction::SortRecent),
    ("?", PaletteAction::Help),
    ("f1", PaletteAction::Help),
    ("h", PaletteAction::Help),
];

/// Keys owned by navigation: no action may take them.
const RESERVED: [&str; 16] = [
    "up", "down", "left", "right", "tab", "backtab", "esc", "pageup", "pagedown", "home", "end",
    " ", "j", "k", "ctrl+p", "ctrl+n",
];

/// Chords a text input edits with; an action bound to one stays out of inputs.
const EDITING: [&str; 9] = [
    "ctrl+a", "ctrl+e", "ctrl+b", "ctrl+f", "ctrl+h", "ctrl+d", "ctrl+u", "ctrl+k", "ctrl+w",
];

/// Letters of the two-step sequence. They belong to the sequence only and never
/// change the meaning of the direct shortcuts (`d` deletes there, duplicates here).
pub(super) const QUICK: [(char, PaletteAction); 15] = [
    ('a', PaletteAction::Add),
    ('e', PaletteAction::Edit),
    ('d', PaletteAction::Duplicate),
    ('m', PaletteAction::Move),
    ('n', PaletteAction::Note),
    ('i', PaletteAction::Info),
    ('t', PaletteAction::Profiles),
    ('k', PaletteAction::Keys),
    ('b', PaletteAction::BatchOpen),
    ('f', PaletteAction::SourceFilter),
    ('g', PaletteAction::GroupMode),
    ('c', PaletteAction::Columns),
    ('r', PaletteAction::Reload),
    ('?', PaletteAction::Help),
    ('q', PaletteAction::Quit),
];

pub(super) fn quick_action(letter: char) -> Option<PaletteAction> {
    QUICK
        .iter()
        .find(|(bound, _)| *bound == letter)
        .map(|(_, action)| *action)
}

/// Preferences accept `Ctrl+G` or `F6`; one-character keys keep their case.
fn normalize_key(key: &str) -> String {
    let key = key.trim_matches(|c: char| c != ' ' && c.is_whitespace());
    if key.chars().count() == 1 {
        key.to_owned()
    } else {
        key.to_lowercase()
    }
}

/// Resolved shortcuts of the host list: the single source for dispatch and display.
#[derive(Clone, Debug)]
pub(super) struct Keymap {
    bindings: Vec<(String, PaletteAction)>,
    prefix: Option<String>,
    /// Refused preferences, each with its reason; shown in the list and the help.
    pub(super) conflicts: Vec<String>,
}

impl Keymap {
    pub(super) fn new(preferences: &KeyBindings) -> Self {
        let mut bindings: Vec<_> = HISTORICAL
            .iter()
            .map(|(key, action)| ((*key).to_owned(), *action))
            .collect();
        let mut conflicts = Vec::new();
        // An override replaces the historical keys of its own action only. Release
        // them all first, so two actions may exchange their keys.
        let mut overrides = Vec::new();
        for (id, keys) in &preferences.shortcuts {
            match PaletteAction::CATALOGUE
                .into_iter()
                .find(|action| action.id() == id)
            {
                Some(PaletteAction::Quit) => conflicts.push(format!(
                    "shortcuts.{id} : utilisez key_bindings.quit_keys pour Quitter"
                )),
                Some(action) => {
                    bindings.retain(|(_, bound)| *bound != action);
                    overrides.push((action, keys));
                }
                None => conflicts.push(format!("shortcuts.{id} : action inconnue")),
            }
        }
        for (action, keys) in overrides {
            for key in keys {
                let key = normalize_key(key);
                if key.is_empty() {
                    continue;
                }
                let shown = display_key(&key);
                if RESERVED.contains(&key.as_str()) {
                    conflicts.push(format!(
                        "{shown} est réservé à la navigation : ignoré pour « {} »",
                        action.label()
                    ));
                } else if let Some((_, holder)) = bindings.iter().find(|(bound, _)| *bound == key) {
                    if *holder != action {
                        conflicts.push(format!(
                            "{shown} sert déjà à « {} » : ignoré pour « {} »",
                            holder.label(),
                            action.label()
                        ));
                    }
                } else {
                    bindings.push((key, action));
                }
            }
        }
        // Configured quit keys keep their precedence over a historical letter.
        for key in &preferences.quit_keys {
            bindings.retain(|(bound, _)| bound != key);
            bindings.push((key.clone(), PaletteAction::Quit));
        }
        let prefix = normalize_key(&preferences.prefix_key);
        let prefix = if prefix.is_empty() {
            None
        } else if RESERVED.contains(&prefix.as_str()) || prefix.chars().count() == 1 {
            conflicts.push(format!(
                "Préfixe {} refusé : touche de navigation ou caractère de saisie",
                display_key(&prefix)
            ));
            None
        } else if let Some((_, holder)) = bindings.iter().find(|(bound, _)| *bound == prefix) {
            conflicts.push(format!(
                "Préfixe {} désactivé : sert déjà à « {} »",
                display_key(&prefix),
                holder.label()
            ));
            None
        } else {
            if EDITING.contains(&prefix.as_str()) {
                conflicts.push(format!(
                    "Préfixe {} : édite le texte dans les champs, actif hors saisie seulement",
                    display_key(&prefix)
                ));
            }
            Some(prefix)
        };
        Self {
            bindings,
            prefix,
            conflicts,
        }
    }

    /// `key` uses the same spelling as the preferences: `a`, `D`, `f6`, `ctrl+f`.
    pub(super) fn action(&self, key: &str) -> Option<PaletteAction> {
        self.bindings
            .iter()
            .find(|(bound, _)| bound == key)
            .map(|(_, action)| *action)
    }

    /// Whether `key` opens the quick actions panel; text inputs keep their chords.
    pub(super) fn is_prefix(&self, key: &str, text_focus: bool) -> bool {
        self.prefix
            .as_deref()
            .is_some_and(|prefix| prefix == key && !(text_focus && EDITING.contains(&prefix)))
    }

    pub(super) fn prefix(&self) -> Option<String> {
        self.prefix.as_deref().map(display_key)
    }

    pub(super) fn keys(&self, action: PaletteAction) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(key, _)| display_key(key))
            .collect()
    }

    /// First shortcut of an action, ready for display; `None` when palette-only.
    pub(super) fn shortcut(&self, action: PaletteAction) -> Option<String> {
        self.keys(action).into_iter().next()
    }

    /// Display form used by footers: the shortcut, or a dash when there is none.
    pub(super) fn hint(&self, action: PaletteAction) -> String {
        self.shortcut(action).unwrap_or_else(|| "—".into())
    }
}

pub(super) fn display_key(key: &str) -> String {
    let (modifier, base) = match key.split_once('+') {
        Some((modifier, base)) if !base.is_empty() => (Some(modifier), base),
        _ => (None, key),
    };
    let base = match base {
        "enter" => "Enter".to_owned(),
        "esc" => "Esc".to_owned(),
        "tab" => "Tab".to_owned(),
        " " => "Space".to_owned(),
        _ if base.len() > 1 && base.starts_with('f') && base[1..].parse::<u8>().is_ok() => {
            base.to_uppercase()
        }
        _ if modifier.is_some() && base.chars().count() == 1 => base.to_uppercase(),
        _ => base.to_owned(),
    };
    match modifier {
        Some("ctrl") => format!("Ctrl+{base}"),
        Some("alt") => format!("Alt+{base}"),
        Some(other) => format!("{other}+{base}"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_unique_and_labels_never_embed_a_shortcut() {
        let mut ids: Vec<_> = PaletteAction::CATALOGUE.iter().map(|a| a.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PaletteAction::CATALOGUE.len());
        for action in PaletteAction::ALL {
            assert!(PaletteAction::CATALOGUE.contains(&action), "{action:?}");
        }
        let keymap = Keymap::new(&KeyBindings::default());
        for action in PaletteAction::CATALOGUE {
            for key in keymap.keys(action) {
                assert!(
                    !action.label().split_whitespace().any(|word| word == key),
                    "{action:?} embeds {key}"
                );
            }
        }
    }

    #[test]
    fn keymap_is_the_single_source_for_dispatch_and_display() {
        let keymap = Keymap::new(&KeyBindings::default());
        assert_eq!(keymap.action("D"), Some(PaletteAction::Duplicate));
        assert_eq!(keymap.action("d"), Some(PaletteAction::Delete));
        assert_eq!(
            keymap.shortcut(PaletteAction::Profiles).as_deref(),
            Some("F6")
        );
        assert_eq!(keymap.keys(PaletteAction::Search), ["/", "Ctrl+F"]);
        assert_eq!(keymap.keys(PaletteAction::Quit), ["q", "Ctrl+C"]);
        assert_eq!(keymap.shortcut(PaletteAction::FavoritesOnly), None);
        assert_eq!(keymap.hint(PaletteAction::FavoritesOnly), "—");
        // A configured quit key replaces the historical action on that key.
        let custom = Keymap::new(&KeyBindings {
            quit_keys: vec!["x".into()],
            ..KeyBindings::default()
        });
        assert_eq!(custom.action("x"), Some(PaletteAction::Quit));
        assert_eq!(custom.action("q"), None);
        assert_eq!(custom.shortcut(PaletteAction::BatchMark), None);
    }

    fn bindings(prefix: &str, shortcuts: &[(&str, &[&str])]) -> KeyBindings {
        KeyBindings {
            prefix_key: prefix.into(),
            shortcuts: shortcuts
                .iter()
                .map(|(id, keys)| {
                    (
                        (*id).to_owned(),
                        keys.iter().map(|k| (*k).to_owned()).collect(),
                    )
                })
                .collect(),
            ..KeyBindings::default()
        }
    }

    #[test]
    fn overrides_replace_only_their_action_and_explain_conflicts() {
        let keymap = Keymap::new(&bindings(
            "Ctrl+G",
            &[
                ("host.edit", &["E", "d", "j"]),
                ("host.note", &[]),
                ("nope", &["z"]),
                ("app.quit", &["z"]),
            ],
        ));
        // The historical `e` is released; `d` still deletes and `j` still navigates.
        assert_eq!(keymap.action("e"), None);
        assert_eq!(keymap.action("E"), Some(PaletteAction::Edit));
        assert_eq!(keymap.action("d"), Some(PaletteAction::Delete));
        assert_eq!(keymap.action("j"), None);
        assert_eq!(keymap.shortcut(PaletteAction::Note), None);
        assert_eq!(keymap.action("z"), None);
        assert_eq!(keymap.conflicts.len(), 4, "{:?}", keymap.conflicts);
        assert!(keymap.conflicts.iter().any(|c| c.contains("Supprimer")));
        assert!(keymap.is_prefix("ctrl+g", true));
        assert_eq!(keymap.prefix().as_deref(), Some("Ctrl+G"));
        // Two actions may exchange their keys without any conflict.
        let swapped = Keymap::new(&bindings(
            "ctrl+g",
            &[("host.add", &["e"]), ("host.edit", &["a"])],
        ));
        assert!(swapped.conflicts.is_empty(), "{:?}", swapped.conflicts);
        assert_eq!(swapped.action("e"), Some(PaletteAction::Add));
        assert_eq!(swapped.action("a"), Some(PaletteAction::Edit));
    }

    #[test]
    fn prefix_is_configurable_disableable_and_never_steals_text_editing() {
        assert!(Keymap::new(&bindings("", &[])).prefix().is_none());
        let editing = Keymap::new(&bindings("ctrl+a", &[]));
        assert!(editing.is_prefix("ctrl+a", false));
        assert!(!editing.is_prefix("ctrl+a", true));
        assert_eq!(editing.conflicts.len(), 1);
        for refused in ["f5", "g", "tab", "ctrl+c"] {
            let keymap = Keymap::new(&bindings(refused, &[]));
            assert!(keymap.prefix().is_none(), "{refused}");
            assert_eq!(keymap.conflicts.len(), 1, "{refused}");
        }
        let letters: std::collections::BTreeSet<_> = QUICK.iter().map(|(c, _)| *c).collect();
        assert_eq!(letters.len(), QUICK.len());
        assert_eq!(quick_action('d'), Some(PaletteAction::Duplicate));
        assert_eq!(quick_action('z'), None);
    }
}
