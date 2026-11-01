//! Key assistant navigation; native operations leave this draft on screen on error.
use super::*;
use runtime::keys::{Entry, PublicKey, Scanner};
#[derive(Default)]
pub(super) struct State {
    pub entries: Vec<Entry>,
    pub scanner: Scanner,
    session: Vec<PathBuf>,
    refresh_pending: Option<(PathBuf, Vec<Host>)>,
}
#[derive(Clone, Debug)]
pub(super) enum Mode {
    List,
    Generate {
        path: Input,
        comment: Input,
        focus: usize,
    },
    Deploy {
        public: PublicKey,
        private: PathBuf,
        alias: String,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub picker: bool,
    pub target: Option<Host>,
    pub selected: usize,
    pub mode: Mode,
    details: bool,
    details_scroll: usize,
    notice: String,
    preferred: Option<PathBuf>,
    /// Focused named action: of the list (see `actions`), or of the deployment
    /// review, where 0 goes back and 1 deploys.
    button: usize,
}
/// What a named action of the key list does, and where it acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Use,
    Create,
    Agent,
    Attach,
    Deploy,
}
impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::Use => crate::t!("Use this key", "Utiliser cette clé"),
            Self::Create => crate::t!("Create key", "Créer une clé"),
            Self::Agent => crate::t!("Add to agent", "Ajouter à l'agent"),
            Self::Attach => crate::t!("Associate with this host", "Associer à cet hôte"),
            Self::Deploy => crate::t!("Deploy public key", "Déployer la clé publique"),
        }
    }
    /// Deploying is the only action that reaches the server.
    fn remote(self) -> bool {
        self == Self::Deploy
    }
}
/// Choosing a key for a draft never offers what would act on a host or a server.
pub(super) fn actions(picker: bool) -> &'static [Command] {
    if picker {
        &[Command::Use, Command::Create, Command::Agent]
    } else {
        &[
            Command::Create,
            Command::Agent,
            Command::Attach,
            Command::Deploy,
        ]
    }
}
pub(super) fn open_view(m: &mut Model, picker: bool) {
    let target = if picker { None } else { m.selected_host() };
    m.open_view(View::Keys(Editor {
        picker,
        target,
        selected: 0,
        mode: Mode::List,
        details: false,
        details_scroll: 0,
        notice: String::new(),
        preferred: None,
        button: 0,
    }));
    m.error.clear();
}
impl State {
    fn refresh(&mut self, home: PathBuf, hosts: Vec<Host>) {
        if self.scanner.busy() {
            // One coalesced request; generation during a scan must not lose its candidate.
            self.refresh_pending = Some((home, hosts));
        } else {
            self.scanner.start(home, hosts, self.session.clone());
        }
    }
    fn remember_generated(&mut self, generated: PathBuf) {
        self.session.retain(|path| path != &generated);
        if self.session.len() == runtime::keys::INVENTORY_LIMIT {
            self.session.remove(0);
        }
        self.session.push(generated);
    }
}
pub(super) fn refresh(m: &mut Model) {
    if let Some(home) = std::env::var_os("HOME") {
        m.keys.refresh(PathBuf::from(home), m.all.clone());
    } else {
        m.error = crate::t!(
            "HOME unavailable; cannot inventory ~/.ssh",
            "HOME indisponible ; impossible d'inventorier ~/.ssh"
        )
        .into();
    }
}
pub(super) fn input(editor: &mut Editor) -> Option<&mut Input> {
    if editor.details {
        return None;
    }
    if let Mode::Generate {
        path,
        comment,
        focus,
    } = &mut editor.mode
    {
        match *focus {
            0 => Some(path),
            1 => Some(comment),
            // A focused button receives no text.
            _ => None,
        }
    } else {
        None
    }
}
pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Keys(editor) = &mut m.view else {
        return Action::None;
    };
    if k.code == KeyCode::F(4) {
        editor.details = !editor.details;
        editor.details_scroll = 0;
        return Action::None;
    }
    if editor.details {
        match k.code {
            _ if nav::cancels(k) => editor.details = false,
            KeyCode::Down => editor.details_scroll = editor.details_scroll.saturating_add(1),
            KeyCode::Up => editor.details_scroll = editor.details_scroll.saturating_sub(1),
            KeyCode::PageDown => {
                editor.details_scroll = editor.details_scroll.saturating_add(m.page)
            }
            KeyCode::PageUp => editor.details_scroll = editor.details_scroll.saturating_sub(m.page),
            KeyCode::Home => editor.details_scroll = 0,
            KeyCode::End => editor.details_scroll = usize::MAX,
            _ => {}
        }
        return Action::None;
    }
    if nav::cancels(k) {
        if matches!(editor.mode, Mode::List) {
            m.back();
        } else {
            editor.mode = Mode::List;
        }
        m.error.clear();
        return Action::None;
    }
    let previous_selection = editor.selected;
    match &mut editor.mode {
        // Fields, then two buttons: Enter in a field moves on, and only the
        // crate::t!("Continue in terminal", "Continuer dans le terminal") button hands over to ssh-keygen.
        Mode::Generate { focus, .. } => match k.code {
            KeyCode::Tab | KeyCode::Down => *focus = (*focus + 1) % 4,
            KeyCode::BackTab | KeyCode::Up => *focus = (*focus + 3) % 4,
            KeyCode::Left | KeyCode::Right if *focus >= 2 => *focus = 5 - *focus,
            KeyCode::Enter | KeyCode::Char(' ') if *focus == 2 => return Action::GenerateKey,
            KeyCode::Enter | KeyCode::Char(' ') if *focus == 3 => {
                editor.mode = Mode::List;
                m.error.clear();
            }
            KeyCode::Enter => *focus += 1,
            _ => {
                if let Some(input) = input(editor) {
                    input.key(k);
                }
            }
        },
        // Reading the review deploys nothing: the Deploy button has to be chosen.
        Mode::Deploy { .. } => match k.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                editor.button = 1 - editor.button.min(1);
            }
            KeyCode::Enter if editor.button == 1 => return Action::DeployKey,
            KeyCode::Enter => {
                editor.mode = Mode::List;
                editor.button = 0;
                m.error.clear();
            }
            KeyCode::Down => m.scroll = m.scroll.saturating_add(1),
            KeyCode::Up => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::PageDown => m.scroll = m.scroll.saturating_add(m.page),
            KeyCode::PageUp => m.scroll = m.scroll.saturating_sub(m.page),
            KeyCode::Home => m.scroll = 0,
            _ => {}
        },
        Mode::List => {
            // Enter runs the focused named action; each is also a direct key.
            let commands = actions(editor.picker);
            editor.button = editor.button.min(commands.len() - 1);
            let k = match k.code {
                KeyCode::Enter => match commands[editor.button] {
                    Command::Use => k,
                    Command::Create => KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                    Command::Agent => KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                    Command::Attach => KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
                    Command::Deploy => KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                },
                _ => k,
            };
            match k.code {
                KeyCode::Tab | KeyCode::Right => {
                    editor.button = nav::wrap(editor.button, commands.len(), 1)
                }
                KeyCode::BackTab | KeyCode::Left => {
                    editor.button = nav::wrap(editor.button, commands.len(), -1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    editor.selected = if editor.selected == usize::MAX {
                        m.keys.entries.len().saturating_sub(1)
                    } else {
                        editor.selected.saturating_sub(1)
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    editor.selected = if editor.selected == usize::MAX {
                        0
                    } else {
                        editor
                            .selected
                            .saturating_add(1)
                            .min(m.keys.entries.len().saturating_sub(1))
                    }
                }
                KeyCode::PageUp => {
                    editor.selected = if editor.selected == usize::MAX {
                        0
                    } else {
                        editor.selected.saturating_sub(m.page)
                    }
                }
                KeyCode::PageDown => {
                    editor.selected = editor
                        .selected
                        .saturating_add(m.page)
                        .min(m.keys.entries.len().saturating_sub(1))
                }
                KeyCode::Home => editor.selected = 0,
                KeyCode::End => editor.selected = m.keys.entries.len().saturating_sub(1),
                KeyCode::F(5) => return Action::RefreshKeys,
                KeyCode::Char('n') => {
                    editor.mode = Mode::Generate {
                        path: Input::default(),
                        comment: Input::default(),
                        focus: 0,
                    }
                }
                KeyCode::Char('a') => return Action::AddKey,
                KeyCode::Char('t') if !editor.picker => return Action::AttachKey,
                KeyCode::Char('d') if !editor.picker => {
                    if let (Some(entry), Some(target)) =
                        (m.keys.entries.get(editor.selected), &editor.target)
                    {
                        if let Some(public) = &entry.public {
                            m.scroll = 0;
                            editor.button = 0;
                            editor.mode = Mode::Deploy {
                                public: public.clone(),
                                private: entry.private.clone(),
                                alias: target.name.clone(),
                            };
                        } else {
                            m.error =
                                crate::t!("No reviewed public fingerprint; refresh or fix the public file", "Aucune empreinte publique vérifiée ; rafraîchissez ou corrigez le fichier public")
                                    .into();
                        }
                    } else {
                        m.error = crate::t!(
                            "Open Keys with an exact host alias selected to deploy",
                            "Ouvrez Clés avec un alias d'hôte exact sélectionné pour déployer"
                        )
                        .into();
                    }
                }
                KeyCode::Enter if editor.picker => {
                    if let Some(entry) = m.keys.entries.get(editor.selected) {
                        if !entry.private.is_absolute() {
                            m.error = crate::t!(
                                "Unresolved identity cannot fill the draft",
                                "Une identité non résolue ne peut pas remplir le brouillon"
                            )
                            .into();
                            return Action::None;
                        }
                        let value = entry.private.to_string_lossy().into_owned();
                        m.back();
                        if let View::Host(form) = &mut m.view {
                            form.inputs[4].set(&value);
                        }
                        return Action::None;
                    }
                }
                _ => {}
            }
        }
    }
    if editor.selected != previous_selection {
        editor.notice.clear();
        editor.preferred = None;
    }
    Action::None
}
pub(super) fn attach(m: &mut Model, root: &Path) -> Result<()> {
    let View::Keys(editor) = &m.view else {
        anyhow::bail!("key assistant is closed")
    };
    let host = editor.target.as_ref().context(crate::t!(
        "Open Keys with an exact host alias selected to attach",
        "Ouvrez Clés avec un alias d'hôte exact sélectionné pour associer"
    ))?;
    let entry = m
        .keys
        .entries
        .get(editor.selected)
        .context(crate::t!("select a key", "sélectionnez une clé"))?;
    anyhow::ensure!(
        entry.private.is_absolute(),
        crate::t!(
            "identity path is unresolved",
            "le chemin de l'identité n'est pas résolu"
        )
    );
    let next = runtime::keys::attach(host, &entry.private);
    let action = Action::Save(Some(host.clone()), Box::new(next), host.source_file.clone());
    changes::open(m, action, root)
}
use anyhow::Context;
/// Called only after the batch foreground guard and terminal suspension.
pub(super) fn native(m: &mut Model, action: &Action, root: &Path) -> Result<()> {
    m.batch.ensure_foreground_idle()?;
    let View::Keys(editor) = &m.view else {
        anyhow::bail!("key assistant is closed")
    };
    match action {
        Action::GenerateKey => {
            let Mode::Generate { path, comment, .. } = &editor.mode else {
                anyhow::bail!("generation form is closed")
            };
            let generated = PathBuf::from(&path.value);
            runtime::keys::generate(
                Path::new(&path.value),
                &comment.value,
                Path::new("ssh-keygen"),
            )?;
            m.keys.remember_generated(generated.clone());
            if let View::Keys(editor) = &mut m.view {
                editor.preferred = Some(generated);
                editor.selected = usize::MAX;
                editor.notice = crate::t!(
                    "Generated key awaiting inventory; F5 refresh / F4 details",
                    "Clé générée en attente d'inventaire ; F5 rafraîchir / F4 détails"
                )
                .into();
            }
        }
        Action::AddKey => {
            let entry = m.keys.entries.get(editor.selected).context(crate::t!(
                "select a private key",
                "sélectionnez une clé privée"
            ))?;
            runtime::keys::agent_add(&entry.private)?;
        }
        Action::DeployKey => {
            let Mode::Deploy {
                public,
                private,
                alias,
            } = &editor.mode
            else {
                anyhow::bail!(crate::t!(
                    "deployment has not been reviewed",
                    "le déploiement n'a pas été vérifié"
                ))
            };
            // Exact declared alias, frozen in review, remains present before launch.
            config::find_host(root, alias)?;
            runtime::keys::Deployment::prepare(public, private, root, alias)?
                .run(Path::new("ssh-copy-id"))?;
        }
        _ => anyhow::bail!("not a native key operation"),
    }
    if let View::Keys(editor) = &mut m.view {
        editor.mode = Mode::List;
    }
    m.error = crate::t!(
        "Native key operation completed",
        "Opération native sur la clé terminée"
    )
    .into();
    refresh(m);
    Ok(())
}
fn selected_details(entry: Option<&Entry>) -> String {
    entry
        .map(|entry| {
            crate::tf!(
                "Private: {}\nFingerprint: {}\nUses: {}\nDiagnostic: {}",
                "Privée : {}\nEmpreinte : {}\nUsages : {}\nDiagnostic : {}",
                entry.private.display(),
                entry
                    .public
                    .as_ref()
                    .map(|p| p.fingerprint.as_str())
                    .unwrap_or(crate::t!("unavailable", "indisponible")),
                entry.usages.join(", "),
                entry.diagnostic
            )
        })
        .unwrap_or_else(|| {
            crate::t!(
                "No selected key; use ↑/↓ to choose.",
                "Aucune clé sélectionnée ; ↑/↓ pour choisir."
            )
            .into()
        })
}
fn list_help() -> &'static str {
    crate::t!(
        "↑/↓ PgUp/PgDn Home/End: select key • ←/→: named action • Enter: run it\nLocal — n: create key • a: add to agent • t: associate with this host (review; keep other identities)\nRemote server — d: deploy public key (review public key + exact alias)\nF5: refresh • F4: full details/error • Esc: back",
        "↑/↓ PgUp/PgDn Début/Fin : choisir une clé • ←/→ : action nommée • Entrée : la lancer\nLocal — n : créer une clé • a : ajouter à l'agent • t : associer à cet hôte (aperçu ; les autres identités sont conservées)\nServeur distant — d : déployer la clé publique (revue de la clé publique et de l'alias exact)\nF5 : rafraîchir • F4 : détails/erreur complets • Échap : retour"
    )
}

