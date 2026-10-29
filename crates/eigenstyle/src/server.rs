//! The rmcp server: tool surface and handler dispatch.
//!
//! Domain logic does not move in the WS1 migration — the six tool
//! handlers below are the hand-rolled server's, verbatim in behavior
//! (same strict argument validation, same payload construction, same
//! failure taxonomy): params failures return as protocol errors
//! (`-32602`), domain failures as `isError` content carrying the core
//! error's stable code. Only the framing changes sides of the seam:
//! rmcp owns the envelope, the era guard (see `guard.rs`) owns the
//! stdio front door, and this type owns the tools.
//!
//! Modern-only posture lives in two places: the era guard refuses
//! `initialize` before the SDK sees it, and
//! [`ServerHandler::supported_protocol_versions`] pins this handler
//! to 2026-07-28, so `server/discover` advertises exactly one version
//! and the SDK validates every request's `_meta` against it.

use std::borrow::Cow;
use std::io::Read;
use std::sync::{Arc, Mutex};

use eigenstyle_core::corpus::{Authorship, NewSample};
use eigenstyle_core::{
    DRAFT_TEXT_MAX_BYTES, EXEMPLAR_MAX_COUNT, SAMPLE_TEXT_MAX_BYTES, Store, VoiceError, check_draft,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Map, Value, json};

/// Maximum bytes read from a file argument.
const MAX_FILE_BYTES: usize = SAMPLE_TEXT_MAX_BYTES;
/// Instructions reported for the server (unchanged from the baseline).
const INSTRUCTIONS: &str = "Matt Porter's voice profile: corpus of his writing, measured stylometric \
    fingerprint, deterministic draft checker. Ingesting never recomputes the fingerprint; recompute is \
    explicit (dry_run previews the induced changes first).";

fn string_prop(max_len: usize) -> Value {
    json!({"type": "string", "maxLength": max_len})
}

/// The tool catalog: names, descriptions, and input schemas are the
/// baseline's, byte-for-byte — rmcp accepts raw schemas, so no
/// schemars translation stands between the baseline and the wire.
fn tool_definitions() -> Vec<Tool> {
    let specs: Vec<(&str, &str, Value)> = vec![
        (
            "check_draft",
            "Score a draft against the fingerprint: verdict (CLOSE/DRIFTING/OFF-VOICE/\
                UNRELIABLE), delta, top contributing features, avoid-list hits, and — when \
                source_text is supplied — dropped numbers/URLs vs the source.",
            json!({
                "type": "object",
                "properties": {
                    "text": string_prop(DRAFT_TEXT_MAX_BYTES),
                    "file_path": string_prop(4096),
                    "source_text": string_prop(DRAFT_TEXT_MAX_BYTES),
                    "source_file_path": string_prop(4096)
                },
                "additionalProperties": false
            }),
        ),
        (
            "get_exemplars",
            "Verbatim exemplar chunks (paragraphs) from included corpus samples of one genre.",
            json!({
                "type": "object",
                "properties": {
                    "genre": string_prop(40),
                    "limit": {"type": "integer", "minimum": 1, "maximum": EXEMPLAR_MAX_COUNT}
                },
                "required": ["genre"],
                "additionalProperties": false
            }),
        ),
        (
            "get_profile",
            "The readable voice profile plus the fingerprint summary (effective values, \
                user-corrected flags), bands, and avoid list.",
            json!({"type": "object", "properties": {}, "additionalProperties": false}),
        ),
        (
            "ingest_sample",
            "Add a writing sample to the corpus (exactly one of text / file_path / url). \
                Requires title, genre, and authorship (sole|blended) — attribution is never defaulted. \
                Does NOT recompute the fingerprint.",
            json!({
                "type": "object",
                "properties": {
                    "title": string_prop(200),
                    "genre": string_prop(40),
                    "authorship": {"type": "string", "enum": ["sole", "blended"]},
                    "source": string_prop(500),
                    "text": string_prop(SAMPLE_TEXT_MAX_BYTES),
                    "file_path": string_prop(4096),
                    "url": string_prop(2048)
                },
                "required": ["title", "genre", "authorship"],
                "additionalProperties": false
            }),
        ),
        (
            "list_corpus",
            "Manifest view of the corpus: every sample's metadata, word count, and \
                included flag, plus totals.",
            json!({"type": "object", "properties": {}, "additionalProperties": false}),
        ),
        (
            "recompute_fingerprint",
            "Recompute the fingerprint from included samples and re-derive the verdict \
                bands. dry_run=true previews the induced changes (feature deltas, band shifts) \
                without saving — review and correct before applying.",
            json!({
                "type": "object",
                "properties": {
                    "dry_run": {"type": "boolean"}
                },
                "additionalProperties": false
            }),
        ),
    ];
    specs
        .into_iter()
        .map(|(name, description, schema)| {
            let Value::Object(map) = schema else {
                unreachable!("catalog schemas are objects");
            };
            Tool::new(name, description, map)
        })
        .collect()
}

