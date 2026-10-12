//! The existing list actions offered by the modal palette.
#[derive(Clone, Copy, Debug)]
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
}

impl PaletteAction {
    pub(super) const ALL: [Self; 26] = [
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
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Availability => "Availability method P (SSH / ICMP local)",
            Self::Keys => "Keys K (OpenSSH assistant)",
            Self::BatchMark => "Lot : marquer alias x",
            Self::BatchSelectVisible => "Lot : sélectionner résultats du filtre",
            Self::BatchClear => "Lot : effacer les marques",
            Self::BatchOpen => "Lot : préparer / actif B",
            Self::BatchResults => "Lot : derniers résultats",
            Self::GroupMode => "Groupes g",
            Self::ToggleAliases => "Alias regroupés A",
            Self::ChooseAlias => "Choisir un alias F3",
            Self::SourceFilter => "Fichier source c",
            Self::ClearSourceFilter => "Tous les fichiers C",
            Self::Add => "Ajouter",
            Self::Edit => "Modifier",
            Self::Tunnel => "Tunnel",
            Self::Test => "Tester",
            Self::Help => "Aide",
            Self::Details => "Détails",
            Self::Reload => "Recharger",
            Self::Favorite => "Favori *",
            Self::Note => "Note N",
            Self::FavoritesOnly => "Favoris seuls",
            Self::Duplicate => "Dupliquer D",
            Self::Backups => "Sauvegardes F8",
            Self::Active => "Tunnels actifs F7",
            Self::Profiles => "Profils F6",
        }
    }

    pub(super) fn enabled(self, has_host: bool) -> bool {
        has_host
            || matches!(
                self,
                Self::Keys
                    | Self::Availability
                    | Self::BatchSelectVisible
                    | Self::BatchClear
                    | Self::BatchOpen
                    | Self::BatchResults
                    | Self::GroupMode
                    | Self::ToggleAliases
                    | Self::SourceFilter
                    | Self::ClearSourceFilter
                    | Self::Active
                    | Self::Backups
                    | Self::Add
                    | Self::Help
                    | Self::Details
                    | Self::Reload
                    | Self::FavoritesOnly
            )
    }
}
