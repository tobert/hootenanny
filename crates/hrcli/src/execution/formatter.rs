use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;
use crate::discovery::schema::{DynamicToolSchema, OutputFormat, ColumnDef, Alignment};
use handlebars::Handlebars;
use std::collections::HashMap;

pub struct DynamicFormatter {
    schema: DynamicToolSchema,
    no_color: bool,
}

impl DynamicFormatter {
    pub fn new(schema: DynamicToolSchema, no_color: bool) -> Self {
        Self { schema, no_color }
    }

    /// Format the response based on server-provided format hints
    pub fn format(&self, response: Value) -> Result<String> {
        match &self.schema.output_format {
            OutputFormat::Plain => self.format_plain(response),
            OutputFormat::Json { pretty } => self.format_json(response, *pretty),
            OutputFormat::Table { columns } => self.format_table(response, columns),
            OutputFormat::Template { template, colors } => {
                self.format_template(response, template, colors)
            }
            OutputFormat::Custom { formatter, config } => {
                self.format_custom(response, formatter, config)
            }
        }
    }

    fn format_plain(&self, response: Value) -> Result<String> {
        // Extract text content
        if let Some(text) = response.as_str() {
            Ok(text.to_string())
        } else if let Some(content) = response.get("content") {
            if let Some(text) = content.as_str() {
                Ok(text.to_string())
            } else {
                Ok(content.to_string())
            }
        } else {
            // Fallback to string representation
            Ok(response.to_string())
        }
    }

    fn format_json(&self, response: Value, pretty: bool) -> Result<String> {
        if pretty {
            serde_json::to_string_pretty(&response)
                .context("Failed to format JSON")
        } else {
            serde_json::to_string(&response)
                .context("Failed to format JSON")
        }
    }

    fn format_table(&self, response: Value, columns: &[ColumnDef]) -> Result<String> {
        use prettytable::{Table, Row, Cell, format};

        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_CLEAN);

        // Add header
        let header: Vec<Cell> = columns.iter()
            .map(|col| {
                if self.no_color {
                    Cell::new(&col.header)
                } else {
                    Cell::new(&col.header).style_spec("Fb")
                }
            })
            .collect();
        table.add_row(Row::new(header));

        // Add data rows
        if let Some(rows) = response.as_array() {
            for row_data in rows {
                let cells: Vec<Cell> = columns.iter()
                    .map(|col| {
                        let value = row_data.get(&col.key)
                            .map(|v| self.value_to_string(v))
                            .unwrap_or_default();

                        let mut cell = Cell::new(&value);

                        // Apply alignment
                        match col.align {
                            Alignment::Left => cell = cell.style_spec("l"),
                            Alignment::Right => cell = cell.style_spec("r"),
                            Alignment::Center => cell = cell.style_spec("c"),
                        }

                        cell
                    })
                    .collect();
                table.add_row(Row::new(cells));
            }
        } else if let Some(obj) = response.as_object() {
            // Single row
            let cells: Vec<Cell> = columns.iter()
                .map(|col| {
                    let value = obj.get(&col.key)
                        .map(|v| self.value_to_string(v))
                        .unwrap_or_default();
                    Cell::new(&value)
                })
                .collect();
            table.add_row(Row::new(cells));
        }

