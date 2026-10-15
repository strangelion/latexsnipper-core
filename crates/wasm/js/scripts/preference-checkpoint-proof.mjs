// Shared real-WASM checkpoint checks. Fixtures are public; never log live keys.
export async function checkpointProof(create, restore, complete, { capacity = false } = {}) {
  const verify = (ok, message) => { if (!ok) throw new Error(`Checkpoint proof: ${message}`); };
  const data = async (value) => { const reply = await value; verify(reply.ok, reply.error?.code ?? "missing success"); return reply.data; };
  const code = async (value, expected) => { const reply = await value; verify(!reply.ok && reply.error.code === expected, `expected ${expected}`); };
  const owners = [];
  const open = async (factory, configuration) => { const owner = await factory(configuration); owners.push(owner); return owner; };
  const refused = async (configuration, expected) => {
    try { await open(restore, configuration); }
    catch (cause) {
      verify((cause?.error?.code ?? cause?.detail?.code) === expected, `restore expected ${expected}`);
      verify(!JSON.stringify(cause).includes("PRIVATE_SECRET"), "redacted restore failure");
      return;
    }
    throw new Error(`Checkpoint proof: restore should refuse ${expected}`);
  };
  const maximum = "18446744073709551615";
  const configuration = { schemaVersion: 1, keyHex: "ab".repeat(32), scope: {
    userSha256: "a".repeat(64), projectSha256: null, documentSha256: "c".repeat(64), generation: "0" } };
  const permission = { storageEnabled: true, allowSensitiveStorage: false };
  const storageKeyHex = "cd".repeat(32);
  const policy = { rankingEnabled: true, feedbackEnabled: true, allowSensitiveFeedback: false };
  const input = { source: "\\lo", cursor: 3, revision: maximum, inputFormat: "latex", catalog: {
    commands: { scope: configuration.scope, definitions: [] },
    policy: { retrievalEnabled: true, sensitiveDocument: false, allowInSensitiveDocument: false },
    fragments: ["a", "z"].map((id) => ({ id, revision: "0", userSha256: configuration.scope.userSha256,
      projectSha256: null, documentSha256: null, trigger: `local${id}`, label: `PRIVATE_SECRET_${id}`, source: id === "a" ? "x" : "y^2" })) } };
  const applied = { ...input, snapshot: (await data(complete(input))).snapshot, candidateId: "fragment:z" };
  const capture = async (owner) => (await data(owner.command({ kind: "capture" }))).captureId;
  const accept = async (owner, eventId, request = applied) => data(owner.command({ kind: "record-acceptance", captureId: await capture(owner), eventId, request }));
  const exported = (owner, checkpoint, consent = permission) => owner.command({ kind: "export-checkpoint", storageKeyHex, checkpoint, permission: consent });
  try {
    const original = await open(create, configuration);
    await data(original.command({ kind: "set-policy", policy }));
    const oldCapture = await capture(original);
    const oldReceipt = (await accept(original, "9")).receiptId;
    const undone = (await accept(original, "10")).receiptId;
    await data(original.command({ kind: "undo", receiptId: undone }));
    await code(exported(original, maximum, { ...permission, storageEnabled: false }), "PREFERENCE_STORAGE_DISABLED");
    const checkpoint = await data(exported(original, maximum));
    verify(checkpoint.checkpoint === maximum, "exact exported checkpoint");
    for (const privateValue of ["PRIVATE_SECRET", "fragment:z", configuration.keyHex, storageKeyHex, "captureId", "receiptId"]) {
      verify(!checkpoint.snapshotJson.includes(privateValue), "snapshot excludes private content/tokens");
    }
    // Freshness is fixed by trusted fixture state, never read from the snapshot.
    const restoration = { ...configuration, storageKeyHex, expectedCheckpoint: maximum, snapshotJson: checkpoint.snapshotJson, permission };
    await refused({ ...restoration, permission: { ...permission, storageEnabled: false } }, "PREFERENCE_STORAGE_DISABLED");
    await refused({ ...restoration, storageKeyHex: configuration.keyHex }, "INVALID_ARGUMENT");
    await refused({ ...restoration, storageKeyHex: "ef".repeat(32) }, "AUTHENTICATION_FAILED");
    await refused({ ...restoration, keyHex: "ef".repeat(32) }, "AUTHENTICATION_FAILED");
    await refused({ ...restoration, expectedCheckpoint: "18446744073709551614" }, "STALE_CHECKPOINT");
    await refused({ ...restoration, scope: { ...configuration.scope, generation: "1" } }, "SCOPE_MISMATCH");
    await refused({ ...restoration, snapshotJson: '{"PRIVATE_SECRET":true}' }, "INVALID_SNAPSHOT");
    const tampered = JSON.parse(checkpoint.snapshotJson); tampered.body.events[0].undone = true;
    await refused({ ...restoration, snapshotJson: JSON.stringify(tampered) }, "AUTHENTICATION_FAILED");
    const restored = await open(restore, restoration);
    const info = await data(restored.command({ kind: "info" }));
    verify(info.eventCount === 2 && info.captureCount === 0 && info.receiptCount === 0 && !Object.values(info.policy).some(Boolean), "new owner is default-off without tokens");
    verify((await data(restored.command({ kind: "rank-fragments", request: input }))).order[0] === 0, "disabled rank");
    await data(restored.command({ kind: "set-policy", policy }));
    const ranked = await data(restored.command({ kind: "rank-fragments", request: input }));
    verify(ranked.order[0] === 1 && ranked.features[1].accepted === 2 && ranked.features[1].undone === 1 && ranked.features[1].lastAcceptedEvent === "9", "restored statistics");
    await code(restored.command({ kind: "undo", receiptId: oldReceipt }), "STALE_FEEDBACK");
    await code(restored.command({ kind: "record-acceptance", captureId: oldCapture, eventId: "11", request: applied }), "STALE_FEEDBACK");
    const currentCapture = await capture(restored);
    await code(restored.command({ kind: "record-acceptance", captureId: currentCapture, eventId: "10", request: applied }), "STALE_FEEDBACK");
    await data(restored.command({ kind: "record-acceptance", captureId: currentCapture, eventId: "11", request: applied }));
    await code(exported(restored, maximum), "STALE_CHECKPOINT");
    await code(exported(original, maximum), "STALE_CHECKPOINT");

    const sensitive = await open(create, configuration);
    await data(sensitive.command({ kind: "set-policy", policy: { ...policy, allowSensitiveFeedback: true } }));
    const privateInput = structuredClone(input);
    privateInput.catalog.policy.sensitiveDocument = true; privateInput.catalog.policy.allowInSensitiveDocument = true;
    const privateApply = { ...privateInput, snapshot: (await data(complete(privateInput))).snapshot, candidateId: "fragment:z" };
    await accept(sensitive, "1", privateApply);
    await data(sensitive.command({ kind: "set-policy", policy: { rankingEnabled: false, feedbackEnabled: false, allowSensitiveFeedback: false } }));
    await code(exported(sensitive, "1"), "SENSITIVE_STORAGE_DENIED");
    const allowed = { ...permission, allowSensitiveStorage: true };
    const privateCheckpoint = await data(exported(sensitive, "1", allowed));
    const privateRestore = { ...configuration, storageKeyHex, expectedCheckpoint: "1", snapshotJson: privateCheckpoint.snapshotJson, permission: allowed };
    await refused({ ...privateRestore, permission }, "SENSITIVE_STORAGE_DENIED");
    const privateOwner = await open(restore, privateRestore);
    verify(!(await data(privateOwner.command({ kind: "info" }))).policy.feedbackEnabled, "sensitive restore does not enable feedback");
    if (capacity) {
      await data(sensitive.command({ kind: "set-policy", policy }));
      for (let event = 2; event <= 1024; event++) await accept(sensitive, String(event));
      const full = await data(exported(sensitive, "2", allowed));
      verify(full.snapshotJson.length > 4096 && new TextEncoder().encode(full.snapshotJson).byteLength <= 256 * 1024, "full snapshot transfer budget");
      const fullOwner = await open(restore, { ...privateRestore, expectedCheckpoint: "2", snapshotJson: full.snapshotJson });
      verify((await data(fullOwner.command({ kind: "info" }))).eventCount === 1024, "full checkpoint restore");
      await data(fullOwner.command({ kind: "set-policy", policy }));
      await code(fullOwner.command({ kind: "record-acceptance", captureId: await capture(fullOwner), eventId: "1025", request: applied }), "LIMIT_EXCEEDED");
    }
    return { authenticatedCheckpoints: true, restoreDefaultOff: true, exactCheckpointU64: true,
      noOldFeedbackTokens: true, sensitiveStorageConsent: true, restoredEvents: capacity ? 1024 : 2 };
  } finally { for (const owner of owners) await owner.close(); }
}
