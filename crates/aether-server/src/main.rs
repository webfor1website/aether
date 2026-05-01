use axum::{
    extract::Json,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
    Router,
    middleware,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

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
    detail: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    name: String,
    version: String,
    endpoints: Vec<String>,
}

#[tokio::main]
async fn main() {
    // Configurable port via environment variable
    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/analyze", post(analyze))
        .layer(middleware::from_fn(request_logger));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap();

    println!("🚀 Aether Server listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}

async fn analyze(
    Json(request): Json<AnalyzeRequest>,
) -> Result<ResponseJson<AnalyzeResponse>, (StatusCode, ResponseJson<ErrorResponse>)> {
    let start_time = Instant::now();
    
    // Wrap the entire analysis in a try-catch to prevent panics
    let result = std::panic::catch_unwind(|| {
        // 1. Parse the submitted code
        let parsed = aether_parser::parse(&request.code);
        if !parsed.errors.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                ResponseJson(ErrorResponse {
                    error: "parse failed".to_string(),
                    detail: format!("Parse errors: {:?}", parsed.errors),
                }),
            ));
        }

        // 2. Run checker
        let name_result = aether_checker::resolve_names(&parsed);
        if !name_result.errors.is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                ResponseJson(ErrorResponse {
                    error: "check failed".to_string(),
                    detail: format!("Name resolution errors: {:?}", name_result.errors),
                }),
            ));
        }

        let type_result = aether_checker::infer_types(&name_result.resolved_ast);
        if !type_result.errors.is_empty() {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                ResponseJson(ErrorResponse {
                    error: "check failed".to_string(),
                    detail: format!("Type checking errors: {:?}", type_result.errors),
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

        Ok(AnalyzeResponse {
            trust_score,
            verdict: verdict.to_string(),
            functions,
            blocked,
        })
    });

    match result {
        Ok(Ok(response)) => {
            let duration = start_time.elapsed().as_millis();
            println!("[aether] POST /analyze — trust: {:.2} — verdict: {} — {}ms", 
                response.trust_score, response.verdict, duration);
            Ok(ResponseJson(response))
        },
        Ok(Err((status, error_response))) => {
            let duration = start_time.elapsed().as_millis();
            println!("[aether] POST /analyze — error: {} — {}ms", error_response.error, duration);
            Err((status, error_response))
        },
        Err(_) => {
            let duration = start_time.elapsed().as_millis();
            println!("[aether] POST /analyze — panic recovered — {}ms", duration);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                ResponseJson(ErrorResponse {
                    error: "internal error".to_string(),
                    detail: "An unexpected error occurred during analysis".to_string(),
                }),
            ))
        }
    }
}

async fn health() -> ResponseJson<HealthResponse> {
    ResponseJson(HealthResponse {
        status: "ok".to_string(),
        version: "0.1.0".to_string(),
    })
}

async fn root() -> ResponseJson<RootResponse> {
    ResponseJson(RootResponse {
        name: "aether-server".to_string(),
        version: "0.1.0".to_string(),
        endpoints: vec!["/health".to_string(), "/analyze".to_string()],
    })
}

// Request logging middleware
async fn request_logger(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    
    let response = next.run(req).await;
    
    // Only log non-analyze endpoints (analyze endpoint has its own detailed logging)
    if uri.path() != "/analyze" {
        println!("[aether] {} {} — {}", method, uri, response.status());
    }
    
    response
}
