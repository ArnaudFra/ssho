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

/// These journeys read the French interface: English, the default language, is
/// what the other test modules read.
fn french() -> AppConfig {
    AppConfig {
        language: crate::i18n::Language::Fr,
        ..AppConfig::default()
    }
}

/// Table focus by default; `preferences` receives the `key_bindings` object.
fn model(key_bindings: serde_json::Value) -> Model {
    let preferences: AppConfig = serde_json::from_value(
        serde_json::json!({ "key_bindings": key_bindings, "language": "fr" }),
    )
    .unwrap();
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
    let mut m = Model::new(vec![], french(), BTreeMap::new(), false);
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
    // The French rows are longer: the reminder needs a wider terminal.
    assert!(screen(&mut enabled, 140, 30).contains("Ctrl+G : rapide"));
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
    assert!(footer.contains("E : modifier"), "{footer}");
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
    assert!(screen(&mut m, 100, 20).contains("Échap ne quitte pas la liste"));
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
    assert!(
        matches!(m.view, View::GroupMode { selected } if selected == groups::modes().len() - 1)
    );
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
        editing.contains("● Recherche") && editing.contains("Recherche : > "),
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
        !browsing.contains("Recherche : > ") && !browsing.contains("Recherche──"),
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
    assert!(text.contains("Recherche : Guillemet non fermé"), "{text}");
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
        "Utilisateur       selon OpenSSH",
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
    assert!(screen(&mut m, 110, 30).contains("> Utilisateur       selon OpenSSH"));
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
    // "Tous les fichiers" is a choice of its own, applied like any other.
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
    let mut m = Model::new(hosts, french(), BTreeMap::new(), false);
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
    assert!(text.contains("Entrée : déplier"), "{text}");
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
    assert!(screen(&mut m, 120, 24).contains("Entrée : connexion"));
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
            column_of(&screen(&mut m, width, 50), "Dernière tentative") < 140,
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
            column_of(&screen(&mut m, width, 50), "État") < 160,
            "{width}"
        );
        m.key(key(KeyCode::Esc));
        m.tunnel_profiles.insert(
            "alpha".into(),
            BTreeMap::from([("db".to_owned(), Forward::default())]),
        );
        m.key(key(KeyCode::F(6)));
        assert!(
            column_of(&screen(&mut m, width, 50), "Transfert") < 160,
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
        "Note personnelle",
        "Hôte : alpha",
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
    assert!(screen(&mut m, 120, 40).contains("Note personnelle: Bastion requis"));
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
        text.contains("Écouter sur ce poste, port ? → joindre localhost:? depuis le serveur SSH"),
        "{text}"
    );
    assert!(
        text.contains("Port local (écoute sur ce poste) :"),
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
            "Écouter sur ce poste, port 5433 → joindre db.internal:5432 depuis le serveur SSH"
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
            "Écouter sur le serveur SSH, port 5433 → joindre db.internal:5432 depuis ce poste"
        ),
        "{text}"
    );
    assert!(
        text.contains("Port distant (écoute sur le serveur SSH) :"),
        "{text}"
    );
    m.key(key(KeyCode::Right));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains(
            "Proxy SOCKS sur ce poste, port 5433 → les connexions sortent par le serveur SSH"
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
        text.contains("> (•) Session shell   ( ) Tunnel seul"),
        "{text}"
    );
    assert!(
        text.contains("le tunnel s'arrête quand vous quittez ce shell"),
        "{text}"
    );
    m.key(key(KeyCode::Char(' ')));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("( ) Session shell   (•) Tunnel seul"),
        "{text}"
    );
    assert!(text.contains("suivi et arrêt dans F7"), "{text}");
    m.key(key(KeyCode::Tab));
    let text = screen(&mut m, 110, 40);
    assert!(
        text.contains("[▶ Lancer ]  [  Enregistrer le profil ]  [  Annuler ]"),
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
            "Écouter sur ce poste, port 5433 → joindre db.internal:5432 depuis le serveur SSH"
        ),
        "{text}"
    );
    assert!(
        text.contains("[▶ Ouvrir ] [  Lancer ] [  Dupliquer ] [  Supprimer ] [  Nouveau ]"),
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
    assert!(screen(&mut m, 120, 30).contains("Supprimer le profil « db » ?"));
    m.key(key(KeyCode::Esc));
    // Without any profile only New can act.
    m.tunnel_profiles.clear();
    assert!(screen(&mut m, 120, 30).contains("[▶ Nouveau ]"));
    m.key(key(KeyCode::Enter));
    assert!(forward_form(&m).original_name.is_none());
}

#[test]
fn no_target_page_never_turns_the_filter_into_targets_on_its_own() {
    let (_temp, mut m) = marking_model(GroupMode::Flat, false);
    m.key(key(KeyCode::Char('B')));
    let text = screen(&mut m, 120, 24);
    for expected in [
        "Lot : 0 cibles • [Cibles] → Commande → Vérification → Résultats",
        "Aucune cible sélectionnée.",
        "le filtre n'est jamais transformé en cibles de lui-même",
        "[▶ Retour à la liste ]  [  Ajouter les 4 hôte(s) affichés par le filtre ]",
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
    assert!(screen(&mut m, 120, 24).contains("Ajouter les 1 hôte(s) affichés par le filtre"));
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
        .contains("Lot : 1 cibles • Cibles ✓ → [Commande] → Vérification → Résultats"));
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
        screen(&mut m, 120, 30).contains("Parallèle : 4"),
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
    assert!(text.contains("[▶ Vérification ]  [  Retour ]"), "{text}");
    assert!(!m.batch.manager.has_active());
    // The list announces when the arrows are its own.
    m.key(key(KeyCode::BackTab));
    assert!(screen(&mut m, 120, 30).contains("● Cibles figées — ↑↓ choisir"));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("Confirmer le lot : 4 cibles"), "{text}");
    assert!(
        text.contains("Commande : uptime") && text.contains("Parallèle : 4 • Délai : 30 s"),
        "{text}"
    );
    assert!(text.contains("[  Lancer sur 4 cible(s) ]"), "{text}");
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
        text.contains("Clés — Assistant OpenSSH • cible : alpha"),
        "{text}"
    );
    assert!(
        text.contains("Local : [▶ Créer une clé ] [  Ajouter à l'agent ] [  Associer à cet hôte ] • Serveur distant : [  Déployer la clé publique ]"),
        "{text}"
    );
    // Without a reviewed public key, deploying explains itself and does nothing.
    m.key(key(KeyCode::Left));
    assert!(screen(&mut m, 130, 30).contains("[▶ Déployer la clé publique ]"));
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
            "Choisir une clé pour ce brouillon : seul son IdentityFile est rempli, rien n'est déployé"
        ),
        "{text}"
    );
    assert!(
        text.contains("Local : [▶ Utiliser cette clé ] [  Créer une clé ] [  Ajouter à l'agent ]"),
        "{text}"
    );
    assert!(
        !text.contains("Déployer la clé publique") && !text.contains("Associate"),
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
        text.contains("ssh-keygen demande ensuite la phrase secrète dans le terminal"),
        "{text}"
    );
    assert!(
        text.contains("[  Continuer dans le terminal ]  [  Retour ]"),
        "{text}"
    );
    // Enter in the first field no longer generates.
    typing(&mut m, "/tmp/new-key");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    typing(&mut m, "comment");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(screen(&mut m, 120, 30).contains("[▶ Continuer dans le terminal ]"));
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
    let mut m = Model::new(hosts, french(), BTreeMap::new(), false);
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
        "Opération : SUPPRIMER l'hôte « alpha »",
        &format!("Fichier : {}", root.display()),
        "Son favori, sa note et ses profils de tunnels restent dans les données de ssho.",
        "[  Supprimer ]  [  Retour ]  (lecture du diff)",
        "Ctrl+S : confirmer ces modifications",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Enter repeated from the list, or while reading, confirms nothing.
    for _ in 0..3 {
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    }
    assert!(screen(&mut m, 120, 30).contains("Rien d'écrit : Tab atteint les boutons"));
    assert_eq!(std::fs::read(&root).unwrap(), before);
    assert!(matches!(m.view, View::Changes(_)));
    // Tab reaches Delete, then Back; each does what its name says.
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 30).contains("[▶ Supprimer ]"));
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 120, 30).contains("[▶ Retour ]"));
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
    assert!(
        text.contains("Opération : MODIFIER l'hôte « alpha »"),
        "{text}"
    );
    assert!(
        text.contains(
            "Alias : alpha → gamma. Favori, note et profils de tunnels suivent un alias renommé."
        ),
        "{text}"
    );
    assert!(text.contains("[  Enregistrer ]  [  Retour ]"), "{text}");
    m.key(key(KeyCode::Esc));
    // A move shows where from and where to.
    std::fs::write(&other, "").unwrap();
    changes::open(&mut m, Action::MoveTo(original, other.clone()), &root).unwrap();
    let text = screen(&mut m, 130, 30);
    assert!(
        text.contains("Opération : DÉPLACER l'hôte « alpha »"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "De : {} → Vers : {}",
            root.display(),
            other.display()
        )),
        "{text}"
    );
    assert!(text.contains("[  Déplacer ]  [  Retour ]"), "{text}");
}

