use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::fs;
pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}
fn model() -> Model {
    Model::new(
        vec![
            Host {
                name: "alpha".into(),
                hostname: "a.example".into(),
                user: "deploy".into(),
                tags: vec!["work".into()],
                ..Host::default()
            },
            Host {
                name: "beta".into(),
                hostname: "b.example".into(),
                ..Host::default()
            },
            Host {
                name: "secret".into(),
                tags: vec!["hidden".into()],
                ..Host::default()
            },
        ],
        AppConfig::default(),
        BTreeMap::new(),
        false,
    )
}
/// Leave a modified draft: the confirmation focuses "continue editing", so the
/// discard choice must be selected explicitly.
fn discard(m: &mut Model) {
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    assert!(!m.discard);
}
/// Open F2, find one action by typing words of its name, and run it with Enter.
pub(super) fn palette(m: &mut Model, words: &str) -> Action {
    m.key(key(KeyCode::F(2)));
    assert!(matches!(m.view, View::Actions(_)), "F2 opens the palette");
    typing(m, words);
    m.key(key(KeyCode::Enter))
}
pub(super) fn typing(m: &mut Model, s: &str) {
    for c in s.chars() {
        m.key(key(KeyCode::Char(c)));
    }
}
pub(super) fn screen(m: &mut Model, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| render::draw(f, m)).unwrap();
    t.backend()
        .buffer()
        .content()
        .chunks(w as usize)
        .map(|r| r.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn search_first_selection_and_escape() {
    let mut m = model();
    assert!(m.search_focus);
    assert_eq!(m.visible.len(), 2);
    m.key(ctrl('n'));
    assert_eq!(m.selected, 1);
    typing(&mut m, "deploy work");
    assert_eq!(m.visible.len(), 1);
    assert_eq!(m.selected, 0);
    assert!(m.search_focus);
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), None)
    );
    // Esc closes the edit; the filter stays applied until it is cleared explicitly.
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.search_focus);
    assert_eq!(m.query.value, "deploy work");
    assert_eq!(m.visible.len(), 1);
    assert_eq!(m.key(key(KeyCode::Backspace)), Action::None);
    assert!(m.query.value.is_empty());
    assert_eq!(m.visible.len(), 2);
    // Esc goes back one level; the list is the root and stays open.
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(matches!(m.view, View::List));
}
#[test]
fn search_never_runs_shortcuts() {
    let mut m = model();
    typing(&mut m, "aefdmqHsnriph/D");
    assert!(matches!(m.view, View::List));
    assert_eq!(m.query.value, "aefdmqHsnriph/D");
}
#[test]
fn tab_hidden_delete_and_preferences() {
    let mut m = model();
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('H')));
    assert_eq!(m.visible.len(), 3);
    assert!(matches!(m.key(key(KeyCode::Char('d'))), Action::Delete(_)));
    assert!(m.delete.is_none());
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    m.preferences.key_bindings.quit_keys.push("esc".into());
    assert_eq!(m.key(key(KeyCode::Esc)), Action::Quit);
    m.preferences.key_bindings.disable_esc_quit = true;
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert_eq!(m.key(ctrl('c')), Action::Quit);
    m.preferences.key_bindings.quit_keys = vec!["x".into()];
    assert_eq!(m.key(key(KeyCode::Char('x'))), Action::Quit);
}
#[test]
fn focus_flags_and_empty_connect() {
    let mut m = model();
    m.preferences.focus_search_on_start = false;
    let m2 = Model::new(vec![], m.preferences.clone(), BTreeMap::new(), true);
    assert!(m2.search_focus);
    typing(&mut m, "missing");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(m.search_focus);
}
#[test]
fn layout_columns_order_and_small_sizes() {
    let mut m = model();
    let s = screen(&mut m, 100, 30);
    let n = s.find("Name").unwrap();
    assert!(n < s.find("Hostname").unwrap());
    assert!(s.find("Hostname").unwrap() < s.find("Tags").unwrap());
    assert!(s.find("Tags").unwrap() < s.find("Last Attempt").unwrap());
    assert!(s.contains("SSHO"));
    assert!(s.contains("#work"));
    for (w, h) in [(60, 15), (24, 8), (1, 1)] {
        screen(&mut m, w, h);
    }
}

#[test]
fn focused_search_has_a_real_cursor_and_dim_placeholder() {
    use ratatui::{backend::Backend, style::Color};
    let mut m = model();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[position].symbol(), "S");
    assert_eq!(buffer[position].fg, Color::DarkGray);
    let row: String = (0..position.x)
        .map(|x| buffer[(x, position.y)].symbol())
        .collect();
    assert!(row.ends_with("Search: > "), "{row:?}");
    typing(&mut m, "abc");
    m.key(key(KeyCode::Left));
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let typed_position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(typed_position.x, position.x + 2);
    assert_eq!(terminal.backend().buffer()[typed_position].symbol(), "c");
    assert_eq!(terminal.backend().buffer()[typed_position].fg, Color::Reset);
    m.key(key(KeyCode::Tab));
    terminal
        .backend_mut()
        .set_cursor_position((99, 29))
        .unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    terminal.backend_mut().assert_cursor_position((99, 29));
}

#[test]
fn form_inputs_show_a_cursor_on_the_placeholder_after_scrolling() {
    use ratatui::{backend::Backend, style::Color};
    let mut m = model();
    m.view = View::Host(Box::new(forms::HostForm::add(
        String::new(),
        "config".into(),
    )));
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    // An empty field says what it means ("requis"); the caret sits on its start.
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(terminal.backend().buffer()[position].symbol(), "r");
    assert_eq!(terminal.backend().buffer()[position].fg, Color::DarkGray);
    // The last advanced field is below the first screen: the form scrolls to it.
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.reveal(forms::Section::Advanced);
    form.focus_on(forms::Stop::Field(forms::FormField::Input(10)));
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let View::Host(form) = &m.view else { panic!() };
    assert!(form.scroll > 0, "the focused field is scrolled into view");
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(terminal.backend().buffer()[position].symbol(), "l");
    assert_eq!(terminal.backend().buffer()[position].fg, Color::DarkGray);
    let row: String = (0..80)
        .map(|x| {
            terminal.backend().buffer()[(x, position.y)]
                .symbol()
                .to_owned()
        })
        .collect();
    assert!(
        row.contains("> RequestTTY") && row.contains("laissé à OpenSSH"),
        "{row}"
    );
    // A focused button owns no caret: the terminal cursor is left where it was.
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.focus_on(forms::Stop::Control(forms::Control::Preview));
    terminal
        .backend_mut()
        .set_cursor_position((79, 19))
        .unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    terminal.backend_mut().assert_cursor_position((79, 19));
}

#[test]
fn input_viewport_respects_wide_characters_and_reserves_cursor_space() {
    let input = forms::Input::new("界界abc");
    assert_eq!(input.viewport(true, 5), ("abc".into(), Some(3)));
    assert_eq!(input.viewport(false, 5), ("界界a".into(), None));
    assert_eq!(input.viewport(true, 0), (String::new(), None));
}
#[test]
fn forms_aliases_navigation_identity_and_validation() {
    let h = Host {
        name: "one".into(),
        aliases: vec!["one".into(), "two".into()],
        hostname: "host".into(),
        extra_identities: vec!["~/.ssh/extra key".into()],
        ..Host::default()
    };
    let mut f = forms::HostForm::edit(h);
    assert_eq!(f.names.len(), 2);
    f.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert_eq!(f.names.len(), 3);
    f.names[2].set("three");
    f.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
    assert_eq!(f.names.len(), 2);
    // The focus stays among the aliases, on the visible Add command.
    use forms::{Control, FormField, Section, Stop};
    assert_eq!(f.current(), Stop::Control(Control::AddAlias));
    f.key(key(KeyCode::Tab));
    assert_eq!(
        f.current(),
        Stop::Control(Control::Section(Section::Advanced))
    );
    assert!(!f.is_open(Section::Advanced), "Tab never unfolds a section");
    f.key(key(KeyCode::Enter));
    f.key(key(KeyCode::Tab));
    assert_eq!(f.current(), Stop::Field(FormField::Input(5)));
    let result = f.host().unwrap();
    assert_eq!(result.extra_identities, ["~/.ssh/extra key"]);
    assert_eq!(result.aliases, ["one", "two"]);
    let mut add = forms::HostForm::add("test".into(), PathBuf::from("config"));
    assert!(add.host().is_err());
    add.inputs[1].set("host.example");
    assert!(add.host().is_ok());
}
#[test]
fn form_render_fields_and_readonly_keys() {
    let mut m = model();
    let h = Host {
        name: "one".into(),
        hostname: "example".into(),
        extra_identities: vec!["~/.ssh/extra key".into()],
        ..Host::default()
    };
    m.view = View::Host(Box::new(forms::HostForm::edit(h)));
    let s = screen(&mut m, 120, 60);
    for label in [
        "Hôtes > Modifier one",
        "Fichier : ",
        "Alias *",
        "Adresse *",
        "Utilisateur",
        "Clé SSH",
        "[  Choisir ]",
        "extra key (lecture seule, conservée)",
        "▸ Alias supplémentaires (0)",
        "▸ Options avancées (0 renseignée(s))",
        "▸ Tags et note",
        "[  Prévisualiser les modifications ]",
        "[  Annuler ]",
    ] {
        assert!(s.contains(label), "missing {label}: {s}");
    }
    // Closed sections keep their fields out of the way.
    assert!(
        !s.contains("ProxyJump") && !s.contains("ProxyCommand"),
        "{s}"
    );
}
#[test]
fn tunnel_dynamic_navigation_and_history() {
    let old = Forward {
        kind: "dynamic".into(),
        local_port: "1080".into(),
        bind_address: "127.0.0.1".into(),
        ..Forward::default()
    };
    let mut f = forms::ForwardForm::new("one".into(), Some(old));
    assert_eq!(f.kind, 2);
    f.key(key(KeyCode::Tab));
    assert_eq!(f.focus, 1);
    f.key(key(KeyCode::Tab));
    assert_eq!(f.focus, 4);
    assert!(f.forward().is_ok());
    f.key(key(KeyCode::Up));
    assert_eq!(f.focus, 1);
    f.inputs[1].set("70000");
    assert!(f.forward().is_err());
}
#[test]
fn text_cursor_unicode_and_word_deletion() {
    let mut t = forms::Input::new("éab cd");
    t.key(key(KeyCode::Home));
    t.key(key(KeyCode::Delete));
    assert_eq!(t.value, "ab cd");
    t.key(key(KeyCode::End));
    t.key(ctrl('w'));
    assert_eq!(t.value, "ab ");
    t.key(ctrl('u'));
    assert!(t.value.is_empty());
}
#[test]
fn emacs_cursor_keys_and_limits() {
    let mut t = forms::Input::new("abcd");
    t.key(ctrl('b'));
    t.key(ctrl('h'));
    assert_eq!(t.value, "abd");
    t.key(ctrl('a'));
    t.key(ctrl('d'));
    assert_eq!(t.value, "bd");
    t.key(ctrl('f'));
    t.key(key(KeyCode::Char('X')));
    assert_eq!(t.value, "bXd");
    let mut m = model();
    typing(&mut m, &"x".repeat(1030));
    assert_eq!(m.query.value.len(), 1024);
}
#[test]
fn sorting_history_and_table_shortcuts() {
    let mut m = model();
    m.history.insert(
        "beta".into(),
        ConnectionInfo {
            host_name: "beta".into(),
            last_connect: Utc::now(),
            connect_count: 2,
            port_forwarding: None,
            ..ConnectionInfo::default()
        },
    );
    m.key(key(KeyCode::Tab));
    assert!(!m.recent);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.key(key(KeyCode::Char('r')));
    assert!(m.recent);
    assert_eq!(m.all[m.visible[0]].name, "beta");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.key(key(KeyCode::Char('s')));
    assert!(!m.recent);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!(
        m.key(key(KeyCode::Char('d'))),
        Action::Delete(m.selected_host().unwrap())
    );
    m.key(key(KeyCode::Char('i')));
    assert!(matches!(m.view, View::Info(_)));
    // Reading a sheet never edits: Enter alone does nothing, `e` asks for it.
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Info(_)));
    m.key(key(KeyCode::Char('e')));
    assert!(matches!(m.view, View::Host(_)));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Info(_)));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}
#[test]
fn form_tab_order_and_submit_shortcuts() {
    use forms::{Control as C, FormField as F, Section as S, Stop};
    let mut add = forms::HostForm::add("one".into(), PathBuf::from("config"));
    assert_eq!(add.current(), Stop::Field(F::Input(0)));
    // Tab follows the visual order through fields, section headers and buttons,
    // wraps around, and never unfolds a section.
    for expected in [
        Stop::Field(F::Input(1)),
        Stop::Field(F::Input(2)),
        Stop::Field(F::Input(3)),
        Stop::Field(F::Input(4)),
        Stop::Control(C::KeyPicker),
        Stop::Control(C::Section(S::Aliases)),
        Stop::Control(C::Section(S::Advanced)),
        Stop::Control(C::Section(S::Organization)),
        Stop::Control(C::Preview),
        Stop::Control(C::Cancel),
        Stop::Control(C::File),
        Stop::Field(F::Input(0)),
    ] {
        assert_eq!(add.key(key(KeyCode::Tab)), forms::FormEvent::None);
        assert_eq!(add.current(), expected);
    }
    assert!(!add.is_open(S::Aliases) && !add.is_open(S::Advanced) && !add.is_open(S::Organization));
    // Enter walks the fields and stops on the main action: reaching the end of
    // the fields never submits; only the button does.
    for expected in 1..=4 {
        assert_eq!(add.key(key(KeyCode::Enter)), forms::FormEvent::None);
        assert_eq!(add.current(), Stop::Field(F::Input(expected)));
    }
    assert_eq!(add.key(key(KeyCode::Enter)), forms::FormEvent::None);
    assert_eq!(add.current(), Stop::Control(C::Preview));
    assert_eq!(add.key(key(KeyCode::Enter)), forms::FormEvent::Submit);
    add.key(key(KeyCode::BackTab));
    assert_eq!(add.current(), Stop::Control(C::Section(S::Organization)));
    // Ctrl+K is text editing again, not a tab switch.
    add.focus_on(Stop::Field(F::Input(1)));
    add.inputs[1].set("abc");
    add.inputs[1].cursor = 1;
    add.key(ctrl('k'));
    assert_eq!(add.inputs[1].value, "a");
    assert_eq!(add.current(), Stop::Field(F::Input(1)));
    assert_eq!(add.key(ctrl('s')), forms::FormEvent::Submit);
    let mut m = model();
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('e')));
    let View::Host(ref mut f) = m.view else {
        panic!()
    };
    f.inputs[1].set("");
    m.key(ctrl('s'));
    let View::Host(ref f) = m.view else { panic!() };
    assert!(!f.errors.is_empty());
    assert_eq!(f.errors[0].field, forms::FormField::Input(1));
}
#[test]
fn selectors_and_tunnels_return_precise_actions() {
    let mut m = model();
    let h = m.all[0].clone();
    m.view = View::Files {
        files: vec!["a".into(), "b".into()],
        selected: 0,
        host: Some(h.clone()),
        seed: String::new(),
        duplicate: None,
        retarget: false,
    };
    m.key(key(KeyCode::Down));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::MoveTo(h, PathBuf::from("b"))
    );
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    m.view = View::Files {
        files: vec!["a".into()],
        selected: 0,
        host: None,
        seed: "new".into(),
        duplicate: None,
        retarget: false,
    };
    m.key(key(KeyCode::Enter));
    let View::Host(ref form) = m.view else {
        panic!()
    };
    assert_eq!(form.target, PathBuf::from("a"));
    assert_eq!(form.inputs[0].value, "new");
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Files { .. }));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('f')));
    let View::Forward(ref mut form) = m.view else {
        panic!()
    };
    form.inputs[1].set("8080");
    form.inputs[3].set("80");
    let action = m.key(ctrl('l'));
    assert!(matches!(action, Action::Connect(_, Some(_))));
}
#[test]
fn secondary_views_render_and_scroll_without_panics() {
    let mut m = model();
    let mut h = m.all[0].clone();
    h.extra_identities = vec!["key1".into(), "key2".into()];
    m.view = View::Info(h.clone());
    let info = screen(&mut m, 100, 30);
    assert!(info.contains("key2"));
    assert!(info.contains("[  Connexion ]") && info.contains("[  Modifier ]"));
    m.view = View::Help;
    let help = screen(&mut m, 100, 30);
    for section in ["Connexion", "Organisation", "Outils", "Application"] {
        assert!(help.contains(section));
    }
    for view in [
        View::Help,
        View::Info(h.clone()),
        View::Host(Box::new(forms::HostForm::edit(h))),
        View::Forward(forms::ForwardForm::new("one".into(), None)),
        View::Files {
            files: vec!["a".into()],
            selected: 0,
            host: None,
            seed: String::new(),
            duplicate: None,
            retarget: false,
        },
    ] {
        m.view = view;
        for (w, h) in [(100, 24), (40, 12), (10, 5), (1, 1)] {
            screen(&mut m, w, h);
        }
    }
}
#[test]
fn list_scroll_tracks_selection_and_resize() {
    let mut m = model();
    m.all = (0..100)
        .map(|i| Host {
            name: format!("host{i:03}"),
            hostname: "example".into(),
            ..Host::default()
        })
        .collect();
    m.filter(true);
    m.search_focus = false;
    m.key(key(KeyCode::End));
    screen(&mut m, 80, 20);
    assert!(m.offset > 0);
    let s = screen(&mut m, 80, 12);
    assert!(s.contains("host099"));
    m.key(key(KeyCode::Home));
    let s = screen(&mut m, 80, 12);
    assert!(s.contains("host000"));
    assert_eq!(m.offset, 0);
}
#[test]
fn edit_form_save_keeps_alias_block_and_extra_identity_files() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    let app = temp.path().join("app");
    std::fs::write(&path,"Host one two\n    HostName example.org\n    IdentityFile /missing-preserved-extra\n    IdentityFile /another-missing-extra\n    # custom note\n    Compression yes\n").unwrap();
    let original = config::find_host(&path, "two").unwrap();
    let mut form = forms::HostForm::edit(original.clone());
    // Clearing the editable primary key is valid; preserved extra keys need not exist on this computer.
    form.inputs[4].set("");
    form.names[0].set("renamed");
    let host = form.host().unwrap();
    config::update_host(&original, &host, &app).unwrap();
    let updated = config::find_host(&path, "renamed").unwrap();
    assert_eq!(updated.aliases, ["renamed", "two"]);
    assert_eq!(updated.identity, "/another-missing-extra");
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("# custom note"));
}
#[test]
fn initial_focus_obeys_preference_without_forced_flag() {
    let config = AppConfig {
        focus_search_on_start: false,
        ..AppConfig::default()
    };
    let mut m = Model::new(vec![], config, BTreeMap::new(), false);
    assert!(!m.search_focus);
    assert_eq!(m.key(key(KeyCode::Char('a'))), Action::Add(String::new()));
    m.key(key(KeyCode::Char('/')));
    assert!(m.search_focus);
}
#[test]
fn short_host_list_uses_available_height_before_status_and_footer() {
    let mut m = model();
    let output = screen(&mut m, 120, 40);
    let rows: Vec<_> = output.lines().collect();
    let last_host = rows.iter().position(|line| line.contains("beta")).unwrap();
    let table_bottom = rows.iter().rposition(|line| line.contains('╰')).unwrap();
    let footer = rows
        .iter()
        .position(|line| line.contains("↑/↓: select"))
        .unwrap();
    assert!(
        table_bottom > last_host + 1,
        "table fills the available height"
    );
    assert!(rows[table_bottom + 1].contains("Hosts: 2/3"));
    assert_eq!(footer, table_bottom + 2, "status and help follow the table");
    assert_eq!(footer, rows.len() - 2, "only the outer margin follows help");
}

#[test]
fn saving_secondary_alias_unchanged_keeps_both_names() {
    save_secondary_alias(false, false);
}
#[test]
fn saving_secondary_alias_property_change_keeps_both_names() {
    save_secondary_alias(true, false);
}
#[test]
fn removing_selected_secondary_alias_keeps_explicit_names() {
    save_secondary_alias(false, true);
}
fn save_secondary_alias(change_user: bool, remove_selected: bool) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    std::fs::write(
        &path,
        "Host one two\n HostName example.org\n User old\n Port 22\n",
    )
    .unwrap();
    let original = config::find_host(&path, "two").unwrap();
    let mut form = forms::HostForm::edit(original.clone());
    if change_user {
        form.inputs[2].set("new");
    }
    if remove_selected {
        form.names.pop();
    }
    config::update_host(&original, &form.host().unwrap(), &temp.path().join("app")).unwrap();
    let updated = config::find_host(&path, "one").unwrap();
    assert_eq!(
        updated.aliases,
        if remove_selected {
            vec!["one"]
        } else {
            vec!["one", "two"]
        }
    );
    assert_eq!(updated.user, if change_user { "new" } else { "old" });
    assert_eq!(config::find_host(&path, "two").is_ok(), !remove_selected);
}

#[test]
fn renaming_selected_secondary_alias_keeps_other_names() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    std::fs::write(&path, "Host one two\n HostName example.org\n").unwrap();
    let original = config::find_host(&path, "two").unwrap();
    let mut form = forms::HostForm::edit(original.clone());
    form.names[1].set("renamed");
    config::update_host(&original, &form.host().unwrap(), &temp.path().join("app")).unwrap();
    assert_eq!(
        config::find_host(&path, "renamed").unwrap().aliases,
        ["one", "renamed"]
    );
    assert!(config::find_host(&path, "two").is_err());
}

#[test]
fn palette_from_search_edits_the_filtered_host() {
    let mut m = model();
    typing(&mut m, "beta");
    assert_eq!(m.key(key(KeyCode::F(2))), Action::None);
    assert!(screen(&mut m, 100, 30).contains("Modifier"));
    typing(&mut m, "modifier");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    let View::Host(form) = &m.view else {
        panic!("palette must open the existing edit form");
    };
    assert_eq!(form.original.as_ref().unwrap().name, "beta");
    assert_eq!(m.query.value, "beta");
    assert!(m.search_focus);
}

#[test]
fn palette_cancel_preserves_query_selection_and_focus() {
    for search_focus in [true, false] {
        let mut m = model();
        typing(&mut m, "a");
        m.key(key(KeyCode::Down));
        m.key(key(KeyCode::Left));
        m.search_focus = search_focus;
        m.key(key(KeyCode::F(2)));
        assert!(screen(&mut m, 100, 30).contains("Modifier"));
        // Modal keys must not edit the search or trigger list shortcuts.
        typing(&mut m, "q");
        m.key(key(KeyCode::Down));
        assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
        assert!(matches!(m.view, View::List));
        assert_eq!(m.query.value, "a");
        assert_eq!(m.search_focus, search_focus);
        assert_eq!(m.selected_host().unwrap().name, "beta");
        if search_focus {
            typing(&mut m, "b");
            assert_eq!(m.query.value, "ba", "search cursor must be retained");
        }
    }
}

