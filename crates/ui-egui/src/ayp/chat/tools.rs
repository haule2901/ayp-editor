//! The six tools the chat gives Claude, and how each call is carried out.
use crate::FilmcraftApp;
use crate::ayp::{palette, vi};
use serde_json::{Value, json};

/// What a tool call came to; the chat loop turns it into a `tool_result` or a card for the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done(Value),
    Error(String),
    Plan { title: String, steps: Vec<String> },
    Choice { question: String, options: Vec<(String, String)> },
    WaitJob(u64),
}

const MAX_CHARS: usize = 20_000;
const NEEDS_PLAN: &str = "Việc này đổi nhiều thứ: gọi propose_plan trước";

fn tool(name: &str, description: &str, strict: bool, properties: Value) -> Value {
    let required: Vec<&String> = properties.as_object().map(|o| o.keys().collect()).unwrap_or_default();
    json!({
        "name": name,
        "description": description,
        "strict": strict,
        "input_schema": {"type": "object", "properties": properties, "required": required, "additionalProperties": false},
    })
}

/// The tool list sent with every request. Fixed order and bytes, so the prompt cache holds.
pub fn schemas() -> Value {
    json!([
        tool(
            "find_commands",
            "Search the editor's commands by words, Vietnamese (with or without accents) or English. Use it before run_command whenever you are not sure of an id. Each hit has the real command id, ready-made params (a JSON object, or a description of the params when the command needs your own values), and whether it can run now.",
            true,
            json!({"query": {"type": "string", "description": "What to do, e.g. \"giảm ồn\" or \"remove pauses\"."}})
        ),
        tool(
            "run_command",
            "Run one editor command by id with a params object ({} when none). Read commands (sequence.inspect, transcript.inspect) are free; commands that change the project count as edits. Call propose_plan before deleting or cutting many clips, replacing captions, or making more than 3 edits.",
            // ponytail: an open `params` object is not allowed in strict mode, so this one tool is not strict
            false,
            json!({
                "id": {"type": "string", "description": "Command id from find_commands."},
                "params": {"type": "object", "description": "Command parameters; {} when none."}
            })
        ),
        tool(
            "get_state",
            "Read the editor's current state: workspace, open tab, selection, playhead, active sequence and the last 5 undo labels.",
            true,
            json!({})
        ),
        tool(
            "wait_job",
            "Wait for a background job (for example the job id returned by transcript.generate with background:true) to finish. Use it after long jobs instead of polling.",
            true,
            json!({"job": {"type": "integer", "description": "Job id."}})
        ),
        tool(
            "propose_plan",
            "Show the user a plan of 1 to 12 steps and wait for approval. Use it before deleting or cutting many clips, replacing captions, or making more than 3 edits. Do not run those edits until the user approves.",
            true,
            json!({
                "title": {"type": "string"},
                "steps": {"type": "array", "items": {"type": "string"}, "description": "1 to 12 short steps."}
            })
        ),
        tool(
            "offer_choices",
            "Ask the user to pick one of 2 to 4 options (no fewer, no more), for example which caption or edit style to use. The chosen label comes back as the answer.",
            true,
            json!({
                "question": {"type": "string"},
                "options": {"type": "array", "description": "2 to 4 options.", "items": {
                    "type": "object",
                    "properties": {"label": {"type": "string"}, "detail": {"type": "string"}},
                    "required": ["label", "detail"],
                    "additionalProperties": false
                }}
            })
        ),
    ])
}

/// At most 20 000 characters; the rest is counted, not sent.
pub fn cap(text: String) -> String {
    let Some((cut, _)) = text.char_indices().nth(MAX_CHARS) else { return text };
    let dropped = text[cut..].chars().count();
    format!("{}…(cắt bớt {dropped} ký tự)", &text[..cut])
}

fn find_commands(app: &FilmcraftApp, query: &str) -> Value {
    use palette::Run;
    let hits = palette::search(app, query, 20).into_iter().map(|h| {
        // the pseudo id of a tab item is not a command: give the real one and the params its button sends
        let (id, params) = match h.run {
            Run::Item(i) => (i.cmd, serde_json::from_str(i.params).unwrap_or_else(|_| json!({}))),
            Run::Tool(c, p) => (c, serde_json::from_str(p).unwrap_or_else(|_| json!({}))),
            Run::Command(c) => (c, json!(filmcraft_engine::find_command(c).map_or("", |s| s.params))),
        };
        let mut o = json!({"id": id, "label": h.label_en, "labelVi": h.label_vi, "params": params, "enabled": h.enabled.is_ok()});
        if let Err(why) = &h.enabled {
            o["reason"] = json!(vi::reason(why));
        }
        o
    });
    Value::Array(hits.collect())
}

