use aether_parser::Parser;

fn main() {
    let input = r#"
fn add(a: Int, b: Int) -> Int {
    a + b
}

fn main() -> Int {
    add(40, 2)
}
"#;
    
    let mut parser = Parser::new(input);
    let result = parser.parse();
    
    println!("Parse result:");
    println!("Errors: {:?}", result.errors);
    println!("Functions: {}", result.ast.functions.len());
    
    for func in &result.ast.functions {
        println!("Function: {}", func.name);
        println!("  Params: {:?}", func.params);
        println!("  Return type: {:?}", func.return_type);
    }
}
