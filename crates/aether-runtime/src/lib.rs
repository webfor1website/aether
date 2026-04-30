//! Aether Runtime - Tree-walking interpreter
//! 
//! This crate implements a tree-walking interpreter for Aether programs.
//! All I/O goes through capability handles and every function call is traced.

use aether_core::*;
use aether_checker::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Runtime values that can be produced by evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Record(HashMap<String, RuntimeValue>),
    Union(String, Box<RuntimeValue>),
    Option(Option<Box<RuntimeValue>>),
    Unit,
}

impl RuntimeValue {
    pub fn type_repr(&self) -> TypeRepr {
        match self {
            RuntimeValue::Int(_) => TypeRepr::Int,
            RuntimeValue::Float(_) => TypeRepr::Float,
            RuntimeValue::String(_) => TypeRepr::String,
            RuntimeValue::Bool(_) => TypeRepr::Bool,
            RuntimeValue::Record(fields) => {
                let field_types: Vec<(String, TypeRepr)> = fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.type_repr()))
                    .collect();
                TypeRepr::Record(field_types)
            }
            RuntimeValue::Union(_variant, _) => TypeRepr::Union(Box::new(TypeRepr::Unit), Box::new(TypeRepr::Unit)),
            RuntimeValue::Option(inner) => {
                let inner_type = inner.as_ref().map(|v| v.type_repr()).unwrap_or(TypeRepr::Unit);
                TypeRepr::Option(Box::new(inner_type))
            }
            RuntimeValue::Unit => TypeRepr::Unit,
        }
    }
}

/// Execution trace entry for provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub function_provenance: Uuid,  // Reference to function's ProvenanceTag
    pub input_values: Vec<RuntimeValue>,
    pub output_value: RuntimeValue,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Execution trace containing all function calls
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionTrace {
    pub entries: Vec<TraceEntry>,
}

impl ExecutionTrace {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    
    pub fn add_entry(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }
    
    pub fn get_calls_by_provenance(&self, provenance_id: &Uuid) -> Vec<&TraceEntry> {
        self.entries
            .iter()
            .filter(|entry| &entry.function_provenance == provenance_id)
            .collect()
    }
    
    pub fn get_all_calls(&self) -> &[TraceEntry] {
        &self.entries
    }
}

/// Effect handler trait for swappable I/O
pub trait EffectHandler: Send + Sync {
    fn handle_console(&self, message: &str) -> Result<(), AetherError>;
    fn handle_file_read(&self, path: &str) -> Result<String, AetherError>;
    fn handle_file_write(&self, path: &str, content: &str) -> Result<(), AetherError>;
}

/// Default console effect handler
pub struct DefaultConsoleHandler;

impl EffectHandler for DefaultConsoleHandler {
    fn handle_console(&self, message: &str) -> Result<(), AetherError> {
        println!("{}", message);
        Ok(())
    }
    
    fn handle_file_read(&self, path: &str) -> Result<String, AetherError> {
        std::fs::read_to_string(path)
            .map_err(|e| AetherError::Runtime(format!("Failed to read file '{}': {}", path, e)))
    }
    
    fn handle_file_write(&self, path: &str, content: &str) -> Result<(), AetherError> {
        std::fs::write(path, content)
            .map_err(|e| AetherError::Runtime(format!("Failed to write file '{}': {}", path, e)))
    }
}

/// Capability set defining allowed effects
#[derive(Debug, Clone)]
pub struct CapabilitySet {
    pub allow_console: bool,
    pub allow_file_read: bool,
    pub allow_file_write: bool,
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self {
            allow_console: true,
            allow_file_read: false,
            allow_file_write: false,
        }
    }
}

/// Tree-walking interpreter
pub struct Interpreter {
    capabilities: CapabilitySet,
    effect_handler: Box<dyn EffectHandler>,
    trace: ExecutionTrace,
}

