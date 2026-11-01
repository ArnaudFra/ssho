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
    Connect,
    Info,
    Move,
    Delete,
    ToggleHidden,
    SortToggle,
    SortName,
    SortRecent,
    Search,
    ClearSearch,
    ClearFilters,
    Messages,
    Language,
    ToggleColors,
    ToggleAscii,
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
    pub(super) const ALL: [Self; 5] = [
        Self::Connection,
        Self::Host,
        Self::Organization,
        Self::Tools,
        Self::Application,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Connection => crate::t!("Connection", "Connexion"),
            Self::Host => crate::t!("Host", "Hôte"),
            Self::Organization => crate::t!("Organization", "Organisation"),
            Self::Tools => crate::t!("Tools", "Outils"),
            Self::Application => "Application",
        }
    }
}

impl PaletteAction {
    /// Every catalogue entry, grouped by category. The palette lists them in this
    /// order; only the action that opens the palette itself is left out of it.
    pub(super) const CATALOGUE: [Self; 44] = [
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
        Self::ClearSearch,
        Self::ClearFilters,
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
        Self::Messages,
        Self::Language,
        Self::ToggleColors,
        Self::ToggleAscii,
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
            Self::ClearSearch => "search.clear",
            Self::ClearFilters => "filter.clear",
            Self::Messages => "app.messages",
            Self::Language => "app.language",
            Self::ToggleColors => "app.colors",
            Self::ToggleAscii => "app.ascii",
            Self::Palette => "app.palette",
            Self::Quit => "app.quit",
        }
    }

    /// English and French names. Both are searched by the palette, so an action
    /// is found by either name whatever the interface language.
    pub(super) fn labels(self) -> (&'static str, &'static str) {
        match self {
            Self::Columns => (
                "Columns: order and visibility",
                "Colonnes : ordre et visibilité",
            ),
            Self::Availability => ("Availability method", "Méthode de disponibilité"),
            Self::Keys => ("OpenSSH keys", "Clés OpenSSH"),
            Self::BatchMark => ("Batch: mark this alias", "Lot : marquer l'alias"),
            Self::BatchSelectVisible => (
                "Batch: target the filter results",
                "Lot : cibler les résultats du filtre",
            ),
            Self::BatchClear => ("Batch: clear the marks", "Lot : effacer les marques"),
            Self::BatchOpen => ("Batch: prepare / follow", "Lot : préparer / suivre"),
            Self::BatchResults => ("Batch: last results", "Lot : derniers résultats"),
            Self::GroupMode => ("Groups", "Groupes"),
            Self::ToggleAliases => ("Grouped aliases", "Alias regroupés"),
            Self::ChooseAlias => ("Choose an alias", "Choisir un alias"),
            Self::SourceFilter => ("Filter by source file", "Filtrer par fichier source"),
            Self::ClearSourceFilter => ("All files", "Tous les fichiers"),
            Self::Add => ("Add host", "Ajouter un hôte"),
            Self::Edit => ("Edit", "Modifier"),
            Self::Tunnel => ("Create a tunnel", "Créer un tunnel"),
            Self::Test => ("Test availability", "Tester la disponibilité"),
            Self::Help => ("Help", "Aide"),
            Self::Details => ("Details pane", "Panneau de détails"),
            Self::Reload => ("Reload", "Recharger"),
            Self::Favorite => ("Favorite: pin / unpin", "Favori : épingler / retirer"),
            Self::Note => ("Personal note", "Note personnelle"),
            Self::FavoritesOnly => ("Favorites only", "Favoris seuls"),
            Self::Duplicate => ("Duplicate", "Dupliquer"),
            Self::Backups => ("Backups", "Sauvegardes"),
            Self::Active => ("Active tunnels", "Tunnels actifs"),
            Self::Profiles => ("Tunnel profiles", "Profils de tunnels"),
            Self::Connect => ("Connect", "Se connecter"),
            Self::Info => ("Host sheet", "Fiche d'informations"),
            Self::Move => ("Move", "Déplacer"),
            Self::Delete => ("Delete", "Supprimer"),
            Self::ToggleHidden => (
                "Hidden hosts: show / hide",
                "Hôtes masqués : afficher / cacher",
            ),
            Self::SortToggle => ("Sort: toggle", "Tri : alterner"),
            Self::SortName => ("Sort: by name", "Tri : par nom"),
            Self::SortRecent => ("Sort: by recent connection", "Tri : par connexion récente"),
            Self::Search => ("Search", "Rechercher"),
            Self::ClearSearch => ("Clear the search", "Effacer la recherche"),
            Self::ClearFilters => ("Clear all filters", "Effacer tous les filtres"),
            Self::Messages => ("Messages and last error", "Messages et dernière erreur"),
            Self::Language => ("Language", "Langue de l'interface"),
            Self::ToggleColors => ("Display: colours", "Affichage : couleurs"),
            Self::ToggleAscii => ("Display: ASCII symbols", "Affichage : symboles ASCII"),
            Self::Palette => ("Action palette", "Palette d'actions"),
            Self::Quit => ("Quit", "Quitter"),
        }
    }

    pub(super) fn label(self) -> &'static str {
        let (en, fr) = self.labels();
        crate::t!(en, fr)
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
            | Self::ClearSearch
            | Self::ClearFilters
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
            Self::Reload
            | Self::Messages
            | Self::Language
            | Self::ToggleColors
            | Self::ToggleAscii
            | Self::Palette
            | Self::Help
            | Self::Quit => Category::Application,
        }
    }

    /// Words that also find the action in the palette, beyond its label.
    pub(super) fn keywords(self) -> &'static str {
        match self {
            Self::Connect => "connexion ssh session ouvrir connect login open",
            Self::Tunnel => "transfert port forwarding redirection socks local distant tunnel port",
            Self::Profiles => "tunnel transfert forwarding enregistrés lancer saved launch",
            Self::Active => "tunnel en cours suivi arrêter processus running follow stop",
            Self::Test => "ping sonde vérifier joignable probe check reachable",
            Self::Availability => "ping icmp ssh sonde tester probe test",
            Self::Add => "nouveau créer new add create host",
            Self::Edit => "éditer changer hôte edit change modify",
            Self::Duplicate => "copier cloner copy clone",
            Self::Move => "fichier include déplacement move file",
            Self::Delete => "effacer retirer remove delete erase",
            Self::Info => {
                "détails configuration effective historique info details effective history sheet"
            }
            Self::Details => "latéral aperçu volet side preview pane",
            Self::Favorite => "étoile épingle pin star favorite",
            Self::Note => "commentaire mémo comment memo",
            Self::ChooseAlias => "cible exacte nom bloc exact target name block",
            Self::Search => "filtrer chercher trouver find filter find",
            Self::ClearSearch => "vider retirer texte requête empty remove query text",
            Self::ClearFilters => "réinitialiser reset vider tout reset empty all",
            Self::FavoritesOnly => "filtre étoile épinglés filter pinned starred",
            Self::ToggleHidden => "cachés hidden visibilité hidden visibility",
            Self::SourceFilter => "include config fichier filtre file filter",
            Self::ClearSourceFilter => "effacer retirer filtre fichier clear remove file filter",
            Self::ToggleAliases => "regrouper bloc noms group block names",
            Self::GroupMode => {
                "arborescence replier fichier tag regroupement tree collapse file tag grouping"
            }
            Self::SortToggle | Self::SortName | Self::SortRecent => {
                "ordre trier classer order sort"
            }
            Self::Columns => "affichage tableau champs display table fields",
            Self::BatchMark
            | Self::BatchSelectVisible
            | Self::BatchClear
            | Self::BatchOpen
            | Self::BatchResults => {
                "commande multi-hôtes exécuter cibles batch command multi-host run targets"
            }
            Self::Keys => {
                "clef ssh-keygen agent identité déployer générer key agent identity deploy generate"
            }
            Self::Backups => "restaurer restauration historique diff annuler restore history undo",
            Self::Reload => "rafraîchir relire actualiser refresh reread",
            Self::Messages => "erreur avertissement warning error détail journal log",
            Self::Language => "langue language english français anglais french traduction",
            Self::ToggleColors => {
                "couleur color monochrome sobre contraste thème clair sombre no_color plain theme light dark"
            }
            Self::ToggleAscii => "ascii unicode bordure cadre caractères police border font glyph",
            Self::Palette => "f2 commandes",
            Self::Help => "raccourcis touches documentation shortcuts keys documentation",
            Self::Quit => "sortir fermer exit leave close",
        }
    }

    /// One sentence shown under the palette for the selected action.
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Connect => crate::t!(
                "Opens an SSH session on the selected alias, then comes back here.",
                "Ouvre une session SSH sur l'alias sélectionné, puis revient ici."
            ),
            Self::Tunnel => crate::t!(
                "Prepares a local, remote or SOCKS port forwarding for this host.",
                "Prépare un transfert de port local, distant ou SOCKS pour cet hôte."
            ),
            Self::Profiles => crate::t!(
                "Lists the saved tunnels of this host: open, launch, delete.",
                "Liste les tunnels enregistrés de cet hôte : ouvrir, lancer, supprimer."
            ),
            Self::Active => crate::t!(
                "Follows the tunnels started by the application and lets you stop them.",
                "Suit les tunnels lancés par l'application et permet de les arrêter."
            ),
            Self::Test => crate::t!(
                "Runs the availability probes of every host again.",
                "Relance les sondes de disponibilité de tous les hôtes."
            ),
            Self::Availability => crate::t!(
                "Chooses what availability measures: SSH or local ICMP.",
                "Choisit ce que mesure la disponibilité : SSH ou ICMP local."
            ),
            Self::Add => crate::t!(
                "Creates a new host in an SSH configuration file.",
                "Crée un nouvel hôte dans un fichier de configuration SSH."
            ),
            Self::Edit => crate::t!(
                "Edits the SSH block of the selected host, with a preview before writing.",
                "Modifie le bloc SSH de l'hôte sélectionné, avec aperçu avant écriture."
            ),
            Self::Duplicate => crate::t!(
                "Copies the host under a new name, in the file of your choice.",
                "Copie l'hôte sous un nouveau nom, dans le fichier de votre choix."
            ),
            Self::Move => crate::t!(
                "Moves the host block to another included file.",
                "Déplace le bloc de l'hôte vers un autre fichier inclus."
            ),
            Self::Delete => crate::t!(
                "Deletes the host after a preview of the change; nothing is written before.",
                "Supprime l'hôte après aperçu de la modification ; rien n'est écrit avant."
            ),
            Self::Info => crate::t!(
                "Shows the declared and effective configuration, the note and the history.",
                "Affiche la configuration déclarée et effective, la note et l'historique."
            ),
            Self::Details => crate::t!(
                "Shows or hides the details pane next to the list.",
                "Affiche ou masque le volet de détails à côté de la liste."
            ),
            Self::Favorite => crate::t!(
                "Pins the host at the top of the list, or removes the pin.",
                "Épingle l'hôte en tête de liste, ou retire l'épingle."
            ),
            Self::Note => crate::t!(
                "Edits the personal note, kept outside the SSH file.",
                "Édite la note personnelle, conservée hors du fichier SSH."
            ),
            Self::ChooseAlias => crate::t!(
                "Chooses the exact alias of the block used to connect.",
                "Choisit l'alias exact du bloc utilisé pour se connecter."
            ),
            Self::Search => crate::t!(
                "Puts the cursor in the search of the list.",
                "Place le curseur dans la recherche de la liste."
            ),
            Self::ClearSearch => crate::t!(
                "Empties the search; the other filters stay active.",
                "Vide la recherche ; les autres filtres restent actifs."
            ),
            Self::ClearFilters => crate::t!(
                "Empties the search and removes the file, favorites and hidden-hosts filters.",
                "Vide la recherche et retire les filtres par fichier, favoris et masqués."
            ),
            Self::FavoritesOnly => crate::t!(
                "Shows favorites only, or goes back to every host.",
                "N'affiche que les favoris, ou revient à tous les hôtes."
            ),
            Self::ToggleHidden => crate::t!(
                "Shows or hides the hosts carrying the hidden tag.",
                "Affiche ou cache les hôtes portant le tag hidden."
            ),
            Self::SourceFilter => crate::t!(
                "Limits the list to the hosts of one configuration file.",
                "Limite la liste aux hôtes d'un fichier de configuration."
            ),
            Self::ClearSourceFilter => crate::t!(
                "Removes the filter by source file.",
                "Retire le filtre par fichier source."
            ),
            Self::ToggleAliases => crate::t!(
                "Groups the aliases of one block on a single row, or separates them.",
                "Regroupe les alias d'un même bloc sur une seule ligne, ou les sépare."
            ),
            Self::GroupMode => crate::t!(
                "Presents the list flat, by file, or by file then tag.",
                "Présente la liste à plat, par fichier, ou par fichier puis tag."
            ),
            Self::SortToggle => crate::t!(
                "Switches between sorting by name and by recent connection.",
                "Alterne entre le tri par nom et par connexion récente."
            ),
            Self::SortName => crate::t!("Sorts the hosts by name.", "Trie les hôtes par nom."),
            Self::SortRecent => crate::t!(
                "Sorts the hosts by last connection.",
                "Trie les hôtes par dernière connexion."
            ),
            Self::Columns => crate::t!(
                "Chooses the columns of the list and their order.",
                "Choisit les colonnes de la liste et leur ordre."
            ),
            Self::BatchMark => crate::t!(
                "Marks or unmarks the selected alias as a batch target.",
                "Marque ou démarque l'alias sélectionné comme cible d'un lot."
            ),
            Self::BatchSelectVisible => crate::t!(
                "Adds every host the filter shows to the targets.",
                "Ajoute aux cibles tous les hôtes que le filtre affiche."
            ),
            Self::BatchClear => crate::t!(
                "Removes every marked target.",
                "Retire toutes les cibles marquées."
            ),
            Self::BatchOpen => crate::t!(
                "Prepares a command for the marked targets, or follows the running batch.",
                "Prépare une commande pour les cibles marquées, ou suit le lot en cours."
            ),
            Self::BatchResults => crate::t!(
                "Opens the results of the last batch again.",
                "Rouvre les résultats du dernier lot lancé."
            ),
            Self::Keys => crate::t!(
                "Lists the keys, generates one, adds it to the agent or deploys it.",
                "Inventorie les clés, en génère, les ajoute à l'agent ou les déploie."
            ),
            Self::Backups => crate::t!(
                "Lists the backups of the SSH files and prepares a restoration.",
                "Liste les sauvegardes des fichiers SSH et prépare une restauration."
            ),
            Self::Reload => crate::t!(
                "Reads the SSH file and its Includes again after an external change.",
                "Relit le fichier SSH et ses Includes après une modification externe."
            ),
            Self::Messages => crate::t!(
                "Shows the last error and the current warnings in full.",
                "Affiche en entier la dernière erreur et les avertissements en cours."
            ),
            Self::Language => crate::t!(
                "Chooses the language of the interface: English or French.",
                "Choisit la langue de l'interface : anglais ou français."
            ),
            Self::ToggleColors => crate::t!(
                "Turns colours on or off; without them every state keeps its text or sign.",
                "Active ou coupe les couleurs ; sans elles chaque état garde son texte ou son signe."
            ),
            Self::ToggleAscii => crate::t!(
                "Draws frames and signs with ASCII characters, for fonts without the symbols.",
                "Dessine cadres et signes en caractères ASCII, pour les polices sans ces symboles."
            ),
            Self::Palette => crate::t!("Opens this palette.", "Ouvre cette palette."),
            Self::Help => crate::t!(
                "Shows the shortcuts and the conventions of the screen.",
                "Affiche les raccourcis et les conventions de l'écran."
            ),
            Self::Quit => crate::t!(
                "Closes the application; running work is summarised first.",
                "Ferme l'application ; les travaux en cours sont récapitulés avant."
            ),
        }
    }

    fn needs_host(self) -> bool {
        matches!(
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
        )
    }

    /// Whether the action can be dispatched at all without a selected host.
    pub(super) fn enabled(self, has_host: bool) -> bool {
        has_host || !self.needs_host()
    }

    /// `Err` carries the precise reason shown instead of running the action.
    pub(super) fn availability(self, context: &Context) -> Result<(), &'static str> {
        let no_host = if context.group {
            crate::t!(
                "Select a host, not a group",
                "Sélectionnez un hôte, pas un groupe"
            )
        } else {
            crate::t!("Select a host", "Sélectionnez un hôte")
        };
        match self {
            _ if self.needs_host() && !context.host => Err(no_host),
            Self::Connect if !context.host => Err(no_host),
            Self::Info if !context.host && !context.group => Err(crate::t!(
                "Select a host or a group",
                "Sélectionnez un hôte ou un groupe"
            )),
            Self::ClearSourceFilter if !context.source_filter => Err(crate::t!(
                "No file filter is active",
                "Aucun filtre de fichier actif"
            )),
            Self::ClearSearch if !context.query => {
                Err(crate::t!("No search is active", "Aucune recherche active"))
            }
            Self::ClearFilters if !context.filters => {
                Err(crate::t!("No filter is active", "Aucun filtre actif"))
            }
            Self::Messages if !context.messages => {
                Err(crate::t!("No message to show", "Aucun message à afficher"))
            }
            Self::BatchClear if context.marks == 0 => {
                Err(crate::t!("No target is marked", "Aucune cible marquée"))
            }
            Self::BatchResults if !context.batch_results => {
                Err(crate::t!("No batch was launched", "Aucun lot lancé"))
            }
            _ => Ok(()),
        }
    }
}

