use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;
use thunderstorm::{AttributeFilter, ConceptId, SearchQuery, SemanticTag, SnomedGraph};

#[derive(Parser)]
#[command(
    name = "thunderstorm-query",
    about = "Clinical SNOMED CT Ontology Query & Exploration Engine",
    long_about = "A high-performance in-memory SNOMED CT clinical query tool.\n\n\
                  Supports Expression Constraint Language (ECL) style searches, hierarchical\n\
                  transitive closure traversal (<< / MINUS <<), defining attribute filtering,\n\
                  semantic category bitmasking, Primary Care Domain (PCD) refsets, and real-world\n\
                  NHS England GP usage ranking."
)]
struct Cli {
    /// Path to compiled binary graph snapshot (.bin) (default: ./thunderstorm_graph.bin or ~/.thunderstorm/thunderstorm_graph.bin)
    #[arg(
        short,
        long,
        default_value = "thunderstorm_graph.bin",
        env = "THUNDERSTORM_GRAPH_PATH",
        global = true,
        value_name = "PATH"
    )]
    graph: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Search concepts using ECL-style criteria (text, hierarchy, tags, attributes, PCD refsets)
    #[command(
        long_about = "Execute multi-criteria clinical queries across the SNOMED CT UK graph.\n\n\
                      TEXTUAL SEARCH RULES:\n\
                        • Positional query: You can provide a search term directly as the first argument:\n\
                            snomed-query search asthma\n\
                        • Multiple words: Wrap multi-word phrases in quotes:\n\
                            snomed-query search \"acute exacerbation of asthma\"\n\
                        • Repeatable inclusions (AND logic):\n\
                            Repeat --include-text (or -t / --text) to require all patterns to match:\n\
                            snomed-query search --include-text \"asthma\" --include-text \"severe\"\n\
                        • Repeatable exclusions (NOT logic):\n\
                            Use --exclude-text to drop any concepts matching unwanted terms:\n\
                            snomed-query search \"asthma\" --exclude-text \"childhood\" --exclude-text \"mild\"\n\
                        • Wildcards (* and ?):\n\
                            Use '*' to match 0 or more characters, or '?' for a single character:\n\
                            snomed-query search \"decompensat*\"          (matches decompensation, decompensated)\n\
                            snomed-query search \"infarct*\"               (matches infarction, infarcts)\n\
                        • Disjunctions / OR logic (|):\n\
                            Separate terms with '|' within a pattern to match any alternative:\n\
                            snomed-query search \"asthma|wheez*\"\n\
                            snomed-query search \"heart failure\" --exclude-text \"suspected|history|review\"\n\n\
                      EXAMPLES:\n\
                        snomed-query search asthma --include-tag disorder --with-usage --limit 15\n\
                        snomed-query search --include-text \"asthma\" --include-text \"severe\" --with-usage\n\
                        snomed-query search --include-isa 84114007 --include-tag disorder --min-usage 100\n\
                        snomed-query search --include-refset CHD_COD --include-refset HF_COD --with-usage\n\
                        snomed-query search --include-attr 363698007:40768004:true --include-tag disorder"
    )]
    Search {
        /// Optional positional text search phrase (e.g. `snomed-query search asthma` or `\"acute asthma\"`).
        /// Supports wildcards (*, ?) and pipe-separated OR alternatives (e.g. \"infarct*|ischemi*\").
        #[arg(value_name = "QUERY")]
        query: Option<String>,

        /// Positive text search pattern (repeatable, all must match: AND).
        /// Supports phrases (\"asthma attack\"), wildcards (\"decompensat*\"), and pipe OR (\"asthma|wheez*\").
        #[arg(short = 't', long = "include-text", alias = "text", value_name = "PATTERN")]
        include_texts: Vec<String>,

        /// Negative text search pattern (repeatable, any match is excluded: NOT).
        /// Supports phrases (\"childhood asthma\"), wildcards (\"suspect*\"), and pipe OR (\"suspected|history|review\").
        #[arg(long = "exclude-text", value_name = "PATTERN")]
        exclude_texts: Vec<String>,

        /// Restrict search strictly to descendants of this root SCTID (repeatable, << root)
        #[arg(long = "include-isa", value_name = "SCTID")]
        include_isa: Vec<ConceptId>,

        /// Exclude descendants of this root SCTID (repeatable, MINUS << root)
        #[arg(long = "exclude-isa", value_name = "SCTID")]
        exclude_isa: Vec<ConceptId>,

        /// Semantic category tags to include (repeatable: disorder, finding, procedure, body structure, etc.)
        #[arg(long = "include-tag", value_name = "TAG")]
        include_tags: Vec<String>,

        /// Semantic category tags to exclude (repeatable: situation, procedure, regime/therapy, etc.)
        #[arg(long = "exclude-tag", value_name = "TAG")]
        exclude_tags: Vec<String>,

        /// Include concepts with defining attribute: TYPE:VALUE[:INCLUDE_DESCENDANTS]\n\
        /// e.g. 363698007:89187006:true (Finding site -> Airway structure)
        #[arg(long = "include-attr", value_name = "TYPE:VAL[:DESC]")]
        include_attrs: Vec<String>,

        /// Exclude concepts with defining attribute: TYPE:VALUE[:INCLUDE_DESCENDANTS]
        #[arg(long = "exclude-attr", value_name = "TYPE:VAL[:DESC]")]
        exclude_attrs: Vec<String>,

        /// Restrict search to members of a PCD refset cluster (repeatable for union: e.g. --include-refset CHD_COD --include-refset HF_COD)
        #[arg(long = "include-refset", alias = "member-of-refset", value_name = "CLUSTER")]
        include_refsets: Vec<String>,

        /// Exclude members belonging to these PCD refset clusters (repeatable)
        #[arg(long = "exclude-refset", value_name = "CLUSTER")]
        exclude_refsets: Vec<String>,

        /// Show real-world NHS England usage count and sort results descending by frequency
        #[arg(long = "with-usage")]
        with_usage: bool,

        /// Filter out concepts with activity lower than this minimum national usage count (e.g. 100)
        #[arg(long = "min-usage", value_name = "MIN_COUNT")]
        min_usage: Option<u64>,

        /// Maximum results to show in output table (default: 25)
        #[arg(short, long, default_value_t = 25, value_name = "N")]
        limit: usize,
    },

    /// Inspect a specific concept: parents, children, attributes, usage, and PCD refsets
    #[command(
        long_about = "Detailed inspection of a single SNOMED CT concept.\n\n\
                      Outputs concept metadata (SCTID, FSN, Tag, Active status, NHS GP usage count,\n\
                      PCD cluster memberships), immediate parents with usage, immediate children with usage,\n\
                      and all modeled defining attributes grouped by role group."
    )]
    Node {
        /// Official SNOMED Concept ID (SCTID) to inspect (e.g. 195967001)
        #[arg(value_name = "SCTID")]
        sctid: ConceptId,
    },

    /// List all embedded Primary Care Domain (PCD) refsets and their concept counts
    #[command(
        alias = "list-refsets",
        long_about = "List embedded Primary Care Domain (PCD) reference set clusters.\n\n\
                      Displays Cluster ID, Total concept count, and Active concept count.\n\
                      If --with-usage is provided, also aggregates annual NHS England GP usage counts."
    )]
    Refsets {
        /// Optional substring filter for cluster names (e.g. "AST", "HF_", "CKD")
        #[arg(short, long, value_name = "PATTERN")]
        filter: Option<String>,

        /// Sort by 'name' (alphabetical), 'count' (total codes descending), or 'usage' (annual usage descending)
        #[arg(short, long, default_value = "name", value_name = "FIELD")]
        sort_by: String,

        /// Show aggregated national GP usage volume across all codes in each cluster
        #[arg(long = "with-usage")]
        with_usage: bool,
    },
}

