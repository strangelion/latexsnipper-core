//! Generated host bindings for the immutable plugin WIT v1 world.

wasmtime::component::bindgen!({
    path: "../../wit/plugin-v1",
    world: "plugin",
    imports: { default: trappable },
    with: {
        "latexsnipper:plugin/model-artifact-broker/artifact": crate::host::ModelArtifactResource,
        "latexsnipper:plugin/temporary-storage-broker/temporary-file": crate::host::TemporaryFileResource,
    },
});