#[test]
fn columns_show_their_outcome_and_moving_is_a_named_action() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('v')));
    let text = screen(&mut m, 120, 30);
    for expected in [
        "[x] Nom (fixe)",
        "Affichées à cette largeur : Nom | Adresse | Tags | Dernière tentative",
        "Toutes les colonnes actives tiennent.",
        "[  Monter ] [  Descendre ] [  Réinitialiser ] [  Appliquer ] [  Annuler ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // A narrow table: the active column short of room is told apart from the disabled one.
    let narrow = screen(&mut m, 60, 30);
    assert!(narrow.contains("Actives mais sans place : "), "{narrow}");
    assert!(narrow.contains("(masquée : largeur)"), "{narrow}");
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
    assert!(screen(&mut m, 120, 30).contains("[▶ Monter ]"));
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
    assert!(screen(&mut m, 120, 30).contains("[▶ Appliquer ]"));
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
    assert!(
        text.contains("Erreur : refusing application file"),
        "{text}"
    );
    assert!(text.contains("(! : texte complet)"), "{text}");
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
    assert!(
        text.contains("[Erreur]") && joined.contains(cause),
        "{text}"
    );
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
        !screen(&mut m, 110, 24).contains("Travaux —"),
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
        "Travaux — tunnels : 1 actif(s) (F7 : suivre ou arrêter) • disponibilité : 1 sonde(s) en cours";
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
    assert!(!screen(&mut m, 110, 30).contains("Travaux —"));
}

#[test]
fn help_explains_the_screen_and_can_be_searched() {
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('?')));
    let text = screen(&mut m, 120, 50);
    assert!(text.contains("Cet écran : la liste des hôtes."), "{text}");
    assert!(text.contains("/ : rechercher"), "{text}");
    // Searching lists the matching lines, with the keys of the active keymap.
    m.key(key(KeyCode::Char('/')));
    typing(&mut m, "dupliquer");
    let text = screen(&mut m, 120, 50);
    assert!(text.contains("D         Dupliquer"), "{text}");
    assert!(!text.contains("Se connecter"), "{text}");
    assert!(text.contains("Recherche : dupliquer_"), "{text}");
    // While searching, the closing letters are text; Esc clears the search first.
    typing(&mut m, "q");
    assert!(matches!(m.view, View::Help));
    assert!(screen(&mut m, 120, 50).contains("Aucune ligne de l'aide ne correspond."));
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
    assert!(screen(&mut m, 120, 50).contains("Cet écran : le formulaire d'hôte."));
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
    assert!(text.contains("✓ alpha supprimé"), "{text}");
    m.key(key(KeyCode::Down));
    assert!(!screen(&mut m, 110, 24).contains("supprimé"));
    drop(root);
}

fn english() -> Model {
    let mut m = Model::new(hosts(), AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    m
}

#[test]
fn english_is_the_default_and_french_follows_the_preference() {
    let mut m = english();
    let text = screen(&mut m, 120, 24);
    for expected in [
        "● Hosts",
        "/ Search • 2 hosts",
        "F2: actions",
        "Enter: connect",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let mut m = model(serde_json::json!({}));
    let text = screen(&mut m, 120, 24);
    for expected in [
        "● Hôtes",
        "/ Rechercher • 2 hôtes",
        "F2 : actions",
        "Entrée : connexion",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // An unknown or absent value never blocks the start: English is used.
    for value in [
        serde_json::json!({}),
        serde_json::json!({ "language": null }),
        serde_json::json!({ "language": "xx" }),
    ] {
        let preferences: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(preferences.language, crate::i18n::Language::En);
    }
}

#[test]
fn language_is_chosen_in_the_application_saved_and_applied_at_once() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut m = english();
    // The action is found in either language, whichever one is displayed.
    for words in ["language", "langue"] {
        m.key(key(KeyCode::F(2)));
        typing(&mut m, words);
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
        assert!(matches!(m.view, View::Language { selected: 0 }), "{words}");
        m.key(key(KeyCode::Esc));
        assert!(matches!(m.view, View::List));
    }
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "language");
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 100, 24);
    for expected in [
        // The title is readable from both languages.
        "Language / Langue de l'interface",
        "▶✓ English",
        "   Français",
        "Enter: save",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Cancelling changes nothing, on screen or on disk.
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Esc));
    assert_eq!(m.preferences.language, crate::i18n::Language::En);
    assert!(!root.join("config.json").exists());
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "language");
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Down));
    let Action::Language(language) = m.key(key(KeyCode::Enter)) else {
        panic!("Enter applies the selected language")
    };
    assert_eq!(language, crate::i18n::Language::Fr);
    save_language(&mut m, root, language).unwrap();
    assert!(matches!(m.view, View::List));
    let text = screen(&mut m, 120, 24);
    assert!(
        text.contains("● Hôtes") && text.contains("F2 : actions"),
        "{text}"
    );
    // Texts kept in the model follow as well.
    m.key(key(KeyCode::Char('/')));
    assert!(
        screen(&mut m, 120, 24).contains("Mots, tag:"),
        "placeholder"
    );
    assert_eq!(
        AppConfig::load(root).unwrap().language,
        crate::i18n::Language::Fr
    );
    let saved = std::fs::read_to_string(root.join("config.json")).unwrap();
    assert!(saved.contains("\"language\": \"fr\""), "{saved}");
}

