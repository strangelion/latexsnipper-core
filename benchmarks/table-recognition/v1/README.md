# Table recognition v1 dataset intake

The benchmark contract and ordered-tree TEDS runner are implemented in
`latexsnipper-benchmark`, but no 30-image baseline is checked in yet.

The repository currently has only four table-oriented images, with incomplete
ground truth and licensing metadata. They are not duplicated or relabelled as a
30-image “real” dataset. Dataset admission requires:

- at least 30 independently sourced images;
- redaction review and an explicit source/license for every image;
- simple, merged, rowspan/colspan, empty, formula, mixed text/formula,
  borderless, scanned, and perspective categories;
- cell coordinates, spans, content kind, content, and reading order;
- predictions produced by a real model rather than copied ground truth.

Until those inputs are supplied, table TEDS and real cell metrics remain
blocked rather than reported as zero or synthesized.
