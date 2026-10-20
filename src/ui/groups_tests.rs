use super::tests::{key, screen};
use super::*;

fn host(name: &str, source: &str, tags: &[&str], inherited: &[&str]) -> Host {
    Host {
        name: name.into(),
        hostname: format!("{name}.example"),
        source_file: source.into(),
        line_number: 1,
        aliases: vec![name.into()],
        tags: tags.iter().map(|s| (*s).into()).collect(),
        file_tags: inherited.iter().map(|s| (*s).into()).collect(),
        ..Host::default()
    }
}
fn model(mode: &str) -> Model {
    Model::new(
        vec![
            host("alpha", "/one/config", &["own", "other"], &["inherited"]),
            host("beta", "/one/config", &[], &["inherited", "other"]),
            host("gamma", "/two/config", &[], &[]),
        ],
        serde_json::from_value(
            serde_json::json!({"group_mode":mode,"focus_search_on_start":false}),
        )
        .unwrap(),
        BTreeMap::new(),
        false,
    )
}
fn query(m: &mut Model, text: &str) {
    m.query.set(text);
    m.filter(true);
}

#[test]
fn groups_preferences_default_null_and_strict_enum() {
    for input in [
        serde_json::json!({}),
        serde_json::json!({"group_mode":null}),
    ] {
        let config: AppConfig = serde_json::from_value(input).unwrap();
        assert_eq!(serde_json::to_value(config).unwrap()["group_mode"], "flat");
    }
    for mode in ["flat", "files", "file_tags"] {
        let config: AppConfig =
            serde_json::from_value(serde_json::json!({"group_mode":mode})).unwrap();
        assert_eq!(serde_json::to_value(config).unwrap()["group_mode"], mode);
    }
    for invalid in [serde_json::json!(true), serde_json::json!("folders")] {
        assert!(
            serde_json::from_value::<AppConfig>(serde_json::json!({"group_mode":invalid})).is_err()
        );
    }
}

#[test]
fn groups_tree_primary_tags_distinct_paths_no_duplicate_hosts() {
    let mut m = model("file_tags");
    let text = screen(&mut m, 120, 30);
    for label in ["/one/config", "/two/config", "own", "inherited", "Sans tag"] {
        assert!(text.contains(label), "missing {label}: {text}");
    }
    // Two file headers + three tags + three concrete hosts.
    m.key(key(KeyCode::End));
    assert_eq!(m.selected, 7);
    assert_eq!(m.selected_host().unwrap().name, "gamma");
    assert_eq!(m.visible.len(), 3);
    query(&mut m, "tag:other");
    assert_eq!(m.visible.len(), 2);
    assert!(m.selected_host().is_some());
}

#[test]
fn groups_headers_block_all_host_actions_and_palette() {
    let mut m = model("files");
    m.key(key(KeyCode::Home));
    assert!(m.selected_host().is_none());
    for code in ['e', 'd', 'm', 'f', 'N', '*', 'D', 'p'] {
        assert_eq!(m.key(key(KeyCode::Char(code))), Action::None);
        assert!(matches!(m.view, View::List));
    }
    for function in [3, 6] {
        assert_eq!(m.key(key(KeyCode::F(function))), Action::None);
        assert!(matches!(m.view, View::List));
    }
    for action in [
        PaletteAction::Edit,
        PaletteAction::Tunnel,
        PaletteAction::Test,
        PaletteAction::Favorite,
        PaletteAction::Note,
        PaletteAction::Duplicate,
        PaletteAction::Profiles,
        PaletteAction::ChooseAlias,
    ] {
        assert!(!action.enabled(m.selected_host().is_some()));
        assert_eq!(m.activate_action(action), Action::None);
    }
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(m.selected_host().is_none());
}

#[test]
fn groups_navigation_collapse_preserves_raw_membership_and_counts_rows() {
    let mut m = model("file_tags");
    m.key(key(KeyCode::Home));
    let visible = m.visible.clone();
    let matching = m.matching.clone();
    m.key(key(KeyCode::Left)); // close file
    m.key(key(KeyCode::End));
    assert_eq!(m.selected, 3); // closed file + other file/tag/host
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Right)); // open file
    assert_eq!(m.selected, 0);
    m.key(key(KeyCode::Right)); // first tag
    assert_eq!(m.selected, 1);
    m.key(key(KeyCode::Right)); // host
    assert!(m.selected_host().is_some());
    m.key(key(KeyCode::Left)); // parent tag
    assert_eq!(m.selected, 1);
    m.key(key(KeyCode::Left)); // close tag
    m.key(key(KeyCode::Left)); // parent file
    assert_eq!(m.selected, 0);
    m.key(key(KeyCode::Char(' '))); // close file
    assert_eq!(m.visible, visible);
    assert_eq!(m.matching, matching);
    m.page = 2;
    m.key(key(KeyCode::PageDown));
    assert_eq!(m.selected, 2);
    m.key(key(KeyCode::PageUp));
    assert_eq!(m.selected, 0);
}

