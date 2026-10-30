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
        retarget: false,
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

fn host_form(m: &Model) -> &forms::HostForm {
    let View::Host(form) = &m.view else {
        panic!("the host form is open")
    };
    form
}

fn add_form(m: &mut Model, target: &str) {
    m.open_view(View::Host(Box::new(forms::HostForm::add(
        String::new(),
        target.into(),
    ))));
}

#[test]
fn simple_host_is_created_without_visiting_advanced_options() {
    use forms::{Control, Section, Stop};
    let mut m = model(serde_json::json!({}));
    add_form(&mut m, "/etc/ssh/work");
    let text = screen(&mut m, 100, 30);
    // The destination is known before any preview, and essentials come first.
    for expected in [
        "Hôtes > Ajouter un hôte",
        "Fichier : /etc/ssh/work",
        "[  Changer ]",
        "> Alias *",
        "Adresse *",
        "▸ Options avancées",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    typing(&mut m, "api-prod");
    m.key(key(KeyCode::Enter));
    typing(&mut m, "api.example.net");
    // Enter walks user, port and key, then stops on the main action.
    for _ in 0..3 {
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    }
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(host_form(&m).current(), Stop::Control(Control::Preview));
    assert!(!host_form(&m).is_open(Section::Advanced));
    assert!(screen(&mut m, 100, 30).contains("[▶ Prévisualiser les modifications ]"));
    let Action::Save(None, host, target) = m.key(key(KeyCode::Enter)) else {
        panic!("the Preview button continues to the review")
    };
    assert_eq!(
        (host.name.as_str(), host.hostname.as_str()),
        ("api-prod", "api.example.net")
    );
    assert_eq!(target, PathBuf::from("/etc/ssh/work"));
    assert!(host.aliases.is_empty() && host.proxy_jump.is_empty() && host.identity.is_empty());
}

#[test]
fn empty_fields_say_what_they_mean_and_examples_are_never_values() {
    let mut m = model(serde_json::json!({}));
    add_form(&mut m, "config");
    let text = screen(&mut m, 110, 30);
    for expected in [
        "requis",
        "Utilisateur       laissé à OpenSSH",
        "22 — défaut OpenSSH, non écrit",
        // The example of the focused field is a hint under the form, not a value.
        "ex. : api-prod",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(
        !text.contains("192.168.1.100"),
        "another field's example is not shown: {text}"
    );
    typing(&mut m, "host");
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 110, 30).contains("ex. : 192.168.1.100 ou example.com"));
    // What is written matches what the form announced.
    typing(&mut m, "host.example");
    let Action::Save(None, host, _) = m.key(ctrl('s')) else {
        panic!("Ctrl+S previews from any field")
    };
    // An empty field writes nothing: the local user name is never added.
    assert!(host.user.is_empty());
    assert_eq!(
        host.port, "22",
        "default port: never serialised as a directive"
    );
    assert!(host.identity.is_empty(), "the key example is not written");
    // An existing block leaves an empty user to OpenSSH.
    let mut edit = forms::HostForm::edit(Host {
        name: "one".into(),
        hostname: "h".into(),
        ..Host::default()
    });
    assert_eq!(edit.host().unwrap().user, "");
    edit.focus_on(forms::Stop::Field(forms::FormField::Input(2)));
    m.view = View::Host(Box::new(edit));
    assert!(screen(&mut m, 110, 30).contains("> Utilisateur       laissé à OpenSSH"));
}

#[test]
fn changing_the_target_file_returns_to_the_same_draft() {
    use forms::{Control, Stop};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let work = temp.path().join("work");
    std::fs::write(&root, "Include work\nHost a\n HostName a.example\n").unwrap();
    std::fs::write(&work, "# FileTags: team\nHost b\n HostName b.example\n").unwrap();
    let mut m = model(serde_json::json!({}));
    add_form(&mut m, root.to_str().unwrap());
    typing(&mut m, "draft-host");
    m.key(key(KeyCode::Tab));
    typing(&mut m, "draft.example");
    m.key(key(KeyCode::Left));
    let before = (
        host_form(&m).inputs[0].value.clone(),
        host_form(&m).inputs[1].cursor,
    );
    // Shift+Tab twice reaches the file control above the alias.
    m.key(key(KeyCode::BackTab));
    m.key(key(KeyCode::BackTab));
    assert_eq!(host_form(&m).current(), Stop::Control(Control::File));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Retarget);
    select_target(&mut m, &root).unwrap();
    let View::Files {
        files,
        selected,
        retarget: true,
        ..
    } = &m.view
    else {
        panic!("the file selector opens over the draft")
    };
    assert_eq!(files[*selected], root, "the current file is preselected");
    // Cancelling changes nothing.
    m.key(key(KeyCode::Esc));
    assert_eq!(host_form(&m).target, root);
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Retarget);
    select_target(&mut m, &root).unwrap();
    m.key(key(KeyCode::Down));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    let form = host_form(&m);
    assert_eq!(form.target, work);
    assert_eq!(
        form.file_tags,
        ["team"],
        "inherited tags follow the new file"
    );
    assert_eq!(
        (form.inputs[0].value.clone(), form.inputs[1].cursor),
        before
    );
    assert_eq!(form.inputs[1].value, "draft.example");
    assert!(form.dirty());
    let text = screen(&mut m, 110, 30);
    assert!(
        text.contains(&format!("Fichier : {}", work.display())),
        "{text}"
    );
    assert!(text.contains("hérités : %team"), "{text}");
    let Action::Save(None, _, target) = m.key(ctrl('s')) else {
        panic!()
    };
    assert_eq!(target, work);
    // A single configuration file has no other destination: explained, not silent.
    std::fs::write(&root, "Host a\n HostName a.example\n").unwrap();
    add_form(&mut m, root.to_str().unwrap());
    m.key(key(KeyCode::BackTab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Retarget);
    select_target(&mut m, &root).unwrap();
    assert!(matches!(m.view, View::Host(_)));
    assert!(screen(&mut m, 110, 30).contains("Un seul fichier de configuration SSH"));
}

#[test]
fn sections_unfold_on_demand_and_for_the_first_error_only() {
    use forms::{Control, FormField, Section, Stop};
    let mut m = model(serde_json::json!({}));
    add_form(&mut m, "config");
    typing(&mut m, "host");
    m.key(key(KeyCode::Tab));
    typing(&mut m, "host.example");
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.focus_on(Stop::Control(Control::Section(Section::Advanced)));
    assert!(!screen(&mut m, 110, 34).contains("ProxyJump"));
    m.key(key(KeyCode::Char(' ')));
    let text = screen(&mut m, 110, 34);
    assert!(
        text.contains("▾ Options avancées") && text.contains("ProxyJump"),
        "{text}"
    );
    m.key(key(KeyCode::Left));
    assert!(!host_form(&m).is_open(Section::Advanced));
    // A hidden invalid value unfolds its own section, focuses the field, and
    // every other value is preserved.
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.inputs[10].set("bad\ntty");
    form.focus_on(Stop::Control(Control::Preview));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    let form = host_form(&m);
    assert_eq!(form.current(), Stop::Field(FormField::Input(10)));
    assert!(form.is_open(Section::Advanced) && !form.is_open(Section::Organization));
    assert_eq!(form.inputs[1].value, "host.example");
    let text = screen(&mut m, 110, 34);
    assert!(text.contains("1 champ(s) à corriger"), "{text}");
    assert!(text.contains("> RequestTTY"), "{text}");
}

