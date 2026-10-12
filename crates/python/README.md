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

## Explicit registered conversion

`FormulaRegistry` is the formal PyO3 interface to the same native registry used by
the additive C ABI. It retains one registry/compiled host per object, independently
of recognition `Session` objects. It does not load models, install packages or
implicitly choose a plugin.

```python
from latexsnipper_core import FormulaRegistry

with FormulaRegistry() as registry:
    report = registry.convert(
        r"\frac{1}{2}", input_format="latex", output_format="omml",
        context_sha256="a" * 64,
    )
    if report["outcome"]["kind"] == "converted":
        candidate = report["outcome"]["result"]["candidate"]["content"]
```

The digest above is a placeholder: bind the actual native binary/build, definitions,
fonts, styles and host dependencies. A report is not document write authority or
proof of mathematical/visual fidelity. Unlike legacy `convert_formula`, this method
returns an audited `converted`/`rejected` dictionary, not a plain string. Valid
registry refusals stay in the report; transport/argument/pre-marshalling failures
raise `LaTeXSnipperError`. Native codes are preserved, with structured `.details`
and a JSON string `.detail` when details are present. Legacy errors retain their
existing `.detail` and additionally have `.details = None`.

Capabilities, backend enablement and explicit availability-only `fallback` use the
[registered route contract](../../docs/formats/conversion-registry.md). A handler's
failure never retries. Source arguments are capped at 64 KiB before marshalling;
native report data retains its 2 MiB serialized limit and 256 KiB candidate limit.
Operations release the GIL. `cancel()` and `close()` are callable from another Python
thread, without waiting for the conversion mutex; native work already entered cannot
be undone. Cancellation is terminal for that instance, not a package blacklist.
`close()` is idempotent and context-manager exit/garbage collection close the handle.
`info()` does not expose transferable native handle numbers.

WASI profiles require an explicit wheel build with `--features wasi-formula`:

```console
python -m maturin build --release --features wasi-formula --out dist-wasi
```

Supply both `plugin_store` (an existing verified/enabled store) and `profile_json`
(the closed v1 JSON string). Raw nested fields are retained for validation, including
duplicate-key rejection; parsing a profile to a dict first can erase such defects.
Preparation leaves the profile route disabled. Call `set_enabled(backend_id, True)`
and select that backend explicitly. It does not enable the underlying package or
approve sensitive effects. Default wheels reject profiles with `FEATURE_NOT_COMPILED`.

`approvals()` returns owned pending/grant snapshots. Only trusted application code
may call `approval(command_json)` after explicit human consent; never expose it
directly as a guest/MCP/plugin endpoint. Existing finite grants do not re-prompt per
action. Do not automatically log/upload snapshots, which can contain sensitive paths
and destinations. Application dialogs, durable human policy and Office writes are not
implemented by this binding. Scope, quota, TTL and revocation remain those of the
[native host contract](../../docs/formats/conversion-registry-c.md#trusted-host-approval).

The finite power fixture verifies real importer/exporter behavior, not generic new
format support. The wheel smoke loads the extracted native binary in subprocesses,
checks its origin/feature identity, and removes the extraction directory afterward
without modifying the active environment. CI verifies default wheels on its native
matrix and additionally exercises an explicit WASI wheel on Linux; other feature-wheel
distribution/host combinations still need their own verification.

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