#[test]
fn groups_query_opens_parents_selects_host_and_restores_manual_collapse() {
    let mut m = model("files");
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    query(&mut m, "alpha");
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("alpha".into(), None)
    );
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Enter)); // temporary query collapse
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None); // expand again, never SSH
    query(&mut m, "");
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Down));
    assert!(m.selected_host().is_none()); // next file, first stays closed
    m.search_focus = true;
    m.key(key(KeyCode::Char('g')));
    m.key(key(KeyCode::Char(' ')));
    let cursor = m.query.cursor;
    m.key(key(KeyCode::Left));
    assert_eq!(m.query.cursor + 1, cursor);
    m.key(key(KeyCode::Right));
    assert_eq!(m.query.cursor, cursor);
    assert_eq!(m.query.value, "g ");
}

#[test]
fn groups_reload_and_ssh_preserve_header_identity_and_collapse() {
    let mut m = model("files");
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let mut all = m.all.clone();
    all.reverse();
    m.replace_hosts(all, None);
    assert_eq!(m.selected, 0);
    assert!(m.selected_host().is_none());
    m.resume_connection(
        "alpha",
        Ok(37),
        History::load(tempfile::tempdir().unwrap().path()),
    );
    m.key(key(KeyCode::Down));
    assert!(m.selected_host().is_none());
    m.key(key(KeyCode::Right));
    assert_eq!(m.selected_host().unwrap().name, "gamma");
}

#[test]
fn groups_saved_host_opens_allowed_ancestors_but_exact_source_wins() {
    let mut m = model("file_tags");
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let saved = m.all[0].clone();
    m.complete_mutation(
        m.all.clone(),
        &Action::Save(None, Box::new(saved.clone()), saved.source_file.clone()),
    );
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    m.source_filter = Some("/two/config".into());
    query(&mut m, "gamma");
    let before = m.query.value.clone();
    let mut hidden = saved.clone();
    hidden.tags.push("hidden".into());
    let mut all = m.all.clone();
    all[0] = hidden.clone();
    m.complete_mutation(
        all,
        &Action::Save(None, Box::new(hidden), saved.source_file),
    );
    assert_eq!(m.source_filter, Some("/two/config".into()));
    assert_eq!(m.query.value, before);
    assert!(!m.show_hidden);
    assert_eq!(m.selected_host().unwrap().name, "gamma");
}

#[test]
fn groups_picker_cancel_preserves_selection_and_palette_access() {
    let mut m = model("files");
    m.key(key(KeyCode::Char('g')));
    let before = m.selected;
    assert!(!matches!(m.view, View::List));
    m.key(key(KeyCode::Esc));
    assert_eq!(m.selected, before);
    assert!(PaletteAction::CATALOGUE
        .iter()
        .any(|a| a.label().contains("Group")));
}

#[test]
fn groups_preferences_fresh_write_failure_and_external_mode_preserve_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let mut m = model("files");
    query(&mut m, "beta");
    m.query.set("");
    m.filter(false);
    assert_eq!(m.selected_host().unwrap().name, "beta");
    std::fs::write(&path, r#"{"ssh_command":["fresh wrapper","literal space"],"group_aliases":true,"ping_on_start":false,"key_bindings":{"quit_keys":["x"],"future":8},"future":{"keep":true}}"#).unwrap();
    groups::open(&mut m);
    groups::save(&mut m, root.path(), GroupMode::FileTags).unwrap();
    assert_eq!(m.selected_host().unwrap().name, "beta");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["group_mode"], "file_tags");
    assert_eq!(saved["group_aliases"], true);
    assert_eq!(
        saved["ssh_command"],
        serde_json::json!(["fresh wrapper", "literal space"])
    );
    assert_eq!(saved["ping_on_start"], false);
    assert_eq!(saved["key_bindings"]["future"], 8);
    assert_eq!(saved["future"]["keep"], true);
    // A failed picker save keeps selection, collapse, settings and the picker visible.
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let rows = m.rows.clone();
    let preferences = m.preferences.clone();
    let selected = m.selected;
    groups::open(&mut m);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    m.error = groups::save(&mut m, root.path(), GroupMode::Flat)
        .unwrap_err()
        .to_string();
    assert_eq!(m.rows, rows);
    assert_eq!(m.preferences, preferences);
    assert_eq!(m.selected, selected);
    assert!(matches!(m.view, View::GroupMode { .. }));
    let text = screen(&mut m, 80, 12);
    assert!(text.contains("config.json"), "{text}");
    // The alias toggle rereads an externally changed group mode without interpreting a header as a host.
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, r#"{"group_mode":"flat"}"#).unwrap();
    m.back();
    aliases::toggle(&mut m, root.path()).unwrap();
    assert_eq!(m.preferences.group_mode, GroupMode::Flat);
    assert_eq!(m.selected_host().unwrap().name, "alpha");
    query(&mut m, "beta");
    std::fs::write(&path, r#"{"group_mode":"file_tags"}"#).unwrap();
    aliases::toggle(&mut m, root.path()).unwrap();
    assert_eq!(m.selected_host().unwrap().name, "beta");
}

