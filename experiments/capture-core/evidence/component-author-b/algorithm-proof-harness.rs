use extension_api::{*,composition::*,continuation::*};
use learning_application::{KnowledgeApplication,annotation_draft::AnnotationDrafts,draft_text::DraftId,provider_events::*};
use storage_sqlite::SqliteStore;
use extension_runtime_api::{ModuleExecutor,ExecutionLimits,RuntimeError};
use runtime_wasm::WasmiRuntime;
use std::collections::BTreeSet;
fn now()->u64{1}
fn api(e:impl std::fmt::Display)->ApiError{eprintln!("APP {e}");ApiError::new(ErrorCode::InvalidState,"app")}
struct SmokeContext{resource:Resource,mount_id:String}
#[path="/workspace/qusor/experiments/capture-core/tests/component_support/fixture.rs"] mod fixture;
fn state(s: EventState) -> ContinuationState {
    match s {
        EventState::Active => ContinuationState::Active,
        EventState::Completed => ContinuationState::Completed,
        EventState::Cancelled => ContinuationState::Cancelled,
        EventState::Expired => ContinuationState::Expired,
    }
}
fn effect_outcome(e: EffectOutcome) -> EventEffectOutcome {
    match e {
        EffectOutcome::Started => EventEffectOutcome::Started {},
        EffectOutcome::ChunkAccepted { next_offset } => EventEffectOutcome::ChunkAccepted {
            next_offset: next_offset as u32,
        },
        EffectOutcome::Replaced { revision } => EventEffectOutcome::Replaced { revision },
        EffectOutcome::Cancelled => EventEffectOutcome::Cancelled {},
    }
}
fn metadata(s: EventSnapshot) -> ContinuationMetadata {
    ContinuationMetadata {
        checkpoint_revision: s.checkpoint_revision,
        checkpoint_bytes: s.checkpoint_bytes as u16,
        state: state(s.state),
        last_effect: s.last_effect.map(effect_outcome),
    }
}
fn effect(e: EventEffect) -> DraftEffect {
    match e {
        EventEffect::BeginReplace {
            expected_revision,
            total_bytes,
        } => DraftEffect::BeginReplace {
            expected_revision,
            total_bytes: total_bytes as usize,
        },
        EventEffect::AppendChunk { offset, text } => DraftEffect::AppendChunk {
            offset: offset as usize,
            text,
        },
        EventEffect::CommitReplace {} => DraftEffect::CommitReplace,
        EventEffect::CancelReplace {} => DraftEffect::CancelReplace,
    }
}
struct Broker {
    drafts: AnnotationDrafts,
    event_mount_draft: DraftId,
    events: ProviderEvents,
    event_mount: EventMount,
    cursor: EventCursor,
    // Fixture fault injection labels only; not an OS failure simulation result.
    injected: BTreeSet<&'static str>,
    inject: bool,
    late_input: Option<String>,
}
impl Broker {
    fn wire(
        &mut self,
        raw: &[u8],
        ctx: &SmokeContext,
    ) -> Result<(Vec<u8>, Option<&'static str>), ApiError> {
        if let Ok(r) = extension_api::draft::DraftRequest::from_slice(raw) {
            if let extension_api::draft::DraftOperation::ReadChunk {revision,offset,max_bytes}=r.operation {
                let c=self.drafts.read_chunk(&self.event_mount_draft,revision,offset as usize,max_bytes as usize).map_err(api)?;
                return Ok((extension_api::draft::DraftResponse{api_version:HOST_API_V0_3,draft_protocol:1,request_id:r.request_id,draft_id:"a".repeat(32),mount_id:ctx.mount_id.clone(),outcome:extension_api::draft::DraftOutcome::Snapshot{revision:c.revision,offset:c.offset as u32,total_bytes:c.total_bytes as u32,text:c.text}}.to_json()?.into_bytes(),None));
            }
        }
        if let Ok(r) = ComponentContextRequest::from_slice(raw) {
            let (sequence, expected_revision, outcome) = match r.operation {
                ContextOperation::ReadContext {} => {
                    let s = self
                        .events
                        .snapshot(&mut self.drafts, &self.event_mount, self.cursor, now())
                        .map_err(api)?;
                    (
                        s.cursor.sequence,
                        s.cursor.base_revision,
                        ContextOutcome::Context {
                            action: match s.action {
                                EventAction::InputChanged => ComponentAction::InputChanged {
                                    total_bytes: s.input_bytes as u32,
                                },
                                EventAction::MoveParagraph { from, to } => {
                                    ComponentAction::MoveParagraph { from, to }
                                }
                                EventAction::ReplaceParagraph { index } => {
                                    ComponentAction::ReplaceParagraph {
                                        index,
                                        input_bytes: s.input_bytes as u32,
                                    }
                                }
                            },
                        },
                    )
                }
                ContextOperation::ReadInputChunk {
                    sequence,
                    expected_revision,
                    offset,
                    max_bytes,
                } => {
                    let cursor = EventCursor {
                        sequence,
                        base_revision: expected_revision,
                    };
                    let c = self
                        .events
                        .read_input_chunk(
                            &mut self.drafts,
                            &self.event_mount,
                            cursor,
                            offset as usize,
                            max_bytes as usize,
                            now(),
                        )
                        .map_err(api)?;
                    (
                        sequence,
                        expected_revision,
                        ContextOutcome::InputChunk {
                            offset: c.offset as u32,
                            total_bytes: c.total_bytes as u32,
                            text: c.text,
                        },
                    )
                }
            };
            return Ok((
                ComponentContextResponse {
                    api_version: HOST_API_V0_3,
                    context_protocol: COMPONENT_CONTEXT_PROTOCOL,
                    request_id: r.request_id,
                    resource: ctx.resource.clone(),
                    mount_id: ctx.mount_id.clone(),
                    sequence,
                    expected_revision,
                    outcome,
                }
                .to_json()?
                .into_bytes(),
                None,
            ));
        }
        let r = ContinuationRequest::from_slice(raw)?;
        let cursor = EventCursor {
            sequence: r.sequence,
            base_revision: r.base_revision,
        };
        let mut fault = None;
        let outcome = match r.operation {
            ContinuationOperation::ReadMetadata {} => ContinuationOutcome::Metadata {
                metadata: metadata(
                    self.events
                        .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                        .map_err(api)?,
                ),
            },
            ContinuationOperation::ReadCheckpointChunk {
                checkpoint_revision,
                offset,
                max_bytes,
            } => {
                let c = self
                    .events
                    .read_checkpoint_chunk(
                        &mut self.drafts,
                        &self.event_mount,
                        cursor,
                        checkpoint_revision,
                        offset as usize,
                        max_bytes as usize,
                        now(),
                    )
                    .map_err(api)?;
                ContinuationOutcome::CheckpointChunk {
                    checkpoint_revision: c.checkpoint_revision,
                    offset: c.offset as u16,
                    total_bytes: c.total_bytes as u16,
                    checkpoint_base64: encode_checkpoint(&c.bytes)?,
                }
            }
            ContinuationOperation::AdvanceEvent {
                expected_checkpoint_revision,
                checkpoint_base64,
                effect: wire_effect,
                complete,
            } => {
                let label = match &wire_effect {
                    Some(EventEffect::BeginReplace { .. }) => Some("begin_reply_lost"),
                    Some(EventEffect::AppendChunk { .. }) => Some("append_reply_lost"),
                    Some(EventEffect::CommitReplace {}) => Some("commit_reply_lost"),
                    _ => None,
                };
                self.events
                    .advance(
                        &mut self.drafts,
                        &self.event_mount,
                        AdvanceEvent {
                            cursor,
                            expected_checkpoint_revision,
                            checkpoint: decode_checkpoint(
                                &checkpoint_base64,
                                extension_api::continuation::MAX_CHECKPOINT_BYTES,
                            )?,
                            effect: wire_effect.map(effect),
                            complete,
                        },
                        now(),
                    )
                    .map_err(api)?;
                if self.inject
                    && let Some(label) = label
                    && self.injected.insert(label)
                {
                    fault = Some(label);
                }
                ContinuationOutcome::Metadata {
                    metadata: metadata(
                        self.events
                            .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                            .map_err(api)?,
                    ),
                }
            }
            ContinuationOperation::CancelEvent {
                expected_checkpoint_revision,
            } => {
                self.events
                    .cancel_event(
                        &mut self.drafts,
                        &self.event_mount,
                        cursor,
                        expected_checkpoint_revision,
                        now(),
                    )
                    .map_err(api)?;
                ContinuationOutcome::Metadata {
                    metadata: metadata(
                        self.events
                            .snapshot(&mut self.drafts, &self.event_mount, cursor, now())
                            .map_err(api)?,
                    ),
                }
            }
        };
        Ok((
            ContinuationResponse {
                api_version: HOST_API_V0_3,
                continuation_protocol: CONTINUATION_PROTOCOL,
                request_id: r.request_id,
                resource: ctx.resource.clone(),
                mount_id: ctx.mount_id.clone(),
                sequence: r.sequence,
                base_revision: r.base_revision,
                outcome,
            }
            .to_json()?
            .into_bytes(),
            fault,
        ))
    }
}
fn main(){let wasm=std::fs::read("/tmp/component_paragraph.wasm").unwrap();let cases:serde_json::Value=serde_json::from_slice(&std::fs::read("/tmp/qusor-paragraph-proof/corrupt-cases.json").unwrap()).unwrap();
for (i,c) in cases.as_array().unwrap().iter().enumerate(){let dir=format!("/tmp/paragraph-proof-core-{}-{i}",std::process::id());let mut app=KnowledgeApplication::new(SqliteStore::open(&dir).unwrap());let(original,snapshot,selection)=fixture::pdf_fixture();app.import_document("import","vault","Synthetic",&original,&snapshot).unwrap();let origin=core_api::knowledge::Provenance{package_id:"ordinary.capture".into(),publisher_id:"ordinary.author".into(),package_digest:"a".repeat(64),installation_id:1};let mut drafts=AnnotationDrafts::new();let id=drafts.create_pdf(&app,1,"vault",selection,origin,1).unwrap();let text=c["text"].as_str().unwrap();let owner=drafts.create_owner().unwrap();let u=drafts.begin_replace(&owner,&id,1,text.len(),1).unwrap();let mut p=0;while p<text.len(){let mut e=(p+512).min(text.len());while !text.is_char_boundary(e){e-=1;}drafts.append_chunk(&owner,&u,p,&text[p..e],1).unwrap();p=e;}drafts.commit_replace(&owner,&u,1).unwrap();drafts.revoke_owner(&owner).unwrap();let mut events=ProviderEvents::new();let m=events.register(&mut drafts,id.clone()).unwrap();let op=&c["operation"];let(action,input)=match op["kind"].as_str().unwrap(){"move"=>(EventAction::MoveParagraph{from:op["from"].as_u64().unwrap() as u32,to:op["to"].as_u64().unwrap() as u32},String::new()),"input"=>(EventAction::InputChanged,op["input"].as_str().unwrap().to_owned()),"replace"=>(EventAction::ReplaceParagraph{index:op["index"].as_u64().unwrap() as u32},op["input"].as_str().unwrap().to_owned()),_=>unreachable!()};let cursor=events.publish(&mut drafts,&m,2,action,input,1).unwrap().cursor;let mut broker=Broker{drafts,event_mount_draft:id.clone(),events,event_mount:m,cursor,injected:BTreeSet::new(),inject:false,late_input:None};let ctx=SmokeContext{resource:Resource::Draft{vault_id:"vault".into(),draft_id:"a".repeat(32)},mount_id:"b".repeat(32)};let mut finished=false;let mut total_calls=0;let mut max_fuel=0;for vm in 0..10000{let result=WasmiRuntime.execute(&wasm,&ExecutionLimits::default(),65536,&mut |raw: &[u8]|{broker.wire(raw,&ctx).map(|v|corrupt_checkpoint(v.0,c.get("corrupt").and_then(|v|v.as_str()),raw)).map_err(|e|{eprintln!("BROKER {e} raw={}",String::from_utf8_lossy(raw));RuntimeError::new("broker_rejected")})}).unwrap_or_else(|e|panic!("{} vm={vm} {e:?}",c["id"]));total_calls+=result.calls;max_fuel=max_fuel.max(result.fuel_consumed);let value:serde_json::Value=serde_json::from_str(&result.output).unwrap();if vm<3||vm%100==0{eprintln!("TRACE {} vm={vm} calls={} fuel={} {}",c["id"],result.calls,result.fuel_consumed,value);}if value["status"]=="rejected"{assert!(c.get("expected_error").is_some(),"{} rejected {value}",c["id"]);finished=true;println!("RESULT {}",serde_json::json!({"id":c["id"],"outcome":"rejected","vms":vm+1,"calls":total_calls,"max_fuel":max_fuel}));let mut actual=String::new();let mut p=0;loop{let x=broker.drafts.read_chunk(&id,None,p,512).unwrap();p+=x.text.len();actual+=&x.text;if p==x.total_bytes{break;}}assert_eq!(actual,text);assert_eq!(broker.events.snapshot(&mut broker.drafts,&broker.event_mount,broker.cursor,1).unwrap().state,EventState::Active);break;}let s=broker.events.snapshot(&mut broker.drafts,&broker.event_mount,broker.cursor,1).unwrap();if s.state==EventState::Completed{assert!(c.get("expected_error").is_none(),"{} expected rejection",c["id"]);let mut actual=String::new();let mut offset=0;loop{let ch=broker.drafts.read_chunk(&id,None,offset,512).unwrap();offset+=ch.text.len();actual+=&ch.text;if offset==ch.total_bytes{break;}}assert_eq!(actual,c["expected"].as_str().unwrap(),"{}",c["id"]);println!("RESULT {}",serde_json::json!({"id":c["id"],"outcome":"passed","vms":vm+1,"calls":total_calls,"max_fuel":max_fuel}));finished=true;break;}}
assert!(finished,"{} stuck",c["id"]);let _=std::fs::remove_dir_all(dir);}}

