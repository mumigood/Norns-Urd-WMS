use super::*;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

struct Harness {
    ctx: Context,
    value: String,
    input: Rect,
    output: egui::FullOutput,
    commits: Vec<String>,
    changed: bool,
    modal: bool,
    enabled: bool,
}
impl Harness {
    fn new() -> Self {
        Self {
            ctx: Context::default(),
            value: String::new(),
            input: Rect::NOTHING,
            output: Default::default(),
            commits: vec![],
            changed: false,
            modal: false,
            enabled: true,
        }
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.output = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 480.0))),
                events,
                ..Default::default()
            },
            |ui| {
                let mut draw = |ui: &mut Ui| {
                    ui.add_enabled_ui(self.enabled, |ui| {
                        let result = suggest_input(
                            ui,
                            "supplier",
                            &mut self.value,
                            &["Alpha".into(), "Beta".into()],
                            "supplier",
                            200.0,
                        );
                        self.input = result.rect;
                        self.changed = result.changed();
                        if result.committed {
                            self.commits.push(self.value.clone());
                        }
                    });
                    let _ = ui.button("Next field");
                };
                if self.modal {
                    egui::Modal::new(egui::Id::unique("edit")).show(ui.ctx(), draw);
                } else {
                    draw(ui);
                }
            },
        );
        self.output.textures_delta.clear();
    }
    fn pointer(&mut self, pos: Pos2, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
        ]);
    }
    fn open(&mut self) {
        self.frame(vec![]);
        let pos = self.input.center();
        self.pointer(pos, true);
        self.pointer(pos, false);
        self.frame(vec![]);
        self.frame(vec![]);
    }
    fn text_position(&self, label: &str) -> Option<Pos2> {
        fn find(shape: &egui::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() / 2.0),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        self.output.shapes.iter().find_map(|s| find(&s.shape, label))
    }
}

#[test]
fn supplier_selection_survives_pointer_press_and_release() {
    let mut h = Harness::new();
    h.open();
    let option = h.text_position("Beta").expect("suggestions visible");
    h.pointer(option, true);
    h.pointer(option, false);
    assert_eq!(h.value, "Beta");
    assert!(h.changed);
    assert_eq!(h.commits, ["Beta"]);
    h.frame(vec![]);
    assert!(h.text_position("Alpha").is_none());
    assert_eq!(h.commits, ["Beta"]);
}

fn key(key: egui::Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

#[test]
fn supplier_selection_works_in_edit_modal() {
    let mut h = Harness::new();
    h.modal = true;
    h.frame(vec![]);
    h.open();
    let option = h.text_position("Beta").expect("modal suggestions visible");
    h.pointer(option, true);
    h.pointer(option, false);
    assert_eq!(h.value, "Beta");
    assert_eq!(h.commits, ["Beta"]);
}

#[test]
fn escape_dismisses_suggestions_without_reopening() {
    let mut h = Harness::new();
    h.open();
    h.frame(vec![key(egui::Key::Escape)]);
    h.frame(vec![]);
    assert!(h.text_position("Alpha").is_none());
    h.frame(vec![]);
    assert!(h.text_position("Alpha").is_none());
}

#[test]
fn typed_text_commits_on_tab_not_each_keystroke() {
    let mut h = Harness::new();
    h.open();
    h.frame(vec![Event::Text("Al".into())]);
    assert!(h.commits.is_empty());
    h.frame(vec![key(egui::Key::Tab)]);
    h.frame(vec![]);
    assert_eq!(h.value, "Al");
    assert_eq!(h.commits, ["Al"]);
    assert!(h.text_position("Alpha").is_none());
}

#[test]
fn outside_click_commits_free_text_and_closes_popup() {
    let mut h = Harness::new();
    h.open();
    h.frame(vec![Event::Text("Al".into())]);
    let outside = egui::pos2(500.0, 400.0);
    h.pointer(outside, true);
    h.pointer(outside, false);
    h.frame(vec![]);
    assert_eq!(h.commits, ["Al"]);
    assert!(h.text_position("Alpha").is_none());
}

#[test]
fn disabled_supplier_does_not_open() {
    let mut h = Harness::new();
    h.enabled = false;
    h.open();
    assert!(h.text_position("Alpha").is_none());
    assert!(h.commits.is_empty());
}

#[test]
fn chosen_supplier_is_persisted_and_survives_reopening_database() {
    use crate::service::{self, ItemInput, Store};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    crate::seed::demo(&store.conn).unwrap();
    let user = service::login(&store, "peter", "peter0821").unwrap();
    let id = service::create_item(
        &store,
        &user,
        &ItemInput { name: Some("Dropdown regression".into()), ..Default::default() },
    )
    .unwrap();
    let mut h = Harness::new();
    h.open();
    let option = h.text_position("Beta").unwrap();
    h.pointer(option, true);
    assert!(h.commits.is_empty(), "must not save the old value on pointer-down");
    h.pointer(option, false);
    for value in &h.commits {
        service::update_item(&store, &user, id, &ItemInput { supplier: Some(value.clone()), ..Default::default() })
            .unwrap();
    }
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    let supplier: String =
        reopened.conn.query_row("SELECT supplier FROM inventory_item WHERE id=?", [id], |r| r.get(0)).unwrap();
    assert_eq!(supplier, "Beta");
}

#[test]
fn supplier_identity_survives_row_reordering() {
    let ctx = Context::default();
    let mut values = [String::new(), String::new()];
    let mut ids = Vec::new();
    for order in [[0, 1], [1, 0]] {
        let mut current = [None, None];
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            for row in order {
                let r = suggest_input(ui, ("supplier", row), &mut values[row], &[], "", 200.0);
                current[row] = Some(r.id);
            }
        });
        out.textures_delta.clear();
        ids.push(current);
    }
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0][0], ids[0][1]);
}

#[test]
fn fixed_and_searchable_selects_accept_pointer_selection() {
    for searchable in [false, true] {
        let mut h = Harness::new();
        let mut value = None;
        let frame = |h: &mut Harness, value: &mut Option<String>, events| {
            h.output = h.ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(640.0, 480.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if searchable {
                        search_select(
                            ui,
                            "picker",
                            value,
                            &[("Alpha".into(), "Alpha".into(), true), ("Beta".into(), "Beta".into(), true)],
                            "Choose",
                            200.0,
                        );
                    } else {
                        select(ui, "picker", value, &["Alpha".into(), "Beta".into()], "Choose", 200.0, true);
                    }
                },
            );
            h.output.textures_delta.clear();
        };
        frame(&mut h, &mut value, vec![]);
        let pos = h.text_position("Choose").unwrap();
        for pressed in [true, false] {
            frame(
                &mut h,
                &mut value,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
                ],
            );
        }
        frame(&mut h, &mut value, vec![]);
        frame(&mut h, &mut value, vec![]);
        let pos = h.text_position("Beta").unwrap();
        for pressed in [true, false] {
            frame(
                &mut h,
                &mut value,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
                ],
            );
        }
        assert_eq!(value.as_deref(), Some("Beta"), "searchable={searchable}");
    }
}

#[test]
fn enter_commits_typed_value_and_closes_suggestions() {
    let mut h = Harness::new();
    h.open();
    h.frame(vec![Event::Text("Al".into())]);
    h.frame(vec![key(egui::Key::Enter)]);
    h.frame(vec![]);
    assert_eq!(h.commits, ["Al"]);
    assert!(h.text_position("Alpha").is_none());
}