#[test]
fn groups_alias_choices_async_membership_and_host_sort_are_independent_of_collapse() {
    let mut m = model("file_tags");
    let mut secondary = m.all[0].clone();
    secondary.name = "a".into();
    secondary.hostname = "alternate.example".into();
    secondary.aliases = vec!["alpha".into(), "a".into()];
    m.all[0].aliases = secondary.aliases.clone();
    m.all.push(secondary);
    m.preferences.group_aliases = true;
    query(&mut m, "alpha");
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    assert_eq!(m.selected_host().unwrap().name, "a");
    assert!(m.query.value.is_empty());
    assert_eq!(m.visible.len(), 3);
    assert_eq!(m.matching.len(), 4);
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let rows = m.rows.clone();
    let visible = m.visible.clone();
    m.recent = true;
    m.history.insert(
        "gamma".into(),
        ConnectionInfo {
            last_connect: Utc::now(),
            ..Default::default()
        },
    );
    m.apply_inspection(runtime::effective::Inspection {
        name: "a".into(),
        effective: Err("fixture".into()),
        ping: None,
    });
    assert_eq!(m.visible, visible);
    assert_eq!(m.rows, rows);
    m.key(key(KeyCode::Right));
    let names: Vec<_> = m
        .rows
        .iter()
        .filter_map(|row| match row {
            groups::Row::Host(i) => Some(m.all[*i].name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["beta", "a", "gamma"]); // primary tags, then ordered hosts
    query(&mut m, "a");
    assert_eq!(m.selected_host().unwrap().name, "beta"); // first matching host in displayed tag order
}

#[test]
fn groups_move_last_source_uses_fresh_hidden_destination_tags_and_delete_bounds() {
    let mut m = model("file_tags");
    let moved = m.all[2].clone();
    m.source_filter = Some(moved.source_file.clone());
    query(&mut m, "gamma");
    let mut all = m.all[..2].to_vec();
    let mut fresh = moved.clone();
    fresh.source_file = "/one/config".into();
    fresh.file_tags = vec!["hidden".into(), "destination".into()];
    all.push(fresh.clone());
    m.complete_mutation(all, &Action::MoveTo(moved, fresh.source_file.clone()));
    assert!(m.source_filter.is_none());
    assert!(m.show_hidden);
    assert_eq!(m.selected_host().unwrap(), fresh);
    let all = m.all[..2].to_vec();
    query(&mut m, "");
    m.complete_mutation(all, &Action::Delete(fresh));
    assert!(m.selected < m.rows.len());
    m.replace_hosts(vec![], None);
    assert_eq!((m.selected, m.offset), (0, 0));
    assert!(m.rows.is_empty());
}

#[test]
fn groups_narrow_viewport_counts_headers_and_selected_colors_and_path_identity() {
    use ratatui::{backend::TestBackend, style::Color, Terminal};
    let mut m = model("file_tags");
    for (width, height) in [(24, 12), (60, 12), (120, 24)] {
        m.key(key(KeyCode::Home));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render::draw(f, &mut m)).unwrap();
        let buffer = terminal.backend().buffer();
        let selected: Vec<_> = buffer
            .content
            .iter()
            .filter(|cell| cell.bg == Color::Rgb(0, 173, 216))
            .collect();
        assert!(!selected.is_empty());
        assert!(selected.iter().all(|cell| cell.fg == Color::Black));
        m.key(key(KeyCode::End));
        let text = screen(&mut m, width, height);
        assert!(text.contains("gamma"), "{text}");
        assert!(text.contains("groups"), "{text}");
        assert!(m.offset > 0 || height >= 24);
        assert!(m.page <= usize::from(height - 3));
    }
    // At compact widths paths with the same long suffix still need distinguishable headers.
    m.all = vec![
        host("one", "/first/very/long/shared/path/config", &[], &[]),
        host("two", "/second/very/long/shared/path/config", &[], &[]),
    ];
    m.preferences.group_mode = GroupMode::Files;
    m.filter(true);
    let text = screen(&mut m, 24, 20);
    let headers: Vec<_> = text.lines().filter(|line| line.contains('▼')).collect();
    assert_eq!(headers.len(), 2, "{text}");
    assert_ne!(
        headers[0], headers[1],
        "different source paths must remain distinguishable: {text}"
    );
}

#[test]
fn groups_compact_info_wraps_full_source_without_enabling_ssh_or_host_actions() {
    let mut m = model("file_tags");
    m.all = vec![host(
        "host",
        "/very-long-source/which/must/be/read/in/full/config",
        &["team"],
        &[],
    )];
    m.filter(true);
    m.key(key(KeyCode::Home));
    assert_eq!(m.key(key(KeyCode::Char('i'))), Action::None);
    assert!(!matches!(m.view, View::List));
    let text = screen(&mut m, 24, 20);
    let flat: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("/very-long-source/which/must/be/read/in/full/config"),
        "{text}"
    );
    for code in ['e', 'd', 'f', 'N', '*'] {
        assert_eq!(m.key(key(KeyCode::Char(code))), Action::None);
    }
    assert!(m.selected_host().is_none());
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(m.view, View::List));
    assert!(m.selected_host().is_none());
}

