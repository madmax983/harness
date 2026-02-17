//! Parsing helpers for rendering structured knowledge/tool payloads.

use serde_json::Value;

/// Parsed knowledge payload for condensed list rendering + drill-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedKnowledgeContent {
    pub kind: String,
    pub summary: String,
    pub pretty_json: Option<String>,
}

const WORKFLOW_RUN_PREFIX: &str = "__coding_agent_workflow_run_v1__:";
const WORKFLOW_DEF_PREFIX: &str = "__coding_agent_workflow_definition_v1__:";
const SCHEDULE_PREFIX: &str = "__coding_agent_schedule_v1__:";

/// Parse knowledge content into a condensed summary plus optional pretty JSON.
pub fn parse_knowledge_content(content: &str) -> ParsedKnowledgeContent {
    if let Some(parsed) = parse_tagged_payload(content, WORKFLOW_RUN_PREFIX, "workflow_run", |v| {
        let run_id = short_id(value_str(v, "run_id"));
        let workflow_id = short_id(value_str(v, "workflow_id"));
        let status = value_str(v, "status").unwrap_or("-");
        format!("run={run_id} wf={workflow_id} status={status}")
    }) {
        return parsed;
    }

    if let Some(parsed) =
        parse_tagged_payload(content, WORKFLOW_DEF_PREFIX, "workflow_definition", |v| {
            let workflow_id = short_id(value_str(v, "workflow_id"));
            let name = value_str(v, "name").unwrap_or("-");
            let steps = v
                .get("steps")
                .and_then(|steps| steps.as_array().map(std::vec::Vec::len))
                .unwrap_or(0);
            format!("wf={workflow_id} name={name} steps={steps}")
        })
    {
        return parsed;
    }

    if let Some(parsed) = parse_tagged_payload(content, SCHEDULE_PREFIX, "schedule", |v| {
        let schedule_id = short_id(value_str(v, "schedule_id"));
        let name = value_str(v, "name").unwrap_or("-");
        let cadence = v
            .get("cadence_minutes")
            .and_then(Value::as_u64)
            .map(|m| format!("{m}m"))
            .unwrap_or_else(|| "-".to_string());
        format!("schedule={schedule_id} name={name} cadence={cadence}")
    }) {
        return parsed;
    }

    if let Some(generic) = parse_generic_double_underscore_payload(content) {
        return generic;
    }

    ParsedKnowledgeContent {
        kind: "text".to_string(),
        summary: content.to_string(),
        pretty_json: None,
    }
}

fn parse_tagged_payload<F>(
    content: &str,
    prefix: &str,
    kind: &str,
    summarize: F,
) -> Option<ParsedKnowledgeContent>
where
    F: Fn(&Value) -> String,
{
    let payload = content.strip_prefix(prefix)?;
    let parsed_value: Value = serde_json::from_str(payload).ok()?;
    let pretty_json = serde_json::to_string_pretty(&parsed_value).ok();
    Some(ParsedKnowledgeContent {
        kind: kind.to_string(),
        summary: summarize(&parsed_value),
        pretty_json,
    })
}

fn parse_generic_double_underscore_payload(content: &str) -> Option<ParsedKnowledgeContent> {
    let content = content.trim();
    if !content.starts_with("__") {
        return None;
    }

    let after_start = &content[2..];
    let (tag, payload) = after_start.split_once("__:")?;
    let parsed_value: Value = serde_json::from_str(payload).ok()?;
    let pretty_json = serde_json::to_string_pretty(&parsed_value).ok();

    let summary = if parsed_value.is_object() {
        let keys = parsed_value
            .as_object()
            .map(|obj| obj.keys().take(4).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .join(",");
        format!("fields={keys}")
    } else {
        "non-object payload".to_string()
    };

    Some(ParsedKnowledgeContent {
        kind: tag.to_string(),
        summary,
        pretty_json,
    })
}

fn value_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn short_id(value: Option<&str>) -> String {
    let raw = value.unwrap_or("-");
    if raw.len() <= 8 {
        raw.to_string()
    } else {
        raw[..8].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_knowledge_content;

    #[test]
    fn parse_workflow_run_payload_extracts_summary() {
        let parsed = parse_knowledge_content(
            r#"__coding_agent_workflow_run_v1__:{"run_id":"a42e2093-88e2-4fc4-a854-e251caf1fa0a","workflow_id":"wf-123","status":"queued"}"#,
        );
        assert_eq!(parsed.kind, "workflow_run");
        assert!(parsed.summary.contains("a42e2093"));
        assert!(parsed.summary.contains("queued"));
        assert!(parsed.pretty_json.is_some());
    }

    #[test]
    fn parse_plain_content_passthrough() {
        let parsed = parse_knowledge_content("regular text payload");
        assert_eq!(parsed.kind, "text");
        assert_eq!(parsed.summary, "regular text payload");
        assert!(parsed.pretty_json.is_none());
    }
}
