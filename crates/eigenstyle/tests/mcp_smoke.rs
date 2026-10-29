//! End-to-end stdio smoke: spawn the real `eigenstyle` binary against a
//! scratch store and drive it like a modern-era MCP client —
//! `server/discover` first, per-request `_meta` on everything after,
//! every tool once, an ingest → recompute (dry-run, then apply) →
//! check round trip — plus the adversarial battery: legacy
//! `initialize` contact refused with the typed `-32022`, missing or
//! legacy `_meta` refused, malformed frames, unknown tool, and
//! bound-violating ingests. Store state must persist across a server
//! restart. (Modernized in WS1; the pre-rmcp version of this file
//! drove the legacy initialize handshake.)

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

const MODERN: &str = "2026-07-28";

fn meta(version: &str) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientInfo": {"name": "smoke", "version": "0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn spawn(home: &PathBuf) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_eigenstyle"))
            .arg("--home")
            .arg(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn eigenstyle");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Client {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    fn send_raw(&mut self, line: &str) -> Option<Value> {
        self.stdin.write_all(line.as_bytes()).expect("write");
        self.stdin.write_all(b"\n").expect("write nl");
        self.stdin.flush().expect("flush");
        let mut buf = String::new();
        let n = self.stdout.read_line(&mut buf).expect("read");
        if n == 0 {
            None
        } else {
            Some(serde_json::from_str(&buf).expect("response is json"))
        }
    }

    /// A request exactly as given — no `_meta` stamping. For probes
    /// that exercise the era machinery itself.
    fn call_unstamped(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let response = self
            .send_raw(
                &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
                    .to_string(),
            )
            .expect("response");
        assert_eq!(response["id"], id, "response id matches request id");
        response
    }

    /// A modern-era request: `_meta` stamped into params.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let mut params = params;
        params
            .as_object_mut()
            .expect("params object")
            .insert("_meta".to_string(), meta(MODERN));
        self.call_unstamped(method, params)
    }

    fn discover(&mut self) -> Value {
        let response = self.call("server/discover", json!({}));
        let result = &response["result"];
        assert_eq!(
            result["supportedVersions"],
            json!([MODERN]),
            "discover advertises exactly the modern version: {response}"
        );
        assert_eq!(
            result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
            "eigenstyle"
        );
        response
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.tool_raw(name, arguments);
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text content");
        serde_json::from_str(text).expect("tool payload is json")
    }

    fn tool_raw(&mut self, name: &str, arguments: Value) -> Value {
        self.call("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn close(mut self) {
        drop(self.stdin);
        let status = self.child.wait().expect("wait");
        assert!(status.success(), "server exits cleanly on EOF");
    }
}

fn scratch_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eigenstyle-smoke-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

const NEW_PROSE: &str = "Whether in the operating room or in the policy memo, the pattern is the same. \
    A tool arrives with a promise. The promise is tested against outcomes that patients report, \
    not outcomes that vendors prefer. We checked the record twice. The record was plain: \
    the intervention helped, the harm was rare, and the cost was honest. That is the whole argument. \
    Safety is not a feature. It is the precondition.";

