//! The existing list actions offered by the modal palette.
#[derive(Clone, Copy, Debug)]
pub(super) enum PaletteAction {
    Add,
    Edit,
    Tunnel,
    Test,
    Help,
    Details,
}

impl PaletteAction {
    pub(super) const ALL: [Self; 6] = [
        Self::Add,
        Self::Edit,
        Self::Tunnel,
        Self::Test,
        Self::Help,
        Self::Details,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Add => "Ajouter",
            Self::Edit => "Modifier",
            Self::Tunnel => "Tunnel",
            Self::Test => "Tester",
            Self::Help => "Aide",
            Self::Details => "Détails",
        }
    }

    pub(super) fn enabled(self, has_host: bool) -> bool {
        has_host || matches!(self, Self::Add | Self::Help | Self::Details)
    }
}
