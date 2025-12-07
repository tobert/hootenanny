//! AI-friendly error formatting for Lua runtime errors.
//!
//! Provides enhanced error messages with:
//! - Clean stack traces (hiding internal runtime lines)
//! - Common error detection and categorization
//! - Troubleshooting hints

use serde::{Deserialize, Serialize};

/// Categories of Lua errors for better troubleshooting guidance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaErrorKind {
    /// Syntax error in Lua code
    SyntaxError,
    /// Runtime error during execution
    RuntimeError,
    /// Missing main() function
    MissingMain,
    /// Calling nil value (often a typo or missing tool)
    NilCall,
    /// Type mismatch error
    TypeError,
    /// Index/key error
    IndexError,
    /// Timeout during execution
    Timeout,
    /// Unknown/other error
    Unknown,
}

/// Structured error information for AI consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaError {
    /// Error category for quick identification
    pub kind: LuaErrorKind,
    /// Primary error message
    pub message: String,
    /// Cleaned stack trace (if available)
    pub stack_trace: Option<Vec<StackFrame>>,
    /// Troubleshooting hints
    pub hints: Vec<String>,
    /// Suggested corrections (e.g., tool names for typos)
    pub suggestions: Vec<String>,
}

/// A single frame in a Lua stack trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Source file or chunk name
    pub source: String,
    /// Line number
    pub line: Option<u32>,
    /// Function name if known
    pub function: Option<String>,
}

impl LuaError {
    /// Format the error for AI consumption.
    pub fn format(&self) -> String {
        let mut output = String::new();

        // Header with error kind
        let kind_str = match self.kind {
            LuaErrorKind::SyntaxError => "Syntax Error",
            LuaErrorKind::RuntimeError => "Runtime Error",
            LuaErrorKind::MissingMain => "Missing main() Function",
            LuaErrorKind::NilCall => "Nil Value Call",
            LuaErrorKind::TypeError => "Type Error",
            LuaErrorKind::IndexError => "Index Error",
            LuaErrorKind::Timeout => "Timeout",
            LuaErrorKind::Unknown => "Error",
        };
        output.push_str(&format!("Lua {} \n\n", kind_str));

        // Main message
        output.push_str(&format!("Error: {}\n", self.message));

        // Stack trace
        if let Some(ref frames) = self.stack_trace {
            if !frames.is_empty() {
                output.push_str("\nStack trace:\n");
                for frame in frames {
                    let line_str = frame
                        .line
                        .map(|l| format!(":{}", l))
                        .unwrap_or_default();
                    let func_str = frame
                        .function
                        .as_ref()
                        .map(|f| format!(" in function '{}'", f))
                        .unwrap_or_default();
                    output.push_str(&format!("  [Lua] {}{}{}\n", frame.source, line_str, func_str));
                }
            }
        }

        // Troubleshooting hints
        if !self.hints.is_empty() {
            output.push_str("\nTroubleshooting:\n");
            for hint in &self.hints {
                output.push_str(&format!("  - {}\n", hint));
            }
        }

        // Suggestions
        if !self.suggestions.is_empty() {
            output.push_str("\nSuggestions:\n");
            for suggestion in &self.suggestions {
                output.push_str(&format!("  - {}\n", suggestion));
            }
        }

        output
    }
}

/// Parse and format an anyhow error from Lua execution.
pub fn format_lua_error(error: &anyhow::Error) -> String {
    let lua_error = parse_error(error);
    lua_error.format()
}

/// Parse an anyhow error into structured LuaError.
pub fn parse_error(error: &anyhow::Error) -> LuaError {
    let error_string = error.to_string();

    // Detect error kind
    let kind = detect_error_kind(&error_string);

    // Extract stack trace
    let stack_trace = extract_stack_trace(&error_string);

    // Generate hints based on error kind
    let hints = generate_hints(&kind, &error_string);

    // Generate suggestions (reserved for future use - e.g., parameter validation hints)
    let suggestions = Vec::new();

    // Clean up the message (remove stack trace from main message)
    let message = clean_error_message(&error_string);

    LuaError {
        kind,
        message,
        stack_trace,
        hints,
        suggestions,
    }
}

/// Detect the error kind from the error message.
fn detect_error_kind(error_string: &str) -> LuaErrorKind {
    let lower = error_string.to_lowercase();

    if lower.contains("syntax error") || lower.contains("unexpected symbol") {
        LuaErrorKind::SyntaxError
    } else if lower.contains("attempt to call a nil value") {
        LuaErrorKind::NilCall
    } else if lower.contains("global 'main' is not a function")
        || lower.contains("main is nil")
        || lower.contains("global 'main'")
    {
        LuaErrorKind::MissingMain
    } else if lower.contains("attempt to") && lower.contains("a nil value") {
        LuaErrorKind::NilCall
    } else if lower.contains("attempt to index")
        || lower.contains("attempt to get")
        || lower.contains("bad argument")
    {
        LuaErrorKind::IndexError
    } else if lower.contains("type") && (lower.contains("expected") || lower.contains("got")) {
        LuaErrorKind::TypeError
    } else if lower.contains("timeout") || lower.contains("timed out") {
        LuaErrorKind::Timeout
    } else if lower.contains("runtime error") {
        LuaErrorKind::RuntimeError
    } else {
        LuaErrorKind::Unknown
    }
}

