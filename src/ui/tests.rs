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
