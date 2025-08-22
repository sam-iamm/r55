mod compile;
use compile::{find_r55_contracts_in_dirs, sort_r55_contracts};

mod config;
use config::R55Config;

mod deployable;
use deployable::generate_deployable;

use std::{fs, path::Path};
use tracing::info;

fn main() -> eyre::Result<()> {
    // Initialize logging
    let tracing_sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(tracing_sub)?;

    // Load configuration
    let config = R55Config::load()?;
    
    // Determine project root
    let project_root = if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        Path::new(&manifest_dir).parent().unwrap().to_path_buf()
    } else {
        std::env::current_dir()?
    };
    
    // Setup output directory from config
    let output_dir = config.get_out_path(&project_root);
    fs::create_dir_all(&output_dir)?;
    
    info!("Using configuration:");
    info!("  Source dirs: {:?}", config.src);
    info!("  Output dir: {}", config.out);
    info!("  Library dirs: {:?}", config.libs);
    
    // Find all R55 contracts in configured directories
    let mut search_dirs = config.get_src_paths(&project_root);
    
    // Add library directories to search
    search_dirs.extend(config.get_lib_paths(&project_root));
    
    // Fallback to examples directory if no source directories exist or are empty
    if search_dirs.is_empty() {
        let examples_dir = project_root.join("examples");
        if examples_dir.exists() {
            info!("No source directories configured, falling back to examples/");
            search_dirs.push(examples_dir);
        }
    }
    
    info!("Searching for contracts in: {:?}", search_dirs);
    let contracts = find_r55_contracts_in_dirs(&search_dirs);

    // Generate deployable files for the dependencies
    if let Some(contracts_with_deps) = contracts.get(&false) {
        for c in contracts_with_deps {
            generate_deployable(c)?;
        }
    }

    // Sort contracts in the correct compilation order
    let contracts = sort_r55_contracts(contracts)?;

    info!(
        "Found {} R55 contracts (in compilation order):\n{}",
        contracts.len(),
        contracts
            .iter()
            .enumerate()
            .map(|(i, c)| format!("  {}. {}", i + 1, c))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Compile each contract
    for contract in contracts {
        info!("Compiling contract: {}", contract.name.ident);

        // Compile deployment code and save in the file
        let deploy_bytecode = contract.compile_r55()?;
        let deploy_path = output_dir.join(format!("{}.bin", contract.name.package));
        fs::write(deploy_path, deploy_bytecode)?;
    }

    Ok(())
}
