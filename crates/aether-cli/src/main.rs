use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Check {
        files: Vec<PathBuf>,
    },
    Run {
        file: PathBuf,
        #[arg(short, long)]
        session_id: String,
        #[arg(long)]
        min_trust: Option<f64>,
    },
    Replay {
        #[arg(short, long)]
        session_id: String,
    },
    Prov {
        files: Vec<PathBuf>,
    },
    Fmt {
        files: Vec<PathBuf>,
    },
    Diff {
        file1: PathBuf,
        file2: PathBuf,
    },
    Wrap {
        file: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        confidence: f64,
    },
    Report {
        path: PathBuf,
    },
}

mod session;
use session::SessionManager;

fn interactive_trust_review(
    function_name: &str,
    confidence: f64,
    source: &str,
    file_path: &str,
    store: &aether_prov_store::ProvStore,
    main_file: &str,
) -> bool {
    println!("\n[aether] TRUST REVIEW — fn {} (confidence: {:.2}, source: {})", function_name, confidence, source);
    println!("File: {}", file_path);
    
    // Try to read and show the function code
    if let Ok(content) = std::fs::read_to_string(file_path) {
        if let Some(lines) = find_function_lines(&content, function_name) {
            println!("Lines: {}-{}", lines.0, lines.1);
            println!();
            println!("Code:");
            for (i, line) in content.lines().skip(lines.0 - 1).take(lines.1 - lines.0 + 1).enumerate() {
                println!("    {}", line);
            }
        }
    }
    
    println!();
    println!("Options:");
    println!("[1] Run anyway (override, logged)");
    println!("[2] Open in Cursor  (cursor {}:{})", file_path, find_function_line_number(file_path, function_name).unwrap_or(1));
    println!("[3] Quarantine function (freeze trust evolution, mark as pending review)");
    println!("[4] Abort");
    
    loop {
        print!("Choose option [1-4]: ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            match input.trim() {
                "1" => {
                    // Override
                    if let Err(e) = store.record_override(function_name, file_path) {
                        eprintln!("Warning: Failed to record override: {}", e);
                    }
                    println!("[aether] override logged for fn {}", function_name);
                    return true; // Continue execution
                }
                "2" => {
                    // Open in Cursor
                    let line_num = find_function_line_number(file_path, function_name).unwrap_or(1);
                    println!("Opening Cursor...");
                    std::process::Command::new("cursor")
                        .arg(format!("{}:{}", file_path, line_num))
                        .spawn()
                        .ok();
                    
                    // Re-show options after opening editor
                    continue;
                }
                "3" => {
                    // Quarantine
                    if let Err(e) = store.record_quarantine(function_name, file_path) {
                        eprintln!("Warning: Failed to record quarantine: {}", e);
                    }
                    println!("[aether] quarantined: fn {} — trust frozen pending review", function_name);
                    
                    // Write to .aether-quarantine file
                    let quarantine_file = format!("{}.aether-quarantine", main_file);
                    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&quarantine_file) {
                        use std::io::Write;
                        let _ = writeln!(file, "{}:{}", function_name, file_path);
                    }
                    
                    return true; // Continue to next function
                }
                "4" => {
                    // Abort
                    return false;
                }
                _ => {
                    println!("Invalid option. Please choose 1-4.");
                }
            }
        }
    }
}

fn find_function_lines(content: &str, function_name: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut start_line = None;
    let mut end_line = None;
    
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&format!("fn {}", function_name)) {
            start_line = Some(i + 1);
        }
        if start_line.is_some() && line.trim().starts_with('}') {
            end_line = Some(i + 1);
            break;
        }
    }
    
    match (start_line, end_line) {
        (Some(start), Some(end)) => Some((start, end)),
        _ => None,
    }
}

fn find_function_line_number(file_path: &str, function_name: &str) -> Option<usize> {
    if let Ok(content) = std::fs::read_to_string(file_path) {
        for (i, line) in content.lines().enumerate() {
            if line.contains(&format!("fn {}", function_name)) {
                return Some(i + 1);
            }
        }
    }
    None
}

fn generate_provenance_report(path: &std::path::Path) {
    let mut all_functions = Vec::new();
    
    // Check if path is a file or directory
    if path.is_file() {
        if let Some(extension) = path.extension() {
            if extension == "aeth" {
                process_aether_file(path, &mut all_functions);
            }
        }
    } else if path.is_dir() {
        // Process all .aeth files in directory
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if let Some(extension) = entry_path.extension() {
                    if extension == "aeth" {
                        process_aether_file(&entry_path, &mut all_functions);
                    }
                }
            }
        }
    }
    
    // Display report for each file
    let mut file_groups: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
    for func in &all_functions {
        file_groups.entry(func.file.clone()).or_insert_with(Vec::new).push(func);
    }
    
    for (file, functions) in file_groups {
        println!("\n[aether] provenance report: {}", file);
        println!();
        
        let mut total_confidence = 0.0;
        let mut tagged_count = 0;
        
        for func in functions {
            let status = if func.confidence >= 0.8 {
                "✓"
            } else if func.confidence > 0.0 {
                "⚠"
            } else {
                "✗"
            };
            
            println!("  fn {:<15} source: {:<8} confidence: {:.2}  {}", 
                func.name, 
                func.source, 
                func.confidence, 
                status
            );
            
            total_confidence += func.confidence;
            if func.confidence > 0.0 {
                tagged_count += 1;
            }
        }
        
        let flat_score = if all_functions.is_empty() { 0.0 } else { total_confidence / all_functions.len() as f64 };
        let untagged_count = all_functions.len() - tagged_count;
        
        println!();
        println!("  flat score:     {:.2}", flat_score);
        println!("  tagged:         {}/{} functions", tagged_count, all_functions.len());
        println!("  untagged:       {} (silence = zero trust)", untagged_count);
    }
    
    if all_functions.is_empty() {
        eprintln!("No .aeth files found in {}", path.display());
    }
}