#[test]
fn full_client_round_trip() {
    let home = scratch_home("roundtrip");
    let mut client = Client::spawn(&home);
    client.discover();

    // The catalog is exactly the six baseline tools.
    let list = client.call("tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
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

    // Seeded corpus is visible.
    let corpus = client.tool("list_corpus", json!({}));
    assert_eq!(corpus["sample_count"], 6);
    assert_eq!(corpus["included_words"], 7674);

    // Profile carries the readable text and 45 features.
    let profile = client.tool("get_profile", json!({}));
    assert_eq!(
        profile["fingerprint"]["features"].as_array().map(Vec::len),
        Some(45)
    );
    assert!(
        profile["profile_markdown"]
            .as_str()
            .unwrap_or("")
            .contains("Porter voice profile")
    );
    assert_eq!(profile["avoid_list"].as_array().map(Vec::len), Some(12));

    // Exemplars come back verbatim from the letter.
    let exemplars = client.tool("get_exemplars", json!({"genre": "letter", "limit": 3}));
    assert_eq!(exemplars["count"], 3);

    // A genuine holdout draft checks CLOSE against the seed fingerprint.
    let holdout = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../eigenstyle-core/tests/fixtures/jas_holdout.txt"),
    )
    .expect("holdout fixture");
    let report = client.tool("check_draft", json!({"text": holdout}));
    assert_eq!(report["verdict"], "close");
    let delta = report["delta"].as_f64().expect("delta");
    assert!(
        (delta - 1.196).abs() <= 0.002,
        "delta {delta} vs python 1.196"
    );

    // Ingest does not recompute.
    let ingested = client.tool(
        "ingest_sample",
        json!({
            "title": "Smoke sample — safety precondition note",
            "genre": "project-prose",
            "authorship": "sole",
            "source": "mcp smoke test",
            "text": NEW_PROSE
        }),
    );
    assert_eq!(ingested["fingerprint_recomputed"], false);
    assert_eq!(ingested["sample"]["words"].as_u64().unwrap_or(0), 70);

    // Dry-run recompute previews without saving.
    let preview = client.tool("recompute_fingerprint", json!({"dry_run": true}));
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["saved"], false);
    assert_eq!(preview["piece_count"], 7);
    let applied = client.tool("recompute_fingerprint", json!({"dry_run": false}));
    assert_eq!(applied["saved"], true);
    assert_eq!(applied["piece_count"], 7);

    client.close();

    // State persists across a restart: 7 samples, recomputed bands on disk.
    let mut client = Client::spawn(&home);
    client.discover();
    let corpus = client.tool("list_corpus", json!({}));
    assert_eq!(corpus["sample_count"], 7);
    client.close();
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn legacy_contact_is_refused_and_the_server_survives() {
    let home = scratch_home("refusal");
    let mut client = Client::spawn(&home);

    // initialize at any legacy version: the designed typed refusal.
    for version in ["2025-06-18", "2025-03-26", "2024-11-05"] {
        let response = client.call_unstamped(
            "initialize",
            json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "legacy", "version": "0"}}),
        );
        assert_eq!(response["error"]["code"], -32022, "initialize {version}");
        assert_eq!(response["error"]["message"], "Unsupported protocol version");
        assert_eq!(response["error"]["data"]["requested"], version);
        assert_eq!(response["error"]["data"]["supported"], json!([MODERN]));
    }
    // Even an initialize claiming the modern version is refused:
    // the modern era has no initialize handshake at all.
    let response = client.call_unstamped(
        "initialize",
        json!({"protocolVersion": MODERN, "capabilities": {}, "clientInfo": {"name": "x", "version": "0"}}),
    );
    assert_eq!(response["error"]["code"], -32022);

    // A first request with no _meta at all is refused by the era
    // machinery (missing request metadata), not served.
    let response = client.call_unstamped("tools/list", json!({}));
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("_meta"),
        "metadata refusal names the missing field: {response}"
    );

    // A first request stamped with a legacy version is refused -32022.
    let mut params = json!({});
    params
        .as_object_mut()
        .expect("obj")
        .insert("_meta".to_string(), meta("2025-06-18"));
    let response = client.call_unstamped("tools/list", params);
    assert_eq!(response["error"]["code"], -32022);
    assert_eq!(response["error"]["data"]["supported"], json!([MODERN]));

    // After every refusal the same process still serves modern work.
    client.discover();
    let corpus = client.tool("list_corpus", json!({}));
    assert_eq!(corpus["sample_count"], 6);

    // initialize mid-session is still refused, and changes nothing.
    let response = client.call_unstamped(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "legacy", "version": "0"}}),
    );
    assert_eq!(response["error"]["code"], -32022);
    let corpus = client.tool("list_corpus", json!({}));
    assert_eq!(corpus["sample_count"], 6);

    client.close();
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn adversarial_frames_and_bounds() {
    let home = scratch_home("adversarial");
    let mut client = Client::spawn(&home);

    // Malformed JSON yields -32700, and the server stays alive.
    let response = client.send_raw("{not json").expect("parse error response");
    assert_eq!(response["error"]["code"], -32700);

    // A non-object request yields -32600.
    let response = client.send_raw("[1,2,3]").expect("response");
    assert_eq!(response["error"]["code"], -32600);

    // Unknown method yields -32601.
    let response = client
        .send_raw("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"bogus\"}")
        .expect("response");
    assert_eq!(response["error"]["code"], -32601);

    // An oversized line yields -32700 and the next request is served.
    let big = "x".repeat(4 * 1024 * 1024 + 16);
    let response = client.send_raw(&big).expect("oversize response");
    assert_eq!(response["error"]["code"], -32700);
    assert_eq!(
        response["error"]["message"],
        "request line exceeds size bound"
    );

    client.discover();

    // Unknown tool is a params error with the baseline's message.
    let response = client.tool_raw("nuke_corpus", json!({}));
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "unknown tool: nuke_corpus");

    // Unknown arguments are rejected before the core sees them.
    let response = client.tool_raw("list_corpus", json!({"surprise": true}));
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["message"], "unknown argument: surprise");

    // Ingest bound violations come back as errors, not panics:
    // empty text (below floor), missing authorship, two body
    // sources, invented authorship value.
    for arguments in [
        json!({"title": "t", "genre": "letter", "authorship": "sole", "text": ""}),
        json!({"title": "t", "genre": "letter", "text": NEW_PROSE}),
        json!({"title": "t", "genre": "letter", "authorship": "sole", "text": NEW_PROSE, "url": "https://example.org/x"}),
        json!({"title": "t", "genre": "letter", "authorship": "ghost", "text": NEW_PROSE}),
    ] {
        let response = client.tool_raw("ingest_sample", arguments);
        let is_error = response["result"]["isError"].as_bool().unwrap_or(false)
            || response.get("error").is_some();
        assert!(is_error, "expected an error response, got {response}");
    }
    // Corpus untouched by all of the above.
    let corpus = client.tool("list_corpus", json!({}));
    assert_eq!(corpus["sample_count"], 6);
    client.close();
    let _ = std::fs::remove_dir_all(&home);
}
