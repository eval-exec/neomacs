use std::collections::BTreeMap;
use std::num::{NonZeroI32, NonZeroUsize};

use base64::Engine as _;
use expect_test::{Expect, expect};
use serde_json::Value;

use super::{
    ApprovedFrame, ApprovedOutput, BASE64, CaptureProjectIdEvidence, CaptureTsserverBundleEvidence,
    ConfigureRequest, DeliveryPlan, DiagnosticKind, FileNameListRequest, FileRenameRequest,
    FileRequest, FixtureExpectation, FixtureFile, FixtureGeneration, FixtureManifest,
    FormatOptions, HostInfoToken, JsonPathSegment, LineOffset, NavToRequest, OpenRequest,
    ParityBatchCase, PointRequest, ProjectErrorsRequest, ProjectInfoRequest, RangeRequest,
    RecordedExchange, RecordedLiteral, ReloadRequest, ReplayExchange, ReplayRuntimeIdentity,
    ReplaySession, ReplayTermination, RequestOrdinal, ResponseToken, ResponseTokenKind, ScriptKind,
    Sha256Digest, TerminalExchange, TideReplay, TideScenario, TideTempFileToken, TsRequest,
    UserPreferences, WorkspaceRelativePath,
};

const CONFIG_BYTES: &str = "{\n  \"compilerOptions\": {\n    \"allowJs\": true,\n    \"checkJs\": true,\n    \"noEmit\": true,\n    \"strict\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"commonjs\"\n  },\n  \"files\": [\"src/main.js\", \"src/math.js\"]\n}\n";
const MAIN_BYTES: &str = "import { multiply } from \"./math.js\";\r\nimport { add } from \"./math.js\";\r\n\r\nexport const 界 = add(3, 4);\r\nexport const tabbed =\t界;\r\n\r\n/** @type {string} */\r\nexport const label = add(1, 2);\r\nexport const total=add(1,2)\r\n\r\nexport class Calculator {\r\n  /** @param {number} left @param {number} right */\r\n  sum(left, right){return add(left,right)}\r\n}\r\n\r\n/** @param {number} value */\r\nexport function describe(value){return `total=${value}`}\r\n";
const MATH_BYTES: &str = "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function add(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n";
const ORGANIZED_MAIN_BYTES: &str = "import { add } from \"./math.js\";\r\n\r\nexport const 界 = add(3, 4);\r\nexport const tabbed = 界;\r\n\r\n/** @type {string} */\r\nexport const label = add(1, 2);\r\nexport const total = add(1, 2)\r\n\r\nexport class Calculator {\r\n  /** @param {number} left @param {number} right */\r\n  sum(left, right){return add(left,right)}\r\n}\r\n\r\n/** @param {number} value */\r\nexport function describe(value){return `total=${value}`}\r\n";
const RENAMED_MAIN_BYTES: &str = "import { multiply } from \"./math.js\";\r\nimport { sum界 } from \"./math.js\";\r\n\r\nexport const 界 = sum界(3, 4);\r\nexport const tabbed =\t界;\r\n\r\n/** @type {string} */\r\nexport const label = sum界(1, 2);\r\nexport const total=sum界(1,2)\r\n\r\nexport class Calculator {\r\n  /** @param {number} left @param {number} right */\r\n  sum(left, right){return sum界(left,right)}\r\n}\r\n\r\n/** @param {number} value */\r\nexport function describe(value){return `total=${value}`}\r\n";
const RENAMED_MATH_BYTES: &str = "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n";
const FILE_RENAMED_CONFIG_BYTES: &str = "{\n  \"compilerOptions\": {\n    \"allowJs\": true,\n    \"checkJs\": true,\n    \"noEmit\": true,\n    \"strict\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"commonjs\"\n  },\n  \"files\": [\"src/main.js\", \"src/arithmetic 界.js\"]\n}\n";
const FILE_RENAMED_MAIN_BYTES: &str = "import { multiply } from \"./arithmetic 界.js\";\r\nimport { sum界 } from \"./arithmetic 界.js\";\r\n\r\nexport const 界 = sum界(3, 4);\r\nexport const tabbed =\t界;\r\n\r\n/** @type {string} */\r\nexport const label = sum界(1, 2);\r\nexport const total=sum界(1,2)\r\n\r\nexport class Calculator {\r\n  /** @param {number} left @param {number} right */\r\n  sum(left, right){return sum界(left,right)}\r\n}\r\n\r\n/** @param {number} value */\r\nexport function describe(value){return `total=${value}`}\r\n";
const REPAIRED_MAIN_BYTES: &str = "import { add } from \"./math.js\";\r\n\r\nexport const 界 = add(3, 4);\r\nexport const tabbed =\t界;\r\n\r\n/** @type {string} */\r\nexport const label = String(add(1, 2));\r\nexport const total=add(1,2)\r\n\r\nexport class Calculator {\r\n  /** @param {number} left @param {number} right */\r\n  sum(left, right){return add(left,right)}\r\n}\r\n\r\n/** @param {number} value */\r\nexport function describe(value){return `total=${value}`}\r\n";
const DIAGNOSTICS_CAPTURE: &str = include_str!("diagnostics_capture.json");
const DIAGNOSTICS_CAPTURE_ASSET_SHA256: &str =
    "156ce8e30615ff5ae52933ad6b1afd41af9f5d463b1f6f7c01b6bada978ee1de";

fn path(value: &str) -> WorkspaceRelativePath {
    WorkspaceRelativePath::new(value).expect("canonical Tide fixture path")
}

fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::parse(value).expect("literal Tide SHA-256")
}

fn assert_recorded_bytes_digest(label: &str, bytes: &str, expected: &str) {
    assert_eq!(
        Sha256Digest::of(bytes.as_bytes()),
        digest(expected),
        "recorded Tide {label} bytes drifted",
    );
}

fn ordinal(value: usize) -> RequestOrdinal {
    RequestOrdinal::new(value).expect("nonzero Tide request ordinal")
}

fn point(line: usize, offset: usize) -> LineOffset {
    LineOffset::new(line, offset).expect("one-based Tide point")
}

fn common_manifest() -> FixtureManifest {
    FixtureManifest::new(vec![
        FixtureFile::new(
            path("jsconfig.json"),
            CONFIG_BYTES.as_bytes().to_vec(),
            digest("06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c"),
        )
        .expect("recorded Tide jsconfig fixture"),
        FixtureFile::new(
            path("src/main.js"),
            MAIN_BYTES.as_bytes().to_vec(),
            digest("da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7"),
        )
        .expect("recorded Tide main fixture"),
        FixtureFile::new(
            path("src/math.js"),
            MATH_BYTES.as_bytes().to_vec(),
            digest("ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4"),
        )
        .expect("recorded Tide math fixture"),
    ])
    .expect("complete recorded Tide fixture manifest")
}

fn organized_generation() -> FixtureGeneration {
    FixtureGeneration::new(vec![
        FixtureExpectation::Present {
            path: path("jsconfig.json"),
            digest: digest("06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c"),
        },
        FixtureExpectation::Present {
            path: path("src/main.js"),
            digest: digest("6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394"),
        },
        FixtureExpectation::Present {
            path: path("src/math.js"),
            digest: digest("ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4"),
        },
    ])
    .expect("complete post-organize Tide fixture generation")
}

fn rename_generation(
    config_digest: &str,
    main_digest: &str,
    peer_path: &str,
    peer_digest: &str,
    old_peer_missing: bool,
) -> FixtureGeneration {
    let mut files = vec![
        FixtureExpectation::Present {
            path: path("jsconfig.json"),
            digest: digest(config_digest),
        },
        FixtureExpectation::Present {
            path: path("src/main.js"),
            digest: digest(main_digest),
        },
        FixtureExpectation::Present {
            path: path(peer_path),
            digest: digest(peer_digest),
        },
        FixtureExpectation::Missing(path("src/live target.js")),
        FixtureExpectation::Missing(path("src/existing target.js")),
    ];
    if old_peer_missing {
        files.push(FixtureExpectation::Missing(path("src/math.js")));
    } else {
        files.push(FixtureExpectation::Missing(path("src/arithmetic 界.js")));
    }
    FixtureGeneration::new(files).expect("complete exact Tide rename generation")
}

fn configure_request() -> TsRequest {
    configure_request_for(path("src/main.js"))
}

fn configure_request_for(file: WorkspaceRelativePath) -> TsRequest {
    TsRequest::Configure(ConfigureRequest {
        file,
        host_info: HostInfoToken::normalized(),
        format: FormatOptions {
            tab_size: std::num::NonZeroUsize::new(2).unwrap(),
            indent_size: std::num::NonZeroUsize::new(2).unwrap(),
        },
        preferences: UserPreferences {
            include_module_exports: true,
            include_insert_text: true,
            allow_new_files: true,
            generate_return_in_doc_template: true,
        },
    })
}

fn decoded_frame(
    encoded: &str,
    expected_digest: &str,
    tokens: Vec<ResponseToken>,
) -> ApprovedFrame {
    decoded_frame_with_delivery(encoded, expected_digest, tokens, DeliveryPlan::WholeFrame)
}

fn decoded_frame_with_delivery(
    encoded: &str,
    expected_digest: &str,
    tokens: Vec<ResponseToken>,
    delivery: DeliveryPlan,
) -> ApprovedFrame {
    let recorded = BASE64.decode(encoded).expect("recorded Tide frame base64");
    let (_, body) = recorded.split_at(
        recorded
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .expect("recorded Tide frame header"),
    );
    assert!(
        !body.ends_with(b"\n"),
        "portable frame body stores JSON once"
    );
    let mut body = body.to_vec();
    body.push(b'\n');
    let mut recorded = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    recorded.extend_from_slice(&body);
    ApprovedFrame::new(recorded, digest(expected_digest), delivery, tokens)
        .expect("digest-locked recorded Tide frame")
}

fn decoded_exact_frame(
    encoded: &str,
    expected_digest: &str,
    tokens: Vec<ResponseToken>,
) -> ApprovedFrame {
    ApprovedFrame::new(
        BASE64
            .decode(encoded)
            .expect("exact recorded Tide frame base64"),
        digest(expected_digest),
        DeliveryPlan::WholeFrame,
        tokens,
    )
    .expect("exact digest-locked recorded Tide frame")
}

fn recorded_json_frame(
    body: String,
    expected_digest: &str,
    delivery: DeliveryPlan,
    tokens: Vec<ResponseToken>,
) -> ApprovedFrame {
    let body = format!("{body}\n");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    ApprovedFrame::new(bytes, digest(expected_digest), delivery, tokens)
        .expect("exact generated form of a recorded Tide frame")
}

fn configure_response_frame(
    request: usize,
    expected_digest: &str,
    delivery: DeliveryPlan,
) -> ApprovedFrame {
    recorded_json_frame(
        format!(
            "{{\"seq\":0,\"type\":\"response\",\"command\":\"configure\",\"request_seq\":\"{request}\",\"success\":true}}"
        ),
        expected_digest,
        delivery,
        Vec::new(),
    )
}

fn startup_frames_for(configure_request: usize, configure_digest: &str) -> Vec<ApprovedFrame> {
    let mut frames = captured_startup_frames();
    frames.pop().expect("captured Tide configure response");
    frames.push(configure_response_frame(
        configure_request,
        configure_digest,
        DeliveryPlan::WholeFrame,
    ));
    frames
}

fn status_response_frame(
    request: usize,
    expected_digest: &str,
    delivery: DeliveryPlan,
) -> ApprovedFrame {
    recorded_json_frame(
        format!(
            "{{\"seq\":0,\"type\":\"response\",\"command\":\"status\",\"request_seq\":\"{request}\",\"success\":true,\"body\":{{\"version\":\"5.1.3\"}}}}"
        ),
        expected_digest,
        delivery,
        Vec::new(),
    )
}

fn project_info_response_frame(
    request: usize,
    expected_digest: &str,
    delivery: DeliveryPlan,
) -> ApprovedFrame {
    recorded_json_frame(
        format!(
            "{{\"seq\":0,\"type\":\"response\",\"command\":\"projectInfo\",\"request_seq\":\"{request}\",\"success\":true,\"body\":{{\"configFileName\":\"[ROOT]/jsconfig.json\",\"languageServiceDisabled\":false}}}}"
        ),
        expected_digest,
        delivery,
        vec![ResponseToken::root_path(
            vec![
                JsonPathSegment::Key("body"),
                JsonPathSegment::Key("configFileName"),
            ],
            path("jsconfig.json"),
        )],
    )
}

fn quickinfo_response_frame(
    request: usize,
    expected_digest: &str,
    delivery: DeliveryPlan,
) -> ApprovedFrame {
    const RECORDED: &str = "Q29udGVudC1MZW5ndGg6IDExMTMNCg0KeyJzZXEiOjAsInR5cGUiOiJyZXNwb25zZSIsImNvbW1hbmQiOiJxdWlja2luZm8tZnVsbCIsInJlcXVlc3Rfc2VxIjoiNCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsia2luZCI6ImFsaWFzIiwia2luZE1vZGlmaWVycyI6ImV4cG9ydCIsInRleHRTcGFuIjp7InN0YXJ0IjoxNzYsImxlbmd0aCI6M30sImRpc3BsYXlQYXJ0cyI6W3sidGV4dCI6IigiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiJhbGlhcyIsImtpbmQiOiJ0ZXh0In0seyJ0ZXh0IjoiKSIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJhZGQiLCJraW5kIjoiYWxpYXNOYW1lIn0seyJ0ZXh0IjoiKCIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6ImxlZnQiLCJraW5kIjoicGFyYW1ldGVyTmFtZSJ9LHsidGV4dCI6IjoiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiIgIiwia2luZCI6InNwYWNlIn0seyJ0ZXh0IjoibnVtYmVyIiwia2luZCI6ImtleXdvcmQifSx7InRleHQiOiIsIiwia2luZCI6InB1bmN0dWF0aW9uIn0seyJ0ZXh0IjoiICIsImtpbmQiOiJzcGFjZSJ9LHsidGV4dCI6InJpZ2h0Iiwia2luZCI6InBhcmFtZXRlck5hbWUifSx7InRleHQiOiI6Iiwia2luZCI6InB1bmN0dWF0aW9uIn0seyJ0ZXh0IjoiICIsImtpbmQiOiJzcGFjZSJ9LHsidGV4dCI6Im51bWJlciIsImtpbmQiOiJrZXl3b3JkIn0seyJ0ZXh0IjoiKSIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IjoiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiIgIiwia2luZCI6InNwYWNlIn0seyJ0ZXh0IjoibnVtYmVyIiwia2luZCI6ImtleXdvcmQifSx7InRleHQiOiJcbiIsImtpbmQiOiJsaW5lQnJlYWsifSx7InRleHQiOiJpbXBvcnQiLCJraW5kIjoia2V5d29yZCJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJhZGQiLCJraW5kIjoiYWxpYXNOYW1lIn1dLCJkb2N1bWVudGF0aW9uIjpbeyJ0ZXh0IjoiQWRkIHR3byBudW1iZXJzLiIsImtpbmQiOiJ0ZXh0In1dLCJ0YWdzIjpbeyJuYW1lIjoicGFyYW0iLCJ0ZXh0IjoibGVmdCJ9LHsibmFtZSI6InBhcmFtIiwidGV4dCI6InJpZ2h0In1dfX0K";
    let recorded = BASE64
        .decode(RECORDED)
        .expect("recorded Tide quickinfo frame base64");
    let separator = recorded
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("recorded Tide quickinfo header");
    let original = std::str::from_utf8(&recorded[separator + 4..])
        .expect("recorded Tide quickinfo body is UTF-8");
    let body = original.replacen(
        "\"request_seq\":\"4\"",
        &format!("\"request_seq\":\"{request}\""),
        1,
    );
    assert_ne!(body, original, "recorded Tide quickinfo owner was absent");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    ApprovedFrame::new(bytes, digest(expected_digest), delivery, Vec::new())
        .expect("exact re-owned recorded Tide quickinfo frame")
}

fn captured_template(body: &str) -> Vec<u8> {
    let body = format!("{body}\n");
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

#[derive(Clone)]
struct DiagnosticsFrameRecord {
    row: usize,
    exchange_owner: usize,
    delivery_after: usize,
    normalized_bytes: Vec<u8>,
    normalized_digest: Sha256Digest,
    raw_digest: Sha256Digest,
    tokens: Vec<ResponseToken>,
}

fn capture_object(value: &Value) -> &serde_json::Map<String, Value> {
    value
        .as_object()
        .expect("recorded Tide diagnostics value is an object")
}

fn capture_array(value: &Value) -> &[Value] {
    value
        .as_array()
        .expect("recorded Tide diagnostics value is an array")
}

fn capture_string<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("recorded Tide diagnostics field {key:?} is not a string"))
}

fn capture_usize(object: &serde_json::Map<String, Value>, key: &str) -> usize {
    usize::try_from(
        object
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("recorded Tide diagnostics field {key:?} is not an integer")),
    )
    .expect("recorded Tide diagnostics integer fits usize")
}

fn capture_path_segment(value: &Value) -> JsonPathSegment {
    if let Some(index) = value.as_u64() {
        return JsonPathSegment::Index(
            usize::try_from(index).expect("recorded Tide token index fits usize"),
        );
    }
    let key = value
        .as_str()
        .expect("recorded Tide token path segment is a string or integer");
    JsonPathSegment::Key(match key {
        "body" => "body",
        "configFile" => "configFile",
        "configFileName" => "configFileName",
        "diagnostics" => "diagnostics",
        "file" => "file",
        "fileNames" => "fileNames",
        "payload" => "payload",
        "projectId" => "projectId",
        "projectName" => "projectName",
        "reason" => "reason",
        "relatedInformation" => "relatedInformation",
        "span" => "span",
        "triggerFile" => "triggerFile",
        other => panic!("unapproved recorded Tide token path key: {other:?}"),
    })
}

fn capture_response_token(value: &Value) -> ResponseToken {
    let object = capture_object(value);
    let field = capture_array(
        object
            .get("field")
            .expect("recorded Tide token has a field path"),
    )
    .iter()
    .map(capture_path_segment)
    .collect::<Vec<_>>();
    match capture_string(object, "kind") {
        "root-path" => ResponseToken::root_path(field, path(capture_string(object, "relative"))),
        "embedded-root-path" => ResponseToken::embedded_root_path(
            field,
            RecordedLiteral::new(capture_string(object, "prefix")).unwrap(),
            path(capture_string(object, "relative")),
            RecordedLiteral::new(capture_string(object, "suffix")).unwrap(),
        ),
        "project-id" => ResponseToken::project_id(field, path(capture_string(object, "relative"))),
        "tsserver-bundled-path" => ResponseToken::tsserver_bundled(
            field,
            super::TsserverRelativePath::new(capture_string(object, "relative")).unwrap(),
        ),
        other => panic!("unapproved recorded Tide response token kind: {other:?}"),
    }
}

fn diagnostics_capture_records(capture: &Value) -> Vec<DiagnosticsFrameRecord> {
    capture_array(
        capture_object(capture)
            .get("frames")
            .expect("recorded Tide diagnostics capture has frames"),
    )
    .iter()
    .map(|value| {
        let object = capture_object(value);
        let normalized_bytes = BASE64
            .decode(capture_string(object, "normalized_base64"))
            .expect("recorded Tide diagnostics frame base64");
        let trailing_newline = object
            .get("trailing_newline")
            .and_then(Value::as_bool)
            .expect("recorded Tide diagnostics frame newline bit");
        assert_eq!(normalized_bytes.ends_with(b"\n"), trailing_newline);
        DiagnosticsFrameRecord {
            row: capture_usize(object, "row"),
            exchange_owner: capture_usize(object, "exchange_owner"),
            delivery_after: capture_usize(object, "delivery_after"),
            normalized_bytes,
            normalized_digest: digest(capture_string(object, "normalized_sha256")),
            raw_digest: digest(capture_string(object, "raw_sha256")),
            tokens: capture_array(
                object
                    .get("tokens")
                    .expect("recorded Tide diagnostics frame has tokens"),
            )
            .iter()
            .map(capture_response_token)
            .collect(),
        }
    })
    .collect()
}

fn diagnostics_project_frames(
    capture: &Value,
    records: &[DiagnosticsFrameRecord],
) -> BTreeMap<usize, ApprovedFrame> {
    let mut approved = BTreeMap::new();
    let evidence = capture_array(
        capture_object(capture)
            .get("project_id_evidence")
            .expect("recorded Tide diagnostics project evidence"),
    );
    assert_eq!(evidence.len(), 2);
    for value in evidence {
        let evidence = capture_object(value);
        let config = path(capture_string(evidence, "config"));
        let loading_digest = digest(capture_string(
            evidence,
            "normalized_project_loading_sha256",
        ));
        let telemetry_digest = digest(capture_string(evidence, "normalized_telemetry_sha256"));
        let loading = records
            .iter()
            .find(|frame| frame.normalized_digest == loading_digest)
            .expect("recorded Tide project-loading frame");
        let telemetry = records
            .iter()
            .find(|frame| frame.normalized_digest == telemetry_digest)
            .expect("recorded Tide project telemetry frame");
        let project_id_field = vec![
            JsonPathSegment::Key("body"),
            JsonPathSegment::Key("payload"),
            JsonPathSegment::Key("projectId"),
        ];
        let project_name_field = vec![
            JsonPathSegment::Key("body"),
            JsonPathSegment::Key("projectName"),
        ];
        let provenance = CaptureProjectIdEvidence::new(
            capture_string(evidence, "capture_config_path_base64"),
            capture_string(evidence, "raw_project_id"),
            digest(capture_string(evidence, "raw_telemetry_sha256")),
            digest(capture_string(evidence, "raw_project_loading_sha256")),
            config,
            telemetry.tokens.clone(),
            loading.tokens.clone(),
            project_id_field,
            project_name_field,
        )
        .expect("recorded Tide diagnostics ProjectId provenance");
        let frames = provenance
            .ingest(
                loading.normalized_bytes.clone(),
                loading.normalized_digest,
                DeliveryPlan::WholeFrame,
                telemetry.normalized_bytes.clone(),
                telemetry.normalized_digest,
                DeliveryPlan::WholeFrame,
            )
            .expect("reconstruct recorded Tide diagnostics project frames");
        assert!(
            approved
                .insert(loading.row, frames.project_loading)
                .is_none()
        );
        assert!(approved.insert(telemetry.row, frames.telemetry).is_none());
    }
    approved
}

fn diagnostics_approved_frames(
    capture: &Value,
    records: &[DiagnosticsFrameRecord],
) -> BTreeMap<usize, ApprovedFrame> {
    let root_object = capture_object(capture);
    assert_eq!(
        capture_string(root_object, "capture_sha256"),
        super::PINNED_DIAGNOSTICS_CAPTURE_SHA256,
    );
    assert_eq!(capture_usize(root_object, "bundle_token_count"), 736);
    assert_eq!(
        capture_usize(root_object, "bundle_evidence_frame_count"),
        426,
    );
    let manifest = capture_object(
        root_object
            .get("bundle_manifest")
            .expect("recorded Tide diagnostics bundle manifest"),
    );
    assert_eq!(manifest.len(), super::PINNED_TSSERVER_BUNDLE.len());
    for (relative, expected) in super::PINNED_TSSERVER_BUNDLE {
        assert_eq!(
            manifest.get(*relative).and_then(Value::as_str),
            Some(*expected)
        );
    }

    let mut approved = diagnostics_project_frames(capture, records);
    let mut bundle = CaptureTsserverBundleEvidence::new(
        capture_string(root_object, "bundle_capture_directory_base64"),
        capture_string(root_object, "bundle_capture_root_base64"),
        digest(capture_string(root_object, "capture_sha256")),
    )
    .expect("recorded Tide diagnostics bundle provenance");
    let affected = records
        .iter()
        .filter(|frame| {
            frame
                .tokens
                .iter()
                .any(|token| matches!(token.kind, ResponseTokenKind::TsserverBundledPath(_)))
        })
        .collect::<Vec<_>>();
    for frame in &affected {
        bundle
            .approve_frame(
                frame.row,
                frame.normalized_bytes.clone(),
                frame.normalized_digest,
                frame.raw_digest,
                DeliveryPlan::WholeFrame,
                frame.tokens.clone(),
            )
            .expect("reconstruct a recorded bundled diagnostics frame");
    }
    let sealed = bundle
        .finalize()
        .expect("complete recorded bundled diagnostics corpus")
        .into_frames();
    assert_eq!(sealed.len(), affected.len());
    for (record, frame) in affected.into_iter().zip(sealed) {
        assert!(approved.insert(record.row, frame).is_none());
    }

    for frame in records {
        if approved.contains_key(&frame.row) {
            continue;
        }
        let ordinary = ApprovedFrame::new(
            frame.normalized_bytes.clone(),
            frame.normalized_digest,
            DeliveryPlan::WholeFrame,
            frame.tokens.clone(),
        )
        .expect("approve an ordinary recorded Tide diagnostics frame");
        assert!(approved.insert(frame.row, ordinary).is_none());
    }
    assert_eq!(approved.len(), records.len());
    approved
}

fn captured_startup_frames() -> Vec<ApprovedFrame> {
    captured_startup_frames_with(
        2,
        "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123",
        [DeliveryPlan::WholeFrame; 5],
    )
}

fn captured_startup_frames_with(
    configure_request: usize,
    configure_digest: &str,
    delivery: [DeliveryPlan; 5],
) -> Vec<ApprovedFrame> {
    let config = path("jsconfig.json");
    let project_name_field = vec![
        JsonPathSegment::Key("body"),
        JsonPathSegment::Key("projectName"),
    ];
    let project_id_field = vec![
        JsonPathSegment::Key("body"),
        JsonPathSegment::Key("payload"),
        JsonPathSegment::Key("projectId"),
    ];
    let loading_tokens = vec![
        ResponseToken::root_path(project_name_field.clone(), config.clone()),
        ResponseToken::embedded_root_path(
            vec![JsonPathSegment::Key("body"), JsonPathSegment::Key("reason")],
            RecordedLiteral::new("Creating possible configured project for ").unwrap(),
            path("src/main.js"),
            RecordedLiteral::new(" to open").unwrap(),
        ),
    ];
    let telemetry_tokens = vec![ResponseToken::project_id(
        project_id_field.clone(),
        config.clone(),
    )];
    let evidence = CaptureProjectIdEvidence::new(
        "L2hvbWUvZXhlYy9Qcm9qZWN0cy9naXRodWIuY29tL2V2YWwtZXhlYy9uZW9tYWNzLXdpbmRvd3MvdG1wL3RpZGUtc3R1ZHkvcmVhbC1qcy1wcm9qZWN0IHNwYWNlIOeVjC9qc2NvbmZpZy5qc29u",
        "9d39a531fe14c923fe80bcd59e0f68d7f975ba9c3e050c7f285c49a9b14bc288",
        digest("5fa8b5422f0d4da50ddd93840cc0ab79968bb3efcd2238471d7e3f6a7ecde673"),
        digest("9c583758c2e1e0ce32857f2852a1c4542dbff2a46f4abda2106aca4c0169cc42"),
        config,
        telemetry_tokens,
        loading_tokens,
        project_id_field,
        project_name_field,
    )
    .expect("portable provenance for the real captured Tide project id");
    let loading_body = r#"{"seq":0,"type":"event","event":"projectLoadingStart","body":{"projectName":"[ROOT]/jsconfig.json","reason":"Creating possible configured project for [ROOT]/src/main.js to open"}}"#;
    let telemetry_body = r#"{"seq":0,"type":"event","event":"telemetry","body":{"telemetryEventName":"projectInfo","payload":{"projectId":"[PROJECT-ID]","fileStats":{"js":2,"jsSize":721,"jsx":0,"jsxSize":0,"ts":0,"tsSize":0,"tsx":0,"tsxSize":0,"dts":47,"dtsSize":1744378,"deferred":0,"deferredSize":0},"compilerOptions":{"allowJs":true,"maxNodeModuleJsDepth":2,"allowSyntheticDefaultImports":true,"skipLibCheck":true,"noEmit":true,"checkJs":true,"strict":true,"target":"es2020","module":"commonjs"},"typeAcquisition":{"enable":true,"include":false,"exclude":false},"extends":false,"files":true,"include":false,"exclude":false,"compileOnSave":false,"configFileName":"jsconfig.json","projectType":"configured","languageServiceEnabled":true,"version":"5.1.3"}}}"#;
    let captured = evidence
        .ingest(
            captured_template(loading_body),
            digest("7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6"),
            delivery[0],
            captured_template(telemetry_body),
            digest("8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce"),
            delivery[2],
        )
        .expect("validate and normalize the paired real Tide startup frames");
    vec![
        captured.project_loading,
        decoded_frame_with_delivery(
            "Q29udGVudC1MZW5ndGg6IDEwMQ0KDQp7InNlcSI6MCwidHlwZSI6ImV2ZW50IiwiZXZlbnQiOiJwcm9qZWN0TG9hZGluZ0ZpbmlzaCIsImJvZHkiOnsicHJvamVjdE5hbWUiOiJbUk9PVF0vanNjb25maWcuanNvbiJ9fQ==",
            "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2",
            vec![ResponseToken::root_path(
                vec![
                    JsonPathSegment::Key("body"),
                    JsonPathSegment::Key("projectName"),
                ],
                path("jsconfig.json"),
            )],
            delivery[1],
        ),
        captured.telemetry,
        decoded_frame_with_delivery(
            "Q29udGVudC1MZW5ndGg6IDE0Ng0KDQp7InNlcSI6MCwidHlwZSI6ImV2ZW50IiwiZXZlbnQiOiJjb25maWdGaWxlRGlhZyIsImJvZHkiOnsidHJpZ2dlckZpbGUiOiJbUk9PVF0vc3JjL21haW4uanMiLCJjb25maWdGaWxlIjoiW1JPT1RdL2pzY29uZmlnLmpzb24iLCJkaWFnbm9zdGljcyI6W119fQ==",
            "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8",
            vec![
                ResponseToken::root_path(
                    vec![
                        JsonPathSegment::Key("body"),
                        JsonPathSegment::Key("triggerFile"),
                    ],
                    path("src/main.js"),
                ),
                ResponseToken::root_path(
                    vec![
                        JsonPathSegment::Key("body"),
                        JsonPathSegment::Key("configFile"),
                    ],
                    path("jsconfig.json"),
                ),
            ],
            delivery[3],
        ),
        configure_response_frame(configure_request, configure_digest, delivery[4]),
    ]
}

fn startup_exchanges(generation: &FixtureGeneration) -> Vec<ReplayExchange> {
    vec![
        RecordedExchange::new(
            ordinal(1),
            TsRequest::Open(
                OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
            ),
            generation.clone(),
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(2),
            configure_request(),
            generation.clone(),
            ApprovedOutput::frames(ordinal(2), captured_startup_frames()).unwrap(),
        )
        .unwrap()
        .into(),
    ]
}

fn open_exchange(
    request: usize,
    file: WorkspaceRelativePath,
    generation: &FixtureGeneration,
    manual_content: Option<&str>,
) -> ReplayExchange {
    let open = match manual_content {
        Some(content) => OpenRequest::manual(file, ScriptKind::JavaScript, content.to_owned()),
        None => OpenRequest::immediate(file, ScriptKind::JavaScript),
    }
    .expect("recorded Tide JavaScript open request");
    RecordedExchange::new(
        ordinal(request),
        TsRequest::Open(open),
        generation.clone(),
        ApprovedOutput::no_frames(),
    )
    .unwrap()
    .into()
}

fn configure_exchange(
    request: usize,
    file: WorkspaceRelativePath,
    generation: &FixtureGeneration,
    delivery_after: usize,
    frames: Vec<ApprovedFrame>,
) -> ReplayExchange {
    let output = if request == delivery_after {
        ApprovedOutput::frames(ordinal(request), frames).unwrap()
    } else {
        ApprovedOutput::frames_delayed(ordinal(delivery_after), frames).unwrap()
    };
    let exchange = if request == delivery_after {
        RecordedExchange::new(
            ordinal(request),
            configure_request_for(file),
            generation.clone(),
            output,
        )
    } else {
        RecordedExchange::new_delayed(
            ordinal(request),
            configure_request_for(file),
            generation.clone(),
            output,
        )
    };
    exchange.unwrap().into()
}

fn materialized_case(
    id: &'static str,
    replay: TideReplay,
    body: &'static str,
    expected: Expect,
) -> ParityBatchCase {
    let runtime = ReplayRuntimeIdentity::preflight().expect("exact Tide replay runtime");
    let body_source = serde_json::to_string(body).expect("encode the exact Tide workflow body");
    let probe = format!(
        "(progn (tide368-test-assert-workflow-body-reader-contract) (tide368-test-run '{} {} {} (tide368-test-read-workflow-body {})))",
        replay.scenario.symbol(),
        replay.elisp_summary(),
        replay.artifacts(&runtime).elisp_plan(),
        body_source,
    );
    ParityBatchCase::value(id, probe, expected)
}

