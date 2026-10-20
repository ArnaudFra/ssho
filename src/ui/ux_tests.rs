//! Interface journeys: common keyboard, focus and
//! return rules checked through the same entry points as the terminal.
use super::tests::{key, screen, typing};
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn hosts() -> Vec<Host> {
    ["alpha", "beta"]
        .into_iter()
        .map(|name| Host {
            name: name.into(),
            hostname: format!("{name}.example"),
            ..Host::default()
        })
        .collect()
}

/// Table focus by default; `preferences` receives the `key_bindings` object.
fn model(key_bindings: serde_json::Value) -> Model {
    let preferences: AppConfig =
        serde_json::from_value(serde_json::json!({ "key_bindings": key_bindings })).unwrap();
    let mut m = Model::new(hosts(), preferences, BTreeMap::new(), false);
    m.search_focus = false;
    m
}

#[test]
fn prefix_sequence_runs_named_letters_and_ignores_unknown_ones() {
    let mut m = model(serde_json::json!({}));
    assert_eq!(m.key(ctrl('g')), Action::None);
    assert!(matches!(m.view, View::Quick { notice: None }));
    let text = screen(&mut m, 100, 30);
    for expected in [
        "Actions rapides — alpha",
        "a Ajouter",
        "d Dupliquer",
        "? Aide",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // An unknown letter runs nothing; the panel has no timeout and stays open.
    assert_eq!(m.key(key(KeyCode::Char('z'))), Action::None);
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::Quick { .. }));
    // Inside the sequence `d` duplicates; the direct `d` still asks to delete.
    assert!(matches!(
        m.key(key(KeyCode::Char('d'))),
        Action::Duplicate(_)
    ));
    assert!(matches!(m.view, View::List));
    assert!(matches!(m.key(key(KeyCode::Char('d'))), Action::Delete(_)));
    m.key(ctrl('g'));
    assert_eq!(m.key(key(KeyCode::Char('a'))), Action::Add(String::new()));
    for cancel in [key(KeyCode::Esc), ctrl('c'), ctrl('g')] {
        m.key(ctrl('g'));
        assert!(matches!(m.view, View::Quick { .. }));
        assert_eq!(m.key(cancel), Action::None);
        assert!(matches!(m.view, View::List));
    }
}

#[test]
fn prefix_explains_an_unavailable_letter_without_running_it() {
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    m.key(ctrl('g'));
    assert_eq!(m.key(key(KeyCode::Char('e'))), Action::None);
    assert!(matches!(m.view, View::Quick { notice: Some(_) }));
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Indisponible : Sélectionnez un hôte"),
        "{text}"
    );
    assert!(text.contains("aucun hôte sélectionné"), "{text}");
    // Actions that need no host remain reachable from the same panel.
    assert_eq!(m.key(key(KeyCode::Char('a'))), Action::Add(String::new()));
}

#[test]
fn prefix_from_search_keeps_the_query_and_the_focus() {
    let mut m = model(serde_json::json!({}));
    m.search_focus = true;
    typing(&mut m, "alp");
    m.key(ctrl('g'));
    assert!(matches!(m.view, View::Quick { .. }));
    m.paste("a");
    assert!(
        matches!(m.view, View::Quick { .. }),
        "paste is never a command"
    );
    m.key(key(KeyCode::Esc));
    assert!(m.search_focus);
    assert_eq!(m.query.value, "alp");
    typing(&mut m, "ha");
    assert_eq!(m.query.value, "alpha");
}

#[test]
fn prefix_on_an_editing_chord_never_swallows_text_editing() {
    let mut m = model(serde_json::json!({"prefix_key": "Ctrl+A"}));
    m.search_focus = true;
    typing(&mut m, "alpha");
    assert_eq!(m.key(ctrl('a')), Action::None);
    assert!(matches!(m.view, View::List));
    assert_eq!(
        m.query.cursor, 0,
        "Ctrl+A still moves to the start of the input"
    );
    m.search_focus = false;
    m.key(ctrl('a'));
    assert!(matches!(m.view, View::Quick { .. }));
    m.key(key(KeyCode::Esc));
    // The restriction is explained, not silent.
    let text = screen(&mut m, 120, 30);
    assert!(
        text.contains("Raccourcis : 1 préférence(s) refusée(s)"),
        "{text}"
    );
}

