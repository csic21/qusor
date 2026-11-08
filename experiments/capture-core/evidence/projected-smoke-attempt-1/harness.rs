#[path="/workspace/qusor/experiments/capture-core/tests/component_support/fixture.rs"] mod fixture;
use extension_api::{*, projection::*};
use extension_host::durable::*;
use extension_package::package::verify_for_host;
use extension_store::Store;
use extension_runtime_api::{ExecutionLimits,RuntimeError,ModuleExecutor};
use runtime_wasm::WasmiRuntime;
use learning_application::{KnowledgeApplication,annotation_draft::AnnotationDrafts};
use qusor_capture_probe::component_projection::ProjectionServices;
use storage_sqlite::SqliteStore;
use std::{fs,path::PathBuf};
fn main(){
 let b=PathBuf::from("/tmp/qusor-component-author-b-projection");
 let package=verify_for_host(&fs::read(b.join("provider-b.qpk")).unwrap(),fs::read_to_string(b.join("publisher-public-key.txt")).unwrap().trim(),&SUPPORTED_HOST_APIS).unwrap();
 let scratch=std::env::temp_dir().join(format!("projection-smoke-{}",std::process::id()));
 fs::create_dir(&scratch).unwrap();
 let mut store=Store::open_for_host(&scratch.join("packages"),&SUPPORTED_HOST_APIS).unwrap();
 store.install("install",0,&package).unwrap();store.bind_component_vault("bind",1,&package.manifest().package_id,"vault").unwrap();
 let mut host=DurableHost::new("vault",[Capability::NoteDraftEdit]).unwrap();host.configure_component_identity([91;16]).unwrap();
 let mut core=KnowledgeApplication::new(SqliteStore::open(scratch.join("core")).unwrap());let(original,snapshot,selection)=fixture::pdf_fixture();core.import_document("import","vault","Synthetic",&original,&snapshot).unwrap();
 let cases:Vec<serde_json::Value>=serde_json::from_slice(&fs::read("/tmp/qusor-projection-wasm-smoke/cases.json").unwrap()).unwrap();
 let mut drafts=AnnotationDrafts::new();let mut total_vms=0;let mut calls=0;let mut max_fuel=0;let mut partial_seen=false;
 for (i,case)in cases.iter().enumerate(){
  let id=drafts.create_pdf(&core,1,"vault",selection.clone(),core_api::knowledge::Provenance{package_id:"synthetic.source".into(),publisher_id:"synthetic.author".into(),package_digest:"a".repeat(64),installation_id:1},1).unwrap();
  let text=case["text"].as_str().unwrap();let owner=drafts.create_owner().unwrap();let u=drafts.begin_replace(&owner,&id,1,text.len(),1).unwrap();let mut off=0;while off<text.len(){let mut end=(off+512).min(text.len());while !text.is_char_boundary(end){end-=1;}drafts.append_chunk(&owner,&u,off,&text[off..end],1).unwrap();off=end;}drafts.commit_replace(&owner,&u,1).unwrap();drafts.revoke_owner(&owner).unwrap();
  let facts=store.component_installation(&package.manifest().package_id,AuthorityRead::Activation).unwrap();let mounted=host.mount_component(&store,UserMountIntent::new(facts,Resource::Draft{vault_id:"vault".into(),draft_id:format!("{:032x}",i+1)}).unwrap(),1).unwrap();
  let mut svc=ProjectionServices::new(&drafts,&mut host,&store,mounted.handle(),id.clone(),1).unwrap();svc.start(&drafts,&mut host,&store,2,case["first"].as_u64().unwrap()as u32,case["max_rows"].as_u64().unwrap()as u8,1).unwrap();let mut finished=false;
  for vm in 0..2000{
   let invocation=svc.invocation(None).unwrap();
   let r=WasmiRuntime.execute(package.module_bytes(),&ExecutionLimits::default(),65536,&mut |raw:&[u8]|svc.call(&drafts,&mut host,&store,&invocation,raw,1,&||Ok(())).map_err(|e|{eprintln!("BROKER {e:?} request={}",String::from_utf8_lossy(raw));RuntimeError::new("broker_rejected")})).unwrap_or_else(|e|panic!("case{i} vm{vm}: {e:?}"));
   total_vms+=1;calls+=r.calls;max_fuel=max_fuel.max(r.fuel_consumed);assert!(r.calls<=6);
   let response=ProjectionResponse::from_json(&r.output).unwrap();let ProjectionResponse::Query{outcome:ProjectionOutcome::Metadata{metadata},..}=response else{panic!("not metadata")};
   if let Some(w)=&metadata.window{partial_seen|=w.rows.iter().any(|r|!r.complete);}
   if metadata.state==ProjectionState::Completed{let w=metadata.window.unwrap();assert_eq!(serde_json::to_value(&w.rows).unwrap(),case["expected"],"case{i}");assert_eq!(w.reached_eof,case["eof"].as_bool().unwrap());assert_eq!(drafts.read_chunk(&id,None,0,512).unwrap().revision,2);finished=true;eprintln!("CASE {i} passed {vm}");break;}
  }
  assert!(finished);svc.close().unwrap();host.close_component(mounted.handle()).unwrap();for t in host.pending_component_cleanup(){host.confirm_component_released(&t).unwrap();}drafts.remove(&id).unwrap();
 }
 println!("{}",serde_json::json!({"ok":true,"cases":cases.len(),"vms":total_vms,"calls":calls,"max_fuel":max_fuel,"partial_seen":partial_seen,"real_store":true,"os_workers":false,"native_widgets":false}));let _=fs::remove_dir_all(scratch);
}
