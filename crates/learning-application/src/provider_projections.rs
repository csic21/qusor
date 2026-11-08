//! Bounded, read-only provider projections, separate from editing continuations.
//!
//! The trusted coordinator owns fresh mount authorization and serializes access
//! with drafts. A window is a provider view: range validation does not prove
//! paragraph ordinals or EOF are semantically truthful. This registry never
//! parses paragraphs, writes draft text, or touches an editing owner/upload.
use crate::{
    annotation_draft::AnnotationDrafts,
    draft_text::{DraftId, MAX_CHUNK_BYTES, MAX_DRAFTS, MAX_TEXT_BYTES},
};
use core_api::{
    contract::sha256,
    knowledge::{KnowledgeError, KnowledgeErrorCode as Code, Result},
};
use std::sync::Arc;

pub const MAX_PROJECTION_ROWS: usize = 8;
pub const MAX_PROJECTION_CHECKPOINT_BYTES: usize = 8192;
pub const MAX_PROJECTION_CHECKPOINT_CHUNK_BYTES: usize = 2048;
pub const PROJECTION_TIMEOUT: u64 = 30;

#[derive(Clone, Debug)]
pub struct ProjectionMount {
    scope: Arc<()>,
    id: u64,
}
#[derive(Clone, Debug)]
pub struct QueryToken {
    scope: Arc<()>,
    mount_id: u64,
    generation: u64,
    base_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionContext {
    pub generation: u64,
    pub base_revision: u64,
    pub first_paragraph: u32,
    pub max_rows: u8,
    pub total_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionState {
    Active,
    Completed,
    Cancelled,
    Invalidated,
    Expired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSpan {
    pub ordinal: u32,
    pub start: usize,
    pub end: usize,
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionWindow {
    pub rows: Vec<RowSpan>,
    pub reached_eof: bool,
}
#[derive(Clone, Debug)]
pub struct AdvanceProjection {
    pub expected_checkpoint_revision: u64,
    pub checkpoint: Vec<u8>,
    pub window: Option<ProjectionWindow>,
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    pub context: ProjectionContext,
    pub state: ProjectionState,
    pub checkpoint_revision: u64,
    pub checkpoint_bytes: usize,
    pub window: Option<ProjectionWindow>,
    pub window_ready: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionCheckpointChunk {
    pub checkpoint_revision: u64,
    pub offset: usize,
    pub total_bytes: usize,
    pub bytes: Vec<u8>,
}
struct Last {
    expected: u64,
    hash: String,
    receipt: ProjectionSnapshot,
}
struct Query {
    context: ProjectionContext,
    state: ProjectionState,
    checkpoint_revision: u64,
    checkpoint: Vec<u8>,
    window: Option<ProjectionWindow>,
    last: Option<Last>,
    touched: u64,
}
impl Query {
    fn snapshot(&self) -> ProjectionSnapshot {
        ProjectionSnapshot {
            context: self.context,
            state: self.state,
            checkpoint_revision: self.checkpoint_revision,
            checkpoint_bytes: self.checkpoint.len(),
            window: self.window.clone(),
            window_ready: self
                .window
                .as_ref()
                .is_some_and(|w| ready(&self.context, w)),
        }
    }
    fn retire(&mut self, state: ProjectionState) {
        self.state = state;
        self.checkpoint.clear();
        self.window = None;
        self.last = None;
    }
}
struct Mount {
    id: u64,
    draft: DraftId,
    generation: u64,
    query: Option<Query>,
}
pub struct ProviderProjections {
    scope: Arc<()>,
    draft_scope: Option<Arc<()>>,
    next: u64,
    clock: u64,
    mounts: Vec<Mount>,
}
fn error(code: Code, message: &'static str) -> KnowledgeError {
    KnowledgeError::new(code, message)
}
fn conflict(message: &'static str) -> KnowledgeError {
    error(Code::Conflict, message)
}
fn ready(context: &ProjectionContext, window: &ProjectionWindow) -> bool {
    window.rows.iter().all(|r| r.complete)
        && (window.reached_eof || window.rows.len() == usize::from(context.max_rows))
}
impl Default for ProviderProjections {
    fn default() -> Self {
        Self::new()
    }
}
impl ProviderProjections {
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
            .is_none_or(|s| Arc::ptr_eq(s, &drafts.provider_scope()))
        {
            Ok(())
        } else {
            Err(conflict("foreign annotation registry"))
        }
    }
    fn mount_index(&self, mount: &ProjectionMount) -> Result<usize> {
        if !Arc::ptr_eq(&self.scope, &mount.scope) {
            return Err(conflict("foreign projection mount"));
        }
        self.mounts
            .iter()
            .position(|m| m.id == mount.id)
            .ok_or_else(|| conflict("projection mount retired"))
    }
    fn query_index(&self, drafts: &AnnotationDrafts, token: &QueryToken) -> Result<usize> {
        self.check_drafts(drafts)?;
        if !Arc::ptr_eq(&self.scope, &token.scope) {
            return Err(conflict("foreign projection query"));
        }
        self.mounts
            .iter()
            .position(|m| {
                m.id == token.mount_id
                    && m.query.as_ref().is_some_and(|q| {
                        q.context.generation == token.generation
                            && q.context.base_revision == token.base_revision
                    })
            })
            .ok_or_else(|| conflict("projection query superseded or retired"))
    }
    fn current(&mut self, drafts: &AnnotationDrafts, token: &QueryToken) -> Result<usize> {
        let index = self.query_index(drafts, token)?;
        if let Err(e) = drafts.read_chunk(
            &self.mounts[index].draft,
            Some(token.base_revision),
            0,
            MAX_CHUNK_BYTES,
        ) {
            self.mounts[index]
                .query
                .as_mut()
                .expect("checked query")
                .retire(ProjectionState::Invalidated);
            return Err(e);
        }
        Ok(index)
    }
    fn query(&self, index: usize) -> &Query {
        self.mounts[index].query.as_ref().expect("checked query")
    }
    fn query_mut(&mut self, index: usize) -> &mut Query {
        self.mounts[index].query.as_mut().expect("checked query")
    }
    fn tick(&mut self, now: u64) {
        self.clock = self.clock.max(now);
        for m in &mut self.mounts {
            if let Some(q) = &mut m.query
                && q.state == ProjectionState::Active
                && self.clock.saturating_sub(q.touched) >= PROJECTION_TIMEOUT
            {
                q.retire(ProjectionState::Expired);
            }
        }
    }
    fn touch(&mut self, index: usize) {
        let clock = self.clock;
        if self.query(index).state == ProjectionState::Active {
            self.query_mut(index).touched = clock;
        }
    }
    pub fn register(
        &mut self,
        drafts: &AnnotationDrafts,
        draft: DraftId,
    ) -> Result<ProjectionMount> {
        self.check_drafts(drafts)?;
        drafts.read_chunk(&draft, None, 0, MAX_CHUNK_BYTES)?;
        if self.mounts.len() >= MAX_DRAFTS {
            return Err(error(
                Code::BudgetExceeded,
                "projection mount budget exceeded",
            ));
        }
        let id = self
            .next
            .checked_add(1)
            .ok_or_else(|| error(Code::BudgetExceeded, "projection mount identity exhausted"))?;
        self.next = id;
        self.draft_scope = Some(drafts.provider_scope());
        self.mounts.push(Mount {
            id,
            draft,
            generation: 0,
            query: None,
        });
        Ok(ProjectionMount {
            scope: self.scope.clone(),
            id,
        })
    }
    /// Trusted viewport publication. Invalid requests leave the current query intact.
    pub fn start(
        &mut self,
        drafts: &AnnotationDrafts,
        mount: &ProjectionMount,
        base_revision: u64,
        first_paragraph: u32,
        max_rows: u8,
        now: u64,
    ) -> Result<QueryToken> {
        self.check_drafts(drafts)?;
        let index = self.mount_index(mount)?;
        if first_paragraph as usize > MAX_TEXT_BYTES
            || !(1..=MAX_PROJECTION_ROWS as u8).contains(&max_rows)
        {
            return Err(error(
                Code::InvalidInput,
                "projection viewport exceeds bounds",
            ));
        }
        let source = drafts.read_chunk(
            &self.mounts[index].draft,
            Some(base_revision),
            0,
            MAX_CHUNK_BYTES,
        )?;
        let generation = self.mounts[index]
            .generation
            .checked_add(1)
            .ok_or_else(|| error(Code::BudgetExceeded, "projection generation exhausted"))?;
        self.tick(now);
        self.mounts[index].generation = generation;
        self.mounts[index].query = Some(Query {
            context: ProjectionContext {
                generation,
                base_revision,
                first_paragraph,
                max_rows,
                total_bytes: source.total_bytes,
            },
            state: ProjectionState::Active,
            checkpoint_revision: 1,
            checkpoint: Vec::new(),
            window: None,
            last: None,
            touched: self.clock,
        });
        Ok(QueryToken {
            scope: self.scope.clone(),
            mount_id: mount.id,
            generation,
            base_revision,
        })
    }
    pub fn metadata(
        &mut self,
        drafts: &AnnotationDrafts,
        token: &QueryToken,
        now: u64,
    ) -> Result<ProjectionSnapshot> {
        let index = self.current(drafts, token)?;
        self.tick(now);
        self.touch(index);
        Ok(self.query(index).snapshot())
    }
    pub fn read_checkpoint(
        &mut self,
        drafts: &AnnotationDrafts,
        token: &QueryToken,
        checkpoint_revision: u64,
        offset: usize,
        max_bytes: usize,
        now: u64,
    ) -> Result<ProjectionCheckpointChunk> {
        let index = self.current(drafts, token)?;
        let q = self.query(index);
        if max_bytes == 0
            || max_bytes > MAX_PROJECTION_CHECKPOINT_CHUNK_BYTES
            || offset > q.checkpoint.len()
        {
            return Err(error(
                Code::InvalidInput,
                "invalid projection checkpoint chunk",
            ));
        }
        if checkpoint_revision != q.checkpoint_revision {
            return Err(conflict("projection checkpoint changed"));
        }
        self.tick(now);
        if !matches!(
            self.query(index).state,
            ProjectionState::Active | ProjectionState::Completed
        ) {
            return Err(conflict("projection checkpoint no longer readable"));
        }
        self.touch(index);
        let q = self.query(index);
        Ok(ProjectionCheckpointChunk {
            checkpoint_revision,
            offset,
            total_bytes: q.checkpoint.len(),
            bytes: q.checkpoint[offset..(offset + max_bytes).min(q.checkpoint.len())].to_vec(),
        })
    }
    pub fn advance(
        &mut self,
        drafts: &AnnotationDrafts,
        token: &QueryToken,
        request: AdvanceProjection,
        now: u64,
    ) -> Result<ProjectionSnapshot> {
        let index = self.current(drafts, token)?;
        if request.checkpoint.len() > MAX_PROJECTION_CHECKPOINT_BYTES {
            return Err(error(
                Code::BudgetExceeded,
                "projection checkpoint exceeds budget",
            ));
        }
        let q = self.query(index);
        if let Some(window) = &request.window {
            validate_window(drafts, &self.mounts[index].draft, &q.context, window)?;
        }
        let hash = request_hash(&request);
        if let Some(last) = &q.last
            && request.expected_checkpoint_revision == last.expected
        {
            if hash != last.hash {
                return Err(conflict("projection replay differs"));
            }
            let receipt = last.receipt.clone();
            self.tick(now);
            if !matches!(
                self.query(index).state,
                ProjectionState::Active | ProjectionState::Completed
            ) {
                return Err(conflict("projection replay no longer live"));
            }
            self.touch(index);
            return Ok(receipt);
        }
        if request.expected_checkpoint_revision != q.checkpoint_revision {
            return Err(conflict("projection checkpoint CAS failed"));
        }
        if q.state != ProjectionState::Active {
            return Err(conflict("projection is not active"));
        }
        if let (Some(old), Some(next)) = (&q.window, &request.window) {
            validate_progress(old, next)?;
        }
        let next_window = request.window.as_ref().or(q.window.as_ref());
        if request.complete && !next_window.is_some_and(|w| ready(&q.context, w)) {
            return Err(conflict("projection cannot complete an unfinished window"));
        }
        let next_revision = q.checkpoint_revision.checked_add(1).ok_or_else(|| {
            error(
                Code::BudgetExceeded,
                "projection checkpoint revision exhausted",
            )
        })?;
        self.tick(now);
        if self.query(index).state != ProjectionState::Active {
            return Err(conflict("projection expired"));
        }
        let clock = self.clock;
        let q = self.query_mut(index);
        q.checkpoint = request.checkpoint;
        if let Some(window) = request.window {
            q.window = Some(window);
        }
        q.checkpoint_revision = next_revision;
        q.touched = clock;
        if request.complete {
            q.state = ProjectionState::Completed;
        }
        let receipt = q.snapshot();
        q.last = Some(Last {
            expected: request.expected_checkpoint_revision,
            hash,
            receipt: receipt.clone(),
        });
        Ok(receipt)
    }
    pub fn cancel(
        &mut self,
        drafts: &AnnotationDrafts,
        token: &QueryToken,
        expected_checkpoint_revision: u64,
        now: u64,
    ) -> Result<ProjectionSnapshot> {
        let index = self.current(drafts, token)?;
        let q = self.query(index);
        if q.state == ProjectionState::Cancelled
            && q.checkpoint_revision.checked_sub(1) == Some(expected_checkpoint_revision)
        {
            return Ok(q.snapshot());
        }
        if expected_checkpoint_revision != q.checkpoint_revision {
            return Err(conflict("projection cancel CAS failed"));
        }
        if q.state != ProjectionState::Active {
            return Err(conflict("projection is not active"));
        }
        let next = q.checkpoint_revision.checked_add(1).ok_or_else(|| {
            error(
                Code::BudgetExceeded,
                "projection checkpoint revision exhausted",
            )
        })?;
        self.tick(now);
        if self.query(index).state != ProjectionState::Active {
            return Err(conflict("projection expired"));
        }
        self.query_mut(index).retire(ProjectionState::Cancelled);
        self.query_mut(index).checkpoint_revision = next;
        Ok(self.query(index).snapshot())
    }
    /// Trusted cleanup remains possible after the draft disappears or changes.
    pub fn close(&mut self, mount: &ProjectionMount) -> Result<()> {
        let index = self.mount_index(mount)?;
        self.mounts.remove(index);
        Ok(())
    }
    /// Projection-only timer. Never calls editing activity/expiry methods.
    pub fn expire(&mut self, drafts: &AnnotationDrafts, now: u64) -> Result<()> {
        self.check_drafts(drafts)?;
        for mount in &mut self.mounts {
            if let Some(q) = &mut mount.query
                && drafts
                    .read_chunk(
                        &mount.draft,
                        Some(q.context.base_revision),
                        0,
                        MAX_CHUNK_BYTES,
                    )
                    .is_err()
            {
                q.retire(ProjectionState::Invalidated);
            }
        }
        self.tick(now);
        Ok(())
    }
}
fn validate_window(
    drafts: &AnnotationDrafts,
    draft: &DraftId,
    context: &ProjectionContext,
    window: &ProjectionWindow,
) -> Result<()> {
    if window.rows.len() > usize::from(context.max_rows) {
        return Err(error(Code::BudgetExceeded, "too many projection rows"));
    }
    if window.rows.is_empty() && (!window.reached_eof || context.first_paragraph == 0) {
        return Err(error(
            Code::InvalidInput,
            "empty projection requires EOF beyond the first paragraph",
        ));
    }
    let mut previous_end = 0;
    for (i, row) in window.rows.iter().enumerate() {
        if context.first_paragraph.checked_add(i as u32) != Some(row.ordinal)
            || row.ordinal as usize > MAX_TEXT_BYTES
            || row.start > row.end
            || row.end > context.total_bytes
            || (i > 0 && row.start < previous_end)
            || (!row.complete && (i + 1 != window.rows.len() || window.reached_eof))
        {
            return Err(error(Code::InvalidInput, "invalid projection row span"));
        }
        // read_chunk checks scalar boundaries; no paragraph parsing is performed.
        drafts.read_chunk(
            draft,
            Some(context.base_revision),
            row.start,
            MAX_CHUNK_BYTES,
        )?;
        drafts.read_chunk(draft, Some(context.base_revision), row.end, MAX_CHUNK_BYTES)?;
        previous_end = row.end;
    }
    if context.total_bytes == 0
        && context.first_paragraph == 0
        && (!window.reached_eof
            || window.rows.as_slice()
                != [RowSpan {
                    ordinal: 0,
                    start: 0,
                    end: 0,
                    complete: true,
                }])
    {
        return Err(error(
            Code::InvalidInput,
            "empty text has one complete empty paragraph",
        ));
    }
    Ok(())
}
fn validate_progress(old: &ProjectionWindow, next: &ProjectionWindow) -> Result<()> {
    if next.rows.len() < old.rows.len() || (old.reached_eof && old != next) {
        return Err(conflict("projection window cannot regress"));
    }
    for (old, next) in old.rows.iter().zip(&next.rows) {
        if old.complete {
            if old != next {
                return Err(conflict("completed projection row changed"));
            }
        } else if old.ordinal != next.ordinal || old.start != next.start || next.end < old.end {
            return Err(conflict("partial projection row regressed"));
        }
    }
    Ok(())
}
fn request_hash(request: &AdvanceProjection) -> String {
    let mut bytes = b"qusor-paragraph-projection/v1\0".to_vec();
    bytes.extend_from_slice(&(request.checkpoint.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&request.checkpoint);
    bytes.push(u8::from(request.complete));
    if let Some(window) = &request.window {
        bytes.push(1);
        bytes.push(u8::from(window.reached_eof));
        bytes.extend_from_slice(&(window.rows.len() as u64).to_le_bytes());
        for row in &window.rows {
            bytes.extend_from_slice(&row.ordinal.to_le_bytes());
            bytes.extend_from_slice(&(row.start as u64).to_le_bytes());
            bytes.extend_from_slice(&(row.end as u64).to_le_bytes());
            bytes.push(u8::from(row.complete));
        }
    } else {
        bytes.push(0);
    }
    sha256(&bytes)
}