#[test]
fn groups_failed_reload_and_empty_exact_scope_keep_projection_and_banners() {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("config");
    let mut m = model("file_tags");
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    let rows = m.rows.clone();
    let inspector = runtime::effective::Inspector::default();
    assert!(reload_hosts(&inspector, &mut m, &config_path).is_err());
    assert_eq!(m.rows, rows);
    assert_eq!(
        m.selected_group(),
        Some(groups::Key::File("/one/config".into()))
    );
    m.source_filter = Some("/empty/config".into());
    query(&mut m, "");
    assert!(m.rows.is_empty());
    m.error = "reload failed".into();
    for (w, h) in [(100, 25), (60, 10), (24, 8), (1, 1)] {
        let text = screen(&mut m, w, h);
        if w > 1 {
            assert!(text.contains("C/F2"), "{text}");
        }
    }
}

#[test]
fn groups_favorites_and_recent_sort_inside_each_group_without_duplicates() {
    let mut m = model("files");
    m.metadata.insert(
        "beta".into(),
        HostMetadata {
            favorite: true,
            note: String::new(),
        },
    );
    m.filter(false);
    let names: Vec<_> = m
        .rows
        .iter()
        .filter_map(|row| match row {
            groups::Row::Host(i) => Some(m.all[*i].name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["beta", "alpha", "gamma"]);
    m.metadata.clear();
    m.history.insert(
        "beta".into(),
        ConnectionInfo {
            last_connect: Utc::now(),
            ..Default::default()
        },
    );
    m.recent = true;
    m.filter(false);
    let names: Vec<_> = m
        .rows
        .iter()
        .filter_map(|row| match row {
            groups::Row::Host(i) => Some(m.all[*i].name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["beta", "alpha", "gamma"]);
}

fn native_inspection(name: &str, user: &str) -> runtime::effective::Inspection {
    runtime::effective::Inspection {
        name: name.into(),
        effective: Ok(runtime::effective::EffectiveConfig::parse(&format!(
            "hostname {name}.resolved\nuser {user}\nport 22\n"
        ))
        .unwrap()),
        ping: None,
    }
}

fn assert_first_native_result_connects(mode: &str) {
    let mut m = model(mode);
    // Manual collapse must survive the temporary search expansion.
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    query(&mut m, "user:resolved-user");
    assert!(m.rows.is_empty());
    m.apply_inspection(native_inspection("beta", "resolved-user"));
    assert_eq!(m.matching, [1]);
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("beta".into(), None),
        "{mode}: first asynchronous result must be the selected SSH target"
    );
    query(&mut m, "");
    assert!(m
        .group_projection
        .collapsed(&groups::Key::File("/one/config".into())));
}

#[test]
fn groups_async_first_native_result_files_connects() {
    assert_first_native_result_connects("files");
}

#[test]
fn groups_async_first_native_result_file_tags_connects() {
    assert_first_native_result_connects("file_tags");
}

#[test]
fn groups_async_first_online_probe_result_connects() {
    for mode in ["files", "file_tags"] {
        let mut m = model(mode);
        m.apply_inspection(native_inspection("gamma", "resolved-user"));
        query(&mut m, "is:online");
        assert!(m.rows.is_empty());
        let mut result = native_inspection("gamma", "resolved-user");
        result.ping = Some(PingResult {
            method: crate::storage::AvailabilityMethod::Ssh,
            name: "gamma".into(),
            status: PingStatus::Online,
            duration: Duration::ZERO,
            error: None,
        });
        m.apply_inspection(result);
        assert_eq!(m.matching, [2]);
        assert_eq!(
            m.key(key(KeyCode::Enter)),
            Action::Connect("gamma".into(), None),
            "{mode}"
        );
    }
}

#[test]
fn groups_async_first_secondary_alias_respects_exact_source_and_connects() {
    for mode in ["files", "file_tags"] {
        let mut m = model(mode);
        let mut secondary = m.all[0].clone();
        secondary.name = "secondary".into();
        secondary.hostname = "secondary.example".into();
        secondary.aliases = vec!["alpha".into(), "secondary".into()];
        m.all[0].aliases = secondary.aliases.clone();
        m.all.push(secondary);
        m.preferences.group_aliases = true;
        m.source_filter = Some("/one/config".into());
        query(&mut m, "user:resolved-user");
        assert!(m.rows.is_empty());
        m.apply_inspection(native_inspection("gamma", "resolved-user"));
        assert!(m.rows.is_empty(), "excluded source cannot produce a result");
        m.apply_inspection(native_inspection("secondary", "resolved-user"));
        assert_eq!(m.matching, [3]);
        assert_eq!(m.visible, [3]);
        assert_eq!(
            m.key(key(KeyCode::Enter)),
            Action::Connect("secondary".into(), None),
            "{mode}"
        );
        m.key(key(KeyCode::F(3)));
        assert!(
            matches!(m.view, View::Aliases { .. }),
            "{mode}: F3 must be enabled"
        );
    }
}

#[test]
fn groups_async_membership_preserves_explicit_header_selection() {
    for mode in ["files", "file_tags"] {
        let mut m = model(mode);
        m.apply_inspection(native_inspection("alpha", "resolved-user"));
        query(&mut m, "user:resolved-user");
        m.key(key(KeyCode::Home));
        let selected = m.selected_group();
        assert!(selected.is_some());
        m.apply_inspection(native_inspection("beta", "resolved-user"));
        assert_eq!(m.selected_group(), selected, "{mode}");
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
        assert!(m.group_projection.collapsed(&selected.unwrap()));
    }
}

#[test]
fn groups_async_membership_preserves_host_and_unchanged_recent_order() {
    for mode in ["files", "file_tags"] {
        let mut m = model(mode);
        // Shared tag makes the within-group order observable in both modes.
        m.all[1].tags = m.all[0].tags.clone();
        m.apply_inspection(native_inspection("beta", "resolved-user"));
        query(&mut m, "user:resolved-user");
        m.apply_inspection(native_inspection("alpha", "resolved-user"));
        assert_eq!(
            m.key(key(KeyCode::Enter)),
            Action::Connect("beta".into(), None),
            "{mode}"
        );
        let rows = m.rows.clone();
        let visible = m.visible.clone();
        assert_eq!(visible, [0, 1]);
        m.recent = true;
        m.history.insert(
            "beta".into(),
            ConnectionInfo {
                last_connect: Utc::now(),
                ..Default::default()
            },
        );
        m.apply_inspection(native_inspection("alpha", "resolved-user"));
        assert_eq!(m.rows, rows, "{mode}: unchanged membership must not resort");
        assert_eq!(m.visible, visible);
        assert_eq!(
            m.key(key(KeyCode::Enter)),
            Action::Connect("beta".into(), None)
        );
    }
}

#[test]
fn groups_async_returning_results_preserve_intentional_query_collapse() {
    for mode in ["files", "file_tags"] {
        let mut m = model(mode);
        m.apply_inspection(native_inspection("alpha", "resolved-user"));
        query(&mut m, "user:resolved-user");
        m.key(key(KeyCode::Home));
        m.key(key(KeyCode::Enter));
        m.apply_inspection(native_inspection("alpha", "other-user"));
        assert!(m.rows.is_empty());
        m.apply_inspection(native_inspection("alpha", "resolved-user"));
        assert_eq!(m.matching, [0]);
        assert_eq!(
            m.rows.len(),
            1,
            "{mode}: do not force open a collapsed result"
        );
        assert!(m.selected_host().is_none());
        assert!(m
            .group_projection
            .collapsed(&groups::Key::File("/one/config".into())));
        assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
        assert!(m.rows.len() > 1);
    }
}
