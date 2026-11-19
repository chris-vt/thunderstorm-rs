use serde::{Deserialize, Serialize};

/// High-level semantic tag extracted from the Fully Specified Name (FSN).
///
/// In SNOMED CT, every FSN ends with a parenthetical tag indicating its domain:
/// - `"Asthma (disorder)"` -> `SemanticTag::Disorder`
/// - `"Wheezing (finding)"` -> `SemanticTag::Finding`
/// - `"Appendectomy (procedure)"` -> `SemanticTag::Procedure`
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticTag {
    Disorder,
    Finding,
    Procedure,
    Situation,
    ObservableEntity,
    BodyStructure,
    Substance,
    Organism,
    Event,
    Specimen,
    PhysicalObject,
    QualifierValue,
    RegimeTherapy,
    SocialContext,
    StagingScale,
    MorphologicAbnormality,
    RecordArtifact,
    Metadata,
    Other,
}

impl SemanticTag {
    /// Extracts the semantic tag from the end of a Fully Specified Name.
    ///
    /// Example: `"Asthma (disorder)"` -> `SemanticTag::Disorder`
    pub fn from_fsn(fsn: &str) -> Self {
        let trimmed = fsn.trim();
        if !trimmed.ends_with(')') {
            return SemanticTag::Other;
        }

        if let Some(open_paren) = trimmed.rfind('(') {
            let tag_str = &trimmed[open_paren + 1..trimmed.len() - 1].trim();
            match tag_str.to_ascii_lowercase().as_str() {
                "disorder" => SemanticTag::Disorder,
                "finding" => SemanticTag::Finding,
                "procedure" => SemanticTag::Procedure,
                "situation" | "situation with explicit context" => SemanticTag::Situation,
                "observable entity" => SemanticTag::ObservableEntity,
                "body structure" => SemanticTag::BodyStructure,
                "substance" => SemanticTag::Substance,
                "organism" => SemanticTag::Organism,
                "event" => SemanticTag::Event,
                "specimen" => SemanticTag::Specimen,
                "physical object" => SemanticTag::PhysicalObject,
                "qualifier value" => SemanticTag::QualifierValue,
                "regime/therapy" => SemanticTag::RegimeTherapy,
                "social context" => SemanticTag::SocialContext,
                "staging scale" => SemanticTag::StagingScale,
                "morphologic abnormality" => SemanticTag::MorphologicAbnormality,
                "record artifact" => SemanticTag::RecordArtifact,
                "metadata" | "foundation metadata concept" => SemanticTag::Metadata,
                _ => SemanticTag::Other,
            }
        } else {
            SemanticTag::Other
        }
    }

    /// Returns a string representation of the semantic tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            SemanticTag::Disorder => "disorder",
            SemanticTag::Finding => "finding",
            SemanticTag::Procedure => "procedure",
            SemanticTag::Situation => "situation",
            SemanticTag::ObservableEntity => "observable entity",
            SemanticTag::BodyStructure => "body structure",
            SemanticTag::Substance => "substance",
            SemanticTag::Organism => "organism",
            SemanticTag::Event => "event",
            SemanticTag::Specimen => "specimen",
            SemanticTag::PhysicalObject => "physical object",
            SemanticTag::QualifierValue => "qualifier value",
            SemanticTag::RegimeTherapy => "regime/therapy",
            SemanticTag::SocialContext => "social context",
            SemanticTag::StagingScale => "staging scale",
            SemanticTag::MorphologicAbnormality => "morphologic abnormality",
            SemanticTag::RecordArtifact => "record artifact",
            SemanticTag::Metadata => "metadata",
            SemanticTag::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tag_extraction() {
        assert_eq!(SemanticTag::from_fsn("Asthma (disorder)"), SemanticTag::Disorder);
        assert_eq!(SemanticTag::from_fsn("Wheezing (finding)"), SemanticTag::Finding);
        assert_eq!(SemanticTag::from_fsn("Appendectomy (procedure)"), SemanticTag::Procedure);
        assert_eq!(SemanticTag::from_fsn("Family history of diabetes (situation)"), SemanticTag::Situation);
        assert_eq!(SemanticTag::from_fsn("Lung structure (body structure)"), SemanticTag::BodyStructure);
        assert_eq!(SemanticTag::from_fsn("Amoxicillin (substance)"), SemanticTag::Substance);
        assert_eq!(SemanticTag::from_fsn("Unknown concept without tag"), SemanticTag::Other);
    }
}