#[test]
fn palette_empty_list_disables_host_actions_but_allows_add_and_help() {
    for has_filtered_hosts in [false, true] {
        let mut m = if has_filtered_hosts {
            let mut m = model();
            typing(&mut m, "missing");
            m
        } else {
            Model::new(vec![], AppConfig::default(), BTreeMap::new(), false)
        };
        m.key(key(KeyCode::F(2)));
        let output = screen(&mut m, 100, 30);
        assert!(output.contains("Ajouter un hôte"), "{output}");
        // Host actions stay listed, explain why they cannot run, and run nothing.
        for words in ["modifier", "dupliquer", "creer tunnel"] {
            typing(&mut m, words);
            assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
            assert!(matches!(m.view, View::Actions(_)));
            let output = screen(&mut m, 100, 30);
            assert!(
                output.contains("Indisponible : Sélectionnez un hôte. Rien n'a été lancé."),
                "{words}: {output}"
            );
            m.key(ctrl('u'));
        }
        typing(&mut m, "aide");
        for (width, height) in [(40, 12), (10, 5), (1, 1)] {
            screen(&mut m, width, height);
        }
        m.key(key(KeyCode::Enter));
        assert!(matches!(m.view, View::Help));
        m.key(key(KeyCode::Esc));
        assert_eq!(palette(&mut m, "ajouter"), Action::Add(String::new()));
    }
}

#[test]
fn palette_tunnel_and_test_reuse_existing_actions() {
    let mut m = model();
    typing(&mut m, "beta");
    m.history.insert(
        "beta".into(),
        ConnectionInfo {
            host_name: "beta".into(),
            last_connect: Utc::now(),
            connect_count: 1,
            port_forwarding: Some(Forward {
                kind: "dynamic".into(),
                local_port: "1080".into(),
                ..Forward::default()
            }),
            ..ConnectionInfo::default()
        },
    );
    m.key(key(KeyCode::F(2)));
    assert!(screen(&mut m, 100, 30).contains("Créer un tunnel"));
    typing(&mut m, "creer tunnel");
    m.key(key(KeyCode::Enter));
    let View::Forward(form) = &m.view else {
        panic!("palette must open the existing tunnel form");
    };
    assert_eq!(form.host, "beta");
    assert_eq!(form.kind, 2);
    assert_eq!(form.inputs[1].value, "1080");
    m.key(key(KeyCode::Esc));
    assert_eq!(palette(&mut m, "tester"), Action::Ping);
    assert!(matches!(m.view, View::List));
    assert_eq!(m.query.value, "beta");
}

#[test]
fn contextual_bar_exposes_palette_and_existing_list_actions() {
    let mut m = model();
    assert!(screen(&mut m, 120, 30).contains("F2: actions"));
    m.key(key(KeyCode::Tab));
    let output = screen(&mut m, 120, 30);
    for shortcut in [
        "F2: actions",
        "a: add",
        "e: edit",
        "f: tunnel",
        "p: test",
        "?: help",
    ] {
        assert!(output.contains(shortcut), "missing {shortcut}");
    }
}

fn adaptive_model() -> Model {
    let mut hosts: Vec<_> = (0..40)
        .map(|i| Host {
            name: format!("fleet{i:02}-{}", "界e\u{301}👩‍💻".repeat(12)),
            hostname: format!("host{i:02}.very-long-address.example.org"),
            tags: vec!["production-long-tag".into()],
            ..Host::default()
        })
        .collect();
    hosts.push(Host {
        name: "secret".into(),
        tags: vec!["hidden".into()],
        ..Host::default()
    });
    Model::new(hosts, AppConfig::default(), BTreeMap::new(), false)
}

#[test]
fn adaptive_list_keeps_selection_counts_and_scroll_range_visible() {
    use ratatui::style::Color;
    for (width, height, first_range, last_range, minimum_rows) in [
        (28, 8, "1-3", "38-40", 3),
        // Without an inactive search box the table gains two rows.
        (70, 18, "1-8", "33-40", 8),
        (80, 18, "1-8", "33-40", 8),
        (120, 35, "1-25", "16-40", 25),
    ] {
        let mut m = adaptive_model();
        m.key(key(KeyCode::Tab));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let output = screen(&mut m, width, height);
        for text in ["40/41", "1/40", first_range, "fleet00-", "F2: actions"] {
            assert!(
                output.contains(text),
                "{width}x{height}: missing {text}\n{output}"
            );
        }
        let visible_rows = output.lines().filter(|row| row.contains("fleet")).count();
        assert!(visible_rows >= minimum_rows, "unused height: {output}");
        let buffer = terminal.backend().buffer();
        let selected = buffer
            .content()
            .chunks(width as usize)
            .find(|row| {
                row.iter()
                    .any(|cell| cell.symbol() == "f" && cell.bg == Color::Rgb(0, 173, 216))
            })
            .expect("selected host must be drawn with a highlight");
        for cell in selected
            .iter()
            .filter(|cell| cell.bg == Color::Rgb(0, 173, 216) && !cell.symbol().trim().is_empty())
        {
            assert_eq!(cell.fg, Color::Black, "selected host must remain readable");
        }
        m.key(key(KeyCode::End));
        let output = screen(&mut m, width, height);
        for text in ["40/40", last_range, "fleet39-"] {
            assert!(
                output.contains(text),
                "{width}x{height}: missing {text}\n{output}"
            );
        }
        assert_eq!(
            m.key(key(KeyCode::Enter)),
            Action::Connect(m.all[39].name.clone(), None)
        );
        m.key(key(KeyCode::PageUp));
        screen(&mut m, width, height);
        assert!(m.selected < 39);
        m.key(key(KeyCode::Tab));
        typing(&mut m, "fleet0");
        let output = screen(&mut m, width, height);
        assert!(output.contains("10/41"), "{output}");
        assert!(output.contains("1/10"), "{output}");
        typing(&mut m, "missing");
        let output = screen(&mut m, width, height);
        for text in ["0/41", "0/0", "0-0"] {
            assert!(
                output.contains(text),
                "missing empty count {text}: {output}"
            );
        }
    }
}

#[test]
fn adaptive_list_truncates_unicode_in_rendered_columns() {
    use unicode_width::UnicodeWidthStr;
    for (width, height, columns) in [(28, 8, 1), (50, 18, 2), (70, 18, 3), (120, 35, 4)] {
        let mut m = adaptive_model();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let buffer = terminal.backend().buffer();
        let row = buffer
            .content()
            .chunks(width as usize)
            .find(|row| {
                row.windows(7).any(|cells| {
                    cells.iter().map(|cell| cell.symbol()).collect::<String>() == "fleet00"
                })
            })
            .expect("a usable host name must be visible");
        assert!(
            row.iter().any(|cell| cell.symbol() == "…"),
            "long cells need an ellipsis"
        );
        // Reconstruct visible graphemes while skipping the continuation cells of wide glyphs.
        let mut visible = String::new();
        let mut x = 0;
        while x < row.len() {
            let symbol = row[x].symbol();
            visible.push_str(symbol);
            x += UnicodeWidthStr::width(symbol).max(1);
        }
        assert_eq!(UnicodeWidthStr::width(visible.as_str()), width as usize);
        assert!(
            visible.contains("界e\u{301}👩‍💻"),
            "graphemes must survive: {visible}"
        );
        let output = screen(&mut m, width, height);
        assert_eq!(output.contains("Hostname"), columns >= 2, "{output}");
        assert_eq!(output.contains("Tags"), columns >= 3, "{output}");
        assert_eq!(output.contains("Last Attempt"), columns >= 4, "{output}");
    }
}

#[test]
fn adaptive_footer_at_eighty_columns_keeps_actions_and_help_visible() {
    let mut m = model();
    m.key(key(KeyCode::Tab));
    let output = screen(&mut m, 80, 18);
    for text in [
        "F2: actions",
        "Enter: connect",
        "f: tunnel",
        "p: test",
        "?: help",
    ] {
        assert!(
            output.contains(text),
            "missing footer control {text}: {output}"
        );
    }
}

#[test]
fn adaptive_compact_status_keeps_four_digit_counts_and_complete_range() {
    let mut m = adaptive_model();
    m.all = (0..1000)
        .map(|i| Host {
            name: format!("host{i:04}"),
            ..Host::default()
        })
        .chain(std::iter::once(Host {
            name: "hidden".into(),
            tags: vec!["hidden".into()],
            ..Host::default()
        }))
        .collect();
    m.filter(true);
    m.search_focus = false;
    m.key(key(KeyCode::End));
    let output = screen(&mut m, 28, 8);
    for text in ["1000/1001", "1000/1000", "998-1000", "host0999"] {
        assert!(output.contains(text), "missing {text}: {output}");
    }
}

#[test]
fn adaptive_compact_notices_reserve_real_selected_host_rows() {
    use ratatui::style::Color;
    let mut m = adaptive_model();
    m.show_hidden = true;
    m.filter(true);
    m.search_focus = false;
    m.error = "boom".into();
    m.update = Some(UpdateInfo {
        current: "1.0".into(),
        latest: "2.0".into(),
        available: true,
        ..UpdateInfo::default()
    });
    m.key(key(KeyCode::End));
    let mut terminal = Terminal::new(TestBackend::new(28, 8)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let output = screen(&mut m, 28, 8);
    for text in [
        "secret",
        "Error",
        "Update",
        "Filtres : Masqués",
        "41/41",
        "40-41",
        "F2: actions",
    ] {
        assert!(output.contains(text), "missing {text}: {output}");
    }
    let selected = terminal
        .backend()
        .buffer()
        .content()
        .chunks(28)
        .find(|row| {
            row.iter()
                .any(|cell| cell.symbol() == "s" && cell.bg == Color::Rgb(0, 173, 216))
        })
        .expect("selected host must remain visible with every notice");
    assert!(selected
        .iter()
        .any(|cell| cell.symbol() == "s" && cell.fg == Color::Black));
    assert_eq!(m.page, 2, "page size must match real host row capacity");
    let output = screen(&mut m, 28, 6);
    assert!(
        !output.contains("secret"),
        "this smaller screen has no host row"
    );
    assert!(
        output.contains("0-0"),
        "no rendered rows must produce an empty range: {output}"
    );
}

#[test]
fn palette_selected_actions_and_selector_have_explicit_high_contrast_colors() {
    use ratatui::style::{Color, Modifier};

    for (has_host, words, label) in [
        (true, "ajouter", "Ajouter un hôte"),
        (true, "creer tunnel", "Créer un tunnel"),
        (false, "modifier", "Modifier"),
    ] {
        let mut m = if has_host {
            model()
        } else {
            Model::new(vec![], AppConfig::default(), BTreeMap::new(), false)
        };
        m.key(key(KeyCode::F(2)));
        typing(&mut m, words);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let row = terminal
            .backend()
            .buffer()
            .content()
            .chunks(100)
            .find(|row| row.iter().any(|cell| cell.symbol() == "▶"))
            .expect("the selected palette action must have a selector");
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains(label), "wrong selected action: {text}");
        let mut selected_text = vec![label, "▶"];
        if !has_host {
            assert!(text.contains("Sélectionnez un hôte"));
            selected_text.push("Sélectionnez un hôte");
        }
        for text in selected_text {
            // The list stays visible beside the palette: locate the whole text.
            let needle: Vec<String> = text.chars().map(|c| c.to_string()).collect();
            let start = (0..=row.len() - needle.len())
                .find(|&start| {
                    needle
                        .iter()
                        .enumerate()
                        .all(|(i, symbol)| row[start + i].symbol() == symbol)
                })
                .unwrap();
            for cell in &row[start..start + text.chars().count()] {
                assert_eq!(cell.fg, Color::Black, "foreground of {text}: {cell:?}");
                assert_eq!(cell.bg, Color::Rgb(0, 173, 216));
                assert!(cell.modifier.contains(Modifier::BOLD));
            }
        }
    }
}

#[test]
fn details_f4_from_search_tracks_selection_and_keeps_list() {
    let mut m = model();
    m.all[0].port = "2201".into();
    m.all[0].identity = "/keys/primary".into();
    m.all[0].extra_identities = vec!["/keys/second".into()];
    m.all[0].proxy_jump = "bastion".into();
    m.all[0].proxy_command = "nc %h %p".into();
    m.all[0].source_file = "/ssh/config".into();
    m.all[0].line_number = 42;
    m.statuses.insert(
        "alpha".into(),
        PingResult {
            method: crate::storage::AvailabilityMethod::Ssh,
            name: "alpha".into(),
            status: PingStatus::Offline,
            duration: Duration::ZERO,
            error: Some("connection refused".into()),
        },
    );
    assert!(!screen(&mut m, 150, 40).contains("Details"));
    m.key(key(KeyCode::F(4)));
    let output = screen(&mut m, 150, 40);
    for text in [
        "Details",
        "User: deploy",
        "Port: 2201",
        "/keys/primary",
        "/keys/second",
        "ProxyJump: bastion",
        "ProxyCommand: nc %h %p",
        "/ssh/config:42",
        "connection refused",
        "Hosts: 2/3",
        "beta",
    ] {
        assert!(output.contains(text), "missing {text}: {output}");
    }
    assert!(m.search_focus);
    m.key(key(KeyCode::Down));
    let output = screen(&mut m, 150, 40);
    assert!(output.contains("Host: beta"));
    assert!(!output.contains("User: deploy"));
    m.key(key(KeyCode::F(4)));
    assert!(!screen(&mut m, 150, 40).contains("Host: beta"));
}

#[test]
fn details_palette_appends_toggle_and_allows_empty_list() {
    for empty in [false, true] {
        let mut m = if empty {
            Model::new(vec![], AppConfig::default(), BTreeMap::new(), false)
        } else {
            model()
        };
        m.key(key(KeyCode::F(2)));
        typing(&mut m, "panneau");
        let output = screen(&mut m, 150, 40);
        assert!(output.contains("Panneau de détails"));
        m.key(key(KeyCode::Enter));
        assert!(matches!(m.view, View::List));
        let output = screen(&mut m, 150, 40);
        assert!(output.contains("Details"));
        assert!(output.contains(if empty {
            "No selected host"
        } else {
            "Host: alpha"
        }));
    }
}

#[test]
fn details_narrow_view_hides_temporarily_and_offers_info_hint() {
    let mut m = model();
    m.key(key(KeyCode::F(4)));
    assert!(screen(&mut m, 110, 30).contains("Host: alpha"));
    let output = screen(&mut m, 109, 30);
    assert!(!output.contains("Host: alpha"));
    assert!(output.contains("i: info"));
    assert!(output.contains("Tab: table"));
    assert!(
        output.contains("Last Attempt"),
        "narrow mode must restore full list width"
    );
    for (w, h) in [(80, 18), (28, 8), (1, 1)] {
        screen(&mut m, w, h);
    }
    assert!(screen(&mut m, 110, 30).contains("Host: alpha"));
    m.key(key(KeyCode::F(4)));
    assert!(!screen(&mut m, 110, 30).contains("Host: alpha"));
}

#[test]
fn details_long_values_scroll_to_all_tails_without_changing_search_or_selection() {
    let mut m = model();
    m.all[0].user = format!("{}USERTAIL", "u".repeat(1500));
    m.all[0].identity = format!("{}KEYTAIL", "界e\u{301}".repeat(600));
    m.all[0].extra_identities = vec![format!("{}EXTRATAIL", "x".repeat(1500))];
    m.all[0].proxy_jump = format!("{}JUMPTAIL", "j".repeat(1500));
    m.all[0].proxy_command = format!("{}COMMANDTAIL", "c".repeat(1500));
    m.all[0].options = format!("{}OPTIONTAIL", "o".repeat(1500));
    m.key(key(KeyCode::F(4)));
    let mut gathered = String::new();
    for _ in 0..120 {
        let output = screen(&mut m, 120, 20);
        assert!(output.contains("alpha"));
        assert!(output.contains("2/3"));
        gathered.extend(
            output
                .lines()
                .filter_map(|line| line.rsplit('│').nth(1))
                .flat_map(|line| line.chars().filter(|c| !c.is_whitespace())),
        );
        m.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    }
    for tail in [
        "USERTAIL",
        "KEYTAIL",
        "EXTRATAIL",
        "JUMPTAIL",
        "COMMANDTAIL",
        "OPTIONTAIL",
    ] {
        assert!(gathered.contains(tail), "inaccessible {tail}");
    }
    assert_eq!(m.selected, 0);
    assert!(m.query.value.is_empty());
    assert!(m.search_focus);
    let bottom = screen(&mut m, 120, 20);
    assert!(bottom.contains("Last attempt: aucune connexion enregistrée"));
    for _ in 0..120 {
        m.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
    }
    assert!(screen(&mut m, 120, 20).contains("Host: alpha"));
    for _ in 0..120 {
        m.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    }
    assert!(screen(&mut m, 120, 20).contains("Last attempt: aucune connexion enregistrée"));
    m.key(key(KeyCode::Down));
    assert!(
        screen(&mut m, 120, 20).contains("Host: beta"),
        "selection changes reset detail scroll"
    );
}

#[test]
fn details_split_preserves_list_row_capacity_and_high_contrast_selection() {
    use ratatui::style::Color;
    let mut m = adaptive_model();
    m.search_focus = false;
    screen(&mut m, 120, 35);
    let capacity = m.page;
    m.key(key(KeyCode::F(4)));
    m.key(key(KeyCode::End));
    let mut terminal = Terminal::new(TestBackend::new(120, 35)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    assert_eq!(m.page, capacity);
    let output = screen(&mut m, 120, 35);
    for text in ["fleet39-", "40/41", "40/40", "16-40"] {
        assert!(output.contains(text), "missing {text}: {output}");
    }
    assert!(
        !output.contains("Last Attempt"),
        "columns must use the list pane width"
    );
    assert!(terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|cell| cell.symbol() == "f"
            && cell.bg == Color::Rgb(0, 173, 216)
            && cell.fg == Color::Black));
}

#[test]
fn details_info_fallback_reaches_wrapped_command_tail() {
    let mut m = model();
    m.all[0].proxy_command = format!("{}INFOTAIL", "c".repeat(2500));
    m.search_focus = false;
    m.key(key(KeyCode::Char('i')));
    let mut gathered = String::new();
    for _ in 0..100 {
        gathered.push_str(&screen(&mut m, 80, 18));
        m.key(key(KeyCode::PageDown));
    }
    assert!(
        gathered.contains("INFOTAIL"),
        "the narrow-terminal info fallback must reach wrapped content"
    );
}

#[test]
fn details_keep_search_home_end_cursor_editing() {
    let mut m = model();
    typing(&mut m, "alp");
    m.key(key(KeyCode::F(4)));
    screen(&mut m, 120, 30);
    m.key(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL));
    typing(&mut m, "h");
    assert_eq!(m.query.value, "halp");
    m.query.set("alp");
    m.filter(true);
    m.key(key(KeyCode::Home));
    m.key(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL));
    typing(&mut m, "h");
    assert_eq!(m.query.value, "alph");
}

#[test]
fn details_alt_arrows_scroll_without_navigating_or_consuming_search_text() {
    for search_focus in [true, false] {
        let mut m = model();
        m.search_focus = search_focus;
        m.key(key(KeyCode::F(4)));
        let output = screen(&mut m, 120, 20);
        assert!(output.contains("── Résumé") && output.contains("Host: alpha"));
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        let output = screen(&mut m, 120, 20);
        assert!(!output.contains("── Résumé"));
        assert!(output.contains("Host: alpha"));
        assert_eq!(m.selected, 0, "Alt+Down scrolls details, not the list");
        m.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert!(screen(&mut m, 120, 20).contains("── Résumé"));
        assert_eq!(m.search_focus, search_focus);
        if search_focus {
            typing(&mut m, "[]jk");
            assert_eq!(m.query.value, "[]jk");
            assert!(matches!(m.view, View::List));
        }
    }
}

#[test]
fn navigation_sort_and_visibility_preserve_selected_secondary_alias() {
    let mut m = model();
    m.search_focus = false;
    m.all[1].aliases = vec!["primary".into(), "beta".into()];
    m.all[1].source_file = "included".into();
    m.navigate(1);
    let selected = m.selected_host().unwrap();
    m.history.insert(
        "beta".into(),
        ConnectionInfo {
            host_name: "beta".into(),
            last_connect: Utc::now(),
            connect_count: 1,
            port_forwarding: None,
            ..ConnectionInfo::default()
        },
    );
    for code in ['r', 'n', 's', 'H', 'H'] {
        m.key(key(KeyCode::Char(code)));
        assert_eq!(m.selected_host(), Some(selected.clone()), "after {code}");
    }
}

#[test]
fn navigation_info_edit_cancel_returns_to_info_with_scroll() {
    let mut m = model();
    m.search_focus = false;
    m.key(key(KeyCode::Char('i')));
    m.scroll = 2;
    m.key(key(KeyCode::Char('e')));
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Info(h) if h.name == "alpha"));
    assert_eq!(m.scroll, 2);
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}

#[test]
fn navigation_files_add_cancel_returns_to_selector() {
    let mut m = model();
    m.view = View::Files {
        files: vec!["first".into(), "second".into()],
        selected: 1,
        host: None,
        seed: "new".into(),
        duplicate: None,
        retarget: false,
    };
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Host(_)));
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Files { selected: 1, seed, .. } if seed == "new"));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}

#[test]
fn navigation_escape_leaves_search_focus_and_backtab_matches_tab() {
    let mut m = model();
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.search_focus);
    m.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(m.search_focus);
    typing(&mut m, "beta");
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert_eq!(m.query.value, "beta");
    assert!(!m.search_focus);
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    let text = screen(&mut m, 100, 20);
    assert!(
        text.contains("Esc ne quitte pas la liste — q / Ctrl+C : quitter"),
        "{text}"
    );
    m.key(key(KeyCode::Down));
    assert!(!screen(&mut m, 100, 20).contains("Esc ne quitte pas"));
}

#[test]
fn navigation_reload_restores_name_and_source_before_replacing_indices() {
    let mut m = model();
    m.search_focus = false;
    m.all[1].aliases = vec!["primary".into(), "beta".into()];
    m.all[1].source_file = "included".into();
    m.navigate(1);
    let selected = m.selected_host().unwrap();
    let same_name_elsewhere = Host {
        source_file: "other".into(),
        ..selected.clone()
    };
    m.offset = 90;
    m.replace_hosts(
        vec![selected.clone(), same_name_elsewhere, m.all[0].clone()],
        None,
    );
    assert_eq!(m.selected_host(), Some(selected.clone()));
    assert!(m.offset <= m.selected);
    m.replace_hosts(vec![m.all[2].clone()], None);
    assert_eq!(m.selected, 0);
    assert_eq!(m.offset, 0);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.replace_hosts(vec![], None);
    assert!(m.selected_host().is_none());
    assert_eq!((m.selected, m.offset), (0, 0));
}

#[test]
fn navigation_saved_rename_refreshes_info_and_reveals_selection() {
    let mut m = model();
    typing(&mut m, "beta");
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('i')));
    m.key(key(KeyCode::Char('e')));
    let old = m.selected_host().unwrap();
    let updated = Host {
        name: "renamed".into(),
        source_file: "included".into(),
        tags: vec!["hidden".into()],
        ..old.clone()
    };
    let action = Action::Save(
        Some(old),
        Box::new(updated.clone()),
        updated.source_file.clone(),
    );
    m.complete_mutation(vec![m.all[0].clone(), updated.clone()], &action);
    assert_eq!(m.selected_host(), Some(updated.clone()));
    assert!(m.query.value.is_empty());
    assert!(m.show_hidden);
    assert!(matches!(&m.view, View::Info(h) if h == &updated));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}

