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
    path: Option<String>,
    paths: Option<Vec<String>>,
    content: Option<String>,
    start_line: Option<usize>,
    end_line: Option<usize>,
    numbered: Option<bool>,
    overwrite: Option<bool>,
    search: Option<String>,
    replace: Option<String>,
    patch: Option<String>,
}

#[derive(Serialize)]
struct DirEntryInfo {
    name: String,
    is_dir: bool,
    size_bytes: u64,
}

fn apply_patch_content(
    original: &str,
    patch_str: Option<&str>,
    search_opt: Option<&str>,
    replace_opt: Option<&str>,
) -> Result<String, ToolError> {
    if let Some(search) = search_opt {
        let replace = replace_opt.unwrap_or("");
        if !original.contains(search) {
            return Err(ToolError::ExecutionFailed(format!(
                "Patch target search block not found in file: '{}'",
                search
            )));
        }
        return Ok(original.replacen(search, replace, 1));
    }

    if let Some(patch) = patch_str {
        // Search/Replace block format
        if patch.contains("<<<<<<< SEARCH")
            && patch.contains("=======")
            && patch.contains(">>>>>>>")
        {
            let parts: Vec<&str> = patch.split("<<<<<<< SEARCH").collect();
            let mut result = original.to_string();
            for part in parts.iter().skip(1) {
                if let Some((search_part, rest)) = part.split_once("=======") {
                    if let Some((replace_part, _)) = rest.split_once(">>>>>>>") {
                        let search = search_part.trim_matches('\n');
                        let replace = replace_part.trim_matches('\n');
                        if !result.contains(search) {
                            return Err(ToolError::ExecutionFailed(format!(
                                "Patch search block not found in file: '{}'",
                                search
                            )));
                        }
                        result = result.replacen(search, replace, 1);
                    }
                }
            }
            return Ok(result);
        }

        // Basic unified diff hunk format
        if patch.contains("@@") {
            let mut lines: Vec<String> = original.lines().map(String::from).collect();
            for hunk in patch.split("@@").filter(|h| !h.trim().is_empty()) {
                let hunk_lines: Vec<&str> = hunk.lines().filter(|l| !l.is_empty()).collect();
                let mut removes = Vec::new();
                let mut adds = Vec::new();
                for l in &hunk_lines {
                    if let Some(stripped) = l.strip_prefix('-') {
                        removes.push(stripped);
                    } else if let Some(stripped) = l.strip_prefix('+') {
                        adds.push(stripped);
                    }
                }
                if !removes.is_empty() {
                    let remove_text = removes.join("\n");
                    let add_text = adds.join("\n");
                    let full = lines.join("\n");
                    if full.contains(&remove_text) {
                        let updated = full.replacen(&remove_text, &add_text, 1);
                        lines = updated.lines().map(String::from).collect();
                    }
                }
            }
            return Ok(lines.join("\n"));
        }

        return Err(ToolError::InvalidArguments(
            "Patch format not recognized. Use search/replace blocks or unified diff hunks.".into(),
        ));
    }

    Err(ToolError::InvalidArguments(
        "Either 'search'/'replace' or 'patch'/'content' must be provided for patch action".into(),
    ))
}

#[async_trait]
impl Tool for FilesystemTool {
    fn name(&self) -> &str {
        "filesystem"
    }

