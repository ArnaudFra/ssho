use super::*;
use std::fs;

#[test]
fn header_tags_are_local_stable_and_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    fs::write(&root, " #fIlEtAgS : Work, hidden, Work\n# FileTags: lab, work\nInclude child\n# Tags: own, Work\nHost root\n HostName root.example\n").unwrap();
    fs::write(
        dir.path().join("child"),
        "#FileTags: child\nHost included\n HostName child.example\n",
    )
    .unwrap();
    let hosts = load_hosts(&root).unwrap();
    let host = hosts.iter().find(|h| h.name == "root").unwrap();
    assert!(host.hidden(), "inherited hidden controls visibility");
    assert!(host.matches("lab own"));
    assert_eq!(host.tags, ["own", "Work"]);
    assert_eq!(
        serde_json::to_value(host).unwrap()["file_tags"],
        serde_json::json!(["Work", "hidden", "lab", "work"])
    );
    let child = hosts.iter().find(|h| h.name == "included").unwrap();
    assert!(!child.hidden());
    assert!(child.matches("child"));
}

#[test]
fn deleting_first_host_never_activates_late_filetags() {
    for newline in ["\n", "\r\n"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config");
        let text = "Host first\n HostName first.example\n# FileTags: hidden\nHost second\n HostName second.example\n".replace('\n', newline);
        fs::write(&root, text).unwrap();
        let first = find_host(&root, "first").unwrap();
        delete_host(&first, &dir.path().join("backups")).unwrap();
        let remaining = fs::read_to_string(&root).unwrap();
        assert!(
            remaining.contains("# Inactive FileTags:"),
            "late directive must be explicitly inactive: {remaining}"
        );
        assert!(!find_host(&root, "second").unwrap().hidden());
    }
}

#[test]
fn header_survives_edits_moves_deletes_and_new_host() {
    for preamble in [
        "#FileTags: source\n# Tags: own\n",
        "# Tags: own\n#FileTags: source\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config");
        let target = dir.path().join("target");
        fs::write(
            &root,
            format!("{preamble}Host first\n HostName first.example\n#FileTags: late\n"),
        )
        .unwrap();
        fs::write(&target, "# FileTags: destination\n").unwrap();
        let first = find_host(&root, "first").unwrap();
        let mut edited = first.clone();
        edited.tags = vec!["edited".into()];
        update_host(&first, &edited, &dir.path().join("app")).unwrap();
        let first = find_host(&root, "first").unwrap();
        assert_eq!(first.file_tags, ["source"]);
        assert_eq!(first.tags, ["edited"]);
        let before = fs::read(&root).unwrap();
        let plan = preview::prepare_move(&first, &target).unwrap();
        assert_eq!(fs::read(&root).unwrap(), before);
        let after: Vec<_> = plan
            .files()
            .iter()
            .map(|f| (f.path().to_owned(), f.after().to_vec()))
            .collect();
        plan.apply(&dir.path().join("app")).unwrap();
        for (path, bytes) in after {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        let moved = find_host(&target, "first").unwrap();
        assert_eq!(moved.file_tags, ["destination"]);
        assert_eq!(moved.tags, ["edited"]);
        assert!(!fs::read_to_string(&target).unwrap().contains("source"));
        assert!(fs::read_to_string(&root)
            .unwrap()
            .contains("#FileTags: source"));
        add_host(
            &Host {
                name: "new".into(),
                hostname: "new.example".into(),
                ..Host::default()
            },
            &root,
            &dir.path().join("app"),
        )
        .unwrap();
        assert_eq!(find_host(&root, "new").unwrap().file_tags, ["source"]);
    }
}

#[test]
fn copy_preserves_destination_header_and_original_own_tags() {
    for preamble in [
        "#FileTags: destination\n# Tags: original\n",
        "# Tags: original\n#FileTags: destination\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config");
        fs::write(
            &root,
            format!("{preamble}Host first\n HostName first.example\n"),
        )
        .unwrap();
        let copy = Host {
            name: "copy".into(),
            hostname: "copy.example".into(),
            port: "22".into(),
            tags: vec!["copied".into()],
            file_tags: vec!["source".into()],
            ..Host::default()
        };
        let before = fs::read(&root).unwrap();
        let plan = preview::prepare_copy(&copy, &root, &root).unwrap();
        assert_eq!(fs::read(&root).unwrap(), before);
        plan.apply(&dir.path().join("app")).unwrap();
        let first = find_host(&root, "first").unwrap();
        let copy = find_host(&root, "copy").unwrap();
        assert_eq!(first.file_tags, ["destination"]);
        assert_eq!(copy.file_tags, ["destination"]);
        assert_eq!(first.tags, ["original"]);
        assert_eq!(copy.tags, ["copied"]);
    }
}

