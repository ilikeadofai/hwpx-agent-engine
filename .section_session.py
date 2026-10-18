from pathlib import Path
p=Path('crates/hae-exec/src/lib.rs');s=p.read_text();s='mod section;\n'+s if not s.startswith('//!') else s.replace('mod registry;','mod registry;\nmod section;');p.write_text(s)
p=Path('crates/hae-exec/src/session.rs');s=p.read_text().replace('command_describe, command_list,','command_describe, command_list, SectionSetPageParams, SectionSetPageNumberingParams, SectionSetVisibilityParams,')
s=s.replace('        VerifyOperation::ParagraphFormat { entry, .. } => {','''        VerifyOperation::SectionSetPage{entry,settings,..}=> {
            add_slice_bytes(seen,total,entry.as_bytes());
            for s in [&settings.landscape,&settings.gutter_type].into_iter().flatten(){add_slice_bytes(seen,total,s.as_bytes());}
        }
        VerifyOperation::SectionSetPageNumbering{entry,settings,..}=> {
            add_slice_bytes(seen,total,entry.as_bytes());
            if let Some(s)=&settings.page_starts_on{add_slice_bytes(seen,total,s.as_bytes());}
        }
        VerifyOperation::SectionSetVisibility{entry,settings,..}=> {
            add_slice_bytes(seen,total,entry.as_bytes());
            for s in [&settings.border,&settings.fill].into_iter().flatten(){add_slice_bytes(seen,total,s.as_bytes());}
        }
        VerifyOperation::ParagraphFormat { entry, .. } => {''')
dispatch='''            ParsedCommand::SectionGetPage(p) => {
                let s=Self::require_doc_mut(&mut self.open_doc,req.document_id.as_deref())?;
                Ok((crate::section::get_page(&s.current.doc,p.scope.as_deref(),p.section_index,s.revision)?,Vec::new()))
            }
'''
for var,command in [('SectionSetPage','section.setPage'),('SectionSetPageNumbering','section.setPageNumbering'),('SectionSetVisibility','section.setVisibility')]:
    dispatch+=f'''            ParsedCommand::{var}(p) => self.handle_edit_steps(req,&[BatchStepParam{{command:"{command}".into(),params:serde_json::to_value(p).unwrap()}}]),\n'''
s=s.replace('            ParsedCommand::TextFormat(p)',dispatch+'            ParsedCommand::TextFormat(p)')
build=''
for var,command in [('SectionSetPage','section.setPage'),('SectionSetPageNumbering','section.setPageNumbering'),('SectionSetVisibility','section.setVisibility')]:
    build+=f'''            "{command}" => {{
                let p:{var}Params=serde_json::from_value(step.params.clone()).map_err(|e|wrap_err(EngineError::new(Code::InvalidRequest,Stage::Request,e.to_string())))?;
                let node=hae_edit::resolve_section(doc,p.scope.as_deref(),p.section_index).map_err(&wrap_err)?;
                let entry=node.entry.clone(); let path=doc.doc_of(&node).path(node.el);
                let mut settings=serde_json::to_value(p).unwrap();
                let obj=settings.as_object_mut().unwrap(); obj.remove("scope");obj.remove("sectionIndex");
                Ok((EditOp::{var}{{target:node.id,settings:serde_json::from_value(settings.clone()).unwrap()}},VerifyOperation::{var}{{entry,path,settings:serde_json::from_value(settings).unwrap()}}))
            }}\n'''
s=s.replace('        match step.command.as_str() {','        match step.command.as_str() {\n'+build)
p.write_text(s)
