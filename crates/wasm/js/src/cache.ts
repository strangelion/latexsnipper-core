export const MODEL_CACHE_SCHEMA_VERSION = 2;

export interface CachedModelArtifact {
  key: string;
  profile: string;
  bytes: Uint8Array;
  sha256: string;
  byteLength: number;
  lastUsed: number;
  sourceUrl?: string;
  schemaVersion: number;
}

interface StoredArtifact extends Omit<CachedModelArtifact, "bytes"> {
  bytes: ArrayBuffer;
}

export class ModelCacheError extends Error {
  constructor(public readonly code: "CACHE_UNAVAILABLE" | "CACHE_QUOTA" | "CACHE_OPERATION_FAILED", message: string) {
    super(message);
    this.name = "ModelCacheError";
  }
}

export interface IndexedDbModelCacheOptions {
  namespace?: string;
  totalBudgetBytes?: number;
}

export class IndexedDbModelCache {
  private readonly namespace: string;
  private readonly totalBudgetBytes: number;

  constructor(options: IndexedDbModelCacheOptions = {}) {
    this.namespace = options.namespace ?? "latexsnipper-model-cache-v2";
    this.totalBudgetBytes = options.totalBudgetBytes ?? 512 * 1024 * 1024;
    if (!Number.isSafeInteger(this.totalBudgetBytes) || this.totalBudgetBytes <= 0) {
      throw new RangeError("totalBudgetBytes must be a positive safe integer");
    }
  }

  static available(): boolean {
    return typeof indexedDB !== "undefined";
  }

  async get(key: string, expectedSha256: string): Promise<CachedModelArtifact | undefined> {
    const db = await this.open();
    try {
      const stored = await request<StoredArtifact | undefined>(db.transaction("artifacts").objectStore("artifacts").get(key));
      if (!stored) return undefined;
      const bytes = new Uint8Array(stored.bytes);
      const actual = await sha256Hex(bytes);
      if (actual !== normalizeSha(expectedSha256) || stored.sha256 !== actual || stored.byteLength !== bytes.byteLength) {
        await transactionDone(db.transaction("artifacts", "readwrite"), (store) => store.delete(key));
        return undefined;
      }
      stored.lastUsed = Date.now();
      await this.putStored(db, stored);
      return { ...stored, bytes };
    } finally {
      db.close();
    }
  }

  async put(artifact: Omit<CachedModelArtifact, "byteLength" | "lastUsed" | "schemaVersion">): Promise<void> {
    const actual = await sha256Hex(artifact.bytes);
    if (actual !== normalizeSha(artifact.sha256)) throw new ModelCacheError("CACHE_OPERATION_FAILED", "Refusing to cache unverified model bytes");
    if (artifact.bytes.byteLength > this.totalBudgetBytes) throw new ModelCacheError("CACHE_QUOTA", "Artifact exceeds total cache budget");
    const db = await this.open();
    try {
      await this.putStored(db, {
        ...artifact,
        bytes: artifact.bytes.slice().buffer,
        sha256: actual,
        byteLength: artifact.bytes.byteLength,
        lastUsed: Date.now(),
        schemaVersion: MODEL_CACHE_SCHEMA_VERSION,
      });
      await this.evictToBudget(db);
    } finally {
      db.close();
    }
  }

  async delete(key: string): Promise<void> {
    const db = await this.open();
    try {
      await transactionDone(db.transaction("artifacts", "readwrite"), (store) => store.delete(key));
    } finally {
      db.close();
    }
  }

  async clear(): Promise<void> {
    const db = await this.open();
    try {
      await transactionDone(db.transaction("artifacts", "readwrite"), (store) => store.clear());
    } finally {
      db.close();
    }
  }

  private open(): Promise<IDBDatabase> {
    if (!IndexedDbModelCache.available()) return Promise.reject(new ModelCacheError("CACHE_UNAVAILABLE", "IndexedDB is unavailable"));
    return new Promise((resolve, reject) => {
      let settled = false;
      const opening = indexedDB.open(this.namespace, MODEL_CACHE_SCHEMA_VERSION);
      opening.onupgradeneeded = () => {
        const db = opening.result;
        const store = db.objectStoreNames.contains("artifacts")
          ? opening.transaction!.objectStore("artifacts")
          : db.createObjectStore("artifacts", { keyPath: "key" });
        if (!store.indexNames.contains("lastUsed")) store.createIndex("lastUsed", "lastUsed");
        if (!store.indexNames.contains("profile")) store.createIndex("profile", "profile");
      };
      opening.onerror = () => {
        settled = true;
        reject(cacheFailure(opening.error));
      };
      opening.onsuccess = () => {
        if (settled) {
          opening.result.close();
          return;
        }
        settled = true;
        opening.result.onversionchange = () => opening.result.close();
        resolve(opening.result);
      };
      opening.onblocked = () => {
        settled = true;
        reject(new ModelCacheError("CACHE_OPERATION_FAILED", "IndexedDB schema migration is blocked"));
      };
    });
  }

  private async putStored(db: IDBDatabase, artifact: StoredArtifact): Promise<void> {
    try {
      await transactionDone(db.transaction("artifacts", "readwrite"), (store) => store.put(artifact));
    } catch (cause) {
      throw cacheFailure(cause);
    }
  }

  private async evictToBudget(db: IDBDatabase): Promise<void> {
    const artifacts = await request<StoredArtifact[]>(db.transaction("artifacts").objectStore("artifacts").getAll());
    let total = artifacts.reduce((sum, artifact) => sum + artifact.byteLength, 0);
    artifacts.sort((a, b) => a.lastUsed - b.lastUsed);
    for (const artifact of artifacts) {
      if (total <= this.totalBudgetBytes) break;
      await transactionDone(db.transaction("artifacts", "readwrite"), (store) => store.delete(artifact.key));
      total -= artifact.byteLength;
    }
  }
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes.slice().buffer);
  return Array.from(new Uint8Array(digest), (value) => value.toString(16).padStart(2, "0")).join("");
}

function normalizeSha(value: string): string {
  return value.replace(/^sha256:/i, "").toLowerCase();
}

function request<T>(value: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    value.onsuccess = () => resolve(value.result);
    value.onerror = () => reject(cacheFailure(value.error));
  });
}

function transactionDone(transaction: IDBTransaction, action: (store: IDBObjectStore) => IDBRequest): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(cacheFailure(transaction.error));
    transaction.onabort = () => reject(cacheFailure(transaction.error));
    action(transaction.objectStore("artifacts"));
  });
}

function cacheFailure(cause: unknown): ModelCacheError {
  const error = cause instanceof DOMException ? cause : undefined;
  return new ModelCacheError(error?.name === "QuotaExceededError" ? "CACHE_QUOTA" : "CACHE_OPERATION_FAILED", error?.message ?? String(cause));
}
