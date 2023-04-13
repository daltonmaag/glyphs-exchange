use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use glyphstool::ToPlist;

pub mod to_designspace;
pub mod to_glyphs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Ufo2glyphs {
        /// Source Designspace to convert.
        #[arg(required = true)]
        designspace_path: PathBuf,

        /// The path to the Glyphs.app file to write (default: next to the input
        /// Designspace).
        output_path: Option<PathBuf>,
    },
    Glyphs2ufo {
        /// Source Glyphs.app file to convert.
        #[arg(required = true)]
        glyphs_path: PathBuf,

        /// The path to the Designspace file to write (default: next to the input
        /// Glyphs.app).
        designspace_path: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ufo2glyphs {
            designspace_path,
            output_path,
        } => {
            let context = to_glyphs::DesignspaceContext::from_path(&designspace_path);
            let glyphs_font = to_glyphs::convert_ufos_to_glyphs(&context);

            let output_path =
                output_path.unwrap_or_else(|| designspace_path.with_extension("glyphs"));
            let plist = glyphs_font.to_plist();
            fs::write(output_path, plist.to_string()).unwrap();
        }
        Commands::Glyphs2ufo {
            glyphs_path,
            designspace_path,
        } => {
            let designspace_path =
                designspace_path.unwrap_or_else(|| glyphs_path.with_extension("designspace"));
            to_designspace::command_to_designspace(&glyphs_path, &designspace_path);
        }
    }
}
