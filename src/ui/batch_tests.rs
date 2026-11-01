use super::tests::{key, screen, typing};
use super::*;
fn fixture() -> (tempfile::TempDir, Model) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("config");
    std::fs::write(
        &root,
        "Host alpha secondary\n HostName alpha.invalid\nHost beta\n HostName beta.invalid\n",
    )
    .unwrap();
    let hosts = config::load_report(&root).unwrap().hosts;
    let mut m = Model::new(hosts, AppConfig::default(), BTreeMap::new(), false);
    m.search_focus = false;
    m.batch.root = root;
    (tmp, m)
}
#[test]
fn marks_freeze_active_alias_root_and_source_across_reload_filter() {
    let (_tmp, mut m) = fixture();
    m.preferences.group_aliases = true;
    m.filter(true);
    m.key(key(KeyCode::F(3)));
    m.key(key(KeyCode::Down));
    m.key(key(KeyCode::Enter));
    assert_eq!(m.selected_host().unwrap().name, "secondary");
    m.key(key(KeyCode::Char('x')));
    let marked = m.batch.marks.clone();
    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0].alias, "secondary");
    m.search_focus = true;
    typing(&mut m, "beta");
    m.search_focus = false;
    assert_eq!(m.key(key(KeyCode::F(5))), Action::Reload);
    m.replace_hosts(m.all.clone().into_iter().rev().collect(), None);
    assert_eq!(m.batch.marks, marked);
    m.key(key(KeyCode::Char('B')));
    let View::Batch(editor) = &m.view else {
        panic!("missing batch")
    };
    assert_eq!(editor.targets, marked);
}
#[test]
fn headers_are_never_marked_and_explicit_select_uses_collapsed_projected_results() {
    let (_tmp, mut m) = fixture();
    m.preferences.group_mode = GroupMode::Files;
    m.preferences.group_aliases = true;
    m.filter(true);
    m.key(key(KeyCode::Home));
    m.key(key(KeyCode::Left));
    m.key(key(KeyCode::Char('x')));
    assert!(m.batch.marks.is_empty());
    m.activate_action(PaletteAction::BatchSelectVisible);
    assert_eq!(
        m.batch
            .marks
            .iter()
            .map(|t| t.alias.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    m.activate_action(PaletteAction::BatchClear);
    assert!(m.batch.marks.is_empty());
}
#[test]
fn confirmation_is_explicit_and_command_settings_targets_survive_resize() {
    let (_tmp, mut m) = fixture();
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    typing(&mut m, "echo literal ; $HOME");
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    for (w, h) in [(100, 30), (32, 12), (24, 8), (1, 1)] {
        screen(&mut m, w, h);
    }
    assert!(!m.batch.manager.has_active());
    // Ctrl+L opens a review with full command and navigable targets; reading it
    // launches nothing: only the Launch button or an explicit Ctrl+L does.
    let ctrl_l = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
    m.key(ctrl_l);
    let text = screen(&mut m, 100, 25);
    assert!(text.contains("Confirm batch"), "{text}");
    assert!(
        text.contains("Targets ✓ → Command ✓ → [Review] → Results"),
        "{text}"
    );
    assert!(
        text.contains("[▶ Back to edit ]  [  Launch on 3 target(s) ]"),
        "{text}"
    );
    assert_eq!(m.key(key(KeyCode::Esc)), Action::None);
    let View::Batch(editor) = &m.view else {
        panic!()
    };
    assert_eq!(editor.command.value, "echo literal ; $HOME");
    m.key(ctrl_l);
    // Enter on the focused, safe choice returns to the edit step.
    assert_eq!(m.key(key(KeyCode::Enter)), Action::None);
    assert!(matches!(&m.view, View::Batch(editor) if !editor.confirm));
    m.key(ctrl_l);
    m.key(key(KeyCode::Tab));
    assert_eq!(m.key(key(KeyCode::Enter)), Action::StartBatch);
    assert_eq!(
        m.key(ctrl_l),
        Action::StartBatch,
        "the explicit shortcut also confirms"
    );
}

#[test]
fn batch_settings_have_visible_cursor_on_compact_terminals() {
    let (_tmp, mut m) = fixture();
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    m.key(key(KeyCode::Tab));
    let text = screen(&mut m, 32, 8);
    assert!(text.contains("Parallel: 4"), "{text}");
    m.key(key(KeyCode::Tab));
    let text = screen(&mut m, 32, 8);
    assert!(text.contains("Timeout: 30"), "{text}");
}
#[test]
fn batch_running_quit_is_confirmed_and_pending_cancel_has_no_history() {
    let (tmp, mut m) = fixture();
    m.batch.history_root = Some(tmp.path().join("app"));
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    typing(&mut m, "echo safe");
    m.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    batch::start(&mut m).unwrap();
    assert!(m.batch.manager.has_active());
    // Leaving the results neither quits nor cancels: the batch keeps running.
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::None);
    assert!(matches!(m.view, View::List) && !m.quit_tunnels);
    assert!(m.batch.manager.has_active());
    // Quitting from the list is what the main loop confirms, naming the batch.
    m.search_focus = false;
    assert_eq!(m.key(key(KeyCode::Char('q'))), Action::Quit);
    m.ask_quit();
    let text = screen(&mut m, 100, 20);
    assert!(
        text.contains("Running batch") && text.contains("Stop and quit"),
        "{text}"
    );
    assert_eq!(
        m.key(key(KeyCode::Enter)),
        Action::None,
        "the focused choice returns"
    );
    assert!(!m.quit_tunnels);
    m.key(key(KeyCode::Char('B')));
    assert!(matches!(m.view, View::BatchResults { .. }));
    assert_eq!(m.key(key(KeyCode::Char('s'))), Action::CancelBatch);
    m.batch.manager.cancel();
    m.batch.poll().unwrap();
    assert!(History::load(m.batch.history_root.as_ref().unwrap())
        .unwrap()
        .connections
        .is_empty());
}

#[test]
fn batch_history_sequentially_keeps_other_connections_and_launch_failures() {
    let (tmp, mut m) = fixture();
    m.batch.history_root = Some(tmp.path().join("app"));
    let root = m.batch.history_root.as_ref().unwrap();
    let mut history = History::load(root).unwrap();
    history.record("other", None).unwrap();
    m.batch.manager = runtime::batch::Manager::with_program(tmp.path().join("missing-native-ssh"));
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    typing(&mut m, "fixture");
    m.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    batch::start(&mut m).unwrap();
    m.batch.poll().unwrap();
    let history = History::load(m.batch.history_root.as_ref().unwrap()).unwrap();
    assert_eq!(history.connections.len(), 4);
    assert_eq!(history.connections["other"].connect_count, 1);
    for name in ["alpha", "secondary", "beta"] {
        let info = &history.connections[name];
        assert_eq!(info.connect_count, 1);
        assert!(info
            .last_result
            .as_ref()
            .unwrap()
            .launch_error
            .as_ref()
            .unwrap()
            .contains("launch native SSH"));
    }
    for (w, h) in [(100, 24), (32, 8), (1, 1)] {
        screen(&mut m, w, h);
    }
    m.key(key(KeyCode::Down));
    assert!(screen(&mut m, 80, 25).contains("Failed"));
}

#[test]
fn batch_unicode_command_uses_display_cells_for_cursor() {
    use ratatui::{
        backend::{Backend, TestBackend},
        Terminal,
    };
    let (_tmp, mut m) = fixture();
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.key(key(KeyCode::Char('B')));
    typing(&mut m, "界e\u{301}👩‍💻Z");
    m.key(key(KeyCode::Left));
    let mut terminal = Terminal::new(TestBackend::new(32, 12)).unwrap();
    terminal.draw(|f| render::draw(f, &mut m)).unwrap();
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    assert_eq!(terminal.backend().buffer()[position].symbol(), "Z");
}

#[test]
fn batch_blocks_synchronous_copy_preflight_without_losing_editor() {
    let (_tmp, mut m) = fixture();
    let host = m.selected_host().unwrap();
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.batch
        .manager
        .start(
            m.batch.marks.clone(),
            "fixture".into(),
            runtime::batch::Options::default(),
        )
        .unwrap();
    m.open_view(View::Host(Box::new(HostForm::edit(host.clone()))));
    let error = changes::open(
        &mut m,
        Action::SaveCopy(Box::new(host.clone()), host.source_file.clone()),
        &host.source_file,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Batch active"), "{error:#}");
    assert!(matches!(m.view, View::Host(_)));
    m.batch.manager.cancel();
}

#[test]
fn active_batch_quit_also_warns_about_suspended_dirty_form() {
    let (_tmp, mut m) = fixture();
    m.activate_action(PaletteAction::BatchSelectVisible);
    m.batch
        .manager
        .start(
            m.batch.marks.clone(),
            "fixture".into(),
            runtime::batch::Options::default(),
        )
        .unwrap();
    m.key(key(KeyCode::Char('e')));
    typing(&mut m, "changed");
    m.key(key(KeyCode::F(7)));
    // Returning from the owned tunnels gives the suspended draft back untouched.
    m.key(key(KeyCode::Char('q')));
    assert!(matches!(&m.view, View::Host(form) if form.dirty()));
    assert!(!m.quit_tunnels);
    m.ask_quit();
    let text = screen(&mut m, 100, 20);
    assert!(text.contains("Running batch"), "{text}");
    assert!(text.contains("Unsaved changes"), "{text}");
    m.batch.manager.cancel();
}

#[cfg(unix)]
#[test]
fn symlink_source_mark_survives_reload_and_alias_projection() {
    let (tmp, mut m) = fixture();
    let access = tmp.path().join("access-config");
    std::os::unix::fs::symlink(&m.batch.root, &access).unwrap();
    m.batch.root = access.clone();
    m.replace_hosts(config::load_report(&access).unwrap().hosts, None);
    m.key(key(KeyCode::Char('x')));
    let host = m.selected_host().unwrap();
    assert!(
        m.batch.marked(&host),
        "canonical mark must be visible through access path"
    );
    assert_ne!(m.batch.marks[0].source, host.source_file);
    let frozen = m.batch.marks.clone();
    m.replace_hosts(config::load_report(&access).unwrap().hosts, None);
    m.preferences.group_aliases = true;
    m.filter(true);
    assert!(m.batch.marked(&m.selected_host().unwrap()));
    assert_eq!(m.batch.marks, frozen);
    m.key(key(KeyCode::Char('x')));
    assert!(m.batch.marks.is_empty());
}

#[test]
fn moved_alias_has_no_history_attempt() {
    let (tmp, mut m) = fixture();
    m.batch.history_root = Some(tmp.path().join("history"));
    m.key(key(KeyCode::Char('x')));
    let child = tmp.path().join("included");
    std::fs::write(&child, "Host alpha\n HostName invalid.test\n").unwrap();
    std::fs::write(&m.batch.root, format!("Include {}\n", child.display())).unwrap();
    m.batch.manager = runtime::batch::Manager::with_program(tmp.path().join("must-not-run"));
    m.batch
        .manager
        .start(m.batch.marks.clone(), "cmd".into(), Default::default())
        .unwrap();
    m.batch.poll().unwrap();
    assert!(m.batch.manager.records()[0]
        .diagnostic
        .contains("source changed"));
    assert!(History::load(m.batch.history_root.as_ref().unwrap())
        .unwrap()
        .connections
        .is_empty());
}
