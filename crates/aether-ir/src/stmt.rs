use serde::{Deserialize, Serialize};
use crate::expr::{IrExpr, ProvId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrStmt {
    pub kind: IrStmtKind,
    pub prov_id: ProvId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrStmtKind {
    /// let name: Type = expr
    /// Aether has no `var` — all bindings are immutable by default
    /// Re-binding with `shadow` keyword is explicit
    Let {
        name: String,
        value: IrExpr,
        is_shadow: bool,
    },

    /// Expression used as a statement (side effects)
    Expr(IrExpr),

    /// Return from function
    Return(Option<IrExpr>),

    /// while condition { body }
    While {
        condition: IrExpr,
        body: Vec<IrStmt>,
    },
}
