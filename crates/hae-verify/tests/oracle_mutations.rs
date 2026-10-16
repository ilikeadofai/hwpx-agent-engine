//! Cross-check hae-verify against the 30 independent mutation cases generated
//! by `tests/oracle/mutations.py` (plan tasks 1-13 & 1-14).

use hae_diag::{Code, Limits};
use hae_verify::{verify_archive, Alignment, LineSpacing, LineSpacingKind, VerifyOperation};
use hae_zip::Archive;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn parse_oracle_op(op: &Value, candidate: &Archive) -> VerifyOperation {
    let kind = op["op"].as_str().unwrap();
    let entry = op["entry"].as_str().unwrap().to_string();
    let path: Vec<usize> = op["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    match kind {
        "text.replace" => {
            let start = op["range"]["start"].as_u64().unwrap() as usize;
            let end = op["range"]["end"].as_u64().unwrap() as usize;
            let replacement = op["text"].as_str().unwrap().to_string();
            VerifyOperation::TextReplace {
                entry,
                path,
                range: start..end,
                replacement,
            }
        }
        "cell.setText" => VerifyOperation::CellSetText {
            entry,
            path,
            text: op["text"].as_str().unwrap().to_string(),
        },
        "paragraph.format" => {
            let alignment = op["alignment"].as_str().map(|s| match s {
                "left" => Alignment::Left,
                "right" => Alignment::Right,
                "center" => Alignment::Center,
                "justify" => Alignment::Justify,
                "distribute" => Alignment::Distribute,
                "distribute_space" => Alignment::DistributeSpace,
                other => panic!("unexpected alignment {other}"),
            });
            let line_spacing = op["lineSpacingPercent"].as_u64().map(|v| LineSpacing {
                kind: LineSpacingKind::Percent,
                value: v as u32,
            });
            VerifyOperation::ParagraphFormat {
                entry,
                path,
                alignment,
                line_spacing,
            }
        }
        "image.replace" => {
            // Decode the expected new PNG from the fixture's base64 or use the fixed fixture PNG
            let _ = candidate;
            let py = Command::new("python3")
                .args([
                    "-I",
                    "-c",
                    &format!(
                        "import base64, sys; sys.stdout.buffer.write(base64.b64decode({:?}))",
                        op["image"]["base64"].as_str().unwrap()
                    ),
                ])
                .output()
                .unwrap();
            assert!(py.status.success());
            VerifyOperation::ImageReplace {
                entry,
                path,
                bytes: py.stdout,
            }
        }
        other => panic!("unexpected op {other}"),
    }
}

#[test]
fn hae_verify_rejects_all_30_independent_oracle_mutations() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = std::env::temp_dir().join(format!("hae-oracle-mut-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    let status = Command::new("python3")
        .arg("-I")
        .arg(repo_root.join("tests/oracle/mutations.py"))
        .arg(&out_dir)
        .status()
        .unwrap();
    assert!(status.success());

    let manifest: Value =
        serde_json::from_slice(&fs::read(out_dir.join("manifest.json")).unwrap()).unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);

    let limits = Limits::default();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let orig_bytes =
            fs::read(out_dir.join(c["files"]["original.hwpx"]["file"].as_str().unwrap())).unwrap();
        let cand_bytes =
            fs::read(out_dir.join(c["files"]["candidate.hwpx"]["file"].as_str().unwrap())).unwrap();
        let req_json: Value = serde_json::from_slice(
            &fs::read(out_dir.join(c["files"]["request.json"]["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let orig_arch = Archive::open(orig_bytes, &limits).unwrap();
        let cand_arch = Archive::open(cand_bytes.clone(), &limits).unwrap();
        let ops: Vec<VerifyOperation> = req_json["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| parse_oracle_op(o, &cand_arch))
            .collect();
        let res = verify_archive(&orig_arch, &cand_bytes, &ops, &limits);
        assert!(
            res.is_err(),
            "oracle mutation case '{name}' should be rejected by hae-verify, got: {res:?}"
        );
        assert_eq!(res.unwrap_err().code(), Code::VerifyFailed, "{name}");
    }
    let _ = fs::remove_dir_all(&out_dir);
}
