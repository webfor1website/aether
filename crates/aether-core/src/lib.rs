use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthorType {
    Human,
    AI(String),
    Transform(String),
}

impl std::fmt::Display for AuthorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthorType::Human => write!(f, "user"),
            AuthorType::AI(_model) => write!(f, "ai"),
            AuthorType::Transform(_pass) => write!(f, "transform"),
        }
    }
}

impl std::str::FromStr for AuthorType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s == "user" {
            Ok(AuthorType::Human)
        } else if s == "ai" {
            Ok(AuthorType::AI("unknown".to_string()))
        } else if let Some(model) = s.strip_prefix("ai:") {
            Ok(AuthorType::AI(model.to_string()))
        } else if let Some(pass) = s.strip_prefix("transform:") {
            Ok(AuthorType::Transform(pass.to_string()))
        } else if s == "claude" {
            Ok(AuthorType::AI("claude".to_string()))
        } else if s == "cursor" {
            Ok(AuthorType::AI("cursor".to_string()))
        } else if s == "grok" {
            Ok(AuthorType::AI("grok".to_string()))
        } else if s == "claude-likely" {
            Ok(AuthorType::AI("claude-likely".to_string()))
        } else if s == "claude-possible" {
            Ok(AuthorType::AI("claude-possible".to_string()))
        } else if s == "cursor-likely" {
            Ok(AuthorType::AI("cursor-likely".to_string()))
        } else if s == "grok-likely" {
            Ok(AuthorType::AI("grok-likely".to_string()))
        } else {
            Err(format!("Invalid author type: {}", s))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenanceTag {
    pub id: Uuid,
    pub author: AuthorType,
    pub model: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub prompt: Option<String>,
    pub confidence: f64,
    pub parents: Vec<Uuid>,
    pub version: String,
}

/// Wellbeing configuration — can be set per-project in .aether-wellbeing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WellbeingConfig {
    /// Max session length before soft stop triggers (default: 90 minutes)
    pub session_limit_minutes: u64,
    /// Cooldown between sessions (default: 480 minutes / 8 hours)  
    pub cooldown_minutes: u64,
    /// Only stop at clean build, not mid-error (default: true)
    pub stop_at_clean_build_only: bool,
    /// Whether to reduce trust score for over-limit code (default: true)
    pub penalize_overlimit_trust: bool,
}

impl Default for WellbeingConfig {
    fn default() -> Self {
        Self {
            session_limit_minutes: 90,
            cooldown_minutes: 480,
            stop_at_clean_build_only: true,
            penalize_overlimit_trust: true,
        }
    }
}

/// Runtime session state — written to .aether-session in project root
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionState {
    /// When this session started (Unix timestamp seconds)
    pub started_at: u64,
    /// When cooldown ends (Unix timestamp seconds), None if no cooldown active
    pub cooldown_until: Option<u64>,
    /// Total minutes spent this session
    pub elapsed_minutes: u64,
    /// Whether session was stopped cleanly (vs interrupted)
    pub clean_stop: bool,
    /// Number of clean builds this session
    pub clean_builds: u64,
}

