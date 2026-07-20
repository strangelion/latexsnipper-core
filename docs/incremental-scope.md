# Incremental Engine Scope

`latexsnipper-incremental` currently performs stable-ID local edits for formula
and paragraph blocks. Formula conversion and rendering are cached per stable
ID; paragraph edits invalidate the affected block and are checked against a
full source rebuild.

Structural source-range edits remain intentionally conservative: they rebuild
the document and mark `full_reconcile_required`. Table cells, headings, lists,
quotes, and nested blocks are not advertised as local-edit targets until their
parsers provide stable source spans at that granularity.

This boundary prevents a false incremental claim: every supported local edit
must be equivalent to a clean rebuild, and every unsupported edit must report
the conservative fallback.