/// Extract stack trace frames from error string.
fn extract_stack_trace(error_string: &str) -> Option<Vec<StackFrame>> {
    let mut frames = Vec::new();

    // Look for patterns like "[string "..."]:N:" or "script.lua:N:"
    for line in error_string.lines() {
        // Skip internal stdlib/runtime lines
        if line.contains("=[C]")
            || line.contains("stdlib")
            || line.contains("runtime")
            || line.contains("tool_bridge")
        {
            continue;
        }

        // Pattern: [string "chunk"]:line: message or script.lua:line:
        if let Some(frame) = parse_stack_frame(line) {
            frames.push(frame);
        }
    }

    if frames.is_empty() {
        None
    } else {
        Some(frames)
    }
}

/// Parse a single stack frame from a line.
fn parse_stack_frame(line: &str) -> Option<StackFrame> {
    // Pattern 1: [string "..."]:N: in function 'name'
    // Pattern 2: script.lua:N: in function 'name'
    // Pattern 3: [string "..."]:N:

    let line = line.trim();

    // Extract source and line number
    let (source, rest): (String, String) = if line.starts_with('[') {
        // [string "chunk"]:N:
        if let Some(bracket_end) = line.find(']') {
            let source = line[1..bracket_end].to_string();
            let rest = line[bracket_end + 1..].to_string();
            (source, rest)
        } else {
            return None;
        }
    } else if line.contains(':') {
        // script.lua:N:
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() >= 2 {
            (parts[0].to_string(), line[parts[0].len()..].to_string())
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Extract line number
    let rest = rest.trim_start_matches(':');
    let line_num: Option<u32> = rest
        .split(':')
        .next()
        .and_then(|s| s.trim().parse().ok());

    // Extract function name
    let function = if rest.contains("in function '") {
        rest.split("in function '")
            .nth(1)
            .and_then(|s| s.split('\'').next())
            .map(String::from)
    } else if rest.contains("in main chunk") {
        Some("main chunk".to_string())
    } else {
        None
    };

    Some(StackFrame {
        source,
        line: line_num,
        function,
    })
}

/// Generate troubleshooting hints based on error kind.
fn generate_hints(kind: &LuaErrorKind, _error_string: &str) -> Vec<String> {
    match kind {
        LuaErrorKind::SyntaxError => vec![
            "Check for missing 'end' statements".to_string(),
            "Verify string quotes are properly closed".to_string(),
            "Check for typos in Lua keywords".to_string(),
        ],
        LuaErrorKind::MissingMain => vec![
            "Scripts must define a main(params) function".to_string(),
            "Example: function main(params) return params end".to_string(),
        ],
        LuaErrorKind::NilCall => vec![
            "The function or method doesn't exist".to_string(),
            "Check if the tool name is spelled correctly".to_string(),
            "Verify the namespace is correct (e.g., mcp.hootenanny.tool_name)".to_string(),
        ],
        LuaErrorKind::TypeError => vec![
            "Check parameter types match what the function expects".to_string(),
            "Use type() to inspect variable types".to_string(),
        ],
        LuaErrorKind::IndexError => vec![
            "Check if the table/object exists before accessing".to_string(),
            "Verify the key/index is correct".to_string(),
        ],
        LuaErrorKind::Timeout => vec![
            "Script exceeded execution time limit".to_string(),
            "Consider breaking into smaller scripts".to_string(),
            "Check for infinite loops".to_string(),
        ],
        LuaErrorKind::RuntimeError | LuaErrorKind::Unknown => vec![
            "Check the stack trace for the error location".to_string(),
            "Verify all required parameters are provided".to_string(),
        ],
    }
}


/// Clean the error message (remove stack traces, extract core message).
fn clean_error_message(error_string: &str) -> String {
    // Take just the first meaningful line
    let mut message = error_string
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("stack traceback")
                && !trimmed.starts_with("...")
        })
        .unwrap_or(error_string)
        .to_string();

    // Remove source prefix like "[string "..."]:N: "
    if let Some(idx) = message.find("]: ") {
        if message.starts_with('[') {
            // Find the line number part
            let after_bracket = &message[idx + 2..];
            if let Some(colon_idx) = after_bracket.find(": ") {
                message = after_bracket[colon_idx + 2..].to_string();
            }
        }
    }

    message.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_nil_call() {
        let error = "attempt to call a nil value (field 'orpheus_generete')";
        assert_eq!(detect_error_kind(error), LuaErrorKind::NilCall);
    }

    #[test]
    fn test_detect_missing_main() {
        let error = "global 'main' is not a function";
        assert_eq!(detect_error_kind(error), LuaErrorKind::MissingMain);
    }

    #[test]
    fn test_parse_stack_frame() {
        let line = "[string \"chunk\"]:15: in function 'process'";
        let frame = parse_stack_frame(line).unwrap();
        assert_eq!(frame.source, "string \"chunk\"");
        assert_eq!(frame.line, Some(15));
        assert_eq!(frame.function, Some("process".to_string()));
    }
}
