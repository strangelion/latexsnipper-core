# ADR 0002: Executable capability registry

Status: accepted

Format identity, aliases, MIME type, binary/text classification, fidelity, compile
features, runtime requirements, target availability, and unavailable reasons come
from the shared conversion capability registry. CLI help/suggestions and WASM
metadata project that registry instead of maintaining target-specific tables.

Drift tests compare all projections. A feature is unavailable unless both compiled
support and runtime prerequisites are true; documentation is explanatory and never
overrides executable metadata.
