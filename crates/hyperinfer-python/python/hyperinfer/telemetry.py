def init_langfuse_telemetry(
    public_key: str, secret_key: str, langfuse_host: str | None = None
) -> None:
    """Initialize OpenTelemetry pointing at a Langfuse instance."""
    from hyperinfer._hyperinfer import init_langfuse_telemetry as _init

    _init(public_key, secret_key, langfuse_host)


def shutdown_telemetry() -> None:
    """Flush and shut down the global tracer provider."""
    from hyperinfer._hyperinfer import shutdown_telemetry as _shutdown

    _shutdown()
