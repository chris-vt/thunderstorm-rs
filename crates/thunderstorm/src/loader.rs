use crate::tag::SemanticTag;
use crate::{Attribute, ConceptId, NodeId, ReleaseMetadata, SnomedGraph};
use hashbrown::{HashMap, HashSet};
use roaring::RoaringBitmap;
use smallvec::SmallVec;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const IS_A_RELATIONSHIP: u64 = 116680003;
const FSN_DESCRIPTION_TYPE: u64 = 900000000000003001;

/// Options for loading and compiling a SNOMED CT release snapshot.
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    /// If true, includes inactive concepts (marked with active_mask = false).
    /// Default: true.
    pub include_inactive: bool,
    /// Edition name for metadata. Default: "UK Clinical Edition".
    pub edition: String,
    /// Optional path to NHS England code usage text file (e.g. SNOMED_code_usage_2024-25.txt).
    pub usage_path: Option<PathBuf>,
    /// Optional path to PCD refsets file or archive (e.g. PCD_Refset_Content_text_files.zip).
    pub refsets_path: Option<PathBuf>,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            include_inactive: true,
            edition: "UK Clinical Edition".to_string(),
            usage_path: None,
            refsets_path: None,
        }
    }
}

impl SnomedGraph {
    /// Loads a complete SNOMED CT graph from an official RF2 release ZIP archive.
    pub fn from_rf2_zip<P: AsRef<Path>>(zip_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_rf2_zip_with_options(zip_path, LoaderOptions::default())
    }

    /// Loads a complete SNOMED CT graph with optional extensions (PCD refsets and code usage).
    pub fn from_rf2_zip_with_options<P: AsRef<Path>>(
        zip_path: P,
        options: LoaderOptions,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(zip_path)?;
        let mut archive = ZipArchive::new(BufReader::new(file))?;

        // Discover Snapshot entry names inside the ZIP
        let mut concept_files = Vec::new();
        let mut desc_files = Vec::new();
        let mut rel_files = Vec::new();

        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            if name.contains("/Snapshot/Terminology/sct2_Concept_") && name.ends_with(".txt") {
                concept_files.push(name);
            } else if name.contains("/Snapshot/Terminology/sct2_Description_") && name.ends_with(".txt") {
                desc_files.push(name);
            } else if name.contains("/Snapshot/Terminology/sct2_Relationship_")
                && !name.contains("RelationshipConcreteValues")
                && name.ends_with(".txt")
            {
                rel_files.push(name);
            }
        }

        if concept_files.is_empty() || rel_files.is_empty() {
            return Err("ZIP archive does not contain required RF2 Concept and Relationship snapshot files.".into());
        }

        // ==========================================
        // Stage 1: Load Concepts & Status
        // ==========================================
        let mut all_concepts = HashSet::new();
        let mut active_concepts = HashSet::new();
        let mut detected_release_date = String::new();

        for file_name in &concept_files {
            let zip_entry = archive.by_name(file_name)?;
            let reader = BufReader::new(zip_entry);

            for (line_idx, line_res) in reader.lines().enumerate() {
                if line_idx == 0 {
                    continue;
                }
                let line = line_res?;
                let mut parts = line.split('\t');

                let id_str = match parts.next() {
                    Some(s) => s.trim(),
                    None => continue,
                };
                let effective_time = parts.next().unwrap_or("").trim();
                let active_str = parts.next().unwrap_or("").trim();

                if let Ok(id) = id_str.parse::<ConceptId>() {
                    let is_active = active_str == "1";
                    if is_active {
                        active_concepts.insert(id);
                        all_concepts.insert(id);
                    } else if options.include_inactive {
                        all_concepts.insert(id);
                    }

                    if detected_release_date.is_empty() && effective_time.len() == 8 {
                        detected_release_date = format!(
                            "{}-{}-{}",
                            &effective_time[0..4],
                            &effective_time[4..6],
                            &effective_time[6..8]
                        );
                    }
                }
            }
        }