#[test]
fn prefix_can_be_disabled_and_every_action_stays_reachable() {
    let mut m = model(serde_json::json!({"prefix_key": ""}));
    assert_eq!(m.key(ctrl('g')), Action::None);
    assert!(matches!(m.view, View::List));
    assert!(!screen(&mut m, 120, 30).contains("quick"));
    // Every palette action stays reachable through F2 without any prefix.
    m.key(key(KeyCode::F(2)));
    assert!(matches!(m.view, View::Actions { .. }));
    let enabled = model(serde_json::json!({}));
    let mut enabled = enabled;
    assert!(screen(&mut enabled, 120, 30).contains("Ctrl+G: quick"));
}

#[test]
fn help_opens_with_f1_or_question_mark_and_h_remains() {
    for open in [KeyCode::F(1), KeyCode::Char('?'), KeyCode::Char('h')] {
        let mut m = model(serde_json::json!({}));
        m.key(key(open));
        assert!(matches!(m.view, View::Help), "{open:?}");
        m.key(key(open));
        assert!(
            matches!(m.view, View::List),
            "{open:?} closes the help it opened"
        );
    }
    // In the search input a question mark is text; F1 is not a character.
    let mut m = model(serde_json::json!({}));
    m.search_focus = true;
    m.key(key(KeyCode::Char('?')));
    assert_eq!(m.query.value, "?");
    assert!(matches!(m.view, View::List));
    m.key(key(KeyCode::F(1)));
    assert!(matches!(m.view, View::Help));
    let text = screen(&mut m, 120, 50);
    assert!(text.contains("?, F1, h  Aide"), "{text}");
    assert!(text.contains("Ctrl+G    Actions rapides"), "{text}");
}

#[test]
fn configured_shortcut_updates_dispatch_palette_footer_and_help() {
    let mut m = model(serde_json::json!({"shortcuts": {"host.edit": ["E"]}}));
    assert_eq!(m.key(key(KeyCode::Char('e'))), Action::None);
    assert!(
        matches!(m.view, View::List),
        "the released key runs nothing"
    );
    let footer = screen(&mut m, 120, 30);
    assert!(footer.contains("E: edit"), "{footer}");
    m.key(key(KeyCode::Char('E')));
    assert!(matches!(m.view, View::Host(_)));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "modifier");
    let palette = screen(&mut m, 120, 30);
    let row = palette
        .lines()
        .find(|line| line.contains("▶"))
        .expect("selected palette row");
    assert!(row.contains("Modifier") && row.contains(" E "), "{row}");
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(1)));
    assert!(screen(&mut m, 120, 50).contains("E         Modifier"));
}

#[test]
fn conflicting_preferences_are_refused_and_explained() {
    let mut m = model(serde_json::json!({
        "shortcuts": {"host.edit": ["d"], "unknown.action": ["z"]}
    }));
    // The historical meaning of `d` is never changed silently.
    assert!(matches!(m.key(key(KeyCode::Char('d'))), Action::Delete(_)));
    let list = screen(&mut m, 120, 30);
    assert!(
        list.contains("Raccourcis : 2 préférence(s) refusée(s)"),
        "{list}"
    );
    // Compact terminals keep a badge instead of dropping the warning.
    m.error = "boom".into();
    let compact = screen(&mut m, 80, 10);
    assert!(compact.contains("Raccourcis refusés"), "{compact}");
    m.key(key(KeyCode::F(1)));
    for _ in 0..40 {
        m.key(key(KeyCode::Down));
    }
    let help = screen(&mut m, 120, 50);
    assert!(help.contains("d sert déjà à « Supprimer »"), "{help}");
    assert!(
        help.contains("shortcuts.unknown.action : action inconnue"),
        "{help}"
    );
}