#[test]
fn both_languages_draw_every_screen_and_keep_the_legend_whole() {
    use unicode_width::UnicodeWidthStr;
    for language in crate::i18n::Language::ALL {
        let openers: [&[KeyCode]; 8] = [
            &[],
            &[KeyCode::F(2)],
            &[KeyCode::F(1)],
            &[KeyCode::Char('i')],
            &[KeyCode::Char('e')],
            &[KeyCode::Char('f')],
            &[KeyCode::Char('g')],
            &[KeyCode::Char('v')],
        ];
        for keys in openers {
            let preferences = AppConfig {
                language,
                ..AppConfig::default()
            };
            let mut m = Model::new(hosts(), preferences, BTreeMap::new(), false);
            m.search_focus = false;
            for code in keys {
                m.key(key(*code));
            }
            for (width, height) in [(80, 24), (40, 12)] {
                let text = screen(&mut m, width, height);
                assert!(
                    text.lines().all(|row| row.width() <= usize::from(width)),
                    "{language:?} {keys:?} {width}: {text}"
                );
            }
            // The list legend is never cut: a row ends with a whole word.
            if keys.is_empty() {
                let text = screen(&mut m, 80, 24);
                let legend = text.lines().rev().find(|row| row.contains("F2")).unwrap();
                let end = crate::t!("quit", "quitter");
                assert!(legend.trim_end().ends_with(end), "{language:?}: {legend}");
            }
        }
    }
}

#[test]
fn without_colours_every_state_keeps_a_text_a_sign_or_an_emphasis() {
    use ratatui::{backend::TestBackend, style::Color, style::Modifier, Terminal};
    let draw = |m: &mut Model| {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| render::draw(frame, m)).unwrap();
        terminal.backend().buffer().clone()
    };
    let mut m = english();
    m.error = "Error: unreadable file".into();
    let coloured = draw(&mut m);
    assert!(coloured.content.iter().any(|cell| cell.fg != Color::Reset));
    // The preference and the NO_COLOR environment variable give the same frame.
    for from_environment in [false, true] {
        let mut m = english();
        m.error = "Error: unreadable file".into();
        m.preferences.colors = from_environment;
        m.no_color = from_environment;
        let plain = draw(&mut m);
        assert!(
            plain
                .content
                .iter()
                .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset),
            "no cell keeps a colour"
        );
        // Same characters as the coloured frame: nothing was told by colour alone.
        let text = |buffer: &ratatui::buffer::Buffer| -> String {
            buffer.content.iter().map(|cell| cell.symbol()).collect()
        };
        assert_eq!(text(&plain), text(&coloured));
        assert!(text(&plain).contains("Error: unreadable file"));
        // The selected row is still told apart from the others.
        let emphasised = |buffer: &ratatui::buffer::Buffer, name: &str| {
            let row = (0..buffer.area.height)
                .find(|y| {
                    let line: String = (0..buffer.area.width)
                        .map(|x| buffer[(x, *y)].symbol())
                        .collect();
                    line.contains(name) && line.contains('│')
                })
                .unwrap();
            (0..buffer.area.width).any(|x| {
                let modifier = buffer[(x, row)].modifier;
                modifier.contains(Modifier::REVERSED) || modifier.contains(Modifier::BOLD)
            })
        };
        assert!(
            emphasised(&plain, "alpha"),
            "the selection keeps an emphasis"
        );
        assert!(!emphasised(&plain, "beta"));
    }
    // With NO_COLOR the palette explains why its toggle cannot turn colours on.
    let mut m = english();
    m.no_color = true;
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "colours");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(m.hint.contains("NO_COLOR"), "{}", m.hint);
    let mut m = english();
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "colours");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::ToggleColors);
}

#[test]
fn ascii_symbols_replace_frames_and_signs_and_are_saved() {
    let temp = tempfile::tempdir().unwrap();
    let mut m = english();
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "ascii");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::ToggleAscii);
    save_display(&mut m, temp.path(), |config| config.ascii = !config.ascii).unwrap();
    assert!(matches!(m.view, View::List), "the palette is closed");
    assert!(AppConfig::load(temp.path()).unwrap().ascii);
    for opener in [None, Some(KeyCode::F(2)), Some(KeyCode::Char('e'))] {
        if let Some(code) = opener {
            m.key(key(code));
        }
        let text = screen(&mut m, 100, 24);
        assert!(text.is_ascii(), "{opener:?}: {text}");
        m.key(key(KeyCode::Esc));
    }
    let text = screen(&mut m, 100, 24);
    for expected in ["+o Hosts", "|?  alpha", "F2: actions | ", "^v: select"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // User data is not rewritten: only interface symbols are.
    m.all[0].name = "界-alpha".into();
    m.filter(true);
    let text = screen(&mut m, 100, 24);
    assert!(text.contains('界') && text.contains("-alpha"), "{text}");
}

#[test]
fn a_list_too_small_to_be_used_says_so_and_forms_keep_their_caret() {
    let mut m = english();
    let text = screen(&mut m, 23, 8);
    assert!(text.contains("Terminal too small"), "{text}");
    let joined: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(joined.contains("24x6"), "{joined}");
    assert!(!screen(&mut m, 24, 6).contains("Terminal too small"));
    let mut m = model(serde_json::json!({}));
    assert!(screen(&mut m, 60, 5).contains("Terminal trop petit"));
    // A form keeps its compact layout: the notice never replaces a draft.
    m.key(key(KeyCode::Char('e')));
    assert!(!screen(&mut m, 20, 5).contains("Terminal trop petit"));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}

fn preferences_of(m: &Model) -> &prefs::Editor {
    let View::Preferences(editor) = &m.view else {
        panic!("the preferences screen is open")
    };
    editor
}

/// Move the focus of the preferences screen to `row`.
fn focus_row(m: &mut Model, row: prefs::Row) {
    let target = prefs::rows().iter().position(|r| *r == row).unwrap();
    m.key(key(KeyCode::Home));
    for _ in 0..target {
        m.key(key(KeyCode::Down));
    }
    assert_eq!(preferences_of(m).row, target);
}

