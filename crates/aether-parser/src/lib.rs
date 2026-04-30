//! Aether Parser - Hand-written lexer and recursive descent parser
//! 
//! This crate implements parsing for Aether source code according to the grammar
//! defined in spec/grammar.ebnf. The parser never panics and always returns
//! a ParseResult even on failure.
//! 
//! Added: Function call support (print("hello") and add(40, 2))

use aether_core::*;
use std::str::FromStr;
use uuid::Uuid;
use std::iter::Peekable;
use std::str::CharIndices;

/// Parser error codes (E1xxx range)
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParserError {
    #[error("E1001: Unexpected token '{0}' at {1}, expected {2}")]
    UnexpectedToken(String, String, String),
    
    #[error("E1002: Invalid version header format at {0}")]
    InvalidVersionHeader(String),
    
    #[error("E1003: Missing @prov tag on extern declaration at {0}")]
    MissingProvenanceTag(String),
    
    #[error("E1004: Invalid provenance tag format at {0}")]
    InvalidProvenanceTag(String),
    
    #[error("E1005: Unterminated string literal at {0}")]
    UnterminatedString(String),
    
    #[error("E1006: Invalid number format at {0}")]
    InvalidNumber(String),
    
    #[error("E1007: Unexpected end of file at {0}")]
    UnexpectedEOF(String),
    
    #[error("E1008: Invalid identifier '{0}' at {1}")]
    InvalidIdentifier(String, String),
    
    #[error("E1009: Duplicate declaration '{0}' at {1}")]
    DuplicateDeclaration(String, String),
    
    #[error("E1010: Invalid ISO8601 timestamp '{0}' at {1}")]
    InvalidTimestamp(String, String),
}

/// Hint about provenance that couldn't be fully parsed
#[derive(Debug, Clone)]
pub struct ProvenanceHint {
    pub span: Span,
    pub hint_type: String,
    pub suggestion: String,
}

/// Result of parsing operation
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub ast: Program,
    pub errors: Vec<ParserError>,
    pub provenance_hints: Vec<ProvenanceHint>,
}

/// Token with grammar rule ID
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
    pub grammar_rule: Option<String>, // EBNF production name
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Import,
    Extern,
    Type,
    Effect,
    Fn,
    Declare,
    Let,
    Shadow,
    Return,
    If,
    Else,
    Match,
    True,
    False,
    Some,
    None,
    Option,
    Unit,
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    EqEq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Not,
    Assign,
    Arrow,
    Pipe,
    Bang,
    
    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LSquare,
    RSquare,
    Semicolon,
    Comma,
    Dot,
    Colon,
    Less,
    Greater,
    
    // Literals and identifiers
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    
    // Special
    AtProv,
    EOF,
}

