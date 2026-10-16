"""Persistent Python bindings for LaTeXSnipper Core."""

from ._native import (
    CancellationToken, FormulaRegistry, LaTeXSnipperError, Session, __version__,
    FormulaPreferences, FormulaPreferenceCapture, FormulaPreferenceReceipt,
    FormulaPreferenceStorageStage,
    convert_formula, formula_conversion_capabilities,
    complete_formula, apply_formula_completion, formula_completion_capabilities,
    complete_formula_scoped, apply_formula_completion_scoped, formula_scoped_completion_capabilities,
    complete_formula_fragments, apply_formula_completion_fragments, formula_fragment_completion_capabilities,
)

__all__ = [
    "CancellationToken", "FormulaRegistry", "LaTeXSnipperError", "Session", "__version__",
    "FormulaPreferences", "FormulaPreferenceCapture", "FormulaPreferenceReceipt",
    "FormulaPreferenceStorageStage",
    "convert_formula", "formula_conversion_capabilities",
    "complete_formula", "apply_formula_completion", "formula_completion_capabilities",
    "complete_formula_scoped", "apply_formula_completion_scoped", "formula_scoped_completion_capabilities",
    "complete_formula_fragments", "apply_formula_completion_fragments", "formula_fragment_completion_capabilities",
]
