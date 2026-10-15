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

routes = formula_conversion_capabilities()  # 176 native direction/mode rows
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
`unavailableReason`; UnicodeMath/AsciiMath have finite formula-only input/output
routes in explicit best-effort mode; MTEF remains unsupported here. See the
[linear grammar and binding contract](../../docs/formats/linear-formula-conversion.md).
OLE is a
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

The extracted-wheel smoke test covers all 176 advertised routes (78 successful
conversions, including the strict route), invalid arguments, failure isolation
and resource limits, alongside the existing persistent-session tests. It does
not measure visual fidelity or real Office host compatibility.

## Structured formula completion

```python
from latexsnipper_core import (
    complete_formula, apply_formula_completion, formula_completion_capabilities,
)

source = r"\fr"
revision = 42  # An unsigned 64-bit Python integer owned by the editor.
cursor = len(source.encode("utf-8"))
result = complete_formula(source, cursor=cursor, revision=revision)
selected = apply_formula_completion(
    source, cursor=cursor, revision=revision,
    snapshot=result["snapshot"], candidate_id="builtin:frac",
)
assert selected["source"] == r"\frac{}{}"
assert selected["cursor"] == 6
```

These are formal PyO3 functions, independent of `Session` and `FormulaRegistry`.
The finite grammar accepts only exact `input_format="latex"` (the default).
Cursor, replacement ranges and tab stops are **UTF-8 byte offsets**, not Python
character counts or JavaScript UTF-16 positions. Query data has `snapshot` and
`completion` (context/candidates); acceptance returns source, cursor and absolute
tabStops. Requests and edits use the same Core implementation as Rust/C/JSONL.

Python `revision` arguments must be integers from 0 through `2**64-1`; bools,
strings, floats, negatives and overflow are refused. Serialized revisions in
snapshot/context remain canonical decimal **strings**, preserving all bits
across transports. Capabilities distinguish `revisionArgumentEncoding` from
`revisionEncoding`. Echo the returned snapshot unchanged; provide the actual
current source/cursor/revision, not stale values copied from the snapshot.

Core rechecks schema/hash/revision/cursor and regenerates the canonical edit by
candidate ID. No client-supplied insert/replace fields, unknown snapshot fields,
or automatic writes are accepted. The snapshot is not an authorization token;
the host owns explicit acceptance, revision increments, undo and document writes.
An edit is not proof of mathematical, rendering or conversion validity.

Core calculation and result serialization release the GIL. Input is limited to
64 KiB, editing nodes to 4096, depth below 64, and candidates to 128; result data
is at most 1 MiB after UTF-8 JSON serialization. Snapshot JSON is at most 4 KiB.
These are transfer/work budgets, not peak allocation or hard deadline guarantees.
Wrong binding argument types/unknown keywords use Python TypeError/OverflowError;
invalid revisions/formats/snapshot schemas use `LaTeXSnipperError` with
`INVALID_ARGUMENT`. Core editing failures retain `INVALID_CURSOR`,
`LIMIT_EXCEEDED`, `STALE_CONTEXT`, `UNKNOWN_CANDIDATE` and `OUTPUT_TOO_LARGE`,
with no implicit retry, fallback, network, macro execution or session changes.

See [Core grammar and editing boundaries](../../docs/formula-completion.md).

### Scoped command signatures

The additive `complete_formula_scoped`, `apply_formula_completion_scoped` and
`formula_scoped_completion_capabilities` expose editing schema 2, leaving v1
functions and capability shapes unchanged. Pass the **current** `catalog` dict
to both query and acceptance; query returns `snapshot/context/candidates`
(not the v1 nested completion shape). Candidates contain `edit` and nullable
`definition` (ID and exact revision). Acceptance also takes snapshot/candidate_id.

```python
from latexsnipper_core import complete_formula_scoped, apply_formula_completion_scoped

catalog = {
    "scope": {"userSha256": "a" * 64, "projectSha256": None,
              "documentSha256": None, "generation": "0"},
    "definitions": [{"id": "custom-pair", "revision": "1", "userSha256": "a" * 64,
                     "projectSha256": None, "command": "custompair", "arguments": ["math", "text"]}],
}
result = complete_formula_scoped(r"\cust", cursor=5, revision=42, catalog=catalog)
selected = apply_formula_completion_scoped(
    r"\cust", cursor=5, revision=42, catalog=catalog,
    snapshot=result["snapshot"], candidate_id="definition:custom-pair",
)
assert selected["source"] == r"\custompair{}{}"
```

