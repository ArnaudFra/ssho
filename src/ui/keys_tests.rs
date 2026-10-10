use super::tests::{key, screen};
use super::*;
#[test]
fn picker_restores_dirty_fields_carets_and_parent_then_changes_only_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    m.open_view(View::Host(Box::new(HostForm::add(
        "draft".into(),
        tmp.path().join("config"),
    ))));
    if let View::Host(form) = &mut m.view {
        form.inputs[1].set("unsaved user");
        form.inputs[1].cursor = 3;
        form.focus = 1;
    }
    assert_eq!(m.key(key(KeyCode::F(9))), Action::OpenKeys(true));
    keys::open_view(&mut m, true);
    m.key(key(KeyCode::Esc));
    let View::Host(form) = &m.view else {
        panic!("lost parent")
    };
    assert_eq!(form.inputs[1].value, "unsaved user");
    assert_eq!(form.inputs[1].cursor, 3);
    assert_eq!(form.focus, 1);
    assert_eq!(m.parents.len(), 1);
    keys::open_view(&mut m, true);
    m.keys.entries = vec![runtime::keys::Entry {
        private: tmp.path().join("new key"),
        public: None,
        usages: vec![],
        diagnostic: String::new(),
    }];
    m.key(key(KeyCode::Enter));
    let View::Host(form) = &m.view else {
        panic!("lost form")
    };
    assert_eq!(
        form.inputs[4].value,
        tmp.path().join("new key").display().to_string()
    );
    assert_eq!(form.inputs[1].value, "unsaved user");
    assert_eq!(form.inputs[1].cursor, 3);
    assert_eq!(form.focus, 1);
}
#[test]
fn search_k_is_text_palette_and_global_picker_remain_available() {
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), true);
    m.key(key(KeyCode::Char('K')));
    assert_eq!(m.query.value, "K");
    assert!(matches!(m.view, View::List));
    m.search_focus = false;
    assert_eq!(m.key(key(KeyCode::Char('K'))), Action::OpenKeys(false));
    m.key(key(KeyCode::F(2)));
    assert!(matches!(m.view, View::Actions { .. }));
    for _ in 0..PaletteAction::ALL.len() - 1 {
        m.key(key(KeyCode::Down));
    }
    assert!(screen(&mut m, 100, 40).contains("Keys"));
}
#[test]
fn generation_error_keeps_path_comment_and_form_parent() {
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    keys::open_view(&mut m, true);
    m.key(key(KeyCode::Char('n')));
    m.paste("/missing parent/key");
    m.key(key(KeyCode::Tab));
    m.paste("my comment");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::GenerateKey);
    m.error = "native failure".into();
    let text = screen(&mut m, 100, 30);
    assert!(text.contains("my comment"));
    assert!(text.contains("native failure"));
}
#[test]
fn attach_review_keeps_original_until_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("config");
    let original="Host primary secondary\n HostName invalid.test\n IdentityFile old\n IdentityFile other\n # Tags prod\n";
    std::fs::write(&root, original).unwrap();
    let hosts = config::load_report(&root).unwrap().hosts;
    let mut m = Model::new(hosts, AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    m.selected = 1;
    keys::open_view(&mut m, false);
    m.keys.entries = vec![runtime::keys::Entry {
        private: tmp.path().join("new key"),
        public: None,
        usages: vec![],
        diagnostic: String::new(),
    }];
    keys::attach(&mut m, &root).unwrap();
    assert_eq!(std::fs::read_to_string(&root).unwrap(), original);
    assert!(matches!(m.view, View::Changes(_)));
    let action = changes::apply(&mut m, &tmp.path().join("app")).unwrap();
    assert!(matches!(action, Action::Save(Some(_), _, _)));
    let host = config::find_host(&root, "secondary").unwrap();
    assert_eq!(
        host.identity,
        tmp.path().join("new key").display().to_string()
    );
    assert_eq!(host.extra_identities, ["old", "other"]);
    assert_eq!(host.aliases, ["primary", "secondary"]);
}
#[cfg(unix)]
#[test]
fn native_foreground_guard_preserves_generation_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("config");
    std::fs::write(&root, "Host fixture\n HostName invalid.test\n").unwrap();
    let host = config::find_host(&root, "fixture").unwrap();
    let mut m = Model::new(
        vec![host.clone()],
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let target = runtime::batch::Target::freeze(&root, &host).unwrap();
    m.batch.manager = runtime::batch::Manager::with_program("/nonexistent-test-native".into());
    m.batch
        .manager
        .start(vec![target], "fixture".into(), Default::default())
        .unwrap();
    keys::open_view(&mut m, false);
    m.key(key(KeyCode::Char('n')));
    m.paste("/tmp/never-created");
    assert!(keys::native(&mut m, &Action::GenerateKey, &root)
        .unwrap_err()
        .to_string()
        .contains("Batch active"));
    let View::Keys(editor) = &m.view else {
        panic!("lost keys")
    };
    let keys::Mode::Generate { path, .. } = &editor.mode else {
        panic!("lost generation")
    };
    assert_eq!(path.value, "/tmp/never-created");
}
#[test]
fn deployment_review_scrolls_on_small_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let public = tmp.path().join("key.pub");
    std::fs::write(&public,"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\n").unwrap();
    let reviewed = runtime::keys::review_public(&public, Path::new("ssh-keygen")).unwrap();
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    keys::open_view(&mut m, false);
    if let View::Keys(editor) = &mut m.view {
        editor.mode = keys::Mode::Deploy {
            public: reviewed,
            private: tmp.path().join("key"),
            alias: "exact-secondary-alias".into(),
        };
    }
    screen(&mut m, 32, 12);
    m.key(key(KeyCode::PageDown));
    assert!(m.scroll > 0);
}

