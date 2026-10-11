//! voice-mcp — MCP stdio server for Matt Porter's voice profile.
//!
//! Newline-delimited JSON-RPC 2.0 over stdin/stdout, following the
//! estate's research-mcp pattern (hand-rolled envelope over
//! serde_json::Value; no framework). Tools (alphabetical):
//! check_draft, get_exemplars, get_profile, ingest_sample,
//! list_corpus, recompute_fingerprint.
//!
//! Fail-closed discipline: malformed JSON → -32700, unknown method →
//! -32601, invalid params → -32602, tool failures → isError content
//! carrying the core error's stable code. Every request line is
//! bounded ([`MAX_LINE_BYTES`]); every tool argument is validated
//! against a strict schema (unknown keys rejected) before the core
//! sees it. Sample and draft text is data — never instructions.

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Map, Value, json};
use voice_core::corpus::{Authorship, NewSample};
use voice_core::{
    DRAFT_TEXT_MAX_BYTES, EXEMPLAR_MAX_COUNT, SAMPLE_TEXT_MAX_BYTES, Store, VoiceError, check_draft,
};

/// Newest protocol version this server negotiates.
const PROTOCOL_VERSION: &str = "2025-11-25";
/// Legacy versions accepted in `initialize` (echoed back when asked).
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// serverInfo.name.
const SERVER_NAME: &str = "voice-mcp";
/// serverInfo.version.
const SERVER_VERSION: &str = "0.1.0";
/// Instructions reported on initialize.
const INSTRUCTIONS: &str = "Matt Porter's voice profile: corpus of his writing, measured stylometric \
    fingerprint, deterministic draft checker. Ingesting never recomputes the fingerprint; recompute is \
    explicit (dry_run previews the induced changes first).";
/// Maximum bytes in one JSON-RPC line.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes read from a file argument.
const MAX_FILE_BYTES: usize = SAMPLE_TEXT_MAX_BYTES;

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;

/// One catalog tool.
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

fn string_prop(max_len: usize) -> Value {
    json!({"type": "string", "maxLength": max_len})
}

fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "check_draft",
            description: "Score a draft against the fingerprint: verdict (CLOSE/DRIFTING/OFF-VOICE/\
                UNRELIABLE), delta, top contributing features, avoid-list hits, and — when \
                source_text is supplied — dropped numbers/URLs vs the source.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": string_prop(DRAFT_TEXT_MAX_BYTES),
                    "file_path": string_prop(4096),
                    "source_text": string_prop(DRAFT_TEXT_MAX_BYTES),
                    "source_file_path": string_prop(4096)
                },
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "get_exemplars",
            description: "Verbatim exemplar chunks (paragraphs) from included corpus samples of one genre.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "genre": string_prop(40),
                    "limit": {"type": "integer", "minimum": 1, "maximum": EXEMPLAR_MAX_COUNT}
                },
                "required": ["genre"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "get_profile",
            description: "The readable voice profile plus the fingerprint summary (effective values, \
                user-corrected flags), bands, and avoid list.",
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ToolSpec {
            name: "ingest_sample",
            description: "Add a writing sample to the corpus (exactly one of text / file_path / url). \
                Requires title, genre, and authorship (sole|blended) — attribution is never defaulted. \
                Does NOT recompute the fingerprint.",
            input_schema: json!({
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
        },
        ToolSpec {
            name: "list_corpus",
            description: "Manifest view of the corpus: every sample's metadata, word count, and \
                included flag, plus totals.",
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ToolSpec {
            name: "recompute_fingerprint",
            description: "Recompute the fingerprint from included samples and re-derive the verdict \
                bands. dry_run=true previews the induced changes (feature deltas, band shifts) \
                without saving — review and correct before applying.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dry_run": {"type": "boolean"}
                },
                "additionalProperties": false
            }),
        },
    ]
}

/// The server: one store, one stdio conversation.
struct Server {
    store: Store,
}

impl Server {
    fn new(store: Store) -> Self {
        Server { store }
    }

