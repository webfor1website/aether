use std::collections::HashMap;
use aether_ir::expr::{IrExpr, IrExprKind, BinOpKind, UnaryOpKind, ProvId, SYNTHETIC_PROV};
use aether_ir::stmt::{IrStmt, IrStmtKind};
use aether_ir::module::{IrModule, IrFunction};
use aether_prov_store::ProvStore;
use crate::env::Env;
use crate::error::InterpError;
use crate::value::{Value, ValueKind};
use crate::builtins;

pub struct Interpreter {
    env: Env,
    functions: HashMap<String, IrFunction>,
    pub store: ProvStore,
    call_depth: usize,
}

impl Interpreter {
    pub fn new(store: ProvStore) -> Self {
        Self {
            env: Env::new(),
            functions: HashMap::new(),
            store,
            call_depth: 0,
        }
    }

    /// Load all functions from a module into the interpreter's registry.
    /// Call this before `run_main`.
    pub fn load_module(&mut self, module: &IrModule) {
        for func in &module.functions {
            self.functions.insert(func.name.clone(), func.clone());
        }
    }

    /// Entry point — finds `main` and runs it.
    pub fn run_main(&mut self, file_path: &str) -> Result<(Value, f64, f64), InterpError> {
        self.store.begin_session(file_path)
            .map_err(|e| InterpError::ProvStore(e.to_string()))?;

        let result = self.call_function("main", vec![]);

        let trust = self.store.end_session()
            .map_err(|e| InterpError::ProvStore(e.to_string()))?;

        let weighted_trust = self.store.weighted_trust_score()
            .map_err(|e| InterpError::ProvStore(e.to_string()))?;

        let flat_trust = self.store.flat_trust_score()
            .map_err(|e| InterpError::ProvStore(e.to_string()))?;

        // Display both scores when they differ
        if (weighted_trust - flat_trust).abs() > 0.01 {
            eprintln!("\n[aether] trust score: {:.2} (weighted) / {:.2} (flat)", weighted_trust, flat_trust);
        } else {
            eprintln!("\n[aether] trust score: {:.2}", weighted_trust);
        }

        result.map(|v| (v, weighted_trust, flat_trust))
    }

    // -----------------------------------------------------------------------
    // Function calls
    // -----------------------------------------------------------------------

    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, InterpError> {
        if let Some(builtin) = crate::builtins::lookup(name) {
            return builtin(args);
        }

        let func = self.functions.get(name)
            .ok_or_else(|| InterpError::UndefinedFunction(name.to_string()))?
            .clone();

        // Handle extern functions
        if func.is_extern {
            eprintln!("[aether] extern fn `{}` called — returning zero value (not linked)", name);
            
            // Record extern function call with current call depth
            self.store.record_function_call(name, self.call_depth)
                .map_err(|e| InterpError::ProvStore(e.to_string()))?;
            
            // Return placeholder value based on return type
            let placeholder = match func.return_type {
                aether_ir::module::IrType::Int => Value::int(0, func.prov_id),
                aether_ir::module::IrType::Bool => Value::bool_(false, func.prov_id),
                aether_ir::module::IrType::Unit => Value::unit(func.prov_id),
                aether_ir::module::IrType::Float => Value::float(0.0, func.prov_id),
                aether_ir::module::IrType::String => Value::string("".to_string(), func.prov_id),
                _ => Value::unit(func.prov_id), // Default fallback
            };
            
            return Ok(placeholder);
        }

        // Record function call with current call depth
        self.store.record_function_call(name, self.call_depth)
            .map_err(|e| InterpError::ProvStore(e.to_string()))?;

        // Increment call depth for nested calls
        self.call_depth += 1;

        self.env.push_scope();
        for (param, arg) in func.params.iter().zip(args.into_iter()) {
            self.env.define(&param.name, arg);
        }

        let result = self.exec_body(&func.body, func.return_expr.as_ref());

        self.env.pop_scope();   // MUST be AFTER exec_body

        // Decrement call depth after function completes
        self.call_depth -= 1;

        match result {
            Err(InterpError::Return(v)) => Ok(v),
            Ok(v) => Ok(v),        // expression bodies
            Err(e) => Err(e),
        }
    }

    fn exec_body(
        &mut self,
        stmts: &[IrStmt],
        return_expr: Option<&IrExpr>,
    ) -> Result<Value, InterpError> {
        if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] exec_body: {} statements, return_expr: {:?}", stmts.len(), return_expr.is_some()); }
        
        // Execute statements first
        for stmt in stmts {
            self.exec_stmt(stmt)?;
        }

