pub mod loader;
pub mod query;
pub mod tag;

pub use query::{AttributeFilter, GroupedAttributeFilter, RefsetSummary, SearchQuery, SearchResult};
pub use tag::SemanticTag;

use hashbrown::HashMap;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Official SNOMED CT Concept Identifier (SCTID).
/// Up to 18 digits long, fits in an unsigned 64-bit integer.
pub type ConceptId = u64;

/// Internal dense graph node index (0 .. N-1).
/// Enables direct slice indexing and SIMD bitset operations.
pub type NodeId = u32;

/// Release metadata embedded directly into the binary snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    /// Date of the underlying SNOMED release (e.g., "2026-09-23").
    pub release_date: String,
    /// Edition name (e.g., "UK Clinical Edition").
    pub edition: String,
    /// Timestamp when this binary snapshot was compiled.
    pub built_at: String,
    /// Total count of all concepts (active + inactive).
    pub total_concepts: usize,
    /// Count of active concepts.
    pub active_concepts: usize,
    /// Total Is-A hierarchical edges.
    pub total_isa_relationships: usize,
    /// Total defining attribute edges.
    pub total_attributes: usize,
    /// Filename/source of embedded code usage data (if loaded).
    pub usage_source: Option<String>,
    /// Filename/source of embedded PCD refsets data (if loaded).
    pub refsets_source: Option<String>,
    /// Count of indexed PCD clusters.
    pub refset_cluster_count: usize,
}

/// A non-hierarchical semantic attribute defining a concept.
///
/// Example: Appendicitis has:
/// - Finding site (type_id) -> Appendix structure (value_id)
/// - Associated morphology (type_id) -> Inflammation (value_id)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Attribute {
    /// Role group (0 = ungrouped, 1, 2, 3... for grouped attributes).
    pub group: u8,
    /// The NodeId of the attribute type (e.g., Finding site, Causative agent).
    pub type_id: NodeId,
    /// The NodeId of the target attribute value (e.g., Appendix structure, Bacteria).
    pub value_id: NodeId,
}

/// In-memory SNOMED CT graph index utilizing Compressed Sparse Row (CSR) representations,
/// optionally enriched with NHS England primary care usage and PCD refset clusters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnomedGraph {
    // --- 0. Metadata, Status & Descriptions ---
    /// Embedded release metadata and audit trail.
    pub metadata: ReleaseMetadata,
    /// High-speed bitset marking which NodeIds are active (bit set = active).
    pub active_mask: RoaringBitmap,
    /// Canonical Fully Specified Name (FSN) for each NodeId (length = N).
    pub terms: Vec<String>,
    /// High-level semantic tag for each NodeId (length = N).
    pub tags: Vec<SemanticTag>,
    /// Precomputed bitmap of NodeIds for each semantic tag.
    pub tag_masks: HashMap<SemanticTag, RoaringBitmap>,

    // --- 1. Bidirectional ConceptId <-> NodeId Mapping ---
    /// Map clinical 64-bit SCTID to internal contiguous 32-bit NodeId.
    pub sctid_to_node: HashMap<ConceptId, NodeId>,
    /// Array indexed by NodeId to retrieve the original 64-bit SCTID.
    pub node_to_sctid: Vec<ConceptId>,

    // --- 2. Upward Hierarchy (Parents) CSR ---
    /// Start and end offsets into `parent_nodes` for each NodeId (length = N + 1).
    pub parent_offsets: Vec<u32>,
    /// Contiguous array storing all parent NodeIds.
    pub parent_nodes: Vec<NodeId>,

    // --- 3. Downward Hierarchy (Children) CSR ---
    /// Start and end offsets into `child_nodes` for each NodeId (length = N + 1).
    pub child_offsets: Vec<u32>,
    /// Contiguous array storing all child NodeIds.
    pub child_nodes: Vec<NodeId>,

    // --- 4. Forward Semantic Attributes CSR ---
    /// Start and end offsets into `attributes` for each NodeId (length = N + 1).
    pub attribute_offsets: Vec<u32>,
    /// Contiguous array storing all defining attributes.
    pub attributes: Vec<Attribute>,

    // --- 5. Reverse Semantic Attribute Index ---
    /// Inverted index: (attribute_type_node, attribute_value_node) -> Vec of concept NodeIds.
    /// Used for fast Expression Constraint Language (ECL) attribute evaluations.
    pub reverse_attributes: HashMap<(NodeId, NodeId), Vec<NodeId>>,

    // --- 6. Optional NHS England Extensions (Usage & PCD Refsets) ---
    /// Optional national code usage counts indexed by NodeId (length = N).
    pub usage: Option<Vec<u64>>,
    /// Optional PCD cluster index: Cluster_ID -> RoaringBitmap of NodeIds.
    pub pcd_clusters: Option<HashMap<String, RoaringBitmap>>,
    /// Optional reverse PCD lookup: NodeId -> list of Cluster_IDs (length = N).
    pub node_to_pcd: Option<Vec<SmallVec<[String; 2]>>>,
}