#[test]
fn additional_aliases_have_visible_add_and_remove_commands() {
    use forms::{Control, FormField, Section, Stop};
    let mut m = model(serde_json::json!({}));
    add_form(&mut m, "config");
    typing(&mut m, "web");
    m.key(key(KeyCode::Tab));
    typing(&mut m, "web.example");
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.focus_on(Stop::Control(Control::Section(Section::Aliases)));
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Tab));
    assert_eq!(host_form(&m).current(), Stop::Control(Control::AddAlias));
    let text = screen(&mut m, 110, 34);
    assert!(text.contains("[▶ + Ajouter un alias ]"), "{text}");
    assert!(text.contains("Les alias partagent ce bloc"), "{text}");
    m.key(key(KeyCode::Enter));
    assert_eq!(host_form(&m).current(), Stop::Field(FormField::Name(0)));
    typing(&mut m, "www");
    m.key(key(KeyCode::Tab));
    assert_eq!(
        host_form(&m).current(),
        Stop::Control(Control::RemoveAlias(0))
    );
    assert!(screen(&mut m, 110, 34).contains("[▶ Retirer ]"));
    // A new block may declare several aliases sharing one configuration.
    let Action::Save(None, host, _) = m.key(ctrl('s')) else {
        panic!()
    };
    assert_eq!(host.name, "web");
    assert_eq!(host.aliases, ["web", "www"]);
    m.key(key(KeyCode::Enter));
    assert!(host_form(&m).names.is_empty());
    assert_eq!(host_form(&m).current(), Stop::Control(Control::AddAlias));
    let Action::Save(None, host, _) = m.key(ctrl('s')) else {
        panic!()
    };
    assert!(host.aliases.is_empty());
}

#[test]
fn modes_are_named_and_the_note_returns_to_the_draft() {
    use forms::{Control, Section, Stop};
    let mut m = model(serde_json::json!({}));
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            note: "Bastion requis\nseconde ligne".into(),
            ..HostMetadata::default()
        },
    );
    m.key(key(KeyCode::Char('e')));
    let text = screen(&mut m, 110, 34);
    assert!(text.contains("Hôtes > Modifier alpha"), "{text}");
    assert!(
        !text.contains("[  Changer ]"),
        "an existing block is moved by its own action"
    );
    typing(&mut m, "-draft");
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.focus_on(Stop::Control(Control::Section(Section::Organization)));
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Tab));
    assert_eq!(host_form(&m).current(), Stop::Control(Control::Note));
    let text = screen(&mut m, 110, 34);
    assert!(
        text.contains("Note personnelle") && text.contains("Bastion requis"),
        "{text}"
    );
    assert!(text.contains("hors du fichier SSH"), "{text}");
    let draft = format!("{:?}", m.view);
    m.key(key(KeyCode::Enter));
    assert!(matches!(&m.view, View::Note(editor) if editor.host == "alpha"));
    m.key(key(KeyCode::Esc));
    assert_eq!(format!("{:?}", m.view), draft);
    // Paste never types into a button.
    m.paste("ignored");
    assert_eq!(format!("{:?}", m.view), draft);
    let copy = forms::HostForm::duplicate(m.all[0].clone(), "config".into());
    m.view = View::Host(Box::new(copy));
    let text = screen(&mut m, 110, 34);
    assert!(
        text.contains("Hôtes > Dupliquer alpha — saisir un NOUVEAU nom"),
        "{text}"
    );
    assert!(
        !text.contains("Alias supplémentaires"),
        "a copy takes one new name: {text}"
    );
}

fn source_selector(m: &mut Model, files: &[&str], selected: usize) {
    m.open_view(View::SourceFiles {
        files: files.iter().map(PathBuf::from).collect(),
        selected,
    });
}

#[test]
fn current_choice_is_marked_and_cancelling_changes_nothing() {
    let mut m = model(serde_json::json!({}));
    m.all[0].source_file = "/etc/ssh/work".into();
    m.all[1].source_file = "/etc/ssh/home".into();
    m.source_filter = Some("/etc/ssh/work".into());
    m.filter(false);
    source_selector(
        &mut m,
        &["/etc/ssh/work", "/etc/ssh/home", "/etc/ssh/empty"],
        1,
    );
    let text = screen(&mut m, 100, 24);
    // The value in effect has its own mark, distinct from the cursor.
    assert!(
        text.contains("▶✓ /etc/ssh/work") && text.contains("(actuel)"),
        "{text}"
    );
    assert!(text.contains("   Tous les fichiers"), "{text}");
    assert!(text.contains("1 hôte dans ce fichier"), "{text}");
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Down));
    let text = screen(&mut m, 100, 24);
    assert!(
        text.contains(" ✓ /etc/ssh/work"),
        "the mark stays on the value in effect: {text}"
    );
    assert!(text.contains("▶  /etc/ssh/empty"), "{text}");
    assert!(
        text.contains("aucun hôte : fichier vide ou sans bloc Host"),
        "{text}"
    );
    for cancel in [key(KeyCode::Esc), ctrl('c')] {
        m.key(cancel);
        assert!(matches!(m.view, View::List));
        assert_eq!(m.source_filter, Some(PathBuf::from("/etc/ssh/work")));
        source_selector(
            &mut m,
            &["/etc/ssh/work", "/etc/ssh/home", "/etc/ssh/empty"],
            3,
        );
    }
    // "All files" is a choice of its own, applied like any other.
    m.key(key(KeyCode::Down));
    assert!(
        matches!(m.view, View::SourceFiles { selected: 0, .. }),
        "the list wraps"
    );
    assert!(screen(&mut m, 100, 24).contains("Aucun filtre par fichier."));
    m.key(key(KeyCode::Enter));
    assert!(m.source_filter.is_none());
}