fn parse_attr_str(s: &str) -> Result<AttributeFilter, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid attribute format '{}'. Expected TYPE:VALUE or TYPE:VALUE:TRUE", s));
    }

    let type_id = parts[0].parse::<ConceptId>().map_err(|e| e.to_string())?;
    let value_id = parts[1].parse::<ConceptId>().map_err(|e| e.to_string())?;
    let include_descendants = if parts.len() >= 3 {
        parts[2].eq_ignore_ascii_case("true") || parts[2] == "1"
    } else {
        true
    };

    Ok(AttributeFilter::new(type_id, value_id, include_descendants))
}

fn parse_semantic_tag(tag_str: &str) -> SemanticTag {
    SemanticTag::from_fsn(&format!("({})", tag_str.trim()))
}

fn resolve_graph_path(requested: &std::path::Path) -> PathBuf {
    if requested.exists() {
        return requested.to_path_buf();
    }

    // 1. Check parent directories if default relative name
    if requested == std::path::Path::new("thunderstorm_graph.bin") || requested == std::path::Path::new("snomed_graph.bin") {
        let parent1 = PathBuf::from("../thunderstorm_graph.bin");
        if parent1.exists() {
            return parent1;
        }
        let parent2 = PathBuf::from("../../thunderstorm_graph.bin");
        if parent2.exists() {
            return parent2;
        }
        // Also check legacy snomed_graph.bin in current or parent dirs
        let legacy0 = PathBuf::from("snomed_graph.bin");
        if legacy0.exists() {
            return legacy0;
        }
        let legacy1 = PathBuf::from("../snomed_graph.bin");
        if legacy1.exists() {
            return legacy1;
        }
        let legacy2 = PathBuf::from("../../snomed_graph.bin");
        if legacy2.exists() {
            return legacy2;
        }

        // 2. Check user's home global installation directory: ~/.thunderstorm/thunderstorm_graph.bin
        if let Ok(home) = std::env::var("HOME") {
            let global_path = PathBuf::from(home).join(".thunderstorm").join("thunderstorm_graph.bin");
            if global_path.exists() {
                return global_path;
            }
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let global_path = PathBuf::from(userprofile).join(".thunderstorm").join("thunderstorm_graph.bin");
            if global_path.exists() {
                return global_path;
            }
        }
    }

    requested.to_path_buf()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let resolved_graph_path = resolve_graph_path(&cli.graph);
    if !resolved_graph_path.exists() {
        eprintln!("❌ Error: Graph snapshot file not found at '{}'", cli.graph.display());
        eprintln!("   Please specify the path using `-g <PATH>` or set THUNDERSTORM_GRAPH_PATH.");
        eprintln!("   To compile and install a global snapshot from official RF2 ZIP:");
        eprintln!("     thunderstorm-build build --zip <PATH.zip> --install");
        std::process::exit(1);
    }

    let start = Instant::now();
    let graph = SnomedGraph::load_from_file(&resolved_graph_path)?;
    let load_elapsed = start.elapsed();

    match cli.command {
        Commands::Search {
            query: positional_query,
            include_texts,
            exclude_texts,
            include_isa,
            exclude_isa,
            include_tags,
            exclude_tags,
            include_attrs,
            exclude_attrs,
            include_refsets,
            exclude_refsets,
            with_usage,
            min_usage,
            limit,
        } => {
            // Guard: Check if usage was requested but not embedded
            if (with_usage || min_usage.is_some()) && !graph.has_usage() {
                eprintln!("❌ Error: This graph was compiled without NHS code usage data.");
                eprintln!("   Rebuild the graph using `snomed-build build --with-usage <PATH>` to enable usage ranking.");
                return Ok(());
            }

            // Guard: Check if PCD refset was requested but not embedded
            if (!include_refsets.is_empty() || !exclude_refsets.is_empty()) && !graph.has_pcd_refsets() {
                eprintln!("❌ Error: This graph was compiled without PCD refsets data.");
                eprintln!("   Rebuild the graph using `snomed-build build --with-refsets <PATH>` to enable refset filtering.");
                return Ok(());
            }

            let mut query = SearchQuery::new().with_limit(limit);

            // Add positional query if specified
            if let Some(pos) = positional_query {
                query = query.include_text(pos);
            }

            // Add all positive text search patterns
            for pat in include_texts {
                query = query.include_text(pat);
            }

            // Add all negative text search patterns
            for pat in exclude_texts {
                query = query.exclude_text(pat);
            }

            for isa in include_isa {
                query = query.include_isa(isa);
            }

            for exclude in exclude_isa {
                query = query.exclude_isa(exclude);
            }

            for tag_str in include_tags {
                query = query.include_tag(parse_semantic_tag(&tag_str));
            }

            for tag_str in exclude_tags {
                query = query.exclude_tag(parse_semantic_tag(&tag_str));
            }

            for attr_str in include_attrs {
                match parse_attr_str(&attr_str) {
                    Ok(f) => query.include_attributes.push(f),
                    Err(e) => eprintln!("⚠️ Warning: {}", e),
                }
            }

            for attr_str in exclude_attrs {
                match parse_attr_str(&attr_str) {
                    Ok(f) => query.exclude_attributes.push(f),
                    Err(e) => eprintln!("⚠️ Warning: {}", e),
                }
            }

            for cluster in include_refsets {
                query = query.include_refset(cluster);
            }

            for cluster in exclude_refsets {
                query = query.exclude_refset(cluster);
            }

            if let Some(m) = min_usage {
                query = query.min_usage(m);
            }

            if with_usage {
                query = query.sort_by_usage(true);
            }

            let search_start = Instant::now();
            let results = graph.search(&query);
            let search_elapsed = search_start.elapsed();

            println!("Found {} results (search: {:.2?}, graph load: {:.2?}):\n", 
                results.len(), 
                search_elapsed, 
                load_elapsed
            );

            if with_usage || min_usage.is_some() {
                println!("{:<18} | {:<12} | {:<10} | {:<8} | Term", "SCTID", "GP Usage/Yr", "Tag", "Status");
                println!("{:-<18}-+-{:-<12}-+-{:-<10}-+-{:-<8}-+-{:-<50}", "", "", "", "", "");

                for res in results {
                    let status_str = if res.is_active { "Active" } else { "Inactive" };
                    let usage_str = res.usage.map(format_number).unwrap_or_else(|| "0".to_string());

                    println!("{:<18} | {:>12} | {:<10} | {:<8} | {}", 
                        res.sctid, 
                        usage_str, 
                        res.tag.as_str(), 
                        status_str, 
                        res.term
                    );
                }
            } else {
                println!("{:<18} | {:<10} | {:<8} | Term", "SCTID", "Tag", "Status");
                println!("{:-<18}-+-{:-<10}-+-{:-<8}-+-{:-<60}", "", "", "", "");

                for res in results {
                    let status_str = if res.is_active { "Active" } else { "Inactive" };
                    println!("{:<18} | {:<10} | {:<8} | {}", res.sctid, res.tag.as_str(), status_str, res.term);
                }
            }
        }

        Commands::Node { sctid } => {
            let node = match graph.get_node_id(sctid) {
                Some(n) => n,
                None => {
                    eprintln!("❌ Concept ID {} not found in graph.", sctid);
                    return Ok(());
                }
            };

            let term = graph.get_term(node).unwrap_or("(No description)");
            let tag = graph.get_tag(node).map(|t| t.as_str()).unwrap_or("other");
            let is_active = graph.is_active(node);
            let usage = graph.get_usage(node);
            let pcd_clusters = graph.get_pcd_clusters(node);

            println!("╔════════════════════════════════════════════════════════════╗");
            println!("║                    SNOMED Node Details                     ║");
            println!("╚════════════════════════════════════════════════════════════╝");
            println!("Concept ID:      {}", sctid);
            println!("Internal Node:   {}", node);
            println!("Term:            {}", term);
            println!("Semantic Tag:    {}", tag);
            println!("Status:          {}", if is_active { "Active" } else { "Inactive" });

            if let Some(u) = usage {
                println!("National Usage:  {} uses/yr (England primary care)", format_number(u));
            }

            if !pcd_clusters.is_empty() {
                println!("PCD Refsets:     {}", pcd_clusters.join(", "));
            }

            // Immediate Parents
            let parents = graph.get_parents(node);
            println!("\nImmediate Parents ({}):", parents.len());
            if parents.is_empty() {
                println!("  (None / Root concept)");
            } else {
                for &p_node in parents {
                    let p_id = graph.get_concept_id(p_node).unwrap_or(0);
                    let p_term = graph.get_term(p_node).unwrap_or("(Unknown)");
                    let u_str = graph.get_usage(p_node).map(|u| format!(" [{} uses]", format_number(u))).unwrap_or_default();
                    println!("  ▲ {:<18} | {}{}", p_id, p_term, u_str);
                }
            }

            // Immediate Children
            let children = graph.get_children(node);
            println!("\nImmediate Children ({}):", children.len());
            if children.is_empty() {
                println!("  (None / Leaf concept)");
            } else {
                for &c_node in children {
                    let c_id = graph.get_concept_id(c_node).unwrap_or(0);
                    let c_term = graph.get_term(c_node).unwrap_or("(Unknown)");
                    let u_str = graph.get_usage(c_node).map(|u| format!(" [{} uses]", format_number(u))).unwrap_or_default();
                    println!("  ▼ {:<18} | {}{}", c_id, c_term, u_str);
                }
            }

            // Defining Attributes
            let attrs = graph.get_attributes(node);
            println!("\nDefining Attributes ({}):", attrs.len());
            if attrs.is_empty() {
                println!("  (None)");
            } else {
                for a in attrs {
                    let type_id = graph.get_concept_id(a.type_id).unwrap_or(0);
                    let type_term = graph.get_term(a.type_id).unwrap_or("(Unknown)");
                    let val_id = graph.get_concept_id(a.value_id).unwrap_or(0);
                    let val_term = graph.get_term(a.value_id).unwrap_or("(Unknown)");
                    println!("  • [Group {}] {} ({}) -> {} ({})", a.group, type_term, type_id, val_term, val_id);
                }
            }
            println!("════════════════════════════════════════════════════════════");
        }

        Commands::Refsets {
            filter,
            sort_by,
            with_usage,
        } => {
            if !graph.has_pcd_refsets() {
                eprintln!("❌ This graph was compiled without PCD refsets.");
                eprintln!("   Rebuild with `snomed-build build --with-refsets <PATH>` to embed refsets.");
                return Ok(());
            }

            let mut clusters = graph.list_pcd_clusters_summary();

            if let Some(ref pat) = filter {
                let pat_lower = pat.to_ascii_lowercase();
                clusters.retain(|c| c.name.to_ascii_lowercase().contains(&pat_lower));
            }

            match sort_by.to_ascii_lowercase().as_str() {
                "count" | "codes" => clusters.sort_unstable_by_key(|a| std::cmp::Reverse(a.total_codes)),
                "usage" => clusters.sort_unstable_by_key(|a| std::cmp::Reverse(a.total_usage)),
                _ => clusters.sort_unstable_by(|a, b| a.name.cmp(&b.name)),
            }

            println!("Available PCD Refsets ({} clusters):\n", clusters.len());

            if with_usage && graph.has_usage() {
                println!("{:<25} | {:<12} | {:<12} | {:<16}", "Cluster ID", "Total Codes", "Active Codes", "Est. GP Usage/Yr");
                println!("{:-<25}-+-{:-<12}-+-{:-<12}-+-{:-<16}", "", "", "", "");
                for c in clusters {
                    println!(
                        "{:<25} | {:>12} | {:>12} | {:>16}",
                        c.name,
                        format_number(c.total_codes as u64),
                        format_number(c.active_codes as u64),
                        format_number(c.total_usage)
                    );
                }
            } else {
                println!("{:<25} | {:<12} | {:<12}", "Cluster ID", "Total Codes", "Active Codes");
                println!("{:-<25}-+-{:-<12}-+-{:-<12}", "", "", "");
                for c in clusters {
                    println!(
                        "{:<25} | {:>12} | {:>12}",
                        c.name,
                        format_number(c.total_codes as u64),
                        format_number(c.active_codes as u64)
                    );
                }
            }
        }
    }

    Ok(())
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (count, c) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