#[test]
fn late_boundaries_report_exact_lines_and_union_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    for boundary in [
        "Host first\n",
        "Host\n",
        "Include missing\n",
        "Include\n",
        "Match all\n",
    ] {
        fs::write(&root, format!("#FileTags: Work, Work, work\n{boundary} # filetags: ignored\nHost second\n HostName second.example\n")).unwrap();
        let report = load_report(&root).unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].path, root);
        assert_eq!(report.warnings[0].line, 3);
        let host = report.hosts.iter().find(|h| h.name == "second").unwrap();
        assert_eq!(host.all_tags(), ["Work", "work"]);
        assert!(!host.matches("ignored"));
    }
}

#[test]
fn alias_deletion_and_match_include_late_comments_stay_inactive() {
    let dir = tempfile::tempdir().unwrap();
    for body in [
        "Host first alias\n HostName first.example\n#FileTags: hidden\nHost second\n HostName second.example\n",
        "Host first alias\n #FileTags: hidden\n Include missing\n HostName first.example\nMatch all\n#FileTags: late\nHost second\n HostName second.example\n",
        "# Tags: own\n#FileTags: base\nHost first alias\n #FileTags: ignored\n HostName first.example\n#FileTags: hidden\n",
    ] {
        let root = dir.path().join("config");
        fs::write(&root, body.replace('\n', "\r\n")).unwrap();
        delete_host(&find_host(&root, "first").unwrap(), &dir.path().join("app")).unwrap();
        let alias = find_host(&root, "alias").unwrap();
        assert!(!alias.hidden());
        delete_host(&alias, &dir.path().join("app")).unwrap();
        let text = fs::read_to_string(&root).unwrap();
        assert!(!text.replace("\r\n", "").contains('\n'));
        assert!(text.contains("# Inactive FileTags:"));
        if body.contains("Include") { assert!(text.contains("Include missing")); }
        if body.contains("Match") { assert!(text.contains("Match all")); }
        add_host(&Host { name:"new".into(), hostname:"new.example".into(), ..Host::default() }, &root, &dir.path().join("app")).unwrap();
        let fresh = find_host(&root, "new").unwrap();
        assert!(!fresh.hidden());
        assert!(!fresh.matches("ignored"));
        assert!(!fresh.matches("late"));
    }
}

#[test]
fn recursive_include_warnings_are_local_and_not_duplicated() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("config");
    let child = dir.path().join("child");
    fs::write(&root, "#FileTags: root\nInclude child\nInclude child\n#FileTags: ignored\nHost root\n HostName root.example\n").unwrap();
    fs::write(&child, "#FileTags: child\nInclude config\n#FileTags: late\nHost nested\n HostName nested.example\n").unwrap();
    let report = load_report_checked(&root).unwrap();
    assert_eq!(report.files, [root.clone(), child.clone()]);
    assert_eq!(report.warnings.len(), 2);
    assert_eq!(report.warnings[0].path, child);
    assert_eq!(report.warnings[0].line, 3);
    assert_eq!(report.warnings[1].path, root);
    assert_eq!(report.warnings[1].line, 4);
    assert_eq!(report.hosts[0].all_tags(), ["child"]);
    assert_eq!(report.hosts[1].all_tags(), ["root"]);
    assert_eq!(load_hosts(&root).unwrap(), report.hosts);
    assert_eq!(load_hosts_checked(&root).unwrap(), report.hosts);
}

#[test]
fn alias_detach_and_move_keep_scope_declarations_in_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::write(&source, "# Tags: own\n#FileTags: source\nHost first alias\n HostName shared.example\n#FileTags: late\n").unwrap();
    fs::write(&target, "#FileTags: destination\n").unwrap();
    let original = find_host(&source, "first").unwrap();
    let mut detached = original.clone();
    detached.aliases.clear();
    detached.tags = vec!["detached".into()];
    update_host(&original, &detached, &dir.path().join("app")).unwrap();
    let first = find_host(&source, "first").unwrap();
    let alias = find_host(&source, "alias").unwrap();
    assert_eq!(first.file_tags, ["source"]);
    assert_eq!(alias.file_tags, ["source"]);
    assert_eq!(first.tags, ["detached"]);
    assert_eq!(alias.tags, ["own"]);
    move_host(&alias, &target, &dir.path().join("app")).unwrap();
    assert_eq!(
        find_host(&target, "alias").unwrap().file_tags,
        ["destination"]
    );
    assert_eq!(find_host(&source, "first").unwrap().file_tags, ["source"]);
    assert!(!fs::read_to_string(&target)
        .unwrap()
        .contains("FileTags: source"));
}