fn entry(path: &str) -> runtime::keys::Entry {
    runtime::keys::Entry {
        private: path.into(),
        public: None,
        usages: vec!["alias-one".into(), "alias-two".into()],
        diagnostic: "missing public sibling; repair this diagnostic-end".into(),
    }
}

#[test]
fn compact_selection_and_focused_input_remain_visible() {
    use ratatui::{
        backend::{Backend, TestBackend},
        Terminal,
    };
    for (w, h) in [(32, 12), (32, 8), (28, 8)] {
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        keys::open_view(&mut m, false);
        m.keys.entries = vec![
            entry("/long/common/prefix/first-key"),
            entry("/long/common/prefix/second-key"),
        ];
        m.key(key(KeyCode::Down));
        let text = screen(&mut m, w, h);
        assert!(
            text.contains("second-key"),
            "selected key invisible at {w}x{h}: {text}"
        );
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| render::draw(f, &mut m)).unwrap();
        let selected_cell = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .find(|cell| cell.symbol() == "s" && cell.bg == ratatui::style::Color::Cyan)
            .expect("selected row must be highlighted");
        assert_eq!(selected_cell.fg, ratatui::style::Color::Black);

        m.key(key(KeyCode::Char('n')));
        m.paste("/long/common/prefix/destinationZ");
        for focus in 0..2 {
            if focus == 1 {
                m.key(key(KeyCode::Tab));
                m.paste("long-commentZ");
            }
            m.key(key(KeyCode::Left));
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| render::draw(f, &mut m)).unwrap();
            let pos = terminal.backend_mut().get_cursor_position().unwrap();
            assert_eq!(
                terminal.backend().buffer()[pos].symbol(),
                "Z",
                "focused caret at {w}x{h}, field {focus}"
            );
        }
    }
}

