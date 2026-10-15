// Shared real-WASM assertions for Node packages and a browser Worker.
export async function preferenceProof(create, complete, { capacity = false } = {}) {
  const verify = (condition, message) => { if (!condition) throw new Error(`Preference proof: ${message}`); };
  const data = async (value) => { const reply = await value; verify(reply.ok, reply.error?.code ?? "missing success"); return reply.data; };
  const code = async (value, expected) => { const reply = await value; verify(!reply.ok && reply.error.code === expected, `expected ${expected}, got ${reply.error?.code}`); };
  const revision = "18446744073709551615";
  const scope = { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: "0" };
  // Public deterministic fixture keys; production hosts must supply fresh randomness.
  const configuration = { schemaVersion: 1, scope, keyHex: "ab".repeat(32) };
  const catalog = { commands: { scope, definitions: [] },
    policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
    fragments: ["a", "z"].map((id) => ({ id, revision, userSha256: scope.userSha256, projectSha256: scope.projectSha256,
      documentSha256: scope.documentSha256, trigger: `local${id}`, label: `Local ${id}`, source: id === "a" ? "x^2" : "\\frac{α}{β}" })) };
  const input = { source: "中😀\\lo", cursor: 10, revision, inputFormat: "latex", catalog };
  const completion = await data(complete(input));
  const apply = { ...input, snapshot: completion.snapshot, candidateId: "fragment:z" };
  const owners = [];
  const own = async () => { const owner = await create(configuration); owners.push(owner); return owner; };
  try {
    const owner = await own(), other = await own();
    const call = (command) => owner.command(command);
    const capture = () => data(call({ kind: "capture" })).then((v) => v.captureId);
    const accept = (captureId, eventId = revision, request = apply) => call({ kind: "record-acceptance", captureId, eventId, request });
    const info = await data(call({ kind: "info" }));
    verify(!info.policy.feedbackEnabled && !info.policy.rankingEnabled && info.persistence === false, "default policy");
    verify((await data(accept(await capture()))).receiptId === null, "disabled acceptance is not captured");
    verify((await data(call({ kind: "info" }))).eventCount === 0, "disabled state stays empty");
    const policy = { rankingEnabled: true, feedbackEnabled: true, allowSensitiveFeedback: false };
    await data(call({ kind: "set-policy", policy }));
    const first = await capture();
    await code(other.command({ kind: "record-acceptance", captureId: first, eventId: "1", request: apply }), "STALE_FEEDBACK");
    await code(accept(first, "1", { ...apply, revision: "0" }), "STALE_CONTEXT");
    const receipt = (await data(accept(first))).receiptId;
    verify(typeof receipt === "string", "receipt identity");
    await code(accept(first), "STALE_FEEDBACK");
    const ranked = await data(call({ kind: "rank-fragments", request: input }));
    const index = ranked.completion.candidates.findIndex((c) => c.edit.id === "fragment:z");
    verify(ranked.order[0] === index && ranked.features[index].lastAcceptedEvent === revision, "exact revision and preferred order");
    verify(ranked.completion.candidates[index].edit.insert === "\\frac{α}{β}", "ranking cannot replace source");
    await code(other.command({ kind: "undo", receiptId: receipt }), "STALE_FEEDBACK");
    await data(call({ kind: "undo", receiptId: receipt }));
    await code(call({ kind: "undo", receiptId: receipt }), "STALE_FEEDBACK");
    const staleCapture = await capture();
    await data(call({ kind: "set-policy", policy: { ...policy, feedbackEnabled: false } }));
    await data(call({ kind: "set-policy", policy }));
    await code(accept(staleCapture), "STALE_FEEDBACK");
    await data(call({ kind: "clear", generation: "1", keyHex: "cd".repeat(32) }));
    verify((await data(call({ kind: "info" }))).eventCount === 0, "clear erases feedback");
    await code(call({ kind: "rank-fragments", request: input }), "SCOPE_MISMATCH");
    const current = structuredClone(input); current.catalog.commands.scope.generation = "1";
    const currentCompletion = await data(complete(current));
    const currentApply = { ...current, snapshot: currentCompletion.snapshot, candidateId: "fragment:z" };
    const sensitive = structuredClone(currentApply);
    sensitive.catalog.policy.sensitiveDocument = true;
    sensitive.catalog.policy.allowInSensitiveDocument = true;
    sensitive.snapshot = (await data(complete(sensitiveQuery(sensitive)))).snapshot;
    verify((await data(accept(await capture(), "1", sensitive))).receiptId === null, "sensitive feedback refused independently of retrieval");
    await data(call({ kind: "set-policy", policy: { ...policy, allowSensitiveFeedback: true } }));
    const sensitiveReceipt = (await data(accept(await capture(), "1", sensitive))).receiptId;
    verify(typeof sensitiveReceipt === "string", "explicit sensitive feedback allow");
    await data(call({ kind: "undo", receiptId: sensitiveReceipt }));
    const captures = [];
    for (let i = 0; i < 128; i++) captures.push(await capture());
    await code(call({ kind: "capture" }), "LIMIT_EXCEEDED");
    for (const captureId of captures) await data(call({ kind: "discard-capture", captureId }));
    verify((await data(call({ kind: "info" }))).captureCount === 0, "discard releases capture quota");
    if (capacity) {
      for (let i = 2; i <= 1024; i++) await data(accept(await capture(), String(i), currentApply));
      await code(accept(await capture(), "1025", currentApply), "LIMIT_EXCEEDED");
      verify((await data(call({ kind: "info" }))).eventCount === 1024, "bounded event storage");
    }
    const closedCapture = await capture(); await owner.close();
    const fresh = await own();
    await code(fresh.command({ kind: "record-acceptance", captureId: closedCapture, eventId: "1", request: apply }), "STALE_FEEDBACK");
    verify((await data(fresh.command({ kind: "info" }))).eventCount === 0, "fresh owner is disabled and empty");
    return { exactU64: true, policyAndReset: true, isolation: true, sensitive: true, captures: 128, events: capacity ? 1024 : "not-exhausted" };
  } finally { for (const owner of owners) await owner.close(); }
}
function sensitiveQuery(apply) {
  const { snapshot: _snapshot, candidateId: _candidateId, ...request } = apply;
  return request;
}