/// The server handler: one store behind a mutex (the baseline served
/// sequentially; concurrent calls serialize here, and the blocking
/// dispatch runs on a blocking thread so a slow URL fetch cannot
/// stall the runtime).
pub struct EigenstyleServer {
    store: Arc<Mutex<Store>>,
}

impl EigenstyleServer {
    pub fn new(store: Store) -> Self {
        EigenstyleServer {
            store: Arc::new(Mutex::new(store)),
        }
    }
}

impl ServerHandler for EigenstyleServer {
    fn get_info(&self) -> InitializeResult {
        // Both model types are non-exhaustive: start from the
        // default and set exactly the fields this server reports.
        let mut server_info = Implementation::default();
        server_info.name = "eigenstyle".to_string();
        server_info.version = env!("CARGO_PKG_VERSION").to_string();
        let mut info = InitializeResult::default();
        info.protocol_version = ProtocolVersion::V_2026_07_28;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = server_info;
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tool_definitions(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let store = Arc::clone(&self.store);
        // The handlers are the baseline's synchronous code (one even
        // fetches URLs); run them off the async runtime, as the
        // baseline's sequential loop effectively did.
        let outcome = tokio::task::spawn_blocking(move || {
            let mut guard = store.lock().expect("store mutex poisoned");
            dispatch(&mut guard, &name, &arguments)
        })
        .await;
        match outcome {
            Ok(Ok(payload)) => Ok(CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&payload).unwrap_or_default(),
            )])
            .into()),
            Ok(Err(failure)) if failure.is_params => {
                Err(ErrorData::invalid_params(failure.message, None))
            }
            Ok(Err(failure)) => Ok(CallToolResult::error(vec![ContentBlock::text(
                serde_json::to_string(&json!({
                    "error": {"code": failure.code, "message": failure.message}
                }))
                .unwrap_or_default(),
            )])
            .into()),
            Err(join) => Err(ErrorData::internal_error(
                format!("tool task failed: {join}"),
                None,
            )),
        }
    }
}

/// Route one call to its handler (the baseline's dispatch, unchanged).
fn dispatch(store: &mut Store, name: &str, arguments: &Value) -> Result<Value, ToolFailure> {
    match name {
        "check_draft" => tool_check_draft(store, arguments),
        "get_exemplars" => tool_get_exemplars(store, arguments),
        "get_profile" => tool_get_profile(store, arguments),
        "ingest_sample" => tool_ingest_sample(store, arguments),
        "list_corpus" => tool_list_corpus(store, arguments),
        "recompute_fingerprint" => tool_recompute_fingerprint(store, arguments),
        "" => Err(ToolFailure::params("tools/call requires a tool name")),
        other => Err(ToolFailure::params(format!("unknown tool: {other}"))),
    }
}

fn tool_check_draft(store: &Store, args: &Value) -> Result<Value, ToolFailure> {
    let args = strict_object(
        args,
        &["text", "file_path", "source_text", "source_file_path"],
    )?;
    let draft = read_text_source(&args, "text", "file_path")?;
    let source = if args.contains_key("source_text") || args.contains_key("source_file_path") {
        Some(read_text_source(&args, "source_text", "source_file_path")?)
    } else {
        None
    };
    let report = check_draft(
        &store.fingerprint,
        &store.avoid_list,
        &draft,
        source.as_deref(),
    )?;
    Ok(serde_json::to_value(&report)?)
}