#[test]
fn navigation_added_host_selected_after_file_picker_and_incompatible_search() {
    let mut m = model();
    typing(&mut m, "alpha");
    m.open_view(View::Files {
        files: vec!["target".into()],
        selected: 0,
        host: None,
        seed: String::new(),
        duplicate: None,
        retarget: false,
    });
    m.key(key(KeyCode::Enter));
    let added = Host {
        name: "new".into(),
        source_file: "target".into(),
        ..Host::default()
    };
    let action = Action::Save(None, Box::new(added.clone()), "target".into());
    let mut hosts = m.all.clone();
    hosts.push(added.clone());
    m.complete_mutation(hosts, &action);
    assert_eq!(m.selected_host(), Some(added));
    assert!(m.query.value.is_empty());
    assert!(matches!(m.view, View::List));
    assert!(m.parents.is_empty());
}

#[test]
fn navigation_secondary_alias_unchanged_save_keeps_active_alias() {
    navigation_save_secondary_alias(false, false);
}

#[test]
fn navigation_secondary_alias_property_save_keeps_active_alias() {
    navigation_save_secondary_alias(true, false);
}

#[test]
fn navigation_secondary_alias_rename_selects_returned_primary_alias() {
    navigation_save_secondary_alias(false, true);
}

fn navigation_save_secondary_alias(change_user: bool, rename_selected: bool) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    std::fs::write(
        &path,
        "Host one two\n HostName example.org\n User old\n Port 22\n",
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&path).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    typing(&mut m, "two");
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('i')));
    m.key(key(KeyCode::Char('e')));
    let View::Host(form) = &mut m.view else {
        panic!("expected host form")
    };
    assert_eq!(form.original.as_ref().unwrap().name, "two");
    if change_user {
        form.inputs[2].set("new");
    }
    if rename_selected {
        form.names[1].set("renamed");
    }
    let action = m.key(ctrl('s'));
    let Action::Save(Some(original), host, _) = &action else {
        panic!("expected save")
    };
    config::update_host(original, host, &temp.path().join("app")).unwrap();
    m.complete_mutation(config::load_hosts(&path).unwrap(), &action);
    let selected = m.selected_host().unwrap();
    assert_eq!(selected.name, if rename_selected { "one" } else { "two" });
    assert_eq!(selected.source_file, path);
    assert_eq!(selected.user, if change_user { "new" } else { "old" });
    assert_eq!(m.query.value, if rename_selected { "" } else { "two" });
    assert!(matches!(&m.view, View::Info(h) if h == &selected));
}

#[test]
fn navigation_move_tracks_target_file_then_delete_clamps_and_clears_dialog() {
    let mut m = model();
    m.navigate(1);
    let original = m.selected_host().unwrap();
    let moved = Host {
        source_file: "target".into(),
        ..original.clone()
    };
    let action = Action::MoveTo(original, "target".into());
    m.complete_mutation(vec![moved.clone(), m.all[0].clone()], &action);
    assert_eq!(m.selected_host(), Some(moved.clone()));
    m.delete = Some(moved.clone());
    m.complete_mutation(vec![m.all[1].clone()], &Action::Delete(moved));
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!((m.selected, m.offset), (0, 0));
    assert!(m.delete.is_none());
}

#[test]
fn dirty_host_cancel_resume_and_discard() {
    let mut m = model();
    m.activate_action(PaletteAction::Edit);
    typing(&mut m, "-changed");
    m.key(key(KeyCode::Esc));
    let text = screen(&mut m, 100, 24);
    assert!(
        text.contains("Abandonner les modifications de ce brouillon ?"),
        "{text}"
    );
    assert!(
        text.contains("[▶ Continuer l'édition ]") && text.contains("[  Abandonner ]"),
        "{text}"
    );
    // Enter on the focused choice, like Esc, keeps the draft.
    m.key(key(KeyCode::Enter));
    let View::Host(f) = &m.view else {
        panic!("resume form")
    };
    assert_eq!(f.names[0].value, "alpha-changed");
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Host(f) if f.names[0].value == "alpha-changed"));
    discard(&mut m);
    assert!(matches!(m.view, View::List));
}

#[test]
fn invalid_host_focuses_first_invalid_field() {
    let mut m = model();
    let mut f = HostForm::add("valid".into(), "config".into());
    f.inputs[1].set("bad address");
    f.inputs[3].set("0");
    f.focus = 8;
    m.open_view(View::Host(Box::new(f)));
    m.key(ctrl('s'));
    let View::Host(f) = &m.view else {
        panic!("host form")
    };
    assert_eq!(f.focus, 1);
    let output = screen(&mut m, 100, 30);
    assert!(output.contains("invalid hostname"));
}

#[test]
fn ctrl_a_edits_alias_and_alt_a_adds_alias() {
    let mut f = HostForm::edit(model().all[0].clone());
    f.key(ctrl('a'));
    assert_eq!(f.names.len(), 1);
    assert_eq!(f.names[0].cursor, 0);
    f.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert_eq!(f.names.len(), 2);
}

#[test]
fn baselines_ignore_navigation_and_recognize_reverted_values() {
    let mut host = HostForm::edit(model().all[0].clone());
    assert!(!host.dirty());
    host.key(ctrl('a'));
    host.key(key(KeyCode::Tab));
    host.key(ctrl('j'));
    host.scroll = 100;
    assert!(!host.dirty());
    host.inputs[1].set("changed");
    assert!(host.dirty());
    host.inputs[1].set("a.example");
    assert!(!host.dirty());
    host.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert!(host.dirty());
    host.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
    assert!(!host.dirty());
    let mut forward = ForwardForm::new("alpha".into(), None);
    forward.focus = 4;
    forward.inputs[4].cursor = 0;
    forward.scroll = 50;
    assert!(!forward.dirty());
    forward.kind = 2;
    assert!(forward.dirty());
    forward.kind = 0;
    forward.inputs[2].set("changed");
    assert!(forward.dirty());
    forward.inputs[2].set("localhost");
    assert!(!forward.dirty());
    let add = HostForm::add("seeded".into(), "config".into());
    assert!(!add.dirty(), "seed and placeholders are the baseline");
}

#[test]
fn clean_host_and_forward_cancel_without_confirmation() {
    for forward in [false, true] {
        let mut m = model();
        m.open_view(View::Help);
        m.scroll = 7;
        m.open_view(if forward {
            View::Forward(ForwardForm::new("alpha".into(), None))
        } else {
            View::Host(Box::new(HostForm::edit(m.all[0].clone())))
        });
        m.key(key(KeyCode::Tab));
        m.key(key(KeyCode::Esc));
        assert!(!m.discard);
        assert!(matches!(m.view, View::Help));
        assert_eq!(m.scroll, 7);
        assert_eq!(m.parents.len(), 1);
    }
}

#[test]
fn forward_dirty_cancel_resume_preserves_every_form_value_and_parent() {
    let mut m = model();
    m.open_view(View::Info(m.all[0].clone()));
    m.scroll = 3;
    let mut form = ForwardForm::new("alpha".into(), None);
    form.kind = 2;
    form.focus = 1;
    form.inputs[1].set("1080");
    form.inputs[1].cursor = 2;
    form.scroll = 5;
    m.open_view(View::Forward(form));
    m.key(ctrl('c'));
    assert!(m.discard);
    let depth = m.parents.len();
    m.key(key(KeyCode::Char('x')));
    m.key(ctrl('s'));
    assert!(m.discard, "confirmation blocks editing/submission");
    m.key(key(KeyCode::Esc));
    let View::Forward(form) = &m.view else {
        panic!("forward form")
    };
    assert_eq!(form.kind, 2);
    assert_eq!(form.focus, 1);
    assert_eq!(form.inputs[1].value, "1080");
    assert_eq!(form.inputs[1].cursor, 2);
    assert_eq!(form.scroll, 5);
    assert_eq!(m.parents.len(), depth);
    discard(&mut m);
    assert!(matches!(m.view, View::Info(_)));
    assert_eq!(m.scroll, 3);
}

#[test]
fn host_validation_attaches_each_error_and_focuses_its_field() {
    use forms::FormField;
    for (field, invalid) in [
        (0, "bad name"),
        (1, "bad address"),
        (3, "65536"),
        (4, "/path/to/ssho-nonexistent-identity"),
        (2, "bad\nuser"),
        (5, "bad\njump"),
        (6, "bad\ncommand"),
        (7, "-o X=bad\0value"),
        (8, "bad\ntag"),
        (9, "bad\ncommand"),
        (10, "bad\ntty"),
    ] {
        let mut form = HostForm::add("valid".into(), "config".into());
        form.inputs[1].set("valid.example");
        form.inputs[field].set(invalid);
        form.focus_on(forms::Stop::Control(forms::Control::Cancel));
        assert!(!form.validate(), "field {field}");
        // The focus goes to the field and only its own section is unfolded.
        assert_eq!(
            form.current(),
            forms::Stop::Field(FormField::Input(field)),
            "field {field}"
        );
        assert_eq!(
            form.is_open(forms::Section::Advanced),
            [5, 6, 7, 9, 10].contains(&field)
        );
        assert_eq!(form.is_open(forms::Section::Organization), field == 8);
        assert_eq!(form.errors[0].field, FormField::Input(field));
        let error = form.errors[0].message.clone();
        let mut m = model();
        m.open_view(View::Host(Box::new(form)));
        let text = screen(&mut m, 120, 60);
        assert!(text.contains(&error), "field {field}: {text}");
    }
    let mut form = HostForm::edit(Host {
        aliases: vec!["alpha".into(), "bad name".into()],
        ..model().all[0].clone()
    });
    assert!(!form.validate());
    assert!(form.name_focus);
    assert_eq!(form.focus, 1);
    assert_eq!(form.errors[0].field, FormField::Name(1));
}

#[test]
fn forward_validation_attaches_and_focuses_each_active_field() {
    use forms::FormField;
    for kind in 0..3 {
        for (field, value) in [(1, "0"), (2, "bad host"), (3, "70000"), (4, "bad bind")] {
            if kind == 2 && [2, 3].contains(&field) {
                continue;
            }
            let mut f = ForwardForm::new("alpha".into(), None);
            f.kind = kind;
            f.inputs[1].set("8080");
            f.inputs[3].set("80");
            f.inputs[field].set(value);
            assert!(!f.validate());
            assert_eq!(f.focus, field);
            assert_eq!(f.errors[0].field, FormField::Input(field));
            let error = f.errors[0].message.clone();
            let mut m = model();
            m.open_view(View::Forward(f));
            assert!(screen(&mut m, 120, 40).contains(&error));
        }
    }
}

#[test]
fn fields_take_one_row_and_errors_stay_under_their_field() {
    let mut m = model();
    let mut f = HostForm::add("valid".into(), "config".into());
    f.inputs[1].set("bad address");
    f.inputs[3].set("70000");
    f.validate();
    m.open_view(View::Host(Box::new(f)));
    let text = screen(&mut m, 100, 26);
    let rows: Vec<_> = text.lines().collect();
    assert!(
        text.contains("2 champ(s) à corriger — invalid hostname"),
        "{text}"
    );
    let name = rows.iter().position(|s| s.contains("Alias *")).unwrap();
    let address = rows.iter().position(|s| s.contains("Adresse *")).unwrap();
    assert_eq!(address, name + 1);
    assert!(rows[address].contains("> Adresse *") && rows[address].contains("bad address"));
    assert!(rows[address + 1].contains("invalid hostname"));
    let port = rows
        .iter()
        .position(|s| s.trim_start().starts_with("Port "))
        .unwrap();
    assert!(rows[port].contains("70000") && rows[port + 1].contains("port must"));
    // On a narrow terminal the label goes above its input.
    let narrow = screen(&mut m, 40, 26);
    let rows: Vec<_> = narrow.lines().collect();
    let address = rows.iter().position(|s| s.contains("Adresse *")).unwrap();
    assert!(rows[address + 1].contains("> bad address"), "{narrow}");
    assert!(rows[address + 2].contains("invalid hostname"), "{narrow}");
}

#[test]
fn unicode_caret_stays_on_focused_input_after_wrapped_errors() {
    use ratatui::backend::Backend;
    for (w, h) in [(12, 10), (30, 12), (80, 24), (4, 4)] {
        let mut m = model();
        let mut f = HostForm::edit(Host {
            aliases: vec!["bad alias 界".repeat(8)],
            ..m.all[0].clone()
        });
        f.inputs[1].set("界e\u{301}👩‍💻.example");
        f.validate();
        f.name_focus = false;
        f.focus = 1;
        f.inputs[1].cursor = 0;
        m.open_view(View::Host(Box::new(f)));
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .backend_mut()
            .set_cursor_position((w - 1, h - 1))
            .unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let position = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(
            terminal.backend().buffer()[position].symbol(),
            "界",
            "{w}x{h}"
        );
    }
}

#[test]
fn input_viewport_measures_emoji_and_combining_graphemes() {
    let input = Input::new("界e\u{301}👩‍💻Z");
    assert_eq!(input.viewport(true, 20), ("界e\u{301}👩‍💻Z".into(), Some(6)));
    assert_eq!(input.viewport(true, 4), ("👩‍💻Z".into(), Some(3)));
    assert_eq!(
        Input::new("e\u{301}👩‍💻").viewport(false, 2),
        ("e\u{301}".into(), None)
    );
}

#[test]
fn host_confirmation_resume_preserves_advanced_tab_cursor_errors_and_parents() {
    let mut m = model();
    m.open_view(View::Info(m.all[0].clone()));
    m.scroll = 8;
    let mut form = HostForm::edit(m.all[0].clone());
    form.inputs[3].set("0");
    form.validate();
    form.reveal(forms::Section::Advanced);
    form.focus_on(forms::Stop::Field(forms::FormField::Input(9)));
    form.inputs[9].set("echo 界e\u{301}");
    form.inputs[9].cursor = 3;
    form.scroll = 9;
    let before = format!("{form:?}");
    m.open_view(View::Host(Box::new(form)));
    let parents = format!("{:?}", m.parents);
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(ctrl('c'));
    let View::Host(form) = &m.view else {
        panic!("host form")
    };
    assert_eq!(format!("{form:?}"), before);
    assert_eq!(format!("{:?}", m.parents), parents);
    discard(&mut m);
    assert!(matches!(m.view, View::Info(_)));
    assert_eq!(m.scroll, 8);
}

#[test]
fn ctrl_a_and_ctrl_d_remain_ordinary_text_editing_in_every_field() {
    let mut host = HostForm::edit(model().all[0].clone());
    host.key(ctrl('a'));
    host.key(ctrl('d'));
    assert_eq!(host.names[0].value, "lpha");
    assert_eq!(host.names.len(), 1);
    host.name_focus = false;
    for field in 0..11 {
        host.focus = field;
        host.inputs[field].set("value");
        host.key(ctrl('a'));
        assert_eq!(host.inputs[field].cursor, 0);
        host.key(ctrl('d'));
        assert_eq!(host.inputs[field].value, "alue");
        assert_eq!(host.names.len(), 1);
    }
    let mut forward = ForwardForm::new("alpha".into(), None);
    for field in 1..5 {
        forward.focus = field;
        forward.inputs[field].set("value");
        forward.key(ctrl('a'));
        forward.key(ctrl('d'));
        assert_eq!(forward.inputs[field].value, "alue");
        assert_eq!(forward.inputs[field].cursor, 0);
    }
}

#[test]
fn forward_unicode_caret_survives_wrapped_errors_and_tiny_terminals() {
    use ratatui::backend::Backend;
    for (w, h) in [(12, 10), (30, 12), (80, 24), (4, 4)] {
        let mut m = model();
        let mut f = ForwardForm::new("alpha".into(), None);
        f.inputs[1].set("70000");
        f.inputs[2].set("界e\u{301}👩‍💻");
        f.validate();
        f.focus = 2;
        f.inputs[2].cursor = 0;
        m.open_view(View::Forward(f));
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .backend_mut()
            .set_cursor_position((w - 1, h - 1))
            .unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let position = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(
            terminal.backend().buffer()[position].symbol(),
            "界",
            "{w}x{h}"
        );
    }
}

#[test]
fn help_keeps_existing_system_keys_after_form_shortcuts() {
    let mut m = model();
    m.open_view(View::Help);
    let text = screen(&mut m, 120, 50);
    for shortcut in [
        "Ctrl+S",
        "Alt+A/D",
        "Ctrl+A/E",
        "Modified draft:",
        "q, Ctrl+C Quitter",
        "back one level",
    ] {
        assert!(text.contains(shortcut), "missing {shortcut}: {text}");
    }
}

#[test]
fn ssh_return_preserves_browser_context_and_refreshes_history_without_resorting() {
    let root = tempfile::tempdir().unwrap();
    let mut history = History::load(root.path()).unwrap();
    history.record("alpha", None).unwrap();
    let mut m = model();
    m.history = history.connections.clone();
    m.query.set("a");
    m.show_hidden = true;
    m.recent = true;
    m.filter(true);
    m.selected = 1;
    m.offset = 1;
    m.scroll = 3;
    m.details.shown = true;
    m.open_view(View::Forward(ForwardForm::new("beta".into(), None)));
    history.record("beta", None).unwrap();
    m.resume_connection("beta", Ok(37), Ok(history));
    assert_eq!(m.query.value, "a");
    assert!(m.search_focus && m.show_hidden && m.recent && m.details.shown);
    assert_eq!(m.visible, vec![0, 1]);
    assert_eq!((m.selected, m.offset, m.scroll), (1, 1, 3));
    assert_eq!(m.selected_host().unwrap().name, "beta");
    assert_eq!(m.history["beta"].connect_count, 1);
    assert!(matches!(m.view, View::List));
    assert!(m.parents.is_empty());
    assert!(screen(&mut m, 120, 40).contains("SSH exit 37: beta"));
    m.key(key(KeyCode::F(2)));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("beta".into(), None)
    );
}

#[test]
fn ssh_return_displays_success_spawn_error_and_history_refresh_error() {
    let root = tempfile::tempdir().unwrap();
    let mut m = model();
    m.resume_connection("alpha", Ok(0), History::load(root.path()));
    assert!(screen(&mut m, 80, 24).contains("SSH exit 0: alpha"));
    m.resume_connection(
        "alpha",
        Err(anyhow::anyhow!("SSH unavailable")),
        History::load(root.path()),
    );
    assert!(screen(&mut m, 80, 24).contains("SSH unavailable"));
    m.resume_connection("alpha", Ok(143), Err(anyhow::anyhow!("broken history")));
    let rendered = screen(&mut m, 120, 40);
    assert!(rendered.contains("SSH exit 143: alpha"));
    assert!(rendered.contains("broken history"));
    assert_eq!(m.key(ctrl('c')), Action::Quit);
}

#[test]
fn ssh_result_does_not_shift_the_viewport_when_selection_is_at_its_bottom() {
    let root = tempfile::tempdir().unwrap();
    let mut m = model();
    m.all = (0..100)
        .map(|i| Host {
            name: format!("server-{i:03}"),
            ..Host::default()
        })
        .collect();
    m.filter(true);
    m.selected = 70;
    screen(&mut m, 100, 24);
    let before = m.offset;
    m.resume_connection("server-070", Ok(0), History::load(root.path()));
    let rendered = screen(&mut m, 100, 24);
    assert!(rendered.contains("SSH exit 0: server-070"));
    assert_eq!(m.offset, before);
    m.resume_connection("server-070", Ok(0), Err(anyhow::anyhow!("broken history")));
    screen(&mut m, 100, 24);
    assert_eq!(m.offset, before);
}

#[test]
fn ssh_result_stays_readable_for_long_aliases_in_compact_terminals() {
    let root = tempfile::tempdir().unwrap();
    let mut m = model();
    m.resume_connection(&"long-alias".repeat(10), Ok(37), History::load(root.path()));
    assert!(screen(&mut m, 28, 8).contains("exit 37"));
}

#[test]
fn effective_details_are_shared_pending_errors_and_raw_edits_remain_distinct() {
    let mut m = model();
    m.key(key(KeyCode::F(4)));
    assert!(screen(&mut m, 170, 50).contains("Pending"));
    let value = runtime::effective::EffectiveConfig::parse("hostname resolved.example\nuser inherited-user\nport 2207\nidentityfile /effective/key-one\nidentityfile /effective/key-two\nproxyjump bastion\nremotecommand uptime\nrequesttty force\n").unwrap();
    let raw = m.all[0].clone();
    m.effective.insert(raw.name.clone(), Ok(value));
    let pane = screen(&mut m, 170, 50);
    for expected in [
        "Resolved via ssh -G",
        "resolved.example",
        "inherited-user",
        "Port: 2207",
        "/effective/key-one",
        "/effective/key-two",
        "ProxyJump: bastion",
        "RemoteCommand: uptime",
        "RequestTTY: force",
        "Declared source",
        "? Unknown",
    ] {
        assert!(pane.contains(expected), "missing {expected}: {pane}");
    }
    m.open_view(View::Info(raw.clone()));
    let info = screen(&mut m, 170, 50);
    assert!(info.contains("inherited-user"));
    assert!(info.contains("RemoteCommand: uptime"));
    assert_eq!(m.all[0], raw);
    m.effective
        .insert(raw.name.clone(), Err("resolver-fixture-failure".into()));
    let info = screen(&mut m, 170, 50);
    assert!(info.contains("resolver-fixture-failure"));
    assert!(info.contains("showing declared values"));
}

#[test]
fn inspection_refresh_clears_cache_and_resolves_all_aliases_without_ping() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    std::fs::write(
        &path,
        "Host first second\n HostName 127.0.0.1\nHost *\n User inherited\n",
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&path).unwrap(),
        AppConfig {
            ping_on_start: false,
            ..AppConfig::default()
        },
        BTreeMap::new(),
        false,
    );
    let raw = m.all.clone();
    let inspector = runtime::effective::Inspector::default();
    let inspection = start_inspection(&inspector, &mut m, &path, false);
    for result in inspection {
        assert!(result.ping.is_none());
        m.effective.insert(result.name, result.effective);
    }
    assert_eq!(m.effective.len(), 2);
    assert!(m
        .effective
        .values()
        .all(|r| r.as_ref().unwrap().user == "inherited"));
    assert_eq!(m.all, raw);
    let inspection = start_inspection(&inspector, &mut m, &path, false);
    assert!(m.effective.is_empty());
    drop(inspection);
}

#[test]
fn diagnostic_symbols_text_latency_and_failure_are_color_independent() {
    for status in [
        PingStatus::Unknown,
        PingStatus::Connecting,
        PingStatus::Online,
        PingStatus::Offline,
    ] {
        let host = Host::default();
        let result = PingResult {
            method: crate::storage::AvailabilityMethod::Ssh,
            name: String::new(),
            status,
            duration: Duration::from_millis(123),
            error: Some("cause-fixture".into()),
        };
        let text = details::content(&host, Some(&result), None, None, "")
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(status.label()));
        assert!(text.contains("123 ms"));
        assert!(text.contains("cause-fixture"));
    }
}

#[test]
fn manual_reload_key_works_from_search_table_and_info() {
    let mut m = model();
    typing(&mut m, "a");
    assert_eq!(m.key(key(KeyCode::F(5))), Action::Reload);
    assert_eq!(m.query.value, "a");
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::F(5))), Action::Reload);
    m.key(key(KeyCode::Char('i')));
    assert!(matches!(m.view, View::Info(_)));
    assert_eq!(m.key(key(KeyCode::F(5))), Action::Reload);
}

#[test]
fn manual_reload_palette_is_available_with_empty_filtered_list_and_retains_caret() {
    let mut m = model();
    typing(&mut m, "missing");
    m.key(key(KeyCode::Left));
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "recharger");
    assert!(screen(&mut m, 100, 30).contains("Recharger"));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Reload);
    assert!(matches!(m.view, View::List));
    assert!(m.search_focus);
    typing(&mut m, "X");
    assert_eq!(m.query.value, "missinXg");
}

