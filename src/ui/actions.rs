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
}

impl PaletteAction {
    pub(super) const ALL: [Self; 14] = [
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
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
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
                Self::Active
                    | Self::Backups
                    | Self::Add
                    | Self::Help
                    | Self::Details
                    | Self::Reload
                    | Self::FavoritesOnly
            )
    }
}