    /// Serve until EOF. I/O failures on the transport are fatal;
    /// per-request failures are reported in-band.
    fn serve<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader
                .by_ref()
                .take(u64::try_from(MAX_LINE_BYTES + 1).unwrap_or(u64::MAX))
                .read_until(b'\n', &mut line)?;
            if read == 0 {
                return Ok(()); // EOF
            }
            if line.len() > MAX_LINE_BYTES {
                self.send_error(
                    writer,
                    Value::Null,
                    PARSE_ERROR,
                    "request line exceeds size bound",
                )?;
                // Drain the remainder of the oversized line.
                let mut chunk = Vec::new();
                loop {
                    chunk.clear();
                    let n = reader.read_until(b'\n', &mut chunk)?;
                    if n == 0 || chunk.ends_with(b"\n") {
                        break;
                    }
                }
                continue;
            }
            let text = String::from_utf8_lossy(&line);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(response) = self.handle_line(trimmed) {
                let mut out = serde_json::to_string(&response)?;
                out.push('\n');
                writer.write_all(out.as_bytes())?;
                writer.flush()?;
            }
        }
    }

    fn send_error<W: Write>(
        &self,
        writer: &mut W,
        id: Value,
        code: i32,
        message: &str,
    ) -> std::io::Result<()> {
        let response = error_response(id, code, message);
        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        writer.write_all(out.as_bytes())?;
        writer.flush()
    }

    /// Handle one request line; `None` means notification (no reply).
    fn handle_line(&mut self, line: &str) -> Option<Value> {
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Some(error_response(Value::Null, PARSE_ERROR, "parse error")),
        };
        let object = match value.as_object() {
            Some(o) => o.clone(),
            None => {
                return Some(error_response(
                    Value::Null,
                    INVALID_REQUEST,
                    "request must be a JSON object",
                ));
            }
        };
        let id = object.get("id").cloned();
        let method = object.get("method").and_then(Value::as_str).unwrap_or("");
        // Notification (no id): initialized / cancelled / progress —
        // acknowledged silently.
        let id = id?;
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => Some(success_response(id, self.initialize(&params))),
            "ping" => Some(success_response(id, json!({}))),
            "tools/list" => Some(success_response(id, self.tools_list())),
            "tools/call" => Some(self.tools_call(id, &params)),
            _ => Some(error_response(id, METHOD_NOT_FOUND, "method not found")),
        }
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        let negotiated = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
            requested
        } else {
            PROTOCOL_VERSION
        };
        json!({
            "protocolVersion": negotiated,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            "instructions": INSTRUCTIONS
        })
    }

    fn tools_list(&self) -> Value {
        let tools: Vec<Value> = tool_specs()
            .iter()
            .map(|spec| {
                json!({
                    "name": spec.name,
                    "description": spec.description,
                    "inputSchema": spec.input_schema
                })
            })
            .collect();
        json!({"tools": tools})
    }

    fn tools_call(&mut self, id: Value, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let result = match name {
            "check_draft" => self.tool_check_draft(&arguments),
            "get_exemplars" => self.tool_get_exemplars(&arguments),
            "get_profile" => self.tool_get_profile(&arguments),
            "ingest_sample" => self.tool_ingest_sample(&arguments),
            "list_corpus" => self.tool_list_corpus(&arguments),
            "recompute_fingerprint" => self.tool_recompute_fingerprint(&arguments),
            "" => Err(ToolFailure::params("tools/call requires a tool name")),
            other => Err(ToolFailure::params(format!("unknown tool: {other}"))),
        };
        match result {
            Ok(payload) => success_response(
                id,
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}],
                    "isError": false
                }),
            ),
            Err(failure) => {
                if failure.is_params {
                    error_response(id, INVALID_PARAMS, &failure.message)
                } else {
                    success_response(
                        id,
                        json!({
                            "content": [{"type": "text", "text": serde_json::to_string(&json!({
                                "error": {"code": failure.code, "message": failure.message}
                            })).unwrap_or_default()}],
                            "isError": true
                        }),
                    )
                }
            }
        }
    }

    fn tool_check_draft(&self, args: &Value) -> Result<Value, ToolFailure> {
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
            &self.store.fingerprint,
            &self.store.avoid_list,
            &draft,
            source.as_deref(),
        )?;
        Ok(serde_json::to_value(&report)?)
    }

    fn tool_get_exemplars(&self, args: &Value) -> Result<Value, ToolFailure> {
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
        let exemplars = self.store.corpus.exemplars(genre, limit);
        Ok(json!({
            "genre": genre,
            "count": exemplars.len(),
            "exemplars": serde_json::to_value(&exemplars)?
        }))
    }

    fn tool_get_profile(&self, args: &Value) -> Result<Value, ToolFailure> {
        let _ = strict_object(args, &[])?;
        let doc = &self.store.fingerprint;
        let effective = doc.effective_reference();
        let features: Vec<Value> = doc
            .features
            .iter()
            .map(|f| {
                let stat = effective
                    .get(f)
                    .copied()
                    .unwrap_or(voice_core::fingerprint::RefStat { mean: 0.0, sd: 0.0 });
                json!({
                    "name": f,
                    "mean": stat.mean,
                    "sd": stat.sd,
                    "user_corrected": doc.is_corrected(f)
                })
            })
            .collect();
        Ok(json!({
            "profile_markdown": self.store.profile_markdown,
            "fingerprint": {
                "features": features,
                "bands": {"close_max": doc.bands.close_max, "drifting_max": doc.bands.drifting_max},
                "calibration_method": doc.calibration.method,
                "corpus_pieces": doc.corpus_pieces
            },
            "avoid_list": self.store.avoid_list,
            "corpus": {
                "samples": self.store.corpus.samples().len(),
                "included_words": self.store.corpus.included_word_count()
            }
        }))
    }

    fn tool_ingest_sample(&mut self, args: &Value) -> Result<Value, ToolFailure> {
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
        let sample = self.store.corpus.add_sample(NewSample {
            title,
            genre,
            authorship,
            source,
            text,
        })?;
        self.store.save_corpus()?;
        Ok(json!({
            "sample": manifest_row(&sample),
            "corpus_included_words": self.store.corpus.included_word_count(),
            "fingerprint_recomputed": false,
            "note": "Fingerprint unchanged. Call recompute_fingerprint (dry_run first) to fold this sample in."
        }))
    }

    fn tool_list_corpus(&self, args: &Value) -> Result<Value, ToolFailure> {
        let _ = strict_object(args, &[])?;
        let rows: Vec<Value> = self
            .store
            .corpus
            .samples()
            .iter()
            .map(manifest_row)
            .collect();
        Ok(json!({
            "samples": rows,
            "sample_count": self.store.corpus.samples().len(),
            "included_words": self.store.corpus.included_word_count(),
            "total_words": self.store.corpus.total_word_count()
        }))
    }

    fn tool_recompute_fingerprint(&mut self, args: &Value) -> Result<Value, ToolFailure> {
        let args = strict_object(args, &["dry_run"])?;
        let dry_run = args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let (doc, deltas, old_bands, new_bands) = self.store.recompute(dry_run)?;
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

fn success_response(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
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
        voice_core::fetch::fetch_url_text(url)?,
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

fn manifest_row(sample: &voice_core::Sample) -> Value {
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

fn parse_home(args: &[String]) -> Result<PathBuf, String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--home" {
            return args
                .get(i + 1)
                .map(PathBuf::from)
                .ok_or_else(|| "--home needs a directory argument".to_string());
        }
        i += 1;
    }
    Ok(Store::default_dir())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = match parse_home(&args) {
        Ok(h) => h,
        Err(message) => {
            eprintln!("voice-mcp: {message}");
            return ExitCode::from(2);
        }
    };
    let store = match Store::open(&home) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("voice-mcp: cannot open store at {}: {e}", home.display());
            return ExitCode::from(2);
        }
    };
    let mut server = Server::new(store);
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    match server.serve(&mut reader, &mut writer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("voice-mcp: transport failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_session(tag: &str, input: &str) -> Vec<Value> {
        let dir = std::env::temp_dir().join(format!("voice-mcp-unit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("seeded store");
        let mut server = Server::new(store);
        let mut reader = BufReader::new(std::io::Cursor::new(input.as_bytes().to_vec()));
        let mut out: Vec<u8> = Vec::new();
        server.serve(&mut reader, &mut out).expect("serves");
        let _ = std::fs::remove_dir_all(&dir);
        String::from_utf8(out)
            .expect("utf8 output")
            .lines()
            .map(|l| serde_json::from_str(l).expect("response json"))
            .collect()
    }

    #[test]
    fn handshake_and_tool_catalog() {
        let responses = run_session(
            "catalog",
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\"}}\n\
             {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        );
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
        let tools = responses[1]["result"]["tools"]
            .as_array()
            .expect("tools array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(
            names,
            [
                "check_draft",
                "get_exemplars",
                "get_profile",
                "ingest_sample",
                "list_corpus",
                "recompute_fingerprint"
            ]
        );
    }

    #[test]
    fn adversarial_protocol_inputs() {
        let responses = run_session(
            "adversarial",
            "this is not json\n\
             {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"no/such\"}\n\
             {\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"tools/call\",\"params\":{\"name\":\"explode\"}}\n\
             [1,2,3]\n",
        );
        assert_eq!(responses[0]["error"]["code"], PARSE_ERROR);
        assert_eq!(responses[1]["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(responses[2]["error"]["code"], INVALID_PARAMS);
        assert_eq!(responses[3]["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn tool_argument_validation_is_strict() {
        let responses = run_session(
            "strict",
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"list_corpus\",\"arguments\":{\"surprise\":true}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"ingest_sample\",\"arguments\":{\"title\":\"x\"}}}\n",
        );
        assert_eq!(responses[0]["error"]["code"], INVALID_PARAMS);
        assert_eq!(responses[1]["error"]["code"], INVALID_PARAMS);
    }
}