#[test]
fn preferences_show_each_option_with_its_value_effect_and_moment() {
    let mut m = model(serde_json::json!({}));
    // One named action, with a key, also listed in the palette.
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "preferences");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::Preferences(_)));
    m.key(key(KeyCode::Esc));
    assert!(
        matches!(m.view, View::List),
        "an untouched draft closes at once"
    );
    m.key(key(KeyCode::Char('o')));
    let text = screen(&mut m, 110, 44);
    for expected in [
        "Préférences",
        "Apparence",
        "Liste des hôtes",
        "Connexion et diagnostics",
        "Clavier",
        "Données (lecture seule)",
        "> Langue",
        "Français",
        "Langue de tous les écrans : anglais ou français.",
        "Prend effet dès l'application.",
        "[  Appliquer ]  [  Annuler ]  [  Valeurs par défaut ]",
        "Aucune modification",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // An option read at start says so instead of pretending to apply now.
    focus_row(&mut m, prefs::Row::PingOnStart);
    let text = screen(&mut m, 110, 44);
    assert!(
        text.contains("Prend effet au prochain démarrage."),
        "{text}"
    );
    // The launcher is explained with its real scope, and is not editable here.
    focus_row(&mut m, prefs::Row::SshCommand);
    let text = screen(&mut m, 110, 44);
    assert!(text.contains("Liste d'arguments littéraux"), "{text}");
    assert!(text.contains("tunnels, lots, ssh -G et sondes"), "{text}");
    m.key(key(KeyCode::Char(' ')));
    assert!(preferences_of(&m).notice.contains("Lecture seule"));
    assert!(!preferences_of(&m).dirty());
    // On a very wide terminal the buttons stay next to the options.
    let wide = screen(&mut m, 300, 44);
    assert!(column_of(&wide, "Appliquer ]") < 12, "{wide}");
    // A small terminal keeps the focused option and the buttons.
    let small = screen(&mut m, 80, 14);
    assert!(
        small.contains("> Lanceur SSH") && small.contains("Appliquer ]"),
        "{small}"
    );
}

#[test]
fn nothing_is_written_before_apply_and_a_modified_draft_is_not_dropped() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut m = model(serde_json::json!({}));
    m.preferences.save(root).unwrap();
    let before = std::fs::read(root.join("config.json")).unwrap();
    m.key(key(KeyCode::Char('o')));
    focus_row(&mut m, prefs::Row::Colors);
    m.key(key(KeyCode::Char(' ')));
    focus_row(&mut m, prefs::Row::GroupMode);
    m.key(key(KeyCode::Right));
    let text = screen(&mut m, 110, 44);
    assert!(text.contains("● modifié"), "{text}");
    assert!(
        text.contains("● Modifications non enregistrées — rien n'est écrit avant Appliquer"),
        "{text}"
    );
    assert!(
        m.preferences.colors,
        "the running preferences are untouched"
    );
    assert_eq!(std::fs::read(root.join("config.json")).unwrap(), before);
    // Esc asks, with the focus on continuing; Enter keeps the draft.
    m.key(key(KeyCode::Esc));
    let text = screen(&mut m, 110, 44);
    assert!(text.contains("[▶ Continuer l'édition ]"), "{text}");
    m.key(key(KeyCode::Enter));
    assert!(preferences_of(&m).dirty());
    // An option edited elsewhere meanwhile is not overwritten by Apply.
    AppConfig::update(root, |config| {
        config.ssh_command = vec!["wrapper".into(), "--flag".into()];
    })
    .unwrap();
    // Enter on a row only changes the draft; Apply is a button or Ctrl+S.
    m.key(key(KeyCode::Tab));
    let Action::Preferences(draft) = m.key(key(KeyCode::Enter)) else {
        panic!("the Apply button returns the draft")
    };
    assert!(!draft.colors);
    save_preferences(&mut m, root, &draft).unwrap();
    assert!(matches!(m.view, View::List));
    assert!(!m.preferences.colors);
    assert_eq!(m.preferences.group_mode, GroupMode::Files);
    assert!(m.hint.contains("✓ Préférences enregistrées"), "{}", m.hint);
    let saved = AppConfig::load(root).unwrap();
    assert!(!saved.colors);
    assert_eq!(saved.ssh_command, ["wrapper", "--flag"]);
    // The grouping applies at once: the list shows its file headers.
    assert!(!m.rows.is_empty());
}

#[test]
fn restore_defaults_fills_the_draft_and_writes_nothing() {
    let mut m = model(serde_json::json!({ "prefix_key": "" }));
    m.preferences.ping_on_start = false;
    m.preferences.host_columns = vec![HostColumn::Name, HostColumn::Port];
    m.key(key(KeyCode::Char('o')));
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    assert_eq!(preferences_of(&m).button, Some(2));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    let editor = preferences_of(&m);
    assert!(editor.draft.ping_on_start);
    assert_eq!(editor.draft.language, crate::i18n::Language::En);
    assert_eq!(
        editor.draft.key_bindings,
        crate::storage::KeyBindings::default()
    );
    // Options without a row here keep their value.
    assert_eq!(
        editor.draft.host_columns,
        [HostColumn::Name, HostColumn::Port]
    );
    assert!(
        editor.notice.contains("Appliquer les écrit"),
        "{}",
        editor.notice
    );
    assert!(
        !m.preferences.ping_on_start,
        "nothing is adopted before Apply"
    );
    // The focus moved to Apply: one more Enter is an explicit decision.
    assert_eq!(preferences_of(&m).button, Some(0));
}

