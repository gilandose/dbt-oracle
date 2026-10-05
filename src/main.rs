use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlparser::dialect::DuckDbDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Deserialize, Default, Clone)]
struct Column {
    name: String,
    data_type: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct DependsOn {
    nodes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct Node {
    name: String,
    columns: Option<HashMap<String, Column>>,
    depends_on: Option<DependsOn>,
    unique_id: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct Manifest {
    nodes: Option<HashMap<String, Node>>,
    sources: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, String>,
    manifest: DashMap<String, Manifest>,
}

fn strip_jinja(sql: &str) -> String {
    let mut result = String::new();
    let mut in_jinja = false;
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        if !in_jinja && c == '{' && chars.peek() == Some(&'{') {
            in_jinja = true;
            chars.next(); // consume second '{'
            result.push_str("jinja_macro");
            continue;
        }
        if in_jinja && c == '}' && chars.peek() == Some(&'}') {
            in_jinja = false;
            chars.next(); // consume second '}'
            continue;
        }
        if !in_jinja {
            result.push(c);
        }
    }
    result
}

impl Backend {
    async fn load_manifest(&self) {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let manifest_path = workspace_root.join("target").join("manifest.json");

        if let Ok(content) = fs::read_to_string(&manifest_path) {
            if let Ok(parsed) = serde_json::from_str::<Manifest>(&content) {
                self.manifest.insert("default".to_string(), parsed);
                self.client
                    .log_message(MessageType::INFO, "Successfully loaded dbt manifest.json")
                    .await;
                return;
            }
        }
        self.client
            .log_message(MessageType::WARNING, "No target/manifest.json found.")
            .await;
    }

    fn get_model_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(manifest) = self.manifest.get("default") {
            if let Some(nodes) = &manifest.nodes {
                for (key, node) in nodes {
                    if key.starts_with("model.") {
                        names.push(node.name.clone());
                    }
                }
            }
        }
        names
    }

    fn get_source_names(&self) -> Vec<(String, String)> {
        let mut sources = Vec::new();
        if let Some(manifest) = self.manifest.get("default") {
            if let Some(source_nodes) = &manifest.sources {
                for (_, val) in source_nodes {
                    if let (Some(source_name), Some(table_name)) = (
                        val.get("source_name").and_then(|n| n.as_str()),
                        val.get("name").and_then(|n| n.as_str()),
                    ) {
                        sources.push((source_name.to_string(), table_name.to_string()));
                    }
                }
            }
        }
        sources
    }

    fn get_downstream_models(&self, target_unique_id: &str) -> Vec<String> {
        let mut downstream = Vec::new();
        if let Some(manifest) = self.manifest.get("default") {
            if let Some(nodes) = &manifest.nodes {
                for (_, node) in nodes {
                    if let Some(depends) = &node.depends_on {
                        if let Some(deps) = &depends.nodes {
                            if deps.contains(&target_unique_id.to_string()) {
                                downstream.push(node.name.clone());
                            }
                        }
                    }
                }
            }
        }
        downstream
    }

    fn find_column_info(&self, word: &str) -> Vec<String> {
        let mut info = Vec::new();
        if let Some(manifest) = self.manifest.get("default") {
            if let Some(nodes) = &manifest.nodes {
                for (_, node) in nodes {
                    if let Some(columns) = &node.columns {
                        for (col_name, col_data) in columns {
                            if col_name.eq_ignore_ascii_case(word) {
                                let dtype = col_data.data_type.as_deref().unwrap_or("UNKNOWN");
                                let desc =
                                    col_data.description.as_deref().unwrap_or("No description");
                                info.push(format!(
                                    "**Model**: `{}`\n**Type**: `{}`\n**Desc**: {}",
                                    node.name, dtype, desc
                                ));
                            }
                        }
                    }
                }
            }
        }
        info
    }

    fn find_model_info(&self, word: &str) -> Option<String> {
        if let Some(manifest) = self.manifest.get("default") {
            if let Some(nodes) = &manifest.nodes {
                for (unique_id, node) in nodes {
                    if node.name.eq_ignore_ascii_case(word) && unique_id.starts_with("model.") {
                        let mut upstream = Vec::new();
                        if let Some(depends) = &node.depends_on {
                            if let Some(deps) = &depends.nodes {
                                for dep in deps {
                                    if let Some(dep_node) = nodes.get(dep) {
                                        upstream.push(dep_node.name.clone());
                                    }
                                }
                            }
                        }
                        let downstream = self.get_downstream_models(unique_id);

                        let up_str = if upstream.is_empty() {
                            "None".to_string()
                        } else {
                            upstream.join(", ")
                        };
                        let down_str = if downstream.is_empty() {
                            "None".to_string()
                        } else {
                            downstream.join(", ")
                        };

                        return Some(format!(
                            "### Model: `{}`\n\n**⬆️ Upstream Dependencies:**\n{}\n\n**⬇️ Downstream Dependents:**\n{}",
                            node.name, up_str, down_str
                        ));
                    }
                }
            }
        }
        None
    }

    fn extract_word_at_position(&self, uri: &str, position: Position) -> Option<String> {
        if let Some(text) = self.document_map.get(uri) {
            let lines: Vec<&str> = text.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let char_idx = position.character as usize;
                if char_idx >= line.len() {
                    return None;
                }

                let mut start = char_idx;
                while start > 0
                    && (line.chars().nth(start - 1).unwrap().is_alphanumeric()
                        || line.chars().nth(start - 1).unwrap() == '_')
                {
                    start -= 1;
                }

                let mut end = char_idx;
                while end < line.len()
                    && (line.chars().nth(end).unwrap().is_alphanumeric()
                        || line.chars().nth(end).unwrap() == '_')
                {
                    end += 1;
                }

                if start < end {
                    return Some(line[start..end].to_string());
                }
            }
        }
        None
    }

