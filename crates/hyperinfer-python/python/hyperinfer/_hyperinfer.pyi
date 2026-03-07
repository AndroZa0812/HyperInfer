"""
Type stubs for the ``_hyperinfer`` native extension module.

These stubs allow IDEs (PyCharm, VS Code) and static type-checkers
(mypy, pyright) to understand the Rust-exported symbols without requiring
the compiled ``.so`` / ``.pyd`` binary at analysis time.
"""

from __future__ import annotations

from typing import Any

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

        Must be awaited before calling :meth:`chat`.  Idempotent – subsequent
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

    async def close(self) -> None:
        """Flush pending telemetry and release resources.

        Optional – safe to call multiple times.
        """
        ...