#[test]
fn same_named_files_stay_distinguishable_when_paths_are_shortened() {
    let mut m = model(serde_json::json!({}));
    let files = [
        "/home/me/.ssh/config",
        "/home/me/.ssh/customers/acme-corporation/hosts",
        "/home/me/.ssh/customers/globex-industries/hosts",
    ];
    m.open_view(View::Files {
        files: files.iter().map(PathBuf::from).collect(),
        selected: 2,
        host: None,
        seed: String::new(),
        duplicate: None,
        retarget: false,
    });
    // Wide enough: full paths. Too narrow: the shortest tails that differ, and
    // the full path of the highlighted file under the list.
    assert!(screen(&mut m, 110, 24).contains("   /home/me/.ssh/customers/acme-corporation/hosts"));
    let text = screen(&mut m, 56, 24);
    assert!(text.contains("   …/acme-corporation/hosts"), "{text}");
    assert!(text.contains("▶  …/globex-industries/hosts"), "{text}");
    assert!(text.contains("   …/config"), "{text}");
    assert!(
        text.contains("/home/me/.ssh/customers/globex-industries/hosts"),
        "{text}"
    );
    assert!(
        text.contains("Hôtes > Ajouter un hôte — fichier de destination"),
        "{text}"
    );
}

#[test]
fn typing_narrows_a_selector_and_applies_nothing_before_enter() {
    let mut m = model(serde_json::json!({}));
    source_selector(&mut m, &["/etc/ssh/work", "/etc/ssh/home"], 0);
    typing(&mut m, "HÔME");
    let text = screen(&mut m, 100, 24);
    assert!(
        text.contains("Rechercher : HÔME") && text.contains("▶  /etc/ssh/home"),
        "{text}"
    );
    assert!(!text.contains("/etc/ssh/work"), "{text}");
    assert!(m.source_filter.is_none(), "narrowing is not choosing");
    // Letters are search text here: `q` does not close, `j` does not move.
    typing(&mut m, "qj");
    assert!(matches!(m.view, View::SourceFiles { .. }));
    assert!(screen(&mut m, 100, 24).contains("Aucun choix ne correspond à la recherche."));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::SourceFiles { .. }) && m.source_filter.is_none());
    m.key(ctrl('u'));
    m.paste("work\n");
    assert!(
        matches!(m.view, View::SourceFiles { .. }),
        "paste never applies"
    );
    m.key(key(KeyCode::Enter));
    assert_eq!(m.source_filter, Some(PathBuf::from("/etc/ssh/work")));
    // The search of a previous selector is not carried into the next one.
    source_selector(&mut m, &["/etc/ssh/work", "/etc/ssh/home"], 0);
    assert!(m.selector_query.value.is_empty());
}

#[test]
fn alias_and_method_selectors_explain_each_choice() {
    let mut m = model(serde_json::json!({}));
    let mut second = m.all[0].clone();
    second.name = "alpha-admin".into();
    second.user = "root".into();
    second.source_file = "/etc/ssh/work".into();
    m.open_view(View::Aliases {
        hosts: vec![m.all[0].clone(), second],
        selected: 1,
    });
    let text = screen(&mut m, 100, 24);
    for expected in [
        "Choisir un alias — cible exacte de connexion",
        " ✓ alpha",
        "▶  alpha-admin  root@alpha.example",
        "Fichier : /etc/ssh/work",
        "le bloc SSH est partagé",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Char('P')));
    let text = screen(&mut m, 110, 24);
    assert!(
        text.contains("▶✓ SSH") && text.contains("Ne prouve pas l'authentification."),
        "{text}"
    );
    m.key(key(KeyCode::Char('j')));
    let text = screen(&mut m, 110, 24);
    assert!(
        text.contains("sans bastion ni authentification SSH"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::None);
    assert!(matches!(m.view, View::List));
    assert_eq!(m.preferences.availability_method, AvailabilityMethod::Ssh);
    m.key(key(KeyCode::Char('g')));
    let text = screen(&mut m, 110, 24);
    assert!(
        text.contains("Groupes — présentation de la liste"),
        "{text}"
    );
    assert!(
        text.contains("▶✓ Flat — liste plate") && text.contains("une seule liste triée"),
        "{text}"
    );
}

/// Real files: batch targets are frozen against the configuration on disk.
fn marking_model(mode: GroupMode, group_aliases: bool) -> (tempfile::TempDir, Model) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(
        &root,
        "Host alpha secondary\n HostName alpha.invalid\nHost beta\n HostName beta.invalid\nHost gamma\n HostName gamma.invalid\n",
    )
    .unwrap();
    let hosts = config::load_report(&root).unwrap().hosts;
    let mut m = Model::new(hosts, AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    m.batch.root = root;
    m.batch.cache_sources(&m.all.clone());
    m.preferences.group_mode = mode;
    m.preferences.group_aliases = group_aliases;
    m.filter(true);
    (temp, m)
}

fn row_of<'a>(text: &'a str, name: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(name) && line.contains('│'))
        .unwrap_or_else(|| panic!("no row for {name}: {text}"))
}

#[test]
fn favorite_and_batch_target_use_distinct_zones() {
    let (_temp, mut m) = marking_model(GroupMode::Flat, false);
    m.metadata.insert(
        "beta".into(),
        HostMetadata {
            favorite: true,
            ..HostMetadata::default()
        },
    );
    m.filter(false);
    // Without any target there is no target zone at all.
    let text = screen(&mut m, 110, 24);
    assert!(row_of(&text, "beta").contains("│? *beta"), "{text}");
    assert!(!text.contains("[ ]") && !text.contains("Lot :"), "{text}");
    while m.selected_host().unwrap().name != "alpha" {
        m.key(key(KeyCode::Down));
    }
    m.key(key(KeyCode::Char('x')));
    let text = screen(&mut m, 110, 24);
    // A favorite that is not a target, and a target that is not a favorite.
    assert!(row_of(&text, "beta").contains("│[ ] ? *beta"), "{text}");
    assert!(row_of(&text, "alpha").contains("[x] ?  alpha"), "{text}");
    assert!(row_of(&text, "gamma").contains("│[ ] ?  gamma"), "{text}");
    assert!(
        text.contains("Légende : [x] cible de lot · * favori"),
        "{text}"
    );
    // The legend is contextual and leaves small terminals to the table.
    assert!(!screen(&mut m, 110, 18).contains("Légende"));
}

