# Execution-provider plugin boundary

`ExecutionProviderPlugin` is an ABI-neutral planning boundary. It describes,
probes, and validates provider configuration without exposing an ONNX Runtime
execution-provider ABI. The runtime adapter remains responsible for converting
a `ResolvedProvider` into the version-specific ORT API.

Built-in descriptors cover CPU, DirectML, CUDA, TensorRT, and CoreML. Every
descriptor records its runtime family, supported platforms and architectures,
required capabilities, trusted runtime libraries, priority, and experimental
status.

Provider probing only receives `RuntimeEnvironment`. Its library set must be
populated from application-owned, trusted runtime installation roots. Model
directories are not library discovery roots. The model resolver independently
rejects DLL, SO, dylib, executable, and script artifacts.

Fallback is explicit. Callers pass `allow_cpu_fallback`; the resolver never
silently widens a no-fallback request. A DirectML request on a system without
the trusted DirectML provider library produces evidence shaped like:

```json
{
  "model": "formula-rec",
  "requestedProvider": "directml",
  "selectedProvider": "cpu",
  "fallback": true,
  "reasons": [
    {
      "candidate": "directml",
      "accepted": false,
      "code": "PROVIDER_LIBRARY_MISSING",
      "message": "trusted runtime library is missing"
    },
    {
      "candidate": "cpu",
      "accepted": true,
      "code": "ACCEPTED",
      "message": "provider is available and configured"
    }
  ]
}
```

Provider options reject nested arrays and objects. Runtime-specific adapters
may impose a stricter allowlist before session creation.