#[test]
fn data_locations_are_found_from_the_interface() {
    let mut m = model(serde_json::json!({}));
    m.batch.history_root = Some("/home/me/.config/ssho".into());
    m.batch.root = "/home/me/.ssh/config".into();
    m.key(key(KeyCode::Char('o')));
    focus_row(&mut m, prefs::Row::DataMetadata);
    let text = screen(&mut m, 110, 44);
    for expected in [
        "Favoris et notes",
        "/home/me/.config/ssho/metadata.json",
        "/home/me/.config/ssho/config.json",
        "/home/me/.config/ssho/sshm_history.json",
        "/home/me/.config/ssho/tunnel_profiles.json",
        "/home/me/.config/ssho/backups",
        "/home/me/.ssh/config",
        "Jamais écrits dans les fichiers",
        "cet écran ne l'écrit jamais",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // In a narrow terminal the focused path is repeated in full under the list.
    let narrow = screen(&mut m, 50, 30);
    let joined: String = narrow.split_whitespace().collect();
    assert!(
        joined.contains("/home/me/.config/ssho/metadata.json"),
        "{narrow}"
    );
}

fn shortcuts_of(m: &Model) -> &prefs::Shortcuts {
    let View::Shortcuts(editor) = &m.view else {
        panic!("the shortcut editor is open")
    };
    editor
}

#[test]
fn shortcut_editor_changes_a_key_refuses_conflicts_and_returns_to_the_draft() {
    let temp = tempfile::tempdir().unwrap();
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('o')));
    focus_row(&mut m, prefs::Row::Shortcuts);
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 110, 30);
    for expected in [
        "Préférences > Raccourcis",
        "Touches",
        "Origine",
        "▶ Se connecter",
        "[  Changer la touche ]  [  Aucune touche ]  [  Défaut ]  [  Terminer ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let actions = prefs::shortcut_actions();
    let edit = actions
        .iter()
        .position(|a| *a == PaletteAction::Edit)
        .unwrap();
    for _ in 0..edit {
        m.key(key(KeyCode::Down));
    }
    // Enter waits for a key; a letter is then a key, never a command.
    m.key(key(KeyCode::Enter));
    assert!(shortcuts_of(&m).capture);
    assert!(screen(&mut m, 110, 30).contains("En attente d'une touche"));
    m.key(key(KeyCode::Char('d')));
    let notice = shortcuts_of(&m).notice.clone();
    assert!(
        notice.contains("Refusé") && notice.contains("d sert déjà à « Supprimer »"),
        "{notice}"
    );
    assert!(shortcuts_of(&m).bindings.shortcuts.is_empty());
    // Esc while waiting keeps the current key and stays in the editor.
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Shortcuts(_)));
    assert!(shortcuts_of(&m).notice.contains("Touche inchangée"));
    // A navigation key is refused with the reason of the keymap.
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Up));
    assert!(shortcuts_of(&m).notice.contains("réservé à la navigation"));
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Char('E')));
    let text = screen(&mut m, 110, 30);
    assert!(
        text.contains("E lance maintenant « Modifier » dans ce brouillon"),
        "{text}"
    );
    let row = text
        .lines()
        .find(|line| line.contains("▶ Modifier"))
        .unwrap();
    assert!(row.contains(" E ") && row.contains("personnalisé"), "{row}");
    // Done hands the shortcuts to the preferences draft; nothing is written yet.
    for _ in 0..4 {
        m.key(key(KeyCode::Tab));
    }
    assert_eq!(shortcuts_of(&m).button, Some(3));
    m.key(key(KeyCode::Enter));
    let editor = preferences_of(&m);
    assert_eq!(editor.draft.key_bindings.shortcuts["host.edit"], ["E"]);
    assert!(editor.notice.contains("Appliquer les écrit"));
    assert!(m.keymap().shortcut(PaletteAction::Edit).as_deref() == Some("e"));
    let text = screen(&mut m, 110, 44);
    assert!(text.contains("1 personnalisé(s)"), "{text}");
    let Action::Preferences(draft) = m.key(ctrl('s')) else {
        panic!("Ctrl+S applies")
    };
    save_preferences(&mut m, temp.path(), &draft).unwrap();
    assert_eq!(
        m.keymap().shortcut(PaletteAction::Edit).as_deref(),
        Some("E")
    );
    // The new key runs the action, the old one no longer does.
    m.key(key(KeyCode::Char('e')));
    assert!(matches!(m.view, View::List));
    m.key(key(KeyCode::Char('E')));
    assert!(matches!(m.view, View::Host(_)));
}

#[test]
fn shortcut_editor_removes_and_restores_a_key_and_cancel_asks() {
    let mut m = model(serde_json::json!({ "shortcuts": { "host.edit": ["E"] } }));
    m.key(key(KeyCode::Char('o')));
    focus_row(&mut m, prefs::Row::Shortcuts);
    m.key(key(KeyCode::Enter));
    let edit = prefs::shortcut_actions()
        .iter()
        .position(|a| *a == PaletteAction::Edit)
        .unwrap();
    for _ in 0..edit {
        m.key(key(KeyCode::Down));
    }
    assert_eq!(shortcuts_of(&m).state(PaletteAction::Edit), "personnalisé");
    // "Default" drops the customisation.
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    m.key(key(KeyCode::Enter));
    assert_eq!(shortcuts_of(&m).state(PaletteAction::Edit), "défaut");
    // "No key" leaves the action to the palette.
    m.key(key(KeyCode::Left));
    m.key(key(KeyCode::Enter));
    assert_eq!(shortcuts_of(&m).state(PaletteAction::Edit), "F2 seul");
    let text = screen(&mut m, 110, 30);
    let row = text
        .lines()
        .find(|line| line.contains("▶ Modifier"))
        .unwrap();
    assert!(row.contains("—"), "{row}");
    // Leaving a modified editor asks first; discarding returns to the draft unchanged.
    m.key(key(KeyCode::Esc));
    assert!(screen(&mut m, 110, 30).contains("[▶ Continuer l'édition ]"));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    let editor = preferences_of(&m);
    assert_eq!(editor.draft.key_bindings.shortcuts["host.edit"], ["E"]);
    assert!(!editor.dirty());
}

#[test]
fn empty_list_tells_apart_empty_configuration_filters_and_read_error() {
    let empty = || {
        let mut m = Model::new(vec![], french(), BTreeMap::new(), false);
        m.search_focus = false;
        m.batch.root = "/home/me/.ssh/config".into();
        m
    };
    let mut m = empty();
    let text = screen(&mut m, 110, 20);
    assert!(
        text.contains("Aucun hôte dans /home/me/.ssh/config. a : ajouter un hôte"),
        "{text}"
    );
    // The key named by the message is the one of the keymap, and it works.
    assert_eq!(m.key(key(KeyCode::Char('a'))), Action::Add(String::new()));
    let mut m = empty();
    m.error = "permission denied".into();
    let text = screen(&mut m, 110, 20);
    assert!(
        text.contains(
            "La configuration SSH n'a pas pu être lue. ! : voir l'erreur • F5 : recharger"
        ),
        "{text}"
    );
    let mut m = model(serde_json::json!({}));
    m.key(key(KeyCode::Char('/')));
    typing(&mut m, "zzz");
    m.key(key(KeyCode::Esc));
    let text = screen(&mut m, 110, 20);
    assert!(
        text.contains("Aucun hôte ne correspond aux filtres. Backspace : effacer la recherche • F2 : Effacer tous les filtres"),
        "{text}"
    );
    m.key(key(KeyCode::Backspace));
    assert_eq!(m.visible.len(), 2);
    m.favorites_only = true;
    m.filter(true);
    let text = screen(&mut m, 110, 20);
    assert!(
        text.contains("Aucun favori ne correspond aux filtres. F2 : Favoris seuls"),
        "{text}"
    );
}

#[test]
fn first_launch_notice_stays_readable_in_the_messages() {
    let mut m = model(serde_json::json!({}));
    assert!(m.messages().is_empty());
    m.startup_notice = "Données de l'application copiées de /a/sshm vers /a/ssho.".into();
    m.key(key(KeyCode::Char('!')));
    assert!(matches!(m.view, View::Messages));
    let text = screen(&mut m, 110, 20);
    assert!(
        text.contains("[Info]") && text.contains("copiées de /a/sshm vers /a/ssho"),
        "{text}"
    );
}

fn view_fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    Model,
    crate::storage::views::Views,
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let config = temp.path().join("config");
    std::fs::write(
        &config,
        "Host alpha\n HostName alpha.example\nHost beta\n HostName beta.example\n",
    )
    .unwrap();
    let hosts = crate::config::load_report(&config).unwrap().hosts;
    let mut m = Model::new(hosts, french(), BTreeMap::new(), false);
    m.search_focus = false;
    m.batch.root.clone_from(&config);
    let store = crate::storage::views::Views::load(&root, &config).unwrap();
    m.preferences.save(&root).unwrap();
    (temp, root, config, m, store)
}