#[test]
fn escape_walks_back_to_the_list_and_never_closes_it() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('i')));
    m.key(key(KeyCode::Char('e')));
    m.key(key(KeyCode::F(7)));
    assert_eq!(m.parents.len(), 3);
    for expected in [2, 1, 0] {
        assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
        assert_eq!(m.parents.len(), expected);
    }
    assert!(matches!(m.view, View::List));
    // At the root Esc explains how to quit instead of closing the application.
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(matches!(m.view, View::List));
    assert!(screen(&mut m, 100, 20).contains("Esc ne quitte pas la liste"));
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::Quit);
}

#[test]
fn ctrl_c_follows_escape_in_every_secondary_screen() {
    // Each opener leaves the list; Ctrl+C must return to it without quitting.
    let openers: [&[KeyCode]; 9] = [
        &[KeyCode::F(2)],
        &[KeyCode::F(7)],
        &[KeyCode::Char('i')],
        &[KeyCode::Char('?')],
        &[KeyCode::Char('g')],
        &[KeyCode::Char('P')],
        &[KeyCode::Char('v')],
        &[KeyCode::Char('N')],
        &[KeyCode::Char('B')],
    ];
    for opener in openers {
        let mut m = model(serde_json::json!({}));
        for code in opener {
            m.key(key(*code));
        }
        assert!(!matches!(m.view, View::List), "{opener:?} opens a screen");
        assert_eq!(m.key(ctrl('c')), Action::None, "{opener:?}");
        assert!(
            matches!(m.view, View::List),
            "{opener:?} returns to the list"
        );
        assert!(!m.quit_tunnels && !m.discard, "{opener:?}");
    }
    // The key details opened by F4 close first, then the key screen itself.
    let mut m = model(serde_json::json!({}));
    keys::open_view(&mut m, false);
    m.key(key(KeyCode::F(4)));
    assert_eq!(m.key(ctrl('c')), Action::None);
    assert!(matches!(m.view, View::Keys(_)), "one level at a time");
    assert_eq!(m.key(ctrl('c')), Action::None);
    assert!(matches!(m.view, View::List));
}

#[test]
fn simple_selectors_wrap_while_the_host_table_stays_bounded() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('g')));
    m.key(key(KeyCode::Up));
    assert!(matches!(m.view, View::GroupMode { selected } if selected == groups::MODES.len() - 1));
    m.key(key(KeyCode::Down));
    assert!(matches!(m.view, View::GroupMode { selected: 0 }));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Char('P')));
    m.key(key(KeyCode::Up));
    assert!(matches!(m.view, View::Availability { selected: 1 }));
    m.key(key(KeyCode::Down));
    assert!(matches!(m.view, View::Availability { selected: 0 }));
    m.key(key(KeyCode::Esc));
    m.view = View::SourceFiles {
        files: vec!["/one".into(), "/two".into()],
        selected: 0,
    };
    m.key(key(KeyCode::Up));
    assert!(matches!(m.view, View::SourceFiles { selected: 2, .. }));
    m.key(key(KeyCode::Down));
    assert!(matches!(m.view, View::SourceFiles { selected: 0, .. }));
    m.view = View::Files {
        files: vec!["/one".into(), "/two".into(), "/three".into()],
        selected: 2,
        host: None,
        seed: String::new(),
        duplicate: None,
    };
    m.key(key(KeyCode::Down));
    assert!(matches!(m.view, View::Files { selected: 0, .. }));
    m.key(key(KeyCode::End));
    assert!(matches!(m.view, View::Files { selected: 2, .. }));
    // The host table keeps its ends as spatial landmarks.
    m.view = View::List;
    m.key(key(KeyCode::Up));
    assert_eq!(m.selected, 0);
    m.key(key(KeyCode::End));
    m.key(key(KeyCode::Down));
    assert_eq!(m.selected, m.rows.len() - 1);
}