#[test]
fn selection_stays_counted_and_visible_through_search_and_collapse() {
    let (_temp, mut m) = marking_model(GroupMode::Files, false);
    for name in ["alpha", "gamma"] {
        while m.selected_host().map(|host| host.name).as_deref() != Some(name) {
            m.key(key(KeyCode::Down));
        }
        m.key(key(KeyCode::Char('x')));
    }
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("Lot : 2 cibles marquées • B : voir / préparer • F2 : tout retirer"),
        "{text}"
    );
    assert!(
        text.contains("(4 • 2 cibles)"),
        "the header counts its marked hosts: {text}"
    );
    // A search hides one target: it stays counted, and is said to be outside.
    m.query.set("alpha");
    m.filter(true);
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("Lot : 2 cibles marquées, dont 1 hors du filtre"),
        "{text}"
    );
    assert_eq!(
        m.batch.marks.len(),
        2,
        "searching never replaces the selection"
    );
    m.query.set("");
    m.filter(true);
    // Collapsing the group hides the rows, not the count.
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let text = screen(&mut m, 120, 24);
    assert!(
        !text.contains("gamma") && text.contains("(4 • 2 cibles)"),
        "{text}"
    );
    assert!(text.contains("Lot : 2 cibles marquées •"), "{text}");
    // A compact terminal keeps the count on its single band row.
    assert!(screen(&mut m, 80, 10).contains("Lot : 2 cible(s)"));
    m.key(key(KeyCode::Char('B')));
    let View::Batch(editor) = &m.view else {
        panic!("B shows the marked targets")
    };
    assert_eq!(editor.targets.len(), 2);
}

#[test]
fn group_header_is_never_a_silent_target_and_enter_is_announced() {
    let (_temp, mut m) = marking_model(GroupMode::Files, false);
    m.key(key(KeyCode::Home));
    assert!(m.selected_group().is_some());
    let text = screen(&mut m, 120, 24);
    assert!(text.contains("Enter: open/close group"), "{text}");
    // Host shortcuts explain why they do nothing on a header.
    assert_eq!(m.key(key(KeyCode::Char('x'))), Action::None);
    assert!(m.batch.marks.is_empty());
    assert!(screen(&mut m, 120, 24)
        .contains("Lot : marquer l'alias — Sélectionnez un hôte, pas un groupe"));
    for shortcut in ['e', 'd', 'm', 'N'] {
        assert_eq!(
            m.key(key(KeyCode::Char(shortcut))),
            Action::None,
            "{shortcut}"
        );
        assert!(matches!(m.view, View::List), "{shortcut}");
        assert!(m.hint.contains("pas un groupe"), "{shortcut}: {}", m.hint);
    }
    // Enter toggles the header and connects to a host; the footer says which.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(
        m.hint.is_empty(),
        "the explanation lasts until the next key"
    );
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Down));
    assert!(screen(&mut m, 120, 24).contains("Enter: connect"));
    assert!(matches!(m.key(key(KeyCode::Enter)), Action::Connect(..)));
}

#[test]
fn grouped_block_shows_its_connection_alias_and_other_aliases() {
    let (_temp, mut m) = marking_model(GroupMode::Flat, true);
    let text = screen(&mut m, 120, 24);
    assert!(row_of(&text, "alpha").contains("alpha +1 alias"), "{text}");
    assert!(!row_of(&text, "beta").contains("alias"), "{text}");
    assert!(
        text.contains("Légende : a→b alias actif d'un bloc"),
        "{text}"
    );
    // Choosing the secondary alias keeps both names visible, target last.
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 120, 24);
    assert!(
        row_of(&text, "secondary").contains("alpha→secondary +1 alias"),
        "{text}"
    );
    assert_eq!(m.selected_host().unwrap().name, "secondary");
    // A narrow column keeps the connection alias rather than the indicator.
    let narrow = screen(&mut m, 30, 24);
    assert!(
        narrow.contains("→") && !narrow.contains("+1 alias"),
        "{narrow}"
    );
}

#[test]
fn reading_a_sheet_never_edits_and_actions_are_named() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('i')));
    let text = screen(&mut m, 110, 40);
    for expected in [
        "── Résumé",
        "── Configuration effective",
        "── Déclaration et source",
        "── Disponibilité mesurée",
        "── Historique des connexions",
        "[  Connexion ] [  Modifier ] [  Note ] [  Copier ] [  Fermer ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Enter while reading changes nothing and says how to act.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::Info(_)));
    assert!(screen(&mut m, 110, 40).contains("Entrée seule ne modifie rien"));
    // Tab names the action before Enter runs it; the arrows wrap.
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 110, 40).contains("[▶ Connexion ]"));
    m.key(key(KeyCode::Left));
    assert!(screen(&mut m, 110, 40).contains("[▶ Fermer ]"));
    m.key(key(KeyCode::Right));
    m.key(key(KeyCode::Right));
    assert!(screen(&mut m, 110, 40).contains("[▶ Modifier ]"));
    m.key(key(KeyCode::Enter));
    assert!(matches!(&m.view, View::Host(form) if form.original.is_some()));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Info(_)));
    m.key(key(KeyCode::BackTab));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), None)
    );
    // A sheet opened again starts in reading mode.
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Char('i')));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::Info(_)));
}

#[test]
fn copy_is_explicit_with_feedback_and_a_selectable_fallback() {
    let mut m = model(serde_json::json!({}));
    m.all[0].user = "deploy".into();
    m.all[0].source_file = "/etc/ssh/work".into();
    m.filter(false);
    m.key(key(KeyCode::Char('i')));
    for _ in 0..4 {
        m.key(key(KeyCode::Tab));
    }
    assert!(screen(&mut m, 110, 40).contains("[▶ Copier ]"));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 110, 30);
    for expected in [
        "Copier une valeur — alpha",
        "Une seule valeur est copiée ; rien n'est exporté",
        "Alias",
        "Commande             ssh alpha",
        "Utilisateur@adresse  deploy@alpha.example",
        "Fichier source       /etc/ssh/work",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Cancelling copies nothing.
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Info(_)) && m.copied.is_none());
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Down));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Copy("deploy@alpha.example".into())
    );
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("Copie demandée au terminal (OSC 52) : Utilisateur@adresse"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains("│deploy@alpha.example")),
        "the value is on a line of its own, selectable by hand: {text}"
    );
    // The clipboard request is the standard OSC 52 sequence, base64-encoded.
    assert_eq!(details::osc52("a"), "\x1b]52;c;YQ==\x07");
    assert_eq!(details::osc52("ab"), "\x1b]52;c;YWI=\x07");
    assert_eq!(details::osc52("ssh é"), "\x1b]52;c;c3NoIMOp\x07");
}

#[test]
fn group_sheet_explains_its_criterion() {
    let mut m = model(serde_json::json!({}));
    m.all[0].source_file = "/etc/ssh/work".into();
    m.all[1].source_file = "/etc/ssh/work".into();
    m.all[0].tags = vec!["prod".into()];
    m.preferences.group_mode = GroupMode::FileTags;
    m.filter(false);
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Char('i')));
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("Critère : hôtes déclarés dans ce fichier de configuration."),
        "{text}"
    );
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Down));
    assert!(m.selected_group().is_some());
    m.key(key(KeyCode::Char('i')));
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("dont le tag principal est celui-ci"),
        "{text}"
    );
}

/// Column where `needle` starts on its row, in terminal cells.
fn column_of(text: &str, needle: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    let row = text
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle} is displayed: {text}"));
    row[..row.find(needle).unwrap()].width()
}

