use thiserror::Error;
use aether_ir::expr::ProvId;

#[derive(Debug, Error)]
pub enum InterpError {
    #[error("undefined variable: `{0}`")]
    UndefinedVar(String),

    #[error("type mismatch: expected {expected}, got {got} (prov_id={prov_id})")]
    TypeMismatch { expected: String, got: String, prov_id: ProvId },

    #[error("division by zero (prov_id={prov_id})")]
    DivisionByZero { prov_id: ProvId },

    #[error("undefined function: `{0}`")]
    UndefinedFunction(String),

    #[error("wrong number of arguments: `{name}` expects {expected}, got {got}")]
    ArityMismatch { name: String, expected: usize, got: usize },

    #[error("effect violation: function `{func}` has effect `{effect}` not in caller's effect set")]
    EffectViolation { func: String, effect: String },

    #[error("provenance store error: {0}")]
    ProvStore(String),

    #[error("return signal")]  // Not a real error — used for control flow
    Return(crate::value::Value),

    #[error("internal interpreter error: {0}")]
    Internal(String),
}
