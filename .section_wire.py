from pathlib import Path
p=Path('crates/hae-verify/src/lib.rs');s=p.read_text().replace('mod image;','mod image;\nmod section;').replace('Alignment, CharProps, LineSpacing,','Alignment, CharProps, PageSettings, Margins, PageNumbering, Visibility, LineSpacing,')
s=s.replace('        match op {\n            VerifyOperation::TextFormat', '''        match op {
            VerifyOperation::SectionSetPage {..} | VerifyOperation::SectionSetPageNumbering {..} | VerifyOperation::SectionSetVisibility {..} => {
                if section::check(&mut expected, entry, &doc, target, op)? {
                    anchors.entry(entry.into()).and_modify(|a| a.0=0).or_insert((0,BTreeSet::new()));
                    material=true;
                }
            }
            VerifyOperation::TextFormat''');p.write_text(s)
# Registry request types are separately owned schema/API definitions.
types=Path('crates/hae-edit/src/section.rs').read_text().split('fn shape')[0].split('#[derive',1)[1]
types='#[derive'+types
# rename types and add schema derives
for a,b in [('PageSettings','SectionSetPageParams'),('PageNumbering','SectionSetPageNumberingParams'),('Visibility','SectionSetVisibilityParams')]:types=types.replace(a,b)
types=types.replace('Serialize, Deserialize)]','Serialize, Deserialize, JsonSchema)]')
for name in ['SectionSetPageParams','SectionSetPageNumberingParams','SectionSetVisibilityParams']:
    types=types.replace('pub struct '+name+' {','pub struct '+name+' {\n    pub scope: Option<String>,\n    pub section_index: Option<usize>,')
# expose precise enum constraints, integer positivity
for key,pat in [('landscape','^(NARROWLY|WIDELY)$'),('gutter_type','^(LEFT_ONLY|LEFT_RIGHT|TOP_BOTTOM)$'),('page_starts_on','^(BOTH|EVEN|ODD)$'),('border','^(SHOW_ALL|HIDE_ALL|SHOW_FIRST|HIDE_FIRST)$'),('fill','^(SHOW_ALL|HIDE_ALL|SHOW_FIRST|HIDE_FIRST)$')]:
    types=types.replace('pub '+key+': Option<String>', '#[schemars(regex(pattern = r"'+pat+'"))]\n    pub '+key+': Option<String>')
for key in ['width','height']:types=types.replace('pub '+key+': Option<u32>', '#[schemars(range(min = 1))]\n    pub '+key+': Option<u32>')
p=Path('crates/hae-exec/src/registry.rs');s=p.read_text();mark='/// Exact-match search and replace'
s=s.replace(mark,types+'''\n#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionGetPageParams { pub scope: Option<String>, pub section_index: Option<usize> }
\n'''+mark)
s=s.replace('    TextFormat(TextFormatParams),','''    SectionGetPage(SectionGetPageParams),
    SectionSetPage(SectionSetPageParams),
    SectionSetPageNumbering(SectionSetPageNumberingParams),
    SectionSetVisibility(SectionSetVisibilityParams),
    TextFormat(TextFormatParams),''')
blocks=''
for variant,name,cli,mcp,summary,mutation in [
('SectionGetPage','section.getPage','section-get-page','section_get_page','Read explicit page, margin, numbering, visibility and grid values; absent attributes are null with unspecified source; no inherited defaults inferred',False),
('SectionSetPage','section.setPage','section-set-page','section_set_page','Edit only requested existing-section page/margin attributes in HWPUNIT; reject non-positive body area; never create nodes',True),
('SectionSetPageNumbering','section.setPageNumbering','section-set-page-numbering','section_set_page_numbering','Edit only requested pageStartsOn BOTH/EVEN/ODD and page start number; preserve pic/tbl/equation counters',True),
('SectionSetVisibility','section.setVisibility','section-set-visibility','section_set_visibility','Edit only requested first-page visibility flags and border/fill; preserve all other attributes and children',True)]:
    b='true' if mutation else 'false'
    blocks+=f'''    {variant} {{
        name: "{name}", cli: "{cli}", mcp: "{mcp}",
        summary: "{summary}", category: {'Mutation' if mutation else 'Query'},
        mutates: {b}, doc: true, ver: {b}, seq: {b}, dry: {b}, hist: {b},
        stage1: true, reason: None,
        params: {variant}Params, output: {'MutationResultShape' if mutation else 'GenericResultShape'},
    }},
'''
s=s.replace('    TextFormat {\n        name:',blocks+'    TextFormat {\n        name:')
s=s.replace('            ("text_format", [false, true, false, false]),','''            ("text_format", [false, true, false, false]),
            ("section_get_page", [true,false,true,false]),
            ("section_set_page", [false,true,false,false]),
            ("section_set_page_numbering", [false,true,false,false]),
            ("section_set_visibility", [false,true,false,false]),''')
p.write_text(s)