    async fn publish_diagnostics(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        let open_tags = text.matches("{{").count();
        let close_tags = text.matches("}}").count();
        if open_tags != close_tags {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 1,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("JINJA_TAG_MISMATCH".to_string())),
                source: Some("dbt-oracle".to_string()),
                message: "Mismatched Jinja braces '{{' and '}}'".to_string(),
                ..Default::default()
            });
        }

        // Run actual DuckDB SQL parser validation!
        let stripped_sql = strip_jinja(&text);
        let dialect = DuckDbDialect {};
        if let Err(e) = Parser::parse_sql(&dialect, &stripped_sql) {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 10,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("SQL_SYNTAX_ERROR".to_string())),
                source: Some("dbt-oracle".to_string()),
                message: format!("Syntax Error: {}", e),
                ..Default::default()
            });
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "dbt-oracle".to_string(),
                version: Some("0.4.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "{".to_string(),
                        "(".to_string(),
                        "'".to_string(),
                        "\"".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "dbt-oracle initialized!")
            .await;
        self.load_manifest().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.document_map.insert(
            params.text_document.uri.to_string(),
            params.text_document.text.clone(),
        );
        self.publish_diagnostics(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.first() {
            self.document_map
                .insert(params.text_document.uri.to_string(), change.text.clone());
            self.publish_diagnostics(params.text_document.uri, change.text.clone())
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map
            .remove(&params.text_document.uri.to_string());
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();

        if let Some(word) = self.extract_word_at_position(&uri, position) {
            if let Some(model_info) = self.find_model_info(&word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Scalar(MarkedString::String(model_info)),
                    range: None,
                }));
            }

            let column_info = self.find_column_info(&word);
            if !column_info.is_empty() {
                let mut hover_text = format!("### Column: `{}`\n\n", word);
                hover_text.push_str(&column_info.join("\n\n---\n\n"));
                return Ok(Some(Hover {
                    contents: HoverContents::Scalar(MarkedString::String(hover_text)),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let mut completions = Vec::new();

        let models = self.get_model_names();
        for model in models {
            completions.push(CompletionItem {
                label: format!("ref('{}')", model),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(format!("Reference model {}", model)),
                insert_text: Some(format!("ref('{}')", model)),
                ..Default::default()
            });
        }

        let sources = self.get_source_names();
        for (source, table) in sources {
            completions.push(CompletionItem {
                label: format!("source('{}', '{}')", source, table),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(format!("Reference source {}.{}", source, table)),
                insert_text: Some(format!("source('{}', '{}')", source, table)),
                ..Default::default()
            });
        }

        completions.push(CompletionItem {
            label: "QUALIFY".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Snowflake: QUALIFY Clause".to_string()),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "read_parquet".to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("DuckDB: read_parquet()".to_string()),
            insert_text: Some("read_parquet('${1:path.parquet}')".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        Ok(Some(CompletionResponse::Array(completions)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_jinja_ref() {
        let sql = "SELECT * FROM {{ ref('stg_customers') }}";
        let stripped = strip_jinja(sql);
        assert_eq!(stripped, "SELECT * FROM jinja_macro");
    }

    #[test]
    fn test_strip_jinja_config() {
        let sql = "{{ config(materialized='table') }}\nSELECT * FROM data";
        let stripped = strip_jinja(sql);
        assert_eq!(stripped, "jinja_macro\nSELECT * FROM data");
    }

    #[test]
    fn test_duckdb_sql_validation_valid() {
        let sql = "SELECT customer_id FROM jinja_macro";
        let dialect = DuckDbDialect {};
        let result = Parser::parse_sql(&dialect, sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_duckdb_sql_validation_invalid() {
        let sql = "SELECT FROM table"; // Missing column before FROM
        let dialect = DuckDbDialect {};
        let result = Parser::parse_sql(&dialect, sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_duckdb_specific_sql() {
        // Test duckdb-specific extensions parse ok
        let sql = "SELECT * FROM read_parquet('data.parquet')";
        let dialect = DuckDbDialect {};
        let result = Parser::parse_sql(&dialect, sql);
        assert!(result.is_ok());
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: DashMap::new(),
        manifest: DashMap::new(),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