const LIFECYCLE_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((wait-until
        (predicate process label)
        (let ((deadline (+ (float-time) 20.0)))
          (while (and (not (funcall predicate)) (< (float-time) deadline))
            (accept-process-output process 0.02))
          (unless (funcall predicate)
            (error "Tide lifecycle wait failed: %S" label))))
       (relative-file
        (file root)
        (and file (file-relative-name file root)))
       (normalize-command
        (command world)
        (mapcar
         (lambda (value)
           (cond ((equal value (plist-get world :adapter)) "[ADAPTER]")
                 ((equal value (plist-get world :server)) "[TSSERVER]")
                 (t value)))
         command))
       (process-state
        (process root world)
        (list :live (process-live-p process)
              :type (process-type process)
              :buffer-live (buffer-live-p (process-buffer process))
              :command (normalize-command (process-command process) world)
              :cwd (and (equal (process-get process 'project-root)
                               (file-name-as-directory root))
                        "[ROOT]/")
              :project (equal (process-get process 'project-name)
                              (tide-project-name))
              :coding (process-coding-system process)
              :filter (eq (process-filter process) #'tide-net-filter)
              :sentinel (eq (process-sentinel process) #'tide-net-sentinel)
              :query (process-query-on-exit-flag process)))
       (buffer-state
        (buffer root)
        (with-current-buffer buffer
          (list :mode tide-mode :lighter (assq 'tide-mode minor-mode-alist)
                :file (relative-file (tide-buffer-file-name) root)
                :active-file (relative-file tide-active-buffer-file-name root)
                :tab-width tab-width :indent js-indent-level
                :after-save
                (list (and (memq #'tide-sync-buffer-contents after-save-hook) t)
                      (and (memq #'tide-auto-compile-file after-save-hook) t))
                :after-change
                (and (memq #'tide-handle-change after-change-functions) t)
                :kill
                (list (and (memq #'tide-cleanup-buffer kill-buffer-hook) t)
                      (and (memq #'tide-schedule-dead-projects-cleanup
                                 kill-buffer-hook) t))
                :eldoc (and (memq #'tide-eldoc-function
                                   eldoc-documentation-functions) t)
                :imenu (eq imenu-create-index-function #'tide-imenu-index)
                :xref (and (memq #'xref-tide-xref-backend
                                  xref-backend-functions) t)
                :manual-local (local-variable-p 'tide-require-manual-setup)
                :manual tide-require-manual-setup)))
       (normalized-text
        (text root)
        (replace-regexp-in-string
         (regexp-quote (file-name-as-directory root)) "[ROOT]/" text t t))
       (normalized-position
        (buffer position root)
        (with-current-buffer buffer
          (+ (point-min)
             (length
              (normalized-text
               (buffer-substring-no-properties (point-min) position)
               root)))))
       (face-runs
        (buffer root)
        (with-current-buffer buffer
          (let ((position (point-min)) runs)
            (while (< position (point-max))
              (let* ((value (get-text-property position 'face))
                     (next (or (next-single-property-change
                                position 'face nil (point-max))
                               (point-max))))
                (when value
                  (push (list (normalized-position buffer position root)
                              (normalized-position buffer next root)
                              (copy-tree value))
                        runs))
                (setq position next)))
            (nreverse runs))))
       (project-info-state
        (root process)
        (wait-until (lambda () (get-buffer "*tide-project-info*"))
                    process 'project-info)
        (let ((buffer (get-buffer "*tide-project-info*")))
          (with-current-buffer buffer
            (list :mode major-mode :read-only buffer-read-only
                  :point (normalized-position buffer (point) root)
                  :text (normalized-text
                         (buffer-substring-no-properties (point-min) (point-max))
                         root)
                  :faces (face-runs buffer root)))))
       (kill-publicly
        (process command label)
        (wait-until
         (lambda ()
           (tide368-test-terminal-record
            (process-get process 'tide368-session-index)))
         process (list label 'ready))
        (funcall command)
        (wait-until (lambda () (not (process-live-p process))) process label)
        (not (process-live-p process)))
       (list-state
        (root world)
        (let ((rows
               (mapcar
                (lambda (entry)
                  (let* ((process (car entry)) (columns (cadr entry))
                         (project-cell (aref columns 0))
                         (project-text (if (stringp project-cell)
                                           project-cell
                                         (car project-cell)))
                         (project-properties
                          (if (stringp project-cell)
                              (text-properties-at 0 project-cell)
                            (cdr project-cell)))
                         (cpu (aref columns 1)) (last (aref columns 2)))
                    (unless (or (string= cpu "--")
                                (string-match-p "\\`[0-9]+\\'" cpu))
                      (error "Tide server CPU column is invalid: %S" cpu))
                    (list :process-current (eq process (tide-current-server))
                          :project-cell
                          (list
                           :text (equal project-text (tide-project-name))
                           :face (plist-get project-properties 'face)
                           :help
                           (equal (plist-get project-properties 'help-echo)
                                  (format-message "Verify setup of `%s'"
                                                  (tide-project-name)))
                           :follow-link
                           (plist-get project-properties 'follow-link)
                           :project-name
                           (equal (plist-get project-properties 'project-name)
                                  (tide-project-name))
                           :action
                           (eq (plist-get project-properties 'action)
                               #'tide--list-servers-verify-setup))
                          :cpu "[CPU]"
                          :last
                          (cond
                           ((equal last (file-name-as-directory root)) "[ROOT]/")
                           (t (mapconcat #'identity
                                         (normalize-command
                                          (split-string last " " t) world)
                                         " "))))))
                tabulated-list-entries)))
          (list :column tide--server-list-mode-last-column
                :format
                (mapcar (lambda (column)
                          (list (elt column 0) (elt column 1)))
                        (append tabulated-list-format nil))
                :rows rows))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (math (expand-file-name "src/math.js" root))
           (main-buffer (find-file-noselect main))
           (math-buffer (find-file-noselect math))
           immediate verify-info root-list command-list list-removed
           restart-dead restart-live ordinary-before ordinary-after
           indirect-before indirect-after immediate-order shared
           first second third fourth fifth sixth indirect)
      (switch-to-buffer main-buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (setq tide-tsserver-start-method 'immediate)
      (tide-setup)
      (setq first (tide368-test-assert-current-server)
            immediate (list :buffer (buffer-state main-buffer root)
                            :process (process-state first root world)))
      (tide-verify-setup)
      (setq verify-info (project-info-state root first))
      (tide-list-servers)
      (let ((list-buffer (get-buffer "*Tide Server List*")))
        (unless list-buffer (error "Tide public server list was not created"))
        (pop-to-buffer list-buffer)
        (goto-char (point-min))
        (setq root-list (list-state root world))
        (execute-kbd-macro (kbd "/"))
        (setq command-list (list-state root world))
        (goto-char (point-min))
        (wait-until
         (lambda ()
           (tide368-test-terminal-record
            (process-get first 'tide368-session-index)))
         first '(list-kill ready))
        (execute-kbd-macro (kbd "d")))
      (wait-until (lambda () (not (process-live-p first))) first 'list-kill)
      (with-current-buffer (get-buffer "*Tide Server List*")
        (revert-buffer)
        (setq list-removed (null tabulated-list-entries)))
      (switch-to-buffer main-buffer)
      (tide-restart-server)
      (setq second (tide368-test-assert-current-server)
            restart-dead (process-state second root world))
      (wait-until
       (lambda ()
         (tide368-test-terminal-record
          (process-get second 'tide368-session-index)))
       second '(restart-live ready))
      (tide-restart-server)
      (setq third (tide368-test-assert-current-server)
            restart-live (process-state third root world))
      (when (or (eq first second) (eq second third) (eq first third))
        (error "Tide lifecycle reused a process across public restarts"))
      (kill-publicly third #'tide-kill-server 'kill-third)

      (tide-mode -1)
      (setq tide-tsserver-start-method 'manual)
      (let ((counter tide-request-counter))
        (tide-setup)
        (setq ordinary-before
              (list :buffer (buffer-state main-buffer root)
                    :server (tide-current-server)
                    :counter tide-request-counter))
        (unless (and (null (tide-current-server))
                     (= counter tide-request-counter))
          (error "Tide ordinary manual setup started a server")))
      (tide-restart-server)
      (setq fourth (tide368-test-assert-current-server)
            ordinary-after (process-state fourth root world))
      (kill-publicly fourth #'tide-kill-server 'kill-fourth)
      (with-current-buffer main-buffer (tide-mode -1))

      (switch-to-buffer main-buffer)
      (setq indirect (clone-indirect-buffer " *tide368-manual*" nil))
      (switch-to-buffer indirect)
      (unless (and (null buffer-file-name)
                   (eq (buffer-base-buffer) main-buffer))
        (error "Tide manual-open buffer is not genuinely base-backed"))
      (setq tide-default-mode "JS" tide-tsserver-start-method 'manual)
      (let ((counter tide-request-counter))
        (tide-setup)
        (setq indirect-before
              (list :buffer (buffer-state indirect root)
                    :base (eq (buffer-base-buffer) main-buffer)
                    :content (buffer-substring-no-properties
                              (point-min) (point-max))
                    :counter tide-request-counter))
        (unless (and tide-require-manual-setup
                     (local-variable-p 'tide-require-manual-setup)
                     (= counter tide-request-counter))
          (error "Tide genuine manual-open setup boundary drifted")))
      (tide-restart-server)
      (setq fifth (tide368-test-assert-current-server)
            indirect-after
            (list :process (process-state fifth root world)
                  :buffer (buffer-state indirect root)))
      (kill-publicly fifth #'tide-kill-server 'kill-fifth)
      (with-current-buffer indirect (tide-mode -1))
      (kill-buffer indirect)

      (setq tide-tsserver-start-method 'manual)
      (switch-to-buffer main-buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (tide-setup)
      (switch-to-buffer math-buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (tide-setup)
      (switch-to-buffer main-buffer)
      (setq immediate-order
            (mapcar (lambda (buffer)
                      (with-current-buffer buffer
                        (relative-file (tide-buffer-file-name) root)))
                    (seq-filter
                     (lambda (buffer)
                       (with-current-buffer buffer
                         (and (bound-and-true-p tide-mode)
                              (equal (tide-project-name)
                                     (with-current-buffer main-buffer
                                       (tide-project-name))))))
                     (buffer-list))))
      (setq tide-tsserver-start-method 'immediate)
      (tide-setup)
      (setq sixth (tide368-test-assert-current-server)
            shared (list :process (process-state sixth root world)
                         :main (buffer-state main-buffer root)
                         :math (buffer-state math-buffer root)
                         :same-server
                         (and (with-current-buffer main-buffer
                                (eq (tide-current-server) sixth))
                              (with-current-buffer math-buffer
                                (eq (tide-current-server) sixth)))))
      (kill-publicly sixth #'tide-kill-server 'kill-sixth)
      (list :immediate immediate :verify verify-info
            :server-list (list :root root-list :command command-list
                               :removed list-removed)
            :restarts (list :dead restart-dead :live restart-live)
            :ordinary-manual (list :before ordinary-before
                                   :after ordinary-after)
            :indirect-manual (list :before indirect-before
                                   :after indirect-after)
            :shared-immediate (list :order immediate-order :state shared)
            :process-distinct
            (= (length (delete-dups (list first second third fourth fifth sixth))) 6)
            :all-dead
            (cl-every (lambda (process) (not (process-live-p process)))
                      (list first second third fourth fifth sixth))
            :request-counter tide-request-counter
            :callbacks (hash-table-count tide-response-callbacks)
            :servers (hash-table-count tide-servers)))))"#;

fn setup_verify_list_kill_and_restart() -> ParityBatchCase {
    let fixtures = common_manifest();
    let generation = fixtures.generation();
    let manual_main_content = MAIN_BYTES.replace("\r\n", "\n");
    let sessions = vec![
        ReplaySession::new(
            vec![
                open_exchange(1, path("src/main.js"), &generation, None),
                configure_exchange(
                    2,
                    path("src/main.js"),
                    &generation,
                    3,
                    startup_frames_for(
                        2,
                        "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123",
                    ),
                ),
                RecordedExchange::new(
                    ordinal(3),
                    TsRequest::Status,
                    generation.clone(),
                    ApprovedOutput::frames(
                        ordinal(3),
                        vec![status_response_frame(
                            3,
                            "4c3161826b2a2eeeca691adf7750c5e467869fe0e31bbd1abc6e95a2068118aa",
                            DeliveryPlan::WholeFrame,
                        )],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
                RecordedExchange::new(
                    ordinal(4),
                    TsRequest::ProjectInfo(ProjectInfoRequest {
                        file: path("src/main.js"),
                        file_names: FileNameListRequest::Null,
                    }),
                    generation.clone(),
                    ApprovedOutput::frames(
                        ordinal(4),
                        vec![project_info_response_frame(
                            4,
                            "301b7820d5de76949740aff780c5b81356fc87bbf1652d138e097dbd5dba13ea",
                            DeliveryPlan::WholeFrame,
                        )],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
            ],
            digest("a6a36e2ba8eace1a0042d25d923b94b662e15c5d2fe3644ec0bf70361e5ed177"),
            digest("c2220d1e9227abc00ebd0dc8f1bed601bcb0dcef9aba373a32886928d8de29b6"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(4),
            },
        )
        .unwrap(),
        ReplaySession::new(
            vec![
                open_exchange(5, path("src/main.js"), &generation, None),
                configure_exchange(
                    6,
                    path("src/main.js"),
                    &generation,
                    6,
                    startup_frames_for(
                        6,
                        "87d87fc3635e0f92d1b2eef40c7403aef1b0d226cab5a5dc7dcf5bbfb3f3b314",
                    ),
                ),
            ],
            digest("e6595dd271c4e54c5d998ec681f9296223fb60442794a186baae6d9cec18d7f4"),
            digest("490ba16ed1a4a5d32ab3dffd785035d5650222c1497a22a7bb653c14b7522b59"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(6),
            },
        )
        .unwrap(),
        ReplaySession::new(
            vec![
                open_exchange(7, path("src/main.js"), &generation, None),
                configure_exchange(
                    8,
                    path("src/main.js"),
                    &generation,
                    8,
                    startup_frames_for(
                        8,
                        "dadd4089846b33b96a59cba05e3628d60c1c9e4e6fffb14cc06046e74075e375",
                    ),
                ),
            ],
            digest("8382294fdae59919f3fff952c2bad9891628b71e465bdb89f363bea297338a56"),
            digest("6aa7915daf17d2843c3f4d78e0ed8c09091750a784133d6e0ec5388f6ba872cc"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(8),
            },
        )
        .unwrap(),
        ReplaySession::new(
            vec![
                open_exchange(9, path("src/main.js"), &generation, None),
                configure_exchange(
                    10,
                    path("src/main.js"),
                    &generation,
                    10,
                    startup_frames_for(
                        10,
                        "547cb3a0a10b3e2262133db2aa9c8f4011b5dcdb3eef38ff954832aca6d9cd5d",
                    ),
                ),
            ],
            digest("c1ce88244dfe2ef943ec806eab9bc7d2d264393a4c5bc1f9475c94b86d4e039a"),
            digest("a149cfebedad56fff0c2074be5342d3d15ff5b1d3fcc60fa0ea1455232b2872c"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(10),
            },
        )
        .unwrap(),
        ReplaySession::new(
            vec![
                open_exchange(
                    11,
                    path("src/main.js"),
                    &generation,
                    Some(&manual_main_content),
                ),
                configure_exchange(
                    12,
                    path("src/main.js"),
                    &generation,
                    13,
                    startup_frames_for(
                        12,
                        "7a843bdb06bc87c495a7d2db73eec248d14ebaf9457a3e2dd765d71ee24eccde",
                    ),
                ),
                RecordedExchange::new(
                    ordinal(13),
                    TsRequest::ProjectInfo(ProjectInfoRequest {
                        file: path("src/main.js"),
                        file_names: FileNameListRequest::Null,
                    }),
                    generation.clone(),
                    ApprovedOutput::frames(
                        ordinal(13),
                        vec![project_info_response_frame(
                            13,
                            "2894878e00b55860b569219fc00b5c782a5626e5462f01611a8f4f21f4e09c7c",
                            DeliveryPlan::WholeFrame,
                        )],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
            ],
            digest("94fd9c97f8685f7c99bb87d2aa71fd601af17f814f2251bb99b0e60541a50a29"),
            digest("e654b04cd2700c4434914b1294031c05e2af36cdb8980ecff16f567878fb508c"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(13),
            },
        )
        .unwrap(),
        ReplaySession::new(
            vec![
                open_exchange(14, path("src/main.js"), &generation, None),
                configure_exchange(
                    15,
                    path("src/main.js"),
                    &generation,
                    15,
                    startup_frames_for(
                        15,
                        "e0deb4c09c55bf79c9ba4f16e871845eb4b6150bea68b2666a74a726854cbe22",
                    ),
                ),
                open_exchange(16, path("src/math.js"), &generation, None),
                configure_exchange(
                    17,
                    path("src/math.js"),
                    &generation,
                    17,
                    vec![configure_response_frame(
                        17,
                        "412cc2bab76c7bbefeb4381be4b29cbb950941afcc35374942e8dbc0d0370f32",
                        DeliveryPlan::WholeFrame,
                    )],
                ),
            ],
            digest("5ae869ba0e6cf149c6a02e290175d8c5a9d6730389c4d19789f2350385d59665"),
            digest("863e8558db2e0eb7b728f749a99af5a9657aee76dc7dfd4de39da6d12f700c4e"),
            ReplayTermination::ClientKilled {
                ready_after: ordinal(17),
            },
        )
        .unwrap(),
    ];
    let replay = TideReplay::new(TideScenario::Lifecycle, fixtures, sessions).unwrap();
    materialized_case(
        "setup_verify_list_kill_and_restart",
        replay,
        LIFECYCLE_BODY,
        expect![[
            r#"OK (:result (:immediate (:buffer (:mode t :lighter #2=(tide-mode " tide") :file "src/main.js" :active-file "src/main.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local nil :manual nil) :process (:live #1=(run open listen connect stop) :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil)) :verify (:mode special-mode :read-only t :point 64 :text "tsserver version: 5.1.3\n\nconfig file path: [ROOT]/jsconfig.json" :faces ((19 24 (success bold)) (44 64 success))) :server-list (:root (:column project-root :format (("Project Name" 20) ("CPU" 5) ("Project Root" 0)) :rows ((:process-current t :project-cell (:text t :face link :help t :follow-link t :project-name t :action t) :cpu "[CPU]" :last "[ROOT]/"))) :command (:column command :format (("Project Name" 20) ("CPU" 5) ("Project Command" 0)) :rows ((:process-current t :project-cell (:text t :face link :help t :follow-link t :project-name t :action t) :cpu "[CPU]" :last "[ADAPTER] [TSSERVER] --disableAutomaticTypingAcquisition"))) :removed t) :restarts (:dead (:live #1# :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil) :live (:live #1# :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil)) :ordinary-manual (:before (:buffer (:mode t :lighter #2# :file "src/main.js" :active-file "src/main.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local nil :manual nil) :server nil :counter 8) :after (:live #1# :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil)) :indirect-manual (:before (:buffer (:mode t :lighter #2# :file "src/main.js" :active-file "src/main.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local t :manual t) :base t :content "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :counter 10) :after (:process (:live #1# :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil) :buffer (:mode t :lighter #2# :file "src/main.js" :active-file "src/main.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local t :manual t))) :shared-immediate (:order ("src/main.js" "src/math.js") :state (:process (:live #1# :type real :buffer-live t :command ("[ADAPTER]" "[TSSERVER]" "--disableAutomaticTypingAcquisition") :cwd "[ROOT]/" :project t :coding (utf-8-unix . utf-8-unix) :filter t :sentinel t :query nil) :main (:mode t :lighter #2# :file "src/main.js" :active-file "src/main.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local nil :manual nil) :math (:mode t :lighter #2# :file "src/math.js" :active-file "src/math.js" :tab-width 2 :indent 2 :after-save (t t) :after-change t :kill (t t) :eldoc t :imenu t :xref t :manual-local nil :manual nil) :same-server t)) :process-distinct t :all-dead t :request-counter 17 :callbacks 0 :servers 0) :typed (:scenario lifecycle :fixture-count 3 :session-count 6 :sessions ((:first-ordinal 1 :requests (open configure status projectInfo) :request-count 4 :frame-count 7 :request-sha256 "a6a36e2ba8eace1a0042d25d923b94b662e15c5d2fe3644ec0bf70361e5ed177" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 3 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "status") :bytes 130 :sha256 "4c3161826b2a2eeeca691adf7750c5e467869fe0e31bbd1abc6e95a2068118aa" :delivery whole-frame))) :json "{\"command\":\"status\",\"seq\":\"3\",\"arguments\":null}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 4 :frames ((:kind response :owner (:response 4 "projectInfo") :bytes 189 :sha256 "301b7820d5de76949740aff780c5b81356fc87bbf1652d138e097dbd5dba13ea" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":null}}")) :termination (:client-killed :ready-after 4)) (:first-ordinal 5 :requests (open configure) :request-count 2 :frame-count 5 :request-sha256 "e6595dd271c4e54c5d998ec681f9296223fb60442794a186baae6d9cec18d7f4" :recordings ((:ordinal 5 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 6 :outcome complete :callback not-registered :output (:delivery-after 6 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 6 "configure") :bytes 105 :sha256 "87d87fc3635e0f92d1b2eef40c7403aef1b0d226cab5a5dc7dcf5bbfb3f3b314" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"6\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}")) :termination (:client-killed :ready-after 6)) (:first-ordinal 7 :requests (open configure) :request-count 2 :frame-count 5 :request-sha256 "8382294fdae59919f3fff952c2bad9891628b71e465bdb89f363bea297338a56" :recordings ((:ordinal 7 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"7\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 8 :outcome complete :callback not-registered :output (:delivery-after 8 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 8 "configure") :bytes 105 :sha256 "dadd4089846b33b96a59cba05e3628d60c1c9e4e6fffb14cc06046e74075e375" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"8\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}")) :termination (:client-killed :ready-after 8)) (:first-ordinal 9 :requests (open configure) :request-count 2 :frame-count 5 :request-sha256 "c1ce88244dfe2ef943ec806eab9bc7d2d264393a4c5bc1f9475c94b86d4e039a" :recordings ((:ordinal 9 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"9\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 10 :outcome complete :callback not-registered :output (:delivery-after 10 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 10 "configure") :bytes 106 :sha256 "547cb3a0a10b3e2262133db2aa9c8f4011b5dcdb3eef38ff954832aca6d9cd5d" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"10\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}")) :termination (:client-killed :ready-after 10)) (:first-ordinal 11 :requests (open configure projectInfo) :request-count 3 :frame-count 6 :request-sha256 "94fd9c97f8685f7c99bb87d2aa71fd601af17f814f2251bb99b0e60541a50a29" :recordings ((:ordinal 11 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"11\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\",\"fileContent\":\"import { multiply } from \\\"./math.js\\\";\\nimport { add } from \\\"./math.js\\\";\\n\\nexport const 界 = add(3, 4);\\nexport const tabbed =\\t界;\\n\\n/** @type {string} */\\nexport const label = add(1, 2);\\nexport const total=add(1,2)\\n\\nexport class Calculator {\\n  /** @param {number} left @param {number} right */\\n  sum(left, right){return add(left,right)}\\n}\\n\\n/** @param {number} value */\\nexport function describe(value){return `total=${value}`}\\n\"}}") (:ordinal 12 :outcome complete :callback not-registered :output (:delivery-after 13 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 12 "configure") :bytes 106 :sha256 "7a843bdb06bc87c495a7d2db73eec248d14ebaf9457a3e2dd765d71ee24eccde" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"12\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 13 :outcome complete :callback registered :output (:delivery-after 13 :frames ((:kind response :owner (:response 13 "projectInfo") :bytes 190 :sha256 "2894878e00b55860b569219fc00b5c782a5626e5462f01611a8f4f21f4e09c7c" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"13\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":null}}")) :termination (:client-killed :ready-after 13)) (:first-ordinal 14 :requests (open configure open configure) :request-count 4 :frame-count 6 :request-sha256 "5ae869ba0e6cf149c6a02e290175d8c5a9d6730389c4d19789f2350385d59665" :recordings ((:ordinal 14 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"14\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 15 :outcome complete :callback not-registered :output (:delivery-after 15 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 15 "configure") :bytes 106 :sha256 "e0deb4c09c55bf79c9ba4f16e871845eb4b6150bea68b2666a74a726854cbe22" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"15\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 16 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"16\",\"arguments\":{\"file\":\"[ROOT]/src/math.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 17 :outcome complete :callback not-registered :output (:delivery-after 17 :frames ((:kind response :owner (:response 17 "configure") :bytes 106 :sha256 "412cc2bab76c7bbefeb4381be4b29cbb950941afcc35374942e8dbc0d0370f32" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"17\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/math.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}")) :termination (:client-killed :ready-after 17)))) :launches ((:name "tsserver" :buffer "*tide-server*" :program #3=[ADAPTER] :arguments (#4=[TSSERVER] "--disableAutomaticTypingAcquisition") :cwd #5=[ROOT] :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #3# :arguments (#4# "--disableAutomaticTypingAcquisition") :cwd #5# :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #3# :arguments (#4# "--disableAutomaticTypingAcquisition") :cwd #5# :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #3# :arguments (#4# "--disableAutomaticTypingAcquisition") :cwd #5# :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #3# :arguments (#4# "--disableAutomaticTypingAcquisition") :cwd #5# :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #3# :arguments (#4# "--disableAutomaticTypingAcquisition") :cwd #5# :environment-count 23)) :terminals ((:session 1 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 2 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 3 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 4 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 5 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 6 :status signal :exit 9 :message "killed\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "status" :callback registered) (:ordinal 4 :command "projectInfo" :callback registered) (:ordinal 5 :command "open" :callback not-registered) (:ordinal 6 :command "configure" :callback not-registered) (:ordinal 7 :command "open" :callback not-registered) (:ordinal 8 :command "configure" :callback not-registered) (:ordinal 9 :command "open" :callback not-registered) (:ordinal 10 :command "configure" :callback not-registered) (:ordinal 11 :command "open" :callback not-registered) (:ordinal 12 :command "configure" :callback not-registered) (:ordinal 13 :command "projectInfo" :callback registered) (:ordinal 14 :command "open" :callback not-registered) (:ordinal 15 :command "configure" :callback not-registered) (:ordinal 16 :command "open" :callback not-registered) (:ordinal 17 :command "configure" :callback not-registered)) :public-deletes ((:session 1 :route server-list-kill) (:session 2 :route restart-server) (:session 3 :route kill-server) (:session 4 :route kill-server) (:session 5 :route kill-server) (:session 6 :route kill-server)) :cleanup clean)"#
        ]],
    )
}

fn documentation_imenu_definition_back_and_named_navigation() -> ParityBatchCase {
    let fixtures = common_manifest();
    let generation = fixtures.generation();
    let mut exchanges = startup_exchanges(&generation);
    exchanges.extend([
        RecordedExchange::new(
            ordinal(3),
            TsRequest::Definition(PointRequest {
                file: path("src/main.js"),
                point: point(8, 23),
            }),
            generation.clone(),
            ApprovedOutput::frames(ordinal(3), vec![decoded_exact_frame(
                "Q29udGVudC1MZW5ndGg6IDI1NQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImRlZmluaXRpb24iLCJyZXF1ZXN0X3NlcSI6IjMiLCJzdWNjZXNzIjp0cnVlLCJib2R5IjpbeyJmaWxlIjoiW1JPT1RdL3NyYy9tYXRoLmpzIiwic3RhcnQiOnsibGluZSI6Niwib2Zmc2V0IjoxN30sImVuZCI6eyJsaW5lIjo2LCJvZmZzZXQiOjIwfSwiY29udGV4dFN0YXJ0Ijp7ImxpbmUiOjYsIm9mZnNldCI6MX0sImNvbnRleHRFbmQiOnsibGluZSI6OCwib2Zmc2V0IjoyfX1dfQo=",
                "cd41f45fa3d2cdccf3926c4eeb14b4611dff8d125b1852d099e68ff3d6faa725",
                vec![ResponseToken::root_path(
                    vec![
                        JsonPathSegment::Key("body"),
                        JsonPathSegment::Index(0),
                        JsonPathSegment::Key("file"),
                    ],
                    path("src/math.js"),
                )],
            )])
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(4),
            TsRequest::QuickInfoFull(PointRequest {
                file: path("src/main.js"),
                point: point(8, 23),
            }),
            generation.clone(),
            ApprovedOutput::frames(ordinal(4), vec![decoded_exact_frame(
                "Q29udGVudC1MZW5ndGg6IDExMTMNCg0KeyJzZXEiOjAsInR5cGUiOiJyZXNwb25zZSIsImNvbW1hbmQiOiJxdWlja2luZm8tZnVsbCIsInJlcXVlc3Rfc2VxIjoiNCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsia2luZCI6ImFsaWFzIiwia2luZE1vZGlmaWVycyI6ImV4cG9ydCIsInRleHRTcGFuIjp7InN0YXJ0IjoxNzYsImxlbmd0aCI6M30sImRpc3BsYXlQYXJ0cyI6W3sidGV4dCI6IigiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiJhbGlhcyIsImtpbmQiOiJ0ZXh0In0seyJ0ZXh0IjoiKSIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJhZGQiLCJraW5kIjoiYWxpYXNOYW1lIn0seyJ0ZXh0IjoiKCIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6ImxlZnQiLCJraW5kIjoicGFyYW1ldGVyTmFtZSJ9LHsidGV4dCI6IjoiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiIgIiwia2luZCI6InNwYWNlIn0seyJ0ZXh0IjoibnVtYmVyIiwia2luZCI6ImtleXdvcmQifSx7InRleHQiOiIsIiwia2luZCI6InB1bmN0dWF0aW9uIn0seyJ0ZXh0IjoiICIsImtpbmQiOiJzcGFjZSJ9LHsidGV4dCI6InJpZ2h0Iiwia2luZCI6InBhcmFtZXRlck5hbWUifSx7InRleHQiOiI6Iiwia2luZCI6InB1bmN0dWF0aW9uIn0seyJ0ZXh0IjoiICIsImtpbmQiOiJzcGFjZSJ9LHsidGV4dCI6Im51bWJlciIsImtpbmQiOiJrZXl3b3JkIn0seyJ0ZXh0IjoiKSIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IjoiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiIgIiwia2luZCI6InNwYWNlIn0seyJ0ZXh0IjoibnVtYmVyIiwia2luZCI6ImtleXdvcmQifSx7InRleHQiOiJcbiIsImtpbmQiOiJsaW5lQnJlYWsifSx7InRleHQiOiJpbXBvcnQiLCJraW5kIjoia2V5d29yZCJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJhZGQiLCJraW5kIjoiYWxpYXNOYW1lIn1dLCJkb2N1bWVudGF0aW9uIjpbeyJ0ZXh0IjoiQWRkIHR3byBudW1iZXJzLiIsImtpbmQiOiJ0ZXh0In1dLCJ0YWdzIjpbeyJuYW1lIjoicGFyYW0iLCJ0ZXh0IjoibGVmdCJ9LHsibmFtZSI6InBhcmFtIiwidGV4dCI6InJpZ2h0In1dfX0K",
                "39df65cab6b06cf08a1397462e8f3779692b4eac6aefcf30a720ae969e541e55",
                Vec::new(),
            )])
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(5),
            TsRequest::SignatureHelp(PointRequest {
                file: path("src/main.js"),
                point: point(8, 28),
            }),
            generation.clone(),
            ApprovedOutput::frames(ordinal(5), vec![decoded_exact_frame(
                "Q29udGVudC1MZW5ndGg6IDExOTANCg0KeyJzZXEiOjAsInR5cGUiOiJyZXNwb25zZSIsImNvbW1hbmQiOiJzaWduYXR1cmVIZWxwIiwicmVxdWVzdF9zZXEiOiI1Iiwic3VjY2VzcyI6dHJ1ZSwiYm9keSI6eyJpdGVtcyI6W3siaXNWYXJpYWRpYyI6ZmFsc2UsInByZWZpeERpc3BsYXlQYXJ0cyI6W3sidGV4dCI6ImFkZCIsImtpbmQiOiJhbGlhc05hbWUifSx7InRleHQiOiIoIiwia2luZCI6InB1bmN0dWF0aW9uIn1dLCJzdWZmaXhEaXNwbGF5UGFydHMiOlt7InRleHQiOiIpIiwia2luZCI6InB1bmN0dWF0aW9uIn0seyJ0ZXh0IjoiOiIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJudW1iZXIiLCJraW5kIjoia2V5d29yZCJ9XSwic2VwYXJhdG9yRGlzcGxheVBhcnRzIjpbeyJ0ZXh0IjoiLCIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifV0sInBhcmFtZXRlcnMiOlt7Im5hbWUiOiJsZWZ0IiwiZG9jdW1lbnRhdGlvbiI6W10sImRpc3BsYXlQYXJ0cyI6W3sidGV4dCI6ImxlZnQiLCJraW5kIjoicGFyYW1ldGVyTmFtZSJ9LHsidGV4dCI6IjoiLCJraW5kIjoicHVuY3R1YXRpb24ifSx7InRleHQiOiIgIiwia2luZCI6InNwYWNlIn0seyJ0ZXh0IjoibnVtYmVyIiwia2luZCI6ImtleXdvcmQifV0sImlzT3B0aW9uYWwiOmZhbHNlLCJpc1Jlc3QiOmZhbHNlfSx7Im5hbWUiOiJyaWdodCIsImRvY3VtZW50YXRpb24iOltdLCJkaXNwbGF5UGFydHMiOlt7InRleHQiOiJyaWdodCIsImtpbmQiOiJwYXJhbWV0ZXJOYW1lIn0seyJ0ZXh0IjoiOiIsImtpbmQiOiJwdW5jdHVhdGlvbiJ9LHsidGV4dCI6IiAiLCJraW5kIjoic3BhY2UifSx7InRleHQiOiJudW1iZXIiLCJraW5kIjoia2V5d29yZCJ9XSwiaXNPcHRpb25hbCI6ZmFsc2UsImlzUmVzdCI6ZmFsc2V9XSwiZG9jdW1lbnRhdGlvbiI6W3sidGV4dCI6IkFkZCB0d28gbnVtYmVycy4iLCJraW5kIjoidGV4dCJ9XSwidGFncyI6W3sibmFtZSI6InBhcmFtIiwidGV4dCI6ImxlZnQifSx7Im5hbWUiOiJwYXJhbSIsInRleHQiOiJyaWdodCJ9XX1dLCJhcHBsaWNhYmxlU3BhbiI6eyJzdGFydCI6eyJsaW5lIjo4LCJvZmZzZXQiOjI2fSwiZW5kIjp7ImxpbmUiOjgsIm9mZnNldCI6MzB9fSwic2VsZWN0ZWRJdGVtSW5kZXgiOjAsImFyZ3VtZW50SW5kZXgiOjEsImFyZ3VtZW50Q291bnQiOjJ9fQo=",
                "9c51039c86967dc379ae27b1c946f4d0ca5648b6e1be1318acdb1e962a0a4d3c",
                Vec::new(),
            )])
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(6),
            TsRequest::NavTree(FileRequest {
                file: path("src/main.js"),
            }),
            generation.clone(),
            ApprovedOutput::frames(ordinal(6), vec![decoded_exact_frame(
                "Q29udGVudC1MZW5ndGg6IDIwNjENCg0KeyJzZXEiOjAsInR5cGUiOiJyZXNwb25zZSIsImNvbW1hbmQiOiJuYXZ0cmVlIiwicmVxdWVzdF9zZXEiOiI2Iiwic3VjY2VzcyI6dHJ1ZSwiYm9keSI6eyJ0ZXh0IjoiXCJtYWluXCIiLCJraW5kIjoibW9kdWxlIiwia2luZE1vZGlmaWVycyI6IiIsInNwYW5zIjpbeyJzdGFydCI6eyJsaW5lIjoxLCJvZmZzZXQiOjF9LCJlbmQiOnsibGluZSI6MTcsIm9mZnNldCI6NTl9fV0sImNoaWxkSXRlbXMiOlt7InRleHQiOiJhZGQiLCJraW5kIjoiYWxpYXMiLCJraW5kTW9kaWZpZXJzIjoiIiwic3BhbnMiOlt7InN0YXJ0Ijp7ImxpbmUiOjIsIm9mZnNldCI6MTB9LCJlbmQiOnsibGluZSI6Miwib2Zmc2V0IjoxM319XSwibmFtZVNwYW4iOnsic3RhcnQiOnsibGluZSI6Miwib2Zmc2V0IjoxMH0sImVuZCI6eyJsaW5lIjoyLCJvZmZzZXQiOjEzfX19LHsidGV4dCI6IkNhbGN1bGF0b3IiLCJraW5kIjoiY2xhc3MiLCJraW5kTW9kaWZpZXJzIjoiZXhwb3J0Iiwic3BhbnMiOlt7InN0YXJ0Ijp7ImxpbmUiOjExLCJvZmZzZXQiOjF9LCJlbmQiOnsibGluZSI6MTQsIm9mZnNldCI6Mn19XSwibmFtZVNwYW4iOnsic3RhcnQiOnsibGluZSI6MTEsIm9mZnNldCI6MTR9LCJlbmQiOnsibGluZSI6MTEsIm9mZnNldCI6MjR9fSwiY2hpbGRJdGVtcyI6W3sidGV4dCI6InN1bSIsImtpbmQiOiJtZXRob2QiLCJraW5kTW9kaWZpZXJzIjoiIiwic3BhbnMiOlt7InN0YXJ0Ijp7ImxpbmUiOjEzLCJvZmZzZXQiOjN9LCJlbmQiOnsibGluZSI6MTMsIm9mZnNldCI6NDN9fV0sIm5hbWVTcGFuIjp7InN0YXJ0Ijp7ImxpbmUiOjEzLCJvZmZzZXQiOjN9LCJlbmQiOnsibGluZSI6MTMsIm9mZnNldCI6Nn19fV19LHsidGV4dCI6ImRlc2NyaWJlIiwia2luZCI6ImZ1bmN0aW9uIiwia2luZE1vZGlmaWVycyI6ImV4cG9ydCIsInNwYW5zIjpbeyJzdGFydCI6eyJsaW5lIjoxNywib2Zmc2V0IjoxfSwiZW5kIjp7ImxpbmUiOjE3LCJvZmZzZXQiOjU3fX1dLCJuYW1lU3BhbiI6eyJzdGFydCI6eyJsaW5lIjoxNywib2Zmc2V0IjoxN30sImVuZCI6eyJsaW5lIjoxNywib2Zmc2V0IjoyNX19fSx7InRleHQiOiJsYWJlbCIsImtpbmQiOiJjb25zdCIsImtpbmRNb2RpZmllcnMiOiJleHBvcnQiLCJzcGFucyI6W3sic3RhcnQiOnsibGluZSI6OCwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjMxfX1dLCJuYW1lU3BhbiI6eyJzdGFydCI6eyJsaW5lIjo4LCJvZmZzZXQiOjE0fSwiZW5kIjp7ImxpbmUiOjgsIm9mZnNldCI6MTl9fX0seyJ0ZXh0IjoibXVsdGlwbHkiLCJraW5kIjoiYWxpYXMiLCJraW5kTW9kaWZpZXJzIjoiIiwic3BhbnMiOlt7InN0YXJ0Ijp7ImxpbmUiOjEsIm9mZnNldCI6MTB9LCJlbmQiOnsibGluZSI6MSwib2Zmc2V0IjoxOH19XSwibmFtZVNwYW4iOnsic3RhcnQiOnsibGluZSI6MSwib2Zmc2V0IjoxMH0sImVuZCI6eyJsaW5lIjoxLCJvZmZzZXQiOjE4fX19LHsidGV4dCI6InRhYmJlZCIsImtpbmQiOiJjb25zdCIsImtpbmRNb2RpZmllcnMiOiJleHBvcnQiLCJzcGFucyI6W3sic3RhcnQiOnsibGluZSI6NSwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo1LCJvZmZzZXQiOjI0fX1dLCJuYW1lU3BhbiI6eyJzdGFydCI6eyJsaW5lIjo1LCJvZmZzZXQiOjE0fSwiZW5kIjp7ImxpbmUiOjUsIm9mZnNldCI6MjB9fX0seyJ0ZXh0IjoidG90YWwiLCJraW5kIjoiY29uc3QiLCJraW5kTW9kaWZpZXJzIjoiZXhwb3J0Iiwic3BhbnMiOlt7InN0YXJ0Ijp7ImxpbmUiOjksIm9mZnNldCI6MTR9LCJlbmQiOnsibGluZSI6OSwib2Zmc2V0IjoyOH19XSwibmFtZVNwYW4iOnsic3RhcnQiOnsibGluZSI6OSwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo5LCJvZmZzZXQiOjE5fX19LHsidGV4dCI6IueVjCIsImtpbmQiOiJjb25zdCIsImtpbmRNb2RpZmllcnMiOiJleHBvcnQiLCJzcGFucyI6W3sic3RhcnQiOnsibGluZSI6NCwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo0LCJvZmZzZXQiOjI3fX1dLCJuYW1lU3BhbiI6eyJzdGFydCI6eyJsaW5lIjo0LCJvZmZzZXQiOjE0fSwiZW5kIjp7ImxpbmUiOjQsIm9mZnNldCI6MTV9fX1dfX0K",
                "7bfdc1a3f765bb9787b272fb6e9cb38a07f67b03f4af3d4a7ed0bbce418be29d",
                Vec::new(),
            )])
            .unwrap(),
        )
        .unwrap()
        .into(),
    ]);
    for request in 7..=13 {
        exchanges.push(navto_exchange(
            request,
            "Missing",
            match request {
                7 => "bcefbd91da5ee643955dbd8992d006e5edae0b1d2865caa2a3c292e490d870ef",
                8 => "1a63326b0af5d4986f6dceb081552c10e06b7ece8e95561e75b33a484341c59c",
                9 => "8c77ad3a58a0c1b36ed474113502e2c5c78829150e5b53fd34d7ba06fecf5e0f",
                10 => "67633372990c61e5a3b032f0d3fa17efee51d8c0f959ca2d326fc54c1797a335",
                11 => "9bdfb7f52c67f8a01d58db2e6e207484098ca8c22baa37fb2083194903703ab0",
                12 => "53ece11c21172f81c0efc5c5f1b782aae605b4dccb318367d8e651fe7db5bd72",
                13 => "6315b47d044b299938fb943c205d81d03b04d72688e01141324b65d4ca3096f1",
                _ => unreachable!(),
            },
            &generation,
            false,
        ));
    }
    for request in 14..=16 {
        exchanges.push(navto_exchange(
            request,
            "Calculator",
            match request {
                14 => "4748a7297c9b9239c2e7e9a508034885fc325e3cbe75c64c02b321ada93d1a34",
                15 => "fcece839622df0e58f1a5814a168810711258cf28d371167761305213491e120",
                16 => "1f98d2b71fdcc52ff778877473f8f97e4c9914ca9710a9e87727550c61930271",
                _ => unreachable!(),
            },
            &generation,
            true,
        ));
    }
    let session = ReplaySession::new(
        exchanges,
        digest("7171167a48fd2932b77dbbac0cfd6cfcc55a544d499e33be3545a4494e7dbd43"),
        digest("3016b3d7fb7ce69b3c6fe10da5ea21574cd298c656835e41fbe734e516ec17d1"),
        ReplayTermination::CleanEof,
    )
    .unwrap();
    let replay = TideReplay::new(TideScenario::Navigation, fixtures, vec![session]).unwrap();
    materialized_case(
        "documentation_imenu_definition_back_and_named_navigation",
        replay,
        NAVIGATION_BODY,
        expect![[
            r#"OK (:result (:definition (:origin (:file "src/main.js" :line 8 :column 22 :point 171 :current t :window-point 171 :line-text "export const label = add(1, 2);") :destination (:file "src/math.js" :line 6 :column 16 :point 94 :window-point 94 :line-text "export function add(left, right) {" :selected t :selected-buffer-name "math.js" :ambient-current nil) :history (:backward ((:live t :file "src/main.js" :point 171)) :forward nil) :back (:file "src/main.js" :line 8 :column 22 :point 171 :window-point 171 :line-text "export const label = add(1, 2);" :selected t :selected-buffer-name "main.js" :ambient-current t) :back-history (:backward nil :forward ((:live t :file "src/math.js" :point 94)))) :documentation (:mode fundamental-mode :read-only t :point 1 :text "(alias) add(left: number, right: number): number\nimport add\n\nAdd two numbers.\n\n@param left\n@param right\n" :face-runs ((9 12 font-lock-type-face) (19 25 font-lock-keyword-face) (34 40 font-lock-keyword-face) (43 49 font-lock-keyword-face) (50 56 font-lock-keyword-face) (57 60 font-lock-type-face) (80 86 font-lock-keyword-face) (92 98 font-lock-keyword-face))) :signature #("add(left: number, right: number): number" 0 3 (face font-lock-type-face) 10 16 (face font-lock-keyword-face) 18 23 (face eldoc-highlight-function-argument) 25 31 (face font-lock-keyword-face) 34 40 (face font-lock-keyword-face)) :imenu (:minibuffer (:prompt nil :initial nil :final nil :result nil :condition nil :unread-empty t :minibuffer-history nil) :index (((:text "add alias" :properties ((0 4 nil) (4 9 (face tide-imenu-type-face)))) . 48) ((:text "Calculator" :properties ((0 10 nil))) ((:text "Calculator class" :properties ((0 11 nil) (11 16 (face tide-imenu-type-face)))) . 210) ((:text "sum method" :properties ((0 4 nil) (4 10 (face tide-imenu-type-face)))) . 290)) ((:text "describe function" :properties ((0 9 nil) (9 17 (face tide-imenu-type-face)))) . 363) ((:text "label const" :properties ((0 6 nil) (6 11 (face tide-imenu-type-face)))) . 162) ((:text "multiply alias" :properties ((0 9 nil) (9 14 (face tide-imenu-type-face)))) . 10) ((:text "tabbed const" :properties ((0 7 nil) (7 12 (face tide-imenu-type-face)))) . 114) ((:text "total const" :properties ((0 6 nil) (6 11 (face tide-imenu-type-face)))) . 194) ((:text "界 const" :properties ((0 2 nil) (2 7 (face tide-imenu-type-face)))) . 86)) :destination (:file "src/main.js" :line 1 :column 0 :point 1 :window-point 1 :line-text "import { multiply } from \"./math.js\";" :selected t :selected-buffer-name "main.js" :ambient-current t)) :missing (:minibuffer (:prompt "Search: " :initial "" :final "Missing" :result nil :condition (minibuffer-quit nil "Quit") :unread-empty t :minibuffer-history nil) :before (:file "src/main.js" :line 1 :column 0 :point 1 :window-point 1 :line-text "import { multiply } from \"./math.js\";" :selected t :selected-buffer-name "main.js" :ambient-current t) :after (:file "src/main.js" :line 1 :column 0 :point 1 :window-point 1 :line-text "import { multiply } from \"./math.js\";" :selected t :selected-buffer-name "main.js" :ambient-current t) :candidate-batches nil) :navigation (:minibuffer (:prompt "Search: " :initial "" :final "Calculator" :result nil :condition nil :unread-empty t :minibuffer-history ("Calculator")) :before (:file "src/main.js" :line 1 :column 0 :point 1 :window-point 1 :line-text "import { multiply } from \"./math.js\";" :selected t :selected-buffer-name "main.js" :ambient-current t) :candidate-batches ((:input ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2))) :output ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2)))) (:input ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2))) :output ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2)))) (:input ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2))) :output ((:name "Calculator" :kind "class" :modifiers "export" :match "exact" :file "src/main.js" :start (11 1) :end (14 2))))) :destination (:file "src/main.js" :line 11 :column 0 :point 210 :window-point 210 :line-text "export class Calculator {" :selected t :selected-buffer-name "main.js" :ambient-current t) :xref-history (:backward ((:live t :file "src/main.js" :point 1)) :forward nil) :xref-cache nil)) :typed (:scenario navigation :fixture-count 3 :session-count 1 :sessions ((:first-ordinal 1 :requests (open configure definition quickinfo-full signatureHelp navtree navto navto navto navto navto navto navto navto navto navto) :request-count 16 :frame-count 19 :request-sha256 "7171167a48fd2932b77dbbac0cfd6cfcc55a544d499e33be3545a4494e7dbd43" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 2 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "definition") :bytes 278 :sha256 "cd41f45fa3d2cdccf3926c4eeb14b4611dff8d125b1852d099e68ff3d6faa725" :delivery whole-frame))) :json "{\"command\":\"definition\",\"seq\":\"3\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":23}}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 4 :frames ((:kind response :owner (:response 4 "quickinfo-full") :bytes 1137 :sha256 "39df65cab6b06cf08a1397462e8f3779692b4eac6aefcf30a720ae969e541e55" :delivery whole-frame))) :json "{\"command\":\"quickinfo-full\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":23}}") (:ordinal 5 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "signatureHelp") :bytes 1214 :sha256 "9c51039c86967dc379ae27b1c946f4d0ca5648b6e1be1318acdb1e962a0a4d3c" :delivery whole-frame))) :json "{\"command\":\"signatureHelp\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":28}}") (:ordinal 6 :outcome complete :callback registered :output (:delivery-after 6 :frames ((:kind response :owner (:response 6 "navtree") :bytes 2085 :sha256 "7bfdc1a3f765bb9787b272fb6e9cb38a07f67b03f4af3d4a7ed0bbce418be29d" :delivery whole-frame))) :json "{\"command\":\"navtree\",\"seq\":\"6\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 7 :outcome complete :callback registered :output (:delivery-after 7 :frames ((:kind response :owner (:response 7 "navto") :bytes 111 :sha256 "bcefbd91da5ee643955dbd8992d006e5edae0b1d2865caa2a3c292e490d870ef" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"7\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 8 :outcome complete :callback registered :output (:delivery-after 8 :frames ((:kind response :owner (:response 8 "navto") :bytes 111 :sha256 "1a63326b0af5d4986f6dceb081552c10e06b7ece8e95561e75b33a484341c59c" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"8\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 9 :outcome complete :callback registered :output (:delivery-after 9 :frames ((:kind response :owner (:response 9 "navto") :bytes 111 :sha256 "8c77ad3a58a0c1b36ed474113502e2c5c78829150e5b53fd34d7ba06fecf5e0f" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"9\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 10 :outcome complete :callback registered :output (:delivery-after 10 :frames ((:kind response :owner (:response 10 "navto") :bytes 112 :sha256 "67633372990c61e5a3b032f0d3fa17efee51d8c0f959ca2d326fc54c1797a335" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"10\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 11 :outcome complete :callback registered :output (:delivery-after 11 :frames ((:kind response :owner (:response 11 "navto") :bytes 112 :sha256 "9bdfb7f52c67f8a01d58db2e6e207484098ca8c22baa37fb2083194903703ab0" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"11\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 12 :outcome complete :callback registered :output (:delivery-after 12 :frames ((:kind response :owner (:response 12 "navto") :bytes 112 :sha256 "53ece11c21172f81c0efc5c5f1b782aae605b4dccb318367d8e651fe7db5bd72" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"12\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 13 :outcome complete :callback registered :output (:delivery-after 13 :frames ((:kind response :owner (:response 13 "navto") :bytes 112 :sha256 "6315b47d044b299938fb943c205d81d03b04d72688e01141324b65d4ca3096f1" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"13\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Missing\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 14 :outcome complete :callback registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 14 "navto") :bytes 305 :sha256 "4748a7297c9b9239c2e7e9a508034885fc325e3cbe75c64c02b321ada93d1a34" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"14\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Calculator\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 15 :outcome complete :callback registered :output (:delivery-after 15 :frames ((:kind response :owner (:response 15 "navto") :bytes 305 :sha256 "fcece839622df0e58f1a5814a168810711258cf28d371167761305213491e120" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"15\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Calculator\",\"maxResultCount\":100,\"currentFileOnly\":false}}") (:ordinal 16 :outcome complete :callback registered :output (:delivery-after 16 :frames ((:kind response :owner (:response 16 "navto") :bytes 305 :sha256 "1f98d2b71fdcc52ff778877473f8f97e4c9914ca9710a9e87727550c61930271" :delivery whole-frame))) :json "{\"command\":\"navto\",\"seq\":\"16\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"searchValue\":\"Calculator\",\"maxResultCount\":100,\"currentFileOnly\":false}}")) :termination clean-eof))) :launches ((:name "tsserver" :buffer "*tide-server*" :program [ADAPTER] :arguments ([TSSERVER] "--disableAutomaticTypingAcquisition") :cwd [ROOT] :environment-count 23)) :terminals ((:session 1 :status exit :exit 0 :message "finished\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "definition" :callback registered) (:ordinal 4 :command "quickinfo-full" :callback registered) (:ordinal 5 :command "signatureHelp" :callback registered) (:ordinal 6 :command "navtree" :callback registered) (:ordinal 7 :command "navto" :callback registered) (:ordinal 8 :command "navto" :callback registered) (:ordinal 9 :command "navto" :callback registered) (:ordinal 10 :command "navto" :callback registered) (:ordinal 11 :command "navto" :callback registered) (:ordinal 12 :command "navto" :callback registered) (:ordinal 13 :command "navto" :callback registered) (:ordinal 14 :command "navto" :callback registered) (:ordinal 15 :command "navto" :callback registered) (:ordinal 16 :command "navto" :callback registered)) :public-deletes nil :cleanup clean)"#
        ]],
    )
}

fn navto_exchange(
    request: usize,
    query: &str,
    frame_digest: &str,
    generation: &FixtureGeneration,
    found: bool,
) -> ReplayExchange {
    let body = if found {
        format!(
            "{{\"seq\":0,\"type\":\"response\",\"command\":\"navto\",\"request_seq\":\"{request}\",\"success\":true,\"body\":[{{\"name\":\"Calculator\",\"kind\":\"class\",\"kindModifiers\":\"export\",\"isCaseSensitive\":true,\"matchKind\":\"exact\",\"file\":\"[ROOT]/src/main.js\",\"start\":{{\"line\":11,\"offset\":1}},\"end\":{{\"line\":14,\"offset\":2}}}}]}}"
        )
    } else {
        format!(
            "{{\"seq\":0,\"type\":\"response\",\"command\":\"navto\",\"request_seq\":\"{request}\",\"success\":true,\"body\":[]}}"
        )
    };
    let mut body = body.into_bytes();
    body.push(b'\n');
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend_from_slice(&body);
    RecordedExchange::new(
        ordinal(request),
        TsRequest::NavTo(NavToRequest::new(query, path("src/main.js"), false).unwrap()),
        generation.clone(),
        ApprovedOutput::frames(
            ordinal(request),
            vec![
                ApprovedFrame::new(
                    frame,
                    digest(frame_digest),
                    DeliveryPlan::WholeFrame,
                    if found {
                        vec![ResponseToken::root_path(
                            vec![
                                JsonPathSegment::Key("body"),
                                JsonPathSegment::Index(0),
                                JsonPathSegment::Key("file"),
                            ],
                            path("src/main.js"),
                        )]
                    } else {
                        Vec::new()
                    },
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    )
    .unwrap()
    .into()
}

const NAVIGATION_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((buffer-locus
        (root)
        (list :file (and buffer-file-name
                         (file-relative-name buffer-file-name root))
              :line (line-number-at-pos) :column (current-column)
              :point (point)
              :current (eq (current-buffer)
                           (window-buffer (selected-window)))
              :window-point (window-point (selected-window))
              :line-text
              (substring-no-properties
               (buffer-substring (line-beginning-position)
                                 (line-end-position)))))
       (selected-locus
        (root)
        (let* ((window (selected-window))
               (selected-buffer (window-buffer window))
               (ambient-buffer (current-buffer)))
          (with-current-buffer selected-buffer
            (list :file (and buffer-file-name
                             (file-relative-name buffer-file-name root))
                  :line (line-number-at-pos) :column (current-column)
                  :point (point) :window-point (window-point window)
                  :line-text
                  (substring-no-properties
                   (buffer-substring (line-beginning-position)
                                     (line-end-position)))
                  :selected t
                  :selected-buffer-name (buffer-name selected-buffer)
                  :ambient-current (eq ambient-buffer selected-buffer)))))
       (marker-state
        (marker root)
        (list :live (and (marker-buffer marker) t)
              :file (and (marker-buffer marker)
                         (with-current-buffer (marker-buffer marker)
                           (and buffer-file-name
                                (file-relative-name buffer-file-name root))))
              :point (marker-position marker)))
       (history-state
        (root)
        (let ((history (xref--get-history)))
          (list :backward (mapcar (lambda (marker)
                                    (marker-state marker root))
                                  (car history))
                :forward (mapcar (lambda (marker)
                                   (marker-state marker root))
                                 (cdr history)))))
       (property-runs
        (property)
        (let ((position (point-min)) runs)
          (while (< position (point-max))
            (let* ((value (get-text-property position property))
                   (next (or (next-single-property-change
                              position property nil (point-max))
                             (point-max))))
              (when value
                (push (list position next (copy-tree value)) runs))
              (setq position next)))
          (nreverse runs)))
       (normalize-imenu
        (value)
        (cond
         ((markerp value) (marker-position value))
         ((stringp value)
          (list :text (substring-no-properties value)
                :properties
                (let ((position 0) runs)
                  (while (< position (length value))
                    (let ((next (or (next-property-change
                                     position value (length value))
                                    (length value))))
                      (push (list position next
                                  (copy-tree
                                   (text-properties-at position value)))
                            runs)
                      (setq position next)))
                  (nreverse runs))))
         ((consp value)
          (cons (normalize-imenu (car value))
                (normalize-imenu (cdr value))))
         (t value)))
       (minibuffer-command
        (command keys cancel)
        (let ((setup (make-symbol "tide368-minibuffer-setup"))
              (exit (make-symbol "tide368-minibuffer-exit"))
              (events (append (string-to-list keys)
                              (listify-key-sequence
                               (kbd (if cancel "TAB RET C-g" "TAB RET")))))
              (unread-command-events nil)
              initial final prompt result condition)
          (fset setup
                (lambda ()
                  (setq prompt (minibuffer-prompt)
                        initial (minibuffer-contents-no-properties)
                        unread-command-events
                        (append events unread-command-events))))
          (fset exit
                (lambda ()
                  (setq final (minibuffer-contents-no-properties))))
          (unwind-protect
              (condition-case caught
                  (let ((executing-kbd-macro t))
                    (add-hook 'minibuffer-setup-hook setup)
                    (add-hook 'minibuffer-exit-hook exit)
                    (setq result (call-interactively command)))
                ((quit error)
                 (setq condition
                       (list (car caught) (copy-tree (cdr caught))
                             (error-message-string caught)))))
            (remove-hook 'minibuffer-setup-hook setup)
            (remove-hook 'minibuffer-exit-hook exit)
            (fmakunbound setup) (fmakunbound exit))
          (unless (null unread-command-events)
            (error "Tide minibuffer workflow left unread input: %S"
                   unread-command-events))
          (list :prompt prompt :initial initial :final final
                :result result :condition condition
                :unread-empty t
                :minibuffer-history (copy-tree minibuffer-history))))
       (nav-item-state
        (item root)
        (list :name (plist-get item :name) :kind (plist-get item :kind)
              :modifiers (plist-get item :kindModifiers)
              :match (plist-get item :matchKind)
              :file (file-relative-name (plist-get item :file) root)
              :start (list (tide-plist-get item :start :line)
                           (tide-plist-get item :start :offset))
              :end (list (tide-plist-get item :end :line)
                         (tide-plist-get item :end :offset)))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (buffer (find-file-noselect main))
           candidate-batches definition-origin definition-destination
           definition-history back-state back-history documentation
           signature imenu-state missing-state navigation-state)
      (switch-to-buffer buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (tide-setup)
      (tide368-test-assert-current-server)
      (goto-char (point-min))
      (search-forward "add(1")
      (search-backward "add")
      (forward-char 1)
      (setq definition-origin (buffer-locus root))
      (execute-kbd-macro (kbd "M-."))
      (let ((deadline (+ (float-time) 5.0)))
        (while (and (with-current-buffer
                        (window-buffer (selected-window))
                      (equal buffer-file-name main))
                    (< (float-time) deadline))
          (accept-process-output (tide-current-server) 0.01)))
      (when (with-current-buffer (window-buffer (selected-window))
              (equal buffer-file-name main))
        (error "Tide definition did not select its peer source"))
      (setq definition-destination (selected-locus root)
            definition-history (history-state root))
      (set-buffer (window-buffer (selected-window)))
      (execute-kbd-macro (kbd "M-,"))
      (setq back-state (selected-locus root)
            back-history (history-state root))
      (unless (and (eq (window-buffer (selected-window)) buffer)
                   (= (window-point (selected-window))
                      (plist-get definition-origin :point)))
        (error "Tide public back did not restore the definition origin"))
      (set-buffer buffer)
      (tide-documentation-at-point)
      (let ((deadline (+ (float-time) 5.0)))
        (while (and (not (get-buffer "*tide-documentation*"))
                    (< (float-time) deadline))
          (accept-process-output (tide-current-server) 0.01)))
      (let ((help (get-buffer "*tide-documentation*")))
        (unless help (error "Tide documentation buffer was not created"))
        (with-current-buffer help
          (setq documentation
                (list :mode major-mode :read-only buffer-read-only
                      :point (point)
                      :text (buffer-substring-no-properties
                             (point-min) (point-max))
                      :face-runs (property-runs 'face)))))
      (goto-char (point-min))
      (search-forward "add(1,")
      (let ((deadline (+ (float-time) 5.0)))
        (tide-eldoc-function (lambda (text) (setq signature text)))
        (while (and (null signature) (< (float-time) deadline))
          (accept-process-output (tide-current-server) 0.01)))
      (unless signature (error "Tide signature help did not finish"))
      (goto-char (point-min))
      (let ((minibuffer (minibuffer-command #'imenu "Calculator" nil)))
        (setq imenu-state
              (list :minibuffer minibuffer
                    :index (normalize-imenu imenu--index-alist)
                    :destination (selected-locus root))))
      ;; The successful named-navigation origin is deliberately distinct from
      ;; the Calculator target selected by Imenu, so a skipped jump cannot
      ;; satisfy the semantic snapshot.
      (set-buffer buffer)
      (goto-char (point-min))
      (let ((original-filter tide-navto-item-filter)
            (tide-navto-item-filter
             (lambda (items)
               (let ((filtered (funcall original-filter items)))
                 (push (list :input
                             (mapcar (lambda (item)
                                       (nav-item-state item root))
                                     items)
                             :output
                             (mapcar (lambda (item)
                                       (nav-item-state item root))
                                     filtered))
                       candidate-batches)
                 filtered))))
        (let ((before (selected-locus root))
              (minibuffer
               (minibuffer-command #'tide-nav "Missing" t)))
          (setq missing-state
                (list :minibuffer minibuffer
                      :before before :after (selected-locus root)
                      :candidate-batches
                      (nreverse (copy-tree candidate-batches)))))
        (setq candidate-batches nil)
        (let ((before (selected-locus root))
              (minibuffer
               (minibuffer-command #'tide-nav "Calculator" nil)))
          (setq navigation-state
                (list :minibuffer minibuffer :before before
                      :candidate-batches
                      (nreverse (copy-tree candidate-batches))
                      :destination (selected-locus root)
                      :xref-history (history-state root)
                      :xref-cache
                      (mapcar (lambda (item) (nav-item-state item root))
                              tide-xref--last-completion-table)))))
      (list :definition (list :origin definition-origin
                              :destination definition-destination
                              :history definition-history
                              :back back-state :back-history back-history)
            :documentation documentation :signature signature
            :imenu imenu-state :missing missing-state
            :navigation navigation-state))))"#;

const RENAME_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((condition-state
       (condition)
        (and condition
             (list :type (car condition)
                   :data (normalize-value (copy-tree (cdr condition)))
                   :message (tide368-test-normalize-string
                             (error-message-string condition)))))
       (normalize-value
        (value)
        (cond
         ((stringp value) (tide368-test-normalize-string
                           (copy-sequence value)))
         ((consp value) (cons (normalize-value (car value))
                              (normalize-value (cdr value))))
         (t value)))
       (relative-file
        (file root)
        (and file (file-relative-name file root)))
       (disk-state
        (file root)
        (list :file (relative-file file root)
              :exists (file-exists-p file)
              :symlink (file-symlink-p file)
              :sha256 (and (file-exists-p file)
                           (tide368-test-file-sha256 file))))
       (buffer-state
        (buffer file root)
        (unless (buffer-live-p buffer)
          (error "Tide expected a live owned buffer for %s" file))
        (with-current-buffer buffer
          (list :identity (eq buffer (get-file-buffer file))
                :name (copy-sequence (buffer-name buffer))
                :file (relative-file buffer-file-name root)
                :mode major-mode :tide-mode (bound-and-true-p tide-mode)
                :point (point) :mark (mark t) :mark-active mark-active
                :modified (buffer-modified-p)
                :coding buffer-file-coding-system
                :undo (cond
                       ((eq buffer-undo-list t) 'disabled)
                       ((null buffer-undo-list) 'empty)
                       (t (list :present t
                                :entries (length buffer-undo-list)
                                :boundaries (cl-count nil buffer-undo-list))))
                :text (buffer-substring-no-properties (point-min) (point-max))
                :disk (disk-state file root))))
       (maybe-buffer-state
        (buffer file root)
        (and (buffer-live-p buffer) (buffer-state buffer file root)))
       (selected-state
        (root)
        (let* ((window (selected-window))
               (buffer (window-buffer window)))
          (list :window-live (window-live-p window)
                :buffer-live (buffer-live-p buffer)
                :buffer-name (and (buffer-live-p buffer)
                                  (copy-sequence (buffer-name buffer)))
                :file (and (buffer-live-p buffer)
                           (with-current-buffer buffer
                             (relative-file buffer-file-name root)))
                :point (window-point window))))
       (atomic-state
        (root main main-buffer math math-buffer target target-buffer)
        (let* ((config (expand-file-name "jsconfig.json" root))
               (config-buffer (get-file-buffer config)))
          (list :selected (selected-state root)
                :main (buffer-state main-buffer main root)
                :source (buffer-state math-buffer math root)
                :config-disk (disk-state config root)
                :config-buffer (maybe-buffer-state config-buffer config root)
                :target-disk (disk-state target root)
                :target-buffer (maybe-buffer-state target-buffer target root))))
       (record-save
        (root)
        (let ((relative (and buffer-file-name
                             (relative-file buffer-file-name root))))
          (when (member relative
                        '("src/main.js" "src/math.js"
                          "src/arithmetic 界.js" "jsconfig.json"))
            (push (list :file relative
                        :modified (buffer-modified-p)
                        :disk-sha256
                        (tide368-test-file-sha256 buffer-file-name))
                  save-ledger))))
       (record-post-edit
        (root)
        (push (list :file (relative-file buffer-file-name root)
                    :modified (buffer-modified-p)
                    :text (buffer-substring-no-properties
                           (point-min) (point-max)))
              post-edit-ledger))
       (input-command
        (command input)
        (let ((setup (make-symbol "tide368-rename-input"))
              (exit (make-symbol "tide368-rename-exit"))
              (events (append (string-to-list input)
                              (listify-key-sequence (kbd "RET"))))
              (unread-command-events nil)
              prompt initial final result condition)
          (fset setup
                (lambda ()
                  (setq prompt (minibuffer-prompt)
                        initial (minibuffer-contents-no-properties))
                  (delete-minibuffer-contents)
                  (setq unread-command-events
                        (append events unread-command-events))))
          (fset exit
                (lambda ()
                  (setq final (minibuffer-contents-no-properties))))
          (unwind-protect
              (condition-case caught
                  (let ((executing-kbd-macro t))
                    (add-hook 'minibuffer-setup-hook setup)
                    (add-hook 'minibuffer-exit-hook exit)
                    (setq result (call-interactively command)))
                ((quit error) (setq condition caught)))
            (remove-hook 'minibuffer-setup-hook setup)
            (remove-hook 'minibuffer-exit-hook exit)
            (fmakunbound setup) (fmakunbound exit))
          (unless (null unread-command-events)
            (error "Tide rename minibuffer left unread input: %S"
                   unread-command-events))
          (list :prompt (normalize-value prompt)
                :initial (normalize-value initial)
                :final (normalize-value final)
                :result result :condition (condition-state condition)
                :minibuffer-history (normalize-value minibuffer-history)
                :file-name-history (normalize-value file-name-history)))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (math (expand-file-name "src/math.js" root))
           (renamed (expand-file-name "src/arithmetic 界.js" root))
           (live-target (expand-file-name "src/live target.js" root))
           (existing-target (expand-file-name "src/existing target.js" root))
           (main-buffer (find-file-noselect main))
           (math-buffer (find-file-noselect math))
           (save-observer (make-symbol "tide368-save-observer"))
           (post-edit-observer (make-symbol "tide368-post-edit-observer"))
           save-ledger post-edit-ledger blank-state symbol-state
           live-state existing-state file-state live-buffer)
      (fset save-observer (lambda () (record-save root)))
      (fset post-edit-observer (lambda () (record-post-edit root)))
      (unwind-protect
          (progn
            (switch-to-buffer main-buffer)
            (js-mode)
            (setq-local tab-width 2 js-indent-level 2)
            (tide-setup)
            (tide368-test-assert-current-server)
            (add-hook 'after-save-hook save-observer)
            (goto-char (point-min))
            (search-forward "add(1")
            (search-backward "add")
            (let* ((before (atomic-state root main main-buffer math math-buffer
                                         live-target nil))
                   (input (input-command #'tide-rename-symbol " \t"))
                   (after (atomic-state root main main-buffer math math-buffer
                                        live-target nil)))
              (setq blank-state
                    (list :input input :before before :after after
                          :saves (nreverse (copy-tree save-ledger))))
              (unless (equal before after)
                (error "Tide blank symbol rename mutated state before rejecting"))
              (unless (plist-get (plist-get blank-state :input) :condition)
                (error "Tide blank symbol rename did not signal")))
            (setq save-ledger nil)
            (setq symbol-state
                  (list :input (input-command #'tide-rename-symbol "sum界")
                        :message (tide368-test-normalize-string
                                  (or (current-message) ""))
                        :saves (nreverse (copy-tree save-ledger))
                        :main (buffer-state main-buffer main root)
                        :math (buffer-state math-buffer math root)))
            (unless (and
                     (equal (tide368-test-file-sha256 main)
                            "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")
                     (equal (tide368-test-file-sha256 math)
                            "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078"))
              (error "Tide symbol rename produced unexpected fixture bytes"))
            (switch-to-buffer math-buffer)
            (setq live-buffer (find-file-noselect live-target))
            (let* ((before (atomic-state root main main-buffer math math-buffer
                                         live-target live-buffer))
                   (input (input-command #'tide-rename-file live-target))
                   (after (atomic-state root main main-buffer math math-buffer
                                        live-target live-buffer)))
              (setq live-state
                    (list :input input :before before :after after
                          :same-buffer (eq live-buffer
                                           (get-file-buffer live-target)))))
            (unless (plist-get (plist-get live-state :input) :condition)
              (error "Tide live-target file rename did not signal"))
            (with-current-buffer live-buffer (set-buffer-modified-p nil))
            (kill-buffer live-buffer)
            (setq live-buffer nil)
            (with-temp-buffer
              (insert "occupied\n")
              (let ((coding-system-for-write 'utf-8-unix))
                (write-region (point-min) (point-max) existing-target nil 'silent)))
            (let* ((before (atomic-state root main main-buffer math math-buffer
                                         existing-target
                                         (get-file-buffer existing-target)))
                   (input (input-command #'tide-rename-file existing-target))
                   (after (atomic-state root main main-buffer math math-buffer
                                        existing-target
                                        (get-file-buffer existing-target))))
              (setq existing-state
                    (list :input input :before before :after after))
              (unless (equal before after)
                (error "Tide existing-target rename mutated state before rejecting")))
            (unless (plist-get (plist-get existing-state :input) :condition)
              (error "Tide existing-target file rename did not signal"))
            (delete-file existing-target)
            (setq save-ledger nil post-edit-ledger nil)
            (let ((tide-post-code-edit-hook (list post-edit-observer)))
              (setq file-state
                    (list :input (input-command #'tide-rename-file renamed)
                          :message (tide368-test-normalize-string
                                    (or (current-message) ""))
                          :saves (nreverse (copy-tree save-ledger))
                          :post-edits (nreverse (copy-tree post-edit-ledger))
                          :old (disk-state math root)
                          :new (disk-state renamed root)
                          :new-directory
                          (list :exists (file-directory-p
                                         (file-name-directory renamed))
                                :symlink
                                (file-symlink-p
                                 (directory-file-name
                                  (file-name-directory renamed))))
                          :same-buffer (eq math-buffer
                                           (get-file-buffer renamed))
                          :old-buffer-absent (null (get-file-buffer math))
                          :renamed-buffer
                          (buffer-state math-buffer renamed root)
                          :main (buffer-state main-buffer main root)
                          :config
                          (let* ((config (expand-file-name "jsconfig.json" root))
                                 (config-buffer (get-file-buffer config)))
                            (list
                             :buffer-live (buffer-live-p config-buffer)
                             :buffer (and (buffer-live-p config-buffer)
                                          (buffer-state config-buffer config root))
                             :disk (disk-state config root))))))
            (unless (and
                     (not (file-exists-p math))
                     (equal (tide368-test-file-sha256 renamed)
                            "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")
                     (equal (tide368-test-file-sha256 main)
                            "7aa4a05c1e09bab0e7c91d85c52818c5bf862138caa505bdc7f6de35f45c423e")
                     (equal (tide368-test-file-sha256
                             (expand-file-name "jsconfig.json" root))
                            "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353"))
              (error
               "Tide file rename produced unexpected fixture bytes: %S"
               (list :old-exists (file-exists-p math)
                     :input (plist-get file-state :input)
                     :renamed (and (file-exists-p renamed)
                                   (tide368-test-file-sha256 renamed))
                     :main (tide368-test-file-sha256 main)
                     :config
                     (tide368-test-file-sha256
                      (expand-file-name "jsconfig.json" root)))))
            (list :blank blank-state :symbol symbol-state
                  :live-target live-state :existing-target existing-state
                  :file file-state))
        (when (buffer-live-p live-buffer)
          (with-current-buffer live-buffer (set-buffer-modified-p nil))
          (kill-buffer live-buffer))
        (when (file-exists-p existing-target) (delete-file existing-target))
        (remove-hook 'after-save-hook save-observer)
        (fmakunbound save-observer)
        (fmakunbound post-edit-observer)))))"#;

fn references_ui_and_async_identifier_highlight() -> ParityBatchCase {
    let fixtures = common_manifest();
    let generation = fixtures.generation();
    let mut exchanges = startup_exchanges(&generation);
    exchanges.extend([
        RecordedExchange::new(
            ordinal(3),
            TsRequest::References(PointRequest {
                file: path("src/main.js"),
                point: point(8, 23),
            }),
            generation.clone(),
            ApprovedOutput::frames(ordinal(3), vec![decoded_frame(
                "Q29udGVudC1MZW5ndGg6IDEzMjYNCg0KeyJzZXEiOjAsInR5cGUiOiJyZXNwb25zZSIsImNvbW1hbmQiOiJyZWZlcmVuY2VzIiwicmVxdWVzdF9zZXEiOiIzIiwic3VjY2VzcyI6dHJ1ZSwiYm9keSI6eyJyZWZzIjpbeyJmaWxlIjoiW1JPT1RdL3NyYy9tYWluLmpzIiwic3RhcnQiOnsibGluZSI6Miwib2Zmc2V0IjoxMH0sImVuZCI6eyJsaW5lIjoyLCJvZmZzZXQiOjEzfSwiY29udGV4dFN0YXJ0Ijp7ImxpbmUiOjIsIm9mZnNldCI6MX0sImNvbnRleHRFbmQiOnsibGluZSI6Miwib2Zmc2V0IjozM30sImxpbmVUZXh0IjoiaW1wb3J0IHsgYWRkIH0gZnJvbSBcIi4vbWF0aC5qc1wiOyIsImlzV3JpdGVBY2Nlc3MiOnRydWV9LHsiZmlsZSI6IltST09UXS9zcmMvbWFpbi5qcyIsInN0YXJ0Ijp7ImxpbmUiOjQsIm9mZnNldCI6MTh9LCJlbmQiOnsibGluZSI6NCwib2Zmc2V0IjoyMX0sImxpbmVUZXh0IjoiZXhwb3J0IGNvbnN0IOeVjCA9IGFkZCgzLCA0KTsiLCJpc1dyaXRlQWNjZXNzIjpmYWxzZX0seyJmaWxlIjoiW1JPT1RdL3NyYy9tYWluLmpzIiwic3RhcnQiOnsibGluZSI6OCwib2Zmc2V0IjoyMn0sImVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjI1fSwibGluZVRleHQiOiJleHBvcnQgY29uc3QgbGFiZWwgPSBhZGQoMSwgMik7IiwiaXNXcml0ZUFjY2VzcyI6ZmFsc2V9LHsiZmlsZSI6IltST09UXS9zcmMvbWFpbi5qcyIsInN0YXJ0Ijp7ImxpbmUiOjksIm9mZnNldCI6MjB9LCJlbmQiOnsibGluZSI6OSwib2Zmc2V0IjoyM30sImxpbmVUZXh0IjoiZXhwb3J0IGNvbnN0IHRvdGFsPWFkZCgxLDIpIiwiaXNXcml0ZUFjY2VzcyI6ZmFsc2V9LHsiZmlsZSI6IltST09UXS9zcmMvbWFpbi5qcyIsInN0YXJ0Ijp7ImxpbmUiOjEzLCJvZmZzZXQiOjI3fSwiZW5kIjp7ImxpbmUiOjEzLCJvZmZzZXQiOjMwfSwibGluZVRleHQiOiIgIHN1bShsZWZ0LCByaWdodCl7cmV0dXJuIGFkZChsZWZ0LHJpZ2h0KX0iLCJpc1dyaXRlQWNjZXNzIjpmYWxzZX0seyJmaWxlIjoiW1JPT1RdL3NyYy9tYXRoLmpzIiwic3RhcnQiOnsibGluZSI6Niwib2Zmc2V0IjoxN30sImVuZCI6eyJsaW5lIjo2LCJvZmZzZXQiOjIwfSwiY29udGV4dFN0YXJ0Ijp7ImxpbmUiOjYsIm9mZnNldCI6MX0sImNvbnRleHRFbmQiOnsibGluZSI6OCwib2Zmc2V0IjoyfSwibGluZVRleHQiOiJleHBvcnQgZnVuY3Rpb24gYWRkKGxlZnQsIHJpZ2h0KSB7IiwiaXNXcml0ZUFjY2VzcyI6dHJ1ZX1dLCJzeW1ib2xOYW1lIjoiYWRkIiwic3ltYm9sU3RhcnRPZmZzZXQiOjIyLCJzeW1ib2xEaXNwbGF5U3RyaW5nIjoiKGFsaWFzKSBhZGQobGVmdDogbnVtYmVyLCByaWdodDogbnVtYmVyKTogbnVtYmVyXG5pbXBvcnQgYWRkIn19",
                "9ff47c4e692616702b4a8d13a4811962f4eb21fbf656dfd78538a1b23e181e9f",
                (0..5)
                    .map(|index| {
                        ResponseToken::root_path(
                            vec![
                                JsonPathSegment::Key("body"),
                                JsonPathSegment::Key("refs"),
                                JsonPathSegment::Index(index),
                                JsonPathSegment::Key("file"),
                            ],
                            path("src/main.js"),
                        )
                    })
                    .chain(std::iter::once(ResponseToken::root_path(
                        vec![
                            JsonPathSegment::Key("body"),
                            JsonPathSegment::Key("refs"),
                            JsonPathSegment::Index(5),
                            JsonPathSegment::Key("file"),
                        ],
                        path("src/math.js"),
                    )))
                    .collect(),
            )])
            .unwrap(),
        )
        .unwrap()
        .into(),
        highlight_exchange(4, "c864beed01d97fe37b035820fab2e2873e380a8910135b4111c47b5b653d0df2", "Q29udGVudC1MZW5ndGg6IDM5MQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImRvY3VtZW50SGlnaGxpZ2h0cyIsInJlcXVlc3Rfc2VxIjoiNCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOlt7ImZpbGUiOiJbUk9PVF0vc3JjL21haW4uanMiLCJoaWdobGlnaHRTcGFucyI6W3sic3RhcnQiOnsibGluZSI6NCwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo0LCJvZmZzZXQiOjE1fSwiY29udGV4dFN0YXJ0Ijp7ImxpbmUiOjQsIm9mZnNldCI6MX0sImNvbnRleHRFbmQiOnsibGluZSI6NCwib2Zmc2V0IjoyOH0sImtpbmQiOiJ3cml0dGVuUmVmZXJlbmNlIn0seyJzdGFydCI6eyJsaW5lIjo1LCJvZmZzZXQiOjIzfSwiZW5kIjp7ImxpbmUiOjUsIm9mZnNldCI6MjR9LCJraW5kIjoicmVmZXJlbmNlIn1dfV19", &generation),
        highlight_exchange(5, "a3477926eccade914b0c9844134ea80b154be991013e82180a93c2eb161f3165", "Q29udGVudC1MZW5ndGg6IDM5MQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImRvY3VtZW50SGlnaGxpZ2h0cyIsInJlcXVlc3Rfc2VxIjoiNSIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOlt7ImZpbGUiOiJbUk9PVF0vc3JjL21haW4uanMiLCJoaWdobGlnaHRTcGFucyI6W3sic3RhcnQiOnsibGluZSI6NCwib2Zmc2V0IjoxNH0sImVuZCI6eyJsaW5lIjo0LCJvZmZzZXQiOjE1fSwiY29udGV4dFN0YXJ0Ijp7ImxpbmUiOjQsIm9mZnNldCI6MX0sImNvbnRleHRFbmQiOnsibGluZSI6NCwib2Zmc2V0IjoyOH0sImtpbmQiOiJ3cml0dGVuUmVmZXJlbmNlIn0seyJzdGFydCI6eyJsaW5lIjo1LCJvZmZzZXQiOjIzfSwiZW5kIjp7ImxpbmUiOjUsIm9mZnNldCI6MjR9LCJraW5kIjoicmVmZXJlbmNlIn1dfV19", &generation),
    ]);
    let session = ReplaySession::new(
        exchanges,
        digest("651182dfe4dbe9d5ff957b2f7c39e84250da15d399383bb5d1828fba78278526"),
        digest("8a5f67a0054c75c2ea4a75b5e37bf3c86f15988a34878a34b1eab05d29741089"),
        ReplayTermination::CleanEof,
    )
    .unwrap();
    let replay = TideReplay::new(TideScenario::References, fixtures, vec![session]).unwrap();
    materialized_case(
        "references_ui_and_async_identifier_highlight",
        replay,
        REFERENCES_BODY,
        expect![[
            r#"OK (:result (:references (:mode tide-references-mode :read-only t :text "\nimport { add } from \"./math.js\";\nexport const 界 = add(3, 4);\nexport const label = add(1, 2);\nexport const total=add(1,2)\n  sum(left, right){return add(left,right)}\n\nexport function add(left, right) {\n" :property-runs ((line-prefix (1 2 (:text "src/main.js" :properties ((0 11 (face tide-file))))) (2 35 (:text " 2: " :properties ((0 2 (face tide-line-number)) (2 4 nil)))) (35 63 (:text " 4: " :properties ((0 2 (face tide-line-number)) (2 4 nil)))) (63 95 (:text " 8: " :properties ((0 2 (face tide-line-number)) (2 4 nil)))) (95 123 (:text " 9: " :properties ((0 2 (face tide-line-number)) (2 4 nil)))) (123 166 (:text "13: " :properties ((0 2 (face tide-line-number)) (2 4 nil)))) (166 167 (:text "src/math.js" :properties ((0 11 (face tide-file))))) (167 202 (:text " 6: " :properties ((0 2 (face tide-line-number)) (2 4 nil))))) (wrap-prefix (2 166 (:text "  " :properties ((0 2 nil)))) (167 202 (:text "  " :properties ((0 2 nil))))) (tide-line-reference (2 35 (:file "src/main.js" :start (2 10) :end (2 13) :context-start (2 1) :context-end (2 33) :line-text "import { add } from \"./math.js\";" :write-access t)) (35 63 (:file "src/main.js" :start (4 18) :end (4 21) :context-start nil :context-end nil :line-text "export const 界 = add(3, 4);" :write-access :json-false)) (63 95 (:file "src/main.js" :start (8 22) :end (8 25) :context-start nil :context-end nil :line-text "export const label = add(1, 2);" :write-access :json-false)) (95 123 (:file "src/main.js" :start (9 20) :end (9 23) :context-start nil :context-end nil :line-text "export const total=add(1,2)" :write-access :json-false)) (123 166 (:file "src/main.js" :start (13 27) :end (13 30) :context-start nil :context-end nil :line-text "  sum(left, right){return add(left,right)}" :write-access :json-false)) (167 202 (:file "src/math.js" :start (6 17) :end (6 20) :context-start (6 1) :context-end (8 2) :line-text "export function add(left, right) {" :write-access t))) (tide-reference (11 14 (:file "src/main.js" :start (2 10) :end (2 13) :context-start (2 1) :context-end (2 33) :line-text "import { add } from \"./math.js\";" :write-access t)) (52 55 (:file "src/main.js" :start (4 18) :end (4 21) :context-start nil :context-end nil :line-text "export const 界 = add(3, 4);" :write-access :json-false)) (84 87 (:file "src/main.js" :start (8 22) :end (8 25) :context-start nil :context-end nil :line-text "export const label = add(1, 2);" :write-access :json-false)) (114 117 (:file "src/main.js" :start (9 20) :end (9 23) :context-start nil :context-end nil :line-text "export const total=add(1,2)" :write-access :json-false)) (149 152 (:file "src/main.js" :start (13 27) :end (13 30) :context-start nil :context-end nil :line-text "  sum(left, right){return add(left,right)}" :write-access :json-false)) (183 186 (:file "src/math.js" :start (6 17) :end (6 20) :context-start (6 1) :context-end (8 2) :line-text "export function add(left, right) {" :write-access t))) (face (11 14 tide-match) (52 55 tide-match) (84 87 tide-match) (114 117 tide-match) (149 152 tide-match) (183 186 tide-match)) (mouse-face (11 14 highlight) (52 55 highlight) (84 87 highlight) (114 117 highlight) (149 152 highlight) (183 186 highlight)) (help-echo (11 14 (:text "mouse-1: Visit the reference." :properties ((0 29 nil)))) (52 55 (:text "mouse-1: Visit the reference." :properties ((0 29 nil)))) (84 87 (:text "mouse-1: Visit the reference." :properties ((0 29 nil)))) (114 117 (:text "mouse-1: Visit the reference." :properties ((0 29 nil)))) (149 152 (:text "mouse-1: Visit the reference." :properties ((0 29 nil)))) (183 186 (:text "mouse-1: Visit the reference." :properties ((0 29 nil))))))) :navigation ((:point 11 :current t :selected t :window-point 11) (:point 52 :current t :selected t :window-point 52) (:point 11 :current t :selected t :window-point 11) (error "Moved before first reference" (:point 11 :current t :selected t :window-point 11)) (:point 183 :current t :selected t :window-point 183) (:point 11 :current t :selected t :window-point 11) (:file "src/main.js" :line 2 :column 9 :point 48 :current-buffer t :selected-window-buffer t)) :highlight ((86 87 "界" sameid tide-hl-identifier-face) (123 124 "界" sameid tide-hl-identifier-face)) :unhighlight (:count 0 :overlays nil :first-deleted t) :rehighlight ((86 87 "界" sameid tide-hl-identifier-face) (123 124 "界" sameid tide-hl-identifier-face)) :first-distinct t :second-distinct t :fresh-overlays t) :typed (:scenario references :fixture-count 3 :session-count 1 :sessions ((:first-ordinal 1 :requests (open configure references documentHighlights documentHighlights) :request-count 5 :frame-count 8 :request-sha256 "651182dfe4dbe9d5ff957b2f7c39e84250da15d399383bb5d1828fba78278526" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 2 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "references") :bytes 1351 :sha256 "9ff47c4e692616702b4a8d13a4811962f4eb21fbf656dfd78538a1b23e181e9f" :delivery whole-frame))) :json "{\"command\":\"references\",\"seq\":\"3\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":23}}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 4 :frames ((:kind response :owner (:response 4 "documentHighlights") :bytes 415 :sha256 "c864beed01d97fe37b035820fab2e2873e380a8910135b4111c47b5b653d0df2" :delivery whole-frame))) :json "{\"command\":\"documentHighlights\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":5,\"offset\":24,\"filesToSearch\":[\"[ROOT]/src/main.js\"]}}") (:ordinal 5 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "documentHighlights") :bytes 415 :sha256 "a3477926eccade914b0c9844134ea80b154be991013e82180a93c2eb161f3165" :delivery whole-frame))) :json "{\"command\":\"documentHighlights\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":5,\"offset\":24,\"filesToSearch\":[\"[ROOT]/src/main.js\"]}}")) :termination clean-eof))) :launches ((:name "tsserver" :buffer "*tide-server*" :program [ADAPTER] :arguments ([TSSERVER] "--disableAutomaticTypingAcquisition") :cwd [ROOT] :environment-count 23)) :terminals ((:session 1 :status exit :exit 0 :message "finished\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "references" :callback registered) (:ordinal 4 :command "documentHighlights" :callback registered) (:ordinal 5 :command "documentHighlights" :callback registered)) :public-deletes nil :cleanup clean)"#
        ]],
    )
}

fn highlight_exchange(
    request: usize,
    frame_digest: &str,
    frame_base64: &str,
    generation: &FixtureGeneration,
) -> ReplayExchange {
    RecordedExchange::new(
        ordinal(request),
        TsRequest::DocumentHighlights(PointRequest {
            file: path("src/main.js"),
            point: point(5, 24),
        }),
        generation.clone(),
        ApprovedOutput::frames(
            ordinal(request),
            vec![decoded_frame(
                frame_base64,
                frame_digest,
                vec![ResponseToken::root_path(
                    vec![
                        JsonPathSegment::Key("body"),
                        JsonPathSegment::Index(0),
                        JsonPathSegment::Key("file"),
                    ],
                    path("src/main.js"),
                )],
            )],
        )
        .unwrap(),
    )
    .unwrap()
    .into()
}

const REFERENCES_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((reference-value
        (value root)
        (if (and (listp value) (plist-get value :file))
            (list :file (file-relative-name (plist-get value :file) root)
                  :start (list (tide-plist-get value :start :line)
                               (tide-plist-get value :start :offset))
                  :end (list (tide-plist-get value :end :line)
                             (tide-plist-get value :end :offset))
                  :context-start
                  (and (plist-get value :contextStart)
                       (list (tide-plist-get value :contextStart :line)
                             (tide-plist-get value :contextStart :offset)))
                  :context-end
                  (and (plist-get value :contextEnd)
                       (list (tide-plist-get value :contextEnd :line)
                             (tide-plist-get value :contextEnd :offset)))
                  :line-text (and (plist-get value :lineText)
                                  (copy-sequence (plist-get value :lineText)))
                  :write-access (plist-get value :isWriteAccess))
          value))
       (property-value
        (value root)
        (cond
         ((and (listp value) (plist-get value :file))
          (reference-value value root))
         ((stringp value)
          (list :text (substring-no-properties value)
                :properties
                (let ((position 0) runs)
                  (while (< position (length value))
                    (let ((next (or (next-property-change
                                     position value (length value))
                                    (length value))))
                      (push (list position next
                                  (copy-tree (text-properties-at position value)))
                            runs)
                      (setq position next)))
                  (nreverse runs))))
         (t value)))
       (property-runs
        (property root)
        (let ((position (point-min)) runs)
          (while (< position (point-max))
            (let* ((value (get-text-property position property))
                   (next (or (next-single-property-change
                              position property nil (point-max))
                             (point-max))))
              (when value
                (push (list position next (property-value value root)) runs))
              (setq position next)))
          (nreverse runs)))
       (overlay-state
        ()
        (sort
         (mapcar (lambda (overlay)
                   (list (overlay-start overlay) (overlay-end overlay)
                         (buffer-substring-no-properties
                          (overlay-start overlay) (overlay-end overlay))
                         (overlay-get overlay 'tide-overlay)
                         (overlay-get overlay 'face)))
                 (seq-filter
                  (lambda (overlay)
                    (eq (overlay-get overlay 'tide-overlay) 'sameid))
                  (overlays-in (point-min) (point-max))))
         (lambda (left right) (< (car left) (car right)))))
       (overlay-objects
        ()
        (seq-filter
         (lambda (overlay) (eq (overlay-get overlay 'tide-overlay) 'sameid))
         (overlays-in (point-min) (point-max))))
       (reference-locus
        (references)
        (list :point (point)
              :current (eq (current-buffer) references)
              :selected (eq (window-buffer (selected-window)) references)
              :window-point (window-point (selected-window))))
       (settled-overlay-state
        ()
        (let ((first-objects (overlay-objects))
              (first-state (overlay-state)))
          (accept-process-output (tide-current-server) 0.01)
          (let ((second-objects (overlay-objects))
                (second-state (overlay-state)))
            (accept-process-output (tide-current-server) 0.01)
            (let ((third-objects (overlay-objects))
                  (third-state (overlay-state)))
              (unless (and (equal first-state second-state)
                           (equal second-state third-state)
                           (= (length first-objects) (length second-objects))
                           (= (length second-objects) (length third-objects))
                           (cl-every #'eq first-objects second-objects)
                           (cl-every #'eq second-objects third-objects))
                (error "Tide identifier overlays did not settle"))
              (list :objects third-objects :state third-state))))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (buffer (find-file-noselect main))
           references-state next-1 next-2 previous terminal last cycle visit
           highlights unhighlight rehighlight first-overlays second-overlays)
      (switch-to-buffer buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (tide-setup)
      (tide368-test-assert-current-server)
      (goto-char (point-min))
      (search-forward "add(1")
      (search-backward "add")
      (forward-char 1)
      (tide-references)
      (let ((references (get-buffer "*tide-references*")))
        (unless references (error "Tide references UI was not created"))
        (with-current-buffer references
          (setq references-state
                (list :mode major-mode
                      :read-only buffer-read-only
                      :text (buffer-substring-no-properties (point-min) (point-max))
                      :property-runs
                      (mapcar (lambda (property)
                                (cons property (property-runs property root)))
                              '(line-prefix wrap-prefix tide-line-reference
                                tide-reference face mouse-face help-echo))))
          (pop-to-buffer references)
          (execute-kbd-macro (kbd "n"))
          (setq next-1 (reference-locus references))
          (execute-kbd-macro (kbd "n"))
          (setq next-2 (reference-locus references))
          (execute-kbd-macro (kbd "p"))
          (setq previous (reference-locus references))
          (condition-case condition
              (execute-kbd-macro (kbd "p"))
            (error
             (setq terminal (list (car condition)
                                  (error-message-string condition)
                                  (reference-locus references)))))
          (dotimes (_ 5) (execute-kbd-macro (kbd "n")))
          (setq last (reference-locus references))
          (execute-kbd-macro (kbd "TAB"))
          (setq cycle (reference-locus references))
          (execute-kbd-macro (kbd "RET"))
          (setq visit
                (list :file (file-relative-name buffer-file-name root)
                      :line (line-number-at-pos) :column (current-column)
                      :point (point) :current-buffer (eq (current-buffer) buffer)
                      :selected-window-buffer
                      (eq (window-buffer (selected-window)) buffer)))))
      (switch-to-buffer buffer)
      (goto-char (point-min))
      (search-forward "tabbed")
      (search-forward "界")
      (let ((request-id (number-to-string (1+ tide-request-counter)))
            (deadline (+ (float-time) 5.0)))
        (tide-hl-identifier)
        (while (and (gethash request-id tide-response-callbacks)
                    (< (float-time) deadline))
          (accept-process-output (tide-current-server) 0.01))
      (when (gethash request-id tide-response-callbacks)
          (error "Tide identifier highlight callback did not finish")))
      (let ((settled (settled-overlay-state)))
        (setq first-overlays (plist-get settled :objects)
              highlights (plist-get settled :state)))
      (tide-unhighlight-identifiers)
      (setq unhighlight (list :count (length (overlay-state))
                              :overlays (overlay-state)
                              :first-deleted
                              (cl-every (lambda (overlay)
                                          (null (overlay-buffer overlay)))
                                        first-overlays)))
      (let ((request-id (number-to-string (1+ tide-request-counter)))
            (deadline (+ (float-time) 5.0)))
        (tide-hl-identifier)
        (while (and (gethash request-id tide-response-callbacks)
                    (< (float-time) deadline))
          (accept-process-output (tide-current-server) 0.01))
      (when (gethash request-id tide-response-callbacks)
          (error "Tide identifier rehighlight callback did not finish")))
      (let ((settled (settled-overlay-state)))
        (setq second-overlays (plist-get settled :objects)
              rehighlight (plist-get settled :state)))
      (list :references references-state
            :navigation (list next-1 next-2 previous terminal last cycle visit)
            :highlight highlights :unhighlight unhighlight
            :rehighlight rehighlight
            :first-distinct
            (= (length first-overlays)
               (length (cl-delete-duplicates
                        (copy-sequence first-overlays) :test #'eq)))
            :second-distinct
            (= (length second-overlays)
               (length (cl-delete-duplicates
                        (copy-sequence second-overlays) :test #'eq)))
            :fresh-overlays
            (and (= (length first-overlays) (length second-overlays))
                 (cl-every
                  (lambda (overlay) (not (memq overlay first-overlays)))
                  second-overlays))))))"#;

const DIAGNOSTICS_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((normalize-file
        (file root server-dir)
        (cond
         ((not (stringp file)) file)
         ((string-prefix-p (file-name-as-directory root) file)
          (concat "[ROOT]/" (file-relative-name file root)))
         ((string-prefix-p server-dir file)
          (concat "[TSSERVER-DIR]/"
                  (file-relative-name file server-dir)))
         (t file)))
       (normalize-value
        (value root server-dir)
        (cond
         ((stringp value) (normalize-file (substring-no-properties value)
                                          root server-dir))
         ((consp value)
          (cons (normalize-value (car value) root server-dir)
                (normalize-value (cdr value) root server-dir)))
         ((vectorp value)
          (apply #'vector
                 (mapcar (lambda (entry)
                           (normalize-value entry root server-dir))
                         value)))
         (t value)))
       (diagnostic-state
        (diagnostic root server-dir)
        (and diagnostic
             (normalize-value (copy-tree diagnostic) root server-dir)))
       (flycheck-error-state
        (error root server-dir)
        (list :file (normalize-file (flycheck-error-filename error)
                                    root server-dir)
              :line (flycheck-error-line error)
              :column (flycheck-error-column error)
              :end-line (flycheck-error-end-line error)
              :end-column (flycheck-error-end-column error)
              :level (flycheck-error-level error)
              :id (flycheck-error-id error)
              :checker (flycheck-error-checker error)
              :message (substring-no-properties
                        (flycheck-error-message error))))
       (flycheck-state
        (buffer root server-dir)
        (with-current-buffer buffer
          (list
           :checker flycheck-checker
           :status flycheck-last-status-change
           :errors
           (mapcar (lambda (error)
                     (flycheck-error-state error root server-dir))
                   flycheck-current-errors)
           :overlays
           (mapcar
            (lambda (overlay)
              (let ((error (overlay-get overlay 'flycheck-error)))
                (list :span (list (overlay-start overlay)
                                  (overlay-end overlay))
                      :face (overlay-get overlay 'face)
                      :category (overlay-get overlay 'category)
                      :index (overlay-get overlay 'flycheck-error-index)
                      :owned (and (overlay-get overlay 'flycheck-overlay) t)
                      :help (eq (overlay-get overlay 'help-echo)
                                #'flycheck-help-echo)
                      :error (and error
                                  (flycheck-error-state
                                   error root server-dir)))))
            (flycheck-overlays-in (point-min) (point-max))))))
       (settled-flycheck-state
        (buffer server root server-dir)
        (let ((sample (flycheck-state buffer root server-dir)))
          (dotimes (_ 2)
            (accept-process-output server 0.01)
            (unless (equal sample (flycheck-state buffer root server-dir))
              (error "Tide Flycheck state changed after completion")))
          sample))
       (project-state
        (buffer root server-dir)
        (unless (buffer-live-p buffer)
          (error "Tide project-errors buffer is missing"))
        (with-current-buffer buffer
          (let ((position (point-min)) errors headings pending-heading summary)
            (while (< position (point-max))
              (let* ((diagnostic (get-text-property position 'tide-error))
                     (file-face (get-text-property position 'face))
                     (next (or (next-property-change position nil (point-max))
                               (point-max))))
                (when diagnostic
                  (when pending-heading
                    (push (append pending-heading
                                  (list :file
                                        (normalize-file
                                         (plist-get diagnostic :file)
                                         root server-dir)))
                          headings)
                    (setq pending-heading nil))
                  (push (list :span (list position next)
                              :text (buffer-substring-no-properties position next)
                              :face file-face
                              :diagnostic
                              (diagnostic-state diagnostic root server-dir))
                        errors))
                (when (eq file-face 'tide-file)
                  (setq pending-heading
                        (list :span (list position next) :face file-face)))
                (setq position next)))
            (save-excursion
              (goto-char (point-max))
              (when (re-search-backward
                     "[0-9]+ syntax error(s), [0-9]+ semantic error(s), [0-9]+ suggestion error(s)"
                     nil t)
                (setq summary (match-string-no-properties 0))))
            (list :mode major-mode :point (point)
                  :summary summary :headings (nreverse headings)
                  :errors (nreverse errors)))))
       (settled-project-state
        (buffer server root server-dir)
        (let ((sample (project-state buffer root server-dir)))
          (dotimes (_ 2)
            (accept-process-output server 0.01)
            (unless (equal sample (project-state buffer root server-dir))
              (error "Tide project-errors state changed after completion")))
          sample))
       (wait-until
        (predicate server label)
        (let ((deadline (+ (float-time) 20.0)))
          (while (and (not (funcall predicate)) (< (float-time) deadline))
            (accept-process-output server 0.02))
          (unless (funcall predicate)
            (error "Tide diagnostics wait failed: %S" label))))
       (condition-state
        (thunk)
        (condition-case condition
            (progn (funcall thunk) nil)
          (error (list :condition
                       (list (car condition) (copy-tree (cdr condition))
                             (error-message-string condition))
                       :point (point))))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (math (expand-file-name "src/math.js" root))
           (config (expand-file-name "jsconfig.json" root))
           (tsconfig (expand-file-name "tsconfig.json" root))
           (buffer (find-file-noselect main))
           (server-dir (file-name-as-directory
                        (file-name-directory (plist-get world :server))))
           first-flycheck point-error first-listener second-listener
           repaired-listener first-project
           second-project next-1 next-2 previous before-first last after-last
           visit repaired-flycheck repaired-project option-transition)
      (switch-to-buffer buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (tide-setup)
      (let ((server (tide368-test-assert-current-server)))
        (flycheck-mode 1)
        (flycheck-select-checker 'javascript-tide)
        (flycheck-buffer)
        (wait-until (lambda () (eq flycheck-last-status-change 'finished))
                    server 'initial-flycheck)
        (setq first-flycheck
              (settled-flycheck-state buffer server root server-dir))
        (goto-char (point-min))
        (forward-line 7)
        (move-to-column 13)
        (tide-error-at-point)
        (setq point-error
              (with-current-buffer (get-buffer "*tide-error*")
                (list :mode major-mode
                      :text (buffer-substring-no-properties (point-min) (point-max))
                      :runs
                      (let ((position (point-min)) runs)
                        (while (< position (point-max))
                          (let ((next (or (next-property-change position nil (point-max))
                                          (point-max))))
                            (push (list position next
                                        (normalize-value
                                         (text-properties-at position)
                                         root server-dir))
                                  runs)
                            (setq position next)))
                        (nreverse runs)))))
        (let* ((project (tide-project-name))
               (errors-name (tide-project-errors-buffer-name)))
          (tide-project-errors)
          (wait-until
           (lambda () (gethash project tide-event-listeners))
           server 'jsconfig-listener)
          (setq first-listener (gethash project tide-event-listeners))
          (unless first-listener
            (error "Tide jsconfig project listener was not installed"))
          (wait-until
           (lambda ()
             (tide368-test-terminal-record
              (process-get server 'tide368-session-index)))
           server 'jsconfig-replay-ready)
          (let ((quiet 0) (deadline (+ (float-time) 20.0)))
            (while (and (< quiet 3) (< (float-time) deadline))
              (if (accept-process-output server 0.05)
                  (setq quiet 0)
                (setq quiet (1+ quiet))))
            (unless (= quiet 3) (error "Tide jsconfig diagnostics stayed active")))
          (let* ((errors (get-buffer errors-name))
                 (sample (project-state errors root server-dir)))
            (dotimes (_ 2)
              (accept-process-output server 0.01)
              (unless (and (equal sample (project-state errors root server-dir))
                           (eq first-listener
                               (gethash project tide-event-listeners)))
                (error "Tide jsconfig partial report was unstable")))
            (setq first-project
                  (list :state sample
                        :listener-retained
                        (and first-listener
                             (eq first-listener
                                 (gethash project tide-event-listeners)))
                        :callbacks (hash-table-count tide-response-callbacks))))
          (tide-kill-server)
          (wait-until (lambda () (not (process-live-p server)))
                      server 'public-kill)
          (rename-file config tsconfig t)
          (tide-restart-server)
          (setq server (tide368-test-assert-current-server))
          (tide-project-errors)
          (wait-until
           (lambda ()
             (let ((listener (gethash project tide-event-listeners)))
               (and listener (not (eq listener first-listener)))))
           server 'tsconfig-listener)
          (setq second-listener (gethash project tide-event-listeners))
          (unless (and second-listener (not (eq second-listener first-listener)))
            (error "Tide tsconfig project listener did not replace jsconfig"))
          (wait-until
           (lambda ()
             (and (null (gethash project tide-event-listeners))
                  (let ((errors (get-buffer errors-name)))
                    (and errors
                         (with-current-buffer errors
                           (save-excursion
                             (goto-char (point-max))
                             (search-backward "88 suggestion error(s)" nil t)))))))
           server 'tsconfig-project-errors)
          (setq second-project
                (settled-project-state
                 (get-buffer errors-name) server root server-dir))
          (pop-to-buffer (get-buffer errors-name))
          (execute-kbd-macro (kbd "n"))
          (setq next-1
                (list :point (point)
                      :error (diagnostic-state
                              (get-text-property (point) 'tide-error)
                              root server-dir)))
          (execute-kbd-macro (kbd "n"))
          (setq next-2
                (list :point (point)
                      :error (diagnostic-state
                              (get-text-property (point) 'tide-error)
                              root server-dir)))
          (execute-kbd-macro (kbd "p"))
          (setq previous
                (list :point (point)
                      :error (diagnostic-state
                              (get-text-property (point) 'tide-error)
                              root server-dir)))
          (setq before-first
                (condition-state (lambda () (execute-kbd-macro (kbd "p")))))
          (goto-char (point-max))
          (execute-kbd-macro (kbd "p"))
          (setq last
                (list :point (point)
                      :error (diagnostic-state
                              (get-text-property (point) 'tide-error)
                              root server-dir)))
          (setq after-last
                (condition-state (lambda () (execute-kbd-macro (kbd "n")))))
          (execute-kbd-macro (kbd "RET"))
          (setq visit
                (let* ((window (selected-window))
                       (destination (window-buffer window))
                       (ambient (current-buffer))
                       (destination-point (window-point window)))
                  (with-current-buffer destination
                    (list :file (normalize-file buffer-file-name root server-dir)
                          :line (line-number-at-pos destination-point)
                          :column (save-excursion
                                    (goto-char destination-point)
                                    (current-column))
                          :point destination-point
                          :selected t
                          :current (eq destination ambient)))))
          (switch-to-buffer buffer)
          (goto-char (point-min))
          (when (looking-at "import { multiply } from \"\\./math\\.js\";\r?\n")
            (delete-region (match-beginning 0) (match-end 0)))
          (goto-char (point-min))
          (search-forward "export const label = add(1, 2);")
          (replace-match "export const label = String(add(1, 2));" t t)
          (setq option-transition (list tide-disable-suggestions t)
                tide-disable-suggestions t)
          (unless (null (car option-transition))
            (error "Tide suggestions were disabled before the full report"))
          (save-buffer)
          (flycheck-buffer)
          (wait-until (lambda () (eq flycheck-last-status-change 'finished))
                      server 'repaired-flycheck)
          (setq repaired-flycheck
                (settled-flycheck-state buffer server root server-dir))
          (tide-project-errors)
          (wait-until
           (lambda ()
             (let ((listener (gethash project tide-event-listeners)))
               (and listener
                    (not (eq listener first-listener))
                    (not (eq listener second-listener)))))
           server 'repaired-listener)
          (setq repaired-listener (gethash project tide-event-listeners))
          (unless (and repaired-listener
                       (not (eq repaired-listener first-listener))
                       (not (eq repaired-listener second-listener)))
            (error "Tide repaired project listener identity was reused"))
          (wait-until
           (lambda ()
             (and (null (gethash project tide-event-listeners))
                  (let ((errors (get-buffer errors-name)))
                    (and errors
                         (with-current-buffer errors
                           (save-excursion
                             (goto-char (point-max))
                             (search-backward "0 suggestion error(s)" nil t)))))))
           server 'repaired-project-errors)
          (setq repaired-project
                (settled-project-state
                 (get-buffer errors-name) server root server-dir))))
      (list :initial-flycheck first-flycheck :error-at-point point-error
            :jsconfig first-project :tsconfig second-project
            :listener-replaced (and second-listener
                                    (not (eq second-listener first-listener)))
            :repair-listener-fresh
            (and repaired-listener
                 (not (eq repaired-listener first-listener))
                 (not (eq repaired-listener second-listener)))
            :navigation (list next-1 next-2 previous before-first
                              last after-last visit)
            :option option-transition :repaired-flycheck repaired-flycheck
            :repaired-project repaired-project
            :listener-count (hash-table-count tide-event-listeners)
            :callback-count (hash-table-count tide-response-callbacks)
            :config (list :jsconfig-exists (file-exists-p config)
                          :tsconfig-exists (file-exists-p tsconfig)
                          :tsconfig-sha256
                          (and (file-exists-p tsconfig)
                               (tide368-test-file-sha256 tsconfig)))
            :main-sha256 (secure-hash 'sha256
                                      (with-temp-buffer
                                        (set-buffer-multibyte nil)
                                        (insert-file-contents-literally main)
                                        (buffer-string)))
            :math-sha256 (secure-hash 'sha256
                                      (with-temp-buffer
                                        (set-buffer-multibyte nil)
                                        (insert-file-contents-literally math)
                                        (buffer-string)))))))"#;

fn flycheck_diagnostics_and_project_errors() -> ParityBatchCase {
    let capture: Value = serde_json::from_str(DIAGNOSTICS_CAPTURE)
        .expect("parse the normalized Tide diagnostics capture");
    assert_eq!(
        Sha256Digest::of(DIAGNOSTICS_CAPTURE.as_bytes()),
        digest(DIAGNOSTICS_CAPTURE_ASSET_SHA256),
    );
    let records = diagnostics_capture_records(&capture);
    assert_eq!(records.len(), 464);
    let mut approved = diagnostics_approved_frames(&capture, &records);
    let main = path("src/main.js");
    let initial = common_manifest().generation();
    let tsconfig = FixtureGeneration::new(vec![
        FixtureExpectation::Missing(path("jsconfig.json")),
        FixtureExpectation::Present {
            path: path("tsconfig.json"),
            digest: digest("06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c"),
        },
        FixtureExpectation::Present {
            path: main.clone(),
            digest: digest("da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7"),
        },
        FixtureExpectation::Present {
            path: path("src/math.js"),
            digest: digest("ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4"),
        },
    ])
    .expect("complete post-config-rename Tide generation");
    assert_recorded_bytes_digest(
        "repaired diagnostics main fixture",
        REPAIRED_MAIN_BYTES,
        "9ce9f7d98fe4d67dc9fb68114aad839fe221f1953dbcc497b727bacf6d6c0b80",
    );
    let repaired = FixtureGeneration::new(vec![
        FixtureExpectation::Missing(path("jsconfig.json")),
        FixtureExpectation::Present {
            path: path("tsconfig.json"),
            digest: digest("06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c"),
        },
        FixtureExpectation::Present {
            path: main.clone(),
            digest: digest("9ce9f7d98fe4d67dc9fb68114aad839fe221f1953dbcc497b727bacf6d6c0b80"),
        },
        FixtureExpectation::Present {
            path: path("src/math.js"),
            digest: digest("ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4"),
        },
    ])
    .expect("complete repaired Tide diagnostics generation");
    let requests = vec![
        TsRequest::Open(OpenRequest::immediate(main.clone(), ScriptKind::JavaScript).unwrap()),
        configure_request(),
        TsRequest::Diagnostics(
            DiagnosticKind::Syntactic,
            FileRequest { file: main.clone() },
        ),
        TsRequest::Diagnostics(DiagnosticKind::Semantic, FileRequest { file: main.clone() }),
        TsRequest::Diagnostics(
            DiagnosticKind::Suggestion,
            FileRequest { file: main.clone() },
        ),
        TsRequest::ProjectInfo(ProjectInfoRequest {
            file: main.clone(),
            file_names: FileNameListRequest::Include,
        }),
        TsRequest::ProjectErrors(ProjectErrorsRequest { file: main.clone() }),
        TsRequest::Open(OpenRequest::immediate(main.clone(), ScriptKind::JavaScript).unwrap()),
        configure_request(),
        TsRequest::ProjectInfo(ProjectInfoRequest {
            file: main.clone(),
            file_names: FileNameListRequest::Include,
        }),
        TsRequest::ProjectErrors(ProjectErrorsRequest { file: main.clone() }),
        TsRequest::Reload(ReloadRequest {
            file: main.clone(),
            temporary_file: TideTempFileToken::new(
                main.clone(),
                digest("9ce9f7d98fe4d67dc9fb68114aad839fe221f1953dbcc497b727bacf6d6c0b80"),
            ),
        }),
        TsRequest::Diagnostics(
            DiagnosticKind::Syntactic,
            FileRequest { file: main.clone() },
        ),
        TsRequest::Diagnostics(DiagnosticKind::Semantic, FileRequest { file: main.clone() }),
        TsRequest::ProjectInfo(ProjectInfoRequest {
            file: main.clone(),
            file_names: FileNameListRequest::Include,
        }),
        TsRequest::ProjectErrors(ProjectErrorsRequest { file: main.clone() }),
    ];
    let root = capture_object(&capture);
    let request_records = capture_array(
        root.get("requests")
            .expect("recorded Tide diagnostics requests"),
    );
    assert_eq!(request_records.len(), requests.len());
    for (index, (request, record)) in requests.iter().zip(request_records).enumerate() {
        let record = capture_object(record);
        let ordinal = ordinal(index + 1);
        let normalized = format!("{}\n", request.normalized_json(ordinal));
        assert_eq!(normalized, capture_string(record, "normalized"));
        assert_eq!(
            Sha256Digest::of(normalized.as_bytes()),
            digest(capture_string(record, "normalized_sha256")),
        );
        assert_eq!(capture_usize(record, "ordinal"), index + 1);
        assert_eq!(capture_string(record, "command"), request.command());
    }
    let mut exchanges = Vec::with_capacity(requests.len());
    for (index, request) in requests.into_iter().enumerate() {
        let owner = index + 1;
        let owned = records
            .iter()
            .filter(|frame| frame.exchange_owner == owner)
            .collect::<Vec<_>>();
        let output = if owned.is_empty() {
            ApprovedOutput::no_frames()
        } else {
            let boundary = owned[0].delivery_after;
            assert!(owned.iter().all(|frame| frame.delivery_after == boundary));
            let frames = owned
                .iter()
                .map(|record| {
                    approved
                        .remove(&record.row)
                        .expect("approved Tide diagnostics frame by capture row")
                })
                .collect();
            if boundary == owner {
                ApprovedOutput::frames(ordinal(owner), frames).unwrap()
            } else {
                ApprovedOutput::frames_delayed(ordinal(boundary), frames).unwrap()
            }
        };
        let generation = match owner {
            1..=7 => initial.clone(),
            8..=11 => tsconfig.clone(),
            12..=16 => repaired.clone(),
            _ => unreachable!(),
        };
        let exchange = if output
            .delivery_after()
            .is_some_and(|boundary| boundary.get() > owner)
        {
            RecordedExchange::new_delayed(ordinal(owner), request, generation, output).unwrap()
        } else {
            RecordedExchange::new(ordinal(owner), request, generation, output).unwrap()
        };
        exchanges.push(exchange.into());
    }
    assert!(approved.is_empty());
    let second = exchanges.split_off(7);
    let first = ReplaySession::new(
        exchanges,
        digest("2753958578bf0a45f42485fff003c02444af453da19a31cfc32bcdd49f40b654"),
        digest("a93ce1225454939e7c635e83051925d2cde86165e372b52ef079ad9dfa44b27d"),
        ReplayTermination::ClientKilled {
            ready_after: ordinal(7),
        },
    )
    .unwrap();
    let second = ReplaySession::new(
        second,
        digest("f189d280cbaab1a1dd35df4f37455bbb3cb7fe74630cea129e8fed243fef47a8"),
        digest("1b4b9ce059b1503af65602f42608671d3e379448bafb8465a6380c23ec63c29a"),
        ReplayTermination::CleanEof,
    )
    .unwrap();
    let replay = TideReplay::new(
        TideScenario::Diagnostics,
        common_manifest(),
        vec![first, second],
    )
    .unwrap();
    materialized_case(
        "flycheck_diagnostics_and_project_errors",
        replay,
        DIAGNOSTICS_BODY,
        expect![[
            r#"OK (:result (:initial-flycheck (:checker javascript-tide :status finished :errors ((:file "[ROOT]/src/main.js" :line 8 :column 14 :end-line nil :end-column nil :level error :id 2322 :checker javascript-tide :message "Type 'number' is not assignable to type 'string'.") (:file "[ROOT]/src/main.js" :line 1 :column 1 :end-line nil :end-column nil :level info :id 6133 :checker javascript-tide :message "'multiply' is declared but its value is never read.")) :overlays ((:span (1 7) :face flycheck-info :category flycheck-info-overlay :index 1 :owned t :help t :error (:file "[ROOT]/src/main.js" :line 1 :column 1 :end-line nil :end-column nil :level info :id 6133 :checker javascript-tide :message "'multiply' is declared but its value is never read.")) (:span (162 167) :face flycheck-error :category flycheck-error-overlay :index 2 :owned t :help t :error (:file "[ROOT]/src/main.js" :line 8 :column 14 :end-line nil :end-column nil :level error :id 2322 :checker javascript-tide :message "Type 'number' is not assignable to type 'string'.")))) :error-at-point (:mode fundamental-mode :text "Code: 2322 Category: error\n\nType 'number' is not assignable to type 'string'.\n\n" :runs ((1 7 (face bold)) (7 12 nil) (12 22 (face bold)) (22 29 nil) (29 30 (diagnostic (:start (:line 8 :offset 14) :end (:line 8 :offset 19) :text "Type 'number' is not assignable to type 'string'." :code 2322 :category "error"))) (30 80 nil))) :jsconfig (:state (:mode tide-project-errors-mode :point 1 :summary nil :headings ((:span (1 12) :face tide-file :file "[ROOT]/src/main.js")) :errors ((:span (13 18) :text "    8" :face tide-line-number :diagnostic (:start (:line 8 :offset 14) :end (:line 8 :offset 19) :text "Type 'number' is not assignable to type 'string'." :code 2322 :category "error" :file "[ROOT]/src/main.js")) (:span (70 75) :text "    1" :face tide-line-number :diagnostic (:start (:line 1 :offset 1) :end (:line 1 :offset 38) :text "'multiply' is declared but its value is never read." :code 6133 :category "suggestion" :reportsUnnecessary t :file "[ROOT]/src/main.js")))) :listener-retained t :callbacks 0) :tsconfig (:mode tide-project-errors-mode :point 1 :summary "0 syntax error(s), 1 semantic error(s), 88 suggestion error(s)" :headings ((:span (1 12) :face tide-file :file "[ROOT]/src/main.js") (:span (129 358) :face tide-file :file "[TSSERVER-DIR]/lib.es5.d.ts") (:span (411 640) :face tide-file :file "[TSSERVER-DIR]/lib.dom.d.ts") (:span (3968 4206) :face tide-file :file "[TSSERVER-DIR]/lib.dom.iterable.d.ts")) :errors ((:span (13 18) :text "    8" :face tide-line-number :diagnostic (:start (:line 8 :offset 14) :end (:line 8 :offset 19) :text "Type 'number' is not assignable to type 'string'." :code 2322 :category "error" :file "[ROOT]/src/main.js")) (:span (70 75) :text "    1" :face tide-line-number :diagnostic (:start (:line 1 :offset 1) :end (:line 1 :offset 38) :text "'multiply' is declared but its value is never read." :code 6133 :category "suggestion" :reportsUnnecessary t :file "[ROOT]/src/main.js")) (:span (359 364) :text " 1666" :face tide-line-number :diagnostic (:start (:line 1666 :offset 19) :end (:line 1666 :offset 22) :text "'T' is declared but its value is never read." :code 6133 :category "suggestion" :reportsUnnecessary t :file "[TSSERVER-DIR]/lib.es5.d.ts")) (:span (641 646) :text " 2824" :face tide-line-number :diagnostic (:start (:line 2824 :offset 16) :end (:line 2824 :offset 36) :text "'AudioProcessingEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 2797 :offset 4) :end (:line 2800 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (686 691) :text " 2825" :face tide-line-number :diagnostic (:start (:line 2825 :offset 65) :end (:line 2825 :offset 85) :text "'AudioProcessingEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 2797 :offset 4) :end (:line 2800 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (731 736) :text " 3015" :face tide-line-number :diagnostic (:start (:line 3015 :offset 114) :end (:line 3015 :offset 133) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (775 780) :text " 7069" :face tide-line-number :diagnostic (:start (:line 7069 :offset 58) :end (:line 7069 :offset 78) :text "'AudioProcessingEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 2797 :offset 4) :end (:line 2800 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (820 825) :text " 7099" :face tide-line-number :diagnostic (:start (:line 7099 :offset 51) :end (:line 7099 :offset 64) :text "'MutationEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15585 :offset 4) :end (:line 15588 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (858 863) :text " 7100" :face tide-line-number :diagnostic (:start (:line 7100 :offset 52) :end (:line 7100 :offset 65) :text "'MutationEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15585 :offset 4) :end (:line 15588 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (896 901) :text " 8210" :face tide-line-number :diagnostic (:start (:line 8210 :offset 16) :end (:line 8210 :offset 24) :text "'External' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 8189 :offset 4) :end (:line 8192 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (929 934) :text " 8211" :face tide-line-number :diagnostic (:start (:line 8211 :offset 12) :end (:line 8211 :offset 20) :text "'External' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 8189 :offset 4) :end (:line 8192 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (962 967) :text " 9947" :face tide-line-number :diagnostic (:start (:line 9947 :offset 85) :end (:line 9947 :offset 105) :text "'HTMLDirectoryElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9943 :offset 5) :end (:line 9943 :offset 17) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1007 1012) :text " 9949" :face tide-line-number :diagnostic (:start (:line 9949 :offset 88) :end (:line 9949 :offset 108) :text "'HTMLDirectoryElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9943 :offset 5) :end (:line 9943 :offset 17) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1052 1057) :text " 9955" :face tide-line-number :diagnostic (:start (:line 9955 :offset 16) :end (:line 9955 :offset 36) :text "'HTMLDirectoryElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9943 :offset 5) :end (:line 9943 :offset 17) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1097 1102) :text " 9956" :face tide-line-number :diagnostic (:start (:line 9956 :offset 12) :end (:line 9956 :offset 32) :text "'HTMLDirectoryElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9943 :offset 5) :end (:line 9943 :offset 17) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1142 1147) :text " 9985" :face tide-line-number :diagnostic (:start (:line 9985 :offset 82) :end (:line 9985 :offset 94) :text "'HTMLDocument' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9983 :offset 5) :end (:line 9983 :offset 30) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1179 1184) :text " 9987" :face tide-line-number :diagnostic (:start (:line 9987 :offset 85) :end (:line 9987 :offset 97) :text "'HTMLDocument' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9983 :offset 5) :end (:line 9983 :offset 30) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1216 1221) :text " 9993" :face tide-line-number :diagnostic (:start (:line 9993 :offset 16) :end (:line 9993 :offset 28) :text "'HTMLDocument' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9983 :offset 5) :end (:line 9983 :offset 30) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1253 1258) :text " 9994" :face tide-line-number :diagnostic (:start (:line 9994 :offset 12) :end (:line 9994 :offset 24) :text "'HTMLDocument' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9983 :offset 5) :end (:line 9983 :offset 30) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1290 1295) :text "10179" :face tide-line-number :diagnostic (:start (:line 10179 :offset 85) :end (:line 10179 :offset 100) :text "'HTMLFontElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10155 :offset 4) :end (:line 10158 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1330 1335) :text "10181" :face tide-line-number :diagnostic (:start (:line 10181 :offset 88) :end (:line 10181 :offset 103) :text "'HTMLFontElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10155 :offset 4) :end (:line 10158 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1370 1375) :text "10187" :face tide-line-number :diagnostic (:start (:line 10187 :offset 16) :end (:line 10187 :offset 31) :text "'HTMLFontElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10155 :offset 4) :end (:line 10158 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1410 1415) :text "10188" :face tide-line-number :diagnostic (:start (:line 10188 :offset 12) :end (:line 10188 :offset 27) :text "'HTMLFontElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10155 :offset 4) :end (:line 10158 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1450 1455) :text "10397" :face tide-line-number :diagnostic (:start (:line 10397 :offset 85) :end (:line 10397 :offset 101) :text "'HTMLFrameElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10322 :offset 4) :end (:line 10325 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1491 1496) :text "10399" :face tide-line-number :diagnostic (:start (:line 10399 :offset 88) :end (:line 10399 :offset 104) :text "'HTMLFrameElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10322 :offset 4) :end (:line 10325 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1532 1537) :text "10405" :face tide-line-number :diagnostic (:start (:line 10405 :offset 16) :end (:line 10405 :offset 32) :text "'HTMLFrameElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10322 :offset 4) :end (:line 10325 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1573 1578) :text "10406" :face tide-line-number :diagnostic (:start (:line 10406 :offset 12) :end (:line 10406 :offset 28) :text "'HTMLFrameElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10322 :offset 4) :end (:line 10325 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1614 1619) :text "10429" :face tide-line-number :diagnostic (:start (:line 10429 :offset 93) :end (:line 10429 :offset 112) :text "'HTMLFrameSetElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10414 :offset 4) :end (:line 10417 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1658 1663) :text "10431" :face tide-line-number :diagnostic (:start (:line 10431 :offset 96) :end (:line 10431 :offset 115) :text "'HTMLFrameSetElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10414 :offset 4) :end (:line 10417 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1702 1707) :text "10437" :face tide-line-number :diagnostic (:start (:line 10437 :offset 16) :end (:line 10437 :offset 35) :text "'HTMLFrameSetElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10414 :offset 4) :end (:line 10417 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1746 1751) :text "10438" :face tide-line-number :diagnostic (:start (:line 10438 :offset 12) :end (:line 10438 :offset 31) :text "'HTMLFrameSetElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10414 :offset 4) :end (:line 10417 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1790 1795) :text "11316" :face tide-line-number :diagnostic (:start (:line 11316 :offset 85) :end (:line 11316 :offset 103) :text "'HTMLMarqueeElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 11285 :offset 4) :end (:line 11288 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1833 1838) :text "11318" :face tide-line-number :diagnostic (:start (:line 11318 :offset 88) :end (:line 11318 :offset 106) :text "'HTMLMarqueeElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 11285 :offset 4) :end (:line 11288 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1876 1881) :text "11324" :face tide-line-number :diagnostic (:start (:line 11324 :offset 16) :end (:line 11324 :offset 34) :text "'HTMLMarqueeElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 11285 :offset 4) :end (:line 11288 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1919 1924) :text "11325" :face tide-line-number :diagnostic (:start (:line 11325 :offset 12) :end (:line 11325 :offset 30) :text "'HTMLMarqueeElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 11285 :offset 4) :end (:line 11288 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (1962 1967) :text "12116" :face tide-line-number :diagnostic (:start (:line 12116 :offset 85) :end (:line 12116 :offset 101) :text "'HTMLParamElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12083 :offset 4) :end (:line 12086 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2003 2008) :text "12118" :face tide-line-number :diagnostic (:start (:line 12118 :offset 88) :end (:line 12118 :offset 104) :text "'HTMLParamElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12083 :offset 4) :end (:line 12086 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2044 2049) :text "12124" :face tide-line-number :diagnostic (:start (:line 12124 :offset 16) :end (:line 12124 :offset 32) :text "'HTMLParamElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12083 :offset 4) :end (:line 12086 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2085 2090) :text "12125" :face tide-line-number :diagnostic (:start (:line 12125 :offset 12) :end (:line 12125 :offset 28) :text "'HTMLParamElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12083 :offset 4) :end (:line 12086 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2126 2131) :text "12735" :face tide-line-number :diagnostic (:start (:line 12735 :offset 85) :end (:line 12735 :offset 109) :text "'HTMLTableDataCellElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12733 :offset 5) :end (:line 12733 :offset 45) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2175 2180) :text "12737" :face tide-line-number :diagnostic (:start (:line 12737 :offset 88) :end (:line 12737 :offset 112) :text "'HTMLTableDataCellElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12733 :offset 5) :end (:line 12733 :offset 45) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2224 2229) :text "12908" :face tide-line-number :diagnostic (:start (:line 12908 :offset 85) :end (:line 12908 :offset 111) :text "'HTMLTableHeaderCellElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12906 :offset 5) :end (:line 12906 :offset 45) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2275 2280) :text "12910" :face tide-line-number :diagnostic (:start (:line 12910 :offset 88) :end (:line 12910 :offset 114) :text "'HTMLTableHeaderCellElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12906 :offset 5) :end (:line 12906 :offset 45) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2326 2331) :text "15467" :face tide-line-number :diagnostic (:start (:line 15467 :offset 29) :end (:line 15467 :offset 35) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2357 2362) :text "15486" :face tide-line-number :diagnostic (:start (:line 15486 :offset 16) :end (:line 15486 :offset 24) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2390 2395) :text "15487" :face tide-line-number :diagnostic (:start (:line 15487 :offset 12) :end (:line 15487 :offset 20) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2423 2428) :text "15508" :face tide-line-number :diagnostic (:start (:line 15508 :offset 26) :end (:line 15508 :offset 34) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2456 2461) :text "15514" :face tide-line-number :diagnostic (:start (:line 15514 :offset 30) :end (:line 15514 :offset 38) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2489 2494) :text "15515" :face tide-line-number :diagnostic (:start (:line 15515 :offset 22) :end (:line 15515 :offset 30) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2522 2527) :text "15520" :face tide-line-number :diagnostic (:start (:line 15520 :offset 16) :end (:line 15520 :offset 29) :text "'MimeTypeArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15492 :offset 4) :end (:line 15495 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2560 2565) :text "15521" :face tide-line-number :diagnostic (:start (:line 15521 :offset 12) :end (:line 15521 :offset 25) :text "'MimeTypeArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15492 :offset 4) :end (:line 15495 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2598 2603) :text "15633" :face tide-line-number :diagnostic (:start (:line 15633 :offset 16) :end (:line 15633 :offset 29) :text "'MutationEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15585 :offset 4) :end (:line 15588 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2636 2641) :text "15634" :face tide-line-number :diagnostic (:start (:line 15634 :offset 12) :end (:line 15634 :offset 25) :text "'MutationEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15585 :offset 4) :end (:line 15588 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2674 2679) :text "15982" :face tide-line-number :diagnostic (:start (:line 15982 :offset 25) :end (:line 15982 :offset 38) :text "'MimeTypeArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15492 :offset 4) :end (:line 15495 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2712 2717) :text "15990" :face tide-line-number :diagnostic (:start (:line 15990 :offset 23) :end (:line 15990 :offset 34) :text "'PluginArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17483 :offset 4) :end (:line 17486 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2748 2753) :text "16897" :face tide-line-number :diagnostic (:start (:line 16897 :offset 26) :end (:line 16897 :offset 47) :text "'PerformanceNavigation' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17015 :offset 4) :end (:line 17018 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2794 2799) :text "16907" :face tide-line-number :diagnostic (:start (:line 16907 :offset 22) :end (:line 16907 :offset 39) :text "'PerformanceTiming' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17200 :offset 4) :end (:line 17203 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2836 2841) :text "17046" :face tide-line-number :diagnostic (:start (:line 17046 :offset 16) :end (:line 17046 :offset 37) :text "'PerformanceNavigation' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17015 :offset 4) :end (:line 17018 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2882 2887) :text "17047" :face tide-line-number :diagnostic (:start (:line 17047 :offset 12) :end (:line 17047 :offset 33) :text "'PerformanceNavigation' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17015 :offset 4) :end (:line 17018 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2928 2933) :text "17341" :face tide-line-number :diagnostic (:start (:line 17341 :offset 16) :end (:line 17341 :offset 33) :text "'PerformanceTiming' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17200 :offset 4) :end (:line 17203 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (2970 2975) :text "17342" :face tide-line-number :diagnostic (:start (:line 17342 :offset 12) :end (:line 17342 :offset 29) :text "'PerformanceTiming' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17200 :offset 4) :end (:line 17203 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3012 3017) :text "17465" :face tide-line-number :diagnostic (:start (:line 17465 :offset 26) :end (:line 17465 :offset 34) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3045 3050) :text "17471" :face tide-line-number :diagnostic (:start (:line 17471 :offset 30) :end (:line 17471 :offset 38) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3078 3083) :text "17472" :face tide-line-number :diagnostic (:start (:line 17472 :offset 22) :end (:line 17472 :offset 30) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3111 3116) :text "17477" :face tide-line-number :diagnostic (:start (:line 17477 :offset 16) :end (:line 17477 :offset 22) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3142 3147) :text "17478" :face tide-line-number :diagnostic (:start (:line 17478 :offset 12) :end (:line 17478 :offset 18) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3173 3178) :text "17499" :face tide-line-number :diagnostic (:start (:line 17499 :offset 26) :end (:line 17499 :offset 32) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3204 3209) :text "17505" :face tide-line-number :diagnostic (:start (:line 17505 :offset 30) :end (:line 17505 :offset 36) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3235 3240) :text "17512" :face tide-line-number :diagnostic (:start (:line 17512 :offset 22) :end (:line 17512 :offset 28) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3266 3271) :text "17517" :face tide-line-number :diagnostic (:start (:line 17517 :offset 16) :end (:line 17517 :offset 27) :text "'PluginArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17483 :offset 4) :end (:line 17486 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3302 3307) :text "17518" :face tide-line-number :diagnostic (:start (:line 17518 :offset 12) :end (:line 17518 :offset 23) :text "'PluginArray' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17483 :offset 4) :end (:line 17486 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3338 3343) :text "20934" :face tide-line-number :diagnostic (:start (:line 20934 :offset 21) :end (:line 20934 :offset 41) :text "'AudioProcessingEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 2797 :offset 4) :end (:line 2800 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3383 3388) :text "20955" :face tide-line-number :diagnostic (:start (:line 20955 :offset 29) :end (:line 20955 :offset 48) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3427 3432) :text "20955" :face tide-line-number :diagnostic (:start (:line 20955 :offset 54) :end (:line 20955 :offset 74) :text "'AudioProcessingEvent' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 2797 :offset 4) :end (:line 2800 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3472 3477) :text "20956" :face tide-line-number :diagnostic (:start (:line 20956 :offset 93) :end (:line 20956 :offset 112) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3516 3521) :text "20958" :face tide-line-number :diagnostic (:start (:line 20958 :offset 96) :end (:line 20958 :offset 115) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3560 3565) :text "20964" :face tide-line-number :diagnostic (:start (:line 20964 :offset 16) :end (:line 20964 :offset 35) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3604 3609) :text "20965" :face tide-line-number :diagnostic (:start (:line 20965 :offset 12) :end (:line 20965 :offset 31) :text "'ScriptProcessorNode' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 20939 :offset 4) :end (:line 20942 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3648 3653) :text "25429" :face tide-line-number :diagnostic (:start (:line 25429 :offset 24) :end (:line 25429 :offset 32) :text "'External' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 8189 :offset 4) :end (:line 8192 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3681 3686) :text "26832" :face tide-line-number :diagnostic (:start (:line 26832 :offset 12) :end (:line 26832 :offset 32) :text "'HTMLDirectoryElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 9943 :offset 5) :end (:line 9943 :offset 17) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3726 3731) :text "26833" :face tide-line-number :diagnostic (:start (:line 26833 :offset 13) :end (:line 26833 :offset 28) :text "'HTMLFontElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10155 :offset 4) :end (:line 10158 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3766 3771) :text "26834" :face tide-line-number :diagnostic (:start (:line 26834 :offset 14) :end (:line 26834 :offset 30) :text "'HTMLFrameElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10322 :offset 4) :end (:line 10325 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3807 3812) :text "26835" :face tide-line-number :diagnostic (:start (:line 26835 :offset 17) :end (:line 26835 :offset 36) :text "'HTMLFrameSetElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 10414 :offset 4) :end (:line 10417 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3851 3856) :text "26839" :face tide-line-number :diagnostic (:start (:line 26839 :offset 16) :end (:line 26839 :offset 34) :text "'HTMLMarqueeElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 11285 :offset 4) :end (:line 11288 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3894 3899) :text "26846" :face tide-line-number :diagnostic (:start (:line 26846 :offset 14) :end (:line 26846 :offset 30) :text "'HTMLParamElement' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 12083 :offset 4) :end (:line 12086 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (3935 3940) :text "27000" :face tide-line-number :diagnostic (:start (:line 27000 :offset 23) :end (:line 27000 :offset 31) :text "'External' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 8189 :offset 4) :end (:line 8192 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.d.ts")) (:span (4207 4212) :text "  207" :face tide-line-number :diagnostic (:start (:line 207 :offset 43) :end (:line 207 :offset 51) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.iterable.d.ts")) (:span (4240 4245) :text "  246" :face tide-line-number :diagnostic (:start (:line 246 :offset 43) :end (:line 246 :offset 51) :text "'MimeType' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 15449 :offset 4) :end (:line 15452 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.iterable.d.ts")) (:span (4273 4278) :text "  250" :face tide-line-number :diagnostic (:start (:line 250 :offset 43) :end (:line 250 :offset 49) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.iterable.d.ts")))) :listener-replaced t :repair-listener-fresh t :navigation ((:point 13 :error (:start (:line 8 :offset 14) :end (:line 8 :offset 19) :text "Type 'number' is not assignable to type 'string'." :code 2322 :category "error" :file "[ROOT]/src/main.js")) (:point 70 :error (:start (:line 1 :offset 1) :end (:line 1 :offset 38) :text "'multiply' is declared but its value is never read." :code 6133 :category "suggestion" :reportsUnnecessary t :file "[ROOT]/src/main.js")) (:point 13 :error (:start (:line 8 :offset 14) :end (:line 8 :offset 19) :text "Type 'number' is not assignable to type 'string'." :code 2322 :category "error" :file "[ROOT]/src/main.js")) (:condition (error ("Moved back before first error") "Moved back before first error") :point 13) (:point 4273 :error (:start (:line 250 :offset 43) :end (:line 250 :offset 49) :text "'Plugin' is deprecated." :code 6385 :category "suggestion" :reportsDeprecated t :relatedInformation ((:span (:start (:line 17428 :offset 4) :end (:line 17431 :offset 2) :file "[TSSERVER-DIR]/lib.dom.d.ts") :message "The declaration was marked as deprecated here." :category "error" :code 2798)) :file "[TSSERVER-DIR]/lib.dom.iterable.d.ts")) (:condition (error ("Moved past last error") "Moved past last error") :point 4273) (:file "[TSSERVER-DIR]/lib.dom.iterable.d.ts" :line 250 :column 42 :point 9037 :selected t :current t)) :option (nil t) :repaired-flycheck (:checker javascript-tide :status finished :errors nil :overlays nil) :repaired-project (:mode tide-project-errors-mode :point 1 :summary "0 syntax error(s), 0 semantic error(s), 0 suggestion error(s)" :headings nil :errors nil) :listener-count 0 :callback-count 0 :config (:jsconfig-exists nil :tsconfig-exists t :tsconfig-sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :main-sha256 "9ce9f7d98fe4d67dc9fb68114aad839fe221f1953dbcc497b727bacf6d6c0b80" :math-sha256 "ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4") :typed (:scenario diagnostics :fixture-count 3 :session-count 2 :sessions ((:first-ordinal 1 :requests (open configure syntacticDiagnosticsSync semanticDiagnosticsSync suggestionDiagnosticsSync projectInfo geterrForProject) :request-count 7 :frame-count 157 :request-sha256 "2753958578bf0a45f42485fff003c02444af453da19a31cfc32bcdd49f40b654" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 5 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 3 "syntacticDiagnosticsSync") :bytes 131 :sha256 "41c6f708a871007feb98ce2203579621c54798cc0d0cc6cf684d97fd79bd7f13" :delivery whole-frame))) :json "{\"command\":\"syntacticDiagnosticsSync\",\"seq\":\"3\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 4 "semanticDiagnosticsSync") :bytes 281 :sha256 "bce2c36371275072e450d9970fde1c5e534e3479b6adfed1b0ae2baf33ceae07" :delivery whole-frame))) :json "{\"command\":\"semanticDiagnosticsSync\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 5 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "suggestionDiagnosticsSync") :bytes 315 :sha256 "147c6889c8b2fd31608d514d1173323b8ce2fbc2c6d9b84dc6f7c24b21dc6ffa" :delivery whole-frame))) :json "{\"command\":\"suggestionDiagnosticsSync\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 6 :outcome complete :callback registered :output (:delivery-after 6 :frames ((:kind response :owner (:response 6 "projectInfo") :bytes 2150 :sha256 "6f2b2d875fecc9f66f3271a5ea67c32f61f3a7fd73f564176e567cd9368c00cd" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"6\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":true}}") (:ordinal 7 :outcome complete :callback not-registered :output (:delivery-after 7 :frames ((:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "0d4e111326bc3af4ba39376b68db9938c14db1190a3138e7f06bdaed4e7cd9e8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 276 :sha256 "8672b7515433258165c816cfa0593fa5cfdd670ecd2b2be581eb217a69afdce1" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 310 :sha256 "2689d489fceb74f419d6d398969c0e294466cd4b50ceb6364e271bf4f9576d06" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "413cd61038697a6689a85d072a64db33da53a8e5a7635fce14c02eb757207ab1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 125 :sha256 "bcaa4852453a930845157158366499235093602124f0caeba33d4e3c80f3e6c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 127 :sha256 "5f61beb2584b4a6f66e56ef8f7843db254c6f81251b7c5a8f06596eab4661116" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "8ea0300969791be8aa46713870c8f665b60a7c2107b3f225956ebc554ae3a5f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "cb901ae31554d8a48f8045f5471dd107c427523a297465170f0fd8fcb8196310" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 136 :sha256 "7588b9b6377d17e3ee814bb009f34041d8a3ce09517053d9a0c2fb596630aa24" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "5f099d206bcdfc6af6d63b19fe21fc4ab295d1fe0b3a1efdfd05407b38c31d3f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "79c589fd428d536ebcdec9200fbceb1ec426bd1ea0c42c17846034cade93c4ae" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "333102224e6f0ada69c88b377dd308df45c6e7bb2ad55da484eed51b95c47c19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "742b4fee0a971ec7f66956e7164c1ec393469e9a4f4299837fc9ef51f1388632" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "f1d8476e053ded043c55d53e5ac0797fe9dbe18db5ff44530b029fe09915f5f9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "2087a6ee198e1535792edc0614182fec66b8e1d7b6358c34bf7d223ebca19b73" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53c6870e8afbd8c209afb07e845c4b2b48c7fe60c38223fe68f38b07ee06a48a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "71001654875e3f1fe0dd7b91b636982f0c5204f2a499a9e67c7f456ece3e8345" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "74fb3f40f217509ff51d13a493507d41cfa147a5a1add72fd6883e6c5e17752f" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53a49af37906f007c41911be3eba08ec5c1bddd859cbc4bf551f6bed711d0b7a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "58125398906518cac469fbc0879b34bb7a3f2bb83fcf6505e23bd2defb0cb2ba" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "a8157b81a021b42e7dbf1870b465d3fa1039bb2c369dcedc9ae38a12267a86f1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "6f50d058a56f4363da12c236d933e47d28e33940f4437b0105bc5243a87f2d28" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "daccc5a166d678c297dfa8cabdd54f98ba2708e323a1028bd5779b1de20bc281" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "143caff58ca6360cc2f43407a7044382fb750c9bf4087e44b6f284b636d76d22" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "d466e6adf142362dbb9df8c58c2e330c4abc581421db92d47b7ff3de906f65ac" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "dda38924dd2575bd41ce148fa0c38b6bc91e1e733cdc0625d1e1097bd07b7d6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "aabc9a0c1d34a662675838cd82585d92a6db79a916b9ad612545d8a3c2ea9a45" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "117416684cfd553d7a4f424020f9ae968af3d991f4bf455a6fdb7feff1214c78" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "5366023b1a3ea85a2ba8d79275f3e73b3694aea1f4703e2b09101a6bedebb37c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 136 :sha256 "738a0f60a179e0107f99d397556b9228dcb2d4d91e0320bb5b5b5d4179a95072" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "1437b88bac23774ea649f4c1210476a6071e6e83573c63cffa975f48941067c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "377f4d2ad40e4d52866fec4b376c49fa3adcf4e1a4e318a5fca46a51c150ebbb" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "aa65df158ccd6f48a539f1b80e14ec7d594a122beeb1adcc573c09ee61d6a4ce" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "7baf806899411394e0a4b920cea8181243ec77d59730cd810dde32af457a26a7" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "e201ce622718d9a8251281952ed75a71c845a079bc9a86611171f9f17ba540a4" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "e0f735400e446b65bdf8c322dae7d4d0f9ac11cb3d48764aa8c0e3c62d2a9d39" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "562984c29af49e45200fb24a2147339040aee204a1e83dc6cfd97a68ee1fd2a5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "eecaa4f675adf9ad558e99e1da63509a4fab47e0a7773a3a5e3868fd595bfc7e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "aa6fa1e0be32e7e285b72b1673eeb548265fceb5c09d18dd8ce4ecfa0ac87456" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "f1c6df906b2a07c794f2b38d1594d89d5c2167acd8991ed24d3aca2f1defe13e" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "cf0b97891f9a97274a93977f99afb2db35191f5b4930f8f181dc0f7430ed393c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "66f4404c0f4a2793e69255b73db3f213cf558bdc473cc8c2ea88f64159288b9d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "6b67132fe1a43ce4ba18051c0e4ab55045a473ebbc97ab7b1ba3452090029f75" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "98ffffddfb964e74e85059479f358a9b25cd9525780be086f4893cad93824eb9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "f125b5483c342d50504d7d424f88a23a27280811129ec5cefec6cf4621803d36" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 145 :sha256 "07aef7b83293561bb15df86ce375971e05574fd2ccd092cca7503028575c4fda" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 147 :sha256 "619eb45195a5c9e9e8eff773abfe5a97fa72c58031ab04b6fa940d711114a63e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 149 :sha256 "9b85d006ac32b1f3c892dd4e0839a986dc541f41bfcbd47f2ab4e793c332a864" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 144 :sha256 "8684f14ce08df40a259887277829a9023b98ea0690e6dfd9a37fe2078688abd5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 146 :sha256 "6fddc8d12fc531022dd3ba81d144e326420db231d05fd32c720eedc2aaaecf78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 148 :sha256 "36286b51415e666edfc7263f0c769eefca308ac586d39d5dbf1afd0b74e7bc78" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "339415aa21c5a8667d4ecae276b3fbea3158284680f17f53368729ce6c4e70a2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "39f7b482afdcabe06fb350bd250ff4dbbdfc8991886e60df2f2c7ef049246f1d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "78f1db5b95e69e5ea9acd40c5a89233d3843de02a5b81aaff9b050c860843b70" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "411ae9b3e8a34c9ca4210ca34bb7d128d453346cf2fc17345f5418331dbb45c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "cb39227ac14eba75cbd20cdba5fc52132064d151b99def28edcae443486751bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "0da9af9c495f4210eea9d2357d8e6d6a98909a98091c09933d43e2a9cf4c19e1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "3b7d8b414c8c7b55bb02396ff96d8bdff16895c0696e78dbcb73056083ce0656" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "74b96c43d0fbef079d38c4de844c7b17abaa3c0d667ba3431097f70524d18a78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "a62a0eaf0a5340738eb86f550d6ef5844852926420b14949bd634836d6491b18" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "cbb64469b935460adf3f94c98862f85608d756a5f7d2b046589b032ae10791f4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "80afcc2df0c613129fa5e40fad85165964bbd21c0b455e1f25b410eabe8dff85" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "3a7d56cbc4397e0bdda5368f85271f8496e1e590c590c03de069576aefb3d9c2" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "bef5aa9efc2ffe83775488ee0d99fc89820a8483895175db5158e75d7160d8fe" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "f2f3591baab43f66049152c712cda236324c6548f2f6ca3e15a349ac8cae8179" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "b8c4292b9c44a23774173b14f6fa4b6f184f150744405eeef345bd5b450de6b5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "6f85e8552b39a255609c484beeeadc8c0f9a71ab8a6fd65053be47fb811a4e40" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "6e0c065108795c9a263cd47e0a1550faa1b4c552eca73d5df5eb6cd7fd5e4ca6" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "efffc1a93ee5eaca625d33167b0dc79bc007b5246f9820e24114b272d3a6edf5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "d55c7e20971b4d6beb67e41047da4d7770417661b9caf721ccd2e9a1ddb44ec2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c32dda777d1b8bdde9d34918227f19981c66659d42cc60bb408487513972df97" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "d76335e5b96a8438eb485cbf5bfbdd54e3765ef72d40efae2a283bb90fb608fd" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "c57f6b30164552178509fb095306bfbc92ad407b8e8540f76ae2f17f2cad4ba4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "b8645a664c08798f76b0661407100cc73124e6d3e19dcf2980af25409038f66b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "1ff302c2a14ba52a008018ebbc6a916e6bee1cfea831ca5fa274fa08e14777db" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "702fc6566b3d6104e146aadd6cac8c591fbfeda79eac6f77bd7b3f29993ab1f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "fa1ba9dda149d99875d757228ff53dfc4123178ee93cb4c381ddf0740c118a9f" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "021f0e9164ee2e5bc5ffb19e18f20989b575a98a8fbe55477d4ff7455e3de039" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "026021d7a06a2f58ec25f51313b9571736bf5350dff0484b959f3119e105ab2d" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "783c2371f57305bfead74b2496a6cd4ae51fe37c43ab62f4ebf9b325430e5474" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "4488a0895c2080cdf7ae1f9d9074c34e96fc21dd7cf88f5a5e359fb2d636ec0d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 147 :sha256 "698c9eca2575651fcb3632858b6e953b519ad22d93c37f2236801e8e1c48a1f5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 149 :sha256 "0edccb3598fb06c19979db40d788fdb54b50b20ecaffd380dbd12f193c85450d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 151 :sha256 "1e9b8549222e1b228c56410ea885b16bbc88f5ee9d5ae494c0fcaa9dc74d2aab" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 150 :sha256 "ee16bd081b2b9b727421fb89f13f87ac4d38f2061aef4329ee0820b5609e49e0" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 152 :sha256 "f24a6e18244103c1c1ab9353b94a88df54dc4710ba957ee296ebc377a92186bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 154 :sha256 "b0e8343c31f8e5ca6cc028d60cf2c860ec9d253926ed5dc92484fdf351ed5294" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "4dce71e003a9165c2655b955e16da8514fcde6bb9f0a239a707d29eec6624ff1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "1522fb1bc631a6250f7ff80392a7d6dc2529af98255b1c2b9cd194ab5fc0da04" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "aa7c23dac2619a9b759cec97d5f9633f85af83b5d5f273f84c93d635ba9e6e19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "11b6a8e5112ff9a6fbfcde376da6552ef02754a6e69f530e085146ba784e0c86" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "03c50b2fb3b14882d5968d66e7a1784c21ee99aa39c65ec545fab7dd09e56dcc" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "d963ebb621d5ee3c05834dffef1eecc31f354fe5783a4c3645dc56efe8b8e5ff" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "134022869d2827380a72dae9202ea0b73a307c1d0755ad30c7172413065b450a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "f19a85438d3e80ea3f1ca62fbcf3be442070aa992fd9fd833576f9305f04c213" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "d5d62d14f02b911c89e09df38a9a4d75d7dbc6a81d61dc0c3c9cd29040469194" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "82c6ac888c00fedb965252c4b00a87452bd24ec8beecd9436dff42ceb8810ab3" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "ca75fc6ba4fc5c77bce541e2ff87db73de96d7e6588db0e58fb419326963d79c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bf35d0a7d5395544baa981544d05ce89f34360a44f4f711464a116b0171f94fe" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "5f5e5f68f84e011d2480e7c77b0aa77b026ba705ff4a4caaaeb2f9aa1222caf1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "fb431a346ae340b1f2c2bfcafeb4d2b14bdea97a6bde5815205cc6e66249313c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "7ef2ba3f1cb07a2d58676821da520ae3fd99249ad5ec026877fdbabade738698" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "f0ed068b84aa06d3ac5a64a0028e8dc7dbf5830b5c932fccc2bc83376f485e49" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "6a52dfea32a5102c675d2efbe70a91b5ad136008bd7f0e9aa60fb81f0f37ca16" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "294a12baa83efe6da7a7ac376e4fdcb862d4530e3bb315be26612d1ea002354b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "8b5b5c2d8cc0f861f9a864aeb945287b67c1cf8323cb85c004cb36d5e803fe12" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "17bc182e8ac22809fef7f186c08ebda7ff944002a55d6bb50d20dd13a89220c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "69066b8289e73b604cc00941fb46a40f957ceb6f6fbfbccfdd103b7dfd3a94e5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "fccd90da70d4fc444c9ef9b05c3d36a22de2229614ad4581fb37d26942711bf9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c0ac537a4cf79afaa9326adcdbeb23a408521beb502969ea03dff92326a9567b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "fa58bfd55c33a0983ed56673b9587cd3aca45c545a470eec3036e0cfa9d84df9" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "6ed2c0ff95902a8f3dae5389955c4004780c8b851a3135352de9ff2864438a84" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "5410a829b69b857f60cd15bb98a8e28d1ab8e6bad8f9f0ab8f55cc891c985fad" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "cd5d52eaf3fae61760c116ef2b2141ca24fef5e98f401cec1dae233bf8324ca1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "bde98858506fea1b66db5da953e8666005cbfbd82beda49ba06527c9cf982681" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "d10f30172843a7b601e8b74dc39d58567ee91afd6be176b3749cf39e40832623" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "0efbea94078a2d2f47d500fc3e99f6bab6e6d9dcb9a1d0ba02e18928af222ade" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "7c8d6f364dbf5521b557758efcc33268075be62ba75f445a547c3e1c3a990fc8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "950918fb34f714331d8c3bd601c3fcf8635cb3ae253ebdbbbfa7ad499f863dd8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "3821dc1f990b4940f975f92a05f667b7e4b79422fda76089bf9e190615e72329" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "c1083b115ce49254f50d208ec4873936246265914c4f5d106d6fffa30ce2e0d5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "50be04c4d96ea007405fc0cffd6d27a612bb5f93b420bad0832b117b4b0d3edd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "6343b295d7c132b22722daeec797c6ca4d3212e36d8a57218cc661e1db1ef69a" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "6a77b85c2ee24a68bc73d465f1d7a8cac5cec84dc06df48bbe3541ed18309796" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "0ba715d3a7e2f64438a2ed2a6f0d375ddd5b54b393c8060111bb887bdaaf372d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "f27772e376cd43f26ca3db5f11a7e77994a3d3582f0055a81696607052aa3d57" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "c9013dcbbe4aa814624d5d0c11722232781823f5584e4c1ca4c6309dbd7d49d9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "58cafca7078cb984bf6aa9f77f69e1d18fafa37e777b46fbb8e430a60721f1b8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bffe5914312bfe60fe368109c17dfd66c6c128f5fca00bdbdf02e2dfb0902062" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "15a794ebc412b7228b65a74780aa6a53100bebbc3eed8163834d41b99a87d8ec" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "cedb634a1b813e7cc2c7adc52d0f735a4d851ed342085806d9119ff53baf0183" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "dc1b84ce4e841f5a9498458f03ffa39efe9da12ac7f2b8144d11109a29bdf4f6" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "95329897239ada0e9cf954c623b974b2eacaea240ab94260c502bae44f55fb21" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "304e64667d7f05e69fae8eda9607589018ed5dfe4641efa4d1d0b14235e75b6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "58f4504eac619975e45b31e22c359e2af5cbb343461dea026e9cb34989131685" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "7c355a826715400c85ddd60dd7876dbcbc135f808fb9a7df56f47eff7e9ccb7f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "cfed0892af48042ba3b9b1e431b2c360b08802db76038a427c5441ea2ebc32bf" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "32d437e125d9e70fe6a96a2345ce177367638904e54e3bb63d460f362c5d011d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "f78b7e797ee5540bed6ad5ab5b1a8a1fc903ce3e2962644399b52bf83050ca8b" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "c28ba1e91308de706d08e6e1c5273fc144d8015d2d800395b42fa7d918083221" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "ae0a8643947f9f716bbc58c06f4461c49b77b625aad448b80fb962b27bb6fd7b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "e5abade1878104a270a89467ee182cf8a127432043c2eaa4498f51aa64162ffb" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "ac0bea20f2843c8698207d6407299c672a174865e14d95f32be3c81c52debc4c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "5928391fa6991fc8b0f03d1a7a2f4f8247a2bf6fb3cdcf8f75dc50e26cdb2f0c" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "02e0003566e95a4f392f8efd8e8181a4f6524b20ed3432d2d7846b16f2705582" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "30531903f7e94224a64e5dda8fe1b69f0f573940025565bf03c1044d258630ce" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "e571f81b0ef5c0a43155b0593c30deff3626c4b32db9327e8996779573af3a23" :delivery whole-frame) (:kind request-completed :owner (:request-completed 7) :bytes 101 :sha256 "8699d41b87f14dc9357daab15047c26f6acbae3d0fca0f9cb484f24138910e23" :delivery whole-frame))) :json "{\"command\":\"geterrForProject\",\"seq\":\"7\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"delay\":0}}")) :termination (:client-killed :ready-after 7)) (:first-ordinal 8 :requests (open configure projectInfo geterrForProject reload syntacticDiagnosticsSync semanticDiagnosticsSync projectInfo geterrForProject) :request-count 9 :frame-count 307 :request-sha256 "f189d280cbaab1a1dd35df4f37455bbb3cb7fe74630cea129e8fed243fef47a8" :recordings ((:ordinal 8 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"8\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 9 :outcome complete :callback not-registered :output (:delivery-after 10 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "4756d2e42fe3af5e87073a2deda7465c34fcfd166cd81d9efe03b90675a089f6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc7819912d084fd5dfd90594c093b6992e07d3969a1cb983e305c4678dcad67a" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 674 :sha256 "497a93ed17bca7ae839f23742ca433de652057dfb66f611649a358f251164786" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "df13b5a220de4a8fd0277ca38977f740140ae00e7e81ced93645d40fbc8e3e42" :delivery whole-frame) (:kind response :owner (:response 9 "configure") :bytes 105 :sha256 "bdab086215bde42b8ae9df5a66ed7969025a4cb93662d94a82df02367cf4eed8" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"9\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 10 :outcome complete :callback registered :output (:delivery-after 10 :frames ((:kind response :owner (:response 10 "projectInfo") :bytes 2151 :sha256 "07d66bdba17106f1391a39c122bcb488aa802a5db8bab0d40185a5dc3ec9f373" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"10\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":true}}") (:ordinal 11 :outcome complete :callback not-registered :output (:delivery-after 11 :frames ((:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "0d4e111326bc3af4ba39376b68db9938c14db1190a3138e7f06bdaed4e7cd9e8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 276 :sha256 "8672b7515433258165c816cfa0593fa5cfdd670ecd2b2be581eb217a69afdce1" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 310 :sha256 "2689d489fceb74f419d6d398969c0e294466cd4b50ceb6364e271bf4f9576d06" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "413cd61038697a6689a85d072a64db33da53a8e5a7635fce14c02eb757207ab1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 125 :sha256 "bcaa4852453a930845157158366499235093602124f0caeba33d4e3c80f3e6c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 127 :sha256 "5f61beb2584b4a6f66e56ef8f7843db254c6f81251b7c5a8f06596eab4661116" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "8ea0300969791be8aa46713870c8f665b60a7c2107b3f225956ebc554ae3a5f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "cb901ae31554d8a48f8045f5471dd107c427523a297465170f0fd8fcb8196310" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 319 :sha256 "385c37c81a9dcd9a6b9026e94ae99e2165ab28f2d20915f4be94896b665c1012" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "5f099d206bcdfc6af6d63b19fe21fc4ab295d1fe0b3a1efdfd05407b38c31d3f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "79c589fd428d536ebcdec9200fbceb1ec426bd1ea0c42c17846034cade93c4ae" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "333102224e6f0ada69c88b377dd308df45c6e7bb2ad55da484eed51b95c47c19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "742b4fee0a971ec7f66956e7164c1ec393469e9a4f4299837fc9ef51f1388632" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "f1d8476e053ded043c55d53e5ac0797fe9dbe18db5ff44530b029fe09915f5f9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "2087a6ee198e1535792edc0614182fec66b8e1d7b6358c34bf7d223ebca19b73" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53c6870e8afbd8c209afb07e845c4b2b48c7fe60c38223fe68f38b07ee06a48a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "71001654875e3f1fe0dd7b91b636982f0c5204f2a499a9e67c7f456ece3e8345" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "74fb3f40f217509ff51d13a493507d41cfa147a5a1add72fd6883e6c5e17752f" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53a49af37906f007c41911be3eba08ec5c1bddd859cbc4bf551f6bed711d0b7a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "58125398906518cac469fbc0879b34bb7a3f2bb83fcf6505e23bd2defb0cb2ba" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "a8157b81a021b42e7dbf1870b465d3fa1039bb2c369dcedc9ae38a12267a86f1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "6f50d058a56f4363da12c236d933e47d28e33940f4437b0105bc5243a87f2d28" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "daccc5a166d678c297dfa8cabdd54f98ba2708e323a1028bd5779b1de20bc281" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "143caff58ca6360cc2f43407a7044382fb750c9bf4087e44b6f284b636d76d22" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "d466e6adf142362dbb9df8c58c2e330c4abc581421db92d47b7ff3de906f65ac" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "dda38924dd2575bd41ce148fa0c38b6bc91e1e733cdc0625d1e1097bd07b7d6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "aabc9a0c1d34a662675838cd82585d92a6db79a916b9ad612545d8a3c2ea9a45" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "117416684cfd553d7a4f424020f9ae968af3d991f4bf455a6fdb7feff1214c78" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "5366023b1a3ea85a2ba8d79275f3e73b3694aea1f4703e2b09101a6bedebb37c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 33309 :sha256 "07684dabccedf0b6c9138d647b4aff504b4170f8e8e6e9d7ce67e91c25707e86" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "1437b88bac23774ea649f4c1210476a6071e6e83573c63cffa975f48941067c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "377f4d2ad40e4d52866fec4b376c49fa3adcf4e1a4e318a5fca46a51c150ebbb" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 1310 :sha256 "7d6ffc9f0fef49fee4895e46bbda4bbae85f0e72b5baa17ac12dea931e457c85" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "7baf806899411394e0a4b920cea8181243ec77d59730cd810dde32af457a26a7" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "e201ce622718d9a8251281952ed75a71c845a079bc9a86611171f9f17ba540a4" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "e0f735400e446b65bdf8c322dae7d4d0f9ac11cb3d48764aa8c0e3c62d2a9d39" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "562984c29af49e45200fb24a2147339040aee204a1e83dc6cfd97a68ee1fd2a5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "eecaa4f675adf9ad558e99e1da63509a4fab47e0a7773a3a5e3868fd595bfc7e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "aa6fa1e0be32e7e285b72b1673eeb548265fceb5c09d18dd8ce4ecfa0ac87456" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "f1c6df906b2a07c794f2b38d1594d89d5c2167acd8991ed24d3aca2f1defe13e" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "cf0b97891f9a97274a93977f99afb2db35191f5b4930f8f181dc0f7430ed393c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "66f4404c0f4a2793e69255b73db3f213cf558bdc473cc8c2ea88f64159288b9d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "6b67132fe1a43ce4ba18051c0e4ab55045a473ebbc97ab7b1ba3452090029f75" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "98ffffddfb964e74e85059479f358a9b25cd9525780be086f4893cad93824eb9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "f125b5483c342d50504d7d424f88a23a27280811129ec5cefec6cf4621803d36" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 145 :sha256 "07aef7b83293561bb15df86ce375971e05574fd2ccd092cca7503028575c4fda" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 147 :sha256 "619eb45195a5c9e9e8eff773abfe5a97fa72c58031ab04b6fa940d711114a63e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 149 :sha256 "9b85d006ac32b1f3c892dd4e0839a986dc541f41bfcbd47f2ab4e793c332a864" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 144 :sha256 "8684f14ce08df40a259887277829a9023b98ea0690e6dfd9a37fe2078688abd5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 146 :sha256 "6fddc8d12fc531022dd3ba81d144e326420db231d05fd32c720eedc2aaaecf78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 148 :sha256 "36286b51415e666edfc7263f0c769eefca308ac586d39d5dbf1afd0b74e7bc78" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "339415aa21c5a8667d4ecae276b3fbea3158284680f17f53368729ce6c4e70a2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "39f7b482afdcabe06fb350bd250ff4dbbdfc8991886e60df2f2c7ef049246f1d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "78f1db5b95e69e5ea9acd40c5a89233d3843de02a5b81aaff9b050c860843b70" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "411ae9b3e8a34c9ca4210ca34bb7d128d453346cf2fc17345f5418331dbb45c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "cb39227ac14eba75cbd20cdba5fc52132064d151b99def28edcae443486751bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "0da9af9c495f4210eea9d2357d8e6d6a98909a98091c09933d43e2a9cf4c19e1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "3b7d8b414c8c7b55bb02396ff96d8bdff16895c0696e78dbcb73056083ce0656" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "74b96c43d0fbef079d38c4de844c7b17abaa3c0d667ba3431097f70524d18a78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "a62a0eaf0a5340738eb86f550d6ef5844852926420b14949bd634836d6491b18" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "cbb64469b935460adf3f94c98862f85608d756a5f7d2b046589b032ae10791f4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "80afcc2df0c613129fa5e40fad85165964bbd21c0b455e1f25b410eabe8dff85" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "3a7d56cbc4397e0bdda5368f85271f8496e1e590c590c03de069576aefb3d9c2" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "bef5aa9efc2ffe83775488ee0d99fc89820a8483895175db5158e75d7160d8fe" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "f2f3591baab43f66049152c712cda236324c6548f2f6ca3e15a349ac8cae8179" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "b8c4292b9c44a23774173b14f6fa4b6f184f150744405eeef345bd5b450de6b5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "6f85e8552b39a255609c484beeeadc8c0f9a71ab8a6fd65053be47fb811a4e40" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "6e0c065108795c9a263cd47e0a1550faa1b4c552eca73d5df5eb6cd7fd5e4ca6" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "efffc1a93ee5eaca625d33167b0dc79bc007b5246f9820e24114b272d3a6edf5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "d55c7e20971b4d6beb67e41047da4d7770417661b9caf721ccd2e9a1ddb44ec2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c32dda777d1b8bdde9d34918227f19981c66659d42cc60bb408487513972df97" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "d76335e5b96a8438eb485cbf5bfbdd54e3765ef72d40efae2a283bb90fb608fd" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "c57f6b30164552178509fb095306bfbc92ad407b8e8540f76ae2f17f2cad4ba4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "b8645a664c08798f76b0661407100cc73124e6d3e19dcf2980af25409038f66b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "1ff302c2a14ba52a008018ebbc6a916e6bee1cfea831ca5fa274fa08e14777db" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "702fc6566b3d6104e146aadd6cac8c591fbfeda79eac6f77bd7b3f29993ab1f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "fa1ba9dda149d99875d757228ff53dfc4123178ee93cb4c381ddf0740c118a9f" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "021f0e9164ee2e5bc5ffb19e18f20989b575a98a8fbe55477d4ff7455e3de039" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "026021d7a06a2f58ec25f51313b9571736bf5350dff0484b959f3119e105ab2d" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "783c2371f57305bfead74b2496a6cd4ae51fe37c43ab62f4ebf9b325430e5474" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "4488a0895c2080cdf7ae1f9d9074c34e96fc21dd7cf88f5a5e359fb2d636ec0d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 147 :sha256 "698c9eca2575651fcb3632858b6e953b519ad22d93c37f2236801e8e1c48a1f5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 149 :sha256 "0edccb3598fb06c19979db40d788fdb54b50b20ecaffd380dbd12f193c85450d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 151 :sha256 "1e9b8549222e1b228c56410ea885b16bbc88f5ee9d5ae494c0fcaa9dc74d2aab" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 150 :sha256 "ee16bd081b2b9b727421fb89f13f87ac4d38f2061aef4329ee0820b5609e49e0" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 152 :sha256 "f24a6e18244103c1c1ab9353b94a88df54dc4710ba957ee296ebc377a92186bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 154 :sha256 "b0e8343c31f8e5ca6cc028d60cf2c860ec9d253926ed5dc92484fdf351ed5294" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "4dce71e003a9165c2655b955e16da8514fcde6bb9f0a239a707d29eec6624ff1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "1522fb1bc631a6250f7ff80392a7d6dc2529af98255b1c2b9cd194ab5fc0da04" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "aa7c23dac2619a9b759cec97d5f9633f85af83b5d5f273f84c93d635ba9e6e19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "11b6a8e5112ff9a6fbfcde376da6552ef02754a6e69f530e085146ba784e0c86" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "03c50b2fb3b14882d5968d66e7a1784c21ee99aa39c65ec545fab7dd09e56dcc" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "d963ebb621d5ee3c05834dffef1eecc31f354fe5783a4c3645dc56efe8b8e5ff" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "134022869d2827380a72dae9202ea0b73a307c1d0755ad30c7172413065b450a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "f19a85438d3e80ea3f1ca62fbcf3be442070aa992fd9fd833576f9305f04c213" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "d5d62d14f02b911c89e09df38a9a4d75d7dbc6a81d61dc0c3c9cd29040469194" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "82c6ac888c00fedb965252c4b00a87452bd24ec8beecd9436dff42ceb8810ab3" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "ca75fc6ba4fc5c77bce541e2ff87db73de96d7e6588db0e58fb419326963d79c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bf35d0a7d5395544baa981544d05ce89f34360a44f4f711464a116b0171f94fe" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "5f5e5f68f84e011d2480e7c77b0aa77b026ba705ff4a4caaaeb2f9aa1222caf1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "fb431a346ae340b1f2c2bfcafeb4d2b14bdea97a6bde5815205cc6e66249313c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "7ef2ba3f1cb07a2d58676821da520ae3fd99249ad5ec026877fdbabade738698" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "f0ed068b84aa06d3ac5a64a0028e8dc7dbf5830b5c932fccc2bc83376f485e49" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "6a52dfea32a5102c675d2efbe70a91b5ad136008bd7f0e9aa60fb81f0f37ca16" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "294a12baa83efe6da7a7ac376e4fdcb862d4530e3bb315be26612d1ea002354b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "8b5b5c2d8cc0f861f9a864aeb945287b67c1cf8323cb85c004cb36d5e803fe12" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "17bc182e8ac22809fef7f186c08ebda7ff944002a55d6bb50d20dd13a89220c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "69066b8289e73b604cc00941fb46a40f957ceb6f6fbfbccfdd103b7dfd3a94e5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "fccd90da70d4fc444c9ef9b05c3d36a22de2229614ad4581fb37d26942711bf9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c0ac537a4cf79afaa9326adcdbeb23a408521beb502969ea03dff92326a9567b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "fa58bfd55c33a0983ed56673b9587cd3aca45c545a470eec3036e0cfa9d84df9" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "6ed2c0ff95902a8f3dae5389955c4004780c8b851a3135352de9ff2864438a84" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "5410a829b69b857f60cd15bb98a8e28d1ab8e6bad8f9f0ab8f55cc891c985fad" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "cd5d52eaf3fae61760c116ef2b2141ca24fef5e98f401cec1dae233bf8324ca1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "bde98858506fea1b66db5da953e8666005cbfbd82beda49ba06527c9cf982681" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "d10f30172843a7b601e8b74dc39d58567ee91afd6be176b3749cf39e40832623" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "0efbea94078a2d2f47d500fc3e99f6bab6e6d9dcb9a1d0ba02e18928af222ade" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "7c8d6f364dbf5521b557758efcc33268075be62ba75f445a547c3e1c3a990fc8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "950918fb34f714331d8c3bd601c3fcf8635cb3ae253ebdbbbfa7ad499f863dd8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "3821dc1f990b4940f975f92a05f667b7e4b79422fda76089bf9e190615e72329" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "c1083b115ce49254f50d208ec4873936246265914c4f5d106d6fffa30ce2e0d5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "50be04c4d96ea007405fc0cffd6d27a612bb5f93b420bad0832b117b4b0d3edd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "6343b295d7c132b22722daeec797c6ca4d3212e36d8a57218cc661e1db1ef69a" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "6a77b85c2ee24a68bc73d465f1d7a8cac5cec84dc06df48bbe3541ed18309796" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "0ba715d3a7e2f64438a2ed2a6f0d375ddd5b54b393c8060111bb887bdaaf372d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "f27772e376cd43f26ca3db5f11a7e77994a3d3582f0055a81696607052aa3d57" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "c9013dcbbe4aa814624d5d0c11722232781823f5584e4c1ca4c6309dbd7d49d9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "58cafca7078cb984bf6aa9f77f69e1d18fafa37e777b46fbb8e430a60721f1b8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bffe5914312bfe60fe368109c17dfd66c6c128f5fca00bdbdf02e2dfb0902062" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "15a794ebc412b7228b65a74780aa6a53100bebbc3eed8163834d41b99a87d8ec" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "cedb634a1b813e7cc2c7adc52d0f735a4d851ed342085806d9119ff53baf0183" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "dc1b84ce4e841f5a9498458f03ffa39efe9da12ac7f2b8144d11109a29bdf4f6" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "95329897239ada0e9cf954c623b974b2eacaea240ab94260c502bae44f55fb21" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "304e64667d7f05e69fae8eda9607589018ed5dfe4641efa4d1d0b14235e75b6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "58f4504eac619975e45b31e22c359e2af5cbb343461dea026e9cb34989131685" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "7c355a826715400c85ddd60dd7876dbcbc135f808fb9a7df56f47eff7e9ccb7f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "cfed0892af48042ba3b9b1e431b2c360b08802db76038a427c5441ea2ebc32bf" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "32d437e125d9e70fe6a96a2345ce177367638904e54e3bb63d460f362c5d011d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "f78b7e797ee5540bed6ad5ab5b1a8a1fc903ce3e2962644399b52bf83050ca8b" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "c28ba1e91308de706d08e6e1c5273fc144d8015d2d800395b42fa7d918083221" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "ae0a8643947f9f716bbc58c06f4461c49b77b625aad448b80fb962b27bb6fd7b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "e5abade1878104a270a89467ee182cf8a127432043c2eaa4498f51aa64162ffb" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "ac0bea20f2843c8698207d6407299c672a174865e14d95f32be3c81c52debc4c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "5928391fa6991fc8b0f03d1a7a2f4f8247a2bf6fb3cdcf8f75dc50e26cdb2f0c" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "02e0003566e95a4f392f8efd8e8181a4f6524b20ed3432d2d7846b16f2705582" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "30531903f7e94224a64e5dda8fe1b69f0f573940025565bf03c1044d258630ce" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "e571f81b0ef5c0a43155b0593c30deff3626c4b32db9327e8996779573af3a23" :delivery whole-frame) (:kind request-completed :owner (:request-completed 11) :bytes 102 :sha256 "fc802e6a0fd7859925c95af54d33ffd5e6e99e6022af2c54b95a290e8d57215c" :delivery whole-frame))) :json "{\"command\":\"geterrForProject\",\"seq\":\"11\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"delay\":0}}") (:ordinal 12 :outcome complete :callback not-registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 12 "reload") :bytes 103 :sha256 "ee751cf2ae0aaa9f6199609ee8da9a658211a5949193eaa2ae12f730cd9786b6" :delivery whole-frame) (:kind response :owner (:response 12 "reload") :bytes 135 :sha256 "bb19385c2fc495faf748a92db9b21eeafcf5a08b1a86444289c3016934c92e07" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"12\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 13 :outcome complete :callback registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 13 "syntacticDiagnosticsSync") :bytes 194 :sha256 "c171c3917893e99b5efbf629aee93c5c1030e5a4f50a6e9539ce29102e93dfcc" :delivery whole-frame))) :json "{\"command\":\"syntacticDiagnosticsSync\",\"seq\":\"13\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 14 :outcome complete :callback registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 14 "semanticDiagnosticsSync") :bytes 131 :sha256 "c62de79e9a65eec02f0adf1fe5aa5f8278a1f4f00915f1f13fd7e04443110819" :delivery whole-frame))) :json "{\"command\":\"semanticDiagnosticsSync\",\"seq\":\"14\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\"}}") (:ordinal 15 :outcome complete :callback registered :output (:delivery-after 15 :frames ((:kind response :owner (:response 15 "projectInfo") :bytes 2151 :sha256 "b72973b77bfbd58e9db320d724c8e44c7a0c20aab0686051bddd3f4ce8ca81b4" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"15\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":true}}") (:ordinal 16 :outcome complete :callback not-registered :output (:delivery-after 16 :frames ((:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "0d4e111326bc3af4ba39376b68db9938c14db1190a3138e7f06bdaed4e7cd9e8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 125 :sha256 "5b4a7cca9ce256510c5e3c08803eae215b46bf01de2db0d0c5529cc19e547b99" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 127 :sha256 "8195c32e46e3529d0aafd5fdca6389d1c82731382ab9a581dd5453be8b3d2964" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 123 :sha256 "413cd61038697a6689a85d072a64db33da53a8e5a7635fce14c02eb757207ab1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 125 :sha256 "bcaa4852453a930845157158366499235093602124f0caeba33d4e3c80f3e6c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 127 :sha256 "5f61beb2584b4a6f66e56ef8f7843db254c6f81251b7c5a8f06596eab4661116" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "8ea0300969791be8aa46713870c8f665b60a7c2107b3f225956ebc554ae3a5f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "cb901ae31554d8a48f8045f5471dd107c427523a297465170f0fd8fcb8196310" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 319 :sha256 "385c37c81a9dcd9a6b9026e94ae99e2165ab28f2d20915f4be94896b665c1012" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "5f099d206bcdfc6af6d63b19fe21fc4ab295d1fe0b3a1efdfd05407b38c31d3f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "79c589fd428d536ebcdec9200fbceb1ec426bd1ea0c42c17846034cade93c4ae" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "333102224e6f0ada69c88b377dd308df45c6e7bb2ad55da484eed51b95c47c19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "742b4fee0a971ec7f66956e7164c1ec393469e9a4f4299837fc9ef51f1388632" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "f1d8476e053ded043c55d53e5ac0797fe9dbe18db5ff44530b029fe09915f5f9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "2087a6ee198e1535792edc0614182fec66b8e1d7b6358c34bf7d223ebca19b73" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53c6870e8afbd8c209afb07e845c4b2b48c7fe60c38223fe68f38b07ee06a48a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "71001654875e3f1fe0dd7b91b636982f0c5204f2a499a9e67c7f456ece3e8345" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "74fb3f40f217509ff51d13a493507d41cfa147a5a1add72fd6883e6c5e17752f" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "53a49af37906f007c41911be3eba08ec5c1bddd859cbc4bf551f6bed711d0b7a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "58125398906518cac469fbc0879b34bb7a3f2bb83fcf6505e23bd2defb0cb2ba" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "a8157b81a021b42e7dbf1870b465d3fa1039bb2c369dcedc9ae38a12267a86f1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "6f50d058a56f4363da12c236d933e47d28e33940f4437b0105bc5243a87f2d28" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "daccc5a166d678c297dfa8cabdd54f98ba2708e323a1028bd5779b1de20bc281" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "143caff58ca6360cc2f43407a7044382fb750c9bf4087e44b6f284b636d76d22" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 135 :sha256 "d466e6adf142362dbb9df8c58c2e330c4abc581421db92d47b7ff3de906f65ac" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 137 :sha256 "dda38924dd2575bd41ce148fa0c38b6bc91e1e733cdc0625d1e1097bd07b7d6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 139 :sha256 "aabc9a0c1d34a662675838cd82585d92a6db79a916b9ad612545d8a3c2ea9a45" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 132 :sha256 "117416684cfd553d7a4f424020f9ae968af3d991f4bf455a6fdb7feff1214c78" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 134 :sha256 "5366023b1a3ea85a2ba8d79275f3e73b3694aea1f4703e2b09101a6bedebb37c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 33309 :sha256 "07684dabccedf0b6c9138d647b4aff504b4170f8e8e6e9d7ce67e91c25707e86" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "1437b88bac23774ea649f4c1210476a6071e6e83573c63cffa975f48941067c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "377f4d2ad40e4d52866fec4b376c49fa3adcf4e1a4e318a5fca46a51c150ebbb" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 1310 :sha256 "7d6ffc9f0fef49fee4895e46bbda4bbae85f0e72b5baa17ac12dea931e457c85" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "7baf806899411394e0a4b920cea8181243ec77d59730cd810dde32af457a26a7" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "e201ce622718d9a8251281952ed75a71c845a079bc9a86611171f9f17ba540a4" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "e0f735400e446b65bdf8c322dae7d4d0f9ac11cb3d48764aa8c0e3c62d2a9d39" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "562984c29af49e45200fb24a2147339040aee204a1e83dc6cfd97a68ee1fd2a5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "eecaa4f675adf9ad558e99e1da63509a4fab47e0a7773a3a5e3868fd595bfc7e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "aa6fa1e0be32e7e285b72b1673eeb548265fceb5c09d18dd8ce4ecfa0ac87456" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "f1c6df906b2a07c794f2b38d1594d89d5c2167acd8991ed24d3aca2f1defe13e" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "cf0b97891f9a97274a93977f99afb2db35191f5b4930f8f181dc0f7430ed393c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "66f4404c0f4a2793e69255b73db3f213cf558bdc473cc8c2ea88f64159288b9d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "6b67132fe1a43ce4ba18051c0e4ab55045a473ebbc97ab7b1ba3452090029f75" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "98ffffddfb964e74e85059479f358a9b25cd9525780be086f4893cad93824eb9" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "f125b5483c342d50504d7d424f88a23a27280811129ec5cefec6cf4621803d36" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 145 :sha256 "07aef7b83293561bb15df86ce375971e05574fd2ccd092cca7503028575c4fda" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 147 :sha256 "619eb45195a5c9e9e8eff773abfe5a97fa72c58031ab04b6fa940d711114a63e" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 149 :sha256 "9b85d006ac32b1f3c892dd4e0839a986dc541f41bfcbd47f2ab4e793c332a864" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 144 :sha256 "8684f14ce08df40a259887277829a9023b98ea0690e6dfd9a37fe2078688abd5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 146 :sha256 "6fddc8d12fc531022dd3ba81d144e326420db231d05fd32c720eedc2aaaecf78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 148 :sha256 "36286b51415e666edfc7263f0c769eefca308ac586d39d5dbf1afd0b74e7bc78" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "339415aa21c5a8667d4ecae276b3fbea3158284680f17f53368729ce6c4e70a2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "39f7b482afdcabe06fb350bd250ff4dbbdfc8991886e60df2f2c7ef049246f1d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "78f1db5b95e69e5ea9acd40c5a89233d3843de02a5b81aaff9b050c860843b70" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "411ae9b3e8a34c9ca4210ca34bb7d128d453346cf2fc17345f5418331dbb45c9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "cb39227ac14eba75cbd20cdba5fc52132064d151b99def28edcae443486751bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "0da9af9c495f4210eea9d2357d8e6d6a98909a98091c09933d43e2a9cf4c19e1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "3b7d8b414c8c7b55bb02396ff96d8bdff16895c0696e78dbcb73056083ce0656" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "74b96c43d0fbef079d38c4de844c7b17abaa3c0d667ba3431097f70524d18a78" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "a62a0eaf0a5340738eb86f550d6ef5844852926420b14949bd634836d6491b18" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "cbb64469b935460adf3f94c98862f85608d756a5f7d2b046589b032ae10791f4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "80afcc2df0c613129fa5e40fad85165964bbd21c0b455e1f25b410eabe8dff85" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "3a7d56cbc4397e0bdda5368f85271f8496e1e590c590c03de069576aefb3d9c2" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "bef5aa9efc2ffe83775488ee0d99fc89820a8483895175db5158e75d7160d8fe" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "f2f3591baab43f66049152c712cda236324c6548f2f6ca3e15a349ac8cae8179" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "b8c4292b9c44a23774173b14f6fa4b6f184f150744405eeef345bd5b450de6b5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "6f85e8552b39a255609c484beeeadc8c0f9a71ab8a6fd65053be47fb811a4e40" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "6e0c065108795c9a263cd47e0a1550faa1b4c552eca73d5df5eb6cd7fd5e4ca6" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "efffc1a93ee5eaca625d33167b0dc79bc007b5246f9820e24114b272d3a6edf5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "d55c7e20971b4d6beb67e41047da4d7770417661b9caf721ccd2e9a1ddb44ec2" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c32dda777d1b8bdde9d34918227f19981c66659d42cc60bb408487513972df97" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "d76335e5b96a8438eb485cbf5bfbdd54e3765ef72d40efae2a283bb90fb608fd" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "c57f6b30164552178509fb095306bfbc92ad407b8e8540f76ae2f17f2cad4ba4" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "b8645a664c08798f76b0661407100cc73124e6d3e19dcf2980af25409038f66b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "1ff302c2a14ba52a008018ebbc6a916e6bee1cfea831ca5fa274fa08e14777db" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "702fc6566b3d6104e146aadd6cac8c591fbfeda79eac6f77bd7b3f29993ab1f8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "fa1ba9dda149d99875d757228ff53dfc4123178ee93cb4c381ddf0740c118a9f" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "021f0e9164ee2e5bc5ffb19e18f20989b575a98a8fbe55477d4ff7455e3de039" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "026021d7a06a2f58ec25f51313b9571736bf5350dff0484b959f3119e105ab2d" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "783c2371f57305bfead74b2496a6cd4ae51fe37c43ab62f4ebf9b325430e5474" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "4488a0895c2080cdf7ae1f9d9074c34e96fc21dd7cf88f5a5e359fb2d636ec0d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 147 :sha256 "698c9eca2575651fcb3632858b6e953b519ad22d93c37f2236801e8e1c48a1f5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 149 :sha256 "0edccb3598fb06c19979db40d788fdb54b50b20ecaffd380dbd12f193c85450d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 151 :sha256 "1e9b8549222e1b228c56410ea885b16bbc88f5ee9d5ae494c0fcaa9dc74d2aab" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 150 :sha256 "ee16bd081b2b9b727421fb89f13f87ac4d38f2061aef4329ee0820b5609e49e0" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 152 :sha256 "f24a6e18244103c1c1ab9353b94a88df54dc4710ba957ee296ebc377a92186bd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 154 :sha256 "b0e8343c31f8e5ca6cc028d60cf2c860ec9d253926ed5dc92484fdf351ed5294" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 149 :sha256 "4dce71e003a9165c2655b955e16da8514fcde6bb9f0a239a707d29eec6624ff1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 151 :sha256 "1522fb1bc631a6250f7ff80392a7d6dc2529af98255b1c2b9cd194ab5fc0da04" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 153 :sha256 "aa7c23dac2619a9b759cec97d5f9633f85af83b5d5f273f84c93d635ba9e6e19" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "11b6a8e5112ff9a6fbfcde376da6552ef02754a6e69f530e085146ba784e0c86" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "03c50b2fb3b14882d5968d66e7a1784c21ee99aa39c65ec545fab7dd09e56dcc" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "d963ebb621d5ee3c05834dffef1eecc31f354fe5783a4c3645dc56efe8b8e5ff" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "134022869d2827380a72dae9202ea0b73a307c1d0755ad30c7172413065b450a" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "f19a85438d3e80ea3f1ca62fbcf3be442070aa992fd9fd833576f9305f04c213" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "d5d62d14f02b911c89e09df38a9a4d75d7dbc6a81d61dc0c3c9cd29040469194" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "82c6ac888c00fedb965252c4b00a87452bd24ec8beecd9436dff42ceb8810ab3" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "ca75fc6ba4fc5c77bce541e2ff87db73de96d7e6588db0e58fb419326963d79c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bf35d0a7d5395544baa981544d05ce89f34360a44f4f711464a116b0171f94fe" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 141 :sha256 "5f5e5f68f84e011d2480e7c77b0aa77b026ba705ff4a4caaaeb2f9aa1222caf1" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 143 :sha256 "fb431a346ae340b1f2c2bfcafeb4d2b14bdea97a6bde5815205cc6e66249313c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 145 :sha256 "7ef2ba3f1cb07a2d58676821da520ae3fd99249ad5ec026877fdbabade738698" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "f0ed068b84aa06d3ac5a64a0028e8dc7dbf5830b5c932fccc2bc83376f485e49" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "6a52dfea32a5102c675d2efbe70a91b5ad136008bd7f0e9aa60fb81f0f37ca16" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "294a12baa83efe6da7a7ac376e4fdcb862d4530e3bb315be26612d1ea002354b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "8b5b5c2d8cc0f861f9a864aeb945287b67c1cf8323cb85c004cb36d5e803fe12" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "17bc182e8ac22809fef7f186c08ebda7ff944002a55d6bb50d20dd13a89220c2" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "69066b8289e73b604cc00941fb46a40f957ceb6f6fbfbccfdd103b7dfd3a94e5" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "fccd90da70d4fc444c9ef9b05c3d36a22de2229614ad4581fb37d26942711bf9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "c0ac537a4cf79afaa9326adcdbeb23a408521beb502969ea03dff92326a9567b" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "fa58bfd55c33a0983ed56673b9587cd3aca45c545a470eec3036e0cfa9d84df9" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "6ed2c0ff95902a8f3dae5389955c4004780c8b851a3135352de9ff2864438a84" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "5410a829b69b857f60cd15bb98a8e28d1ab8e6bad8f9f0ab8f55cc891c985fad" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "cd5d52eaf3fae61760c116ef2b2141ca24fef5e98f401cec1dae233bf8324ca1" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "bde98858506fea1b66db5da953e8666005cbfbd82beda49ba06527c9cf982681" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "d10f30172843a7b601e8b74dc39d58567ee91afd6be176b3749cf39e40832623" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "0efbea94078a2d2f47d500fc3e99f6bab6e6d9dcb9a1d0ba02e18928af222ade" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "7c8d6f364dbf5521b557758efcc33268075be62ba75f445a547c3e1c3a990fc8" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "950918fb34f714331d8c3bd601c3fcf8635cb3ae253ebdbbbfa7ad499f863dd8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "3821dc1f990b4940f975f92a05f667b7e4b79422fda76089bf9e190615e72329" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 143 :sha256 "c1083b115ce49254f50d208ec4873936246265914c4f5d106d6fffa30ce2e0d5" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 145 :sha256 "50be04c4d96ea007405fc0cffd6d27a612bb5f93b420bad0832b117b4b0d3edd" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 147 :sha256 "6343b295d7c132b22722daeec797c6ca4d3212e36d8a57218cc661e1db1ef69a" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 148 :sha256 "6a77b85c2ee24a68bc73d465f1d7a8cac5cec84dc06df48bbe3541ed18309796" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 150 :sha256 "0ba715d3a7e2f64438a2ed2a6f0d375ddd5b54b393c8060111bb887bdaaf372d" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 152 :sha256 "f27772e376cd43f26ca3db5f11a7e77994a3d3582f0055a81696607052aa3d57" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "c9013dcbbe4aa814624d5d0c11722232781823f5584e4c1ca4c6309dbd7d49d9" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "58cafca7078cb984bf6aa9f77f69e1d18fafa37e777b46fbb8e430a60721f1b8" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "bffe5914312bfe60fe368109c17dfd66c6c128f5fca00bdbdf02e2dfb0902062" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 152 :sha256 "15a794ebc412b7228b65a74780aa6a53100bebbc3eed8163834d41b99a87d8ec" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 154 :sha256 "cedb634a1b813e7cc2c7adc52d0f735a4d851ed342085806d9119ff53baf0183" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 156 :sha256 "dc1b84ce4e841f5a9498458f03ffa39efe9da12ac7f2b8144d11109a29bdf4f6" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "95329897239ada0e9cf954c623b974b2eacaea240ab94260c502bae44f55fb21" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "304e64667d7f05e69fae8eda9607589018ed5dfe4641efa4d1d0b14235e75b6a" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "58f4504eac619975e45b31e22c359e2af5cbb343461dea026e9cb34989131685" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 142 :sha256 "7c355a826715400c85ddd60dd7876dbcbc135f808fb9a7df56f47eff7e9ccb7f" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 144 :sha256 "cfed0892af48042ba3b9b1e431b2c360b08802db76038a427c5441ea2ebc32bf" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 146 :sha256 "32d437e125d9e70fe6a96a2345ce177367638904e54e3bb63d460f362c5d011d" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 139 :sha256 "f78b7e797ee5540bed6ad5ab5b1a8a1fc903ce3e2962644399b52bf83050ca8b" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 141 :sha256 "c28ba1e91308de706d08e6e1c5273fc144d8015d2d800395b42fa7d918083221" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 143 :sha256 "ae0a8643947f9f716bbc58c06f4461c49b77b625aad448b80fb962b27bb6fd7b" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 146 :sha256 "e5abade1878104a270a89467ee182cf8a127432043c2eaa4498f51aa64162ffb" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 148 :sha256 "ac0bea20f2843c8698207d6407299c672a174865e14d95f32be3c81c52debc4c" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 150 :sha256 "5928391fa6991fc8b0f03d1a7a2f4f8247a2bf6fb3cdcf8f75dc50e26cdb2f0c" :delivery whole-frame) (:kind syntaxDiag :owner asynchronous :bytes 140 :sha256 "02e0003566e95a4f392f8efd8e8181a4f6524b20ed3432d2d7846b16f2705582" :delivery whole-frame) (:kind semanticDiag :owner asynchronous :bytes 142 :sha256 "30531903f7e94224a64e5dda8fe1b69f0f573940025565bf03c1044d258630ce" :delivery whole-frame) (:kind suggestionDiag :owner asynchronous :bytes 144 :sha256 "e571f81b0ef5c0a43155b0593c30deff3626c4b32db9327e8996779573af3a23" :delivery whole-frame) (:kind request-completed :owner (:request-completed 16) :bytes 102 :sha256 "afe2338b4e6fcde8aa077aa1c57caf7305fe84f091ee499d608cec1eba3b13a8" :delivery whole-frame))) :json "{\"command\":\"geterrForProject\",\"seq\":\"16\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"delay\":0}}")) :termination clean-eof))) :launches ((:name "tsserver" :buffer "*tide-server*" :program #1=[ADAPTER] :arguments (#2=[TSSERVER] "--disableAutomaticTypingAcquisition") :cwd #3=[ROOT] :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #1# :arguments (#2# "--disableAutomaticTypingAcquisition") :cwd #3# :environment-count 23)) :terminals ((:session 1 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 2 :status exit :exit 0 :message "finished\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "syntacticDiagnosticsSync" :callback registered) (:ordinal 4 :command "semanticDiagnosticsSync" :callback registered) (:ordinal 5 :command "suggestionDiagnosticsSync" :callback registered) (:ordinal 6 :command "projectInfo" :callback registered) (:ordinal 7 :command "geterrForProject" :callback not-registered) (:ordinal 8 :command "open" :callback not-registered) (:ordinal 9 :command "configure" :callback not-registered) (:ordinal 10 :command "projectInfo" :callback registered) (:ordinal 11 :command "geterrForProject" :callback not-registered) (:ordinal 12 :command "reload" :callback not-registered) (:ordinal 13 :command "syntacticDiagnosticsSync" :callback registered) (:ordinal 14 :command "semanticDiagnosticsSync" :callback registered) (:ordinal 15 :command "projectInfo" :callback registered) (:ordinal 16 :command "geterrForProject" :callback not-registered)) :public-deletes ((:session 1 :route kill-server)) :cleanup clean)"#
        ]],
    )
}

const EDITS_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((relative-file
        (file root)
        (and file (file-relative-name file root)))
       (undo-state
        ()
        (cond
         ((eq buffer-undo-list t) (list :enabled nil))
         ((null buffer-undo-list) (list :enabled t :entries 0
                                        :boundaries nil :head-boundary nil))
         (t
          (let ((index 0) boundaries)
            (dolist (entry buffer-undo-list)
              (when (null entry) (push index boundaries))
              (setq index (1+ index)))
            (list :enabled t :entries (length buffer-undo-list)
                  :boundaries (nreverse boundaries)
                  :head-boundary (null (car buffer-undo-list)))))))
       (buffer-state
        (buffer file root)
        (unless (and (buffer-live-p buffer)
                     (eq buffer (get-file-buffer file)))
          (error "Tide edit lost the owned source buffer"))
        (with-current-buffer buffer
          (let ((text (buffer-substring-no-properties (point-min) (point-max))))
            (list :identity t
                  :name (copy-sequence (buffer-name buffer))
                  :file (relative-file buffer-file-name root)
                  :text text
                  :crlf-sha256
                  (tide368-test-bytes-sha256
                   (encode-coding-string text 'utf-8-dos t))
                  :point (point) :mark (mark t) :mark-active mark-active
                  :modified (buffer-modified-p)
                  :coding buffer-file-coding-system
                  :undo (undo-state)
                  :disk-exists (file-exists-p file)
                  :disk-sha256 (tide368-test-file-sha256 file)))))
       (record-save
        (root main)
        (when (and buffer-file-name (equal buffer-file-name main))
          (push (list :file (relative-file buffer-file-name root)
                      :modified (buffer-modified-p)
                      :coding buffer-file-coding-system
                      :disk-sha256
                      (tide368-test-file-sha256 buffer-file-name))
                save-ledger)))
       (record-post-edit
        (root)
        (let ((text (buffer-substring-no-properties (point-min) (point-max))))
          (push (list :file (relative-file buffer-file-name root)
                      :point (point) :mark (mark t)
                      :modified (buffer-modified-p)
                      :crlf-sha256
                      (tide368-test-bytes-sha256
                       (encode-coding-string text 'utf-8-dos t))
                      :disk-sha256
                      (tide368-test-file-sha256 buffer-file-name))
                post-edit-ledger))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (math (expand-file-name "src/math.js" root))
           (config (expand-file-name "jsconfig.json" root))
           (buffer (find-file-noselect main))
           (save-observer (make-symbol "tide368-edit-save-observer"))
           (post-edit-observer (make-symbol "tide368-edit-post-observer"))
           (transient-mark-mode t)
           save-ledger post-edit-ledger
           selection before formatted undone redone organized
           full-before full-after noop-before noop-after jsdoc
           region-ledger organize-ledger full-ledger noop-ledger jsdoc-ledger)
      (fset save-observer (lambda () (record-save root main)))
      (fset post-edit-observer (lambda () (record-post-edit root)))
      (unwind-protect
          (progn
            (switch-to-buffer buffer)
            (js-mode)
            (setq-local tab-width 2 js-indent-level 2)
            (tide-setup)
            (tide368-test-assert-current-server)
            (add-hook 'after-save-hook save-observer)
            (buffer-enable-undo)
            (setq buffer-undo-list nil)
            (undo-boundary)
            (setq before (buffer-state buffer main root))
            (unless (and (= (plist-get before :point) 1)
                         (null (plist-get before :mark))
                         (not (plist-get before :mark-active))
                         (not (plist-get before :modified))
                         (eq (plist-get before :coding) 'utf-8-dos))
              (error "Tide edit initial editor state drifted"))
            (goto-char (point-min))
            (set-mark (save-excursion (forward-line 9) (point)))
            (activate-mark)
            (setq selection
                  (list :point (point) :mark (mark t)
                        :active mark-active
                        :region (buffer-substring-no-properties
                                 (region-beginning) (region-end))))
            (unless (and (= (plist-get selection :point) 1)
                         (= (plist-get selection :mark) 209)
                         (plist-get selection :active))
              (error "Tide selected format region drifted"))
            (tide-format)
            (deactivate-mark)
            (undo-boundary)
            (setq formatted (buffer-state buffer main root))
            (unless (and
                     (equal (plist-get formatted :crlf-sha256)
                            "43a379d57ca8ea31dfb0772c7236948a242bdee1053a4498c10663133b949e1b")
                     (= (plist-get formatted :point) 1)
                     (= (plist-get formatted :mark) 212)
                     (plist-get formatted :modified)
                     (equal (plist-get formatted :disk-sha256)
                            "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7"))
              (error "Tide selected-region format produced unexpected state"))
            (undo-only)
            (setq undone (buffer-state buffer main root))
            (undo-redo)
            (setq redone (buffer-state buffer main root))
            (unless (and
                     (equal (plist-get redone :text) (plist-get formatted :text))
                     (equal (plist-get redone :point) (plist-get formatted :point))
                     (equal (plist-get redone :mark) (plist-get formatted :mark))
                     (eq (plist-get redone :modified)
                         (plist-get formatted :modified)))
              (error "Tide public redo did not restore the formatted state"))
            (unless (and (null save-ledger) (null post-edit-ledger))
              (error "Tide region format/undo/redo unexpectedly saved or ran edit hooks"))
            (setq region-ledger
                  (list :saves (copy-tree save-ledger)
                        :post-edits (copy-tree post-edit-ledger)))
            (let ((tide-post-code-edit-hook (list post-edit-observer)))
              (tide-organize-imports))
            (setq organized (buffer-state buffer main root))
            (unless (and
                     (equal (plist-get organized :crlf-sha256)
                            "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")
                     (equal (plist-get organized :disk-sha256)
                            "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")
                     (= (plist-get organized :point) 1)
                     (= (plist-get organized :mark) 174)
                     (not (plist-get organized :modified))
                     (= (length save-ledger) 1)
                     (= (length post-edit-ledger) 1)
                     (let ((save (car save-ledger)))
                       (and (equal (plist-get save :file) "src/main.js")
                            (not (plist-get save :modified))
                            (eq (plist-get save :coding) 'utf-8-dos)
                            (equal (plist-get save :disk-sha256)
                                   "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")))
                     (let ((post-edit (car post-edit-ledger)))
                       (and (equal (plist-get post-edit :file) "src/main.js")
                            (not (plist-get post-edit :modified))
                            (equal (plist-get post-edit :crlf-sha256)
                                   "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")
                            (equal (plist-get post-edit :disk-sha256)
                                   "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394"))))
              (error "Tide organize-imports did not save and hook exactly once"))
            (setq organize-ledger
                  (list :saves (reverse (copy-tree save-ledger))
                        :post-edits (reverse (copy-tree post-edit-ledger))))
            (deactivate-mark)
            (goto-char (point-min))
            (setq full-before (buffer-state buffer main root))
            (tide-format)
            (setq full-after (buffer-state buffer main root))
            (unless (and
                     (equal (plist-get full-after :crlf-sha256)
                            "04cdc0dbb144aec0702e1c2cd52c7c79d6edeb7d04f77fb05fa3cf1261c1a1db")
                     (= (plist-get full-after :point) 1)
                     (= (plist-get full-after :mark) 174)
                     (plist-get full-after :modified)
                     (equal (plist-get full-after :disk-sha256)
                            "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")
                     (= (length save-ledger) 1)
                     (= (length post-edit-ledger) 1))
              (error "Tide public full-buffer format produced unexpected state: %S"
                     full-after))
            (setq full-ledger
                  (list :saves (reverse (copy-tree save-ledger))
                        :post-edits (reverse (copy-tree post-edit-ledger))))
            (goto-char (point-min))
            (setq noop-before (buffer-state buffer main root))
            (let ((undo-before (copy-tree buffer-undo-list)))
              (tide-format)
              (setq noop-after (buffer-state buffer main root))
              (unless (and (equal noop-before noop-after)
                           (equal undo-before buffer-undo-list)
                           (= (length save-ledger) 1)
                           (= (length post-edit-ledger) 1))
                (error "Tide no-op format changed editor, disk, undo, or hook state")))
            (setq noop-ledger
                  (list :saves (reverse (copy-tree save-ledger))
                        :post-edits (reverse (copy-tree post-edit-ledger))))
            (goto-char (point-min))
            (search-forward "export const total")
            (beginning-of-line)
            (tide-jsdoc-template)
            (setq jsdoc (buffer-state buffer main root))
            (unless (and
                     (equal (tide368-test-bytes-sha256
                             (encode-coding-string
                              (plist-get jsdoc :text) 'utf-8-unix t))
                            "dbc3fa8a7fd72c07c4b41898ea0e3136f4e453f345c5f6bd2aad366195313573")
                     (= (plist-get jsdoc :point) 146)
                     (= (plist-get jsdoc :mark) 180)
                     (plist-get jsdoc :modified)
                     (equal (plist-get jsdoc :disk-sha256)
                            "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")
                     (= (length save-ledger) 1)
                     (= (length post-edit-ledger) 1)
                     (equal (tide368-test-file-sha256 math)
                            "ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4")
                     (equal (tide368-test-file-sha256 config)
                            "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c"))
              (error "Tide JSDoc recovery produced unexpected final state"))
            (setq jsdoc-ledger
                  (list :saves (reverse (copy-tree save-ledger))
                        :post-edits (reverse (copy-tree post-edit-ledger))))
            (list :region (list :before before :selection selection
                                :formatted formatted :undone undone :redone redone
                                :ledger region-ledger)
                  :organize (list :before redone :after organized
                                  :ledger organize-ledger)
                  :full-format (list :before full-before :after full-after
                                     :ledger full-ledger)
                  :no-op (list :before noop-before :after noop-after :equal t
                               :ledger noop-ledger)
                  :jsdoc (list :state jsdoc :ledger jsdoc-ledger)
                  :saves (nreverse (copy-tree save-ledger))
                  :post-edits (nreverse (copy-tree post-edit-ledger))))
        (remove-hook 'after-save-hook save-observer)
        (fmakunbound save-observer)
        (fmakunbound post-edit-observer)))))"#;

fn format_organize_jsdoc_and_undo() -> ParityBatchCase {
    assert_recorded_bytes_digest(
        "organized main fixture",
        ORGANIZED_MAIN_BYTES,
        "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394",
    );
    let fixtures = common_manifest();
    let initial = fixtures.generation();
    let organized = organized_generation();
    let mut exchanges = vec![
        RecordedExchange::new(
            ordinal(1),
            TsRequest::Open(
                OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
            ),
            initial.clone(),
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(2),
            configure_request(),
            initial.clone(),
            ApprovedOutput::frames_delayed(ordinal(3), captured_startup_frames()).unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(3),
            TsRequest::Format(
                RangeRequest::new(path("src/main.js"), point(1, 1), point(10, 1)).unwrap(),
            ),
            initial.clone(),
            ApprovedOutput::frames(
                ordinal(3),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDM5Mw0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImZvcm1hdCIsInJlcXVlc3Rfc2VxIjoiMyIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOlt7InN0YXJ0Ijp7ImxpbmUiOjUsIm9mZnNldCI6MjJ9LCJlbmQiOnsibGluZSI6NSwib2Zmc2V0IjoyM30sIm5ld1RleHQiOiIgIn0seyJzdGFydCI6eyJsaW5lIjo5LCJvZmZzZXQiOjE5fSwiZW5kIjp7ImxpbmUiOjksIm9mZnNldCI6MTl9LCJuZXdUZXh0IjoiICJ9LHsic3RhcnQiOnsibGluZSI6OSwib2Zmc2V0IjoyMH0sImVuZCI6eyJsaW5lIjo5LCJvZmZzZXQiOjIwfSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjksIm9mZnNldCI6MjZ9LCJlbmQiOnsibGluZSI6OSwib2Zmc2V0IjoyNn0sIm5ld1RleHQiOiIgIn1dfQo=",
                    "328d23d6f65cac198a924f331e52044e01e887e25343a8384fc77ba176edd891",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(4),
            TsRequest::Reload(ReloadRequest {
                file: path("src/main.js"),
                temporary_file: TideTempFileToken::new(
                    path("src/main.js"),
                    digest(
                        "43a379d57ca8ea31dfb0772c7236948a242bdee1053a4498c10663133b949e1b",
                    ),
                ),
            }),
            initial.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(5),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDgwDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoicmVsb2FkIiwicmVxdWVzdF9zZXEiOiI0Iiwic3VjY2VzcyI6dHJ1ZX0K",
                        "91206875942d9a5548ec342919598ca87d8c02d0af2e325ca751015e57c7c244",
                        Vec::new(),
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDExMQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbG9hZCIsInJlcXVlc3Rfc2VxIjoiNCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsicmVsb2FkRmluaXNoZWQiOnRydWV9fQo=",
                        "db1c4d256fd47f9adf4cf82d6b53275f528ae06b4d8a9a970cf9a935db27e204",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(5),
            TsRequest::OrganizeImports(FileRequest {
                file: path("src/main.js"),
            }),
            initial.clone(),
            ApprovedOutput::frames(
                ordinal(5),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDM5Mg0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6Im9yZ2FuaXplSW1wb3J0cyIsInJlcXVlc3Rfc2VxIjoiNSIsInN1Y2Nlc3MiOnRydWUsInBlcmZvcm1hbmNlRGF0YSI6eyJ1cGRhdGVHcmFwaER1cmF0aW9uTXMiOjMuMDc3ODk0MDAwMDAwMDE1fSwiYm9keSI6W3siZmlsZU5hbWUiOiJbUk9PVF0vc3JjL21haW4uanMiLCJ0ZXh0Q2hhbmdlcyI6W3sic3RhcnQiOnsibGluZSI6MSwib2Zmc2V0IjoxfSwiZW5kIjp7ImxpbmUiOjIsIm9mZnNldCI6MX0sIm5ld1RleHQiOiJpbXBvcnQgeyBhZGQgfSBmcm9tIFwiLi9tYXRoLmpzXCI7XG4ifSx7InN0YXJ0Ijp7ImxpbmUiOjIsIm9mZnNldCI6MX0sImVuZCI6eyJsaW5lIjozLCJvZmZzZXQiOjF9LCJuZXdUZXh0IjoiIn1dfV19Cg==",
                    "9b6d939e8610e51ede3e12d86d6ce15c6116eda9a1cea23fdbb80a300841e117",
                    vec![ResponseToken::root_path(
                        vec![
                            JsonPathSegment::Key("body"),
                            JsonPathSegment::Index(0),
                            JsonPathSegment::Key("fileName"),
                        ],
                        path("src/main.js"),
                    )],
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(6),
            TsRequest::Reload(ReloadRequest {
                file: path("src/main.js"),
                temporary_file: TideTempFileToken::new(
                    path("src/main.js"),
                    digest(
                        "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394",
                    ),
                ),
            }),
            initial.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(7),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDgwDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoicmVsb2FkIiwicmVxdWVzdF9zZXEiOiI2Iiwic3VjY2VzcyI6dHJ1ZX0K",
                        "97b55a4cb8d6a5470506330054f201ef2308cb038b8a97b793d1a9be176159be",
                        Vec::new(),
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDExMQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbG9hZCIsInJlcXVlc3Rfc2VxIjoiNiIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsicmVsb2FkRmluaXNoZWQiOnRydWV9fQo=",
                        "bb4d768182ec85f8dfd7b6dd788c0fc9a89505607a05a324ec6402f057af76b3",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(7),
            TsRequest::Format(
                RangeRequest::new(path("src/main.js"), point(1, 1), point(2, 1)).unwrap(),
            ),
            initial,
            ApprovedOutput::frames(
                ordinal(7),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDkwDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoiZm9ybWF0IiwicmVxdWVzdF9zZXEiOiI3Iiwic3VjY2VzcyI6dHJ1ZSwiYm9keSI6W119Cg==",
                    "c942e9a4e6ea613ad1f63d731b77c09c3bda1cd221eddba8681c24c57d601d78",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
    ];
    exchanges.extend([
        RecordedExchange::new(
            ordinal(8),
            TsRequest::Format(
                RangeRequest::new(path("src/main.js"), point(1, 1), point(17, 1)).unwrap(),
            ),
            organized.clone(),
            ApprovedOutput::frames(
                ordinal(8),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDYzNQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImZvcm1hdCIsInJlcXVlc3Rfc2VxIjoiOCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOlt7InN0YXJ0Ijp7ImxpbmUiOjEyLCJvZmZzZXQiOjE5fSwiZW5kIjp7ImxpbmUiOjEyLCJvZmZzZXQiOjE5fSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjEyLCJvZmZzZXQiOjIwfSwiZW5kIjp7ImxpbmUiOjEyLCJvZmZzZXQiOjIwfSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjEyLCJvZmZzZXQiOjM2fSwiZW5kIjp7ImxpbmUiOjEyLCJvZmZzZXQiOjM2fSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjEyLCJvZmZzZXQiOjQyfSwiZW5kIjp7ImxpbmUiOjEyLCJvZmZzZXQiOjQyfSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjE2LCJvZmZzZXQiOjMyfSwiZW5kIjp7ImxpbmUiOjE2LCJvZmZzZXQiOjMyfSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjE2LCJvZmZzZXQiOjMzfSwiZW5kIjp7ImxpbmUiOjE2LCJvZmZzZXQiOjMzfSwibmV3VGV4dCI6IiAifSx7InN0YXJ0Ijp7ImxpbmUiOjE2LCJvZmZzZXQiOjU2fSwiZW5kIjp7ImxpbmUiOjE2LCJvZmZzZXQiOjU2fSwibmV3VGV4dCI6IiAifV19Cg==",
                    "a6cc4ff3973a31140260dd5e98d7248f77e898143a318235ba0c08bbcbcd02a3",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(9),
            TsRequest::Reload(ReloadRequest {
                file: path("src/main.js"),
                temporary_file: TideTempFileToken::new(
                    path("src/main.js"),
                    digest(
                        "04cdc0dbb144aec0702e1c2cd52c7c79d6edeb7d04f77fb05fa3cf1261c1a1db",
                    ),
                ),
            }),
            organized.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(10),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDgwDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoicmVsb2FkIiwicmVxdWVzdF9zZXEiOiI5Iiwic3VjY2VzcyI6dHJ1ZX0K",
                        "5391c0b4ca7ddb4f6f7db748dd1f154b69d739f2d9ad49e66e735bb64c38282b",
                        Vec::new(),
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDExMQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbG9hZCIsInJlcXVlc3Rfc2VxIjoiOSIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsicmVsb2FkRmluaXNoZWQiOnRydWV9fQo=",
                        "7f8d956be7a5a43658bf1c259d90b7cfde6d5582e6317ff4e957ff77375f41bb",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(10),
            TsRequest::Format(
                RangeRequest::new(path("src/main.js"), point(1, 1), point(17, 1)).unwrap(),
            ),
            organized.clone(),
            ApprovedOutput::frames(
                ordinal(10),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDkxDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoiZm9ybWF0IiwicmVxdWVzdF9zZXEiOiIxMCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOltdfQo=",
                    "d9e3875c9ba3b8cfee152aadbc8448c0cb07e684dc4e7a90f776c63e4db707e1",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(11),
            TsRequest::DocCommentTemplate(PointRequest {
                file: path("src/main.js"),
                point: point(8, 1),
            }),
            organized,
            ApprovedOutput::frames(
                ordinal(11),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDEzNw0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImRvY0NvbW1lbnRUZW1wbGF0ZSIsInJlcXVlc3Rfc2VxIjoiMTEiLCJzdWNjZXNzIjp0cnVlLCJib2R5Ijp7Im5ld1RleHQiOiIvKiogKi8iLCJjYXJldE9mZnNldCI6M319Cg==",
                    "f41f4b9030c36ab3fb489b11e84fbdba58a41f9e7a1c9d023e7a4cf998156afd",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
    ]);
    let session = ReplaySession::new(
        exchanges,
        digest("fa63404ddc6b344fd25d4169904581202e8cf34e3afeea43e5ad986d063e602b"),
        digest("b912062ef19d4677df395d211f7c66499339a1fee577e0aa1cede053ed0832fa"),
        ReplayTermination::CleanEof,
    )
    .unwrap();
    let replay = TideReplay::new(TideScenario::Edits, fixtures, vec![session]).unwrap();
    materialized_case(
        "format_organize_jsdoc_and_undo",
        replay,
        EDITS_BODY,
        expect![[
            r#"OK (:result (:region (:before (:identity t :name "main.js" :file "src/main.js" :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7" :point 1 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:enabled t :entries 0 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7") :selection (:point 1 :mark 209 :active t :region "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n") :formatted (:identity t :name "main.js" :file "src/main.js" :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "43a379d57ca8ea31dfb0772c7236948a242bdee1053a4498c10663133b949e1b" :point 1 :mark 212 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 2 :boundaries (0) :head-boundary t) :disk-exists t :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7") :undone (:identity t :name "main.js" :file "src/main.js" :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7" :point 1 :mark 209 :mark-active nil :modified nil :coding utf-8-dos :undo (:enabled t :entries 3 :boundaries (1) :head-boundary nil) :disk-exists t :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7") :redone #1=(:identity t :name "main.js" :file "src/main.js" :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "43a379d57ca8ea31dfb0772c7236948a242bdee1053a4498c10663133b949e1b" :point 1 :mark 212 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 1 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7") :ledger (:saves nil :post-edits nil)) :organize (:before #1# :after (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :point 1 :mark 174 :mark-active nil :modified nil :coding utf-8-dos :undo (:enabled t :entries 6 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :ledger (:saves ((:file "src/main.js" :modified nil :coding utf-8-dos :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")) :post-edits ((:file "src/main.js" :point 1 :mark 174 :modified nil :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")))) :full-format (:before (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :point 1 :mark 174 :mark-active nil :modified nil :coding utf-8-dos :undo (:enabled t :entries 6 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :after (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right) { return add(left, right) }\n}\n\n/** @param {number} value */\nexport function describe(value) { return `total=${value}` }\n" :crlf-sha256 "04cdc0dbb144aec0702e1c2cd52c7c79d6edeb7d04f77fb05fa3cf1261c1a1db" :point 1 :mark 174 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 7 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :ledger (:saves ((:file "src/main.js" :modified nil :coding utf-8-dos :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")) :post-edits ((:file "src/main.js" :point 1 :mark 174 :modified nil :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")))) :no-op (:before (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right) { return add(left, right) }\n}\n\n/** @param {number} value */\nexport function describe(value) { return `total=${value}` }\n" :crlf-sha256 "04cdc0dbb144aec0702e1c2cd52c7c79d6edeb7d04f77fb05fa3cf1261c1a1db" :point 1 :mark 174 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 7 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :after (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right) { return add(left, right) }\n}\n\n/** @param {number} value */\nexport function describe(value) { return `total=${value}` }\n" :crlf-sha256 "04cdc0dbb144aec0702e1c2cd52c7c79d6edeb7d04f77fb05fa3cf1261c1a1db" :point 1 :mark 174 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 7 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :equal t :ledger (:saves ((:file "src/main.js" :modified nil :coding utf-8-dos :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")) :post-edits ((:file "src/main.js" :point 1 :mark 174 :modified nil :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")))) :jsdoc (:state (:identity t :name "main.js" :file "src/main.js" :text "import { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed = 界;\n\n/** @type {string} */\nexport const label = add(1, 2);\n/** */export const total = add(1, 2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right) { return add(left, right) }\n}\n\n/** @param {number} value */\nexport function describe(value) { return `total=${value}` }\n" :crlf-sha256 "86b7b01db578b876f57f7effdc255ffc06dcf37319bd74dbb847391a3c51d446" :point 146 :mark 180 :mark-active nil :modified t :coding utf-8-dos :undo (:enabled t :entries 8 :boundaries nil :head-boundary nil) :disk-exists t :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394") :ledger (:saves ((:file "src/main.js" :modified nil :coding utf-8-dos :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")) :post-edits ((:file "src/main.js" :point 1 :mark 174 :modified nil :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")))) :saves ((:file "src/main.js" :modified nil :coding utf-8-dos :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394")) :post-edits ((:file "src/main.js" :point 1 :mark 174 :modified nil :crlf-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394" :disk-sha256 "6f718339f7709571ebed3a743b82f6ff3598e19f4f2b7dbd3937392ae83bc394"))) :typed (:scenario edits :fixture-count 3 :session-count 1 :sessions ((:first-ordinal 1 :requests (open configure format reload organizeImports reload format format reload format docCommentTemplate) :request-count 11 :frame-count 17 :request-sha256 "fa63404ddc6b344fd25d4169904581202e8cf34e3afeea43e5ad986d063e602b" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 3 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "format") :bytes 416 :sha256 "328d23d6f65cac198a924f331e52044e01e887e25343a8384fc77ba176edd891" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"3\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":1,\"offset\":1,\"endLine\":10,\"endOffset\":1}}") (:ordinal 4 :outcome complete :callback not-registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 4 "reload") :bytes 102 :sha256 "91206875942d9a5548ec342919598ca87d8c02d0af2e325ca751015e57c7c244" :delivery whole-frame) (:kind response :owner (:response 4 "reload") :bytes 134 :sha256 "db1c4d256fd47f9adf4cf82d6b53275f528ae06b4d8a9a970cf9a935db27e204" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 5 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "organizeImports") :bytes 415 :sha256 "9b6d939e8610e51ede3e12d86d6ce15c6116eda9a1cea23fdbb80a300841e117" :delivery whole-frame))) :json "{\"command\":\"organizeImports\",\"seq\":\"5\",\"arguments\":{\"scope\":{\"type\":\"file\",\"args\":{\"file\":\"[ROOT]/src/main.js\"}}}}") (:ordinal 6 :outcome complete :callback not-registered :output (:delivery-after 7 :frames ((:kind response :owner (:response 6 "reload") :bytes 102 :sha256 "97b55a4cb8d6a5470506330054f201ef2308cb038b8a97b793d1a9be176159be" :delivery whole-frame) (:kind response :owner (:response 6 "reload") :bytes 134 :sha256 "bb4d768182ec85f8dfd7b6dd788c0fc9a89505607a05a324ec6402f057af76b3" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"6\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 7 :outcome complete :callback registered :output (:delivery-after 7 :frames ((:kind response :owner (:response 7 "format") :bytes 112 :sha256 "c942e9a4e6ea613ad1f63d731b77c09c3bda1cd221eddba8681c24c57d601d78" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"7\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":1,\"offset\":1,\"endLine\":2,\"endOffset\":1}}") (:ordinal 8 :outcome complete :callback registered :output (:delivery-after 8 :frames ((:kind response :owner (:response 8 "format") :bytes 658 :sha256 "a6cc4ff3973a31140260dd5e98d7248f77e898143a318235ba0c08bbcbcd02a3" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"8\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":1,\"offset\":1,\"endLine\":17,\"endOffset\":1}}") (:ordinal 9 :outcome complete :callback not-registered :output (:delivery-after 10 :frames ((:kind response :owner (:response 9 "reload") :bytes 102 :sha256 "5391c0b4ca7ddb4f6f7db748dd1f154b69d739f2d9ad49e66e735bb64c38282b" :delivery whole-frame) (:kind response :owner (:response 9 "reload") :bytes 134 :sha256 "7f8d956be7a5a43658bf1c259d90b7cfde6d5582e6317ff4e957ff77375f41bb" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"9\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 10 :outcome complete :callback registered :output (:delivery-after 10 :frames ((:kind response :owner (:response 10 "format") :bytes 113 :sha256 "d9e3875c9ba3b8cfee152aadbc8448c0cb07e684dc4e7a90f776c63e4db707e1" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"10\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":1,\"offset\":1,\"endLine\":17,\"endOffset\":1}}") (:ordinal 11 :outcome complete :callback registered :output (:delivery-after 11 :frames ((:kind response :owner (:response 11 "docCommentTemplate") :bytes 160 :sha256 "f41f4b9030c36ab3fb489b11e84fbdba58a41f9e7a1c9d023e7a4cf998156afd" :delivery whole-frame))) :json "{\"command\":\"docCommentTemplate\",\"seq\":\"11\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":1}}")) :termination clean-eof))) :launches ((:name "tsserver" :buffer "*tide-server*" :program [ADAPTER] :arguments ([TSSERVER] "--disableAutomaticTypingAcquisition") :cwd [ROOT] :environment-count 23)) :terminals ((:session 1 :status exit :exit 0 :message "finished\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "format" :callback registered) (:ordinal 4 :command "reload" :callback not-registered) (:ordinal 5 :command "organizeImports" :callback registered) (:ordinal 6 :command "reload" :callback not-registered) (:ordinal 7 :command "format" :callback registered) (:ordinal 8 :command "format" :callback registered) (:ordinal 9 :command "reload" :callback not-registered) (:ordinal 10 :command "format" :callback registered) (:ordinal 11 :command "docCommentTemplate" :callback registered)) :public-deletes nil :cleanup clean)"#
        ]],
    )
}

fn cross_file_symbol_and_file_rename() -> ParityBatchCase {
    for (label, bytes, expected) in [
        (
            "symbol-renamed main fixture",
            RENAMED_MAIN_BYTES,
            "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b",
        ),
        (
            "symbol-renamed math fixture",
            RENAMED_MATH_BYTES,
            "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078",
        ),
        (
            "file-renamed config fixture",
            FILE_RENAMED_CONFIG_BYTES,
            "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353",
        ),
        (
            "file-renamed main fixture",
            FILE_RENAMED_MAIN_BYTES,
            "7aa4a05c1e09bab0e7c91d85c52818c5bf862138caa505bdc7f6de35f45c423e",
        ),
    ] {
        assert_recorded_bytes_digest(label, bytes, expected);
    }
    let fixtures = common_manifest();
    let initial = fixtures.generation();
    let symbol_renamed = rename_generation(
        "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c",
        "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b",
        "src/math.js",
        "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078",
        false,
    );
    let peer_moved = rename_generation(
        "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c",
        "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b",
        "src/arithmetic 界.js",
        "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078",
        true,
    );
    let peer_move_transition =
        FixtureGeneration::one_of(vec![symbol_renamed.clone(), peer_moved.clone()])
            .expect("the no-callback close may race only the exact atomic peer move");
    let config_saved = rename_generation(
        "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353",
        "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b",
        "src/arithmetic 界.js",
        "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078",
        true,
    );
    let rename_tokens = || {
        vec![
            ResponseToken::root_path(
                vec![
                    JsonPathSegment::Key("body"),
                    JsonPathSegment::Key("locs"),
                    JsonPathSegment::Index(0),
                    JsonPathSegment::Key("file"),
                ],
                path("src/main.js"),
            ),
            ResponseToken::root_path(
                vec![
                    JsonPathSegment::Key("body"),
                    JsonPathSegment::Key("locs"),
                    JsonPathSegment::Index(1),
                    JsonPathSegment::Key("file"),
                ],
                path("src/math.js"),
            ),
        ]
    };
    let mut exchanges = vec![
        RecordedExchange::new(
            ordinal(1),
            TsRequest::Open(
                OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
            ),
            initial.clone(),
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(2),
            configure_request(),
            initial.clone(),
            ApprovedOutput::frames_delayed(ordinal(3), captured_startup_frames()).unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(3),
            TsRequest::Rename(PointRequest {
                file: path("src/main.js"),
                point: point(8, 22),
            }),
            initial.clone(),
            ApprovedOutput::frames(
                ordinal(3),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDg4MQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbmFtZSIsInJlcXVlc3Rfc2VxIjoiMyIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsiaW5mbyI6eyJjYW5SZW5hbWUiOnRydWUsImRpc3BsYXlOYW1lIjoiYWRkIiwiZnVsbERpc3BsYXlOYW1lIjoiYWRkIiwia2luZCI6ImFsaWFzIiwia2luZE1vZGlmaWVycyI6ImV4cG9ydCIsInRyaWdnZXJTcGFuIjp7InN0YXJ0Ijp7ImxpbmUiOjgsIm9mZnNldCI6MjJ9LCJlbmQiOnsibGluZSI6OCwib2Zmc2V0IjoyNX19fSwibG9jcyI6W3siZmlsZSI6IltST09UXS9zcmMvbWFpbi5qcyIsImxvY3MiOlt7InN0YXJ0Ijp7ImxpbmUiOjIsIm9mZnNldCI6MTB9LCJlbmQiOnsibGluZSI6Miwib2Zmc2V0IjoxM30sImNvbnRleHRTdGFydCI6eyJsaW5lIjoyLCJvZmZzZXQiOjF9LCJjb250ZXh0RW5kIjp7ImxpbmUiOjIsIm9mZnNldCI6MzN9fSx7InN0YXJ0Ijp7ImxpbmUiOjQsIm9mZnNldCI6MTh9LCJlbmQiOnsibGluZSI6NCwib2Zmc2V0IjoyMX19LHsic3RhcnQiOnsibGluZSI6OCwib2Zmc2V0IjoyMn0sImVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjI1fX0seyJzdGFydCI6eyJsaW5lIjo5LCJvZmZzZXQiOjIwfSwiZW5kIjp7ImxpbmUiOjksIm9mZnNldCI6MjN9fSx7InN0YXJ0Ijp7ImxpbmUiOjEzLCJvZmZzZXQiOjI3fSwiZW5kIjp7ImxpbmUiOjEzLCJvZmZzZXQiOjMwfX1dfSx7ImZpbGUiOiJbUk9PVF0vc3JjL21hdGguanMiLCJsb2NzIjpbeyJzdGFydCI6eyJsaW5lIjo2LCJvZmZzZXQiOjE3fSwiZW5kIjp7ImxpbmUiOjYsIm9mZnNldCI6MjB9LCJjb250ZXh0U3RhcnQiOnsibGluZSI6Niwib2Zmc2V0IjoxfSwiY29udGV4dEVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjJ9fV19XX19Cg==",
                    "3386eca93c5be8aad93e27497d2d7fefbd55b76c1cffb6b68087d15ac043c498",
                    rename_tokens(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(4),
            TsRequest::Rename(PointRequest {
                file: path("src/main.js"),
                point: point(8, 22),
            }),
            initial,
            ApprovedOutput::frames(
                ordinal(4),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDg4MQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbmFtZSIsInJlcXVlc3Rfc2VxIjoiNCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsiaW5mbyI6eyJjYW5SZW5hbWUiOnRydWUsImRpc3BsYXlOYW1lIjoiYWRkIiwiZnVsbERpc3BsYXlOYW1lIjoiYWRkIiwia2luZCI6ImFsaWFzIiwia2luZE1vZGlmaWVycyI6ImV4cG9ydCIsInRyaWdnZXJTcGFuIjp7InN0YXJ0Ijp7ImxpbmUiOjgsIm9mZnNldCI6MjJ9LCJlbmQiOnsibGluZSI6OCwib2Zmc2V0IjoyNX19fSwibG9jcyI6W3siZmlsZSI6IltST09UXS9zcmMvbWFpbi5qcyIsImxvY3MiOlt7InN0YXJ0Ijp7ImxpbmUiOjIsIm9mZnNldCI6MTB9LCJlbmQiOnsibGluZSI6Miwib2Zmc2V0IjoxM30sImNvbnRleHRTdGFydCI6eyJsaW5lIjoyLCJvZmZzZXQiOjF9LCJjb250ZXh0RW5kIjp7ImxpbmUiOjIsIm9mZnNldCI6MzN9fSx7InN0YXJ0Ijp7ImxpbmUiOjQsIm9mZnNldCI6MTh9LCJlbmQiOnsibGluZSI6NCwib2Zmc2V0IjoyMX19LHsic3RhcnQiOnsibGluZSI6OCwib2Zmc2V0IjoyMn0sImVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjI1fX0seyJzdGFydCI6eyJsaW5lIjo5LCJvZmZzZXQiOjIwfSwiZW5kIjp7ImxpbmUiOjksIm9mZnNldCI6MjN9fSx7InN0YXJ0Ijp7ImxpbmUiOjEzLCJvZmZzZXQiOjI3fSwiZW5kIjp7ImxpbmUiOjEzLCJvZmZzZXQiOjMwfX1dfSx7ImZpbGUiOiJbUk9PVF0vc3JjL21hdGguanMiLCJsb2NzIjpbeyJzdGFydCI6eyJsaW5lIjo2LCJvZmZzZXQiOjE3fSwiZW5kIjp7ImxpbmUiOjYsIm9mZnNldCI6MjB9LCJjb250ZXh0U3RhcnQiOnsibGluZSI6Niwib2Zmc2V0IjoxfSwiY29udGV4dEVuZCI6eyJsaW5lIjo4LCJvZmZzZXQiOjJ9fV19XX19Cg==",
                    "94459339e2392db41db0541039151875f8abc9207310a0dcdf9d95aab5d66161",
                    rename_tokens(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
    ];
    exchanges.extend([
        RecordedExchange::new(
            ordinal(5),
            TsRequest::Reload(ReloadRequest {
                file: path("src/main.js"),
                temporary_file: TideTempFileToken::new(
                    path("src/main.js"),
                    digest("6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b"),
                ),
            }),
            symbol_renamed.clone(),
            ApprovedOutput::frames(
                ordinal(5),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDgwDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoicmVsb2FkIiwicmVxdWVzdF9zZXEiOiI1Iiwic3VjY2VzcyI6dHJ1ZX0K",
                        "3fa81aca945a7956c2d44e16691bd6b3a794a0c6bb8e1d761c7f8c59165743ff",
                        Vec::new(),
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDExMQ0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbG9hZCIsInJlcXVlc3Rfc2VxIjoiNSIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOnsicmVsb2FkRmluaXNoZWQiOnRydWV9fQo=",
                        "e18255d2573ddf1d26ee426c7a94b0246d1afac36bd28cd71ccf712f87e41fd0",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(6),
            TsRequest::Open(
                OpenRequest::immediate(path("src/math.js"), ScriptKind::JavaScript).unwrap(),
            ),
            symbol_renamed.clone(),
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(7),
            configure_request_for(path("src/math.js")),
            symbol_renamed.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(8),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDgzDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoiY29uZmlndXJlIiwicmVxdWVzdF9zZXEiOiI3Iiwic3VjY2VzcyI6dHJ1ZX0K",
                    "922c0f9501f357adaa0413d5bf61e39d2f5b7c8e81a9ac61d43ab694809f155a",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(8),
            TsRequest::FileRename(FileRenameRequest {
                old_file: path("src/math.js"),
                new_file: path("src/arithmetic 界.js"),
            }),
            symbol_renamed.clone(),
            ApprovedOutput::frames(
                ordinal(8),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDQ5Mg0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImdldEVkaXRzRm9yRmlsZVJlbmFtZSIsInJlcXVlc3Rfc2VxIjoiOCIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOlt7ImZpbGVOYW1lIjoiW1JPT1RdL2pzY29uZmlnLmpzb24iLCJ0ZXh0Q2hhbmdlcyI6W3sic3RhcnQiOnsibGluZSI6MTAsIm9mZnNldCI6Mjl9LCJlbmQiOnsibGluZSI6MTAsIm9mZnNldCI6NDB9LCJuZXdUZXh0Ijoic3JjL2FyaXRobWV0aWMg55WMLmpzIn1dfSx7ImZpbGVOYW1lIjoiW1JPT1RdL3NyYy9tYWluLmpzIiwidGV4dENoYW5nZXMiOlt7InN0YXJ0Ijp7ImxpbmUiOjEsIm9mZnNldCI6Mjd9LCJlbmQiOnsibGluZSI6MSwib2Zmc2V0IjozNn0sIm5ld1RleHQiOiIuL2FyaXRobWV0aWMg55WMLmpzIn0seyJzdGFydCI6eyJsaW5lIjoyLCJvZmZzZXQiOjIzfSwiZW5kIjp7ImxpbmUiOjIsIm9mZnNldCI6MzJ9LCJuZXdUZXh0IjoiLi9hcml0aG1ldGljIOeVjC5qcyJ9XX1dfQo=",
                    "b0ea4d03010662536a6fc8254f783e98a0beb9e95c294f47afb2ac8daa3c83e2",
                    vec![
                        ResponseToken::root_path(
                            vec![
                                JsonPathSegment::Key("body"),
                                JsonPathSegment::Index(0),
                                JsonPathSegment::Key("fileName"),
                            ],
                            path("jsconfig.json"),
                        ),
                        ResponseToken::root_path(
                            vec![
                                JsonPathSegment::Key("body"),
                                JsonPathSegment::Index(1),
                                JsonPathSegment::Key("fileName"),
                            ],
                            path("src/main.js"),
                        ),
                    ],
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(9),
            TsRequest::Close(FileRequest {
                file: path("src/math.js"),
            }),
            peer_move_transition,
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(10),
            TsRequest::Open(OpenRequest::inferred(path("jsconfig.json")).unwrap()),
            peer_moved.clone(),
            ApprovedOutput::no_frames(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(11),
            configure_request_for(path("jsconfig.json")),
            peer_moved.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(12),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDUyOA0KDQp7InNlcSI6MCwidHlwZSI6ImV2ZW50IiwiZXZlbnQiOiJjb25maWdGaWxlRGlhZyIsImJvZHkiOnsidHJpZ2dlckZpbGUiOiJbUk9PVF0vanNjb25maWcuanNvbiIsImNvbmZpZ0ZpbGUiOiJbUk9PVF0vanNjb25maWcuanNvbiIsImRpYWdub3N0aWNzIjpbeyJ0ZXh0IjoiRmlsZSAnW1JPT1RdL3NyYy9tYXRoLmpzJyBub3QgZm91bmQuXG4gIFRoZSBmaWxlIGlzIGluIHRoZSBwcm9ncmFtIGJlY2F1c2U6XG4gICAgUGFydCBvZiAnZmlsZXMnIGxpc3QgaW4gdHNjb25maWcuanNvbiIsImNvZGUiOjYwNTMsImNhdGVnb3J5IjoiZXJyb3IiLCJyZWxhdGVkSW5mb3JtYXRpb24iOlt7InNwYW4iOnsic3RhcnQiOnsibGluZSI6MTAsIm9mZnNldCI6Mjh9LCJlbmQiOnsibGluZSI6MTAsIm9mZnNldCI6NDF9LCJmaWxlIjoiW1JPT1RdL2pzY29uZmlnLmpzb24ifSwibWVzc2FnZSI6IkZpbGUgaXMgbWF0Y2hlZCBieSAnZmlsZXMnIGxpc3Qgc3BlY2lmaWVkIGhlcmUuIiwiY2F0ZWdvcnkiOiJtZXNzYWdlIiwiY29kZSI6MTQxMH1dfV19fQo=",
                        "dda0b539a89781de51cfe5cfe4ba02ae9ebb7a1e34ff404f247d34e7d2405686",
                        vec![
                            ResponseToken::root_path(
                                vec![
                                    JsonPathSegment::Key("body"),
                                    JsonPathSegment::Key("triggerFile"),
                                ],
                                path("jsconfig.json"),
                            ),
                            ResponseToken::root_path(
                                vec![
                                    JsonPathSegment::Key("body"),
                                    JsonPathSegment::Key("configFile"),
                                ],
                                path("jsconfig.json"),
                            ),
                            ResponseToken::embedded_root_path(
                                vec![
                                    JsonPathSegment::Key("body"),
                                    JsonPathSegment::Key("diagnostics"),
                                    JsonPathSegment::Index(0),
                                    JsonPathSegment::Key("text"),
                                ],
                                RecordedLiteral::new("File '").unwrap(),
                                path("src/math.js"),
                                RecordedLiteral::new("' not found.\n  The file is in the program because:\n    Part of 'files' list in tsconfig.json").unwrap(),
                            ),
                            ResponseToken::root_path(
                                vec![
                                    JsonPathSegment::Key("body"),
                                    JsonPathSegment::Key("diagnostics"),
                                    JsonPathSegment::Index(0),
                                    JsonPathSegment::Key("relatedInformation"),
                                    JsonPathSegment::Index(0),
                                    JsonPathSegment::Key("span"),
                                    JsonPathSegment::Key("file"),
                                ],
                                path("jsconfig.json"),
                            ),
                        ],
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDg0DQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoiY29uZmlndXJlIiwicmVxdWVzdF9zZXEiOiIxMSIsInN1Y2Nlc3MiOnRydWV9Cg==",
                        "62c9cf6fd2276d578d2b66a0d9565529cabaca1724c79b5d498b1796cd1e90b4",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(12),
            TsRequest::Format(
                RangeRequest::new(path("jsconfig.json"), point(10, 29), point(10, 48)).unwrap(),
            ),
            peer_moved,
            ApprovedOutput::frames(
                ordinal(12),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDkxDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoiZm9ybWF0IiwicmVxdWVzdF9zZXEiOiIxMiIsInN1Y2Nlc3MiOnRydWUsImJvZHkiOltdfQo=",
                    "0c0cf3abcfe142120582b8d90b860f5e81b663514f417694a855f08bbd4998fb",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new_delayed(
            ordinal(13),
            TsRequest::Reload(ReloadRequest {
                file: path("src/main.js"),
                temporary_file: TideTempFileToken::new(
                    path("src/main.js"),
                    digest("7aa4a05c1e09bab0e7c91d85c52818c5bf862138caa505bdc7f6de35f45c423e"),
                ),
            }),
            config_saved.clone(),
            ApprovedOutput::frames_delayed(
                ordinal(14),
                vec![
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDgxDQoNCnsic2VxIjowLCJ0eXBlIjoicmVzcG9uc2UiLCJjb21tYW5kIjoicmVsb2FkIiwicmVxdWVzdF9zZXEiOiIxMyIsInN1Y2Nlc3MiOnRydWV9Cg==",
                        "f018c6329ae79e703329ff41b8ee509d24efb21e5f233dae9d3244c226757332",
                        Vec::new(),
                    ),
                    decoded_exact_frame(
                        "Q29udGVudC1MZW5ndGg6IDExMg0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6InJlbG9hZCIsInJlcXVlc3Rfc2VxIjoiMTMiLCJzdWNjZXNzIjp0cnVlLCJib2R5Ijp7InJlbG9hZEZpbmlzaGVkIjp0cnVlfX0K",
                        "c114dc9766882943841a291bd70c484968dc193d5ddf6f4a159fd89cc43b5706",
                        Vec::new(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            ordinal(14),
            TsRequest::Format(
                RangeRequest::new(path("src/main.js"), point(1, 27), point(2, 40)).unwrap(),
            ),
            config_saved,
            ApprovedOutput::frames(
                ordinal(14),
                vec![decoded_exact_frame(
                    "Q29udGVudC1MZW5ndGg6IDE1Mw0KDQp7InNlcSI6MCwidHlwZSI6InJlc3BvbnNlIiwiY29tbWFuZCI6ImZvcm1hdCIsInJlcXVlc3Rfc2VxIjoiMTQiLCJzdWNjZXNzIjp0cnVlLCJwZXJmb3JtYW5jZURhdGEiOnsidXBkYXRlR3JhcGhEdXJhdGlvbk1zIjoyMTcuMzE0MTY5MDAwMDAwMX0sImJvZHkiOltdfQo=",
                    "cc4cb2b24651aabf8f27a2386ded5f4df726dbbe2b2848612d7fc5e56b74d399",
                    Vec::new(),
                )],
            )
            .unwrap(),
        )
        .unwrap()
        .into(),
    ]);
    let session = ReplaySession::new(
        exchanges,
        digest("ff76fb2563ae57a7cc6b8e40a3807185c4d5d38d1aafa677a46d3fe4f857fcad"),
        digest("457fd752461207d7808343c2367d49484f803444e3a9d4062377e8b056a15aa5"),
        ReplayTermination::CleanEof,
    )
    .unwrap();
    let replay = TideReplay::new(TideScenario::Rename, fixtures, vec![session]).unwrap();
    materialized_case(
        "cross_file_symbol_and_file_rename",
        replay,
        RENAME_BODY,
        expect![[
            r#"OK (:result (:blank (:input (:prompt "Rename add to: " :initial "add" :final " \11" :result nil :condition (:type error :data ("Invalid name") :message "Invalid name") :minibuffer-history (" \11") :file-name-history nil) :before (:selected (:window-live t :buffer-live t :buffer-name "main.js" :file "src/main.js" :point 170) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 170 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo empty :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 1 :mark nil :mark-active nil :modified nil :coding undecided-unix :undo empty :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function add(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil) :target-buffer nil) :after (:selected (:window-live t :buffer-live t :buffer-name "main.js" :file "src/main.js" :point 170) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 170 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo empty :text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 1 :mark nil :mark-active nil :modified nil :coding undecided-unix :undo empty :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function add(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "ae07cf6aa47c9fac97a9c92d1d5ccf8ac59b04a5995112b14863b37141ad30b4")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil) :target-buffer nil) :saves nil) :symbol (:input (:prompt "Rename add to: " :initial "add" :final "sum界" :result "Renamed 6 occurrences." :condition nil :minibuffer-history ("sum界" " \11") :file-name-history nil) :message "" :saves ((:file "src/math.js" :modified nil :disk-sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078") (:file "src/main.js" :modified nil :disk-sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 172 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 11 :boundaries 0) :text "import { multiply } from \"./math.js\";\nimport { sum界 } from \"./math.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :math (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078"))) :live-target (:input (:prompt "New name: " :initial "[ROOT]/src/math.js" :final "[ROOT]/src/live target.js" :result nil :condition (:type error :data ("A buffer named ’[ROOT]/src/live target.js’ already exists.") :message "A buffer named ’[ROOT]/src/live target.js’ already exists.") :minibuffer-history ("sum界" " \11") :file-name-history ("[ROOT]/src/live target.js")) :before (:selected (:window-live t :buffer-live t :buffer-name "math.js" :file "src/math.js" :point 98) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 172 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 11 :boundaries 0) :text "import { multiply } from \"./math.js\";\nimport { sum界 } from \"./math.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil) :target-buffer (:identity t :name "live target.js" :file "src/live target.js" :mode js-mode :tide-mode nil :point 1 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo empty :text "" :disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil))) :after (:selected (:window-live t :buffer-live t :buffer-name "math.js" :file "src/math.js" :point 98) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 172 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 11 :boundaries 0) :text "import { multiply } from \"./math.js\";\nimport { sum界 } from \"./math.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil) :target-buffer (:identity t :name "live target.js" :file "src/live target.js" :mode js-mode :tide-mode nil :point 1 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo empty :text "" :disk (:file "src/live target.js" :exists nil :symlink nil :sha256 nil))) :same-buffer t) :existing-target (:input (:prompt "New name: " :initial "[ROOT]/src/math.js" :final "[ROOT]/src/existing target.js" :result nil :condition (:type error :data ("A file named ’[ROOT]/src/existing target.js’ already exists.") :message "A file named ’[ROOT]/src/existing target.js’ already exists.") :minibuffer-history ("sum界" " \11") :file-name-history ("[ROOT]/src/existing target.js" "[ROOT]/src/live target.js")) :before (:selected (:window-live t :buffer-live t :buffer-name "math.js" :file "src/math.js" :point 98) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 172 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 11 :boundaries 0) :text "import { multiply } from \"./math.js\";\nimport { sum界 } from \"./math.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/existing target.js" :exists t :symlink nil :sha256 "f552e1ee6261f13793bda2c7517fbf0cbc3388d238eba38c2df8d89e7ead50c2") :target-buffer nil) :after (:selected (:window-live t :buffer-live t :buffer-name "math.js" :file "src/math.js" :point 98) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 172 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 11 :boundaries 0) :text "import { multiply } from \"./math.js\";\nimport { sum界 } from \"./math.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "6603412fe72d5ba3ecea37196b2dc5eb4c4411be445017f1c75424539a868f5b")) :source (:identity t :name "math.js" :file "src/math.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/math.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")) :config-disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "06db7c5eb521a63cd90dfbdf36a7cb6c9e4713d78aace138029bf9666bba416c") :config-buffer nil :target-disk (:file "src/existing target.js" :exists t :symlink nil :sha256 "f552e1ee6261f13793bda2c7517fbf0cbc3388d238eba38c2df8d89e7ead50c2") :target-buffer nil)) :file (:input (:prompt "New name: " :initial "[ROOT]/src/math.js" :final "[ROOT]/src/arithmetic 界.js" :result "Renamed ’math.js’ to ’arithmetic 界.js’." :condition nil :minibuffer-history ("sum界" " \11") :file-name-history ("[ROOT]/src/arithmetic 界.js" "[ROOT]/src/existing target.js" "[ROOT]/src/live target.js")) :message "" :saves ((:file "jsconfig.json" :modified nil :disk-sha256 "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353") (:file "src/main.js" :modified nil :disk-sha256 "7aa4a05c1e09bab0e7c91d85c52818c5bf862138caa505bdc7f6de35f45c423e")) :post-edits ((:file "jsconfig.json" :modified nil :text "{\n  \"compilerOptions\": {\n    \"allowJs\": true,\n    \"checkJs\": true,\n    \"noEmit\": true,\n    \"strict\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"commonjs\"\n  },\n  \"files\": [\"src/main.js\", \"src/arithmetic 界.js\"]\n}\n") (:file "src/main.js" :modified nil :text "import { multiply } from \"./arithmetic 界.js\";\nimport { sum界 } from \"./arithmetic 界.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n")) :old (:file "src/math.js" :exists nil :symlink nil :sha256 nil) :new (:file "src/arithmetic 界.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078") :new-directory (:exists t :symlink nil) :same-buffer t :old-buffer-absent t :renamed-buffer (:identity t :name "arithmetic 界.js" :file "src/arithmetic 界.js" :mode js-mode :tide-mode nil :point 98 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "/**\n * Add two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function sum界(left, right) {\n  return left + right;\n}\n\n/**\n * Multiply two numbers.\n * @param {number} left\n * @param {number} right\n */\nexport function multiply(left, right) {\n  return left * right;\n}\n" :disk (:file "src/arithmetic 界.js" :exists t :symlink nil :sha256 "e46f535bbd15cf16b72182724dc4b269e150c08f5186b7cc7b295b5afcf80078")) :main (:identity t :name "main.js" :file "src/main.js" :mode js-mode :tide-mode t :point 188 :mark nil :mark-active nil :modified nil :coding utf-8-dos :undo (:present t :entries 16 :boundaries 0) :text "import { multiply } from \"./arithmetic 界.js\";\nimport { sum界 } from \"./arithmetic 界.js\";\n\nexport const 界 = sum界(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = sum界(1, 2);\nexport const total=sum界(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return sum界(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :disk (:file "src/main.js" :exists t :symlink nil :sha256 "7aa4a05c1e09bab0e7c91d85c52818c5bf862138caa505bdc7f6de35f45c423e")) :config (:buffer-live t :buffer (:identity t :name "jsconfig.json" :file "jsconfig.json" :mode js-json-mode :tide-mode nil :point 1 :mark nil :mark-active nil :modified nil :coding utf-8-unix :undo (:present t :entries 3 :boundaries 0) :text "{\n  \"compilerOptions\": {\n    \"allowJs\": true,\n    \"checkJs\": true,\n    \"noEmit\": true,\n    \"strict\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"commonjs\"\n  },\n  \"files\": [\"src/main.js\", \"src/arithmetic 界.js\"]\n}\n" :disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353")) :disk (:file "jsconfig.json" :exists t :symlink nil :sha256 "7f071d1675efa60017668aa84eb7ac2d3c0984a73ab1e6332b733c44ba93d353")))) :typed (:scenario rename :fixture-count 3 :session-count 1 :sessions ((:first-ordinal 1 :requests (open configure rename rename reload open configure getEditsForFileRename close open configure format reload format) :request-count 14 :frame-count 17 :request-sha256 "ff76fb2563ae57a7cc6b8e40a3807185c4d5d38d1aafa677a46d3fe4f857fcad" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 3 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "rename") :bytes 904 :sha256 "3386eca93c5be8aad93e27497d2d7fefbd55b76c1cffb6b68087d15ac043c498" :delivery whole-frame))) :json "{\"command\":\"rename\",\"seq\":\"3\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":22}}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 4 :frames ((:kind response :owner (:response 4 "rename") :bytes 904 :sha256 "94459339e2392db41db0541039151875f8abc9207310a0dcdf9d95aab5d66161" :delivery whole-frame))) :json "{\"command\":\"rename\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":22}}") (:ordinal 5 :outcome complete :callback not-registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "reload") :bytes 102 :sha256 "3fa81aca945a7956c2d44e16691bd6b3a794a0c6bb8e1d761c7f8c59165743ff" :delivery whole-frame) (:kind response :owner (:response 5 "reload") :bytes 134 :sha256 "e18255d2573ddf1d26ee426c7a94b0246d1afac36bd28cd71ccf712f87e41fd0" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 6 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"6\",\"arguments\":{\"file\":\"[ROOT]/src/math.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 7 :outcome complete :callback not-registered :output (:delivery-after 8 :frames ((:kind response :owner (:response 7 "configure") :bytes 105 :sha256 "922c0f9501f357adaa0413d5bf61e39d2f5b7c8e81a9ac61d43ab694809f155a" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"7\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/math.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 8 :outcome complete :callback registered :output (:delivery-after 8 :frames ((:kind response :owner (:response 8 "getEditsForFileRename") :bytes 515 :sha256 "b0ea4d03010662536a6fc8254f783e98a0beb9e95c294f47afb2ac8daa3c83e2" :delivery whole-frame))) :json "{\"command\":\"getEditsForFileRename\",\"seq\":\"8\",\"arguments\":{\"oldFilePath\":\"[ROOT]/src/math.js\",\"newFilePath\":\"[ROOT]/src/arithmetic 界.js\",\"file\":\"[ROOT]/src/math.js\"}}") (:ordinal 9 :outcome complete :callback not-registered :output none :json "{\"command\":\"close\",\"seq\":\"9\",\"arguments\":{\"file\":\"[ROOT]/src/math.js\"}}") (:ordinal 10 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"10\",\"arguments\":{\"file\":\"[ROOT]/jsconfig.json\"}}") (:ordinal 11 :outcome complete :callback not-registered :output (:delivery-after 12 :frames ((:kind config-file-diagnostic :owner asynchronous :bytes 551 :sha256 "dda0b539a89781de51cfe5cfe4ba02ae9ebb7a1e34ff404f247d34e7d2405686" :delivery whole-frame) (:kind response :owner (:response 11 "configure") :bytes 106 :sha256 "62c9cf6fd2276d578d2b66a0d9565529cabaca1724c79b5d498b1796cd1e90b4" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"11\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/jsconfig.json\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 12 :outcome complete :callback registered :output (:delivery-after 12 :frames ((:kind response :owner (:response 12 "format") :bytes 113 :sha256 "0c0cf3abcfe142120582b8d90b860f5e81b663514f417694a855f08bbd4998fb" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"12\",\"arguments\":{\"file\":\"[ROOT]/jsconfig.json\",\"line\":10,\"offset\":29,\"endLine\":10,\"endOffset\":48}}") (:ordinal 13 :outcome complete :callback not-registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 13 "reload") :bytes 103 :sha256 "f018c6329ae79e703329ff41b8ee509d24efb21e5f233dae9d3244c226757332" :delivery whole-frame) (:kind response :owner (:response 13 "reload") :bytes 135 :sha256 "c114dc9766882943841a291bd70c484968dc193d5ddf6f4a159fd89cc43b5706" :delivery whole-frame))) :json "{\"command\":\"reload\",\"seq\":\"13\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"tmpfile\":\"[TIDE-TMP]\"}}") (:ordinal 14 :outcome complete :callback registered :output (:delivery-after 14 :frames ((:kind response :owner (:response 14 "format") :bytes 176 :sha256 "cc4cb2b24651aabf8f27a2386ded5f4df726dbbe2b2848612d7fc5e56b74d399" :delivery whole-frame))) :json "{\"command\":\"format\",\"seq\":\"14\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":1,\"offset\":27,\"endLine\":2,\"endOffset\":40}}")) :termination clean-eof))) :launches ((:name "tsserver" :buffer "*tide-server*" :program [ADAPTER] :arguments ([TSSERVER] "--disableAutomaticTypingAcquisition") :cwd [ROOT] :environment-count 23)) :terminals ((:session 1 :status exit :exit 0 :message "finished\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "rename" :callback registered) (:ordinal 4 :command "rename" :callback registered) (:ordinal 5 :command "reload" :callback not-registered) (:ordinal 6 :command "open" :callback not-registered) (:ordinal 7 :command "configure" :callback not-registered) (:ordinal 8 :command "getEditsForFileRename" :callback registered) (:ordinal 9 :command "close" :callback not-registered) (:ordinal 10 :command "open" :callback not-registered) (:ordinal 11 :command "configure" :callback not-registered) (:ordinal 12 :command "format" :callback registered) (:ordinal 13 :command "reload" :callback not-registered) (:ordinal 14 :command "format" :callback registered)) :public-deletes nil :cleanup clean)"#
        ]],
    )
}

const FAILURE_RECOVERY_BODY: &str = r#"(lambda (world)
  (cl-labels
      ((file-sha256
        (file)
        (tide368-test-file-sha256 file))
       (source-state
        (buffer file)
        (with-current-buffer buffer
          (list :text (buffer-substring-no-properties (point-min) (point-max))
                :point (point) :mark (mark t) :mark-active mark-active
                :modified (buffer-modified-p)
                :undo (cond ((eq buffer-undo-list t) 'disabled)
                            ((null buffer-undo-list) 'empty)
                            (t (length buffer-undo-list)))
                :coding buffer-file-coding-system
                :disk-sha256 (file-sha256 file)
                :selected (eq buffer (window-buffer (selected-window)))
                :window-point (and (eq buffer (window-buffer (selected-window)))
                                   (window-point (selected-window))))))
       (normalized-text
        (text root)
        (replace-regexp-in-string
         (regexp-quote (file-name-as-directory root)) "[ROOT]/" text t t))
       (normalized-position
        (buffer position root)
        (with-current-buffer buffer
          (+ (point-min)
             (length
              (normalized-text
               (buffer-substring-no-properties (point-min) position)
               root)))))
       (property-runs
        (buffer property root)
        (with-current-buffer buffer
          (let ((position (point-min)) runs)
            (while (< position (point-max))
              (let* ((value (get-text-property position property))
                     (next (or (next-single-property-change
                                position property nil (point-max))
                               (point-max))))
                (when value
                  (push (list (normalized-position buffer position root)
                              (normalized-position buffer next root)
                              (copy-tree value))
                        runs))
                (setq position next)))
            (nreverse runs))))
       (buffer-state
        (buffer root)
        (unless (buffer-live-p buffer)
          (error "Tide failure-recovery output buffer is missing"))
        (with-current-buffer buffer
          (list :mode major-mode
                :point (normalized-position buffer (point) root)
                :text (normalized-text
                       (buffer-substring-no-properties (point-min) (point-max))
                       root)
                :face-runs (property-runs buffer 'face root))))
       (wait-until
        (predicate process label)
        (let ((deadline (+ (float-time) 20.0)))
          (while (and (not (funcall predicate)) (< (float-time) deadline))
            (accept-process-output process 0.02))
          (unless (funcall predicate)
            (error "Tide failure-recovery wait failed: %S" label))))
       (settled-output
        (buffer process root)
        (let ((sample (buffer-state buffer root)))
          (dotimes (_ 2)
            (accept-process-output process 0.01)
            (unless (equal sample (buffer-state buffer root))
              (error "Tide failure-recovery UI changed after completion")))
          sample))
       (condition-state
        (thunk)
        (condition-case condition
            (list :value (funcall thunk))
          (error (list :condition
                       (list (car condition) (copy-tree (cdr condition))
                             (error-message-string condition)))))))
    (let* ((root (plist-get world :root))
           (main (expand-file-name "src/main.js" root))
           (buffer (find-file-noselect main))
           before healthy-verify healthy-doc failure-verify dead-request
           recovered-verify recovered-doc after
           first-process second-process third-process)
      (switch-to-buffer buffer)
      (js-mode)
      (setq-local tab-width 2 js-indent-level 2)
      (buffer-enable-undo)
      (setq buffer-undo-list nil)
      (goto-char (point-min))
      (search-forward "add(1")
      (backward-char 4)
      (setq before (source-state buffer main))
      (tide-setup)
      (setq first-process (tide368-test-assert-current-server))
      (tide-verify-setup)
      (wait-until (lambda () (get-buffer "*tide-project-info*"))
                  first-process 'healthy-verify)
      (setq healthy-verify
            (settled-output (get-buffer "*tide-project-info*")
                            first-process root))
      (tide-documentation-at-point)
      (wait-until (lambda () (get-buffer "*tide-documentation*"))
                  first-process 'healthy-documentation)
      (setq healthy-doc
            (settled-output (get-buffer "*tide-documentation*")
                            first-process root))
      (unless (equal before (source-state buffer main))
        (error "Tide healthy framing mutated the source state"))
      (tide-restart-server)
      (setq second-process (tide368-test-assert-current-server))
      (when (eq first-process second-process)
        (error "Tide live restart reused the old process"))
      (setq failure-verify (condition-state #'tide-verify-setup))
      (wait-until (lambda () (not (process-live-p second-process)))
                  second-process 'expected-external-exit)
      (unless (equal before (source-state buffer main))
        (error "Tide external exit mutated the source state"))
      (setq dead-request
            (condition-state #'tide-documentation-at-point))
      (unless (and (plist-get dead-request :condition)
                   (string=
                    (caddr (plist-get dead-request :condition))
                    "Server does not exist. Run M-x tide-restart-server to start it again"))
        (error "Tide dead-server public boundary drifted: %S" dead-request))
      (tide-restart-server)
      (setq third-process (tide368-test-assert-current-server))
      (when (or (eq third-process first-process)
                (eq third-process second-process))
        (error "Tide public recovery reused an earlier process"))
      (tide-verify-setup)
      (wait-until
       (lambda ()
         (let ((info (get-buffer "*tide-project-info*")))
           (and info
                (with-current-buffer info
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "5.1.3" nil t))))))
       third-process 'recovered-verify)
      (setq recovered-verify
            (settled-output (get-buffer "*tide-project-info*")
                            third-process root))
      (tide-documentation-at-point)
      (wait-until
       (lambda ()
         (let ((documentation (get-buffer "*tide-documentation*")))
           (and documentation
                (with-current-buffer documentation
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "Add two numbers." nil t))))))
       third-process 'recovered-documentation)
      (setq recovered-doc
            (settled-output (get-buffer "*tide-documentation*")
                            third-process root))
      (unless (and (equal healthy-verify recovered-verify)
                   (equal healthy-doc recovered-doc))
        (error "Tide public recovery did not restore the healthy UI"))
      (setq after (source-state buffer main))
      (unless (equal before after)
        (error "Tide public recovery changed the source state"))
      (tide-kill-server)
      (wait-until (lambda () (not (process-live-p third-process)))
                  third-process 'public-final-kill)
      (list :source (list :before before :after after)
            :healthy (list :verify healthy-verify :documentation healthy-doc)
            :failure (list :verify failure-verify :dead-request dead-request)
            :recovered (list :verify recovered-verify
                             :documentation recovered-doc)
            :processes (list :first-second-distinct
                             (not (eq first-process second-process))
                             :second-third-distinct
                             (not (eq second-process third-process))
                             :all-dead
                             (not (or (process-live-p first-process)
                                      (process-live-p second-process)
                                      (process-live-p third-process))))
            :request-counter tide-request-counter
            :callbacks (hash-table-count tide-response-callbacks)
            :servers (hash-table-count tide-servers)))))"#;

fn framing_process_death_and_public_recovery() -> ParityBatchCase {
    let fixtures = common_manifest();
    let generation = fixtures.generation();

    let first = ReplaySession::new(
        vec![
            RecordedExchange::new(
                ordinal(1),
                TsRequest::Open(
                    OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
                ),
                generation.clone(),
                ApprovedOutput::no_frames(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new_delayed(
                ordinal(2),
                configure_request(),
                generation.clone(),
                ApprovedOutput::frames_delayed(
                    ordinal(3),
                    captured_startup_frames_with(
                        2,
                        "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123",
                        [
                            DeliveryPlan::SplitHeader {
                                at: NonZeroUsize::new(7).unwrap(),
                            },
                            DeliveryPlan::WholeFrame,
                            DeliveryPlan::CoalescedWithNext,
                            DeliveryPlan::WholeFrame,
                            DeliveryPlan::SplitBody {
                                at: NonZeroUsize::new(11).unwrap(),
                            },
                        ],
                    ),
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(3),
                TsRequest::Status,
                generation.clone(),
                ApprovedOutput::frames(
                    ordinal(3),
                    vec![status_response_frame(
                        3,
                        "4c3161826b2a2eeeca691adf7750c5e467869fe0e31bbd1abc6e95a2068118aa",
                        DeliveryPlan::WholeFrame,
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(4),
                TsRequest::ProjectInfo(ProjectInfoRequest {
                    file: path("src/main.js"),
                    file_names: FileNameListRequest::Null,
                }),
                generation.clone(),
                ApprovedOutput::frames(
                    ordinal(4),
                    vec![project_info_response_frame(
                        4,
                        "301b7820d5de76949740aff780c5b81356fc87bbf1652d138e097dbd5dba13ea",
                        DeliveryPlan::WholeFrame,
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(5),
                TsRequest::QuickInfoFull(PointRequest {
                    file: path("src/main.js"),
                    point: point(8, 23),
                }),
                generation.clone(),
                ApprovedOutput::frames(
                    ordinal(5),
                    vec![quickinfo_response_frame(
                        5,
                        "7032f3852c78bbb1655a5a17b3af1a850d7418298abd55bb9c1333a25672abb2",
                        DeliveryPlan::SplitBody {
                            at: NonZeroUsize::new(37).unwrap(),
                        },
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
        ],
        digest("bcff5226d44ed65cdfd5aa28b5c5597fa3055883b730c8f4403eafeea6f7eaec"),
        digest("4c8d1bae06768dc49e5679230de0c19d0e1d2bfe43975c7761f556ce3593d622"),
        ReplayTermination::ClientKilled {
            ready_after: ordinal(5),
        },
    )
    .unwrap();

    let second = ReplaySession::new(
        vec![
            RecordedExchange::new(
                ordinal(6),
                TsRequest::Open(
                    OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
                ),
                generation.clone(),
                ApprovedOutput::no_frames(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new_delayed(
                ordinal(7),
                configure_request(),
                generation.clone(),
                ApprovedOutput::frames_delayed(
                    ordinal(8),
                    captured_startup_frames_with(
                        7,
                        "922c0f9501f357adaa0413d5bf61e39d2f5b7c8e81a9ac61d43ab694809f155a",
                        [DeliveryPlan::WholeFrame; 5],
                    ),
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            TerminalExchange::new(
                ordinal(8),
                TsRequest::Status,
                generation.clone(),
                ApprovedOutput::no_frames(),
            )
            .unwrap()
            .into(),
        ],
        digest("57419f5c523e659fdbd61616f3ef9959453762ae83e0a063a15af254bb650013"),
        digest("96fef9f4c3bd8793d1d42b23f160cc049965890441e7d81246936dd893d081f4"),
        ReplayTermination::ExitAfter {
            request: ordinal(8),
            code: NonZeroI32::new(87).unwrap(),
        },
    )
    .unwrap();

    let third = ReplaySession::new(
        vec![
            RecordedExchange::new(
                ordinal(9),
                TsRequest::Open(
                    OpenRequest::immediate(path("src/main.js"), ScriptKind::JavaScript).unwrap(),
                ),
                generation.clone(),
                ApprovedOutput::no_frames(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new_delayed(
                ordinal(10),
                configure_request(),
                generation.clone(),
                ApprovedOutput::frames_delayed(
                    ordinal(11),
                    captured_startup_frames_with(
                        10,
                        "547cb3a0a10b3e2262133db2aa9c8f4011b5dcdb3eef38ff954832aca6d9cd5d",
                        [DeliveryPlan::WholeFrame; 5],
                    ),
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(11),
                TsRequest::Status,
                generation.clone(),
                ApprovedOutput::frames(
                    ordinal(11),
                    vec![status_response_frame(
                        11,
                        "0f523175400e34f7f61f689693b6aba813c00749a94cdb4162bae5d1465f9df4",
                        DeliveryPlan::WholeFrame,
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(12),
                TsRequest::ProjectInfo(ProjectInfoRequest {
                    file: path("src/main.js"),
                    file_names: FileNameListRequest::Null,
                }),
                generation.clone(),
                ApprovedOutput::frames(
                    ordinal(12),
                    vec![project_info_response_frame(
                        12,
                        "300ee89a4a9be1182bc443a9424d1f42433ad937d027578c8407feaa37a84f66",
                        DeliveryPlan::WholeFrame,
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                ordinal(13),
                TsRequest::QuickInfoFull(PointRequest {
                    file: path("src/main.js"),
                    point: point(8, 23),
                }),
                generation,
                ApprovedOutput::frames(
                    ordinal(13),
                    vec![quickinfo_response_frame(
                        13,
                        "c990a0b82d8177cc7b72299c292da3738e8d005d4530daa2b4761dea07fa4214",
                        DeliveryPlan::WholeFrame,
                    )],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
        ],
        digest("4166a762c383f6bed980d40a6f230576327730cf05c075000885fbff44dd5de6"),
        digest("4a7b312baa0cb44f2a004f971196b8d1d454132bd63001190d3f06bd6ba5ee52"),
        ReplayTermination::ClientKilled {
            ready_after: ordinal(13),
        },
    )
    .unwrap();

    let replay = TideReplay::new(
        TideScenario::FailureRecovery,
        fixtures,
        vec![first, second, third],
    )
    .unwrap();
    materialized_case(
        "framing_process_death_and_public_recovery",
        replay,
        FAILURE_RECOVERY_BODY,
        expect![[
            r#"OK (:result (:source (:before (:text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :point 171 :mark nil :mark-active nil :modified nil :undo empty :coding utf-8-dos :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7" :selected t :window-point 171) :after (:text "import { multiply } from \"./math.js\";\nimport { add } from \"./math.js\";\n\nexport const 界 = add(3, 4);\nexport const tabbed =\11界;\n\n/** @type {string} */\nexport const label = add(1, 2);\nexport const total=add(1,2)\n\nexport class Calculator {\n  /** @param {number} left @param {number} right */\n  sum(left, right){return add(left,right)}\n}\n\n/** @param {number} value */\nexport function describe(value){return `total=${value}`}\n" :point 171 :mark nil :mark-active nil :modified nil :undo empty :coding utf-8-dos :disk-sha256 "da3803e73eb1417e6b143f28cf68c25baa1bb50ced48781f62651b53c88051c7" :selected t :window-point 171)) :healthy (:verify (:mode special-mode :point 64 :text "tsserver version: 5.1.3\n\nconfig file path: [ROOT]/jsconfig.json" :face-runs ((19 24 (success bold)) (44 64 success))) :documentation (:mode fundamental-mode :point 1 :text "(alias) add(left: number, right: number): number\nimport add\n\nAdd two numbers.\n\n@param left\n@param right\n" :face-runs ((9 12 font-lock-type-face) (19 25 font-lock-keyword-face) (34 40 font-lock-keyword-face) (43 49 font-lock-keyword-face) (50 56 font-lock-keyword-face) (57 60 font-lock-type-face) (80 86 font-lock-keyword-face) (92 98 font-lock-keyword-face)))) :failure (:verify (:value nil) :dead-request (:condition (error ("Server does not exist. Run M-x tide-restart-server to start it again") "Server does not exist. Run M-x tide-restart-server to start it again"))) :recovered (:verify (:mode special-mode :point 64 :text "tsserver version: 5.1.3\n\nconfig file path: [ROOT]/jsconfig.json" :face-runs ((19 24 (success bold)) (44 64 success))) :documentation (:mode fundamental-mode :point 1 :text "(alias) add(left: number, right: number): number\nimport add\n\nAdd two numbers.\n\n@param left\n@param right\n" :face-runs ((9 12 font-lock-type-face) (19 25 font-lock-keyword-face) (34 40 font-lock-keyword-face) (43 49 font-lock-keyword-face) (50 56 font-lock-keyword-face) (57 60 font-lock-type-face) (80 86 font-lock-keyword-face) (92 98 font-lock-keyword-face)))) :processes (:first-second-distinct t :second-third-distinct t :all-dead t) :request-counter 13 :callbacks 0 :servers 0) :typed (:scenario failure-recovery :fixture-count 3 :session-count 3 :sessions ((:first-ordinal 1 :requests (open configure status projectInfo quickinfo-full) :request-count 5 :frame-count 8 :request-sha256 "bcff5226d44ed65cdfd5aa28b5c5597fa3055883b730c8f4403eafeea6f7eaec" :recordings ((:ordinal 1 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"1\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 2 :outcome complete :callback not-registered :output (:delivery-after 3 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery (:split-header 7)) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery coalesced-with-next) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 2 "configure") :bytes 105 :sha256 "e402fa662bd9f543bcac1abc8f5c913af23e5c8bcb6c79cc5bf3e66c0ecb4123" :delivery (:split-body 11)))) :json "{\"command\":\"configure\",\"seq\":\"2\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 3 :outcome complete :callback registered :output (:delivery-after 3 :frames ((:kind response :owner (:response 3 "status") :bytes 130 :sha256 "4c3161826b2a2eeeca691adf7750c5e467869fe0e31bbd1abc6e95a2068118aa" :delivery whole-frame))) :json "{\"command\":\"status\",\"seq\":\"3\",\"arguments\":null}") (:ordinal 4 :outcome complete :callback registered :output (:delivery-after 4 :frames ((:kind response :owner (:response 4 "projectInfo") :bytes 189 :sha256 "301b7820d5de76949740aff780c5b81356fc87bbf1652d138e097dbd5dba13ea" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"4\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":null}}") (:ordinal 5 :outcome complete :callback registered :output (:delivery-after 5 :frames ((:kind response :owner (:response 5 "quickinfo-full") :bytes 1137 :sha256 "7032f3852c78bbb1655a5a17b3af1a850d7418298abd55bb9c1333a25672abb2" :delivery (:split-body 37)))) :json "{\"command\":\"quickinfo-full\",\"seq\":\"5\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":23}}")) :termination (:client-killed :ready-after 5)) (:first-ordinal 6 :requests (open configure status) :request-count 3 :frame-count 5 :request-sha256 "57419f5c523e659fdbd61616f3ef9959453762ae83e0a063a15af254bb650013" :recordings ((:ordinal 6 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"6\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 7 :outcome complete :callback not-registered :output (:delivery-after 8 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 7 "configure") :bytes 105 :sha256 "922c0f9501f357adaa0413d5bf61e39d2f5b7c8e81a9ac61d43ab694809f155a" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"7\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 8 :outcome external-exit-before-completion :callback registered :output none :json "{\"command\":\"status\",\"seq\":\"8\",\"arguments\":null}")) :termination (:exit-after 8 :code 87)) (:first-ordinal 9 :requests (open configure status projectInfo quickinfo-full) :request-count 5 :frame-count 8 :request-sha256 "4166a762c383f6bed980d40a6f230576327730cf05c075000885fbff44dd5de6" :recordings ((:ordinal 9 :outcome complete :callback not-registered :output none :json "{\"command\":\"open\",\"seq\":\"9\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"scriptKindName\":\"JS\"}}") (:ordinal 10 :outcome complete :callback not-registered :output (:delivery-after 11 :frames ((:kind project-loading-start :owner asynchronous :bytes 203 :sha256 "7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6" :delivery whole-frame) (:kind project-loading-finish :owner asynchronous :bytes 125 :sha256 "cc87b74ec4f7b697d792f7a5beacaffe3c6592d6290d7d3c7b1c7ed12f9562d2" :delivery whole-frame) (:kind telemetry :owner asynchronous :bytes 754 :sha256 "8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce" :delivery whole-frame) (:kind config-file-diagnostic :owner asynchronous :bytes 170 :sha256 "584f742b4aeec6d9da05e7660bf8b1a26875ac049df1ba3f9b08717225fd29c8" :delivery whole-frame) (:kind response :owner (:response 10 "configure") :bytes 106 :sha256 "547cb3a0a10b3e2262133db2aa9c8f4011b5dcdb3eef38ff954832aca6d9cd5d" :delivery whole-frame))) :json "{\"command\":\"configure\",\"seq\":\"10\",\"arguments\":{\"hostInfo\":\"[HOSTINFO]\",\"file\":\"[ROOT]/src/main.js\",\"formatOptions\":{\"tabSize\":2,\"indentSize\":2},\"preferences\":{\"includeCompletionsForModuleExports\":true,\"includeCompletionsWithInsertText\":true,\"allowTextChangesInNewFiles\":true,\"generateReturnInDocTemplate\":true}}}") (:ordinal 11 :outcome complete :callback registered :output (:delivery-after 11 :frames ((:kind response :owner (:response 11 "status") :bytes 131 :sha256 "0f523175400e34f7f61f689693b6aba813c00749a94cdb4162bae5d1465f9df4" :delivery whole-frame))) :json "{\"command\":\"status\",\"seq\":\"11\",\"arguments\":null}") (:ordinal 12 :outcome complete :callback registered :output (:delivery-after 12 :frames ((:kind response :owner (:response 12 "projectInfo") :bytes 190 :sha256 "300ee89a4a9be1182bc443a9424d1f42433ad937d027578c8407feaa37a84f66" :delivery whole-frame))) :json "{\"command\":\"projectInfo\",\"seq\":\"12\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"needFileNameList\":null}}") (:ordinal 13 :outcome complete :callback registered :output (:delivery-after 13 :frames ((:kind response :owner (:response 13 "quickinfo-full") :bytes 1138 :sha256 "c990a0b82d8177cc7b72299c292da3738e8d005d4530daa2b4761dea07fa4214" :delivery whole-frame))) :json "{\"command\":\"quickinfo-full\",\"seq\":\"13\",\"arguments\":{\"file\":\"[ROOT]/src/main.js\",\"line\":8,\"offset\":23}}")) :termination (:client-killed :ready-after 13)))) :launches ((:name "tsserver" :buffer "*tide-server*" :program #1=[ADAPTER] :arguments (#2=[TSSERVER] "--disableAutomaticTypingAcquisition") :cwd #3=[ROOT] :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #1# :arguments (#2# "--disableAutomaticTypingAcquisition") :cwd #3# :environment-count 23) (:name "tsserver" :buffer "*tide-server*" :program #1# :arguments (#2# "--disableAutomaticTypingAcquisition") :cwd #3# :environment-count 23)) :terminals ((:session 1 :status signal :exit 9 :message "killed\n" :stderr "\n") (:session 2 :status exit :exit 87 :message "exited abnormally with code 87\n" :stderr "\nTIDE368 expected external exit\n") (:session 3 :status signal :exit 9 :message "killed\n" :stderr "\n")) :callbacks ((:ordinal 1 :command "open" :callback not-registered) (:ordinal 2 :command "configure" :callback not-registered) (:ordinal 3 :command "status" :callback registered) (:ordinal 4 :command "projectInfo" :callback registered) (:ordinal 5 :command "quickinfo-full" :callback registered) (:ordinal 6 :command "open" :callback not-registered) (:ordinal 7 :command "configure" :callback not-registered) (:ordinal 8 :command "status" :callback registered) (:ordinal 9 :command "open" :callback not-registered) (:ordinal 10 :command "configure" :callback not-registered) (:ordinal 11 :command "status" :callback registered) (:ordinal 12 :command "projectInfo" :callback registered) (:ordinal 13 :command "quickinfo-full" :callback registered)) :public-deletes ((:session 1 :route restart-server) (:session 3 :route kill-server)) :cleanup clean)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_verify_list_kill_and_restart(),
        documentation_imenu_definition_back_and_named_navigation(),
        references_ui_and_async_identifier_highlight(),
        flycheck_diagnostics_and_project_errors(),
        format_organize_jsdoc_and_undo(),
        cross_file_symbol_and_file_rename(),
        framing_process_death_and_public_recovery(),
    ]
}
