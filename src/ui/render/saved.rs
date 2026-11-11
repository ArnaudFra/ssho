//! Rendering of the saved views and of the command catalogue.
use super::*;

/// Saved views: the view in use, the list, the criteria of the selected one,
/// then the named actions. Naming and deleting are steps of this same screen.
pub(super) fn saved_views(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Views(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(crate::t!("Saved views", "Vues enregistrées"), true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let naming = matches!(editor.mode, views::Mode::SaveAs | views::Mode::Rename);
    let selected = m.views.iter().nth(editor.selected);
    let detail: Vec<String> = match (naming, editor.mode, selected) {
        // A new view is made of the current context: show exactly what is kept.
        (true, views::Mode::SaveAs, _) => views::summary(&m.current_view()),
        (_, _, Some((_, view))) => views::summary(view),
        _ => Vec::new(),
    };
    let detail_rows: usize = detail
        .iter()
        .map(|line| select::wrapped_rows(line, width))
        .sum();
    let footer = if naming {
        crate::t!(
            "Enter: to the Save button • Tab: name and buttons • Ctrl+S: save • Esc: cancel",
            "Entrée : vers le bouton Enregistrer • Tab : nom et boutons • Ctrl+S : enregistrer • Échap : annuler"
        )
    } else {
        crate::t!(
            "↑↓: view • Tab/←→: action • Enter: run the action • Esc: back",
            "↑↓ : vue • Tab/←→ : action • Entrée : lancer l'action • Échap : retour"
        )
    };
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    // A refused action (missing source file...) is shown in full, not cut.
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 4) as u16;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length((detail_rows as u16).min((inner.height / 3).max(1))),
        Constraint::Length(2),
        Constraint::Length(notice_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let in_use = match m.view_state() {
        Some((name, true)) => crate::tf!(
            "View in use: {name} (modified — Update stores the current list)",
            "Vue en cours : {name} (modifiée — Mettre à jour enregistre la liste actuelle)"
        ),
        Some((name, false)) => crate::tf!("View in use: {name}", "Vue en cours : {name}"),
        None => crate::t!("No view in use", "Aucune vue en cours").to_owned(),
    };
    f.render_widget(Paragraph::new(in_use).style(accent()), parts[0]);
    let lines: Vec<Line<'_>> = if m.views.is_empty() {
        vec![Line::styled(
            crate::t!(
                "No saved view. \"Save current as…\" keeps the search, the filters, the sort, the groups and the columns of the list under a name.",
                "Aucune vue enregistrée. « Enregistrer sous… » garde sous un nom la recherche, les filtres, le tri, les groupes et les colonnes de la liste."
            ),
            Style::default().fg(GRAY),
        )]
    } else {
        let height = usize::from(parts[1].height).max(1);
        let offset = (editor.selected + 1).saturating_sub(height);
        m.views
            .keys()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(index, name)| {
                let current = index == editor.selected;
                let used = m.active_view.as_deref() == Some(name.as_str());
                Line::styled(
                    format!(
                        "{} {name}{}",
                        if current { "▶" } else { " " },
                        if used {
                            crate::t!("  ✓ in use", "  ✓ en cours")
                        } else {
                            ""
                        }
                    ),
                    if current {
                        Style::default().fg(Color::Black).bg(CYAN)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[1]);
    f.render_widget(
        Paragraph::new(
            detail
                .into_iter()
                .map(Line::from)
                .collect::<Vec<Line<'_>>>(),
        )
        .style(Style::default().fg(GRAY))
        .wrap(Wrap { trim: true }),
        parts[2],
    );
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(parts[3]);
    if naming {
        let label = if editor.mode == views::Mode::SaveAs {
            crate::t!("Name of the new view: ", "Nom de la nouvelle vue : ")
        } else {
            crate::t!("New name: ", "Nouveau nom : ")
        };
        let prefix = label.width() as u16 + 2;
        let room = usize::from(rows[0].width.saturating_sub(prefix)).max(1);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} {label}", if editor.name_focus == 0 { ">" } else { " " }),
                    if editor.name_focus == 0 {
                        accent()
                    } else {
                        Style::default()
                    },
                ),
                input_span(&editor.name, editor.name_focus == 0, room),
            ])),
            rows[0],
        );
        if editor.name_focus == 0 {
            input_cursor(f, &editor.name, rows[0], prefix, 0);
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                nav::button(crate::t!("Save", "Enregistrer"), editor.name_focus == 1),
                Span::raw("  "),
                nav::button(crate::t!("Cancel", "Annuler"), editor.name_focus == 2),
            ])),
            rows[1],
        );
    } else {
        // Two rows of three, so that no button is cut at 80 columns.
        for (row, chunk) in views::buttons().chunks(3).enumerate() {
            let mut spans = Vec::new();
            for (index, label) in chunk.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(nav::button(label, editor.button == row * 3 + index));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), rows[row]);
        }
    }
    f.render_widget(
        Paragraph::new(notice)
            .style(notice_style)
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[5],
    );
    if editor.mode == views::Mode::Delete {
        if let Some((name, _)) = selected {
            m.confirm.draw(
                f,
                whole,
                crate::t!("Delete this view?", "Supprimer cette vue ?"),
                vec![
                    Line::from(name.clone()),
                    Line::from(crate::t!(
                        "Only the saved view is removed; the list and the SSH files are unchanged.",
                        "Seule la vue enregistrée est retirée ; la liste et les fichiers SSH sont inchangés."
                    )),
                ],
                crate::t!("Go back", "Revenir"),
                crate::t!("Delete the view", "Supprimer la vue"),
            );
        }
    }
}