fn views_of(m: &Model) -> &views::Editor {
    let View::Views(editor) = &m.view else {
        panic!("the saved views screen is open")
    };
    editor
}

/// Run the action a key returned, the way the event loop does.
fn run_view_action(
    m: &mut Model,
    store: &mut crate::storage::views::Views,
    root: &Path,
    config: &Path,
    action: Action,
) -> Result<()> {
    match action {
        Action::ApplyView(name) => views::apply(m, root, config, &name),
        Action::SaveView {
            original,
            name,
            view,
        } => views::save(m, store, original.as_deref(), &name, &view),
        Action::DeleteView(name) => views::delete(m, store, &name),
        other => panic!("unexpected action {other:?}"),
    }
}

#[test]
fn a_view_keeps_the_list_context_under_a_name_and_applies_it_back() {
    let (_temp, root, config, mut m, mut store) = view_fixture();
    m.key(key(KeyCode::Char('/')));
    typing(&mut m, "alp");
    m.key(key(KeyCode::Esc));
    m.recent = true;
    m.filter(true);
    // Without any view the screen says what a view keeps and offers to save one.
    m.key(key(KeyCode::Char('V')));
    let text = screen(&mut m, 100, 30);
    for expected in [
        "Vues enregistrées",
        "Aucune vue en cours",
        "Aucune vue enregistrée.",
        "[▶ Enregistrer sous… ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // The other actions explain that there is nothing to act on.
    m.key(key(KeyCode::BackTab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(views_of(&m).notice.contains("Aucune vue enregistrée"));
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    let text = screen(&mut m, 100, 30);
    for expected in [
        "> Nom de la nouvelle vue :",
        // What is about to be kept is shown before it is named.
        "Recherche : alp",
        "Fichier : tous les fichiers",
        "tri par connexion récente",
        "Colonnes : Nom, Adresse, Tags, Dernière tentative",
        "[  Enregistrer ]  [  Annuler ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Letters are the name, never a command; pasted text goes to the name too.
    typing(&mut m, "Produ");
    m.paste("ction");
    assert_eq!(views_of(&m).name.value, "Production");
    // Enter in the name only reaches the Save button.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(store.all().is_empty());
    assert!(screen(&mut m, 100, 30).contains("[▶ Enregistrer ]"));
    let action = m.key(key(KeyCode::Enter));
    assert!(
        matches!(&action, Action::SaveView { original: None, name, .. } if name == "Production")
    );
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    let text = screen(&mut m, 100, 30);
    for expected in [
        "✓ Vue « Production » enregistrée",
        "Vue en cours : Production",
        "▶ Production  ✓ en cours",
        "[▶ Appliquer ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(root.join("views.json").exists());
    m.key(key(KeyCode::Esc));
    assert!(screen(&mut m, 100, 30).contains("Filtres : Vue : Production • alp"));
    // Changing the list marks the view as modified; the saved one is untouched.
    m.key(key(KeyCode::Backspace));
    m.recent = false;
    m.filter(true);
    let text = screen(&mut m, 100, 30);
    assert!(text.contains("Vue : Production (modifiée)"), "{text}");
    assert_eq!(store.all()["Production"].query, "alp");
    m.key(key(KeyCode::Char('V')));
    assert!(screen(&mut m, 100, 30).contains("Vue en cours : Production (modifiée"));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(action, Action::ApplyView("Production".into()));
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    assert!(matches!(m.view, View::List));
    assert_eq!(m.query.value, "alp");
    assert!(m.recent);
    assert_eq!(m.visible.len(), 1);
    assert!(
        m.hint.contains("✓ Vue « Production » appliquée"),
        "{}",
        m.hint
    );
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Vue : Production") && !text.contains("(modifiée)"),
        "{text}"
    );
    // Clearing every filter leaves the view, instead of calling it modified for ever.
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "effacer tous les filtres");
    m.key(key(KeyCode::Enter));
    assert!(m.active_view.is_none());
}

#[test]
fn update_rename_and_delete_are_named_and_never_silent() {
    let (_temp, root, config, mut m, mut store) = view_fixture();
    let sample = m.current_view();
    store.save(None, "Bases", &sample).unwrap();
    store.save(None, "Production", &sample).unwrap();
    m.views.clone_from(store.all());
    m.favorites_only = true;
    m.key(key(KeyCode::Char('V')));
    m.key(key(KeyCode::Down));
    assert_eq!(views_of(&m).selected, 1);
    // Update stores the current list in the selected view, and says so.
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 100, 30).contains("[▶ Mettre à jour ]"));
    let action = m.key(key(KeyCode::Enter));
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    assert!(store.all()["Production"].favorites_only);
    assert!(!store.all()["Bases"].favorites_only);
    assert!(views_of(&m)
        .notice
        .contains("✓ Vue « Production » mise à jour"));
    // Rename: the name is pre-filled, an existing name is refused and explained.
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    assert!(screen(&mut m, 100, 30).contains("[▶ Renommer ]"));
    m.key(key(KeyCode::Enter));
    assert_eq!(views_of(&m).name.value, "Production");
    assert!(screen(&mut m, 100, 30).contains("> Nouveau nom :"));
    for _ in 0.."Production".len() {
        m.key(key(KeyCode::Backspace));
    }
    typing(&mut m, "Bases");
    let action = m.key(ctrl('s'));
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    assert!(
        views_of(&m).notice.contains("existe déjà"),
        "{}",
        views_of(&m).notice
    );
    assert_eq!(
        views_of(&m).mode,
        views::Mode::Rename,
        "the name can be corrected"
    );
    assert_eq!(store.all().len(), 2);
    typing(&mut m, " 2");
    let action = m.key(ctrl('s'));
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    assert!(store.all().contains_key("Bases 2") && !store.all().contains_key("Production"));
    assert!(
        store.all()["Bases 2"].favorites_only,
        "a rename keeps the criteria"
    );
    // An empty name is refused without leaving the step.
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    assert_eq!(m.key(ctrl('s')), Action::None);
    assert!(views_of(&m).notice.contains("Un nom est requis"));
    m.key(key(KeyCode::Esc));
    assert_eq!(views_of(&m).mode, views::Mode::Browse);
    // Delete asks, with the focus on going back.
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    assert!(screen(&mut m, 100, 30).contains("[▶ Supprimer ]"));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Supprimer cette vue ?") && text.contains("[▶ Revenir ]"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(store.all().len(), 2);
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Tab));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(action, Action::DeleteView("Bases 2".into()));
    run_view_action(&mut m, &mut store, &root, &config, action).unwrap();
    assert_eq!(store.all().len(), 1);
    assert!(views_of(&m).notice.contains("✓ Vue « Bases 2 » supprimée"));
    assert!(matches!(m.view, View::Views(_)), "the screen stays open");
}

