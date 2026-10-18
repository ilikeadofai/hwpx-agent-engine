use hae_diag::Limits;
use hae_edit::{apply, EditOp, EquationProps};
use hae_model::{Document, Kind};
use std::sync::Arc;
const SECTION: &str = "Contents/section0.xml";
fn doc(from: &str, to: &str) -> Document {
    let d = Document::open(
        include_bytes!("../../../tests/fixtures/editing/equations.hwpx").to_vec(),
        &Limits::default(),
    )
    .unwrap();
    if from.is_empty() {
        return d;
    }
    let mut s = d.state.clone();
    let x = std::str::from_utf8(s.get(SECTION).unwrap()).unwrap();
    assert!(x.contains(from));
    s.set(SECTION, Arc::from(x.replacen(from, to, 1).into_bytes()));
    d.rebuild(s, vec![]).unwrap()
}
fn eq(d: &Document) -> String {
    d.nodes
        .iter()
        .find(|n| n.kind == Kind::Equation)
        .unwrap()
        .id
        .clone()
}
fn script(d: &Document, s: &str) -> hae_diag::Result<hae_edit::Applied> {
    apply(
        d,
        &[EditOp::EquationSetScript {
            target: eq(d),
            script: s.into(),
        }],
    )
}
#[test]
fn equation_edit_rejects_cdata_comments_nested_and_duplicate_script_carriers() {
    let old = "<hp:script>{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}</hp:script>";
    for new in [
        "<hp:script><![CDATA[x]]></hp:script>",
        "<hp:script>x<!--keep-->y</hp:script>",
        "<hp:script><hp:t>x</hp:t></hp:script>",
        "<hp:script>x</hp:script><hp:script>y</hp:script>",
        "<hp:script x:keep=\"1\">x</hp:script>",
        "",
    ] {
        let d = doc(old, new);
        assert!(script(&d, "x").is_err(), "{new}");
        let n = d.equation_info(&eq(&d), true).unwrap();
        assert!(
            n.script_raw.is_some() || new.is_empty() || new.contains("</hp:script><hp:script>")
        );
        // Opaque script contents do not prevent an attribute-only change.
        apply(
            &d,
            &[EditOp::EquationSetProperties {
                target: eq(&d),
                props: EquationProps {
                    base_line: Some(90),
                    ..Default::default()
                },
            }],
        )
        .unwrap();
    }
}
#[test]
fn equation_edit_expands_existing_selfclosing_script_without_new_model_nodes() {
    let d = doc(
        "<hp:script>{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}</hp:script>",
        "<hp:script />",
    );
    let a = script(&d, "x < y").unwrap();
    assert_eq!(d.map_rows(), a.doc.map_rows());
    assert_eq!(
        a.doc
            .equation_info(&eq(&d), false)
            .unwrap()
            .script_raw
            .as_deref(),
        Some("x &lt; y")
    );
}
#[test]
fn equation_properties_add_missing_attributes_and_escape_both_quotes() {
    let d = doc("font=\"HancomEQN\"", "");
    let a = apply(
        &d,
        &[EditOp::EquationSetProperties {
            target: eq(&d),
            props: EquationProps {
                font: Some("A'B\" & C".into()),
                ..Default::default()
            },
        }],
    )
    .unwrap();
    assert_eq!(d.map_rows(), a.doc.map_rows());
    let s = std::str::from_utf8(a.doc.payload(SECTION).unwrap()).unwrap();
    assert!(s.contains("font=\"A&apos;B&quot; &amp; C\""));
}
#[test]
fn equation_properties_on_empty_equation_do_not_create_a_script() {
    let d = doc(
        "<hp:script>{a} over {b}  TIMES  {c} over {d} = {ac} over {bd}</hp:script>",
        "",
    );
    let a = apply(
        &d,
        &[EditOp::EquationSetProperties {
            target: eq(&d),
            props: EquationProps {
                version: Some("Equation Version 60".into()),
                base_unit: Some(2000),
                ..Default::default()
            },
        }],
    )
    .unwrap();
    assert_eq!(a.doc.equation_info(&eq(&d), false).unwrap().script, None);
    assert_eq!(d.map_rows(), a.doc.map_rows());
}