        let mut output = Vec::new();
        table.print(&mut output)?;
        String::from_utf8(output).context("Failed to convert table to string")
    }

    fn format_template(&self, response: Value, template: &str, colors: &HashMap<String, String>) -> Result<String> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(false);

        // Register helpers for common formatting needs
        handlebars.register_helper("icon", Box::new(icon_helper));
        handlebars.register_helper("color", Box::new(color_helper));
        handlebars.register_helper("time", Box::new(time_helper));

        // Apply colors to the data if not disabled
        let data = if self.no_color {
            response
        } else {
            self.apply_colors(response, colors)?
        };

        handlebars.render_template(template, &data)
            .context("Failed to render template")
    }

    fn format_custom(&self, response: Value, formatter: &str, config: &Value) -> Result<String> {
        // Delegate to custom formatter plugins
        match formatter {
            "musical_score" => self.format_musical_score(response, config),
            "tree" => self.format_tree(response, config),
            "diff" => self.format_diff(response, config),
            "graph" => self.format_graph(response, config),
            _ => {
                // Unknown formatter, fallback to JSON
                self.format_json(response, true)
            }
        }
    }

    fn format_musical_score(&self, response: Value, _config: &Value) -> Result<String> {
        // Format as ASCII musical notation
        let mut output = String::new();

        if let Some(notes) = response.get("notes").and_then(|n| n.as_array()) {
            output.push_str("🎼 Musical Score\n");
            output.push_str("═══════════════\n\n");

            for note in notes {
                if let Some(pitch) = note.get("pitch").and_then(|p| p.as_str()) {
                    let duration = note.get("duration").and_then(|d| d.as_f64()).unwrap_or(1.0);
                    let symbol = if duration >= 1.0 { "○" } else { "●" };

                    output.push_str(&format!("  {} {} ", symbol, pitch));

                    if !self.no_color {
                        output = format!("{}", output.bright_cyan());
                    }
                }
            }
            output.push('\n');
        }

        Ok(output)
    }

    fn format_tree(&self, response: Value, _config: &Value) -> Result<String> {
        // Format as tree structure
        let mut output = String::new();

        if let Some(root) = response.get("root") {
            self.format_tree_node(&mut output, root, "", true)?;
        }

        Ok(output)
    }

    fn format_tree_node(&self, output: &mut String, node: &Value, prefix: &str, is_last: bool) -> Result<()> {
        let connector = if is_last { "└── " } else { "├── " };
        let name = node.get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("?");

        let line = format!("{}{}{}\n", prefix, connector, name);
        if !self.no_color {
            output.push_str(&line.bright_green().to_string());
        } else {
            output.push_str(&line);
        }

        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            let child_prefix = format!("{}{}  ", prefix, if is_last { " " } else { "│" });

            for (i, child) in children.iter().enumerate() {
                let is_last_child = i == children.len() - 1;
                self.format_tree_node(output, child, &child_prefix, is_last_child)?;
            }
        }

        Ok(())
    }

    fn format_diff(&self, response: Value, _config: &Value) -> Result<String> {
        // Format as diff output
        let mut output = String::new();

        if let Some(changes) = response.get("changes").and_then(|c| c.as_array()) {
            for change in changes {
                let op = change.get("op").and_then(|o| o.as_str()).unwrap_or("?");
                let line = change.get("line").and_then(|l| l.as_str()).unwrap_or("");

                let formatted = match op {
                    "add" => {
                        if self.no_color {
                            format!("+ {}\n", line)
                        } else {
                            format!("{}\n", format!("+ {}", line).green())
                        }
                    }
                    "remove" => {
                        if self.no_color {
                            format!("- {}\n", line)
                        } else {
                            format!("{}\n", format!("- {}", line).red())
                        }
                    }
                    _ => format!("  {}\n", line)
                };

                output.push_str(&formatted);
            }
        }

        Ok(output)
    }

    fn format_graph(&self, response: Value, _config: &Value) -> Result<String> {
        // Simple ASCII graph
        let mut output = String::new();

        if let Some(data) = response.get("data").and_then(|d| d.as_array()) {
            let max_value = data.iter()
                .filter_map(|v| v.as_f64())
                .fold(0.0f64, f64::max);

            for (i, value) in data.iter().enumerate() {
                if let Some(v) = value.as_f64() {
                    let bar_length = ((v / max_value) * 50.0) as usize;
                    let bar = "█".repeat(bar_length);

                    let line = format!("{:3} │{} {:.2}\n", i, bar, v);
                    if !self.no_color {
                        output.push_str(&line.bright_blue().to_string());
                    } else {
                        output.push_str(&line);
                    }
                }
            }
        }

        Ok(output)
    }

    fn apply_colors(&self, mut data: Value, colors: &HashMap<String, String>) -> Result<Value> {
        // Apply color transformations to specific fields
        if let Some(obj) = data.as_object_mut() {
            for (field, color) in colors {
                if let Some(value) = obj.get_mut(field) {
                    if let Some(text) = value.as_str() {
                        let colored = self.colorize(text, color);
                        *value = Value::String(colored);
                    }
                }
            }
        }

        Ok(data)
    }

    fn colorize(&self, text: &str, color: &str) -> String {
        if self.no_color {
            return text.to_string();
        }

        match color {
            "red" => text.red().to_string(),
            "green" => text.green().to_string(),
            "yellow" => text.yellow().to_string(),
            "blue" => text.blue().to_string(),
            "magenta" => text.magenta().to_string(),
            "cyan" => text.cyan().to_string(),
            "white" => text.white().to_string(),
            "bright_red" => text.bright_red().to_string(),
            "bright_green" => text.bright_green().to_string(),
            "bright_yellow" => text.bright_yellow().to_string(),
            "bright_blue" => text.bright_blue().to_string(),
            _ => text.to_string(),
        }
    }

    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            _ => value.to_string(),
        }
    }
}

// Handlebars helpers
fn icon_helper(
    h: &handlebars::Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut dyn handlebars::Output
) -> handlebars::HelperResult {
    if let Some(param) = h.param(0) {
        if let Some(icon_type) = param.value().as_str() {
            let icon = match icon_type {
                "success" => "✅",
                "error" => "❌",
                "warning" => "⚠️",
                "info" => "ℹ️",
                "music" => "🎵",
                "tree" => "🌳",
                "branch" => "🔱",
                _ => "●",
            };
            out.write(icon)?;
        }
    }
    Ok(())
}

fn color_helper(
    h: &handlebars::Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut dyn handlebars::Output
) -> handlebars::HelperResult {
    if let (Some(text), Some(color)) = (h.param(0), h.param(1)) {
        if let (Some(text_str), Some(color_str)) = (text.value().as_str(), color.value().as_str()) {
            // In real implementation, apply color
            out.write(text_str)?;
        }
    }
    Ok(())
}

fn time_helper(
    h: &handlebars::Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut dyn handlebars::Output
) -> handlebars::HelperResult {
    if let Some(param) = h.param(0) {
        if let Some(timestamp) = param.value().as_i64() {
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
                .unwrap_or_default();
            out.write(&dt.format("%Y-%m-%d %H:%M:%S").to_string())?;
        }
    }
    Ok(())
}