// Aether IR Lowering - Convert typed AST to intermediate representation
// 
// This crate implements lowering from the checker's TypedProgram to the
// interpreter's IrModule. It handles type conversion and generates the
// intermediate representation that the interpreter can execute.
//
// Added: Function call lowering to IrExprKind::Call

use crate::module::{IrModule, IrFunction, IrParam, IrType};
use crate::stmt::{IrStmt, IrStmtKind};
use crate::expr::{IrExpr, IrExprKind, BinOpKind, SYNTHETIC_PROV};

/// Errors that can occur during lowering
#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("unsupported AST node: {0}")]
    Unsupported(String),
    #[error("internal lowering error: {0}")]
    Internal(String),
}

pub type LowerResult<T> = Result<T, LowerError>;

/// Entry point. Call this after the checker has validated the AST.
pub fn lower_module(ast: &aether_checker::TypedProgram) -> LowerResult<IrModule> {
    let mut functions = vec![];

    // Lower regular functions
    for fn_decl in &ast.functions {
        // Lower statements from the function body
        let mut body = Vec::new();
        for stmt in &fn_decl.body.statements {
            body.push(lower_stmt(stmt)?);
        }
        
        // Lower the return expression
        let return_expr = lower_expr(&fn_decl.body.expr)?;
        
        // Lower function parameters
        let params: Result<Vec<_>, _> = fn_decl.params.iter().map(|(name, type_repr, _symbol_info)| {
            let ir_type = match type_repr {
                aether_core::TypeRepr::Unit => IrType::Unit,
                aether_core::TypeRepr::Int => IrType::Int,
                aether_core::TypeRepr::Float => IrType::Float,
                aether_core::TypeRepr::Bool => IrType::Bool,
                aether_core::TypeRepr::String => IrType::String,
                _ => todo!("lower_module: unsupported param type {:?}", type_repr),
            };
            Ok(IrParam {
                name: name.clone(),
                ty: ir_type,
                prov_id: SYNTHETIC_PROV,
            })
        }).collect();
        let params = params?;
        
        // Lower return type
        let return_type = match &fn_decl.return_type {
            aether_core::TypeRepr::Unit => IrType::Unit,
            aether_core::TypeRepr::Int => IrType::Int,
            aether_core::TypeRepr::Float => IrType::Float,
            aether_core::TypeRepr::Bool => IrType::Bool,
            aether_core::TypeRepr::String => IrType::String,
            _ => todo!("lower_module: unsupported return type {:?}", fn_decl.return_type),
        };
        
        functions.push(IrFunction {
            name: fn_decl.name.clone(),
            params,
            return_type,
            effects: fn_decl.effects.clone(),
            body,
            return_expr: Some(return_expr),
            prov_id: SYNTHETIC_PROV,
            is_extern: false,
        });
    }
    
    // Lower extern functions
    for extern_decl in &ast.externs {
        // Lower extern function parameters (empty for externs)
        let params = vec![];
        
        // Lower return type
        let return_type = match &extern_decl.type_expr {
            aether_core::TypeRepr::Function(_, ret, _) => {
                // For function types, we need to extract the return type
                match &**ret {
                    aether_core::TypeRepr::Unit => IrType::Unit,
                    aether_core::TypeRepr::Int => IrType::Int,
                    aether_core::TypeRepr::Float => IrType::Float,
                    aether_core::TypeRepr::Bool => IrType::Bool,
                    aether_core::TypeRepr::String => IrType::String,
                    _ => todo!("lower_module: unsupported extern return type {:?}", extern_decl.type_expr),
                }
            }
            _ => todo!("lower_module: unsupported extern type {:?}", extern_decl.type_expr),
        };
        
        functions.push(IrFunction {
            name: extern_decl.name.clone(),
            params,
            return_type,
            effects: vec![], // Extern functions don't have effects in this context
            body: vec![], // No body for extern functions
            return_expr: None, // No return expression for extern functions
            prov_id: SYNTHETIC_PROV,
            is_extern: true,
        });
    }

    Ok(IrModule {
        name: "main_module".to_string(),
        functions,
        prov_id: SYNTHETIC_PROV,
    })
}


fn lower_stmt(stmt: &aether_checker::TypedStatement) -> LowerResult<IrStmt> {
    match stmt {
        aether_checker::TypedStatement::Let(name, _type, expr, _symbol_info) => {
            let lowered_expr = lower_expr(expr)?;
            Ok(IrStmt {
                kind: IrStmtKind::Let {
                    name: name.clone(),
                    value: lowered_expr,
                    is_shadow: false,
                },
                prov_id: SYNTHETIC_PROV,
            })
        }
        aether_checker::TypedStatement::Shadow(_name, _type, _expr, _symbol_info) => {
            return Err(LowerError::Unsupported("Shadow bindings not yet supported".to_string()));
        }
        aether_checker::TypedStatement::Return(_expr) => {
            return Err(LowerError::Unsupported("Return statements not yet supported".to_string()));
        }
        aether_checker::TypedStatement::ExprStmt(_expr) => {
            return Err(LowerError::Unsupported("Expression statements not yet supported".to_string()));
        }
    }
}