#[test]
fn a_missing_source_is_reported_and_the_filter_is_never_widened() {
    let (temp, root, config, mut m, mut store) = view_fixture();
    let gone = temp.path().join("gone.conf");
    let mut view = m.current_view();
    view.source = Some(gone.clone());
    view.query = "beta".into();
    store.save(None, "Ancienne", &view).unwrap();
    m.views.clone_from(store.all());
    m.key(key(KeyCode::Char('V')));
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains(&format!("Fichier : {}", gone.display())),
        "{text}"
    );
    let action = m.key(key(KeyCode::Enter));
    let error = run_view_action(&mut m, &mut store, &root, &config, action).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("ne fait plus partie de la configuration SSH")
            && message.contains("Rien n'a été appliqué"),
        "{message}"
    );
    // The event loop keeps the refusal: the screen shows it in full, until the next key.
    m.error = message;
    let text = screen(&mut m, 100, 30);
    let joined: String = text
        .lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("Erreur : La vue « Ancienne » filtre sur")
            && joined.contains("mettez à jour ou supprimez cette vue."),
        "{text}"
    );
    m.key(key(KeyCode::Down));
    assert!(m.error.is_empty());
    // Nothing moved: no query, no source filter, no view in use.
    assert!(m.query.value.is_empty());
    assert!(m.source_filter.is_none());
    assert!(m.active_view.is_none());
    assert!(matches!(m.view, View::Views(_)));
}

#[test]
fn batch_marks_and_presentation_changes_are_told_apart() {
    let (_temp, root, config, mut m, mut store) = view_fixture();
    let before = m.current_view();
    // A batch mark is a moment, not a criterion: it never enters a view.
    m.key(key(KeyCode::Char('x')));
    assert!(!m.batch.marks.is_empty());
    assert_eq!(m.current_view(), before);
    // A view carries the grouping and the columns, and applies them as preferences.
    let mut grouped = before.clone();
    grouped.group_mode = GroupMode::Files;
    grouped.columns = vec![HostColumn::Name, HostColumn::Port];
    store.save(None, "Par fichier", &grouped).unwrap();
    m.views.clone_from(store.all());
    views::apply(&mut m, &root, &config, "Par fichier").unwrap();
    assert_eq!(m.preferences.group_mode, GroupMode::Files);
    assert_eq!(
        m.preferences.host_columns,
        [HostColumn::Name, HostColumn::Port]
    );
    assert_eq!(AppConfig::load(&root).unwrap().group_mode, GroupMode::Files);
    assert!(!m.batch.marks.is_empty(), "applying a view keeps the marks");
    assert_eq!(m.view_state(), Some(("Par fichier", false)));
    // Changing the grouping by hand afterwards is a modification of the view.
    m.preferences.group_mode = GroupMode::Flat;
    assert_eq!(m.view_state(), Some(("Par fichier", true)));
}

use crate::storage::commands::{Commands, SavedCommand, Scope};

