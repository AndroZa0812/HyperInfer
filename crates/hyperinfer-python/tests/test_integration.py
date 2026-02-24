"""Integration tests for Phase 2 - Python SDK, LangChain, LlamaIndex."""

import pytest
from hyperinfer import Client, Config
from hyperinfer_langchain import HyperInferChatModel
from hyperinfer_llamaindex import HyperInferLLM


@pytest.fixture
def config():
    return (
        Config()
        .with_api_key("openai", "sk-test")
        .with_alias("fast", "gpt-4o-mini")
        .with_default_provider("openai")
    )


def test_config_fluent_api(config):
    config = (
        Config()
        .with_api_key("openai", "sk-test")
        .with_api_key("anthropic", "sk-ant-test")
        .with_alias("smart", "gpt-4")
        .with_quota("team-a", max_requests_per_minute=100)
    )

    d = config.to_dict()
    assert "openai" in d["api_keys"]
    assert "anthropic" in d["api_keys"]
    assert d["model_aliases"]["smart"] == "gpt-4"


def test_langchain_model_properties(config):
    model = HyperInferChatModel(model="gpt-4o", temperature=0.7)
    assert model.model == "gpt-4o"
    assert model.temperature == 0.7
    assert model._llm_type == "hyperinfer"


def test_llamaindex_model_properties(config):
    llm = HyperInferLLM(model="gpt-4o", context_window=8192)
    assert llm.model == "gpt-4o"
    assert llm.metadata.context_window == 8192