impl SnomedGraph {
    /// Creates an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes and saves the graph snapshot to a binary file using Bincode.
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    /// Deserializes and restores the graph snapshot from a binary file in < 50ms.
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let graph = bincode::deserialize_from(reader)?;
        Ok(graph)
    }

    /// Total number of indexed concepts (active + inactive).
    #[inline]
    pub fn concept_count(&self) -> usize {
        self.node_to_sctid.len()
    }

    /// Look up the internal NodeId for a given clinical SCTID.
    #[inline]
    pub fn get_node_id(&self, sctid: ConceptId) -> Option<NodeId> {
        self.sctid_to_node.get(&sctid).copied()
    }

    /// Look up the clinical SCTID for a given internal NodeId.
    #[inline]
    pub fn get_concept_id(&self, node: NodeId) -> Option<ConceptId> {
        self.node_to_sctid.get(node as usize).copied()
    }

    /// Returns whether an internal NodeId is active in this release.
    #[inline]
    pub fn is_active(&self, node: NodeId) -> bool {
        self.active_mask.contains(node)
    }

    /// Returns whether a clinical ConceptId is active (or None if not found).
    #[inline]
    pub fn is_sctid_active(&self, sctid: ConceptId) -> Option<bool> {
        self.get_node_id(sctid).map(|node| self.is_active(node))
    }

    /// Count of active concepts.
    #[inline]
    pub fn active_concept_count(&self) -> usize {
        self.active_mask.len() as usize
    }

    /// Count of inactive concepts.
    #[inline]
    pub fn inactive_concept_count(&self) -> usize {
        self.concept_count().saturating_sub(self.active_concept_count())
    }

    /// Returns whether this graph snapshot has national usage data embedded.
    #[inline]
    pub fn has_usage(&self) -> bool {
        self.usage.is_some()
    }

    /// Returns the national usage count for an internal NodeId (or 0 if no recorded usage).
    #[inline]
    pub fn get_usage(&self, node: NodeId) -> Option<u64> {
        self.usage.as_ref().and_then(|u| u.get(node as usize).copied())
    }

    /// Returns the national usage count for a clinical ConceptId.
    #[inline]
    pub fn get_sctid_usage(&self, sctid: ConceptId) -> Option<u64> {
        self.get_node_id(sctid).and_then(|node| self.get_usage(node))
    }

    /// Returns whether this graph snapshot has PCD refsets embedded.
    #[inline]
    pub fn has_pcd_refsets(&self) -> bool {
        self.pcd_clusters.is_some()
    }

    /// Returns the list of PCD cluster names that contain this NodeId.
    #[inline]
    pub fn get_pcd_clusters(&self, node: NodeId) -> &[String] {
        static EMPTY: [String; 0] = [];
        if let Some(ref map) = self.node_to_pcd {
            if let Some(clusters) = map.get(node as usize) {
                return clusters.as_slice();
            }
        }
        &EMPTY
    }

    /// Returns the list of PCD cluster names that contain this ConceptId.
    #[inline]
    pub fn get_sctid_pcd_clusters(&self, sctid: ConceptId) -> &[String] {
        match self.get_node_id(sctid) {
            Some(node) => self.get_pcd_clusters(node),
            None => &[],
        }
    }

    /// Returns the bitmap of concepts in a given PCD cluster.
    pub fn get_cluster_bitmap(&self, cluster_id: &str) -> Option<&RoaringBitmap> {
        self.pcd_clusters.as_ref().and_then(|c| c.get(cluster_id))
    }

    /// Returns a list of all available PCD cluster names and their member code counts.
    pub fn list_pcd_clusters(&self) -> Vec<(&str, usize)> {
        if let Some(ref clusters) = self.pcd_clusters {
            let mut list: Vec<(&str, usize)> = clusters
                .iter()
                .map(|(k, v)| (k.as_str(), v.len() as usize))
                .collect();
            list.sort_unstable_by_key(|(k, _)| *k);
            list
        } else {
            Vec::new()
        }
    }

    /// Returns a comprehensive list of all PCD refset clusters with code counts, active counts, and aggregate usage.
    pub fn list_pcd_clusters_summary(&self) -> Vec<RefsetSummary> {
        if let Some(ref clusters) = self.pcd_clusters {
            let mut list: Vec<RefsetSummary> = clusters
                .iter()
                .map(|(cluster_name, bitmap)| {
                    let total_codes = bitmap.len() as usize;
                    let mut active_codes = 0;
                    let mut total_usage = 0u64;

                    for node in bitmap {
                        if self.is_active(node) {
                            active_codes += 1;
                        }
                        if let Some(u) = self.get_usage(node) {
                            total_usage += u;
                        }
                    }

                    RefsetSummary {
                        name: cluster_name.clone(),
                        total_codes,
                        active_codes,
                        total_usage,
                    }
                })
                .collect();

            list.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            list
        } else {
            Vec::new()
        }
    }

    /// Returns the canonical term (FSN) for a given internal NodeId.
    #[inline]
    pub fn get_term(&self, node: NodeId) -> Option<&str> {
        self.terms.get(node as usize).map(|s| s.as_str())
    }

    /// Returns the canonical term (FSN) for a given clinical ConceptId.
    #[inline]
    pub fn get_sctid_term(&self, sctid: ConceptId) -> Option<&str> {
        self.get_node_id(sctid).and_then(|node| self.get_term(node))
    }

    /// Returns the semantic tag for an internal NodeId.
    #[inline]
    pub fn get_tag(&self, node: NodeId) -> Option<SemanticTag> {
        self.tags.get(node as usize).copied()
    }

    /// Returns the immediate parent NodeIds of a concept.
    /// Runs in O(1) time without heap allocations.
    #[inline]
    pub fn get_parents(&self, node: NodeId) -> &[NodeId] {
        let idx = node as usize;
        if idx + 1 >= self.parent_offsets.len() {
            return &[];
        }
        let start = self.parent_offsets[idx] as usize;
        let end = self.parent_offsets[idx + 1] as usize;
        &self.parent_nodes[start..end]
    }

    /// Returns the immediate child NodeIds of a concept.
    /// Runs in O(1) time without heap allocations.
    #[inline]
    pub fn get_children(&self, node: NodeId) -> &[NodeId] {
        let idx = node as usize;
        if idx + 1 >= self.child_offsets.len() {
            return &[];
        }
        let start = self.child_offsets[idx] as usize;
        let end = self.child_offsets[idx + 1] as usize;
        &self.child_nodes[start..end]
    }

    /// Returns all defining attributes of a concept.
    /// Runs in O(1) time without heap allocations.
    #[inline]
    pub fn get_attributes(&self, node: NodeId) -> &[Attribute] {
        let idx = node as usize;
        if idx + 1 >= self.attribute_offsets.len() {
            return &[];
        }
        let start = self.attribute_offsets[idx] as usize;
        let end = self.attribute_offsets[idx + 1] as usize;
        &self.attributes[start..end]
    }

    /// Computes the full transitive descendant closure (`<< node`) as a RoaringBitmap.
    /// Includes the root node itself.
    pub fn get_descendants_bitmap(&self, root: NodeId) -> RoaringBitmap {
        let mut bitmap = RoaringBitmap::new();
        let mut stack = vec![root];
        bitmap.insert(root);

        while let Some(current) = stack.pop() {
            for &child in self.get_children(current) {
                if bitmap.insert(child) {
                    stack.push(child);
                }
            }
        }
        bitmap
    }

    /// Computes the full transitive ancestor closure (`>> node`) as a RoaringBitmap.
    /// Includes the root node itself.
    pub fn get_ancestors_bitmap(&self, root: NodeId) -> RoaringBitmap {
        let mut bitmap = RoaringBitmap::new();
        let mut stack = vec![root];
        bitmap.insert(root);

        while let Some(current) = stack.pop() {
            for &parent in self.get_parents(current) {
                if bitmap.insert(parent) {
                    stack.push(parent);
                }
            }
        }
        bitmap
    }

    /// Evaluates a single `AttributeFilter` to find all concepts matching the condition.
    pub fn evaluate_attribute_filter(&self, filter: &AttributeFilter) -> RoaringBitmap {
        let mut bitmap = RoaringBitmap::new();

        let type_node = match self.get_node_id(filter.type_id) {
            Some(n) => n,
            None => return bitmap,
        };

        let val_node = match self.get_node_id(filter.value_id) {
            Some(n) => n,
            None => return bitmap,
        };

        if filter.include_descendant_values {
            let value_descendants = self.get_descendants_bitmap(val_node);
            for target_val in value_descendants {
                if let Some(concepts) = self.reverse_attributes.get(&(type_node, target_val)) {
                    bitmap.extend(concepts.iter().copied());
                }
            }
        } else {
            if let Some(concepts) = self.reverse_attributes.get(&(type_node, val_node)) {
                bitmap.extend(concepts.iter().copied());
            }
        }

        bitmap
    }

    /// Verifies if a candidate node satisfies all attributes in a GroupedAttributeFilter
    /// within the SAME role group.
    fn matches_grouped_attributes(&self, node: NodeId, grouped: &GroupedAttributeFilter) -> bool {
        if grouped.attributes.is_empty() {
            return true;
        }

        let attrs = self.get_attributes(node);
        if attrs.is_empty() {
            return false;
        }

        let mut groups = hashbrown::HashSet::new();
        for a in attrs {
            groups.insert(a.group);
        }

        groups.into_iter().any(|g| {
            grouped.attributes.iter().all(|filter| {
                let type_node = match self.get_node_id(filter.type_id) {
                    Some(n) => n,
                    None => return false,
                };
                let val_node = match self.get_node_id(filter.value_id) {
                    Some(n) => n,
                    None => return false,
                };

                let target_values: RoaringBitmap = if filter.include_descendant_values {
                    self.get_descendants_bitmap(val_node)
                } else {
                    let mut b = RoaringBitmap::new();
                    b.insert(val_node);
                    b
                };

                attrs.iter().any(|a| a.group == g && a.type_id == type_node && target_values.contains(a.value_id))
            })
        })
    }
}

