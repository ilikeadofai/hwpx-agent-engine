use super::*;
use crate::export_text::{ExportOptions, TextFormat};
use hae_zip::{keep_all, write, Archive, OutEntry};

const NS: &str = r#"xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hc="http://www.hancom.co.kr/hwpml/2011/core""#;

const HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head" version="1.2" secCnt="1"><hh:refList><hh:paraProperties itemCnt="3"><hh:paraPr id="0"><hh:align horizontal="LEFT" vertical="BASELINE"/></hh:paraPr><hh:paraPr id="1"><hh:align horizontal="CENTER" vertical="BASELINE"/></hh:paraPr><hh:paraPr id="2"><hh:align horizontal="RIGHT" vertical="BASELINE"/></hh:paraPr></hh:paraProperties></hh:refList></hh:head>"#;

const HPF: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="" unique-identifier="" id=""><opf:metadata><opf:title/><opf:language>ko</opf:language></opf:metadata><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/><opf:item id="img0" href="BinData/img0.png" media-type="image/png" isEmbeded="1"/></opf:manifest><opf:spine><opf:itemref idref="header"/><opf:itemref idref="section0"/></opf:spine></opf:package>"#;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot-a-real-image-but-bytes";

/// A document whose section0 holds `body`, with paraPr 0/1/2 = left/center/right.
fn synthetic(body: &str) -> Document {
    let base = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/adversarial/valid_base.hwpx"
    ))
    .unwrap();
    let limits = Limits::default();
    let archive = Archive::open(base, &limits).unwrap();
    let section = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?><hs:sec {NS}>{body}</hs:sec>"#
    );
    let mut plan = keep_all(&archive);
    for item in &mut plan {
        let replace = |n: &str, bytes: &str| {
            OutEntry::Replace(n.to_string(), Arc::from(bytes.as_bytes().to_vec()))
        };
        match item {
            OutEntry::Keep(n) if n == "Contents/section0.xml" => *item = replace(n, &section),
            OutEntry::Keep(n) if n == "Contents/header.xml" => *item = replace(n, HEADER),
            OutEntry::Keep(n) if n == "Contents/content.hpf" => *item = replace(n, HPF),
            _ => {}
        }
    }
    plan.push(OutEntry::Add {
        name: "BinData/img0.png".into(),
        payload: Arc::from(PNG.to_vec()),
        method: 0,
    });
    Document::open(write(&archive, &plan).unwrap(), &limits).unwrap()
}