impl Interpreter {
    pub fn new(capabilities: CapabilitySet, effect_handler: Box<dyn EffectHandler>) -> Self {
        Self {
            capabilities,
            effect_handler,
            trace: ExecutionTrace::new(),
        }
    }
    
    pub fn with_default_handler(capabilities: CapabilitySet) -> Self {
        Self::new(capabilities, Box::new(DefaultConsoleHandler))
    }
    
    pub fn execute(&mut self, ast: &TypedAst) -> Result<RuntimeValue, AetherError> {
        // Find the main function or first function
        let main_fn = ast.program.functions
            .iter()
            .find(|f| f.name == "main")
            .or_else(|| ast.program.functions.first())
            .ok_or_else(|| AetherError::Runtime("No function to execute".to_string()))?;
        
        self.execute_function(main_fn, &[])
    }
    
    pub fn execute_function(&mut self, fn_decl: &TypedFnDecl, args: &[RuntimeValue]) -> Result<RuntimeValue, AetherError> {
        let start_time = chrono::Utc::now();
        
        // Execute the function body
        let result = self.execute_block(&fn_decl.body)?;
        
        // Record the function call in trace
        let provenance_id = fn_decl.provenance
            .as_ref()
            .map(|p| p.id)
            .unwrap_or_else(Uuid::new_v4);
        
        let trace_entry = TraceEntry {
            function_provenance: provenance_id,
            input_values: args.to_vec(),
            output_value: result.clone(),
            timestamp: start_time,
        };
        
        self.trace.add_entry(trace_entry);
        
        Ok(result)
    }
    
    fn execute_block(&mut self, block: &TypedBlock) -> Result<RuntimeValue, AetherError> {
        // Execute all statements
        for stmt in &block.statements {
            self.execute_statement(stmt)?;
        }
        
        // Execute and return the block expression
        self.execute_expr(&block.expr)
    }
    
    fn execute_statement(&mut self, stmt: &TypedStatement) -> Result<RuntimeValue, AetherError> {
        match stmt {
            TypedStatement::Let(_, _, expr, _) => {
                self.execute_expr(expr)
            }
            TypedStatement::Shadow(_, _, expr, _) => {
                self.execute_expr(expr)
            }
            TypedStatement::Return(expr) => {
                self.execute_expr(expr)
            }
            TypedStatement::ExprStmt(expr) => {
                self.execute_expr(expr)
            }
        }
    }
    
    fn execute_expr(&mut self, expr: &TypedExpr) -> Result<RuntimeValue, AetherError> {
        match expr {
            TypedExpr::Literal(lit, _) => {
                self.execute_literal(lit)
            }
            TypedExpr::Ident(_, _, _) => {
                // For now, return a placeholder value
                Ok(RuntimeValue::Unit)
            }
            TypedExpr::Binary(left, op, right, _) => {
                let left_val = self.execute_expr(left)?;
                let right_val = self.execute_expr(right)?;
                self.execute_binary_op(op, &left_val, &right_val)
            }
            TypedExpr::Unary(op, operand, _) => {
                let operand_val = self.execute_expr(operand)?;
                self.execute_unary_op(op, &operand_val)
            }
            TypedExpr::Call(callee, args, _) => {
                self.execute_call(callee, args)
            }
            TypedExpr::Field(base, field, _) => {
                let base_val = self.execute_expr(base)?;
                self.execute_field_access(&base_val, field)
            }
            TypedExpr::If(cond, then_block, else_block, _) => {
                let cond_val = self.execute_expr(cond)?;
                if let RuntimeValue::Bool(true) = cond_val {
                    self.execute_block(then_block)
                } else {
                    self.execute_block(else_block)
                }
            }
            TypedExpr::Match(scrutinee, arms, _) => {
                let scrutinee_val = self.execute_expr(scrutinee)?;
                self.execute_match(&scrutinee_val, arms)
            }
        }
    }
    
