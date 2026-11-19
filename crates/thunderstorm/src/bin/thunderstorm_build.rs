use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;
use thunderstorm::loader::LoaderOptions;
use thunderstorm::SnomedGraph;

/// Default global graph filename in user's home directory: ~/.thunderstorm/thunderstorm_graph.bin
fn get_default_graph_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("THUNDERSTORM_GRAPH_PATH") {
        return PathBuf::from(env_path);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".thunderstorm").join("thunderstorm_graph.bin");
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        return PathBuf::from(userprofile).join(".thunderstorm").join("thunderstorm_graph.bin");
    }
    PathBuf::from("thunderstorm_graph.bin")
}

#[derive(Parser)]
#[command(
    name = "thunderstorm-build",
    about = "Thunderstorm SNOMED CT In-Memory Graph Builder & Metadata Inspector",
    long_about = "Compiles official SNOMED CT RF2 release archives into a high-performance in-memory\n\
                  CSR graph snapshot with optional national NHS England code usage statistics\n\
                  and Primary Care Domain (PCD) reference sets."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest an official RF2 release ZIP file and compile it to a binary graph (.bin)
    Build {
        /// Path to the official release ZIP file (e.g. uk_sct2cl_...zip)
        #[arg(short, long, value_name = "ZIP_PATH")]
        zip: PathBuf,

        /// Destination path for the compiled binary snapshot (default: ./thunderstorm_graph.bin or env THUNDERSTORM_GRAPH_PATH)
        #[arg(
            short,
            long,
            default_value = "thunderstorm_graph.bin",
            env = "THUNDERSTORM_GRAPH_PATH",
            value_name = "OUTPUT_PATH"
        )]
        output: PathBuf,

        /// Automatically install the compiled graph to the user's default global directory (~/.thunderstorm/thunderstorm_graph.bin)
        #[arg(long = "install")]
        install: bool,

        /// Optional path to NHS England code usage text file (e.g. SNOMED_code_usage_2024-25.txt)
        #[arg(long = "with-usage", value_name = "USAGE_TXT")]
        with_usage: Option<PathBuf>,

        /// Optional path to PCD refsets ZIP or text file (e.g. PCD_Refset_Content_text_files.zip)
        #[arg(long = "with-refsets", value_name = "REFSETS_PATH")]
        with_refsets: Option<PathBuf>,

        /// Include inactive concepts marked with active_mask = false
        #[arg(long, default_value_t = true)]
        include_inactive: bool,

        /// Clinical edition label
        #[arg(long, default_value = "UK Clinical Edition")]
        edition: String,
    },

    /// Inspect release metadata and statistics of a compiled binary graph (.bin)
    Info {
        /// Path to the compiled binary snapshot (.bin)
        #[arg(
            short,
            long,
            default_value = "thunderstorm_graph.bin",
            env = "THUNDERSTORM_GRAPH_PATH",
            value_name = "GRAPH_PATH"
        )]
        graph: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            zip,
            output,
            install,
            with_usage,
            with_refsets,
            include_inactive,
            edition,
        } => {
            let destination = if install {
                let default_dir = get_default_graph_path();
                if let Some(parent) = default_dir.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                default_dir
            } else {
                output
            };

            println!("╔════════════════════════════════════════════════════════════╗");
            println!("║              SNOMED CT In-Memory Graph Builder             ║");
            println!("╚════════════════════════════════════════════════════════════╝");
            println!("📦 Source ZIP:       {}", zip.display());
            println!("💾 Destination:      {}", destination.display());
            if install {
                println!("🌐 Global Install:   ✅ Automatically saving to user directory");
            }
            println!("⚙️  Include Inactive:   {}", include_inactive);
            println!("🏷️  Edition:           {}", edition);

            if let Some(ref u) = with_usage {
                println!("📊 Code Usage:       {}", u.display());
            }
            if let Some(ref r) = with_refsets {
                println!("📚 PCD Refsets:      {}", r.display());
            }

            println!("\nStreaming and parsing RF2 snapshot files...");
            let start = Instant::now();
            let options = LoaderOptions {
                include_inactive,
                edition,
                usage_path: with_usage,
                refsets_path: with_refsets,
            };

            let graph = SnomedGraph::from_rf2_zip_with_options(&zip, options)?;
            let parse_elapsed = start.elapsed();
            println!("✅ Ingestion complete in {:.2?}", parse_elapsed);

            println!("\nSaving binary snapshot to {}...", destination.display());
            let save_start = Instant::now();
            graph.save_to_file(&destination)?;
            let save_elapsed = save_start.elapsed();

            let file_size_mb = std::fs::metadata(&destination)
                .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                .unwrap_or(0.0);

            println!("✅ Snapshot saved in {:.2?} ({:.1} MB)\n", save_elapsed, file_size_mb);
            print_metadata_summary(&graph);
        }

        Commands::Info { graph } => {
            println!("Loading metadata from {}...", graph.display());
            let start = Instant::now();
            let g = SnomedGraph::load_from_file(&graph)?;
            println!("✅ Graph loaded in {:.2?}\n", start.elapsed());
            print_metadata_summary(&g);
        }
    }

    Ok(())
}

fn print_metadata_summary(graph: &SnomedGraph) {
    let m = &graph.metadata;
    println!("════════════════════════════════════════════════════════════");
    println!("  SNOMED CT Release:    {}", m.edition);
    println!("  Release Date:         {}", m.release_date);
    println!("  Total Concepts:       {} (Active: {}, Inactive: {})", 
        m.total_concepts, 
        m.active_concepts, 
        m.total_concepts.saturating_sub(m.active_concepts)
    );
    println!("  Is-A Hierarchy Edges: {}", m.total_isa_relationships);
    println!("  Defining Attributes:  {}", m.total_attributes);

    if let Some(ref u) = m.usage_source {
        println!("  National Code Usage:  ✅ Embedded ({})", u);
    } else {
        println!("  National Code Usage:  ❌ Not embedded");
    }

    if let Some(ref r) = m.refsets_source {
        println!("  PCD Refsets:          ✅ Embedded ({} clusters from {})", m.refset_cluster_count, r);
    } else {
        println!("  PCD Refsets:          ❌ Not embedded");
    }
    println!("════════════════════════════════════════════════════════════");
}
