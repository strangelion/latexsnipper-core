"""Installed-extension smoke test for persistent Python session semantics."""

from __future__ import annotations

import base64
import tempfile
from pathlib import Path

from latexsnipper_core import CancellationToken, LaTeXSnipperError, Session, __version__


def main() -> None:
    with tempfile.TemporaryDirectory() as first_root, tempfile.TemporaryDirectory() as second_root:
        first = Session(Path(first_root))
        second = Session(Path(second_root), runtime_preference="cpu")

        assert __version__
        assert first.closed is False
        assert second.closed is False
        assert isinstance(first.health(), dict)
        assert isinstance(first.capabilities(), dict)

        first_warmup = first.warmup("formula")
        second_warmup = first.warmup("formula")
        assert first_warmup["alreadyWarm"] is False
        assert second_warmup["alreadyWarm"] is True

        try:
            first.recognize_bytes(memoryview(b"not-an-image"), format_hint="png")
        except LaTeXSnipperError as error:
            assert error.code in {"INVALID_INPUT", "IMAGE_DECODE_FAILED"}
            assert isinstance(error.retryable, bool)
        else:
            raise AssertionError("invalid encoded image unexpectedly succeeded")

        # A request failure must not poison this session or leak into another.
        assert isinstance(first.health(), dict)
        assert isinstance(second.health(), dict)

        cancellation = CancellationToken()
        assert cancellation.cancelled is False
        cancellation.cancel()
        assert cancellation.cancelled is True
        try:
            first.recognize_bytes(
                memoryview(b"not-an-image"),
                cancellation=cancellation,
            )
        except LaTeXSnipperError as error:
            assert error.code == "CANCELLED"
        else:
            raise AssertionError("cancelled request unexpectedly succeeded")

        one_pixel_png = base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwC"
            "AAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        )
        try:
            first.recognize_bytes(memoryview(one_pixel_png), timeout_ms=0)
        except LaTeXSnipperError as error:
            assert error.code == "TIMEOUT"
        else:
            raise AssertionError("zero-timeout request unexpectedly succeeded")

        assert isinstance(first.health(), dict)

        first.close()
        first.close()
        assert first.closed is True
        try:
            first.health()
        except LaTeXSnipperError as error:
            assert error.code == "INVALID_INPUT"
            assert str(error) == "The recognition session is closed."
        else:
            raise AssertionError("closed session unexpectedly accepted a request")

        second.close()

    with tempfile.TemporaryDirectory() as model_root:
        with Session(model_root) as managed:
            assert managed.closed is False
        assert managed.closed is True


if __name__ == "__main__":
    main()