/// Hand-written lexer
pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<CharIndices<'a>>,
    position: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices().peekable(),
            position: 0,
            line: 1,
            column: 1,
        }
    }
    
    fn current_span(&self, start_pos: usize) -> Span {
        Span::new(start_pos, self.position)
    }
    
    fn advance(&mut self) -> Option<char> {
        if let Some((pos, ch)) = self.chars.next() {
            self.position = pos;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }
    
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, ch)| ch)
    }
    
    pub fn next_token(&mut self) -> Result<Token, ParserError> {
        // Skip whitespace
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
        
        let start_pos = self.position;
        
        if let Some(ch) = self.advance() {
            let token = match ch {
                '+' => TokenKind::Plus,
                '-' => {
                    if self.peek() == Some('>') {
                        self.advance();
                        TokenKind::Arrow
                    } else {
                        TokenKind::Minus
                    }
                }
                '*' => TokenKind::Star,
                '/' => TokenKind::Slash,
                '=' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::EqEq
                    } else {
                        TokenKind::Assign
                    }
                }
                '!' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::NotEq
                    } else if self.peek() == Some('{') {
                        self.advance();
                        TokenKind::Bang
                    } else {
                        return Err(ParserError::UnexpectedToken(
                            "!".to_string(),
                            format!("{}:{}", self.line, self.column),
                            "expression or effect set".to_string(),
                        ));
                    }
                }
                '#' => {
                    // Handle comments - skip until end of line
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                    }
                    // Return the next token after the comment
                    return self.next_token();
                }
                '<' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Le
                    } else {
                        TokenKind::Less
                    }
                }
                '>' => {
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Ge
                    } else {
                        TokenKind::Greater
                    }
                }
                '&' => {
                    if self.peek() == Some('&') {
                        self.advance();
                        TokenKind::And
                    } else {
                        return Err(ParserError::UnexpectedToken(
                            "&".to_string(),
                            format!("{}:{}", self.line, self.column),
                            "&&".to_string(),
                        ));
                    }
                }
                '|' => {
                    if self.peek() == Some('|') {
                        self.advance();
                        TokenKind::Or
                    } else {
                        TokenKind::Pipe
                    }
                }
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                '[' => TokenKind::LSquare,
                ']' => TokenKind::RSquare,
                ';' => TokenKind::Semicolon,
                ',' => TokenKind::Comma,
                '.' => TokenKind::Dot,
                ':' => TokenKind::Colon,
                '"' => {
                    let mut content = String::new();
                    while let Some(ch) = self.peek() {
                        if ch == '"' {
                            self.advance();
                            break;
                        }
                        if ch == '\\' {
                            self.advance();
                            if let Some(escaped) = self.advance() {
                                match escaped {
                                    'n' => content.push('\n'),
                                    't' => content.push('\t'),
                                    'r' => content.push('\r'),
                                    '\\' => content.push('\\'),
                                    '"' => content.push('"'),
                                    _ => content.push(escaped),
                                }
                            }
                        } else {
                            content.push(self.advance().unwrap());
                        }
                    }
                    TokenKind::StringLit(content)
                }
                '@' => {
                    if self.peek() == Some('p') {
                        // Check for @prov
                        let mut chars = Vec::new();
                        chars.push('p');
                        self.advance();
                        if self.peek() == Some('r') {
                            chars.push('r');
                            self.advance();
                        }
                        if self.peek() == Some('o') {
                            chars.push('o');
                            self.advance();
                        }
                        if self.peek() == Some('v') {
                            chars.push('v');
                            self.advance();
                        }
                        
                        if chars == ['p', 'r', 'o', 'v'] {
                            TokenKind::AtProv
                        } else {
                            TokenKind::Ident(format!("@{}", String::from_iter(chars)))
                        }
                    } else {
                        return Err(ParserError::UnexpectedToken(
                            "@".to_string(),
                            format!("{}:{}", self.line, self.column),
                            "identifier".to_string(),
                        ));
                    }
                }
                ch if ch.is_ascii_digit() => {
                    let mut num_str = String::new();
                    num_str.push(ch);
                    
                    while let Some(&(_, next_ch)) = self.chars.peek() {
                        if next_ch.is_ascii_digit() || next_ch == '.' {
                            num_str.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                    
                    if num_str.contains('.') {
                        match num_str.parse::<f64>() {
                            Ok(f) => TokenKind::FloatLit(f),
                            Err(_) => return Err(ParserError::InvalidNumber(
                                format!("{}:{}", self.line, self.column)
                            )),
                        }
                    } else {
                        match num_str.parse::<i64>() {
                            Ok(i) => TokenKind::IntLit(i),
                            Err(_) => return Err(ParserError::InvalidNumber(
                                format!("{}:{}", self.line, self.column)
                            )),
                        }
                    }
                }
                ch if ch.is_ascii_alphabetic() || ch == '_' => {
                    let mut ident = String::new();
                    ident.push(ch);
                    
                    while let Some(&(_, next_ch)) = self.chars.peek() {
                        if next_ch.is_ascii_alphanumeric() || next_ch == '_' {
                            ident.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                    
                    match ident.as_str() {
                        "import" => TokenKind::Import,
                        "extern" => TokenKind::Extern,
                        "type" => TokenKind::Type,
                        "effect" => TokenKind::Effect,
                        "fn" => TokenKind::Fn,
                        "declare" => TokenKind::Declare,
                        "let" => TokenKind::Let,
                        "shadow" => TokenKind::Shadow,
                        "return" => TokenKind::Return,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "match" => TokenKind::Match,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "some" => TokenKind::Some,
                        "none" => TokenKind::None,
                        "option" => TokenKind::Option,
                        "unit" => TokenKind::Unit,
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "not" => TokenKind::Not,
                        _ => TokenKind::Ident(ident),
                    }
                }
                _ => {
                    return Err(ParserError::UnexpectedToken(
                        ch.to_string(),
                        format!("{}:{}", self.line, self.column),
                        "valid token".to_string(),
                    ));
                }
            };
            
            Ok(Token {
            kind: token,
            text: self.input[start_pos..self.position].to_string(),
            span: self.current_span(start_pos),
            grammar_rule: None, // Will be set by parser
        })
        } else {
            Ok(Token {
                kind: TokenKind::EOF,
                text: String::new(),
                span: Span::new(start_pos, self.position),
                grammar_rule: None,
            })
        }
    }
}

/// Recursive descent parser
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current_token: Option<Token>,
    peek_token: Option<Token>,
    errors: Vec<ParserError>,
    provenance_hints: Vec<ProvenanceHint>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token().ok();
        let peek_token = lexer.next_token().ok();
        
        Self {
            lexer,
            current_token,
            peek_token,
            errors: Vec::new(),
            provenance_hints: Vec::new(),
        }
    }
    
    fn advance(&mut self) {
        self.current_token = self.peek_token.take();
        self.peek_token = self.lexer.next_token().ok();
    }
    
    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParserError> {
        if let Some(ref token) = self.current_token {
            if token.kind == expected {
                let token = token.clone();
                self.advance();
                Ok(token)
            } else {
                Err(ParserError::UnexpectedToken(
                    format!("{:?}", token.kind),
                    format!("{}:{}", token.span.start, token.span.end),
                    format!("{:?}", expected),
                ))
            }
        } else {
            Err(ParserError::UnexpectedEOF(
                "expected token".to_string(),
            ))
        }
    }
    
    // Dead methods - commented out to remove warnings
