use super::tests::{key, screen, typing};
use super::*;
use std::fs;

fn assert_move_completion(
    destination_tags: &str,
    query: &str,
    scoped: bool,
    remaining: bool,
    expected_query: &str,
    expected_hidden: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    let source = dir.path().join("source");
    let target = dir.path().join("destination");
    let app = dir.path().join("app");
    fs::write(&root, "Include source destination\n").unwrap();
    fs::write(
        &source,
        format!(
            "#FileTags: source, shared\n# Tags: own\nHost moved\n HostName moved.example\n{}",
            if remaining {
                "# Tags: hidden\nHost remaining\n HostName remaining.example\n"
            } else {
                ""
            }
        ),
    )
    .unwrap();
    fs::write(&target, format!("#FileTags: {destination_tags}\n")).unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        false,
    );
    let inspector = runtime::effective::Inspector::default();
    for result in start_inspection(&inspector, &mut m, &root, false) {
        assert!(result.effective.is_ok());
        m.apply_inspection(result);
    }
    if scoped {
        m.source_filter = Some(source.clone());
    }
    m.query.set(query);
    m.filter(true);
    let original = m.selected_host().unwrap();
    assert_eq!(original.name, "moved");
    assert_eq!(original.file_tags, ["source", "shared"]);
    assert!(!m.show_hidden);
    config::move_host(&original, &target, &app).unwrap();
    let mut metadata = Metadata::load(&app, &root).unwrap();
    let mut profiles = TunnelProfiles::load(&app, &root).unwrap();
    profiles::complete_saved(
        &mut m,
        config::load_hosts(&root).unwrap(),
        &Action::MoveTo(original, target.clone()),
        &mut metadata,
        &mut profiles,
    );

    for inspected in [false, true] {
        if inspected {
            let mut received_moved = false;
            for result in start_inspection(&inspector, &mut m, &root, false) {
                received_moved |= result.name == "moved";
                assert!(result.effective.is_ok());
                m.apply_inspection(result);
            }
            assert!(
                received_moved,
                "deliver the moved host's fresh ssh -G result"
            );
            assert!(m.pending_preferred.is_none());
        }
        assert_eq!(m.query.value, expected_query, "inspected={inspected}");
        assert_eq!(m.show_hidden, expected_hidden, "inspected={inspected}");
        let moved = m.all.iter().find(|host| host.name == "moved").unwrap();
        assert_eq!(moved.source_file, target);
        assert_eq!(moved.tags, ["own"]);
        assert_eq!(
            moved.file_tags,
            destination_tags.split(", ").collect::<Vec<_>>()
        );
        if scoped && remaining {
            assert_eq!(m.source_filter.as_ref(), Some(&source));
            assert!(m.visible.is_empty());
            assert!(m.selected_host().is_none());
            assert!(m.pending_preferred.is_none());
            assert!(m
                .all
                .iter()
                .any(|host| host.source_file == source && host.hidden()));
        } else {
            assert!(m.source_filter.is_none());
            let selected = m.selected_host().unwrap();
            assert_eq!(selected.name, "moved");
            assert_eq!(selected.source_file, target);
        }
        assert!(m.selected <= m.visible.len().saturating_sub(1));
    }
}

#[test]
fn move_completion_clears_lost_inherited_query_and_reveals_saved_host() {
    for scoped in [false, true] {
        assert_move_completion("destination", "tag:source", scoped, false, "", false);
    }
}

#[test]
fn move_completion_reveals_destination_inherited_hidden() {
    for scoped in [false, true] {
        assert_move_completion("destination, hidden", "", scoped, false, "", true);
    }
}

#[test]
fn move_completion_retains_source_scope_query_and_hidden_preference_for_raw_remainder() {
    assert_move_completion(
        "destination, hidden",
        "tag:source AND tag:own",
        true,
        true,
        "tag:source AND tag:own",
        false,
    );
}

#[test]
fn move_completion_retains_destination_compatible_query() {
    for scoped in [false, true] {
        assert_move_completion(
            "destination, shared",
            "tag:shared",
            scoped,
            false,
            "tag:shared",
            false,
        );
    }
}

#[test]
fn ui_search_display_hidden_and_source_compose() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    fs::write(
        &root,
        "#FileTags: inherited, Hidden\n# Tags: own\nHost alpha\n HostName a.example\n",
    )
    .unwrap();
    let mut m = Model::new(
        config::load_hosts(&root).unwrap(),
        AppConfig::default(),
        BTreeMap::new(),
        true,
    );
    m.source_filter = Some(root);
    assert!(m.visible.is_empty());
    m.show_hidden = true;
    typing(&mut m, "tag:inherited own");
    assert_eq!(m.visible.len(), 1);
    let text = screen(&mut m, 120, 30);
    assert!(text.contains("%inherited"), "{text}");
    assert!(text.contains("#own"));
    assert!(
        text.contains("File: ") && text.contains("(C/F2: all files)"),
        "{text}"
    );
    assert!(text.contains("Hidden shown (H)"), "{text}");
    m.search_focus = false;
    m.key(key(KeyCode::Char('H')));
    assert!(m.visible.is_empty());
    assert!(m.source_filter.is_some());
}

#[test]
fn ui_reload_warning_is_durable_and_failed_reload_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    fs::write(&root, "Host alpha\n HostName a.example\n#FileTags: late\n").unwrap();
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), true);
    let inspector = runtime::effective::Inspector::default();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    m.source_filter = Some(root.clone());
    m.error = "Other notice".into();
    for (w, h) in [(120, 30), (60, 10)] {
        let text = screen(&mut m, w, h);
        assert!(text.contains("FileTags"), "{text}");
        assert!(text.contains("File") && text.contains("C/F2"), "{text}");
    }
    fs::remove_file(&root).unwrap();
    assert!(reload_hosts(&inspector, &mut m, &root).is_err());
    assert!(screen(&mut m, 120, 30).contains("FileTags"));
    fs::write(&root, "Host alpha\n HostName a.example\n").unwrap();
    drop(reload_hosts(&inspector, &mut m, &root).unwrap());
    assert!(!screen(&mut m, 120, 30).contains("FileTags warning"));
}

#[test]
fn forms_show_destination_inheritance_separately_from_own_tags() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    let target = dir.path().join("empty");
    fs::write(
        &root,
        "#FileTags: source\n# Tags: own\nHost alpha\n HostName a.example\n",
    )
    .unwrap();
    fs::write(&target, "#FileTags: destination\n").unwrap();
    let host = config::find_host(&root, "alpha").unwrap();
    for (form, expected) in [
        (HostForm::edit(host.clone()), "%source"),
        (HostForm::duplicate(host, target.clone()), "%destination"),
        (HostForm::add("new".into(), target), "%destination"),
    ] {
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        m.view = View::Host(Box::new(form));
        let text = screen(&mut m, 110, 45);
        assert!(text.contains(expected), "{text}");
        let View::Host(form) = &m.view else {
            unreachable!()
        };
        assert!(!form.inputs[8].value.contains("source"));
        assert!(!form.inputs[8].value.contains("destination"));
    }
}

#[test]
fn all_warnings_are_readable_in_help_on_narrow_screen() {
    let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
    m.config_warnings = vec![config::ConfigWarning {
        path: PathBuf::from(format!("/very/long/{}/config", "directory/".repeat(15))),
        line: 42,
        message: "FileTags ignored after Host, Include or Match".into(),
    }];
    m.view = View::Help;
    m.scroll = usize::MAX;
    let text = screen(&mut m, 60, 14);
    assert!(text.contains("ignored after Host"), "{text}");
    assert!(text.contains("config:42:"), "{text}");
}
