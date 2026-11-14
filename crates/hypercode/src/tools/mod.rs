//! Built-in coding tools for HyperCode.

pub mod bash;
pub mod edit_file;
pub mod grep;
pub mod list_files;
pub mod read_file;
pub mod write_file;

pub use bash::BashTool;
pub use edit_file::EditFileTool;
pub use grep::GrepTool;
pub use list_files::ListFilesTool;
pub use read_file::ReadFileTool;
pub use write_file::WriteFileTool;

use crate::tool::ToolRegistry;

/// Register all default coding tools into `registry`.
pub fn register_default_tools(registry: &mut ToolRegistry) {
    registry.register(ReadFileTool);
    registry.register(WriteFileTool);
    registry.register(EditFileTool);
    registry.register(ListFilesTool);
    registry.register(GrepTool);
    registry.register(BashTool);
}
