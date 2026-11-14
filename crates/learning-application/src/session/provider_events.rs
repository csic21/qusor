//! Bounded own-event input and atomic effect/checkpoint continuation.
//! A trusted caller owns fresh authorization and serializes this registry with drafts.
use crate::session::{annotation_draft::AnnotationDrafts, draft_text::*};
use core_api::{
    contract::sha256,
    knowledge::{KnowledgeError, KnowledgeErrorCode as Code, Result},
};
use std::sync::Arc;
pub const MAX_EVENT_INPUT_BYTES: usize = MAX_TEXT_BYTES;
pub const MAX_EVENT_SESSION_BYTES: usize = 8 * MAX_TEXT_BYTES;
pub const MAX_CHECKPOINT_BYTES: usize = 8192;
pub const MAX_CHECKPOINT_CHUNK_BYTES: usize = 2048;
pub const EVENT_TIMEOUT: u64 = 30;
#[derive(Clone, Debug)]
pub struct EventMount {
    scope: Arc<()>,
    id: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventCursor {
    pub sequence: u64,
    pub base_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventAction {
    InputChanged,
    MoveParagraph { from: u32, to: u32 },
    ReplaceParagraph { index: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventState {
    Active,
    Completed,
    Cancelled,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftEffect {
    BeginReplace {
        expected_revision: u64,
        total_bytes: usize,
    },
    AppendChunk {
        offset: usize,
        text: String,
    },
    CommitReplace,
    CancelReplace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectOutcome {
    Started,
    ChunkAccepted { next_offset: usize },
    Replaced { revision: u64 },
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct AdvanceEvent {
    pub cursor: EventCursor,
    pub expected_checkpoint_revision: u64,
    pub effect: Option<DraftEffect>,
    pub checkpoint: Vec<u8>,
    pub complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvanceReceipt {
    pub cursor: EventCursor,
    pub checkpoint_revision: u64,
    pub state: EventState,
    pub last_effect: Option<EffectOutcome>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSnapshot {
    pub cursor: EventCursor,
    pub action: EventAction,
    pub state: EventState,
    pub input_bytes: usize,
    pub checkpoint_bytes: usize,
    pub checkpoint_revision: u64,
    pub last_effect: Option<EffectOutcome>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct InputChunk {
    pub cursor: EventCursor,
    pub offset: usize,
    pub total_bytes: usize,
    pub text: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CheckpointChunk {
    pub cursor: EventCursor,
    pub checkpoint_revision: u64,
    pub offset: usize,
    pub total_bytes: usize,
    pub bytes: Vec<u8>,
}
struct Upload {
    id: UploadId,
    total: usize,
    received: usize,
}
struct Last {
    expected: u64,
    hash: String,
    receipt: AdvanceReceipt,
}
struct Event {
    cursor: EventCursor,
    action: EventAction,
    input: String,
    state: EventState,
    checkpoint: Vec<u8>,
    checkpoint_revision: u64,
    upload: Option<Upload>,
    last: Option<Last>,
    last_effect: Option<EffectOutcome>,
    touched: u64,
}
impl Event {
    fn snapshot(&self) -> EventSnapshot {
        EventSnapshot {
            cursor: self.cursor,
            action: self.action,
            state: self.state,
            input_bytes: self.input.len(),
            checkpoint_bytes: self.checkpoint.len(),
            checkpoint_revision: self.checkpoint_revision,
            last_effect: self.last_effect,
        }
    }
    fn receipt(&self) -> AdvanceReceipt {
        AdvanceReceipt {
            cursor: self.cursor,
            checkpoint_revision: self.checkpoint_revision,
            state: self.state,
            last_effect: self.last_effect,
        }
    }
}
struct Mount {
    id: u64,
    draft: DraftId,
    owner: MountOwner,
    sequence: u64,
    event: Option<Event>,
}
pub struct ProviderEvents {
    scope: Arc<()>,
    draft_scope: Option<Arc<()>>,
    next: u64,
    clock: u64,
    mounts: Vec<Mount>,
}
fn fail(code: Code, message: &'static str) -> KnowledgeError {
    KnowledgeError::new(code, message)
}
fn conflict(message: &'static str) -> KnowledgeError {
    fail(Code::Conflict, message)
}
fn revision(drafts: &AnnotationDrafts, id: &DraftId) -> Result<u64> {
    Ok(drafts.read_chunk(id, None, 0, MAX_CHUNK_BYTES)?.revision)
}
impl Default for ProviderEvents {
    fn default() -> Self {
        Self::new()
    }
}
impl ProviderEvents {
    pub fn new() -> Self {
        Self {
            scope: Arc::new(()),
            draft_scope: None,
            next: 0,
            clock: 0,
            mounts: Vec::new(),
        }
    }
    fn check_drafts(&self, drafts: &AnnotationDrafts) -> Result<()> {
        if self
            .draft_scope
            .as_ref()
            .is_none_or(|scope| Arc::ptr_eq(scope, &drafts.provider_scope()))
        {
            Ok(())
        } else {
            Err(conflict("foreign annotation registry"))
        }
    }
    fn index(&self, mount: &EventMount) -> Result<usize> {
        if !Arc::ptr_eq(&self.scope, &mount.scope) {
            return Err(conflict("foreign event registry"));
        }
        self.mounts
            .iter()
            .position(|m| m.id == mount.id)
            .ok_or_else(|| conflict("event mount retired"))
    }
    fn event(&self, index: usize, cursor: EventCursor) -> Result<&Event> {
        self.mounts[index]
            .event
            .as_ref()
            .filter(|event| event.cursor == cursor)
            .ok_or_else(|| conflict("event sequence or base revision changed"))
    }
    fn access(&self, drafts: &AnnotationDrafts, mount: &EventMount) -> Result<usize> {
        self.check_drafts(drafts)?;
        let index = self.index(mount)?;
        drafts.source(&self.mounts[index].draft)?;
        Ok(index)
    }
    fn activity(&mut self, drafts: &mut AnnotationDrafts, index: usize) -> Result<()> {
        if self.mounts[index]
            .event
            .as_ref()
            .is_none_or(|e| e.state != EventState::Active)
        {
            return Ok(());
        }
        let expected_upload = self.mounts[index]
            .event
            .as_ref()
            .is_some_and(|e| e.upload.is_some());
        let (clock, present) = drafts.touch_owner_uploads(&self.mounts[index].owner, self.clock)?;
        self.clock = self.clock.max(clock);
        let event = self.mounts[index].event.as_mut().expect("validated event");
        if expected_upload && !present {
            event.state = EventState::Expired;
            event.upload = None;
            event.checkpoint.clear();
            event.last = None;
            event.last_effect = None;
            return Err(conflict("event upload expired"));
        }
        event.touched = self.clock;
        Ok(())
    }
    /// Seconds from the same monotonic clock as AnnotationDrafts. Regressions clamp.
    pub fn expire(&mut self, drafts: &mut AnnotationDrafts, now: u64) -> Result<()> {
        self.check_drafts(drafts)?;
        self.clock = self.clock.max(now).max(drafts.provider_clock());
        for mount in &mut self.mounts {
            if let Some(event) = &mut mount.event
                && event.state == EventState::Active
                && self.clock.saturating_sub(event.touched) >= EVENT_TIMEOUT
            {
                if let Some(upload) = event.upload.take() {
                    let _ = drafts.cancel_replace(&mount.owner, &upload.id);
                }
                event.state = EventState::Expired;
                event.checkpoint.clear();
                event.last = None;
                event.last_effect = None;
            }
        }
        drafts.expire_uploads(self.clock);
        Ok(())
    }
    pub fn register(
        &mut self,
        drafts: &mut AnnotationDrafts,
        draft: DraftId,
    ) -> Result<EventMount> {
        self.check_drafts(drafts)?;
        if self.mounts.len() >= MAX_DRAFTS {
            return Err(fail(Code::BudgetExceeded, "event mount budget exceeded"));
        }
        if drafts.state(&draft)? != DraftState::Editing {
            return Err(conflict("draft is not editable"));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or_else(|| fail(Code::BudgetExceeded, "event mount identity exhausted"))?;
        let owner = drafts.create_owner()?;
        self.draft_scope = Some(drafts.provider_scope());
        self.next = next;
        self.mounts.push(Mount {
            id: next,
            draft,
            owner,
            sequence: 0,
            event: None,
        });
        Ok(EventMount {
            scope: self.scope.clone(),
            id: next,
        })
    }
    pub fn reserved_input_bytes(&self) -> usize {
        self.mounts
            .iter()
            .filter_map(|m| m.event.as_ref())
            .map(|e| e.input.len())
            .sum()
    }
    /// Only trusted native/user input publication; input is a complete same-draft snapshot.
    pub fn publish(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        base_revision: u64,
        action: EventAction,
        input: String,
        now: u64,
    ) -> Result<EventSnapshot> {
        let index = self.access(drafts, mount)?;
        if let EventAction::MoveParagraph { from, to } = action
            && (!input.is_empty() || from as usize > MAX_TEXT_BYTES || to as usize > MAX_TEXT_BYTES)
        {
            return Err(fail(
                Code::InvalidInput,
                "move event has unexpected input or unbounded indices",
            ));
        }
        if input.len() > MAX_EVENT_INPUT_BYTES {
            return Err(fail(
                Code::BudgetExceeded,
                "event input exceeds text budget",
            ));
        }
        if let EventAction::ReplaceParagraph { index } = action
            && index as usize > MAX_TEXT_BYTES
        {
            return Err(fail(Code::InvalidInput, "paragraph index exceeds budget"));
        }
        if let Some(pending) = self.mounts[index].event.as_ref()
            && matches!(pending.state, EventState::Active | EventState::Expired)
        {
            let may_replace = match action {
                EventAction::InputChanged => true,
                EventAction::MoveParagraph { .. } => false,
                EventAction::ReplaceParagraph { index } => {
                    pending.action == EventAction::ReplaceParagraph { index }
                        && pending.cursor.base_revision == base_revision
                }
            };
            if !may_replace {
                return Err(conflict(
                    "pending input must complete or be cancelled before another partial action",
                ));
            }
        }
        if drafts.state(&self.mounts[index].draft)? != DraftState::Editing
            || revision(drafts, &self.mounts[index].draft)? != base_revision
        {
            return Err(conflict(
                "event base revision is not current editable draft",
            ));
        }
        let old_bytes = self.mounts[index]
            .event
            .as_ref()
            .map_or(0, |e| e.input.len());
        if self.reserved_input_bytes() - old_bytes + input.len() > MAX_EVENT_SESSION_BYTES {
            return Err(fail(
                Code::BudgetExceeded,
                "event input session budget exceeded",
            ));
        }
        let sequence = self.mounts[index]
            .sequence
            .checked_add(1)
            .ok_or_else(|| fail(Code::BudgetExceeded, "event sequence exhausted"))?;
        self.expire(drafts, now)?;
        let record = &mut self.mounts[index];
        if let Some(upload) = record.event.as_ref().and_then(|e| e.upload.as_ref()) {
            drafts.cancel_replace(&record.owner, &upload.id)?;
        }
        record.sequence = sequence;
        record.event = Some(Event {
            cursor: EventCursor {
                sequence,
                base_revision,
            },
            action,
            input,
            state: EventState::Active,
            checkpoint: Vec::new(),
            checkpoint_revision: 1,
            upload: None,
            last: None,
            last_effect: None,
            touched: self.clock,
        });
        Ok(record.event.as_ref().expect("installed event").snapshot())
    }
    pub fn snapshot(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        cursor: EventCursor,
        now: u64,
    ) -> Result<EventSnapshot> {
        let index = self.access(drafts, mount)?;
        self.expire(drafts, now)?;
        self.event(index, cursor)?;
        self.activity(drafts, index)?;
        Ok(self.event(index, cursor)?.snapshot())
    }
    pub fn read_input_chunk(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        cursor: EventCursor,
        offset: usize,
        max_bytes: usize,
        now: u64,
    ) -> Result<InputChunk> {
        let index = self.access(drafts, mount)?;
        self.expire(drafts, now)?;
        let event = self.event(index, cursor)?;
        if max_bytes == 0
            || max_bytes > MAX_CHUNK_BYTES
            || offset > event.input.len()
            || !event.input.is_char_boundary(offset)
        {
            return Err(fail(Code::InvalidInput, "invalid event input chunk"));
        }
        let mut end = offset.saturating_add(max_bytes).min(event.input.len());
        while !event.input.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset && offset != event.input.len() {
            return Err(fail(
                Code::InvalidInput,
                "input chunk cannot fit next scalar",
            ));
        }
        self.activity(drafts, index)?;
        let event = self.event(index, cursor)?;
        let result = InputChunk {
            cursor,
            offset,
            total_bytes: event.input.len(),
            text: event.input[offset..end].into(),
        };
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)] // Exact event and checkpoint CAS plus bounded chunk geometry.
    pub fn read_checkpoint_chunk(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        cursor: EventCursor,
        checkpoint_revision: u64,
        offset: usize,
        max_bytes: usize,
        now: u64,
    ) -> Result<CheckpointChunk> {
        let index = self.access(drafts, mount)?;
        self.expire(drafts, now)?;
        let event = self.event(index, cursor)?;
        if matches!(event.state, EventState::Expired | EventState::Cancelled) {
            return Err(conflict("event checkpoint retired"));
        }
        if checkpoint_revision != event.checkpoint_revision {
            return Err(conflict("checkpoint revision changed"));
        }
        if max_bytes == 0
            || max_bytes > MAX_CHECKPOINT_CHUNK_BYTES
            || offset > event.checkpoint.len()
        {
            return Err(fail(Code::InvalidInput, "invalid checkpoint chunk"));
        }
        self.activity(drafts, index)?;
        let event = self.event(index, cursor)?;
        let end = offset.saturating_add(max_bytes).min(event.checkpoint.len());
        let result = CheckpointChunk {
            cursor,
            checkpoint_revision,
            offset,
            total_bytes: event.checkpoint.len(),
            bytes: event.checkpoint[offset..end].into(),
        };
        Ok(result)
    }
    pub fn advance(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        request: AdvanceEvent,
        now: u64,
    ) -> Result<AdvanceReceipt> {
        let index = self.access(drafts, mount)?;
        if request.checkpoint.len() > MAX_CHECKPOINT_BYTES {
            return Err(fail(Code::BudgetExceeded, "checkpoint exceeds budget"));
        }
        validate_effect(&request.effect, request.complete)?;
        let hash = request_hash(&request);
        self.expire(drafts, now)?;
        let event = self.event(index, request.cursor)?;
        if let Some(last) = &event.last
            && request.expected_checkpoint_revision == last.expected
        {
            if hash == last.hash {
                let receipt = last.receipt;
                self.activity(drafts, index)?;
                return Ok(receipt);
            }
            return Err(conflict("same checkpoint CAS with different payload"));
        }
        if event.state != EventState::Active
            || request.expected_checkpoint_revision != event.checkpoint_revision
        {
            return Err(conflict("event is not active or checkpoint CAS changed"));
        }
        let next = event
            .checkpoint_revision
            .checked_add(1)
            .ok_or_else(|| fail(Code::BudgetExceeded, "checkpoint revision exhausted"))?;
        let record = &self.mounts[index];
        if drafts.state(&record.draft)? != DraftState::Editing
            || revision(drafts, &record.draft)? != event.cursor.base_revision
        {
            return Err(conflict("draft revision changed during input event"));
        }
        // Validate completion and all local geometry before applying any draft effect.
        match &request.effect {
            Some(DraftEffect::BeginReplace {
                expected_revision, ..
            }) if *expected_revision != event.cursor.base_revision || event.upload.is_some() => {
                return Err(conflict(
                    "begin differs from event base or upload already active",
                ));
            }
            Some(DraftEffect::AppendChunk { offset, text }) => {
                let u = event
                    .upload
                    .as_ref()
                    .ok_or_else(|| conflict("event has no upload"))?;
                if *offset != u.received || text.len() > u.total - u.received {
                    return Err(fail(Code::InvalidInput, "event append is not consecutive"));
                }
            }
            Some(DraftEffect::CommitReplace) => {
                let u = event
                    .upload
                    .as_ref()
                    .ok_or_else(|| conflict("event has no upload"))?;
                if u.received != u.total {
                    return Err(conflict("event upload incomplete"));
                }
                event
                    .cursor
                    .base_revision
                    .checked_add(1)
                    .ok_or_else(|| fail(Code::BudgetExceeded, "draft revision exhausted"))?;
            }
            Some(DraftEffect::CancelReplace) if event.upload.is_none() => {
                return Err(conflict("event has no upload"));
            }
            None if request.complete && event.upload.is_some() => {
                return Err(conflict("cannot complete an active upload"));
            }
            _ => {}
        }
        self.activity(drafts, index)?;
        // All checkpoint/CAS/response construction failure points are above. Below,
        // a successful draft effect is followed solely by infallible bounded assignments.
        let record = &mut self.mounts[index];
        let event = record.event.as_mut().expect("validated event");
        let outcome = match request.effect {
            Some(DraftEffect::BeginReplace {
                expected_revision,
                total_bytes,
            }) => {
                let id = drafts.begin_replace(
                    &record.owner,
                    &record.draft,
                    expected_revision,
                    total_bytes,
                    self.clock,
                )?;
                event.upload = Some(Upload {
                    id,
                    total: total_bytes,
                    received: 0,
                });
                Some(EffectOutcome::Started)
            }
            Some(DraftEffect::AppendChunk { offset, text }) => {
                let u = event.upload.as_mut().expect("validated upload");
                let next_offset =
                    drafts.append_chunk(&record.owner, &u.id, offset, &text, self.clock)?;
                u.received = next_offset;
                Some(EffectOutcome::ChunkAccepted { next_offset })
            }
            Some(DraftEffect::CommitReplace) => {
                let revision = drafts.commit_replace(
                    &record.owner,
                    &event.upload.as_ref().expect("validated upload").id,
                    self.clock,
                )?;
                event.upload = None;
                Some(EffectOutcome::Replaced { revision })
            }
            Some(DraftEffect::CancelReplace) => {
                drafts.cancel_replace(
                    &record.owner,
                    &event.upload.as_ref().expect("validated upload").id,
                )?;
                event.upload = None;
                Some(EffectOutcome::Cancelled)
            }
            None => None,
        };
        event.checkpoint = request.checkpoint;
        event.checkpoint_revision = next;
        event.last_effect = outcome;
        event.touched = self.clock;
        if request.complete {
            event.state = EventState::Completed;
        }
        let receipt = event.receipt();
        event.last = Some(Last {
            expected: request.expected_checkpoint_revision,
            hash,
            receipt,
        });
        Ok(receipt)
    }
    pub fn cancel_event(
        &mut self,
        drafts: &mut AnnotationDrafts,
        mount: &EventMount,
        cursor: EventCursor,
        expected_checkpoint_revision: u64,
        now: u64,
    ) -> Result<AdvanceReceipt> {
        let index = self.access(drafts, mount)?;
        self.expire(drafts, now)?;
        let event = self.event(index, cursor)?;
        if event.state == EventState::Cancelled
            && event.checkpoint_revision.checked_sub(1) == Some(expected_checkpoint_revision)
        {
            return Ok(event.receipt());
        }
        if matches!(event.state, EventState::Completed | EventState::Cancelled)
            || event.checkpoint_revision != expected_checkpoint_revision
        {
            return Err(conflict("event cannot be cancelled at this CAS"));
        }
        let next = event
            .checkpoint_revision
            .checked_add(1)
            .ok_or_else(|| fail(Code::BudgetExceeded, "checkpoint revision exhausted"))?;
        let record = &mut self.mounts[index];
        let event = record.event.as_mut().expect("validated event");
        if let Some(u) = &event.upload {
            drafts.cancel_replace(&record.owner, &u.id)?;
        }
        event.upload = None;
        event.checkpoint.clear();
        event.last = None;
        event.last_effect = Some(EffectOutcome::Cancelled);
        event.checkpoint_revision = next;
        event.state = EventState::Cancelled;
        Ok(event.receipt())
    }
    pub fn close(&mut self, drafts: &mut AnnotationDrafts, mount: &EventMount) -> Result<()> {
        self.check_drafts(drafts)?;
        let index = self.index(mount)?;
        drafts.revoke_owner(&self.mounts[index].owner)?;
        self.mounts.remove(index);
        Ok(())
    }
}
fn validate_effect(effect: &Option<DraftEffect>, complete: bool) -> Result<()> {
    match effect {
        Some(DraftEffect::BeginReplace {
            expected_revision,
            total_bytes,
        }) => {
            if *expected_revision == 0 || *total_bytes > MAX_TEXT_BYTES {
                return Err(fail(Code::BudgetExceeded, "invalid event begin budget"));
            }
            if complete {
                return Err(conflict("begin cannot complete an event"));
            }
        }
        Some(DraftEffect::AppendChunk { offset, text }) => {
            if text.is_empty()
                || text.len() > MAX_CHUNK_BYTES
                || offset
                    .checked_add(text.len())
                    .is_none_or(|n| n > MAX_TEXT_BYTES)
            {
                return Err(fail(Code::InvalidInput, "invalid event append"));
            }
            if complete {
                return Err(conflict("append cannot complete an event"));
            }
        }
        Some(DraftEffect::CommitReplace) if !complete => {
            return Err(conflict("commit must complete the event"));
        }
        _ => {}
    }
    Ok(())
}
fn request_hash(request: &AdvanceEvent) -> String {
    let mut bytes = Vec::with_capacity(MAX_CHECKPOINT_BYTES + 1024);
    fn number(bytes: &mut Vec<u8>, v: u64) {
        bytes.extend_from_slice(&v.to_be_bytes());
    }
    number(&mut bytes, request.cursor.sequence);
    number(&mut bytes, request.cursor.base_revision);
    number(&mut bytes, request.expected_checkpoint_revision);
    bytes.push(u8::from(request.complete));
    match &request.effect {
        None => bytes.push(0),
        Some(DraftEffect::BeginReplace {
            expected_revision,
            total_bytes,
        }) => {
            bytes.push(1);
            number(&mut bytes, *expected_revision);
            number(&mut bytes, *total_bytes as u64);
        }
        Some(DraftEffect::AppendChunk { offset, text }) => {
            bytes.push(2);
            number(&mut bytes, *offset as u64);
            number(&mut bytes, text.len() as u64);
            bytes.extend_from_slice(text.as_bytes());
        }
        Some(DraftEffect::CommitReplace) => bytes.push(3),
        Some(DraftEffect::CancelReplace) => bytes.push(4),
    }
    number(&mut bytes, request.checkpoint.len() as u64);
    bytes.extend_from_slice(&request.checkpoint);
    sha256(&bytes)
}