#[derive(Debug)]
struct FunctionInfo {
    name: String,
    source: String,
    confidence: f64,
    file: String,
}

fn process_aether_file(file_path: &std::path::Path, all_functions: &mut Vec<FunctionInfo>) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path.display(), e);
            return;
        }
    };
    
    let parsed = aether_parser::parse(&content);
    if !parsed.errors.is_empty() {
        eprintln!("Parse errors in {}:", file_path.display());
        for error in &parsed.errors {
            eprintln!("  {}", error);
        }
        return;
    }
    
    let file_display = file_path.display().to_string();
    
    // Process functions
    for func in &parsed.ast.functions {
        let (source, confidence) = match &func.provenance {
            Some(tag) => {
                let source_str = match &tag.author {
                    aether_core::AuthorType::Human => "user".to_string(),
                    aether_core::AuthorType::AI(model) => model.clone(),
                    aether_core::AuthorType::Transform(_) => "transform".to_string(),
                };
                (source_str, tag.confidence)
            }
            None => ("?".to_string(), 0.0),
        };
        
        all_functions.push(FunctionInfo {
            name: func.name.clone(),
            source,
            confidence,
            file: file_display.clone(),
        });
    }
    
    // Process externs
    for extern_decl in &parsed.ast.externs {
        let source_str = match &extern_decl.provenance.author {
            aether_core::AuthorType::Human => "user".to_string(),
            aether_core::AuthorType::AI(model) => model.clone(),
            aether_core::AuthorType::Transform(_) => "transform".to_string(),
        };
        
        all_functions.push(FunctionInfo {
            name: extern_decl.name.clone(),
            source: source_str,
            confidence: extern_decl.provenance.confidence,
            file: file_display.clone(),
        });
    }
}

fn wrap_rust_file(content: &str, source: &str, confidence: f64) -> String {
    let mut output = Vec::new();
    
    // Cursor adapts to codebase style and uses multiple underlying 
    // models. Explicit markers are the only reliable high-confidence 
    // signal. Medium confidence detects .cursorrules bleedthrough — 
    // structural patterns that emerge when Cursor follows its rules 
    // file regardless of which model it uses underneath.
    let cursor_detection = detect_cursor_authorship(content);
    let (final_source, final_confidence) = match cursor_detection {
        CursorDetection::High => {
            eprintln!("[aether] detected: cursor (explicit marker)");
            ("cursor".to_string(), 0.90)
        }
        CursorDetection::Medium(score) => {
            eprintln!("[aether] detected: cursor-likely (medium — {}/5 .cursorrules bleedthrough patterns matched)", score);
            ("cursor-likely".to_string(), 0.65)
        }
        CursorDetection::None => {
            // Fall through to Grok detection
            let grok_detection = detect_grok_authorship(content);
            match grok_detection {
                GrokDetection::High => {
                    eprintln!("[aether] detected: grok (explicit marker)");
                    ("grok".to_string(), 0.90)
                }
                GrokDetection::Medium(score) => {
                    eprintln!("[aether] detected: grok-likely (medium — {}/7 patterns matched)", score);
                    ("grok-likely".to_string(), 0.65)
                }
                GrokDetection::None => {
                    // Fall through to Claude detection
                    let claude_detection = detect_claude_authorship(content);
                    match claude_detection {
                        ClaudeDetection::High => {
                            eprintln!("[aether] detected: claude (high confidence — explicit marker)");
                            ("claude".to_string(), 0.85)
                        }
                        ClaudeDetection::Medium(score) => {
                            eprintln!("[aether] detected: claude-likely (medium — {}/6 structural patterns matched)", score);
                            ("claude-likely".to_string(), 0.65)
                        }
                        ClaudeDetection::Low(score) => {
                            eprintln!("[aether] detected: claude-possible (low — style heuristics only, verify manually)");
                            ("claude-possible".to_string(), 0.45)
                        }
                        ClaudeDetection::None => {
                            if source == "ai" {
                                ("ai".to_string(), 0.60)
                            } else {
                                (source.to_string(), confidence)
                            }
                        }
                    }
                }
            }
        }
    };
    
    // Add the @prov tag at the top
    output.push(format!("@prov(source: \"{}\", confidence: {:.2})", final_source, final_confidence));
    output.push(String::new());
    
    // Regex to match public function signatures
    let function_regex = regex::Regex::new(r"pub\s+fn\s+(\w+)\s*\((.*?)\)\s*(?:->\s*(.*?))\s*(?:where\s+.+?)?\s*;").unwrap();
    
    for cap in function_regex.captures_iter(content) {
        let function_name = cap.get(1).unwrap().as_str();
        let params = cap.get(2).unwrap().as_str();
        let return_type = cap.get(3).map_or("()", |m| m.as_str().trim());
        
        // Map Rust types to Aether types
        let aether_params = map_rust_types_to_aether(params);
        let aether_return = map_rust_type_to_aether(return_type);
        
        output.push(format!("extern fn {}({}) -> {};", function_name, aether_params, aether_return));
    }
    
    output.join("\n")
}

