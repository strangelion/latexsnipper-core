import { ModelCacheError, type IndexedDbModelCache, sha256Hex } from "./cache.js";

export interface DownloadProgress {
  receivedBytes: number;
  totalBytes?: number;
  sourceUrl: string;
}

export interface ModelDownloadOptions {
  urls: string[];
  expectedSha256: string;
  maxBytes: number;
  signal?: AbortSignal;
  onProgress?: (progress: DownloadProgress) => void;
  cache?: IndexedDbModelCache;
  cacheKey?: string;
  profile?: string;
  cachePolicy?: "best-effort" | "required";
  onCacheWarning?: (warning: ModelCacheWarning) => void;
}

export interface ModelCacheWarning {
  code: "CACHE_UNAVAILABLE" | "CACHE_QUOTA" | "CACHE_OPERATION_FAILED";
  message: string;
  sourceUrl: string;
}

export class ModelDownloadError extends Error {
  constructor(
    public readonly code: "DOWNLOAD_ABORTED" | "DOWNLOAD_HTTP" | "DOWNLOAD_TOO_LARGE" | "DOWNLOAD_CHECKSUM" | "DOWNLOAD_CACHE_REQUIRED" | "DOWNLOAD_FAILED",
    message: string,
    public readonly attempts: string[],
  ) {
    super(message);
    this.name = "ModelDownloadError";
  }
}

export async function downloadVerifiedModel(options: ModelDownloadOptions): Promise<Uint8Array> {
  if (options.urls.length === 0) throw new ModelDownloadError("DOWNLOAD_FAILED", "At least one model URL is required", []);
  if (!Number.isSafeInteger(options.maxBytes) || options.maxBytes <= 0) {
    throw new ModelDownloadError("DOWNLOAD_TOO_LARGE", "maxBytes must be a positive safe integer", []);
  }
  const attempts: string[] = [];
  for (const url of options.urls) {
    try {
      const bytes = await downloadOne(url, options);
      const actual = await sha256Hex(bytes);
      const expected = options.expectedSha256.replace(/^sha256:/i, "").toLowerCase();
      if (actual !== expected) throw new ModelDownloadError("DOWNLOAD_CHECKSUM", `Checksum mismatch for ${url}`, attempts);
      if (options.cache && options.cacheKey) {
        try {
          await options.cache.put({ key: options.cacheKey, profile: options.profile ?? "unknown", bytes, sha256: actual, sourceUrl: url });
        } catch (cause) {
          const cacheError = cause instanceof ModelCacheError
            ? cause
            : new ModelCacheError("CACHE_OPERATION_FAILED", cause instanceof Error ? cause.message : String(cause));
          if (options.cachePolicy === "required") {
            throw new ModelDownloadError(
              "DOWNLOAD_CACHE_REQUIRED",
              `Verified model could not be stored in the required cache: ${cacheError.message}`,
              attempts,
            );
          }
          options.onCacheWarning?.({ code: cacheError.code, message: cacheError.message, sourceUrl: url });
        }
      }
      return bytes;
    } catch (cause) {
      if (options.signal?.aborted) throw new ModelDownloadError("DOWNLOAD_ABORTED", "Model download was aborted", attempts);
      if (cause instanceof ModelDownloadError && cause.code === "DOWNLOAD_CACHE_REQUIRED") throw cause;
      attempts.push(`${url}: ${cause instanceof Error ? cause.message : String(cause)}`);
    }
  }
  throw new ModelDownloadError("DOWNLOAD_FAILED", "Every model mirror failed", attempts);
}

async function downloadOne(url: string, options: ModelDownloadOptions): Promise<Uint8Array> {
  const response = await fetch(url, { signal: options.signal, redirect: "follow" });
  if (!response.ok) throw new ModelDownloadError("DOWNLOAD_HTTP", `${response.status} ${response.statusText}`, []);
  const contentLength = response.headers.get("content-length");
  const totalBytes = contentLength ? Number(contentLength) : undefined;
  if (totalBytes !== undefined && (!Number.isSafeInteger(totalBytes) || totalBytes > options.maxBytes)) {
    await response.body?.cancel();
    throw new ModelDownloadError("DOWNLOAD_TOO_LARGE", "Content-Length exceeds the configured model limit", []);
  }
  if (!response.body) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.byteLength > options.maxBytes) throw new ModelDownloadError("DOWNLOAD_TOO_LARGE", "Model exceeds the configured limit", []);
    options.onProgress?.({ receivedBytes: bytes.byteLength, totalBytes, sourceUrl: url });
    return bytes;
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let receivedBytes = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      receivedBytes += value.byteLength;
      if (receivedBytes > options.maxBytes) {
        await reader.cancel("model size limit exceeded");
        throw new ModelDownloadError("DOWNLOAD_TOO_LARGE", "Model exceeds the configured limit", []);
      }
      chunks.push(value);
      options.onProgress?.({ receivedBytes, totalBytes, sourceUrl: url });
    }
  } catch (cause) {
    await reader.cancel().catch(() => undefined);
    throw cause;
  }
  const bytes = new Uint8Array(receivedBytes);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}