pub fn lower_expr(expr: &aether_checker::TypedExpr) -> LowerResult<IrExpr> {
    if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] lower_expr called with: {:?}", std::mem::discriminant(expr)); }
    match expr {
        aether_checker::TypedExpr::Literal(literal, _type) => {
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Processing Literal"); }
            let ir_expr = match literal {
                aether_core::Literal::Int(value) => IrExprKind::IntLit(*value),
                aether_core::Literal::Float(value) => IrExprKind::FloatLit(*value),
                aether_core::Literal::Bool(value) => IrExprKind::BoolLit(*value),
                aether_core::Literal::String(value) => IrExprKind::StringLit(value.clone()),
                aether_core::Literal::Unit => IrExprKind::Unit,
                aether_core::Literal::Record(_) => {
                    return Err(LowerError::Unsupported("Record literals not yet supported".to_string()));
                }
                aether_core::Literal::Option(_) => {
                    return Err(LowerError::Unsupported("Option literals not yet supported".to_string()));
                }
                aether_core::Literal::None => {
                    return Err(LowerError::Unsupported("None literal not yet supported".to_string()));
                }
            };
            
            Ok(IrExpr {
                kind: ir_expr,
                prov_id: SYNTHETIC_PROV,
            })
        }
        aether_checker::TypedExpr::Ident(name, _type, _symbol_info) => {
            Ok(IrExpr {
                kind: IrExprKind::Var(name.clone()),
                prov_id: SYNTHETIC_PROV,
            })
        }
        aether_checker::TypedExpr::Binary(left, op, right, _type) => {
            let lowered_left = lower_expr(left)?;
            let lowered_right = lower_expr(right)?;
            
            let bin_op = match op {
                aether_core::BinaryOp::Add => BinOpKind::Add,
                aether_core::BinaryOp::Sub => BinOpKind::Sub,
                aether_core::BinaryOp::Mul => BinOpKind::Mul,
                aether_core::BinaryOp::Div => BinOpKind::Div,
                aether_core::BinaryOp::Equal => BinOpKind::Eq,
                aether_core::BinaryOp::NotEqual => BinOpKind::NotEq,
                aether_core::BinaryOp::Less => BinOpKind::Lt,
                aether_core::BinaryOp::Greater => BinOpKind::Gt,
                aether_core::BinaryOp::LessEqual => BinOpKind::LtEq,
                aether_core::BinaryOp::GreaterEqual => BinOpKind::GtEq,
                aether_core::BinaryOp::And => BinOpKind::And,
                aether_core::BinaryOp::Or => BinOpKind::Or,
            };
            
            Ok(IrExpr {
                kind: IrExprKind::BinOp {
                    op: bin_op,
                    lhs: Box::new(lowered_left),
                    rhs: Box::new(lowered_right),
                },
                prov_id: SYNTHETIC_PROV,
            })
        }
        aether_checker::TypedExpr::Unary(_op, _expr, _type) => {
            return Err(LowerError::Unsupported("Unary expressions not yet supported".to_string()));
        }
        aether_checker::TypedExpr::Call(func, args, _type) => {
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Processing Call"); }
            let lowered_args: Result<Vec<_>, _> = args.iter().map(|arg| lower_expr(arg)).collect();
            let lowered_args = lowered_args?;
            
            // For now, treat function calls as builtin operations
            // Extract function name from the callee expression
            let func_name = match &**func {
                aether_checker::TypedExpr::Ident(name, _, _) => name.clone(),
                _ => "unknown".to_string(),
            };
            
            Ok(IrExpr {
                kind: IrExprKind::Call {
                    callee: func_name,
                    args: lowered_args,
                },
                prov_id: SYNTHETIC_PROV,
            })
        }
        aether_checker::TypedExpr::Field(_expr, _field, _type) => {
            return Err(LowerError::Unsupported("Field access not yet supported".to_string()));
        }
        aether_checker::TypedExpr::If(cond, then_block, else_block, _type) => {
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Lowering if-else expression"); }
            let lowered_cond = lower_expr(cond)?;
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Lowered condition: {:?}", lowered_cond); }
            let lowered_then = lower_expr(&then_block.expr)?;
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Lowered then branch: {:?}", lowered_then); }
            let lowered_else = lower_expr(&else_block.expr)?;
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Lowered else branch: {:?}", lowered_else); }
            
            let result = Ok(IrExpr {
                kind: IrExprKind::IfElse {
                    condition: Box::new(lowered_cond),
                    then_branch: Box::new(lowered_then),
                    else_branch: Box::new(lowered_else),
                },
                prov_id: SYNTHETIC_PROV,
            });
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] If-else lowering complete: {:?}", result); }
            result
        }
        aether_checker::TypedExpr::Match(_expr, _arms, _type) => {
            return Err(LowerError::Unsupported("Match expressions not yet supported".to_string()));
        }
    }
}