#[test]
fn manual_reload_does_not_interrupt_forms_or_confirmations() {
    let mut m = model();
    m.search_focus = false;
    m.key(key(KeyCode::Char('e')));
    typing(&mut m, "changed");
    assert_eq!(m.key(key(KeyCode::F(5))), Action::None);
    let View::Host(form) = &m.view else {
        panic!("still editing");
    };
    assert!(form.dirty());
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    assert_eq!(m.key(key(KeyCode::F(5))), Action::None);
    assert!(m.discard);
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Enter));
    m.key(key(KeyCode::Char('f')));
    typing(&mut m, "123");
    assert_eq!(m.key(key(KeyCode::F(5))), Action::None);
    assert!(matches!(m.view, View::Forward(_)));
    // The forwarding type has the focus: nothing was typed, so Esc just returns.
    m.key(key(KeyCode::Esc));
    assert!(!m.discard && matches!(m.view, View::List));
    assert!(matches!(m.key(key(KeyCode::Char('d'))), Action::Delete(_)));
}

#[test]
fn manual_reload_reloads_root_and_include_changes_preserving_browser_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let includes = temp.path().join("parts");
    std::fs::create_dir(&includes).unwrap();
    let retained = includes.join("retained");
    let removed = includes.join("removed");
    std::fs::write(
        &root,
        "Include parts/*\nHost srv-duplicate\n HostName root-old\n",
    )
    .unwrap();
    std::fs::write(&retained, "Host srv-duplicate\n HostName include-old\n").unwrap();
    std::fs::write(&removed, "Host srv-removed\n HostName gone\n").unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        true,
    );
    typing(&mut m, "srv");
    m.key(key(KeyCode::Left));
    m.show_hidden = true;
    m.recent = true;
    m.details.shown = true;
    m.selected = m
        .visible
        .iter()
        .position(|&i| m.all[i].name == "srv-duplicate" && m.all[i].source_file == root)
        .unwrap();
    m.offset = m.selected;
    let inspector = runtime::effective::Inspector::default();
    std::fs::write(
        &root,
        "Include parts/*\nHost srv-duplicate\n HostName root-new\nHost srv-new\n HostName new\n",
    )
    .unwrap();
    std::fs::write(&retained, "Host srv-duplicate\n HostName include-new\n").unwrap();
    std::fs::remove_file(&removed).unwrap();
    std::fs::write(includes.join("added"), "Host srv-added\n HostName added\n").unwrap();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert_eq!(m.all.len(), 4);
    assert!(!m.all.iter().any(|h| h.name == "srv-removed"));
    assert!(m.all.iter().any(|h| h.name == "srv-added"));
    assert!(m.all.iter().any(|h| h.hostname == "include-new"));
    let selected = m.selected_host().unwrap();
    assert_eq!(selected.source_file, root);
    assert_eq!(selected.name, "srv-duplicate");
    assert_eq!(selected.hostname, "root-new");
    assert_eq!(m.query.value, "srv");
    assert!(m.search_focus && m.show_hidden && m.recent && m.details.shown);
    assert_eq!(m.offset, 1);
    typing(&mut m, "X");
    assert_eq!(m.query.value, "srXv");
    m.query.set("");
    m.filter(false);
    m.selected = m.visible.len() - 1;
    m.offset = m.selected;
    std::fs::remove_file(&retained).unwrap();
    std::fs::remove_file(includes.join("added")).unwrap();
    std::fs::write(&root, "Include parts/*\nHost only\n HostName last\n").unwrap();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert_eq!(m.all.len(), 1);
    assert_eq!(m.selected, 0);
    assert_eq!(m.offset, 0);
    assert_eq!(m.selected_host().unwrap().name, "only");
}

#[test]
fn manual_reload_read_errors_leave_hosts_cache_view_and_selection_intact() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(&root, "Include parts/*\nHost keep\n HostName old\n").unwrap();
    let includes = temp.path().join("parts");
    std::fs::create_dir(&includes).unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        true,
    );
    m.effective
        .insert("keep".into(), Err("previous diagnostic".into()));
    m.statuses.insert(
        "keep".into(),
        PingResult {
            method: crate::storage::AvailabilityMethod::Ssh,
            name: "keep".into(),
            status: PingStatus::Online,
            duration: Duration::from_millis(12),
            error: None,
        },
    );
    m.open_view(View::Info(m.all[0].clone()));
    let before = m.all.clone();
    let inspector = runtime::effective::Inspector::default();
    std::fs::remove_file(&root).unwrap();
    let error = reload_hosts(&inspector, &mut m, &root).unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(format!("{error:#}").contains(&root.display().to_string()));
    m.error = format!("{error:#}");
    let rendered = screen(&mut m, 120, 40);
    assert!(rendered.contains("Error: reading SSH config"));
    assert!(rendered.contains(&root.display().to_string()));
    assert_eq!(m.all, before);
    assert_eq!(m.selected_host().unwrap().name, "keep");
    assert!(matches!(m.view, View::Info(_)));
    assert_eq!(
        m.effective["keep"].as_ref().unwrap_err(),
        "previous diagnostic"
    );
    assert_eq!(m.statuses["keep"].status, PingStatus::Online);
    std::fs::write(&root, "Include parts/*\nHost changed\n HostName new\n").unwrap();
    std::fs::write(includes.join("unreadable"), [0xff]).unwrap();
    let error = reload_hosts(&inspector, &mut m, &root).unwrap_err();
    assert!(format!("{error:#}").contains("unreadable"));
    assert_eq!(m.all, before);
    assert_eq!(
        m.effective["keep"].as_ref().unwrap_err(),
        "previous diagnostic"
    );
}

#[test]
fn manual_reload_refreshes_info_or_returns_to_list_when_identity_is_removed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(&root, "Host keep\n HostName old\n").unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    m.open_view(View::Info(m.all[0].clone()));
    m.error = "previous failure".into();
    let inspector = runtime::effective::Inspector::default();
    std::fs::write(&root, "Host keep\n HostName changed\n").unwrap();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    let View::Info(host) = &m.view else {
        panic!("keep info open");
    };
    assert_eq!(host.hostname, "changed");
    assert!(screen(&mut m, 120, 40).contains("changed"));
    assert!(m.error.is_empty());
    std::fs::write(&root, "Host other\n HostName changed\n").unwrap();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert!(matches!(m.view, View::List));
    assert!(m.parents.is_empty());
    assert_eq!(m.selected_host().unwrap().name, "other");
}

#[test]
fn manual_reload_refreshes_effective_cache_without_ping() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(
        &root,
        "Host fixture\n HostName 127.0.0.1\n User old-user\n Port 2201\n",
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig {
            ping_on_start: false,
            ..AppConfig::default()
        },
        BTreeMap::new(),
        false,
    );
    let inspector = runtime::effective::Inspector::default();
    let mut receiver = start_inspection(&inspector, &mut m, &root, false);
    for result in &receiver {
        m.effective.insert(result.name, result.effective);
    }
    assert_eq!(m.effective["fixture"].as_ref().unwrap().user, "old-user");
    m.statuses.get_mut("fixture").unwrap().status = PingStatus::Online;
    std::fs::write(
        &root,
        "Host fixture\n HostName 127.0.0.2\n User new-user\n Port 2202\n",
    )
    .unwrap();
    receiver = reload_hosts(&inspector, &mut m, &root).unwrap();
    assert!(m.effective.is_empty());
    assert_eq!(m.statuses["fixture"].status, PingStatus::Unknown);
    for result in receiver {
        assert!(result.ping.is_none());
        m.effective.insert(result.name, result.effective);
    }
    let effective = m.effective["fixture"].as_ref().unwrap();
    assert_eq!(effective.user, "new-user");
    assert_eq!(effective.hostname, "127.0.0.2");
    assert_eq!(effective.port, "2202");
    assert_eq!(m.all[0].user, "new-user");
}

#[cfg(unix)]
#[test]
fn manual_reload_old_native_generation_cannot_overwrite_new_cache() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let ready = temp.path().join("ready");
    let release = temp.path().join("release");
    let finished = temp.path().join("finished");
    // Block a real old ssh -G after it has read the old values. The new generation
    // can finish on another worker while this one still owns its old sender.
    std::fs::write(&root, format!(
        "Host fixture\n HostName 127.0.0.1\n User old-user\n Port 2201\nMatch exec \"touch {}; while test ! -e {}; do sleep 0.02; done; touch {}\"\n",
        ready.display(), release.display(), finished.display()
    )).unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let inspector = runtime::effective::Inspector::default();
    let mut receiver = start_inspection(&inspector, &mut m, &root, false);
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "old native resolver did not reach barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        receiver.try_recv().is_err(),
        "old generation is still blocked"
    );
    std::fs::write(
        &root,
        "Host fixture\n HostName 127.0.0.2\n User new-user\n Port 2202\n",
    )
    .unwrap();
    receiver = reload_hosts(&inspector, &mut m, &root).unwrap();
    let fresh = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(fresh.effective.as_ref().unwrap().user, "new-user");
    m.effective.insert(fresh.name, fresh.effective);
    std::fs::write(&release, "").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !finished.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "old native resolver did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    for result in receiver {
        m.effective.insert(result.name, result.effective);
    }
    let effective = m.effective["fixture"].as_ref().unwrap();
    assert_eq!(effective.user, "new-user");
    assert_eq!(effective.hostname, "127.0.0.2");
    assert_eq!(effective.port, "2202");
}

#[cfg(unix)]
#[test]
fn manual_reload_unreadable_include_directory_preserves_previous_snapshot() {
    use std::os::unix::fs::PermissionsExt;
    struct RestorePermissions(PathBuf, std::fs::Permissions);
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            std::fs::set_permissions(&self.0, self.1.clone()).unwrap();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let parts = temp.path().join("parts");
    std::fs::create_dir_all(parts.join("nested")).unwrap();
    std::os::unix::fs::symlink(parts.join("nested"), temp.path().join("alias-dir")).unwrap();
    std::fs::write(
        &root,
        "Include parts/*\nHost root-host\n HostName root.example\n",
    )
    .unwrap();
    std::fs::write(
        parts.join("hosts"),
        "Host included-host\n HostName include.example\n",
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let hosts = m.all.clone();
    m.effective
        .insert("included-host".into(), Err("previous diagnostic".into()));
    let _restore = RestorePermissions(
        parts.clone(),
        std::fs::metadata(&parts).unwrap().permissions(),
    );
    std::fs::set_permissions(&parts, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&parts).is_ok() {
        // Elevated identities can bypass mode bits; don't claim permission coverage there.
        eprintln!("permission regression unavailable: current identity bypasses mode 000");
        return;
    }
    let traversal = glob::glob(&format!("{}/*", parts.display()))
        .unwrap()
        .find_map(Result::err)
        .expect("fixture must produce an actual glob traversal error");
    assert_eq!(
        traversal.error().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let inspector = runtime::effective::Inspector::default();
    // glob reports the first traversal error but silently drops metadata failures
    // on literal components. Strict discovery must surface every permission error.
    for pattern in ["parts/*", "parts/hosts", "*/hosts", "alias*/*"] {
        std::fs::write(
            &root,
            format!("Include {pattern}\nHost root-host\n HostName root.example\n"),
        )
        .unwrap();
        let legacy = config::load_hosts(&root).unwrap();
        assert_eq!(
            legacy.len(),
            1,
            "legacy parser still ignores Include failures"
        );
        assert_eq!(legacy[0].name, "root-host");
        let error = reload_hosts(&inspector, &mut m, &root).unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let cause = format!("{error:#}");
        assert!(cause.starts_with("expanding SSH Include"));
        assert!(cause.contains(&temp.path().display().to_string()));
        assert_eq!(m.all, hosts);
        assert_eq!(
            m.effective["included-host"].as_ref().unwrap_err(),
            "previous diagnostic"
        );
    }
}

#[test]
fn manual_reload_allows_unmatched_literal_and_wildcard_includes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    std::fs::write(
        &root,
        "Include missing.conf absent/*.conf\nHost keep\n HostName 127.0.0.1\n",
    )
    .unwrap();
    let mut m = model();
    let inspector = runtime::effective::Inspector::default();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert_eq!(m.all.len(), 1);
    assert_eq!(m.all[0].name, "keep");
}

#[test]
fn favorites_notes_shortcuts_and_discard_are_available_without_ssh_edits() {
    let mut m = model();
    m.search_focus = false;
    assert_ne!(m.key(key(KeyCode::Char('*'))), Action::None);
    m.key(key(KeyCode::Char('N')));
    assert!(!matches!(m.view, View::List));
    typing(&mut m, "personal note");
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(key(KeyCode::Esc));
    assert!(!m.discard);
    assert_ne!(m.key(ctrl('s')), Action::None);
}

#[test]
fn favorites_commit_before_reordering_preserve_identity_and_filter_empty_palette() {
    let temp = tempfile::tempdir().unwrap();
    let mut storage = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
    let mut m = model();
    m.search_focus = false;
    m.selected = 1;
    let beta = m.selected_host().unwrap();
    let action = m.key(key(KeyCode::Char('*')));
    assert!(!m.is_favorite("beta"));
    apply_metadata_action(&mut m, &mut storage, &action).unwrap();
    assert!(m.is_favorite("beta"));
    assert_eq!(m.selected_host(), Some(beta));
    assert_eq!(m.visible[0], 1);
    m.recent = true;
    m.filter(false);
    assert_eq!(m.visible[0], 1);
    m.activate_action(PaletteAction::FavoritesOnly);
    assert_eq!(m.visible.len(), 1);
    let action = m.key(key(KeyCode::Char('*')));
    apply_metadata_action(&mut m, &mut storage, &action).unwrap();
    assert!(m.visible.is_empty());
    m.key(key(KeyCode::F(2)));
    typing(&mut m, "favoris seuls");
    let text = screen(&mut m, 80, 24);
    assert!(
        text.contains("Favoris seuls") && text.contains("activé"),
        "{text}"
    );
    m.key(key(KeyCode::Enter));
    assert!(!m.favorites_only);
    assert_eq!(m.visible.len(), 2);
    // A failed favorite write leaves authoritative storage/model/selection intact.
    fs::remove_file(temp.path().join("metadata.json")).unwrap();
    fs::create_dir(temp.path().join("metadata.json")).unwrap();
    let selected = m.selected_host();
    let action = m.key(key(KeyCode::Char('*')));
    assert!(apply_metadata_action(&mut m, &mut storage, &action).is_err());
    assert_eq!(m.selected_host(), selected);
    assert!(!m.is_favorite(&selected.unwrap().name));
}

#[test]
fn notes_cancel_failed_save_success_and_shared_details() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    fs::write(&config, "Host alpha\n HostName a.example\n").unwrap();
    let before = fs::read(&config).unwrap();
    let mut storage = Metadata::load(temp.path(), &config).unwrap();
    let mut m = model();
    m.search_focus = false;
    m.key(key(KeyCode::Char('N')));
    typing(&mut m, "discard me");
    discard(&mut m);
    assert!(matches!(m.view, View::List));
    assert!(!temp.path().join("metadata.json").exists());
    m.key(key(KeyCode::Char('N')));
    typing(&mut m, "personal 界");
    m.key(key(KeyCode::Enter));
    typing(&mut m, "second line");
    let action = m.key(ctrl('s'));
    fs::create_dir(temp.path().join("metadata.json")).unwrap();
    assert!(apply_metadata_action(&mut m, &mut storage, &action).is_err());
    assert!(matches!(&m.view,View::Note(editor) if editor.input.value=="personal 界\nsecond line"));
    assert_eq!(m.note("alpha"), "");
    fs::remove_dir(temp.path().join("metadata.json")).unwrap();
    apply_metadata_action(&mut m, &mut storage, &action).unwrap();
    assert!(matches!(m.view, View::List));
    assert_eq!(
        Metadata::load(temp.path(), &config).unwrap().hosts()["alpha"].note,
        "personal 界\nsecond line"
    );
    m.key(key(KeyCode::Char('i')));
    assert!(screen(&mut m, 100, 30).contains("Personal note: personal 界"));
    m.key(key(KeyCode::Esc));
    m.details.shown = true;
    assert!(screen(&mut m, 120, 30).contains("Personal note: personal 界"));
    assert_eq!(fs::read(config).unwrap(), before);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}

#[test]
fn notes_vertical_navigation_and_line_home_end() {
    let mut editor = notes::Editor::new("alpha".into(), "first\nsecond");
    editor.key(key(KeyCode::Up));
    assert_eq!(editor.input.cursor, 5);
    editor.key(key(KeyCode::Home));
    assert_eq!(editor.input.cursor, 0);
    editor.key(key(KeyCode::Down));
    assert_eq!(editor.input.cursor, 6);
    editor.key(key(KeyCode::End));
    assert_eq!(editor.input.cursor, 12);
}

#[test]
fn notes_rename_committed_ssh_refreshes_ui_on_metadata_collision() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    fs::write(&config, "Host old\n HostName a.example\n").unwrap();
    let mut storage = Metadata::load(temp.path(), &config).unwrap();
    storage.set_note("old", "recover me").unwrap();
    storage.set_note("new", "keep me").unwrap();
    let old = config::find_host(&config, "old").unwrap();
    let mut replacement = old.clone();
    replacement.name = "new".into();
    replacement.aliases.clear();
    let mut m = Model::new(
        vec![old.clone()],
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    m.view = View::Host(Box::new(HostForm::edit(old.clone())));
    let action = Action::Save(
        Some(old.clone()),
        Box::new(replacement.clone()),
        config.clone(),
    );
    config::update_host(&old, &replacement, temp.path()).unwrap();
    complete_saved_mutation(
        &mut m,
        config::load_hosts(&config).unwrap(),
        &action,
        &mut storage,
    );
    assert!(matches!(m.view, View::List));
    assert_eq!(m.selected_host().unwrap().name, "new");
    assert!(m.error.contains("SSH saved; metadata rename failed"));
    assert!(m.error.contains("occupied"));
    assert_eq!(storage.hosts()["old"].note, "recover me");
    assert_eq!(storage.hosts()["new"].note, "keep me");
}

#[test]
fn notes_ui_rename_transfers_ordinary_and_secondary_alias_metadata() {
    for secondary in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        fs::write(
            &config,
            if secondary {
                "Host primary old\n HostName a.example\n"
            } else {
                "Host old\n HostName a.example\n"
            },
        )
        .unwrap();
        let mut storage = Metadata::load(temp.path(), &config).unwrap();
        storage.set_note("old", "personal note").unwrap();
        storage.set_favorite("old", true).unwrap();
        if secondary {
            storage.set_note("primary", "other note").unwrap();
        }
        let old = config::find_host(&config, "old").unwrap();
        let mut form = HostForm::edit(old.clone());
        form.names[usize::from(secondary)].set("renamed");
        let replacement = form.host().unwrap();
        let mut m = Model::new(
            config::load_hosts(&config).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        m.metadata = storage.hosts().clone();
        m.filter(false);
        let action = Action::Save(
            Some(old.clone()),
            Box::new(replacement.clone()),
            config.clone(),
        );
        config::update_host(&old, &replacement, temp.path()).unwrap();
        complete_saved_mutation(
            &mut m,
            config::load_hosts(&config).unwrap(),
            &action,
            &mut storage,
        );
        assert!(m.error.is_empty());
        assert_eq!(storage.hosts()["renamed"].note, "personal note");
        assert!(storage.hosts()["renamed"].favorite);
        assert!(!storage.hosts().contains_key("old"));
        assert_eq!(m.all[m.visible[0]].name, "renamed");
        assert_eq!(m.selected_host().unwrap().name, replacement.name);
        if secondary {
            assert_eq!(storage.hosts()["primary"].note, "other note");
        }
    }
}

#[test]
fn notes_editor_draws_unicode_and_long_lines_at_compact_sizes() {
    use ratatui::backend::Backend;
    let mut m = model();
    m.search_focus = false;
    m.key(key(KeyCode::Char('N')));
    typing(&mut m, "界e\u{301}👩‍💻".repeat(15).as_str());
    m.key(key(KeyCode::Enter));
    typing(&mut m, "tail");
    for (w, h) in [(100, 30), (40, 10), (10, 5), (1, 1)] {
        screen(&mut m, w, h);
    }
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    assert!(cursor.x < 40 && cursor.y < 10);
    m.key(key(KeyCode::Up));
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    assert!(cursor.x < 40 && cursor.y < 10);
}

#[test]
fn favorites_preferred_add_from_palette_reveals_real_save_even_when_filter_empty() {
    for existing_favorite in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        fs::write(&config, "Host existing\n HostName existing.example\n").unwrap();
        let mut metadata = Metadata::load(temp.path(), &config).unwrap();
        if existing_favorite {
            metadata.set_favorite("existing", true).unwrap();
        }
        let mut m = Model::new(
            config::load_hosts(&config).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        m.metadata = metadata.hosts().clone();
        m.activate_action(PaletteAction::FavoritesOnly);
        assert_eq!(m.visible.len(), usize::from(existing_favorite));
        let Action::Add(seed) = palette(&mut m, "ajouter") else {
            panic!("F2 Add must work without a selected host");
        };
        select_files(&mut m, &config, None, seed).unwrap();
        typing(&mut m, "new-host");
        m.key(key(KeyCode::Tab));
        typing(&mut m, "new.example");
        let action = m.key(ctrl('s'));
        let Action::Save(None, host, target) = &action else {
            panic!("new host form must submit");
        };
        config::add_host(host, target, temp.path()).unwrap();
        complete_saved_mutation(
            &mut m,
            config::load_hosts(&config).unwrap(),
            &action,
            &mut metadata,
        );
        assert!(
            !m.favorites_only,
            "a nonfavorite save must disable its incompatible filter"
        );
        assert_eq!(m.selected_host().unwrap().name, "new-host");
        assert!(m.visible.iter().any(|&i| m.all[i].name == "new-host"));
        assert!(!m.is_favorite("new-host"));
        assert_eq!(
            config::find_host(&config, "new-host").unwrap().hostname,
            "new.example"
        );
    }
}

#[test]
fn favorites_preferred_mutations_preserve_compatible_filter_and_reload() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    fs::write(
        &config,
        "Host favorite\n HostName favorite.example\nHost ordinary\n HostName ordinary.example\n",
    )
    .unwrap();
    let mut metadata = Metadata::load(temp.path(), &config).unwrap();
    metadata.set_favorite("favorite", true).unwrap();
    let hosts = config::load_hosts(&config).unwrap();
    let favorite = hosts
        .iter()
        .find(|host| host.name == "favorite")
        .unwrap()
        .clone();
    let ordinary = hosts
        .iter()
        .find(|host| host.name == "ordinary")
        .unwrap()
        .clone();
    let mut m = Model::new(hosts.clone(), AppConfig::default(), BTreeMap::new(), false);
    m.metadata = metadata.hosts().clone();
    m.activate_action(PaletteAction::FavoritesOnly);
    let action = Action::Save(
        Some(favorite.clone()),
        Box::new(favorite.clone()),
        config.clone(),
    );
    complete_saved_mutation(&mut m, hosts.clone(), &action, &mut metadata);
    assert!(
        m.favorites_only,
        "a compatible save must retain favorites-only"
    );
    assert_eq!(m.selected_host().unwrap().name, "favorite");
    let inspector = runtime::effective::Inspector::default();
    let _receiver = reload_hosts(&inspector, &mut m, &config).unwrap();
    assert!(m.favorites_only, "ordinary reload must preserve filtering");
    assert_eq!(m.visible.len(), 1);
    let target = temp.path().join("moved");
    let moved = Host {
        source_file: target.clone(),
        ..favorite.clone()
    };
    m.complete_mutation(
        vec![moved.clone(), ordinary.clone()],
        &Action::MoveTo(favorite, target),
    );
    assert!(
        m.favorites_only,
        "a compatible move must retain favorites-only"
    );
    assert_eq!(m.selected_host(), Some(moved));
    // A preferred nonfavorite result needs the same visibility guarantee for moves.
    let target = temp.path().join("ordinary-moved");
    let moved = Host {
        source_file: target.clone(),
        ..ordinary.clone()
    };
    m.complete_mutation(vec![moved.clone()], &Action::MoveTo(ordinary, target));
    assert!(!m.favorites_only);
    assert_eq!(m.selected_host(), Some(moved));
}

