#![allow(dead_code)]
#![allow(deprecated)]

use hyperinfer_core::{ChatMessage, ChatRequest, ChatResponse, MessageRole};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::IntoPyObjectExt;
use pyo3::Py;

pub fn message_from_py(dict: &Bound<'_, PyDict>) -> PyResult<ChatMessage> {
    let role: String = dict
        .get_item("role")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("message missing 'role' field"))?
        .extract()?;

    let content: String = dict
        .get_item("content")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("message missing 'content' field"))?
        .extract()?;

    let role = match role.as_str() {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "invalid role: {}",
                role
            )))
        }
    };

    Ok(ChatMessage { role, content })
}

pub fn request_from_py(_py: Python<'_>, obj: Py<PyAny>) -> PyResult<ChatRequest> {
    let dict = obj.downcast_bound::<PyDict>(_py)?;

    let model: String = dict
        .get_item("model")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("request missing 'model' field"))?
        .extract()?;

    let messages_list: Bound<'_, pyo3::types::PyList> = dict
        .get_item("messages")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("request missing 'messages' field"))?
        .downcast_into()?;

    let mut messages = Vec::new();
    for item in messages_list.iter() {
        let msg_dict: Bound<'_, PyDict> = item.downcast_into()?;
        messages.push(message_from_py(&msg_dict)?);
    }

    let temperature: Option<f64> = dict
        .get_item("temperature")?
        .map(|v: Bound<'_, PyAny>| v.extract())
        .transpose()?;
    let max_tokens: Option<u32> = dict
        .get_item("max_tokens")?
        .map(|v: Bound<'_, PyAny>| v.extract())
        .transpose()?;

    Ok(ChatRequest {
        model,
        messages,
        temperature,
        max_tokens,
    })
}

fn message_role_to_py(py: Python<'_>, role: &MessageRole) -> PyResult<Py<PyAny>> {
    match role {
        MessageRole::System => Ok("system".into_py_any(py)?),
        MessageRole::User => Ok("user".into_py_any(py)?),
        MessageRole::Assistant => Ok("assistant".into_py_any(py)?),
    }
}

pub fn response_to_py(py: Python<'_>, response: ChatResponse) -> PyResult<Py<PyAny>> {
    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("id", &response.id)?;
    dict.set_item("model", &response.model)?;

    let choices_list = pyo3::types::PyList::empty(py);
    for choice in &response.choices {
        let choice_dict = pyo3::types::PyDict::new(py);
        choice_dict.set_item("index", choice.index)?;

        let msg_dict = pyo3::types::PyDict::new(py);
        msg_dict.set_item("role", message_role_to_py(py, &choice.message.role)?)?;
        msg_dict.set_item("content", &choice.message.content)?;
        choice_dict.set_item("message", msg_dict)?;

        choice_dict.set_item("finish_reason", &choice.finish_reason)?;
        choices_list.append(choice_dict)?;
    }
    dict.set_item("choices", choices_list)?;

    let usage_dict = pyo3::types::PyDict::new(py);
    usage_dict.set_item("input_tokens", response.usage.input_tokens)?;
    usage_dict.set_item("output_tokens", response.usage.output_tokens)?;
    dict.set_item("usage", usage_dict)?;

    Ok(dict.into())
}
