use std::collections::HashMap;
use aether_ir::expr::ProvId;
use aether_ir::module::IrFunction;

/// Runtime value — every value carries a prov_id of the expression that produced it.
/// This is the core of provenance-at-runtime.
#[derive(Debug, Clone)]
pub struct Value {
    pub kind: ValueKind,
    pub prov_id: ProvId,
}

impl Value {
    pub fn new(kind: ValueKind, prov_id: ProvId) -> Self {
        Self { kind, prov_id }
    }

    /// Convenience constructors
    pub fn int(n: i64, prov_id: ProvId) -> Self { Self::new(ValueKind::Int(n), prov_id) }
    pub fn float(f: f64, prov_id: ProvId) -> Self { Self::new(ValueKind::Float(f), prov_id) }
    pub fn bool_(b: bool, prov_id: ProvId) -> Self { Self::new(ValueKind::Bool(b), prov_id) }
    pub fn string(s: String, prov_id: ProvId) -> Self { Self::new(ValueKind::Str(s), prov_id) }
    pub fn unit(prov_id: ProvId) -> Self { Self::new(ValueKind::Unit, prov_id) }
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    Struct {
        name: String,
        fields: HashMap<String, Value>,
    },
    /// First-class function value
    Function(IrFunction),
    /// Built-in function (Rust closure)
    Builtin(String),  // name — looked up in builtins registry
}
