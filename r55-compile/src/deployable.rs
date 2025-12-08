use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::compile::ContractWithDeps;

pub fn generate_deployable(contract: &ContractWithDeps, output_dir: &PathBuf) -> eyre::Result<()> {
    // Calculate relative path from contract's src/ directory to output directory
    // pathdiff::diff_paths(target, base) returns the relative path from 'base' to 'target'
    // We need both paths to be absolute for pathdiff to work correctly
    let contract_src_dir = contract.path.join("src");
    
    // Canonicalize paths to ensure they're absolute and resolved (handles symlinks, etc.)
    // Note: output_dir is created in main.rs before this is called, so it should exist
    let contract_src_dir = contract_src_dir
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, std::io::Error>(contract_src_dir.clone()))?;
    let output_dir_canonical = output_dir
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, std::io::Error>(output_dir.clone()))?;
    
    let relative_path = pathdiff::diff_paths(&output_dir_canonical, &contract_src_dir)
        .ok_or_else(|| eyre::eyre!("Failed to calculate relative path from {:?} to {:?}. Paths may be on different drives or have no common ancestor.", contract_src_dir, output_dir))?;
    
    // Convert to string with forward slashes (works on all platforms for include_bytes!)
    let relative_path_str = relative_path.to_string_lossy().replace('\\', "/");

    // Generate the file content
    let mut content = String::new();

    // Add header comments + common imports
    content.push_str("//! Auto-generated based on Cargo.toml dependencies\n");
    content
        .push_str("//! This file provides Deployable implementations for contract dependencies\n");
    content.push_str("//! TODO (phase-2): rather than using `fn deploy(args: Args)`, figure out the constructor selector from the contract dependency\n\n");
    content.push_str("use alloy_core::primitives::{Address, Bytes};\n");
    content.push_str("use eth_riscv_runtime::{create::Deployable, InitInterface, ReadOnly};\n");
    content.push_str("use core::include_bytes;\n\n");

    // Add imports for each dependency (convert package name to valid Rust crate path)
    for dep in &contract.deps {
        let interface_name = format!("I{}", dep.name.ident);
        let rust_crate = dep.name.package.replace('-', "_");
        content.push_str(&format!("use {}::{};\n", rust_crate, interface_name));
    }
    content.push('\n');

    // Add bytecode constants for each dependency
    for dep in &contract.deps {
        // Convert dependency name to uppercase for the constant name
        let const_name = dep.name.ident.to_uppercase();
        content.push_str(&format!(
            "const {}_BYTECODE: &'static [u8] = include_bytes!(\"{}/{}.bin\");\n", 
            const_name, relative_path_str, dep.name.package
        ));
    }
    content.push('\n');

    // Add Deployable implementation for each dependency
    for dep in &contract.deps {
        let interface_name = format!("I{}", dep.name.ident);
        let const_name = dep.name.ident.to_uppercase();

        content.push_str(&format!("pub struct {};\n\n", dep.name.ident));
        content.push_str(&format!("impl Deployable for {} {{\n", dep.name.ident));
        content.push_str(&format!(
            "    type Interface = {}<ReadOnly>;\n\n",
            interface_name
        ));
        content.push_str("    fn __runtime() -> &'static [u8] {\n");
        content.push_str(&format!("        {}_BYTECODE\n", const_name));
        content.push_str("    }\n");
        content.push_str("}\n\n");
    }

    // Write the file
    let output_path = contract.path.join("src").join("deployable.rs");
    fs::write(&output_path, content)?;

    info!(
        "Generated {:?} for contract: {}",
        output_path, contract.name.ident
    );

    Ok(())
}
