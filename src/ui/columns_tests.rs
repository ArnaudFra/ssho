use super::tests::{key, screen, typing};
use super::*;
use serde_json::json;

fn model(columns: serde_json::Value) -> Model {
    let preferences = serde_json::from_value(json!({"host_columns": columns})).unwrap();
    Model::new(
        vec![Host {
            name: "héros-東京".into(),
            hostname: "declared.example".into(),
            user: "declared".into(),
            port: "2200".into(),
            ..Host::default()
        }],
        preferences,
        BTreeMap::new(),
        false,
    )
}
#[test]
fn columns_preferences_default_null_and_invalid() {
    for value in [json!({}), json!({"host_columns":null})] {
        let config: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap()["host_columns"],
            json!(["name", "hostname", "tags", "last_attempt"])
        );
    }
    for value in [
        json!([]),
        json!(["user"]),
        json!(["user", "name"]),
        json!(["name", "name"]),
        json!(["name", "unknown"]),
    ] {
        let error = serde_json::from_value::<AppConfig>(json!({"host_columns":value})).unwrap_err();
        assert!(error.to_string().contains("host_columns"), "{error}");
    }
}
#[test]
fn columns_custom_order_effective_secondary_and_empty_values() {
    let mut m = model(json!(["name", "port", "user", "hostname"]));
    m.search_focus = false;
    m.effective.insert(
        "héros-東京".into(),
        Ok(runtime::effective::EffectiveConfig::parse(
            "hostname native.example\nuser native-secondary\nport 4321\n",
        )
        .unwrap()),
    );
    let s = screen(&mut m, 120, 40);
    assert!(s.find("Port").unwrap() < s.find("User").unwrap());
    assert!(s.find("User").unwrap() < s.find("Hostname").unwrap());
    assert!(s.contains("4321") && s.contains("native-secondary") && !s.contains("declared"));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("héros-東京".into(), None)
    );
    m.effective.clear();
    let declared = screen(&mut m, 120, 40);
    assert!(declared.contains("declared") && declared.contains("2200"));
    m.all[0].user.clear();
    m.all[0].port.clear();
    assert!(screen(&mut m, 120, 40).contains('—'));
}
#[test]
fn columns_selector_search_draft_cancel_and_small_screens() {
    let mut m = model(json!(["name", "user", "port"]));
    typing(&mut m, "v");
    assert_eq!(m.query.value, "v");
    assert!(matches!(m.view, View::List));
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::Esc));
    let before = m.preferences.clone();
    m.key(key(KeyCode::Char('v')));
    for (w, h) in [(28, 8), (32, 12), (80, 24), (120, 40)] {
        let s = screen(&mut m, w, h);
        assert!(s.contains("Columns"), "{w}x{h}: {s}");
        assert!(s.contains("Enter"), "{w}x{h}: {s}");
    }
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Char(' ')));
    assert_eq!(m.preferences, before);
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    assert_eq!(m.preferences, before);
}
#[test]
fn columns_name_only_and_one_optional_remain_visible_without_cursor_overflow() {
    for cols in [json!(["name"]), json!(["name", "port"])] {
        for (w, h) in [(28, 8), (32, 12), (80, 24), (120, 40)] {
            let mut m = model(cols.clone());
            let mut t = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            t.draw(|f| render::draw(f, &mut m)).unwrap();
            let cursor = t.get_cursor_position().unwrap();
            assert!(cursor.x < w && cursor.y < h);
            assert!(t
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.symbol() == "h"
                    && cell.bg == ratatui::style::Color::Rgb(0, 173, 216)
                    && cell.fg == ratatui::style::Color::Black));
            assert!(screen(&mut m, w, h).replace(' ', "").contains("héros-東京"));
        }
    }
}

#[test]
fn columns_layout_preserves_legacy_exact_widths_and_custom_priority() {
    use HostColumn::*;
    let default = crate::storage::default_host_columns();
    for (width, widths) in [
        (0, vec![0]),
        (36, vec![36]),
        (37, vec![16, 20]),
        (64, vec![28, 35]),
        (65, vec![25, 23, 15]),
        (94, vec![36, 34, 22]),
        (95, vec![23, 27, 21, 21]),
        (120, vec![30, 35, 26, 26]),
    ] {
        assert_eq!(
            columns::layout(&default, width)
                .iter()
                .map(|(_, w)| *w)
                .collect::<Vec<_>>(),
            widths,
            "{width}"
        );
    }
    let custom = [Name, Port, User, Hostname, Tags, LastAttempt];
    for width in 0..200 {
        let plan = columns::layout(&custom, width);
        assert_eq!(
            plan.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            custom[..plan.len()]
        );
        assert_eq!(
            plan.iter().map(|(_, w)| w).sum::<u16>() + plan.len() as u16 - 1,
            width
        );
    }
    assert_eq!(columns::layout(&custom, 17), vec![(Name, 17)]);
    assert_eq!(columns::layout(&custom, 18), vec![(Name, 12), (Port, 5)]);
    assert_eq!(
        columns::layout(&[Name, Hostname, Port], 28),
        vec![(Name, 28)]
    );
}