fn para(text: &str) -> String {
    format!(r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#)
}

fn aligned(pr: u8, text: &str) -> String {
    format!(
        r#"<hp:p paraPrIDRef="{pr}"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p>"#
    )
}

fn cell(row: u32, col: u32, rs: u32, cs: u32, text: &str) -> String {
    format!(
        r#"<hp:tc><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>{text}</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="{col}" rowAddr="{row}"/><hp:cellSpan colSpan="{cs}" rowSpan="{rs}"/></hp:tc>"#
    )
}

/// 3 rows x 3 cols: (0,0) spans two columns, (1,2) spans two rows.
fn merged_table() -> String {
    format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="7" rowCnt="3" colCnt="3"><hp:tr>{}{}</hp:tr><hp:tr>{}{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl></hp:run></hp:p>"#,
        cell(0, 0, 1, 2, "합친가로"),
        cell(0, 2, 1, 1, "C0"),
        cell(1, 0, 1, 1, "A1"),
        cell(1, 1, 1, 1, "B1"),
        cell(1, 2, 2, 1, "합친세로"),
        cell(2, 0, 1, 1, "A2"),
        cell(2, 1, 1, 1, "B2"),
    )
}

fn export(d: &Document, format: TextFormat) -> export_text::ExportedText {
    export_with(d, format, true)
}

fn export_with(d: &Document, format: TextFormat, notes: bool) -> export_text::ExportedText {
    d.export_text(&ExportOptions {
        format,
        scope: None,
        include_notes: notes,
        max_bytes: Limits::default().max_text_export_bytes,
    })
    .unwrap()
}

fn open(name: &str) -> Document {
    let bytes = std::fs::read(format!(
        "{}/../../tests/fixtures/corpus/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    Document::open(bytes, &Limits::default()).unwrap()
}

fn sha(entry: &[u8]) -> String {
    hae_package::sha256_hex(entry)
}

#[test]
fn markdown_paragraphs_keep_text_tab_and_line_break() {
    let d = synthetic(&para("첫째 줄<hp:lineBreak/>둘째 줄<hp:tab/>탭 뒤"));
    let out = export(&d, TextFormat::Markdown);
    assert!(
        out.text.contains("첫째 줄\\\n둘째 줄&emsp;탭 뒤"),
        "{}",
        out.text
    );
    assert_eq!(out.counts.paragraphs, 1);
    assert!(out.text.starts_with("> **변환 안내** (HWPX → Markdown)"));
}

#[test]
fn html_paragraphs_are_a_complete_document_with_alignment_styles() {
    let d = synthetic(&format!(
        "{}{}{}",
        aligned(0, "왼쪽"),
        aligned(1, "가운데"),
        aligned(2, "오른쪽")
    ));
    let out = export(&d, TextFormat::Html);
    assert!(out.text.starts_with("<!DOCTYPE html>\n<html lang=\"ko\">"));
    assert!(out.text.trim_end().ends_with("</html>"));
    assert!(out.text.contains("<p>왼쪽</p>"));
    assert!(out
        .text
        .contains("<p style=\"text-align:center\">가운데</p>"));
    assert!(out
        .text
        .contains("<p style=\"text-align:right\">오른쪽</p>"));
    assert_eq!(out.counts.alignment_ignored, 0);
}

#[test]
fn markdown_drops_alignment_and_says_so_in_a_comment_and_the_notice() {
    let d = synthetic(&format!(
        "{}{}",
        aligned(1, "가운데 문단"),
        aligned(2, "오른쪽 문단")
    ));
    let out = export(&d, TextFormat::Markdown);
    assert!(out
        .text
        .contains("<!-- hwpx: 문단 정렬(가운데) 무시됨 -->\n가운데 문단"));
    assert!(out
        .text
        .contains("<!-- hwpx: 문단 정렬(오른쪽) 무시됨 -->\n오른쪽 문단"));
    assert!(out.text.contains("문단 정렬(가운데·오른쪽) 2곳은"));
    assert_eq!(out.counts.alignment_ignored, 2);
    assert!(!out.text.contains("text-align"));
}

#[test]
fn markdown_table_becomes_a_pipe_table_with_an_empty_header_row() {
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="1" rowCnt="2" colCnt="2"><hp:tr>{}{}</hp:tr><hp:tr>{}{}</hp:tr></hp:tbl></hp:run></hp:p>"#,
        cell(0, 0, 1, 1, "이름"),
        cell(0, 1, 1, 1, "값"),
        cell(1, 0, 1, 1, "a|b"),
        cell(1, 1, 1, 1, "2"),
    );
    let out = export(&synthetic(&body), TextFormat::Markdown);
    assert!(
        out.text
            .contains("|   |   |\n| --- | --- |\n| 이름 | 값 |\n| a\\|b | 2 |"),
        "{}",
        out.text
    );
    assert_eq!(out.counts.tables, 1);
    assert_eq!(out.counts.merged_tables, 0);
    assert!(!out.text.contains("병합 셀이 있는 표"));
}

#[test]
fn markdown_merged_cells_are_blank_not_repeated_and_the_head_says_so() {
    let out = export(&synthetic(&merged_table()), TextFormat::Markdown);
    // Anchor text appears once; the covered slots stay empty.
    assert_eq!(out.text.matches("합친가로").count(), 1);
    assert_eq!(out.text.matches("합친세로").count(), 1);
    assert!(
        out.text
            .contains("| 합친가로 |  | C0 |\n| A1 | B1 | 합친세로 |\n| A2 | B2 |  |"),
        "{}",
        out.text
    );
    let notice_end = out.text.find("\n\n").unwrap();
    assert!(
        out.text[..notice_end].contains("병합 셀이 있는 표: 표 1."),
        "{}",
        &out.text[..notice_end]
    );
    assert!(out
        .text
        .contains("<!-- hwpx: 표 1 병합 셀 포함, 병합된 칸은 비워 둠"));
    assert_eq!(out.counts.merged_tables, 1);
}

#[test]
fn html_merged_cells_use_colspan_and_rowspan() {
    let out = export(&synthetic(&merged_table()), TextFormat::Html);
    assert!(out
        .text
        .contains("<td colspan=\"2\"><p>합친가로</p></td><td><p>C0</p></td>"));
    assert!(out.text.contains("<td rowspan=\"2\"><p>합친세로</p></td>"));
    // The row that is covered by the rowspan has only its own two cells.
    assert!(out
        .text
        .contains("<tr><td><p>A2</p></td><td><p>B2</p></td></tr>"));
    assert_eq!(out.text.matches("합친세로").count(), 1);
}

#[test]
fn picture_references_media_with_sha256_and_caption_without_creating_an_image() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>앞</hp:t><hp:pic id="9"><hp:sz width="100" height="100"/><hc:img binaryItemIDRef="img0" bright="0" contrast="0" effect="REAL_PIC"/><hp:caption><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>그림 1. 설명</hp:t></hp:run></hp:p></hp:subList></hp:caption></hp:pic><hp:t>뒤</hp:t></hp:run></hp:p>"#;
    let d = synthetic(body);
    let want = sha(PNG);
    let md = export(&d, TextFormat::Markdown);
    assert!(
        md.text.contains(&format!(
            "[그림] `img0.png` (미디어 항목 `BinData/img0.png`, SHA-256 `{want}`)\n\n그림 캡션: 그림 1. 설명"
        )),
        "{}",
        md.text
    );
    assert!(md.text.contains("앞\n\n[그림]") && md.text.contains("그림 캡션: 그림 1. 설명\n\n뒤"));
    assert!(!md.text.contains("!["), "must not emit an image link");
    assert_eq!(md.counts.pictures, 1);
    let html = export(&d, TextFormat::Html);
    assert!(html.text.contains(&format!(
        "<figure class=\"hwpx-picture\" data-media-entry=\"BinData/img0.png\" data-sha256=\"{want}\""
    )));
    assert!(html.text.contains("<figcaption>그림 1. 설명</figcaption>"));
    assert!(!html.text.contains("<img"));
    // The package is read, never written.
    assert!(d.is_unmodified());
}

#[test]
fn equation_script_is_wrapped_as_code_verbatim() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>식 </hp:t><hp:equation version="1.0"><hp:script>a_1^2 + `b` | c &lt; d</hp:script></hp:equation><hp:t> 끝</hp:t></hp:run></hp:p>"#;
    let d = synthetic(body);
    let md = export(&d, TextFormat::Markdown);
    // The script contains a backtick, so the fence is two backticks, padded.
    assert!(
        md.text.contains("식 ``a_1^2 + `b` | c < d`` 끝"),
        "{}",
        md.text
    );
    assert_eq!(md.counts.equations, 1);
    let html = export(&d, TextFormat::Html);
    assert!(html
        .text
        .contains("<code class=\"hwpx-equation\">a_1^2 + `b` &#124; c &lt; d</code>"));
}