// fn matches(&mut self, expected: TokenKind) -> bool {
//         if let Some(ref token) = self.current_token {
//             if token.kind == expected {
//                 self.advance();
//                 true
//             } else {
//                 false
//             }
//         } else {
//             false
//         }
//     }
    
    fn expect_identifier(&mut self) -> Result<String, ParserError> {
        if let Some(ref token) = self.current_token {
            if let TokenKind::Ident(ref name) = token.kind {
                let name = name.clone();
                self.advance();
                Ok(name)
            } else {
                Err(ParserError::UnexpectedToken(
                    format!("{:?}", token.kind),
                    format!("{}:{}", token.span.start, token.span.end),
                    "identifier".to_string(),
                ))
            }
        } else {
            Err(ParserError::UnexpectedEOF(
                "expected identifier".to_string(),
            ))
        }
    }
    
    fn expect_string(&mut self) -> Result<String, ParserError> {
        if let Some(ref token) = self.current_token {
            if let TokenKind::StringLit(ref content) = token.kind {
                let content = content.clone();
                self.advance();
                Ok(content)
            } else {
                Err(ParserError::UnexpectedToken(
                    format!("{:?}", token.kind),
                    format!("{}:{}", token.span.start, token.span.end),
                    "string literal".to_string(),
                ))
            }
        } else {
            Err(ParserError::UnexpectedEOF(
                "expected string literal".to_string(),
            ))
        }
    }
    
    pub fn parse(&mut self) -> ParseResult {
        let mut program = Program {
            statements: Vec::new(),
            imports: Vec::new(),
            externs: Vec::new(),
            types: Vec::new(),
            effects: Vec::new(),
            functions: Vec::new(),
            version: "0.1.0".to_string(), // Default
        };
        
        // Parse version header
        self.parse_version_header(&mut program);
        
        // Parse top-level declarations
        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::EOF) {
            match self.parse_top_level() {
                Ok(Some(decl)) => {
                    match decl {
                        TopLevelDecl::Statement(stmt) => program.statements.push(stmt),
                        TopLevelDecl::Import(import) => program.imports.push(import),
                        TopLevelDecl::Extern(extern_decl) => program.externs.push(extern_decl),
                        TopLevelDecl::Type(type_decl) => program.types.push(type_decl),
                        TopLevelDecl::Effect(effect_decl) => program.effects.push(effect_decl),
                        TopLevelDecl::Function(fn_decl) => {
                            program.functions.push(fn_decl);
                        }
                    }
                }
                Ok(None) => {
                    // Skip unknown tokens
                    self.advance();
                }
                Err(e) => {
                    self.errors.push(e);
                    self.advance();
                }
            }
        }
        
        ParseResult {
            ast: program,
            errors: self.errors.clone(),
            provenance_hints: self.provenance_hints.clone(),
        }
    }
    
    fn parse_version_header(&mut self, _program: &mut Program) {
        // TODO: Parse "# Aether 0.1.0+git.abc123" format
        // For now, skip only actual comment lines (starting with #), not @prov tags
        while let Some(ref token) = self.current_token {
            if matches!(token.kind, TokenKind::Ident(_)) && token.text.starts_with('#') && !token.text.starts_with("@") {
                // Skip comment line
                while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::Semicolon) {
                    self.advance();
                }
                self.advance();
            } else if token.kind == TokenKind::AtProv {
                // Don't consume @prov tags - they are part of function declarations
                break;
            } else {
                break;
            }
        }
    }
    
    fn parse_top_level(&mut self) -> Result<Option<TopLevelDecl>, ParserError> {
        if let Some(ref token) = self.current_token {
            match token.kind {
                TokenKind::Import => {
                    // Check if this is a file import (import "path.ae";) or module import
                    if let Some(peek_token) = self.peek_token.clone() {
                        if let TokenKind::StringLit(_) = peek_token.kind {
                            // File import - parse as statement
                            let stmt = self.parse_import_stmt()?;
                            Ok(Some(TopLevelDecl::Statement(aether_core::Stmt::Import(stmt))))
                        } else {
                            // Module import - parse as declaration
                            Ok(Some(TopLevelDecl::Import(self.parse_import()?)))
                        }
                    } else {
                        Ok(None)
                    }
                },
                TokenKind::Extern => Ok(Some(TopLevelDecl::Extern(self.parse_extern()?))),
                TokenKind::Type => Ok(Some(TopLevelDecl::Type(self.parse_type_decl()?))),
                TokenKind::Effect => Ok(Some(TopLevelDecl::Effect(self.parse_effect_decl()?))),
                TokenKind::AtProv => {
                    // @prov tag before a declaration - parse the declaration that follows
                    let fn_decl = self.parse_fn_decl()?;
                    Ok(Some(TopLevelDecl::Function(fn_decl)))
                }
                TokenKind::Fn => Ok(Some(TopLevelDecl::Function(self.parse_fn_decl()?))),
                _ => {
                    // Try to parse as a statement
                    if let Ok(stmt) = self.parse_statement() {
                        Ok(Some(TopLevelDecl::Statement(stmt)))
                    } else {
                        Ok(None)
                    }
                },
            }
        } else {
            Ok(None)
        }
    }
    fn parse_import(&mut self) -> Result<ImportDecl, ParserError> {
        self.expect(TokenKind::Import)?;
        // TODO: Parse module path and alias
        Ok(ImportDecl {
            module_path: vec!["module".to_string()],
            alias: None,
        })
    }
    
    fn parse_import_stmt(&mut self) -> Result<aether_core::ImportStmt, ParserError> {
        self.expect(TokenKind::Import)?;
        let path = self.expect_string()?;
        self.expect(TokenKind::Semicolon)?;
        Ok(aether_core::ImportStmt { path })
    }
    
    fn parse_statement(&mut self) -> Result<aether_core::Stmt, ParserError> {
        if let Some(ref token) = self.current_token {
            match token.kind {
                TokenKind::Import => self.parse_import_stmt().map(aether_core::Stmt::Import),
                TokenKind::Let => {
                    // Parse let statement
                    self.advance(); // consume 'let'
                    let name = self.expect_identifier()?;
                    self.expect(TokenKind::Colon)?;
                    let type_repr = aether_core::TypeRepr::Unit; // TODO: parse actual type
                    self.expect(TokenKind::Assign)?;
                    let expr = self.parse_simple_expr()?;
                    self.expect(TokenKind::Semicolon)?;
                    Ok(aether_core::Stmt::Let(name, Some(type_repr), Box::new(expr)))
                },
                _ => Err(ParserError::UnexpectedToken(format!("{:?}", token.kind), "statement".to_string(), "EOF".to_string())),
            }
        } else {
            Err(ParserError::UnexpectedToken("EOF".to_string(), "statement".to_string(), "EOF".to_string()))
        }
    }
    
    fn parse_extern(&mut self) -> Result<ExternDecl, ParserError> {
        let extern_token = self.expect(TokenKind::Extern)?;
        
        // Check for @prov tag - this is required
        let provenance = if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::AtProv) {
            self.parse_provenance_tag()?
        } else {
            return Err(ParserError::MissingProvenanceTag(
                format!("{}:{}", extern_token.span.start, extern_token.span.end)
            ));
        };
        
        // TODO: Parse name and type
        Ok(ExternDecl {
            name: "extern_name".to_string(),
            type_expr: TypeRepr::Unit,
            provenance,
        })
    }
    
    fn parse_provenance_tag(&mut self) -> Result<ProvenanceTag, ParserError> {
        self.expect(TokenKind::AtProv)?;
        self.expect(TokenKind::LParen)?;
        
        let mut author = AuthorType::Human;
        let mut model = None;
        let mut timestamp = chrono::Utc::now();
        let mut prompt = None;
        let mut confidence = 1.0;
        let mut parents = Vec::new();
        let mut version = "0.1.0".to_string();
        
        // Parse provenance fields
        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RParen) {
            if let Some(ref token) = self.current_token {
                match &token.kind {
                    TokenKind::Ident(name) if name == "author" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        let value = self.expect_string()?;
                        author = AuthorType::from_str(&value).map_err(|e| {
                            ParserError::InvalidProvenanceTag(format!("Invalid author: {}", e))
                        })?;
                    }
                    TokenKind::Ident(name) if name == "model" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        model = Some(self.expect_string()?);
                    }
                    TokenKind::Ident(name) if name == "timestamp" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        let ts_str = self.expect_string()?;
                        timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                            .map_err(|e| ParserError::InvalidProvenanceTag(format!("Invalid timestamp: {}", e)))?.into();
                    }
                    TokenKind::Ident(name) if name == "prompt" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        prompt = Some(self.expect_string()?);
                    }
                    TokenKind::Ident(name) if name == "confidence" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        match self.current_token.clone() {
                            Some(t) => match t.kind {
                                TokenKind::FloatLit(v) => { self.advance(); confidence = v; }
                                TokenKind::IntLit(v) => { self.advance(); confidence = v as f64; }
                                _ => return Err(ParserError::InvalidProvenanceTag("confidence must be a number".to_string())),
                            },
                            None => return Err(ParserError::InvalidProvenanceTag("expected confidence value".to_string())),
                        }
                    }
                    TokenKind::Ident(name) if name == "parent" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        let parent_str = self.expect_string()?;
                        let parent_uuid = Uuid::parse_str(&parent_str)
                            .map_err(|e| ParserError::InvalidProvenanceTag(format!("Invalid parent UUID: {}", e)))?;
                        parents.push(parent_uuid);
                    }
                    TokenKind::Ident(name) if name == "version" => {
                        self.advance();
                        self.expect(TokenKind::Colon)?;
                        version = self.expect_string()?;
                    }
                    TokenKind::Ident(name) if name == "source" => {
                        self.advance(); // consume "source"
                        self.expect(TokenKind::Colon)?; // consume ":"
                        let value = self.expect_string()?; // consume the string value
                        author = AuthorType::from_str(&value).map_err(|e| {
                            ParserError::InvalidProvenanceTag(format!("Invalid source: {}", e))
                        })?;
                    }
                    _ => {
                        return Err(ParserError::InvalidProvenanceTag(format!("Unexpected provenance field: {}", token.text)));
                    }
                }
            }
            // consume optional comma between fields
            if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Comma) {
                self.advance();
            }
        }
        
        self.expect(TokenKind::RParen)?;
        
        Ok(ProvenanceTag {
            id: Uuid::new_v4(),
            author,
            model,
            timestamp,
            prompt,
            confidence,
            parents,
            version,
        })
    }
    
    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParserError> {
        self.expect(TokenKind::Type)?;
        // TODO: Parse type declaration
        Ok(TypeDecl {
            name: "TypeName".to_string(),
            type_params: Vec::new(),
            definition: TypeRepr::Unit,
        })
    }
    
    fn parse_effect_decl(&mut self) -> Result<EffectDecl, ParserError> {
        self.expect(TokenKind::Effect)?;
        // TODO: Parse effect declaration
        Ok(EffectDecl {
            name: "EffectName".to_string(),
            operations: Vec::new(),
        })
    }
    
    
    fn parse_simple_expr(&mut self) -> Result<aether_core::Expr, ParserError> {
        let lhs = match self.current_token.clone() {
            Some(tok) => match tok.kind {
                TokenKind::IntLit(v) => {
                    self.advance();
                    aether_core::Expr::Literal(aether_core::Literal::Int(v))
                }
                TokenKind::True => {
                    self.advance();
                    aether_core::Expr::Literal(aether_core::Literal::Bool(true))
                }
                TokenKind::False => {
                    self.advance();
                    aether_core::Expr::Literal(aether_core::Literal::Bool(false))
                }
                TokenKind::Ident(name) => {
                    self.advance();
                    if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::LParen) {
                        self.advance(); // consume (
                        let mut args = Vec::new();
                        let mut first = true;
                        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RParen) {
                            if !first {
                                if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Comma) {
                                    self.advance();
                                } else { break; }
                            }
                            first = false;
                            args.push(self.parse_simple_expr()?);
                        }
                        self.expect(TokenKind::RParen)?;
                        aether_core::Expr::Call(Box::new(aether_core::Expr::Ident(name)), args)
                    } else {
                        aether_core::Expr::Ident(name)
                    }
                }
                TokenKind::If => {
                    self.advance(); // consume 'if'
                    let condition = self.parse_simple_expr()?;
                    self.expect(TokenKind::LBrace)?;
                    let then_expr = self.parse_simple_expr()?;
                    self.expect(TokenKind::RBrace)?;
                    let else_expr = if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Else) {
                        self.advance(); // consume 'else'
                        self.expect(TokenKind::LBrace)?;
                        let expr = self.parse_simple_expr()?;
                        self.expect(TokenKind::RBrace)?;
                        Some(expr)
                    } else {
                        None
                    };
                    aether_core::Expr::If(Box::new(condition), vec![aether_core::Stmt::ExprStmt(Box::new(then_expr))], else_expr.map(|e| vec![aether_core::Stmt::ExprStmt(Box::new(e))]))
                }
                _ => {
                    return Err(ParserError::UnexpectedToken(
                        format!("{:?}", tok.kind),
                        tok.span.start.to_string(),
                        "expression".to_string(),
                    ))
                }
            },
            None => return Err(ParserError::UnexpectedEOF("expression".to_string())),
        };

        // Check for binary operator
        let op = match self.current_token.as_ref().map(|t| &t.kind) {
            Some(TokenKind::Plus)  => Some(aether_core::BinaryOp::Add),
            Some(TokenKind::Minus) => Some(aether_core::BinaryOp::Sub),
            Some(TokenKind::Star)  => Some(aether_core::BinaryOp::Mul),
            Some(TokenKind::EqEq)  => Some(aether_core::BinaryOp::Equal),
            Some(TokenKind::NotEq) => Some(aether_core::BinaryOp::NotEqual),
            Some(TokenKind::Lt)    => Some(aether_core::BinaryOp::Less),
            Some(TokenKind::Gt)    => Some(aether_core::BinaryOp::Greater),
            _ => None,
        };

        if let Some(bin_op) = op {
            self.advance(); // consume operator
            let rhs = self.parse_simple_expr()?;
            Ok(aether_core::Expr::Binary(Box::new(lhs), bin_op, Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    fn parse_fn_decl(&mut self) -> Result<FnDecl, ParserError> {
        // Check for optional @prov tag before fn keyword
        let provenance = if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::AtProv) {
            Some(self.parse_provenance_tag()?)
        } else {
            None
        };
        
        // Expect the 'fn' keyword (always present whether @prov was there or not)
        self.expect(TokenKind::Fn)?;
        
        // Parse the function name
        let name = self.expect_identifier()?;
        
        // Parse function parameters
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        let mut first_param = true;
        
        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RParen) {
            if !first_param {
                // Expect comma
                self.expect(TokenKind::Comma)?;
            }
            first_param = false;
            
            // Parse parameter: name: Type
            let param_name = self.expect_identifier()?;
            self.expect(TokenKind::Colon)?;
            
            // Parse parameter type
            let param_type = if let Some(ref type_token) = self.current_token {
                match &type_token.kind {
                    TokenKind::Ident(type_name) => {
                        match type_name.as_str() {
                            "Int" => aether_core::TypeRepr::Int,
                            "Float" => aether_core::TypeRepr::Float,
                            "Bool" => aether_core::TypeRepr::Bool,
                            "String" => aether_core::TypeRepr::String,
                            "Unit" => aether_core::TypeRepr::Unit,
                            _ => aether_core::TypeRepr::Unit,
                        }
                    }
                    _ => aether_core::TypeRepr::Unit,
                }
            } else {
                aether_core::TypeRepr::Unit
            };
            self.advance(); // consume type
            
            params.push((param_name, param_type));
        }
        
        self.expect(TokenKind::RParen)?;
        
        // Parse return type
        let return_type = if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Arrow) {
            self.expect(TokenKind::Arrow)?;
            let type_token = self.current_token.as_ref().ok_or_else(|| ParserError::UnexpectedToken(
                "EOF".to_string(),
                format!("{}:{}", self.lexer.line, self.lexer.column),
                "type".to_string(),
            ))?;
            
            let return_type = match &type_token.kind {
                TokenKind::Ident(name) => {
                    match name.as_str() {
                        "Int" => TypeRepr::Int,
                        "Float" => TypeRepr::Float,
                        "Bool" => TypeRepr::Bool,
                        "String" => TypeRepr::String,
                        "Unit" => TypeRepr::Unit,
                        _ => TypeRepr::Unit,
                    }
                }
                _ => TypeRepr::Unit,
            };
            self.advance();
            return_type
        } else {
            TypeRepr::Unit
        };
        
        // Parse function body
        self.expect(TokenKind::LBrace)?;
        
        // Create a proper Block structure
        let mut statements = Vec::new();
        let mut expr = Box::new(aether_core::Expr::Literal(aether_core::Literal::Unit));
        
        // Parse statements in the body
        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RBrace) {
            if let Some(ref token) = self.current_token {
                match &token.kind {
                    TokenKind::If => {
                        self.advance(); // consume 'if'
                        let condition = self.parse_simple_expr()?;
                        // Parse then-block
                        self.expect(TokenKind::LBrace)?;
                        let mut then_stmts = Vec::new();
                        while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RBrace) {
                            let e = self.parse_simple_expr()?;
                            then_stmts.push(aether_core::Stmt::ExprStmt(Box::new(e)));
                            // consume optional semicolon
                            if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Semicolon) {
                                self.advance();
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                        // Parse optional else-block
                        let else_stmts = if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Else) {
                            self.advance(); // consume 'else'
                            self.expect(TokenKind::LBrace)?;
                            let mut stmts = Vec::new();
                            while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RBrace) {
                                let e = self.parse_simple_expr()?;
                                stmts.push(aether_core::Stmt::ExprStmt(Box::new(e)));
                                if self.current_token.as_ref().map_or(false, |t| t.kind == TokenKind::Semicolon) {
                                    self.advance();
                                }
                            }
                            self.expect(TokenKind::RBrace)?;
                            Some(stmts)
                        } else {
                            None
                        };
                        let if_expr = aether_core::Expr::If(Box::new(condition), then_stmts, else_stmts);
                        statements.push(aether_core::Stmt::ExprStmt(Box::new(if_expr)));
                    }
                    TokenKind::Let => {
                        // Parse let statement: let x: Int = 42;
                        self.advance(); // consume 'let'
                        
                        if let Some(ref name_token) = self.current_token {
                            if let TokenKind::Ident(var_name) = &name_token.kind {
                                let var_name = var_name.clone(); // clone to avoid borrow issues
                                self.advance(); // consume variable name
                                
                                // Expect colon and type
                                self.expect(TokenKind::Colon)?;
                                let var_type = if let Some(ref type_token) = self.current_token {
                                    match &type_token.kind {
                                        TokenKind::Ident(type_name) => {
                                            match type_name.as_str() {
                                                "Int" => aether_core::TypeRepr::Int,
                                                "Float" => aether_core::TypeRepr::Float,
                                                "Bool" => aether_core::TypeRepr::Bool,
                                                "String" => aether_core::TypeRepr::String,
                                                "Unit" => aether_core::TypeRepr::Unit,
                                                _ => aether_core::TypeRepr::Unit,
                                            }
                                        }
                                        _ => aether_core::TypeRepr::Unit,
                                    }
                                } else {
                                    aether_core::TypeRepr::Unit
                                };
                                self.advance(); // consume type
                                
                                // Expect equals
                                self.expect(TokenKind::Assign)?;
                                
                                // Parse expression (including function calls)
                                let init_expr = if let Some(expr_token) = self.current_token.clone() {
                                    match expr_token.kind {
                                        TokenKind::IntLit(value) => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::Int(value))
                                        }
                                        TokenKind::FloatLit(value) => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::Float(value))
                                        }
                                        TokenKind::True => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::Bool(true))
                                        }
                                        TokenKind::False => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::Bool(false))
                                        }
                                        TokenKind::StringLit(value) => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::String(value))
                                        }
                                        TokenKind::Ident(name) => {
                                            // Could be a variable or function call
                                            let var_expr = aether_core::Expr::Ident(name.clone());
                                            self.advance();
                                            
                                            // Check for function call
                                            if let Some(ref next_token) = self.current_token {
                                                if next_token.kind == TokenKind::LParen {
                                                    self.advance(); // consume (
                                                    
                                                    // Parse arguments
                                                    let mut args = Vec::new();
                                                    let mut first_arg = true;
                                                    
                                                    while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RParen) {
                                                        if !first_arg {
                                                            // Expect comma
                                                            if let Some(ref comma_token) = self.current_token {
                                                                if comma_token.kind == TokenKind::Comma {
                                                                    self.advance(); // consume ,
                                                                } else {
                                                                    break;
                                                                }
                                                            } else {
                                                                break;
                                                            }
                                                        }
                                                        first_arg = false;
                                                        
                                                        // Parse argument expression
                                                        args.push(self.parse_simple_expr()?);
                                                    }
                                                    
                                                    // Expect closing parenthesis
                                                    self.expect(TokenKind::RParen)?;
                                                    
                                                    // Create function call expression
                                                    aether_core::Expr::Call(Box::new(var_expr), args)
                                                } else {
                                                    var_expr
                                                }
                                            } else {
                                                var_expr
                                            }
                                        }
                                        _ => {
                                            self.advance();
                                            aether_core::Expr::Literal(aether_core::Literal::Unit)
                                        }
                                    }
                                } else {
                                    aether_core::Expr::Literal(aether_core::Literal::Unit)
                                };
                                
                                // Expect semicolon
                                self.expect(TokenKind::Semicolon)?;
                                
                                // Create let statement
                                let let_stmt = aether_core::Stmt::Let(
                                    var_name.clone(),
                                    Some(var_type),
                                    Box::new(init_expr),
                                );
                                statements.push(let_stmt);
                            }
                        }
                    }
                    TokenKind::Import => {
                        let import_stmt = self.parse_import_stmt()?;
                        statements.push(aether_core::Stmt::Import(import_stmt));
                    }
                    TokenKind::IntLit(value) => {
                        // This is the return expression or start of binary expression
                        let lit_expr = aether_core::Expr::Literal(aether_core::Literal::Int(*value));
                        self.advance();
                        
                        // Check for binary operator
                        if let Some(ref op_token) = self.current_token {
                            let op = match op_token.kind {
    TokenKind::Plus  => Some(aether_core::BinaryOp::Add),
    TokenKind::Minus => Some(aether_core::BinaryOp::Sub),
    TokenKind::Star  => Some(aether_core::BinaryOp::Mul),
    TokenKind::EqEq  => Some(aether_core::BinaryOp::Equal),
    TokenKind::NotEq => Some(aether_core::BinaryOp::NotEqual),
    TokenKind::Lt    => Some(aether_core::BinaryOp::Less),
    TokenKind::Gt    => Some(aether_core::BinaryOp::Greater),
    _ => None,
};
if let Some(bin_op) = op {
    self.advance();
    let rhs = self.parse_simple_expr()?;
    expr = Box::new(aether_core::Expr::Binary(Box::new(lit_expr), bin_op, Box::new(rhs)));
} else {
    expr = Box::new(lit_expr);
}
                        } else {
                            expr = Box::new(lit_expr);
                        }
                    }
                    TokenKind::Ident(name) => {
                        // Variable reference, function call, or start of binary expression
                        let var_expr = aether_core::Expr::Ident(name.clone());
                        self.advance();
                        
                        // Check for function call
                        if let Some(ref next_token) = self.current_token {
                            if next_token.kind == TokenKind::LParen {
                                self.advance(); // consume (
                                
                                // Parse arguments
                                let mut args = Vec::new();
                                let mut first_arg = true;
                                
                                while self.current_token.as_ref().map_or(false, |t| t.kind != TokenKind::RParen) {
                                    if !first_arg {
                                        // Expect comma
                                        if let Some(ref comma_token) = self.current_token {
                                            if comma_token.kind == TokenKind::Comma {
                                                self.advance(); // consume ,
                                            } else {
                                                break;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                    first_arg = false;
                                    
                                    // Parse argument expression
                                    args.push(self.parse_simple_expr()?);
                                }
                                
                                // Expect closing parenthesis
                                self.expect(TokenKind::RParen)?;
                                
                                // Create function call expression
                                expr = Box::new(aether_core::Expr::Call(
                                    Box::new(var_expr),
                                    args
                                ));
                            } else {
                                let op = match next_token.kind {
                                    TokenKind::Plus  => Some(aether_core::BinaryOp::Add),
                                    TokenKind::Minus => Some(aether_core::BinaryOp::Sub),
                                    TokenKind::Star  => Some(aether_core::BinaryOp::Mul),
                                    TokenKind::EqEq  => Some(aether_core::BinaryOp::Equal),
                                    TokenKind::NotEq => Some(aether_core::BinaryOp::NotEqual),
                                    TokenKind::Lt    => Some(aether_core::BinaryOp::Less),
                                    TokenKind::Gt    => Some(aether_core::BinaryOp::Greater),
                                    _ => None,
                                };
                                if let Some(bin_op) = op {
                                    self.advance(); // consume operator
                                    let rhs = self.parse_simple_expr()?;
                                    expr = Box::new(aether_core::Expr::Binary(Box::new(var_expr), bin_op, Box::new(rhs)));
                                } else {
                                    expr = Box::new(var_expr);
                                }
                            }
                        } else {
                            expr = Box::new(var_expr);
                        }
                    }
                    TokenKind::FloatLit(value) => {
                        expr = Box::new(aether_core::Expr::Literal(
                            aether_core::Literal::Float(*value)
                        ));
                        self.advance();
                    }
                    TokenKind::True => {
                        expr = Box::new(aether_core::Expr::Literal(
                            aether_core::Literal::Bool(true)
                        ));
                        self.advance();
                    }
                    TokenKind::False => {
                        expr = Box::new(aether_core::Expr::Literal(
                            aether_core::Literal::Bool(false)
                        ));
                        self.advance();
                    }
                    TokenKind::StringLit(value) => {
                        expr = Box::new(aether_core::Expr::Literal(
                            aether_core::Literal::String(value.clone())
                        ));
                        self.advance();
                    }
                    _ => {
                        // Skip unknown tokens for now
                        self.advance();
                    }
                }
            }
        }
        
                
        self.expect(TokenKind::RBrace)?;
        
        // Create a proper Block structure
        let body = aether_core::Block {
            statements,
            expr,
        };
        
        Ok(FnDecl {
            name,
            provenance,
            type_params: Vec::new(),
            params,
            return_type,
            effects: Vec::new(),
            body,
            based: BasedAnnotation::new(),
        })
    }
}