        // Then evaluate return expression if present
        if let Some(expr) = return_expr {
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] exec_body: evaluating return expression"); }
            self.eval_expr(expr)
        } else {
            if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] exec_body: no return expression, returning unit"); }
            Ok(Value::unit(SYNTHETIC_PROV))
        }
    }

    // -----------------------------------------------------------------------
    // Statement execution
    // -----------------------------------------------------------------------

    fn exec_stmt(&mut self, stmt: &IrStmt) -> Result<(), InterpError> {
        match &stmt.kind {
            IrStmtKind::Let { name, value, is_shadow } => {
                let val = self.eval_expr(value)?;
                if *is_shadow {
                    self.env.shadow(name, val);
                } else {
                    self.env.define(name, val);
                }
            }

            IrStmtKind::Expr(expr) => {
                self.eval_expr(expr)?;
            }

            IrStmtKind::Return(maybe_expr) => {
                let val = match maybe_expr {
                    Some(e) => self.eval_expr(e)?,
                    None    => Value::unit(stmt.prov_id),
                };
                // Use error as control flow (standard Rust interpreter pattern)
                return Err(InterpError::Return(val));
            }

            IrStmtKind::While { condition, body } => {
                loop {
                    let cond = self.eval_expr(condition)?;
                    match cond.kind {
                        ValueKind::Bool(false) => break,
                        ValueKind::Bool(true)  => {
                            self.env.push_scope();
                            for s in body { self.exec_stmt(s)?; }
                            self.env.pop_scope();
                        }
                        _ => return Err(InterpError::TypeMismatch {
                            expected: "Bool".into(), got: "other".into(),
                            prov_id: cond.prov_id,
                        }),
                    }
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Expression evaluation
    // -----------------------------------------------------------------------

    pub fn eval_expr(&mut self, expr: &IrExpr) -> Result<Value, InterpError> {
        let prov_id = expr.prov_id;

        match &expr.kind {
            // Literals — prov_id comes directly from AST node
            IrExprKind::IntLit(n)    => Ok(Value::int(*n, prov_id)),
            IrExprKind::FloatLit(f)  => Ok(Value::float(*f, prov_id)),
            IrExprKind::BoolLit(b)   => Ok(Value::bool_(*b, prov_id)),
            IrExprKind::StringLit(s) => Ok(Value::string(s.clone(), prov_id)),
            IrExprKind::Unit         => Ok(Value::unit(prov_id)),

            // Variable lookup
            IrExprKind::Var(name) => {
                self.env.get(name).cloned()
            }

            // Binary operations
            IrExprKind::BinOp { op, lhs, rhs } => {
                self.eval_binop(*op, lhs, rhs, prov_id)
            }

            // Unary operations
            IrExprKind::UnaryOp { op, operand } => {
                self.eval_unaryop(*op, operand, prov_id)
            }

            // Function call
            IrExprKind::Call { callee, args } => {
                self.eval_call(callee, args, prov_id)
            }

            // If-else
            IrExprKind::IfElse { condition, then_branch, else_branch } => {
                let cond = self.eval_expr(condition)?;
                if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] If-else condition evaluated: {:?}", cond); }
                match cond.kind {
                    ValueKind::Bool(true)  => {
                        if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Taking then branch"); }
                        self.eval_expr(then_branch)
                    },
                    ValueKind::Bool(false) => {
                        if std::env::var("AETHER_DEBUG").is_ok() { eprintln!("[DEBUG] Taking else branch"); }
                        self.eval_expr(else_branch)
                    },
                    _ => Err(InterpError::TypeMismatch {
                        expected: "Bool".into(), got: "other".into(), prov_id: cond.prov_id
                    }),
                }
            }

            // Block
            IrExprKind::Block { stmts, result } => {
                self.env.push_scope();
                for stmt in stmts { self.exec_stmt(stmt)?; }
                let val = match result {
                    Some(e) => self.eval_expr(e)?,
                    None    => Value::unit(prov_id),
                };
                self.env.pop_scope();
                Ok(val)
            }

            // Struct literal
            IrExprKind::StructLit { name, fields } => {
                let mut evaluated = HashMap::new();
                for (field_name, field_expr) in fields {
                    evaluated.insert(field_name.clone(), self.eval_expr(field_expr)?);
                }
                Ok(Value::new(ValueKind::Struct {
                    name: name.clone(),
                    fields: evaluated,
                }, prov_id))
            }

            // Field access
            IrExprKind::FieldAccess { object, field } => {
                let obj_val = self.eval_expr(object)?;
                match obj_val.kind {
                    ValueKind::Struct { fields, .. } => {
                        fields.get(field).cloned()
                            .ok_or_else(|| InterpError::UndefinedVar(field.clone()))
                    }
                    _ => Err(InterpError::TypeMismatch {
                        expected: "Struct".into(), got: "other".into(), prov_id: obj_val.prov_id
                    }),
                }
            }
        }
    }

    fn eval_binop(
        &mut self,
        op: BinOpKind,
        lhs: &IrExpr,
        rhs: &IrExpr,
        prov_id: ProvId,
    ) -> Result<Value, InterpError> {
        // Evaluate both sides in current scope
        let l = self.eval_expr(lhs)?;
        let r = self.eval_expr(rhs)?;

        match (op, &l.kind, &r.kind) {
            // Integer arithmetic
            (BinOpKind::Add, ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::int(a + b, prov_id)),
            (BinOpKind::Sub, ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::int(a - b, prov_id)),
            (BinOpKind::Mul, ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::int(a * b, prov_id)),
            (BinOpKind::Div, ValueKind::Int(a), ValueKind::Int(b)) => {
                if *b == 0 { return Err(InterpError::DivisionByZero { prov_id }); }
                Ok(Value::int(a / b, prov_id))
            }
            (BinOpKind::Mod, ValueKind::Int(a), ValueKind::Int(b)) => {
                if *b == 0 { return Err(InterpError::DivisionByZero { prov_id }); }
                Ok(Value::int(a % b, prov_id))
            }

            // Float arithmetic
            (BinOpKind::Add, ValueKind::Float(a), ValueKind::Float(b)) => Ok(Value::float(a + b, prov_id)),
            (BinOpKind::Sub, ValueKind::Float(a), ValueKind::Float(b)) => Ok(Value::float(a - b, prov_id)),
            (BinOpKind::Mul, ValueKind::Float(a), ValueKind::Float(b)) => Ok(Value::float(a * b, prov_id)),
            (BinOpKind::Div, ValueKind::Float(a), ValueKind::Float(b)) => Ok(Value::float(a / b, prov_id)),

            // String concat
            (BinOpKind::Add, ValueKind::Str(a), ValueKind::Str(b)) => {
                Ok(Value::string(format!("{}{}", a, b), prov_id))
            }

            // Comparisons — Int
            (BinOpKind::Eq,    ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a == b, prov_id)),
            (BinOpKind::NotEq, ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a != b, prov_id)),
            (BinOpKind::Lt,    ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a < b, prov_id)),
            (BinOpKind::LtEq,  ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a <= b, prov_id)),
            (BinOpKind::Gt,    ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a > b, prov_id)),
            (BinOpKind::GtEq,  ValueKind::Int(a), ValueKind::Int(b)) => Ok(Value::bool_(a >= b, prov_id)),

            // Boolean logic
            (BinOpKind::And, ValueKind::Bool(a), ValueKind::Bool(b)) => Ok(Value::bool_(*a && *b, prov_id)),
            (BinOpKind::Or,  ValueKind::Bool(a), ValueKind::Bool(b)) => Ok(Value::bool_(*a || *b, prov_id)),

            // Bool equality
            (BinOpKind::Eq,    ValueKind::Bool(a), ValueKind::Bool(b)) => Ok(Value::bool_(a == b, prov_id)),
            (BinOpKind::NotEq, ValueKind::Bool(a), ValueKind::Bool(b)) => Ok(Value::bool_(a != b, prov_id)),

            _ => Err(InterpError::TypeMismatch {
                expected: format!("compatible types for {:?}", op),
                got: format!("{:?} and {:?}", l.kind, r.kind),
                prov_id,
            }),
        }
    }

    fn eval_unaryop(
        &mut self,
        op: UnaryOpKind,
        operand: &IrExpr,
        prov_id: ProvId,
    ) -> Result<Value, InterpError> {
        let val = self.eval_expr(operand)?;
        match (op, &val.kind) {
            (UnaryOpKind::Neg, ValueKind::Int(n))   => Ok(Value::int(-n, prov_id)),
            (UnaryOpKind::Neg, ValueKind::Float(f)) => Ok(Value::float(-f, prov_id)),
            (UnaryOpKind::Not, ValueKind::Bool(b))  => Ok(Value::bool_(!b, prov_id)),
            _ => Err(InterpError::TypeMismatch {
                expected: format!("compatible type for {:?}", op),
                got: format!("{:?}", val.kind),
                prov_id,
            }),
        }
    }

    fn eval_call(
        &mut self,
        callee: &str,
        args: &[IrExpr],
        _prov_id: ProvId,
    ) -> Result<Value, InterpError> {
        // Evaluate all arguments before the call
        let mut arg_vals = Vec::with_capacity(args.len());
        for arg in args {
            arg_vals.push(self.eval_expr(arg)?);
        }

        // Check builtins first
        if let Some(builtin) = builtins::lookup(callee) {
            return builtin(arg_vals);
        }

        self.call_function(callee, arg_vals)
    }
}