#[test]
fn footnotes_endnotes_headers_and_footers_are_marked_in_place_and_listed() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:header id="2" applyPageType="BOTH"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>머리말 글</hp:t></hp:run></hp:p></hp:subList></hp:header></hp:ctrl><hp:ctrl><hp:footer id="3" applyPageType="ODD"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>꼬리말 글</hp:t></hp:run></hp:p></hp:subList></hp:footer></hp:ctrl><hp:t>본문</hp:t><hp:ctrl><hp:footNote number="1"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>각주 내용</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl><hp:t>다음</hp:t><hp:ctrl><hp:endNote number="1"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>미주 내용</hp:t></hp:run></hp:p></hp:subList></hp:endNote></hp:ctrl></hp:run></hp:p>"#;
    let d = synthetic(body);
    let md = export(&d, TextFormat::Markdown);
    assert!(md.text.contains("본문[^fn1]다음[^en1]"), "{}", md.text);
    assert!(md.text.contains("[^fn1]: 각주 내용"));
    assert!(md.text.contains("[^en1]: 미주 내용"));
    assert!(md.text.contains("## 각주\n") && md.text.contains("## 미주\n"));
    assert!(md
        .text
        .contains("## 머리말·꼬리말\n\n### 머리말 1 (구역 1, 적용 쪽: BOTH)\n\n머리말 글"));
    assert!(md
        .text
        .contains("### 꼬리말 1 (구역 1, 적용 쪽: ODD)\n\n꼬리말 글"));
    // Headers and footers sit before the body, notes after it.
    let (h, b, n) = (
        md.text.find("머리말 글").unwrap(),
        md.text.find("본문[^fn1]").unwrap(),
        md.text.find("[^fn1]: ").unwrap(),
    );
    assert!(h < b && b < n);
    assert_eq!(
        (
            md.counts.footnotes,
            md.counts.endnotes,
            md.counts.headers,
            md.counts.footers
        ),
        (1, 1, 1, 1)
    );
    // The header paragraph does not leak into the body flow.
    assert_eq!(md.text.matches("머리말 글").count(), 1);

    let html = export(&d, TextFormat::Html);
    assert!(html
        .text
        .contains("<sup class=\"hwpx-noteref\"><a id=\"ref-fn1\" href=\"#fn1\">[각주1]</a></sup>"));
    assert!(html
        .text
        .contains("<li id=\"fn1\" value=\"1\"><p>각주 내용</p> <a href=\"#ref-fn1\">↩</a></li>"));
    assert!(html
        .text
        .contains("<h3>머리말 1 (구역 1, 적용 쪽: BOTH)</h3>"));
}