#[test]
fn notes_control_a_e_match_line_local_home_end_without_changing_multiline_text() {
    let value = "first\nsecond\nthird";
    let mut editor = notes::Editor::new("alpha".into(), value);
    editor.input.cursor = 9;
    assert_eq!(editor.key(ctrl('a')), FormEvent::None);
    assert_eq!(editor.input.cursor, 6, "Ctrl+A must start the current line");
    editor.key(ctrl('e'));
    assert_eq!(editor.input.cursor, 12, "Ctrl+E must end the current line");
    editor.key(key(KeyCode::Home));
    assert_eq!(editor.input.cursor, 6);
    editor.key(key(KeyCode::End));
    assert_eq!(editor.input.cursor, 12);
    assert_eq!(editor.input.value, value);
    assert!(!editor.dirty());
}

#[test]
fn advanced_search_fields_phrases_notes_and_unknown_prefix() {
    let mut m = model();
    m.all[0].source_file = PathBuf::from("/tmp/Team Hosts/config");
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            favorite: true,
            note: "Release café 東京 ticket:42".into(),
        },
    );
    for query in [
        "tag:WORK user:DEPLOY file:\"Team Hosts\" is:favorite",
        "\"Release café\" 東京",
        "ticket:42",
        "is:unknown alpha",
    ] {
        m.query.set(query);
        m.filter(true);
        assert_eq!(
            m.selected_host().map(|h| h.name),
            Some("alpha".into()),
            "{query}"
        );
    }
    for query in ["tag:other", "user:root", "is:online", "\"café Release\""] {
        m.query.set(query);
        m.filter(true);
        assert!(m.visible.is_empty(), "{query}");
    }
}

#[test]
fn advanced_search_errors_are_actionable_and_recover_without_losing_other_errors() {
    let mut m = model();
    m.error = "reload failure".into();
    for (query, expected) in [
        ("is:maybe", "online"),
        ("tag:", "value"),
        ("\"unfinished", "quote"),
    ] {
        m.query.set(query);
        m.filter(true);
        assert!(m.visible.is_empty());
        assert!(screen(&mut m, 110, 20).contains(expected), "{query}");
    }
    m.query.set("alpha");
    m.filter(true);
    assert_eq!(m.visible.len(), 1);
    assert_eq!(m.error, "reload failure");
}

#[test]
fn advanced_search_preferred_host_retains_compatible_expression_and_long_query() {
    let mut m = model();
    let query = "tag:work user:deploy alpha a.example tag:work user:deploy";
    typing(&mut m, query);
    assert_eq!(m.query.value, query);
    let preferred = m.all[0].clone();
    m.replace_hosts(m.all.clone(), Some(preferred));
    assert_eq!(m.query.value, query);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
}

#[test]
fn advanced_search_resolved_user_and_address_override_declared_values() {
    let mut m = model();
    m.effective.insert(
        "alpha".into(),
        Ok(runtime::effective::EffectiveConfig::parse(
            "hostname resolved.example\nuser inherited\nport 22\n",
        )
        .unwrap()),
    );
    m.query.set("user:inherited resolved.example");
    m.filter(true);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.query.set("deploy");
    m.filter(true);
    assert!(m.visible.is_empty());
}

#[test]
fn advanced_search_selected_status_survives_compact_connection_title() {
    let mut m = model();
    m.connection_result = "SSH exit 0: alpha".into();
    let text = screen(&mut m, 40, 8);
    assert!(text.contains("Unknown"), "{text}");
    assert!(text.contains("alpha"), "{text}");
}

#[test]
fn advanced_search_explicit_and_et_and_literal_quoted_connectors() {
    let mut m = model();
    for query in [
        "tag:work AND user:deploy",
        "tag:work et user:deploy",
        "alpha AnD tag:work ET deploy",
    ] {
        m.query.set(query);
        m.filter(true);
        assert_eq!(
            m.selected_host().map(|h| h.name),
            Some("alpha".into()),
            "{query}"
        );
    }
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            favorite: false,
            note: "AND ET".into(),
        },
    );
    for query in ["alpha \"AND\"", "alpha \"ET\""] {
        m.query.set(query);
        m.filter(true);
        assert_eq!(m.visible.len(), 1);
    }
    m.query.set("tag:and");
    m.filter(true);
    assert!(m.visible.is_empty());
    for query in ["AND alpha", "alpha ET", "alpha AND ET deploy"] {
        m.query.set(query);
        m.filter(true);
        assert!(
            m.search.as_ref().unwrap_err().contains("conjunction"),
            "{query}"
        );
    }
}

#[test]
fn advanced_search_unicode_highlights_preserve_graphemes_and_selected_markers() {
    let q = search::Query::parse("i 東京 ς").unwrap();
    assert_eq!(q.ranges("İ東京 ΟΣ", search::Field::Any), vec![0..8, 11..13]);
    let q = search::Query::parse("e").unwrap();
    assert_eq!(q.ranges("e\u{301}cho", search::Field::Any), vec![0..3]);
    let q = search::Query::parse("tag:work user:deploy is:favorite").unwrap();
    assert!(q
        .ranges("work deploy favorite", search::Field::Any)
        .is_empty());
    assert_eq!(q.ranges("#work", search::Field::Tag), vec![1..5]);
    let mut m = model();
    m.all[0].name = "İ東京".into();
    m.metadata.insert(
        "İ東京".into(),
        HostMetadata {
            favorite: true,
            note: String::new(),
        },
    );
    m.query.set("i 東京");
    m.filter(true);
    let mut terminal = Terminal::new(TestBackend::new(100, 15)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    let cells = terminal.backend().buffer().content();
    for symbol in ["İ", "東", "京"] {
        let cell = cells
            .iter()
            .find(|cell| {
                cell.symbol() == symbol && cell.bg == ratatui::style::Color::Rgb(0, 173, 216)
            })
            .unwrap();
        assert_eq!(cell.fg, ratatui::style::Color::Black);
        assert!(cell.modifier.contains(ratatui::style::Modifier::UNDERLINED));
    }
    let marker = cells
        .iter()
        .find(|cell| cell.symbol() == "*" && cell.bg == ratatui::style::Color::Rgb(0, 173, 216))
        .unwrap();
    assert_eq!(marker.fg, ratatui::style::Color::Black);
    assert!(!marker
        .modifier
        .contains(ratatui::style::Modifier::UNDERLINED));
}

fn inspection(name: &str, user: &str, status: PingStatus) -> runtime::effective::Inspection {
    runtime::effective::Inspection {
        name: name.into(),
        effective: Ok(runtime::effective::EffectiveConfig::parse(&format!(
            "hostname {name}.resolved\nuser {user}\nport 22\n"
        ))
        .unwrap()),
        ping: Some(PingResult {
            method: crate::storage::AvailabilityMethod::Ssh,
            name: name.into(),
            status,
            duration: Duration::ZERO,
            error: None,
        }),
    }
}

#[test]
fn advanced_search_async_membership_preserves_identity_and_unchanged_recent_order() {
    let mut m = model();
    m.query.set("user:inherited is:online");
    m.filter(true);
    assert!(m.visible.is_empty());
    m.apply_inspection(inspection("beta", "inherited", PingStatus::Online));
    assert_eq!(m.selected_host().unwrap().name, "beta");
    m.apply_inspection(inspection("alpha", "inherited", PingStatus::Online));
    assert_eq!(m.selected_host().unwrap().name, "beta");
    m.offset = 1;
    m.recent = true;
    // Simulate the history update after SSH without applying a new sort.
    m.visible = vec![1, 0];
    m.selected = 1;
    m.apply_inspection(inspection("alpha", "inherited", PingStatus::Online));
    assert_eq!(m.visible, vec![1, 0]);
    assert_eq!((m.selected, m.offset), (1, 1));
    m.apply_inspection(inspection("beta", "inherited", PingStatus::Offline));
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!((m.selected, m.offset), (0, 0));
    m.apply_inspection(inspection("alpha", "changed", PingStatus::Online));
    assert!(m.visible.is_empty());
}

#[test]
fn advanced_search_preferred_edit_drops_stale_effective_cache_before_matching() {
    let mut m = model();
    m.apply_inspection(inspection("alpha", "old-user", PingStatus::Unknown));
    m.query.set("user:new-user");
    m.filter(true);
    let mut saved = m.all[0].clone();
    saved.user = "new-user".into();
    let mut hosts = m.all.clone();
    hosts[0] = saved.clone();
    m.replace_hosts(hosts, Some(saved));
    assert_eq!(m.query.value, "user:new-user");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
}

#[test]
fn advanced_search_probe_reset_reevaluates_connecting_and_unknown_filters() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    fs::write(&config, "Host alpha\n HostName 127.0.0.1\n Port 1\n").unwrap();
    let mut m = model();
    m.all.truncate(1);
    m.query.set("is:connecting");
    m.filter(true);
    let inspector = runtime::effective::Inspector::default();
    drop(start_inspection(&inspector, &mut m, &config, true));
    assert_eq!(m.visible.len(), 1);
    drop(start_inspection(&inspector, &mut m, &config, false));
    assert!(m.visible.is_empty());
    m.query.set("is:unknown");
    m.filter(true);
    assert_eq!(m.visible.len(), 1);
}

#[test]
fn advanced_search_paste_is_text_only_and_preserves_note_lines() {
    let mut m = model();
    m.paste("tag:work\r\nAND user:deploy\n");
    assert_eq!(m.query.value, "tag:work AND user:deploy ");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.search_focus = false;
    m.paste("qae\n");
    assert!(matches!(m.view, View::List));
    assert_eq!(m.query.value, "tag:work AND user:deploy ");
    m.open_view(View::Note(notes::Editor::new("alpha".into(), "")));
    m.paste("qae\r\nline two\n\u{13}\u{1b}");
    let View::Note(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.input.value, "qae\nline two\n");
    m.discard = true;
    m.paste("\nignored");
    let View::Note(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.input.value, "qae\nline two\n");
    m.discard = false;
    m.view = View::Host(Box::new(HostForm::add(
        String::new(),
        PathBuf::from("unused"),
    )));
    m.paste("server\nuser\tq");
    let View::Host(form) = &m.view else { panic!() };
    assert_eq!(form.inputs[0].value, "server user q");
    assert_eq!(form.focus, 0);
    m.view = View::Actions(palette::State::new(palette::Scope::List));
    m.paste("\nq");
    assert!(matches!(
        &m.view,
        View::Actions(state)
            if state.query.value.is_empty() && state.selected == Some(PaletteAction::Connect)
    ));
}

#[test]
fn advanced_search_details_highlight_user_file_and_multiline_notes() {
    let mut m = model();
    m.all[0].source_file = PathBuf::from("team/config");
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            favorite: false,
            note: "Release\ncafé".into(),
        },
    );
    m.query.set("user:deploy file:team café");
    m.filter(true);
    m.view = View::Info(m.selected_host().unwrap());
    let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    let highlighted: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.modifier.contains(ratatui::style::Modifier::UNDERLINED))
        .map(|cell| cell.symbol())
        .collect();
    for expected in ["café", "deploy", "team"] {
        assert!(highlighted.contains(expected), "{highlighted}");
    }
}

#[test]
fn advanced_search_saved_unrelated_edit_preserves_native_inherited_query_until_resolved() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config");
    fs::write(&path, "Host *\n User inherited\n HostName inherited.example\nHost target\n HostName declared.example\n").unwrap();
    let mut m = Model::new(
        config::load_hosts(&path).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let inspector = runtime::effective::Inspector::default();
    for result in start_inspection(&inspector, &mut m, &path, false) {
        m.apply_inspection(result);
    }
    let query = "user:inherited AND inherited.example";
    m.query.set(query);
    m.filter(true);
    let original = m.selected_host().unwrap();
    assert!(original.user.is_empty());
    let mut saved = original.clone();
    saved.tags.push("new-tag".into());
    config::update_host(&original, &saved, &temp.path().join("app")).unwrap();
    m.complete_mutation(
        config::load_hosts(&path).unwrap(),
        &Action::Save(Some(original), Box::new(saved), path.clone()),
    );
    assert_eq!(
        m.query.value, query,
        "unrelated edit cannot erase an inherited filter"
    );
    assert_eq!(m.selected_host().unwrap().name, "target");
    let receiver = start_inspection(&inspector, &mut m, &path, false);
    assert_eq!(m.query.value, query);
    assert_eq!(
        m.selected_host().unwrap().name,
        "target",
        "keep saved selection while pending"
    );
    assert!(
        m.effective.is_empty(),
        "never expose stale values as freshly resolved"
    );
    for result in receiver {
        m.apply_inspection(result);
    }
    assert_eq!(m.query.value, query);
    assert_eq!(m.selected_host().unwrap().name, "target");
    assert_eq!(m.effective["target"].as_ref().unwrap().user, "inherited");
}

#[test]
fn advanced_search_saved_move_settles_against_fresh_native_inheritance() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let source = temp.path().join("first");
    let target = temp.path().join("last");
    fs::write(
        &source,
        "Host target\n User old-user\n HostName old.example\n",
    )
    .unwrap();
    fs::write(&target, "").unwrap();
    fs::write(
        &root,
        format!(
            "Include {}\nHost *\n User middle-user\n HostName middle.example\nInclude {}\n",
            source.display(),
            target.display()
        ),
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let inspector = runtime::effective::Inspector::default();
    for result in start_inspection(&inspector, &mut m, &root, false) {
        m.apply_inspection(result);
    }
    m.query.set("user:old-user AND old.example");
    m.filter(true);
    let original = m.selected_host().unwrap();
    config::move_host(&original, &target, &temp.path().join("app")).unwrap();
    m.complete_mutation(
        config::load_hosts(&root).unwrap(),
        &Action::MoveTo(original, target.clone()),
    );
    assert_eq!(m.query.value, "user:old-user AND old.example");
    let receiver = start_inspection(&inspector, &mut m, &root, false);
    assert_eq!(m.selected_host().unwrap().source_file, target);
    for result in receiver {
        m.apply_inspection(result);
    }
    assert_eq!(m.effective["target"].as_ref().unwrap().user, "middle-user");
    assert!(
        m.query.value.is_empty(),
        "fresh incompatibility must reveal the saved host"
    );
    assert_eq!(m.selected_host().unwrap().source_file, target);
}

#[test]
fn advanced_search_saved_explicit_target_changes_wait_for_fresh_incompatibility() {
    let mut m = model();
    m.apply_inspection(inspection("alpha", "old-user", PingStatus::Unknown));
    m.query.set("user:old-user AND alpha.resolved");
    m.filter(true);
    let original = m.selected_host().unwrap();
    let mut saved = original.clone();
    saved.user = "new-user".into();
    saved.hostname = "new.example".into();
    let mut hosts = m.all.clone();
    hosts[0] = saved.clone();
    m.complete_mutation(
        hosts,
        &Action::Save(Some(original), Box::new(saved), PathBuf::new()),
    );
    assert_eq!(m.query.value, "user:old-user AND alpha.resolved");
    assert!(m.pending_preferred.is_some());
    assert_eq!(m.selected_host().unwrap().user, "new-user");
    assert_eq!(m.selected_host().unwrap().hostname, "new.example");
    assert!(!m.effective.contains_key("alpha"));
    let mut fresh = inspection("alpha", "new-user", PingStatus::Unknown);
    fresh.effective.as_mut().unwrap().hostname = "new.example".into();
    m.apply_inspection(fresh);
    assert!(m.query.value.is_empty());
    assert!(m.pending_preferred.is_none());
    assert_eq!(m.selected_host().unwrap().name, "alpha");
}

#[test]
fn advanced_search_saved_pending_resolution_cannot_clear_new_query_or_resort_unchanged_list() {
    let mut m = model();
    m.apply_inspection(inspection("alpha", "inherited", PingStatus::Unknown));
    m.query.set("user:inherited");
    m.filter(true);
    let mut saved = m.all[0].clone();
    saved.tags.push("edited".into());
    let mut hosts = m.all.clone();
    hosts[0] = saved.clone();
    m.replace_hosts(hosts, Some(saved));
    assert!(m.pending_preferred.is_some());
    m.query.set("beta");
    m.filter(true);
    m.apply_inspection(inspection("alpha", "changed", PingStatus::Unknown));
    assert_eq!(m.query.value, "beta");
    assert_eq!(m.selected_host().unwrap().name, "beta");
    m.query.set("example");
    m.filter(true);
    m.effective.clear();
    let saved = m.all[0].clone();
    m.replace_hosts(m.all.clone(), Some(saved));
    m.visible = vec![1, 0];
    m.selected = 1;
    m.offset = 1;
    m.recent = true;
    let mut fresh = inspection("alpha", "deploy", PingStatus::Unknown);
    fresh.effective.as_mut().unwrap().hostname = "a.example".into();
    m.apply_inspection(fresh);
    assert!(m.pending_preferred.is_none());
    assert_eq!(m.visible, vec![1, 0]);
    assert_eq!((m.selected, m.offset), (1, 1));
    assert_eq!(m.query.value, "example");
}

#[test]
fn advanced_search_saved_pending_resolution_error_settles_with_declared_fallback() {
    let mut m = model();
    m.apply_inspection(inspection("alpha", "inherited", PingStatus::Unknown));
    m.query.set("user:inherited");
    m.filter(true);
    let mut saved = m.all[0].clone();
    saved.tags.push("edited".into());
    let mut hosts = m.all.clone();
    hosts[0] = saved.clone();
    m.replace_hosts(hosts, Some(saved));
    m.apply_inspection(runtime::effective::Inspection {
        name: "alpha".into(),
        effective: Err("resolver failed".into()),
        ping: None,
    });
    assert!(m.pending_preferred.is_none());
    assert!(m.query.value.is_empty());
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert!(m.effective["alpha"].is_err());
}

#[cfg(unix)]
#[test]
fn advanced_search_saved_pending_reload_cancels_preference_and_excludes_old_generation() {
    for removed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let ready = temp.path().join("ready");
        let release = temp.path().join("release");
        let finished = temp.path().join("finished");
        fs::write(&root, format!(
            "Host fixture\n HostName old.example\n User old-user\nMatch exec \"touch {}; while test ! -e {}; do sleep 0.02; done; touch {}\"\n",
            ready.display(), release.display(), finished.display()
        )).unwrap();
        let mut m = Model::new(
            config::load_hosts(&root).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        m.query.set("user:old-user");
        m.filter(true);
        let inspector = runtime::effective::Inspector::default();
        let mut receiver = start_inspection(&inspector, &mut m, &root, false);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !ready.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "old native generation must reach barrier"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(receiver.try_recv().is_err());
        let original = m.selected_host().unwrap();
        let mut saved = original.clone();
        saved.tags.push("edited".into());
        config::update_host(&original, &saved, &temp.path().join("app")).unwrap();
        m.complete_mutation(
            config::load_hosts(&root).unwrap(),
            &Action::Save(Some(original), Box::new(saved), root.clone()),
        );
        assert!(m.pending_preferred.is_some());
        receiver = start_inspection(&inspector, &mut m, &root, false);
        assert_eq!(m.query.value, "user:old-user");
        assert!(receiver.try_recv().is_err());
        let fresh_name = if removed { "replacement" } else { "fixture" };
        fs::write(
            &root,
            format!("Host {fresh_name}\n HostName new.example\n User new-user\n"),
        )
        .unwrap();
        receiver = reload_hosts(&inspector, &mut m, &root).unwrap();
        assert!(
            m.pending_preferred.is_none(),
            "F5 cancels the old save preference, including a missing alias"
        );
        assert_eq!(
            m.query.value, "user:old-user",
            "reload must not clear a user's query"
        );
        m.query.set("user:new-user");
        m.filter(true);
        m.apply_inspection(receiver.recv_timeout(Duration::from_secs(3)).unwrap());
        fs::write(&release, "").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !finished.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "old generation must finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        for result in receiver {
            m.apply_inspection(result);
        }
        assert_eq!(m.query.value, "user:new-user");
        assert_eq!(m.selected_host().unwrap().name, fresh_name);
        assert_eq!(m.effective[fresh_name].as_ref().unwrap().user, "new-user");
        if removed {
            assert!(!m.effective.contains_key("fixture"));
        }
    }
}

#[test]
fn advanced_search_two_real_saves_keep_inherited_intent_until_latest_resolution() {
    for (change_user, change_hostname) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let direct_second_edit = change_user || change_hostname;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(
            &path,
            "Host target\nHost *\n User inherited\n HostName inherited.example\n",
        )
        .unwrap();
        let mut m = Model::new(
            config::load_hosts(&path).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        let inspector = runtime::effective::Inspector::default();
        let mut receiver = start_inspection(&inspector, &mut m, &path, false);
        for result in &receiver {
            m.apply_inspection(result);
        }
        let query = "user:inherited AND inherited.example";
        m.query.set(query);
        m.filter(true);
        for edit in 0..2 {
            let original = m.selected_host().unwrap();
            let mut saved = original.clone();
            saved.tags.push(format!("edit-{edit}"));
            if edit == 1 && change_user {
                saved.user = "new-user".into();
            }
            if edit == 1 && change_hostname {
                saved.hostname = "new.example".into();
            }
            config::update_host(&original, &saved, &temp.path().join("app")).unwrap();
            m.complete_mutation(
                config::load_hosts(&path).unwrap(),
                &Action::Save(Some(original), Box::new(saved), path.clone()),
            );
            let expected = query;
            assert_eq!(
                m.query.value, expected,
                "edit {edit}, direct={direct_second_edit}"
            );
            assert_eq!(m.selected_host().unwrap().name, "target");
            // Replace the receiver without delivering any result from the first save.
            receiver = start_inspection(&inspector, &mut m, &path, false);
            assert!(
                m.effective.is_empty(),
                "compatibility snapshot must never become resolved cache"
            );
            assert_eq!(m.query.value, expected);
            assert_eq!(m.selected_host().unwrap().name, "target");
        }
        for result in receiver {
            m.apply_inspection(result);
        }
        assert!(m.pending_preferred.is_none());
        let effective = m.effective["target"].as_ref().unwrap();
        if direct_second_edit {
            assert!(m.query.value.is_empty());
            assert_eq!(
                effective.user,
                if change_user { "new-user" } else { "inherited" }
            );
            assert_eq!(
                effective.hostname,
                if change_hostname {
                    "new.example"
                } else {
                    "inherited.example"
                }
            );
        } else {
            assert_eq!(m.query.value, query);
            assert_eq!(effective.user, "inherited");
            assert_eq!(effective.hostname, "inherited.example");
        }
        assert_eq!(m.selected_host().unwrap().tags, ["edit-0", "edit-1"]);
    }
}