        let mut sorted_concepts: Vec<ConceptId> = all_concepts.into_iter().collect();
        sorted_concepts.sort_unstable();

        let n = sorted_concepts.len();
        let mut sctid_to_node = HashMap::with_capacity(n);
        let mut active_mask = RoaringBitmap::new();

        for (idx, &sctid) in sorted_concepts.iter().enumerate() {
            let node = idx as NodeId;
            sctid_to_node.insert(sctid, node);
            if active_concepts.contains(&sctid) {
                active_mask.insert(node);
            }
        }

        // ==========================================
        // Stage 2: Load Descriptions (FSNs)
        // ==========================================
        let mut node_to_term: Vec<String> = vec![String::new(); n];

        for file_name in &desc_files {
            let zip_entry = archive.by_name(file_name)?;
            let reader = BufReader::new(zip_entry);

            for (line_idx, line_res) in reader.lines().enumerate() {
                if line_idx == 0 {
                    continue;
                }
                let line = line_res?;
                let mut parts = line.split('\t');

                let _id = parts.next();
                let _eff_time = parts.next();
                let active_str = parts.next().unwrap_or("").trim();
                let _module_id = parts.next();
                let concept_id_str = match parts.next() {
                    Some(s) => s.trim(),
                    None => continue,
                };
                let _lang = parts.next();
                let type_id_str = parts.next().unwrap_or("").trim();
                let term = parts.next().unwrap_or("").trim();

                if let (Ok(concept_id), Ok(type_id)) = (
                    concept_id_str.parse::<ConceptId>(),
                    type_id_str.parse::<u64>(),
                ) {
                    if let Some(&node) = sctid_to_node.get(&concept_id) {
                        let is_fsn = type_id == FSN_DESCRIPTION_TYPE;
                        let is_active = active_str == "1";

                        if (is_fsn && is_active) || node_to_term[node as usize].is_empty() {
                            node_to_term[node as usize] = term.to_string();
                        }
                    }
                }
            }
        }

        for (idx, term) in node_to_term.iter_mut().enumerate() {
            if term.is_empty() {
                *term = format!("Concept {}", sorted_concepts[idx]);
            }
        }

        let mut tags = Vec::with_capacity(n);
        let mut tag_masks = HashMap::new();

        for (idx, term) in node_to_term.iter().enumerate() {
            let tag = SemanticTag::from_fsn(term);
            tags.push(tag);
            tag_masks
                .entry(tag)
                .or_insert_with(RoaringBitmap::new)
                .insert(idx as u32);
        }

        // ==========================================
        // Stage 3: Load Relationships
        // ==========================================
        let mut temp_parents: Vec<SmallVec<[NodeId; 2]>> = vec![SmallVec::new(); n];
        let mut temp_children: Vec<Vec<NodeId>> = vec![Vec::new(); n];
        let mut temp_attributes: Vec<Vec<Attribute>> = vec![Vec::new(); n];
        let mut reverse_attributes: HashMap<(NodeId, NodeId), Vec<NodeId>> = HashMap::new();

        let mut total_isa = 0;
        let mut total_attrs = 0;

