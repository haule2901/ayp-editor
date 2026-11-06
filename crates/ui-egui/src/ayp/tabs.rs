//! The ten tabs of the AYP left panel and the built-in features each one holds.
//! Static data only: the panel, the command finder and the control API read `TABS`.
//! Position / scale / rotation / opacity / keyframes are not buttons here, they live in the
//! Thông số tab (Task 6).
use crate::dock::PanelKind;
use crate::icons::Icon;

pub struct Item {
    pub cmd: &'static str,
    /// JSON object passed to the command; `{}` lets the command open its own dialog.
    pub params: &'static str,
    /// Vietnamese label.
    pub label: &'static str,
    /// Unique across `TABS`: the command id, or `effects.apply.<effect>` for effects.
    pub key: &'static str,
}

pub struct Tab {
    pub id: &'static str,
    pub icon: Icon,
    pub label: &'static str,
    pub items: &'static [Item],
    pub embed: Option<PanelKind>,
}

const fn it(cmd: &'static str, params: &'static str, label: &'static str, key: &'static str) -> Item {
    Item { cmd, params, label, key }
}
const fn c(cmd: &'static str, label: &'static str) -> Item {
    it(cmd, "{}", label, cmd)
}

pub const TABS: &[Tab] = &[
    Tab { id: "media", icon: Icon::Folder, label: "Phương tiện", embed: Some(PanelKind::Project), items: &[
        c("file.import", "Nhập tệp…"),
        c("audio.voiceover.start", "Bắt đầu ghi âm lời"),
        c("audio.voiceover.stop", "Dừng ghi âm"),
        c("export.quick", "Xuất nhanh MP4"),
        c("export.queue.add", "Thêm vào hàng đợi xuất"),
    ]},
    Tab { id: "audio", icon: Icon::Speaker, label: "Âm thanh", embed: Some(PanelKind::EssentialSound), items: &[
        c("clip.extractAudio", "Tách âm thanh"),
        it("effects.apply", r#"{"effect":"denoise"}"#, "Giảm ồn", "effects.apply.denoise"),
        it("effects.apply", r#"{"effect":"speech_enhance"}"#, "Làm rõ giọng", "effects.apply.speech_enhance"),
        c("essentialSound.autoMatch", "Chuẩn hoá âm lượng"),
        c("essentialSound.generateDucking", "Nhạc nền tự nhỏ khi có lời"),
        it("sequence.applyAudioTransition", r#"{"effect":"constant_power","frames":24,"edge":"in"}"#, "Rõ dần", "audio.fadeIn"),
        it("sequence.applyAudioTransition", r#"{"effect":"constant_power","frames":24,"edge":"out"}"#, "Mờ dần", "audio.fadeOut"),
    ]},
    Tab { id: "text", icon: Icon::Type, label: "Văn bản", embed: Some(PanelKind::EssentialGraphics), items: &[
        it("graphics.newText", r#"{"text":"Chữ mới","font":"Be Vietnam Pro","fontStyle":"SemiBold"}"#, "Thêm chữ", "graphics.newText"),
        it("graphics.set", r#"{"props":{"font":"Be Vietnam Pro","font_style":"SemiBold","tracking":-60}}"#, "Kiểu chữ AYP (giãn −60)", "graphics.set"),
    ]},
    Tab { id: "captions", icon: Icon::Captions, label: "Phụ đề", embed: Some(PanelKind::Text), items: &[
        c("transcript.createCaptionsVi", "Phụ đề tiếng Việt…"),
        c("transcript.captionsFromChunks", "Chia cụm theo nghĩa"),
        c("captions.applyPreset", "Kiểu phụ đề lưu sẵn…"),
    ]},
    Tab { id: "cut", icon: Icon::Razor, label: "Cắt bằng chữ", embed: Some(PanelKind::Text), items: &[
        c("transcript.removePauses", "Xoá khoảng lặng"),
        c("transcript.removeFillers", "Xoá từ đệm"),
        c("transcript.lift", "Xoá chữ chọn (để lại khoảng trống)"),
        c("transcript.extract", "Xoá chữ chọn (kéo khít)"),
        c("markers.add", "Thêm marker"),
        c("multicam.cut", "Cắt sang góc máy"),
    ]},
    Tab { id: "voice", icon: Icon::Mic, label: "Giọng đọc", embed: Some(PanelKind::TextToSpeech), items: &[
        c("tts.create", "Tạo giọng đọc…"),
    ]},
    Tab { id: "effects", icon: Icon::Fx, label: "Hiệu ứng", embed: Some(PanelKind::Effects), items: &[
        it("effects.apply", r#"{"effect":"skin_smooth"}"#, "Làm mịn da", "effects.apply.skin_smooth"),
        it("effects.apply", r#"{"effect":"gaussian_blur"}"#, "Làm mờ", "effects.apply.gaussian_blur"),
        it("effects.apply", r#"{"effect":"warp_stabilizer"}"#, "Chống rung", "effects.apply.warp_stabilizer"),
        it("effects.apply", r#"{"effect":"auto_reframe"}"#, "Tự khung hình 9:16", "effects.apply.auto_reframe"),
        it("effects.apply", r#"{"effect":"ultra_key"}"#, "Xoá phông xanh", "effects.apply.ultra_key"),
        it("effects.apply", r#"{"effect":"horizontal_flip"}"#, "Lật ngang", "effects.apply.horizontal_flip"),
        it("effects.apply", r#"{"effect":"crop"}"#, "Cắt khung", "effects.apply.crop"),
        c("clip.speedDuration", "Tốc độ / đảo ngược…"),
        c("clip.frameHold", "Dừng hình"),
        it("masks.add", r#"{"shape":"ellipse"}"#, "Tạo mặt nạ", "masks.add"),
        c("masks.track", "Mặt nạ bám theo vật"),
        it("clip.nest", r#"{"name":"Nest"}"#, "Gộp thành nest", "clip.nest"),
        c("clip.group", "Nhóm clip"),
    ]},
    Tab { id: "transitions", icon: Icon::Slide, label: "Chuyển tiếp", embed: Some(PanelKind::Effects), items: &[
        it("sequence.applyVideoTransition", r#"{"effect":"cross_dissolve","frames":12}"#, "Chuyển cảnh mờ", "sequence.applyVideoTransition"),
    ]},
    Tab { id: "color", icon: Icon::Adjust, label: "Màu", embed: Some(PanelKind::LumetriColor), items: &[
        c("lumetri.applyPreset", "Bộ lọc màu…"),
        c("lut.import", "Nhập LUT…"),
        c("lumetri.applyMatch", "Khớp màu"),
    ]},
    Tab { id: "motion", icon: Icon::Sparkle, label: "Motion AYP", embed: None, items: &[
        it("effects.apply", r#"{"effect":"gaussian_blur"}"#, "Làm mờ cả video khi có minh hoạ", "motion.blur"),
        c("review.open", "Duyệt góp ý…"),
        c("review.import", "Góp ý thành marker"),
    ]},
];

/// First tab/item that runs `cmd` (items with parameters of their own share a command id).
pub fn find_cmd(cmd: &str) -> Option<(&'static Tab, &'static Item)> {
    TABS.iter().find_map(|t| t.items.iter().find(|i| i.cmd == cmd).map(|i| (t, i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_ids_are_the_ten_tabs() {
        let ids: Vec<_> = TABS.iter().map(|t| t.id).collect();
        assert_eq!(ids, ["media", "audio", "text", "captions", "cut", "voice", "effects", "transitions", "color", "motion"]);
    }

    #[test]
    fn every_item_is_a_real_command_with_json_params() {
        for t in TABS {
            for i in t.items {
                assert!(
                    filmcraft_engine::find_command(i.cmd).is_some() || crate::menus::UI_COMMANDS.iter().any(|u| u.id == i.cmd),
                    "{}",
                    i.cmd
                );
                let v = serde_json::from_str::<serde_json::Value>(i.params).unwrap_or_else(|e| panic!("{}: {e}", i.cmd));
                assert!(v.is_object(), "{}", i.cmd);
            }
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut k: Vec<_> = TABS.iter().flat_map(|t| t.items.iter().map(|i| i.key)).collect();
        let n = k.len();
        k.sort();
        k.dedup();
        assert_eq!(k.len(), n);
    }

    #[test]
    fn effect_items_name_real_effects() {
        let ids: Vec<_> = filmcraft_project::effect_defs().iter().map(|d| d.id).collect();
        let mut n = 0;
        for i in TABS.iter().flat_map(|t| t.items) {
            if matches!(i.cmd, "effects.apply" | "sequence.applyVideoTransition" | "sequence.applyAudioTransition") {
                let e = serde_json::from_str::<serde_json::Value>(i.params).unwrap()["effect"].as_str().unwrap().to_owned();
                assert!(ids.contains(&e.as_str()), "{} names unknown effect {e}", i.key);
                n += 1;
            }
        }
        assert!(n >= 10);
    }

    #[test]
    fn the_41_features_are_placed() {
        for c in [
            "file.import", "audio.voiceover.start", "clip.extractAudio", "essentialSound.autoMatch", "essentialSound.generateDucking",
            "graphics.newText", "transcript.createCaptionsVi", "transcript.captionsFromChunks", "captions.applyPreset",
            "transcript.removePauses", "transcript.removeFillers", "transcript.lift", "tts.create",
            "sequence.applyVideoTransition", "lumetri.applyPreset", "lut.import", "lumetri.applyMatch", "clip.speedDuration",
            "clip.frameHold", "masks.add", "masks.track", "clip.nest", "clip.group", "multicam.cut", "markers.add", "review.open",
            "review.import", "export.quick", "export.queue.add",
        ] {
            assert!(find_cmd(c).is_some(), "{c} không nằm ở thẻ nào");
        }
        for e in ["skin_smooth", "gaussian_blur", "warp_stabilizer", "auto_reframe", "ultra_key", "horizontal_flip", "crop", "denoise", "speech_enhance"] {
            assert!(TABS.iter().flat_map(|t| t.items).any(|i| i.key == format!("effects.apply.{e}")), "{e}");
        }
        assert!(TABS.iter().map(|t| t.items.len()).sum::<usize>() >= 41);
    }
}
