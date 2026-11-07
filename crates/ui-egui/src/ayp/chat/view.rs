//! The AI tab: cards, the message box, ⚙ Cài đặt AI and Chỉ chỗ.
use super::cards::{self, Act};
use crate::FilmcraftApp;
use egui::{Event, Key, Rect, RichText, vec2};

const INPUT_H: f32 = 70.0;
const FLASH_SECS: f32 = 1.5;
const KEYCHAIN_ACCOUNT: &str = "anthropic";

fn input_id() -> egui::Id {
    egui::Id::new("ayp.chat.input")
}

/// Track whether an IME is composing in the box, from this frame's events.
fn follow_ime(app: &mut FilmcraftApp, ctx: &egui::Context) {
    use egui::ImeEvent::{Commit, Preedit};
    let focused = ctx.memory(|m| m.has_focus(input_id()));
    ctx.input(|i| {
        for e in &i.events {
            match e {
                Event::Ime(Preedit { text, .. }) => app.ayp_chat.composing = !text.is_empty(),
                Event::Ime(Commit(_)) => app.ayp_chat.composing = false,
                _ => {}
            }
        }
    });
    if !focused {
        app.ayp_chat.composing = false;
    }
}

/// An Enter without Shift pressed this frame, taken out of the input so the box does not add a line.
fn take_enter(ctx: &egui::Context) -> bool {
    let mut found = false;
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            let hit = matches!(e, Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if !modifiers.shift);
            found |= hit;
            !hit
        })
    });
    found
}

fn send_input(app: &mut FilmcraftApp) {
    let text = app.ayp_chat.input.clone();
    app.ui.status.clear();
    match super::send(app, &text) {
        Ok(()) => app.ayp_chat.input.clear(),
        Err(e) => app.ui.status = format!("Chưa làm được: {e}"),
    }
}

fn save_key(app: &mut FilmcraftApp) {
    let key = app.ayp_chat.key_field.trim().to_string();
    app.ayp_chat.note = if key.is_empty() {
        "Chưa nhập khoá".into()
    } else {
        match app.hooks.secret_set.as_mut() {
            None => "Chưa lưu được khoá: app này không có Keychain".into(),
            Some(set) => match set(KEYCHAIN_ACCOUNT, &key) {
                Ok(()) => {
                    app.ayp_chat.key_field.clear();
                    "Đã lưu khoá".into()
                }
                Err(e) => format!("Chưa lưu được khoá: {e}"),
            },
        }
    };
}

/// Chỉ chỗ: open the AYP tab holding `cmd`'s button and outline it; a command without a button opens
/// Tìm lệnh with its name typed in.
pub fn place(app: &mut FilmcraftApp, cmd: &str) {
    match crate::ayp::tabs::find_cmd(cmd) {
        Some((tab, item)) => {
            if crate::ayp::open_tab(app, tab.id).is_ok() {
                app.ui.ayp_flash = Some((item.key.to_string(), web_time::Instant::now()));
            }
        }
        None => app.ui.ayp_palette = Some(super::command_label(cmd)),
    }
}

/// The key of the AYP item to outline now, and when it has run out clears itself.
pub fn flashing(app: &mut FilmcraftApp) -> Option<String> {
    let (key, since) = app.ui.ayp_flash.as_ref()?;
    if since.elapsed().as_secs_f32() < FLASH_SECS {
        return Some(key.clone());
    }
    app.ui.ayp_flash = None;
    None
}