#[test]
fn columns_selector_toggle_reorder_name_lock_reset_and_hidden_width() {
    use HostColumn::*;
    let mut m = model(json!(["name", "user", "port"]));
    m.search_focus = false;
    m.key(key(KeyCode::Char('v')));
    m.key(key(KeyCode::Char(' '))); // Name cannot be disabled.
    m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)); // Name cannot move.
    m.key(key(KeyCode::Down));
    m.key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)); // Cannot cross Name.
    m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft, [Name, Port, User]);
    assert_eq!(editor.selected, 2);
    let small = screen(&mut m, 28, 8);
    assert!(small.contains("hidden: width"), "{small}");
    m.key(key(KeyCode::Char(' '))); // remove User
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft, [Name, Port]);
    m.key(key(KeyCode::Char(' '))); // append User again
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft, [Name, Port, User]);
    m.key(key(KeyCode::Char('r')));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Columns(crate::storage::default_host_columns())
    );
    m.key(key(KeyCode::Esc));
    m.key(key(KeyCode::F(2)));
    for _ in 0..26 {
        m.key(key(KeyCode::Down));
    }
    m.key(key(KeyCode::Enter));
    assert!(matches!(m.view, View::Columns(_)));
}

fn secondary_model(config: &Path) -> Model {
    let mut m = model(json!(["name", "user", "port", "hostname"]));
    m.all = ["primary", "secondary"]
        .into_iter()
        .map(|name| Host {
            name: name.into(),
            aliases: vec!["primary".into(), "secondary".into()],
            source_file: config.into(),
            line_number: 1,
            user: "declared".into(),
            port: "2200".into(),
            ..Host::default()
        })
        .collect();
    m.preferences.group_aliases = true;
    m.preferences.group_mode = GroupMode::Files;
    m.source_filter = Some(config.into());
    for (name, user, port) in [
        ("primary", "native-primary", 22),
        ("secondary", "native-secondary", 4321),
    ] {
        m.effective.insert(
            name.into(),
            Ok(runtime::effective::EffectiveConfig::parse(&format!(
                "hostname {name}.example\nuser {user}\nport {port}\n"
            ))
            .unwrap()),
        );
    }
    m.history.insert(
        "secondary".into(),
        ConnectionInfo {
            last_connect: Utc::now(),
            connect_count: 3,
            ..ConnectionInfo::default()
        },
    );
    m.statuses.insert(
        "secondary".into(),
        PingResult {
            name: "secondary".into(),
            method: AvailabilityMethod::Ssh,
            status: PingStatus::Online,
            duration: Duration::ZERO,
            error: None,
        },
    );
    m.query.set("user:native-secondary");
    m.filter(true);
    m.search_focus = false;
    m
}

#[test]
fn columns_save_adopts_external_preferences_and_renews_method_without_retargeting() {
    use HostColumn::*;
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("ssh-config");
    // Resolution fails before any connection, agent, key or ping access.
    std::fs::write(&config, "Host primary secondary\n HostName\n").unwrap();
    let mut m = secondary_model(&config);
    let host = m.selected_host().unwrap();
    assert_eq!(host.name, "secondary");
    let shown = screen(&mut m, 120, 40);
    assert!(shown.contains("native-secondary") && shown.contains("4321"));
    assert!(!shown.contains("native-primary"));
    let mut external = serde_json::to_value(&m.preferences).unwrap();
    external["availability_method"] = json!("icmp");
    external["future"] = json!({"keep":true});
    external["ssh_command"] = json!(["fixture-ssh", "literal ; arg"]);
    std::fs::write(
        temp.path().join("config.json"),
        serde_json::to_vec(&external).unwrap(),
    )
    .unwrap();
    let history = m.history.clone();
    let rows = m.rows.clone();
    m.key(key(KeyCode::Char('v')));
    m.key(key(KeyCode::Down));
    m.key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    let action = m.key(key(KeyCode::Enter));
    assert_eq!(action, Action::Columns(vec![Name, Port, User, Hostname]));
    let inspector = runtime::effective::Inspector::default();
    let (old_sender, mut receiver) = std::sync::mpsc::channel();
    apply_preference_action(
        &inspector,
        &mut m,
        temp.path(),
        &config,
        &mut receiver,
        &action,
    )
    .unwrap();
    assert!(old_sender
        .send(runtime::effective::Inspection {
            name: "secondary".into(),
            effective: Err("stale".into()),
            ping: None
        })
        .is_err());
    assert_eq!(m.preferences.availability_method, AvailabilityMethod::Icmp);
    assert!(m
        .statuses
        .values()
        .all(|r| r.method == AvailabilityMethod::Icmp && r.status == PingStatus::Connecting));
    assert_eq!(m.selected_host().unwrap(), host);
    assert_eq!(m.query.value, "user:native-secondary");
    assert_eq!(m.source_filter.as_deref(), Some(config.as_path()));
    assert_eq!(m.rows, rows);
    assert_eq!(m.history, history);
    assert_eq!(m.preferences.ssh_command, ["fixture-ssh", "literal ; arg"]);
    assert!(matches!(m.view, View::List));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["future"], json!({"keep":true}));
    assert_eq!(
        saved["host_columns"],
        json!(["name", "port", "user", "hostname"])
    );
    for result in receiver {
        if let Some(ping) = result.ping {
            assert_eq!(ping.method, AvailabilityMethod::Icmp);
        }
    }
}

