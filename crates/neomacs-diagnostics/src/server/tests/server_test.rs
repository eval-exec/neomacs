use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // for `oneshot`

use std::sync::atomic::{AtomicBool, Ordering};

use crate::metrics::{FrameMetrics, GcMetrics, MetricsSnapshot};
use crate::server::{MetricsProvider, ProfileController, port_from_str, router};

/// A ProfileController that returns fixed folded stacks, for exercising the
/// capture endpoints without a real Lisp thread.
struct StubController {
    folded: String,
    started: AtomicBool,
    live: bool,
    /// What `start` reports: true = started fresh, false = already running.
    started_fresh: bool,
}

impl ProfileController for StubController {
    fn is_live(&self) -> bool {
        self.live
    }
    fn start(&self, _interval_ns: u64) -> Result<bool, String> {
        self.started.store(true, Ordering::Relaxed);
        Ok(self.started_fresh)
    }
    fn stop_and_fold(&self) -> Result<String, String> {
        Ok(self.folded.clone())
    }
    fn abort(&self) {}
}

fn stub(folded: &str) -> Arc<StubController> {
    Arc::new(StubController {
        folded: folded.to_string(),
        started: AtomicBool::new(false),
        live: true,
        started_fresh: true,
    })
}

#[test]
fn port_from_str_accepts_valid_and_rejects_invalid() {
    assert_eq!(port_from_str("9099"), Some(9099));
    assert_eq!(port_from_str("  8080 "), Some(8080));
    assert_eq!(port_from_str("65535"), Some(65535));
    assert_eq!(port_from_str("0"), None); // zero is not a usable bind port
    assert_eq!(port_from_str(""), None);
    assert_eq!(port_from_str("nope"), None);
    assert_eq!(port_from_str("70000"), None); // out of u16 range
    assert_eq!(port_from_str("-1"), None);
}

fn fixed_provider() -> Arc<dyn MetricsProvider> {
    Arc::new(|| MetricsSnapshot {
        frame: FrameMetrics {
            presents: 42,
            ..Default::default()
        },
        gc: GcMetrics {
            collections: 3,
            ..Default::default()
        },
    })
}

#[tokio::test]
async fn metrics_route_returns_snapshot_json() {
    let app = router(fixed_provider(), None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["frame"]["presents"], 42);
    assert_eq!(v["gc"]["collections"], 3);
}

#[tokio::test]
async fn index_route_is_self_describing() {
    let app = router(fixed_provider(), None);
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["name"], "neomacs-diagnostics");
    assert!(v["endpoints"]["/metrics"].is_string());
}

#[tokio::test]
async fn live_route_emits_event_stream() {
    let app = router(fixed_provider(), None);
    let resp = app
        .oneshot(Request::builder().uri("/live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/event-stream"), "content-type was {ct}");

    // Read only the first SSE data frame; the stream is otherwise infinite.
    let mut body = resp.into_body();
    let frame = body
        .frame()
        .await
        .expect("at least one body frame")
        .expect("frame ok");
    let data = frame.into_data().expect("data frame");
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("data:"), "frame was {text}");
    assert!(text.contains("\"presents\":42"), "frame was {text}");
}

#[tokio::test]
async fn serve_on_listener_answers_metrics_over_tcp() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(fixed_provider(), None);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("200 OK"), "response was {text}");
    assert!(text.contains("\"presents\":42"), "response was {text}");
}