Scope hashes above are illustrative opaque identities, not credentials. The host
must supply authorized user/project definitions, actual document identity, and a
generation advanced on clear/reset. A definition has a 1–64 byte ASCII ID
(letters/digits/`-_.`), an ASCII-letter control word without backslash/digits, and
at most four arguments (empty for zero-argument symbols). Required kinds are
`math`/`text`; one leading `optional-math`/`optional-text` is supported when
capabilities advertise it in `argumentKinds` and `maxLeadingOptionalArguments`.
`definition:<id>` omits that optional argument; `definition:<id>:optional` inserts
`[]` and required arguments with typed tab stops. Existing lists are not duplicated,
and Core never expands default values.
Other user/project items are filtered; effective duplicate IDs/commands and
builtin collisions are refused. Core binds actual effective content as well as
claimed versions, user/project/document and reset epoch. Stale scoped contexts
cannot be accepted via v1. These guards do not authorize macro or document writes.

Editor `revision` remains a Python integer; **catalog definition revisions and
generation remain canonical decimal strings**, as do returned snapshots. Catalog
JSON is at most 64 KiB, snapshots 4 KiB, and serialized result data 1 MiB; source,
tree and candidate budgets are unchanged. Unknown nested keys and malformed
encodings are INVALID_ARGUMENT; invalid effective declarations are INVALID_CATALOG,
and Core errors retain their codes. Wrong dict/keyword types use Python errors.
Core work/serialization release the GIL; Python dict encoding/result decoding do
not. No global registry, IO, macro expansion, preferences, learning or history is
introduced. Hosts still own consent, acceptance, revisions and undo.

### Explicit local fragments

`complete_formula_fragments`, `apply_formula_completion_fragments` and
`formula_fragment_completion_capabilities` expose editing schema 3 separately
from v1/v2. They use the same keyword-only arguments as scoped completion, with
`catalog = {commands, policy, fragments}`. Commands is the v2 catalog; policy has
`retrievalEnabled`, `sensitiveDocument`, and `allowInSensitiveDocument` booleans.

```python
from latexsnipper_core import complete_formula_fragments, apply_formula_completion_fragments

catalog = {
    "commands": {"scope": {"userSha256": "a" * 64, "projectSha256": None,
                  "documentSha256": None, "generation": "0"}, "definitions": []},
    "policy": {"retrievalEnabled": True, "sensitiveDocument": False, "allowInSensitiveDocument": False},
    "fragments": [{"id": "saved", "revision": "1", "userSha256": "a" * 64,
        "projectSha256": None, "documentSha256": None, "trigger": "myfraction",
        "label": "Saved fraction", "source": r"\frac{}{}"}],
}
result = complete_formula_fragments(r"\myf", cursor=4, revision=42, catalog=catalog)
selected = apply_formula_completion_fragments(
    r"\myf", cursor=4, revision=42, catalog=catalog,
    snapshot=result["snapshot"], candidate_id="fragment:saved",
)
assert selected["source"] == r"\frac{}{}"
```

The illustrative hashes are identities, not consent credentials. The host must
authorize local retrieval and pass actual current policy/catalog/editor state.
Foreign user/project/document and disabled/sensitive-disallowed fragments are
filtered before private semantic checks or hashing. Source is preserved, must
have bare formula shape and closed finite editing structure, and is never executed.
The snapshot binds effective content, policy, scope and reset epoch; changing them
invalidates old edits. Do not downgrade failed acceptance to an old API.

At most 64 fragments / 8 KiB source each are allowed. This API has its own 1 MiB
UTF-8 catalog JSON budget (without ASCII-escape inflation), preserving v2's 64 KiB limit. Snapshots remain 4 KiB and
serialized result data 1 MiB. Schema/encoding errors use INVALID_ARGUMENT;
semantic catalog errors use INVALID_CATALOG and other Core errors keep their codes.
Rust decoding, Core work and result serialization release the GIL; Python dict
encoding and result construction do not. These serialized budgets do not promise
peak object memory limits. No private storage, history, preferences or learning
is introduced. See the [shared fragment contract](../../docs/formula-completion.md).

