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
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.search_focus);
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
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(terminal.backend().buffer()[position].symbol(), "s");
    assert_eq!(terminal.backend().buffer()[position].fg, Color::DarkGray);
    for _ in 0..7 {
        m.key(key(KeyCode::Tab));
    }
    terminal.draw(|frame| render::draw(frame, &mut m)).unwrap();
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(terminal.backend().buffer()[position].symbol(), "p");
    assert_eq!(terminal.backend().buffer()[position].fg, Color::DarkGray);
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
    assert_eq!(m.all[m.visible[0]].name, "beta");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
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
    assert!(matches!(m.view, View::Info(_)));
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
    assert!(matches!(m.view, View::Files { .. }));
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
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Up));
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
        assert!(output.contains("Ajouter"));
        assert!(output.contains("Aide"));
        for _ in 0..3 {
            m.key(key(KeyCode::Down));
            assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
            assert!(screen(&mut m, 100, 30).contains("Ajouter"));
        }
        m.key(key(KeyCode::Down));
        for (width, height) in [(40, 12), (10, 5), (1, 1)] {
            screen(&mut m, width, height);
        }
        m.key(key(KeyCode::Enter));
        assert!(matches!(m.view, View::Help));
        m.key(key(KeyCode::Esc));
        m.key(key(KeyCode::F(2)));
        assert_eq!(m.key(key(KeyCode::Enter)), Action::Add(String::new()));
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
        },
    );
    m.key(key(KeyCode::F(2)));
    assert!(screen(&mut m, 100, 30).contains("Tunnel"));
    m.key(key(KeyCode::Up));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    let View::Forward(form) = &m.view else {
        panic!("palette must open the existing tunnel form");
    };
    assert_eq!(form.host, "beta");
    assert_eq!(form.kind, 2);
    assert_eq!(form.inputs[1].value, "1080");
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(2)));
    for _ in 0..3 {
        m.key(key(KeyCode::Down));
    }
    assert_eq!(m.key(key(KeyCode::Enter)), Action::Ping);
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
        "h: help",
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
        (70, 18, "1-6", "35-40", 6),
        (80, 18, "1-6", "35-40", 6),
        (120, 35, "1-23", "18-40", 23),
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
        assert_eq!(output.contains("Last Login"), columns >= 4, "{output}");
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
        "h: help",
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
        "Hidden",
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

    for (has_host, selected, label) in [
        (true, 0, "Ajouter"),
        (true, 2, "Tunnel"),
        (false, 1, "Modifier"),
    ] {
        let mut m = if has_host {
            model()
        } else {
            Model::new(vec![], AppConfig::default(), BTreeMap::new(), false)
        };
        m.key(key(KeyCode::F(2)));
        for _ in 0..selected {
            m.key(key(KeyCode::Down));
        }
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
            assert!(text.contains("Unavailable: no host"));
            selected_text.push("Unavailable: no host");
        }
        for text in selected_text {
            let first = text.chars().next().unwrap().to_string();
            let start = row.iter().position(|cell| cell.symbol() == first).unwrap();
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
        let output = screen(&mut m, 150, 40);
        assert!(output.contains("Détails"));
        for _ in 0..5 {
            m.key(key(KeyCode::Down));
        }
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
        output.contains("Last Login"),
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
    assert!(bottom.contains("Tags: work"));
    for _ in 0..120 {
        m.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
    }
    assert!(screen(&mut m, 120, 20).contains("Host: alpha"));
    for _ in 0..120 {
        m.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
    }
    assert!(screen(&mut m, 120, 20).contains("Tags: work"));
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
    for text in ["fleet39-", "40/41", "40/40", "18-40"] {
        assert!(output.contains(text), "missing {text}: {output}");
    }
    assert!(
        !output.contains("Last Login"),
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
        assert!(screen(&mut m, 120, 20).contains("Host: alpha"));
        m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        let output = screen(&mut m, 120, 20);
        assert!(!output.contains("Host: alpha"));
        assert!(output.contains("Hostname/IP: a.example"));
        assert_eq!(m.selected, 0, "Alt+Down scrolls details, not the list");
        m.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert!(screen(&mut m, 120, 20).contains("Host: alpha"));
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
    assert!(m.query.value.is_empty());
    assert!(m.search_focus);
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    assert!(!m.search_focus);
    assert_eq!(m.key(key(KeyCode::Esc)), Action::Quit);
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
