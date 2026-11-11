//! Rendering of the export / import screen.
use super::*;

/// Export / import: the scope of an export, or what an import would change,
/// is always on screen before the button that writes.
pub(super) fn data_transfer(f: &mut Frame, m: &mut Model, area: Rect) {
    use crate::storage::transfer::{Policy, Section};
    let View::Transfer(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let title = match editor.mode {
        transfer::Mode::Home => {
            crate::t!("Export / import data", "Exporter / importer les données")
        }
        transfer::Mode::Export => crate::t!(
            "Export / import data > Export",
            "Exporter / importer les données > Exporter"
        ),
        transfer::Mode::ImportPath | transfer::Mode::Preview => crate::t!(
            "Export / import data > Import",
            "Exporter / importer les données > Importer"
        ),
    };
    let border = block(title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 4) as u16;
    let footer = match editor.mode {
        transfer::Mode::Home => crate::t!(
            "Tab/←→: choose • Enter: open • Esc: back",
            "Tab/←→ : choisir • Entrée : ouvrir • Échap : retour"
        ),
        transfer::Mode::Export => crate::t!(
            "Tab/↑↓: move • Space: include or not • Enter: next • Ctrl+S: export • Esc: back",
            "Tab/↑↓ : se déplacer • Espace : inclure ou non • Entrée : suivant • Ctrl+S : exporter • Échap : retour"
        ),
        transfer::Mode::ImportPath => crate::t!(
            "Enter: to the Read button • Ctrl+L: read the file • Esc: back — reading changes nothing",
            "Entrée : vers le bouton Lire • Ctrl+L : lire le fichier • Échap : retour — lire ne modifie rien"
        ),
        transfer::Mode::Preview => crate::t!(
            "Tab/↑↓: move • Space: include or not • ←/→: change the choice • Enter: next • Esc: back",
            "Tab/↑↓ : se déplacer • Espace : inclure ou non • ←/→ : changer le choix • Entrée : suivant • Échap : retour"
        ),
    };
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(notice_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(notice)
            .style(notice_style)
            .wrap(Wrap { trim: true }),
        parts[1],
    );
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[2],
    );
    let body = parts[0];
    let gray = Style::default().fg(GRAY);
    let mark = |focused: bool| if focused { ">" } else { " " };
    let style = |focused: bool| {
        if focused {
            accent()
        } else {
            Style::default()
        }
    };
    let never = crate::t!(
        "Never included: private keys, passwords, SSH files, connection history, backups, SSH launcher.",
        "Jamais inclus : clés privées, mots de passe, fichiers SSH, historique des connexions, sauvegardes, lanceur SSH."
    );
    let private = crate::t!(
        "Notes and commands may hold private information: the file is created readable by you only.",
        "Notes et commandes peuvent contenir des informations privées : le fichier est créé lisible par vous seul."
    );
    let mut lines: Vec<Line<'_>> = Vec::new();
    // Row of the line that owns the caret, with the width of its label.
    let mut caret: Option<(usize, u16, &Input)> = None;
    match editor.mode {
        transfer::Mode::Home => {
            for text in [
                crate::tf!(
                    "Configuration in use: {}",
                    "Configuration en cours : {}",
                    m.batch.root.display()
                ),
                crate::t!(
                    "An export writes, to a file you name, what you choose among: favorites and notes, tunnel profiles and saved views of this configuration, saved commands, portable preferences.",
                    "Un export écrit, dans un fichier que vous nommez, ce que vous choisissez parmi : favoris et notes, profils de tunnels et vues enregistrées de cette configuration, commandes enregistrées, préférences portables."
                )
                .to_owned(),
                never.to_owned(),
                crate::t!(
                    "An import reads such a file into this configuration. It first shows what is new, identical or different; nothing changes before you confirm, and the current files are copied aside first.",
                    "Un import lit un tel fichier dans cette configuration. Il montre d'abord ce qui est nouveau, identique ou différent ; rien ne change avant votre confirmation, et les fichiers actuels sont d'abord copiés à part."
                )
                .to_owned(),
            ] {
                lines.push(Line::from(text));
                lines.push(Line::from(""));
            }
            let mut spans = Vec::new();
            for (index, label) in transfer::home_buttons().into_iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(nav::button(label, editor.focus == index));
            }
            lines.push(Line::from(spans));
        }
        transfer::Mode::Export => {
            lines.push(Line::styled(
                crate::tf!(
                    "What the file will hold, from {}:",
                    "Ce que le fichier contiendra, depuis {} :",
                    m.batch.root.display()
                ),
                accent(),
            ));
            for (index, section) in Section::ALL.into_iter().enumerate() {
                let focused = editor.focus == index;
                let count = editor.counts.get(&section).copied().unwrap_or(0);
                lines.push(Line::styled(
                    format!(
                        "{} [{}] {:<26} {}",
                        mark(focused),
                        if editor.chosen[index] { "x" } else { " " },
                        transfer::section_label(section),
                        if section == Section::Preferences {
                            crate::t!(
                                "language, display, list, keyboard",
                                "langue, affichage, liste, clavier"
                            )
                            .to_owned()
                        } else {
                            crate::tf!("{count} entries", "{count} entrées")
                        }
                    ),
                    style(focused),
                ));
            }
            lines.push(Line::from(""));
            lines.push(Line::styled(private, gray));
            lines.push(Line::styled(never, gray));
            lines.push(Line::from(""));
            let label = crate::t!("File to create: ", "Fichier à créer : ");
            let focused = editor.focus == 5;
            let prefix = label.width() as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            if focused {
                caret = Some((lines.len(), prefix, &editor.path));
            }
            lines.push(Line::from(vec![
                Span::styled(format!("{} {label}", mark(focused)), style(focused)),
                input_span(&editor.path, focused, room),
            ]));
            lines.push(Line::styled(
                crate::t!(
                    "  An existing file is never replaced. ~/ is your home directory.",
                    "  Un fichier existant n'est jamais remplacé. ~/ est votre dossier personnel."
                ),
                gray,
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(crate::t!("Export", "Exporter"), editor.focus == 6),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == 7),
            ]));
        }
        transfer::Mode::ImportPath => {
            lines.push(Line::from(crate::tf!(
                "The file is imported into the configuration in use: {}",
                "Le fichier est importé dans la configuration en cours : {}",
                m.batch.root.display()
            )));
            lines.push(Line::from(""));
            let label = crate::t!("Export file to read: ", "Fichier d'export à lire : ");
            let focused = editor.focus == 0;
            let prefix = label.width() as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            if focused {
                caret = Some((lines.len(), prefix, &editor.import_path));
            }
            lines.push(Line::from(vec![
                Span::styled(format!("{} {label}", mark(focused)), style(focused)),
                input_span(&editor.import_path, focused, room),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(
                    crate::t!("Read the file", "Lire le fichier"),
                    editor.focus == 1,
                ),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == 2),
            ]));
        }
        transfer::Mode::Preview => {
            if let Some(export) = &editor.export {
                lines.push(Line::styled(
                    crate::tf!(
                        "From: {} ({}, {})",
                        "Depuis : {} ({}, {})",
                        export.configuration,
                        export.application,
                        export.exported_at
                    ),
                    gray,
                ));
                lines.push(Line::styled(
                    crate::tf!("Into: {}", "Vers : {}", m.batch.root.display()),
                    accent(),
                ));
            }
            let listed = transfer::sections(editor);
            for (stop, section) in listed.iter().enumerate() {
                let Some(plan) = editor.plan.get(section) else {
                    continue;
                };
                let index = Section::ALL.iter().position(|s| s == section).unwrap_or(0);
                let focused = editor.focus == stop;
                let mut counts = vec![
                    crate::tf!("{} new", "{} nouvelles", plan.new.len()),
                    crate::tf!("{} identical", "{} identiques", plan.same),
                    crate::tf!("{} different", "{} différentes", plan.conflicts.len()),
                ];
                if !plan.unknown.is_empty() {
                    counts.push(crate::tf!(
                        "{} for an alias unknown here",
                        "{} pour un alias inconnu ici",
                        plan.unknown.len()
                    ));
                }
                if plan.local_only > 0 && *section != Section::Preferences {
                    counts.push(crate::tf!(
                        "{} only here",
                        "{} seulement ici",
                        plan.local_only
                    ));
                }
                lines.push(Line::styled(
                    format!(
                        "{} [{}] {:<24} {}",
                        mark(focused),
                        if editor.include[index] { "x" } else { " " },
                        transfer::section_label(*section),
                        counts.join(" • ")
                    ),
                    style(focused),
                ));
            }
            let missing: Vec<&str> = editor
                .plan
                .values()
                .flat_map(|plan| plan.missing_sources.iter().map(String::as_str))
                .collect();
            if !missing.is_empty() {
                lines.push(Line::styled(
                    crate::tf!(
                        "  View(s) whose source file is not in this configuration: {} — applying them will ask for an update.",
                        "  Vue(s) dont le fichier source n'est pas dans cette configuration : {} — leur application demandera une mise à jour.",
                        missing.join(", ")
                    ),
                    Style::default().fg(Color::Yellow),
                ));
            }
            lines.push(Line::from(""));
            let policy_stop = listed.len();
            let focused = editor.focus == policy_stop;
            lines.push(Line::styled(
                format!(
                    "{} {}‹ {} ›{}",
                    mark(focused),
                    crate::t!(
                        "Entries present on both sides: ",
                        "Entrées présentes des deux côtés : "
                    ),
                    transfer::policy_label(editor.policy),
                    if focused { "  ←/→" } else { "" }
                ),
                style(focused),
            ));
            lines.push(Line::styled(
                format!("  {}", transfer::policy_detail(editor.policy)),
                if editor.policy == Policy::ReplaceSections {
                    Style::default().fg(Color::Yellow)
                } else {
                    gray
                },
            ));
            let mut next = policy_stop + 1;
            if transfer::has_unknown(editor) {
                let focused = editor.focus == next;
                let names: Vec<&str> = editor
                    .plan
                    .values()
                    .flat_map(|plan| plan.unknown.iter().map(String::as_str))
                    .collect();
                lines.push(Line::styled(
                    format!(
                        "{} {}‹ {} ›{}",
                        mark(focused),
                        crate::t!("Aliases unknown here: ", "Alias inconnus ici : "),
                        if editor.keep_unknown {
                            crate::t!("import them anyway", "les importer quand même")
                        } else {
                            crate::t!("leave them out", "les laisser de côté")
                        },
                        if focused { "  ←/→" } else { "" }
                    ),
                    style(focused),
                ));
                lines.push(Line::styled(
                    crate::tf!(
                        "  {} — an entry is attached to an alias of this configuration, never to a name alone.",
                        "  {} — une entrée est rattachée à un alias de cette configuration, jamais à un nom seul.",
                        names.join(", ")
                    ),
                    gray,
                ));
                next += 1;
            }
            lines.push(Line::from(""));
            let (added, replaced, removed, skipped) = transfer::totals(editor);
            lines.push(Line::from(crate::tf!(
                "This import would add {added}, replace {replaced}, remove {removed} and leave out {skipped}.",
                "Cet import ajouterait {added} entrées, en remplacerait {replaced}, en supprimerait {removed} et en laisserait {skipped} de côté."
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(crate::t!("Import", "Importer"), editor.focus == next),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == next + 1),
            ]));
        }
    }
    // Wrapped rows before the caret line decide where the caret really is.
    let caret = caret.map(|(index, prefix, input)| {
        let above: usize = lines[..index]
            .iter()
            .map(|line| select::wrapped_rows(&line.to_string(), width))
            .sum();
        (above, prefix, input)
    });
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
    if let Some((row, prefix, input)) = caret {
        input_cursor(f, input, body, prefix, row);
    }
    if editor.pending {
        let (added, replaced, removed, skipped) = transfer::totals(editor);
        let root = m
            .batch
            .history_root
            .as_ref()
            .map_or_else(String::new, |root| {
                root.join("data-backups").display().to_string()
            });
        m.confirm.draw(
            f,
            whole,
            crate::t!("Import these data?", "Importer ces données ?"),
            vec![
                Line::from(crate::tf!(
                    "{added} added, {replaced} replaced, {removed} removed, {skipped} left out.",
                    "{added} ajoutées, {replaced} remplacées, {removed} supprimées, {skipped} laissées de côté."
                )),
                Line::from(crate::tf!(
                    "The current files are first copied to {root}",
                    "Les fichiers actuels sont d'abord copiés dans {root}"
                )),
            ],
            crate::t!("Go back", "Revenir"),
            crate::t!("Import", "Importer"),
        );
    }
}
