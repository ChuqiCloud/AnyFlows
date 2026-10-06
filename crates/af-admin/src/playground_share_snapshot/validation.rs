use std::collections::HashSet;

use af_domain::MAX_MODEL_NAME_BYTES;

use super::{
    MAX_PLAYGROUND_SHARE_MESSAGE_BYTES, MAX_PLAYGROUND_SHARE_MESSAGES,
    MAX_PLAYGROUND_SHARE_SESSIONS, PlaygroundShareInputError, PlaygroundShareMessage,
    PlaygroundShareMessageRole, PlaygroundShareSession,
};

pub(super) fn validate_sessions(
    sessions: &[PlaygroundShareSession],
) -> Result<(), PlaygroundShareInputError> {
    if !(1..=MAX_PLAYGROUND_SHARE_SESSIONS).contains(&sessions.len()) {
        return Err(PlaygroundShareInputError::InvalidSnapshot);
    }
    let mut models = HashSet::with_capacity(sessions.len());
    let mut total_messages = 0_usize;
    for session in sessions {
        if !valid_model_name(&session.model)
            || !models.insert(session.model.as_str())
            || !has_complete_round_trips(&session.messages)
        {
            return Err(PlaygroundShareInputError::InvalidSnapshot);
        }
        total_messages = total_messages
            .checked_add(session.messages.len())
            .ok_or(PlaygroundShareInputError::InvalidSnapshot)?;
    }
    if total_messages > MAX_PLAYGROUND_SHARE_MESSAGES {
        return Err(PlaygroundShareInputError::InvalidSnapshot);
    }
    Ok(())
}

pub(super) fn valid_model_name(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

pub(super) fn has_complete_round_trips(messages: &[PlaygroundShareMessage]) -> bool {
    messages.len() >= 2
        && messages.len().is_multiple_of(2)
        && messages.iter().enumerate().all(|(index, message)| {
            message.content.len() <= MAX_PLAYGROUND_SHARE_MESSAGE_BYTES
                && !message.content.is_empty()
                && !message.content.contains('\0')
                && message.role
                    == if index.is_multiple_of(2) {
                        PlaygroundShareMessageRole::User
                    } else {
                        PlaygroundShareMessageRole::Assistant
                    }
        })
}