#[test]
fn advanced_search_repeated_save_snapshot_is_cancelled_by_typing_or_reload() {
    for reload in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        fs::write(
            &path,
            "Host target\nHost *\n User inherited\n HostName inherited.example\n",
        )
        .unwrap();
        let mut m = Model::new(
            config::load_hosts(&path).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        let inspector = runtime::effective::Inspector::default();
        let mut receiver = start_inspection(&inspector, &mut m, &path, false);
        for result in &receiver {
            m.apply_inspection(result);
        }
        let original_query = "user:inherited AND inherited.example";
        m.query.set(original_query);
        m.filter(true);
        for edit in 0..2 {
            let original = m.selected_host().unwrap();
            let mut saved = original.clone();
            saved.tags.push(format!("edit-{edit}"));
            config::update_host(&original, &saved, &temp.path().join("app")).unwrap();
            m.complete_mutation(
                config::load_hosts(&path).unwrap(),
                &Action::Save(Some(original), Box::new(saved), path.clone()),
            );
            receiver = start_inspection(&inspector, &mut m, &path, false);
        }
        assert!(m.pending_preferred.as_ref().unwrap().provisional.is_some());
        if reload {
            fs::write(
                &path,
                "Host replacement\n User changed\n HostName changed.example\n",
            )
            .unwrap();
            receiver = reload_hosts(&inspector, &mut m, &path).unwrap();
            assert!(m.pending_preferred.is_none());
            assert_eq!(m.query.value, original_query);
            for result in receiver {
                m.apply_inspection(result);
            }
            assert_eq!(m.query.value, original_query);
            assert!(!m.effective.contains_key("target"));
        } else {
            m.query.set("user:other");
            m.filter(true);
            assert!(m.pending_preferred.is_none());
            for result in receiver {
                m.apply_inspection(result);
            }
            assert_eq!(m.query.value, "user:other");
        }
        assert!(m.pending_preferred.is_none());
        assert!(m.visible.is_empty());
    }
}

#[test]
fn duplication_shortcut_and_palette_offer_selected_host() {
    let mut m = model();
    m.search_focus = false;
    m.apply_inspection(inspection("alpha", "deploy", PingStatus::Unknown));
    assert_eq!(
        m.key(key(KeyCode::Char('D'))),
        Action::Duplicate(m.all[0].clone())
    );
    assert_eq!(PaletteAction::Duplicate.label(), "Dupliquer");
    assert_eq!(
        m.keymap().shortcut(PaletteAction::Duplicate).as_deref(),
        Some("D")
    );
    assert!(!PaletteAction::Duplicate.enabled(false));
    m.query.set("alpha");
    m.filter(true);
    m.search_focus = true;
    assert_eq!(
        palette(&mut m, "dupliquer"),
        Action::Duplicate(m.all[0].clone())
    );
    assert_eq!(m.query.value, "alpha");
    assert!(m.search_focus);
}

#[test]
fn duplication_preserves_native_effective_target_and_declared_fields_without_alias_sharing() {
    for source_in_include in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let included = temp.path().join("included");
        let keyfile = temp.path().join("key");
        fs::write(&keyfile, "test key").unwrap();
        let declaration = format!("# original comment\n# Tags: prod,work\nHost source secondary\n HostName source.example\n User declared\n Port 2255\n IdentityFile {}\n IdentityFile /missing/additional-key\n ProxyJump bastion\n ProxyCommand ssh -W %h:%p gateway\n RemoteCommand uptime\n RequestTTY force\n Compression yes\n ServerAliveInterval 13\n SendEnv FOO\n SendEnv BAR\n LocalCommand printf \"quoted space\"\n", keyfile.display());
        let defaults = "Host *\n User inherited-user\n Port 2207\n";
        fs::write(
            &root,
            format!(
                "{defaults}Include {}\n{}",
                included.display(),
                if source_in_include { "" } else { &declaration }
            ),
        )
        .unwrap();
        fs::write(&included, if source_in_include { declaration.as_str() } else { "# destination\nHost hidden-collision alias-collision\n HostName hidden.example\n# Tags: hidden\nHost secret-collision\n HostName secret.example\n" }).unwrap();
        let original_root = fs::read(&root).unwrap();
        let original_include = fs::read(&included).unwrap();
        let source = config::find_host(&root, "secondary").unwrap();
        let effective = runtime::effective::resolve(&root, "secondary").unwrap();
        assert_eq!(effective.user, "inherited-user");
        assert_eq!(effective.port, "2207");
        let mut m = Model::new(
            config::load_hosts(&root).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        let mut metadata = Metadata::load(&temp.path().join("app"), &root).unwrap();
        metadata.set_favorite("secondary", true).unwrap();
        metadata
            .set_note("secondary", "personal source note")
            .unwrap();
        m.metadata = metadata.hosts().clone();
        m.effective
            .insert(source.name.clone(), Ok(effective.clone()));
        select_duplicate_files(&mut m, &root, source.clone()).unwrap();
        let View::Files {
            files,
            selected,
            duplicate,
            ..
        } = &mut m.view
        else {
            panic!("expected destination picker")
        };
        assert!(duplicate.is_some());
        assert!(
            files.contains(&source.source_file),
            "same-file copy must be offered"
        );
        let target = if source_in_include {
            root.clone()
        } else {
            included.clone()
        };
        *selected = files.iter().position(|path| path == &target).unwrap();
        m.key(key(KeyCode::Enter));
        let View::Host(form) = &mut m.view else {
            panic!("expected prefilled add form")
        };
        assert!(form.original.is_none());
        assert!(form.names.is_empty());
        assert!(!form.dirty());
        assert!(!form.validate(), "new name is mandatory");
        for invalid in [
            "source",
            "secondary",
            "SOURCE",
            "SECONDARY",
            "*",
            "!other",
            "wild?",
        ] {
            form.inputs[0].set(invalid);
            assert!(!form.validate(), "must reject {invalid}");
        }
        form.inputs[0].set("new-copy");
        assert!(form.validate(), "{} {:?}", form.error, form.errors);
        let copy = form.host().unwrap();
        assert_eq!(copy.hostname, effective.hostname);
        assert_eq!(copy.user, effective.user);
        assert_eq!(copy.port, effective.port);
        assert_eq!(copy.identity, source.identity);
        assert_eq!(copy.extra_identities, source.extra_identities);
        assert_eq!(copy.proxy_jump, source.proxy_jump);
        assert_eq!(copy.proxy_command, source.proxy_command);
        assert_eq!(copy.remote_command, source.remote_command);
        assert_eq!(copy.request_tty, source.request_tty);
        assert_eq!(copy.options, source.options);
        assert_eq!(copy.tags, source.tags);
        assert!(copy.aliases.is_empty());
        assert!(
            screen(&mut m, 120, 40).contains("/missing/additional-key"),
            "copy form must display preserved extra identities"
        );
        let action = m.key(ctrl('s'));
        assert!(matches!(&action, Action::SaveCopy(..)));
        save_copy(&root, &copy, &target, &temp.path().join("app")).unwrap();
        let mut resolved_copy = runtime::effective::resolve(&root, "new-copy").unwrap();
        // ssh -G reports the requested alias separately from its effective target.
        resolved_copy
            .directives
            .insert("host".into(), vec!["secondary".into()]);
        assert_eq!(
            resolved_copy, effective,
            "native ssh -G must retain all represented source values"
        );
        assert_eq!(config::find_host(&root, "secondary").unwrap(), source);
        assert_eq!(
            config::find_host(&root, "source").unwrap().aliases,
            ["source", "secondary"]
        );
        if source_in_include {
            assert_eq!(fs::read(&included).unwrap(), original_include);
            assert!(fs::read(&root).unwrap().ends_with(&original_root));
        } else {
            assert_eq!(fs::read(&root).unwrap(), original_root);
            assert!(fs::read(&included).unwrap().ends_with(&original_include));
        }
        complete_saved_mutation(
            &mut m,
            config::load_hosts(&root).unwrap(),
            &action,
            &mut metadata,
        );
        assert!(m.is_favorite("secondary"));
        assert_eq!(m.note("secondary"), "personal source note");
        assert_eq!(m.selected_host().unwrap().name, "new-copy");
        assert!(!m.is_favorite("new-copy"));
        assert_eq!(m.note("new-copy"), "");
        // Also allow a copy into its own original file.
        let mut same = copy.clone();
        same.name = "same-file-copy".into();
        save_copy(&root, &same, &source.source_file, &temp.path().join("app")).unwrap();
        for name in ["source", "secondary", "new-copy", "same-file-copy"]
            .into_iter()
            .chain(if source_in_include {
                vec![]
            } else {
                vec![
                    "alias-collision",
                    "secret-collision",
                    "ALIAS-COLLISION",
                    "SECRET-COLLISION",
                ]
            })
        {
            let mut collision = copy.clone();
            collision.name = name.into();
            let before = fs::read(&target).unwrap();
            assert!(
                save_copy(&root, &collision, &target, &temp.path().join("app"))
                    .unwrap_err()
                    .to_string()
                    .contains("already exists")
            );
            assert_eq!(fs::read(&target).unwrap(), before);
        }
    }
}

#[test]
fn duplication_implicit_keys_stay_unset_and_resolution_errors_are_responsive() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    fs::write(&root, "Host original secondary\n").unwrap();
    let source = config::find_host(&root, "secondary").unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    assert!(select_duplicate_files(&mut m, &root, source.clone())
        .unwrap_err()
        .to_string()
        .contains("pending"));
    assert!(matches!(m.view, View::List));
    m.effective
        .insert("secondary".into(), Err("fake failure".into()));
    assert!(select_duplicate_files(&mut m, &root, source.clone())
        .unwrap_err()
        .to_string()
        .contains("fake failure"));
    assert!(matches!(m.view, View::List));
    let effective = runtime::effective::resolve(&root, "secondary").unwrap();
    assert!(!effective.identity_files.is_empty());
    m.effective.insert("secondary".into(), Ok(effective));
    select_duplicate_files(&mut m, &root, source).unwrap();
    m.key(key(KeyCode::Enter));
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.inputs[0].set("new");
    assert_eq!(form.inputs[1].value, "secondary");
    assert!(form.inputs[4].value.is_empty());
    assert!(form.host().unwrap().extra_identities.is_empty());
    assert!(form.validate());
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(key(KeyCode::Esc));
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    assert_eq!(form.inputs[0].value, "new");
    discard(&mut m);
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    assert_eq!(
        fs::read_to_string(&root).unwrap(),
        "Host original secondary\n"
    );
}

#[test]
fn duplication_before_trailing_fallback_retains_explicit_port_22_and_old_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let original = "# Tags: original-tag\n# attached original comment\nHost source secondary\n HostName source.example\n User specific\n Port 22\n Compression yes\nHost other\n HostName other.example\n User other-user\n Port 2244\nHost *\n User fallback\n Port 2207\n";
    fs::write(&root, original).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let before_source = runtime::effective::resolve(&root, "secondary").unwrap();
    let before_other = runtime::effective::resolve(&root, "other").unwrap();
    let source = config::find_host(&root, "secondary").unwrap();
    let mut copy = source.clone();
    copy.name = "fresh-copy".into();
    copy.aliases.clear();
    copy.line_number = 0;
    save_copy(&root, &copy, &root, &temp.path().join("app")).unwrap();
    assert!(fs::read_to_string(&root).unwrap().ends_with(original));
    assert_eq!(
        fs::read_to_string(temp.path().join("app/backups/config.backup")).unwrap(),
        original
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    let after = runtime::effective::resolve(&root, "fresh-copy").unwrap();
    assert_eq!(
        (&after.hostname, &after.user, &after.port),
        (
            &before_source.hostname,
            &before_source.user,
            &before_source.port
        )
    );
    assert_eq!(
        runtime::effective::resolve(&root, "secondary").unwrap(),
        before_source
    );
    assert_eq!(
        runtime::effective::resolve(&root, "other").unwrap(),
        before_other
    );
    let reloaded = config::find_host(&root, "secondary").unwrap();
    assert_eq!(reloaded.tags, source.tags);
    assert_eq!(reloaded.aliases, source.aliases);
    assert_eq!(reloaded.options, source.options);
    assert!(fs::read_to_string(&root)
        .unwrap()
        .contains("# Tags: original-tag\n# attached original comment\nHost source secondary"));
}

#[test]
fn duplication_rejects_earlier_include_or_global_fallback_without_writes_or_backups() {
    for copy_to_root in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let included = temp.path().join("included");
        let declaration =
            "Host source secondary\n HostName source.example\n User specific\n Port 22\n";
        if copy_to_root {
            // A preceding Include matches the new name but does not match the source.
            let earlier = temp.path().join("earlier");
            fs::write(&earlier, "Host new-*\n User fallback\n Port 2207\n").unwrap();
            fs::write(
                &root,
                format!(
                    "Include {}\nInclude {}\n",
                    earlier.display(),
                    included.display()
                ),
            )
            .unwrap();
            fs::write(&included, declaration).unwrap();
        } else {
            // The root source precedes a fallback which wins before the target Include.
            fs::write(
                &root,
                format!(
                    "{declaration}Host *\n User fallback\n Port 2207\n Include {}\n",
                    included.display()
                ),
            )
            .unwrap();
            fs::write(
                &included,
                "# destination\nHost other\n HostName other.example\n",
            )
            .unwrap();
        }
        let native_source = runtime::effective::resolve(&root, "secondary").unwrap();
        assert_eq!(native_source.user, "specific");
        assert_eq!(native_source.port, "22");
        let root_before = fs::read(&root).unwrap();
        let include_before = fs::read(&included).unwrap();
        let mut copy = config::find_host(&root, "secondary").unwrap();
        copy.name = "new-copy".into();
        copy.aliases.clear();
        copy.user = "specific".into();
        copy.port = "22".into();
        let target = if copy_to_root { &root } else { &included };
        let app = temp.path().join("app");
        let result = save_copy(&root, &copy, target, &app);
        assert!(
            result.is_err(),
            "incompatible destination must reject rather than silently change the copy target"
        );
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("destination") && error.contains("choose"),
            "{error}"
        );
        assert_eq!(fs::read(&root).unwrap(), root_before);
        assert_eq!(fs::read(&included).unwrap(), include_before);
        assert!(!app.join("backups").exists());
    }
}

#[test]
fn save_and_enter_never_launch_but_ctrl_l_does() {
    let mut m = model();
    m.activate_action(PaletteAction::Tunnel);
    let View::Forward(form) = &mut m.view else {
        panic!()
    };
    form.inputs[1].set("8080");
    form.inputs[3].set("80");
    form.focus = 4;
    assert!(
        !matches!(m.key(ctrl('s')), Action::Connect(..)),
        "Save must not launch SSH"
    );
    assert!(
        !matches!(m.key(key(KeyCode::Enter)), Action::Connect(..)),
        "Enter must not launch SSH"
    );
    assert!(
        matches!(m.key(ctrl('l')), Action::Connect(_, Some(_))),
        "Ctrl+L explicitly launches"
    );
}

#[test]
fn f6_opens_profiles_from_focused_search() {
    let mut m = model();
    m.key(key(KeyCode::F(6)));
    assert!(screen(&mut m, 100, 30).contains("Tunnel profiles"));
}

fn sample_forward(kind: &str) -> Forward {
    Forward {
        kind: kind.into(),
        local_port: "8080".into(),
        remote_host: "::1".into(),
        remote_port: "80".into(),
        bind_address: "127.0.0.1".into(),
    }
}

#[test]
fn save_persists_without_connect_and_launches_each_type_without_saving() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let mut store = TunnelProfiles::load(dir.path(), &config).unwrap();
    let mut m = model();
    m.key(key(KeyCode::F(6)));
    m.key(key(KeyCode::Char('a')));
    for (kind, flag) in [("local", "-L"), ("remote", "-R"), ("dynamic", "-D")] {
        m.view = View::Forward(ForwardForm::new("alpha".into(), Some(sample_forward(kind))));
        let View::Forward(form) = &mut m.view else {
            panic!()
        };
        form.inputs[5].set(kind);
        let save = m.key(ctrl('s'));
        assert!(matches!(save, Action::SaveProfile { .. }));
        profiles::apply(&mut m, &mut store, &save).unwrap();
        let View::Forward(form) = &mut m.view else {
            panic!()
        };
        assert!(!form.dirty());
        assert_eq!(form.original_name.as_deref(), Some(kind));
        form.inputs[1].set("9090");
        form.inputs[5].set(""); // An ad hoc launch does not require a profile name.
        let Action::Connect(host, Some(forward)) = m.key(ctrl('l')) else {
            panic!()
        };
        assert_eq!(host, "alpha");
        assert_eq!(forward.ssh_args().unwrap()[0], flag);
        assert!(forward.ssh_args().unwrap()[1].contains("9090"));
        assert_eq!(store.hosts()["alpha"][kind].local_port, "8080");
    }
    assert_eq!(
        TunnelProfiles::load(dir.path(), &config).unwrap().hosts()["alpha"].len(),
        3
    );
    assert!(!dir.path().join("history.json").exists());
}

#[test]
fn profile_crud_confirmations_dirty_paste_and_failed_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TunnelProfiles::load(dir.path(), &dir.path().join("config")).unwrap();
    store
        .save("alpha", None, "first", &sample_forward("local"))
        .unwrap();
    store
        .save("alpha", None, "taken", &sample_forward("dynamic"))
        .unwrap();
    let mut m = model();
    m.tunnel_profiles = store.hosts().clone();
    m.key(key(KeyCode::F(6)));
    m.key(key(KeyCode::Enter));
    let View::Forward(form) = &m.view else {
        panic!()
    };
    assert_eq!(form.focus, 5);
    assert!(!form.dirty());
    m.key(ctrl('u'));
    m.paste("réseau\r\nname\u{1b}");
    let View::Forward(form) = &m.view else {
        panic!()
    };
    assert_eq!(form.inputs[5].value, "réseau name");
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(key(KeyCode::Esc));
    assert!(!m.discard);
    let View::Forward(form) = &mut m.view else {
        panic!()
    };
    form.inputs[5].set("taken");
    let action = m.key(ctrl('s'));
    assert!(profiles::apply(&mut m, &mut store, &action).is_err());
    assert!(matches!(&m.view, View::Forward(form) if form.dirty()));
    assert!(store.hosts()["alpha"].contains_key("first"));
    let View::Forward(form) = &mut m.view else {
        panic!()
    };
    form.inputs[5].set("renamed");
    let action = m.key(ctrl('s'));
    profiles::apply(&mut m, &mut store, &action).unwrap();
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Profiles(_)));
    m.key(key(KeyCode::Char('d')));
    assert!(screen(&mut m, 100, 30).contains("Delete profile 'renamed'?"));
    assert_eq!(m.key(ctrl('l')), Action::None);
    m.key(key(KeyCode::Esc));
    assert!(store.hosts()["alpha"].contains_key("renamed"));
    m.key(key(KeyCode::Char('d')));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(
        action,
        Action::DeleteProfile("alpha".into(), "renamed".into())
    );
    profiles::apply(&mut m, &mut store, &action).unwrap();
    assert!(!store.hosts()["alpha"].contains_key("renamed"));
    m.key(key(KeyCode::Char('a')));
    m.paste("cancelled");
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
    m.key(key(KeyCode::Enter));
    assert_eq!(store.hosts()["alpha"].len(), 1);
}

#[test]
fn name_validation_caret_and_table_selection_are_visible() {
    use ratatui::{backend::Backend, style::Color};
    for (width, height) in [(100, 32), (30, 12), (4, 4)] {
        let mut m = model();
        let mut form = ForwardForm::new("alpha".into(), Some(sample_forward("local")));
        assert!(!form.validate_save());
        assert_eq!(form.focus, 5);
        assert_eq!(form.errors[0].field, forms::FormField::Input(5));
        form.inputs[5].set("界profil");
        form.inputs[5].cursor = 0;
        m.view = View::Forward(form);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
        let position = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(terminal.backend().buffer()[position].symbol(), "界");
    }
    let mut m = model();
    m.tunnel_profiles.insert(
        "alpha".into(),
        [("selected".into(), sample_forward("local"))].into(),
    );
    m.key(key(KeyCode::F(6)));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let cells: Vec<_> = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.bg == Color::Rgb(0, 173, 216))
        .collect();
    assert!(!cells.is_empty());
    assert!(cells.iter().all(|cell| cell.fg == Color::Black));
    assert!(matches!(m.key(ctrl('l')), Action::Connect(_, Some(_))));
    assert!(matches!(
        PaletteAction::CATALOGUE
            .iter()
            .find(|action| action.id() == "tunnel.profiles"),
        Some(PaletteAction::Profiles)
    ));
}

#[test]
fn alias_rename_storage_failure_refreshes_committed_host_with_warning() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let mut metadata = Metadata::load(dir.path(), &config).unwrap();
    let mut profiles = TunnelProfiles::load(dir.path(), &config).unwrap();
    profiles
        .save("alpha", None, "keep", &sample_forward("local"))
        .unwrap();
    profiles
        .save("renamed", None, "occupied", &sample_forward("dynamic"))
        .unwrap();
    metadata.set_note("alpha", "retained").unwrap();
    let mut m = model();
    let old = m.all[0].clone();
    let new = Host {
        name: "renamed".into(),
        ..old.clone()
    };
    m.open_view(View::Host(Box::new(HostForm::edit(old.clone()))));
    profiles::complete_saved(
        &mut m,
        vec![new.clone()],
        &Action::Save(Some(old), Box::new(new), config),
        &mut metadata,
        &mut profiles,
    );
    assert!(matches!(m.view, View::List));
    assert_eq!(m.selected_host().unwrap().name, "renamed");
    assert!(m.error.contains("SSH saved; tunnel profile rename failed"));
    assert!(m.error.contains("occupied"));
    assert_eq!(m.note("renamed"), "retained");
    assert!(m.tunnel_profiles["alpha"].contains_key("keep"));
    assert!(m.tunnel_profiles["renamed"].contains_key("occupied"));
}

#[cfg(unix)]
#[test]
fn reload_symlink_rebinds_both_scopes_and_stages_before_publishing() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let link = dir.path().join("config");
    fs::write(&a, "Host alpha\n HostName a.example\n").unwrap();
    fs::write(&b, "Host alpha\n HostName b.example\n").unwrap();
    symlink(&a, &link).unwrap();
    let mut metadata = Metadata::load(dir.path(), &link).unwrap();
    let mut profiles = TunnelProfiles::load(dir.path(), &link).unwrap();
    metadata.set_note("alpha", "note A").unwrap();
    profiles
        .save("alpha", None, "A", &sample_forward("local"))
        .unwrap();
    Metadata::load(dir.path(), &b)
        .unwrap()
        .set_note("alpha", "note B")
        .unwrap();
    TunnelProfiles::load(dir.path(), &b)
        .unwrap()
        .save("alpha", None, "B", &sample_forward("dynamic"))
        .unwrap();
    let mut m = model();
    m.metadata = metadata.hosts().clone();
    m.tunnel_profiles = profiles.hosts().clone();
    fs::remove_file(&link).unwrap();
    symlink(&b, &link).unwrap();
    let inspector = runtime::effective::Inspector::default();
    drop(
        profiles::reload(
            &inspector,
            &mut m,
            &link,
            dir.path(),
            &mut metadata,
            &mut profiles,
        )
        .unwrap(),
    );
    assert_eq!(m.all[0].hostname, "b.example");
    assert_eq!(m.note("alpha"), "note B");
    assert!(m.tunnel_profiles["alpha"].contains_key("B"));
    profiles
        .save("alpha", None, "saved B", &sample_forward("remote"))
        .unwrap();
    assert!(
        !TunnelProfiles::load(dir.path(), &a).unwrap().hosts()["alpha"].contains_key("saved B")
    );
    // A store read failure after changing the symlink leaves every published state on B.
    fs::remove_file(&link).unwrap();
    symlink(&a, &link).unwrap();
    fs::remove_file(dir.path().join("tunnel_profiles.json")).unwrap();
    fs::create_dir(dir.path().join("tunnel_profiles.json")).unwrap();
    assert!(profiles::reload(
        &inspector,
        &mut m,
        &link,
        dir.path(),
        &mut metadata,
        &mut profiles
    )
    .is_err());
    assert_eq!(m.all[0].hostname, "b.example");
    assert_eq!(m.note("alpha"), "note B");
    assert!(m.tunnel_profiles["alpha"].contains_key("B"));
    metadata.set_note("alpha", "still B").unwrap();
    assert_eq!(
        Metadata::load(dir.path(), &b).unwrap().hosts()["alpha"].note,
        "still B"
    );
    assert_eq!(
        Metadata::load(dir.path(), &a).unwrap().hosts()["alpha"].note,
        "note A"
    );
}

