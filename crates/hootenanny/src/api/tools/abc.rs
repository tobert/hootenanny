//! ABC notation MCP tools

use crate::api::responses::{AbcParseResponse, AbcTransposeResponse, AbcValidateResponse};
use crate::api::schema::{AbcParseRequest, AbcTransposeRequest, AbcValidateRequest};
use crate::api::service::EventDualityServer;
use hooteproto::{ToolError, ToolOutput, ToolResult};

impl EventDualityServer {
    /// Parse ABC notation into a structured AST
    #[tracing::instrument(name = "mcp.tool.abc_parse", skip(self, request))]
    pub async fn abc_parse(&self, request: AbcParseRequest) -> ToolResult {
        let result = abc::parse(&request.abc);

        let response = AbcParseResponse {
            success: !result.has_errors(),
            ast: serde_json::to_value(&result.value).ok(),
            errors: if result.has_errors() {
                Some(result.feedback.iter().map(|f| format!("{:?}", f)).collect())
            } else {
                None
            },
            warnings: None,
        };

        let text = if response.success {
            "ABC parsed successfully".to_string()
        } else {
            format!(
                "ABC parse failed: {} errors",
                response.errors.as_ref().map(|e| e.len()).unwrap_or(0)
            )
        };

        Ok(ToolOutput::new(text, &response))
    }

    /// Validate ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_validate", skip(self, request))]
    pub async fn abc_validate(&self, request: AbcValidateRequest) -> ToolResult {
        let result = abc::parse(&request.abc);

        let valid = !result.has_errors();

        let feedback: Vec<String> = result.feedback.iter().map(|f| format!("{:?}", f)).collect();

        let response = AbcValidateResponse {
            valid,
            errors: if !valid { feedback.clone() } else { vec![] },
            warnings: vec![],
        };

        let text = if valid {
            "ABC notation is valid".to_string()
        } else {
            format!("ABC validation failed: {} errors", response.errors.len())
        };

        Ok(ToolOutput::new(text, &response))
    }

    /// Transpose ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_transpose", skip(self, request))]
    pub async fn abc_transpose(&self, request: AbcTransposeRequest) -> ToolResult {
        let parse_result = abc::parse(&request.abc);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(ToolError::validation(
                "invalid_params",
                format!("ABC parse errors: {:?}", errors),
            ));
        }

        let semitones = if let Some(s) = request.semitones {
            s
        } else if let Some(target) = &request.target_key {
            abc::semitones_to_key(&parse_result.value.header.key, target)
                .map_err(|e| ToolError::validation("invalid_params", e))?
        } else {
            return Err(ToolError::validation(
                "invalid_params",
                "Must specify either semitones or target_key",
            ));
        };

        let response = AbcTransposeResponse {
            abc: request.abc.clone(),
            transposed_by: semitones,
            target_key: request.target_key.clone(),
        };

        Ok(ToolOutput::new(
            format!("Transposed ABC by {} semitones", semitones),
            &response,
        ))
    }
}