#[test]
fn modified_draft_is_kept_until_discard_is_chosen_explicitly() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('e')));
    typing(&mut m, "-draft");
    let draft = format!("{:?}", m.view);
    // Esc, Ctrl+C, Enter on the default choice and unrelated keys all keep it.
    for keep in [key(KeyCode::Esc), ctrl('c'), key(KeyCode::Enter)] {
        m.key(key(KeyCode::Esc));
        assert!(m.discard);
        m.key(key(KeyCode::Char('y')));
        assert!(m.discard, "an unrelated key decides nothing");
        m.key(keep);
        assert!(!m.discard);
        assert_eq!(format!("{:?}", m.view), draft);
    }
    // A previous visit never leaves the destructive choice focused.
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Esc));
    assert!(screen(&mut m, 100, 24).contains("[▶ Continuer l'édition ]"));
    m.key(key(KeyCode::Right));
    assert!(screen(&mut m, 100, 24).contains("[▶ Abandonner ]"));
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::List));
}

#[test]
fn quit_request_summarises_owned_work_and_names_both_decisions() {
    use runtime::tunnels::{Record, Snapshot, State};
    let mut m = model(serde_json::json!({}));
    let record = |id, state| Record {
        id,
        snapshot: Snapshot {
            config: "/config".into(),
            host: "alpha".into(),
            profile: None,
            forward: Forward::default(),
        },
        state,
        exit_code: None,
        diagnostic: String::new(),
    };
    m.tunnels = vec![
        record(1, State::Active),
        record(2, State::Stopped),
        record(3, State::Active),
    ];
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::Quit);
    m.ask_quit();
    let text = screen(&mut m, 100, 20);
    assert!(
        text.contains("2 tunnel(s) actif(s) : ils seront arrêtés"),
        "{text}"
    );
    assert!(
        text.contains("[▶ Revenir ]") && text.contains("[  Arrêter et quitter ]"),
        "{text}"
    );
    assert!(
        !text.contains("Lot en cours") && !text.contains("non enregistrées"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(!m.quit_tunnels, "the default decision returns to the list");
    m.ask_quit();
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Quit);
}

fn palette_state(m: &Model) -> &palette::State {
    let View::Actions(state) = &m.view else {
        panic!("the palette is open")
    };
    state
}

#[test]
fn palette_wraps_at_both_ends_over_displayed_actions() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::F(2)));
    let entries = palette_state(&m).entries();
    assert_eq!(entries.len(), PaletteAction::CATALOGUE.len() - 1);
    assert_eq!(palette_state(&m).selected, entries.first().copied());
    m.key(key(KeyCode::Up));
    assert_eq!(palette_state(&m).selected, entries.last().copied());
    assert!(
        screen(&mut m, 100, 30).contains("Quitter"),
        "the last action is scrolled in"
    );
    m.key(key(KeyCode::Down));
    assert_eq!(palette_state(&m).selected, entries.first().copied());
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Connexion") && text.contains("Se connecter"),
        "{text}"
    );
    m.key(key(KeyCode::End));
    assert_eq!(palette_state(&m).selected, entries.last().copied());
    m.key(key(KeyCode::Home));
    assert_eq!(palette_state(&m).selected, entries.first().copied());
    // The wrap follows the filtered list, not the whole catalogue.
    typing(&mut m, "tunnel");
    let filtered = palette_state(&m).entries();
    assert_eq!(
        filtered,
        [
            PaletteAction::Tunnel,
            PaletteAction::Profiles,
            PaletteAction::Active
        ]
    );
    m.key(key(KeyCode::Up));
    assert_eq!(palette_state(&m).selected, Some(PaletteAction::Active));
    m.key(key(KeyCode::Down));
    assert_eq!(palette_state(&m).selected, Some(PaletteAction::Tunnel));
}

