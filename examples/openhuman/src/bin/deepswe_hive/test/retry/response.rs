//! Scripted OpenAI-compatible provider responses for retry tests.

use serde_json::json;
use wiremock::ResponseTemplate;

use super::super::super::DEFAULT_MODEL;

pub(super) fn provider_error_response(status: u16) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "error": {
            "message": if status == 401 {
                "invalid API key"
            } else {
                "provider temporarily overloaded"
            },
            "type": if status == 401 {
                "authentication_error"
            } else {
                "rate_limit_error"
            }
        }
    }))
}

pub(super) fn empty_completion_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id":"chatcmpl-empty",
        "object":"chat.completion",
        "created":1_700_000_000_u64,
        "model":DEFAULT_MODEL,
        "choices":[{
            "index":0,
            "message":{"role":"assistant","content":null},
            "finish_reason":"stop"
        }],
        "usage":{"prompt_tokens":10,"completion_tokens":0,"total_tokens":10}
    }))
}

pub(super) fn completion_response(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id":"chatcmpl-deepswe",
        "object":"chat.completion",
        "created":1_700_000_000_u64,
        "model":DEFAULT_MODEL,
        "choices":[{
            "index":0,
            "message":{"role":"assistant","content":content},
            "finish_reason":"stop"
        }],
        "usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}
    }))
}

pub(super) fn tool_call_response(seat: &str, serial: u32) -> ResponseTemplate {
    hive_action_response(seat, serial, "complete_episode")
}

pub(super) fn hive_action_response(seat: &str, serial: u32, tool: &str) -> ResponseTemplate {
    let message = match tool {
        "complete_episode" => format!("{seat} complete"),
        _ => format!("{seat} {tool}"),
    };
    let arguments = json!({
        "server":format!("tinyhive-{seat}"),
        "tool":tool,
        "arguments":{"message":message}
    })
    .to_string();
    ResponseTemplate::new(200).set_body_json(json!({
        "id":format!("chatcmpl-{seat}-{serial}"),
        "object":"chat.completion",
        "created":1_700_000_000_u64,
        "model":DEFAULT_MODEL,
        "choices":[{
            "index":0,
            "message":{
                "role":"assistant",
                "content":null,
                "tool_calls":[{
                    "id":format!("call-{seat}-{serial}"),
                    "type":"function",
                    "function":{"name":"mcp_call_tool","arguments":arguments}
                }]
            },
            "finish_reason":"tool_calls"
        }],
        "usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}
    }))
}
