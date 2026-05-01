use axum::{
    extract::Json,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct AnalyzeRequest {
    code: String,
    min_trust: f64,
}

#[derive(Debug, Serialize)]
struct FunctionInfo {
    name: String,
    source: String,
    confidence: f64,
}

#[derive(Debug, Serialize)]
struct AnalyzeResponse {
    trust_score: f64,
    verdict: String,
    functions: Vec<FunctionInfo>,
    blocked: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/analyze", post(analyze));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001")
        .await
        .unwrap();

    println!("🚀 Aether Server listening on http://0.0.0.0:3001");

    axum::serve(listener, app).await.unwrap();
}

async fn analyze(
    Json(request): Json<AnalyzeRequest>,
) -> Result<ResponseJson<AnalyzeResponse>, (StatusCode, ResponseJson<ErrorResponse>)> {
    // 1. Parse the submitted code
    let parsed = aether_parser::parse(&request.code);
    if !parsed.errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ErrorResponse {
                error: format!("Parse errors: {:?}", parsed.errors),
            }),
        ));
    }

    // 2. Run checker
    let name_result = aether_checker::resolve_names(&parsed);
    if !name_result.errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ErrorResponse {
                error: format!("Name resolution errors: {:?}", name_result.errors),
            }),
        ));
    }

    let type_result = aether_checker::infer_types(&name_result.resolved_ast);
    if !type_result.errors.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            ResponseJson(ErrorResponse {
                error: format!("Type checking errors: {:?}", type_result.errors),
            }),
        ));
    }

    // 3. Extract @prov tags from AST
    let mut functions = Vec::new();
    let mut total_confidence = 0.0;
    let mut function_count = 0;

    for function in &type_result.typed_ast.program.functions {
        if function.name == "main" {
            continue; // Skip main - it's infrastructure
        }

        let (source, confidence) = if let Some(prov_tag) = &function.provenance {
            let source_str = match &prov_tag.author {
                aether_core::AuthorType::Human => "user".to_string(),
                aether_core::AuthorType::AI(model) => model.clone(),
                aether_core::AuthorType::Transform(pass) => format!("transform:{}", pass),
            };
            (source_str, prov_tag.confidence)
        } else {
            ("unknown".to_string(), 0.0)
        };

        functions.push(FunctionInfo {
            name: function.name.clone(),
            source,
            confidence,
        });

        total_confidence += confidence;
        function_count += 1;
    }

    // 4. Calculate flat trust score
    let trust_score = if function_count > 0 {
        total_confidence / function_count as f64
    } else {
        0.0
    };

    // 5. Return verdict
    let blocked = trust_score < request.min_trust;
    let verdict = if blocked { "fail" } else { "pass" };

    Ok(ResponseJson(AnalyzeResponse {
        trust_score,
        verdict: verdict.to_string(),
        functions,
        blocked,
    }))
}