#[test]
fn wide_terminals_keep_controls_and_counts_next_to_their_content() {
    // Full HD, 4K and 32:9 terminals: nothing useful is pushed to the far edge.
    for width in [200, 380, 500] {
        let mut m = model(serde_json::json!({}));
        m.metadata.insert(
            "alpha".into(),
            HostMetadata {
                note: "note".into(),
                ..HostMetadata::default()
            },
        );
        // List: the host count follows the filters or the search hint.
        let text = screen(&mut m, width, 50);
        assert!(column_of(&text, "2 hôtes") < 40, "{width}: {text}");
        m.query.set("alp");
        m.filter(true);
        let text = screen(&mut m, width, 50);
        assert!(column_of(&text, "1 hôte sur 2") < 80, "{width}");
        m.search_focus = true;
        assert!(
            column_of(&screen(&mut m, width, 50), "1 hôte sur 2") < 40,
            "{width}"
        );
        m.search_focus = false;
        m.query.set("");
        m.filter(true);
        // Table: the last column stays within reading distance of the name.
        assert!(
            column_of(&screen(&mut m, width, 50), "Last Attempt") < 140,
            "{width}"
        );
        // Host form: every button sits right after its value.
        m.key(key(KeyCode::Char('e')));
        m.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
        let View::Host(form) = &mut m.view else {
            panic!()
        };
        form.reveal(forms::Section::Organization);
        let text = screen(&mut m, width, 50);
        for button in ["Choisir ]", "Retirer ]", "Modifier ]", "Annuler ]"] {
            assert!(column_of(&text, button) < 70, "{width}: {button}: {text}");
        }
        // A long value moves its button along, still attached and within the form.
        let View::Host(form) = &mut m.view else {
            panic!()
        };
        form.inputs[4].set(&"k".repeat(60));
        let text = screen(&mut m, width, 50);
        let end_of_value = column_of(&text, "kkkk") + 60;
        assert_eq!(
            column_of(&text, "[  Choisir ]"),
            end_of_value + 2,
            "{width}"
        );
        m.view = View::List;
        m.parents.clear();
        // Add form: the destination path keeps its button attached.
        add_form(&mut m, "/etc/ssh/config");
        let text = screen(&mut m, width, 50);
        assert!(column_of(&text, "Changer ]") < 50, "{width}: {text}");
        m.view = View::List;
        m.parents.clear();
        // Sheet, palette and confirmation are bounded panels, never full width.
        m.key(key(KeyCode::Char('i')));
        let text = screen(&mut m, width, 50);
        let fermer = column_of(&text, "Fermer ]");
        assert!(fermer - column_of(&text, "Connexion ]") < 80, "{width}");
        m.key(key(KeyCode::Esc));
        // Tables of the other screens keep their last column in reach too.
        m.key(key(KeyCode::F(7)));
        assert!(
            column_of(&screen(&mut m, width, 50), "State") < 160,
            "{width}"
        );
        m.key(key(KeyCode::Esc));
        m.tunnel_profiles.insert(
            "alpha".into(),
            BTreeMap::from([("db".to_owned(), Forward::default())]),
        );
        m.key(key(KeyCode::F(6)));
        assert!(
            column_of(&screen(&mut m, width, 50), "Forwarding") < 160,
            "{width}"
        );
    }
}

#[test]
fn note_says_whose_it_is_where_it_lives_and_whether_it_changed() {
    let mut m = model(serde_json::json!({}));
    m.all[0].source_file = "/etc/ssh/work".into();
    m.all[0].tags = vec!["prod".into()];
    m.batch.history_root = Some("/home/me/.config/ssho".into());
    m.filter(false);
    m.key(key(KeyCode::Char('N')));
    let text = screen(&mut m, 120, 24);
    for expected in [
        "Personal note",
        "Host: alpha",
        "Enregistrée",
        "Fichier SSH de l'hôte : /etc/ssh/work",
        "Donnée de ssho : /home/me/.config/ssho/metadata.json",
        "Jamais écrite dans le fichier SSH ; à ne pas confondre avec un tag.",
        "[  Enregistrer ]  [  Annuler ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    typing(&mut m, "Bastion requis");
    m.key(key(KeyCode::Enter));
    typing(&mut m, "ligne 2");
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("● Modifiée") && text.contains("ligne 2"),
        "{text}"
    );
    // Tab reaches the buttons; Enter there acts, and no text is typed into them.
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 24).contains("[▶ Enregistrer ]"));
    typing(&mut m, "x");
    m.paste("pasted");
    let View::Note(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.input.value, "Bastion requis\nligne 2");
    m.key(key(KeyCode::Right));
    assert!(screen(&mut m, 120, 24).contains("[▶ Annuler ]"));
    // Cancelling a modified note still offers to resume it.
    m.key(key(KeyCode::Enter));
    assert!(m.discard);
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Note(_)));
    m.key(key(KeyCode::BackTab));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::SaveNote("alpha".into(), "Bastion requis\nligne 2".into())
    );
}

#[test]
fn list_marks_a_note_discreetly_without_a_column() {
    let mut m = model(serde_json::json!({}));
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            note: "Bastion requis".into(),
            favorite: true,
        },
    );
    m.filter(false);
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("│?≡*alpha") && text.contains("│?  beta"),
        "{text}"
    );
    assert!(text.contains("≡ note personnelle"), "{text}");
    assert!(
        !text.contains("Bastion requis"),
        "the note itself stays in the sheet: {text}"
    );
    m.key(key(KeyCode::Char('i')));
    assert!(screen(&mut m, 120, 40).contains("Personal note: Bastion requis"));
}

fn forward_form(m: &Model) -> &forms::ForwardForm {
    let View::Forward(form) = &m.view else {
        panic!("the tunnel form is open")
    };
    form
}

#[test]
fn tunnel_direction_is_said_in_words_for_each_type() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('f')));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("(•) Local (-L)   ( ) Remote (-R)   ( ) Dynamic (-D)"),
        "{text}"
    );
    assert!(
        text.contains("Listen on this machine, port ? → reach localhost:? from the SSH server"),
        "{text}"
    );
    assert!(
        text.contains("Local Port (listens on this machine):"),
        "{text}"
    );
    m.key(key(KeyCode::Tab));
    typing(&mut m, "5433");
    m.key(key(KeyCode::Tab));
    m.key(ctrl('u'));
    typing(&mut m, "db.internal");
    m.key(key(KeyCode::Tab));
    typing(&mut m, "5432");
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains(
            "Listen on this machine, port 5433 → reach db.internal:5432 from the SSH server"
        ),
        "{text}"
    );
    // The same values read the other way round for a remote forwarding.
    m.key(key(KeyCode::BackTab));
    m.key(key(KeyCode::BackTab));
    m.key(key(KeyCode::BackTab));
    m.key(key(KeyCode::Right));
    let text = screen(&mut m, 110, 40);
    assert!(text.contains("( ) Local (-L)   (•) Remote (-R)"), "{text}");
    assert!(
        text.contains(
            "Listen on the SSH server, port 5433 → reach db.internal:5432 from this machine"
        ),
        "{text}"
    );
    assert!(
        text.contains("Remote Port (listens on the SSH server):"),
        "{text}"
    );
    m.key(key(KeyCode::Right));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains(
            "SOCKS proxy on this machine, port 5433 → connections leave from the SSH server"
        ),
        "{text}"
    );
}