/// A screen as running text: frame borders and wrapping removed.
fn flat(text: &str) -> String {
    text.lines()
        .map(|line| line.trim().trim_matches('│').trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn commands_of(m: &Model) -> &commands::Editor {
    let View::Commands(editor) = &m.view else {
        panic!("the command catalogue is open")
    };
    editor
}

/// Run the action a key returned, the way the event loop does.
fn run_command_action(m: &mut Model, store: &mut Commands, action: Action) -> Result<()> {
    match action {
        Action::SaveCommand {
            original,
            name,
            command,
        } => commands::save(m, store, original.as_deref(), &name, &command),
        Action::DeleteCommand(name) => commands::delete(m, store, &name),
        Action::UseCommand { command, batch } => commands::prepare(m, &command, batch),
        other => panic!("unexpected action {other:?}"),
    }
}

fn command_fixture() -> (tempfile::TempDir, Model, Commands) {
    let (temp, root, _config, mut m, _views) = view_fixture();
    m.batch.cache_sources(&m.all.clone());
    let store = Commands::load(&root).unwrap();
    (temp, m, store)
}

fn journal() -> SavedCommand {
    SavedCommand {
        description: "Fin du journal".into(),
        scope: Scope::Any,
        template: "tail -n {lines:number=50} {file=/var/log/syslog}".into(),
    }
}

#[test]
fn a_command_is_written_in_a_named_form_and_enter_never_saves() {
    let (_temp, mut m, mut store) = command_fixture();
    m.key(key(KeyCode::Char('R')));
    let text = flat(&screen(&mut m, 100, 30));
    for expected in [
        "Commandes enregistrées",
        "Hôte sélectionné : alpha • Lot : 0 cible(s) marquée(s)",
        "Aucune commande enregistrée.",
        "vous vérifiez encore les cibles et la commande avant toute exécution",
        "[▶ Nouvelle ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 100, 30);
    for expected in [
        "Commandes enregistrées > Nouvelle commande",
        "> Nom *",
        "Prévue pour  ‹ un hôte ou un lot ›",
        "Commande *",
        "La commande est vide",
        "Chaque valeur est insérée comme un seul mot protégé",
        "N'écrivez jamais de mot de passe ni de secret ici",
        "[  Enregistrer ]  [  Annuler ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    typing(&mut m, "Journal");
    m.key(key(KeyCode::Enter));
    typing(&mut m, "Fin du journal");
    m.key(key(KeyCode::Enter));
    // The scope is a choice, not a text: arrows change it, letters do nothing.
    assert_eq!(commands_of(&m).focus, commands::SCOPE);
    typing(&mut m, "zz");
    m.key(key(KeyCode::Right));
    assert_eq!(commands_of(&m).scope, Scope::Host);
    m.key(key(KeyCode::Left));
    assert_eq!(commands_of(&m).scope, Scope::Any);
    m.key(key(KeyCode::Enter));
    // A malformed command is explained while it is typed, and refused on save.
    typing(&mut m, "tail -n {lines:number=50} {file");
    assert!(screen(&mut m, 100, 30).contains("Accolades non appariées"));
    assert_eq!(m.key(ctrl('s')), Action::None);
    assert_eq!(commands_of(&m).focus, commands::TEMPLATE);
    typing(&mut m, "=/var/log/syslog}");
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains(
            "Demandé à chaque utilisation : lines (nombre entier, défaut 50) • file (texte, défaut"
        ),
        "{text}"
    );
    // Enter in the last field reaches Save and writes nothing.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(commands_of(&m).focus, commands::SAVE);
    assert!(store.all().is_empty());
    let action = m.key(key(KeyCode::Enter));
    assert!(
        matches!(&action, Action::SaveCommand { original: None, name, .. } if name == "Journal")
    );
    run_command_action(&mut m, &mut store, action).unwrap();
    assert_eq!(store.all()["Journal"], journal());
    let text = screen(&mut m, 100, 30);
    for expected in [
        "✓ Commande « Journal » enregistrée",
        "▶ Journal — Fin du journal",
        "Commande : tail -n {lines:number=50} {file=/var/log/syslog}",
        "Prévue pour : un hôte ou un lot",
        "[▶ Utiliser sur cet hôte ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // Editing: a modified form is not dropped by Esc without asking.
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    assert!(screen(&mut m, 100, 30).contains("[▶ Modifier ]"));
    m.key(key(KeyCode::Enter));
    assert!(screen(&mut m, 100, 30).contains("Commandes enregistrées > Modifier Journal"));
    m.key(key(KeyCode::Esc));
    assert_eq!(
        commands_of(&m).mode,
        commands::Mode::Browse,
        "untouched form closes"
    );
    m.key(key(KeyCode::Enter));
    typing(&mut m, "x");
    m.key(key(KeyCode::Esc));
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Abandonner les modifications de cette commande ?")
            && text.contains("[▶ Continuer l'édition ]"),
        "{text}"
    );
    m.key(key(KeyCode::Enter));
    assert_eq!(commands_of(&m).inputs[0].value, "Journalx");
    // A name already taken is refused and explained; the form stays open.
    store.save(None, "Journalx", &journal()).unwrap();
    m.commands.clone_from(store.all());
    let action = m.key(ctrl('s'));
    run_command_action(&mut m, &mut store, action).unwrap();
    assert!(commands_of(&m).notice.contains("existe déjà"));
    assert_eq!(commands_of(&m).mode, commands::Mode::Form);
    assert!(store.all().contains_key("Journal"));
    assert!(m.has_unsaved_context());
}

#[test]
fn using_a_command_asks_its_values_and_only_fills_the_batch_preparation() {
    let (_temp, mut m, mut store) = command_fixture();
    store.save(None, "Journal", &journal()).unwrap();
    m.commands.clone_from(store.all());
    m.key(key(KeyCode::Char('R')));
    m.key(key(KeyCode::Enter));
    assert_eq!(commands_of(&m).mode, commands::Mode::Params);
    let text = screen(&mut m, 100, 30);
    for expected in [
        "Commandes enregistrées > Paramètres",
        "Journal — pour alpha",
        "> lines (nombre entier)",
        "file (texte)",
        "Commande à vérifier : tail -n 50 '/var/log/syslog'",
        "[  Passer à la vérification ]  [  Retour ]",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    // A typed value that is not of its type is explained, and nothing continues.
    typing(&mut m, "; reboot");
    assert!(screen(&mut m, 100, 30).contains("lines : un nombre entier est attendu"));
    assert_eq!(m.key(ctrl('l')), Action::None);
    assert_eq!(commands_of(&m).param_focus, 0);
    assert!(commands_of(&m)
        .notice
        .contains("un nombre entier est attendu"));
    for _ in 0.."50; reboot".len() {
        m.key(key(KeyCode::Backspace));
    }
    typing(&mut m, "20");
    // Enter on a value goes to the next one; shell syntax stays inside its word.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(commands_of(&m).param_focus, 1);
    typing(&mut m, " x; id");
    m.paste("$(id)");
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Commande à vérifier : tail -n 20 '/var/log/syslog x; id$(id)'"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(screen(&mut m, 100, 30).contains("[▶ Passer à la vérification ]"));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(
        action,
        Action::UseCommand {
            command: "tail -n 20 '/var/log/syslog x; id$(id)'".into(),
            batch: false
        }
    );
    run_command_action(&mut m, &mut store, action).unwrap();
    // The batch preparation takes over, with one explicit target.
    let View::Batch(editor) = &m.view else {
        panic!("the batch preparation is open")
    };
    assert_eq!(editor.targets.len(), 1);
    assert_eq!(editor.targets[0].alias, "alpha");
    assert_eq!(
        editor.command.value,
        "tail -n 20 '/var/log/syslog x; id$(id)'"
    );
    assert!(!editor.confirm, "nothing is confirmed, nothing runs");
    assert!(
        m.batch.marks.is_empty(),
        "the marks of the list are not touched"
    );
    assert!(!m.batch.manager.has_active());
    let text = screen(&mut m, 110, 30);
    assert!(
        text.contains("[Commande]") && text.contains("1 cibles"),
        "{text}"
    );
    m.key(key(KeyCode::Esc));
    assert!(
        matches!(m.view, View::List),
        "the preparation returns to the list"
    );
}

#[test]
fn the_catalogue_never_chooses_targets_and_respects_the_intended_scope() {
    let (_temp, mut m, mut store) = command_fixture();
    store
        .save(
            None,
            "Charge",
            &SavedCommand {
                description: String::new(),
                scope: Scope::Any,
                template: "uptime".into(),
            },
        )
        .unwrap();
    store
        .save(
            None,
            "Redémarrage",
            &SavedCommand {
                description: "Un hôte à la fois".into(),
                scope: Scope::Host,
                template: "sudo systemctl restart {service}".into(),
            },
        )
        .unwrap();
    m.commands.clone_from(store.all());
    m.key(key(KeyCode::Char('R')));
    // No mark: the batch use explains, and marks nothing on its own.
    m.key(key(KeyCode::Tab));
    assert!(screen(&mut m, 100, 30).contains("[▶ Utiliser sur le lot ]"));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(commands_of(&m)
        .notice
        .contains("Aucune cible n'est marquée"));
    assert!(m.batch.marks.is_empty());
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Char('x')));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Char('x')));
    assert_eq!(m.batch.marks.len(), 2);
    m.key(key(KeyCode::Char('R')));
    assert!(screen(&mut m, 100, 30).contains("Lot : 2 cible(s) marquée(s)"));
    m.key(key(KeyCode::Tab));
    // A command without parameter goes straight to the preparation.
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(
        action,
        Action::UseCommand {
            command: "uptime".into(),
            batch: true
        }
    );
    run_command_action(&mut m, &mut store, action).unwrap();
    let View::Batch(editor) = &m.view else {
        panic!("the batch preparation is open")
    };
    assert_eq!(editor.targets.len(), 2);
    assert_eq!(editor.command.value, "uptime");
    m.key(key(KeyCode::Esc));
    // A command written for one host is refused for a batch, with the reason.
    m.key(key(KeyCode::Char('R')));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(
        commands_of(&m)
            .notice
            .contains("prévue pour un hôte seulement"),
        "{}",
        commands_of(&m).notice
    );
    // Delete asks first, with the focus on going back.
    for _ in 0..3 {
        m.key(key(KeyCode::Tab));
    }
    assert!(screen(&mut m, 100, 30).contains("[▶ Supprimer ]"));
    m.key(key(KeyCode::Enter));
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Supprimer cette commande ?") && text.contains("[▶ Revenir ]"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert_eq!(store.all().len(), 2);
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Tab));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(action, Action::DeleteCommand("Redémarrage".into()));
    run_command_action(&mut m, &mut store, action).unwrap();
    assert_eq!(store.all().len(), 1);
    assert!(commands_of(&m)
        .notice
        .contains("✓ Commande « Redémarrage » supprimée"));
}