### Owned preference feedback

`FormulaPreferences` is an explicit volatile owner of the shared Core preference
store, independent of `Session` and `FormulaRegistry`. `FormulaPreferenceCapture`
and `FormulaPreferenceReceipt` are opaque native objects: callers cannot construct,
modify, pickle or serialize their internals. Nothing is recorded by the existing
completion functions. Feature-detect the three new classes on older wheels.

```python
import secrets
from latexsnipper_core import FormulaPreferences

def open_preferences_after_consent(current_catalog):
    preferences = FormulaPreferences(
        scope=current_catalog["commands"]["scope"], key=secrets.token_bytes(32),
    )
    preferences.set_policy({
        "rankingEnabled": True, "feedbackEnabled": True,
        "allowSensitiveFeedback": False,
    })
    return preferences
```

Defaults are all disabled. `info()` reports policy, exact scope/generation, event
count, limits and `persistence: false`, never the key or event internals.
`rank_fragments(source, *, cursor, revision, catalog, input_format="latex")`
returns `{completion, order, features}`; `completion` is freshly validated Core
profile-3 data, `order` is only an index permutation, and features align with
the original candidates. It cannot inject candidates or apply an editor change.

Capture `capture = preferences.capture_context()` BEFORE dispatching an acceptance
or feedback job. After a real host edit has succeeded, report its pre-edit request:
`preferences.record_acceptance_at(capture, source, cursor=..., revision=...,
catalog=..., snapshot=..., candidate_id=..., event_id=...)`. The `event_id` is a
strictly increasing positive Python `int` up to `2**64-1`, not bool/float/string.
Source offsets use UTF-8 bytes and editor revisions use the existing Python-u64
contract; nested catalog/snapshot revisions remain canonical decimal strings.
The returned receipt is `None` when capture is disabled; otherwise report a real
undo using `record_undo(receipt)` exactly once. Feedback failure must never undo
a successful document transaction, block editing, or trigger an automatic retry.

Scope/epoch and actual candidate/catalog are revalidated. Other-instance capture
or receipt, late feedback after policy ABA, duplicate events, and stale snapshots
refuse explicitly. Do not mint a new capture inside a late job to bypass its age.
Allowing sensitive template retrieval does not permit feedback: the separate
`allowSensitiveFeedback` flag gates sensitive ranking/capture.

`clear(generation=..., key=secrets.token_bytes(32))` requires a strictly newer
scope generation and a different nonzero 32-byte key, even when disabled. Update
the actual catalog epoch and query a new snapshot before further acceptance.
It clears only this owner's logical state, not host databases/backups or secure
heap erasure. Disabling is not clearing. The host owns consent, key isolation,
cache cleanup and actual editor acceptance/undo; opaque objects are not permission
tokens. There is no cross-document merge, export/import, disk persistence or model.

Instances support `with`, idempotent `close()` and `closed`. Closed owners refuse
with `PREFERENCE_CLOSED`; remaining opaque objects do not keep their store alive.
Concurrent calls to one busy owner fail promptly with retryable `PREFERENCE_BUSY`,
without waiting or replaying work; the caller coordinates close and retry policy.
Independent instances can run concurrently. Rust parsing/ranking/recording runs
with the GIL detached; Python dict encoding and returned-object creation hold it.

Per owner: 1024 events including undos; metadata JSON 4 KiB, catalog UTF-8 JSON
1 MiB, snapshot 4 KiB and serialized rank data 1 MiB. Source/tree/candidate limits
remain unchanged. These are serialized-data limits, not peak object-memory
guarantees. Errors use `LaTeXSnipperError`; key/schema failures are
`INVALID_ARGUMENT`, identity mismatch `SCOPE_MISMATCH`, old feedback
`STALE_FEEDBACK`, and Core completion failures preserve their existing codes.
At capacity, ordinary deterministic completion remains independently usable.

This binding implements the [Core preference boundary](../../docs/formula-completion-preferences.md),
not learned ranking or a claimed user-productivity improvement.

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
