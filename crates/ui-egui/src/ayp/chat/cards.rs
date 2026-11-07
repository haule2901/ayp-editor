//! The chat's cards, drawn: what each one shows and which button it offers.
use super::{Card, Decision, command_label};
use crate::automation::Registry;
use crate::theme::Tokens;
use egui::{Color32, RichText};

/// What a click on a card asks for; the view applies it once the cards are drawn.
pub enum Act {
    Decide(bool),
    EditPlan,
    Choose(usize),
    Place(String),
    Undo(usize),
}

fn button(ui: &mut egui::Ui, auto: &mut Registry, id: &str, label: &str, enabled: bool) -> bool {
    let r = ui.add_enabled(enabled, egui::Button::new(label));
    auto.add_response(id, &r, label);
    r.clicked()
}

/// Draw card `i`. `open`: it is the plan or the question the conversation waits on (`Chat::waiting_card`).
pub fn show(ui: &mut egui::Ui, t: &Tokens, auto: &mut Registry, i: usize, card: &Card, open: bool, acts: &mut Vec<Act>) {
    let (fill, ink) = match card {
        Card::User(_) => (t.accent, Color32::WHITE),
        Card::Error(_) => (t.field_bg, Color32::from_rgb(235, 110, 110)),
        _ => (t.field_bg, t.text),
    };
    let label = match card {
        Card::User(s) | Card::Text(s) | Card::Error(s) => s.clone(),
        Card::Plan { title, .. } => title.clone(),
        Card::Choice { question, .. } => question.clone(),
        Card::Doing { steps } => steps.iter().map(|s| s.0.as_str()).collect::<Vec<_>>().join(", "),
        Card::Result { label, .. } => label.clone(),
    };
    let w = ui.available_width();
    let frame = egui::Frame::new().fill(fill).corner_radius(8.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(w - 16.0);
        match card {
            Card::User(s) | Card::Text(s) | Card::Error(s) => {
                ui.label(RichText::new(s).color(ink));
            }
            Card::Plan { title, steps, state } => {
                ui.label(RichText::new(title).strong().color(ink));
                for (n, s) in steps.iter().enumerate() {
                    ui.label(RichText::new(format!("{}. {s}", n + 1)).color(ink));
                }
                match state {
                    Decision::Pending if open => {
                        ui.horizontal(|ui| {
                            if button(ui, auto, "ayp.chat.plan.approve", "Làm", true) {
                                acts.push(Act::Decide(true));
                            }
                            if button(ui, auto, "ayp.chat.plan.edit", "Sửa", true) {
                                acts.push(Act::EditPlan);
                            }
                            if button(ui, auto, "ayp.chat.plan.reject", "Bỏ", true) {
                                acts.push(Act::Decide(false));
                            }
                        });
                    }
                    Decision::Pending => {}
                    Decision::Approved => {
                        ui.label(RichText::new("Đã duyệt").color(t.text_dim));
                    }
                    Decision::Rejected => {
                        ui.label(RichText::new("Đã bỏ").color(t.text_dim));
                    }
                }
            }
            Card::Choice { question, options, picked } => {
                ui.label(RichText::new(question).strong().color(ink));
                for (n, (text, _)) in options.iter().enumerate() {
                    let mine = *picked == Some(n);
                    if button(ui, auto, &format!("ayp.chat.choice.{n}"), &if mine { format!("✓ {text}") } else { text.clone() }, open && picked.is_none()) {
                        acts.push(Act::Choose(n));
                    }
                }
            }
            Card::Doing { steps } => {
                for (s, ok) in steps {
                    let (mark, color) = match ok {
                        Some(true) => ("✓", ink),
                        Some(false) => ("✗", Color32::from_rgb(235, 110, 110)),
                        None => ("…", t.text_dim),
                    };
                    ui.label(RichText::new(format!("{mark} {s}")).color(color));
                }
            }
            Card::Result { label, commands, undone, .. } => {
                ui.label(RichText::new(label).strong().color(ink));
                for (n, cmd) in commands.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(command_label(cmd)).color(ink));
                        if button(ui, auto, &format!("ayp.chat.place.{i}.{n}"), "Chỉ chỗ", true) {
                            acts.push(Act::Place(cmd.clone()));
                        }
                    });
                }
                if *undone {
                    ui.label(RichText::new("Đã hoàn tác").color(t.text_dim));
                } else if button(ui, auto, &format!("ayp.chat.undoTurn.{i}"), "Hoàn tác cả lượt", true) {
                    acts.push(Act::Undo(i));
                }
            }
        }
    });
    auto.add_response(&format!("ayp.chat.card.{i}"), &frame.response, &label);
    ui.add_space(4.0);
}