#[test]
fn include_notes_false_omits_marks_and_lists_but_keeps_headers() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:header id="2" applyPageType="BOTH"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>머리</hp:t></hp:run></hp:p></hp:subList></hp:header></hp:ctrl><hp:t>본문</hp:t><hp:ctrl><hp:footNote><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>숨길 각주</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl></hp:run></hp:p>"#;
    let d = synthetic(body);
    for f in [TextFormat::Markdown, TextFormat::Html] {
        let out = export_with(&d, f, false);
        assert!(!out.text.contains("숨길 각주"));
        assert!(!out.text.contains("[^fn1]") && !out.text.contains("ref-fn1"));
        assert!(out.text.contains("머리"));
        assert_eq!(out.counts.footnotes, 0);
        assert_eq!(out.counts.notes_omitted, 1);
        assert!(out.text.contains("includeNotes=false"));
    }
}

#[test]
fn special_characters_are_escaped_in_markdown_and_html() {
    let d = synthetic(&para(
        "a|b *c* _d_ &lt;e&gt; &amp;amp; [f](g) `h` \\i ~j~ $k$",
    ));
    let md = export(&d, TextFormat::Markdown);
    assert!(
        md.text.contains(
            "a\\|b \\*c\\* \\_d\\_ \\<e\\> \\&amp; \\[f\\](g) \\`h\\` \\\\i \\~j\\~ \\$k\\$"
        ),
        "{}",
        md.text
    );
    let html = export(&d, TextFormat::Html);
    assert!(
        html.text
            .contains("<p>a|b *c* _d_ &lt;e&gt; &amp;amp; [f](g) `h` \\i ~j~ $k$</p>"),
        "{}",
        html.text
    );
    // Markup in the text must not survive as markup.
    let tag = synthetic(&para(
        "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; \" '",
    ));
    let html = export(&tag, TextFormat::Html);
    assert!(html
        .text
        .contains("<p>&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &quot; &#39;</p>"));
    assert!(!html.text.contains("<script"));
    let md = export(&tag, TextFormat::Markdown);
    assert!(!md.text.replace("\\<", "").contains("<script"));
    assert!(md.text.contains("\\<script\\>"));
}

#[test]
fn line_start_syntax_cannot_turn_text_into_markdown_blocks() {
    let body = [
        "# 제목처럼",
        "- 목록처럼",
        "+ 목록",
        "1. 번호",
        "2) 번호",
        "=== 밑줄",
        "    들여쓰기 코드처럼",
        "&gt; 인용처럼",
        "--- 구분선",
    ]
    .iter()
    .map(|t| para(t))
    .collect::<String>();
    let out = export(&synthetic(&body), TextFormat::Markdown);
    for expect in [
        "\n\\# 제목처럼",
        "\n\\- 목록처럼",
        "\n\\+ 목록",
        "\n1\\. 번호",
        "\n2\\) 번호",
        "\n\\=== 밑줄",
        "\n&nbsp;&nbsp;&nbsp;&nbsp;들여쓰기 코드처럼",
        "\n\\> 인용처럼",
        "\n\\--- 구분선",
    ] {
        assert!(out.text.contains(expect), "{expect:?} in {}", out.text);
    }
}

#[test]
fn unsupported_objects_are_marked_and_counted_never_dropped_silently() {
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>전</hp:t><hp:rect id="3"/><hp:t>후</hp:t><hp:ctrl><hp:hiddenComment/></hp:ctrl><hp:ctrl><hp:pageNum/></hp:ctrl></hp:run></hp:p>"#;
    let out = export(&synthetic(body), TextFormat::Markdown);
    assert!(
        out.text
            .contains("전⟦미지원: rect⟧후⟦미지원: hiddenComment⟧"),
        "{}",
        out.text
    );
    assert_eq!(out.counts.unsupported.get("rect"), Some(&1));
    assert_eq!(out.counts.unsupported.get("hiddenComment"), Some(&1));
    assert_eq!(out.counts.ignored_markers.get("pageNum"), Some(&1));
    assert!(out
        .text
        .contains("변환하지 못한 요소: hiddenComment ×1, rect ×1."));
}

