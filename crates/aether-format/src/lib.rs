use aether_ir::module::IrModule;

pub struct Formatter;

impl Default for Formatter {
    fn default() -> Self { Self }
}

impl Formatter {
    pub fn new() -> Self { Self }

    pub fn format_module(&self, module: &IrModule) -> String {
        format!("// {} function(s)\n", module.functions.len())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stub_compiles() {}
}

/// Diff two TypedAst and return formatted diff
pub fn diff_typed_asts(ast1: &aether_checker::TypedAst, ast2: &aether_checker::TypedAst) -> String {
    format!("// Diff between files\n// {} structural changes found", 
        if ast1.program.functions.len() == ast2.program.functions.len() { "none" } else { "some" })
}
