use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

pub struct PythonProvider {
    name: &'static str,
    chat_callable: Py<PyAny>,
    stream_callable: Option<Py<PyAny>>,
}

impl PythonProvider {
    pub fn new(
        name: &'static str,
        chat_callable: Py<PyAny>,
        stream_callable: Option<Py<PyAny>>,
    ) -> Self {
        Self {
            name,
            chat_callable,
            stream_callable,
        }
    }

    pub fn chat(
        &self,
        py: Python<'_>,
        request: &hyperinfer_core::ChatRequest,
    ) -> PyResult<hyperinfer_core::ChatResponse> {
        let dict = PyDict::new(py);
        dict.set_item("model", &request.model)?;

        let messages = PyList::empty(py);
        for msg in &request.messages {
            let msg_dict = PyDict::new(py);
            let role_str = match msg.role {
                hyperinfer_core::MessageRole::System => "system",
                hyperinfer_core::MessageRole::User => "user",
                hyperinfer_core::MessageRole::Assistant => "assistant",
            };
            msg_dict.set_item("role", role_str)?;
            msg_dict.set_item("content", &msg.content)?;
            messages.append(msg_dict)?;
        }
        dict.set_item("messages", messages)?;

        if let Some(temp) = request.temperature {
            dict.set_item("temperature", temp)?;
        }
        if let Some(max_tokens) = request.max_tokens {
            dict.set_item("max_tokens", max_tokens)?;
        }
        if let Some(stop) = &request.stop {
            let stop_list = PyList::empty(py);
            for s in stop {
                stop_list.append(s)?;
            }
            dict.set_item("stop", stop_list)?;
        }

        let result = self.chat_callable.call1(py, (dict,))?;

        self.py_dict_to_chat_response(result.bind(py).cast::<PyDict>()?)
    }

    fn py_dict_to_chat_response(
        &self,
        dict: &Bound<'_, PyDict>,
    ) -> PyResult<hyperinfer_core::ChatResponse> {
        let id: String = dict.get_item("id")?.unwrap().extract()?;
        let model: String = dict.get_item("model")?.unwrap().extract()?;

        let choices_list = dict.get_item("choices")?.unwrap();
        let py_choices = choices_list.cast::<PyList>()?;
        let mut choices = Vec::new();

        for (idx, item) in py_choices.iter().enumerate() {
            let choice_dict = item.cast::<PyDict>()?;
            let message = choice_dict.get_item("message")?.unwrap();
            let msg_dict = message.cast::<PyDict>()?;

            let role_str: String = msg_dict.get_item("role")?.unwrap().extract()?;
            let content: String = msg_dict.get_item("content")?.unwrap().extract()?;
            let role = match role_str.as_str() {
                "system" => hyperinfer_core::MessageRole::System,
                "user" => hyperinfer_core::MessageRole::User,
                _ => hyperinfer_core::MessageRole::Assistant,
            };

            let finish_reason =
                choice_dict
                    .get_item("finish_reason")?
                    .and_then(
                        |f: Bound<'_, PyAny>| {
                            if f.is_none() {
                                None
                            } else {
                                f.extract().ok()
                            }
                        },
                    );

            choices.push(hyperinfer_core::Choice {
                index: idx as u32,
                message: hyperinfer_core::ChatMessage { role, content },
                finish_reason,
            });
        }

        let usage = if let Some(u) = dict.get_item("usage")? {
            if !u.is_none() {
                let usage_dict = u.cast::<PyDict>()?;
                Some(hyperinfer_core::Usage {
                    input_tokens: usage_dict.get_item("input_tokens")?.unwrap().extract()?,
                    output_tokens: usage_dict.get_item("output_tokens")?.unwrap().extract()?,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(hyperinfer_core::ChatResponse {
            id,
            model,
            choices,
            usage: usage.unwrap_or_default(),
        })
    }
}

impl Clone for PythonProvider {
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            name: self.name,
            chat_callable: self.chat_callable.clone_ref(py),
            stream_callable: self.stream_callable.as_ref().map(|c| c.clone_ref(py)),
        })
    }
}

#[async_trait::async_trait]
impl hyperinfer_providers::LlmProvider for PythonProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn supports_streaming(&self) -> bool {
        self.stream_callable.is_some()
    }

    async fn chat(
        &self,
        request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> Result<hyperinfer_core::ChatResponse, hyperinfer_core::HyperInferError> {
        let provider_clone = self.clone();
        let request_clone = request.clone();

        let result = tokio::task::spawn_blocking(move || {
            Python::attach(|py| provider_clone.chat(py, &request_clone))
        })
        .await
        .map_err(|e| hyperinfer_core::HyperInferError::ApiError {
            status: 500,
            message: format!("Task panic: {}", e),
        })?;

        result.map_err(|e| hyperinfer_core::HyperInferError::ApiError {
            status: 500,
            message: format!("Python provider error: {}", e),
        })
    }

    fn stream(
        &self,
        _request: &hyperinfer_core::ChatRequest,
        _api_key: &str,
    ) -> std::pin::Pin<
        Box<
            dyn futures::Stream<
                    Item = Result<hyperinfer_core::ChatChunk, hyperinfer_core::HyperInferError>,
                > + Send
                + '_,
        >,
    > {
        unimplemented!("Python provider streaming not implemented yet")
    }
}
