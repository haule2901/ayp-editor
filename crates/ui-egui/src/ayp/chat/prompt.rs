//! The system prompt: Vietnamese role and rules, then the AYP styles and caption rules the app ships.
//! Same inputs give the same bytes, so the prompt cache keeps hitting.

const STYLES: &str = include_str!("../../../../../docs/ayp-styles.md");
const CLAUDE_MD: &str = include_str!("../../../../../CLAUDE.md");
const CAPTION_HEADING: &str = "## AYP caption rules (this fork)";

const ROLE: &str = "\
Bạn là trợ lý biên tập trong AYP Editor, một phần mềm dựng video. Bạn điều khiển phần mềm bằng các công cụ được cung cấp.
Quy tắc:
- Luôn trả lời bằng tiếng Việt, ngắn gọn, đi thẳng vào việc.
- Không đoán id lệnh: gọi find_commands trước rồi mới run_command với đúng id và params nó trả về.
- Muốn biết dự án đang thế nào: get_state, hoặc run_command với lệnh đọc như sequence.inspect, transcript.inspect.
- Trước khi xoá hoặc cắt nhiều clip, thay phụ đề, hoặc làm hơn 3 thay đổi: gọi propose_plan và chờ người dùng đồng ý.
- Khi người dùng phải chọn một phong cách: gọi offer_choices (2 đến 4 lựa chọn).
- Việc dài (ví dụ transcript.generate với background:true): dùng wait_job để chờ xong.
- Không bịa đường dẫn tệp. Chưa có đường dẫn thì hỏi người dùng.
- Phụ đề theo luật AYP bên dưới: bạn nhóm các cụm theo nghĩa rồi gửi transcript.captionsFromChunks.
- Lệnh báo lỗi thì nói rõ lỗi bằng lời thường và đề xuất cách sửa, đừng thử lại mù quáng.";

/// The `## AYP caption rules` section of CLAUDE.md: from its heading up to the next `## ` line.
fn caption_rules(md: &str) -> &str {
    let Some(start) = md.find(CAPTION_HEADING) else { return "" };
    let rest = &md[start..];
    let body = start + CAPTION_HEADING.len();
    let end = md[body..].find("\n## ").map_or(md.len(), |i| body + i);
    rest.get(..end - start).unwrap_or(rest).trim_end()
}

/// `extra_skill`: the text of `~/.claude/skills/ayp-phu-de/SKILL.md` when the caller found it.
pub fn system_prompt(extra_skill: Option<&str>) -> String {
    let mut s = String::from(ROLE);
    s.push_str("\n\n# Phong cách AYP\n");
    s.push_str(STYLES.trim());
    s.push_str("\n\n# Luật phụ đề\n");
    s.push_str(caption_rules(CLAUDE_MD));
    if let Some(skill) = extra_skill.map(str::trim).filter(|k| !k.is_empty()) {
        s.push_str("\n\n# Kỹ năng phụ đề\n");
        s.push_str(skill);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_has_the_caption_rules_and_is_stable() {
        let p = system_prompt(None);
        assert!(p.contains("Rule #1: every caption"), "caption rules missing");
        assert!(p.contains("captionsFromChunks"));
        assert!(!p.contains("Start every session here"), "must stop at the section's end");
        assert!(!p.contains("## Running and looking"));
        assert!(p.contains(STYLES.trim().lines().next().unwrap_or("?")));
        assert_eq!(p, system_prompt(None), "same input, same bytes");
        assert!(ROLE.lines().count() <= 40);
        let with = system_prompt(Some("  SKILL-TEXT \n"));
        assert!(with.ends_with("SKILL-TEXT") && with.starts_with(&p));
    }

    #[test]
    fn missing_heading_does_not_panic() {
        assert_eq!(caption_rules("# nothing here"), "");
        assert_eq!(caption_rules(&format!("{CAPTION_HEADING}\nbody")), format!("{CAPTION_HEADING}\nbody"));
    }
}