/// Fast case-insensitive wildcard glob matching supporting '*' (0 or more chars),
/// '?' (single char), and '|' (OR alternative patterns).
fn matches_pattern(text: &str, pattern: &str) -> bool {
    // If pattern contains '|', any sub-pattern match suffices (OR logic)
    if pattern.contains('|') {
        return pattern.split('|').any(|sub| matches_pattern(text, sub.trim()));
    }

    let p = pattern.trim();
    if p.is_empty() {
        return true;
    }

    // Fast path: if no wildcards, simple case-insensitive substring search
    if !p.contains('*') && !p.contains('?') {
        return text.contains(p);
    }

    // Wildcard matching algorithm
    let text_chars: Vec<char> = text.chars().collect();
    let pat_chars: Vec<char> = p.chars().collect();
    let n = text_chars.len();
    let m = pat_chars.len();

    let mut t_idx = 0;
    let mut p_idx = 0;
    let mut star_idx = None;
    let mut match_idx = 0;

    while t_idx < n {
        if p_idx < m && (pat_chars[p_idx] == '?' || pat_chars[p_idx] == text_chars[t_idx]) {
            t_idx += 1;
            p_idx += 1;
        } else if p_idx < m && pat_chars[p_idx] == '*' {
            star_idx = Some(p_idx);
            match_idx = t_idx;
            p_idx += 1;
        } else if let Some(star) = star_idx {
            p_idx = star + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < m && pat_chars[p_idx] == '*' {
        p_idx += 1;
    }

    p_idx == m
}

impl SnomedGraph {
    /// Executes a composable clinical search query across the in-memory graph.
    ///
    /// Evaluates text, Is-A inclusions/exclusions, semantic tags, defining attributes,
    /// PCD refsets, and activity thresholds.
    pub fn search<'a>(&'a self, query: &SearchQuery) -> Vec<SearchResult<'a>> {
        // --- Step 1: Initial Base Candidates ---
        let mut candidates = if !query.include_texts.is_empty() {
            let prepared_patterns: Vec<String> = query
                .include_texts
                .iter()
                .map(|p| p.to_ascii_lowercase())
                .collect();

            let mut text_matched = RoaringBitmap::new();
            for (idx, term) in self.terms.iter().enumerate() {
                let term_lower = term.to_ascii_lowercase();
                if prepared_patterns.iter().all(|pat| {
                    if !pat.contains('*') && !pat.contains('?') && !pat.contains('|') {
                        term_lower.contains(pat.as_str())
                    } else if !pat.starts_with('*') && !pat.ends_with('*') && !pat.contains('|') {
                        // If no boundary wildcards provided, check substring with wrapped wildcard
                        let wrapped = format!("*{}*", pat);
                        matches_pattern(&term_lower, &wrapped)
                    } else {
                        matches_pattern(&term_lower, pat)
                    }
                }) {
                    text_matched.insert(idx as u32);
                }
            }
            text_matched
        } else {
            self.active_mask.clone()
        };

        if candidates.is_empty() {
            return Vec::new();
        }

        // --- Step 2: Apply PCD Refset Inclusions (Union: Cluster1 OR Cluster2 ...) ---
        if !query.include_refsets.is_empty() {
            let mut refset_inclusions = RoaringBitmap::new();
            for cluster_name in &query.include_refsets {
                if let Some(cluster_bitmap) = self.get_cluster_bitmap(cluster_name) {
                    refset_inclusions |= cluster_bitmap;
                }
            }
            candidates &= refset_inclusions;
            if candidates.is_empty() {
                return Vec::new();
            }
        }

        // --- Step 3: Apply Is-A Inclusions (<< root1 OR << root2 ...) ---
        if !query.include_isa.is_empty() {
            let mut isa_inclusions = RoaringBitmap::new();
            for &root_id in &query.include_isa {
                if let Some(root_node) = self.get_node_id(root_id) {
                    isa_inclusions |= self.get_descendants_bitmap(root_node);
                }
            }
            candidates &= isa_inclusions;
        }

        // --- Step 4: Apply Semantic Tag Inclusions ---
        if !query.include_tags.is_empty() {
            let mut tag_inclusions = RoaringBitmap::new();
            for tag in &query.include_tags {
                if let Some(mask) = self.tag_masks.get(tag) {
                    tag_inclusions |= mask;
                }
            }
            candidates &= tag_inclusions;
        }

        // --- Step 5: Apply Independent Attribute Inclusions ---
        for attr_filter in &query.include_attributes {
            let matching_concepts = self.evaluate_attribute_filter(attr_filter);
            candidates &= matching_concepts;
            if candidates.is_empty() {
                return Vec::new();
            }
        }

        // --- Step 6: Apply Grouped Attribute Inclusions ---
        if !query.include_grouped_attributes.is_empty() {
            let mut surviving = RoaringBitmap::new();
            for node in &candidates {
                if query.include_grouped_attributes.iter().all(|grp| self.matches_grouped_attributes(node, grp)) {
                    surviving.insert(node);
                }
            }
            candidates = surviving;
        }

        // --- Step 7: Apply Is-A Exclusions (MINUS << root ...) ---
        for &exclude_id in &query.exclude_isa {
            if let Some(exclude_node) = self.get_node_id(exclude_id) {
                let exclude_branch = self.get_descendants_bitmap(exclude_node);
                candidates -= exclude_branch;
            }
        }

        // --- Step 8: Apply Semantic Tag Exclusions ---
        for tag in &query.exclude_tags {
            if let Some(mask) = self.tag_masks.get(tag) {
                candidates -= mask;
            }
        }

        // --- Step 9: Apply Attribute Exclusions ---
        for attr_filter in &query.exclude_attributes {
            let exclude_concepts = self.evaluate_attribute_filter(attr_filter);
            candidates -= exclude_concepts;
        }

        // --- Step 10: Apply PCD Refset Exclusions ---
        for cluster_name in &query.exclude_refsets {
            if let Some(cluster_bitmap) = self.get_cluster_bitmap(cluster_name) {
                candidates -= cluster_bitmap;
            }
        }

        // --- Step 11: Apply Negative Text Exclusions ---
        if !query.exclude_texts.is_empty() {
            let prepared_excludes: Vec<String> = query
                .exclude_texts
                .iter()
                .map(|p| p.to_ascii_lowercase())
                .collect();

            let mut to_remove = RoaringBitmap::new();
            for node in &candidates {
                if let Some(term) = self.get_term(node) {
                    let term_lower = term.to_ascii_lowercase();
                    if prepared_excludes.iter().any(|pat| {
                        if !pat.contains('*') && !pat.contains('?') && !pat.contains('|') {
                            term_lower.contains(pat.as_str())
                        } else if !pat.starts_with('*') && !pat.ends_with('*') && !pat.contains('|') {
                            let wrapped = format!("*{}*", pat);
                            matches_pattern(&term_lower, &wrapped)
                        } else {
                            matches_pattern(&term_lower, pat)
                        }
                    }) {
                        to_remove.insert(node);
                    }
                }
            }
            candidates -= to_remove;
        }

        // --- Step 12: Apply Minimum Usage Threshold ---
        if let Some(min_u) = query.min_usage {
            if let Some(ref usage_arr) = self.usage {
                let mut above_min = RoaringBitmap::new();
                for node in &candidates {
                    let u = usage_arr.get(node as usize).copied().unwrap_or(0);
                    if u >= min_u {
                        above_min.insert(node);
                    }
                }
                candidates = above_min;
            }
        }

        // --- Step 11: Collect Candidate NodeIds ---
        let mut matched_nodes: Vec<NodeId> = candidates.into_iter().collect();

        // Sort by real-world usage descending if requested
        if query.sort_by_usage {
            if let Some(ref usage_arr) = self.usage {
                matched_nodes.sort_unstable_by(|&a, &b| {
                    let u_a = usage_arr.get(a as usize).copied().unwrap_or(0);
                    let u_b = usage_arr.get(b as usize).copied().unwrap_or(0);
                    u_b.cmp(&u_a) // Descending
                });
            }
        }

        // --- Step 12: Build Structured Results up to limit ---
        let mut results = Vec::with_capacity(query.limit.min(matched_nodes.len()));
        for node in matched_nodes.into_iter().take(query.limit) {
            if let (Some(sctid), Some(term)) = (self.get_concept_id(node), self.get_term(node)) {
                let tag = self.get_tag(node).unwrap_or(SemanticTag::Other);
                let is_active = self.is_active(node);
                let usage = self.get_usage(node);
                let pcd_clusters = self.get_pcd_clusters(node);

                results.push(SearchResult {
                    sctid,
                    term,
                    tag,
                    is_active,
                    usage,
                    pcd_clusters,
                });
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_graph() -> SnomedGraph {
        let sctid_to_node = HashMap::from([
            (10, 0),        // Clinical finding
            (100, 1),       // Asthma
            (101, 2),       // Childhood asthma
            (200, 3),       // Bronchitis
            (20, 4),        // Procedure root
            (300, 5),       // Asthma review
            (500, 6),       // Lung
            (501, 7),       // Bronchus
            (363698007, 8), // Finding site attribute
        ]);

        let node_to_sctid = vec![10, 100, 101, 200, 20, 300, 500, 501, 363698007];

        let terms = vec![
            "Clinical finding (finding)".to_string(),
            "Asthma (disorder)".to_string(),
            "Childhood asthma (disorder)".to_string(),
            "Bronchitis (disorder)".to_string(),
            "Procedure (procedure)".to_string(),
            "Asthma review (procedure)".to_string(),
            "Lung structure (body structure)".to_string(),
            "Bronchial structure (body structure)".to_string(),
            "Finding site (attribute)".to_string(),
        ];

        let tags: Vec<SemanticTag> = terms.iter().map(|t| SemanticTag::from_fsn(t)).collect();
        let mut tag_masks = HashMap::new();
        for (idx, &tag) in tags.iter().enumerate() {
            tag_masks.entry(tag).or_insert_with(RoaringBitmap::new).insert(idx as u32);
        }

        let mut active_mask = RoaringBitmap::new();
        for i in 0..node_to_sctid.len() as u32 {
            active_mask.insert(i);
        }

        let parent_offsets = vec![0, 0, 1, 2, 3, 3, 4, 4, 5, 5];
        let parent_nodes = vec![0, 1, 0, 4, 6];

        let child_offsets = vec![0, 2, 3, 3, 3, 4, 4, 5, 5, 5];
        let child_nodes = vec![1, 3, 2, 5, 7];

        let attribute_offsets = vec![0, 0, 1, 2, 2, 2, 2, 2, 2, 2];
        let attributes = vec![
            Attribute { group: 0, type_id: 8, value_id: 6 },
            Attribute { group: 0, type_id: 8, value_id: 7 },
        ];

        let mut reverse_attributes = HashMap::new();
        reverse_attributes.insert((8, 6), vec![1]);
        reverse_attributes.insert((8, 7), vec![2]);

        // Embed usage data: Asthma (100) has 5000 uses, Childhood asthma (101) has 50 uses
        let usage = Some(vec![10000, 5000, 50, 1200, 8000, 2500, 0, 0, 0]);

        // Embed PCD cluster: AST_COD has Node 1 (Asthma) and Node 2 (Childhood asthma)
        let mut pcd_clusters = HashMap::new();
        let mut ast_bitmap = RoaringBitmap::new();
        ast_bitmap.insert(1);
        ast_bitmap.insert(2);
        pcd_clusters.insert("AST_COD".to_string(), ast_bitmap);

        let mut node_to_pcd: Vec<SmallVec<[String; 2]>> = vec![SmallVec::new(); 9];
        node_to_pcd[1].push("AST_COD".to_string());
        node_to_pcd[2].push("AST_COD".to_string());

        SnomedGraph {
            metadata: ReleaseMetadata::default(),
            active_mask,
            terms,
            tags,
            tag_masks,
            sctid_to_node,
            node_to_sctid,
            parent_offsets,
            parent_nodes,
            child_offsets,
            child_nodes,
            attribute_offsets,
            attributes,
            reverse_attributes,
            usage,
            pcd_clusters: Some(pcd_clusters),
            node_to_pcd: Some(node_to_pcd),
        }
    }

    #[test]
    fn test_search_pure_text() {
        let graph = build_test_graph();
        let query = SearchQuery::new().with_text("asthma");
        let results = graph.search(&query);

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_text_and_tag_filter() {
        let graph = build_test_graph();

        let query = SearchQuery::new()
            .with_text("asthma")
            .include_tag(SemanticTag::Disorder);
        let results = graph.search(&query);

        assert_eq!(results.len(), 2);
        let ids: Vec<ConceptId> = results.iter().map(|r| r.sctid).collect();
        assert!(ids.contains(&100)); // Asthma
        assert!(ids.contains(&101)); // Childhood asthma
        assert!(!ids.contains(&300)); // Dropped Asthma review
    }

    #[test]
    fn test_search_hierarchy_exclusion() {
        let graph = build_test_graph();

        let query = SearchQuery::new()
            .include_isa(10)
            .exclude_isa(101);
        let results = graph.search(&query);

        let ids: Vec<ConceptId> = results.iter().map(|r| r.sctid).collect();
        assert!(ids.contains(&10));  // Root
        assert!(ids.contains(&100)); // Asthma
        assert!(ids.contains(&200)); // Bronchitis
        assert!(!ids.contains(&101)); // Excluded Childhood asthma
    }

    #[test]
    fn test_search_attribute_refinement() {
        let graph = build_test_graph();

        let query = SearchQuery::new()
            .include_attribute(363698007, 500, true);
        let results = graph.search(&query);

        assert_eq!(results.len(), 2);
        let ids: Vec<ConceptId> = results.iter().map(|r| r.sctid).collect();
        assert!(ids.contains(&100));
        assert!(ids.contains(&101));
    }

    #[test]
    fn test_search_with_usage_and_min_threshold() {
        let graph = build_test_graph();

        // Search "asthma" with min-usage = 100
        // Asthma has 5000 uses -> matches!
        // Childhood asthma has 50 uses -> excluded!
        let query = SearchQuery::new()
            .with_text("asthma")
            .include_tag(SemanticTag::Disorder)
            .min_usage(100);
        let results = graph.search(&query);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sctid, 100);
        assert_eq!(results[0].usage, Some(5000));
    }

    #[test]
    fn test_search_member_of_pcd_refset() {
        let graph = build_test_graph();

        // Restrict search to AST_COD members
        let query = SearchQuery::new().member_of_refset("AST_COD");
        let results = graph.search(&query);

        assert_eq!(results.len(), 2);
        let ids: Vec<ConceptId> = results.iter().map(|r| r.sctid).collect();
        assert!(ids.contains(&100));
        assert!(ids.contains(&101));
    }

    #[test]
    fn test_binary_serialization_roundtrip() {
        let graph = build_test_graph();
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_snomed_graph.bin");

        graph.save_to_file(&path).expect("Failed to save graph");
        let restored = SnomedGraph::load_from_file(&path).expect("Failed to load graph");
        let _ = std::fs::remove_file(&path);

        assert_eq!(restored.concept_count(), graph.concept_count());
        assert_eq!(restored.active_concept_count(), graph.active_concept_count());
        assert_eq!(restored.get_parents(1), graph.get_parents(1));
        assert_eq!(restored.get_term(1), graph.get_term(1));
        assert_eq!(restored.has_usage(), true);
        assert_eq!(restored.has_pcd_refsets(), true);
        assert_eq!(restored.get_usage(1), Some(5000));
    }
}
