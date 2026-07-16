# CLI option propagation matrix

This matrix is the Core 3 audit record. `propagated` means the value reaches the
operation that owns its semantics. `rejected` means Clap or the handler returns
a stable non-zero exit instead of silently ignoring the value.

| Command | Options and arguments | Disposition |
|---|---|---|
| `import` | `input`, `--output` | propagated to importer and JSON writer |
| `convert` | inputs, `--to`, `--output`, `--force-binary-stdout` | propagated to format resolution and output policy |
| `convert` | `--force`, `--no-clobber`, `--atomic` | propagated; contradictory overwrite flags rejected by Clap |
| `convert` | `--diagnostics`, `--strict`, `--fail-on-warning` | propagated to diagnostic rendering and failure policy |
| `convert` | `--page-range`, `--strict-preservation` | parsed and propagated to typed import/conversion options |
| `convert` | `--quiet`, `--verbose` | propagated; simultaneous use rejected by Clap |
| `convert` | `--recursive`, `--output-dir`, `--jobs`, `--continue-on-error`, `--report` | propagated to batch discovery, scheduling, paths, and report writer |
| `export` | `input`, `--to`, `--output` | propagated to import and visual exporter |
| `inspect` | `input`, `--json` | propagated to detection/import and renderer |
| `validate` | `input` | propagated to structural importer |
| `recognize`, `rec` | `--input`, `--format`, `--output`, `--parse-mode`, `--recognize-mode` | propagated; output extension never overrides `--format` |
| `parse`, `render` | `--latex` | propagated to parser/renderer |
| `capabilities` | `--format`, `--input`, `--output`, `--api-version` | propagated; unknown format rejected; explicit API versions are JSON-only, JSON defaults to v3, and v2 is the compatibility adapter |
| `models download` | `--category`, `--all`, `--manifest-url` | propagated; `--category` with `--all` rejected by Clap |
| `models list`, `models verify` | `--category` | propagated as category filter |
| `models purge` | `--category`, `--variant`, `--yes` | propagated; variant requires category; missing confirmation rejected |
| `plugin` | `--store-dir` | propagated to local store and signed-registry state roots |
| `plugin search/info/verify/install/update/rollback/uninstall/enable/disable/revoke` | positional IDs/sources, `--all` | propagated; update requires ID or `--all`; remote uninstall is explicitly rejected pending atomic removal support |
| `plugin registry add/remove/trust/refresh` | names, origins, roots, `--yes`, `--offline` | propagated; trust/removal require confirmation; insecure origins rejected |
| `job run` | `--input`, `--format`, `--output`, `--mode` | propagated to recognition and conversion; output extension does not change format |
| `job inspect` | job ID | propagated to job lookup |
| `migrate plugin-manifest/model-manifest/document` | input, `--output`, `--force`, `--json` | propagated; source overwrite and unsafe automatic migration rejected |
| `migrate inspect` | input, `--json` | propagated; read-only by contract |
| `completions` | shell | propagated through the generated Clap schema |
| `manpages` | `--output-dir` | propagated to the roff writer |

The CLI integration suite exercises conflicts, migration source preservation,
manual-action exit code 11, capability envelope selection, overwrite policy,
batch propagation, plugin trust boundaries, model purge scope, completions, and
man-page generation.
