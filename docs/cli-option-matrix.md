# CLI advanced option propagation

The CLI rejects every unknown flag through Clap. It never accepts and silently ignores
an advanced option. Currently implemented options are deliberately narrower than the
underlying AST option types.

| Option family | CLI status | Propagation and validation |
|---|---|---|
| Page range | implemented for `convert` | Parsed as typed `PageRange`, rejected before I/O when invalid, passed through `AdvancedConversionOptions` to `ImportOptions`, and reported by `CLI_OPTIONS_APPLIED`. |
| Strict preservation | implemented for `convert` | Sets strict import plus unknown OOXML part preservation; diagnostics identify activation. |
| Parse mode | implemented where parsing is selected | Accepted values and aliases come from `DocumentParseMode`; invalid values are rejected. |
| Recognition/OCR mode | implemented for recognition commands | Accepted values and aliases come from `RecognizeMode` and propagate to the engine. |
| Slide range, sheet selection | not exposed | Importers have separate safety budgets but no complete CLI selection contract; supplied flags are rejected. |
| DPI, scale, background, transparency | not exposed | Renderer-specific typed options are not consistently supported across exporters; supplied flags are rejected. |
| Provider, acceleration | not exposed as conversion flags | Runtime selection has no uniform cross-target CLI guarantee; supplied flags are rejected. |
| Model variant, models directory | command-specific existing paths only | They are not accepted by unrelated conversion commands. |
| Timeout, memory limit | plugin/WASM runtime-specific | Plugin manifests and WASM model limits enforce their own budgets; conversion flags are rejected. |
| Fidelity mode | not exposed | Strict preservation is the current enforceable mode; a general fidelity enum requires importer/exporter support first. |

Adding a flag requires a typed options field, format/target support matrix, early
invalid-combination rejection, end-to-end propagation, diagnostic evidence, and an
integration test in the same change.