fn corrupt_checkpoint(mut bytes:Vec<u8>,kind:Option<&str>,raw:&[u8])->Vec<u8>{
 use base64::{Engine,engine::general_purpose::STANDARD};
 let Some(kind)=kind else{return bytes;};
 let req:serde_json::Value=serde_json::from_slice(raw).unwrap();if req["operation"]["kind"]!="read_checkpoint_chunk"{return bytes;}
 let mut value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let encoded=value["outcome"]["checkpoint_base64"].as_str().unwrap();
 if kind=="base64"{value["outcome"]["checkpoint_base64"]=format!("{encoded}\n").into();return serde_json::to_vec(&value).unwrap();}
 let packed=STANDARD.decode(encoded).unwrap();let mut decoded=Vec::new();let mut i=3;while i<packed.len(){let c=packed[i];i+=1;let n=usize::from(c&127)+1;if c&128!=0{decoded.resize(decoded.len()+n,0);}else{decoded.extend_from_slice(&packed[i..i+n]);i+=n;}}
 let offset=match kind{"phase"=>4,"action"=>8,"cache-n"=>132,"span"=>44,"packed-overflow"=>0,_=>unreachable!()};
 let mut next=vec![1,packed[1],packed[2]];if kind=="packed-overflow"{next.extend_from_slice(&[255;10]);}else{decoded[offset..offset+4].copy_from_slice(&u32::MAX.to_le_bytes());for chunk in decoded.chunks(128){next.push((chunk.len()-1)as u8);next.extend_from_slice(chunk);}}
 value["outcome"]["checkpoint_base64"]=STANDARD.encode(next).into();bytes=serde_json::to_vec(&value).unwrap();bytes
}
