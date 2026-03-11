"""
Type stubs for the ``_hyperinfer`` native extension module.

These stubs allow IDEs (PyCharm, VS Code) and static type-checkers
(mypy, pyright) to understand the Rust-exported symbols without requiring
the compiled ``.so`` / ``.pyd`` binary at analysis time.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Any

def init_langfuse_telemetry(
    public_key: str,
    secret_key: str,
    langfuse_host: str | None = None,
) -> None:
    """Initialize OpenTelemetry pointing at a Langfuse instance.

    Langfuse's OTLP endpoint requires HTTP Basic Authentication.

    Args:
        public_key: The Langfuse public key (acts as username).
        secret_key: The Langfuse secret key (acts as password).
        langfuse_host: Optional host URL. Defaults to 'https://cloud.langfuse.com'.

    Raises:
        RuntimeError: If telemetry fails to initialize.
    """
    ...

def shutdown_telemetry() -> None:
    """Flush and shut down the global tracer provider.

    Should be called before process exit to ensure all buffered spans are exported.
    """
    ...

class HyperInferClient:
    """Low-level PyO3-exported Rust client.

    Prefer using the high-level :class:`hyperinfer.Client` wrapper which
    wraps this class and wires the :class:`hyperinfer.Config` builder.

    Args:
        redis_url: Redis connection URL used for rate-limiting and async
            telemetry (e.g. ``"redis://localhost:6379"``).
        config: Optional configuration dictionary as produced by
            :meth:`hyperinfer.Config.to_dict`.  When omitted an empty
            configuration is used.
    """

    def __init__(
        self,
        redis_url: str,
        config: dict[str, Any] | None = None,
    ) -> None: ...
    async def init(self) -> None:
        """Initialise the underlying Rust client.

        Must be awaited before calling :meth:`chat`.  Idempotent - subsequent
        calls are no-ops.
        """
        ...

    async def chat(
        self,
        key: str,
        request: dict[str, Any],
    ) -> dict[str, Any]:
        """Send a chat request through the data plane.

        Args:
            key: Virtual key used for rate-limiting and telemetry attribution.
            request: Request dict with the following structure::

                {
                    "model": "gpt-4",
                    "messages": [
                        {"role": "user", "content": "Hello"},
                    ],
                    "temperature": 0.7,   # optional
                    "max_tokens": 1024,   # optional
                }

        Returns:
            Response dict::

                {
                    "id": "chatcmpl-...",
                    "model": "gpt-4",
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": "..."},
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 34,
                    },
                }

        Raises:
            RuntimeError: If the client has not been initialised via
                :meth:`init`, if the rate limit is exceeded, or if the
                upstream provider returns an error.
        """
        ...

    async def chat_stream(
        self,
        key: str,
        request: dict[str, Any],
    ) -> AsyncIterator[dict[str, Any]]:
        """Stream token chunks through the data plane.

        Returns an async iterator that yields one chunk dict per SSE event::

            {
                "id": "chatcmpl-...",
                "model": "gpt-4",
                "delta": "Hello",        # incremental text
                "finish_reason": None,   # "stop" on the last chunk
                "usage": None,           # dict with token counts on last chunk
            }

        Args:
            key: Virtual key used for rate-limiting and telemetry attribution.
            request: Same shape as :meth:`chat`.

        Raises:
            RuntimeError: If the client has not been initialised.
        """
        ...

    async def set_mirror(self, model: str | None = None, sample_rate: float | None = None) -> None:
        """Configure traffic mirroring for the client.

        Args:
            model: Target model for the shadow request.
            sample_rate: Fraction of requests to mirror in [0.0, 1.0].
        """
        ...

    async def close(self) -> None:
        """Flush pending telemetry and release resources.

        Optional - safe to call multiple times.
        """
        ...

class ChunkStream:
    """Async iterator over SSE token chunks.  Returned by :meth:`HyperInferClient.chat_stream`."""

    def __aiter__(self) -> ChunkStream: ...
    async def __anext__(self) -> dict[str, Any]: ...
