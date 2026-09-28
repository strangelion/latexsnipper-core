# LaTeXSnipper Core for Python

The Python adapter owns one long-lived Core recognition session per `Session`
object. Models, the runtime, warmup state, and inference-session caches are
reused until `close()` or context-manager exit.

```python
from latexsnipper_core import Session

with Session("models", runtime_preference="auto") as session:
    session.warmup("croppedFormula")
    result = session.recognize_path(
        "formula.png",
        profile="croppedFormula",
        formats=["latex", "omml", "mathml"],
    )
    print(result["document"])
    print(result["outputs"]["latex"])
```

Encoded images may also be supplied through any contiguous or strided Python
buffer object. The adapter copies the encoded bytes once into Core-owned memory
but never decodes and re-encodes the image in Python.

```python
result = session.recognize_bytes(
    memoryview(encoded_png),
    format_hint="png",
    profile="formula",
)
```

Long-running calls accept a cloneable `CancellationToken`. Its `cancel()`
method can be called from another Python thread while recognition runs; Core
observes it at safe pipeline boundaries, and the owning session remains usable.

Build and install an editable development extension from this directory with:

```console
python -m maturin develop --release
```

Build a distributable wheel with external shared-library repair enabled by the
package configuration:

```console
python -m maturin build --release --out dist
```

All Core failures raise `LaTeXSnipperError`. Its `code`, `detail`, and
`retryable` attributes preserve the stable application error contract.