#[test]
fn nested_table_is_html_nested_but_marked_unsupported_in_markdown() {
    let inner = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="2" rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>안쪽</hp:t></hp:run></hp:p></hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#;
    let body = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="1" rowCnt="1" colCnt="1"><hp:tr><hp:tc><hp:subList>{inner}</hp:subList><hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan colSpan="1" rowSpan="1"/></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#
    );
    let d = synthetic(&body);
    let html = export(&d, TextFormat::Html);
    assert_eq!(html.text.matches("<table").count(), 2);
    let md = export(&d, TextFormat::Markdown);
    assert!(
        md.text.contains("⟦중첩 표 1×1 변환 불가: 안쪽⟧"),
        "{}",
        md.text
    );
    assert_eq!(md.counts.unsupported.get("nestedTable"), Some(&1));
}

#[test]
fn table_with_overlapping_or_unaddressed_cells_is_refused_not_guessed() {
    let overlap = format!(
        r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="1" rowCnt="1" colCnt="2"><hp:tr>{}{}</hp:tr></hp:tbl></hp:run></hp:p>"#,
        cell(0, 0, 1, 2, "넓은"),
        cell(0, 1, 1, 1, "겹침"),
    );
    let out = export(&synthetic(&overlap), TextFormat::Markdown);
    assert!(out.text.contains("⟦미지원: tbl⟧"), "{}", out.text);
    assert!(!out.text.contains("넓은") && !out.text.contains("겹침"));
    // Without cellAddr a spanning cell has no position: refuse, and say how
    // much content that hides.
    let missing = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:tbl id="1"><hp:tr><hp:tc><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>숨은 글</hp:t></hp:run></hp:p></hp:subList><hp:cellSpan colSpan="2" rowSpan="1"/></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>"#;
    let out = export(&synthetic(missing), TextFormat::Html);
    assert!(out.text.contains("⟦미지원: tbl⟧"));
    assert!(!out.text.contains("숨은 글"));
    assert_eq!(out.counts.inside_unsupported.paragraphs, 1);
    assert!(out
        .text
        .contains("미지원 개체 안에 있어 변환하지 못한 내용: 문단 1"));
}

#[test]
fn table_in_section_root_with_unaddressed_cells_uses_xml_order() {
    // The shape of corpus/irb-sample.hwpx: hp:tbl directly under the section,
    // hp:p directly under hp:tc, no cellAddr/cellSpan/subList.
    let body = r#"<hp:tbl rowCnt="2" colCnt="2"><hp:tr><hp:tc><hp:p><hp:run><hp:t>이름</hp:t></hp:run></hp:p></hp:tc><hp:tc><hp:p><hp:run><hp:t></hp:t></hp:run></hp:p></hp:tc></hp:tr><hp:tr><hp:tc><hp:p><hp:run><hp:t>소속</hp:t></hp:run></hp:p></hp:tc><hp:tc><hp:p><hp:run><hp:t>값</hp:t></hp:run></hp:p></hp:tc></hp:tr></hp:tbl>"#;
    let d = synthetic(body);
    let md = export(&d, TextFormat::Markdown);
    assert!(
        md.text.contains("| 이름 |  |\n| 소속 | 값 |"),
        "{}",
        md.text
    );
    assert_eq!(md.counts.tables, 1);
    let html = export(&d, TextFormat::Html);
    assert!(html
        .text
        .contains("<tr><td><p>소속</p></td><td><p>값</p></td></tr>"));
}

#[test]
fn unknown_children_of_a_container_are_marked_not_dropped() {
    let body = format!(
        r#"{}<hp:frame id="5"><hp:p><hp:run><hp:t>틀 안 글</hp:t></hp:run></hp:p></hp:frame>"#,
        para("본문")
    );
    let out = export(&synthetic(&body), TextFormat::Markdown);
    assert!(out.text.contains("⟦미지원: frame⟧"), "{}", out.text);
    assert!(!out.text.contains("틀 안 글"));
    assert_eq!(out.counts.inside_unsupported.paragraphs, 1);
}