    fn execute_literal(&self, lit: &Literal) -> Result<RuntimeValue, AetherError> {
        match lit {
            Literal::Int(i) => Ok(RuntimeValue::Int(*i)),
            Literal::Float(f) => Ok(RuntimeValue::Float(*f)),
            Literal::String(s) => Ok(RuntimeValue::String(s.clone())),
            Literal::Bool(b) => Ok(RuntimeValue::Bool(*b)),
            Literal::Record(fields) => {
                let mut record = HashMap::new();
                for (name, _expr) in fields {
                    let value = RuntimeValue::Int(0); // Placeholder for now
                    record.insert(name.clone(), value);
                }
                Ok(RuntimeValue::Record(record))
            }
            Literal::Option(_opt) => {
                // TODO: Handle Option values properly
                Ok(RuntimeValue::Option(None)) // Placeholder for now
            }
            Literal::Unit => Ok(RuntimeValue::Unit),
            Literal::None => Ok(RuntimeValue::Option(None)),
        }
    }
    
    fn execute_binary_op(&self, op: &BinaryOp, left: &RuntimeValue, right: &RuntimeValue) -> Result<RuntimeValue, AetherError> {
        match (op, left, right) {
            (BinaryOp::Add, RuntimeValue::Int(l), RuntimeValue::Int(r)) => Ok(RuntimeValue::Int(l + r)),
            (BinaryOp::Sub, RuntimeValue::Int(l), RuntimeValue::Int(r)) => Ok(RuntimeValue::Int(l - r)),
            (BinaryOp::Mul, RuntimeValue::Int(l), RuntimeValue::Int(r)) => Ok(RuntimeValue::Int(l * r)),
            (BinaryOp::Div, RuntimeValue::Int(l), RuntimeValue::Int(r)) => {
                if *r == 0 {
                    Err(AetherError::Runtime("Division by zero".to_string()))
                } else {
                    Ok(RuntimeValue::Int(l / r))
                }
            }
            (BinaryOp::Equal, l, r) => Ok(RuntimeValue::Bool(self.values_equal(l, r))),
            (BinaryOp::NotEqual, l, r) => Ok(RuntimeValue::Bool(!self.values_equal(l, r))),
            (BinaryOp::And, RuntimeValue::Bool(l), RuntimeValue::Bool(r)) => Ok(RuntimeValue::Bool(*l && *r)),
            (BinaryOp::Or, RuntimeValue::Bool(l), RuntimeValue::Bool(r)) => Ok(RuntimeValue::Bool(*l || *r)),
            _ => Err(AetherError::Runtime(format!("Invalid binary operation: {:?} {:?} {:?}", op, left, right))),
        }
    }
    
    fn execute_unary_op(&self, op: &UnaryOp, operand: &RuntimeValue) -> Result<RuntimeValue, AetherError> {
        match (op, operand) {
            (UnaryOp::Neg, RuntimeValue::Int(i)) => Ok(RuntimeValue::Int(-i)),
            (UnaryOp::Neg, RuntimeValue::Float(f)) => Ok(RuntimeValue::Float(-f)),
            (UnaryOp::Not, RuntimeValue::Bool(b)) => Ok(RuntimeValue::Bool(!b)),
            _ => Err(AetherError::Runtime(format!("Invalid unary operation: {:?} {:?}", op, operand))),
        }
    }
    
    fn execute_call(&mut self, callee: &TypedExpr, args: &[TypedExpr]) -> Result<RuntimeValue, AetherError> {
        match callee {
            TypedExpr::Ident(name, _, _) => {
                if name == "console" && self.capabilities.allow_console {
                    // Handle console effect
                    if let Some(arg) = args.first() {
                        let arg_val = self.execute_expr(arg)?;
                        let message = match arg_val {
                            RuntimeValue::String(s) => s,
                            _ => format!("{:?}", arg_val),
                        };
                        self.effect_handler.handle_console(&message)?;
                        Ok(RuntimeValue::Unit)
                    } else {
                        Err(AetherError::Runtime("console effect requires an argument".to_string()))
                    }
                } else {
                    // For now, return a placeholder for other function calls
                    Ok(RuntimeValue::Unit)
                }
            }
            _ => {
                // Indirect call - not supported in this simple interpreter
                Err(AetherError::Runtime("Indirect calls not supported".to_string()))
            }
        }
    }
    