        for file_name in &rel_files {
            let zip_entry = archive.by_name(file_name)?;
            let reader = BufReader::new(zip_entry);

            for (line_idx, line_res) in reader.lines().enumerate() {
                if line_idx == 0 {
                    continue;
                }
                let line = line_res?;
                let mut parts = line.split('\t');

                let _id = parts.next();
                let _eff_time = parts.next();
                let active_str = parts.next().unwrap_or("").trim();
                let _module_id = parts.next();
                let source_str = match parts.next() {
                    Some(s) => s.trim(),
                    None => continue,
                };
                let dest_str = parts.next().unwrap_or("").trim();
                let group_str = parts.next().unwrap_or("0").trim();
                let type_str = parts.next().unwrap_or("").trim();

                if active_str != "1" {
                    continue;
                }

                if let (Ok(source_id), Ok(dest_id), Ok(type_id)) = (
                    source_str.parse::<ConceptId>(),
                    dest_str.parse::<ConceptId>(),
                    type_str.parse::<u64>(),
                ) {
                    let group = group_str.parse::<u8>().unwrap_or(0);
                    if let (Some(&source_node), Some(&dest_node)) =
                        (sctid_to_node.get(&source_id), sctid_to_node.get(&dest_id))
                    {
                        if type_id == IS_A_RELATIONSHIP {
                            temp_parents[source_node as usize].push(dest_node);
                            temp_children[dest_node as usize].push(source_node);
                            total_isa += 1;
                        } else {
                            if let Some(&type_node) = sctid_to_node.get(&type_id) {
                                temp_attributes[source_node as usize].push(Attribute {
                                    group,
                                    type_id: type_node,
                                    value_id: dest_node,
                                });
                                reverse_attributes
                                    .entry((type_node, dest_node))
                                    .or_default()
                                    .push(source_node);
                                total_attrs += 1;
                            }
                        }
                    }
                }
            }
        }

        // ==========================================
        // Stage 4: Flatten into CSR Arrays
        // ==========================================
        let mut parent_offsets = Vec::with_capacity(n + 1);
        let mut parent_nodes = Vec::with_capacity(total_isa);
        parent_offsets.push(0);

        for mut parents in temp_parents {
            parents.sort_unstable();
            parents.dedup();
            parent_nodes.extend_from_slice(&parents);
            parent_offsets.push(parent_nodes.len() as u32);
        }

        let mut child_offsets = Vec::with_capacity(n + 1);
        let mut child_nodes = Vec::with_capacity(total_isa);
        child_offsets.push(0);

        for mut children in temp_children {
            children.sort_unstable();
            children.dedup();
            child_nodes.extend_from_slice(&children);
            child_offsets.push(child_nodes.len() as u32);
        }

        let mut attribute_offsets = Vec::with_capacity(n + 1);
        let mut attributes = Vec::with_capacity(total_attrs);
        attribute_offsets.push(0);

        for mut attrs in temp_attributes {
            attrs.sort_by_key(|a| (a.group, a.type_id, a.value_id));
            attrs.dedup();
            attributes.extend_from_slice(&attrs);
            attribute_offsets.push(attributes.len() as u32);
        }

        // ==========================================
        // Stage 5: Ingest Code Usage (Optional)
        // ==========================================
        let mut usage_data: Option<Vec<u64>> = None;
        let mut usage_source: Option<String> = None;

        if let Some(ref path) = options.usage_path {
            println!("📊 Loading national code usage from {}...", path.display());
            let u_file = File::open(path)?;
            let u_reader = BufReader::new(u_file);
            let mut u_vec = vec![0u64; n];
            let mut loaded_count = 0;

            for (line_idx, line_res) in u_reader.lines().enumerate() {
                if line_idx == 0 {
                    continue;
                }
                let line = line_res?;
                let mut parts = line.split('\t');

                let id_str = match parts.next() {
                    Some(s) => s.trim(),
                    None => continue,
                };
                let _desc = parts.next();
                let usage_str = parts.next().unwrap_or("0").trim();

                if let (Ok(sctid), Ok(count)) = (id_str.parse::<ConceptId>(), usage_str.parse::<u64>()) {
                    if let Some(&node) = sctid_to_node.get(&sctid) {
                        u_vec[node as usize] = count;
                        loaded_count += 1;
                    }
                }
            }
            println!("✅ Loaded usage counts for {} concepts.", loaded_count);
            usage_data = Some(u_vec);
            usage_source = Some(path.file_name().unwrap_or_default().to_string_lossy().to_string());
        }

        // ==========================================
        // Stage 6: Ingest PCD Refsets (Optional)
        // ==========================================
        let mut pcd_clusters: Option<HashMap<String, RoaringBitmap>> = None;
        let mut node_to_pcd: Option<Vec<SmallVec<[String; 2]>>> = None;
        let mut refsets_source: Option<String> = None;
        let mut cluster_count = 0;

