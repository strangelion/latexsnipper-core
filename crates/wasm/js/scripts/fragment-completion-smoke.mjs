import assert from "node:assert/strict";

export function fragmentCompletionSmoke(api) {
  const revision = "18446744073709551615";
  const catalog = {
    commands: { scope: { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: revision }, definitions: [] },
    policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
    fragments: [{ id: "bayes", revision, userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64),
      trigger: "bayes", label: "Local fraction", source: "\\frac{α}{}" }],
  };
  const input = { source: "中😀\\bay", cursor: 11, revision, inputFormat: "latex", catalog };
  const query = (value = input) => api.complete_formula_fragments_v3(JSON.stringify(value));
  const caps = api.formula_fragment_completion_capabilities_v3();
  assert.equal(caps.ok, true);
  assert.equal(caps.data.schemaVersion, 3);
  assert.equal(caps.data.maxFragments, 64);
  assert.equal(caps.data.maxFragmentBytes, 8192);
  assert.equal(caps.data.learning, false);
  assert.equal(caps.data.sensitiveDocumentRequiresHostAllow, true);
  const reply = query();
  assert.equal(reply.ok, true, JSON.stringify(reply));
  assert.equal(reply.data.context.schemaVersion, 3);
  assert.equal(reply.data.context.nodes[0].parent, null);
  assert.equal(reply.data.candidates[0].definition, null);
  assert.deepEqual(reply.data.candidates[0].fragment, { id: "bayes", revision });
  const apply = { ...input, snapshot: reply.data.snapshot, candidateId: "fragment:bayes" };
  const accept = (value = apply) => api.apply_formula_completion_fragments_v3(JSON.stringify(value));
  assert.deepEqual(accept().data, { source: "中😀\\frac{α}{}", cursor: 17, tabStops: [{ start: 17, end: 17 }] });
  for (const field of ["generation", "projectSha256", "documentSha256", "revision", "source", "label", "retrievalEnabled", "sensitiveDocument", "allowInSensitiveDocument"]) {
    const changed = structuredClone(catalog);
    if (field in changed.policy) changed.policy[field] = !changed.policy[field];
    else if (field in changed.commands.scope) changed.commands.scope[field] = field === "generation" ? "0" : "d".repeat(64);
    else changed.fragments[0][field] = field === "revision" ? "0" : field === "source" ? "x" : "Changed";
    assert.equal(accept({ ...apply, catalog: changed }).error.code, "STALE_CONTEXT", field);
  }
  for (const disabled of ["retrieval", "sensitive", "foreign-user", "foreign-project", "foreign-document"]) {
    const changed = structuredClone(catalog);
    if (disabled === "retrieval") changed.policy.retrievalEnabled = false;
    else if (disabled === "sensitive") changed.policy.sensitiveDocument = true;
    else changed.fragments[0][{ "foreign-user": "userSha256", "foreign-project": "projectSha256", "foreign-document": "documentSha256" }[disabled]] = "d".repeat(64);
    changed.fragments[0].source = "PRIVATE_SECRET\\documentclass{article}";
    const hidden = query({ ...input, catalog: changed });
    assert.equal(hidden.ok, true, disabled);
    assert.deepEqual(hidden.data.candidates, []);
    assert.equal(JSON.stringify(hidden).includes("PRIVATE_SECRET"), false);
    const builtin = query({ ...input, source: "\\fr", cursor: 3, catalog: changed });
    assert.equal(builtin.data.candidates.some((item) => item.edit.id === "builtin:frac" && item.fragment === null), true);
  }
  const sensitive = structuredClone(catalog);
  sensitive.policy.sensitiveDocument = true;
  sensitive.policy.allowInSensitiveDocument = true;
  assert.equal(query({ ...input, catalog: sensitive }).data.candidates[0].edit.id, "fragment:bayes");
  for (const body of ["$$x$$", "\\[x\\]", "\\documentclass{article}x", "\\frac{x}{", "x%unfinished"]) {
    const changed = structuredClone(catalog); changed.fragments[0].source = body;
    assert.equal(query({ ...input, catalog: changed }).error.code, "INVALID_CATALOG", body);
  }
  const wide = structuredClone(catalog);
  wide.fragments = Array.from({ length: 64 }, (_, index) => ({ ...catalog.fragments[0], id: `entry${index}`, source: "α".repeat(3900) }));
  assert.equal(query({ ...input, source: "x", cursor: 1, catalog: wide }).ok, true, "UTF-8 catalog budget");
  const huge = structuredClone(catalog);
  huge.fragments = Array.from({ length: 60 }, (_, index) => ({ ...catalog.fragments[0], id: `entry${index}`, source: "x %" + '"'.repeat(8188) + "\n" }));
  assert.equal(query({ ...input, source: "x".repeat(3900) + "\\bay", cursor: 3904, catalog: huge }).error.code, "OUTPUT_TOO_LARGE");
  assert.equal(query().ok, true, "output limit must not poison next call");
  assert.equal(accept({ ...apply, snapshot: { ...reply.data.snapshot, schemaVersion: 2 } }).error.code, "STALE_CONTEXT");
  assert.equal(accept({ ...apply, candidateId: "unknown" }).error.code, "UNKNOWN_CANDIDATE");
  for (const changed of [{ revision: 0 }, { revision: "01" }, { extra: true }, { inputFormat: "typst" }]) {
    assert.equal(query({ ...input, ...changed }).error.code, "INVALID_JSON");
  }
  assert.equal(api.complete_formula_fragments_v3(" ".repeat(1024 * 1024 + 1)).error.code, "INPUT_TOO_LARGE");
}
