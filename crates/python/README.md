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

## Formula strings without recognition models

```python
from latexsnipper_core import convert_formula, formula_conversion_capabilities

routes = formula_conversion_capabilities()  # 144 native direction/mode rows
omml = convert_formula(r"\frac{a}{b}", input_format="latex", output_format="omml")
bare_latex = convert_formula(
    "frac(a, b)", input_format="typst", output_format="latex-fragment", mode="best-effort",
)
```

The default is `strict`: currently only the supported LaTeX-to-OMML source subset
is accepted. All other strict routes fail explicitly. Select `best-effort` to use
the existing LaTeX, MathML, OMML, Typst or Markdown parsers and nine semantic
outputs. Reconstruction is not the author's original source, and unsupported
syntax or style may be lost. Consult each row's `available`, `limitations` and
`unavailableReason`; UnicodeMath, AsciiMath and MTEF remain unsupported. OLE is a
host object, not a string format; this does not implement MathType conversion.

Each new conversion call bounds its source and reconstructed LaTeX to 64 KiB,
64 lexical nesting levels and 512 structural tokens (XML elements, or opening
delimiters/backslashes/scripts). These conservative budgets are not a grammar
validator; XML DTDs are rejected. Exceeding them raises `INPUT_TOO_LARGE`.
Unavailable routes raise `UNSUPPORTED_FORMAT`, unknown labels/modes raise
`INVALID_ARGUMENT`, and source/conversion failures raise `CONVERSION_FAILED`.
Conversion releases the GIL and does not require or alter a `Session`.

`latex-fragment` (also accepting `latex_fragment`) is an explicit single-formula
projection through the `latex_display` capability, not a new semantic format.
It returns bare LaTeX and rejects document splicing. Legacy `latex` still returns
a complete document; single-formula `markdown_inline` returns `$...$`.
This does not guarantee that a renderer supports the projected source.
Older adapters can reject this new output label even if `convert_formula` exists;
handle refusal explicitly, without stripping a document or silently falling back.

Conversion result strings are limited to 256 KiB after UTF-8 JSON serialization,
including JSON string escaping and quotes. Exceeding the limit raises
`OUTPUT_TOO_LARGE`, with no partial result or retry. This limit does not describe
peak memory usage or a hard execution deadline. The Python API continues to
return a plain string, not a transport envelope.

The installed-wheel smoke test covers all 144 advertised routes (46 successful
conversions, including the strict route), invalid arguments, failure isolation
and resource limits, alongside the existing persistent-session tests. It does
not measure visual fidelity or real Office host compatibility.

Build and install an editable development extension from this directory with:

Use an existing non-base Conda/virtual environment and verify the active Python
first. Do not install development packages into a frozen Conda base environment.

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