fn line(app: &mut FilmcraftApp, ui: &mut egui::Ui, id: &str, text: &str, y: &mut f32, x: f32, w: f32) {
    let r = Rect::from_min_size(egui::pos2(x, *y), vec2(w, 34.0));
    let l = ui.put(r, egui::Label::new(RichText::new(text).color(app.tokens.text)));
    app.auto.add_response(id, &l, text);
    *y = r.bottom() + 4.0;
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, body: Rect) {
    let ctx = ui.ctx().clone();
    let t = app.tokens;
    let (x, w) = (body.left() + 6.0, body.width() - 12.0);
    let mut y = body.top() + 4.0;
    follow_ime(app, &ctx);
    // the cover that keeps the pointer off the window while the AI works leaves this free
    app.ui.ayp_chat_rect = Some(body);

    if let Some(port) = app.control_busy_port {
        line(app, ui, "ayp.side.portBusy", &format!("Cổng {port} đang bận — AI/MCP đang nối với app khác"), &mut y, x, w);
    }
    let gear = ui.put(Rect::from_min_size(egui::pos2(x, y), vec2(130.0, 24.0)), egui::Button::new("⚙ Cài đặt AI"));
    app.auto.add_response("ayp.chat.settings", &gear, "Cài đặt AI");
    if gear.clicked() {
        app.ayp_chat.settings_open = !app.ayp_chat.settings_open;
        app.ayp_chat.model_field = app.ayp_chat.model.clone();
    }
    let fresh = ui.put(Rect::from_min_size(egui::pos2(x + 136.0, y), vec2(130.0, 24.0)), egui::Button::new("Trò chuyện mới"));
    app.auto.add_response("ayp.chat.new", &fresh, "Trò chuyện mới");
    if fresh.clicked()
        && let Err(e) = super::new_conversation(app)
    {
        app.ui.status = format!("Chưa làm được: {e}");
    }
    y += 28.0;

    if app.ayp_chat.settings_open {
        let key = ui.put(
            Rect::from_min_size(egui::pos2(x, y), vec2(w - 76.0, 24.0)),
            egui::TextEdit::singleline(&mut app.ayp_chat.key_field).password(true).hint_text("Dán khoá Claude (sk-ant-…)"),
        );
        app.auto.add_response("ayp.chat.key", &key, "Khoá Claude");
        let save = ui.put(Rect::from_min_size(egui::pos2(x + w - 70.0, y), vec2(70.0, 24.0)), egui::Button::new("Lưu khoá"));
        app.auto.add_response("ayp.chat.saveKey", &save, "Lưu khoá");
        if save.clicked() {
            save_key(app);
        }
        y += 28.0;
        if !app.ayp_chat.note.is_empty() {
            let note = app.ayp_chat.note.clone();
            line(app, ui, "ayp.chat.note", &note, &mut y, x, w);
        }
        let model = ui.put(Rect::from_min_size(egui::pos2(x, y), vec2(w, 24.0)), egui::TextEdit::singleline(&mut app.ayp_chat.model_field).hint_text("Model"));
        app.auto.add_response("ayp.chat.model", &model, "Model");
        if model.lost_focus() && app.ayp_chat.model_field.trim() != app.ayp_chat.model {
            let m = app.ayp_chat.model_field.clone();
            super::set_model(app, &m);
            app.ayp_chat.model_field = app.ayp_chat.model.clone();
        }
        y += 32.0;
    }

    // cards, sticking to the bottom as they grow
    let input_top = (body.bottom() - INPUT_H - 36.0).max(y + 40.0);
    let cards_rect = Rect::from_min_max(egui::pos2(x, y), egui::pos2(x + w, input_top - 4.0));
    let mut acts = Vec::new();
    let busy = app.ayp_chat.busy();
    let waiting = app.ayp_chat.waiting_card();
    let mut area = ui.new_child(egui::UiBuilder::new().max_rect(cards_rect));
    egui::ScrollArea::vertical().id_salt("ayp.chat.cards").stick_to_bottom(true).auto_shrink([false, false]).show(&mut area, |ui| {
        for (i, card) in app.ayp_chat.cards.iter().enumerate() {
            cards::show(ui, &t, &mut app.auto, i, card, waiting == Some(i), &mut acts);
        }
    });
    for act in acts {
        let r = match act {
            Act::Decide(ok) => super::decide_plan(app, ok),
            Act::EditPlan => {
                app.ayp_chat.input = "Sửa kế hoạch: ".into();
                ctx.memory_mut(|m| m.request_focus(input_id()));
                Ok(())
            }
            Act::Choose(n) => super::choose(app, n),
            Act::Place(cmd) => {
                place(app, &cmd);
                Ok(())
            }
            Act::Undo(card) => super::undo_turn(app, &ctx, card),
        };
        if let Err(e) = r {
            app.ui.status = format!("Chưa làm được: {e}");
        }
    }

    // the message box: Enter sends, Shift+Enter adds a line, Enter while an IME composes is the IME's
    let enter = ctx.memory(|m| m.has_focus(input_id())) && take_enter(&ctx) && !app.ayp_chat.composing;
    let box_rect = Rect::from_min_size(egui::pos2(x, input_top), vec2(w, INPUT_H));
    let input = ui.put(
        box_rect,
        egui::TextEdit::multiline(&mut app.ayp_chat.input)
            .id(input_id())
            .hint_text("Nói AI làm gì… (Enter gửi, Shift+Enter xuống dòng)")
            .desired_width(w)
            .desired_rows(3),
    );
    app.auto.add_response("ayp.chat.input", &input, "Tin nhắn cho AI");
    let btn = Rect::from_min_size(egui::pos2(x + w - 80.0, box_rect.bottom() + 4.0), vec2(80.0, 26.0));
    if busy {
        let stop = ui.put(btn, egui::Button::new("Dừng"));
        app.auto.add_response("ayp.chat.stop", &stop, "Dừng");
        if stop.clicked() {
            super::stop(app);
        }
    } else {
        let can = !app.ayp_chat.input.trim().is_empty();
        let send = ui.put(btn, egui::Button::new("Gửi").fill(if can { t.accent } else { egui::Color32::TRANSPARENT }));
        app.auto.add_response("ayp.chat.send", &send, "Gửi");
        if (send.clicked() && can) || (enter && can) {
            send_input(app);
            input.request_focus();
        }
    }
}

/// While the AI works, the window is covered but for the chat itself, so the pointer cannot reach the
/// timeline, the player, the tabs or Thông số (Dừng and the chat stay free). When the chat is not on
/// screen everything is covered, and a small bar keeps Dừng in reach.
pub fn cover(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let chat = app.ui.ayp_chat_rect.take();
    if !app.ayp_chat.busy() {
        return;
    }
    let s = ctx.content_rect();
    let p = egui::pos2;
    let parts = match chat {
        Some(c) => vec![
            Rect::from_min_max(s.min, p(s.max.x, c.top())),
            Rect::from_min_max(p(s.min.x, c.bottom()), s.max),
            Rect::from_min_max(p(s.min.x, c.top()), p(c.left(), c.bottom())),
            Rect::from_min_max(p(c.right(), c.top()), p(s.max.x, c.bottom())),
        ],
        None => vec![s],
    };
    for (n, r) in parts.into_iter().enumerate().filter(|(_, r)| r.width() > 0.0 && r.height() > 0.0) {
        egui::Area::new(egui::Id::new(("ayp.chat.cover", n))).order(egui::Order::Foreground).fixed_pos(r.min).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click_and_drag());
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::NotAllowed);
            }
        });
    }
    if chat.is_some() {
        return;
    }
    // above the cover
    let t = app.tokens;
    egui::Area::new(egui::Id::new("ayp.chat.stopBar")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 8.0)).show(ctx, |ui| {
        egui::Frame::new().fill(t.field_bg).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("AI đang làm").color(t.text));
                let stop = ui.button("Dừng");
                app.auto.add_response("ayp.chat.stopBar", &stop, "Dừng");
                if stop.clicked() {
                    super::stop(app);
                }
            });
        });
    });
}
