use crate::{ConceptId, SemanticTag};

/// Filter condition for defining attributes.
///
/// Example: `Finding site (363698007) = << Lung structure (39607008)`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeFilter {
    /// The relationship type concept (e.g., Finding site, Causative agent, Method).
    pub type_id: ConceptId,
    /// The target value concept (e.g., Lung structure, Bacteria).
    pub value_id: ConceptId,
    /// If true, matches value_id OR any of its hierarchical descendants (<< value_id).
    pub include_descendant_values: bool,
}

impl AttributeFilter {
    /// Creates a new attribute filter.
    pub fn new(type_id: ConceptId, value_id: ConceptId, include_descendant_values: bool) -> Self {
        Self {
            type_id,
            value_id,
            include_descendant_values,
        }
    }
}

/// A group of attribute filters that must ALL be satisfied within the SAME role group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupedAttributeFilter {
    pub attributes: Vec<AttributeFilter>,
}

/// A summary of a PCD refset cluster's membership and clinical usage.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RefsetSummary {
    pub name: String,
    pub total_codes: usize,
    pub active_codes: usize,
    pub total_usage: u64,
}

/// A structured clinical search result from the ontology engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult<'a> {
    pub sctid: ConceptId,
    pub term: &'a str,
    pub tag: SemanticTag,
    pub is_active: bool,
    pub usage: Option<u64>,
    pub pcd_clusters: &'a [String],
}

/// A comprehensive search and ECL-style query representation.
///
/// Combines text, hierarchical Is-A closures, semantic tags, defining attributes,
/// PCD refsets, and activity thresholds.
#[derive(Clone, Debug, Default)]
pub struct SearchQuery {
    /// Positive text search filters. If multiple, all must match (AND).
    /// Supports wildcards (* and ?). Values with '|' denote OR (e.g., "asthma|wheeze").
    pub include_texts: Vec<String>,
    /// Negative text search filters. Any match is excluded (NOT).
    /// Supports wildcards (* and ?). Values with '|' denote OR (e.g., "suspected|history").
    pub exclude_texts: Vec<String>,

    /// Roots whose descendants should be included: (<< Root1 OR << Root2 ...)
    pub include_isa: Vec<ConceptId>,
    /// Branches to surgically carve out: (MINUS << Exclude1 ...)
    pub exclude_isa: Vec<ConceptId>,

    /// Semantic category tags to include (e.g., [Disorder, Finding]).
    pub include_tags: Vec<SemanticTag>,
    /// Semantic category tags to exclude (e.g., [Situation, Procedure]).
    pub exclude_tags: Vec<SemanticTag>,

    /// Independent attribute filters (all must be satisfied: Attr1 AND Attr2).
    pub include_attributes: Vec<AttributeFilter>,
    /// Attribute filters to exclude (e.g. MINUS Severity = Severe).
    pub exclude_attributes: Vec<AttributeFilter>,

    /// Grouped attribute filters (all attributes in a group must share the same role group).
    pub include_grouped_attributes: Vec<GroupedAttributeFilter>,

    /// Restrict results to members of any specified PCD refset clusters (union: Cluster1 OR Cluster2).
    pub include_refsets: Vec<String>,
    /// Exclude members belonging to any of these PCD refset clusters.
    pub exclude_refsets: Vec<String>,

    /// Minimum England GP code usage threshold (filters out codes with usage < min).
    pub min_usage: Option<u64>,

    /// If true, sorts results descending by real-world NHS England usage count.
    pub sort_by_usage: bool,

    /// Maximum results to return (default: 100).
    pub limit: usize,
}

impl SearchQuery {
    /// Creates a new empty query with standard limit (100).
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }

    /// Add a positive text search pattern. If called multiple times, all patterns must match (AND).
    /// Supports wildcards (*, ?) and pipe-separated alternatives (e.g. "asthma|wheez*").
    pub fn include_text(mut self, text: impl Into<String>) -> Self {
        self.include_texts.push(text.into());
        self
    }

    /// Add a negative text search pattern. If matched, the concept is excluded.
    /// Supports wildcards (*, ?) and pipe-separated alternatives (e.g. "mild|suspected").
    pub fn exclude_text(mut self, text: impl Into<String>) -> Self {
        self.exclude_texts.push(text.into());
        self
    }

    /// Backwards compatible alias for include_text.
    pub fn with_text(self, text: impl Into<String>) -> Self {
        self.include_text(text)
    }

    /// Include concepts under this Is-A root (<< root).
    pub fn include_isa(mut self, root: ConceptId) -> Self {
        self.include_isa.push(root);
        self
    }

    /// Exclude concepts under this Is-A root (MINUS << root).
    pub fn exclude_isa(mut self, root: ConceptId) -> Self {
        self.exclude_isa.push(root);
        self
    }

    /// Include concepts with this semantic tag.
    pub fn include_tag(mut self, tag: SemanticTag) -> Self {
        self.include_tags.push(tag);
        self
    }

    /// Exclude concepts with this semantic tag.
    pub fn exclude_tag(mut self, tag: SemanticTag) -> Self {
        self.exclude_tags.push(tag);
        self
    }

    /// Include concepts possessing this attribute.
    pub fn include_attribute(
        mut self,
        type_id: ConceptId,
        value_id: ConceptId,
        include_descendant_values: bool,
    ) -> Self {
        self.include_attributes.push(AttributeFilter::new(
            type_id,
            value_id,
            include_descendant_values,
        ));
        self
    }

    /// Exclude concepts possessing this attribute.
    pub fn exclude_attribute(
        mut self,
        type_id: ConceptId,
        value_id: ConceptId,
        include_descendant_values: bool,
    ) -> Self {
        self.exclude_attributes.push(AttributeFilter::new(
            type_id,
            value_id,
            include_descendant_values,
        ));
        self
    }

    /// Include concepts that satisfy a set of attributes in the SAME role group.
    pub fn include_grouped_attributes(mut self, filters: Vec<AttributeFilter>) -> Self {
        self.include_grouped_attributes
            .push(GroupedAttributeFilter { attributes: filters });
        self
    }

    /// Restrict search to members of a PCD cluster (e.g. "AST_COD", "CKD_COD"). Repeatable for union.
    pub fn include_refset(mut self, cluster: impl Into<String>) -> Self {
        self.include_refsets.push(cluster.into());
        self
    }

    /// Exclude members belonging to this PCD cluster.
    pub fn exclude_refset(mut self, cluster: impl Into<String>) -> Self {
        self.exclude_refsets.push(cluster.into());
        self
    }

    /// Backwards compatible alias for include_refset.
    pub fn member_of_refset(self, cluster: impl Into<String>) -> Self {
        self.include_refset(cluster)
    }

    /// Filter out concepts with activity lower than this minimum national usage count.
    pub fn min_usage(mut self, min: u64) -> Self {
        self.min_usage = Some(min);
        self
    }

    /// Sort results descending by real-world usage count.
    pub fn sort_by_usage(mut self, sort: bool) -> Self {
        self.sort_by_usage = sort;
        self
    }

    /// Set result limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}