#[test]
fn alias_rename_failed_app_writes_report_both_without_stale_host() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let mut metadata = Metadata::load(dir.path(), &config).unwrap();
    let mut profiles = TunnelProfiles::load(dir.path(), &config).unwrap();
    metadata.set_note("alpha", "recover note").unwrap();
    profiles
        .save("alpha", None, "recover profile", &sample_forward("local"))
        .unwrap();
    for name in ["metadata.json", "tunnel_profiles.json"] {
        fs::remove_file(dir.path().join(name)).unwrap();
        fs::create_dir(dir.path().join(name)).unwrap();
    }
    let mut m = model();
    let old = m.all[0].clone();
    let new = Host {
        name: "renamed".into(),
        ..old.clone()
    };
    m.open_view(View::Host(Box::new(HostForm::edit(old.clone()))));
    profiles::complete_saved(
        &mut m,
        vec![new.clone()],
        &Action::Save(Some(old), Box::new(new), config),
        &mut metadata,
        &mut profiles,
    );
    assert!(matches!(m.view, View::List));
    assert_eq!(m.selected_host().unwrap().name, "renamed");
    assert!(m.error.contains("SSH saved; metadata rename failed"));
    assert!(m.error.contains("SSH saved; tunnel profile rename failed"));
    assert_eq!(m.note("alpha"), "recover note");
    assert!(m.tunnel_profiles["alpha"].contains_key("recover profile"));
}

#[test]
fn f7_from_search_and_mode_choice_are_reactive() {
    let mut m = model();
    m.key(key(KeyCode::F(7)));
    assert!(screen(&mut m, 100, 30).contains("Owned tunnels"));
    m.key(key(KeyCode::Esc));
    m.view = View::Forward(ForwardForm::new(
        "alpha".into(),
        Some(Forward {
            kind: "dynamic".into(),
            local_port: "1080".into(),
            ..Forward::default()
        }),
    ));
    m.key(ctrl('t'));
    assert!(screen(&mut m, 100, 30).contains("Tunnel only"));
    m.key(key(KeyCode::Esc));
    assert!(m.discard);
}

#[test]
fn mode_save_launch_and_quit_cancel_preserve_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TunnelProfiles::load(dir.path(), &dir.path().join("config")).unwrap();
    let mut m = model();
    let mut form = ForwardForm::new(
        "alpha".into(),
        Some(Forward {
            kind: "dynamic".into(),
            local_port: "1080".into(),
            ..Forward::default()
        }),
    );
    form.inputs[5].set("sock");
    m.view = View::Forward(form);
    assert_eq!(m.key(ctrl('t')), Action::None);
    let save = m.key(ctrl('s'));
    assert!(matches!(
        save,
        Action::SaveProfile {
            mode: LaunchMode::TunnelOnly,
            ..
        }
    ));
    profiles::apply(&mut m, &mut store, &save).unwrap();
    assert!(matches!(&m.view, View::Forward(f) if !f.dirty()));
    assert!(matches!(m.key(ctrl('l')), Action::StartTunnel(_, Some(name), _) if name == "sock"));
    m.ask_quit();
    m.paste("should not edit");
    let text = screen(&mut m, 90, 30);
    assert!(
        text.contains("Quitter ssho ?") && text.contains("[▶ Revenir ]"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.quit_tunnels);
    assert!(matches!(&m.view, View::Forward(f) if !f.dirty()));
    m.ask_quit();
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::None,
        "Enter keeps the safe choice"
    );
    assert!(!m.quit_tunnels);
    m.ask_quit();
    m.key(key(KeyCode::Right));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Quit);
    m.quit_tunnels = false;
    m.view = View::Profiles(profiles::List::new("alpha".into()));
    assert!(matches!(m.key(ctrl('l')), Action::StartTunnel(_, _, _)));
    m.key(key(KeyCode::Enter));
    assert!(matches!(&m.view, View::Forward(f) if f.mode == LaunchMode::TunnelOnly && !f.dirty()));
}
#[test]
fn active_ids_survive_profile_reload_and_visible_high_contrast() {
    let mut m = model();
    m.tunnels.push(runtime::tunnels::Record {
        id: 91,
        snapshot: runtime::tunnels::Snapshot {
            config: "/old/config".into(),
            host: "deleted-alias".into(),
            profile: Some("deleted-profile".into()),
            forward: Forward {
                kind: "dynamic".into(),
                local_port: "1080".into(),
                ..Forward::default()
            },
        },
        state: runtime::tunnels::State::Active,
        exit_code: None,
        diagnostic: String::new(),
    });
    m.tunnel_profiles.clear();
    m.view = View::Active { selected: 0 };
    // Stopping asks first, naming the tunnel; the focused choice keeps it running.
    assert_eq!(m.key(key(KeyCode::Char('s'))), Action::None);
    let text = screen(&mut m, 100, 30);
    assert!(
        text.contains("Arrêter ce tunnel ?") && text.contains("#91"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    m.key(key(KeyCode::Char('s')));
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::StopTunnel(91));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    assert!(terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|c| c.bg == ratatui::style::Color::Cyan && c.fg == ratatui::style::Color::Black));
    assert!(screen(&mut m, 100, 30).contains("readiness unverified"));
    for (w, h) in [(4, 4), (1, 1), (20, 8)] {
        screen(&mut m, w, h);
    }
}

#[cfg(unix)]
#[test]
fn session_history_nonfatal_errors_and_drop_completion() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(dir.path().join("app"), dir.path().into());
    paths.initialize().unwrap();
    let config = dir.path().join("config");
    fs::write(&config, "Host alpha\n HostName localhost\n").unwrap();
    let exe = dir.path().join("ssh");
    fs::write(&exe, "#!/bin/sh\nsleep 60 & wait\n").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    let mut session = active::Session::default();
    session.manager = runtime::tunnels::Manager::with_program(exe);
    let mut m = model();
    session
        .start(
            &mut m,
            &paths,
            &config,
            "alpha".into(),
            None,
            sample_forward("dynamic"),
        )
        .unwrap();
    let history = History::load(&paths.root).unwrap();
    assert_eq!(history.connections["alpha"].connect_count, 1);
    assert!(history.connections["alpha"].last_result.is_none());
    drop(session);
    let history = History::load(&paths.root).unwrap();
    assert_eq!(
        history.connections["alpha"]
            .last_result
            .as_ref()
            .unwrap()
            .exit_code,
        Some(137)
    );
    assert!(history.connections["alpha"]
        .last_successful_completion
        .is_none());
    let history_path = paths.root.join("sshm_history.json");
    let external = dir.path().join("history-external");
    fs::rename(&history_path, &external).unwrap();
    let original = fs::read(&external).unwrap();
    symlink(&external, &history_path).unwrap();
    let mut session = active::Session::default();
    session.manager = runtime::tunnels::Manager::with_program(dir.path().join("ssh"));
    session
        .start(
            &mut m,
            &paths,
            &config,
            "alpha".into(),
            None,
            sample_forward("dynamic"),
        )
        .unwrap();
    assert!(session.manager.has_active());
    assert!(m.error.contains("history"));
    assert!(screen(&mut m, 120, 30).contains("Warning"));
    session.manager.stop_all();
    session.poll(&mut m, &paths);
    assert!(m.error.contains("history"));
    assert_eq!(fs::read(external).unwrap(), original);
}

#[test]
fn preview_delete_is_the_only_confirmation_and_f8_opens_backups() {
    let mut m = model();
    m.search_focus = false;
    assert!(
        matches!(m.key(key(KeyCode::Char('d'))), Action::Delete(_)),
        "delete should prepare a diff directly"
    );
    assert!(m.delete.is_none());
    assert_eq!(format!("{:?}", m.key(key(KeyCode::F(8)))), "Backups");
}

#[test]
fn preview_preserves_dirty_form_caret_parents_and_f7_overlay_then_completes_info() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let app = temp.path().join("app");
    fs::write(&root, "Host alpha\n HostName a.example\n").unwrap();
    let mut m = model();
    m.all = config::load_hosts(&root).unwrap();
    m.filter(false);
    m.open_view(View::Info(m.all[0].clone()));
    m.key(key(KeyCode::Char('e')));
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.names[0].set("new-name");
    form.names[0].cursor = 3;
    form.name_focus = true;
    form.focus = 0;
    let raw = format!("{form:?}");
    let parents = format!("{:?}", m.parents);
    m.key(key(KeyCode::F(7)));
    m.paste("q\n\x13junk");
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Host(form) if format!("{form:?}") == raw));
    assert_eq!(format!("{:?}", m.parents), parents);
    let action = m.key(ctrl('s'));
    changes::open(&mut m, action.clone(), &root).unwrap();
    assert!(screen(&mut m, 100, 30).contains("Review SSH changes"));
    m.paste("\r\nconfirm");
    assert!(matches!(m.view, View::Changes(_)));
    m.key(key(KeyCode::F(7)));
    m.paste("q\n");
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Changes(_)));
    assert!(!app.exists());
    assert!(fs::read_to_string(&root).unwrap().contains("Host alpha"));
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Host(form) if format!("{form:?}") == raw));
    assert_eq!(format!("{:?}", m.parents), parents);
    changes::open(&mut m, action.clone(), &root).unwrap();
    assert_eq!(m.key(key(KeyCode::Enter)), Action::ConfirmChanges);
    let completed = changes::apply(&mut m, &app).unwrap();
    assert_eq!(completed, action);
    assert!(matches!(m.view, View::Host(_)));
    let mut metadata = Metadata::load(&app, &root).unwrap();
    let mut profiles = TunnelProfiles::load(&app, &root).unwrap();
    metadata
        .set_note("alpha", "retained personal note")
        .unwrap();
    profiles
        .save("alpha", None, "retained", &sample_forward("dynamic"))
        .unwrap();
    profiles::complete_saved(
        &mut m,
        config::load_hosts(&root).unwrap(),
        &completed,
        &mut metadata,
        &mut profiles,
    );
    assert!(matches!(&m.view, View::Info(host) if host.name == "new-name"));
    assert_eq!(m.note("new-name"), "retained personal note");
    assert!(m.tunnel_profiles["new-name"].contains_key("retained"));
}

#[test]
fn preview_failure_keeps_review_and_restoration_cannot_drop_suspended_editors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let app = temp.path().join("app");
    fs::write(&root, "Host alpha\n HostName a.example\n").unwrap();
    let mut m = model();
    m.all = config::load_hosts(&root).unwrap();
    m.filter(false);
    m.open_view(View::Host(Box::new(HostForm::edit(m.all[0].clone()))));
    let action = m.key(ctrl('s'));
    changes::open(&mut m, action, &root).unwrap();
    let parents = format!("{:?}", m.parents);
    fs::write(&root, "# external\n").unwrap();
    assert!(changes::apply(&mut m, &app).is_err());
    assert!(matches!(m.view, View::Changes(_)));
    assert_eq!(format!("{:?}", m.parents), parents);
    assert!(!app.exists());
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(7)));
    assert!(
        changes::open_backups(&mut m, &app).is_err(),
        "F8 must not enable restore over a suspended editor"
    );
    assert!(matches!(m.view, View::Active { .. }));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::Host(_)));
}

#[test]
fn preview_move_cancel_keeps_file_selection_and_shows_both_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let target = temp.path().join("included");
    fs::write(
        &root,
        format!(
            "Include {}\nHost alpha\n HostName a.example\n",
            target.display()
        ),
    )
    .unwrap();
    fs::write(&target, "# target\n").unwrap();
    let mut m = model();
    m.all = config::load_hosts(&root).unwrap();
    m.filter(false);
    let host = m.all[0].clone();
    m.open_view(View::Files {
        files: vec![root.clone(), target.clone()],
        selected: 1,
        host: Some(host),
        seed: String::new(),
        duplicate: None,
        retarget: false,
    });
    let picker = format!("{:?}", m.view);
    let parents = format!("{:?}", m.parents);
    let action = m.key(key(KeyCode::Enter));
    changes::open(&mut m, action, &root).unwrap();
    let View::Changes(review) = &m.view else {
        panic!()
    };
    let text = format!("{review:?}");
    assert!(text.contains(&root.display().to_string()));
    assert!(text.contains(&target.display().to_string()));
    assert_eq!(m.key(key(KeyCode::F(5))), Action::None);
    m.key(key(KeyCode::Esc));
    assert_eq!(format!("{:?}", m.view), picker);
    assert_eq!(format!("{:?}", m.parents), parents);
    assert_eq!(fs::read_to_string(target).unwrap(), "# target\n");
}

#[test]
fn backups_keep_selected_long_destination_visible_on_short_narrow_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    let root = temp
        .path()
        .join("a-long-explicit-destination-for-backup-provenance");
    fs::write(&root, "# original\n").unwrap();
    for n in 0..12 {
        config::add_host(
            &Host {
                name: format!("host{n}"),
                hostname: "example.org".into(),
                ..Host::default()
            },
            &root,
            &app,
        )
        .unwrap();
    }
    let mut m = model();
    changes::open_backups(&mut m, &app).unwrap();
    screen(&mut m, 40, 10);
    for _ in 0..8 {
        m.key(key(KeyCode::Down));
    }
    let text = screen(&mut m, 40, 10);
    assert!(
        text.contains('›'),
        "selected backup must remain visible with wrapped destination: {text}"
    );
    m.paste("\n\r");
    assert!(matches!(m.view, View::Backups { selected: 8, .. }));
}

#[test]
fn active_quit_confirms_discard_of_suspended_dirty_forms_and_preserves_caret_on_cancel() {
    let mut m = model();
    m.open_view(View::Info(m.all[0].clone()));
    m.key(key(KeyCode::Char('e')));
    let View::Host(form) = &mut m.view else {
        panic!()
    };
    form.names[0].set("unsaved-name");
    form.names[0].cursor = 3;
    let raw = format!("{form:?}");
    let parents = format!("{:?}", m.parents);
    m.key(key(KeyCode::F(7)));
    // Returning from the owned tunnels never quits: the suspended draft comes back.
    for back in [key(KeyCode::Char('q')), ctrl('c'), key(KeyCode::Esc)] {
        assert!(matches!(m.view, View::Active { .. }));
        assert_eq!(m.key(back), Action::None);
        assert!(!m.quit_tunnels);
        assert!(matches!(&m.view, View::Host(form) if format!("{form:?}") == raw));
        assert_eq!(format!("{:?}", m.parents), parents);
        m.key(key(KeyCode::F(7)));
    }
    // A quit request names the draft and only an explicit choice loses it.
    m.ask_quit();
    let text = screen(&mut m, 100, 20);
    assert!(text.contains("Modifications non enregistrées"), "{text}");
    assert!(text.contains("[  Abandonner et quitter ]"), "{text}");
    m.paste("\r\n");
    assert!(m.quit_tunnels);
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(!m.quit_tunnels);
    m.ask_quit();
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Quit);
    m.quit_tunnels = false;
    m.key(key(KeyCode::Esc));
    assert!(matches!(&m.view, View::Host(form) if format!("{form:?}") == raw));
    let mut clean = model();
    clean.search_focus = false;
    clean.key(key(KeyCode::F(7)));
    assert_eq!(clean.key(key(KeyCode::Char('q'))), Action::None);
    assert!(matches!(clean.view, View::List));
    assert_eq!(clean.key(key(KeyCode::Char('q'))), Action::Quit);
    assert!(!clean.quit_tunnels);
}

#[test]
fn preview_end_shows_last_page_instead_of_a_blank_screen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    fs::write(&root, "# preserved\n").unwrap();
    let mut m = model();
    let host = Host {
        name: "new".into(),
        hostname: "tail.example".into(),
        ..Host::default()
    };
    changes::open(
        &mut m,
        Action::Save(None, Box::new(host), root.clone()),
        &root,
    )
    .unwrap();
    screen(&mut m, 50, 10);
    m.key(key(KeyCode::End));
    assert!(screen(&mut m, 50, 10).contains("tail.example"));
    assert!(screen(&mut m, 100, 30).contains("tail.example"));
}

#[test]
fn active_quit_also_protects_notes_profile_drafts_and_prepared_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    fs::write(&root, "# original\n").unwrap();
    let mut note = notes::Editor::new("alpha".into(), "original");
    note.input.set("changed");
    let mut forward = ForwardForm::new("alpha".into(), None);
    forward.inputs[1].set("changed");
    for view in [View::Note(note), View::Forward(forward)] {
        let mut m = model();
        m.open_view(view);
        let before = format!("{:?}", m.view);
        m.key(key(KeyCode::F(7)));
        assert_eq!(m.key(ctrl('c')), Action::None);
        assert!(!m.quit_tunnels);
        assert_eq!(format!("{:?}", m.view), before);
        assert!(m.has_unsaved_context());
    }
    let mut m = model();
    changes::open(
        &mut m,
        Action::Save(
            None,
            Box::new(Host {
                name: "new".into(),
                hostname: "example.org".into(),
                ..Host::default()
            }),
            root.clone(),
        ),
        &root,
    )
    .unwrap();
    let before = format!("{:?}", m.view);
    m.key(key(KeyCode::F(7)));
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::None);
    assert!(!m.quit_tunnels);
    assert_eq!(format!("{:?}", m.view), before);
    assert!(m.has_unsaved_context());
    assert_eq!(fs::read_to_string(root).unwrap(), "# original\n");
}

#[test]
fn final_review_validation_errors_follow_visual_order_including_copy_name() {
    use forms::FormField;
    let mut form = HostForm::add("valid".into(), "config".into());
    form.inputs[1].set("valid.example");
    form.inputs[7].set("-o X=bad\0value");
    form.inputs[8].set("bad\ntag");
    assert!(!form.validate());
    assert_eq!(
        form.focus, 7,
        "the form is read top to bottom: options, then tags"
    );
    assert!(form.is_open(forms::Section::Advanced));
    assert!(
        !form.is_open(forms::Section::Organization),
        "only the section of the first error is unfolded"
    );
    assert_eq!(
        form.errors.iter().map(|e| e.field).collect::<Vec<_>>(),
        vec![FormField::Input(7), FormField::Input(8)]
    );
}

#[test]
fn final_review_copy_name_error_precedes_invalid_port() {
    use forms::FormField;
    let source = model().all[0].clone();
    let mut copy = HostForm::duplicate(source, "config".into());
    copy.inputs[0].set("alpha");
    copy.inputs[3].set("65536");
    assert!(!copy.validate());
    assert_eq!(copy.focus, 0, "copy-name collision precedes invalid port");
    assert_eq!(
        copy.errors.iter().map(|e| e.field).collect::<Vec<_>>(),
        vec![FormField::Input(0), FormField::Input(3)]
    );
}

#[test]
fn final_review_picker_errors_wrap_without_losing_destination_or_parent() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let target = temp.path().join("included");
    fs::write(
        &root,
        format!(
            "Include {}\nHost alpha\n HostName a.example\n",
            target.display()
        ),
    )
    .unwrap();
    fs::write(&target, "Host alpha\n HostName occupied.example\n").unwrap();
    let before = (fs::read(&root).unwrap(), fs::read(&target).unwrap());
    let mut m = model();
    let host = config::load_hosts(&root)
        .unwrap()
        .into_iter()
        .find(|h| h.source_file == root)
        .unwrap();
    m.open_view(View::Files {
        files: vec![root.clone(), target.clone()],
        selected: 1,
        host: Some(host),
        seed: String::new(),
        duplicate: None,
        retarget: false,
    });
    let picker = format!("{:?}", m.view);
    let parents = format!("{:?}", m.parents);
    let action = m.key(key(KeyCode::Enter));
    m.error = format!("{:#}", changes::open(&mut m, action, &root).unwrap_err());
    assert!(m.error.contains("already exists"));
    for (width, height) in [(100, 25), (40, 10)] {
        let text = screen(&mut m, width, height);
        assert!(text.contains("already exists"), "{text}");
        assert!(text.contains('▶'), "destination remains visible: {text}");
    }
    assert_eq!(m.key(key(KeyCode::F(8))), Action::Backups);
    m.error = format!(
        "{:#}",
        changes::open_backups(&mut m, &temp.path().join("app")).unwrap_err()
    );
    let text = screen(&mut m, 40, 10);
    assert!(text.contains("Return to the host list"), "{text}");
    assert!(text.contains("unchanged"), "wrapped instruction: {text}");
    assert_eq!(format!("{:?}", m.view), picker);
    assert_eq!(format!("{:?}", m.parents), parents);
    assert_eq!((fs::read(root).unwrap(), fs::read(target).unwrap()), before);
}

#[test]
fn final_review_explicit_edits_keep_query_when_earlier_native_values_mask_them() {
    for prefix in [
        "Host *\n User inherited\n HostName inherited.example\n",
        "User inherited\nHostName inherited.example\n",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        fs::write(
            &root,
            format!("{prefix}Host target\n User declared\n HostName declared.example\n"),
        )
        .unwrap();
        let mut m = Model::new(
            config::load_hosts(&root).unwrap(),
            AppConfig::default(),
            BTreeMap::new(),
            false,
        );
        let inspector = runtime::effective::Inspector::default();
        for result in start_inspection(&inspector, &mut m, &root, false) {
            m.apply_inspection(result);
        }
        let query = "user:inherited AND inherited.example";
        m.query.set(query);
        m.filter(true);
        for edit in 0..2 {
            let original = m.selected_host().unwrap();
            let mut saved = original.clone();
            saved.user = format!("new-user-{edit}");
            saved.hostname = format!("new-{edit}.example");
            config::update_host(&original, &saved, &temp.path().join("app")).unwrap();
            m.complete_mutation(
                config::load_hosts(&root).unwrap(),
                &Action::Save(Some(original), Box::new(saved), root.clone()),
            );
            assert_eq!(m.query.value, query, "defer to native precedence");
            let receiver = start_inspection(&inspector, &mut m, &root, false);
            assert_eq!(m.selected_host().unwrap().name, "target");
            assert!(m.effective.is_empty());
            for result in receiver {
                m.apply_inspection(result);
            }
            assert_eq!(m.query.value, query);
            assert!(m.pending_preferred.is_none());
            assert_eq!(m.effective["target"].as_ref().unwrap().user, "inherited");
        }
    }
}

#[test]
fn table_c_opens_source_selector_action_and_search_keeps_text() {
    let mut m = model();
    typing(&mut m, "cC");
    assert_eq!(m.query.value, "cC");
    assert!(matches!(m.view, View::List));
    m.key(key(KeyCode::Tab));
    assert_eq!(
        format!("{:?}", m.key(key(KeyCode::Char('c')))),
        "SourceFilter"
    );
}

#[test]
fn f2_source_actions_are_available_with_empty_search_results() {
    let mut m = model();
    typing(&mut m, "no-such-host");
    assert!(m.visible.is_empty());
    assert_eq!(
        format!("{:?}", palette(&mut m, "filtrer par fichier")),
        "SourceFilter"
    );
    assert!(m.search_focus);
    assert_eq!(m.query.value, "no-such-host");
}