#[test]
fn save_and_launch_are_separate_buttons_and_enter_reaches_neither() {
    use forms::ForwardForm as F;
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('f')));
    m.key(key(KeyCode::Tab));
    typing(&mut m, "8080");
    // Repeating Enter walks the fields and stops on the last one.
    for _ in 0..12 {
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    }
    assert_eq!(forward_form(&m).focus, 4);
    assert!(matches!(m.view, View::Forward(_)));
    // The launch mode is a visible control, explained before launching.
    m.key(key(KeyCode::Tab));
    assert_eq!(forward_form(&m).focus, F::MODE);
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("> (•) Shell session   ( ) Tunnel only"),
        "{text}"
    );
    assert!(
        text.contains("the tunnel ends when you leave that shell"),
        "{text}"
    );
    m.key(key(KeyCode::Char(' ')));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("( ) Shell session   (•) Tunnel only"),
        "{text}"
    );
    assert!(text.contains("follow or stop it in F7"), "{text}");
    m.key(key(KeyCode::Tab));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("[▶ Launch ]  [  Save profile ]  [  Cancel ]"),
        "{text}"
    );
    // Saving needs a name; launching does not.
    m.key(key(KeyCode::Right));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(forward_form(&m).focus, 5, "the missing name gets the focus");
    assert!(!forward_form(&m).errors.is_empty());
    let View::Forward(form) = &mut m.view else {
        panic!()
    };
    form.inputs[3].set("80");
    form.focus = F::LAUNCH;
    assert!(matches!(
        m.key(key(KeyCode::Enter)),
        Action::StartTunnel(host, None, _) if host == "alpha"
    ));
    // A button receives no text.
    typing(&mut m, "zz");
    m.paste("zz");
    assert_eq!(forward_form(&m).inputs[1].value, "8080");
    let View::Forward(form) = &mut m.view else {
        panic!()
    };
    form.focus = F::CANCEL;
    m.key(key(KeyCode::Enter));
    assert!(m.discard, "a modified draft is not dropped silently");
}

#[test]
fn profiles_show_their_route_and_named_actions() {
    let mut m = model(serde_json::json!({}));
    let forward = Forward {
        kind: "local".into(),
        local_port: "5433".into(),
        remote_host: "db.internal".into(),
        remote_port: "5432".into(),
        ..Forward::default()
    };
    m.tunnel_profiles.insert(
        "alpha".into(),
        BTreeMap::from([("db".to_owned(), forward.clone())]),
    );
    m.key(key(KeyCode::F(6)));
    let text = screen(&mut m, 120, 30);
    assert!(
        text.contains(
            "Listen on this machine, port 5433 → reach db.internal:5432 from the SSH server"
        ),
        "{text}"
    );
    assert!(
        text.contains("[▶ Open ] [  Launch ] [  Duplicate ] [  Delete ] [  New ]"),
        "{text}"
    );
    // Launch is a named action, no longer a line of the help.
    m.key(key(KeyCode::Right));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), Some(forward))
    );
    m.key(key(KeyCode::Right));
    m.key(key(KeyCode::Enter));
    let form = forward_form(&m);
    assert_eq!(form.inputs[5].value, "db-copy");
    assert!(
        form.original_name.is_none() && form.dirty(),
        "a copy is an unsaved draft"
    );
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    // Deleting names the profile; Enter on the default action opens it.
    m.key(key(KeyCode::Right));
    m.key(key(KeyCode::Enter));
    assert!(screen(&mut m, 120, 30).contains("Delete profile 'db'?"));
    m.key(key(KeyCode::Esc));
    // Without any profile only New can act.
    m.tunnel_profiles.clear();
    assert!(screen(&mut m, 120, 30).contains("[▶ New ]"));
    m.key(key(KeyCode::Enter));
    assert!(forward_form(&m).original_name.is_none());
}

#[test]
fn no_target_page_never_turns_the_filter_into_targets_on_its_own() {
    let (_temp, mut m) = marking_model(GroupMode::Flat, false);
    m.key(key(KeyCode::Char('B')));
    let text = screen(&mut m, 120, 24);
    for expected in [
        "Batch: 0 targets • [Targets] → Command → Review → Results",
        "No targets selected.",
        "the filter is never turned into targets on its own",
        "[▶ Back to the list ]  [  Add the 4 host(s) shown by the filter ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Typing or pasting a command is impossible before there is a target.
    typing(&mut m, "rm -rf /");
    m.paste("uptime");
    assert!(m.batch.marks.is_empty());
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::List), "the focused choice goes back");
    // Taking the filter results is an explicit, named decision.
    m.query.set("beta");
    m.filter(true);
    m.key(key(KeyCode::Char('B')));
    assert!(screen(&mut m, 120, 24).contains("Add the 1 host(s) shown by the filter"));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    let View::Batch(editor) = &m.view else {
        panic!("the preparation step opens with the chosen targets")
    };
    assert_eq!(
        editor
            .targets
            .iter()
            .map(|t| t.alias.as_str())
            .collect::<Vec<_>>(),
        ["beta"]
    );
    assert!(screen(&mut m, 120, 24)
        .contains("Batch: 1 targets • Targets ✓ → [Command] → Review → Results"));
}

#[test]
fn arrows_belong_to_the_focused_control_and_enter_stops_on_review() {
    let (_temp, mut m) = marking_model(GroupMode::Flat, false);
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    let selected = |m: &mut Model| {
        screen(m, 120, 30)
            .lines()
            .position(|line| line.contains("alpha"))
            .is_some()
    };
    assert!(selected(&mut m));
    typing(&mut m, "uptime");
    // From an input, the arrows move between controls and leave the list alone.
    let before = screen(&mut m, 120, 30);
    m.key(key(KeyCode::Down));
    assert!(
        screen(&mut m, 120, 30).contains("Parallel: 4"),
        "focus moved to the next input"
    );
    m.key(key(KeyCode::Up));
    assert_eq!(
        screen(&mut m, 120, 30),
        before,
        "the target list did not move"
    );
    // Enter walks command, parallel, timeout, then stops on Review.
    for _ in 0..3 {
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    }
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("[▶ Review ]  [  Back ]"), "{text}");
    assert!(!m.batch.manager.has_active());
    // The list announces when the arrows are its own.
    m.key(key(KeyCode::BackTab));
    assert!(screen(&mut m, 120, 30).contains("● Frozen targets — ↑↓ select"));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("Confirm batch: 4 targets"), "{text}");
    assert!(
        text.contains("Command: uptime") && text.contains("Parallel: 4 • Timeout: 30s"),
        "{text}"
    );
    assert!(text.contains("[  Launch on 4 target(s) ]"), "{text}");
    // Repeating Enter never launches: it goes back to the edit step.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(!m.batch.manager.has_active());
}

