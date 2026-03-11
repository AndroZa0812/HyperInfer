"""End-to-end integration tests for the HyperInfer Python SDK using a real Redis instance via Testcontainers."""

import pytest
from hyperinfer import Client, Config
from testcontainers.redis import RedisContainer


@pytest.fixture(scope="module")
def redis_container():
    """Start a real Redis container for integration tests."""
    with RedisContainer("redis:7-alpine") as redis:
        # Give Redis a moment to be fully ready to accept connections
        yield redis


@pytest.fixture
def redis_url(redis_container):
    """Get the connection URL for the test Redis instance."""
    return f"redis://{redis_container.get_container_host_ip()}:{redis_container.get_exposed_port(6379)}"


@pytest.mark.asyncio
@pytest.mark.integration
async def test_client_init_connects_to_redis(redis_url):
    """Test that the client can actually connect to a real Redis instance and initialize."""
    config = Config().with_api_key("openai", "dummy-key")

    # We should be able to create the client
    client = Client(redis_url=redis_url, config=config)
    assert not client._initialized

    # Init should successfully connect to Redis
    await client.init()
    assert client._initialized

    # Second init should be a no-op
    await client.init()

    # Close should clean up
    await client.close()
    assert not client._initialized


@pytest.mark.asyncio
@pytest.mark.integration
async def test_client_context_manager(redis_url):
    """Test the async context manager correctly initializes and closes."""
    config = Config().with_api_key("openai", "dummy-key")

    async with Client(redis_url=redis_url, config=config) as client:
        assert client._initialized

    assert not client._initialized


@pytest.mark.asyncio
@pytest.mark.integration
async def test_chat_without_api_key_fails(redis_url):
    """Test that a chat request without valid API keys fails appropriately."""
    # Start client without configuring the 'dummy' provider
    config = Config()

    async with Client(redis_url=redis_url, config=config) as client:
        with pytest.raises(Exception) as excinfo:
            await client.chat(
                key="test-user", model="gpt-4", messages=[{"role": "user", "content": "Hello"}]
            )

        # The exact error message depends on the Rust backend's routing/auth logic,
        # but it should fail cleanly rather than crashing.
        assert (
            "key" in str(excinfo.value).lower()
            or "provider" in str(excinfo.value).lower()
            or "routing" in str(excinfo.value).lower()
        )


@pytest.mark.asyncio
@pytest.mark.integration
async def test_stream_without_api_key_fails(redis_url):
    """Test that a stream request without valid API keys fails appropriately."""
    config = Config()

    async with Client(redis_url=redis_url, config=config) as client:
        with pytest.raises(RuntimeError):
            chunk_gen = client.stream(
                key="test-user", model="gpt-4", messages=[{"role": "user", "content": "Hello"}]
            )
            # The async generator evaluates lazily, so we need to request the first chunk
            await chunk_gen.__anext__()
