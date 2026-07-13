use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

use crate::request::PluginRequest;
use crate::response::PluginResponse;
use crate::{DocumentPatch, DocumentView, PluginExecutionContext, PluginManifest};

/// Trait for extending Core capabilities with standard interfaces.
///
/// Plugins can:
/// - Post-process OCR results
/// - Add new detection/recognition capabilities
/// - Transform documents
/// - Add custom export formats
///
/// Plugins are isolated — one plugin's failure doesn't affect others.
pub trait Plugin: Send + Sync {
    /// Plugin name (unique identifier).
    fn name(&self) -> &str;

    /// Plugin version.
    fn version(&self) -> &str;

    /// Return the versioned identity, ordering, permission, and hook contract.
    fn manifest(&self) -> PluginManifest {
        PluginManifest::built_in(self.name(), self.version())
    }

    /// Initialize the plugin.
    /// Called once when the plugin is registered.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Handle a request and return a response.
    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse>;

    /// Handle a request with cooperative cancellation and enforced host permissions.
    ///
    /// The default preserves the version 1 plugin API. Long-running trusted plugins
    /// should override this method and call `context.checkpoint()` regularly.
    fn handle_with_context(
        &self,
        request: &PluginRequest,
        context: &PluginExecutionContext,
    ) -> Result<PluginResponse> {
        context.checkpoint()?;
        let response = self.handle(request)?;
        context.checkpoint()?;
        Ok(response)
    }

    /// Produce a bounded patch without cloning the full document.
    ///
    /// Legacy plugins may keep implementing `handle`; patch-aware plugins can
    /// override this method and return `Some` to use the transactional path.
    fn document_patch(&self, _view: DocumentView<'_>) -> Result<Option<DocumentPatch>> {
        Ok(None)
    }

    /// Produce a patch with cooperative cancellation support.
    fn document_patch_with_context(
        &self,
        view: DocumentView<'_>,
        context: &PluginExecutionContext,
    ) -> Result<Option<DocumentPatch>> {
        context.checkpoint()?;
        let patch = self.document_patch(view)?;
        context.checkpoint()?;
        Ok(patch)
    }

    /// Cleanup resources.
    /// Called when the plugin is unregistered.
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A plugin that produces atomic document patches from a read-only view.
pub struct PatchPlugin {
    name: String,
    version: String,
    manifest: PluginManifest,
    #[allow(clippy::type_complexity)]
    patch: Box<dyn Fn(DocumentView<'_>) -> Result<DocumentPatch> + Send + Sync>,
}

impl PatchPlugin {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        patch: impl Fn(DocumentView<'_>) -> Result<DocumentPatch> + Send + Sync + 'static,
    ) -> Self {
        let name = name.into();
        let version = version.into();
        Self {
            manifest: PluginManifest::built_in(name.clone(), version.clone()),
            name,
            version,
            patch: Box::new(patch),
        }
    }

    pub fn with_manifest(mut self, manifest: PluginManifest) -> Self {
        self.name.clone_from(&manifest.id);
        self.version.clone_from(&manifest.version);
        self.manifest = manifest;
        self
    }
}

impl Plugin for PatchPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
        let mut document = request.document.clone();
        (self.patch)(DocumentView::new(&document))?.apply(&mut document)?;
        Ok(PluginResponse {
            document,
            metadata: request.metadata.clone(),
        })
    }

    fn document_patch(&self, view: DocumentView<'_>) -> Result<Option<DocumentPatch>> {
        (self.patch)(view).map(Some)
    }
}

/// A simple plugin that transforms documents.
pub struct TransformPlugin {
    name: String,
    version: String,
    manifest: PluginManifest,
    #[allow(clippy::type_complexity)]
    transform: Box<dyn Fn(&mut Document) -> Result<()> + Send + Sync>,
}

impl TransformPlugin {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        transform: impl Fn(&mut Document) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        let name = name.into();
        let version = version.into();
        Self {
            manifest: PluginManifest::built_in(name.clone(), version.clone()),
            name,
            version,
            transform: Box::new(transform),
        }
    }

    pub fn with_manifest(mut self, manifest: PluginManifest) -> Self {
        self.name = manifest.id.clone();
        self.version = manifest.version.clone();
        self.manifest = manifest;
        self
    }
}

impl Plugin for TransformPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
        let mut doc = request.document.clone();
        (self.transform)(&mut doc)?;

        Ok(PluginResponse {
            document: doc,
            metadata: request.metadata.clone(),
        })
    }
}
