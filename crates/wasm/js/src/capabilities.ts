export interface BrowserRuntimeCapabilities {
  workerExecution: {
    supported: boolean;
    hardCancellationMode: "terminate-worker-and-restart";
    maxConcurrentRecognitions: 1;
  };
  indexedDbCache: { supported: boolean; schemaVersion: number };
  incrementalDownloads: { supported: boolean; abortable: true; checksumRequired: true };
}

export function browserRuntimeCapabilities(): BrowserRuntimeCapabilities {
  return {
    workerExecution: {
      supported: typeof Worker !== "undefined",
      hardCancellationMode: "terminate-worker-and-restart",
      maxConcurrentRecognitions: 1,
    },
    indexedDbCache: {
      supported: typeof indexedDB !== "undefined",
      schemaVersion: 2,
    },
    incrementalDownloads: {
      supported: typeof fetch !== "undefined" && typeof ReadableStream !== "undefined",
      abortable: true,
      checksumRequired: true,
    },
  };
}
