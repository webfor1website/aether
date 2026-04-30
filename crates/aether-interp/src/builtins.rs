use crate::value::{Value, ValueKind};
use crate::error::InterpError;
use aether_ir::expr::SYNTHETIC_PROV;

pub type BuiltinFn = fn(Vec<Value>) -> Result<Value, InterpError>;

/// Registry of built-in functions
pub fn lookup(name: &str) -> Option<BuiltinFn> {
    match name {
        "add"     => Some(builtin_add),
        "greeting" => Some(builtin_greeting),
        "print"   => Some(builtin_print),
        "println" => Some(builtin_println),
        "assert"  => Some(builtin_assert),
        "int_to_str" => Some(builtin_int_to_str),
        _ => None,
    }
}

fn builtin_greeting(args: Vec<Value>) -> Result<Value, InterpError> {
    if args.len() != 0 {
        return Err(InterpError::ArityMismatch {
            name: "greeting".into(), expected: 0, got: args.len()
        });
    }
    
    Ok(Value::string("Hello from Aether!".to_string(), SYNTHETIC_PROV))
}

fn builtin_add(args: Vec<Value>) -> Result<Value, InterpError> {
    if args.len() != 2 {
        return Err(InterpError::ArityMismatch {
            name: "add".into(), expected: 2, got: args.len()
        });
    }
    
    match (&args[0].kind, &args[1].kind) {
        (ValueKind::Int(a), ValueKind::Int(b)) => {
            Ok(Value::int(a + b, SYNTHETIC_PROV))
        }
        _ => Err(InterpError::TypeMismatch {
            expected: "Int".into(),
            got: "other".into(),
            prov_id: args[0].prov_id,
        }),
    }
}

fn builtin_print(args: Vec<Value>) -> Result<Value, InterpError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 { print!(" "); }
        print!("{}", display_value(arg));
    }
    Ok(Value::unit(SYNTHETIC_PROV))
}

fn builtin_println(args: Vec<Value>) -> Result<Value, InterpError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 { print!(" "); }
        print!("{}", display_value(arg));
    }
    println!();
    Ok(Value::unit(SYNTHETIC_PROV))
}

fn builtin_assert(args: Vec<Value>) -> Result<Value, InterpError> {
    if args.len() != 1 {
        return Err(InterpError::ArityMismatch {
            name: "assert".into(), expected: 1, got: args.len()
        });
    }
    match &args[0].kind {
        ValueKind::Bool(true) => Ok(Value::unit(SYNTHETIC_PROV)),
        ValueKind::Bool(false) => Err(InterpError::Internal(
            format!("assertion failed (prov_id={})", args[0].prov_id)
        )),
        _ => Err(InterpError::TypeMismatch {
            expected: "Bool".into(),
            got: "other".into(),
            prov_id: args[0].prov_id,
        }),
    }
}

fn builtin_int_to_str(args: Vec<Value>) -> Result<Value, InterpError> {
    if args.len() != 1 {
        return Err(InterpError::ArityMismatch {
            name: "int_to_str".into(), expected: 1, got: args.len()
        });
    }
    match &args[0].kind {
        ValueKind::Int(n) => Ok(Value::string(n.to_string(), args[0].prov_id)),
        _ => Err(InterpError::TypeMismatch {
            expected: "Int".into(), got: "other".into(), prov_id: args[0].prov_id
        }),
    }
}

fn display_value(v: &Value) -> String {
    match &v.kind {
        ValueKind::Int(n)    => n.to_string(),
        ValueKind::Float(f)  => f.to_string(),
        ValueKind::Bool(b)   => b.to_string(),
        ValueKind::Str(s)    => s.clone(),
        ValueKind::Unit      => "()".to_string(),
        ValueKind::Struct { name, .. } => format!("<struct {}>", name),
        ValueKind::Function(f) => format!("<fn {}>", f.name),
        ValueKind::Builtin(n) => format!("<builtin {}>", n),
    }
}