fn tool_get_exemplars(store: &Store, args: &Value) -> Result<Value, ToolFailure> {
    let args = strict_object(args, &["genre", "limit"])?;
    let genre = required_str(&args, "genre")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(EXEMPLAR_MAX_COUNT);
    if limit == 0 || limit > EXEMPLAR_MAX_COUNT {
        return Err(ToolFailure::params(format!(
            "limit must be 1..={EXEMPLAR_MAX_COUNT}"
        )));
    }
    let exemplars = store.corpus.exemplars(genre, limit);
    Ok(json!({
        "genre": genre,
        "count": exemplars.len(),
        "exemplars": serde_json::to_value(&exemplars)?
    }))
}

fn tool_get_profile(store: &Store, args: &Value) -> Result<Value, ToolFailure> {
    let _ = strict_object(args, &[])?;
    let doc = &store.fingerprint;
    let effective = doc.effective_reference();
    let features: Vec<Value> = doc
        .features
        .iter()
        .map(|f| {
            let stat = effective
                .get(f)
                .copied()
                .unwrap_or(eigenstyle_core::fingerprint::RefStat { mean: 0.0, sd: 0.0 });
            json!({
                "name": f,
                "mean": stat.mean,
                "sd": stat.sd,
                "user_corrected": doc.is_corrected(f)
            })
        })
        .collect();
    Ok(json!({
        "profile_markdown": store.profile_markdown,
        "fingerprint": {
            "features": features,
            "bands": {"close_max": doc.bands.close_max, "drifting_max": doc.bands.drifting_max},
            "calibration_method": doc.calibration.method,
            "corpus_pieces": doc.corpus_pieces
        },
        "avoid_list": store.avoid_list,
        "corpus": {
            "samples": store.corpus.samples().len(),
            "included_words": store.corpus.included_word_count()
        }
    }))
}

fn tool_ingest_sample(store: &mut Store, args: &Value) -> Result<Value, ToolFailure> {
    let args = strict_object(
        args,
        &[
            "title",
            "genre",
            "authorship",
            "source",
            "text",
            "file_path",
            "url",
        ],
    )?;
    let title = required_str(&args, "title")?.to_string();
    let genre = required_str(&args, "genre")?.to_string();
    let authorship = Authorship::parse(required_str(&args, "authorship")?)?;
    let source_note = args.get("source").and_then(Value::as_str).unwrap_or("");
    let (text, derived_source) = read_sample_text(&args)?;
    let source = if source_note.is_empty() {
        derived_source
    } else {
        source_note.to_string()
    };
    let sample = store.corpus.add_sample(NewSample {
        title,
        genre,
        authorship,
        source,
        text,
    })?;
    store.save_corpus()?;
    Ok(json!({
        "sample": manifest_row(&sample),
        "corpus_included_words": store.corpus.included_word_count(),
        "fingerprint_recomputed": false,
        "note": "Fingerprint unchanged. Call recompute_fingerprint (dry_run first) to fold this sample in."
    }))
}

fn tool_list_corpus(store: &Store, args: &Value) -> Result<Value, ToolFailure> {
    let _ = strict_object(args, &[])?;
    let rows: Vec<Value> = store.corpus.samples().iter().map(manifest_row).collect();
    Ok(json!({
        "samples": rows,
        "sample_count": store.corpus.samples().len(),
        "included_words": store.corpus.included_word_count(),
        "total_words": store.corpus.total_word_count()
    }))
}

fn tool_recompute_fingerprint(store: &mut Store, args: &Value) -> Result<Value, ToolFailure> {
    let args = strict_object(args, &["dry_run"])?;
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (doc, deltas, old_bands, new_bands) = store.recompute(dry_run)?;
    let changed: Vec<Value> = deltas
        .iter()
        .map(|d| {
            json!({
                "feature": d.feature,
                "old_mean": d.old_mean, "new_mean": d.new_mean,
                "old_sd": d.old_sd, "new_sd": d.new_sd
            })
        })
        .collect();
    Ok(json!({
        "dry_run": dry_run,
        "saved": !dry_run,
        "piece_count": doc.corpus_pieces.len(),
        "bands_before": {"close_max": old_bands.close_max, "drifting_max": old_bands.drifting_max},
        "bands_after": {"close_max": new_bands.close_max, "drifting_max": new_bands.drifting_max},
        "changed_features": changed,
        "loo_deltas": doc.calibration.loo_deltas
    }))
}