#[tokio::test(start_paused = true)]
async fn profile_folded_endpoint_captures_and_starts() {
    let ctrl = stub("a;b;c 10\na;b 5");
    let app = router(fixed_provider(), Some(ctrl.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp.folded?secs=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("a;b;c 10"), "body was {text}");
    assert!(
        ctrl.started.load(Ordering::Relaxed),
        "start() was not called"
    );
}

#[tokio::test(start_paused = true)]
async fn report_endpoint_returns_ranked_json() {
    let app = router(fixed_provider(), Some(stub("a;b;c 10\na;b 5\na;d 3")));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/report?secs=1&top=2&sort=total")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["sort"], "total");
    assert_eq!(v["report"]["total_samples"], 18);
    assert_eq!(v["report"]["top"][0]["function"], "a");
    assert_eq!(v["report"]["top"].as_array().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn profile_svg_endpoint_renders_flamegraph() {
    let app = router(fixed_provider(), Some(stub("a;b;c 10\na;b 5")));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp.svg?secs=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(ct, "image/svg+xml");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&bytes).contains("<svg"));
}

#[tokio::test(start_paused = true)]
async fn capture_endpoints_503_without_controller() {
    let app = router(fixed_provider(), None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp.folded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test(start_paused = true)]
async fn profile_pprof_endpoint_returns_protobuf() {
    let app = router(fixed_provider(), Some(stub("main;foo 10")));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp.pprof?secs=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(ct, "application/octet-stream");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(!bytes.is_empty());
    assert!(
        bytes.windows(3).any(|w| w == b"foo"),
        "function name missing"
    );
}

#[tokio::test(start_paused = true)]
async fn callers_endpoint_returns_edges() {
    let app = router(fixed_provider(), Some(stub("a;b;c 10\na;b;d 5")));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp/callers?fn=b&secs=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["function"], "b");
    assert_eq!(v["total_samples"], 15);
    assert_eq!(v["callers"][0]["function"], "a");
    assert_eq!(v["callees"].as_array().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn callers_endpoint_400_without_fn() {
    let app = router(fixed_provider(), Some(stub("a;b 1")));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp/callers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(start_paused = true)]
async fn capture_409_when_a_session_is_already_running() {
    // started_fresh=false => a CPU session is already running; must not hijack.
    let ctrl = Arc::new(StubController {
        folded: String::new(),
        started: AtomicBool::new(false),
        live: true,
        started_fresh: false,
    });
    let app = router(fixed_provider(), Some(ctrl));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/profile/lisp.folded?secs=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test(start_paused = true)]
async fn capture_503_when_not_live() {
    let ctrl = Arc::new(StubController {
        folded: String::new(),
        started: AtomicBool::new(false),
        live: false,
        started_fresh: true,
    });
    let app = router(fixed_provider(), Some(ctrl.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/report?secs=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    // A not-live controller must be rejected before start() is ever called.
    assert!(!ctrl.started.load(Ordering::Relaxed));
}

fn get_req(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

async fn json_of(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(start_paused = true)]
async fn captures_list_and_diff_flow() {
    let app = router(fixed_provider(), Some(stub("main;a 10")));

    let cap1 = app
        .clone()
        .oneshot(get_req("/profile/lisp.folded?secs=1"))
        .await
        .unwrap();
    assert_eq!(
        cap1.headers()
            .get("x-capture-id")
            .unwrap()
            .to_str()
            .unwrap(),
        "1"
    );
    let cap2 = app
        .clone()
        .oneshot(get_req("/report?secs=1"))
        .await
        .unwrap();
    assert_eq!(
        cap2.headers()
            .get("x-capture-id")
            .unwrap()
            .to_str()
            .unwrap(),
        "2"
    );

    let cl = app.clone().oneshot(get_req("/captures")).await.unwrap();
    let v = json_of(cl).await;
    assert_eq!(v["captures"].as_array().unwrap().len(), 2);

    let df = app
        .clone()
        .oneshot(get_req("/diff?before=1&after=2"))
        .await
        .unwrap();
    assert_eq!(df.status(), StatusCode::OK);
    let dv = json_of(df).await;
    assert_eq!(dv["before"], 1);
    assert_eq!(dv["after"], 2);
    assert!(dv["diff"]["top"].is_array());

    let bad = app.clone().oneshot(get_req("/diff")).await.unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    let nf = app
        .clone()
        .oneshot(get_req("/diff?before=1&after=999"))
        .await
        .unwrap();
    assert_eq!(nf.status(), StatusCode::NOT_FOUND);
}
