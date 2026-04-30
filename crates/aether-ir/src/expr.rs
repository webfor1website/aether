use serde::{Deserialize, Serialize};

/// Handle into provenance store. u64::MAX = synthetic/compiler-generated.
pub type ProvId = u64;

pub const SYNTHETIC_PROV: ProvId = u64::MAX;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrExpr {
    pub kind: IrExprKind,
    pub prov_id: ProvId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrExprKind {
    // Literals
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    StringLit(String),
    Unit,

    // Variables
    Var(String),

    // Arithmetic & logic (binary)
    BinOp {
        op: BinOpKind,
        lhs: Box<IrExpr>,
        rhs: Box<IrExpr>,
    },

    // Unary
    UnaryOp {
        op: UnaryOpKind,
        operand: Box<IrExpr>,
    },

    // Function call — no implicit anything, args are positional and fully typed
    Call {
        callee: String,
        args: Vec<IrExpr>,
    },

    // If-else — always has an else branch (no implicit unit else = compile error upstream)
    IfElse {
        condition: Box<IrExpr>,
        then_branch: Box<IrExpr>,
        else_branch: Box<IrExpr>,
    },

    // Block — sequence of stmts, final expr is value
    Block {
        stmts: Vec<IrStmt>,
        result: Option<Box<IrExpr>>,
    },

    // Field access (for structs)
    FieldAccess {
        object: Box<IrExpr>,
        field: String,
    },

    // Struct literal
    StructLit {
        name: String,
        fields: Vec<(String, IrExpr)>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BinOpKind {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnaryOpKind {
    Neg,
    Not,
}

// Avoid circular import — inline IrStmt here for Block nodes
// The real IrStmt is in stmt.rs; this re-export keeps things tidy
use crate::stmt::IrStmt;
