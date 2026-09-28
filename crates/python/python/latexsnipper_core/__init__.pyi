from collections.abc import Buffer, Sequence
from os import PathLike
from types import TracebackType
from typing import Any, Self

class LaTeXSnipperError(Exception):
    code: str
    detail: str | None
    retryable: bool

class CancellationToken:
    def __init__(self) -> None: ...
    @property
    def cancelled(self) -> bool: ...
    def cancel(self) -> None: ...

class Session:
    def __init__(
        self,
        models_dir: str | PathLike[str],
        *,
        quality_baselines_dir: str | PathLike[str] | None = ...,
        provider_smoke_fixture: str | PathLike[str] | None = ...,
        runtime_preference: str = ...,
        parse_mode: str = ...,
        max_threads: int = ...,
    ) -> None: ...
    @property
    def closed(self) -> bool: ...
    def health(self) -> dict[str, Any]: ...
    def capabilities(self) -> dict[str, Any]: ...
    def warmup(self, profile: str = ...) -> dict[str, Any]: ...
    def reload_models(self) -> dict[str, Any]: ...
    def recognize_path(
        self,
        path: str | PathLike[str],
        *,
        profile: str = ...,
        parse_mode: str | None = ...,
        timeout_ms: int | None = ...,
        strict: bool = ...,
        include_source_asset: bool = ...,
        cancellation: CancellationToken | None = ...,
        formats: Sequence[str] | None = ...,
    ) -> dict[str, Any]: ...
    def recognize_bytes(
        self,
        data: Buffer,
        *,
        format_hint: str | None = ...,
        profile: str = ...,
        parse_mode: str | None = ...,
        timeout_ms: int | None = ...,
        strict: bool = ...,
        include_source_asset: bool = ...,
        cancellation: CancellationToken | None = ...,
        formats: Sequence[str] | None = ...,
    ) -> dict[str, Any]: ...
    def close(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> bool: ...

__version__: str