#[derive(Debug)]
enum ClaudeDetection {
    High,
    Medium(usize), // score out of 6
    Low(usize),    // score out of 5
    None,
}

#[derive(Debug)]
enum CursorDetection {
    High,
    Medium(usize), // score out of 5
    None,
}

#[derive(Debug)]
enum GrokDetection {
    High,
    Medium(usize), // score out of 7
    None,
}

fn detect_claude_authorship(content: &str) -> ClaudeDetection {
    // HIGH CONFIDENCE: explicit markers
    let high_patterns = [
        "// Claude",
        "// Generated by Claude",
        "// Note:",
        "# Arguments\n# Returns\n# Examples",
    ];
    
    let high_matches = high_patterns.iter().filter(|&&pattern| content.contains(pattern)).count();
    if high_matches > 0 {
        return ClaudeDetection::High;
    }
    
    // MEDIUM CONFIDENCE: structural patterns
    let mut medium_score = 0;
    
    // Function names average over 18 characters
    let function_regex = regex::Regex::new(r"fn\s+(\w+)\s*\(").unwrap();
    let function_names: Vec<_> = function_regex.captures_iter(content)
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();
    
    if !function_names.is_empty() {
        let avg_length: f64 = function_names.iter().map(|name| name.len() as f64).sum::<f64>() / function_names.len() as f64;
        if avg_length > 18.0 {
            medium_score += 1;
        }
    }
    
    // More than 40% of functions have dedicated helpers
    if function_names.len() > 2 {
        let helper_count = function_names.iter()
            .filter(|name| name.contains("helper") || name.contains("_helper") || 
                           name.contains("util") || name.contains("_util"))
            .count();
        if helper_count as f64 / function_names.len() as f64 > 0.4 {
            medium_score += 1;
        }
    }
    
    // unwrap_or appears more than twice
    if content.matches("unwrap_or").count() > 2 {
        medium_score += 1;
    }
    
    // if let Some ratio > 60%
    let if_let_count = content.matches("if let Some(").count();
    let option_count = content.matches("Option<").count() + content.matches("&Option").count();
    if option_count > 0 && if_let_count as f64 / option_count as f64 > 0.6 {
        medium_score += 1;
    }
    
    // .to_string() appears more than 3 times
    if content.matches(".to_string()").count() > 3 {
        medium_score += 1;
    }
    
    // "// Simplified" comment present
    if content.contains("// Simplified") {
        medium_score += 1;
    }
    
    if medium_score >= 2 {
        return ClaudeDetection::Medium(medium_score);
    }
    
    // LOW CONFIDENCE: style patterns
    let mut low_score = 0;
    
    // Intermediate variable names over 15 chars average
    let var_regex = regex::Regex::new(r"let\s+(\w+)\s*=").unwrap();
    let var_names: Vec<_> = var_regex.captures_iter(content)
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();
    
    if !var_names.is_empty() {
        let avg_var_length: f64 = var_names.iter().map(|name| name.len() as f64).sum::<f64>() / var_names.len() as f64;
        if avg_var_length > 15.0 {
            low_score += 1;
        }
    }
    
    // Struct initialization uses explicit field names
    if content.contains("{") && content.contains(":") && 
       !content.contains("..Default") && content.matches(".*:.*,.*:.*,").count() > 0 {
        low_score += 1;
    }
    
    // Functions average under 12 lines
    let lines: Vec<_> = content.lines().collect();
    let mut total_lines = 0;
    let mut function_count = 0;
    
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with("fn ") {
            function_count += 1;
            // Count lines until next function or end
            for j in i+1..lines.len() {
                if lines[j].trim().starts_with("fn ") || (j == lines.len() - 1) {
                    total_lines += (j - i);
                    break;
                }
            }
        }
    }
    
    if function_count > 0 && total_lines as f64 / function_count as f64 < 12.0 {
        low_score += 1;
    }
    
    // "// Calculate" or "// Handle" comment prefixes
    if content.contains("// Calculate") || content.contains("// Handle") {
        low_score += 1;
    }
    
    // Rustdoc patterns (but not the full high-confidence set)
    if content.contains("///") && (content.contains("# Arguments") || content.contains("# Returns")) {
        low_score += 1;
    }
    
    if low_score >= 3 {
        return ClaudeDetection::Low(low_score);
    }
    
    ClaudeDetection::None
}