#[test]
fn key_screen_names_its_context_and_separates_local_from_remote() {
    let mut m = model(serde_json::json!({}));
    keys::open_view(&mut m, false);
    let text = screen(&mut m, 130, 30);
    assert!(
        text.contains("Keys — OpenSSH assistant • target: alpha"),
        "{text}"
    );
    assert!(
        text.contains("Local: [▶ Create key ] [  Add to agent ] [  Associate with this host ] • Remote server: [  Deploy public key ]"),
        "{text}"
    );
    // Without a reviewed public key, deploying explains itself and does nothing.
    m.key(key(KeyCode::Left));
    assert!(screen(&mut m, 130, 30).contains("[▶ Deploy public key ]"));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(!m.error.is_empty());
    m.key(key(KeyCode::Esc));
    // Choosing a key for a draft never offers to associate or deploy.
    m.key(key(KeyCode::Char('e')));
    typing(&mut m, "-draft");
    let draft = format!("{:?}", m.view);
    keys::open_view(&mut m, true);
    let text = screen(&mut m, 130, 30);
    assert!(
        text.contains(
            "Choose a key for this draft: only its IdentityFile is filled, nothing is deployed"
        ),
        "{text}"
    );
    assert!(
        text.contains("Local: [▶ Use this key ] [  Create key ] [  Add to agent ]"),
        "{text}"
    );
    assert!(
        !text.contains("Deploy public key") && !text.contains("Associate"),
        "{text}"
    );
    for forbidden in ['d', 't'] {
        assert_eq!(m.key(key(KeyCode::Char(forbidden))), Action::None);
        assert!(matches!(&m.view, View::Keys(editor) if matches!(editor.mode, keys::Mode::List)));
    }
    m.key(key(KeyCode::Esc));
    assert_eq!(format!("{:?}", m.view), draft, "the draft is found again");
}

#[test]
fn generation_waits_for_its_button() {
    let mut m = model(serde_json::json!({}));
    keys::open_view(&mut m, false);
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 120, 30);
    assert!(
        text.contains("ssh-keygen then asks for the passphrase in the terminal"),
        "{text}"
    );
    assert!(
        text.contains("[  Continue in terminal ]  [  Back ]"),
        "{text}"
    );
    // Enter in the first field no longer generates.
    typing(&mut m, "/tmp/new-key");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    typing(&mut m, "comment");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(screen(&mut m, 120, 30).contains("[▶ Continue in terminal ]"));
    m.key(key(KeyCode::Right));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(&m.view, View::Keys(editor) if matches!(editor.mode, keys::Mode::List)));
    m.key(key(KeyCode::Char('n')));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::GenerateKey);
}

/// A real configuration file, so a change can be prepared and reviewed.
fn review_model() -> (tempfile::TempDir, PathBuf, Model) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(
        &root,
        "Host alpha\n HostName alpha.invalid\nHost beta\n HostName beta.invalid\n",
    )
    .unwrap();
    let hosts = config::load_report(&root).unwrap().hosts;
    let mut m = Model::new(hosts, AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    (temp, root, m)
}

#[test]
fn reading_a_diff_writes_nothing_and_the_button_names_the_operation() {
    let (temp, root, mut m) = review_model();
    let before = std::fs::read(&root).unwrap();
    let Action::Delete(host) = m.key(key(KeyCode::Char('d'))) else {
        panic!()
    };
    changes::open(&mut m, Action::Delete(host), &root).unwrap();
    let text = screen(&mut m, 120, 30);
    for expected in [
        // The object of a deletion is written, not only coloured.
        "Operation: DELETE host 'alpha'",
        &format!("File: {}", root.display()),
        "Its favorite, note and tunnel profiles stay in the ssho data.",
        "[  Delete ]  [  Back ]  (reading the diff)",
        "Ctrl+S: confirm exact changes",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Enter repeated from the list, or while reading, confirms nothing.
    for _ in 0..3 {
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    }
    assert!(screen(&mut m, 120, 30).contains("Nothing written: Tab reaches the buttons"));
    assert_eq!(std::fs::read(&root).unwrap(), before);
    assert!(matches!(m.view, View::Changes(_)));
    // Tab reaches Delete, then Back; each does what its name says.
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 30).contains("[▶ Delete ]"));
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 30).contains("[▶ Back ]"));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::List));
    assert_eq!(std::fs::read(&root).unwrap(), before);
    // Ctrl+S is the explicit confirmation, from any focus.
    let Action::Delete(host) = m.key(key(KeyCode::Char('d'))) else {
        panic!()
    };
    changes::open(&mut m, Action::Delete(host), &root).unwrap();
    assert_eq!(m.key(ctrl('s')), Action::ConfirmChanges);
    drop(temp);
}

#[test]
fn summary_tells_files_and_what_happens_to_personal_data() {
    let (_temp, root, mut m) = review_model();
    let other = root.with_file_name("other");
    // A rename says that personal data follows.
    let original = m.all[0].clone();
    let mut renamed = original.clone();
    renamed.name = "gamma".into();
    renamed.aliases = vec!["gamma".into()];
    changes::open(
        &mut m,
        Action::Save(Some(original.clone()), Box::new(renamed), root.clone()),
        &root,
    )
    .unwrap();
    let text = screen(&mut m, 130, 30);
    assert!(text.contains("Operation: EDIT host 'alpha'"), "{text}");
    assert!(
        text.contains(
            "Aliases: alpha → gamma. Favorite, note and tunnel profiles follow a renamed alias."
        ),
        "{text}"
    );
    assert!(text.contains("[  Save ]  [  Back ]"), "{text}");
    m.key(key(KeyCode::Esc));
    // A move shows where from and where to.
    std::fs::write(&other, "").unwrap();
    changes::open(&mut m, Action::MoveTo(original, other.clone()), &root).unwrap();
    let text = screen(&mut m, 130, 30);
    assert!(text.contains("Operation: MOVE host 'alpha'"), "{text}");
    assert!(
        text.contains(&format!(
            "From: {} → To: {}",
            root.display(),
            other.display()
        )),
        "{text}"
    );
    assert!(text.contains("[  Move ]  [  Back ]"), "{text}");
}

