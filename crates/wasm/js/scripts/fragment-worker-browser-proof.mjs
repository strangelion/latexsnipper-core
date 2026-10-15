export async function fragmentWorkerProof(client, lifecycle, workers, setEntered) {
  const verify = (condition, message) => { if (!condition) throw new Error(message); };
  const revision = "18446744073709551615";
  const catalog = { commands: { scope: { userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), generation: revision }, definitions: [] },
    policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
    fragments: [{ id: "bayes", revision, userSha256: "a".repeat(64), projectSha256: "b".repeat(64), documentSha256: "c".repeat(64), trigger: "bayes", label: "Local fraction", source: "\\frac{α}{}" }] };
  const input = { source: "中😀\\bay", cursor: 11, revision, inputFormat: "latex", catalog };
  const reply = await client.completeFormulaFragments(input);
  verify(reply.ok && reply.data.context.schemaVersion === 3 && reply.data.context.nodes[0].parent === null && reply.data.candidates[0].definition === null, "fragment reply/null profile");
  verify(reply.data.candidates[0].fragment.revision === revision, "fragment revision precision");
  const apply = { ...input, snapshot: reply.data.snapshot, candidateId: "fragment:bayes" };
  const accepted = await client.applyFormulaCompletionFragments(apply);
  verify(accepted.ok && accepted.data.source === "中😀\\frac{α}{}" && accepted.data.cursor === 17, "fragment UTF-8 canonical edit");
  for (const field of ["retrievalEnabled", "sensitiveDocument", "allowInSensitiveDocument", "generation", "source"]) {
    const changed = structuredClone(catalog);
    if (field in changed.policy) changed.policy[field] = !changed.policy[field];
    else if (field === "generation") changed.commands.scope.generation = "0";
    else changed.fragments[0].source = "x";
    const stale = await client.applyFormulaCompletionFragments({ ...apply, catalog: changed });
    verify(!stale.ok && stale.error.code === "STALE_CONTEXT", `fragment ${field} stale guard`);
  }
  for (const kind of ["disabled", "sensitive", "foreign"]) {
    const hiddenCatalog = structuredClone(catalog);
    if (kind === "disabled") hiddenCatalog.policy.retrievalEnabled = false;
    else if (kind === "sensitive") hiddenCatalog.policy.sensitiveDocument = true;
    else hiddenCatalog.fragments[0].userSha256 = "d".repeat(64);
    hiddenCatalog.fragments[0].source = "PRIVATE_SECRET\\documentclass{article}";
    const hidden = await client.completeFormulaFragments({ ...input, catalog: hiddenCatalog });
    verify(hidden.ok && hidden.data.candidates.length === 0 && !JSON.stringify(hidden).includes("PRIVATE_SECRET"), `fragment ${kind} privacy`);
  }
  const allowedCatalog = structuredClone(catalog);
  allowedCatalog.policy.sensitiveDocument = true;
  allowedCatalog.policy.allowInSensitiveDocument = true;
  const allowed = await client.completeFormulaFragments({ ...input, catalog: allowedCatalog });
  verify(allowed.ok && allowed.data.candidates[0].edit.id === "fragment:bayes", "explicit sensitive allow");
  const oversized = await lifecycle.completeFormulaFragments({ ...input, source: "fixture:oversize", cursor: 0 })
    .then(() => { throw new Error("Oversized fragment envelope posted"); }, (error) => error);
  verify(oversized.detail.code === "WORKER_RESULT_LIMIT", "fragment Worker result ceiling");
  for (const cancellation of ["abort", "timeout", "acceptance-timeout"]) {
    const signal = new AbortController();
    const entered = new Promise((resolve) => setEntered(resolve));
    const options = { signal: signal.signal, timeoutMillis: 1000 };
    const stalled = cancellation === "acceptance-timeout"
      ? lifecycle.applyFormulaCompletionFragments({ ...apply, source: "fixture:infinite", cursor: 0 }, options)
      : lifecycle.completeFormulaFragments({ ...input, source: "fixture:infinite", cursor: 0 }, options);
    const failed = stalled.then(() => { throw new Error("Stalled fragment completed"); }, (error) => error);
    await entered;
    const before = workers.length;
    if (cancellation === "abort") signal.abort();
    const reason = await failed;
    verify(reason.detail.code === (cancellation === "abort" ? "CANCELLED" : "WORKER_TASK_TIMEOUT"), `fragment ${cancellation}`);
    await lifecycle.ready();
    verify(workers.length === before + 1, "fragment Worker restart");
    const recovered = await lifecycle.completeFormulaFragments(input);
    verify(recovered.ok && recovered.data.snapshot.catalogSha256 === reply.data.snapshot.catalogSha256, "fragment no replay and recovery");
  }
  return { fragmentCompletion: true, fragmentPrivacy: true, fragmentSensitiveAllow: true, fragmentStaleGuards: true,
    fragmentUtf8: true, fragmentHardAbort: true, fragmentTimeout: true, fragmentAcceptanceNoReplay: true, fragmentResultBudget: true };
}
