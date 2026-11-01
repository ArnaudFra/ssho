use super::tests::{key, screen};
use super::*;

fn block(path: &str, line: usize, names: &[&str]) -> Vec<Host> {
    names
        .iter()
        .map(|name| Host {
            name: (*name).into(),
            hostname: "shared.example".into(),
            source_file: path.into(),
            line_number: line,
            aliases: names.iter().map(|s| (*s).into()).collect(),
            ..Host::default()
        })
        .collect()
}
fn grouped(hosts: Vec<Host>) -> Model {
    Model::new(
        hosts,
        serde_json::from_value(serde_json::json!({"group_aliases":true})).unwrap(),
        BTreeMap::new(),
        false,
    )
}
fn query(m: &mut Model, text: &str) {
    m.query.set(text);
    m.filter(true);
}

#[test]
fn grouping_opt_in_preserves_raw_aliases_and_prefers_declared_primary() {
    let hosts = block("/config", 1, &["z-primary", "a-secondary"]);
    let mut flat = Model::new(hosts.clone(), AppConfig::default(), BTreeMap::new(), false);
    assert_eq!(flat.visible.len(), 2);
    let mut m = grouped(hosts);
    assert_eq!(m.all.len(), 2);
    assert_eq!(m.visible.len(), 1);
    assert_eq!(m.selected_host().unwrap().name, "z-primary");
    flat.selected = 0;
    flat.preferences = m.preferences.clone();
    flat.filter(false);
    assert_eq!(flat.selected_host().unwrap().name, "a-secondary");
    query(&mut m, "a-secondary");
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("a-secondary".into(), None)
    );
}

#[test]
fn grouping_uses_source_line_and_membership_never_synthetic_provenance() {
    let mut hosts = block("/one/config", 1, &["a", "b"]);
    hosts.extend(block("/one/config", 9, &["a", "b"]));
    hosts.extend(block("/two/config", 1, &["a", "b"]));
    hosts.extend(block("/one/config", 0, &["c", "d"]));
    hosts.extend(block("", 1, &["e", "f"]));
    let mut inconsistent = block("/one/config", 1, &["a", "b"])[1].clone();
    inconsistent.aliases = vec!["foreign".into()];
    hosts.push(inconsistent);
    assert_eq!(grouped(hosts).visible.len(), 8);
}

#[test]
fn grouping_secondary_metadata_effective_hidden_and_source_are_not_merged() {
    let mut hosts = block("/one", 2, &["primary", "secondary"]);
    hosts[0].tags.push("hidden".into());
    hosts.extend(block("/two", 2, &["other", "alternate"]));
    let mut m = grouped(hosts);
    m.source_filter = Some("/one".into());
    m.metadata.insert(
        "secondary".into(),
        HostMetadata {
            favorite: true,
            note: "special-note".into(),
        },
    );
    m.effective.insert(
        "secondary".into(),
        Ok(runtime::effective::EffectiveConfig::parse(
            "hostname secondary-target\nuser secondary-user\nport 22\n",
        )
        .unwrap()),
    );
    query(&mut m, "user:secondary-user special-note");
    assert_eq!(m.selected_host().unwrap().name, "secondary");
    m.favorites_only = true;
    m.show_hidden = true;
    query(&mut m, "");
    assert_eq!(m.visible.len(), 1);
    assert_eq!(m.selected_host().unwrap().name, "secondary");
    assert!(!m.is_favorite("primary"));
    assert_eq!(m.note("primary"), "");
}

#[test]
fn grouping_f3_selects_ambiguous_secondary_and_cancel_keeps_context() {
    let mut m = grouped(block("/one", 1, &["production", "prod"]));
    query(&mut m, "prod");
    m.query.key(key(KeyCode::Left));
    let caret = m.query.cursor;
    m.key(key(KeyCode::F(3)));
    assert!(!matches!(m.view, View::List));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Esc));
    assert!(matches!(m.view, View::List));
    assert!(m.parents.is_empty());
    assert_eq!(m.query.cursor, caret);
    assert_eq!(m.selected_host().unwrap().name, "production");
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("prod".into(), None)
    );
    assert_eq!(m.query.value, "prod");
    assert!(m.pending_preferred.is_none());
    assert!(m.parents.is_empty());
}

#[test]
fn grouping_reload_sort_and_ssh_return_retain_active_alias() {
    let mut m = grouped(block("/one", 1, &["production", "prod"]));
    query(&mut m, "prod");
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    m.replace_hosts(block("/one", 8, &["production", "prod"]), None);
    m.recent = true;
    m.filter(false);
    m.resume_connection(
        "prod",
        Ok(37),
        History::load(tempfile::tempdir().unwrap().path()),
    );
    assert_eq!(m.selected_host().unwrap().name, "prod");
    assert_eq!(m.visible.len(), 1);
}

#[test]
fn grouping_render_shows_primary_and_active_in_narrow_rows_and_counts_aliases() {
    let mut m = grouped(block("/one", 1, &["primary", "short"]));
    query(&mut m, "short");
    for width in [24, 60, 120] {
        let text = screen(&mut m, width, 20);
        assert!(text.contains("primary"), "{text}");
        assert!(text.contains("short"), "{text}");
        assert!(text.contains("alias"), "{text}");
        assert!(text.contains("block"), "{text}");
    }
}

