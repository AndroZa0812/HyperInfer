"""Reusable background event loop for running async code from sync contexts.

Creates a single daemon thread that owns an asyncio event loop running
``loop.run_forever()``. Coroutines are dispatched to it via
``asyncio.run_coroutine_threadsafe`` and the caller blocks on the result.

This avoids the per-call thread/loop overhead of ``ThreadPoolExecutor`` +
``asyncio.run`` and works correctly whether the caller is on a plain sync
thread or already inside a running event loop (FastAPI, Jupyter, LangGraph).
"""

from __future__ import annotations

import asyncio
import threading
from collections.abc import Callable, Coroutine
from typing import Any, TypeVar

T = TypeVar("T")

_loop_thread: threading.Thread | None = None
_loop: asyncio.AbstractEventLoop | None = None
_loop_ready = threading.Event()
_loop_lock = threading.Lock()


def _ensure_loop() -> asyncio.AbstractEventLoop:
    """Lazily start the background event loop thread if it isn't running."""
    global _loop_thread, _loop, _loop_ready

    with _loop_lock:
        if _loop_thread is not None and _loop_thread.is_alive():
            return _loop  # type: ignore[return-value]

        _loop_ready = threading.Event()

        def _run_loop() -> None:
            global _loop
            _loop = asyncio.new_event_loop()
            asyncio.set_event_loop(_loop)
            _loop_ready.set()
            _loop.run_forever()

        _loop_thread = threading.Thread(target=_run_loop, daemon=True, name="hyperinfer-sync-loop")
        _loop_thread.start()
        _loop_ready.wait(timeout=5.0)

    if _loop is None:
        raise RuntimeError("Background event loop failed to start")
    return _loop


def run_sync(coro: Coroutine[Any, Any, T]) -> T:
    """Run *coro* on the persistent background event loop and return its result."""
    loop = _ensure_loop()
    future = asyncio.run_coroutine_threadsafe(coro, loop)
    return future.result()


def run_sync_generator(agen: Callable[..., Any], *args: Any, **kwargs: Any) -> Any:
    """Run an async generator function on the background loop, yielding results.

    The generator is advanced via ``asyncio.wait_for`` on each ``__anext__``
    call so the caller can consume chunks incrementally.
    """
    loop = _ensure_loop()

    # We need to keep the generator object alive across calls.
    # Use a wrapper that stores it.
    gen_holder: list[Any] = []

    async def _get_next() -> Any:
        if not gen_holder:
            gen_holder.append(agen(*args, **kwargs))
        try:
            return await gen_holder[0].__anext__()
        except StopAsyncIteration:
            raise

    try:
        while True:
            future = asyncio.run_coroutine_threadsafe(_get_next(), loop)
            try:
                yield future.result()
            except StopIteration:
                break
            except StopAsyncIteration:
                break
    finally:
        if gen_holder:
            asyncio.run_coroutine_threadsafe(gen_holder[0].aclose(), loop).result()