#[test]
fn compact_details_reach_full_paths_usages_diagnostic_and_native_recovery() {
    for (w, h) in [(32, 12), (32, 8), (28, 8)] {
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        keys::open_view(&mut m, false);
        let path = "/very/long/common/prefix/with/unbroken-components/selected-key";
        m.keys.entries = vec![entry(path)];
        let tmp = tempfile::tempdir().unwrap();
        let public = tmp.path().join("reviewed.pub");
        std::fs::write(&public, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\n").unwrap();
        let reviewed = runtime::keys::review_public(&public, Path::new("ssh-keygen")).unwrap();
        let fingerprint = reviewed.fingerprint.clone();
        m.keys.entries[0].public = Some(reviewed);
        m.error = "Private /very/long/private/destination preserved. Public /very/long/public/destination.pub occupied. Cause: permission denied. Manual recovery: inspect both files before retrying. recovery-end".into();
        m.key(key(KeyCode::F(4)));
        let mut seen = String::new();
        let mut previous = String::new();
        for _ in 0..100 {
            let current = screen(&mut m, w, h);
            if current == previous {
                seen.extend(current.lines().skip(1).take(h as usize - 2));
                break;
            }
            seen.push_str(current.lines().nth(1).unwrap());
            previous = current;
            m.key(key(KeyCode::Down));
        }
        let seen = seen.split_whitespace().collect::<String>();
        for value in [
            path,
            &fingerprint,
            &m.error,
            "alias-one",
            "alias-two",
            "diagnostic-end",
        ] {
            assert!(
                seen.contains(&value.split_whitespace().collect::<String>()),
                "{value} unreachable at {w}x{h}: {seen}"
            );
        }
        m.key(key(KeyCode::Esc));
        assert!(
            matches!(m.view, View::Keys(_)),
            "details Esc must preserve Keys"
        );
    }
}

#[test]
fn refresh_preserves_picker_target_when_earlier_keys_insert_or_disappear() {
    for entries in [
        vec![entry("/0"), entry("/a"), entry("/b")],
        vec![entry("/b")],
    ] {
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        m.open_view(View::Host(Box::new(HostForm::add(
            "draft".into(),
            "/config".into(),
        ))));
        keys::open_view(&mut m, true);
        m.keys.entries = vec![entry("/a"), entry("/b")];
        m.key(key(KeyCode::Down));
        keys::replace_entries(&mut m, entries);
        m.key(key(KeyCode::Enter));
        let View::Host(form) = &m.view else {
            panic!("lost picker parent")
        };
        assert_eq!(form.inputs[4].value, "/b");
    }
}

#[test]
fn refresh_suspended_picker_preserves_identity_and_requires_choice_if_missing() {
    for disappears in [false, true] {
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        m.open_view(View::Host(Box::new(HostForm::add(
            "draft".into(),
            "/config".into(),
        ))));
        keys::open_view(&mut m, true);
        m.keys.entries = vec![entry("/a"), entry("/b")];
        m.key(key(KeyCode::Down));
        m.open_view(View::Help);
        keys::replace_entries(
            &mut m,
            if disappears {
                vec![entry("/c")]
            } else {
                vec![entry("/0"), entry("/a"), entry("/b")]
            },
        );
        m.back();
        if disappears {
            assert!(screen(&mut m, 100, 30).contains("disappeared"));
            m.key(key(KeyCode::Enter));
            assert!(
                matches!(m.view, View::Keys(_)),
                "missing key silently retargeted"
            );
            m.key(key(KeyCode::Down));
        }
        m.key(key(KeyCode::Enter));
        let View::Host(form) = &m.view else {
            panic!("lost picker parent")
        };
        assert_eq!(form.inputs[4].value, if disappears { "/c" } else { "/b" });
    }
}

#[test]
fn refresh_and_details_preserve_generation_draft_and_deployment_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let public = tmp.path().join("b.pub");
    std::fs::write(&public, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\n").unwrap();
    let reviewed = runtime::keys::review_public(&public, Path::new("ssh-keygen")).unwrap();
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    keys::open_view(&mut m, false);
    m.keys.entries = vec![entry("/a"), entry("/b")];
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Char('n')));
    m.paste("/outside/generated");
    m.key(key(KeyCode::Tab));
    m.paste("unsaved comment");
    m.key(key(KeyCode::Left));
    let draft = format!(
        "{:?}",
        match &m.view {
            View::Keys(editor) => &editor.mode,
            _ => panic!(),
        }
    );
    m.key(key(KeyCode::F(4)));
    keys::replace_entries(&mut m, vec![entry("/0"), entry("/a"), entry("/b")]);
    m.key(key(KeyCode::PageDown));
    screen(&mut m, 28, 8);
    m.key(key(KeyCode::Esc));
    let View::Keys(editor) = &mut m.view else {
        panic!()
    };
    assert_eq!(format!("{:?}", editor.mode), draft);
    editor.mode = keys::Mode::Deploy {
        public: reviewed,
        private: "/b".into(),
        alias: "exact-secondary".into(),
    };
    let snapshot = format!("{:?}", editor.mode);
    m.open_view(View::Help);
    keys::replace_entries(&mut m, vec![entry("/0")]);
    m.back();
    let View::Keys(editor) = &m.view else {
        panic!()
    };
    assert_eq!(format!("{:?}", editor.mode), snapshot);
    assert_eq!(m.key(key(KeyCode::Enter)), Action::DeployKey);
}
