//! Deterministic mixed-Markdown compound documents derived from the frozen
//! full formula corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use latexsnipper_ast::{Block, Inline};
use latexsnipper_conversion::parse_markdown_to_document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::formula_corpus::{
    validate_formula_corpus, validate_formula_plan, FormulaCorpus, FormulaCorpusError,
    FormulaCorpusPlan, FormulaCorpusTier,
};

pub const FORMULA_COMPOUND_CORPUS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCompoundDocument {
    pub id: String,
    pub title: String,
    pub markdown: String,
    pub formula_record_ids: Vec<String>,
    pub expected_formula_count: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCompoundCorpus {
    pub schema_version: u32,
    pub plan_id: String,
    pub seed: u64,
    pub source_formula_sha256: String,
    pub content_sha256: String,
    pub documents: Vec<FormulaCompoundDocument>,
}

pub fn read_formula_compound_corpus(
    path: &Path,
) -> Result<FormulaCompoundCorpus, FormulaCorpusError> {
    let bytes = fs::read(path).map_err(|source| FormulaCorpusError::Read {
        path: PathBuf::from(path),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| FormulaCorpusError::Json {
        path: PathBuf::from(path),
        source,
    })
}

pub fn generate_formula_compound_corpus(
    plan: &FormulaCorpusPlan,
    formula_corpus: &FormulaCorpus,
) -> Result<FormulaCompoundCorpus, FormulaCorpusError> {
    validate_formula_plan(plan)?;
    validate_formula_corpus(plan, formula_corpus)?;
    if formula_corpus.tier != FormulaCorpusTier::Full {
        return invalid("compound documents require the digest-frozen full formula corpus");
    }
    if formula_corpus.records.len() < plan.compound_document_count {
        return invalid("compoundDocumentCount cannot exceed the formula record count");
    }

    let shuffled = deterministic_indices(formula_corpus.records.len(), plan.seed);
    let base_count = formula_corpus.records.len() / plan.compound_document_count;
    let remainder = formula_corpus.records.len() % plan.compound_document_count;
    let mut cursor = 0;
    let mut documents = Vec::with_capacity(plan.compound_document_count);

    for document_index in 0..plan.compound_document_count {
        let formula_count = base_count + usize::from(document_index < remainder);
        let indices = &shuffled[cursor..cursor + formula_count];
        cursor += formula_count;
        let records: Vec<_> = indices
            .iter()
            .map(|index| &formula_corpus.records[*index])
            .collect();
        let title = format!("复合公式文档 {:03}", document_index + 1);
        let markdown = render_compound_markdown(document_index, &title, &records)?;
        let mut tags = vec![
            "mixed-markdown".to_string(),
            "heading".to_string(),
            "code-block-decoy".to_string(),
            "inline-math".to_string(),
            "display-math".to_string(),
            "list".to_string(),
            "blockquote".to_string(),
        ];
        if records.iter().any(|record| {
            record.expected_outcome != crate::formula_corpus::ExpectedFormulaOutcome::Valid
        }) {
            tags.push("contains-intentional-error".to_string());
        }
        tags.sort();
        documents.push(FormulaCompoundDocument {
            id: format!("compound-{document_index:03}"),
            title,
            markdown,
            formula_record_ids: records.iter().map(|record| record.id.clone()).collect(),
            expected_formula_count: formula_count,
            tags,
        });
    }

    let content_sha256 = compound_documents_digest(&documents)?;
    let corpus = FormulaCompoundCorpus {
        schema_version: FORMULA_COMPOUND_CORPUS_SCHEMA_VERSION,
        plan_id: plan.id.clone(),
        seed: plan.seed,
        source_formula_sha256: formula_corpus.content_sha256.clone(),
        content_sha256,
        documents,
    };
    validate_formula_compound_corpus(plan, formula_corpus, &corpus)?;
    Ok(corpus)
}

pub fn validate_formula_compound_corpus(
    plan: &FormulaCorpusPlan,
    formula_corpus: &FormulaCorpus,
    compound_corpus: &FormulaCompoundCorpus,
) -> Result<(), FormulaCorpusError> {
    validate_formula_plan(plan)?;
    validate_formula_corpus(plan, formula_corpus)?;
    if formula_corpus.tier != FormulaCorpusTier::Full {
        return invalid("compound documents require the full formula corpus");
    }
    if compound_corpus.schema_version != FORMULA_COMPOUND_CORPUS_SCHEMA_VERSION {
        return invalid(format!(
            "compound schemaVersion must be {FORMULA_COMPOUND_CORPUS_SCHEMA_VERSION}, got {}",
            compound_corpus.schema_version
        ));
    }
    if compound_corpus.plan_id != plan.id || compound_corpus.seed != plan.seed {
        return invalid("compound corpus plan identity does not match the plan");
    }
    if compound_corpus.source_formula_sha256 != formula_corpus.content_sha256 {
        return invalid("compound corpus sourceFormulaSha256 does not match the full corpus");
    }
    if compound_corpus.documents.len() != plan.compound_document_count {
        return invalid(format!(
            "compound corpus requires {} documents, got {}",
            plan.compound_document_count,
            compound_corpus.documents.len()
        ));
    }
    let digest = compound_documents_digest(&compound_corpus.documents)?;
    if digest != compound_corpus.content_sha256 {
        return invalid(format!(
            "compound corpus contentSha256 mismatch: declared {}, computed {digest}",
            compound_corpus.content_sha256
        ));
    }
    if digest != plan.compound_content_sha256 {
        return invalid(format!(
            "compound digest drifted: plan requires {}, corpus has {digest}",
            plan.compound_content_sha256
        ));
    }

    let formula_ids: BTreeSet<_> = formula_corpus
        .records
        .iter()
        .map(|record| record.id.as_str())
        .collect();
    let mut document_ids = BTreeSet::new();
    let mut formula_occurrences: BTreeMap<&str, usize> = BTreeMap::new();

    for document in &compound_corpus.documents {
        if !document_ids.insert(document.id.as_str()) {
            return invalid(format!("duplicate compound document id '{}'", document.id));
        }
        if document.title.trim().is_empty() || document.markdown.trim().is_empty() {
            return invalid(format!("compound document '{}' is empty", document.id));
        }
        if document.expected_formula_count != document.formula_record_ids.len() {
            return invalid(format!(
                "compound document '{}' formula count metadata drifted",
                document.id
            ));
        }
        if !document.markdown.starts_with("# ")
            || !document.markdown.contains("```text")
            || !document.markdown.contains("$$")
            || !document.markdown.contains("$\\")
        {
            return invalid(format!(
                "compound document '{}' is missing a required Markdown context",
                document.id
            ));
        }
        let parsed = parse_markdown_to_document(&document.markdown);
        let parsed_formula_count = count_document_formulas(&parsed);
        if parsed_formula_count != document.expected_formula_count {
            return invalid(format!(
                "compound document '{}' parsed {parsed_formula_count} formulas, expected {}",
                document.id, document.expected_formula_count
            ));
        }
        for record_id in &document.formula_record_ids {
            if !formula_ids.contains(record_id.as_str()) {
                return invalid(format!(
                    "compound document '{}' references unknown formula record '{record_id}'",
                    document.id
                ));
            }
            *formula_occurrences.entry(record_id.as_str()).or_default() += 1;
        }
    }

    if formula_occurrences.len() != formula_corpus.records.len()
        || formula_occurrences.values().any(|count| *count != 1)
    {
        return invalid("compound documents must cover every full formula record exactly once");
    }
    Ok(())
}

fn deterministic_indices(count: usize, seed: u64) -> Vec<usize> {
    let mut indices: Vec<_> = (0..count).collect();
    let mut state = seed;
    for index in (1..count).rev() {
        state = splitmix64(state);
        let swap_index = (state % (index as u64 + 1)) as usize;
        indices.swap(index, swap_index);
    }
    indices
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn render_compound_markdown(
    document_index: usize,
    title: &str,
    records: &[&crate::formula_corpus::FormulaCorpusRecord],
) -> Result<String, FormulaCorpusError> {
    use std::fmt::Write as _;

    let mut markdown = String::new();
    writeln!(markdown, "# {title}\n").unwrap();
    writeln!(
        markdown,
        "本文件用于验证标题、正文、列表、引用、代码块与公式混排。Document {}.\n",
        document_index + 1
    )
    .unwrap();
    writeln!(markdown, "```text").unwrap();
    writeln!(
        markdown,
        "This decoy must stay code: $ignored_{{{document_index}}}$"
    )
    .unwrap();
    writeln!(markdown, "```\n").unwrap();

    for (position, record) in records.iter().enumerate() {
        let formula = compound_formula(record);
        match position % 4 {
            0 => {
                writeln!(markdown, "## Formula {}\n", position + 1).unwrap();
                writeln!(markdown, "$$").unwrap();
                writeln!(markdown, "{formula}").unwrap();
                writeln!(markdown, "$$\n").unwrap();
            }
            1 => {
                writeln!(
                    markdown,
                    "中文行内公式 {}：${formula}$，用于验证上下文边界。\n",
                    position + 1
                )
                .unwrap();
            }
            2 => {
                writeln!(markdown, "- 列表公式 {}：${formula}$\n", position + 1).unwrap();
            }
            _ => {
                writeln!(markdown, "> 引用中的公式 {}：${formula}$\n", position + 1).unwrap();
            }
        }
    }
    Ok(markdown)
}

fn compound_formula(record: &crate::formula_corpus::FormulaCorpusRecord) -> String {
    if let Some(source) = &record.normalized_source {
        return source.clone();
    }
    let stable_suffix: String = record
        .id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    if record.tags.iter().any(|tag| tag == "missing-brace") {
        format!(r"\frac{{broken_{{{stable_suffix}}}}}{{")
    } else {
        format!(r"\begin{{matrix}}broken_{{{stable_suffix}}}&1")
    }
}

fn count_document_formulas(document: &latexsnipper_ast::Document) -> usize {
    document
        .pages
        .iter()
        .flat_map(|page| page.blocks.iter())
        .map(count_block_formulas)
        .sum()
}

fn count_block_formulas(block: &Block) -> usize {
    match block {
        Block::Formula(_) => 1,
        Block::Heading(heading) => count_inline_formulas(&heading.inlines),
        Block::Paragraph(paragraph) => count_inline_formulas(&paragraph.inlines),
        Block::List(list) => list
            .items
            .iter()
            .flat_map(|item| item.content.iter())
            .map(count_block_formulas)
            .sum(),
        Block::Quote(quote) => quote.blocks.iter().map(count_block_formulas).sum(),
        _ => 0,
    }
}

fn count_inline_formulas(inlines: &[Inline]) -> usize {
    inlines
        .iter()
        .filter(|inline| matches!(inline, Inline::Formula(_)))
        .count()
}

fn compound_documents_digest(
    documents: &[FormulaCompoundDocument],
) -> Result<String, FormulaCorpusError> {
    let bytes = serde_json::to_vec(documents)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn invalid<T>(message: impl Into<String>) -> Result<T, FormulaCorpusError> {
    Err(FormulaCorpusError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::formula_corpus::{generate_formula_full, read_formula_plan};

    use super::*;

    fn plan_and_full() -> (FormulaCorpusPlan, FormulaCorpus) {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evaluation/formula-corpus/plan.json");
        let plan = read_formula_plan(&path).expect("checked-in plan");
        let full = generate_formula_full(&plan).expect("full corpus");
        (plan, full)
    }

    #[test]
    fn compound_generation_is_deterministic_and_covers_every_formula_once() {
        let (plan, full) = plan_and_full();
        let first = generate_formula_compound_corpus(&plan, &full).expect("compound corpus");
        let second = generate_formula_compound_corpus(&plan, &full).expect("repeat");
        assert_eq!(first, second);
        assert_eq!(first.documents.len(), 500);
        assert!(first
            .documents
            .iter()
            .all(|document| document.expected_formula_count == 20));
        validate_formula_compound_corpus(&plan, &full, &first).expect("valid compound corpus");
    }

    #[test]
    fn compound_validation_rejects_duplicate_formula_coverage() {
        let (mut plan, full) = plan_and_full();
        let mut compound = generate_formula_compound_corpus(&plan, &full).expect("compound corpus");
        compound.documents[1].formula_record_ids[0] =
            compound.documents[0].formula_record_ids[0].clone();
        compound.content_sha256 = compound_documents_digest(&compound.documents).unwrap();
        plan.compound_content_sha256 = compound.content_sha256.clone();
        let error = validate_formula_compound_corpus(&plan, &full, &compound)
            .expect_err("duplicate coverage must fail");
        assert!(error.to_string().contains("exactly once"));
    }
}