#[test]
fn scope_limits_output_to_one_section_and_rejects_other_kinds() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/editing/multi_section.hwpx"
    ))
    .unwrap();
    let d = Document::open(bytes, &Limits::default()).unwrap();
    let sections: Vec<&Node> = d.nodes.iter().filter(|n| n.kind == Kind::Section).collect();
    assert_eq!(sections.len(), 2);
    let opts = |scope| ExportOptions {
        format: TextFormat::Markdown,
        scope,
        include_notes: true,
        max_bytes: 1 << 20,
    };
    let all = d.export_text(&opts(None)).unwrap().text;
    assert!(all.contains("구역0 문단0") && all.contains("구역1 문단0"));
    assert!(all.contains("<!-- hwpx: 구역 1 nodeId") && all.contains("<!-- hwpx: 구역 2 nodeId"));
    let second = d.export_text(&opts(Some(&sections[1].id))).unwrap();
    assert!(second.text.contains("구역1 문단0") && !second.text.contains("구역0"));
    assert_eq!(second.counts.paragraphs, 1);
    let para = d.nodes.iter().find(|n| n.kind == Kind::Paragraph).unwrap();
    let err = d.export_text(&opts(Some(&para.id))).unwrap_err();
    assert_eq!(err.code(), Code::Unsupported);
    assert_eq!(err.reason(), Some("wrong_kind"));
    let err = d.export_text(&opts(Some("n-nope"))).unwrap_err();
    assert_eq!(err.code(), Code::TargetNotFound);
}

#[test]
fn output_over_the_limit_is_refused_with_the_limit_value() {
    let d = synthetic(&para(&"가".repeat(2000)));
    for f in [TextFormat::Markdown, TextFormat::Html] {
        let err = d
            .export_text(&ExportOptions {
                format: f,
                scope: None,
                include_notes: true,
                max_bytes: 1000,
            })
            .unwrap_err();
        assert_eq!(err.code(), Code::ResourceLimit);
        assert_eq!(err.reason(), Some("text_export_too_large"));
        let detail = err.diagnostic().detail.as_ref().unwrap();
        assert_eq!(detail["limit"], "max_text_export_bytes");
        assert_eq!(detail["limitBytes"], 1000);
        assert!(err.to_string().contains("max_text_export_bytes = 1000"));
    }
}

#[test]
fn every_corpus_document_exports_deterministically_and_stays_unmodified() {
    for name in [
        "anchor_char_height.hwpx",
        "bodojaryo-reference.hwpx",
        "gani-gian.hwpx",
        "gyehoek-reference.hwpx",
        "ilban-gian.hwpx",
        "irb-sample.hwpx",
        "problem-answer-reference.hwpx",
        "ref_empty.hwpx",
        "research-school-plan-reference.hwpx",
    ] {
        let d = open(name);
        let ids: Vec<_> = d.nodes.iter().map(|n| n.id.clone()).collect();
        for f in [TextFormat::Markdown, TextFormat::Html] {
            let a = export(&d, f);
            let b = export(&d, f);
            assert_eq!(a.text, b.text, "{name} not deterministic");
            assert_eq!(a.counts, b.counts);
            let tables = d.nodes.iter().filter(|n| n.kind == Kind::Table).count();
            assert_eq!(
                a.counts.tables, tables,
                "{name}: every table node is converted"
            );
            assert!(
                !a.text.contains('\u{FFFC}'),
                "{name}: object placeholder leaked"
            );
        }
        assert!(d.is_unmodified(), "{name}");
        assert_eq!(
            ids,
            d.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn note_list_text_leaves_out_the_auto_number_placeholder() {
    // Hancom writes the note's own number as `hp:ctrl > hp:autoNum` at the
    // start of the note paragraph; it is not part of the note text.
    let body = r#"<hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:t>본문</hp:t><hp:ctrl><hp:footNote number="1"><hp:subList><hp:p paraPrIDRef="0"><hp:run charPrIDRef="0"><hp:ctrl><hp:autoNum num="1" numType="FOOTNOTE"><hp:autoNumFormat type="DIGIT" suffixChar=")"/></hp:autoNum></hp:ctrl><hp:t> 각주</hp:t><hp:ctrl><hp:bookmark name="b"/></hp:ctrl><hp:t>내용</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl></hp:run></hp:p>"#;
    let d = synthetic(body);
    let notes = d.note_list(0, None, None, None).unwrap().notes;
    assert_eq!(notes.len(), 1);
    // Only the auto-number placeholder goes; other controls keep theirs.
    assert_eq!(notes[0].text, " 각주\u{FFFC}내용");
}