/// Command catalogue: list and named actions, then the form of one command or
/// the values of its parameters. Each step says what Enter does not do.
pub(super) fn saved_commands(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Commands(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let title = match editor.mode {
        commands::Mode::Browse => crate::t!("Saved commands", "Commandes enregistrées").to_owned(),
        commands::Mode::Form => match &editor.original {
            Some(name) => crate::tf!(
                "Saved commands > Edit {name}",
                "Commandes enregistrées > Modifier {name}"
            ),
            None => crate::t!(
                "Saved commands > New command",
                "Commandes enregistrées > Nouvelle commande"
            )
            .to_owned(),
        },
        commands::Mode::Params => crate::t!(
            "Saved commands > Parameters",
            "Commandes enregistrées > Paramètres"
        )
        .to_owned(),
    };
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let chosen = m.commands.iter().nth(editor.selected);
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 3) as u16;
    let footer = match editor.mode {
        commands::Mode::Browse => crate::t!(
            "↑↓: command • Tab/←→: action • Enter: run the action • Esc: back",
            "↑↓ : commande • Tab/←→ : action • Entrée : lancer l'action • Échap : retour"
        ),
        commands::Mode::Form => crate::t!(
            "Tab/↑↓: field • Enter: next field • ←/→: scope or button • Ctrl+S: save • Esc: cancel",
            "Tab/↑↓ : champ • Entrée : champ suivant • ←/→ : portée ou bouton • Ctrl+S : enregistrer • Échap : annuler"
        ),
        commands::Mode::Params => crate::t!(
            "Tab/↑↓: value • Enter: next • ←/→: choice or button • Ctrl+L: continue to the review • Esc: back",
            "Tab/↑↓ : valeur • Entrée : suivant • ←/→ : choix ou bouton • Ctrl+L : passer à la vérification • Échap : retour"
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
    let row = |index: usize| Rect::new(body.x, body.y + index as u16, body.width, 1);
    let fits = |index: usize| index < usize::from(body.height);
    let label_style = |focused: bool| {
        if focused {
            accent()
        } else {
            Style::default()
        }
    };
    match editor.mode {
        commands::Mode::Browse => {
            let host = m.selected_host().map_or_else(
                || crate::t!("no host selected", "aucun hôte sélectionné").to_owned(),
                |host| host.name,
            );
            let context = crate::tf!(
                "Selected host: {host} • Batch: {} marked target(s)",
                "Hôte sélectionné : {host} • Lot : {} cible(s) marquée(s)",
                m.batch.marks.len()
            );
            let detail: Vec<String> = chosen.map_or_else(Vec::new, |(_, command)| {
                vec![
                    crate::tf!("Command: {}", "Commande : {}", command.template),
                    crate::tf!(
                        "Meant for: {}",
                        "Prévue pour : {}",
                        commands::scope_label(command.scope)
                    ),
                    commands::parameters_summary(&command.template).unwrap_or_else(|e| e),
                ]
            });
            let detail_rows: usize = detail
                .iter()
                .map(|line| select::wrapped_rows(line, width))
                .sum();
            let zones = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length((detail_rows as u16).min((body.height / 2).max(1))),
                Constraint::Length(2),
            ])
            .split(body);
            f.render_widget(Paragraph::new(context).style(accent()), zones[0]);
            let lines: Vec<Line<'_>> = if m.commands.is_empty() {
                vec![Line::styled(
                    crate::t!(
                        "No saved command. \"New\" keeps a command under a name, with the parameters it asks at each use. Using it only fills the batch preparation: you still review the targets and the command before anything runs.",
                        "Aucune commande enregistrée. « Nouvelle » garde une commande sous un nom, avec les paramètres qu'elle demande à chaque utilisation. L'utiliser remplit seulement la préparation du lot : vous vérifiez encore les cibles et la commande avant toute exécution."
                    ),
                    Style::default().fg(GRAY),
                )]
            } else {
                let height = usize::from(zones[1].height).max(1);
                let offset = (editor.selected + 1).saturating_sub(height);
                m.commands
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(height)
                    .map(|(index, (name, command))| {
                        let current = index == editor.selected;
                        let description = if command.description.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", command.description)
                        };
                        Line::styled(
                            format!("{} {name}{description}", if current { "▶" } else { " " }),
                            if current {
                                Style::default().fg(Color::Black).bg(CYAN)
                            } else {
                                Style::default()
                            },
                        )
                    })
                    .collect()
            };
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), zones[1]);
            f.render_widget(
                Paragraph::new(
                    detail
                        .into_iter()
                        .map(Line::from)
                        .collect::<Vec<Line<'_>>>(),
                )
                .style(Style::default().fg(GRAY))
                .wrap(Wrap { trim: true }),
                zones[2],
            );
            let rows =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(zones[3]);
            // Two rows of three, so that no button is cut at 80 columns.
            for (line, chunk) in commands::buttons().chunks(3).enumerate() {
                let mut spans = Vec::new();
                for (index, label) in chunk.iter().enumerate() {
                    if index > 0 {
                        spans.push(Span::raw("  "));
                    }
                    spans.push(nav::button(label, editor.button == line * 3 + index));
                }
                f.render_widget(Paragraph::new(Line::from(spans)), rows[line]);
            }
        }
        commands::Mode::Form => {
            let labels = [
                crate::t!("Name *", "Nom *"),
                crate::t!("Description", "Description"),
                crate::t!("Meant for", "Prévue pour"),
                crate::t!("Command *", "Commande *"),
            ];
            let label_width = labels.iter().map(|l| l.width()).max().unwrap_or(0) + 2;
            let prefix = label_width as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            for (stop, label) in labels.iter().enumerate() {
                if !fits(stop) {
                    break;
                }
                let focused = editor.focus == stop;
                let head = Span::styled(
                    format!("{} {label:<label_width$}", if focused { ">" } else { " " }),
                    label_style(focused),
                );
                let value = if stop == commands::SCOPE {
                    Span::raw(format!(
                        "‹ {} ›{}",
                        commands::scope_label(editor.scope),
                        if focused { "  ←/→" } else { "" }
                    ))
                } else {
                    let input = &editor.inputs[if stop == commands::TEMPLATE { 2 } else { stop }];
                    input_span(input, focused, room)
                };
                f.render_widget(Paragraph::new(Line::from(vec![head, value])), row(stop));
                if focused && stop != commands::SCOPE {
                    let input = &editor.inputs[if stop == commands::TEMPLATE { 2 } else { stop }];
                    input_cursor(f, input, row(stop), prefix, 0);
                }
            }
            let (summary, style) = match commands::parameters_summary(&editor.inputs[2].value) {
                Ok(text) => (text, Style::default()),
                Err(reason) if editor.inputs[2].value.trim().is_empty() => {
                    (reason, Style::default().fg(GRAY))
                }
                Err(reason) => (reason, Style::default().fg(Color::Red)),
            };
            let help = [
                crate::t!(
                    "Parameters: {name} text, {name=default}, {n:number=10}, {c:choice=a|b}; {{ and }} are literal braces.",
                    "Paramètres : {nom} texte, {nom=défaut}, {n:number=10}, {c:choice=a|b} ; {{ et }} sont des accolades littérales."
                ),
                crate::t!(
                    "Each value is inserted as one quoted word: do not put quotes around a parameter.",
                    "Chaque valeur est insérée comme un seul mot protégé : ne mettez pas de guillemets autour d'un paramètre."
                ),
                crate::t!(
                    "Never write a password or a secret here: commands are stored in clear text.",
                    "N'écrivez jamais de mot de passe ni de secret ici : les commandes sont enregistrées en clair."
                ),
            ];
            let mut lines = vec![Line::styled(summary, style)];
            lines.extend(
                help.iter()
                    .map(|line| Line::styled(*line, Style::default().fg(GRAY))),
            );
            let text_rows: usize = lines
                .iter()
                .map(|line| select::wrapped_rows(&line.to_string(), width))
                .sum();
            let top = 5u16;
            let available = body.height.saturating_sub(top + 2);
            let text_height = (text_rows as u16).min(available);
            if body.height > top {
                f.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }),
                    Rect::new(body.x, body.y + top, body.width, text_height),
                );
            }
            let buttons_y = (top + text_height + 1).min(body.height.saturating_sub(1));
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    nav::button(
                        crate::t!("Save", "Enregistrer"),
                        editor.focus == commands::SAVE,
                    ),
                    Span::raw("  "),
                    nav::button(
                        crate::t!("Cancel", "Annuler"),
                        editor.focus == commands::CANCEL,
                    ),
                ])),
                Rect::new(body.x, body.y + buttons_y, body.width, 1),
            );
        }
        commands::Mode::Params => {
            let Some((name, command)) = chosen else {
                return;
            };
            let target = if editor.batch {
                crate::tf!(
                    "the batch ({} marked target(s))",
                    "le lot ({} cible(s) marquée(s))",
                    m.batch.marks.len()
                )
            } else {
                m.selected_host().map_or_else(String::new, |host| host.name)
            };
            f.render_widget(
                Paragraph::new(crate::tf!(
                    "{name} — for {target}",
                    "{name} — pour {target}"
                ))
                .style(accent()),
                row(0),
            );
            let label_width = editor
                .params
                .iter()
                .map(|p| p.name.width() + commands::kind_label(&p.kind).width() + 3)
                .max()
                .unwrap_or(0)
                .min(width / 2)
                + 2;
            let prefix = label_width as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            let count = editor.params.len();
            for (index, (parameter, input)) in editor.params.iter().zip(&editor.values).enumerate()
            {
                let y = index + 2;
                if !fits(y) {
                    break;
                }
                let focused = editor.param_focus == index;
                let label = format!(
                    "{} ({})",
                    parameter.name,
                    commands::kind_label(&parameter.kind)
                );
                let head = Span::styled(
                    format!(
                        "{} {label:<label_width$.label_width$}",
                        if focused { ">" } else { " " }
                    ),
                    label_style(focused),
                );
                let choice = matches!(parameter.kind, crate::storage::commands::Kind::Choice(_));
                let value = if choice {
                    Span::raw(format!(
                        "‹ {} ›{}",
                        input.value,
                        if focused { "  ←/→" } else { "" }
                    ))
                } else {
                    input_span(input, focused, room)
                };
                f.render_widget(Paragraph::new(Line::from(vec![head, value])), row(y));
                if focused && !choice {
                    input_cursor(f, input, row(y), prefix, 0);
                }
            }
            let (result, style) = match editor.rendered(&command.template) {
                Ok(text) => (
                    crate::tf!("Command to review: {text}", "Commande à vérifier : {text}"),
                    Style::default(),
                ),
                Err(reason) => (reason, Style::default().fg(Color::Yellow)),
            };
            let result_y = (count + 3) as u16;
            let result_rows = (select::wrapped_rows(&result, width) as u16).min(4);
            if body.height > result_y + 1 {
                let height = result_rows.min(body.height - result_y - 1);
                f.render_widget(
                    Paragraph::new(result)
                        .style(style)
                        .wrap(Wrap { trim: true }),
                    Rect::new(body.x, body.y + result_y, body.width, height),
                );
            }
            let buttons_y = (result_y + result_rows + 1).min(body.height.saturating_sub(1));
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    nav::button(
                        crate::t!("Continue to the review", "Passer à la vérification"),
                        editor.param_focus == count,
                    ),
                    Span::raw("  "),
                    nav::button(crate::t!("Back", "Retour"), editor.param_focus == count + 1),
                ])),
                Rect::new(body.x, body.y + buttons_y, body.width, 1),
            );
        }
    }
    match editor.pending {
        Some(commands::Pending::Delete) => {
            if let Some((name, _)) = chosen {
                m.confirm.draw(
                    f,
                    whole,
                    crate::t!("Delete this command?", "Supprimer cette commande ?"),
                    vec![
                        Line::from(name.clone()),
                        Line::from(crate::t!(
                            "Only the saved command is removed; nothing is run and no host changes.",
                            "Seule la commande enregistrée est retirée ; rien n'est exécuté et aucun hôte ne change."
                        )),
                    ],
                    crate::t!("Go back", "Revenir"),
                    crate::t!("Delete the command", "Supprimer la commande"),
                );
            }
        }
        Some(commands::Pending::Discard) => {
            m.confirm.draw(
                f,
                whole,
                crate::t!("Unsaved changes", "Modifications non enregistrées"),
                vec![Line::from(crate::t!(
                    "Discard the changes of this command?",
                    "Abandonner les modifications de cette commande ?"
                ))],
                crate::t!("Keep editing", "Continuer l'édition"),
                crate::t!("Discard", "Abandonner"),
            );
        }
        None => {}
    }
}
