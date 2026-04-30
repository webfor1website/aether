use serde::{Deserialize, Serialize};
use crate::expr::{IrExpr, ProvId};
use crate::stmt::IrStmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrModule {
    pub name: String,
    pub functions: Vec<IrFunction>,
    pub prov_id: ProvId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: IrType,
    pub effects: Vec<String>,       // effect names from the checker
    pub body: Vec<IrStmt>,
    pub return_expr: Option<IrExpr>,
    pub prov_id: ProvId,
    pub is_extern: bool,             // true for extern functions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParam {
    pub name: String,
    pub ty: IrType,
    pub prov_id: ProvId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrType {
    Int,
    Float,
    Bool,
    String,
    Unit,
    Named(String),
    Function {
        params: Vec<IrType>,
        ret: Box<IrType>,
        effects: Vec<String>,
    },
}