#[test]
fn search_ignores_case_and_accents_and_never_runs_on_its_own() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::F(2)));
    // Label words, synonyms and categories all find an action.
    for (words, expected) in [
        ("CLES", PaletteAction::Keys),
        ("methode", PaletteAction::Availability),
        ("forwarding socks", PaletteAction::Tunnel),
        ("copier", PaletteAction::Duplicate),
        ("effacer filtre fichier", PaletteAction::ClearSourceFilter),
        ("  restaurer ", PaletteAction::Backups),
    ] {
        typing(&mut m, words);
        assert_eq!(palette_state(&m).selected, Some(expected), "{words}");
        m.key(ctrl('u'));
    }
    // Shortcut letters are search text here: nothing is triggered or deleted.
    typing(&mut m, "dq");
    assert!(matches!(m.view, View::Actions(_)));
    assert_eq!(palette_state(&m).query.value, "dq");
    assert!(palette_state(&m).entries().is_empty());
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(screen(&mut m, 100, 30).contains("Aucune action ne correspond"));
    m.key(ctrl('u'));
    // A single match is selected but still waits for Enter.
    typing(&mut m, "recharger");
    assert_eq!(palette_state(&m).entries(), [PaletteAction::Reload]);
    assert!(matches!(m.view, View::Actions(_)));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Reload);
    assert!(matches!(m.view, View::List));
}

#[test]
fn columns_separate_shortcut_and_state_and_fit_eighty_columns() {
    let mut m = model(serde_json::json!({}));
    m.favorites_only = false;
    m.show_hidden = true;
    m.key(key(KeyCode::F(2)));
    let text = screen(&mut m, 80, 24);
    for expected in ["Action", "Raccourci", "État", "Rechercher une action :"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // No label is truncated at 80 columns, and none carries its key.
    let keymap = m.keymap();
    for action in palette_state(&m).entries() {
        assert_eq!(palette_state(&m).selected, Some(action));
        let text = screen(&mut m, 80, 24);
        let row = text
            .lines()
            .find(|line| line.contains('▶'))
            .unwrap_or_else(|| panic!("{action:?} is selected: {text}"));
        assert!(row.contains(action.label()), "{action:?} truncated: {row}");
        if let Some(shortcut) = keymap.shortcut(action) {
            assert!(row.contains(&format!(" {shortcut} ")), "{action:?}: {row}");
        } else {
            assert!(row.contains(" — "), "{action:?} has no direct key: {row}");
        }
        m.key(key(KeyCode::Down));
    }
    typing(&mut m, "masques");
    let text = screen(&mut m, 80, 24);
    assert!(
        text.lines()
            .any(|l| l.contains('▶') && l.contains("activé")),
        "{text}"
    );
}

#[test]
fn unavailable_action_explains_its_reason_and_keeps_the_palette_open() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::F(2)));
    for (words, reason) in [
        ("tous les fichiers", "Aucun filtre de fichier actif"),
        ("effacer les marques", "Aucune cible marquée"),
        ("derniers resultats", "Aucun lot lancé"),
    ] {
        typing(&mut m, words);
        let before = screen(&mut m, 100, 30);
        assert!(
            before.contains(&format!("Indisponible : {reason}.")),
            "{before}"
        );
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None, "{words}");
        assert!(matches!(m.view, View::Actions(_)), "{words}");
        let after = screen(&mut m, 100, 30);
        assert!(after.contains("Rien n'a été lancé."), "{after}");
        m.key(ctrl('u'));
    }
    assert!(m.source_filter.is_none() && m.batch.marks.is_empty());
    // A group header is not a host: the reason says so.
    let mut grouped = model(serde_json::json!({}));
    grouped.preferences.group_mode = GroupMode::Files;
    grouped.filter(false);
    grouped.selected = 0;
    assert!(grouped.selected_group().is_some());
    grouped.key(key(KeyCode::F(2)));
    typing(&mut grouped, "modifier");
    assert!(screen(&mut grouped, 100, 30).contains("Sélectionnez un hôte, pas un groupe"));
}

