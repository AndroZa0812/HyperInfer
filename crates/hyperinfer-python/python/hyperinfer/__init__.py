"""HyperInfer - High-performance Python SDK for LLM Gateway."""

from typing import TYPE_CHECKING

from hyperinfer.client import Client
from hyperinfer.config import Config

# This block is seen by IDEs/Linters but ignored at runtime
if TYPE_CHECKING:
    from hyperinfer._hyperinfer import HyperInferClient

__version__ = "0.1.0"
__all__ = ["HyperInferClient", "Client", "Config"]


def __getattr__(name: str) -> type["HyperInferClient"]:
    """Lazy-load HyperInferClient from the native Rust extension."""
    if name == "HyperInferClient":
        try:
            from hyperinfer._hyperinfer import HyperInferClient as _HyperInferClient
        except ModuleNotFoundError:
            raise ImportError(
                "HyperInferClient requires the native Rust extension. "
                "Ensure hyperinfer is installed with binary dependencies."
            ) from None
        return _HyperInferClient
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
