use std::sync::Arc;
use dashmap::DashMap;
use lsp_types::*;
use tower_lsp::{jsonrpc, lsp_types, LanguageServer, LspService, Server};
use tokio::sync::RwLock;

use aether_core::{Span, ProvenanceTag, TypeRepr};
use aether_parser::parse;
use aether_checker::{
    resolve_names, infer_types, check_effects, validate_provenance,
    TypedProgram
};

#[derive(Debug, Clone)]
pub struct DocumentState {
    pub uri: Url,
    pub content: String,
    pub typed_program: Option<TypedProgram>,
    pub diagnostics: Vec<Diagnostic>,
    pub trust_score: f64,
}

impl DocumentState {
    pub fn new(uri: Url) -> Self {
        Self {
            uri,
            content: String::new(),
            typed_program: None,
            diagnostics: Vec::new(),
            trust_score: 1.0,
        }
    }
}

pub struct AetherLanguageServer {
    documents: Arc<DashMap<Url, Arc<RwLock<DocumentState>>>>,
}

impl AetherLanguageServer {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(DashMap::new()),
        }
    }

    fn run_checker_pipeline(&self, _uri: &Url, content: &str) -> (Vec<Diagnostic>, f64, Option<TypedProgram>) {
        let mut diagnostics = Vec::new();
        let mut trust_score = 1.0;

        // Parse the file
        let parse_result = parse(content);
        if !parse_result.errors.is_empty() {
            for error in &parse_result.errors {
                // Use placeholder span for parser errors since they don't have span info
                let span = Span { 
    start: 0, 
    end: 0, 
    provenance: None 
};
                diagnostics.push(Diagnostic {
                    range: self.span_to_range(&span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(format!("E{:04}", 1000))),
                    source: Some("aether".to_string()),
                    message: error.to_string(),
                    ..Default::default()
                });
            }
            return (diagnostics, trust_score, None);
        }
        
        // Run the checker pipeline
        let resolved_result = resolve_names(&parse_result);
        let typed_result = infer_types(&resolved_result.resolved_ast);
        
        // Add type inference errors
        diagnostics.extend(typed_result.errors.iter().map(|error| {
            let span = Span { 
    start: 0, 
    end: 0, 
    provenance: None 
}; // Placeholder span
            Diagnostic {
                range: self.span_to_range(&span),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String(format!("E{:04}", 2000))),
                source: Some("aether".to_string()),
                message: error.to_string(),
                ..Default::default()
            }
        }));

        // Effect checking
        let effect_check_result = check_effects(&typed_result.typed_ast);
        if !effect_check_result.errors.is_empty() {
            for error in &effect_check_result.errors {
                let span = Span { 
    start: 0, 
    end: 0, 
    provenance: None 
}; // Placeholder span
                diagnostics.push(Diagnostic {
                    range: self.span_to_range(&span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(format!("E{:04}", 2010))),
                    source: Some("aether".to_string()),
                    message: error.to_string(),
                    ..Default::default()
                });
            }
        }

        // Provenance validation
        let prov_check_result = validate_provenance(&typed_result.typed_ast);
        if !prov_check_result.errors.is_empty() {
            for error in &prov_check_result.errors {
                let span = Span { 
    start: 0, 
    end: 0, 
    provenance: None 
}; // Placeholder span
                diagnostics.push(Diagnostic {
                    range: self.span_to_range(&span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(format!("E{:04}", 3000))),
                    source: Some("aether".to_string()),
                    message: error.to_string(),
                    ..Default::default()
                });
            }
        }

        // Calculate trust score based on provenance tags
        let typed_program = TypedProgram {
            imports: typed_result.typed_ast.program.imports.clone(),
            externs: typed_result.typed_ast.program.externs.clone(),
            types: typed_result.typed_ast.program.types.clone(),
            effects: typed_result.typed_ast.program.effects.clone(),
            functions: typed_result.typed_ast.program.functions.clone(),
            version: typed_result.typed_ast.program.version.clone(),
        };
        
        trust_score = self.calculate_trust_score(&typed_program);

        (diagnostics, trust_score, Some(typed_program))
    }

    fn span_to_range(&self, span: &Span) -> Range {
        // Convert Span to LSP Range
        // For now, we'll use a simple line-based conversion
        // In a real implementation, you'd need to map byte offsets to line/column positions
        Range {
            start: Position {
                line: 0,
                character: span.start as u32,
            },
            end: Position {
                line: 0,
                character: span.end as u32,
            },
        }
    }

    fn calculate_trust_score(&self, program: &TypedProgram) -> f64 {
        let mut total_confidence = 0.0;
        let mut tag_count = 0;

        // Collect all provenance tags from the program
        self.collect_provenance_tags(&program, &mut total_confidence, &mut tag_count);

        if tag_count == 0 {
            1.0 // Default trust score for files without provenance
        } else {
            total_confidence / tag_count as f64
        }
    }

    fn collect_provenance_tags(&self, program: &TypedProgram, total_confidence: &mut f64, tag_count: &mut usize) {
        // Collect tags from extern declarations
        for extern_decl in &program.externs {
            let tag = &extern_decl.provenance;
            if tag.author != aether_core::AuthorType::Human {
                *total_confidence += tag.confidence;
                *tag_count += 1;
            }
        }

        // Collect tags from function declarations
        for fn_decl in &program.functions {
            if let Some(ref tag) = fn_decl.provenance {
                if tag.author != aether_core::AuthorType::Human {
                    *total_confidence += tag.confidence;
                    *tag_count += 1;
                }
            }
        }

        // Type declarations and effect declarations don't have provenance tags in the current implementation
    }

    fn find_identifier_at_position(&self, content: &str, position: Position) -> Option<(String, Option<ProvenanceTag>)> {
        // Convert position to byte offset
        let lines: Vec<&str> = content.lines().collect();
        let mut byte_offset = 0;
        for (line_num, line) in lines.iter().enumerate() {
            if line_num == position.line as usize {
                byte_offset += position.character as usize;
                break;
            }
            byte_offset += line.len() + 1; // +1 for newline
        }

        // Find the function at this position
        let function_start = content[..byte_offset].rfind("fn ");
        if let Some(start) = function_start {
            let function_end = content[start..].find('{').unwrap_or(content.len() - start);
            let function_text = &content[start..start + function_end];
            
            // Parse @prov tag from function text
            if let Some(prov_start) = function_text.find("@prov") {
                let prov_end = function_text[prov_start..].find(')').unwrap_or(function_text.len() - prov_start - 1);
                let prov_text = &function_text[prov_start..prov_start + prov_end + 1];
                
                // Extract source and confidence from @prov tag
                if let Some(source_start) = prov_text.find("source:") {
                    let source_part = &prov_text[source_start + 8..];
                    let source_end = source_part.find(',').unwrap_or(source_part.len());
                    let source = source_part[..source_end].trim().trim_matches('"');
                    
                    if let Some(conf_start) = prov_text.find("confidence:") {
                        let conf_part = &prov_text[conf_start + 12..];
                        let conf_end = conf_part.find(',').unwrap_or(conf_part.find(')').unwrap_or(conf_part.len()));
                        let conf_str = conf_part[..conf_end].trim();
                        
                        if let Ok(confidence) = conf_str.parse::<f64>() {
                            let tag = ProvenanceTag {
                                id: uuid::Uuid::new_v4(),
                                author: if source == "user" { aether_core::AuthorType::Human } else { aether_core::AuthorType::AI(source.to_string()) },
                                model: Some(source.to_string()),
                                confidence,
                                timestamp: chrono::Utc::now(),
                                parents: vec![],
                                prompt: None,
                                version: "1.0".to_string(),
                            };
                            
                            // Extract function name
                            if let Some(name_start) = function_text.find("fn ") {
                                let name_part = &function_text[name_start + 3..];
                                let name_end = name_part.find('(').unwrap_or(name_part.find('{').unwrap_or(name_part.len()));
                                let function_name = name_part[..name_end].trim();
                                
                                return Some((function_name.to_string(), Some(tag)));
                            }
                        }
                    }
                }
            }
        }
        
        None
    }

    fn format_type_repr(&self, type_repr: &TypeRepr) -> String {
        match type_repr {
            TypeRepr::Int => "Int".to_string(),
            TypeRepr::Float => "Float".to_string(),
            TypeRepr::String => "String".to_string(),
            TypeRepr::Bool => "Bool".to_string(),
            TypeRepr::Unit => "Unit".to_string(),
            TypeRepr::Function(params, ret, effects) => {
                let param_strs: Vec<String> = params.iter().map(|p| self.format_type_repr(p)).collect();
                let ret_str = self.format_type_repr(ret);
                let effects_str = if effects.is_empty() { String::new() } else { format!(" effects: {:?}", effects) };
                format!("({}) -> {}{}", param_strs.join(", "), ret_str, effects_str)
            }
            TypeRepr::Record(fields) => {
                let field_strs: Vec<String> = fields.iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.format_type_repr(ty)))
                    .collect();
                format!("{{{}}}", field_strs.join(", "))
            }
            TypeRepr::Option(inner) => format!("Option({})", self.format_type_repr(inner)),
            TypeRepr::Union(left, right) => format!("Union({}, {})", self.format_type_repr(left), self.format_type_repr(right)),
            TypeRepr::TypeVar(name) => format!("'{}", name),
            TypeRepr::Named(name, _) => name.clone(),
        }
    }

    fn format_provenance_tag(&self, tag: &ProvenanceTag) -> String {
        format!(
            "@prov(author: {:?}, model: {:?}, confidence: {:.2}, timestamp: {})",
            tag.author,
            tag.model,
            tag.confidence,
            tag.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for AetherLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Aether Language Server".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        
        let doc_state = Arc::new(RwLock::new(DocumentState::new(uri.clone())));
        {
            let mut state = doc_state.write().await;
            state.content = content.clone();
        }
        
        self.documents.insert(uri.clone(), doc_state);
        
        // Run checker pipeline and publish diagnostics
        let (diagnostics, trust_score, typed_program) = self.run_checker_pipeline(&uri, &content);
        
        {
            let doc_state = self.documents.get(&uri).unwrap();
            let mut state = doc_state.write().await;
            state.diagnostics = diagnostics.clone();
            state.trust_score = trust_score;
            state.typed_program = typed_program;
        }
        
        // Send diagnostics
        // Note: In a real implementation, you'd send this via the connection
        // For now, we'll just store it in the document state
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        
        if let Some(doc_state) = self.documents.get(&uri) {
            let mut state = doc_state.write().await;
            
            // Update content
            if let Some(change) = params.content_changes.first() {
                state.content = change.text.clone();
            }
            
            // Run checker pipeline
            let (diagnostics, trust_score, typed_program) = self.run_checker_pipeline(&uri, &state.content);
            
            state.diagnostics = diagnostics.clone();
            state.trust_score = trust_score;
            state.typed_program = typed_program;
            
            // Send diagnostics and trust score notification
            // Note: In a real implementation, you'd send this via the connection
        }
    }

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        
        if let Some(doc_state) = self.documents.get(&uri) {
            let state = doc_state.read().await;
            
            if let Some((function_name, provenance)) = 
                self.find_identifier_at_position(&state.content, params.text_document_position_params.position) {
                
                if let Some(tag) = provenance {
                    let source = tag.model.as_deref().unwrap_or("unknown");
                    let content = format!(
                        "source: \"{}\"  confidence: {:.2}  [trust: evolving]",
                        source, tag.confidence
                    );
                    
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(tower_lsp::lsp_types::MarkedString::String(content)),
                        range: None,
                    }));
                }
            }
        }
        
        Ok(None)
    }
}

pub fn start_server() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let (service, socket) = LspService::build(|_| AetherLanguageServer::new()).finish();
        Server::new(tokio::io::stdin(), tokio::io::stdout(), socket).serve(service).await;
    });
}
