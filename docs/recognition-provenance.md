# Recognition provenance and postprocessing evidence

Formula rendering remains sourced from `Formula.source`. Recognition evidence
is stored separately in `Formula.recognition_provenance` and
`Formula.recognition_evidence`, so Office and SDK consumers do not need to
parse rendered text or Core-private sessions.

Provenance contains model and version, runtime and provider, optional source
polygon, raw and normalized confidence, and a transformation list. Every
automatic transformation records a stable rule/version, before/after SHA-256,
reason, confidence delta, and automatic/manual mode.

Recognition evidence retains:

- raw decoder output;
- conservative normalized output;
- corrected output;
- before/after diff;
- trigger decision and trigger IDs;
- raw and normalized confidence;
- validation before and after correction;
- transformation evidence;
- review status and stable status code.

The rule processor runs for low confidence or validation failures. It may
append missing closing groups, remove unmatched `\left`/`\right` visual sizing
prefixes, and close still-open environments. It does not collapse duplicate
token runs, repair inconsistent matrix rows, guess truncated content, or
rewrite mathematical meaning. Those cases retain the raw output and return
`POSTPROCESS_REVIEW_REQUIRED`.

High-confidence, valid formulas produce no transformation. The legacy repair
API remains for compatibility, but no longer rewrites valid `\frac{a}{b}` as
an empty-numerator fraction and preserves `&` inside environments.
