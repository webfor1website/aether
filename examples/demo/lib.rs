// Simple library with human-written and AI-generated functions

pub fn add_numbers(a: i32, b: i32) -> i32 {
    a + b
}

pub fn optimize_query(query: &str) -> String {
    // AI-generated query optimization logic
    if query.len() > 100 {
        format!("SELECT * FROM optimized_table WHERE condition = '{}'", query.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
    } else {
        query.to_string()
    }
}
