mod common;
use common::{McpClient, TempDir};
use hae_diag::Limits;
use hae_jsonl::MAX_RESPONSE_BYTES;
use serde_json::{json, Value};
use std::{fs, sync::Arc};

// A double quote is one XML text byte, two bytes in the inner JSON string,
// and four in the JSON-RPC content string. A body-only cap misses the latter.
fn check_quote_response(count: usize) {
    let temp = TempDir::new("wire-limit");
    let (original, mut state, _, _) = hae_package::open(
        include_bytes!("../../../tests/fixtures/adversarial/valid_base.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap();
    let old = String::from_utf8(state.get("Contents/section0.xml").unwrap().to_vec()).unwrap();
    state.set(
        "Contents/section0.xml",
        Arc::from(old.replace("안녕 Hello", &"\"".repeat(count)).into_bytes()),
    );
    let bytes = hae_zip::write(
        &original,
        &hae_package::zip_plan(&original, &state).unwrap(),
    )
    .unwrap();
    let path = temp.path().join("quotes.hwpx");
    fs::write(&path, bytes).unwrap();
    let mut c = McpClient::start(temp.path());
    let (open, _) = c.call_tool("document_open", json!({"path":path}));
    assert_eq!(open["ok"], true);
    let doc = open["result"]["documentId"].clone();
    let (query, _) = c.call_tool(
        "element_query",
        json!({"documentId":doc,"kind":"paragraph"}),
    );
    let target = query["result"]["matches"][0]["nodeId"].clone();
    let reply = c.rpc(
        "tools/call",
        json!({"name":"element_get","arguments":{"documentId":doc,"target":target}}),
    );
    assert!(
        serde_json::to_vec(&reply).unwrap().len() <= MAX_RESPONSE_BYTES,
        "final JSON-RPC exceeds wire cap"
    );
    assert_eq!(
        reply["result"]["isError"], true,
        "size-limited tool result must mark isError"
    );
    let envelope: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["diagnostics"][0]["code"], "RESOURCE_LIMIT");
    assert_eq!(c.rpc("ping", json!({}))["result"], json!({}));
    assert_eq!(c.finish().0, 0);
}
#[test]
fn final_jsonrpc_wire_size_includes_nested_json_escaping() {
    check_quote_response(1_100_000);
}
#[test]
fn oversized_inner_response_also_sets_tool_is_error() {
    check_quote_response(2_200_000);
}