fn filter_sources() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, Model) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("config");
    let a = temp.path().join("a/hosts");
    let b = temp.path().join("b/hosts");
    fs::create_dir_all(a.parent().unwrap()).unwrap();
    fs::create_dir_all(b.parent().unwrap()).unwrap();
    fs::write(
        &root,
        "Include a/hosts b/hosts empty\nHost global\n HostName global.invalid\n",
    )
    .unwrap();
    fs::write(
        &a,
        "# Tags: work\nHost alpha\n User deploy\n# Tags: hidden, work\nHost secret\n",
    )
    .unwrap();
    fs::write(&b, "# Tags: work\nHost beta\n User deploy\n").unwrap();
    fs::write(temp.path().join("empty"), "# Empty config\n").unwrap();
    let m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    (temp, root, a, b, m)
}
fn choose_source(m: &mut Model, root: &Path, target: Option<&Path>) {
    source_filter::open(m, root).unwrap();
    if let View::SourceFiles { files, selected } = &mut m.view {
        *selected = target
            .map(|p| files.iter().position(|f| f == p).unwrap() + 1)
            .unwrap_or(0);
    } else {
        panic!("source selector missing");
    }
    m.key(key(KeyCode::Enter));
}
#[test]
fn selector_lists_empty_files_and_all_and_preserves_parent_caret() {
    let (temp, root, a, _, mut m) = filter_sources();
    typing(&mut m, "tag:work");
    m.query.key(key(KeyCode::Left));
    let caret = m.query.cursor;
    m.scroll = 7;
    source_filter::open(&mut m, &root).unwrap();
    let text = screen(&mut m, 100, 20);
    assert!(text.contains("Tous les fichiers"), "{text}");
    assert!(text.contains(temp.path().join("empty").to_str().unwrap()));
    if let View::SourceFiles { files, .. } = &m.view {
        assert_eq!(files.len(), 4);
        assert!(files.iter().all(|p| p.is_absolute()));
        assert!(m.all.iter().all(|h| files.contains(&h.source_file)));
    } else {
        panic!("source selector missing");
    }
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    assert_eq!(m.scroll, 7);
    assert!(m.parents.is_empty());
    choose_source(&mut m, &root, Some(&a));
    assert_eq!(m.source_filter.as_deref(), Some(a.as_path()));
    assert!(m.search_focus);
    assert_eq!(m.query.value, "tag:work");
    assert_eq!(m.query.cursor, caret);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    choose_source(&mut m, &root, None);
    assert!(m.source_filter.is_none());
    assert_eq!(m.visible.len(), 2);
}
#[test]
fn exact_scope_composes_with_search_hidden_favorites_and_clear() {
    let (_temp, root, a, _, mut m) = filter_sources();
    typing(&mut m, "tag:work user:deploy file:hosts");
    choose_source(&mut m, &root, Some(&a));
    assert_eq!(m.visible.len(), 1);
    m.metadata.insert(
        "beta".into(),
        HostMetadata {
            favorite: true,
            ..HostMetadata::default()
        },
    );
    m.favorites_only = true;
    m.filter(false);
    assert!(m.visible.is_empty());
    assert_eq!(m.selected, 0);
    m.favorites_only = false;
    m.query.set("tag:work");
    m.filter(true);
    m.search_focus = false;
    m.key(key(KeyCode::Char('H')));
    assert_eq!(m.visible.len(), 2);
    assert_eq!(m.source_filter.as_ref(), Some(&a));
    m.key(key(KeyCode::End));
    m.key(key(KeyCode::Char('H')));
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!(m.selected, 0);
    let query = m.query.value.clone();
    m.key(key(KeyCode::Char('C')));
    assert!(m.source_filter.is_none());
    assert_eq!(m.query.value, query);
    assert_eq!(m.visible.len(), 2);
}
#[test]
fn empty_filter_banner_survives_query_reload_and_compact_errors() {
    let (temp, root, _, _, mut m) = filter_sources();
    let empty = temp.path().join("empty");
    choose_source(&mut m, &root, Some(&empty));
    assert!(m.visible.is_empty());
    typing(&mut m, "missing");
    m.key(ctrl('u'));
    assert!(m.query.value.is_empty() && m.search_focus);
    assert_eq!(m.source_filter.as_ref(), Some(&empty));
    let inspector = runtime::effective::Inspector::default();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert_eq!(m.source_filter.as_ref(), Some(&empty));
    m.error = "reload failed".into();
    m.query.set("tag:\"");
    m.filter(true);
    for (w, h) in [(100, 25), (60, 10), (24, 8)] {
        let s = screen(&mut m, w, h);
        assert!(s.contains("Fichier"), "{w}x{h}: {s}");
        assert!(s.contains("C/F2"), "{w}x{h}: {s}");
    }
    screen(&mut m, 1, 1);
    palette(&mut m, "tous les fichiers");
    assert!(m.source_filter.is_none());
    assert!(m.search_focus);
}
#[test]
fn only_delete_or_move_last_raw_host_clears_filter() {
    for moved in [false, true] {
        for remaining in [true, false] {
            let (_temp, root, a, b, mut m) = filter_sources();
            choose_source(&mut m, &root, Some(&a));
            let alpha = m.selected_host().unwrap();
            let mut next: Vec<_> = m
                .all
                .iter()
                .filter(|h| h.name != "alpha" && (remaining || h.name != "secret"))
                .cloned()
                .collect();
            let action = if moved {
                next.push(Host {
                    source_file: b.clone(),
                    ..alpha.clone()
                });
                Action::MoveTo(alpha, b)
            } else {
                Action::Delete(alpha)
            };
            m.query.set("alpha");
            m.filter(true);
            m.complete_mutation(next, &action);
            assert_eq!(m.source_filter.is_some(), remaining);
            assert!(m.selected <= m.visible.len().saturating_sub(1));
            if remaining {
                assert!(m.visible.is_empty());
                assert_eq!(m.query.value, "alpha");
            }
        }
    }
    let (_temp, root, a, b, mut m) = filter_sources();
    choose_source(&mut m, &root, Some(&a));
    m.replace_hosts(vec![], None);
    assert_eq!(m.source_filter.as_ref(), Some(&a));
    let saved = Host {
        name: "outside".into(),
        source_file: b.clone(),
        ..Host::default()
    };
    m.complete_mutation(vec![saved.clone()], &Action::Save(None, Box::new(saved), b));
    assert_eq!(m.source_filter.as_ref(), Some(&a));
    assert!(m.visible.is_empty());
}

#[test]
fn source_selector_selected_row_is_legible_and_help_names_active_file() {
    use ratatui::style::Color;
    let (_temp, root, a, _, mut m) = filter_sources();
    source_filter::open(&mut m, &root).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let cells = terminal.backend().buffer().content();
    let selected = cells.iter().find(|cell| cell.symbol() == "▶").unwrap();
    assert_eq!(selected.fg, Color::Black);
    assert_eq!(selected.bg, Color::Rgb(0, 173, 216));
    m.key(key(KeyCode::Esc));
    choose_source(&mut m, &root, Some(&a));
    m.search_focus = false;
    m.key(key(KeyCode::Char('h')));
    let text = screen(&mut m, 100, 30);
    assert!(text.contains(a.to_str().unwrap()), "{text}");
}

fn pending_filter_save(m: &mut Model, root: &Path, source: &Path) {
    choose_source(m, root, Some(source));
    m.apply_inspection(inspection("alpha", "inherited", PingStatus::Unknown));
    m.query.set("user:inherited");
    m.filter(true);
    let mut saved = m.selected_host().unwrap();
    saved.tags.push("edited".into());
    let mut hosts = m.all.clone();
    *hosts.iter_mut().find(|host| host.name == "alpha").unwrap() = saved.clone();
    m.complete_mutation(
        hosts,
        &Action::Save(
            Some(m.selected_host().unwrap()),
            Box::new(saved),
            source.to_path_buf(),
        ),
    );
    assert_eq!(m.query.value, "user:inherited");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.query.key(key(KeyCode::Left));
}

#[test]
fn selecting_other_source_protects_query_from_late_saved_host_resolution() {
    for empty in [false, true] {
        for search_focus in [false, true] {
            let (temp, root, a, b, mut m) = filter_sources();
            pending_filter_save(&mut m, &root, &a);
            m.search_focus = search_focus;
            let caret = m.query.cursor;
            let target = if empty { temp.path().join("empty") } else { b };
            choose_source(&mut m, &root, Some(&target));
            assert!(m.visible.is_empty());
            m.apply_inspection(inspection("alpha", "changed", PingStatus::Unknown));
            assert_eq!(m.query.value, "user:inherited");
            assert_eq!(m.query.cursor, caret);
            assert_eq!(m.search_focus, search_focus);
            assert_eq!(m.source_filter.as_ref(), Some(&target));
            assert!(m.visible.is_empty());
            assert_eq!(m.selected, 0);
            assert!(matches!(m.view, View::List));
            assert!(m.parents.is_empty());
        }
    }
}

#[test]
fn expanding_or_retaining_source_keeps_applicable_pending_save_resolution() {
    for selection in ["C", "F2-clear", "all", "same"] {
        let (_temp, root, a, _, mut m) = filter_sources();
        pending_filter_save(&mut m, &root, &a);
        m.search_focus = false;
        match selection {
            "C" => {
                m.key(key(KeyCode::Char('C')));
            }
            "F2-clear" => {
                m.search_focus = true;
                palette(&mut m, "tous les fichiers");
            }
            "all" => choose_source(&mut m, &root, None),
            "same" => choose_source(&mut m, &root, Some(&a)),
            _ => unreachable!(),
        }
        assert_eq!(m.query.value, "user:inherited");
        m.apply_inspection(inspection("alpha", "changed", PingStatus::Unknown));
        assert!(
            m.query.value.is_empty(),
            "{selection} must retain still-applicable pending resolution"
        );
        assert_eq!(m.selected_host().unwrap().name, "alpha");
        assert_eq!(m.source_filter.is_some(), selection == "same");
    }
}

#[test]
fn batch_open_requires_explicit_targets_and_search_keeps_shortcuts_literal() {
    let mut m = model();
    typing(&mut m, "xB");
    assert_eq!(m.query.value, "xB");
    assert!(matches!(m.view, View::List));
    m.query.set("");
    m.filter(true);
    m.search_focus = false;
    m.key(key(KeyCode::Char('B')));
    let text = screen(&mut m, 100, 25);
    assert!(text.contains("Batch"), "{text}");
    assert!(
        text.contains("0 targets"),
        "opening must not implicitly select: {text}"
    );
}

#[test]
fn availability_p_opens_selector_but_remains_search_text_and_f2_is_palette() {
    let mut m = model();
    m.key(key(KeyCode::Char('P')));
    assert_eq!(m.query.value, "P");
    assert!(matches!(m.view, View::List));
    // Esc leaves the edit and keeps the text; the table then owns the letter.
    m.key(key(KeyCode::Esc));
    assert_eq!(m.query.value, "P");
    m.key(key(KeyCode::Char('P')));
    let rendered = screen(&mut m, 80, 24);
    assert!(rendered.contains("Méthode de disponibilité"), "{rendered}");
    assert!(rendered.contains("ICMP local"));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(2)));
    assert!(matches!(m.view, View::Actions { .. }));
}

#[test]
fn availability_save_adopts_fresh_settings_preserves_selection_and_external_fields() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("config.json"),
        r#"{"availability_method":"ssh","focus_search_on_start":false,"future":{"x":7}}"#,
    )
    .unwrap();
    let mut m = model();
    m.search_focus = false;
    m.selected = 1;
    m.open_availability();
    save_availability(&mut m, temp.path(), AvailabilityMethod::Icmp).unwrap();
    assert_eq!(m.preferences.availability_method, AvailabilityMethod::Icmp);
    assert!(!m.preferences.focus_search_on_start);
    assert_eq!(m.selected_host().unwrap().name, "beta");
    assert!(matches!(m.view, View::List));
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["future"]["x"], 7);
    assert_eq!(saved["availability_method"], "icmp");
}

#[test]
fn availability_failed_save_keeps_method_selector_and_existing_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    // A directory cannot be atomically replaced by config.json.
    fs::create_dir(temp.path().join("config.json")).unwrap();
    let mut m = model();
    m.open_availability();
    assert!(save_availability(&mut m, temp.path(), AvailabilityMethod::Icmp).is_err());
    assert_eq!(m.preferences.availability_method, AvailabilityMethod::Ssh);
    assert!(matches!(m.view, View::Availability { .. }));
}

#[test]
fn availability_method_visible_in_list_details_and_help_and_filters_use_selected_method() {
    let mut m = model();
    m.preferences.availability_method = AvailabilityMethod::Icmp;
    m.statuses.insert(
        "alpha".into(),
        PingResult {
            name: "alpha".into(),
            method: AvailabilityMethod::Icmp,
            status: PingStatus::Online,
            duration: Duration::from_millis(42),
            error: None,
        },
    );
    assert!(screen(&mut m, 80, 24).contains("ICMP local"));
    let lines = details::content(&m.all[0], m.statuses.get("alpha"), None, None, "");
    let text = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("ICMP local") && text.contains("network availability"),
        "{text}"
    );
    assert!(
        text.contains("diagnostic 42 ms") && text.contains("no SSH authentication"),
        "{text}"
    );
    typing(&mut m, "is:online");
    assert_eq!(m.visible, [0]);
    m.apply_inspection(runtime::effective::Inspection {
        name: "alpha".into(),
        effective: Err("stale SSH resolution".into()),
        ping: Some(PingResult {
            name: "alpha".into(),
            method: AvailabilityMethod::Ssh,
            status: PingStatus::Offline,
            duration: Duration::ZERO,
            error: None,
        }),
    });
    assert_eq!(m.statuses["alpha"].status, PingStatus::Online);
    assert!(!m.effective.contains_key("alpha"));
    m.view = View::Help;
    assert!(screen(&mut m, 110, 60).contains("P         Méthode de disponibilité"));
}

#[test]
fn availability_switch_keeps_effective_search_selection_and_group_context() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    fs::write(
        &config,
        "Host alpha beta\n HostName 127.0.0.1\n User inherited\n",
    )
    .unwrap();
    let mut m = model();
    m.preferences.group_mode = GroupMode::Files;
    for name in ["alpha", "beta"] {
        m.effective.insert(
            name.into(),
            Ok(runtime::effective::EffectiveConfig::parse(
                "hostname 127.0.0.1\nuser inherited\nport 22\n",
            )
            .unwrap()),
        );
    }
    typing(&mut m, "user:inherited");
    m.search_focus = false;
    m.key(key(KeyCode::End));
    assert_eq!(m.selected_host().unwrap().name, "beta");
    let rows = m.rows.clone();
    m.preferences.availability_method = AvailabilityMethod::Icmp;
    let inspector = runtime::effective::Inspector::default();
    // Resolve only: this regression must not perform even loopback probes.
    let receiver = start_availability_inspection(&inspector, &mut m, &config, false);
    assert_eq!(m.query.value, "user:inherited");
    assert_eq!(m.selected_host().map(|host| host.name), Some("beta".into()));
    assert_eq!(m.rows, rows);
    drop(receiver);
}

#[test]
fn availability_selected_option_remains_visible_in_tiny_terminal() {
    let mut m = model();
    m.open_availability();
    m.key(key(KeyCode::Down));
    let rendered = screen(&mut m, 28, 8);
    assert!(rendered.contains("▶  ICMP local"), "{rendered}");
    assert!(
        rendered.contains("✓ SSH"),
        "the value in effect stays marked: {rendered}"
    );
    assert!(rendered.contains("Entrée : enregistrer"), "{rendered}");
}

fn availability_external_preference_action(group_action: bool) {
    for (old_method, new_method) in [
        (AvailabilityMethod::Ssh, AvailabilityMethod::Icmp),
        (AvailabilityMethod::Icmp, AvailabilityMethod::Ssh),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("ssh-config");
        // Native resolution fails before any connection or ping can run.
        fs::write(&config, "Host alpha beta secret\n HostName\n").unwrap();
        let mut m = model();
        m.preferences.availability_method = old_method;
        m.preferences.group_mode = GroupMode::Files;
        for host in &mut m.all {
            host.source_file = config.clone();
        }
        m.source_filter = Some(config.clone());
        for name in ["alpha", "beta"] {
            let mut result = inspection(name, "inherited", PingStatus::Online);
            result.ping.as_mut().unwrap().method = old_method;
            m.apply_inspection(result);
        }
        typing(&mut m, "user:inherited");
        m.search_focus = false;
        m.key(key(KeyCode::End));
        assert_eq!(m.selected_host().unwrap().name, "beta");
        let mut external = m.preferences.clone();
        external.availability_method = new_method;
        external.save(temp.path()).unwrap();
        let action = if group_action {
            m.key(key(KeyCode::Char('g')));
            m.key(key(KeyCode::Down));
            m.key(key(KeyCode::Enter))
        } else {
            m.key(key(KeyCode::Char('A')))
        };
        assert_eq!(
            action,
            if group_action {
                Action::GroupMode(GroupMode::FileTags)
            } else {
                Action::ToggleAliases
            }
        );
        let inspector = runtime::effective::Inspector::default();
        let (old_sender, mut receiver) = std::sync::mpsc::channel();
        let mut stale = inspection("beta", "stale-user", PingStatus::Offline);
        stale.ping.as_mut().unwrap().method = old_method;
        old_sender.send(stale).unwrap();
        apply_preference_action(
            &inspector,
            &mut m,
            temp.path(),
            &config,
            &mut receiver,
            &action,
        )
        .unwrap();
        assert_eq!(m.preferences.availability_method, new_method);
        assert!(
            m.statuses
                .values()
                .all(|ping| ping.method == new_method && ping.status == PingStatus::Connecting),
            "cached results must switch with adopted method"
        );
        assert!(
            old_sender
                .send(inspection("beta", "stale-user", PingStatus::Online))
                .is_err(),
            "old generation receiver must be dropped"
        );
        assert_eq!(m.query.value, "user:inherited");
        assert_eq!(m.source_filter, Some(config.clone()));
        assert_eq!(m.selected_host().unwrap().name, "beta");
        assert_eq!(
            m.preferences.group_mode,
            if group_action {
                GroupMode::FileTags
            } else {
                GroupMode::Files
            }
        );
        assert!(matches!(m.view, View::List));
        m.query.set("is:online");
        m.filter(true);
        assert!(
            m.visible.is_empty(),
            "old Online verdict cannot satisfy the new method filter"
        );
        for result in receiver {
            if let Some(ping) = &result.ping {
                assert_eq!(ping.method, new_method);
            }
            m.apply_inspection(result);
        }
        assert!(m
            .statuses
            .values()
            .all(|ping| ping.method == new_method && ping.status == PingStatus::Unknown));
        assert!(m.visible.is_empty());
        let mut fresh = inspection("beta", "inherited", PingStatus::Online);
        fresh.ping.as_mut().unwrap().method = new_method;
        m.apply_inspection(fresh);
        assert_eq!(m.selected_host().unwrap().name, "beta");
        let mut stale = inspection("beta", "stale-user", PingStatus::Offline);
        stale.ping.as_mut().unwrap().method = old_method;
        m.apply_inspection(stale);
        assert_eq!(m.statuses["beta"].status, PingStatus::Online);
        assert_eq!(m.effective["beta"].as_ref().unwrap().user, "inherited");
        assert_eq!(m.selected_host().unwrap().name, "beta");
    }
}

#[test]
fn availability_alias_dispatch_renews_external_method_generation() {
    availability_external_preference_action(false);
}

#[test]
fn availability_group_dispatch_renews_external_method_generation() {
    availability_external_preference_action(true);
}

#[test]
fn availability_preference_dispatch_failure_preserves_preferences_cache_and_receiver() {
    for group_action in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("config.json")).unwrap();
        let mut m = model();
        m.search_focus = false;
        m.apply_inspection(inspection("alpha", "inherited", PingStatus::Online));
        let action = if group_action {
            m.key(key(KeyCode::Char('g')));
            m.key(key(KeyCode::Down));
            m.key(key(KeyCode::Enter))
        } else {
            m.key(key(KeyCode::Char('A')))
        };
        let preferences = m.preferences.clone();
        let rows = m.rows.clone();
        let inspector = runtime::effective::Inspector::default();
        let (sender, mut receiver) = std::sync::mpsc::channel();
        let error = apply_preference_action(
            &inspector,
            &mut m,
            temp.path(),
            &temp.path().join("unused"),
            &mut receiver,
            &action,
        )
        .unwrap_err();
        m.error = format!("{error:#}");
        assert_eq!(m.preferences, preferences);
        assert_eq!(m.rows, rows);
        assert_eq!(m.statuses["alpha"].status, PingStatus::Online);
        assert!(matches!(m.view, View::GroupMode { .. }) == group_action);
        sender
            .send(inspection("alpha", "still-current", PingStatus::Offline))
            .unwrap();
        m.apply_inspection(receiver.try_recv().unwrap());
        assert_eq!(m.statuses["alpha"].status, PingStatus::Offline);
        assert!(screen(&mut m, 100, 24).contains("config.json"));
    }
}

#[test]
fn initial_query_filters_loaded_metadata_and_forces_editable_unicode_caret() {
    let mut m = model();
    m.preferences.focus_search_on_start = false;
    m.search_focus = false;
    m.metadata.insert(
        "alpha".into(),
        HostMetadata {
            favorite: true,
            note: "Release café 東京".into(),
        },
    );
    let query = "is:favorite tag:work AND \"café 東京\"";
    m.initialize_query(query);
    assert!(matches!(m.view, View::List));
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert!(m.search_focus);
    assert_eq!(m.query.cursor, query.chars().count());
    assert_eq!(m.key(key(KeyCode::F(5))), Action::Reload);
    assert_eq!(m.query.value, query);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    use ratatui::backend::Backend;
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(
        terminal.backend().buffer()[(position.x - 1, position.y)].symbol(),
        "\""
    );
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), None)
    );
    m.resume_connection(
        "alpha",
        Ok(37),
        History::load(tempfile::tempdir().unwrap().path()),
    );
    assert_eq!(m.query.value, query);
    assert!(m.search_focus);
}

#[test]
fn initial_query_empty_invalid_and_no_match_remain_editable_browser() {
    for query in ["", "tag:", "no-such-host", "東京"] {
        let mut m = model();
        m.search_focus = false;
        m.initialize_query(query);
        assert!(m.search_focus && matches!(m.view, View::List));
        assert_eq!(m.query.value, query);
        if !query.is_empty() {
            assert!(m.visible.is_empty());
            assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
        }
        m.key(key(KeyCode::F(2)));
        assert!(screen(&mut m, 100, 30).contains("Actions"));
        m.key(key(KeyCode::Esc));
        assert!(m.search_focus && matches!(m.view, View::List));
        m.key(ctrl('u'));
        typing(&mut m, "alpha");
        assert_eq!(m.selected_host().unwrap().name, "alpha");
    }
}

#[test]
fn initial_query_native_user_arrives_without_erasing_expression() {
    let mut m = model();
    m.initialize_query("user:inherited AND tag:work");
    assert!(m.visible.is_empty());
    m.apply_inspection(runtime::effective::Inspection {
        name: "alpha".into(),
        effective: Ok(runtime::effective::EffectiveConfig::parse(
            "hostname native.example\nuser inherited\nport 22\n",
        )
        .unwrap()),
        ping: None,
    });
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!(m.query.value, "user:inherited AND tag:work");
    assert!(m.search_focus);
}