fn detect_cursor_authorship(content: &str) -> CursorDetection {
    // HIGH CONFIDENCE: explicit markers
    let high_patterns = [
        "// Generated by Cursor",
        "// Cursor:",
        "// cursor-agent:",
        "// @cursor",
        ".cursorrules",
    ];
    
    let high_matches = high_patterns.iter().filter(|&&pattern| content.contains(pattern)).count();
    if high_matches > 0 {
        return CursorDetection::High;
    }
    
    // MEDIUM CONFIDENCE: .cursorrules bleedthrough patterns
    let mut medium_score = 0;
    
    // Structured inline comment blocks: 3+ consecutive lines of "// [noun]" pattern
    let lines: Vec<_> = content.lines().collect();
    let mut consecutive_noun_comments = 0;
    
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("// ") {
            // Check if it follows the "// [Noun]" pattern
            let comment_content = &trimmed[3..].trim();
            if comment_content.chars().next().map_or(false, |c| c.is_uppercase()) && 
               comment_content.split_whitespace().count() <= 3 {
                consecutive_noun_comments += 1;
            } else {
                consecutive_noun_comments = 0;
            }
            
            if consecutive_noun_comments >= 3 {
                medium_score += 1;
                break;
            }
        } else {
            consecutive_noun_comments = 0;
        }
    }
    
    // try/catch or if let Err wrapping EVERY function (100% error handling coverage)
    let function_regex = regex::Regex::new(r"fn\s+\w+\s*\(").unwrap();
    let functions: Vec<_> = function_regex.find_iter(content).collect();
    
    if !functions.is_empty() {
        let mut error_wrapped_functions = 0;
        
        for func_match in functions {
            let func_start = func_match.start();
            // Look for the function body (next opening brace)
            if let Some(brace_pos) = content[func_start..].find('{') {
                let func_content = &content[func_start..func_start + brace_pos + 100]; // Look ahead 100 chars
                if func_content.contains("if let Err(") || func_content.contains("catch") || 
                   func_content.contains("unwrap_or") || func_content.contains("?") {
                    error_wrapped_functions += 1;
                }
            }
        }
        
        if error_wrapped_functions == functions.len() {
            medium_score += 1;
        }
    }
    
    // Functions structured in exact order: imports → types → function → helpers → export
    let mut has_imports = false;
    let mut has_types = false;
    let mut has_functions = false;
    let mut has_helpers = false;
    let mut has_exports = false;
    
    let mut section = "imports";
    for line in &lines {
        let trimmed = line.trim();
        
        if trimmed.starts_with("use ") || trimmed.starts_with("extern ") {
            has_imports = true;
            section = "imports";
        } else if trimmed.starts_with("struct ") || trimmed.starts_with("enum ") || 
                  trimmed.starts_with("type ") {
            has_types = true;
            if section == "imports" { section = "types"; }
        } else if trimmed.starts_with("pub fn ") {
            has_functions = true;
            if section == "types" { section = "functions"; }
        } else if trimmed.starts_with("fn ") && !trimmed.starts_with("pub fn ") {
            has_helpers = true;
            if section == "functions" { section = "helpers"; }
        } else if section == "helpers" && (trimmed.starts_with("pub use ") || 
                                          trimmed.starts_with("pub mod ")) {
            has_exports = true;
            section = "exports";
        }
    }
    
    if has_imports && has_types && has_functions && has_helpers && has_exports {
        medium_score += 1;
    }
    
    // HTTP status codes present with matching error message strings
    let http_status_patterns = [
        ("400", "bad request"),
        ("401", "unauthorized"),
        ("403", "forbidden"),
        ("404", "not found"),
        ("500", "internal server error"),
    ];
    
    for (status, message) in &http_status_patterns {
        if content.contains(status) && content.contains(message) {
            medium_score += 1;
            break;
        }
    }
    
    // Every function has a JSDoc/rustdoc block, no exceptions
    let function_regex = regex::Regex::new(r"fn\s+\w+\s*\(").unwrap();
    let functions: Vec<_> = function_regex.find_iter(content).collect();
    
    if !functions.is_empty() {
        let mut documented_functions = 0;
        
        for func_match in functions {
            let func_start = func_match.start();
            // Look backwards for doc comments
            let before_func = &content[..func_start];
            let lines_before: Vec<_> = before_func.lines().rev().take(10).collect();
            
            let has_docs = lines_before.iter().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("///") || trimmed.starts_with("/**") || 
                trimmed.starts_with("//!") || trimmed.starts_with("/*!")
            });
            
            if has_docs {
                documented_functions += 1;
            }
        }
        
        if documented_functions == functions.len() {
            medium_score += 1;
        }
    }
    
    // NEW PATTERNS - Additional MEDIUM confidence signals
    
    // Every public function has rustdoc (/// comments) — check if pub fn count matches rustdoc block count (ratio > 0.9)
    let pub_fn_regex = regex::Regex::new(r"pub\s+fn\s+\w+\s*\(").unwrap();
    let pub_functions: Vec<_> = pub_fn_regex.find_iter(content).collect();
    let mut documented_pub_functions = 0;
    
    for func_match in pub_functions {
        let func_start = func_match.start();
        // Look backwards for doc comments
        let before_func = &content[..func_start];
        let lines_before: Vec<_> = before_func.lines().rev().take(10).collect();
        
        let has_docs = lines_before.iter().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("///")
        });
        
        if has_docs {
            documented_pub_functions += 1;
        }
    }
    
    if !pub_functions.is_empty() {
        let doc_ratio = documented_pub_functions as f64 / pub_functions.len() as f64;
        if doc_ratio > 0.9 {
            medium_score += 1;
        }
    }
    
    // Sequential step comments present: "// Check if", "// Find", "// Verify", "// Update", "// Generate" — 3+ of these patterns
    let step_patterns = ["// Check if", "// Find ", "// Verify", "// Update", "// Generate"];
    let step_matches = step_patterns.iter().filter(|&&pattern| content.contains(pattern)).count();
    if step_matches >= 3 {
        medium_score += 1;
    }
    
    // Builder pattern struct present (struct name ends in "Builder")
    let builder_regex = regex::Regex::new(r"struct\s+(\w*Builder)\w*\s*").unwrap();
    if builder_regex.is_match(content) {
        medium_score += 1;
    }
    
    // `pub mod utils` or `pub mod helpers` submodule present
    if content.contains("pub mod utils") || content.contains("pub mod helpers") {
        medium_score += 1;
    }
    
    // `impl Default` count matches struct count (blanket Default impls)
    let struct_regex = regex::Regex::new(r"struct\s+\w+").unwrap();
    let default_impl_regex = regex::Regex::new(r"impl\s+Default\s+for").unwrap();
    let struct_count = struct_regex.find_iter(content).count();
    let default_impl_count = default_impl_regex.find_iter(content).count();
    
    if default_impl_count >= 2 && default_impl_count >= struct_count {
        medium_score += 1;
    }
    
    // "// Would need to fetch" or "// In a real" placeholder comments
    if content.contains("// Would need to") || content.contains("// In a real") {
        medium_score += 1;
    }
    
    if medium_score >= 2 {
        CursorDetection::Medium(medium_score)
    } else {
        CursorDetection::None
    }
}

