use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{ConversationId, Event};

/// Helper function for serde to skip serializing false boolean values.
fn is_false(value: &bool) -> bool {
    !value
}

/// Represents a chat request submitted to the forge agent orchestration layer.
///
/// The `internal` flag signals that this request originates from an automated
/// internal path (e.g. a sub-agent dispatch, title generation, or any
/// orchestration-driven call) rather than from a direct human interaction.
/// Provider layers use this flag — in combination with `RequestInitiator` on
/// `Context` — to classify Copilot premium requests correctly and avoid
/// unintended amplification.
#[derive(Debug, Serialize, Deserialize, Clone, Setters)]
#[setters(into, strip_option)]
pub struct ChatRequest {
    /// The triggering event for this chat interaction.
    pub event: Event,
    /// Identifies the conversation this request belongs to.
    pub conversation_id: ConversationId,
    /// When `true`, indicates this request was initiated internally (e.g. by a
    /// sub-agent or the title generator) rather than directly by a human user.
    /// Defaults to `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
}

impl ChatRequest {
    /// Creates a new `ChatRequest` for a direct user interaction.
    ///
    /// `internal` defaults to `false`, meaning the request is treated as a
    /// top-level user-originated request unless explicitly overridden via the
    /// `internal` setter.
    pub fn new(content: Event, conversation_id: ConversationId) -> Self {
        Self { event: content, conversation_id, internal: false }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::EventValue;

    fn fixture_request(internal: bool) -> ChatRequest {
        let event = Event::new(EventValue::text("hello"));
        let conversation_id = ConversationId::generate();
        ChatRequest { event, conversation_id, internal }
    }

    #[test]
    fn test_chat_request_new_defaults_internal_to_false() {
        let event = Event::new(EventValue::text("hello"));
        let conversation_id = ConversationId::generate();
        let actual = ChatRequest::new(event, conversation_id);
        let expected = false;
        assert_eq!(actual.internal, expected);
    }

    #[test]
    fn test_chat_request_internal_flag_can_be_set_true() {
        let fixture = fixture_request(false);
        let actual = fixture.internal(true);
        assert_eq!(actual.internal, true);
    }

    #[test]
    fn test_chat_request_internal_false_is_not_serialized() {
        let fixture = fixture_request(false);
        let json = serde_json::to_value(&fixture).unwrap();
        let actual = json.get("internal");
        // `internal: false` should be omitted from the serialized output
        assert_eq!(actual, None);
    }

    #[test]
    fn test_chat_request_internal_true_is_serialized() {
        let fixture = fixture_request(true);
        let json = serde_json::to_value(&fixture).unwrap();
        let actual = json.get("internal").and_then(|v| v.as_bool());
        assert_eq!(actual, Some(true));
    }

    #[test]
    fn test_chat_request_internal_defaults_to_false_on_deserialize() {
        let event = Event::new(EventValue::text("hello"));
        let conversation_id = ConversationId::generate();
        let original = ChatRequest::new(event, conversation_id);
        // Serialize (without internal field) then deserialize
        let json = serde_json::to_string(&original).unwrap();
        let actual: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(actual.internal, false);
    }
}