#[test]
fn grouping_toggle_is_action_only_outside_search_and_palette_is_available() {
    let mut m = grouped(block("/one", 1, &["a", "b"]));
    m.key(key(KeyCode::Char('A')));
    assert_eq!(m.query.value, "A");
    m.search_focus = false;
    assert_ne!(m.key(key(KeyCode::Char('A'))), Action::None);
    assert!(PaletteAction::CATALOGUE
        .iter()
        .any(|a| a.label().contains("aliases")));
}

#[test]
fn grouping_defaults_null_and_invalid_preference() {
    for value in [
        serde_json::json!({}),
        serde_json::json!({"group_aliases":null}),
    ] {
        let config: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap()["group_aliases"],
            false
        );
    }
    assert!(
        serde_json::from_value::<AppConfig>(serde_json::json!({"group_aliases":"yes"})).is_err()
    );
}

#[test]
fn grouping_async_results_do_not_reapply_recent_sort_for_same_raw_membership() {
    let mut hosts = block("/one", 1, &["alpha", "a"]);
    hosts.extend(block("/one", 4, &["beta", "b"]));
    let mut m = grouped(hosts);
    m.recent = true;
    m.filter(false);
    assert_eq!(m.visible.len(), 2);
    m.history.insert(
        "beta".into(),
        ConnectionInfo {
            last_connect: Utc::now(),
            ..Default::default()
        },
    );
    let before = m.visible.clone();
    m.apply_inspection(runtime::effective::Inspection {
        name: "b".into(),
        effective: Err("fixture".into()),
        ping: None,
    });
    assert_eq!(m.visible, before);
}

#[test]
fn grouping_explicit_choice_cancels_old_mutation_wait_before_late_native_result() {
    let mut m = grouped(block("/one", 1, &["production", "prod"]));
    m.effective.insert(
        "prod".into(),
        Ok(runtime::effective::EffectiveConfig::parse(
            "hostname real\\nuser desired\\nport 22"
                .replace("\\n", "\n")
                .as_str(),
        )
        .unwrap()),
    );
    m.query.set("user:desired");
    m.pending_preferred = Some(PendingPreferred {
        host: m.all[0].clone(),
        query: m.query.value.clone(),
        provisional: None,
    });
    m.filter(false);
    m.key(key(KeyCode::Left));
    let caret = m.query.cursor;
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    assert!(m.pending_preferred.is_none());
    m.apply_inspection(runtime::effective::Inspection {
        name: "production".into(),
        effective: Err("fresh failed".into()),
        ping: None,
    });
    assert_eq!(m.query.value, "user:desired");
    assert_eq!(m.query.cursor, caret);
    assert!(m.search_focus);
    assert_eq!(m.selected_host().unwrap().name, "prod");
}

#[test]
fn grouping_choice_reveals_hidden_nonfavorite_and_clears_only_incompatible_query() {
    let mut hosts = block("/one", 1, &["primary", "secondary"]);
    hosts[1].tags.push("hidden".into());
    let mut m = grouped(hosts);
    m.source_filter = Some("/one".into());
    m.metadata.insert(
        "primary".into(),
        HostMetadata {
            favorite: true,
            note: String::new(),
        },
    );
    m.favorites_only = true;
    query(&mut m, "primary");
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    assert_eq!(m.selected_host().unwrap().name, "secondary");
    assert_eq!(m.source_filter, Some("/one".into()));
    assert!(m.query.value.is_empty());
    assert!(m.show_hidden);
    assert!(!m.favorites_only);
    assert!(m.pending_preferred.is_none());
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::Connect("secondary".into(), None)
    );
}

#[test]
fn grouping_toggle_preserves_fresh_preferences_and_commits_mode_only_after_save() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let mut m = grouped(block("/one", 1, &["primary", "secondary"]));
    std::fs::write(&path,r#"{"ssh_command":["latest wrapper","literal space"],"ping_on_start":false,"focus_search_on_start":false,"key_bindings":{"quit_keys":["x"],"disable_esc_quit":true,"future_binding":7},"future":{"keep":true}}"#).unwrap();
    aliases::toggle(&mut m, root.path()).unwrap();
    assert!(!m.preferences.group_aliases);
    assert_eq!(m.visible.len(), 2);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved["ssh_command"],
        serde_json::json!(["latest wrapper", "literal space"])
    );
    assert_eq!(saved["ping_on_start"], false);
    assert_eq!(saved["focus_search_on_start"], false);
    assert_eq!(saved["key_bindings"]["quit_keys"], serde_json::json!(["x"]));
    assert_eq!(saved["key_bindings"]["future_binding"], 7);
    assert_eq!(saved["future"]["keep"], true);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(aliases::toggle(&mut m, root.path()).is_err());
    assert!(!m.preferences.group_aliases);
    assert_eq!(m.visible.len(), 2);
}