#[test]
fn selection_follows_the_action_identity_through_filtering() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "tunnel");
    m.key(key(KeyCode::Down));
    assert_eq!(palette_state(&m).selected, Some(PaletteAction::Profiles));
    // Narrowing keeps the same action selected while it still matches.
    typing(&mut m, " profils");
    assert_eq!(palette_state(&m).selected, Some(PaletteAction::Profiles));
    for _ in 0..8 {
        m.key(key(KeyCode::Backspace));
    }
    assert_eq!(palette_state(&m).query.value, "tunnel");
    assert_eq!(palette_state(&m).selected, Some(PaletteAction::Profiles));
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Profiles(_)));
}

#[test]
fn contextual_palette_keeps_the_draft_and_offers_compatible_actions() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('e')));
    typing(&mut m, "-draft");
    let draft = format!("{:?}", m.view);
    m.key(key(KeyCode::F(2)));
    assert_eq!(
        palette_state(&m).entries(),
        [PaletteAction::Active, PaletteAction::Help],
        "a draft only offers screens that return to it"
    );
    let text = screen(&mut m, 100, 30);
    assert!(text.contains("Actions — brouillon conservé"), "{text}");
    assert!(
        text.contains("alpha-draft"),
        "the draft stays visible underneath: {text}"
    );
    typing(&mut m, "aide");
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Help));
    m.key(key(KeyCode::Esc));
    assert_eq!(format!("{:?}", m.view), draft);
    m.key(key(KeyCode::F(2)));
    m.key(key(KeyCode::F(2)));
    assert_eq!(
        format!("{:?}", m.view),
        draft,
        "F2 closes the palette it opened"
    );
    // From a host sheet, the palette acts on that host.
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('i')));
    m.key(key(KeyCode::F(2)));
    let entries = palette_state(&m).entries();
    assert!(entries.contains(&PaletteAction::Edit) && entries.contains(&PaletteAction::Connect));
    assert!(
        !entries.contains(&PaletteAction::GroupMode) && !entries.contains(&PaletteAction::Quit)
    );
    typing(&mut m, "connecter");
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), None)
    );
}

#[test]
fn no_inactive_input_remains_after_tab_and_the_filter_stays_visible() {
    let mut m = model(serde_json::json!({}));
    m.search_focus = true;
    let editing = screen(&mut m, 100, 24);
    assert!(
        editing.contains("● Recherche") && editing.contains("Search: > "),
        "{editing}"
    );
    assert!(
        !editing.contains("● Hôtes"),
        "one element has the focus: {editing}"
    );
    typing(&mut m, "alp");
    m.key(key(KeyCode::Left));
    m.key(key(KeyCode::Tab));
    let browsing = screen(&mut m, 100, 24);
    // No boxed input is left behind; the table is named as the focused element.
    assert!(
        !browsing.contains("Search: > ") && !browsing.contains("Recherche──"),
        "{browsing}"
    );
    assert!(browsing.contains("● Hôtes"), "{browsing}");
    assert!(browsing.contains("Filtres : alp"), "{browsing}");
    assert!(
        browsing.contains("(/ : modifier • Backspace : effacer)"),
        "{browsing}"
    );
    assert!(browsing.contains("1 hôte sur 2"), "{browsing}");
    assert_eq!(m.visible.len(), 1);
    // Going back to the search finds the text and its cursor again.
    m.key(key(KeyCode::Char('/')));
    typing(&mut m, "X");
    assert_eq!(m.query.value, "alXp");
}

