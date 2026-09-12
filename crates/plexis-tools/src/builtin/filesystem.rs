//! Filesystem tool with strict sandbox directory traversal containment.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::error::ToolError;
use crate::traits::{Tool, ToolInvocationContext, ToolOutput};

/// Tool for reading, writing, and listing files within a sandboxed directory.
pub struct FilesystemTool;

#[derive(Deserialize)]
struct FsArguments {
    action: String,
    path: String,
    content: Option<String>,
}

#[derive(Serialize)]
struct DirEntryInfo {
    name: String,
    is_dir: bool,
    size_bytes: u64,
}

#[async_trait]
impl Tool for FilesystemTool {
    fn name(&self) -> &str {
        "filesystem"
    }

    fn description(&self) -> &str {
        "Read, write, and inspect files and directories safely inside the workspace sandbox"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read_file", "write_file", "list_dir", "file_exists"],
                    "description": "The filesystem operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Relative or absolute path within the authorized workspace"
                },
                "content": {
                    "type": "string",
                    "description": "File contents to write (required for write_file)"
                }
            },
            "required": ["action", "path"]
        })
    }

    async fn execute(&self, context: &ToolInvocationContext) -> Result<ToolOutput, ToolError> {
        let args: FsArguments = serde_json::from_value(context.arguments.clone()).map_err(|e| {
            ToolError::InvalidArguments(format!("Failed to parse filesystem args: {}", e))
        })?;

        match args.action.as_str() {
            "read_file" => {
                let safe_path = context.sandbox.resolve_safe_path(&args.path, false)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "File '{}' not found",
                        args.path
                    )));
                }

                let metadata = fs::metadata(&safe_path).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to read metadata: {}", e))
                })?;
                if metadata.len() > context.sandbox.policy.limits.max_file_size_bytes as u64 {
                    return Err(ToolError::ResourceExceeded(format!(
                        "File size {} exceeds limit of {} bytes",
                        metadata.len(),
                        context.sandbox.policy.limits.max_file_size_bytes
                    )));
                }

                let content = fs::read_to_string(&safe_path).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to read file: {}", e))
                })?;

                Ok(ToolOutput {
                    data: serde_json::json!({ "path": args.path, "content": content }),
                    stdout: Some(content),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "write_file" => {
                let content = args.content.ok_or_else(|| {
                    ToolError::InvalidArguments("'content' is required for 'write_file'".into())
                })?;

                if content.len() > context.sandbox.policy.limits.max_file_size_bytes {
                    return Err(ToolError::ResourceExceeded(format!(
                        "Content size {} exceeds limit of {} bytes",
                        content.len(),
                        context.sandbox.policy.limits.max_file_size_bytes
                    )));
                }

                let safe_path = context.sandbox.resolve_safe_path(&args.path, true)?;
                if let Some(parent) = safe_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        ToolError::ExecutionFailed(format!(
                            "Failed to create parent directory: {}",
                            e
                        ))
                    })?;
                }

                fs::write(&safe_path, &content).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to write file: {}", e))
                })?;

                Ok(ToolOutput {
                    data: serde_json::json!({
                        "path": args.path,
                        "bytes_written": content.len(),
                        "status": "written"
                    }),
                    stdout: Some(format!(
                        "Successfully wrote {} bytes to {}",
                        content.len(),
                        args.path
                    )),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "list_dir" => {
                let safe_path = context.sandbox.resolve_safe_path(&args.path, false)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "Directory '{}' not found",
                        args.path
                    )));
                }

                let read_dir = fs::read_dir(&safe_path).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to list directory: {}", e))
                })?;

                let mut entries = Vec::new();
                for entry in read_dir {
                    let entry = entry.map_err(|e| {
                        ToolError::ExecutionFailed(format!(
                            "Failed to inspect directory entry: {}",
                            e
                        ))
                    })?;
                    let meta = entry.metadata().map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to read entry metadata: {}", e))
                    })?;

                    entries.push(DirEntryInfo {
                        name: entry.file_name().to_string_lossy().to_string(),
                        is_dir: meta.is_dir(),
                        size_bytes: meta.len(),
                    });
                }

                Ok(ToolOutput {
                    data: serde_json::json!({ "path": args.path, "entries": entries }),
                    stdout: Some(format!("Found {} entries", entries.len())),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "file_exists" => {
                let safe_path = context.sandbox.resolve_safe_path(&args.path, false)?;
                let exists = safe_path.exists();
                Ok(ToolOutput {
                    data: serde_json::json!({ "path": args.path, "exists": exists }),
                    stdout: Some(exists.to_string()),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            unknown => Err(ToolError::InvalidArguments(format!(
                "Unknown filesystem action: '{}'",
                unknown
            ))),
        }
    }
}