// Grok defaults to free functions over wrapper structs, 
// thiserror for error types, IntoResponse on errors directly,
// Arc<RwLock> for concurrency, and thinks web-framework-first.
// Patterns extracted from observed output, not guesswork.
fn detect_grok_authorship(content: &str) -> GrokDetection {
    // HIGH CONFIDENCE: explicit markers
    let high_patterns = [
        "// Grok",
        "// xAI",
        "// @grok",
    ];
    
    let high_matches = high_patterns.iter().filter(|&&pattern| content.contains(pattern)).count();
    if high_matches > 0 {
        return GrokDetection::High;
    }
    
    // MEDIUM CONFIDENCE: structural patterns
    let mut medium_score = 0;
    
    // Free functions for crypto/hashing instead of wrapper structs
    // (detect: no struct containing "Hasher" or "Manager" but hash_password or generate_jwt as standalone fn present)
    let has_hasher_manager_struct = content.contains("Hasher") || content.contains("Manager");
    let has_hash_password_fn = content.contains("hash_password") || content.contains("generate_jwt");
    
    if !has_hasher_manager_struct && has_hash_password_fn {
        medium_score += 1;
    }
    
    // thiserror macro used: "#[derive(Error" present
    if content.contains("#[derive(Error") {
        medium_score += 1;
    }
    
    // IntoResponse implemented on error type: "impl IntoResponse for" present
    if content.contains("impl IntoResponse for") {
        medium_score += 1;
    }
    
    // Arc<RwLock< pattern present (Grok's default concurrency)
    if content.contains("Arc<RwLock<") {
        medium_score += 1;
    }
    
    // Production notes as inline comments: "// PRODUCTION NOTE" or "// In production" present
    if content.contains("// PRODUCTION NOTE") || content.contains("// In production") {
        medium_score += 1;
    }
    
    // No Builder pattern AND no impl Default 
    // (absence of "Builder" struct AND absence of "impl Default")
    let has_builder = content.contains("Builder");
    let has_impl_default = content.contains("impl Default");
    
    if !has_builder && !has_impl_default {
        medium_score += 1;
    }
    
    // Full main.rs with route definitions included in same output
    // (detect: "axum" AND "Router::new()" both present)
    if content.contains("axum") && content.contains("Router::new()") {
        medium_score += 1;
    }
    
    if medium_score >= 2 {
        GrokDetection::Medium(medium_score)
    } else {
        GrokDetection::None
    }
}

