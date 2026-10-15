from collections.abc import Buffer, Sequence
from os import PathLike
from types import TracebackType
from typing import Any, Self

def convert_formula(
    content: str,
    *,
    input_format: str,
    output_format: str,
    mode: str = "strict",
) -> str: ...

def formula_conversion_capabilities() -> list[dict[str, Any]]: ...

def formula_completion_capabilities() -> dict[str, Any]: ...

def complete_formula(
    source: str, *, cursor: int, revision: int, input_format: str = "latex",
) -> dict[str, Any]: ...

def apply_formula_completion(
    source: str, *, cursor: int, revision: int, snapshot: dict[str, Any],
    candidate_id: str, input_format: str = "latex",
) -> dict[str, Any]: ...

def formula_scoped_completion_capabilities() -> dict[str, Any]: ...

def complete_formula_scoped(
    source: str, *, cursor: int, revision: int, catalog: dict[str, Any],
    input_format: str = "latex",
) -> dict[str, Any]: ...

def apply_formula_completion_scoped(
    source: str, *, cursor: int, revision: int, catalog: dict[str, Any],
    snapshot: dict[str, Any], candidate_id: str, input_format: str = "latex",
) -> dict[str, Any]: ...

def formula_fragment_completion_capabilities() -> dict[str, Any]: ...

def complete_formula_fragments(
    source: str, *, cursor: int, revision: int, catalog: dict[str, Any],
    input_format: str = "latex",
) -> dict[str, Any]: ...

def apply_formula_completion_fragments(
    source: str, *, cursor: int, revision: int, catalog: dict[str, Any],
    snapshot: dict[str, Any], candidate_id: str, input_format: str = "latex",
) -> dict[str, Any]: ...

class LaTeXSnipperError(Exception):
    code: str
    detail: str | None
    retryable: bool
    details: Any | None

class FormulaRegistry:
    def __init__(
        self, *, plugin_store: str | PathLike[str] | None = ...,
        profile_json: str | None = ...,
    ) -> None: ...
    @property
    def closed(self) -> bool: ...
    def info(self) -> dict[str, Any]: ...
    def capabilities(self) -> dict[str, Any]: ...
    def set_enabled(self, backend: str, enabled: bool) -> None: ...
    def convert(
        self, content: str, *, input_format: str, output_format: str,
        context_sha256: str, mode: str = ...,
        backend: str | None = ..., fallback: Sequence[str] | None = ...,
    ) -> dict[str, Any]: ...
    def approvals(self) -> dict[str, Any]: ...
    def approval(self, command_json: str) -> None: ...
    def cancel(self) -> None: ...
    def close(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None,
        exc_value: BaseException | None, traceback: TracebackType | None,
    ) -> bool: ...

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
