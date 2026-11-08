//! Restricted Surface operations inside a fresh durable-authority gate.
use crate::{Host, Session, durable::DurableGrant, surface::*};
use extension_api::surface::{
    FrameMetadata, SurfaceDescriptor, SurfaceOperation, SurfaceRequest, SurfaceResponse,
};
use extension_api::{ApiError, ApiVersion, Capability, ErrorCode, Resource};

pub struct DurableSurface<'a> {
    host: &'a mut Host,
    session: &'a Session,
    grants: &'a [DurableGrant],
}

impl<'a> DurableSurface<'a> {
    pub(crate) fn new(
        host: &'a mut Host,
        session: &'a Session,
        grants: &'a [DurableGrant],
    ) -> Self {
        Self {
            host,
            session,
            grants,
        }
    }
    fn version(
        &self,
        capability: Capability,
        resource: &Resource,
        version: &str,
    ) -> Result<(), ApiError> {
        if self.grants.iter().any(|g| {
            g.grant.capability == capability
                && g.grant.resource == *resource
                && g.resource_version.as_deref() == Some(version)
        }) {
            Ok(())
        } else {
            Err(ApiError::new(
                ErrorCode::PermissionDenied,
                "surface source differs from durable authorization",
            ))
        }
    }
    pub fn session_api(&self) -> Result<ApiVersion, ApiError> {
        self.host.session_api(self.session)
    }
    pub fn surface_state(&self, handle: &str) -> Result<SurfaceDescriptor, ApiError> {
        let state = self.host.surface_state(self.session, handle)?;
        self.version(
            Capability::ReaderSurface,
            &state.resource,
            &state.source.extraction_revision,
        )?;
        Ok(state)
    }
    pub fn surface_result_state(&self, handle: &str) -> Result<SurfaceDescriptor, ApiError> {
        let state = self.host.surface_result_state(self.session, handle)?;
        self.version(
            Capability::ReaderSurface,
            &state.resource,
            &state.source.extraction_revision,
        )?;
        Ok(state)
    }
    pub fn preflight_surface_request(&self, request: &SurfaceRequest) -> Result<(), ApiError> {
        request.validate()?;
        match &request.operation {
            SurfaceOperation::Open {
                resource, expected, ..
            } => {
                self.version(
                    Capability::ReaderSurface,
                    resource,
                    &expected.extraction_revision,
                )?;
            }
            SurfaceOperation::Configure { handle, .. } | SurfaceOperation::Close { handle } => {
                self.surface_state(handle)?;
            }
            SurfaceOperation::ReadSelection { handle, .. } => {
                let state = self.surface_state(handle)?;
                self.version(
                    Capability::SelectionRead,
                    &state.resource,
                    &state.source.extraction_revision,
                )?;
            }
        }
        self.host.preflight_surface_request(self.session, request)
    }
    pub fn open_surface(
        &mut self,
        request: &SurfaceRequest,
        document: TrustedSurfaceDocument,
    ) -> Result<SurfaceTransition, ApiError> {
        self.preflight_surface_request(request)?;
        self.host.open_surface(self.session, request, document)
    }
    pub fn configure_surface(
        &mut self,
        request: &SurfaceRequest,
    ) -> Result<SurfaceTransition, ApiError> {
        self.preflight_surface_request(request)?;
        self.host.configure_surface(self.session, request)
    }
    pub fn close_surface(
        &mut self,
        request: &SurfaceRequest,
    ) -> Result<SurfaceDescriptor, ApiError> {
        self.preflight_surface_request(request)?;
        self.host.close_surface(self.session, request)
    }
    pub fn read_surface_selection(
        &self,
        request: &SurfaceRequest,
    ) -> Result<SurfaceResponse, ApiError> {
        self.preflight_surface_request(request)?;
        self.host.read_surface_selection(self.session, request)
    }
    pub fn native_frame_ready(
        &mut self,
        ticket: &NativeFrameTicket,
        frame: FrameMetadata,
    ) -> Result<SurfaceDescriptor, ApiError> {
        // Lookup through this session before accepting any private ticket: a fresh
        // session for B must never publish A's stale asynchronous frame or gesture.
        self.surface_state(ticket.handle())?;
        self.host.native_frame_ready(ticket, frame)
    }
    pub fn native_frame_failed(
        &mut self,
        ticket: &NativeFrameTicket,
    ) -> Result<SurfaceDescriptor, ApiError> {
        self.surface_state(ticket.handle())?;
        self.host.native_frame_failed(ticket)
    }
    pub fn native_input_ticket(
        &self,
        ticket: &NativeFrameTicket,
    ) -> Result<NativeInputTicket, ApiError> {
        self.surface_state(ticket.handle())?;
        self.host.native_input_ticket(ticket)
    }
    pub fn surface_begin_gesture(
        &mut self,
        ticket: &NativeInputTicket,
    ) -> Result<NativeGestureTicket, ApiError> {
        self.surface_state(ticket.handle())?;
        self.host.surface_begin_gesture(ticket)
    }
    pub fn publish_surface_selection(
        &mut self,
        gesture: &NativeGestureTicket,
        selection: TrustedSurfaceSelection,
    ) -> Result<String, ApiError> {
        self.surface_state(gesture.handle())?;
        self.host.publish_surface_selection(gesture, selection)
    }
    pub fn cancel_surface_gesture(
        &mut self,
        gesture: &NativeGestureTicket,
    ) -> Result<(), ApiError> {
        self.surface_state(gesture.handle())?;
        self.host.cancel_surface_gesture(gesture)
    }
    pub fn clear_surface_selection(&mut self, input: &NativeInputTicket) -> Result<(), ApiError> {
        self.surface_state(input.handle())?;
        self.host.clear_surface_selection(input)
    }
}