#[test]
fn columns_show_their_outcome_and_moving_is_a_named_action() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('v')));
    let text = screen(&mut m, 120, 30);
    for expected in [
        "[x] Name (fixed)",
        "Shown at this width: Name | Hostname | Tags | Last Attempt",
        "Every active column fits.",
        "[  Move up ] [  Move down ] [  Reset ] [  Apply ] [  Cancel ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // A narrow table: the active column short of room is told apart from "off".
    let narrow = screen(&mut m, 60, 30);
    assert!(narrow.contains("Active but short of room: "), "{narrow}");
    assert!(narrow.contains("(hidden: width)"), "{narrow}");
    // Switch User on with Space, then move it up with the named button.
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    let user = editor
        .items()
        .iter()
        .position(|column| *column == HostColumn::User)
        .unwrap();
    for _ in 0..user {
        m.key(key(KeyCode::Down));
    }
    m.key(key(KeyCode::Char(' ')));
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft.last(), Some(&HostColumn::User));
    let before = editor.draft.clone();
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 30).contains("[▶ Move up ]"));
    m.key(key(KeyCode::Enter));
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(
        editor.draft[before.len() - 2],
        HostColumn::User,
        "moved up by one"
    );
    assert_eq!(
        m.preferences.host_columns.len(),
        before.len() - 1,
        "still a draft"
    );
    // Moving does not wrap: Name stays first whatever is pressed.
    for _ in 0..10 {
        m.key(key(KeyCode::Enter));
    }
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft[0], HostColumn::Name);
    assert_eq!(editor.draft[1], HostColumn::User);
    let draft = editor.draft.clone();
    // Apply and Cancel are named too; the draft is published only by Apply.
    m.key(key(KeyCode::Right));
    m.key(key(KeyCode::Right));
    m.key(key(KeyCode::Right));
    assert!(screen(&mut m, 120, 30).contains("[▶ Apply ]"));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Columns(draft));
    m.key(key(KeyCode::Right));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::List));
}

#[test]
fn long_error_is_readable_in_full_even_on_a_small_terminal() {
    let mut m = model(serde_json::json!({}));
    let cause = "refusing application file /very/long/path/to/the/application/data/config.json: expected a regular file, not a symbolic link";
    m.error = cause.into();
    // The list says the level in words and how to read the rest.
    let text = screen(&mut m, 60, 20);
    assert!(text.contains("Error: refusing application file"), "{text}");
    assert!(text.contains("(!: full text)"), "{text}");
    assert!(!text.contains("symbolic link"), "{text}");
    m.key(key(KeyCode::Char('!')));
    assert!(matches!(m.view, View::Messages));
    let text = screen(&mut m, 60, 20);
    // The message is wrapped, not cut: joined back, every character is there.
    let joined: String = text
        .lines()
        .filter_map(|line| line.split('│').nth(1))
        .map(str::trim_end)
        .collect();
    assert!(text.contains("[Error]") && joined.contains(cause), "{text}");
    m.key(key(KeyCode::Esc));
    assert_eq!(m.error, cause, "reading a message does not clear it");
    // A warning is named as such, never shown as an error.
    m.error = "Warning: could not record tunnel attempt".into();
    let text = screen(&mut m, 100, 20);
    assert!(
        text.contains("Warning: could not record") && !text.contains("Error:"),
        "{text}"
    );
    // Without any message the action explains itself.
    m.error.clear();
    assert_eq!(m.key(key(KeyCode::Char('!'))), Action::None);
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "messages");
    assert!(screen(&mut m, 100, 30).contains("Indisponible : Aucun message à afficher."));
}

#[test]
fn running_work_stays_visible_on_every_screen() {
    use runtime::tunnels::{Record, Snapshot, State};
    let mut m = model(serde_json::json!({}));
    assert!(
        !screen(&mut m, 110, 24).contains("Jobs —"),
        "nothing runs: no bar"
    );
    m.tunnels = vec![Record {
        id: 7,
        snapshot: Snapshot {
            config: "/config".into(),
            host: "alpha".into(),
            profile: None,
            forward: Forward::default(),
        },
        state: State::Active,
        exit_code: None,
        diagnostic: String::new(),
    }];
    m.statuses.insert(
        "beta".into(),
        PingResult {
            method: AvailabilityMethod::Ssh,
            name: "beta".into(),
            status: PingStatus::Connecting,
            duration: Duration::ZERO,
            error: None,
        },
    );
    let expected =
        "Jobs — tunnels: 1 active (F7: follow or stop) • availability: 1 probe(s) running";
    // The list, a sheet, a form, the help and the palette all keep the reminder.
    assert!(screen(&mut m, 110, 24).contains(expected));
    for opener in [
        KeyCode::Char('i'),
        KeyCode::Char('e'),
        KeyCode::F(1),
        KeyCode::F(2),
    ] {
        m.key(key(opener));
        let text = screen(&mut m, 110, 30);
        assert!(text.contains(expected), "{opener:?}: {text}");
    }
    // A finished tunnel and a settled probe leave the bar.
    m.tunnels[0].state = State::Stopped;
    m.statuses.clear();
    assert!(!screen(&mut m, 110, 30).contains("Jobs —"));
}

#[test]
fn help_explains_the_screen_and_can_be_searched() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('?')));
    let text = screen(&mut m, 120, 50);
    assert!(text.contains("This screen: the host list."), "{text}");
    assert!(text.contains("/: search"), "{text}");
    // Searching lists the matching lines, with the keys of the active keymap.
    m.key(key(KeyCode::Char('/')));
    typing(&mut m, "dupliquer");
    let text = screen(&mut m, 120, 50);
    assert!(text.contains("D         Dupliquer"), "{text}");
    assert!(!text.contains("Se connecter"), "{text}");
    assert!(text.contains("Search: dupliquer_"), "{text}");
    // While searching, the closing letters are text; Esc clears the search first.
    typing(&mut m, "q");
    assert!(matches!(m.view, View::Help));
    assert!(screen(&mut m, 120, 50).contains("No line of the help matches."));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Help));
    assert!(screen(&mut m, 120, 50).contains("Se connecter"));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    // From a form, the help says what that form is for, and returns to it.
    m.key(key(KeyCode::Char('e')));
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "aide");
    m.key(key(KeyCode::Enter));
    assert!(screen(&mut m, 120, 50).contains("This screen: the host form."));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Host(_)));
}

#[test]
fn a_write_is_confirmed_in_words_until_the_next_key() {
    let (_temp, root, mut m) = review_model();
    let host = m.all[0].clone();
    let remaining = vec![m.all[1].clone()];
    m.complete_mutation(remaining, &Action::Delete(host));
    let text = screen(&mut m, 110, 24);
    assert!(text.contains("✓ Deleted alpha"), "{text}");
    m.key(key(KeyCode::Down));
    assert!(!screen(&mut m, 110, 24).contains("✓ Deleted"));
    drop(root);
}
