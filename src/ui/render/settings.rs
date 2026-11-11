//! Rendering of the preferences screen and of its shortcut editor.
use super::*;

/// Preferences: sections of options with their value, the effect of the focused
/// one, then the buttons. The draft is only written by Apply.
pub(super) fn preferences(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Preferences(editor) = &m.view else {
        return;
    };
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(crate::t!("Preferences", "Préférences"), true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let all = prefs::rows();
    let focused = all[editor.row.min(all.len() - 1)];
    let width = usize::from(inner.width).max(1);
    // A value that the row may cut (a path, the launcher) is repeated in full.
    let mut detail: Vec<String> = Vec::new();
    if !focused.editable() {
        detail.push(focused.value(m, editor));
    }
    detail.extend(focused.detail().iter().map(|line| (*line).to_owned()));
    let detail_rows: usize = detail
        .iter()
        .map(|line| select::wrapped_rows(line, width))
        .sum();
    let detail_height = (detail_rows as u16).min((inner.height / 3).max(1));
    let footer = crate::t!(
        "↑↓: option • Space/←→: change • Enter: change or open • Tab: buttons • Ctrl+S: apply • Esc: cancel",
        "↑↓ : option • Espace/←→ : changer • Entrée : changer ou ouvrir • Tab : boutons • Ctrl+S : appliquer • Échap : annuler"
    );
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(detail_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let label_width = all.iter().map(|row| row.label().width()).max().unwrap_or(0) + 2;
    let mut lines = Vec::new();
    let mut focus_y = 0;
    for (title, rows) in prefs::sections() {
        lines.push(Line::styled(title, accent()));
        for row in rows {
            let on_row = *row == focused && editor.button.is_none();
            if *row == focused {
                focus_y = lines.len();
            }
            let label = format!(
                "{} {:<label_width$}",
                if on_row { ">" } else { " " },
                row.label()
            );
            let mut spans = vec![
                Span::styled(label, if on_row { accent() } else { Style::default() }),
                Span::styled(
                    // A long path keeps its end, where the file name is; the
                    // focused row repeats it in full under the list.
                    group_label(&row.value(m, editor), width.saturating_sub(label_width + 2)),
                    if row.editable() {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(GRAY)
                    },
                ),
            ];
            if editor.changed(*row) {
                spans.push(Span::styled(
                    crate::t!("  ● modified", "  ● modifié"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            lines.push(Line::from(spans));
        }
    }
    let mut scroll = editor.scroll;
    // The title of a section stays above its first option.
    scroll_focus(
        &mut scroll,
        focus_y.saturating_sub(1),
        parts[0].height,
        lines.len(),
        true,
    );
    f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), parts[0]);
    f.render_widget(
        Paragraph::new(
            detail
                .into_iter()
                .map(Line::from)
                .collect::<Vec<Line<'_>>>(),
        )
        .style(Style::default().fg(GRAY))
        .wrap(Wrap { trim: true }),
        parts[1],
    );
    let mut buttons = Vec::new();
    for (index, label) in prefs::buttons().into_iter().enumerate() {
        if index > 0 {
            buttons.push(Span::raw("  "));
        }
        buttons.push(nav::button(label, editor.button == Some(index)));
    }
    f.render_widget(Paragraph::new(Line::from(buttons)), parts[2]);
    let state = if !editor.notice.is_empty() {
        Span::styled(editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else if editor.dirty() {
        Span::styled(
            crate::t!(
                "● Unsaved changes — nothing is written before Apply",
                "● Modifications non enregistrées — rien n'est écrit avant Appliquer"
            ),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(
            crate::t!("No change", "Aucune modification"),
            Style::default().fg(GRAY),
        )
    };
    f.render_widget(Paragraph::new(Line::from(state)), parts[3]);
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    if let View::Preferences(editor) = &mut m.view {
        editor.scroll = scroll;
    }
}

/// Shortcut editor: one row per action with its keys and where they come from.
pub(super) fn shortcuts(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Shortcuts(editor) = &m.view else {
        return;
    };
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(
        crate::t!("Preferences > Shortcuts", "Préférences > Raccourcis"),
        true,
    );
    let inner = border.inner(area);
    f.render_widget(border, area);
    let footer = if editor.capture {
        crate::t!(
            "Waiting for a key — Esc: keep the current one",
            "En attente d'une touche — Échap : garder l'actuelle"
        )
    } else {
        crate::t!(
            "↑↓: action • Tab/←→: buttons • Enter: run the button • Ctrl+S: done • Esc: cancel",
            "↑↓ : action • Tab/←→ : boutons • Entrée : lancer le bouton • Ctrl+S : terminer • Échap : annuler"
        )
    };
    let footer_height = select::wrapped_rows(footer, usize::from(inner.width).max(1)).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let keymap = Keymap::new(&editor.bindings);
    let actions = prefs::shortcut_actions();
    let selected = editor.selected.min(actions.len() - 1);
    let label_width = actions
        .iter()
        .map(|action| action.label().width())
        .max()
        .unwrap_or(0)
        + 2;
    let key_width = 22;
    f.render_widget(
        Paragraph::new(format!(
            "  {:<label_width$}{:<key_width$}{}",
            "Action",
            crate::t!("Keys", "Touches"),
            crate::t!("Origin", "Origine")
        ))
        .style(accent()),
        parts[0],
    );
    let height = usize::from(parts[1].height).max(1);
    let mut offset = editor.offset.min(actions.len().saturating_sub(height));
    if selected < offset {
        offset = selected;
    } else if selected >= offset + height {
        offset = selected + 1 - height;
    }
    let lines: Vec<Line<'_>> = actions
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, action)| {
            let keys = keymap.keys(*action);
            let keys = if keys.is_empty() {
                "—".to_owned()
            } else {
                keys.join(", ")
            };
            let current = index == selected;
            let text = format!(
                "{} {:<label_width$}{:<key_width$}{}",
                if current { "▶" } else { " " },
                action.label(),
                keys,
                editor.state(*action)
            );
            Line::styled(
                text,
                if current && editor.button.is_none() {
                    Style::default().fg(Color::Black).bg(CYAN)
                } else if current {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), parts[1]);
    let notice = if editor.notice.is_empty() {
        Span::styled(
            crate::tf!(
                "{}/{} — Enter: press a new key for the selected action",
                "{}/{} — Entrée : saisir une nouvelle touche pour l'action sélectionnée",
                selected + 1,
                actions.len()
            ),
            Style::default().fg(GRAY),
        )
    } else {
        Span::styled(
            editor.notice.clone(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };
    f.render_widget(
        Paragraph::new(Line::from(notice)).wrap(Wrap { trim: true }),
        parts[2],
    );
    let mut buttons = Vec::new();
    for (index, label) in prefs::shortcut_buttons().into_iter().enumerate() {
        if index > 0 {
            buttons.push(Span::raw("  "));
        }
        buttons.push(nav::button(label, editor.button == Some(index)));
    }
    f.render_widget(Paragraph::new(Line::from(buttons)), parts[3]);
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    if let View::Shortcuts(editor) = &mut m.view {
        editor.offset = offset;
    }
}