    fn execute_field_access(&self, base: &RuntimeValue, field: &str) -> Result<RuntimeValue, AetherError> {
        match base {
            RuntimeValue::Record(fields) => {
                fields.get(field)
                    .cloned()
                    .ok_or_else(|| AetherError::Runtime(format!("Field '{}' not found", field)))
            }
            _ => Err(AetherError::Runtime(format!("Cannot access field '{}' on non-record value", field))),
        }
    }
    
    fn execute_match(&mut self, _scrutinee: &RuntimeValue, arms: &[TypedMatchArm]) -> Result<RuntimeValue, AetherError> {
        // Simplified match - just execute the first arm
        if let Some(arm) = arms.first() {
            self.execute_expr(&arm.body)
        } else {
            Err(AetherError::Runtime("Match expression has no arms".to_string()))
        }
    }
    
    fn values_equal(&self, left: &RuntimeValue, right: &RuntimeValue) -> bool {
        match (left, right) {
            (RuntimeValue::Int(l), RuntimeValue::Int(r)) => l == r,
            (RuntimeValue::Float(l), RuntimeValue::Float(r)) => (l - r).abs() < f64::EPSILON,
            (RuntimeValue::String(l), RuntimeValue::String(r)) => l == r,
            (RuntimeValue::Bool(l), RuntimeValue::Bool(r)) => l == r,
            (RuntimeValue::Unit, RuntimeValue::Unit) => true,
            _ => false,
        }
    }
    
    pub fn get_trace(&self) -> &ExecutionTrace {
        &self.trace
    }
    
    pub fn take_trace(&mut self) -> ExecutionTrace {
        std::mem::take(&mut self.trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_value_type_repr() {
        let int_val = RuntimeValue::Int(42);
        assert!(matches!(int_val.type_repr(), TypeRepr::Int));
        
        let string_val = RuntimeValue::String("hello".to_string());
        assert!(matches!(string_val.type_repr(), TypeRepr::String));
    }
    
    #[test]
    fn test_execution_trace() {
        let mut trace = ExecutionTrace::new();
        let entry = TraceEntry {
            function_provenance: Uuid::new_v4(),
            input_values: vec![RuntimeValue::Int(1)],
            output_value: RuntimeValue::Int(2),
            timestamp: chrono::Utc::now(),
        };
        
        trace.add_entry(entry.clone());
        assert_eq!(trace.entries.len(), 1);
        assert_eq!(trace.get_calls_by_provenance(&entry.function_provenance).len(), 1);
    }
    
    #[test]
    fn test_capability_set_defaults() {
        let caps = CapabilitySet::default();
        assert!(caps.allow_console);
        assert!(!caps.allow_file_read);
        assert!(!caps.allow_file_write);
    }
    
    #[test]
    fn test_interpreter_basic_execution() {
        // Test with an empty TypedAst - the interpreter should handle this gracefully
        let typed_ast = aether_checker::TypedAst {
            program: aether_checker::TypedProgram {
                imports: Vec::new(),
                externs: Vec::new(),
                types: Vec::new(),
                effects: Vec::new(),
                functions: Vec::new(),
                version: "0.1.0".to_string(),
            },
            symbol_table: aether_checker::FlatSymbolTable::new(),
        };
        
        let capabilities = CapabilitySet::default();
        let mut interpreter = Interpreter::with_default_handler(capabilities);
        
        // Should fail gracefully with no functions to execute
        let result = interpreter.execute(&typed_ast);
        assert!(result.is_err());
        
        let trace = interpreter.get_trace();
        assert_eq!(trace.entries.len(), 0); // No function calls
    }
    
    #[test]
    fn test_effect_handler() {
        let handler = DefaultConsoleHandler;
        assert!(handler.handle_console("test").is_ok());
    }
}