        if let Some(ref path) = options.refsets_path {
            println!("📚 Loading PCD refsets from {}...", path.display());
            let mut clusters_map: HashMap<String, RoaringBitmap> = HashMap::new();
            let mut node_map: Vec<SmallVec<[String; 2]>> = vec![SmallVec::new(); n];

            let process_pcd_reader = |reader: &mut dyn BufRead,
                                      clusters: &mut HashMap<String, RoaringBitmap>,
                                      node_pcd: &mut Vec<SmallVec<[String; 2]>>|
             -> Result<(), Box<dyn std::error::Error>> {
                for (line_idx, line_res) in reader.lines().enumerate() {
                    if line_idx == 0 {
                        continue;
                    }
                    let line = line_res?;
                    let mut parts = line.split('\t');

                    let cluster_id = match parts.next() {
                        Some(s) => s.trim().to_string(),
                        None => continue,
                    };
                    let _cluster_desc = parts.next();
                    let code_str = parts.next().unwrap_or("").trim();

                    if let Ok(sctid) = code_str.parse::<ConceptId>() {
                        if let Some(&node) = sctid_to_node.get(&sctid) {
                            clusters.entry(cluster_id.clone()).or_default().insert(node);
                            if !node_pcd[node as usize].contains(&cluster_id) {
                                node_pcd[node as usize].push(cluster_id);
                            }
                        }
                    }
                }
                Ok(())
            };

            let is_zip = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
            if is_zip {
                let pcd_archive_file = File::open(path)?;
                let mut pcd_zip = ZipArchive::new(BufReader::new(pcd_archive_file))?;

                // Find the latest PCD refset content file
                let mut target_entry: Option<String> = None;
                for i in 0..pcd_zip.len() {
                    let entry = pcd_zip.by_index(i)?;
                    let name = entry.name().to_string();
                    if name.contains("PCD_Refset_Content")
                        && !name.contains("by_Output")
                        && name.ends_with(".txt")
                        && (target_entry.is_none() || name.contains("_v3") || name.contains("_v2"))
                    {
                        target_entry = Some(name);
                    }
                }

                if let Some(entry_name) = target_entry {
                    println!("   Found refset content: {}", entry_name);
                    let entry = pcd_zip.by_name(&entry_name)?;
                    let mut rdr = BufReader::new(entry);
                    process_pcd_reader(&mut rdr, &mut clusters_map, &mut node_map)?;
                } else {
                    eprintln!("⚠️ Warning: Could not locate PCD_Refset_Content text file inside ZIP.");
                }
            } else {
                let pcd_file = File::open(path)?;
                let mut rdr = BufReader::new(pcd_file);
                process_pcd_reader(&mut rdr, &mut clusters_map, &mut node_map)?;
            }

            cluster_count = clusters_map.len();
            println!("✅ Loaded {} PCD refset clusters.", cluster_count);
            pcd_clusters = Some(clusters_map);
            node_to_pcd = Some(node_map);
            refsets_source = Some(path.file_name().unwrap_or_default().to_string_lossy().to_string());
        }

        // Compile Release Metadata
        let metadata = ReleaseMetadata {
            release_date: if detected_release_date.is_empty() {
                "Unknown".to_string()
            } else {
                detected_release_date
            },
            edition: options.edition,
            built_at: chrono_like_now(),
            total_concepts: n,
            active_concepts: active_mask.len() as usize,
            total_isa_relationships: parent_nodes.len(),
            total_attributes: attributes.len(),
            usage_source,
            refsets_source,
            refset_cluster_count: cluster_count,
        };

        Ok(SnomedGraph {
            metadata,
            active_mask,
            terms: node_to_term,
            tags,
            tag_masks,
            sctid_to_node,
            node_to_sctid: sorted_concepts,
            parent_offsets,
            parent_nodes,
            child_offsets,
            child_nodes,
            attribute_offsets,
            attributes,
            reverse_attributes,
            usage: usage_data,
            pcd_clusters,
            node_to_pcd,
        })
    }
}

/// Simple UTC timestamp helper.
fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