pub(super) fn draw(f: &mut ratatui::Frame, m: &mut Model, area: ratatui::layout::Rect) {
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Color, Style},
        text::Line,
        widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    };
    let View::Keys(editor) = &mut m.view else {
        return;
    };
    let entry = m.keys.entries.get(editor.selected);
    if editor.details {
        let parts = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        f.render_widget(
            Paragraph::new(crate::t!(
                "Keys: full details / error",
                "Clés : détails / erreur complets"
            )),
            parts[0],
        );
        m.page = parts[1].height.max(1) as usize;
        let mode = match &editor.mode {
            Mode::Generate { path, comment, .. } => crate::tf!("Generation path: {}\nComment: {}\nTab: next field • the Continue in terminal button generates", "Chemin de génération : {}\nCommentaire : {}\nTab : champ suivant • le bouton Continuer dans le terminal génère", path.value, comment.value),
            Mode::Deploy { public, private, alias } => crate::tf!("Reviewed public: {}\nPrivate: {}\nFingerprint: {}\nExact alias: {alias}\nRoot: {}", "Publique vérifiée : {}\nPrivée : {}\nEmpreinte : {}\nAlias exact : {alias}\nRacine : {}", public.path.display(), private.display(), public.fingerprint, m.batch.root.display()),
            Mode::List => list_help().into(),
        };
        let text = format!(
            "{}\n{}\n{}\n{mode}",
            m.error,
            editor.notice,
            selected_details(entry)
        );
        details::render_rows(
            f,
            parts[1],
            text.lines()
                .map(|line| Line::from(line.to_owned()))
                .collect(),
            &mut editor.details_scroll,
        );
        f.render_widget(
            Paragraph::new(crate::t!(
                "↑↓ PgUp/Dn Home/End • Esc",
                "↑↓ PgUp/Dn Début/Fin • Échap"
            )),
            parts[2],
        );
        return;
    }
    let compact = area.height < 12 || area.width < 60;
    let parts = Layout::vertical([
        Constraint::Length(if compact { 1 } else { 3 }),
        Constraint::Min(1),
        Constraint::Length(if compact { 2 } else { 5 }),
    ])
    .split(area);
    let target = editor
        .target
        .as_ref()
        .map(|h| h.name.as_str())
        .unwrap_or(crate::t!("none", "aucune"));
    f.render_widget(
        Paragraph::new(crate::tf!("Keys — {} • target: {target}\n{} • {}", "Clés — {} • cible : {target}\n{} • {}",
            if editor.picker {
                crate::t!("IdentityFile picker", "Sélecteur d'IdentityFile")
            } else {
                crate::t!("OpenSSH assistant", "Assistant OpenSSH")
            },
            // What this screen is for, so a draft is never taken for a deployment.
            if editor.picker {
                crate::t!("Choose a key for this draft: only its IdentityFile is filled, nothing is deployed", "Choisir une clé pour ce brouillon : seul son IdentityFile est rempli, rien n'est déployé")
            } else {
                crate::t!("Manage keys: public files only, declared usages", "Gérer les clés : fichiers publics seulement, usages déclarés")
            },
            if m.keys.scanner.busy() {
                crate::t!("Scanning (bounded)…", "Analyse (bornée)…")
            } else {
                crate::t!("F5 refresh", "F5 rafraîchir")
            }
        )),
        parts[0],
    );
    // Named actions of the current step, drawn under its help line.
    let mut actions_line = None;
    let mut deploy_line: Option<(usize, String)> = None;
    let help = match &mut editor.mode {
        Mode::List => {
            let item_height = if compact { 1 } else { 3 };
            m.page = (parts[1].height as usize / item_height).max(1);
            let items = m
                .keys
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let name = entry
                        .private
                        .file_name()
                        .unwrap_or(entry.private.as_os_str())
                        .to_string_lossy();
                    let label = format!("{} {}", index + 1, name);
                    // Keep the distinguishing filename tail visible; F4 exposes every cell.
                    let tail = Input::new(&label).viewport(true, parts[1].width as usize).0;
                    let mut lines = vec![Line::from(if compact {
                        tail
                    } else {
                        entry.private.display().to_string()
                    })];
                    if !compact {
                        lines.push(Line::from(
                            entry
                                .public
                                .as_ref()
                                .map(|p| p.fingerprint.clone())
                                .unwrap_or_else(|| entry.diagnostic.clone()),
                        ));
                        lines.push(Line::from(crate::tf!(
                            "Uses: {} • {}",
                            "Usages : {} • {}",
                            entry.usages.join(", "),
                            entry.diagnostic
                        )));
                    }
                    ListItem::new(lines)
                })
                .collect::<Vec<_>>();
            let selected = (editor.selected < m.keys.entries.len()).then_some(editor.selected);
            f.render_stateful_widget(
                List::new(items).highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black)),
                parts[1],
                &mut ListState::default().with_selected(selected),
            );
            actions_line = Some(editor.button);
            if compact {
                crate::t!(
                    "↑↓ select • F4 details/error",
                    "↑↓ choisir • F4 détails/erreur"
                )
            } else {
                crate::t!("↑↓ select key • ←/→: action • Enter: run it • F4 details/error • F5 refresh • Esc back", "↑↓ choisir une clé • ←/→ : action • Entrée : la lancer • F4 détails/erreur • F5 rafraîchir • Échap retour")
            }
        }
        Mode::Generate {
            path,
            comment,
            focus,
        } => {
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(parts[1]);
            for (index, (label, input)) in [
                (
                    crate::t!(
                        "Absolute private key path",
                        "Chemin absolu de la clé privée"
                    ),
                    path,
                ),
                (crate::t!("Comment", "Commentaire"), comment),
            ]
            .into_iter()
            .enumerate()
            {
                // At heights below the supported eight rows, render the active field first.
                let row = if parts[1].height < 4 {
                    if index == (*focus).min(1) {
                        parts[1]
                    } else {
                        ratatui::layout::Rect::default()
                    }
                } else {
                    rows[index]
                };
                let (text, cursor) = input.viewport(*focus == index, row.width as usize);
                f.render_widget(
                    Paragraph::new(text).block(Block::default().borders(Borders::TOP).title(label)),
                    row,
                );
                if let Some(x) = cursor {
                    if row.height > 1 && row.width > 0 {
                        f.set_cursor_position((row.x + x, row.y + 1));
                    }
                }
            }
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(crate::t!("Ed25519. ssh-keygen then asks for the passphrase in the terminal, and ssho comes back here.", "Ed25519. ssh-keygen demande ensuite la phrase secrète dans le terminal, puis ssho revient ici.")),
                    Line::from(crate::t!("Existing private/.pub paths are never overwritten.", "Les chemins privé/.pub existants ne sont jamais écrasés.")),
                    Line::from(""),
                    Line::from(vec![
                        nav::button(crate::t!("Continue in terminal", "Continuer dans le terminal"), *focus == 2),
                        ratatui::text::Span::raw("  "),
                        nav::button(crate::t!("Back", "Retour"), *focus == 3),
                    ]),
                ])
                .wrap(Wrap { trim: false }),
                rows[2],
            );
            if compact {
                crate::t!(
                    "Tab field • F4 details/error",
                    "Tab champ • F4 détails/erreur"
                )
            } else {
                crate::t!("Tab next field • Enter: next field, or the focused button • Esc back\nF4 full details/error", "Tab champ suivant • Entrée : champ suivant, ou le bouton actif • Échap retour\nF4 détails/erreur complets")
            }
        }
        Mode::Deploy {
            public,
            private,
            alias,
        } => {
            m.page = parts[1].height.max(1) as usize;
            let text = crate::tf!("Confirm deployment — acts on the remote server\nExact alias: {alias}\nFingerprint: {}\nPublic: {}\nPrivate sibling for duplicate check: {}\nRoot config: {}\nNative ssh-copy-id; password and host-key prompts stay native.\nPublic snapshot revalidated before launch; duplicate checking retained.", "Confirmer le déploiement — agit sur le serveur distant\nAlias exact : {alias}\nEmpreinte : {}\nPublique : {}\nPrivée associée pour le contrôle de doublon : {}\nConfiguration racine : {}\nssh-copy-id natif ; mot de passe et clé d'hôte sont demandés nativement.\nClé publique revalidée avant le lancement ; contrôle de doublon conservé.", public.fingerprint, public.path.display(), private.display(), m.batch.root.display());
            deploy_line = Some((editor.button, alias.clone()));
            details::render_rows(
                f,
                parts[1],
                text.lines()
                    .map(|line| Line::from(line.to_owned()))
                    .collect(),
                &mut m.scroll,
            );
            if compact {
                crate::t!(
                    "↑↓ scroll • F4 details/error",
                    "↑↓ défiler • F4 détails/erreur"
                )
            } else {
                crate::t!("↑/↓ PgUp/PgDn scroll • Tab/←→: choose • Enter: confirm the choice • Esc cancel", "↑/↓ PgUp/PgDn défiler • Tab/←→ : choisir • Entrée : valider le choix • Échap annuler")
            }
        }
    };
    let status = if !editor.notice.is_empty() {
        &editor.notice
    } else if !m.error.is_empty() {
        &m.error
    } else {
        match editor.mode {
            Mode::List => crate::t!("F5 refresh • Esc back", "F5 rafraîchir • Échap retour"),
            Mode::Generate { .. } => crate::t!(
                "Continue in terminal generates • Esc back",
                "Continuer dans le terminal génère • Échap retour"
            ),
            Mode::Deploy { .. } => crate::t!(
                "Only the Deploy button deploys • Esc cancel",
                "Seul le bouton Déployer déploie • Échap annuler"
            ),
        }
    };
    // Local actions and the one that reaches the server are told apart by name.
    let mut lines = vec![];
    if let (Some(focused), false) = (actions_line, compact) {
        let commands = actions(editor.picker);
        let focused = focused.min(commands.len() - 1);
        let mut spans = vec![ratatui::text::Span::raw(crate::t!("Local: ", "Local : "))];
        for (index, command) in commands.iter().enumerate() {
            if command.remote() {
                spans.push(ratatui::text::Span::raw(crate::t!(
                    "• Remote server: ",
                    "• Serveur distant : "
                )));
            }
            spans.push(nav::button(command.label(), index == focused));
            spans.push(ratatui::text::Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    if let (Some((focused, alias)), false) = (&deploy_line, compact) {
        lines.push(Line::from(vec![
            nav::button(crate::t!("Back", "Retour"), *focused == 0),
            ratatui::text::Span::raw("  "),
            nav::button(
                &crate::tf!("Deploy to {alias}", "Déployer sur {alias}"),
                *focused == 1,
            ),
        ]));
    }
    // A clipped status always has a visible F4 route to its complete wrapped text.
    lines.extend(
        format!("{help}\n{status}")
            .lines()
            .map(|line| Line::from(line.to_owned())),
    );
    f.render_widget(Paragraph::new(lines), parts[2]);
}

/// Reconcile both visible and suspended editors by path before replacing the scan.
pub(super) fn replace_entries(m: &mut Model, entries: Vec<Entry>) {
    for view in std::iter::once(&mut m.view).chain(m.parents.iter_mut().map(|(view, _)| view)) {
        let View::Keys(editor) = view else { continue };
        let previous = m
            .keys
            .entries
            .get(editor.selected)
            .map(|entry| &entry.private);
        if let Some(index) = editor
            .preferred
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| &entry.private == path))
        {
            editor.selected = index;
            editor.preferred = None;
            editor.notice.clear();
        } else if let Some(previous) = previous {
            if let Some(index) = entries.iter().position(|entry| &entry.private == previous) {
                editor.selected = index;
            } else {
                editor.selected = usize::MAX;
                editor.notice = crate::tf!(
                    "Selected key disappeared: {}. Use ↑/↓ to choose a key.",
                    "La clé sélectionnée a disparu : {}. ↑/↓ pour choisir une clé.",
                    previous.display()
                );
            }
        } else if editor.selected != usize::MAX {
            editor.selected = 0;
        }
        // Mode::Deploy contains its own reviewed public/private/alias snapshot.
    }
    m.keys.entries = entries;
}
pub(super) fn poll(m: &mut Model) {
    if let Some(entries) = m.keys.scanner.poll() {
        replace_entries(m, entries);
        if let Some((home, hosts)) = m.keys.refresh_pending.take() {
            m.keys.refresh(home, hosts);
        }
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn generated_candidate_during_busy_scan_survives_coalesced_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        open_view(&mut m, true);
        m.keys.refresh(tmp.path().into(), vec![]);
        assert!(m.keys.scanner.busy());
        let generated = tmp.path().join("outside/generated");
        // Native successful generation has already published the pair; retain only its path.
        // A missing sibling must still produce an inventory row, never silently disappear.
        m.keys.remember_generated(generated.clone());
        if let View::Keys(editor) = &mut m.view {
            editor.preferred = Some(generated.clone());
        }
        for _ in 0..100 {
            m.keys.refresh(tmp.path().into(), vec![]);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while m.keys.scanner.busy() {
            poll(&mut m);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(m.keys.refresh_pending.is_none());
        let View::Keys(editor) = &m.view else {
            panic!()
        };
        assert_eq!(m.keys.entries[editor.selected].private, generated);
        assert!(m.keys.entries[editor.selected]
            .diagnostic
            .contains("public file missing"));
        assert!(editor.preferred.is_none());
        assert_eq!(m.keys.session.len(), 1);
    }
}