/// What the availability of an action depends on, captured from the list.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Context {
    pub host: bool,
    /// A group header is selected instead of a host.
    pub group: bool,
    pub source_filter: bool,
    /// The search holds text.
    pub query: bool,
    /// Any of search, file, favorites or hidden-hosts filters is active.
    pub filters: bool,
    pub marks: usize,
    pub batch_results: bool,
    /// An error, a warning or a last SSH result can be read in full.
    pub messages: bool,
}

/// Case- and accent-insensitive form used to search action names.
pub(super) fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ÿ' => 'y',
            '’' => '\'',
            _ => c,
        })
        .collect()
}

/// Historical direct shortcuts of the host list. Printable keys only apply while
/// the table has the focus; function keys also apply from the search input.
const HISTORICAL: [(&str, PaletteAction); 38] = [
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
    ("backspace", PaletteAction::ClearSearch),
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
    ("!", PaletteAction::Messages),
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

/// Keys a focused text input consumes: characters, with or without a modifier
/// (its editing chords), and the keys that erase. An action bound to one of
/// them only applies outside inputs.
pub(super) fn owned_by_text(key: &str) -> bool {
    let base = match key.split_once('+') {
        Some((_, base)) if !base.is_empty() => base,
        _ => key,
    };
    base.chars().count() == 1 || matches!(base, "backspace" | "delete")
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
                Some(PaletteAction::Quit) => conflicts.push(crate::tf!(
                    "shortcuts.{id}: use key_bindings.quit_keys for Quit",
                    "shortcuts.{id} : utilisez key_bindings.quit_keys pour Quitter"
                )),
                Some(action) => {
                    bindings.retain(|(_, bound)| *bound != action);
                    overrides.push((action, keys));
                }
                None => conflicts.push(crate::tf!(
                    "shortcuts.{id}: unknown action",
                    "shortcuts.{id} : action inconnue"
                )),
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
                    conflicts.push(crate::tf!(
                        "{shown} is reserved for navigation: ignored for \"{}\"",
                        "{shown} est réservé à la navigation : ignoré pour « {} »",
                        action.label()
                    ));
                } else if let Some((_, holder)) = bindings.iter().find(|(bound, _)| *bound == key) {
                    if *holder != action {
                        conflicts.push(crate::tf!(
                            "{shown} is already used by \"{}\": ignored for \"{}\"",
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
            conflicts.push(crate::tf!(
                "Prefix {} refused: navigation key or text character",
                "Préfixe {} refusé : touche de navigation ou caractère de saisie",
                display_key(&prefix)
            ));
            None
        } else if let Some((_, holder)) = bindings.iter().find(|(bound, _)| *bound == prefix) {
            conflicts.push(crate::tf!(
                "Prefix {} disabled: already used by \"{}\"",
                "Préfixe {} désactivé : sert déjà à « {} »",
                display_key(&prefix),
                holder.label()
            ));
            None
        } else {
            if EDITING.contains(&prefix.as_str()) {
                conflicts.push(crate::tf!(
                    "Prefix {}: it edits text in fields, so it only applies outside inputs",
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
        // Key names follow the interface language, like the texts around them.
        "enter" => crate::t!("Enter", "Entrée").to_owned(),
        "esc" => crate::t!("Esc", "Échap").to_owned(),
        "tab" => "Tab".to_owned(),
        "backspace" => "Backspace".to_owned(),
        "delete" => crate::t!("Del", "Suppr").to_owned(),
        " " => crate::t!("Space", "Espace").to_owned(),
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
        assert!(keymap.conflicts.iter().any(|c| c.contains("Delete")));
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