fn state(app: &FilmcraftApp) -> Value {
    let mut s = crate::ayp::state_json(app);
    let undo = &app.session.history.undo;
    s["history"] = undo.iter().skip(undo.len().saturating_sub(5)).map(|(label, _)| label.as_str()).collect();
    s
}

fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str, Outcome> {
    input.get(key).and_then(Value::as_str).ok_or_else(|| Outcome::Error(format!("thiếu `{key}` (chuỗi)")))
}

fn plan(input: &Value) -> Result<Outcome, Outcome> {
    let title = text(input, "title")?.to_string();
    let steps: Vec<String> =
        input.get("steps").and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if !(1..=12).contains(&steps.len()) {
        return Err(Outcome::Error("`steps` cần 1 đến 12 bước".into()));
    }
    Ok(Outcome::Plan { title, steps })
}

fn choices(input: &Value) -> Result<Outcome, Outcome> {
    let question = text(input, "question")?.to_string();
    let options: Vec<(String, String)> = input
        .get("options")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|o| Some((o.get("label")?.as_str()?.to_string(), o.get("detail")?.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    if !(2..=4).contains(&options.len()) {
        return Err(Outcome::Error("`options` cần 2 đến 4 lựa chọn, mỗi lựa chọn có label và detail".into()));
    }
    Ok(Outcome::Choice { question, options })
}

fn run_command(app: &mut FilmcraftApp, ctx: &egui::Context, input: &Value, edits_without_plan: usize) -> Result<Outcome, Outcome> {
    let id = text(input, "id")?;
    let params = match input.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(p) if p.is_object() => p.clone(),
        Some(_) => return Err(Outcome::Error("`params` phải là một đối tượng JSON".into())),
    };
    // UI-only commands (menus::UI_COMMANDS) are not in the engine list: they count as not journaled
    if edits_without_plan >= 3 && filmcraft_engine::find_command(id).is_some_and(|c| c.journal) {
        return Err(Outcome::Error(NEEDS_PLAN.into()));
    }
    crate::menus::invoke(app, ctx, id, params).map(Outcome::Done).map_err(|e| Outcome::Error(vi::reason(&e)))
}

/// Carry out one tool call. `edits_without_plan` counts the project-changing commands run since the
/// last approved plan; from the 4th on, `run_command` asks for a plan first.
pub fn run(app: &mut FilmcraftApp, ctx: &egui::Context, name: &str, input: &Value, edits_without_plan: usize) -> Outcome {
    let r = match name {
        "find_commands" => text(input, "query").map(|q| Outcome::Done(find_commands(app, q))),
        "run_command" => run_command(app, ctx, input, edits_without_plan),
        "get_state" => Ok(Outcome::Done(state(app))),
        "wait_job" => input.get("job").and_then(Value::as_u64).map(Outcome::WaitJob).ok_or_else(|| Outcome::Error("thiếu `job` (số nguyên)".into())),
        "propose_plan" => plan(input),
        "offer_choices" => choices(input),
        other => Err(Outcome::Error(format!("không có công cụ `{other}`"))),
    };
    r.unwrap_or_else(|e| e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ayp::library::tests::demo;

    fn call(app: &mut FilmcraftApp, ctx: &egui::Context, name: &str, input: Value, n: usize) -> Outcome {
        run(app, ctx, name, &input, n)
    }

    #[test]
    fn schemas_are_strict_and_ordered() {
        let s = schemas();
        let names: Vec<&str> = s.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["find_commands", "run_command", "get_state", "wait_job", "propose_plan", "offer_choices"]);
        for t in s.as_array().unwrap() {
            let n = t["name"].as_str().unwrap();
            assert!(t["description"].as_str().unwrap().len() > 30, "{n}");
            let sch = &t["input_schema"];
            assert_eq!(sch["additionalProperties"], json!(false), "{n}");
            let props: Vec<&str> = sch["properties"].as_object().unwrap().keys().map(String::as_str).collect();
            let mut req: Vec<&str> = sch["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            let mut props = props;
            props.sort_unstable();
            req.sort_unstable();
            assert_eq!(props, req, "{n}: strict tools list every property as required");
            // run_command.params is an open object, which strict mode rejects: that one tool is not strict
            assert_eq!(t["strict"].as_bool().unwrap(), n != "run_command", "{n}");
        }
    }

    #[test]
    fn strict_schemas_use_only_supported_keywords() {
        fn walk(v: &Value, path: &str) {
            match v {
                Value::Object(o) => {
                    for (k, x) in o {
                        let banned = ["minItems", "maxItems", "minimum", "maximum", "multipleOf", "minLength", "maxLength"];
                        assert!(!banned.contains(&k.as_str()), "{path}/{k} is not supported in strict schemas");
                        walk(x, &format!("{path}/{k}"));
                    }
                }
                Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| walk(x, &format!("{path}/{i}"))),
                _ => {}
            }
        }
        walk(&schemas(), "");
    }

    #[test]
    fn find_commands_finds_vietnamese() {
        let (mut app, ctx) = demo();
        let Outcome::Done(v) = call(&mut app, &ctx, "find_commands", json!({"query": "giảm ồn"}), 0) else { panic!("not done") };
        let hits = v.as_array().unwrap();
        let denoise = hits.iter().find(|h| h["params"] == json!({"effect": "denoise"})).expect("a denoise hit");
        assert_eq!(denoise["id"], "effects.apply", "the real command id, not the pseudo key");
        assert_eq!(denoise["labelVi"], "Giảm ồn");
        assert!(denoise["enabled"].is_boolean());
    }

    #[test]
    fn bad_tool_calls_come_back_as_errors() {
        let (mut app, ctx) = demo();
        for (name, input) in [
            ("run_command", json!({"id": "no.such.command", "params": {}})),
            ("run_command", json!({"params": {}})),
            ("run_command", json!({"id": "sequence.inspect", "params": [1, 2]})),
            ("find_commands", json!({"query": 5})),
            ("wait_job", json!({"job": "x"})),
            ("propose_plan", json!({"title": "t", "steps": []})),
            ("offer_choices", json!({"question": "q", "options": [{"label": "a", "detail": "b"}]})),
            ("nope", json!({})),
        ] {
            let o = call(&mut app, &ctx, name, input.clone(), 0);
            assert!(matches!(o, Outcome::Error(_)), "{name} {input} gave {o:?}");
        }
    }

    #[test]
    fn good_cards_and_jobs_map_to_their_outcome() {
        let (mut app, ctx) = demo();
        assert_eq!(call(&mut app, &ctx, "wait_job", json!({"job": 7}), 0), Outcome::WaitJob(7));
        assert_eq!(
            call(&mut app, &ctx, "propose_plan", json!({"title": "T", "steps": ["a", "b"]}), 0),
            Outcome::Plan { title: "T".into(), steps: vec!["a".into(), "b".into()] }
        );
        let o = call(&mut app, &ctx, "offer_choices", json!({"question": "Q", "options": [{"label": "A", "detail": "a"}, {"label": "B", "detail": "b"}]}), 0);
        assert_eq!(o, Outcome::Choice { question: "Q".into(), options: vec![("A".into(), "a".into()), ("B".into(), "b".into())] });
        let Outcome::Done(s) = call(&mut app, &ctx, "get_state", json!({}), 0) else { panic!("state") };
        assert!(s["history"].is_array() && s["tab"].is_string());
    }

    #[test]
    fn big_changes_need_a_plan() {
        let (mut app, ctx) = demo();
        let params = json!({"id": "edit.undo", "params": {}});
        // a read command is never held back
        assert!(matches!(call(&mut app, &ctx, "run_command", json!({"id": "sequence.inspect", "params": {}}), 9), Outcome::Done(_)));
        let o = call(&mut app, &ctx, "run_command", params.clone(), 3);
        assert_eq!(o, Outcome::Error("Việc này đổi nhiều thứ: gọi propose_plan trước".into()));
        assert!(!matches!(call(&mut app, &ctx, "run_command", params, 2), Outcome::Error(m) if m.contains("propose_plan")));
    }

    #[test]
    fn tool_output_is_capped() {
        let big = "ọ".repeat(25_000);
        let c = cap(big);
        assert!(c.ends_with("…(cắt bớt 5000 ký tự)"), "{}", &c[c.len() - 40..]);
        assert_eq!(c.chars().count(), 20_000 + "…(cắt bớt 5000 ký tự)".chars().count());
        assert_eq!(cap("ngắn".into()), "ngắn");
    }
}
