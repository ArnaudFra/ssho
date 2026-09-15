use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
fn key(code: KeyCode) -> KeyEvent {
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
fn typing(m: &mut Model, s: &str) {
    for c in s.chars() {
        m.key(key(KeyCode::Char(c)));
    }
}
fn screen(m: &mut Model, w: u16, h: u16) -> String {
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
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(m.query.value.is_empty());
    assert_eq!(m.key(key(KeyCode::Esc)), Action::Quit);
}
#[test]
fn search_never_runs_shortcuts() {
    let mut m = model();
    typing(&mut m, "aefdmqHsnriph/");
    assert!(matches!(m.view, View::List));
    assert_eq!(m.query.value, "aefdmqHsnriph/");
}
#[test]
fn tab_hidden_delete_and_preferences() {
    let mut m = model();
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('H')));
    assert_eq!(m.visible.len(), 3);
    m.key(key(KeyCode::Char('d')));
    assert!(m.delete.is_some());
    m.key(key(KeyCode::Char('j')));
    assert_eq!(m.selected, 0);
    m.key(key(KeyCode::Esc));
    assert!(m.delete.is_none());
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
    assert!(s.find("Tags").unwrap() < s.find("Last Login").unwrap());
    assert!(s.contains("SSHO"));
    assert!(s.contains("#work"));
    for (w, h) in [(60, 15), (24, 8), (1, 1)] {
        screen(&mut m, w, h);
    }
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
    f.key(ctrl('a'));
    assert_eq!(f.names.len(), 3);
    f.names[2].set("three");
    f.key(ctrl('d'));
    assert_eq!(f.names.len(), 2);
    f.key(key(KeyCode::Tab));
    assert!(!f.name_focus);
    f.key(ctrl('j'));
    assert_eq!(f.tab, 1);
    assert_eq!(f.focus, 7);
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
        "Host Names",
        "Common Properties",
        "General",
        "Advanced",
        "Hostname/IP *",
        "Proxy Jump",
        "Proxy Command",
        "Tags (comma-separated)",
        "extra key",
    ] {
        assert!(s.contains(label), "missing {label}");
    }
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
    typing(&mut m, &"x".repeat(60));
    assert_eq!(m.query.value.len(), 50);
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
        },
    );
    m.key(key(KeyCode::Tab));
    assert!(!m.recent);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.key(key(KeyCode::Char('r')));
    assert!(m.recent);
    assert_eq!(m.selected_host().unwrap().name, "beta");
    m.key(key(KeyCode::Char('s')));
    assert!(!m.recent);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.key(key(KeyCode::Char('d')));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Delete(m.selected_host().unwrap())
    );
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Char('i')));
    assert!(matches!(m.view, View::Info(_)));
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Host(_)));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
}
#[test]
fn form_tab_order_and_submit_shortcuts() {
    let mut add = forms::HostForm::add("one".into(), PathBuf::from("config"));
    for expected in [1, 2, 3, 4, 5, 6, 8, 7, 9, 10] {
        assert_eq!(add.key(key(KeyCode::Tab)), forms::FormEvent::None);
        assert_eq!(add.focus, expected);
    }
    assert_eq!(add.tab, 1);
    assert_eq!(add.key(key(KeyCode::Enter)), forms::FormEvent::Submit);
    add.key(key(KeyCode::BackTab));
    assert_eq!(add.focus, 9);
    add.key(ctrl('k'));
    assert_eq!(add.tab, 0);
    assert_eq!(add.focus, 0);
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
    assert!(!f.error.is_empty());
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
    };
    m.key(key(KeyCode::Char('j')));
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
    };
    m.key(key(KeyCode::Enter));
    let View::Host(ref form) = m.view else {
        panic!()
    };
    assert_eq!(form.target, PathBuf::from("a"));
    assert_eq!(form.inputs[0].value, "new");
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Tab));
    m.key(key(KeyCode::Char('f')));
    let View::Forward(ref mut form) = m.view else {
        panic!()
    };
    form.inputs[1].set("8080");
    form.inputs[3].set("80");
    let action = m.key(ctrl('s'));
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
    assert!(info.contains("Actions:"));
    m.view = View::Help;
    let help = screen(&mut m, 100, 30);
    for section in [
        "Navigation & Connection",
        "Advanced Features",
        "Host Management",
        "System",
    ] {
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
fn short_host_list_places_footer_immediately_after_table() {
    let mut m = model();
    let output = screen(&mut m, 120, 40);
    let rows: Vec<_> = output.lines().collect();
    let last_host = rows.iter().position(|line| line.contains("beta")).unwrap();
    let table_bottom = rows.iter().rposition(|line| line.contains('╰')).unwrap();
    let footer = rows
        .iter()
        .position(|line| line.contains("↑/↓: select"))
        .unwrap();
    assert_eq!(
        table_bottom,
        last_host + 1,
        "table must end after the final host"
    );
    assert_eq!(footer, table_bottom + 1, "help follows the table directly");
    assert!(rows[footer + 2..].iter().all(|row| row.trim().is_empty()));
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