#[test]
fn escape_closes_the_edit_and_clearing_is_an_explicit_action() {
    let mut m = model(serde_json::json!({}));
    m.search_focus = true;
    typing(&mut m, "beta");
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.search_focus);
    assert_eq!(m.query.value, "beta", "Esc never erases the filter");
    assert_eq!(m.selected_host().unwrap().name, "beta");
    // In the input Backspace edits; from the table it clears the whole search.
    m.key(key(KeyCode::Char('/')));
    m.key(key(KeyCode::Backspace));
    assert_eq!(m.query.value, "bet");
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Backspace));
    assert!(m.query.value.is_empty());
    assert_eq!(m.visible.len(), 2);
    assert!(!m.search_focus);
    // The same action exists in the palette and explains when it has nothing to do.
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "effacer la recherche");
    assert!(screen(&mut m, 100, 30).contains("Indisponible : Aucune recherche active."));
    m.key(key(KeyCode::Esc));
    m.query.set("alpha");
    m.filter(true);
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "effacer la recherche");
    m.key(key(KeyCode::Enter));
    assert!(m.query.value.is_empty() && matches!(m.view, View::List));
}

#[test]
fn band_lists_every_active_criterion_in_both_focus_states() {
    let mut m = model(serde_json::json!({}));
    m.all[0].source_file = "/etc/ssh/work".into();
    m.all[1].source_file = "/etc/ssh/home".into();
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            favorite: true,
            ..HostMetadata::default()
        },
    );
    m.query.set("alp");
    m.source_filter = Some("/etc/ssh/work".into());
    m.favorites_only = true;
    m.show_hidden = true;
    m.preferences.group_mode = GroupMode::Files;
    m.filter(true);
    for focus in [false, true] {
        m.search_focus = focus;
        let text = screen(&mut m, 120, 30);
        for criterion in [
            "alp",
            "Favoris seuls",
            "Masqués visibles (H)",
            "Groupes : ",
            "Fichier : /etc/ssh/work",
            "(C/F2 : tous les fichiers)",
            "1 hôte sur 2",
        ] {
            assert!(
                text.contains(criterion),
                "focus={focus}: missing {criterion}: {text}"
            );
        }
    }
    // Compact terminals keep the file scope and its clear key on the single row.
    m.search_focus = false;
    let compact = screen(&mut m, 60, 10);
    assert!(
        compact.contains("Filtres : Fichier (C/F2) : /etc/ssh/work"),
        "{compact}"
    );
    // One action resets every filter; grouping is a presentation and is kept.
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "effacer tous les filtres");
    m.key(key(KeyCode::Enter));
    assert!(m.query.value.is_empty() && m.source_filter.is_none());
    assert!(!m.favorites_only && !m.show_hidden);
    assert_eq!(m.preferences.group_mode, GroupMode::Files);
    let text = screen(&mut m, 120, 30);
    assert!(
        !text.contains("Filtres :") && text.contains("/ Rechercher"),
        "{text}"
    );
    assert!(text.contains("2 hôtes"), "{text}");
}

#[test]
fn count_names_hosts_and_never_counts_group_headers() {
    let mut m = model(serde_json::json!({}));
    m.all[0].source_file = "/one".into();
    m.all[1].source_file = "/two".into();
    m.preferences.group_mode = GroupMode::Files;
    m.filter(false);
    assert_eq!(m.rows.len(), 4, "two headers and two hosts are displayed");
    assert!(screen(&mut m, 120, 30).contains("2 hôtes"));
    m.query.set("alpha");
    m.filter(true);
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("1 hôte sur 2"), "{text}");
    // A syntax error keeps its explanation next to the search.
    m.query.set("tag:\"");
    m.filter(true);
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("Search: Unclosed quote"), "{text}");
    assert!(text.contains("Filtres : tag:\""), "{text}");
}

#[test]
fn editing_keys_stay_text_and_shortcuts_stay_out_of_the_input() {
    let mut m = model(serde_json::json!({}));
    m.search_focus = true;
    typing(&mut m, "adDq/?");
    assert_eq!(m.query.value, "adDq/?");
    m.key(key(KeyCode::Backspace));
    m.key(key(KeyCode::Delete));
    assert_eq!(
        m.query.value, "adDq/",
        "Backspace edits, it does not clear the search"
    );
    m.key(ctrl('a'));
    m.key(ctrl('k'));
    assert!(m.query.value.is_empty() && m.search_focus);
    assert!(matches!(m.view, View::List));
}