#[test]
fn columns_save_failure_keeps_draft_preferences_identity_and_receiver() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("config.json")).unwrap();
    let mut m = model(json!(["name", "user", "port"]));
    m.search_focus = false;
    let preferences = m.preferences.clone();
    let rows = m.rows.clone();
    m.key(key(KeyCode::Char('v')));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Char(' ')));
    let action = m.key(key(KeyCode::Enter));
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
    let View::Columns(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.draft, [HostColumn::Name, HostColumn::Port]);
    sender
        .send(runtime::effective::Inspection {
            name: "still-current".into(),
            effective: Err("retained".into()),
            ping: None,
        })
        .unwrap();
    assert_eq!(receiver.try_recv().unwrap().name, "still-current");
    assert!(screen(&mut m, 80, 24).contains("config.json"));
    std::fs::remove_dir(temp.path().join("config.json")).unwrap();
    apply_preference_action(
        &inspector,
        &mut m,
        temp.path(),
        &temp.path().join("unused"),
        &mut receiver,
        &action,
    )
    .unwrap();
    assert!(matches!(m.view, View::List));
}

#[test]
fn columns_external_adoption_via_alias_and_group_save_renders_fresh_layout() {
    use HostColumn::*;
    for action in [Action::ToggleAliases, Action::GroupMode(GroupMode::Files)] {
        let temp = tempfile::tempdir().unwrap();
        let mut m = model(json!(["name"]));
        m.search_focus = false;
        let mut external = m.preferences.clone();
        external.host_columns = vec![Name, Port, User];
        external.save(temp.path()).unwrap();
        let inspector = runtime::effective::Inspector::default();
        let (_, mut receiver) = std::sync::mpsc::channel();
        apply_preference_action(
            &inspector,
            &mut m,
            temp.path(),
            &temp.path().join("unused"),
            &mut receiver,
            &action,
        )
        .unwrap();
        let shown = screen(&mut m, 120, 40);
        assert!(
            shown.contains("Port")
                && shown.contains("User")
                && shown.contains("2200")
                && shown.contains("declared")
        );
    }
}

#[test]
fn columns_group_counter_stays_in_name_and_save_keeps_collapsed_selection() {
    let temp = tempfile::tempdir().unwrap();
    let mut m = secondary_model(&temp.path().join("ssh"));
    m.preferences.save(temp.path()).unwrap();
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Char(' ')));
    let rows = m.rows.clone();
    let group = m.selected_group().unwrap();
    let mut t = Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    t.draw(|f| render::draw(f, &mut m)).unwrap();
    let buffer = t.backend().buffer();
    let name_end = 2 + columns::layout(&m.preferences.host_columns, 116)[0].1 + 1;
    let mut found = false;
    for y in 0..40 {
        let line = (0..120)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>();
        if line.contains("(1)") {
            found = true;
            assert!(
                (name_end..118).all(|x| buffer[(x, y)].symbol() == " "),
                "{line}"
            );
        }
    }
    assert!(found);
    m.key(key(KeyCode::Char('v')));
    let action = m.key(key(KeyCode::Enter));
    let inspector = runtime::effective::Inspector::default();
    let (_, mut receiver) = std::sync::mpsc::channel();
    apply_preference_action(
        &inspector,
        &mut m,
        temp.path(),
        &temp.path().join("unused"),
        &mut receiver,
        &action,
    )
    .unwrap();
    assert_eq!(m.rows, rows);
    assert_eq!(m.selected_group(), Some(group));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
}

#[test]
fn columns_save_rejects_invalid_programmatic_preferences_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let preferences = AppConfig {
        host_columns: vec![HostColumn::Name, HostColumn::Name],
        ..AppConfig::default()
    };
    assert!(preferences
        .save(temp.path())
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    assert!(!temp.path().join("config.json").exists());
}

#[test]
fn columns_sort_indicator_tracks_last_attempt_after_custom_reorder() {
    let mut m = model(json!(["name", "port", "last_attempt", "user"]));
    m.search_focus = false;
    let screen_name = screen(&mut m, 120, 40);
    assert!(screen_name.contains("Name ↓"));
    m.key(key(KeyCode::Char('r')));
    let screen_recent = screen(&mut m, 120, 40);
    assert!(screen_recent.contains("Last Attempt ↓") && !screen_recent.contains("Name ↓"));
}