/// Tool failure: either a protocol-level params error (-32602) or a
/// domain failure returned as isError content with the core code.
struct ToolFailure {
    is_params: bool,
    code: &'static str,
    message: String,
}

impl ToolFailure {
    fn params(message: impl Into<String>) -> Self {
        ToolFailure {
            is_params: true,
            code: "invalid_params",
            message: message.into(),
        }
    }
}

impl From<VoiceError> for ToolFailure {
    fn from(e: VoiceError) -> Self {
        ToolFailure {
            is_params: false,
            code: e.code(),
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for ToolFailure {
    fn from(e: serde_json::Error) -> Self {
        ToolFailure {
            is_params: false,
            code: "json",
            message: e.to_string(),
        }
    }
}

impl From<std::io::Error> for ToolFailure {
    fn from(e: std::io::Error) -> Self {
        ToolFailure {
            is_params: false,
            code: "io",
            message: e.to_string(),
        }
    }
}

/// Validate that `args` is an object and reject unknown keys.
fn strict_object(args: &Value, allowed: &[&str]) -> Result<Map<String, Value>, ToolFailure> {
    let object = args
        .as_object()
        .ok_or_else(|| ToolFailure::params("arguments must be a JSON object"))?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ToolFailure::params(format!("unknown argument: {key}")));
        }
    }
    Ok(object.clone())
}

fn required_str<'a>(args: &'a Map<String, Value>, field: &str) -> Result<&'a str, ToolFailure> {
    args.get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolFailure::params(format!("missing or non-string argument: {field}")))
}

/// Read exactly one of (text, file_path) from the args.
fn read_text_source(
    args: &Map<String, Value>,
    text_field: &str,
    path_field: &str,
) -> Result<String, ToolFailure> {
    match (args.get(text_field), args.get(path_field)) {
        (Some(Value::String(text)), None) => Ok(text.clone()),
        (None, Some(Value::String(path))) => Ok(read_bounded_file(path)?),
        _ => Err(ToolFailure::params(format!(
            "supply exactly one of {text_field} / {path_field}"
        ))),
    }
}

/// Resolve the sample body for ingest: exactly one of text / file_path / url.
fn read_sample_text(args: &Map<String, Value>) -> Result<(String, String), ToolFailure> {
    let present = ["text", "file_path", "url"]
        .iter()
        .filter(|f| args.get(**f).and_then(Value::as_str).is_some())
        .count();
    if present != 1 {
        return Err(ToolFailure::params(
            "supply exactly one of text / file_path / url",
        ));
    }
    if let Some(text) = args.get("text").and_then(Value::as_str) {
        return Ok((text.to_string(), "inline text".to_string()));
    }
    if let Some(path) = args.get("file_path").and_then(Value::as_str) {
        return Ok((read_bounded_file(path)?, format!("file: {path}")));
    }
    let url = args.get("url").and_then(Value::as_str).unwrap_or("");
    Ok((
        eigenstyle_core::fetch::fetch_url_text(url)?,
        format!("url: {url}"),
    ))
}

fn read_bounded_file(path: &str) -> Result<String, ToolFailure> {
    let file = std::fs::File::open(path).map_err(|e| ToolFailure::from(VoiceError::Io(e)))?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_FILE_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|e| ToolFailure::from(VoiceError::Io(e)))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ToolFailure::from(VoiceError::invalid(
            "file_path",
            format!("file exceeds {MAX_FILE_BYTES} bytes"),
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| ToolFailure::from(VoiceError::invalid("file_path", "file is not valid UTF-8")))
}

fn manifest_row(sample: &eigenstyle_core::Sample) -> Value {
    json!({
        "id": sample.id,
        "title": sample.title,
        "genre": sample.genre,
        "authorship": sample.authorship.as_str(),
        "source": sample.source,
        "words": sample.word_count(),
        "included": sample.included
    })
}