impl SessionState {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            started_at: now,
            cooldown_until: None,
            elapsed_minutes: 0,
            clean_stop: false,
            clean_builds: 0,
        }
    }

    pub fn elapsed_minutes(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        (now.saturating_sub(self.started_at)) / 60
    }

    pub fn cooldown_active(&self) -> bool {
        if let Some(until) = self.cooldown_until {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return now < until;
        }
        false
    }

    pub fn cooldown_remaining_minutes(&self) -> u64 {
        if let Some(until) = self.cooldown_until {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return until.saturating_sub(now) / 60;
        }
        0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BasedAnnotation {
    pub enabled: bool,
    pub message: Option<String>,
}

impl BasedAnnotation {
    pub fn new() -> Self {
        Self {
            enabled: false,
            message: None,
        }
    }
    
    pub fn enable(mut self, message: String) -> Self {
        self.enabled = true;
        self.message = Some(message);
        self
    }
}

impl ProvenanceTag {
    pub fn new(author: AuthorType, confidence: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            author,
            model: None,
            timestamp: Utc::now(),
            prompt: None,
            confidence: confidence.clamp(0.0, 1.0),
            parents: vec![],
            version: "0.1.0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub provenance: Option<Uuid>,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end, provenance: None }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0, provenance: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TypeRepr {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Option(Box<TypeRepr>),
    Function(Vec<TypeRepr>, Box<TypeRepr>, Vec<std::string::String>),
    Record(Vec<(std::string::String, TypeRepr)>),
    Union(Box<TypeRepr>, Box<TypeRepr>),
    Named(std::string::String, Vec<TypeRepr>),
    TypeVar(std::string::String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolInfo {
    pub name: std::string::String,
    pub type_repr: TypeRepr,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(std::string::String),
    Record(Vec<(std::string::String, TypeRepr)>),
    Option(Box<TypeRepr>),
    Unit,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Pattern {
    Literal(Literal),
    Identifier(std::string::String),
    Record(Vec<std::string::String>),
    Tag(std::string::String, Option<Box<Pattern>>),
    Wildcard,
}

// AST node types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Program {
    pub statements: Vec<Stmt>,
    pub imports: Vec<ImportDecl>,
    pub externs: Vec<ExternDecl>,
    pub types: Vec<TypeDecl>,
    pub effects: Vec<EffectDecl>,
    pub functions: Vec<FnDecl>,
    pub version: std::string::String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportDecl {
    pub module_path: Vec<std::string::String>,
    pub alias: Option<std::string::String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternDecl {
    pub name: std::string::String,
    pub type_expr: TypeRepr,
    pub provenance: ProvenanceTag,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDecl {
    pub name: std::string::String,
    pub type_params: Vec<std::string::String>,
    pub definition: TypeRepr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectDecl {
    pub name: std::string::String,
    pub operations: Vec<(std::string::String, TypeRepr)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FnDecl {
    pub name: std::string::String,
    pub provenance: Option<ProvenanceTag>,
    pub type_params: Vec<std::string::String>,
    pub params: Vec<(std::string::String, TypeRepr)>,
    pub return_type: TypeRepr,
    pub effects: Vec<std::string::String>,
    pub body: Block,
    pub based: BasedAnnotation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportStmt {
    pub path: std::string::String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeclareStmt {
    pub name: std::string::String,
    pub type_expr: TypeRepr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub expr: Box<Expr>,
}

// Expression and statement types for the checker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Ident(std::string::String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Field(Box<Expr>, std::string::String),
    If(Box<Expr>, Vec<Stmt>, Option<Vec<Stmt>>),
    Match(Box<Expr>, Vec<(Pattern, Expr)>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Stmt {
    Let(std::string::String, Option<TypeRepr>, Box<Expr>),
    Shadow(std::string::String, Option<TypeRepr>, Box<Expr>),
    Return(Box<Expr>),
    ExprStmt(Box<Expr>),
    Import(ImportStmt),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize, PartialEq)]
pub enum AetherError {
    // Parser errors E1xxx
    #[error("E1001: Unexpected token '{0}' at {1}, expected {2}")]
    UnexpectedToken(std::string::String, std::string::String, std::string::String),
    #[error("E1002: Unexpected end of file: {0}")]
    UnexpectedEOF(std::string::String),
    #[error("E1003: Missing @prov tag on extern declaration at {0}")]
    MissingProvenanceTag(std::string::String),
    #[error("E1004: Invalid provenance tag: {0}")]
    InvalidProvenanceTag(std::string::String),
    #[error("E1005: Parse error: {0}")]
    ParseError(std::string::String),

    // Checker errors E2xxx
    #[error("E2001: Undefined identifier '{0}' at {1}")]
    UndefinedIdentifier(std::string::String, std::string::String),
    #[error("E2002: Shadow keyword required for variable redeclaration '{0}'")]
    ShadowRequired(std::string::String),
    #[error("E2003: Type mismatch: expected {0}, found {1}")]
    TypeMismatch(std::string::String, std::string::String),
    #[error("E2004: No implicit type coercion allowed")]
    NoImplicitCoercion,

    // Effect errors E2xxx
    #[error("E2011: Function '{0}' calls undeclared effect '{1}'")]
    UndeclaredEffect(std::string::String, std::string::String),
    #[error("E2012: Function '{0}' has unannotated effects in call graph")]
    UnannotatedEffects(std::string::String),

    // Provenance errors E3xxx
    #[error("E3001: Extern declaration missing @prov tag")]
    ExternMissingProvenance,
    #[error("E3002: Silent provenance tag drop detected")]
    SilentTagDrop,
    #[error("E3003: Provenance graph contains cycle")]
    ProvenanceCycle,
    #[error("E3004: Child confidence exceeds parent confidence")]
    ConfidenceViolation,
    #[error("E3005: Transform-authored node cannot be root")]
    TransformRoot,

    // Runtime errors
    #[error("Runtime error: {0}")]
    Runtime(std::string::String),
    #[error("Parse error: {0}")]
    Parse(std::string::String),
    #[error("E4001: Runtime error: {0}")]
    RuntimeError(std::string::String),
    #[error("E4002: Effect not handled: {0}")]
    UnhandledEffect(std::string::String),
    #[error("E4003: Capability not available: {0}")]
    CapabilityUnavailable(std::string::String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintSet {
    pub constraints: Vec<(TypeRepr, TypeRepr)>,
}

impl ConstraintSet {
    pub fn new() -> Self {
        Self { constraints: vec![] }
    }

    pub fn add(&mut self, lhs: TypeRepr, rhs: TypeRepr) {
        self.constraints.push((lhs, rhs));
    }
}

impl Default for ConstraintSet {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolTable {
    pub symbols: HashMap<std::string::String, SymbolInfo>,
    pub shadow_chains: HashMap<std::string::String, Vec<std::string::String>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            shadow_chains: HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: std::string::String, info: SymbolInfo) {
        self.symbols.insert(name, info);
    }

    pub fn get(&self, name: &str) -> Option<&SymbolInfo> {
        self.symbols.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_tag() {
        let tag = ProvenanceTag::new(AuthorType::Human, 1.0);
        assert_eq!(tag.confidence, 1.0);
        assert_eq!(tag.parents.len(), 0);
        assert_eq!(tag.version, "0.1.0");
    }

    #[test]
    fn test_type_repr_equality() {
        let t1 = TypeRepr::Int;
        let t2 = TypeRepr::Int;
        assert_eq!(t1, t2);

        let t3 = TypeRepr::Option(Box::new(TypeRepr::Bool));
        let t4 = TypeRepr::Option(Box::new(TypeRepr::Bool));
        assert_eq!(t3, t4);
    }
}