    fn description(&self) -> &str {
        "Read, write, inspect, and patch files and directories safely inside the workspace sandbox"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read_file", "write_file", "list_dir", "file_exists", "read_multiple_files", "apply_patch"],
                    "description": "The filesystem operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Relative or absolute path within the authorized workspace"
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of paths to read for multi-file inspection"
                },
                "content": {
                    "type": "string",
                    "description": "File contents to write or patch"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Starting line number (1-indexed) for partial read"
                },
                "end_line": {
                    "type": "integer",
                    "description": "Ending line number (1-indexed) for partial read"
                },
                "numbered": {
                    "type": "boolean",
                    "description": "Whether to prefix output lines with line numbers"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Whether to allow overwriting an existing file (defaults to true)"
                },
                "search": {
                    "type": "string",
                    "description": "Target string or block to replace for patch action"
                },
                "replace": {
                    "type": "string",
                    "description": "Replacement string or block for patch action"
                },
                "patch": {
                    "type": "string",
                    "description": "Patch content (diff or search/replace block)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, context: &ToolInvocationContext) -> Result<ToolOutput, ToolError> {
        context.validate_confinement()?;
        let args: FsArguments = serde_json::from_value(context.arguments.clone()).map_err(|e| {
            ToolError::InvalidArguments(format!("Failed to parse filesystem args: {}", e))
        })?;

        match args.action.as_str() {
            "read_file" | "read" => {
                let req_path = args.path.as_deref().ok_or_else(|| {
                    ToolError::InvalidArguments("'path' is required for 'read_file'".into())
                })?;
                let safe_path = context.sandbox.resolve_safe_path(req_path, false)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "File '{}' not found",
                        req_path
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

                // Slicing and line numbering
                if args.start_line.is_some()
                    || args.end_line.is_some()
                    || args.numbered == Some(true)
                {
                    let all_lines: Vec<&str> = content.lines().collect();
                    let total_lines = all_lines.len();
                    let start = args.start_line.unwrap_or(1).saturating_sub(1);
                    let end = args.end_line.unwrap_or(total_lines).min(total_lines);

                    let sliced = if start < all_lines.len() && start <= end {
                        &all_lines[start..end]
                    } else {
                        &[]
                    };

                    let formatted: Vec<String> =
                        if args.numbered == Some(true) || args.start_line.is_some() {
                            sliced
                                .iter()
                                .enumerate()
                                .map(|(idx, line)| format!("{:4}: {}", start + idx + 1, line))
                                .collect()
                        } else {
                            sliced.iter().map(|l| l.to_string()).collect()
                        };

                    let sliced_text = formatted.join("\n");
                    Ok(ToolOutput {
                        data: serde_json::json!({
                            "path": req_path,
                            "content": sliced_text,
                            "start_line": start + 1,
                            "end_line": end,
                            "total_lines": total_lines
                        }),
                        stdout: Some(sliced_text),
                        stderr: None,
                        exit_code: Some(0),
                    })
                } else {
                    Ok(ToolOutput {
                        data: serde_json::json!({ "path": req_path, "content": content }),
                        stdout: Some(content),
                        stderr: None,
                        exit_code: Some(0),
                    })
                }
            }
            "read_multiple_files" | "multi_read" => {
                let paths = args
                    .paths
                    .or_else(|| args.path.map(|p| vec![p]))
                    .ok_or_else(|| {
                        ToolError::InvalidArguments(
                            "'paths' array is required for 'read_multiple_files'".into(),
                        )
                    })?;

                let mut results = serde_json::Map::new();
                let mut output_lines = Vec::new();

                for p in &paths {
                    let safe_path = context.sandbox.resolve_safe_path(p, false)?;
                    if !safe_path.exists() {
                        results.insert(p.clone(), serde_json::json!({ "error": "not found" }));
                        output_lines.push(format!("--- {} (NOT FOUND) ---", p));
                        continue;
                    }

                    let content = fs::read_to_string(&safe_path).map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to read file '{}': {}", p, e))
                    })?;

                    results.insert(p.clone(), serde_json::json!({ "content": content }));
                    output_lines.push(format!("--- {} ---\n{}", p, content));
                }

                Ok(ToolOutput {
                    data: serde_json::json!({
                        "files": results,
                        "count": paths.len()
                    }),
                    stdout: Some(output_lines.join("\n")),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "apply_patch" | "patch" => {
                let req_path = args.path.as_deref().ok_or_else(|| {
                    ToolError::InvalidArguments("'path' is required for 'apply_patch'".into())
                })?;
                let safe_path = context.sandbox.resolve_safe_path(req_path, true)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "File '{}' not found to patch",
                        req_path
                    )));
                }

                let original = fs::read_to_string(&safe_path).map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "Failed to read target file '{}': {}",
                        req_path, e
                    ))
                })?;

                let patched = apply_patch_content(
                    &original,
                    args.patch.as_deref().or(args.content.as_deref()),
                    args.search.as_deref(),
                    args.replace.as_deref(),
                )?;

                fs::write(&safe_path, &patched).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to write patched file: {}", e))
                })?;

                Ok(ToolOutput {
                    data: serde_json::json!({
                        "path": req_path,
                        "status": "patched",
                        "bytes": patched.len()
                    }),
                    stdout: Some(format!(
                        "Successfully patched '{}' ({} bytes)",
                        req_path,
                        patched.len()
                    )),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "write_file" | "write" => {
                let req_path = args.path.as_deref().ok_or_else(|| {
                    ToolError::InvalidArguments("'path' is required for 'write_file'".into())
                })?;
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

                let safe_path = context.sandbox.resolve_safe_path(req_path, true)?;
                if args.overwrite == Some(false) && safe_path.exists() {
                    return Err(ToolError::ExecutionFailed(format!(
                        "File '{}' already exists and overwrite is false",
                        req_path
                    )));
                }

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
                        "path": req_path,
                        "bytes_written": content.len(),
                        "status": "written"
                    }),
                    stdout: Some(format!(
                        "Successfully wrote {} bytes to {}",
                        content.len(),
                        req_path
                    )),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "list_dir" => {
                let req_path = args.path.as_deref().unwrap_or(".");
                let safe_path = context.sandbox.resolve_safe_path(req_path, false)?;
                if !safe_path.exists() {
                    return Err(ToolError::NotFound(format!(
                        "Directory '{}' not found",
                        req_path
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
                    data: serde_json::json!({ "path": req_path, "entries": entries }),
                    stdout: Some(format!("Found {} entries", entries.len())),
                    stderr: None,
                    exit_code: Some(0),
                })
            }
            "file_exists" => {
                let req_path = args.path.as_deref().ok_or_else(|| {
                    ToolError::InvalidArguments("'path' is required for 'file_exists'".into())
                })?;
                let safe_path = context.sandbox.resolve_safe_path(req_path, false)?;
                let exists = safe_path.exists();
                Ok(ToolOutput {
                    data: serde_json::json!({ "path": req_path, "exists": exists }),
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
