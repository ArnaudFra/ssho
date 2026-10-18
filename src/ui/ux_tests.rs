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
    m.key(key(KeyCode::Down));
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
    m.show_hidden = true;
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