#[derive(Debug)]
enum TopLevelDecl {
    Statement(aether_core::Stmt),
    Import(ImportDecl),
    Extern(ExternDecl),
    Type(TypeDecl),
    Effect(EffectDecl),
    Function(FnDecl),
}


/// Public API
pub fn parse(input: &str) -> ParseResult {
    let mut parser = Parser::new(input);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic_tokens() {
        let mut lexer = Lexer::new("fn main() {}");
        
        // fn
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::Fn);
        
        // main
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::Ident("main".to_string()));
        
        // (
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::LParen);
        
        // )
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::RParen);
        
        // {
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::LBrace);
        
        // }
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::RBrace);
    }
    
    #[test]
    fn test_lexer_numbers() {
        let mut lexer = Lexer::new("42 3.14");
        
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::IntLit(42));
        
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::FloatLit(3.14));
    }
    
    #[test]
    fn test_lexer_strings() {
        let mut lexer = Lexer::new("\"hello world\"");
        
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::StringLit("hello world".to_string()));
    }
    
    #[test]
    fn test_lexer_provenance_tag() {
        let mut lexer = Lexer::new("@prov");
        
        let token = lexer.next_token().unwrap();
        assert_eq!(token.kind, TokenKind::AtProv);
    }
    
    #[test]
    fn test_parser_basic_function() {
        let result = parse("fn main() {}");
        
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.ast.functions.len(), 1);
        assert_eq!(result.ast.functions[0].name, "main");
    }
    
    #[test]
    fn test_extern_requires_provenance() {
        let result = parse("extern foo: Unit;");
        
        assert!(result.errors.len() > 0);
        match &result.errors[0] {
            ParserError::MissingProvenanceTag(_) => (),
            _ => panic!("Expected MissingProvenanceTag error"),
        }
    }
}