fn map_rust_types_to_aether(rust_params: &str) -> String {
    if rust_params.trim().is_empty() {
        return String::new();
    }
    
    rust_params
        .split(',')
        .map(|param| {
            let param = param.trim();
            // Extract just the type part (ignore parameter name)
            if let Some(colon_pos) = param.rfind(':') {
                let rust_type = param[colon_pos + 1..].trim();
                map_rust_type_to_aether(rust_type)
            } else {
                // If no colon, assume the whole thing is the type
                map_rust_type_to_aether(param)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn map_rust_type_to_aether(rust_type: &str) -> String {
    let clean_type = rust_type.trim();
    match clean_type {
        "i32" | "i64" | "isize" | "usize" => "Int".to_string(),
        "bool" => "Bool".to_string(),
        "()" => "Unit".to_string(),
        "String" | "&str" => "String".to_string(),
        "f32" | "f64" => "Float".to_string(),
        // For generic types, strip the angle brackets
        t if t.contains('<') => {
            let base = t.split('<').next().unwrap_or(t);
            match base {
                "Vec" | "Option" => format!("{}<{}>", base, "Int"), // Simplified
                "Result" => "Int".to_string(), // Simplified
                _ => "Int".to_string(), // Default fallback
            }
        }
        _ => "Int".to_string(), // Default fallback for unknown types
    }
}

fn main() {
    let workspace_root = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let session_mgr = SessionManager::load(&workspace_root);

    if let Some(block) = session_mgr.check_cooldown() {
        eprintln!();
        eprintln!("  ╔══════════════════════════════════════════════╗");
        eprintln!("  ║           aether — session cooldown          ║");
        eprintln!("  ╠══════════════════════════════════════════════╣");
        eprintln!("  ║  you built something real last session.      ║");
        eprintln!("  ║  cooldown: {:>3}m remaining                   ║",
            block.minutes_remaining);
        eprintln!("  ║  the code will still be here.                ║");
        eprintln!("  ╚══════════════════════════════════════════════╝");
        eprintln!();
        std::process::exit(0);
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Check { files }) => {
            for file in files {
                let content = match std::fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading file {}: {}", file.display(), e);
                        std::process::exit(1);
                    }
                };

                let parsed = aether_parser::parse(&content);
                if !parsed.errors.is_empty() {
                    eprintln!("Parse errors in {}:", file.display());
                    for error in &parsed.errors {
                        eprintln!("  {}", error);
                    }
                    std::process::exit(1);
                }

                println!("✓ {} passed validation", file.display());
            }
        }

        Some(Commands::Run { file, session_id, min_trust }) => {
            // Initialize discipline engine
            let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let mut discipline_engine = aether_discipline::DisciplineEngine::new(&workspace_root);
            
            // Check for min_trust in config file if not provided via CLI
            let effective_min_trust = if let Some(cli_min_trust) = min_trust {
                cli_min_trust
            } else {
                // Look for .aether-wellbeing file in the same directory as the .ae file
                if let Some(parent_dir) = file.parent() {
                    let config_path = parent_dir.join(".aether-wellbeing");
                    if let Ok(config_content) = std::fs::read_to_string(&config_path) {
                        // Parse config file for min_trust line
                        let mut config_min_trust = 0.0;
                        for line in config_content.lines() {
                            let line = line.trim();
                            if line.starts_with("min_trust:") {
                                if let Some(value_str) = line.strip_prefix("min_trust:").map(|s| s.trim()) {
                                    if let Ok(parsed_value) = value_str.parse::<f64>() {
                                        config_min_trust = parsed_value;
                                        break; // found the line, stop parsing
                                    }
                                }
                            }
                        }
                        config_min_trust
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            };
            
            // Read the file first
            let content = match std::fs::read_to_string(&file) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("Error reading file {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };
            
            // Enforce discipline before processing
            if let Err(e) = discipline_engine.enforce_before_edit(&file, "parse") {
                eprintln!("Discipline error: {}", e);
                std::process::exit(1);
            }
            
            // Now require that file was read (it should be cached by enforce_before_edit)
            if let Err(e) = discipline_engine.require_read(&file) {
                eprintln!("Discipline error: {}", e);
                std::process::exit(1);
            }

            let parsed = aether_parser::parse(&content);
            if !parsed.errors.is_empty() {
                eprintln!("Parse errors in {}:", file.display());
                for error in &parsed.errors {
                    eprintln!("  {}", error);
                }
                std::process::exit(1);
            }

            // Resolve imports before running the checker
            let mut resolved_program = parsed.ast.clone();
            let mut imports_to_remove = Vec::new();
            let mut import_paths = Vec::new();
            
            // Collect import statements from the main program
            for (stmt_idx, stmt) in resolved_program.statements.iter().enumerate() {
                if let aether_core::Stmt::Import(import_stmt) = stmt {
                    imports_to_remove.push(stmt_idx);
                    
                    // Resolve path relative to the file being run
                    let file_dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
                    let import_path = file_dir.join(&import_stmt.path);
                    import_paths.push(import_path);
                }
            }
            
            // Process each import
            for import_path in import_paths {
                // Read and parse the imported file
                if let Ok(import_content) = std::fs::read_to_string(&import_path) {
                    let import_parsed = aether_parser::parse(&import_content);
                    if import_parsed.errors.is_empty() {
                        // Merge functions from imported file
                        resolved_program.functions.extend(import_parsed.ast.functions.into_iter().filter(|f| f.name != "main"));
                    } else {
                        eprintln!("Parse errors in imported file {}:", import_path.display());
                        for error in &import_parsed.errors {
                            eprintln!("  {}", error);
                        }
                        std::process::exit(1);
                    }
                } else {
                    eprintln!("Error reading imported file {}: {}", import_path.display(), std::io::Error::last_os_error());
                    std::process::exit(1);
                }
            }
            
            // Remove import statements from the main program
            for &stmt_idx in imports_to_remove.iter().rev() {
                if stmt_idx < resolved_program.statements.len() {
                    resolved_program.statements.remove(stmt_idx);
                }
            }

            // Run the full checker pipeline
            let check_result = aether_checker::resolve_names(&aether_parser::ParseResult {
                ast: resolved_program,
                errors: Vec::new(),
                provenance_hints: Vec::new(),
            });
            if !check_result.errors.is_empty() {
                eprintln!("Name resolution errors in {}:", file.display());
                for error in &check_result.errors {
                    eprintln!("  {}", error);
                }
                std::process::exit(1);
            }

            let type_result = aether_checker::infer_types(&check_result.resolved_ast);
            if !type_result.errors.is_empty() {
                eprintln!("Type inference errors in {}:", file.display());
                for error in &type_result.errors {
                    eprintln!("  {}", error);
                }
                std::process::exit(1);
            }

            let mut effect_checker = aether_checker::EffectChecker::new();
            let effect_result = effect_checker.check(&type_result.typed_ast);
            if !effect_result.errors.is_empty() {
                eprintln!("Effect checking errors in {}:", file.display());
                for error in &effect_result.errors {
                    eprintln!("  {}", error);
                }
                std::process::exit(1);
            }

            let mut prov_validator = aether_checker::ProvenanceValidator::new();
            let prov_result = prov_validator.validate(&effect_result.typed_ast);
            if !prov_result.errors.is_empty() {
                eprintln!("Provenance validation errors in {}:", file.display());
                for error in &prov_result.errors {
                    eprintln!("  {}", error);
                }
                std::process::exit(1);
            }

            // Enforce discipline before lowering
            if let Err(e) = discipline_engine.enforce_before_edit(&file, "lower") {
                eprintln!("Discipline error before lowering: {}", e);
                std::process::exit(1);
            }
            
            match aether_ir::lower::lower_module(&prov_result.typed_ast.program) {
                Err(e) => {
                    eprintln!("ERROR: Lowering failed: {}", e);
                    std::process::exit(1);
                }
                Ok(ir_module) => {
                    let store = aether_prov_store::ProvStore::open(
                        &format!("{}-{}.aether-prov.db", file.parent().unwrap_or_else(|| std::path::Path::new(".")).to_string_lossy(), session_id),
                        session_id.clone(),
                    ).unwrap_or_else(|_| {
                        // Fallback: create temporary in-memory store
                        let temp_path = format!("temp-{}.db", std::process::id());
                        aether_prov_store::ProvStore::open(&temp_path, session_id.clone()).unwrap()
                    });

                    // Enforce discipline before execution
                    if let Err(e) = discipline_engine.enforce_before_edit(&file, "execute") {
                        eprintln!("Discipline error before execution: {}", e);
                        std::process::exit(1);
                    }
                    
                    // Wire provenance into store — tagged functions use their confidence, untagged get 0.0
let now = chrono::Utc::now().to_rfc3339();
for func in &prov_result.typed_ast.program.functions {
    if func.name == "main" { continue; } // skip main — it's infrastructure, not authored logic
    match &func.provenance {
        Some(tag) => {
            let author_str = tag.author.to_string();
            let timestamp_str = tag.timestamp.to_rfc3339();
            let parents_json = serde_json::to_string(&tag.parents).unwrap_or_else(|_| "[]".to_string());
            let _ = store.insert_raw(
                &func.name,
                &author_str,
                tag.prompt.as_deref(),
                tag.confidence,
                &timestamp_str,
                &parents_json,
                Some(file.to_str().unwrap_or("unknown")),
                Some(file.to_str().unwrap_or("unknown")),
            );
        }
        None => {
            let _ = store.insert_raw(
                &func.name,
                "unknown",
                None,
                0.0,
                &now,
                "[]",
                Some(file.to_str().unwrap_or("unknown")),
                Some(file.to_str().unwrap_or("unknown")),
            );
        }
    }
}

                    let mut interpreter = aether_interp::Interpreter::new(store);
                    interpreter.load_module(&ir_module);

                    match interpreter.run_main(&file.display().to_string()) {
                        Ok((result, weighted_trust, flat_trust)) => {
                        // Use weighted trust for enforcement (more conservative for deep calls)
                        let trust_score = weighted_trust;
                        
                        // Enforce trust threshold with interactive review
                        if trust_score < effective_min_trust {
                            // Get blocked functions for interactive review
                            let blocked_functions: Vec<_> = if let Ok(records) = interpreter.store.get_function_records() {
                                records.iter()
                                    .filter(|record| record.confidence < effective_min_trust)
                                    .map(|record| (record.function_name.clone(), record.confidence, record.author.clone(), record.file_path.clone().unwrap_or_else(|| file.display().to_string())))
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            if !blocked_functions.is_empty() {
                                let mut should_continue = false;
                                
                                for (function_name, confidence, source, file_path) in blocked_functions {
                                    if !interactive_trust_review(&function_name, confidence, &source, &file_path, &interpreter.store, &file.display().to_string()) {
                                        // User chose to abort
                                        std::process::exit(2);
                                    }
                                    // User chose to override or quarantine, continue to next function
                                    should_continue = true;
                                }
                                
                                if should_continue {
                                    // Continue execution after overrides
                                    eprintln!("[aether] continuing with overrides...");
                                } else {
                                    // All functions were quarantined, still block
                                    if (weighted_trust - flat_trust).abs() > 0.01 {
                                        eprintln!("[aether] blocked — trust score {:.2} (weighted) / {:.2} (flat) is below minimum {:.2}", weighted_trust, flat_trust, effective_min_trust);
                                    } else {
                                        eprintln!("[aether] blocked — trust score {:.2} is below minimum {:.2}", trust_score, effective_min_trust);
                                    }
                                    std::process::exit(2);
                                }
                            } else {
                                // No blocked functions found, but trust score is still low
                                if (weighted_trust - flat_trust).abs() > 0.01 {
                                    eprintln!("[aether] blocked — trust score {:.2} (weighted) / {:.2} (flat) is below minimum {:.2}", weighted_trust, flat_trust, effective_min_trust);
                                } else {
                                    eprintln!("[aether] blocked — trust score {:.2} is below minimum {:.2}", trust_score, effective_min_trust);
                                }
                                std::process::exit(2);
                            }
                        }
                        match result.kind {
                                aether_interp::value::ValueKind::Unit => println!("Unit"),
                                aether_interp::value::ValueKind::Int(i) => println!("{}", i),
                                aether_interp::value::ValueKind::Float(f) => println!("{}", f),
                                aether_interp::value::ValueKind::Bool(b) => println!("{}", b),
                                aether_interp::value::ValueKind::Str(s) => println!("{}", s),
                                aether_interp::value::ValueKind::Struct { .. } => println!("Struct"),
                                aether_interp::value::ValueKind::Function(_) => println!("Function"),
                                aether_interp::value::ValueKind::Builtin(_) => println!("Builtin"),
                            }
                            
                            // Log provenance for successful execution
                            if let Err(e) = discipline_engine.log_provenance(&file, "successful_execution") {
                                eprintln!("Warning: Failed to log provenance: {}", e);
                            }
                            
                            // Evolve trust up for successful execution
                            if let Err(e) = interpreter.store.evolve_trust(0.05) {
                                eprintln!("Warning: Failed to evolve trust: {}", e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Runtime error: {}", e);
                            
                            // Evolve trust down for runtime error
                            if let Err(evol_err) = interpreter.store.evolve_trust(-0.1) {
                                eprintln!("Warning: Failed to evolve trust: {}", evol_err);
                            }
                            
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Some(Commands::Replay { session_id }) => {
            // Find the database file using same path logic as run command
            let db_path = format!("tests-{}.aether-prov.db", session_id);
            
            match aether_prov_store::ProvStore::open(&db_path, session_id.clone()) {
                Ok(store) => {
                    match store.get_replay_records() {
                        Ok(records) => {
                            println!("[aether] replay — session: {}", session_id);
                            for (i, record) in records.iter().enumerate() {
                                println!("  #{}  {:<15} confidence: {:.2}   source: {:<8} {}", 
                                    i + 1,
                                    record.function_name,
                                    record.confidence,
                                    record.author,
                                    record.file_path.as_deref().unwrap_or("unknown"));
                            }
                            println!();
                            
                            // Calculate both trust scores
                            let weighted_trust = store.weighted_trust_score().unwrap_or(0.0);
                            let flat_trust = store.flat_trust_score().unwrap_or(0.0);
                            
                            // Show both scores when they differ
                            if (weighted_trust - flat_trust).abs() > 0.01 {
                                println!("  final trust score: {:.2} (weighted) / {:.2} (flat)", weighted_trust, flat_trust);
                            } else {
                                println!("  final trust score: {:.2}", weighted_trust);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading replay records: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error opening database {}: {}", db_path, e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Prov { files }) => {
            for file in files {
                let content = match std::fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading file {}: {}", file.display(), e);
                        std::process::exit(1);
                    }
                };

                let parsed = aether_parser::parse(&content);
                if !parsed.errors.is_empty() {
                    eprintln!("Parse errors in {}:", file.display());
                    for error in &parsed.errors {
                        eprintln!("  {}", error);
                    }
                    std::process::exit(1);
                }

                println!("✓ {} passed validation", file.display());
            }
        }

        Some(Commands::Fmt { files }) => {
            for file in files {
                let content = match std::fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading file {}: {}", file.display(), e);
                        std::process::exit(1);
                    }
                };

                let parsed = aether_parser::parse(&content);
                if !parsed.errors.is_empty() {
                    eprintln!("Parse errors in {}:", file.display());
                    for error in &parsed.errors {
                        eprintln!("  {}", error);
                    }
                    std::process::exit(1);
                }

                println!("Formatted {}: // {} functions", file.display(), parsed.ast.functions.len());
            }
        }

        Some(Commands::Diff { file1: _, file2: _ }) => {
            eprintln!("diff: not yet implemented");
        }

        Some(Commands::Wrap { file, source, confidence }) => {
            let content = match std::fs::read_to_string(&file) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("Error reading file {}: {}", file.display(), e);
                    std::process::exit(1);
                }
            };

            let output = wrap_rust_file(&content, &source, confidence);
            let output_path = file.with_extension("aeth");
            
            match std::fs::write(&output_path, &output) {
                Ok(()) => {
                    println!("✓ Wrapped {} to {}", file.display(), output_path.display());
                }
                Err(e) => {
                    eprintln!("Error writing output file {}: {}", output_path.display(), e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Report { path }) => {
            generate_provenance_report(&path);
        }

        None => {
            eprintln!("No command provided");
            std::process::exit(1);
        }
    }
}
